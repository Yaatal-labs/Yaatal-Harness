# Local Bo-Plex Smoke Runbook

This runbook is the next lowest-hanging operational task for the repo.

Goal:

- start the search service
- start the voice service
- start the engine orchestrator
- obtain a JWT from the engine
- open `/api/voice/session`
- send one search-like turn using `transcript_hint`
- confirm the Engine completes one grounding loop without changing the outer architecture

This proves the current service contracts are real before any further redesign or backend swaps.

## Branches involved

| Branch | Commit | Role |
|--------|--------|------|
| `codex/search-service` | `02b10a8` | phase-one `/search` service |
| `codex/voice-service` | `b587e0b` | PersonaPlex-compatible mock voice service |
| `codex/engine-orchestrator` | `4c9876b` | Engine wiring for `/api/voice/session` |

## Default local ports

| Surface | Default |
|--------|---------|
| search service | `127.0.0.1:8081` |
| voice service | `127.0.0.1:8082` |
| engine | `http://localhost:5150` |

## Prerequisites

- PowerShell shell
- Rust toolchain already working for the repo
- Python with the `websockets` package available
- enough disk space for Cargo build output

On Windows, use the existing Rust bootstrap helpers or a shell where:

- `cp.exe` is on `PATH`
- MSVC build tools are available

## Terminal 1 — Search service

Worktree:

```powershell
cd C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine\.worktrees\search-service
```

Run:

```powershell
$env:SEARCH_BIND = "127.0.0.1:8081"
$env:SEARCH_BACKEND = "in-memory"
cargo run -p yaatal-search --bin search_service
```

External backend variant:

```powershell
$env:SEARCH_BIND = "127.0.0.1:8081"
$env:SEARCH_BACKEND = "external"
$env:BGE_M3_URL = "http://127.0.0.1:8090"
$env:QDRANT_URL = "http://127.0.0.1:6333"
$env:QDRANT_COLLECTION = "yaatal-search"
cargo run -p yaatal-search --bin search_service
```

Fastest local helper:

```powershell
.\scripts\start-local-search-stack.ps1
```

That helper:

- starts or reuses a local Qdrant Docker container on `127.0.0.1:6333`
- launches the deterministic `mock_embedder` binary on `127.0.0.1:8090`
- runs `search_service` with `SEARCH_BACKEND=external`

If Docker Desktop is unavailable, use:

```powershell
.\scripts\start-local-search-stack.ps1 -UseMockQdrant
```

That keeps the same external HTTP shape but swaps Qdrant for the local mock binary.

Expected behavior:

- the service binds on `127.0.0.1:8081`
- `GET /health` should answer successfully

Quick check:

```powershell
curl http://127.0.0.1:8081/health
```

## Terminal 2 — Voice service

Worktree:

```powershell
cd C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine\.worktrees\voice-service
```

Run:

```powershell
$env:VOICE_SERVICE_BIND = "127.0.0.1:8082"
cargo run -p yaatal-voice --bin personaplex_mock
```

Expected behavior:

- the service binds on `127.0.0.1:8082`
- websocket endpoint lives at `ws://127.0.0.1:8082/session`

## Terminal 3 — Engine orchestrator

Worktree:

```powershell
cd C:\Users\momo-\OneDrive\Desktop\YAATAL\Yaatal-Engine\.worktrees\engine-orchestrator
```

Run:

```powershell
$env:SEARCH_SERVICE_URL = "http://127.0.0.1:8081"
$env:VOICE_SERVICE_URL = "ws://127.0.0.1:8082/session"
$env:JWT_SECRET = "yaatal-dev-secret-change-in-production"
$env:VOICE_ROUTING_DEBUG_MESSAGES = "true"
cargo run -p yaatal-api --bin yaatal_api-cli -- start
```

Expected behavior:

- engine binds to `http://localhost:5150`
- `GET /api/voice/session` is available
- `POST /api/voice/transcribe` still exists as fallback

Quick check:

```powershell
curl http://localhost:5150/health
```

## Terminal 4 — Create a dev user and get a JWT

Register:

```powershell
$registerBody = @{
  email = "boplex-smoke@example.com"
  password = "dev-password-123"
  name = "BoPlex Smoke"
} | ConvertTo-Json

Invoke-RestMethod `
  -Method Post `
  -Uri "http://localhost:5150/api/auth/register" `
  -ContentType "application/json" `
  -Body $registerBody
```

Login:

```powershell
$loginBody = @{
  email = "boplex-smoke@example.com"
  password = "dev-password-123"
} | ConvertTo-Json

$login = Invoke-RestMethod `
  -Method Post `
  -Uri "http://localhost:5150/api/auth/login" `
  -ContentType "application/json" `
  -Body $loginBody

$token = $login.token
$token
```

Expected login response fields:

- `token`
- `pid`
- `name`
- `is_verified`

## Terminal 4 — Open `/api/voice/session` and send one turn

This smoke flow intentionally avoids real microphone capture. It uses:

- `session_config`
- one `audio_chunk` with fake base64 audio
- `transcript_hint` set to a search-like phrase

Run:

```powershell
@'
import asyncio
import json
import websockets

TOKEN = r"""REPLACE_WITH_LOGIN_TOKEN"""
URL = "ws://localhost:5150/api/voice/session"

async def main():
    async with websockets.connect(
        URL,
        additional_headers={"Authorization": f"Bearer {TOKEN}"},
    ) as ws:
        await ws.send(json.dumps({
            "type": "session_config",
            "session_id": "smoke-session-1",
            "persona": "market-guide",
            "lang": "wo",
            "market": "SN-DKR"
        }))

        await ws.send(json.dumps({
            "type": "audio_chunk",
            "audio_base64": "ZmFrZQ==",
            "transcript_hint": "find white fabric near Sandaga"
        }))

        for _ in range(6):
            try:
                message = await asyncio.wait_for(ws.recv(), timeout=5)
            except asyncio.TimeoutError:
                break
            print(message)

asyncio.run(main())
'@ | python -
```

Expected message sequence:

1. `session_ready`
2. `subtitle`
3. `audio_chunk`
4. `turn_end`

Depending on the mock/search state, you may also see:

- `warning` if search is unavailable
- extra downstream messages if the voice service emits them

Reusable helper:

```powershell
python .\scripts\run-local-boplex-smoke.py --transcript "find white fabric near Sandaga"
```

Helpful options:

```powershell
python .\scripts\run-local-boplex-smoke.py `
  --base-url "http://localhost:5150" `
  --ws-url "ws://localhost:5150/api/voice/session" `
  --email "boplex-smoke@example.com" `
  --password "dev-password-123" `
  --lang "wo" `
  --market "SN-DKR" `
  --persona "market-guide" `
  --timeout-seconds 5
```

## What success looks like

The loop is considered proven when:

- search service responds locally
- voice service accepts a websocket session locally
- engine accepts an authenticated `/api/voice/session`
- the engine forwards the turn upstream
- the engine sees subtitle text containing a search intent
- the engine calls `POST /search`
- the engine injects grounding upstream without crashing the session

This is enough to declare the Engine locally testable at the current architecture level.

## What this does not prove

- real PersonaPlex integration
- real BGE-M3 + Qdrant + Postgres retrieval
- production latency or scaling
- mobile client UX
- Redis-backed session coordination
- SigLIP2 or image flow

## Why this is the right next task

This runbook preserves the later `Harness × Runtime` direction without blocking on it:

- the external service boundaries are exercised now
- the mock voice service can later be replaced by PersonaPlex
- the in-memory search backend can later be replaced by harness-style retrieval internals or BGE-M3/Qdrant payload-backed retrieval
- the Engine contract remains stable while internals evolve
