# Hunger Arena — AI Guidelines

Survival multiplayer monorepo: **Colyseus + R3F + shared deterministic sim**.

Read **README.md**, **ENGINE.md**, **ARCHITECTURE.md**, **OPTIMIZATION.md** and **.cursorrules**
before any change.

## Status

MVP complete: authoritative netcode, ability combat, living NPCs + survival
waves, Postgres persistence + offline sync, guest/account auth, pro UI, Ghibli
toon art, debug + optimization, security hardening. Offline / LAN / online all
run on one codebase.

## Structure

```
packages/shared/  → deterministic GameSimulation, @colyseus/schema, zod, abilities, AI (TS pure)
packages/client-core/ → motor: Transport, ArenaController, createInputStore (ver ENGINE.md)
apps/mobile/      → Expo + R3F native (expo-gl), touch controls, AsyncStorage
apps/desktop/     → Tauri + R3F WebGL, Pointer Lock + WASD, IndexedDB
server/           → SurvivalRoom (authoritative), auth REST, Drizzle/Postgres
client/           → Landing web (descarga APK)
```

## Commands

```bash
npm install
npm run dev          # server :2567 + landing :5173
npm run dev:server
npm run dev:client   # landing APK
npm run dev:mobile   # Expo
npm run dev:desktop  # Tauri (Linux AppImage dev)
npm test -w @hunger-arena/shared
```

## Rules

1. Types/sim logic only in `packages/shared` — never duplicate in clients/server.
2. Hot objects (meshes) via `useRef` + `useFrame` — never `useState`.
3. Server is authoritative; clients send **inputs only**. Never trust client
   positions. Validate every message with zod.
4. Log all connect/disconnect/state/security events (rule 9).
5. `EXPO_PUBLIC_SERVER_HOST` (mobile) / `VITE_SERVER_HOST` (desktop) — never hardcode IPs.
6. Android needs `network_security_config.xml` for ws:// cleartext
   (`npm run cap:overlay`).
7. Keep the sim deterministic (seeded RNG, pure functions) — offline must match
   online. Cover with vitest.
8. Visual work follows the `.cursor/skills/ghibli-toon-graphics` skill and
   `OPTIMIZATION.md` budgets.

## QA

- Unit: `npm test -w @hunger-arena/shared`.
- e2e: `scripts/test-{join,survival,auth,persist}.mjs` with server running.
- Manual: two browser tabs or laptop + phone on WiFi. Server console + client
  debug overlay (` key) must show join/sync.
