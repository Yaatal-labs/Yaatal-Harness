# Task: Engine ↔ Harness Inventory Pass — Roadmap Dissection

> **Purpose.** Turn the `[ROADMAP]` placeholder in `ENGINE-MANIFEST.md` into a *real, sequenced
> plan* by walking both repos **crate-by-crate / module-by-module** and bucketing each into
> **V1 / Roadmap / Vision** against the Engine↔Harness layer boundary — with **evidence**, not vibes.
>
> **Built for async/mobile execution.** Each unit below is one short sitting. Do one, fill its row,
> check the box, save. Fully resumable — no unit depends on another.

---

## Mobile kickoff prompt (paste this in Claude Code)

> *"Open `docs/plans/2026-06-06-engine-harness-inventory.md`. Do the next unchecked unit in §4 using
> the method in §3. Append its filled row to §5, check the box, and stop. Don't run the whole list —
> one unit."*

---

## 1. Context (so a cold session needs nothing else)

**Two repos:**
- **Engine** = *this* repo (local). Runtime / control plane. Crates under `crates/`.
- **Harness** = `Yaatal-labs/Yaatal-Harness` @ branch `codex/harness-engine-alignment` (remote).
  Read with `gh api repos/Yaatal-labs/Yaatal-Harness/contents/<path>?ref=codex/harness-engine-alignment --jq '.content' | base64 -d`
  or `gh api .../git/trees/codex/harness-engine-alignment?recursive=1`.

**Layer model:** `Apps → Engine (runtime) → Harness (capabilities) → providers`, plus an **R&D
Harness** lane that proves/hardens before promotion.

**Boundary rules:** Harness never owns auth/profile/transport/routes/deploy. Engine never owns app
UX. Apps never duplicate AI reliability logic. **Dependency is one-way: Engine → Harness.**

**Promotion rule:** one app needs it → app · two apps → Engine · unproven AI → R&D Harness · proven
AI reliability → Runtime Harness.

**Status legend (the buckets):**
- **V1** — real & shippable *today*, with evidence (see §3).
- **ROADMAP** — designed / partial / scaffold / not-wired / clearly next.
- **VISION** — research-gated (the model program), or not started.

Framing source of truth: `ENGINE-MANIFEST.md` (+ `.fr.md`). This task fills its §9 inventory.

---

## 2. Output

A filled table in **§5 of this file** (one row per unit). When the checklist is complete, the §5
table *is* the dissected roadmap — promote it into `ENGINE-MANIFEST.md` §9 and regenerate the HTMLs.

---

## 3. Method — how to inventory ONE unit (the discipline)

For the unit, do exactly this, then write its row:

1. **Read it.** Open the crate's `lib.rs` / `mod.rs` / `Cargo.toml`; skim the public surface and the
   `[features]`.
2. **Belongs (layer).** Where *should* it live per the boundary rules? `Engine` / `Harness` /
   `Shared-core` / `App`.
3. **Lives now.** Which repo(s) actually contain it. **Flag collisions** (same crate name in both
   repos → note divergence).
4. **Status — with EVIDENCE (the part that matters):**
   - Mark **V1** only if you can cite proof: *compiles* **and** (has real tests **or** is wired into
     `app.rs`/routes **or** verified live). Say *which*.
   - Mark **ROADMAP** if it compiles but is scaffold/stub/not-wired, or designed-only.
   - Mark **VISION** if it depends on unfinished model research, or isn't started.
   - **Hunt for fakery:** grep the unit for `todo!`, `unimplemented!`, `unreachable!`, `placeholder`,
     `mock`, `not yet`, `#[ignore]`, `RailNotConfigured`-style stubs, empty/`Ok(())` bodies. These
     demote a unit out of V1.
5. **Gap-to-real.** One line: what would move it up a bucket.
6. **Target / action.** Where it should ultimately live + the action (keep / move to Harness / make
   Engine depend on Harness's copy / delete duplicate / wire into routes / promote).

Keep each row terse — this is a map, not an essay.

---

## 4. The checklist (each = one sitting)

### Engine repo — runtime & shared core
- [ ] **E1 · `yaatal-core/policy`** — sovereignty types (Sensitivity/Tagged/SensitivityTag)
- [ ] **E2 · `yaatal-core/storage`** — StorageDispatcher + memory/postgres/r2 gating
- [ ] **E3 · `yaatal-core/ai`** — 5-tier router, circuit_breaker, network, rate_limit, classify, sensitivity
- [ ] **E4 · `yaatal-core` (rest)** — models, remaining modules
- [ ] **E5 · `yaatal-api/controllers`** — auth, products, orders, merchant, feed, posts, comments, livekit, voice, bobo_* (note which are wired in `app.rs`)
- [ ] **E6 · `yaatal-api/services`** — profile_identity, bobo_commerce, voice_session, xp_service, analytics dispatcher
- [ ] **E7 · `yaatal-api/migration`** — migrator + migrations (which tables real/applied)
- [ ] **E8 · `yaatal-commerce`**
- [ ] **E9 · `yaatal-payments`** — contract, RailSelector, EventStore, Wave vs stubs
- [ ] **E10 · `yaatal-feed`** (Engine copy)
- [ ] **E11 · `yaatal-search`** (Engine copy)
- [ ] **E12 · `yaatal-voice`** (Engine copy; check `speech-core-sys` feature)
- [ ] **E13 · `yaatal-analytics`**

### Harness repo — capability layer (`@ codex/harness-engine-alignment`)
- [ ] **H1 · `yaatal-core`** — RequestContext/Candidate/Retriever/Ranker/PolicyEngine/ModelAdapter traits (vs Engine's yaatal-core — **collision**)
- [ ] **H2 · `yaatal-search`** — retrieval/rerank/streaming (vs Engine's — **collision**)
- [ ] **H3 · `yaatal-models`** — provider adapters + test providers
- [ ] **H4 · `yaatal-tools`** — tool execution contracts + prototype local tools (flag "dangerous" ones)
- [ ] **H5 · `yaatal-memory`** — memory store
- [ ] **H6 · `yaatal-policy`** — policy engines
- [ ] **H7 · `yaatal-feed`** (vs Engine's — **collision**)
- [ ] **H8 · `yaatal-voice`** (vs Engine's — **collision**)
- [ ] **H9 · `yaatal-observability`**
- [ ] **H10 · `yaatal-evals`**
- [ ] **H11 · `yaatal-api` (stub)** — confirm it's excluded from the compiled workspace

### Cross-cutting (do after the crates)
- [ ] **X1 · Collision ledger** — for every duplicated crate name, decide canonical home + merge/delete action
- [ ] **X2 · Dependency direction** — does the Engine yet depend on Harness? what's the wiring gap?
- [ ] **X3 · App seam** — the `@yaatal/client` SDK surface implied by the V1 Engine endpoints (feeds brick-1 design)

---

## 5. Results (fill one row per unit)

| Unit | Repo · path | Belongs | Lives now | Status | Evidence | Gap-to-real | Target / action |
|---|---|---|---|---|---|---|---|
| _ex_ E2 | Engine · `yaatal-core/storage` | Shared-core | Engine | **V1** | compiles + compile-witness doctest proves R2≠Sovereign | n/a | keep; confirm canonical home in alignment |
| | | | | | | | |

*(append rows above this line as you go)*

---

## 6. Definition of done
- Every checklist box ticked, every unit has a row in §5 with a **status + evidence**.
- §X1 collision ledger resolved into concrete actions.
- §5 promoted into `ENGINE-MANIFEST.md` §9 (replace the placeholder buckets with the evidenced ones),
  then regenerate `ENGINE-MANIFEST.html` / `.fr.html`.

## 7. Mobile execution notes
- **One unit per sitting.** Each is self-contained; order doesn't matter (start with Engine crates —
  they're local and don't need `gh`).
- After each unit: append the row, tick the box, save. (Commit later in a batch when you're back.)
- If a unit balloons, split it — e.g. E5 controllers can be one-per-controller across several sittings.
- Don't trust the existing manifest's guesses — this pass exists to *replace* them with evidence.
