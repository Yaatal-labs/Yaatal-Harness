# yaatal-pi-bridge

The door between a [Pi](https://pi.dev) planner and anything real.

Pi is the agent loop the Harness README says it does not have (*"No general
runtime adapters… No model-driven planning"*). This crate is what keeps that
loop from being able to act on its own.

**Read this before writing code here.** It exists so the next person does not
re-derive facts that cost real effort to establish. Design rationale is
`docs/PI-RUNTIME-INTEGRATION.md`; current state and the next concrete task are
`docs/PI-BRIDGE-RUNBOOK.md`.

## The trust model in one paragraph

`pi-agent-core` ships **zero** tools. Its built-ins are opt-in factory
functions and `tools` is optional on `AgentHarnessOptions`, so a harness we
construct starts with nothing — there is no built-in to disable, we simply
never hand any over. The planner emits a `ToolIntent`; Rust resolves it against
a `ToolManifest`, runs it through `ToolPolicyGate`, and only then spawns.
Same shape as `yaatal-edge-turn`: the model proposes, the Harness disposes.

Three gates, in order:

| Gate | Where | Answers |
|---|---|---|
| Manifest | `PiBridge::resolve` | can the planner name this at all? |
| Policy | `ToolPolicyGate::check` | may this actor run it, and is there budget? |
| Drive | `peekAction()` / `executeAction()` | Rust sees the action before the tool boundary |

## Layout

```
src/lib.rs      custody core — ToolIntent, ToolSpec, ToolManifest, PiBridge
node/           the planner (Node), talks to the Engine, never executes
  planner.mjs      builds an AgentHarness whose tools ARE the manifest
  guard.test.mjs   the custody guard — see below
```

## The guard test is the deliverable

`node/guard.test.mjs` is what keeps the planner's reach from silently widening.
Run it:

```bash
cd crates/yaatal-pi-bridge/node && npm install && npm test
```

It proves three things and watches a fourth:

1. `planner.mjs` imports none of `createBashTool` / `createEditTool` /
   `createReadTool` / `createWriteTool`. **All four are named exports of the
   package root**, so one import line is all it would take to hand the planner
   a shell — this grep is the actual control, not a formality. The list is
   `PI_NATIVE_TOOL_FACTORIES` in `src/lib.rs`.
2. The grep is not watching dead strings: each name must still be a live
   export, so a Pi rename fails here instead of passing vacuously forever.
3. A planner's tool surface equals its manifest — the empty manifest included.
4. Version drift: `tools` is still optional, and a harness built without it
   registers nothing.

**`getActiveTools()` alone is not sufficient** and the probe does not rely on
it. It reports `activeToolNames`, which the harness keeps *separate* from the
registered tool set, so a built-in could be registered without appearing there.
The probe also asserts `harness.tools` is empty — reaching past a TS-private
field on purpose, so a rename or removal fails loudly.

If you change `planner.mjs`, run this before anything else.

## Verified facts — do not re-derive

Established against the published `0.84.1` tarballs. Re-verify only when
bumping the pin.

**`pi-agent-core`**
- `AgentHarness.create(options)` → `Promise<{harness, suspended}>`; the class
  constructor is private.
- `AgentHarnessOptions` requires `session`, `models`, `model`. `tools?` is
  **optional** — the whole custody basis. `drive?: "automatic" | "manual"`.
- `HarnessTool = AgentTool & { replay?: "never" | "safe" }`. `AgentTool` needs
  `name`, `description`, `parameters` (typebox `TSchema`), `label`, `execute`.
- `new Session(new InMemorySessionStorage({ id, createdAt }))`.

**`pi-ai`** — the provider seam
- `Provider` is a plain interface: `id`, `name`, `baseUrl?`, `headers?`,
  `auth` (`{apiKey?, oauth?}`), `getModels()`, `stream()`, `streamSimple()`.
  Register with `createModels()` + `setProvider(provider)`.
- `Api = KnownApi | (string & {})` — a custom API identifier is an explicit
  extension point, not a workaround.
- `createAssistantMessageEventStream()` is exported *for use in extensions* —
  that is how a custom provider returns a stream.
- Event order: `start` → content events → `done` with
  `reason: "stop" | "length" | "toolUse" | "deferred"`. Tool calls are content
  blocks (`toolcall_start` / `toolcall_delta` / `toolcall_end`), and
  `AssistantMessage.content` is `(TextContent | ThinkingContent | ToolCall)[]`.
- **There is no `before_provider_request` hook.** An earlier plan said to
  intercept one. `HookName` is `before_run | before_resume | before_run_end |
  transform_context | before_request | before_payload | after_response |
  before_tool | after_tool | before_compaction | before_navigation`.

## Invariants

1. **`pi-agent-core` only — never `pi-coding-agent`.** The coding agent
   auto-wires bash/write/edit/read and runs extensions with its own
   permissions.
2. **Never import a native tool factory** into the Node side.
3. **The model names a tool, never a program.** `ToolSpec.program` and
   `prefix_args` come from the manifest; only trailing args come from the
   model, and `program` never crosses to Node at all.
4. **Every outcome is audited, refusals included.** A denial writes one
   `AuditEvent` with a `Deny` verdict and spawns nothing.
5. **No agentic surface touches money.** Payments, escrow, settlement and
   PI-SPI credentials stay Engine-side. "The AI can take the order" is a new
   trust boundary — escalate, do not implement.
6. **Pin the whole Pi family exactly.** `pi-agent-core` declares `pi-ai` and
   `pi-telemetry` as `^0.84.1`, so a plain install floats them to 0.84.2. The
   direct exact `pi-ai` dep and the `overrides` entry for `pi-telemetry` are
   what hold the line; `package-lock.json` is committed on purpose.

## Weighting

`ToolSpec.cost` is what one call debits from the run's spend cap. It lives on
the manifest so metering a role is a number in configuration, the same place
its allowlist is, and out of the model's reach. `None` means unmetered.

`AuditedExec::run_weighted` attaches it **past the deny gate** — a refusal
spawns nothing, so it spends nothing, and costing denials would let a loop
already over its cap keep inflating the sum the cap is measured against.
`run()` delegates with `None`, so there is still exactly one custody path.

## Gates

```bash
cargo check --workspace --all-targets      # the repo's declared gate
cargo test -p yaatal-pi-bridge
cd crates/yaatal-pi-bridge/node && npm test
```
