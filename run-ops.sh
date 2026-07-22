#!/usr/bin/env bash
set -euo pipefail
export YAATAL_ENGINE_URL="https://engine.njooba.com"
YAATAL_TOKEN="$(yaatal auth login --email "ops@yaatal.dev" --password "YaatalOps2026!" | jq -r .token)"
export YAATAL_TOKEN
exec /workspace/Yaatal-Harness/target/debug/yaatal-ops-runner /workspace/Yaatal-Harness/daily-ops.json