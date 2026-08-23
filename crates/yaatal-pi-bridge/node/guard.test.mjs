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
import { AgentHarness, InMemorySessionStorage, Session } from "@earendil-works/pi-agent-core";

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

const freshSession = () =>
  new Session(new InMemorySessionStorage({ id: `guard-${Date.now()}`, createdAt: Date.now() }));

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

test("an empty manifest yields a planner with zero tools", async () => {
  const { harness } = await createPlanner({ manifest: [], session: freshSession() });
  assert.deepEqual(await harness.getActiveTools(), []);
});

test("a two-tool manifest yields exactly those two tools and nothing else", async () => {
  const manifest = [
    { name: "products_list", description: "List the merchant's products." },
    { name: "orders_show", description: "Show one order by id." },
  ];
  const { harness } = await createPlanner({ manifest, session: freshSession() });

  const active = await harness.getActiveTools();
  assert.deepEqual([...active].sort(), ["orders_show", "products_list"]);
});

test("version drift: `tools` is still optional and yields zero tools when omitted", async () => {
  // Built here rather than through createPlanner precisely because
  // createPlanner always passes `tools`. This asserts the property the whole
  // custody design rests on: a harness built with no `tools` key at all is
  // legal, and starts empty.
  const { models, model } = fauxModels();
  const { harness } = await AgentHarness.create({
    session: freshSession(),
    models,
    model,
    drive: "manual",
  });

  assert.deepEqual(
    await harness.getActiveTools(),
    [],
    "Pi auto-registered a tool into a harness that asked for none",
  );

  // `getActiveTools()` reports `activeToolNames`, a list separate from the
  // registered tool set — a built-in could be registered without being active.
  // Reaching past the TS-private `tools` field is deliberate: if Pi renames or
  // removes it this fails loudly, which is what a drift probe is for.
  assert.ok(
    Array.isArray(harness.tools),
    "AgentHarness#tools is gone — re-verify the custody story before bumping the pin",
  );
  assert.equal(harness.tools.length, 0, "Pi registered a tool the manifest did not ask for");
});
