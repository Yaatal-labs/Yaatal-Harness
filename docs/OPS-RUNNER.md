# OPS-RUNNER — flip the switch (Contabo VPS + Tailscale)

`yaatal-ops-runner` is the Harness's first **L0-operating** tenant: a
deterministic runbook executor that drives the SDK's `yaatal` CLI through the
full custody stack (`ToolPolicyGate` → `AuditedExec` → `JsonlAuditStore` →
`AuditMetrics` → `OpsRunEval` → L1 proposal generator). It has **no model
call** — the first tenant needs hands, not a brain — so every moving part of
CONTROL-LOOP slices 1–5 is exercised in production with zero nondeterminism in
what runs.

This doc is the whole activation. Prereqs already met: the Engine is deployed
on the Contabo box and reachable over the tailnet.

## 1. Build the runner

```bash
cd Yaatal-Harness
cargo build --release -p yaatal-runner
# → target/release/yaatal-ops-runner
# → target/release/yaatal-proposals-push
```

## 2. Install the `yaatal` CLI (from the SDK)

```bash
cd Yaatal-SDK && npm ci && npm run build
npm link          # exposes `yaatal` on PATH
# or, no global install: alias yaatal='node /path/to/Yaatal-SDK/dist/cli.js'
yaatal --help     # sanity: prints the command reference
```

## 3. Point at the Engine — over the tailnet, not the public internet

Bind the Engine to the tailnet interface only (keep it off 0.0.0.0). The dev
config binds `0.0.0.0:5150`; on the VPS bind the tailnet IP, or firewall 5150
to the `tailscale0` interface:

```bash
# example: only the tailnet may reach the Engine port
sudo ufw allow in on tailscale0 to any port 5150 proto tcp
sudo ufw deny 5150/tcp
```

Then the runner's environment points at the Engine by its MagicDNS name (or
tailnet IP).

**Token freshness — do NOT bake a static token into the env file.** Engine
JWTs expire after `JWT_EXPIRATION_SECONDS` (default **3 days**), so a
one-time token in `/etc/yaatal/ops.env` makes the timer start failing
silently on day four. Instead, store the ops account *credentials* in the
0600 env file (`YAATAL_OPS_EMAIL`, `YAATAL_OPS_PASSWORD`) and mint a fresh
token at the top of every run via the service wrapper:

```bash
#!/usr/bin/env bash
# /opt/yaatal/run-ops.sh — systemd ExecStart wrapper: fresh token per run
set -euo pipefail
export YAATAL_ENGINE_URL="http://<magicdns-name>:5150"   # e.g. http://yaatal-engine.tailnet-xxxx.ts.net:5150
YAATAL_TOKEN="$(yaatal auth login --email "$YAATAL_OPS_EMAIL" --password "$YAATAL_OPS_PASSWORD" | jq -r .token)"
export YAATAL_TOKEN
exec /opt/yaatal/yaatal-ops-runner /etc/yaatal/daily-ops.json
```

Every run then carries a token valid for days, not one that quietly aged
out. (A login is one extra request per run; the audit trail shows the ops
account as `actor` either way.)

Register a dedicated **ops service account** on the Engine for this (not a
human's login) — its JWT is the runner's identity and shows up as the audit
`actor`.

## 4. The runbook — it *is* the authorization

Copy the example and edit `audit_dir` + `steps`. The policy allowlist is
**derived from the runbook's distinct programs**: whoever edits this file
decides what may execute; the gate denies (and audits) anything else. There is
no second allowlist to keep in sync.

```bash
sudo mkdir -p /var/lib/yaatal/ops-audit
cp crates/yaatal-runner/runbooks/daily-ops.example.json /etc/yaatal/daily-ops.json
# edit audit_dir → /var/lib/yaatal/ops-audit, adjust steps as needed
```

Fields: `timeout_secs` (per-step kill deadline), `spend_cap` (per-run cost
cap — CLI steps cost 0 today, so this is wired for when model steps arrive),
`p95_latency_ms` (eval threshold), `proposal_window_runs` (how many recent
runs the L1 generator scans, default 5).

## 5. Run it (systemd service + timer)

`/etc/systemd/system/yaatal-ops.service`:

```ini
[Unit]
Description=Yaatal Harness ops runner (L0 tenant)
After=network-online.target

[Service]
Type=oneshot
User=yaatal
Environment=YAATAL_ENGINE_URL=http://<magicdns-name>:5150
# Prefer an EnvironmentFile with 0600 perms for the token, not an inline value:
EnvironmentFile=/etc/yaatal/ops.env
ExecStart=/opt/yaatal/yaatal-ops-runner /etc/yaatal/daily-ops.json
# One JSON summary to stdout → the journal; exit 0 = eval passed, 1 = failed.
StandardOutput=journal
```

`/etc/yaatal/ops.env` (mode 0600, owner yaatal): `YAATAL_TOKEN=…`

The runner also syncs `proposals.jsonl` to the Engine's review API (`POST
/api/harness/proposals`) at the end of every run, so pending L1 proposals show
up in the control-plane dashboard. It uses the same `YAATAL_ENGINE_URL` +
`YAATAL_TOKEN` pair as everything else here; if either is unset the runner
skips the sync entirely and stays fully functional offline — this push is
additive, not required, and a push failure never fails the run (unsent
proposals retry on the next run). Only `Proposed`-status proposals are sent;
the Engine upserts each by `id` and never overwrites one a human has already
decided on, so repeat pushes are harmless no-ops. To push on demand instead of
waiting for a run, use `yaatal-proposals-push` (§ 5.1).

`/etc/systemd/system/yaatal-ops.timer`:

```ini
[Unit]
Description=Run the Yaatal ops runner daily

[Timer]
OnCalendar=*-*-* 06:00:00
Persistent=true

[Install]
WantedBy=timers.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now yaatal-ops.timer
sudo systemctl start yaatal-ops.service   # run once now
journalctl -u yaatal-ops.service -n 50 --no-pager
```

That is the switch. From here the Harness is **L0-operating**: a governed,
audited, policy-gated run every morning against the live Engine.

## 5.1 Push L1 proposals into Engine review

The runner writes L1 suggestions to `proposals.jsonl`; Engine is the review
system of record. Push the local JSONL file after a run:

```bash
YAATAL_ENGINE_URL=http://<magicdns-name>:5150 \
YAATAL_TOKEN="$(cat /etc/yaatal/ops.env | sed -n 's/^YAATAL_TOKEN=//p')" \
/opt/yaatal/yaatal-proposals-push /var/lib/yaatal/ops-audit/proposals.jsonl
```

The pusher only sends `Proposed` artifacts. Engine upserts by proposal id and
does not overwrite already-decided proposals, so re-running this command is
safe. The CLI prints one JSON summary to stdout: read count, pushed count, and
skipped non-proposed count.

To push proposals even when the runner exits `1` because the eval failed, use a
small wrapper as the service `ExecStart`:

```bash
#!/usr/bin/env bash
set +e
/opt/yaatal/yaatal-ops-runner /etc/yaatal/daily-ops.json
runner_rc=$?
/opt/yaatal/yaatal-proposals-push /var/lib/yaatal/ops-audit/proposals.jsonl
push_rc=$?
if [ "$push_rc" -ne 0 ]; then
  exit "$push_rc"
fi
exit "$runner_rc"
```

That preserves the runner's eval exit code while still moving review artifacts
into Engine.

## 6. Read the trail

Two append-only JSONL files under `audit_dir`:

```bash
cd /var/lib/yaatal/ops-audit

# last run's events (one JSON object per line)
tail -n 20 audit.jsonl | jq .

# success rate + p95 latency of the most recent run
jq -s 'group_by(.run_id) | last | {events: length,
        ok: (map(select(.success)) | length),
        p95_ms: (map(.latency_ms) | sort | .[(length*0.95|floor)])}' audit.jsonl

# any policy denials, ever (should be empty in steady state)
jq 'select(.policy_verdicts[]? | .Deny? != null)' audit.jsonl

# L1 proposals awaiting human review
jq . proposals.jsonl
```

## 7. What L0 → L1 means operationally

- **L0 (now):** the runner observes, audits, scores, and *suggests*. It never
  changes configuration. `proposals.jsonl` accumulates `Proposed` artifacts
  (e.g. "tool X timed out in 3 consecutive runs → RaiseTimeout") for a human to
  read and apply by hand.
- **L1 → L2** (later, deliberate): only after the L0 trail is trusted, and only
  for reversible, config-class changes gated on evals passing thresholds — as
  `docs/CONTROL-LOOP.md` specifies. Do not shortcut this.

## Upgrade path (documented ceilings)

- **Model-driven runner:** replace the fixed runbook with a goal, planned via
  `yaatal-models::LlmProvider` — same `AuditedExec` custody. This is when
  `spend_cap` starts mattering (model calls carry cost).
- **Store:** `JsonlAuditStore` is the L0 ceiling; the proposal pass reads the
  whole file each run. Move to a Postgres-backed store with a "last K runs"
  query when the trail outgrows a flat file.
