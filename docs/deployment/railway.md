# Railway Deployment

`Yaatal-Engine` deploys the `yaatal-api` binary, not the whole workspace.

## One-command bootstrap

From the repo root:

```powershell
.\scripts\railway-bootstrap.ps1
.\scripts\railway-bootstrap.ps1 -Apply
```

The script assumes this checkout is already linked to the correct Railway project and environment.

What the script does:

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

That means fresh Railway services only need the real runtime secrets:

- `DATABASE_URL`
- `JWT_SECRET`

## Config layout

This repo has two config trees:

- `crates/yaatal-api/config/` is the Loco runtime config for the deployed API
- `config/` is engine-level workspace config from the initial scaffold and is not the Railway boot source for `yaatal-api`

If Railway starts the API against the root `config/` folder, you will see schema mismatches like missing `database.enable_logging`.

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

## Optional follow-up

Once the service has a Railway or custom domain, set:

- `APP_URL=https://<your-domain>`

That value is not required for the service to boot, but it is the correct production host value for links and cookies.
