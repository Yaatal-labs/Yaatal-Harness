/**
 * Node side of the Yaatal Harness Pi bridge: a planner that can only propose.
 *
 * `@earendil-works/pi-agent-core` ships **zero** tools of its own. `tools` is
 * optional on `AgentHarnessOptions`, so a harness we build starts with nothing
 * — there is no built-in to disable, we simply never hand any over. The four
 * native tool factories are named exports of the package root, and an import
 * is the only way one of them can reach a planner; `guard.test.mjs` greps this
 * file's source for those names (the list is `PI_NATIVE_TOOL_FACTORIES` in
 * `../src/lib.rs`) because a grep is the control, not a convention.
 *
 * Everything the planner may name comes from the manifest the Rust side hands
 * in, and that manifest is the minimal projection `[{ name, description }]`.
 * `program` / `prefix_args` never cross this boundary: the model names a tool,
 * Rust resolves the program.
 *
 * # Why `Agent` and not `AgentHarness`
 *
 * `AgentHarness` at 0.84.1 is a scaffold: 22 of its methods throw
 * `HarnessNotImplemented`, including `prompt`, `peekAction`, `executeAction`
 * and `hooks.on`. It can hold tools and a model but cannot run a turn. 0.84.2
 * is identical. `Agent` and `agentLoop` underneath it are fully implemented,
 * so that is the layer this uses.
 *
 * The custody property is unchanged — `AgentContext.tools` is optional and we
 * supply it — and the gate is better: `beforeToolCall` runs after argument
 * validation and before execution, and `{ block: true, reason }` refuses the
 * call with a model-readable explanation. `terminate: true` halts the batch,
 * which is how an exhausted spend cap stops a runaway loop.
 */

import { Agent } from "@earendil-works/pi-agent-core";
import { Type, createModels, fauxProvider } from "@earendil-works/pi-ai";

/**
 * The parameter schema for every bridged tool, deliberately identical across
 * all of them. Rust's `ToolIntent` carries `args: Vec<String>` positionally,
 * and the manifest already fixes the program plus its leading arguments, so
 * per-tool schema generation would buy nothing but a second place for the tool
 * surface to drift.
 */
export const BRIDGED_TOOL_PARAMETERS = Type.Object({
  args: Type.Array(Type.String(), {
    description: "Positional arguments, appended after the tool's fixed prefix.",
  }),
});

/**
 * What the planner is told about calling tools.
 *
 * **This must agree with `parseToolCall` in `yaatal-provider.mjs`.** The
 * cascade is text-only, so a tool call is a JSON object in the reply and the
 * provider turns it back into a native `ToolCall` block. The parser is strict
 * — whole reply or one fenced block, both keys required — so the prompt asks
 * for exactly that and nothing around it. Loosening one without the other is
 * how a planner starts emitting calls that silently read as prose.
 */
export const DEFAULT_SYSTEM_PROMPT = [
  "You plan operations for the Yaatal control plane.",
  "",
  "To call a tool, reply with a single JSON object and nothing else:",
  '{"tool": "<name>", "args": ["<arg>", ...]}',
  "",
  "Both keys are required. `args` must be an array of strings, empty if the",
  "tool needs none. Do not wrap it in prose — a described call is not a call.",
  "You may only name a tool that was given to you. When you have the answer,",
  "reply in plain text instead.",
].join("\n");

/**
 * Stand-in dispatch until the JSON-RPC client lands. Returns a well-formed
 * `AgentToolResult` so the loop shape is exercisable now.
 *
 * ponytail: a stub, not a fake. Ceiling — it executes nothing and always
 * "succeeds", so a planner run against it proves tool *plumbing* only, never
 * tool behaviour. Upgrade path: slice 4 injects the real JSON-RPC client as
 * `dispatch`, which round-trips a `ToolIntent` to `PiBridge::dispatch` in Rust
 * and returns its `ExecOutput` (or an audited refusal). Nothing else here
 * changes.
 */
async function stubDispatch(toolName, args) {
  return {
    content: [
      {
        type: "text",
        text: `[stub] ${toolName}(${args.join(" ")}) was not executed: this planner has no transport yet.`,
      },
    ],
    details: { stub: true, tool: toolName, args },
  };
}

/**
 * One manifest entry to one `HarnessTool`.
 *
 * `replay: "never"` because a bridged tool is a side-effecting subprocess on
 * the Rust side; replaying a transcript must not re-run it.
 */
function bridgedTool(entry, dispatch) {
  if (typeof entry?.name !== "string" || entry.name.length === 0) {
    throw new TypeError("manifest entry needs a non-empty string `name`");
  }
  if (typeof entry.description !== "string") {
    throw new TypeError(`manifest entry \`${entry.name}\` needs a string \`description\``);
  }
  return {
    name: entry.name,
    label: entry.name,
    description: entry.description,
    parameters: BRIDGED_TOOL_PARAMETERS,
    replay: "never",
    execute: async (_toolCallId, params) => dispatch(entry.name, params?.args ?? []),
  };
}

/**
 * A `Models` collection with one faux model registered, for the slices that
 * have no provider yet.
 *
 * ponytail: a test double promoted to a default. Ceiling — it answers from a
 * scripted response list, so it can drive the loop but cannot plan. Upgrade
 * path: Phase 3's Yaatal `Provider` posts to the Engine's `/api/ai/chat` and
 * registers the same way (`createModels()` + `setProvider()`), so callers pass
 * `{ models, model }` and this default falls away untouched.
 */
export function fauxModels() {
  const faux = fauxProvider();
  const models = createModels();
  models.setProvider(faux.provider);
  return { models, model: faux.getModel(), faux };
}

/**
 * Build a planner whose entire tool surface is the supplied manifest.
 *
 * @param {object} [options]
 * @param {{name: string, description: string}[]} [options.manifest]
 *   The only tools this planner can name. Empty means it can name none.
 * @param {(toolName: string, args: string[]) => Promise<object>} [options.dispatch]
 *   Where an allowed tool call goes. Defaults to a non-executing stub.
 * @param {(toolName: string, args: string[]) => Promise<{block?: boolean, reason?: string, terminate?: boolean}|undefined>} [options.gate]
 *   The custody gate, run after argument validation and before execution.
 *   Return `{block: true, reason}` to refuse; `terminate: true` also halts the
 *   batch, which is how an exhausted spend cap stops a loop. Defaults to
 *   allow-all — Rust supplies the real one, which is `ToolPolicyGate`.
 * @param {string} [options.systemPrompt]
 * @returns {{agent: Agent, tools: object[]}}
 */
export function createPlanner(options = {}) {
  const { manifest = [], dispatch = stubDispatch, gate, systemPrompt = DEFAULT_SYSTEM_PROMPT } = options;
  if (!Array.isArray(manifest)) {
    throw new TypeError("`manifest` must be an array of { name, description }");
  }

  const fallback = options.models && options.model ? null : fauxModels();
  const models = options.models ?? fallback.models;
  const model = options.model ?? fallback.model;
  const tools = manifest.map((entry) => bridgedTool(entry, dispatch));

  const agent = new Agent({
    // `Models.streamSimple` satisfies `StreamFn` by contract, so the Yaatal
    // provider drops in here with no adapter.
    streamFn: (requestModel, context, streamOptions) =>
      models.streamSimple(requestModel, context, streamOptions),
    // The custody design in one line: what goes in is exactly the manifest,
    // mapped, and nothing is ever appended to it.
    initialState: { systemPrompt, model, tools },
    beforeToolCall: gate
      ? async ({ toolCall }) => gate(toolCall.name, toolCall.arguments?.args ?? [])
      : undefined,
  });

  return { agent, tools };
}
