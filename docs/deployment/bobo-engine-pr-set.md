# BOBO + Engine Integration PR Set

## Purpose

This document records the cross-repository PR sequence required to make the
Engine-backed BOBO development slice reproducible from source control.

The current Cloudflare Pages deployment is functional, but it was uploaded
directly from a local BOBO build. The verified BOBO web repair must be committed
and pushed before Git-triggered deployments are reliable.

## Current deployed shape

| Surface | Deployment | Branch | Source state |
| --- | --- | --- | --- |
| Engine API | `https://yaatal-engine-production.up.railway.app` | `codex/bobo-engine-pr27-integration` | Pushed and deployed from `180c61f` |
| BOBO web | `https://bobo-6g9.pages.dev` | `codex/bobo-engine-netlify-integration` | Live from a direct Pages upload; verified repair is still local-only |
| Engine database | Railway Postgres | production environment | Active |

Verified on 2026-05-31:

- `GET https://yaatal-engine-production.up.railway.app/health` returns `200`.
- `GET https://yaatal-engine-production.up.railway.app/api/products` returns `200`.
- `https://bobo-6g9.pages.dev/Login` renders the BOBO login screen.
- The BOBO Pages bundle contains the Railway Engine domain.
- The BOBO Pages bundle does not contain the invalid `import.meta` syntax that
  previously prevented React from mounting.

## Existing PR stack

### Engine PR 27: upstream foundation

- Repository: `Yaatal-labs/Yaatal-Engine`
- PR: `#27`
- URL: `https://github.com/Yaatal-labs/Yaatal-Engine/pull/27`
- Head: `claude/yataal-codex-deploy-review-jKnga`
- Base: `main`
- State: open and ready for review

PR 27 is the upstream Engine foundation. It already contains the app-agnostic
payment, escrow, KYC, analytics, LiveKit, and BOBO HTTP work that the integration
slice builds on.

### Engine PR 28: BOBO commerce bridge and Railway deployment repair

- Repository: `Yaatal-labs/Yaatal-Engine`
- PR: `#28`
- URL: `https://github.com/Yaatal-labs/Yaatal-Engine/pull/28`
- Head: `codex/bobo-engine-pr27-integration`
- Base: `claude/yataal-codex-deploy-review-jKnga`
- State: draft, pushed, and deployed
- Current head: `180c61f`

PR 28 is intentionally stacked on PR 27. It contains:

- the BOBO commerce bridge;
- Railway Docker and runtime corrections;
- valid production CORS configuration;
- stock Railway Postgres compatibility;
- the absolute runtime configuration path required by the deployed image.

Before merging PR 28, review PR 27 first. After PR 27 merges, retarget or rebase
PR 28 onto `main` if GitHub does not update the base automatically.

### BOBO PR 1: Engine runtime cutover and web deployment repair

- Repository: `MouhamedN96/BOBO-`
- PR: `#1`
- URL: `https://github.com/MouhamedN96/BOBO-/pull/1`
- Head: `codex/bobo-engine-netlify-integration`
- Base: `Standalone`
- State: draft
- Remote head before the web repair commit: `0950e9d`

The pushed commit already cuts the active BOBO commerce path to Engine HTTP
services while preserving PowerSync and Supabase files for rollback.

Add one focused follow-up commit to this PR:

```text
fix(web): make Engine-backed Expo export deployable on Cloudflare Pages
```

That commit must include only:

```text
bobo-app/App.tsx
bobo-app/metro.config.js
bobo-app/package.json
packages/ai/package.json
packages/core/package.json
packages/core/src/services/engine.client.ts
pnpm-lock.yaml
```

It repairs:

- Zustand ESM resolution leaving `import.meta.env` in Expo's classic web bundle;
- duplicate and stale Expo module graphs on the startup path;
- SDK 54-compatible speech, camera, image picker, AV, and status bar versions;
- Engine API URL injection from Expo-owned application source.

Do not stage local notes, `.claude/`, or generated artifacts with this commit.

## Follow-up PRs

Keep the remaining work out of the deployment repair commit.

### Engine follow-up: development payment modes

Purpose:

- keep cash checkout operational without provider configuration;
- add an explicit Wave stub mode for development;
- return a pending provider reference that BOBO can poll;
- preserve real Wave provider configuration as an environment-only concern.

Required tests:

- cash checkout reaches success;
- Wave stub reaches pending;
- pending status can be polled;
- terminal status transitions remain ordered correctly.

### Engine follow-up: seed data and E2E smoke fixtures

Purpose:

- add a development buyer;
- add a merchant;
- add two or three products;
- make browse, product detail, QR/deeplink, checkout, and merchant visibility
  testable against the deployed Engine.

The production product endpoint currently returns an empty catalog, which is
valid but insufficient for the end-to-end demo.

### BOBO follow-up: complete Expo SDK 54 alignment

Purpose:

- complete the React Native SDK upgrade without expanding the web deployment
  repair PR;
- remove remaining dependency skew detected by `expo install --check`;
- smoke test native QR camera, audio, video, navigation, and web export.

Remaining expected upgrades:

```text
react 18.3.1 -> 19.1.0
react-dom 18.3.1 -> 19.1.0
react-native 0.76.5 -> 0.81.5
react-native-safe-area-context 4.12.0 -> ~5.6.0
react-native-screens 4.4.0 -> ~4.16.0
react-native-svg 15.8.0 -> 15.12.1
react-native-web 0.19.13 -> ^0.21.0
@types/react 18.3.x -> ~19.1.10
```

### BOBO follow-up: web hardening

Purpose:

- wrap login inputs in a form;
- add input `id`, `name`, and autocomplete attributes;
- version service worker caches;
- add a Pages preview smoke check before production promotion.

## Merge order

1. Review and merge Engine PR 27.
2. Retarget or rebase Engine PR 28 onto `main`, then review and merge it.
3. Commit and push the verified BOBO Pages repair into BOBO PR 1.
4. Land the Engine development payment-mode follow-up.
5. Land the Engine seed and E2E fixture follow-up.
6. Land the BOBO Expo SDK 54 alignment follow-up.
7. Land the BOBO web hardening follow-up.

## BOBO Pages configuration

Cloudflare Pages project:

```text
Project: bobo
Domain: bobo-6g9.pages.dev
Production branch: codex/bobo-engine-netlify-integration
Build command: pnpm install --frozen-lockfile && pnpm build
Output directory: bobo-app/dist
```

Required build-time variable:

```text
EXPO_PUBLIC_ENGINE_API_URL=https://yaatal-engine-production.up.railway.app
```

## Verification gates

Before pushing the BOBO web deployment repair:

```powershell
pnpm install --frozen-lockfile
$env:EXPO_PUBLIC_ENGINE_API_URL='https://yaatal-engine-production.up.railway.app'
pnpm build
pnpm --filter bobo-app type-check
```

Inspect the generated web bundle:

```powershell
rg -n "import\.meta" bobo-app/dist/_expo/static/js/web
rg -n "yaatal-engine-production\.up\.railway\.app" bobo-app/dist/_expo/static/js/web
```

Expected result:

- the first command returns no matches;
- the second command finds the deployed Railway Engine domain;
- `https://bobo-6g9.pages.dev/Login` renders after deployment;
- `GET /api/products` returns `200`.

## Local-only state at handoff

### Engine repository

Branch `codex/bobo-engine-pr27-integration` is synchronized with its remote:

```text
ahead: 0
behind: 0
```

No additional tracked Engine source changes need to be pushed for the current
Railway deployment. Keep unrelated local R&D directories and generated files
out of the Engine integration PR.

### BOBO repository

Branch `codex/bobo-engine-netlify-integration` exists remotely at `0950e9d`.
The verified Pages repair remains local-only until the focused follow-up commit
is created and pushed.

The direct Cloudflare Pages deployment proves the repair, but it does not update
the Git repository. A future Git-triggered Pages rebuild can regress until BOBO
PR 1 receives the repair commit.
