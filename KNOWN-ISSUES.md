# KNOWN ISSUES

Issues inherited from upstream (yokk-engine) or external dependencies that are
**not** caused by this branch. Triage targets and owners are listed.

---

## voice-routing/cloud-recovery-state-skipped

**Where:** `crates/yaatal-api/src/services/voice_routing.rs::tests::cloud_route_sticks_then_recovers` (test is `#[ignore]`d)
**Owner:** yokk-engine team
**Origin:** Lane 0 merge of `origin/codex/deploy-candidate-yokk-engine`
**Status:** Test deterministically fails — not flaky.

**What the test expects.** The voice-routing state machine should walk
`EdgeActive → CloudActive (long turn) → EdgeActive (short follow-up) →
EdgeRecovery (third short follow-up)`. The recovery state is meant to
signal to downstream consumers that the session just came back from cloud
and may need a brief audio buffer re-prime.

**What actually happens.** The third `decide()` call returns
`VoiceRoutingState::EdgeActive` instead of `EdgeRecovery`. The recovery
window in `VoiceRoutingSession` either decays too fast or is never entered
in the first place. The transition logic in `VoiceRoutingSession::decide`
needs review.

**Assertion:**
```
assertion `left == right` failed
  left: EdgeActive
 right: EdgeRecovery
   --> crates/yaatal-api/src/services/voice_routing.rs:470:9
```

**Why we're not fixing it on this branch.** The current branch is BOBO /
Engine readiness work (Postgres cutover, payments wrapper, BOBO commerce
schema). The voice-routing recovery state machine is yokk-engine territory
and changing it without their authorship-context risks silently breaking
the Bo-Plex session lifecycle.

**To re-enable the test:** delete the `#[ignore]` attribute on
`cloud_route_sticks_then_recovers` after the recovery-state transition is
patched in `voice_routing.rs::VoiceRoutingSession`.

---

## yaatal-voice/alsa-sys system-dep on CI

**Where:** `cargo build --workspace` (default features) on a Linux runner without `libasound2-dev`.
**Owner:** this branch (yaatal-voice Cargo.toml)
**Status:** Worked-around at the build level — `cargo build -p yaatal-voice --no-default-features` is the recommended CI invocation.

**Why.** The `edge` feature pulls in `cpal = "0.15"`, which depends on
`alsa-sys` on Linux, which links against system `libasound`. The default
feature set deliberately excludes `edge` so server / CI builds don't need
ALSA. Local dev on a Linux laptop that wants real microphone capture
enables `edge`.

**Fix path (if you want one):** add a CI step `sudo apt-get install -y
libasound2-dev pkg-config` before any `--all-features` build.
