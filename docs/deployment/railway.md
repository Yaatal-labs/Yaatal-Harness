# Railway Deployment

Railway hosts the **Engine**. It does not need to host the full model stack.

Current hosted shape:

- Railway: `yaatal-api`
- Railway: Postgres
- External service: PersonaPlex (local mock first, RunPod later)
- External service: `/search` HTTP endpoint

## One-command bootstrap

From the repo root:

```powershell
.\scripts\railway-bootstrap.ps1
.\scripts\railway-bootstrap.ps1 -Apply
```

The script assumes this checkout is already linked to the correct Railway project and environment.

What the script does today:

- verifies Railway CLI auth
- verifies the linked environment exists
- verifies the sibling `Postgres` service is active
- ensures `DATABASE_URL` exists, using `${{Postgres.DATABASE_URL}}` when it is missing
- generates `JWT_SECRET` if it is missing
- triggers one redeploy after wiring the variables

If your database service uses a different Railway service name, pass `-DatabaseService <name>`.

## Runtime assumptions

The deployable service is configured in [`railway.json`](../../railway.json):

- build: `cargo install --path crates/yaatal-api --bin yaatal_api-cli --root /app`
- start: `./bin/yaatal_api-cli start`
- healthcheck: `GET /health`

The API binary also self-heals two monorepo-specific Loco defaults:

- if `LOCO_CONFIG_FOLDER` is unset and `crates/yaatal-api/config` exists, it uses that folder
- if `RAILWAY_ENVIRONMENT=production` and `LOCO_ENV` is unset, it boots in `production`

## Current required variables

The current deployed API still only requires:

- `DATABASE_URL`
- `JWT_SECRET`

## Planned Bo-Plex variables

Prepare these as the session loop lands:

- `PERSONAPLEX_WS_URL`
- `PERSONAPLEX_BEARER_TOKEN`
- `PERSONAPLEX_DEFAULT_PERSONA`
- `PERSONAPLEX_DEFAULT_LANG`
- `PERSONAPLEX_DEFAULT_MARKET`
- `SEARCH_SERVICE_URL`
- `SEARCH_SERVICE_TIMEOUT_SECONDS`

These are not all wired in code yet. They are the documented runtime shape for the Bo-Plex rollout.

## Verification

After bootstrap or a manual redeploy:

```powershell
railway deployment list -s Yaatal-Engine --limit 5
railway logs --latest --deployment -s Yaatal-Engine --lines 200
railway status --json
```

Healthy deploy checklist:

- latest `Yaatal-Engine` deployment status is `SUCCESS`
- build logs show `Installing /app/bin/yaatal_api-cli`
- deploy logs stop repeating `/health` failures
- the Railway service remains running after the healthcheck window

## Notes

- Railway is the right home for the Engine and Postgres.
- Heavy model services should stay external.
- Redis and SigLIP2 are out of the first Bo-Plex milestone.
