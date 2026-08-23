/**
 * A Pi `Provider` backed by the Yaatal Engine's cascade.
 *
 * The planner must not pin a model. The Engine's `/api/ai/chat` resolves a
 * capability alias to a server-chosen tier and never forwards a caller-supplied
 * model id, so this provider exposes exactly one model — `cascade` — whose
 * whole meaning is "whatever the Engine picks". Every turn therefore passes
 * `route_declared`: one classifier, the 5-tier cascade, the circuit breakers
 * and the shared token budget. **No provider credentials live in the Harness.**
 *
 * # Why this synthesizes tool calls
 *
 * The cascade is text-in/text-out — its `Message` is `{role, content}` and it
 * has no `tools` field. That is deliberate: native tool calling is
 * model-dependent, so requiring it would collapse the planner onto the tiers
 * that support it and break the cascade property we want. Tier 1 on-device
 * stays eligible precisely because we ask for text.
 *
 * Pi models tool calls as *content blocks*, so the translation belongs here:
 * the planner asks for JSON, this parses it into a real `ToolCall` block, and
 * Pi's loop never learns the backend was text-only. One parse, one place.
 */

import { createAssistantMessageEventStream } from "@earendil-works/pi-ai";

export const YAATAL_API = "yaatal-engine-chat";
export const YAATAL_PROVIDER_ID = "yaatal-engine";

/**
 * The one model this provider offers. `cascade` is not a vendor id and must
 * never become one — the Engine chooses the tier, and a caller that could name
 * a model would be routing around that choice.
 */
export const CASCADE_MODEL_ID = "cascade";

/**
 * What the planner is asked to emit for a tool call, and the only shape this
 * accepts. Both keys are required: requiring `args` as well as `tool` is what
 * stops a stray object in prose being read as a call.
 *
 * @example {"tool": "products_list", "args": ["--json"]}
 */
const TOOL_CALL_KEYS = ["tool", "args"];

const FENCED_JSON = /```(?:json)?\s*\n([\s\S]*?)\n?```/;

const zeroUsage = () => ({ input: 0, output: 0, cacheRead: 0, cacheWrite: 0 });

function isToolCallShape(value) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return false;
  if (!TOOL_CALL_KEYS.every((k) => k in value)) return false;
  if (typeof value.tool !== "string" || value.tool.length === 0) return false;
  return Array.isArray(value.args) && value.args.every((a) => typeof a === "string");
}

/**
 * Read a tool call out of the model's text, or return `null`.
 *
 * Deliberately strict, and deliberately not a brace scan. Two candidates only:
 * the whole trimmed reply, and the contents of one fenced block. Hunting for
 * the first `{` in prose would let an assistant *describing* a tool call
 * become one — the failure mode here is not a missed call (the planner simply
 * retries) but a fabricated one, so ambiguity resolves to text every time.
 */
export function parseToolCall(text) {
  if (typeof text !== "string") return null;
  const fenced = FENCED_JSON.exec(text);
  const candidates = [text.trim(), fenced?.[1]?.trim()];

  for (const candidate of candidates) {
    if (!candidate || candidate[0] !== "{") continue;
    let parsed;
    try {
      parsed = JSON.parse(candidate);
    } catch {
      continue; // malformed is text, never a guess
    }
    if (isToolCallShape(parsed)) return { name: parsed.tool, args: parsed.args };
  }
  return null;
}

/** Every bridged tool takes `{args: string[]}` — see planner.mjs. */
const toolCallBlock = (call, id) => ({
  type: "toolCall",
  id,
  name: call.name,
  arguments: { args: call.args },
});

/**
 * POST one turn to the Engine and return what it said.
 *
 * `model` is deliberately absent from the body: the Engine strips a
 * caller-supplied model anyway, and sending one would imply an authority this
 * client does not have.
 */
async function askEngine({ baseUrl, token, messages, signal }) {
  const response = await fetch(`${baseUrl.replace(/\/+$/, "")}/api/ai/chat`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
    body: JSON.stringify({
      messages: messages.map((m) => ({
        role: m.role,
        content: typeof m.content === "string" ? m.content : contentToText(m.content),
      })),
    }),
    signal,
  });

  if (!response.ok) {
    // The body may carry provider text; the status is the part safe to surface.
    throw new Error(`engine /api/ai/chat responded ${response.status}`);
  }
  return response.json();
}

/** Flatten Pi's content blocks to the plain text the cascade accepts. */
function contentToText(content) {
  if (!Array.isArray(content)) return String(content ?? "");
  return content
    .filter((block) => block?.type === "text")
    .map((block) => block.text)
    .join("\n");
}

/**
 * Build the `Provider`. `baseUrl` and `token` default to the same environment
 * variables the runner already uses (`crates/yaatal-runner/src/proposals_push.rs`),
 * so there is one way to address the Engine from this repo.
 */
export function yaatalProvider({
  baseUrl = process.env.YAATAL_ENGINE_URL,
  token = process.env.YAATAL_TOKEN,
} = {}) {
  if (!baseUrl) {
    throw new Error("YAATAL_ENGINE_URL is required — the planner has no other route to a model");
  }

  const model = {
    id: CASCADE_MODEL_ID,
    name: "Yaatal Engine cascade",
    api: YAATAL_API,
    provider: YAATAL_PROVIDER_ID,
    baseUrl,
    reasoning: false,
    input: ["text"],
    // The Engine owns spend (AI_DAILY_TOKEN_BUDGET) and does not report usage
    // on this endpoint. Zeroes here are honest: this client cannot price a
    // turn, and inventing rates would make Pi's accounting confidently wrong.
    // ponytail: upgrade path is the Engine returning usage, then map it.
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    contextWindow: 8192,
    maxTokens: 1024,
  };

  const stream = (requestModel, context, options) => {
    const events = createAssistantMessageEventStream();

    (async () => {
      const partial = {
        role: "assistant",
        content: [],
        api: YAATAL_API,
        provider: YAATAL_PROVIDER_ID,
        model: requestModel?.id ?? CASCADE_MODEL_ID,
        usage: zeroUsage(),
        stopReason: "stop",
        timestamp: Date.now(),
      };
      events.push({ type: "start", partial });

      try {
        const body = await askEngine({
          baseUrl,
          token,
          messages: context?.messages ?? [],
          signal: options?.signal,
        });

        // The tier the cascade actually chose, surfaced for the audit trail.
        partial.responseModel = body?.model;
        partial.responseId = body?.request_id;

        const text = typeof body?.content === "string" ? body.content : "";
        const call = parseToolCall(text);

        if (call) {
          const block = toolCallBlock(call, body?.request_id ?? `yaatal-${Date.now()}`);
          partial.content.push(block);
          events.push({ type: "toolcall_start", contentIndex: 0, partial });
          events.push({ type: "toolcall_end", contentIndex: 0, toolCall: block, partial });
          partial.stopReason = "toolUse";
        } else {
          partial.content.push({ type: "text", text });
          events.push({ type: "text_start", contentIndex: 0, partial });
          events.push({ type: "text_end", contentIndex: 0, content: text, partial });
          partial.stopReason = "stop";
        }

        events.push({ type: "done", reason: partial.stopReason, message: partial });
        events.end(partial);
      } catch (error) {
        partial.stopReason = "error";
        partial.errorMessage = error instanceof Error ? error.message : String(error);
        events.push({ type: "error", reason: "error", error: partial });
        events.end(partial);
      }
    })();

    return events;
  };

  return {
    provider: {
      id: YAATAL_PROVIDER_ID,
      name: "Yaatal Engine",
      baseUrl,
      /**
       * `Models` resolves provider auth before every request, so `resolve` is
       * required — a bare `{name}` throws "apiKey.resolve is not a function"
       * at request time, not construction time. Ambient-only: the Engine takes
       * a Harness-minted bearer token, not a vendor key, so there is no
       * interactive `login` to offer. Returning `undefined` marks the provider
       * unconfigured, which is the honest answer when no token is set.
       */
      auth: {
        apiKey: {
          name: "Yaatal Engine bearer token",
          resolve: async () =>
            token
              ? { auth: { apiKey: token, baseUrl }, source: "YAATAL_TOKEN" }
              : undefined,
        },
      },
      getModels: () => [model],
      stream,
      streamSimple: stream,
    },
    model,
  };
}
