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
 */

import { AgentHarness, InMemorySessionStorage, Session } from "@earendil-works/pi-agent-core";
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
 *   Where a tool call goes. Defaults to a non-executing stub.
 * @param {object} [options.session] Defaults to a fresh in-memory session.
 * @param {object} [options.models] Defaults to {@link fauxModels}.
 * @param {object} [options.model] Defaults to {@link fauxModels}.
 * @param {string} [options.systemPrompt]
 * @returns {Promise<{harness: object, suspended: object[]}>} exactly what
 *   `AgentHarness.create` returns — the class constructor is private.
 */
export async function createPlanner(options = {}) {
  const { manifest = [], dispatch = stubDispatch, systemPrompt } = options;
  if (!Array.isArray(manifest)) {
    throw new TypeError("`manifest` must be an array of { name, description }");
  }

  const fallback = options.models && options.model ? null : fauxModels();
  const session =
    options.session ??
    new Session(
      new InMemorySessionStorage({ id: `pi-bridge-${Date.now()}`, createdAt: Date.now() }),
    );

  return AgentHarness.create({
    session,
    models: options.models ?? fallback.models,
    model: options.model ?? fallback.model,
    // Rust holds the loop: `peekAction()` / `executeAction()` give it sight of
    // a planned action before the tool boundary — a gate above the manifest
    // and the policy check. Never "automatic".
    drive: "manual",
    // The custody design in one line. `tools` is optional; what goes in is
    // exactly the manifest, mapped, and nothing is ever appended to it.
    tools: manifest.map((entry) => bridgedTool(entry, dispatch)),
    ...(systemPrompt === undefined ? {} : { systemPrompt }),
  });
}
