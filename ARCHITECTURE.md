# Architecture — Hunger Arena

Monorepo: **deterministic sim** in `packages/shared`, **authoritative server**, **native Bevy client**.

## Packages (current)

```
packages/shared/   GameSimulation (20Hz), AI, combat, schema, zod
server/            SurvivalRoom, auth REST, Drizzle/Postgres, /updates
client/            Landing ONLY — APK download panel
native/            Bevy ECS game client (Rust) — ha-shared + ha-game
```

## Data flow

```
Phone/PC native app (ha-game) ──inputs──► SurvivalRoom ──► GameSimulation
Landing (client/) ──download──► /downloads/hunger-arena.apk
```

## Invariants

- Server authoritative, input-only clients
- One simulation in `packages/shared` (TS); native ports constants/movement in `ha-shared` (Rust)
- Zod at HTTP/WS boundaries
- No IP hardcoding — `HA_SERVER_HOST` / `VITE_SERVER_HOST` / `UPDATE_BASE_URL`

See `native/README.md` for native client details.
