# Agencia Espacial Survival — AI Guidelines

Survival + space exploration: **Colyseus server** + **native Bevy client (2D)** + shared deterministic sim.  
Product vision: `native/docs/SPACE_SURVIVAL_VISION.md` · context bible: `labiblia.md`.

## Status

Server MVP: authoritative netcode, survival waves, Postgres, auth, `/updates`.
**Game client:** `native/` (Rust/Bevy ECS, **2D**: superficie Terraria + vuelo SFS — rama `rewrite-2d`; el 3D esférico vive solo en el historial de `main`). React/R3F/Expo **removed**.

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
6. **Direction:** juego 2D (Terraria en superficie + Space Flight Simulator en vuelo/planetas) conservando TODO el contenido (survival, máquinas, base lunar, colonos, ship builder). Tierra → naves prefab → off-world → colonos. No orbital physics; objetivo 60 FPS en el Redmi. NO reintroducir render 3D (biblia §1 anti-regresión).
