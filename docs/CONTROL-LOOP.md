# The Minimal Closed Loop

Status: **implemented** (slices 1–5 built and tested; this document remains the
design of record). The loop runs in `yaatal-audit` (events, stores, metrics,
proposals), `yaatal-policy::tool_policy`, `yaatal-tools::audited_exec`,
`yaatal-evals::ops_run`, and `yaatal-runner` — which also pushes proposals to
the Engine's `/api/harness/proposals` review surface (see `docs/OPS-RUNNER.md`).
"(new)"/"(proposed)" markers below are kept as written; read them as the design
names for pieces that now exist under those crates.

## Goal

"AI-native, self-improving, sovereign" is a thesis until one runtime can be shown
running through it end to end. This document specifies the smallest thread that
makes the thesis demonstrable: one runtime, one model call, one tool call, one
audited record of both, one score, one policy check, one proposed change. Not a
platform. A proof that the pieces the charter describes — trust layer, feedback
loop, self-improvement — connect to each other rather than existing as separate
scaffolds.

Everything below is designed to be the *thinnest* version of the loop that still
touches every stage. Breadth (more runtimes, more tools, more eval types) comes
after the thread exists, not before.

## Where the runtime lives

Per the engine/app boundary that governs this workspace, Harness does not host
runtimes — `yaatal-api` is an excluded integration stub, and the README is explicit
that Engine (or Studio) calls Harness contracts, not the other way around. So the
"RUNTIME" box in the loop below is a process that lives in Yaatal-Engine (a
headless cron binary — e.g. a daily merchant-metrics agent that summarizes a
merchant's orders and drafts a note) or in Yaatal-Studio (the livestream agent
loop). What Harness owns is everything that runtime calls *through*: the model
call, the tool call, the audit record, the policy check, the eval, and the
proposal artifact.

This document specifies those contracts. It does not specify the cron agent's
business logic — that's Engine's or Studio's concern.

## The loop

```
RUNTIME  →  MODEL CALL  →  TOOL CALL  →  AUDIT  →  METRICS  →  EVAL  →  POLICY GATE  →  ADJUSTMENT
(Engine/     (yaatal-       (yaatal-      (new)      (new,       (yaatal-  (yaatal-       (new,
 Studio)      models)        tools)                   over        evals)    policy,        proposal
                                                       audit                 new trait)     artifact)
                                                       events)
```

### 1. RUNTIME executes via Harness contracts

The runtime is any process that holds a `RequestContext` (`yaatal-core`) for the
run and drives it forward. It does not call an LLM API or a shell command
directly — it goes through:

- **Model call**: `LlmProvider::chat` / `::embed` (`yaatal-core`, implemented by
  `OpenAiProvider`, `AnthropicProvider`, `FallbackRouter` in `yaatal-models`).
- **Tool call**: `ToolExecutor::execute` (`yaatal-tools`), which already emits
  `PipelineEvent::ToolStarted` / `ToolCompleted` to any registered `Observer`.

This is the one existing wiring point worth calling out: `ToolExecutor` already
has an `Observer` hook and a step counter. The audit stage below is a new
`Observer` implementation, not a new call site.

### 2. AUDIT — every action emits a structured event

**(new)** Every model call and every tool call emits one `AuditEvent` to a
persistent, append-only store. This is the load-bearing piece of the whole
design — without it, "audited" is just a word, and nothing downstream (metrics,
eval, policy, proposals) has anything to read.

Proposed minimal schema (illustrative field list, not a final Rust type):

```
AuditEvent {
    event_id:        Uuid,           // this event
    run_id:          Uuid,           // groups events into one run of the runtime
    actor:            String,         // e.g. "engine:merchant-metrics-cron",
                                       //      "studio:livestream-agent"
    action_kind:      ActionKind,     // ModelCall | ToolCall | PolicyCheck | EvalScore
    action_name:      String,         // model name, tool name, policy name, eval name
    input_digest:     String,         // hash of input, not raw payload (see below)
    output_digest:    String,         // hash of output
    cost:             Option<Cost>,   // token cost, tool cost class, or $ estimate
    latency_ms:       u64,
    policy_verdicts:  Vec<PolicyVerdict>, // empty if this event predates a gate
    started_at:       DateTime<Utc>,
    completed_at:     DateTime<Utc>,
}
```

Digests, not payloads: audit events record a content hash of input/output, not
the raw text, by default. Sovereignty tagging (`Sensitivity`, `Tagged<T, S>` — a
Yaatal-Engine concept, not a Harness one, but the constraint travels) means an
audit store that is queried by CLI or dashboard should not become a second copy
of user data. Full payloads may be retained behind the same storage dispatch
rules Engine already applies elsewhere; the default is digest-only.

Where it lives: proposed as a new `yaatal-audit` crate (the README already
anticipates the workspace growing crates like this — "e.g. `yaatal-agents` or
`yaatal-memory`"). It defines the `AuditEvent` schema and an `AuditSink` trait
(`emit(&self, event: AuditEvent) -> Result<(), AuditError>`) mirroring the shape
of `MemoryStore` in `yaatal-memory`. The in-process implementation is a
`ToolExecutor`/model-call `Observer` adapter that turns `PipelineEvent`s into
`AuditEvent`s; the durable backend is out of scope for Harness (Engine hosts
Postgres) — Harness ships the trait and an in-memory reference implementation
for tests, the same pattern `yaatal-memory::InMemoryStore` already follows for
`MemoryStore`.

### 3. METRICS — aggregates over audit events

**(new)** Nothing exotic: counts, success rate, p50/p95 latency, cost per run,
cost per actor, over a time window, computed by querying the audit store. This
is a read path over `yaatal-audit`, not a new subsystem — it can be a handful of
aggregation functions next to `AuditSink`, analogous to how `mean_reciprocal_rank`
and `ndcg_at_k` sit as plain functions in `yaatal-evals` rather than a service.

### 4. EVAL — one real eval scores the run

`yaatal-evals` currently has ranking metrics (MRR, NDCG) over `ScoredCandidate`.
The loop needs one eval that scores an *agent run*, not a ranked list. Proposed:
a single rubric-based `RunEval` — e.g. "did the merchant-metrics agent's note
match the actual order totals for that day" (task-success, checkable
mechanically) or an output-quality check (e.g. "is the note non-empty, under N
words, and free of a banned-phrase list"). Pick the mechanically-checkable one
first; a rubric that itself requires a judgment call (e.g. LLM-as-judge) is a
second milestone, not the first.

Input: the run's `AuditEvent`s (via `run_id`) plus whatever ground truth the
runtime's domain provides (e.g. Engine's order totals for that merchant/day).
Output: an `EvalResult` — pass/fail plus a score — that itself gets one more
`AuditEvent` (`action_kind: EvalScore`), closing the loop back into the audit
store.

### 5. POLICY GATE — checked in-path, not after the fact

`yaatal-policy` today defines `PolicyEngine` over `ScoredCandidate` (ranking
results). That trait doesn't fit tool calls or spend. Proposed: a new
`ToolPolicy` trait in `yaatal-policy`, checked by `ToolExecutor` (or a thin
wrapper around it) *before* a tool executes, not after:

```
ToolPolicy {
    fn check(&self, ctx: &RequestContext, tool_name: &str, arguments: &str)
        -> PolicyVerdict;   // Allow | Deny(reason) | AllowWithCap(remaining_budget)
}
```

Two rules for v1, deliberately minimal:

- **Tool allowlist**: a runtime's `RequestContext` carries (or resolves to) a
  named policy profile; the profile lists which tool names that actor may call.
  Anything not listed is denied before `Tool::execute` runs.
- **Spend cap per run**: a running total of `cost` across a `run_id`'s
  `AuditEvent`s; the gate denies further model/tool calls once the run's cap is
  hit.

Every verdict — allow or deny — is itself recorded on the `AuditEvent` for that
action (the `policy_verdicts` field above). A denied action is still an audited
action; it just doesn't execute.

### 6. ADJUSTMENT — the self-improvement segment

**(new)** This is the step that turns audit + metrics + eval into something that
resembles self-improvement, and it is the one place in this document where
"autonomous" needs the most restraint.

An `EvalResult` (or a batch of them, e.g. this week's merchant-metrics runs)
feeds a proposal step that outputs a **`ConfigProposal` artifact** — not a
config change. Examples of what a proposal can contain:

- a suggested change to model-tier routing weight (a value the 5-tier cascade
  router in `yaatal-core::ai` — Engine-side, not Harness-side — reads from
  config, not a code change),
- a suggested prompt variant to try next (a named alternative, not an
  in-place mutation of the current prompt),
- a suggested spend cap adjustment for a `ToolPolicy` profile.

The proposal artifact is data: `{ proposal_id, run_ids that motivated it,
eval_scores before/after (if this proposal was already trialed), the proposed
config diff, a human-readable rationale }`. It is written somewhere reviewable
(the audit store is the natural home — `action_kind: ProposalGenerated`) and
that is where v1 stops. See the autonomy ladder below for what happens to a
proposal after it exists.

## Autonomy ladder

Self-improvement is a promotion path, not a switch. Three rungs:

- **L0 — Observe.** Audit + metrics only. The loop runs, every action is
  recorded, aggregates are computable. No eval judgment, no proposals, no
  gating beyond the allowlist/spend-cap policy already in path (those are
  safety rails, not "improvement" — they run at L0 too).
- **L1 — Suggest.** Eval + proposal generation turned on. `ConfigProposal`
  artifacts are produced and land in front of a human for review. Nothing
  auto-applies. A human reads the proposal, the rationale, and the eval
  evidence, and decides.
- **L2 — Gated auto-apply.** A narrow, explicitly-scoped class of proposals
  (config-class only — routing weights, prompt-variant selection, spend caps;
  never schema, never payment logic, never code) may apply automatically, and
  only when: the eval that motivated the proposal passed a pre-declared
  threshold, the change is mechanically reversible (the previous config value
  is retained and a revert is one step), and the apply itself is an audited
  action like any other.

**Yaatal starts at L0.** Each promotion (L0→L1, L1→L2) is a deliberate decision
made once the previous rung has run long enough to be trusted, not a default
the system grows into on its own. Nothing in this design assumes L2 will
happen on any particular timeline — it may never happen for some proposal
classes.

## Sequencing — 4-5 shippable slices

Each slice below is independently shippable: it compiles, it has tests, and it
is useful even if the next slice never lands.

1. **Audit spine.** New `yaatal-audit` crate: `AuditEvent` schema, `AuditSink`
   trait, an in-memory reference implementation (mirrors
   `yaatal-memory::InMemoryStore`), and a `ToolExecutor` `Observer` adapter that
   turns existing `PipelineEvent`s into `AuditEvent`s. Testable in isolation:
   register the adapter on a `ToolExecutor`, run a tool, assert an `AuditEvent`
   landed. Touches: `yaatal-audit` (new), `yaatal-tools` (register the
   adapter), `yaatal-core` (no changes — `Observer`/`PipelineEvent` already
   exist).
2. **Model-call audit + metrics.** Extend the same `AuditSink` pattern to
   `LlmProvider::chat`/`::embed` call sites (a thin wrapping adapter, not a
   change to the trait), then add aggregation functions (count, success rate,
   p50/p95 latency, cost) over a `run_id`'s events. Testable: feed a fixture set
   of `AuditEvent`s, assert the aggregates. Touches: `yaatal-audit`,
   `yaatal-models` (wrap `MockProvider` first, in tests, before real providers).
3. **Policy gate in-path.** New `ToolPolicy` trait in `yaatal-policy` (allowlist
   + per-run spend cap), invoked by `ToolExecutor` before `Tool::execute`, with
   verdicts recorded on the `AuditEvent`. Testable: assert a denied tool call
   never reaches `Tool::execute` and still produces an audited deny event.
   Touches: `yaatal-policy` (new trait + one or two concrete policies),
   `yaatal-tools` (call the gate before execute), `yaatal-audit` (verdict
   field).
4. **One real eval.** One `RunEval` in `yaatal-evals` scoring a run via its
   `AuditEvent`s against a mechanically-checkable ground truth (task-success,
   not LLM-judged, for v1). Testable: fixture run + fixture ground truth in,
   `EvalResult` out, plus the `EvalScore` `AuditEvent` it emits. Touches:
   `yaatal-evals`, `yaatal-audit`.
5. **Proposal artifact (L1).** A proposal generator that reads `EvalResult`s
   and emits a `ConfigProposal` artifact (data only) into the audit store, with
   a CLI or minimal query surface to list pending proposals for human review
   (see `docs/CLI-FIRST-TOOLS.md`, `yaatal-audit` CLI). Testable: given a batch
   of passing/failing `EvalResult`s, assert the right proposal (or none) is
   generated. Touches: `yaatal-evals` (or a new thin module colocated with it),
   `yaatal-audit`.

Slice 5 is deliberately the ceiling for v1 — it produces an artifact a human
reads, nothing more. L2 auto-apply is a follow-on decision, made after slice 5
has run for real and the review step has demonstrated the proposals are worth
trusting, and is out of scope for this document.

## Non-goals for v1

- **No self-modifying code.** Proposals touch config values (routing weights,
  prompt-variant selection, spend caps), never source code. Nothing in this
  loop writes Rust, edits a controller, or changes a schema.
- **No autonomous schema or payment changes.** Both are explicitly excluded
  from the "config-class" changes eligible for L2 even in principle. Schema
  migrations and payment-path changes stay human-initiated, full stop.
- **No unaudited paths.** If a model call, tool call, policy check, or eval
  score doesn't produce an `AuditEvent`, it doesn't happen in this design —
  there is no "fast path" that skips the audit stage for convenience. The
  allowlist/spend-cap gate and the audit spine are the two pieces that make
  everything after them (metrics, eval, proposals) trustworthy, so neither is
  optional or deferred to a later milestone.
