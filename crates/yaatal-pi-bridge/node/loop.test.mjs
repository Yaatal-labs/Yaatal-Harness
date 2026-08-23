/**
 * The whole Node side, end to end: planner + provider + a stand-in Engine.
 *
 * Everything else here tests a component. This is the only thing that proves
 * they compose — that a text reply from the cascade becomes a native tool call,
 * passes the custody gate, executes, and feeds a second turn. It is also what
 * caught that `AgentHarness` cannot run a turn at 0.84.1; unit tests never
 * would have, because they only touch the parts that are implemented.
 *
 * Still a stand-in Engine, not a real one — the model is scripted. What is real
 * is the loop, the provider, the tool-call synthesis and the gate.
 */

import assert from "node:assert/strict";
import { createServer } from "node:http";
import { after, test } from "node:test";

import { createModels } from "@earendil-works/pi-ai";

import { createPlanner } from "./planner.mjs";
import { yaatalProvider } from "./yaatal-provider.mjs";

const servers = [];
after(async () => {
  await Promise.all(servers.map((s) => new Promise((r) => s.close(r))));
});

/** An Engine that replies with `scripted[n]` to the nth turn. */
async function scriptedEngine(scripted) {
  const turns = [];
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      turns.push(JSON.parse(body));
      const content = scripted[turns.length - 1] ?? "done";
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ content, model: "t3/qwen", request_id: `gw-${turns.length}` }));
    });
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  servers.push(server);
  return { baseUrl: `http://127.0.0.1:${server.address().port}`, turns };
}

/** Wire a planner to a scripted Engine. */
async function harnessed(scripted, { gate, manifest } = {}) {
  const { baseUrl, turns } = await scriptedEngine(scripted);
  const { provider, model } = yaatalProvider({ baseUrl, token: "test" });
  const models = createModels();
  models.setProvider(provider);

  const executed = [];
  const { agent } = createPlanner({
    manifest: manifest ?? [{ name: "products_list", description: "List products." }],
    models,
    model,
    gate,
    dispatch: async (tool, args) => {
      executed.push({ tool, args });
      return { content: [{ type: "text", text: "a\nb\nc" }], details: {} };
    },
  });
  return { agent, executed, turns };
}

test("a scripted tool call runs the tool and feeds a second turn", async () => {
  const { agent, executed, turns } = await harnessed([
    '{"tool":"products_list","args":["--json"]}',
    "You have three products.",
  ]);

  await agent.prompt("How many products do I have?");

  assert.deepEqual(executed, [{ tool: "products_list", args: ["--json"] }]);
  assert.equal(turns.length, 2, "the tool result should drive a second turn");
  assert.deepEqual(
    agent.state.messages.map((m) => m.role),
    ["user", "assistant", "toolResult", "assistant"],
  );
});

test("a text reply ends the run without executing anything", async () => {
  const { agent, executed, turns } = await harnessed(["You have three products."]);

  await agent.prompt("How many products do I have?");

  assert.deepEqual(executed, [], "nothing should have run");
  assert.equal(turns.length, 1);
});

test("a blocked call never executes, and the reason reaches the model", async () => {
  const { agent, executed, turns } = await harnessed(
    ['{"tool":"products_list","args":[]}', "Understood, I will stop."],
    { gate: async () => ({ block: true, reason: "run spend cap reached" }) },
  );

  await agent.prompt("List my products");

  assert.deepEqual(executed, [], "a blocked call must not reach dispatch");
  const toolResult = agent.state.messages.find((m) => m.role === "toolResult");
  assert.ok(toolResult, "a blocked call still produces a tool result");
  assert.match(JSON.stringify(toolResult.content), /spend cap/);
  assert.equal(turns.length, 2, "the model is told why, and gets another turn");
});

test("the gate sees the tool name and args the model actually asked for", async () => {
  const seen = [];
  const { agent } = await harnessed(['{"tool":"products_list","args":["--json","--limit=5"]}', "ok"], {
    gate: async (tool, args) => {
      seen.push({ tool, args });
      return undefined;
    },
  });

  await agent.prompt("List my products");

  assert.deepEqual(seen, [{ tool: "products_list", args: ["--json", "--limit=5"] }]);
});

test("a tool outside the manifest is never offered, so it cannot be called", async () => {
  const { agent, executed } = await harnessed(['{"tool":"bash","args":["rm -rf /"]}', "ok"], {
    manifest: [{ name: "products_list", description: "List products." }],
  });

  await agent.prompt("Delete everything");

  assert.deepEqual(executed, [], "an unlisted tool must not execute");
  assert.deepEqual(agent.state.tools.map((t) => t.name), ["products_list"]);
});
