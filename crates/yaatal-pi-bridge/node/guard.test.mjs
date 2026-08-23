/**
 * The custody guard. This file, not `planner.mjs`, is the deliverable of this
 * slice: it is what keeps the planner's reach from silently widening.
 *
 * Two things it proves, and one it watches:
 *   - the planner never imports a native tool factory (a source grep —
 *     importing one is the *only* way a native tool can appear);
 *   - a planner's tool surface is exactly its manifest, the empty manifest
 *     included;
 *   - `tools` is still optional on `AgentHarnessOptions`. If a future Pi makes
 *     it required, or auto-registers a built-in, that must break here rather
 *     than quietly hand the agent a shell.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import * as piAgentCore from "@earendil-works/pi-agent-core";
import { Agent } from "@earendil-works/pi-agent-core";

import { createPlanner, fauxModels } from "./planner.mjs";

/**
 * Mirrors `PI_NATIVE_TOOL_FACTORIES` in `../src/lib.rs`. Written out literally
 * on purpose: the control is that these exact strings do not occur in the
 * planner's source, so they must not be assembled at runtime here either.
 */
const PI_NATIVE_TOOL_FACTORIES = [
  "createBashTool",
  "createEditTool",
  "createReadTool",
  "createWriteTool",
];

const PLANNER_PATH = fileURLToPath(new URL("./planner.mjs", import.meta.url));

test("planner source imports no native tool factory", () => {
  // Read the source rather than importing and inspecting: an import that had
  // already happened is exactly what we are trying to rule out.
  const source = readFileSync(PLANNER_PATH, "utf8");
  for (const factory of PI_NATIVE_TOOL_FACTORIES) {
    assert.ok(
      !source.includes(factory),
      `planner.mjs mentions ${factory} — importing one is the only way a native tool reaches the planner`,
    );

    // …and the grep must not be watching dead strings. If Pi renames a factory
    // the list above silently stops protecting anything, so pin it to the
    // shipped exports.
    assert.equal(
      typeof piAgentCore[factory],
      "function",
      `${factory} is no longer exported by pi-agent-core — the guard list has gone stale`,
    );
  }
});

test("an empty manifest yields a planner with zero tools", () => {
  const { agent } = createPlanner({ manifest: [] });
  assert.deepEqual(agent.state.tools, []);
});

test("a two-tool manifest yields exactly those two tools and nothing else", () => {
  const manifest = [
    { name: "products_list", description: "List the merchant's products." },
    { name: "orders_show", description: "Show one order by id." },
  ];
  const { agent } = createPlanner({ manifest });

  assert.deepEqual(
    agent.state.tools.map((t) => t.name).sort(),
    ["orders_show", "products_list"],
  );
});

test("version drift: an agent given no tools registers none", () => {
  // Built here rather than through createPlanner precisely because
  // createPlanner always passes `tools`. This asserts the property the whole
  // custody design rests on: an agent built with no tools at all is legal and
  // starts empty. If a future Pi auto-registers a built-in, this fails loudly
  // rather than quietly handing the planner a shell.
  const { models, model } = fauxModels();
  const agent = new Agent({
    streamFn: (m, ctx, o) => models.streamSimple(m, ctx, o),
    initialState: { systemPrompt: "", model },
  });

  assert.deepEqual(agent.state.tools, [], "Pi registered a tool nobody asked for");
});

test("version drift: the runnable layer is Agent, not AgentHarness", () => {
  // AgentHarness at 0.84.1 is a scaffold — prompt/peekAction/executeAction and
  // hooks.on all throw HarnessNotImplemented, and 0.84.2 is identical. The
  // planner therefore targets Agent. When a release finally implements the
  // harness this fails, which is the signal to re-evaluate the layer choice
  // deliberately rather than discover it by accident.
  const harnessSource = readFileSync(
    fileURLToPath(
      new URL(
        "./node_modules/@earendil-works/pi-agent-core/dist/harness/agent-harness.js",
        import.meta.url,
      ),
    ),
    "utf8",
  );
  assert.ok(
    harnessSource.includes('unavailable("prompt")'),
    "AgentHarness.prompt is implemented now — reconsider Agent vs AgentHarness",
  );
});
