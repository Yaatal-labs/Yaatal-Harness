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

## Non-goals (reaffirmed + new)

- No from-scratch WhatsApp conversational agent (Meta commoditized it).
- No rebuilding ZeroClaw's channel/runtime layer from scratch.
- No unofficial/ToS-gray platform access in the governed runtime (TikTok Webcast
  stays out, per `SOCIAL-GATEWAYS.md`).
