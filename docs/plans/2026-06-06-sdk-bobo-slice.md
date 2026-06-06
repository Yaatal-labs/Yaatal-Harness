# Plan: SDK + BOBO Slice (frontend wiring + compounding) — minus Harness & R&D

> **Scope cut deliberately.** This slice ships the **platform clock, app-facing** value: the
> `@yaatal/client` SDK, BOBO's commerce path solidly on the Engine, native packaging, and the
> compounding spin/scaffold. **Out of scope:** Engine↔Harness alignment, the Wolof model R&D, and
> migrating BOBO's chat/delivery/payments off PocketBase/DExchange (they work today — a *later*
> decision, §6).
>
> Grounded in the **BOBO wiring audit (2026-06-06)**, not guesses. Mobile-drivable like the inventory:
> one unit per sitting, fill its row, check the box.

---

## Mobile kickoff prompt
> *"Open `docs/plans/2026-06-06-sdk-bobo-slice.md`. Do the next unchecked unit using its Acceptance
> criteria. Update its row in §5, check the box, stop. One unit."*

---

## 1. Audit reality (what we're building on)
- **Commerce already Engine-wired:** `@njooba/core` barrel (`packages/core/src/services/index.ts`) exports the **Engine** variants as canonical (`ProductsServiceEngine as ProductsService`, orders, auth). Screens import `productsService`/`ordersService` from `@njooba/core`. ✅
- **The SDK already exists in disguise:** `engine.client.ts` + `*.service.engine.ts` in `@njooba/core` = the proto-`@yaatal/client`. Extraction, not build.
- **PowerSync is dead weight:** orphaned `bobo-app/src/services/{products,orders}.service.ts`, `*.service.powersync.ts`, `src/lib/powersync/*` — 0 live imports — but `@powersync/*` deps still in `package.json`.
- **Other backends (leave for now):** chat/delivery/AI-search → PocketBase; payments → DExchange (mostly stubbed).
- **Native build absent:** no `eas.json` / `app.config`; only `app.json`. Expo ~54, RN 0.76.5.
- **Engine URL wiring exists:** `engine.client.ts` reads `EXPO_PUBLIC_ENGINE_API_URL` (default `localhost:5150`).

## 2. Scope — in / out
**IN:** SDK extraction · PowerSync purge · commerce verify/finish · EAS native build · env wiring · (compounding) one-command spin · `create-yaatal-app`.
**OUT:** Harness alignment · model R&D/voice · chat/delivery/payments backend migration (§6) · multi-tenant/SaaS.

## 3. Build order (dependencies)
```
SDK extract ─┬─> BOBO commerce verify ─> native build ─> ship
             └─> one-command spin ─────> create-yaatal-app (needs SDK+spin)
PowerSync purge runs anytime (independent, unblocks native)
```

## 4. Units (each = one sitting)

### Track S — the SDK
- [ ] **S1 · Extract `@yaatal/client`** — lift `engine.client.ts` + `auth/products/orders.service.engine.ts` (+ mappers, types) from `@njooba/core` into a standalone package. Keep namespaces (`auth`, `products`, `orders`) so an `actions` seam can be added later. *Accept:* package builds; exports a typed client; `getEngineApiUrl()` reads `EXPO_PUBLIC_ENGINE_API_URL`.
- [ ] **S2 · Point `@njooba/core` at `@yaatal/client`** — re-export from the new package so BOBO keeps importing `@njooba/core` unchanged (no app churn). *Accept:* app compiles; web build still calls the Engine.
- [ ] **S3 · README + version** — install + one-env-var usage; `0.1.0`. *Accept:* a fresh reader can wire a frontend in <5 min.

### Track P — PowerSync purge (independent)
- [ ] **P1 · Delete dead services** — `bobo-app/src/services/{products,orders}.service.ts`, all `*.service.powersync.ts`, `src/lib/powersync/*`, their tests. *Accept:* `grep -r powersync src` is clean (non-test); app compiles.
- [ ] **P2 · Drop deps** — remove `@powersync/react-native`, `@powersync/common` from `package.json`; reinstall. *Accept:* lockfile updated; web + (later) native build don't reference PowerSync.

### Track B — BOBO commerce solid
- [ ] **B1 · Verify auth flow on Engine** — login/register/token via `authServiceEngine`; confirm `useAuthStore` reads the Engine token; no PocketBase auth on the active path. *Accept:* login round-trips to the Engine on web + device.
- [ ] **B2 · Verify products + orders flows** — list/detail/create/update (merchant) + cart→order→checkout against the Engine; map any gaps to Engine endpoints. *Accept:* each screen's data round-trips to the Engine; gaps logged.
- [ ] **B3 · Confirm engine DTO mappers** — `mapEngineProductToProduct` / `mapEngineOrderToOrder` cover all fields the UI needs. *Accept:* no `undefined` UI fields from a real Engine response.

### Track N — native packaging
- [ ] **N1 · Add `eas.json`** — `development` / `preview` / `production` profiles. *Accept:* `eas build --profile preview` config validates.
- [ ] **N2 · Inject engine URL for native** — bake `EXPO_PUBLIC_ENGINE_API_URL=https://yaatal-engine-production.up.railway.app` into native profiles (and dev). *Accept:* a dev-client/build hits the Railway Engine, not `localhost`.
- [ ] **N3 · Device smoke** — login + browse + (mock) checkout on a real device or dev client. *Accept:* the three flows work natively; screenshot.

### Track C — compounding (after S + spin)
- [ ] **C1 · One-command spin** — `docker compose` (or `yaatal up`) → Engine + Postgres + migrations + optional seed + health gate. *Accept:* clone → one command → `/health` 200 + seeded catalog.
- [ ] **C2 · `create-yaatal-app`** — scaffolds Engine(+spin) + a starter frontend pre-wired to `@yaatal/client`. *Accept:* `npx create-yaatal-app x` → login + catalog work out of the box.

## 5. Results / status (fill as you go)
| Unit | Status | Effort (est) | Notes / blockers |
|---|---|---|---|
| _ex_ S1 | done | 0.5d | engine.client + 3 services lifted cleanly |
| | | | |

## 6. Deferred decision (NOT this slice)
BOBO's **chat/delivery** (PocketBase) and **payments** (DExchange, stubbed) are on non-Engine backends.
Options to weigh later: (a) keep as-is (they work), (b) consolidate onto the Engine (needs Engine
endpoints + likely Harness for chat/delivery intelligence). Logging it so it isn't silently assumed
done. **Payments note:** Engine has `yaatal-payments` (Wave) + `bobo_checkout`; BOBO client points at
DExchange — two payment stories to reconcile if/when this is picked up.

## 7. Timeline (this slice, 2 devs + AI)
- SDK (S) ~2–4d · PowerSync purge (P) ~1–2d · commerce verify (B) ~2–3d · native (N) ~3–5d → **core slice ~1.5–2.5 wks**.
- Compounding (C) adds ~1–1.5 wks (spin parallel with B/N; scaffold after).

## 8. Definition of done
SDK published as `@yaatal/client`; `@njooba/core` re-exports it; PowerSync gone from tree + deps;
BOBO commerce verified on Engine (web + device); `eas.json` + native env wired; device smoke green.
(Compounding C optional for the first ship.)
