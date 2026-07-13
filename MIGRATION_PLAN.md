# MIGRATION_PLAN.md — Side-scroller → Top-Down 3/4 (LimeZu Modern)

> **Memoria externa de la migración.** Si la sesión se corta, leer este archivo,
> ver "ÚLTIMO CHECKPOINT" y reanudar desde ahí. Regla: `ha-shared` primero
> (puro + tests), luego el cliente. `main` debe COMPILAR en todo momento — el
> side-scroller sigue vivo hasta que el swap de plugins (Fase 2) lo reemplace.

---

## 0. Estado global

- **Objetivo:** migrar de vista lateral (Terraria-style, side-scroller) a
  **top-down 3/4 (vista aérea)** estilo Stardew/LimeZu, conservando el core:
  supervivencia, crafteo de naves, viaje órbita/Luna/planetas (SFS), y
  exploración subterránea profunda por **Z-Levels**.
- **Assets:** LimeZu — Modern Exteriors, Modern Interiors, Modern Farms,
  Modern UI. Se cargan por la capa drop-in ya existente (`art2d.rs` +
  `assets/art/`, `npm run sync:art`). Ver `assets/art/README.md`.
- **Rama:** `main` (el usuario decidió trabajar directo en main).
- **ÚLTIMO CHECKPOINT:** `Fase 1 COMPLETA (topdown.rs núcleo + 7 tests). Próximo: Fase 2 (render top-down + cámara + player).`

---

## 1. Convenciones de coordenadas (INVIOLABLE — leer antes de tocar nada)

- **Plano de suelo top-down = plano XZ** (el mismo del sistema solar/órbitas).
  - `body.x` = Este(+)/Oeste(−). `body.z` = Norte(−)/Sur(+).
  - `body.y` = altura sobre el suelo (salto/jetpack; normalmente 0 en superficie).
- **Render a pantalla:** `screen = (world.x, -world.z)` (Norte hacia arriba).
  IDÉNTICO al render orbital (`orbital2d.rs` usa `(x, -z)`) → superficie y
  espacio comparten proyección. Esto es EL punto que hace la migración barata.
- **Facing / animación:** `rot = atan2(move_z, move_x)`; 8 direcciones
  (`dir8 = round(rot / (TAU/8))`).
- **Z-Level (profundidad discreta):** `i32`. `0` = superficie, `-1,-2,…` =
  pisos subterráneos (minas/cuevas), NO es el eje Y de física. Es un índice de
  grilla independiente. El espacio NO es un z-level: es `WorldContext::Space`
  (modo orbital, zoom-out).
- **Draw order (3/4):** las entidades se ordenan por Y de pantalla
  (y-sorting): más al "sur" (z mayor) = dibuja delante. `Transform.z_sort`
  deriva de `world.z`. Los tiles van en una capa base fija por debajo.

---

## 2. Reglas técnicas de Z-Levels en Bevy

1. **Recurso `ActiveLayer { body: CelestialBody, z: i32 }`.** Solo se spawnean
   tiles + entidades del layer activo (B6: despawn al cambiar de layer).
2. **Grilla por (body, z):** cada piso es su propia grilla top-down determinista
   (`topdown_tile(body, z, tx, tz)`). Superficie (z=0) = biomas; z<0 = roca +
   vetas de mineral + huecos de cueva (densidad crece con la profundidad).
3. **Transiciones DISCRETAS (no gravedad):** escaleras/portales/taladro. Al
   pisar un `LayerPortal { to_z }` → cambia `ActiveLayer.z`, teleporta al portal
   gemelo, despawnea el layer viejo, spawnea el nuevo. Es un cambio de marco,
   NUNCA una cutscene (hereda B8).
4. **Colisión:** por-tile en la grilla activa, ejes separados (x luego z), sin
   gravedad. El "salto" (JUMP) queda para jetpack/espacio o se reasigna a acción.
5. **Espacio:** lanzar el cohete SALE de los z-levels al modo orbital (zoom-out
   al plano XZ, ya top-down). Aterrizar = zoom-in a la grilla top-down del cuerpo.
6. **Persistencia:** ediciones sparse por `(body, z, tx, tz)` en `tiles.json`
   (extender el esquema actual con el eje z). Flush-on-pause (B10).

---

## 3. Reuso vs cambio

**SE REUSA TAL CUAL (no tocar la lógica):**
- `packages/shared` + `server` (netcode/sim/catálogos).
- Inventario, crafteo, recetas, items, máquinas (lógica), colonos, ship_mass /
  ship_builder, survival, nav_data, hold-interact, HUD/UI, gamepad, persistencia,
  boot UI, perf/crash, arnés `debug/autodrive.rs`.
- **Modelo orbital SFS entero** (`solar_ephemeris`, `celestial_bodies`,
  `flight_physics`, `rocket_flight`, `orbital2d`) — ya es top-down.
- Capa de arte drop-in (`art2d.rs`, `assets/art/`) — hecha para LimeZu.

**SE MIGRA:**
- `ha-shared/flat2d.rs` (heightline + strip) → `ha-shared/topdown.rs`
  (grilla 2D por z-level + locomoción 8-dir). *flat2d queda hasta el swap.*
- `ha-game/sim.rs` tick: `step_flat_movement` → `step_topdown_movement`.
- `ha-game/world2d.rs` (chunks de strip vertical) → `world_topdown.rs`
  (chunks de grilla 2D + y-sorting + layer activo).
- `ha-game/camera.rs`: follow top-down, sin sesgo vertical, zoom-out a órbita.
  **Absorbe el fix de la cámara HUD duplicada (bug de v1.0.150).**
- `ha-game/scene.rs`, `targeting.rs`, `sky2d.rs`: adaptar a top-down.
- Player: sprite LimeZu con anim de caminar 4/8-dir.

**SE ELIMINA al final (Fase 7):** flat2d, world2d, y lo específico de strip.

---

## 4. Fases (con estado y sub-tareas)

Estados: ⬜ Pendiente · 🟨 En progreso · ✅ Completado

### FASE 0 — Preparación ✅
- ✅ Crear `MIGRATION_PLAN.md` (este archivo).
- ✅ Confirmar convenciones de coordenadas (§1) y reglas Z-Level (§2).
- ✅ Confirmar bug de terreno v1.0.150 = cámara HUD duplicada (ya fixeado en
  `7259659`, se reconstruye en Fase 2; el APK .150 es pre-fix).

### FASE 1 — Núcleo top-down en `ha-shared` (`topdown.rs`) ✅
- ✅ Generación de terreno 2D por `(body, z)`: biomas superficie (Tierra:
  agua/arena/pasto/tierra por elevación+humedad; Luna: regolito; planetas:
  roca roja), roca+cuevas subsuelo (`cave_open`, más abierto en profundidad),
  `Tile` enum + `solid()`/`dug_item()`/`color()`. Determinista.
- ✅ Locomoción 8-dir `step_topdown_movement` (colisión grid ejes separados,
  slide en paredes, sin gravedad, sprint/stamina). `dir8()` para animación.
- ✅ Depósitos deterministas `deposit_at` (más densos y con gemas en subsuelo).
- ✅ 7 tests: determinismo, variedad de biomas, cuevas por profundidad, dir8
  cardinales, caminar 8-dir sin gravedad, colisión con pared, depósitos.
- **DECISIÓN:** el remap superficie↔orbital NO es identidad de coordenadas en
  top-down (la superficie es un área 2D, la órbita un plano). La transición es
  un **zoom** (Fase 4): al lanzar se hace zoom-out al plano solar; al aterrizar,
  zoom-in a una grilla top-down centrada en el punto de aterrizaje. Los z-level
  portales (escalera/taladro) son lógica de cliente (Fase 3).

### FASE 2 — Render top-down + cámara + player (`ha-game`) ⬜
- ⬜ `world_topdown.rs`: chunks de grilla 2D (tilemap), streaming del layer
  activo, y-sorting.
- ⬜ `camera.rs`: cámara única top-down (fix HUD cam), follow, zoom.
- ⬜ Player: sprite + animación 4/8-dir (LimeZu vía art2d).
- ⬜ Swap de plugins en `lib.rs` (topdown reemplaza flat/world2d). `main` compila.
- ⬜ Verificar con autodrive: walk 8-dir + captura.

### FASE 3 — Z-Levels: superficie ↔ subsuelo ⬜
- ⬜ Portales escalera/taladro, grillas por piso, transición discreta.
- ⬜ Contenido de cueva (vetas por profundidad, oscuridad/linterna).
- ⬜ Persistencia con eje z; verificar descenso/ascenso.

### FASE 4 — Espacio: superficie top-down ↔ órbita SFS ⬜
- ⬜ Lanzamiento: zoom-out de grilla a plano orbital.
- ⬜ Aterrizaje: zoom-in a grilla top-down del cuerpo destino.
- ⬜ Verificar ignition/space/moon con autodrive adaptado.

### FASE 5 — Adaptar contenido a la grilla 2D ⬜
- ⬜ Árboles/rocas/mineral/máquinas/pad/colonos sobre (x,z).
- ⬜ Targeting top-down (tap/click sobre tile u objeto; rango radial).
- ⬜ Re-verificar dig/place/chop/craft/machine en top-down.

### FASE 6 — Assets LimeZu ⬜
- ⬜ Importar tilesets Modern (Exteriors/Interiors/Farms) + UI a `assets/art/`.
- ⬜ Autotiling de terreno; sprites de objetos y personaje.
- ⬜ `sync:art` + claves; fallback procedural mientras falten.

### FASE 7 — Limpieza + verificación + ship ⬜
- ⬜ Borrar flat2d/world2d y restos de strip.
- ⬜ Extender `debug/autodrive.rs` a top-down (11+ sistemas PASS).
- ⬜ Tests ambos lados verdes + `graphify update` + biblia §4/§1.
- ⬜ `npm run ship:android` (nuevo APK con el fix de cámara + top-down).

---

## 5. Log de checkpoints (append-only)

- **2026-07-13:** Fase 0 completa. Fase 1 COMPLETA: `ha-shared/src/topdown.rs`
  (terreno 2D por z-level, tiles, locomoción 8-dir, depósitos, 7 tests verdes;
  97 tests ha-shared en total). `main` compila; side-scroller intacto. Commit.
  **ÚLTIMO CHECKPOINT: Fase 2 — render top-down + cámara única + player sprite
  (crear `ha-game/world_topdown.rs`, reescribir `camera.rs` a follow top-down
  sin la 2ª cámara HUD, animar player 4/8-dir; luego swap de plugins en lib.rs).**
