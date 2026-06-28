# La Biblia — Starship (Agencia Espacial Survival)

> **Documento maestro de contexto** para Claude, Gemini u otros LLMs.  
> Actualizado: **24 jun 2026** · **Agencia Espacial Survival** · **v1.0.92** (Android versionCode **92**) · Tierra + **hub orbital** + Luna · visión SFS-arcade en `native/docs/SPACE_SURVIVAL_VISION.md`.

---

## Cómo usar este archivo (límite de caracteres)

1. **Primera pasada:** pegá `§0 Identidad` + `§1 Arquitectura` + `§2 Reglas inviolables`.
2. **Tarea sim/netcode:** añadí `§3 Simulación` + `§5 Servidor`.
3. **Tarea cliente nativo:** añadí `§4 Cliente nativo` + `§4b Controles touch` + `native/README.md`.
4. **Tarea crafting/assets/máquinas:** añadí `§4c Catálogo procedural` + `packages/shared/src/content/machines/` + `packages/shared/src/sim/machines.ts`.
5. **Tarea visual:** añadí `§7 Arte visual` + skill `ghibli-toon-graphics`.
6. **Build/jugar:** añadí `§9 Builds` + `§10 Comandos` + `§16 Troubleshooting`.
7. **Feature nueva:** añadí `§12 Dónde va cada cosa`.
8. **Visión espacial / diseño producto:** añadí `§23 Visión espacial` + `native/docs/SPACE_SURVIVAL_VISION.md`.
9. **Legado combate (dormido):** añadí `§22 Auditoría legado` — mapeado, **no borrar**.

Cada sección es **autocontenida**. Los encabezados `§N` permiten recortar sin perder coherencia.

---

## §0 Identidad del proyecto

### Rebrand — Agencia Espacial Survival

| Campo | Valor |
|-------|-------|
| **Nombre producto** | **Starship** (Agencia Espacial Survival; repo interno `hunger-arena`; package Android `com.hungerarena.app`) |
| **Género** | Survival + **construir base/laboratorio** + **automatizar recolección** → (fases futuras) cohetes y otros planetas |
| **Fantasy** | Agencia casera en **Tierra** → máquinas pasivas → módulos → naves prefab → bases off-world → colonos |
| **Referencias** | KSP / Space Flight Simulator — **solo escala**; **cero** física orbital real |
| **Home base (Acto I)** | **Tierra** = isla actual (`world_spawn.rs`, seed **42**) — **no regenerar layout** |
| **Filosofía** | **Construir base → automatizar → explorar** |
| **Plataformas** | Android APK + Linux AppImage (Bevy), landing web, servidor Colyseus |
| **Stack** | Colyseus · **Rust/Bevy 0.15** · Postgres+Drizzle · npm + Cargo workspaces |
| **Sim** | Determinista TS (`packages/shared`); servidor **input-only**; `ha-shared` porta constantes |
| **Estética UI landing** | fStyle / Blood Dragon synthwave (`client/src/landing/`) |
| **Estética UI in-game** | Borderlands-style — bordes gruesos ink + acento amarillo (`ui_theme.rs`) |
| **Estética 3D** | Ghibli cel + outline `#2b2f3a` (WGSL `native/`) |
| **Versión actual** | `native/version.json` → **1.0.92** (Android versionCode **92**) — SFS FlightState + ha-span extendido |

### Regla de migración (INVIOLABLE)

**No reescribir el motor.** Contenido nuevo sobre motor probado:

| Capa | Intacto |
|------|---------|
| `native/crates/` | Bevy, cámara FP, input 3 gestos, AssetFactory, toon, perf (**MENU→PERF** / F3 desktop / ha-perf) |
| `server/` | Colyseus, auth, persistencia, `/updates` |
| `packages/shared/src/sim` core | tick 20 Hz, movimiento, determinismo, parkour, `interactions.ts` |

### Legado dormido (NO borrar, NO referenciar en contenido nuevo)

Clases scout/warrior/mage, habilidades, oleadas survival, AI hostiles — **permanecen** por fauna/peligro futuro. Mapa completo: **§22**.

### Sistemas activos del concepto nuevo

- **hold-interact / tap-ability** — operar máquina, recolectar, construir (mismo pipeline que legacy)
- **`content/machines/` + `sim/machines.ts`** — catálogo, tick, offline accrual
- **`interactions.ts`** — gather / mine / hold
- **`AssetFactory`** — `machine_wood_extractor` procedural (sin esqueletos)

### FASE 2 slice — `wood_extractor` (validación)

| Capa | Implementado |
|------|----------------|
| TS `content/machines/` | `wood_extractor`: pasiva, 0.5 wood/s, W5 S2 T1, hold 2s |
| TS `sim/machines.ts` | `stepMachines`, `accrueOfflineMachines`, vitest ×5 |
| Rust `ha-shared/machines.rs` | paridad catálogo + tests |
| Rust `ha-game/machines.rs` | colocación hold+martillo, tick FixedUpdate, glow+partículas |

`stone_extractor` = preview segundo tipo; **validación oficial = wood_extractor**. Cohetes/planetas = **prompt siguiente**.

**Estado actual (jun 2026):**

| Hecho | Pendiente |
|-------|-----------|
| Sim autoritativa + tests vitest (`shared`, **79** incl. máquinas + launch_pad + catalog-parity + icons) | Colyseus transport en `ha-game` |
| Servidor SurvivalRoom + auth + `/updates` (android + **desktop**) | Colyseus online en APK |
| **Isla 192×192** + playa + acantilados + **océano anillo horizon** (FASE A perf) | Resolución interna 3D si FPS bajo |
| **Cámara primera persona** FOV 72° + crosshair centro | Sensibilidad fina por device |
| Stick izq 38% + **zona look invisible** 62% (3 gestos) | Colyseus transport en `ha-game` |
| HUD: corazones top-left → **prompt acción + barra hold** debajo; **CONTROL PANEL** al acercarse a máquina | Placement/building loop completo |
| **BAG** → pestaña **INVENTORY** (default) + **CRAFT**; panel **spawn/despawn** bajo demanda (no `Display::None` permanente) | Mapa crafteable/usable |
| Botones **USE** + **JUMP** separados; JUMP contextual → **CLIMB** / **MANTLE** | Más estados contextuales en botón parkour |
| **106 assets** + `AssetFactory` + `IconFactory` (6 iconos HUD incl. `drumstick_half`) + `generate_world` (28/55) | Instancing restante catálogo |
| **Parkour Rust** — climb/jump/mantle; salto **independiente de USE**; funciona al sprintar | Paridad TS↔Rust tests parkour extendidos |
| Velocidad caminar reducida (scout **4.0** u/s); sprint stick **92%**; caminar cap **78%** radio | |
| **Máquinas FASE 2 slice** — `wood_extractor` end-to-end; output acredita `PlayerInventory` cada tick | Más tipos de máquina |
| **Launch pad FASE 1** — `machine_launch_pad` en zona reservada (28,-28); 3 módulos hold-interact + ignición; mesh único batcheado | Costes altos balance |
| **Secuencia lanzamiento FASE 2** — **15s** ascenso atmosférico (cielo SKY_TOP→negro, shake+estela 5s, subida 140m) → Space sin corte | Polish VFX reentry |
| **Hotbar 10 slots** — HAM + **8** items + BAG; gestos MC (tap select/deselect, doble-tap split, long-press drag, drop mundo) | |
| **Terreno heightmap** — colinas suaves alineadas a `terrain_height_at` (sin columnas cuadradas) | |
| **CPU profiler `ha-span`** — µs avg por sistema en logcat + embebido en línea `ha-perf` | Atacar culpable tras baseline Luna |
| **Perf v1.0.74–79** — draw3d **45–66** Tierra, **10–14** Luna/Space; frame_ms ~**48–52** → cuello **CPU/ECS**, no GPU | **30+ FPS** |
| **Luna FASE 3 slice** — `WorldContext::Luna`, gravedad 0.17, terreno gris, starfield, Tierra lejana, retorno hold | Más cuerpos celestes |
| **Espacio FASE 4 hub** — `WorldContext::Space`, gravedad 0.05, plataforma orbital 3D, Tierra abajo, base espacial 3 módulos, reingreso Tierra, tránsito Luna | Vuelo arcade entre sistemas |
| **Aterrizaje lunar v1.0.80** — hub → Luna: reentry 2.5s (shake/partículas) → caída libre **200 m** arcade (diseño 2 km) + retro **ACTION** + barra combustible HUD; touchdown suave/duro | Touch esfera Luna en hub |
| **GLB overrides** — drop-in `.glb` por `kind` vía `AssetFactory` (pipeline Blender futuro) | Assets reales |
| Colocación máquina: **martillo** + hold 2s en look zone **sin target** | Craft UI elige máquina (parcial: select en tab CRAFT) |
| **Inventario persistente** — `inventory.json` + `PlayerInventory` (TS `sim/inventory.ts` + Rust `ha-shared/inventory.rs`); **flush-on-pause** | Colyseus inventory schema |
| Startup default si no hay save: 8 wood / 4 stone / 2 sticks | |
| **25 items** + `CRAFT_RECIPES` en `content/items/recipes.ts`; craft consume/output en BAG | Sync codegen TS↔Rust |
| **Stamina** — 6 drumsticks × 2 unidades (=12 segmentos, paridad corazones); drain sprint **7/s**, regen **18/s** | |
| **Perf medido Redmi v1.0.50:** ~17–19 FPS @ 2297×1080; **ui ~52** (BAG cerrado; era ~105 pre-ui52, ~77 en v1.0.36); **draw3d ~62–64**; **chunk ~31–33** | **30+ FPS** sostenido |
| **Render scale FASE A** (`render/render_scale.rs`): **cerrado** — 0.75/0.65 con ui=52 **no** baja frame_ms; cuello = CPU/draw calls, no fill-rate | Reabrir solo en Luna si baseline ≥25 FPS (0.85 por bioma) |
| Pipeline Android + OTA + **`device-ready.sh`** / **`install-android-ready.sh`** (fallback MIUI) | Keystore release |
| **Debug:** `ha-crash` JSON, `ha-perf` ring buffer, catalog-parity snapshot, golden-movement, **perf-history.jsonl** | CI perf-history automático |
| **Linux AppImage** ~15 MB en landing + manifest `desktop` | Windows build |
| Shader toon embebido Android | iOS |
| `TouchLayout` + insets JNI; editor UI: MOVE / USE / **JUMP**; paneles flotantes **centrados** + botón **X/CLOSE** | |
| OASIS overworld + misiones + wallet | Economía real |
| UI editor bajo `MENU`; calibración (cam/move/curve/render) en `ui-layout.cfg` | |
| Smoke Redmi v1.0.50 — startup OK; perf ~17–19 FPS; ui **52**; draw3d **~64** (regresión 187 corregida) | **30+ FPS** · baseline Luna · test persistencia manual pendiente |

**Retirado (no buscar en el repo):** `apps/mobile`, `apps/desktop`, `packages/client-core`, `packages/client-gfx`, React Three Fiber, Expo, Tauri.

**Landing descarga:** Android APK + **Linux AppImage** en `/downloads/` (enlace directo, sin `fetch` de blobs grandes — OOM en Chrome móvil). Plugin `vite-downloads-plugin.ts` pone `Content-Disposition: attachment`.

**Postgres:** sin `server/.env` + sin Docker → store en memoria. Copiar `.env.example` + `docker compose up -d` para persistencia real.

---

## §1 Arquitectura (capas)

```
packages/shared/   VERDAD DEL JUEGO — GameSimulation, AI, combat, schema, zod, content/ (TS puro)
server/            SurvivalRoom Colyseus + Express auth + Drizzle/Postgres + /updates
client/            LANDING ONLY — panel descarga APK + AppImage Linux + FAQ (sin motor de juego, sin R3F)
native/
  crates/ha-shared/   Puerto Rust: constants, palette, classes, input, movement, assets/catalog
  crates/ha-game/     Cliente Bevy ECS — render, input, touch UI, sim local 20Hz, AssetFactory
```

### Dirección de dependencias

```
client/     →  shared (solo tipos manifest en landing)
server/     →  shared
native/ha-shared  →  (espejo numérico de shared; catálogo assets duplicado manualmente)
shared/     →  NADA de server/client/native/React/Rust
```

### Flujo de datos (objetivo online)

```
Input (stick izq + drag derecho + botones) → ha-game → [pendiente: Colyseus WS] → SurvivalRoom → GameSimulation
Landing → GET /downloads/hunger-arena.apk | /downloads/hunger-arena.AppImage → instalación manual / OTA manifest
```

Docs: `ARCHITECTURE.md`, `README.md`, `native/README.md`, `CLAUDE.md`.

---

## §2 Reglas inviolables

1. **Sim solo en `packages/shared`** — reglas de juego nunca duplicadas en Rust salvo port numérico en `ha-shared` (debe coincidir).
2. **Server input-only** — clientes envían `InputState`; nunca posiciones/HP.
3. **Determinismo TS** — RNG con seed; vitest en `shared`.
4. **Zod en fronteras** — mensajes WS/HTTP validados en server.o
5. **Sin IPs hardcodeadas** — `VITE_SERVER_HOST`, `HA_SERVER_HOST`, `UPDATE_BASE_URL`.
6. **Landing ≠ juego** — `client/` solo descarga; **no tocar** `downloadArtifact.ts` salvo bug crítico (enlace directo, sin fetch).
7. **No reintroducir React/R3F** en el cliente de juego — motor = `native/`.
8. **Logging** — connect/disconnect/security en server.
9. **Assets procedurales** — definición en `packages/shared/src/content/`; espejo Rust en `ha-shared/src/assets/`; meshes en `ha-game/src/assets/` (no PNGs para mundo).
10. **Android shader** — `toon.wgsl` embebido con `load_internal_asset!`; no depender de `assets/` APK para el cel.

### Guards B1–B10 (cliente nativo — obligatorios en sistemas nuevos)

| ID | Guard |
|----|--------|
| **B1** | Queries mutables conflictivas (Bevy B0001): `Transform` / `Node` / `BackgroundColor` en el mismo sistema → `ParamSet` o queries disjuntas. |
| **B2** | Geometría de mundo visible → `AssetFactory.require` + `ToonMaterial`; excepción documentada: esferas planetarias unlit en Space/Luna (`EarthSphere`, `MoonSphere`). |
| **B3** | `OceanHorizon` — exactamente **una** instancia; verificar query antes de spawn en `world_spawn.rs`. |
| **B4** | Sistemas por contexto: primera línea `if ctx != Expected { return; }` — `targeting` (Space = ray-sphere Luna), `chunk_manager` (early return Space), `particles` fireflies (no Space/Luna), `ocean_horizon` update (no Space/Luna). |
| **B5** | Paneles UI (BAG, craft, CONTROL PANEL) — **spawn/despawn**, nunca `Display::None` acumulativo. |
| **B6** | Al salir de WorldContext, despawn decor/región (`celestial_cleanup`, `terrain_streaming` release). Entidades Space no viven en Tierra. |
| **B7** | Floating origin: si `player_y > 500` durante ascenso → shift mundo + reset jugador a origen (`floating_origin.rs`). |
| **B8** | Cambio de `WorldContext` solo cuando `FlightState.altitude == 1.0` (despegue) o `== 0.0` (aterrizaje); nunca antes de que termine el lerp de atmósfera. |
| **B9** | Sin alloc en hot path (`Update`/`FixedUpdate`): no `Vec::new()` / `String::new()` sin capacidad pre-reservada. |
| **B10** | `inventory_persist` flush síncrono al entrar `FlightPhase::Launching` / `Entering` (antes de cambiar contexto). |

Validar: `npm run validate:arch`.

---

## §3 Simulación (`packages/shared`)

Sin cambios estructurales — sigue siendo la fuente de verdad del servidor.

| Constante | Valor |
|-----------|-------|
| `TICK_RATE` | 20 Hz (`TICK_MS` = 50 ms) |
| `WORLD_HALF` | **96** (isla jugable ~192×192; no infinito) |
| `OCEAN_HALF` | **384** (mar visual — límite estilo GTA V; movimiento clamped en isla) |
| `generate_world(42)` | 28 obstáculos + 55 interactables (determinista TS↔Rust) |
| Sprint | ×1.6 |
| `FRICTION` | 12 (velocity damping/s) |

Clases: scout (90 HP, **4.0** u/s), warrior (140, **3.8**), mage (80, **4.0**).

`InputState`: `move_x/z`, `aim_x/z`, `abilities` (bitmask 4 slots + craft intent), `sprint`, `action` (interact/gather/portal/climb).

**Cliente nativo (ha-game):** botones touch separados **USE** (action) y **JUMP** (salto parkour). `jump_edge` ya **no** sale del botón USE — evita conflicto con hold/recolectar. En el aire + borde trepable → botón muestra **CLIMB**; colgando → **MANTLE** (`parkour_button_hint` en `ha-shared/parkour.rs`). Pendiente: campo `jump` en `InputState` cuando exista transport Colyseus.

`WorldContext`: `overworld` o `mission` por entidad jugador (portales lógicos).  
`wallet`: saldo autoritativo en sim para recompensas de misión.  
Crafting: recetas en `packages/shared/src/content/items/recipes.ts`; intent de craft codificado en `abilities` para no abrir un protocolo paralelo.

**Máquinas (FASE 2):** catálogo en `packages/shared/src/content/machines/`; sim en `packages/shared/src/sim/machines.ts` (`placeMachine`, `tickMachines` @ 20 Hz). Primer slice: **wood_extractor** — coste 5 wood + 2 stone + 1 stick → **0.5 wood/s** pasivo. Espejo Rust: `ha-shared/src/machines.rs` + render/tick en `ha-game/src/machines.rs`.

AI, survival waves, parkour, schema Colyseus, zod messages — ver `packages/shared/src/sim/`.

Tests: `npm test` (vitest en shared). Catálogo: `npm test -w @hunger-arena/shared -- --run src/content/assets/catalog.test.ts`.

`ha-shared` porta: `constants.rs`, `palette.rs`, `movement.rs`, `classes.rs`, `parkour.rs`, `machines.rs`, `assets/catalog.rs`.

---

## §4 Cliente nativo (`native/`)

| Crate | Rol |
|-------|-----|
| `ha-shared` | Constantes, palette, InputState, step_movement, catálogo assets (106) |
| `ha-game` | App Bevy: sim/input/camera/hud, ToonMaterial, AssetFactory, touch UI |

### Render

- Shader WGSL 3-band cel: `native/crates/ha-game/assets/shaders/toon.wgsl`
- **Android:** shader compilado dentro de `libha_game.so` (`materials.rs` → `load_internal_asset!`) — APK asset I/O no lo resuelve.
- Outline inverted-hull `#2b2f3a`: shell hijo en `materials.rs`
- Luz hybrid dusk: `HYBRID_LIGHTS` en `palette.rs`
- Cámara **primera persona** Minecraft: `PerspectiveProjection` FOV 72°, ojos en jugador (`FP_EYE_HEIGHT_SCALE`), yaw/pitch desde zona look, far 512 para horizonte mar.
- Jugador: mesh oculto (`Visibility::Hidden`); solo crosshair blanco centro (`look_zone_ui.rs`).
- **Dos cámaras:** `Camera3d` order 0 (mundo) + `Camera2d` order 1 (HUD) con `ClearColorConfig::None` + alpha blend — **no borrar** el frame 3D al dibujar UI
- ClearColor cielo: `SKY_TOP` (`#6eb8e8`)
- Android: **resolución nativa** del dispositivo (no forzar 1280×720 en `lib.rs`); `PresentMode::Fifo` (vsync).
- Juiciness: camera shake por trauma, squash & stretch, lerp atmósfera/regiones.
- **Mundo isla:** `world_spawn.rs` — océano **anillo horizon** (`render/ocean_horizon.rs`, material unlit barato), pradera, variación suelo, anillo playa, acantilados perimetral, flora por chunk, props de `generate_world` **batched por chunk** (`render/mesh_batch.rs`).
- Chunk streaming: `chunk_manager.rs` — decor por chunk 40×40; terreno/océano `WorldStatic` siempre visible.

### Parkour + sim local (`sim.rs`)

- `ha-shared/src/parkour.rs` — climb, jump, mantle + `parkour_button_hint()` (Jump / Climb / Mantle).
- Colisión contra obstáculos de `generate_world(42)`; `jump_edge` desde botón **JUMP** (buffer 160 ms); interpolación visual.
- `sim.rs`: `action` = USE/hold/climb; `jump_edge` separado — **salto funciona al sprintar**.
- Joystick: deadzone ~0.07, smooth 42/s, caminar cap **78%** radio si no sprint, sprint entra **92%** (sale 75%), `Time<Virtual>` respeta pause JNI.

### Máquinas + inventario (`machines.rs`, `inventory_persist.rs`)

- Tick pasivo en FixedUpdate; glow + partículas al colocar; output → **`PlayerInventory.add(item)`** cada tick.
- Colocación desde `look_interact.rs`: estado `placing_machine`, requiere **martillo** equipado, hold **2s** en zona look **sin** `LookTarget`.
- Asset `machine_wood_extractor` en catálogo TS + Rust.
- **Persistencia:** `inventory.json` junto a `ui-layout.cfg` (cwd app Android: `/data/user/0/com.hungerarena.app/`). Guarda en gather/craft/place/tick (debounce 3s) + **flush completo al pausar app** (`is_app_paused()` JNI). Carga en Startup.
- **TS:** `packages/shared/src/sim/inventory.ts` — `PlayerInventoryMap`, `addItem/removeItem/craftFromRecipe`, vitest ×7.
- **Rust:** `ha-shared/inventory.rs` + `recipes.rs`; wrapper Bevy `inventory_persist.rs::PlayerInventory` Resource.

### Mundo isla (no infinito)

- **Jugable:** cuadrado `[-96, 96]` en X/Z (~192 m de lado).
- **Límite:** acantilados perimetral + **mar visible** (`OCEAN_HALF=384`) — sensación GTA V / horizonte océano, no chunks infinitos tipo Minecraft.
- **Generación:** `generate_world(seed)` — 28 obstáculos + 55 nodos (árbol/roca/contenedor); flora decor 320 instancias por chunk.
- **Archivos:** `packages/shared/src/sim/world.ts`, `ha-shared/src/world.rs`, `ha-game/src/world_spawn.rs`, `chunk_manager.rs` (decor 40×40).
- Movimiento clamped en `WORLD_HALF`; nadar/fuera de isla = futuro (hoy invisible wall lógico).

### FlightState — secuencia espacial cinematográfica (`flight_state.rs`)

Resource `FlightState` orquesta ascenso/reentrada sin teleports prematuros:

| Campo | Rol |
|-------|-----|
| `phase` | `SfsPhase`: Idle → Launching (~15s) → InSpace → Entering (~10s) → OnSurface |
| `altitude` | 0.0 = superficie, 1.0 = banda vacío; **único** driver del lerp de atmósfera/terrain (B8) |
| `destination` | `WorldContext` objetivo en Entering |
| `exhaust_active` | Partículas rocket_exhaust + shake decaen con altitude |

Efectos orquestados: `LaunchAtmosphere` (fog/clearcolor/luz), `camera` trauma, `terrain_streaming` ventana 5×5→3×3→1×1→release, `floating_origin` (B7), `inventory_persist` flush (B10).

Ignición: hold 2s en pad → `FlightState::begin_launch`. Luna desde Space: ray-sphere `MoonSphere` + hold → `begin_entering(Luna)`. Retorno lunar: `LunarReturn` → `begin_entering(Overworld)`.

`ha-span` (Módulo 0): `cpu_span.rs` loguea cada 60f: `camera chunk target hud particles atmo flight terrain total_update` µs_avg.

### Sim local

- `FixedUpdate` @ 20 Hz → `step_movement`
- `Update` → interpolación visual `PlayerVisual` + `Time<Fixed>::overstep_fraction()`
- Lee `MovementIntent` + `TouchActions` (`action`, `jump`, `jump_edge`, `abilities`) → `InputState`

### Build Android

```bash
npm run ship:android           # cargo-ndk + gradlew + publish + verify
npm run ship:android -- --rebuild-rust
npm run play                   # ship + server + landing
cargo test -p ha-shared        # tests port sim + máquinas + parkour
cargo check -p ha-game --features android
```

### Build Linux (AppImage)

```bash
make -C native release                              # → target/release-native/ha-game (~42 MB)
native/scripts/build-appimage.sh                    # → target/android-artifacts/hunger-arena.AppImage (~15 MB)
node scripts/publish-update.mjs --platform desktop \
  --file native/target/android-artifacts/hunger-arena.AppImage --version $(node -p "JSON.parse(require('fs').readFileSync('native/version.json')).version)")
```

Requisito packaging: `appimagetool` (descarga one-shot en `/tmp/appimagetool-x86_64.AppImage` si falta).

Pipeline: `scripts/ship-native-android.mjs` → `scripts/lib/android-env.mjs`, `cargo-lock.mjs`, `android-ndk-libs.mjs` (copia `libc++_shared.so`).

Gradle assets: `native/android/app/build.gradle` → `../../crates/ha-game/assets`.

Versión: `native/version.json`. APK ~65 MB debug · AppImage ~15 MB.

Última verificación manual (**22 jun 2026**, v**1.0.50**):

```bash
npm run ship:android -- --rebuild-rust
./scripts/device-ready.sh          # espera USB → install → verify → ha-perf
# o solo install:
./scripts/install-android-ready.sh
./scripts/verify-android-startup.sh
./scripts/smoke-android-perf.sh 45
# inspeccionar save:
adb shell "run-as com.hungerarena.app cat inventory.json"
```

Resultado: APK v1.0.50 en Redmi `21121119VL` (`733493b90511`) — startup estable; perf **~17–19 FPS** @ 2297×1080; **ui ~52**; **draw3d ~64**; **chunk ~33**. Regresión draw3d 187→64 corregida (`sync_biome_visibility` ya no fuerza `ChunkDecor` visible). Launch pad + secuencia + Luna slice instalados. **Pendiente:** baseline perf Luna (**MENU → PERF** en superficie lunar); test persistencia manual (extractor → 30s → cerrar → reabrir wood).

---

## §4b Controles touch (ha-game) — estilo Minecraft FP

Módulos: `input/joystick.rs`, `input/layout.rs`, `input/look_interact.rs`, `input/look_zone_ui.rs`, `input/actions.rs`.

### Layout pantalla (`layout.rs`)

| Zona | Fracción | Función |
|------|----------|---------|
| **Move** | Izquierda **38%**, banda inferior | Stick locomoción (cámara-relativo vía `camera_relative_move`) |
| **Look** | Derecha **62%**, altura completa | Zona **invisible** — sin línea cyan ni overlay; solo crosshair `+` centro |
| **USE** | Botón amarillo | Portales, interact, hold gather/mine, colocar máquina (`action=1`) |
| **JUMP** | Botón cyan | Salto parkour; en contexto → **CLIMB** / **MANTLE** (mantiene `action=1` mientras se presiona) |

**Multi-touch:** stick move + **USE** + **JUMP** simultáneos (3 dedos). Los botones de acción usan polling táctil independiente por `pointer_id` (`ActionTouchState`) — **no** el widget `Button` de Bevy (solo 1 dedo).
| **MENU** | Top-center | Opciones + editor UI (MOVE / USE / JUMP) |
| **BAG** | Último slot hotbar | Abre panel **INVENTORY** (stock actual); tab **CRAFT** para recetas + selectores máquina. Panel **no existe en ECS** cuando está cerrado (spawn al abrir, `despawn_recursive` al cerrar). **Esc** / **X** cierran. |

### Tres gestos en zona look (`look_interact.rs`)

| Gesto | Detección | Efecto |
|-------|-----------|--------|
| **Drag** | Movimiento > umbral px | Gira yaw/pitch FP + actualiza `aim_x/z` |
| **Hold ~2000ms** | Sin drag, sin target, martillo equipado | **Colocar máquina** (`wood_extractor` slice) — progreso en barra HUD |
| **Hold ~200ms** | Sin drag, con target | Interact según `interactions.ts` (minar, recoger…) |
| **Tap rápido** | < ~200ms, sin drag | Habilidad de clase (`tapAbilitySlot`) + anillo cooldown |

- **Eliminados:** botones ATK/DASH (duplicaban tap en look).
- Sprint: stick >**92%** radio (histéresis sale 75%); caminar limitado al **78%** del radio si no sprint.
- Salto: botón **JUMP** dedicado + buffer **160 ms** (`ActionInputBuffer`); **no** comparte botón con USE.
- Deadzone ~12%, sensibilidad en `look_sensitivity()`.
- Targeting FP: ray desde `look_forward()` (`targeting.rs`).

### TouchLayout + JNI

- Recalcula cada frame desde `Window` real + insets notch (`MainActivity.nativeSetDisplayInsets`).
- `TouchBlockRects`: botones/menús no roban drag look.

### HUD (`hud.rs` + `ui_editor.rs` + `inventory_persist.rs`)

- **Top-left:** fila corazones → bloque con borde ink (**prompt acción** + **barra hold** debajo). Texto de recolectar/construir/materiales faltantes aquí — **no** abajo en el centro.
- **Top-right:** muslitos energía/stamina — **6 iconos × 2 unidades** (12 segmentos, igual que corazones); `drumstick_full/half/empty` vía **IconFactory**.
- **Centro pantalla (modal):** panel **BAG** (spawn bajo demanda); **CONTROL PANEL** máquinas (~4 m, persistente en ECS — candidato a despawn); **MENU/SETTINGS** editor (centrados + CLOSE).
- Hotbar MC + `BAG`; anillo cooldown habilidad (tap look zone).
- Estilo UI: bordes **4–5 px** estilo Borderlands (`ui_theme.rs`: `ink_outline`, `accent_ink`, `panel_border_width`).
- Editor UI bajo `MENU`; guarda `ui-layout.cfg` (slots `move`, `use`, `jump`, línea `cal` con render_scale).
- **Overlay perf (móvil):** tap **MENU** → **PERF** (el menú se cierra solo; no deja bloqueados stick/look/USE). Desktop/Linux: **F3**. Tap PERF otra vez vía MENU para apagar. Sin overlay: `adb logcat | rg ha-perf` cada 5s.
- Sin minimap fijo (futuro: item crafteable).

**Regla perf UI:** no dejar paneles grandes con `Display::None` permanentes — Bevy sigue iterando entidades (`ha-perf ui=` cuenta nodos). BAG ya usa spawn/despawn (`bag_hotbar_system` + `bag_panel_system`).

### Desktop (Linux AppImage)

WASD mover, mouse look (parcial), E/Espacio interact. Ejecutar: `chmod +x hunger-arena.AppImage && ./hunger-arena.AppImage`.

---

## §4c Catálogo procedural (crafting / construcción)

**Autoritativo TS:** `packages/shared/src/content/`

```
assets/types.ts      AssetClass, AssetCategory, AssetDef, ItemDef
assets/catalog.ts    106 kinds (terreno, flora, construcción, equipo, consumibles, UI…)
assets/registry.ts   getAsset, assetKindForInteractable, assetKindForWorldProp
content/ui/icons.ts  Catálogo iconos HUD (6 kinds: heart×3 + drumstick×3)
content/ui/registry.ts
items/catalog.ts     25 items (wood, stone, sticks, pickaxe, potions…)
items/registry.ts    getItem, itemsByCraftTag
machines/catalog.ts  wood_extractor (+ futuros)
machines/registry.ts requireMachine, machinesByOutputItem
content/index.ts     re-export
```

**Espejo Rust:** `native/crates/ha-shared/src/assets/` (manual sync, sin codegen aún).

**Meshes cliente:** `native/crates/ha-game/src/assets/`

- `factory.rs` — `AssetFactory` lazy cache (1 mesh+material por kind)
- `icon_factory.rs` + `icon_builders.rs` — iconos HUD desde catálogo `ui/icons` (sin patrones hardcoded)
- `builders.rs` — primitivas procedurales por `AssetClass`
- `AssetFactoryPlugin` en `lib.rs`

**Consumo TS:**

```ts
import { getAsset, getItem, assetKindForInteractable } from "@hunger-arena/shared";
```

**Consumo Rust:**

```rust
factory.require("item_potion_red", &mut meshes, &mut materials);
```

**Pendiente:** instancing masivo restante, mapa crafteable/usable, Colyseus inventory schema, despawn CONTROL PANEL/editor cuando ocultos (ui→~77), más máquinas en catálogo, **30+ FPS**.
**Hecho (Jun 2026):** `InstancedDecorPlugin`; props batched por chunk; `machine_wood_extractor`; **`PlayerInventory` + `inventory.json` + craft BAG**; BAG spawn/despawn; flush-on-pause.

---

## §5 Servidor (`server/`)

Igual que antes — **no depende del cliente React retirado**.

- `SurvivalRoom` — sim autoritativa 20Hz, input-only, zod, token-bucket
- Auth REST — register/login/refresh, invitado por deviceId
- `/updates/manifest` — **android** + **desktop** (semver, sha256, url, sizeBytes, mandatory)
- Artifacts: `server/updates/artifacts/hunger-arena.apk` · `hunger-arena.AppImage`
- Health: `/updates/health`

Dev: `http://localhost:2567/colyseus`, `/playground` si dev.

---

## §6 Landing (`client/`)

**NO es cliente de juego.**

- `LandingPage.tsx` — fStyle CSS, botones descarga **Android + Linux + iOS (próximamente)**
- `downloadArtifact.ts` — **enlace directo** `<a href="/downloads/...">` (no fetch)
- `useDownloads.ts` — probe HEAD `/downloads/*.apk|AppImage` + metadata manifest
- `vite-downloads-plugin.ts` — MIME APK + Content-Length + attachment
- Sin R3F, sin showcase, sin `client-core`

---

## §7 Arte visual

| Capa | Estilo | Fuente actual |
|------|--------|---------------|
| Landing HTML/CSS | fStyle Blood Dragon | `client/src/landing/`, `theme.ts` |
| HUD in-game | Borderlands ink borders + BD palette | `ui_theme.rs`, `hud.rs`, `actions.rs` |
| Mundo 3D in-game | Ghibli cel + dusk neon | `palette.rs` + WGSL embebido |
| Assets mundo | Procedural meshes toon | `AssetFactory` + catálogo |

Skills: `ghibli-toon-graphics`, `.cursor/skills/procedural-asset-factory/reference.md`.

---

## §8 Persistencia

Server: Postgres + Drizzle (perfiles, match history) — sin cambios.

Cliente nativo: layout de UI guardado en `ui-layout.cfg` (`UiLayoutConfig`: slots `move`, `use`, `jump`). Inventario/perfil local real pendiente.

---

## §9 Builds y flujo del jugador

### Lo que hace el usuario

```bash
npm install
npm run play
```

1. Compila APK (`ship-native-android.mjs`) si hace falta
2. Publica en `server/updates/` + `client/public/downloads/`
3. Levanta server `:2567` + landing `:5173`
4. Imprime URL WiFi → celular → **Descargar APK** → instalar → abrir

Skip rebuild: `npm run play -- --no-ship`

### Instalar en dispositivo

```bash
npm run ship:android                    # build + publish APK (si cambió Rust)
./scripts/device-ready.sh               # ★ espera USB → install → verify → perf sample
./scripts/install-android-ready.sh      # adb install; fallback MIUI pm install + verify startup
# o manual MIUI:
adb push client/public/downloads/hunger-arena.apk /data/local/tmp/
adb shell pm install -r -t /data/local/tmp/hunger-arena.apk
./scripts/verify-android-startup.sh
```

**MIUI/Xiaomi:** `adb install` suele fallar con `INSTALL_FAILED_USER_RESTRICTED`. **Workaround que funciona:** `adb push` + `pm install -r -t` (ya en `install-android-ready.sh`). Alternativa: activar **Instalar vía USB** + popup en teléfono.

Alternativa WiFi: `http://<IP_LAN>:5173/downloads/hunger-arena.apk` · Linux: `.../hunger-arena.AppImage`

### Desarrollador

```bash
npm run ship:android -- --rebuild-rust   # forzar cargo ndk
npm run ship:android -- --no-bump        # sin bump version.json
npm run sync:downloads
npm run verify:apk
npm run verify
```

### Archivos release

| Qué | Ruta |
|-----|------|
| Manifest | `server/updates/manifest.json` |
| APK servidor | `server/updates/artifacts/hunger-arena.apk` |
| AppImage servidor | `server/updates/artifacts/hunger-arena.AppImage` |
| APK landing | `client/public/downloads/hunger-arena.apk` |
| AppImage landing | `client/public/downloads/hunger-arena.AppImage` |
| Versión bump | `native/version.json` + `scripts/bump-app-version.mjs` |
| AppImage script | `native/scripts/build-appimage.sh` |

### OTA

`publish-update.mjs` actualiza manifest con sha256/size. Clientes con versión menor pueden auto-actualizar al abrir (si el cliente implementa check — servidor listo).

---

## §10 Comandos esenciales

```bash
npm install
npm run play                 # ★ ship + server + landing
npm run play -- --no-ship
npm run ship:android
npm run dev                  # server + landing
npm run dev:server           # :2567
npm run dev:client           # landing :5173
npm run dev:native           # Bevy desktop (make -C native run)
make -C native release       # binario Linux release-native
native/scripts/build-appimage.sh   # empaquetar AppImage
npm test                     # vitest shared (incl. machines.test.ts)
npm run test:native          # cargo test ha-shared
npm run verify:apk
npm run validate:arch
./scripts/device-ready.sh    # cuando el celu está enchufado por USB
```

**Requisitos ship Android:** `ANDROID_HOME`, NDK (`scripts/lib/android-env.mjs`), Rust, **cargo-ndk**, Gradle. **No** usar `cargo-apk` (incompatible Bevy 0.15 GameActivity).

**Espacio disco:** `cargo clean` si linker bus error / disco lleno (~3–9 GB en `native/target/debug`).

---

## §11 Variables de entorno

| Variable | Dónde | Uso |
|----------|-------|-----|
| `VITE_SERVER_HOST` | client landing | IP laptop en WiFi |
| `VITE_SERVER_PORT` | client | default 2567 |
| `HA_SERVER_HOST` | build APK nativo | bake IP en cliente |
| `UPDATE_BASE_URL` | publish-update | URLs en manifest |
| `DATABASE_URL` | server | Postgres |
| `JWT_SECRET` | server | prod ≥16 chars |
| `PORT` / `HOST` | server | 2567 / 0.0.0.0 |
| `ANDROID_NDK_ROOT` | scripts android | toolchain NDK |

---

## §12 Dónde va cada feature

```
¿Reglas de juego?
  → packages/shared + vitest
  → port numérico en ha-shared (sin duplicar lógica compleja)

¿Assets / items / crafting data?
  → packages/shared/src/content/
  → espejo ha-shared/src/assets/ + meshes ha-game/src/assets/

¿Render/input/HUD/touch UI?
  → native/crates/ha-game (input/, hud.rs, ui_theme.rs, camera.rs, scene.rs)
  → Botones USE + JUMP; prompt bajo corazones; CONTROL PANEL cerca de máquinas
  → UI gameplay limpia: no minimap fijo, no panel editor permanente

¿Red autoritativa?
  → server/ (listo) + ha-game transport (pendiente)

¿Visión espacial / naves / colonos / vuelo arcade?
  → native/docs/SPACE_SURVIVAL_VISION.md + labiblia §23
  → NO implementar antes de perf 30+ FPS + craft persistente Tierra

¿Landing / descarga APK o AppImage?
  → client/src/landing/ — NO fetch blob

¿Máquinas / automatización?
  → packages/shared/src/content/machines/ + sim/machines.ts
  → ha-shared/src/machines.rs + ha-game/src/machines.rs
  → colocación: look_interact.rs (martillo + hold 2s sin target)

¿Launch pad / secuencia / Luna?
  → packages/shared/src/sim/launch_pad.ts + world_context.ts + interactions.ts
  → ha-shared/src/launch_pad.rs + world_context.rs
  → ha-game/src/launch_pad.rs (colocación zona 28,-28, markers invisibles)
  → ha-game/src/launch_sequence.rs (~15s transición Tierra→Space)
  → ha-game/src/lunar_landing.rs (reentry + freefall + retro-thrust; `jump_to_luna` ya no teleporta)
  → ha-game/src/moon_world.rs (terreno gris, estrellas, Tierra, retorno)
  → scene.rs RegionId::Luna + chunk_manager culling; **NO** forzar ChunkDecor en sync_biome_visibility
  → Pad visual = **1 mesh** (`machine_launch_pad` merged); interact = ECS sin draw call

¿Script release Android / Linux?
  → scripts/ship-native-android.mjs, native/scripts/build-appimage.sh
  → publish-update.mjs, play.mjs
```

---

## §13 Skills para IA

| Skill | Cuándo |
|-------|--------|
| `ghibli-toon-graphics` | Cel/outline en native WGSL |
| `fstyle-brand` | UI landing Blood Dragon |
| `procedural-asset-factory` | Catálogo + AssetFactory + IconFactory |
| `CLAUDE.md` | Resumen agente Cursor |
| `native/README.md` | Build Android/desktop Rust |
| `native/docs/SPACE_SURVIVAL_VISION.md` | Visión espacial completa (naves, bases, colonos, física arcade) |
| `native/docs/PLAYER_WORLD_DESIGN.md` | Contrato parkour ↔ mundo Tierra |

---

## §14 Mapa de archivos críticos

```
packages/shared/src/sim/GameSimulation.ts
packages/shared/src/content/assets/catalog.ts
packages/shared/src/content/items/catalog.ts
packages/shared/src/content/ui/icons.ts
packages/shared/src/content/machines/catalog.ts
packages/shared/src/test-utils/catalog-parity.ts
packages/shared/golden-movement.json
packages/shared/src/sim/machines.ts
server/src/rooms/SurvivalRoom.ts
server/updates/manifest.json

native/crates/ha-shared/src/constants.rs
native/crates/ha-shared/src/palette.rs
native/crates/ha-shared/src/movement.rs
native/crates/ha-shared/src/parkour.rs            # step_movement + parkour_button_hint
native/crates/ha-shared/src/ui/icons.rs         # espejo catálogo ui/icons
native/crates/ha-shared/src/machines.rs
native/crates/ha-shared/src/assets/catalog.rs
native/crates/ha-game/src/lib.rs
native/crates/ha-game/src/sim.rs                 # sim local + jump/action split + colisión world
native/crates/ha-game/src/machines.rs            # tick máquinas + VFX colocación
native/crates/ha-game/src/ui_theme.rs            # Borderlands-style borders + botones USE/JUMP
native/crates/ha-game/src/ui/icon_factory.rs     # IconFactory HUD
native/crates/ha-game/src/debug/crash.rs         # ha-crash JSON + perf ring buffer
native/crates/ha-game/src/materials.rs          # toon shader embebido
native/crates/ha-game/src/world_spawn.rs          # isla + playa + props generate_world
native/crates/ha-game/src/render/ocean_horizon.rs # anillo océano barato (FASE A perf)
native/crates/ha-game/src/render/mesh_batch.rs    # batch props por chunk (FASE C perf)
native/crates/ha-game/src/hud/icons.rs            # iconos HUD vía IconFactory (FASE B perf)
native/crates/ha-game/src/hud.rs                  # prompt bajo corazones + CONTROL PANEL máquinas
native/crates/ha-game/src/camera.rs             # cámara FP + look_forward + camera_relative_move
native/crates/ha-game/src/input/layout.rs       # split 38/62 move/look + sensibilidad
native/crates/ha-game/src/input/look_zone_ui.rs # solo crosshair centro (sin overlay)
native/crates/ha-shared/src/world.rs            # generate_world espejo TS
native/scripts/build-appimage.sh                # Linux AppImage packaging
native/crates/ha-game/src/scene.rs              # regiones Tierra/Cueva/Luna, atmósfera, sync_biome (sin ChunkDecor)
native/crates/ha-game/src/chunk_manager.rs      # decor visible 3×3 chunks (única fuente culling ChunkDecor)
native/crates/ha-game/src/launch_pad.rs         # pad merged mesh + module/ignition markers
native/crates/ha-game/src/launch_sequence.rs    # jump_to_space/earth + FlightTransition HUD label
native/crates/ha-game/src/moon_world.rs         # Luna decor: ground, craters, starfield, Earth globe
native/crates/ha-shared/src/launch_pad.rs       # paridad assemble_module / launch_pad state
native/crates/ha-shared/src/world_context.rs    # WorldContext Tierra/Luna + gravity_scale
packages/shared/src/sim/launch_pad.ts
packages/shared/src/sim/world_context.ts
native/crates/ha-game/src/assets/glb.rs         # GLB override pipeline (debug/dev)
native/crates/ha-shared/src/interactions.rs
native/crates/ha-game/src/targeting.rs             # ray pick InteractTarget → LookTarget
native/crates/ha-game/src/input/look_interact.rs   # 3 gestos mitad derecha (drag/hold/tap)
native/crates/ha-game/src/debug/perf.rs            # overlay perf (F3 desktop; MENU→PERF móvil) + ha-perf logcat
native/crates/ha-game/src/debug/cpu_span.rs        # ha-span CPU µs avg (camera/chunk/target/hud/particles/atmo/flight/terrain/total)
native/crates/ha-game/src/flight_state.rs          # FlightState + SfsPhase (Launching/InSpace/Entering) + B8/B10
native/crates/ha-game/src/floating_origin.rs       # B7 floating origin shift >500m
native/crates/ha-game/src/greedy_mesh.rs           # greedy 3D cuboid merge terrain (ha-mesh-stats debug)
native/crates/ha-game/src/celestial_sphere.rs      # EarthSphere/MoonSphere/StarfieldSphere + ray_sphere_t
native/crates/ha-game/src/terrain_streaming.rs     # chunks 16m, ventana altitude-driven 5×5→1×1→release
native/crates/ha-game/src/render/instanced_decor.rs # batch earth patches → 1 draw call
native/crates/ha-game/src/input/joystick.rs     # stick izq; look delegado a look_interact
native/crates/ha-game/src/inventory_persist.rs   # inventory.json + flush-on-pause + PlayerInventory Resource
native/crates/ha-shared/src/inventory.rs         # paridad sim/inventory.ts
native/crates/ha-shared/src/recipes.rs           # paridad content/items/recipes.ts
packages/shared/src/sim/inventory.ts             # PlayerInventoryMap + craft helpers
native/crates/ha-game/src/render/render_scale.rs # FASE A blit (default off / cal 1.0)
native/crates/ha-game/src/hud.rs                 # bag_hotbar_system + bag_panel_system (spawn/despawn)
native/crates/ha-game/src/ui_editor.rs          # editor MOVE/USE/JUMP → ui-layout.cfg
native/crates/ha-game/src/platform/android.rs   # JNI insets + nativeSetPaused
scripts/device-ready.sh                         # wait USB → install → verify → perf
scripts/install-android-ready.sh                # adb + fallback MIUI pm install
scripts/verify-android-startup.sh
scripts/verify-android-sustained.sh
native/android/app/src/main/java/.../MainActivity.java
native/android/app/src/main/res/drawable/ic_launcher_background.xml
native/android/app/src/main/res/drawable/ic_launcher_foreground.xml
native/version.json

client/src/landing/LandingPage.tsx
client/public/downloads/hunger-arena.apk
client/public/downloads/hunger-arena.AppImage

scripts/play.mjs
scripts/ship-native-android.mjs
scripts/publish-update.mjs
scripts/verify-apk-pipeline.mjs
scripts/lib/android-env.mjs
scripts/lib/android-ndk-libs.mjs
```

---

## §15 Rendimiento (cliente nativo)

- Target: **60 fps** en gama media (Bevy ECS, sin puente JS).
- **Medido Redmi 21121119VL** (2297×1080, `perf-history.jsonl`):

| Versión | render_scale | ui nodes | fps | frame_ms | Notas |
|---------|-------------|----------|-----|----------|-------|
| v1.0.36 | 1.0 | **77** | ~20 | ~50 | baseline pre-craft panel |
| v1.0.38 | 0.75 | **117** | **12** | **~84** | blit + panel craft oculto permanente |
| v1.0.39 | 1.0 | **115** | ~18 | ~55 | revert scale default |
| v1.0.42 | 1.0 | **115** | ~18 | ~54 | inventario persistente + craft UI |
| **v1.0.46** | **1.0** | **105** | **~17–20** | **~51–58** | BAG spawn/despawn; flush-on-pause |
| v1.0.46-rs0.75 | 0.75 | 105 | ~19 | ~53 | **sin ganancia** vs 1.0 @ ui=105 |
| v1.0.46-rs0.65 | 0.65 | 105 | ~19 | ~52 | **sin ganancia** |
| v1.0.49-ui52 | 1.0 | **52** | ~17–19 | ~52–57 | HUD ui52 refactor; draw3d ~62 |
| v1.0.49 (regresión pad) | 1.0 | 52 | ~16–18 | ~55–63 | **draw3d ~187**, chunk ~122 — bug sync_biome |
| **v1.0.50** `tierra_post_pad_fix` | **1.0** | **52** | **~17–19** | **~53–58** | draw3d **~64**, chunk **~33**; pad 1 mesh + culling fix |

- **Lección ui:** 77→115 (+38 nodos panel craft) empeoró FPS; ui→**52** recuperó baseline draw/cpu.
- **Lección draw3d:** `sync_biome_visibility` no debe tocar `ChunkDecor` — anula culling 3×3 de `chunk_manager`.
- **Lección render scale:** con ui=52, 0.75/0.65 **no** baja frame_ms → cuello = draw calls/CPU, no fill-rate GPU.
- **Precondición naves/colonos:** **30+ FPS** — **NO cumplida** (~17–19 FPS Tierra).
- **Baseline Luna:** pendiente medición **MENU → PERF** post-secuencia (hipótesis: mejor que Tierra sin océano/flora).
- **Render scale FASE A** (`render/render_scale.rs`): implementado; **default 1.0**; ruta **cerrada en Tierra**. Reabrir solo en Luna si baseline ≥25 FPS (0.85 por `WorldContext`, no global).
- Cel 3 bandas + 1 directional light (sin sombras en Android).
- Outline inverted-hull (no postprocess edge).
- Sim `@ 20 Hz` FixedUpdate — interpolación visual en `Update` (no subir tick para suavizar).
- `AssetFactory`: lazy cache — 1 mesh/material por `kind`; flags `instanced`/`billboard` en catálogo.

### FASE 0–D (jun 2026, antes de animación/mobs)

| Fase | Qué | Archivos |
|------|-----|----------|
| **A** | Océano = anillo horizon + `StandardMaterial` unlit (no plano subdiv pesado) | `render/ocean_horizon.rs` |
| **B** | Corazones/muslitos → quads procedurales (12 total vs cientos de nodes) | `hud/icons.rs` |
| **C** | Batch obstáculos + interactables por chunk; markers invisibles hold-interact | `render/mesh_batch.rs`, `chunk_manager.rs` |
| **D** | `verify-android-sustained.sh` sin `INJECT_EVENTS` (MIUI-safe) | `scripts/verify-android-sustained.sh` |

- **Instancing decor:** `InstancedDecorPlugin` fusiona earth patches (4→1 draw call).
- **Frame pacing:** Android `PresentMode::Fifo`; desktop `AutoVsync`.
- **Lifecycle Android:** `MainActivity.onPause/onResume` → `nativeSetPaused` → `Time<Virtual>` pausado + log `ha-life paused=`.
- **Instrumentación:** overlay perf — **Android: tap MENU (arriba centro) → PERF**; desktop/Linux: **F3**; `ha-perf` logcat cada 5s (sin overlay); `ha-crash` JSON en panic. Scripts: `smoke-android-perf.sh`, `device-ready.sh`, `install-android-ready.sh`.
- **Resolución interna diferenciada:** pendiente — siguiente palanca si FPS sigue bajo.
- `Msaa::Off` en Android.
- HUD compacto: iconos procedurales; paneles grandes **spawn/despawn** (BAG) o bajo `MENU` — **no** acumular entidades ocultas.
- Overworld decor por chunks: ocultar decor fuera de vecinos inmediatos.

---

## §16 Troubleshooting

| Síntoma | Causa | Fix |
|---------|-------|-----|
| App se cierra al abrir | `shaders/toon.wgsl` no encontrado en APK | Shader embebido en `materials.rs` (ya aplicado) |
| Pantalla negra + solo joystick | `Camera2d` borraba frame 3D | `ClearColorConfig::None` en HUD camera |
| Knob descentrado | Coordenadas pantalla en hijo UI | `knob_offset_in_base()` relativo al padre |
| Touch/UI desalineado | Resolución forzada 1280×720 | Usar resolución nativa Android |
| `INSTALL_FAILED_USER_RESTRICTED` | MIUI bloquea adb install | **`adb push` + `pm install -r -t`** (`install-android-ready.sh`); o Instalar vía USB + popup |
| `cargo build` bus error desktop | Disco lleno / linker | `rm -rf native/target/debug`; liberar espacio |
| `UnsatisfiedLinkError` libc++ | Falta en APK | `android-ndk-libs.mjs` copia `libc++_shared.so` |
| `Path not found: shaders/toon.wgsl` | Asset I/O Android | `load_internal_asset!` — no depender de assets APK |
| Landing no descarga | fetch blob viejo | Enlace directo en `downloadArtifact.ts` |
| Botón APK gris | Sin archivo en downloads | `npm run ship:android` |
| Botón Linux gris | Sin AppImage en downloads | `make -C native release && native/scripts/build-appimage.sh` + `publish-update.mjs --platform desktop` |
| `npm run play` falla NDK | Sin ANDROID_HOME | `scripts/setup-android-native.mjs` |
| APK no conecta online | Sin Colyseus en nativo | Pendiente transport en ha-game |
| Drag derecho no gira player/cámara | look intent no conectado a cámara/aim | `aim_x/z` + yaw/pitch unificados desde drag (ya aplicado) |
| Cámara gira raro al moverse | Movimiento de teclado contaminaba aim/cámara | Separar `key_move_x/z` de `raw.move_x/z`; cámara solo por aim/look intencional |
| Bevy panic `B0001` en escena | Queries mutables conflictivas con `Transform` | Usar `ParamSet`/queries disjuntas en cave/player/torch overrides |
| Bevy panic `B0001` en bag panel | `bag_panel_system`: queries `Node` solapadas (panel + secciones) | `Without<BagPanelRoot>` en secciones; `ParamSet` para botones |
| UI nodes ~115 con BAG cerrado | Craft panel permanente `Display::None` (~38 entidades ECS) | **spawn/despawn** en `bag_hotbar_system` + `bag_panel_system` → ui~105 |
| Render scale 0.75 empeora FPS | Blit UI pass + overhead nodos UI simultáneo | Default **1.0**; no reintentar hasta ui~77 |
| Inventario se pierde al background | Solo debounce 3s; Android mata proceso | **flush-on-pause** guarda `inventory.json` siempre al pausar |
| Rust `E0119 FromWorld` en `UiLayoutConfig` | `Default` + `FromWorld` explícito chocaban | Insertar `load_ui_layout().unwrap_or_default()` manualmente en plugin |
| Rust `E0596 recipes` | Query no declarada mutable | `mut recipes` en sistema de crafting |
| UI tapa gameplay | Minimap/objetivos/editor visibles permanentemente | Minimap removido; editor bajo `MENU`; crafting bajo hotbar `BAG` |
| Bevy panic `B0001` tras estados de UI | `update_crafting_hud` tenía varias queries mutando `BackgroundColor` | Usar `ParamSet` para hotbar, panel raíz y recipes |
| Sprint parpadea en borde | Umbral único al 82% | Histéresis: entra **92%**, sale 75%; caminar cap 78% |
| JUMP bloqueado con joystick activo | Bevy `Button` = 1 dedo global | Polling multi-touch por botón en `actions.rs` (`ActionTouchState`) |
| Joystick camina muy rápido | move_speed alto + sprint fácil | scout **4.0** u/s; sprint 92%; walk cap 78% |
| Tap rápido se pierde | Frame/cooldown no capturaba el pulso | Buffer USE 120ms; buffer JUMP **160ms** |
| Draw calls altos en decor | Cada earth patch = 1 mesh | `InstancedDecorPlugin` merge en startup |
| Batería en background | Loop sin pausar al HOME | JNI `nativeSetPaused` + `Time<Virtual>::pause` |
| FPS sin techo / jitter | `AutoNoVsync` | `PresentMode::Fifo` en Android |
| Optimizar a ciegas | Sin métricas runtime | **MENU → PERF** (móvil) o F3 (desktop) + `ha-perf` logcat + **`ha-span`** µs/sistema |
| ATK/DASH duplicaban acciones | Botones + look zone | Eliminados; 3 gestos en mitad derecha + USE |
| UI nodes ~250+ en HUD | Corazones/muslitos como muchos nodos Bevy | `IconFactory` + quads procedurales → ui~77 |
| Prompt/hold abajo tapa hotbar | Layout centrado inferior | Prompt + hold **debajo corazones** top-left |
| Stock W/S/T duplicado bajo corazones | HUD sin panel máquina | **CONTROL PANEL** al acercarse a extractor (~4 m) |
| Océano plano caro | Subdiv + material pesado | Anillo horizon unlit (FASE A) |
| Props 83 draw calls | 1 mesh por prop | Batch por chunk+kind (FASE C) |
| draw3d ~187 tras launch pad | `sync_biome_visibility` forzaba **todos** ChunkDecor Visible | Quitar ChunkDecor de sync; culling solo en `chunk_manager` |
| Pad +2–5 draws extra | platform + column meshes separados | `build_launch_pad_mesh()` merge; markers sin mesh |
| Bevy panic `B0001` sync_biome | Queries `Visibility` conflictivas | `ParamSet` en terrain/moon/ocean (sin ChunkDecor) |
| Controles muertos tras abrir MENU/PERF | `menu_open` bloquea stick/look/USE/JUMP mientras el panel está abierto | PERF cierra menú al togglear; si quedó abierto: tap **CLOSE** en panel MENU |
| Lag con overlay PERF visible | Scan de todas las entidades **cada frame** | Throttle 0.5s en overlay (v1.0.52+); apagar PERF si no medís |
| Lag durante secuencia lanzamiento | Partículas spawn **cada frame** (~15s) | Cooldown 0.18s entre bursts (v1.0.52+) |
| **APK instalado pero no veo cambios** | Ver tabla abajo — **no es Godot**; pipeline Bevy+Gradle | |

### APK instalado pero no muestra cambios (Starship — NO Godot)

Este proyecto **no usa Godot** ni GDExtension. Ignorar `.godot/exported/`, “Deploy with changes”, export desde editor Godot.

| Punto ciego | Qué pasa acá | Fix |
|-------------|--------------|-----|
| **`.so` Rust viejo** | `ship-native-android.mjs` puede **saltar** `cargo ndk` si `jniLibsUpToDate` cree que no hubo cambios | `npm run ship:android -- --rebuild-rust` — compila `arm64-v8a` + `armeabi-v7a` → `android/app/src/main/jniLibs/` |
| **Compilaste para PC, no Android** | `cargo build` desktop **no** actualiza el APK | Siempre `cargo ndk … --features android` vía `npm run ship:android` |
| **Cambios solo en `packages/shared` (TS)** | El APK **no embebe** TS; el cliente lee **`ha-shared`** (Rust espejo manual) | Editar `native/crates/ha-shared/` en paralelo + rebuild Rust; `npm test -w @hunger-arena/shared` |
| **Assets/shaders (`crates/ha-game/assets/`)** | Gradle copia esa carpeta al APK | Tras cambiar WGSL/GLB: `ship:android` (gradlew re-empaqueta) |
| **Gradle empaquetó APK stale** | `assembleDebug` a veces “UP-TO-DATE” con jniLibs viejos | `--rebuild-rust` + borrar `native/android/app/build/` si persiste |
| **MIUI / install encima** | `adb install -r` deja caché rara en casos extremos | `adb uninstall com.hungerarena.app` → `./scripts/device-ready.sh` |
| **Versión igual / OTA** | `publish-update` exige subir `versionCode` | `npm run ship:android` (bump auto) o `bump-app-version.mjs android` |
| **Instalaste APK viejo** | Copiaste APK de otra ruta o sin publicar | Verificar `native/version.json` en celu (MENU) + log `ha-perf` / overlay **MENU→PERF** |

**Checklist rápido cuando “no se ve”:**

```bash
npm run ship:android -- --rebuild-rust   # fuerza .so + gradle + publish
./scripts/device-ready.sh                # install + verify + ha-perf
# nuclear:
adb uninstall com.hungerarena.app && ./scripts/device-ready.sh
```

Confirmar en logcat: versión en install + líneas `ha-perf` (incluye `span[cam=… hud=…]us`) + `ha-span` cada ~3s.


## §17 Glosario

| Término | Significado |
|---------|-------------|
| DROP-REACT | Migración Jun 2026: cliente Bevy reemplaza Expo/R3F |
| ha-shared | Puerto Rust de constantes/palette/movement/assets |
| TouchLayout | Resource con anclas sticks + insets + tamaño real ventana |
| Input-only | Server solo acepta intenciones |
| AssetFactory | Caché lazy de meshes procedurales por `kind` |
| play | Un comando: ship + servicios + URL para el celular |
| Tierra | Isla actual — Acto I del juego espacial |
| Nave prefab | Módulos crafteados ensamblados en pad (no cohete voxel) |
| Misión espacial | Instancia de vuelo arcade o mapa off-world |
| Cuerpo celeste | Planeta/luna con `gravity_scale`, bioma, recursos |
| Colono | NPC sim simplificado con necesidades O₂/comida/refugio |

---

## §18 Historial de decisiones

1. Monorepo sim compartida — offline/LAN/online unificados en server.
2. Landing separada — descarga APK, no motor de juego.
3. **Jun 2026:** Cliente nativo Rust/Bevy; eliminados Expo, Tauri, client-core, client-gfx.
4. Descarga landing: enlace directo (no SHA256/fetch en browser).
5. `npm run play` automatiza todo para el jugador.
6. **Jun 2026:** Gradle+cargo-ndk en lugar de cargo-apk; libc++ en jniLibs.
7. **Jun 2026:** Catálogo 106 assets procedurales para crafting/construcción.
8. **Jun 2026:** Stick izq + drag derecho + botones acción + TouchLayout + JNI insets.
9. **Jun 2026:** Shader toon embebido; fix cámara HUD overlay.
10. **Jun 2026:** OASIS base: overworld pacífico, portales/misiones, wallet y daño desactivado en overworld.
11. **Jun 2026:** Juiciness móvil: camera shake, squash/stretch, interpolación de atmósfera y decor por chunks.
12. **Jun 2026:** UI gameplay limpia estilo Minecraft: corazones top-left, muslitos top-right, hotbar `BAG`, `MENU` top-center, sin minimap fijo.
13. **Jun 2026:** APK debug instalado y smoke test Android estable (`verify-android-startup.sh`, sin crash signatures).
14. **Jun 2026:** `ui_theme.rs` centraliza colores/bordes/estados de HUD/editor/actions; botones rectangulares, hotbar centrada por safe area.
15. **Jun 2026:** Input pulido: sprint con histéresis 82/75 y buffer de taps 120ms para `USE`.
16. **Jun 2026:** Zona look 3 gestos (drag cámara, hold interact, tap habilidad); matriz en `interactions.ts`/`ha-shared`; sin ATK/DASH.
17. **Jun 2026:** Perf móvil: overlay **MENU→PERF** (+ F3 desktop), instancing earth patches, Fifo vsync, pause JNI.
18. **Jun 2026:** Cámara **primera persona** Minecraft; mesh jugador oculto; crosshair solo centro; targeting por ray.
19. **Jun 2026:** **Isla enorme** `WORLD_HALF=96`, océano visual `OCEAN_HALF=384`, playa+acantilados, 28/55 props; no mapa infinito (límite mar estilo GTA V).
20. **Jun 2026:** **Parkour Rust** — climb/jump/mantle en `parkour.rs`; colisión con `generate_world`; joystick más grande + smooth.
21. **Jun 2026:** **Perf FASE A–D** — océano anillo, HUD icons procedurales, batch props/chunk, ha-perf desglosado; Redmi ~14–18 FPS post-cambios.
22. **Jun 2026:** **Máquinas slice** — `wood_extractor` end-to-end (shared sim + ha-shared + ha-game); colocación martillo + hold 2s.
23. **Jun 2026:** **Linux AppImage** publicado en landing + manifest `desktop` v1.0.26; `native/scripts/build-appimage.sh`.
24. **Jun 2026:** **MIUI install** — fallback `adb push` + `pm install -r -t` en `install-android-ready.sh`.
25. **Jun 2026:** **Rebrand dirección** — *Agencia Espacial Survival*: exploración espacio + interiores planetarios, naves prefab arcade (no KSP), bases off-world, colonos; doc `SPACE_SURVIVAL_VISION.md`.
26. **Jun 2026:** **IconFactory FASE 1** — catálogo `content/ui/icons.ts` + espejo Rust; HUD sin patrones hardcoded.
27. **Jun 2026:** **Debug observability** — `ha-crash`, catalog-parity snapshot, golden-movement, Zod detail server, perf-history.jsonl.
28. **Jun 2026:** **Controles v1.0.37** — botones USE/JUMP separados; salto al sprintar; JUMP→CLIMB/MANTLE contextual; velocidad scout 4.0; sprint 92% / walk 78%.
29. **Jun 2026:** **HUD v1.0.37** — prompt+hold bajo corazones; CONTROL PANEL máquinas; UI Borderlands-style (`ui_theme.rs`); editor UI slot JUMP.
30. **Jun 2026:** **`device-ready.sh`** — espera USB, install, verify startup, sample ha-perf.
31. **Jun 2026:** **Inventario persistente v1.0.42+** — `PlayerInventory`, `inventory.json`, craft real en BAG, vitest + cargo test paridad.
32. **Jun 2026:** **BAG UX v1.0.46** — tabs INVENTORY/CRAFT; paneles centrados + X; energía 12 segmentos + `drumstick_half`; stamina drain 7/s regen 18/s.
33. **Jun 2026:** **Perf diagnóstico** — panel craft oculto = +38 ui nodes; render scale no ayuda con ui=105; BAG spawn/despawn → ui 115→105.
34. **Jun 2026:** **flush-on-pause v1.0.46** — inventario siempre guardado al pausar app (JNI `nativeSetPaused`).
35. **Jun 2026:** **HUD ui52 v1.0.49** — refactor nodos UI 105→52; render scale descartado como palanca (CPU-bound).
36. **Jun 2026:** **Launch pad + Luna slice v1.0.49–50** — pad zona (28,-28), 3 módulos, secuencia 15s, `RegionId::Luna`, gravedad 0.17, starfield.
38. **Jun 2026:** **Perf overlay móvil** — botón MENU → PERF (F3 solo desktop); docs corregidas.
39. **Jun 2026:** **SFS FlightState v1.0.92** — ascenso 15s altitude-driven, guards B1–B10 en §2, ha-span extendido (flight/terrain/total), greedy mesh terrain, ray-sphere Luna en Space, ocean B3 once-spawn.

---

## §19 Checklist antes de merge

- [ ] `npm test` (shared) pasa
- [ ] `npm run test:native` pasa
- [ ] `cargo check -p ha-game --features android` pasa
- [ ] `npm run validate:arch` pasa
- [ ] `./scripts/device-ready.sh` o `install-android-ready.sh` si se tocó Android/UI/input
- [ ] `./scripts/verify-android-sustained.sh` (opcional) para cambios perf/lifecycle
- [ ] Sin sim duplicada fuera de `shared`
- [ ] Sin posiciones client→server
- [ ] No tocar landing download sin razón fuerte
- [ ] Zod en nuevos mensajes WS
- [ ] Catálogo TS/Rust mismo conteo de assets (test catalog.test.ts)
- [ ] HUD gameplay sin minimap fijo ni paneles permanentes invasivos

---

## §20 Prompt rápido para Claude (copiar/pegar)

```text
Proyecto: Agencia Espacial Survival — survival FP + exploración espacial + colonización.
Stack: Colyseus + packages/shared (sim TS) + native Bevy (ha-game) + landing descargas.
Motor SOLO en native/ — NO React/R3F.
Visión: Tierra (isla) → naves PREFAB (no KSP) → bases en planetas → interiores → colonos. Física ARCADE, no orbital real.
Mundo actual: Tierra Acto I — isla 192×192, océano anillo, máquinas, **launch pad → Luna slice**.
Controles: stick 38/62 FP; botones USE + JUMP (CLIMB/MANTLE contextual); vuelo futuro = 2 stick + botones contextuales.
Perf: 30+ FPS bloqueante antes de más contenido lunar; Redmi ~17–19 FPS Tierra (v1.0.50, ui~52, draw3d~64). Baseline Luna pendiente.
Docs: labiblia.md + native/docs/SPACE_SURVIVAL_VISION.md
Versión: 1.0.50. Lee labiblia antes de codear.
```

---

## §21 Visión a largo plazo — resumen

**Dirección confirmada (jun 2026):** survival centrado en **explorar espacio e interiores planetarios**, construir **bases en otros mundos** y **mandar colonos** desde Tierra.

Detalle completo → **`native/docs/SPACE_SURVIVAL_VISION.md`**.

| Fase | Contenido |
|------|-----------|
| **Ahora** | Tierra — máquinas, craft, launch pad, **slice Luna** (transición + superficie mínima); perf ~17–19 FPS |
| **Siguiente** | Confirmar **baseline Luna** (MENU → PERF); más contenido lunar solo si perf OK |
| **Medio** | Nave prefab 3 módulos + misión suborbital scripted; base campamento Luna |
| **Tarde** | Interiores, más planetas, colonos abstractos, monitoreo remoto |

**Reglas de diseño:** naves **prefabricadas** (slots en pad, no editor KSP); vuelo **arcade** (barras combustible/O₂, asistencia aterrizaje); interiores = **instancias** FP con recursos raros; colonos = necesidades simples + boost a máquinas.

**Precondición técnica:** **30+ FPS** en Redmi + inventario/craft persistente en Tierra antes de naves o colonos.

---

## §23 Visión espacial — Agencia Espacial Survival

### Loop

```
Tierra → craft módulos → ensamblar nave prefab → misión (espacio arcade / FP superficie)
  → base off-world → máquinas → antena → monitoreo desde Tierra → colonos
```

### Dos modos de exploración

1. **Espacio** — misiones instanciadas: ascenso → crucero (waypoints/autopilot) → aterrizaje en zona marcada. Sin órbitas reales.
2. **Superficie + interior** — mapas medianos por planeta; cuevas/lava tubes como instancias separadas; mismo parkour FP + trajes/gravedad por cuerpo.

### Naves prefabricadas (no Kerbal)

Módulos crafteables: **casco, tanque, motor, O₂, bodega, aterrizador, scanner**. Ensamblaje en **pad de lanzamiento** (3–6 slots). Blueprints tarde: Scout, Carguero, Colonizador.

Stats abstractos (`thrust`, `fuel_cap`, `cargo_slots`) — deterministas en `shared`.

### Bases y colonos

- Bases: campamento → puesto → colonia (reuso placement + `machines/`).
- **Antena:** UI monitoreo remoto desde Tierra.
- **Colonos (tarde):** habitat + O₂ + comida → solicitar desde Tierra → viajan en nave Colonizador → asignación a máquinas (output pasivo).

### Física — solo arcade

| Sí | No |
|----|-----|
| `gravity_scale` por planeta | Delta-v, Hohmann, nodos orbitales |
| Thrust + cap velocidad | Masa variable, aerodinámica |
| Combustible/O₂ como barras | Sim rigid-body interplanetaria |
| Aterrizaje asistido 3 niveles | Game over duro en crash |

### Actos de contenido

I Tierra → II Luna → III varios cuerpos → IV interiores → V colonización → VI estación orbital (endgame).

### Anti-goals

Editor orbital, cohete voxel, terraformación sim, PvP arena, mobs animados antes de perf, multijugador colonias antes de offline sólido.

### Dónde irá en código (futuro)

```
packages/shared/src/content/ships/
packages/shared/src/content/bodies/
packages/shared/src/sim/flight.ts
packages/shared/src/sim/colony.ts
```

Reutiliza: isla=Tierra, `WorldContext`, `machines/`, `AssetFactory`, parkour FP, legado combate dormido.

---

## §22 Auditoría legado combate/OASIS (mapeado, sin remover)

Archivos con lógica **no aplicable al concepto nuevo** pero **conservados intencionalmente**:

### `packages/shared/src/sim/`

| Archivo | Contenido legado |
|---------|------------------|
| `GameSimulation.ts` | Loop PvP/survival: cooldowns, projectiles, dash, oleadas, daño entre entidades |
| `GameSimulation.test.ts` | Tests del sim de combate/arena |
| `combat.test.ts` | Daño, AOE, knockback, hitboxes |
| `survival.test.ts` | Director de oleadas survival |
| `content/classes.ts` | scout / warrior / mage stats |
| `content/abilities.ts` | dagger / cleave / fireball |
| `abilities/types.ts` | Tipos de habilidades |
| `content/npcs.ts` | Arquetipos NPC hostiles |
| `ai/brain.ts`, `ai/types.ts`, `ai.test.ts` | AI persecución/ataque NPCs |
| `paths.ts`, `paths.test.ts` | Pathfinding grid para NPCs |
| `worldProps.ts` | Props de misión combate |
| `rng.ts` | RNG usado por AI/combate (shared con otros sistemas) |

### `packages/shared/src/content/`

| Archivo | Contenido legado |
|---------|------------------|
| `interactions.ts` | Reglas **Attack** con espada (también gather/mine — **activo**) |
| `interactions.test.ts` | Tests incl. attack |
| `assets/catalog.ts` | Espadas, arcos, armas PvP (reuso futuro props) |
| `assets/types.ts` | Tags combat en assets |

### `packages/shared/src/schema/` + red

| Archivo | Contenido legado |
|---------|------------------|
| `schema/EntityState.ts` | HP, team, combat state por entidad |
| `schema/GameRoomState.ts` | Estado sala PvP/survival |
| `schema/WorldSchema.ts`, `WorldPropState.ts` | Campos mundo combate |
| `messages.ts` | Ability bits, craft intent WS |
| `profile.ts` | Stats perfil PvP |

### Cliente / espejo Rust

| Archivo | Notas |
|---------|-------|
| `ha-shared/src/classes.rs` | Clases PvP |
| `look_interact.rs` | Tap ability cooldown (legacy); **placing_machine** hold activo |
| `scene.rs` | OASIS overworld/cave, atmósfera mystic |

### Activo en concepto nuevo (NO es legado)

| Archivo | Rol |
|---------|-----|
| `content/machines/` | Automatización |
| `sim/machines.ts` | Tick producción |
| `sim/movement.ts`, `parkour.ts` | Locomoción FP |
| `sim/world.ts` | Layout isla Tierra |
| `content/interactions.ts` | gather/mine/hold |
| `content/items/` | Inventario/craft |

**Se mantiene intacto:** motor Bevy, perf, AssetFactory, hold-interact completo.

---

*Fin de La Biblia. Visión espacial: `native/docs/SPACE_SURVIVAL_VISION.md`. Operativa: `README.md`, `native/README.md`, `ARCHITECTURE.md`.*
