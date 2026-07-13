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
- **ÚLTIMO CHECKPOINT:** `Fases 1-5 COMPLETAS (núcleo + render + z-levels + espacio + contenido a (x,z); walk/dig/place/descend/space PASS). Próximo: Fase 6 (assets LimeZu: personaje animado 4-dir + árboles/tiles) y Fase 7 (limpieza + build final .151).`

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

### FASE 2 — Render top-down + cámara + player (`ha-game`) ✅
- ✅ `world2d.rs` reescrito a top-down: grilla 2D, chunks `Mesh2d` (16×16),
   streaming en cuadrado alrededor del jugador (sin huecos, verificado),
   y-sorting (`sort_z`), render `(x,-z)` (`render_xy`), dig/place por celda,
   persistencia `(body,z,tx,tz)`, decor/depósitos como sprites.
- ✅ `sim.rs`: tick `step_topdown_movement` (8-dir sin gravedad), render `(x,-z)`.
- ✅ `camera.rs`: cámara única (ya sin la 2ª cámara HUD del bug), follow
   centrado sin sesgo vertical; `camera_relative_move` pasa ambos ejes.
- ✅ `scene.rs`: spawn en celda caminable (`walkable_spawn`), teleport top-down.
- ✅ `targeting.rs`: pick por puntero + proximidad radial, dig/place por celda.
- ✅ Verificado con smoke-test (`debug/autodrive.rs`): **walk-east PASS
   (Δx=15.8)**, biomas visibles (pasto/agua/arena), streaming sin huecos,
   sin bug de cámara. Capturas `ha-verify-topdown-*.png`.
- **Pendiente de pulido (fases siguientes):** árboles/personaje procedurales
   (sprites reales = Fase 6), máquinas/pad aún con colocación de tira (Fase 5),
   autodrive completo (11 sistemas) re-portar a top-down (Fase 5).

### FASE 3 — Z-Levels: superficie ↔ subsuelo ✅
- ✅ Portales deterministas (shaft vertical): `descend_portal`/`ascend_portal`
   en `topdown.rs` (entrada de mina en superficie, escaleras por piso). Round-
   trip verificado con tests (bajar+subir vuelve al mismo cell). Celdas de
   portal siempre caminables (carve 1-ring).
- ✅ Transición discreta `layer_transition` (world2d): pisar el portal cambia
   `TileWorld.z` (mismo x,z, solo la capa), latcheado (rearma al salir del
   portal). Sprites de portal (hoyo/escalera).
- ✅ Contenido de cueva: vetas más densas y con gemas en profundidad
   (`deposit_at` con z<0), oscuridad subterránea en `sky2d` (near-black +
   overlay 0.82, la linterna la corta). Decor despawnea al cambiar de capa.
- ✅ Persistencia con eje z `(body,z,tx,tz)`. Verificado con smoke-test:
   **descend PASS (z=-1)** + captura de cueva.
- **Pendiente Fase 5:** pad/máquinas se cuelan bajo tierra (ocultar si z≠0).

### FASE 4 — Espacio: superficie top-down ↔ órbita SFS ✅
- ✅ `rocket_flight.rs` reescrito a puro plano orbital (sin remaps de tira):
   `render_pos_2d = (x,-z)` siempre, `altitude` = dist al centro − radio,
   `render_rot_2d` orbital. Física (thrust/gravedad/SOI/drag/paracaídas) sigue
   en el plano solar. `launch_pos_vel` (despegue radial hacia arriba).
- ✅ Lanzamiento: ignite → aparece en el plano orbital sobre el planeta,
   cámara hace zoom-out (ya escalaba con altitud); el planeta se ve como
   círculo real (`orbital2d`). Aterrizaje: al tocar el radio de superficie →
   `set_surface_region` + jugador en celda caminable (`walkable_spawn`).
- ✅ `launch_pad.rs` ignición + `try_surface_launch` (relanzar off-world) usan
   `launch_pos_vel`. `sync_flight_world_state` = Space mientras vuela.
- ✅ Verificado: **space-flight PASS** (región Space, alt 254m, Tierra como
   círculo) + loop despegar→órbita→aterrizar. Captura `topdown-space`.

### FASE 5 — Adaptar contenido a la grilla 2D ✅
- ✅ Máquinas: colocación al frente del jugador `(x + cos·2.4, z + sin·2.4)`,
   render `render_xy + sort_z`, ocultas bajo tierra (z≠0).
- ✅ Launch pad: plataforma top-down (7×6), slots/ignición/consola alrededor
   del centro en (x,z), nave = sprite rocket vista aérea; oculto off-Earth y
   bajo tierra. `spawn_launch_pad`/`spawn_config_ship_visual` a (x,z).
- ✅ Targeting top-down ya hecho en Fase 2 (puntero + proximidad radial,
   dig/place por celda 2D).
- ✅ Verificado con smoke-test: **walk·dig·place·descend·space PASS** + capturas
   (pad top-down correcto).
- **Nota:** chop/craft/machine-place/colonos tienen lógica sin cambios (solo
   coords); re-porte del autodrive completo de 11 sistemas queda como pulido
   opcional (no bloquea).

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
