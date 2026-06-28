---
name: monorepo-architecture
description: Hunger Arena monorepo constitution — package boundaries, dependency direction, forbidden duplicates, and where each feature belongs. Use before ANY structural change, new package, moving files, adding imports across layers, or when tempted to copy sim/netcode/gfx into apps.
---

# Monorepo Architecture (Constitution)

This project is **not** a loose folder of apps. It is a **layered monorepo**
where each layer has one job. Breaking a boundary causes desync, cheat holes, or
unmaintainable duplication.

**Read first:** `ARCHITECTURE.md`, `ENGINE.md`, `OPTIMIZATION.md`.

**Companion skills (do not contradict this doc):**
- `.cursor/skills/ghibli-toon-graphics` — materials/lighting look
- `.cursor/skills/procedural-asset-factory` — procedural meshes + `client-gfx`
- `.cursor/rules/engine-api.mdc` — engine tick/input/netcode invariants

## Package map (current truth)

```
packages/shared/          VERDAD DEL JUEGO (TS puro, sin DOM/Node/Colyseus en sim)
packages/client-core/     MOTOR DE CLIENTE (Transport, ArenaController, input store, updates API)
packages/client-gfx/      ARTE PROCEDURAL compartida (PALETTE, AssetRegistry, builders — Three.js only)
apps/mobile/              APP Expo — touch, R3F native, OTA APK
apps/desktop/             APP Tauri — WASD/Pointer Lock, R3F WebGL, AppImage OTA
server/                   AUTORITATIVO Colyseus + Express + Postgres + /updates
client/                   LANDING ONLY — showcase bots + download panel (NOT a full game app)
```

## Dependency direction (allowed imports)

```
apps/*  ──►  client-core, client-gfx, shared
client/ ──►  client-core, client-gfx, shared   (landing showcase only)
client-core ──►  shared
client-gfx  ──►  shared (types only if needed)
server/     ──►  shared
shared/     ──►  (nothing from apps/server/client-core)
```

**Never:** `shared` importing React, Three, Colyseus room code, or app UI.

## What each layer OWNS

| Layer | Owns | Must NOT own |
|-------|------|----------------|
| `shared` | `GameSimulation`, AI, combat rules, `InputState`, schema, zod, `updates.ts` types | Rendering, sockets, React, platform input |
| `client-core` | `Transport`, `SessionManager`, `ArenaController`, `createInputStore`, `checkUpdate` | Three.js meshes, shaders, UI screens |
| `client-gfx` | `AssetRegistry`, `Mesh360/FlatBuilder`, `PALETTE`, `makeCelMaterial` | R3F components, game loop, netcode |
| `apps/*` | `gfx/WorldRenderer`, `ui/`, platform input adapters, `config.ts` (env host) | Duplicated `ArenaController`, `GameSimulation`, `Transport` |
| `server` | `SurvivalRoom`, auth, DB, `/updates/manifest`, static artifacts | Client prediction, mesh code |
| `client/` | `landing/*`, showcase transport, download manifest fetch | Full game menus, player input to sim, duplicated netcode |

## Inviolable rules (sync + security)

1. **Server input-only** — clients send `InputState` intentions only; never positions/HP.
2. **One simulation** — `GameSimulation` lives only in `packages/shared`.
3. **One netcode brain** — `ArenaController` lives only in `packages/client-core`.
4. **One input contract** — `InputPayload` = `Omit<InputState,'seq'>` on mobile AND desktop.
5. **Determinism** — seeded RNG + pure functions; offline must match online; vitest required.
6. **Hot path off React** — entity transforms: `useRef` + `useFrame`, never `useState`.
7. **Zod at boundaries** — every inbound WS/HTTP message validated before sim.
8. **No IP hardcoding** — `EXPO_PUBLIC_SERVER_HOST` / `VITE_SERVER_HOST` / `UPDATE_BASE_URL`.
9. **Logging** — connect/disconnect/sync/security events on server (and debug overlay client).

## Where to put a new feature (decision tree)

```
¿Cambia reglas del juego (daño, AI, oleadas, inventario)?
  → packages/shared + test

¿Cambia red, predicción, reconciliación, OTA manifest client?
  → packages/client-core

¿Nuevo mesh/item/árbol/terreno procedural?
  → packages/client-gfx (register in AssetRegistry)
  → apps/*/gfx/WorldRenderer.tsx only if instancing glue needed

¿Solo HUD, menú, botón, pantalla?
  → apps/mobile or apps/desktop (same screen in both when possible)

¿Distribución APK/AppImage/manifest?
  → server/updates + scripts/publish-update.mjs

¿Landing / marketing / descargas?
  → client/landing only — showcase may READ sim via client-core, visitor never inputs
```

## App parity (mobile ≈ desktop)

These must stay **behaviorally identical** (only input adapter differs):

- Screen flow: splash → menu → mode → class → game → results
- `useArena` → `SessionManager` → `ArenaController`
- `createInputStore('touch'|'pc')` feeding same `InputPayload` fields
- Auto-update checks (`UpdateProvider`) — defer apply during active match

## Forbidden patterns (reject in review)

| Pattern | Why forbidden | Correct location |
|---------|---------------|------------------|
| `apps/*/ArenaController.ts` | desync | `packages/client-core` |
| `apps/*/LocalTransport.ts` | drift from server | `packages/client-core` |
| `client/src/net/*` duplicate | drift | delete; use `client-core` |
| `GameSimulation` in app | cheat / divergence | `packages/shared` |
| `MeshStandardMaterial` for world | art + perf | `client-gfx` toon |
| Raw hex in components | palette drift | `PALETTE` from `client-gfx` |
| `import @colyseus/sdk` in apps | bypasses engine | `client-core` only |
| Copy-paste `WorldRenderer` logic into apps | duplication | shared glue + `client-gfx` builders |

## Validation before merge

```bash
node .cursor/skills/monorepo-architecture/scripts/check-boundaries.mjs
npm run build -w @hunger-arena/shared
npm run build -w @hunger-arena/client-core
npm run build -w @hunger-arena/client-gfx
npm test -w @hunger-arena/shared
```

## Landing (`client/`) special case

- **Purpose:** download portal + non-interactive gameplay trailer (`ShowcaseTransport`).
- **May import:** `client-core`, `client-gfx`, `shared`.
- **Must not grow into:** a third game client with menus, auth, or user-driven input.
- Legacy `client/src/game/ArenaController.ts` and `client/src/net/` are **removed**
  — they violated boundaries.

See `reference.md` for dependency matrix and mermaid diagram.
