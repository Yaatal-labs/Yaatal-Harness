# Yaatal Engine — Manifest

> *"No choice is random in the Engine."*
> What each subsystem is, **why** it was chosen, what was rejected, what it deliberately is not —
> so the intent survives the code. **Honest framing:** today Yaatal is an **R&D project with a
> production-grade spine**. This document marks **`[V1]` = real and shippable now** vs
> **`[ROADMAP]` / `[VISION]` = everything else**. The detailed roadmap is dissected in a separate
> inventory/review pass — here we only bucket.
> Status **2026-06-06**. Companions: `README.md`, `SPRINT-LOG.md`, `CLAUDE.md`, and the
> Yaatal-Harness docs. `ARCHITECT-ENGINE.md`'s phase tracker is stale.

---

## 0. In one breath

Yaatal is a **sovereign, edge-to-cloud AI platform for African (Wolof/French-first) commerce and
assistance**, built constraints-first (cheap phones, weak networks, battery, code-mixed speech,
low-resource data, trust-sensitive money). Its defining primitive: **models propose, the Engine
disposes** — AI may *suggest* an action, but only the sovereignty-typed Engine turns intent into a
real effect (money, orders, PII), and `Sovereign` data is enforced *at compile time* to never leave
Senegal. BOBO is the first app surface, not the point.

---

## 1. The layered architecture (read this first)

Yaatal is **not one monolith** — it is three layers plus an R&D lane. This manifest is the **Engine
(runtime)** view; the Harness has its own.

```
APPS      BOBO · YOKK · NJOOBA · DAARA      product UX & journeys          [per-app]
  │  calls over HTTPS/JSON + JWT   ← @yaatal/client SDK lives on this arrow  [ROADMAP]
  ▼
ENGINE    runtime / control plane           auth · session · profile ·     [V1 spine]
  │                                          routes · deploy · "disposes"
  │  invokes in-process Rust contracts
  ▼
HARNESS   AI capability layer               models · search · memory ·     [scaffold]
  │  (Runtime Harness ships INSIDE Engine)   tools · policy · evals · voice
  ▼
PROVIDERS models · search · voice · tools                                   [R&D]

          R&D HARNESS ── proves / evals / hardens, then promotes ──┐        [R&D]
          (notebooks, training, datasets — does NOT ship to prod)  ┘
```

**Direction of dependency (from Yaatal-Harness):** *Engine depends on Harness, not the reverse.*
The Engine provides verified user/session/profile context and calls Harness pipelines through
explicit Rust contracts. **Boundary rules:** Harness never owns auth/profile/transport/routes/deploy;
Engine never owns app-specific UX; apps never duplicate AI reliability logic. **Promotion rule:**
*one app needs it → keep in app; two apps → promote to Engine; unproven AI → R&D Harness; proven AI
reliability → Runtime Harness.*

> **Alignment in progress:** the Engine repo still contains its own copies of capability crates
> (`yaatal-core/ai`, `search`, `feed`, `voice`). The target is for those to be **Harness-owned and
> Engine-consumed**; the name collisions and dependency direction are an active alignment task, not
> the finished state.

---

## 2. Prime directives (the non-negotiables)

| Directive | Why it exists |
|---|---|
| **Layer boundaries** — Apps / Engine / Harness, dependency one-way (Engine→Harness). | Keeps the platform from becoming three competing systems; lets capabilities be proven in R&D and promoted when hardened. |
| **Sovereignty is a type, not a setting.** | Data residency (Senegal / Diamniadio) must be un-bypassable. In the type system, the compiler is the auditor. |
| **African-first economics & networks.** | Metered compute, expensive/intermittent bandwidth, constrained devices. Prefer cheap/local/edge before frontier/cloud; degrade gracefully. |
| **Nothing hardcoded in Rust.** | Same binary, every environment. Config is Loco YAML + `${VAR}`; secrets never compile in. |
| **Promotion over duplication.** | App → Engine → Runtime Harness is a one-way ratchet earned by proof, not copy-paste. |

---

## 3. Status legend

- **`[V1]`** — real and shippable today; verified or trivially verifiable.
- **`[ROADMAP]`** — designed, partially built, or clearly next; not yet real.
- **`[VISION]`** — aspirational, gated by hard/uncertain research (the model program).
- Layer tags: **ENGINE** (runtime) · **HARNESS** (capability) · **SHARED** (core contracts) · **APP**.

---

## 4. Subsystem manifest — choice by choice

### 4.1 Loco — "Rust on Rails"  · ENGINE · `[V1]`
- **What:** batteries-included Rust web framework — routing, controllers, middleware, sea-orm, workers, mailers, JWT auth.
- **Why:** ship product, not plumbing, in a memory-safe, no-GC, low-footprint language suited to cheap VMs; Rust's types enable the sovereignty spine.
- **Rejected:** Axum-from-scratch (boilerplate); Node/Python (footprint, GC, weaker guarantees).
- **Is not:** a microservice swarm. The Engine is one runtime binary.

### 4.2 Sovereignty type system  · SHARED core · `[V1]`
- **What:** `Sensitivity { Sovereign · Operational · Public }`, a **sealed** `SensitivityTag` trait, zero-size markers, phantom-typed `Tagged<T, S>`. `Sovereign` = stays in Diamniadio Postgres only; never mirrors to edge.
- **Why:** residency in Senegal is a first-class invariant. Sealing makes misclassification a **compile error**, not a runtime incident.
- **Rejected:** runtime policy checks / config flags (bypassable, drift, fail open).
- **Is not:** a GDPR checkbox. *(Canonical home — Engine core vs shared crate — is part of the alignment.)*

### 4.3 Tag-gated storage  · SHARED core · `[V1]`
- **What:** `StorageDispatcher<T, S>` over `memory · postgres · r2`. **R2 only implements `…<T, Public>`** — a `Tagged<T, Sovereign>` to R2 **does not compile**; a compile-witness test proves it.
- **Why:** the edge (R2) is Public-by-construction; sovereign data physically cannot route there.
- **Rejected:** one store + ACLs (one bug from an edge leak).
- **Is not:** a place sovereign data can escape to the CDN.

### 4.4 Persistence — sea-orm + migrations  · ENGINE · `[V1]`
- **What:** typed models, migrations-as-code (`crates/yaatal-api/migration/`), applied on boot.
- **Why:** versioned, reviewable, self-applying schema; no manual prod DDL.
- **Scar tissue:** tests run on SQLite, prod on Postgres. A `uuid` column bound as a `String` passed every test and 500'd in prod (42804 / 42883); fixed `uuid → text`. **Green-on-SQLite ≠ green-on-Postgres — permanently.**
- **Is not:** raw SQL as source of truth (legacy `001_initial.sql` deprecated).

### 4.5 Auth · commerce · profile identity  · ENGINE · `[V1]`
- **What:** JWT auth, profile identity resolution, and the BOBO commerce *bridge* (orders, checkout, KYC, escrow, merchant).
- **Why:** this **is** the "disposes" half — the verified-context + execution layer that validates before anything real happens. The commerce/KYC/escrow controllers are exactly the disposal surface for future AI-proposed actions.
- **Rejected:** trusting client/model intent directly.
- **Is not:** app UX — these are app-agnostic primitives (the named BOBO bridge is the one deliberate seam).

### 4.6 Payments  · ENGINE · `[V1 partial]`
- **What:** normalized `contract.rs` + `RailSelector` + idempotent `EventStore` + webhook router. **Wave is real** (HMAC-SHA256); OM/FM/Card/Crypto stub to `RailNotConfigured`.
- **Why:** African payment fragmentation behind one contract; idempotent webhooks.
- **Roadmap seam:** `bobo_checkout` writes intents via raw SQL today — two payment paths to converge.
- **Is not:** Stripe-shaped, single-rail, US-centric.

### 4.7 LiveKit — real-time control plane  · ENGINE · `[V1 control / ROADMAP media]`
- **What:** Engine mints join tokens + receives webhooks; audio/video flows client ↔ SFU. **Unconfigured → 503** until keys are set.
- **Why:** the Engine owns auth/control, not the media path (latency/cost/scale).
- **Is not:** a media server.

### 4.8 AI cascade router  · HARNESS (in Engine today) · `[V1 orchestration / ROADMAP inference]`
- **What:** data-driven 5-tier router (`TierConfig`, not match arms): Tier 1 (on-device) → cloud (SiliconFlow → OpenRouter → HF) via `reqwest`. Guards: per-provider **circuit breaker**, **network gate**, **rate limit**, **task classify** (bilingual FR/EN), **sensitivity** routing.
- **Why:** cost + sovereignty + resilience — cheapest/most-local first, frontier last; sensitive prompts kept off third-party clouds; degrade, don't crash/overspend. Provider-swappable, no lock-in.
- **Status:** orchestration is real but **does no inference itself**; Tier 1 is an explicit placeholder (`"… will use local GGUF model in E7"`); with no keys → `AllTiersExhausted`. Target home is the **Harness**.
- **Rejected:** a single hardcoded provider (cost, lock-in, sovereignty failure).
- **Is not:** a thin proxy — it's a resilient, policy-/network-aware router.

### 4.9 Search · Feed · Voice  · HARNESS (in Engine today) · `[V1 search-runnable / ROADMAP feed,voice]`
- **Search** `[V1 partial]`: `/search` + `/index/upsert` over BGE-M3 sidecar + Qdrant; versioned profiles `canonical-1024`, `edge-512/256/128` (smaller for constrained devices). The most real AI piece.
- **Feed** `[ROADMAP]`: staged pipeline `sources → filters → scorers → hydrators → selectors` (ingestion separated from ranking) — scaffold.
- **Voice** `[ROADMAP]`: WebSocket session server, PersonaPlex-compatible **mock**; real speech behind off-by-default `speech-core-sys` (cmake/bindgen).
- **Why:** all three are **Harness capabilities** the Engine serves; edge-aware profiles apply the African-first constraint to retrieval.

### 4.10 Harness-native capabilities  · HARNESS · `[ROADMAP]`
`models · tools · memory · policy · evals · observability` — contracts (`Retriever`, `Ranker`,
`PolicyEngine`, `ModelAdapter`, `RequestContext`) plus scaffold/mocks. The reusable AI brain the
Engine will call. Real implementations are promoted out of R&D as they harden.

---

## 5. The model program  · VISION · `[R&D]`

The on-device/edge intelligence (from the R&D manifest) — **research stage, not in production**.
This is the "propose" side that the Engine "disposes."

| Lane | Model | Role | Status |
|---|---|---|---|
| Edge voice | Liquid **LFM2.5-Audio** | audio → tool-call / strict JSON, on phone | R&D smoke |
| Streaming ASR | **Nemotron 3.5 ASR** | Wolof/French/code-mix transcript | adaptation R&D |
| Duplex | **NeMo SpeechLM2 / SALM** | full-duplex S2S | early R&D |
| Translation | **Wolof-NMT / NLLB** | FR↔WO target text (auditable text seam) | data-gen + review |
| TTS | **xTTS-v2-wolof / FR** | target speech | synthetic, review-gated |
| Retrieval | **BGE-M3 Matryoshka** | search/memory | partially real (§4.9) |
| Cloud multimodal | **Nemotron Nano / Omni** | heavy reasoning, evaluator | prompt/eval first |

**Design choices that are not random:** the **text seam** (STT→text→MT→text→TTS) is chosen over
opaque direct S2ST for *auditability* — every hop is loggable, correctable, filterable. **Real
anchors + synthetic bridges** with a **human-review gate** fills the low-resource paired-data gap.
The compounding **Wolof commerce data moat** is the long-term differentiator.

---

## 6. On-device — phase E7  · ROADMAP

Two tracks, two meanings of "on-device":
- **Track A — engine/server-side sovereign inference:** fill the Tier-1 placeholder so `Sovereign`
  prompts are answered without leaving Diamniadio. Runtime options: `candle` / `llama-cpp-2` /
  `mistral.rs` / `ort`; small quantized GGUF; feature-gated; RAM/CPU budget on Railway.
- **Track B — true phone-side inference:** a model running *on the user's device* in BOBO native
  (ExecuTorch / MLC / llama.rn). **App work, not Engine work.** This is where the LFM2.5-Audio edge
  voice lands.

---

## 7. Where this sits — industry categorization  · analysis

A **vertically-integrated, sovereign, AI-native Backend-as-a-Service**, delivered as a Rust modular
runtime (Engine) over a capability layer (Harness). By surface it spans: **BaaS** (Supabase/Firebase),
**AI gateway/LLM router** (OpenRouter/LiteLLM/Portkey), **data-residency/policy-as-code** (OPA — but
compile-time), **headless commerce** (Medusa/Saleor), **vector search**, **payment orchestration**,
**CPaaS control plane**. One-liner: *a "sovereign Firebase for African AI" — with data-residency
compiled in, and an AI-action control plane no off-the-shelf BaaS offers.*

---

## 8. Beyond BOBO — the thesis  · VISION

BOBO is the first **tenant**. The Engine is reusable sovereign infrastructure for YOKK / BOBO /
NJOOBA / DAARA — and potentially other African startups, institutions, or governments needing a
**data-resident** AI backend the hyperscalers don't offer. The moat is four things they won't do:
**(1)** sovereignty by construction (compile-time residency), **(2)** African-first AI economics
(graceful, edge-first cascade), **(3)** edge-aware retrieval, **(4)** regional payment rails — plus
**(5)** the compounding low-resource-language (Wolof) commerce dataset that capital can't shortcut.

---

## 9. The V1 / Roadmap / Vision inventory

> Buckets only — the detailed roadmap is dissected in a separate review/inventory pass.

**`[V1]` — real & shippable today (the spine)**
- Engine runtime live on Railway: auth, profile, commerce/products/orders, persistence (private DB), verified end-to-end (frontend → engine → Postgres).
- Sovereignty type system + tag-gated storage (compile-time enforced).
- AI cascade *orchestration* (needs provider keys to do anything), search *runnable* (needs sidecar + Qdrant), Wave payments.

**`[ROADMAP]` — designed / partial / next**
- `@yaatal/client` SDK → one-command spin → `create-yaatal-app` (the spinnable/wireable ladder).
- Engine↔Harness alignment (dependency direction, de-duplicate crates, promote capabilities).
- Tier-1 on-device (E7 Track A), feed pipeline, voice (real), payments convergence, LiveKit config, CORS lock-down, email/SMTP, native BOBO build.

**`[VISION]` — research-gated (the ceiling)**
- The sovereign Wolof/French voice-commerce stack (edge voice → ASR → translation → TTS → duplex), the compounding data moat, multi-tenant SaaS, multi-region/DR, the "sovereign AI backend for African builders."

---

## 10. The two clocks (how to read all of the above)

- **Platform clock** — fast, shippable, compounding **now**: Engine → SDK → spin → scaffold. Uses
  off-the-shelf cloud AI for near-term product value. *This is the fastest path.*
- **Research clock** — slow, uncertain, high-ceiling: the sovereign Wolof model program. De-risked in
  the R&D Harness, promoted only when hardened.

The architecture (R&D Harness + promotion rule) is **literally designed to run both clocks without
merging them.** Ship the spine on the platform clock; let the ceiling arrive on the research clock.
**Today: an R&D project with a production-grade spine and a defensible research thesis** — a rare and
strong position, as long as the framing stays honest.

---

*Maintained alongside the code. If a choice here stops matching the repos, the code wins — update
this file (and `ENGINE-MANIFEST.fr.md`) in the same change.*
