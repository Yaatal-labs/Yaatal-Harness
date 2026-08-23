# Handoff — 2026-08-23, session closed on rate limits

Pick up here. Everything below is verified, not assumed.

## Where things stand

| Work | State |
|---|---|
| Pi slices 1–3 | ✅ merged and pushed, four gates green |
| Voice architecture + execution scope | ✅ `docs/VOICE-AGENT-ARCHITECTURE.md` |
| **V1 + V2** (transcribe leak, safety signals) | ✅ **written and green — patch in this folder, NOT applied** |
| V3 speech-core facade | ⏳ in flight when the session closed; Engine tree may be dirty |
| Pi slice 4 transport | ❌ designed, no code |

Branches: Harness `yaatal/pi-runtime` (pushed). Engine `claude/up-to-hit-it-1dap1e`, **unpushed, branch name still undecided** — the founder was asked whether Engine work should move to `yaatal/voice-agent` and had not answered.

## FIRST ACTION: apply the V1+V2 patch

`docs/handoff/track-a-v1-v2.patch` — 6 files, +308/−11. Verified `git apply --check` clean against Engine `2d1a06d`.

```bash
cd /home/user/Yaatal-Engine && git apply /home/user/Yaatal-Harness/docs/handoff/track-a-v1-v2.patch
```

It was produced in a scratch mirror of the Engine (the container's scratchpad, now gone) with **all four gates green**, including five new tests. It was never applied to the real checkout because a concurrent track was editing the same repo.

**Conflict warning:** the patch touches `crates/yaatal-voice/src/transcribe.rs` (a 12-line hunk adding `HF_INFERENCE_BASE_URL`). The V3 track owns that crate. If V3 landed changes first, that hunk is the likely conflict — resolve by keeping both.

## What V1+V2 actually does

- **V1** — `POST /api/voice/transcribe` refuses with 503 unless `VOICE_AUDIO_EXPORT_TO_HUGGINGFACE_APPROVED` is literally `"true"` (parsed exactly like `BYO_SOVEREIGN_APPROVED`: `trim().eq_ignore_ascii_case("true")`, so `"yes"`/`"1"`/typos are not approval). The refusal returns *before* `TranscriptionRouter::transcribe`, so no credential and no network is touched.
- **The "no outbound request" proof needed a seam.** With the HF URL hardcoded, a wiremock can never be reached and an empty `received_requests()` proves nothing. Hence `HF_INFERENCE_BASE_URL` (defaulting to the real host). The test asserts both halves: gate unset → 503 **and zero requests**; gate `"true"` → 200 **and exactly one** (proving the stub was reachable at all).
- **V2** — `privacy_sensitive` from `classify_data_class`, `network_available` from `NetworkGate`, both threaded into `routing_signals`.

## Traps the next session should not rediscover

1. **`classify_data_class` is in `yaatal_core::ai::sensitivity`**, not `ai/classify.rs` — that file holds the unrelated `classify_task`. `Sensitivity` is at `yaatal_core::policy::Sensitivity`.
2. **`DefaultNetworkGate` always returns `FourGPlus`** — the Engine's only impl. So `network_available` still reads `true` in production. V2 installs the seam, not the behaviour. Marked with a `ponytail:` comment.
3. **`force_edge` short-circuits before `force_cloud`** in `voice_routing::decide`. Enables a strong test: a transcript with two repair hints *and* a phone number would otherwise force Cloud, and routes Edge instead.
4. **An existing test pins the old leak.** `tests/requests/endpoint_inventory.rs::ai_voice_and_livekit_routes_have_testable_runtime_boundaries` asserts `/api/voice/transcribe` → **500**. It must become 503 and the approval var must join its `EnvGuard::clear` list, or the test goes env-dependent. The patch already does this.
5. **No true WS end-to-end for V2 without touching `Cargo.toml`** — `axum-test 17.3` has ws support but loco enables it without the `ws` feature, and `yaatal-api` has no ws client dep. The V2 test drives the real `TurnState → routing_signals → decide` seam instead. V1's test *is* a real endpoint test over HTTP.

## Pi slice 4 — design settled, no code

The transport is fully specified in the last dispatch brief; the load-bearing decision:

`beforeToolCall` returning `{block:true}` means Pi **never calls `execute`** (`agent-loop.js:419-427`), so `PiBridge::dispatch` is never reached and **no `Deny` audit event is written**. Fix: extract `AuditedExec::authorize(...)` out of `run_weighted` so `gate` and `execute` share exactly one deny path. Do not duplicate the deny branch into the RPC host.

Also: on `Err(Denied)`, `execute` must **return** an `AgentToolResult` with `terminate:true`, not throw — a thrown error becomes `createErrorToolResult(msg)` and drops `terminate` (`agent-loop.js:463-486`).

Protocol: NDJSON JSON-RPC 2.0, `plan` (Rust→Node), `gate` and `execute` (Node→Rust). Ids namespaced per direction (`r0…`/`n0…`) — unprefixed numeric ids collide legally across directions. Gate **fails closed** on timeout. A line >1 MiB without a newline is fatal, not skipped.

## Orchestration lessons — worth obeying

- **Worktree isolation is broken in this environment.** The session's primary dir is not a git repo, so isolation anchors to whatever repo is nearest — three agents got worktrees of an unrelated cloned repo and could write nothing. Run agents **without** isolation, with disjoint write sets, and **forbid them from committing**; the orchestrator reviews and commits. That also removes git index contention between two agents in one repo.
- **Write the composition test first.** Twice now, batches of green component tests hid defects only an end-to-end test caught.
- **Re-run gates yourself.** Reviewing rather than trusting reports caught a floating dependency pin last round.
