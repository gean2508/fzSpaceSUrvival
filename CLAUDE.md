# Agencia Espacial Survival — AI Guidelines

Survival + space exploration: **Colyseus server** + **native Bevy client** + shared deterministic sim.  
Product vision: `native/docs/SPACE_SURVIVAL_VISION.md` · context bible: `labiblia.md`.

## Status

Server MVP: authoritative netcode, survival waves, Postgres, auth, `/updates`.
**Game client:** `native/` (Rust/Bevy ECS) — React/R3F/Expo **removed**.

## Structure

```
shared/   → deterministic GameSimulation (TS)
server/   → SurvivalRoom, auth, Drizzle
client/   → landing download panel only
native/   → ha-shared (ported sim constants) + ha-game (Bevy)
```

## Commands

```bash
npm install
npm run play              # ship APK + server + landing — user opens URL on phone
npm run ship:android      # build + publish APK only
npm run dev:server        # :2567
npm run test              # shared vitest
npm run test:native       # ha-shared Rust tests
```

## Rules

1. Sim logic only in `/shared` (TS) — native mirrors via `ha-shared`, never fork rules.
2. Server is authoritative; clients send inputs only.
3. `VITE_SERVER_HOST` / `HA_SERVER_HOST` for LAN — never hardcode IPs.
4. Landing stays minimal — no R3F, no game engine in `client/`.
5. User flow: `npm run play` → URL on phone → download APK → install → play.
6. **Direction:** Tierra (current island) → prefab ships (arcade flight, not KSP) → off-world bases → colonists. No orbital physics; 30+ FPS before space features.
