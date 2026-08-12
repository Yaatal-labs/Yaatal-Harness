# Pi as Yaatal-Harness's agent runtime

## Context

Yaatal-Harness is the AI control plane — runtime custody, audit, behavioural
policy, and the self-improvement loop. Its README names two gaps plainly:

> **No general runtime adapters.** There is no Claw integration and no Hermes
> integration; the ops runner is the only tenant of the custody path.
> **No model-driven planning.** The runner executes a fixed runbook; nothing
> plans actions with an LLM yet.

**Pi** (`earendil-works/pi`, MIT, pi.dev) is the candidate tenant: a minimal
agent harness — four built-in tools, a <1000-token system prompt, a typed
TypeScript extension system, and four run modes including **RPC (JSON-RPC 2.0
over stdin/stdout)**. Packages split as `pi-ai` (providers), `pi-agent-core`
(the loop), `pi-coding-agent` (coding runtime), `pi-tui`.

Pi appears **nowhere** in any of the five Yaatal repos today — this is new.
`docs/GATEWAY-STRATEGY-2026.md` evaluated **ZeroClaw** and landed on *"mirror
the pattern, own the adapters — do not fork,"* but that verdict was about a
30-channel *gateway*, a different axis. Pi is the agent loop, and it inverts the
objection that killed ZeroClaw: the complaint was carrying 26 unused channels;
Pi's whole pitch is four tools.

The founder has ruled that this is load-bearing and needed sooner than the
codebase's current state suggests, overriding an earlier YAGNI read.

**Intended outcome:** one governed agent runtime, with multiple agentic roles
sharing a single custody path, so Harness stops being a control plane with only
a fixed-runbook tenant.

## The architecture decision that shapes everything

**VERIFIED 2026-08-11 against `@earendil-works/pi-agent-core@0.84.1`** — and the
answer is better than assumed, so the design got simpler.

The concern was real: Pi's security docs say *"Extensions are TypeScript modules
that run with the same permissions as Pi"*, Pi *"does NOT sandbox tool execution
by default"*, and *"real isolation needs to come from the operating system or a
virtualization/container boundary."* An agent holding `write` and `bash` could
edit the extension governing it, making an in-process `tool_call` hook a
suggestion rather than a door — and the Harness charter demands a door.

**But that applies to `pi-coding-agent`, the opinionated CLI runtime — not to
`pi-agent-core`.** Inspection of the shipped types shows:

- The built-ins are **opt-in factory functions** — `createBashTool`,
  `createEditTool`, `createReadTool`, `createWriteTool` — exported from
  `harness/tools/index`. Nothing registers them for you.
- `tools?: HarnessTool[]` is **optional** on `AgentHarnessOptions`
  (`dist/harness/agent-harness.d.ts:325`). Omit it and the harness has no tools.
- `setTools(tools, activeNames?)` exists for later mutation (`:444`) — a
  mutation surface to keep on the Rust side, never exposed to the model.

So there is nothing to remove or block: **at this layer Pi ships zero native
tools, and Harness supplies 100% of the tool set.** `pi-agent-core` also
describes itself as a *"General-purpose agent with transport abstraction, state
management, and attachment support"* — it is not coding-specific.

**Decision: depend on `pi-agent-core`, never `pi-coding-agent`.** Every tool is
a Harness-defined stub that emits an intent; the Rust process decides and
executes. Pi is a planner that can only propose.

This is not a new trust architecture. It is exactly the shape
`yaatal-edge-turn` already has (model proposes → Harness validates → Harness
acts), with a better planner behind it.

Probe kept at `scratchpad/pi-spike/probe.mjs`; it should be re-run and promoted
into the bridge crate's test suite so a future Pi upgrade that auto-registers a
tool fails loudly.

## Agentic roles: configuration, not integrations

All four roles are in scope. They do **not** each need an integration, because
`ToolPolicyGate::with_actor_allowed(actor, allowed)`
(`crates/yaatal-policy/src/tool_policy.rs:91`) already supports per-actor
allowlists over a shared default. A role is therefore:

> **actor id + tool allowlist + system prompt + model choice**

| Role | Actor | Tool allowlist |
|---|---|---|
| Ops-runner planner | `ops-runner` | runbook tools (existing) |
| Studio livestream | `studio-live` | the three `edge-turn` overlay tools |
| Merchant assistant | `merchant-agent` | read-mostly catalogue/order queries |
| Internal dev/ops | `dev-agent` | repo CLIs, no production surface |

Build the bridge once; each role is a manifest entry plus a policy line.

## Phases

Each lands independently and leaves the workspace compiling
(`cargo check --workspace --all-targets`, the repo's declared gate).

### Phase 1 — Bridge, ops-runner role only
- New crate `crates/yaatal-pi-bridge`: spawn the Node side, speak JSON-RPC 2.0
  over stdin/stdout, expose a `ToolIntent { actor, run_id, tool, arguments }`
  channel.
- Node side: a small ESM entrypoint that builds an `AgentHarness` with `tools`
  populated **only** from the Harness-supplied manifest. It must not import
  `createBashTool` / `createWriteTool` / `createEditTool` / `createReadTool` at
  all — enforce with a lint or a grep-based test, since importing them is the
  only way they can appear.
- Pin exact: `@earendil-works/pi-agent-core@0.84.1`.
- Route each intent through the existing `ToolPolicyGate::check`, then
  `AuditedExec::run` (`crates/yaatal-tools/src/audited_exec.rs:93`) on Allow.
  Return a model-readable refusal on Deny. **No new policy engine** — the
  decision logic exists and has had no executor to attach to.
- Dependency posture: exact-version npm pin on `@earendil-works/pi-agent-core`,
  revisit vendor-vs-pin at the end of this phase (founder decision: decide here).

### Phase 2 — Audit completeness
- `AuditedExec` already emits one `AuditEvent` per invocation. Add `ModelCall`
  events from Pi's `turn_start`/`turn_end` so a run reconstructs end to end.
- `yaatal-evals::ops_run` then scores Pi runs with no changes, and
  `yaatal-audit::proposals` generates `ConfigProposal`s over them unchanged.

### Phase 3 — Sovereignty via the Engine gateway
- Intercept `before_provider_request`; route Pi's LLM calls to the Engine's
  `/api/ai/chat` rather than letting `pi-ai` call providers directly.
- This puts every planner turn through `route_declared` (one classifier), the
  5-tier cascade, the circuit breakers and the shared token budget — the
  `OnceLock` gateway in `crates/yaatal-api/src/controllers/ai.rs:50`.
- Founder decision: gateway, not direct. Do not duplicate the classifier.

### Phase 4 — Remaining roles
Add `studio-live`, `merchant-agent`, `dev-agent` as actor + allowlist + prompt
entries. `studio-live` reuses `edge-turn`'s existing validation
(`crates/yaatal-edge-turn/src/contract.rs`) rather than re-deriving limits.
**Note:** `studio-live` is only end-to-end demonstrable once STT exists — Studio's
`live/agent_loop/stt_listener.py` raises `NotImplementedError` on both backends.

### Phase 5 — Process isolation
Even with zero native tools, the Pi process runs TS extensions unsandboxed. Per
2026 practice (microVM > gVisor > hardened container) and Pi's own docs, run it
containerised: minimum mounts, no host `~/.pi`, least-privilege credentials,
restricted network.

## Critical files

- **New:** `crates/yaatal-pi-bridge/` (spawn, RPC, intent routing)
- `crates/yaatal-policy/src/tool_policy.rs` — reuse `ToolPolicyGate`,
  `with_actor_allowed`; no changes expected
- `crates/yaatal-tools/src/audited_exec.rs` — reuse `AuditedExec::run`
- `crates/yaatal-audit/src/lib.rs` — `AuditEventBuilder`, `digest()`
- `crates/yaatal-runner/src/` — the runner becomes the bridge's first caller
- `crates/yaatal-edge-turn/src/contract.rs` — reuse for the `studio-live` role
- SDK `src/cli.ts` — the `yaatal` CLI is the tool surface bridged tools wrap

## Verification

- Per phase: `cargo check --workspace --all-targets` and `cargo test --workspace`
  (both currently pass with zero failures).
- **Phase 1 must leave a runnable check** proving the security property, not just
  the happy path: a test asserting Pi boots with **zero** built-in tools
  registered, and one asserting a Denied intent produces no execution and one
  `AuditEvent` carrying `PolicyVerdict::Deny`.
- Phase 2: assert a full run reconstructs from the audit store — `ModelCall` and
  `ToolCall` events under one `run_id`, in order.
- Phase 3: assert a Sovereign-classified turn is refused pre-flight with no
  outbound provider request (mirror the existing `gateway.rs` wiremock test that
  checks `server.received_requests()` is empty).

## Known constraints found during research

- **TypeScript 7.0 (GA 8 July 2026, Go-native) has no stable programmatic API**,
  so `ts-jest`, `typescript-eslint` and `ts-morph` cannot run on it; that API is
  targeted for 7.1. **BOBO uses `ts-jest` 29 on TS 5.3.3 and therefore cannot
  move to TS 7 yet.** SDK is on TS 5.9.3. Do not pin the Pi bridge's toolchain to
  TS 7 expecting the ecosystem to follow.
- **Node**: type stripping is default in Node 24 LTS and stable/unflagged in
  Node 26 (Active LTS Oct 2026). SDK and BOBO both declare `node >= 18`.
- Adding Pi puts a **Node runtime beside a Rust control plane**. Subprocess/RPC
  keeps the boundary clean, but it is a second toolchain in the deploy image.
- **ACP (Agent Client Protocol) is the wrong seam** — it standardises
  editor↔agent, not control-plane↔agent. Do not reach for it here.
- Pi moved npm scope (`@mariozechner/*` → `@earendil-works/*`); pin accordingly.
- `can1357/oh-my-pi` is community prior art for custom-tool distributions.

## Explicitly out of scope

- No agentic surface touches the money path. Payments, escrow, settlement and
  PI-SPI credentials stay Engine-side. A request for "the AI can take the order"
  is a new trust boundary, not a new tool.
- No L2 auto-apply. The loop keeps suggesting; a human keeps approving.
- Nothing here competes with the pilot critical path (PI-SPI credentials,
  Coolify deploy + scheduler, first real e2e).
