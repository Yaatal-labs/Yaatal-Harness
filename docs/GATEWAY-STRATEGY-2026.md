# Gateway & Runtime Strategy — the ground shifted (July 2026)

> Status: decision doc. Written after a July 2026 reality check that changed
> what's worth building. Read alongside `SOCIAL-GATEWAYS.md` (which this updates)
> and the charter (`README.md`).

## Two findings that change the build

### 1. Meta Business Agent Platform — partners live **July 1, 2026**

Meta shipped an agentic platform: **its** AI agent runs the WhatsApp / Instagram /
Messenger conversation, and **connects to external systems** (Shopify, Zendesk,
Shopee) to take commerce action — read catalog, create orders, check status. Any
partner integrates via Meta's APIs. **Free until Aug 1, 2026**, then $2 / 1M
tokens (~4–5¢/message). 1M+ businesses; ~1B business↔customer conversations/day.

**Implication — do NOT rebuild a WhatsApp text-agent.** Meta just commoditized
the conversational NLU layer a from-scratch gateway would reinvent, and gives it
away free this month. (Ponytail rung 1 = YAGNI; rung 5 = a platform already
solves it.)

**The unlock — be the sovereign commerce backend *behind* Meta's agent**, the
way Shopify is. Meta brings the conversation + 1B-convo distribution; Yaatal
brings what Shopify can't for this market: sovereign, Senegalese, mobile-money-
native commerce + the physical loop (NFC delivery codes, livestream attribution).
This is the strategy docs' "ride the giants' rails, don't out-distribute them,"
now real — with a **free build window that closes Aug 1, 2026.** Time-sensitive.

**The wedge survives.** Meta's agent is text-first and major-language-first. It
will not lead with **Wolof voice for non-literate sellers.** That stays Yaatal's
moat, and it's the reason to keep a Yaatal-owned gateway for the voice/sovereign
path even while riding Meta for text.

### 2. ZeroClaw — the runtime the charter already describes

[ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw) (Rust, **MIT OR Apache-2.0**)
is the concrete "Claw/Hermes" the Harness README cites. Its layers map ~1:1 onto
Yaatal-Harness, and it already ships the parts we'd otherwise build from scratch:

| ZeroClaw | Yaatal-Harness today | Move |
|---|---|---|
| Channels gateway (30+: WhatsApp, Telegram, Signal, Discord, email, webhook…) | `SocialGateway` (designed, unbuilt) | **mirror/adopt — don't rebuild 30 channels** |
| SOP engine (event-triggered runbooks) | `yaatal-runner` (fixed runbook) | ours is a baby version of theirs |
| Security policy (supervised / YOLO autonomy) | L0/L1/L2 ladder + `ToolPolicyGate` | same idea, keep ours |
| **Cryptographic tool receipts** on every action | `AuditEvent` (`stdhash:` digest) | **steal — signed receipts fix the digest-stability gap** |
| **OS sandboxes** (Landlock / Bubblewrap / Seatbelt) | `AuditedExec` (policy only) | **steal — real containment, not just a gate** |
| Providers (Anthropic/OpenAI/Ollama/20+, fallback chains) | `yaatal-models` (test providers) | mirror the fallback-chain shape |
| Memory (SQLite + embeddings) | `yaatal-memory` (in-mem) | mirror when persistence is needed |

## The clarified architecture

The Harness's real job is **not** to own channels or an agent loop — those are
increasingly free (ZeroClaw) or commoditized (Meta). The Harness is the
**governance + sovereignty + commerce + evals control plane that wraps whatever
runtime executes**:

```
   Meta Business Agent  ┐
   ZeroClaw runtime     ├─► every action ─► YAATAL-HARNESS ─► audit · policy ·
   Yaatal voice gateway ┘     (custody)      (the moat)       sovereignty · human-gate
                                                                   │
                                                                   ▼
                                                            Yaatal Engine
                                                     (catalog · orders · escrow ·
                                                      delivery codes · attribution)
```

Channels are the commodity; **governed, sovereign, auditable action is the moat.**

## Decisions needed (founder)

1. **Meta path** — pursue being a connected commerce system behind Meta's
   Business Agent? *Rec: yes.* Distribution unlock the strategy always needed;
   free build window closes **Aug 1, 2026** (partnerships/founder track — apply
   now, build the Engine integration against it).
2. **ZeroClaw** — **adopt/fork** (fastest: reuse 30 channels + sandbox + SOP) vs
   **mirror** (Yaatal-owned runtime, more control, more work)? *Rec: adopt its
   channel + sandbox + receipt patterns; keep Yaatal-Harness as the governance
   layer on top. Don't rebuild what it already ships MIT/Apache.*

## What's worth building regardless of the forks (unblocked now)

1. **Signed tool receipts** — upgrade `AuditEvent` from an unstable `stdhash:`
   digest to a signed receipt (ZeroClaw's "cryptographic tool receipts"). Fixes
   the exact digest-stability gap the PR review flagged, and makes the audit
   trail tamper-evident — load-bearing for the "prove sovereignty" story.
2. **Sandbox wrapper** around `AuditedExec` — contain external commands
   (Landlock on Linux; documented no-op elsewhere), so custody is real
   containment, not only a policy check.
3. **Engine "agent commerce actions" facade** — the thin, governed surface Meta's
   agent (or any runtime) calls: catalog-search / create-order / order-status /
   confirm-delivery. Mostly *exists* (products/orders/deliveries) — needs an
   agent-facing, rate-limited, audited front, not a rebuild.

Items 1–2 are pure Rust in the Harness, no external access needed → build now.
Item 3 waits on the Meta path decision (it's the integration surface).

## Tradeoffs & risk (the eyes-open version)

### Meta Business Agent — decided YES, but it is a *channel, not a home*

Real tradeoffs, and one sharp tension with Yaatal's own thesis:

1. **Disintermediation.** Meta's agent owns the conversation, the customer
   relationship, and the conversational data. Yaatal becomes a backend. That
   conversational data *is* the voice-data/consent moat — routing it through
   Meta hands the top of the funnel (and the corpus) to Meta.
2. **Sovereignty tension — the sharp one.** Yaatal's entire pitch (NDT, grants,
   government) is *sovereign, data stays in Africa*. Meta's agent runs on
   global infra. Never route the sovereign / Wolof-voice / consent-data flows
   through it — that would contradict the thesis you sell.
3. **Cost + control.** ~4–5¢/message, Meta sets the price (free → billing Aug 1;
   they can raise it), thin African-commerce margins feel it.
4. **Wedge reach vs erosion.** Meta's agent is text-first — it does **not** serve
   your non-literate voice sellers, so riding it reaches the *literate/text/
   diaspora* segment, not your core wedge. And Orange is already doing Wolof AI
   with OpenAI/Meta (landscape doc) — Meta could erode the voice wedge from above
   over time.

**Resolution:** ride Meta as a **distribution channel for the text/literate
segment** (fast "live," real reach), but keep the **sovereign Wolof-voice path as
the owned, differentiated product and the only home for consent-data**. Meta is a
front door, not *the* front door. This is the strategy docs' "mini-app on the
super-apps" posture — ride the rails, don't become dependent, don't route the
crown jewels through them.

### ZeroClaw — bite-back check

- **License: clean.** Dual **MIT OR Apache-2.0** — no copyleft, no commercial
  restriction. You may fork, modify, keep your additions proprietary, ship
  commercially. The upstream **CLA** only governs contributing *back*; it cannot
  claw back the version you fork — the existing MIT/Apache release is
  irrevocable. ✅ No copyright/license risk.
- **The real risk is maturity, not law.** Open-sourced **2026-02-19** (~4½ months
  old), community-led (@theonlyhennygod created, @JordanTheJet leads; no big-co
  backing), and one of several OpenClaw reimplementations (NanoClaw, Moltis…).
  Risks: API churn, breaking changes, small bus factor, possible
  abandonment/consolidation. "Known secure and working" is optimistic for
  something this young — it's *promising and permissively licensed*, not battle-
  hardened.
- **Security surface.** It executes shell/browser/hardware. Audit the subset you
  adopt (sandbox + tool-exec especially) — don't blind-trust a young runtime in a
  payments-adjacent stack.

**Decision: FORK and pin — do not live-depend on upstream.** Vendor ZeroClaw at a
known-good commit, take the subset (channels + sandbox + signed receipts + SOP
patterns), audit it, and pull upstream patches *selectively*. Forking:
(a) satisfies **sovereignty** (you own and self-host the runtime),
(b) **insulates** you from upstream churn / relicensing / abandonment, and
(c) is explicitly **permitted** by the license. This is exactly what large labs
do with permissive upstreams — adopt the working core, own the fork. Yaatal-
Harness stays the governance + sovereignty + commerce + evals layer *on top of*
the forked runtime.

**First fork step (concrete):** vendor `zeroclaw-channels` + the sandbox +
receipt modules into a `yaatal-runtime/` (fork), wire one channel (WhatsApp or
CLI) through the existing `ToolPolicyGate` + audit, prove one governed round
trip. Then delete/park the from-scratch `SocialGateway` build — the forked
channel trait replaces it.

## Timing — the YAGNI verdict (do NOT build the runtime/fork yet)

Ran the two options up the Ponytail ladder honestly:

- **Extend Harness with a from-scratch gateway now** → rebuilds ZeroClaw *and*
  serves no live load. Fails rung 1 and rung 2.
- **Fork ZeroClaw now** → takes on the maintenance + security-audit burden of a
  4½-month-old runtime for **zero live channels**. The thing that would *consume*
  a multi-channel runtime — Yaatal's own sovereign agent / Wolof-voice product —
  **isn't built yet**. So this is a solution looking for a load. That's the
  *cop-out* kind of lazy (reach for a shiny dep to feel productive), not the
  *disciplined* kind.

**Verdict: neither now. This is disciplined YAGNI, not procrastination —**
because the durable, cheap-to-do-early parts are **already done**:
1. the `SocialGateway`/`SocialEvent` contract is **designed** (`SOCIAL-GATEWAYS.md`),
2. the custody layer (`ToolPolicyGate` + `AuditEvent` + `AuditedExec`) **exists
   and is channel-agnostic** — nothing couples to a specific platform, so
   adopting a channel later is cheap,
3. the fork-when-needed decision is **recorded** (above).

Deferring is not just neutral — it **compounds in our favour**: fork ZeroClaw
later and you fork a **more battle-tested** runtime (today it's 4½ months old),
you fork **only the one channel you actually need** (not 30), and you audit less.
YAGNI here literally makes the eventual adoption cheaper and safer.

**The trigger to build (any one of these):**
- a real seller needs WhatsApp/social order-taking on the **sovereign voice
  path** that Meta's agent doesn't serve (non-literate, Wolof), **with volume**; or
- the ops runner needs to drive tools/channels beyond the `yaatal` CLI; or
- the Meta partnership lands → build the **thin Engine agent-commerce facade**
  (a few endpoints, *not* the runtime) against the real partner spec.

Until a trigger fires, the **one** governed workload (the ops runner over the
CLI) is enough. Don't fork a runtime to police channels that don't exist yet.

**The only time-boxed item** is the Meta partner free window (Aug 1) — and that's
a partnerships action + a thin facade, decoupled from the runtime question above.

## Non-goals (reaffirmed + new)

- No from-scratch WhatsApp conversational agent (Meta commoditized it).
- No rebuilding ZeroClaw's channel/runtime layer from scratch.
- No unofficial/ToS-gray platform access in the governed runtime (TikTok Webcast
  stays out, per `SOCIAL-GATEWAYS.md`).
