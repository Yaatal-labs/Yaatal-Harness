# Pi bridge — pickup runbook

**For whoever comes next, agent or human.** Design lives in
`PI-RUNTIME-INTEGRATION.md`; this file is state and next action. Update the
**Status** and **Handoff log** sections before you stop, whatever you got done.

---

## Status (as of 2026-08-11, commit `e5b75bb`)

| Piece | State |
|---|---|
| `crates/yaatal-pi-bridge` — Rust custody core | ✅ landed, 4 tests green |
| Node planner (`AgentHarness` w/ manifest-only tools) | ❌ not started |
| JSON-RPC transport (stdin/stdout) | ❌ not started |
| `yaatal-runner` wired as first caller | ❌ not started |
| Audit `ModelCall` events (Phase 2) | ❌ not started |
| Sovereignty via Engine gateway (Phase 3) | ❌ not started |
| Roles beyond `ops-runner` (Phase 4) | ❌ not started |
| Container isolation (Phase 5) | ❌ not started |

**The bridge has no caller.** It compiles and is tested, but nothing invokes it
yet. Do not read "crate exists" as "integration works."

Branch: `claude/up-to-hit-it-1dap1e`.

---

## Get your footing (run these first, ~2 min)

```bash
cd /path/to/Yaatal-Harness
cargo check --workspace --all-targets   # must be clean — this is the repo's gate
cargo test -p yaatal-pi-bridge          # must be 4 passed
cargo test --workspace                  # must be 0 failures
```

If any of those are red before you touch anything, **stop and fix that first** —
you are not where this document says you are.

Then read, in order:
1. `crates/yaatal-pi-bridge/src/lib.rs` — the module doc explains the whole
   trust model in ~30 lines. Start there, not here.
2. `crates/yaatal-tools/src/audited_exec.rs` — `AuditedExec::run` is the thing
   the bridge delegates to; it owns policy-check-before-spawn.
3. `crates/yaatal-policy/src/tool_policy.rs` — `ToolPolicyGate`, and
   `with_actor_allowed` which is how agentic roles are expressed.
4. `crates/yaatal-edge-turn/` — the existing propose→validate→act pattern this
   deliberately mirrors. If you are unsure how something should look, look here.

---

## Invariants — do not break these

1. **Depend on `@earendil-works/pi-agent-core`, never `pi-coding-agent`.** The
   coding agent auto-wires `bash`/`write`/`edit`/`read` and runs extensions with
   its own permissions. `pi-agent-core` ships **zero** tools; that is the entire
   basis of the custody design.
2. **The Node side must never import** `createBashTool`, `createEditTool`,
   `createReadTool`, `createWriteTool`. Importing one is the *only* way a native
   tool can reach the planner. The list is `PI_NATIVE_TOOL_FACTORIES` in
   `lib.rs`; a grep test against the entrypoint is required (see Next task).
3. **The model names a tool, never a program.** `ToolSpec.program` and
   `prefix_args` come from the manifest; only trailing args come from the model.
4. **Every outcome is audited**, including refusals. A denial writes one
   `AuditEvent` with a `Deny` verdict and spawns nothing.
5. **No agentic surface touches money.** Payments, escrow, settlement and PI-SPI
   credentials stay Engine-side. "The AI can take the order" is a new trust
   boundary, not a new tool — escalate, don't implement.
6. **Pin Pi exactly** (`0.84.1` today). No `^`, no `~`.

---

## Next task: the Node planner + guard test

Smallest useful next slice. Nothing below needs the RPC transport yet.

1. `mkdir crates/yaatal-pi-bridge/node`, `package.json` with
   `"type": "module"` and an exact dep on
   `@earendil-works/pi-agent-core@0.84.1`.
2. `planner.mjs`: read a manifest (JSON on stdin or argv for now), build an
   `AgentHarness` passing `tools` built **only** from that manifest. Each tool's
   `execute()` should, for this slice, just return a stub result — wiring it to
   real RPC is the step after.
   - `AgentHarnessOptions` needs `session`, `models`, `model`; `tools` is
     optional (that is the point). Types:
     `node_modules/@earendil-works/pi-agent-core/dist/harness/agent-harness.d.ts`.
   - Tool shape: `AgentHarnessTool` in `dist/harness/types.d.ts:57`.
3. **Guard test** (this is the deliverable, not the planner): assert the
   entrypoint source contains none of the four factory names, and assert a
   harness built from an empty manifest exposes zero tools. Wire it into
   `npm test` in that folder, and reference it from the Rust crate's README so
   it is discoverable.
4. Re-run the probe idea from `scratchpad/pi-spike/probe.mjs` (recreate if gone,
   ~20 lines) as a version-drift check: if a future Pi makes `tools` non-optional
   or auto-registers a built-in, that must fail loudly rather than silently
   widen the agent's reach.

After that, in order: JSON-RPC transport over stdin/stdout → `ToolIntent`
round-trip → `yaatal-runner` calls `PiBridge::dispatch` → Phase 2 audit events.

---

## Gotchas already paid for

- `yaatal_audit`'s in-memory store is **`MemoryAuditStore`**, not
  `InMemoryAuditStore`.
- `crates/yaatal-api` is a stub **excluded from the workspace** — leave it out.
- `yaatal-tools` gates dangerous built-ins behind Cargo features,
  `default = ["safe-tools"]` (file read + session notes only). Do not enable the
  others to make something work; that is the wrong fix.
- `digest()` in `yaatal-audit` is FNV-1a and **version-stable on purpose**
  (`proposals::detect` compares digests against the persisted JSONL store). Two
  tests pin it. If they fail, digests drifted and every stored audit trail
  stopped correlating — do not "fix" by updating expected values.
- The repo's gate is `cargo check --workspace --all-targets`, but
  `cargo test --workspace` also passes today. Keep both green.

## Known constraints from research

- **TypeScript 7.0** (GA 8 Jul 2026, Go-native) has **no stable programmatic
  API** until 7.1 — `ts-jest`, `typescript-eslint`, `ts-morph` cannot run on it.
  Do not pin this Node side to TS 7.
- Node type-stripping: default in Node 24 LTS, stable in Node 26.
- **ACP (Agent Client Protocol) is the wrong seam** — it standardises
  editor↔agent, not control-plane↔agent. Do not reach for it.
- Pi moved npm scope: `@mariozechner/*` → `@earendil-works/*`.
- Dependency posture (npm pin vs vendor/fork) was deferred to the end of
  Phase 1. `GATEWAY-STRATEGY-2026.md` chose "fork and pin" for ZeroClaw; decide
  deliberately, do not drift into a floating dep.

---

## Handoff log

Append one entry per session. Newest last. Keep it to what the next person
needs, not what you did.

- **2026-08-11 — Rust custody core landed (`395e0f4`), plan doc (`e5b75bb`).**
  Verified against the shipped types of `pi-agent-core@0.84.1` that built-ins
  are opt-in factories and `tools` is optional, which is why custody is "never
  hand them over" rather than "disable them". `PiBridge` delegates to
  `AuditedExec` rather than duplicating policy/store/actor/timeout — an earlier
  draft duplicated them and clippy correctly flagged four dead fields.
  Next: Node planner + guard test (above). The bridge has no caller yet.
