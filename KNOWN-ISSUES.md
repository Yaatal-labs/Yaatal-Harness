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

---

## supply-chain/rsa-marvin-attack-unreachable

**Where:** `rsa 0.9.10` pulled in transitively via `sqlx-mysql 0.8.6` (sea-orm default features).
**Owner:** sea-orm upstream (gating `sqlx-mysql` behind a feature would let us opt out).
**Status:** Accepted-transitive — code path is unreachable in our deployment.

**Finding.** `cargo audit` reports **RUSTSEC-2023-0071** (CVSS 5.9, medium): the Marvin
timing-sidechannel attack against RSA decryption. **No fixed upgrade is available
upstream.**

**Why it's unreachable for us.** The dependency tree is
`rsa → sqlx-mysql → sqlx-macros-core → sqlx → sea-schema → sea-orm-migration →
yaatal-api`. We run **Postgres only** — `DATABASE_URL=postgres://…`, `sea-orm` feature
set `["sqlx-postgres", "runtime-tokio-rustls"]`. The MySQL driver is never
instantiated at runtime, so the timing sidechannel cannot be exploited against
Yaatal traffic.

**Mitigation in place.** `.github/workflows/rust-ci.yml` runs `cargo audit
--ignore RUSTSEC-2023-0071` so CI still flags any **new** vulnerability while
not noisy-failing on this one.

**Long-term fix.** Track sea-orm for a release that gates `sqlx-mysql` behind a
non-default feature. Draft issue text for upstream (we can't file directly —
GitHub MCP scope is restricted to `yaatal-labs/yaatal-engine`):

> **Title:** Gate `sqlx-mysql` (and `rsa` transitive) behind a non-default feature
>
> **Body:** Projects that use `sea-orm` exclusively with Postgres still pull
> `sqlx-mysql 0.8.6 → rsa 0.9.10`, which carries the unfixed RUSTSEC-2023-0071
> Marvin attack advisory. The `rsa` code path is unreachable for Postgres-only
> deployments, but `cargo audit` flags it on every CI run.
>
> Could the `sqlx-mysql` re-export inside `sea-schema` / `sea-orm-cli` /
> `sea-orm-migration` be gated behind a `mysql` cargo feature that's off by
> default, mirroring the existing `sqlx-postgres` / `sqlx-sqlite` gating?
> Postgres-only consumers would set `default-features = false, features =
> ["sqlx-postgres", "runtime-tokio-rustls"]` and the `rsa` dep would never
> resolve. (Repo to copy-paste this into: https://github.com/SeaQL/sea-orm/issues/new)

**Re-evaluate when:**
- sea-orm ships the `mysql` feature gate.
- `rsa` ships a fix for RUSTSEC-2023-0071.
- We adopt MySQL anywhere (then the finding becomes reachable and must be
  re-triaged).

---

## supply-chain/fxhash-unmaintained

**Where:** `fxhash 0.2.1` pulled in transitively via `loco-rs 0.16.4 → scraper → selectors → fxhash`.
**Owner:** loco-rs upstream.
**Status:** Warning, not a vulnerability. RUSTSEC-2025-0057 — crate is no longer
maintained.

**Reachability.** Scraper is used by Loco-rs for HTML parsing in templating /
scaffolding tooling. Not a runtime cryptography surface. Continuing to depend on
`fxhash` is a maintenance hygiene concern, not an exploitation vector.

**Mitigation in place.** `.github/workflows/rust-ci.yml` runs `cargo audit
--ignore RUSTSEC-2025-0057` so CI doesn't fail on this warning.

**Long-term fix.** Wait for Loco-rs to retire `scraper` or for `scraper` upstream
to swap to `rustc-hash` / `ahash` / `foldhash`.

