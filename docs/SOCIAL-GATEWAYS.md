# Social Runtime Gateways — governed access to where sellers already are

> Status: design, grounded in a July 2026 platform-reality check. Nothing here
> is built yet. This is the base the adapters get built against.

## Why this is the Harness's real job

The Harness is not "audit for its own sake" — audit is the evidence layer under
its actual purpose: **governed runtime access to the outside world.** Social
platforms are the sharpest case. Their APIs are rate-limited, ToS-bound,
24-hour-windowed, and abuse (spam, bulk DMs, off-window sends) gets you
**shadow-banned, rate-throttled, or terminated** — often irreversibly, and often
for a whole business account, not one action.

That means the three things the Harness already has are not nice-to-haves here,
they are the only way to touch these platforms safely:

- **Rate-limit-as-policy** — per-platform send caps and the 24-hour-window rule
  are `ToolPolicyGate` rules checked *before* every outbound action, not hopes.
- **Audit** — every inbound event and outbound action is an `AuditEvent`
  (existing schema). When a platform flags you, the trail is the defense.
- **Human gate** — business-initiated / paid / bulk actions become L1
  `ConfigProposal`-style approvals (existing review surface); only in-window
  service replies run unattended.

A social gateway that is *not* behind this custody is a ban waiting to happen.
This is the use case that turns the control plane from "admirable" into
"load-bearing."

## Platform reality (verified July 2026 — re-check before building each)

| Platform | Inbound (comments/DMs) | Outbound | Access gate | Verdict |
|---|---|---|---|---|
| **WhatsApp Cloud API** | ✅ official webhooks (messages + status) | ✅ official; per-message billing (templates), **24h service window** replies cheap/free | Meta Business + WABA + phone #; App Review for scale; BSP optional at low volume | **Build first** — the substrate of African commerce, official, most attainable |
| **Facebook Live comments** | ✅ official Graph *Live Video* SSE `live_comments` | ✅ comment/reply on Page live videos | **App Review** + your app must produce the RTMPS stream (first-party) | **Build second** — attainable *because* OBS→Yaatal produces the stream |
| **Instagram DM** | ✅ official Messaging API webhooks | ✅ within 24h window | Business/Creator acct + **business verification + App Review** (weeks) | **Third** — same Meta rails as WA/FB; approval-gated |
| **TikTok Live comments** | ❌ **no official API/webhook** (official webhooks cover auth/video events only) | virtual-camera bridge only (already in Studio) | — | **Park** — only unofficial 3rd-party Webcast libs exist (ToS-gray, ban risk); no compliant path today |

Sources checked: Meta WhatsApp Cloud API pricing + webhooks docs; Meta Graph
Live Video API `live_comments` + FAQ (RTMPS/first-party requirement); Meta
Instagram Messaging webhooks + App Review; TikTok Developers webhooks/events
docs (no live-comment event) + third-party Webcast libraries. **These change
fast — re-verify the specific endpoint + permission before coding each adapter.**

## The blocker is authorization, not code

Building an adapter is a few hundred lines. Getting *live authorized access* is
weeks of Meta business verification + App Review with no guarantee — a
**founder/partnerships track that runs in parallel**, not an engineering one.
So the code is designed now and shaped correctly; each adapter lights up when
its approval lands. TikTok has no compliant switch to flip at all today.

## The contract (one shape, every platform)

Two normalized types and one trait — so the agent loop, policy, and audit never
learn platform specifics; only the adapter does.

```
SocialEvent            // inbound, normalized
  platform             // WhatsApp | FacebookLive | InstagramDM | ...
  kind                 // Comment | DirectMessage | OrderIntent | Reaction
  external_id          // platform message/comment id (idempotency)
  author               // opaque handle (never store more PII than needed)
  text
  session_ref?         // ties to a livestream session where applicable
  received_at
  in_service_window    // for 24h-window platforms — drives policy

SocialAction           // outbound, requested by the runtime
  platform
  kind                 // Reply | DirectMessage | Template | Comment | Hide
  target_ref           // what we're replying to / who we're messaging
  body                 // text or template ref + params
  cost_class           // Free (in-window) | Paid (template/marketing)

trait SocialGateway {
  async fn poll_or_subscribe() -> Stream<SocialEvent>;   // ingest
  async fn dispatch(action: SocialAction) -> Result<Receipt>;  // act
  fn platform() -> Platform;
  fn limits() -> RateLimits;   // fed into ToolPolicyGate as policy
}
```

Every `dispatch` goes through the Harness custody path (the existing
`AuditedExec`/policy pattern, generalized from CLI to gateway action):

1. **Policy check** — platform rate cap not exceeded; if `cost_class = Paid` or
   `!in_service_window`, require an approved proposal; ToS constraints
   (e.g. no unsolicited first-message) enforced here.
2. **Dispatch** only if allowed.
3. **Audit** — one `AuditEvent` per action (allowed *or* denied), with the
   receipt/latency/verdict.

Inbound `SocialEvent`s are audited too, and fan out to the agent loop (Studio's
`CommentMonitor` is the existing seam — this replaces its mock).

## Where the pieces live (boundary)

- **Contract + custody + policy rules** → Harness (Rust). This is the governed
  runtime; it must own the gate. `SocialEvent`/`SocialAction`/`SocialGateway`,
  the rate-limit-as-policy rules, and the audit of every action live here.
- **Platform adapters** → pragmatically near the runtime that runs them. Studio's
  agent loop is Python and already holds the `CommentMonitor`/OBS seam; a
  WhatsApp webhook receiver is naturally a small service beside it. The rule:
  an adapter may only *dispatch* through the Harness custody contract, never call
  a platform SDK directly from ungoverned code.
- **Human approvals** → the Engine review surface already built
  (`/api/harness/proposals` + the control-plane dashboard). A "send 200 marketing
  templates" action becomes a proposal a human approves before it fires.

## Autonomy per action (maps to the L0→L2 ladder)

- **L0 / auto, in-window service reply** — a buyer messages first; replying
  within 24h is low-risk and cheap → auto, audited.
- **L1 / human-approved** — anything paid (templates/marketing), bulk, or
  out-of-window → proposal → human approves → fires. This is where the ban risk
  lives, so this is where the human stays.
- **L2 / gated auto** — only after L0 has a trusted trail, and only for
  reversible, capped, in-policy actions. Not now.

## Sequencing

1. **Now (unblocked):** land this contract + the custody rules in the Harness;
   generalize the `AuditedExec` gate to `SocialAction`. Pure design/code, no
   platform access needed.
2. **WhatsApp adapter** — a webhook receiver + `dispatch` through custody. Start
   in the Meta sandbox number (no verification needed to test), then the real
   WABA once approved. Highest value: WhatsApp is the substrate.
3. **Facebook Live comments** — once the seller streams out through Yaatal's
   Page/app (first-party RTMPS), subscribe to `live_comments`; feed the agent
   loop. Needs App Review.
4. **Instagram DM** — same Meta rails; light up after verification.
5. **TikTok** — no compliant inbound today. Keep the virtual-camera *output*
   bridge (already in Studio). Revisit if/when TikTok ships an official live API
   or a partnership grants access. Do not ship the unofficial Webcast hack into
   the governed path — it's a ban vector.

## Non-goals

- No unofficial/ToS-gray platform access in the governed runtime (TikTok Webcast
  scraping stays out).
- No storing more author PII than an interaction needs (sovereignty posture).
- No out-of-window or bulk sends without a human-approved proposal.
