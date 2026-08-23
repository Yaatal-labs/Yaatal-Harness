/**
 * The Yaatal provider's contract with the Engine.
 *
 * Run against a real loopback HTTP server rather than an injected fetch: the
 * thing most likely to break here is the request this actually puts on the
 * wire, and a stubbed fetch would assert my idea of it instead of the real one.
 */

import assert from "node:assert/strict";
import { createServer } from "node:http";
import { after, test } from "node:test";

import { createModels } from "@earendil-works/pi-ai";

import { CASCADE_MODEL_ID, parseToolCall, yaatalProvider } from "./yaatal-provider.mjs";

/** A stand-in Engine. `reply` decides what /api/ai/chat returns. */
async function engineStub(reply) {
  const seen = [];
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      seen.push({ url: req.url, method: req.method, headers: req.headers, body: JSON.parse(body) });
      const { status = 200, json } = reply(seen.length);
      res.writeHead(status, { "content-type": "application/json" });
      res.end(JSON.stringify(json ?? {}));
    });
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  const baseUrl = `http://127.0.0.1:${server.address().port}`;
  return { baseUrl, seen, close: () => new Promise((r) => server.close(r)) };
}

const servers = [];
const stub = async (reply) => {
  const s = await engineStub(reply);
  servers.push(s);
  return s;
};
after(async () => {
  await Promise.all(servers.map((s) => s.close()));
});

const turn = (text) => ({ messages: [{ role: "user", content: text }] });

// ── the wire ────────────────────────────────────────────────────────────

test("posts to /api/ai/chat with a bearer token and no model field", async () => {
  const { baseUrl, seen } = await stub(() => ({ json: { content: "hi", model: "t3/qwen" } }));
  const { provider, model } = yaatalProvider({ baseUrl, token: "tok-123" });

  await provider.streamSimple(model, turn("hello")).result();

  assert.equal(seen.length, 1);
  assert.equal(seen[0].url, "/api/ai/chat");
  assert.equal(seen[0].headers.authorization, "Bearer tok-123");
  assert.deepEqual(seen[0].body.messages, [{ role: "user", content: "hello" }]);
  // The Engine strips a caller-supplied model; sending one would claim an
  // authority this client does not have.
  assert.ok(!("model" in seen[0].body), "must not send a model id");
});

test("the only model offered is the cascade, never a vendor id", async () => {
  const { baseUrl } = await stub(() => ({ json: { content: "" } }));
  const { provider } = yaatalProvider({ baseUrl });
  const models = provider.getModels();

  assert.equal(models.length, 1);
  assert.equal(models[0].id, CASCADE_MODEL_ID);
});

test("registers into a Models collection the harness can use", async () => {
  const { baseUrl } = await stub(() => ({ json: { content: "" } }));
  const { provider } = yaatalProvider({ baseUrl });
  const models = createModels();
  models.setProvider(provider);

  assert.ok(models.getModel(provider.id, CASCADE_MODEL_ID), "cascade model resolvable");
});

test("refuses to construct without an Engine URL", () => {
  assert.throws(() => yaatalProvider({ baseUrl: undefined, token: "t" }), /YAATAL_ENGINE_URL/);
});

// ── text vs tool call ───────────────────────────────────────────────────

test("a plain reply becomes text and stops", async () => {
  const { baseUrl } = await stub(() => ({ json: { content: "no tool needed", model: "t1/local" } }));
  const { provider, model } = yaatalProvider({ baseUrl });

  const message = await provider.streamSimple(model, turn("hi")).result();

  assert.equal(message.stopReason, "stop");
  assert.deepEqual(message.content, [{ type: "text", text: "no tool needed" }]);
  // The tier the cascade actually chose has to reach the audit trail.
  assert.equal(message.responseModel, "t1/local");
});

test("a JSON reply becomes a native ToolCall block", async () => {
  const { baseUrl } = await stub(() => ({
    json: { content: '{"tool":"products_list","args":["--json"]}', request_id: "gw-abc" },
  }));
  const { provider, model } = yaatalProvider({ baseUrl });

  const message = await provider.streamSimple(model, turn("list products")).result();

  assert.equal(message.stopReason, "toolUse");
  assert.deepEqual(message.content, [
    { type: "toolCall", id: "gw-abc", name: "products_list", arguments: { args: ["--json"] } },
  ]);
});

test("a fenced JSON reply becomes a ToolCall too", async () => {
  const { baseUrl } = await stub(() => ({
    json: { content: '```json\n{"tool":"orders_show","args":["42"]}\n```' },
  }));
  const { provider, model } = yaatalProvider({ baseUrl });

  const message = await provider.streamSimple(model, turn("show order")).result();

  assert.equal(message.stopReason, "toolUse");
  assert.equal(message.content[0].name, "orders_show");
});

// ── the failure mode that matters ───────────────────────────────────────

test("malformed and tool-shaped-but-not replies stay text, never a fabricated call", async () => {
  const notCalls = [
    '{"tool":"products_list"',                        // truncated
    '{"tool":"products_list"}',                       // no args
    '{"args":["--json"]}',                            // no tool
    '{"tool":"","args":[]}',                          // empty name
    '{"tool":"products_list","args":"--json"}',       // args not an array
    '{"tool":"products_list","args":[1,2]}',          // args not strings
    'To list them I would call {"tool":"x","args":[]}', // described, not emitted
    "plain prose",
    "",
  ];

  for (const content of notCalls) {
    const { baseUrl } = await stub(() => ({ json: { content } }));
    const { provider, model } = yaatalProvider({ baseUrl });
    const message = await provider.streamSimple(model, turn("go")).result();

    assert.equal(message.stopReason, "stop", `should not be a tool call: ${content}`);
    assert.equal(message.content[0].type, "text", `should be text: ${content}`);
    assert.equal(parseToolCall(content), null, `parseToolCall should refuse: ${content}`);
  }
});

test("a non-200 from the Engine surfaces as an error, not a silent empty turn", async () => {
  const { baseUrl } = await stub(() => ({ status: 503, json: { error: "gateway unavailable" } }));
  const { provider, model } = yaatalProvider({ baseUrl });

  const message = await provider.streamSimple(model, turn("hi")).result();

  assert.equal(message.stopReason, "error");
  assert.match(message.errorMessage, /503/);
});
