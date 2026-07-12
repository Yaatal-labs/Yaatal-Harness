# Observability — OpenTelemetry at the audit seam

> Status: **design note, not built.** Dated 2026-07-11. Banks the "add OTel"
> idea so it lands in the right place when we do it — not urgent pre-deploy.

## The gap

Yaatal has no unified observability story yet, and we're about to run a
distributed system: Engine (Rust/Loco) + the PI-SPI Node bridge + search/voice
sidecars + Studio (Python) + the Harness ops runner. When a request is slow or a
payment errors, there's no single trace across those hops today.

## Why the Harness is the right home for it

The Harness already owns **audit** — every governed action is an `AuditEvent`
("was this allowed, by what policy, with what verdict"). OpenTelemetry answers
the *complementary* question: "how did the request flow, where's the latency,
what threw." They are not redundant:

- **Audit** = governance truth (allow/deny, spend cap, who decided). Append-only.
- **OTel trace** = operational flow (spans, latency, errors across services).

Correlating them — stamp the `trace_id` onto the `AuditEvent` — gives one view:
*this governed action took this path and cost this much.* That correlation is
the reason OTel belongs at the audit seam rather than bolted on per service.

## Shape (when built)

- **Rust (Engine + runner):** `tracing` + `tracing-opentelemetry` + an OTLP
  exporter. No new service in the request path — export to a collector.
- **Node bridge / Python Studio:** their own OTLP SDKs → same collector.
- **Collector + backend:** one OTel collector → any OTLP sink (self-hosted;
  keep it sovereign — no third-party APM that ships trace data off-box).
- **Do NOT** adopt `@supertokens-plugins/opentelemetry-nodejs` for this — it's
  Node-only, days-old (v0.1.x, Sep 2025), and scoped to SuperTokens' own API
  calls. It's the wrong tool for a Rust-first stack. Take the *idea* (auto-mask
  secrets in spans), not the package.

## Sequencing

After deploy, not before. The first useful slice is Engine + bridge traces with
`trace_id` on the payment-path audit events — that's where a real failure
(a stuck PI-SPI poll, a webhook that never fires) will first need diagnosing.

ponytail: OTel is the standard; don't invent a bespoke tracing format. One
collector, OTLP everywhere, correlate to audit by `trace_id`. Nothing more.
