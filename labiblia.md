# La Biblia — Starship (Agencia Espacial Survival)

> Contexto maestro para agentes IA. **Magra y VIGENTE** (reescrita 6 jul 2026, reemplaza la histórica de 1188 líneas). Si algo acá contradice al código, gana el código — pero avisá para actualizar. El historial viejo se descartó a propósito; lo único "histórico" que queda es la sección §10.

---

## §0 Identidad

| Campo | Valor |
|-------|-------|
| **Producto** | **Starship** (Agencia Espacial Survival). Repo interno `hunger-arena`; package Android `com.hungerarena.app`. |
| **Género** | Survival + construir base/laboratorio + automatizar recolección → naves prefab → off-world → colonos. |
| **Referencias** | **Top-Down 3/4 (Stardew/LimeZu Modern)** en superficie + **Space Flight Simulator (2D)** para vuelo/planetas; **cero** física orbital real (siempre arcade). |
| **Home base** | **Tierra** (seed **42**). |
| **Plataformas** | Android APK (arm64) + Linux AppImage (Bevy) · landing web · servidor Colyseus. |
| **Stack** | Colyseus · **Rust/Bevy 0.15 en 2D** (Sprites + Mesh2d, sin pipeline 3D activo) · Postgres+Drizzle · npm + Cargo workspaces. |
| **Sim** | Determinista TS (`packages/shared`); servidor **input-only**; `ha-shared` porta constantes a Rust. |
| **Estética** | **2D top-down 3/4 con assets LimeZu Modern** (Exteriors/Interiors/UI). Personaje direccional, árboles/rocas/tiles LimeZu; fallback procedural donde falten. UI in-game: Borderlands. Landing: synthwave. |
| **Versión** | ver `native/version.json` (no hardcodear). |

### Regla de migración (INVIOLABLE — actualizada 12 jul 2026, rewrite-2d)
**El motor vigente es el cliente 2D** (rama `rewrite-2d`): el render 3D esférico se ELIMINÓ a propósito para ganar 60 fps (Terraria + SFS 2D). NO reintroducir: cámara FP, esferas caminables por-píxel, AssetFactory de meshes 3D, toon/outline, domo de cielo, floating-origin 3D. Intacto y probado: `server/` (Colyseus, auth, `/updates`), core de `packages/shared/src/sim` (tick 20 Hz, determinismo, `interactions.ts`), gesto de input (joystick + hold-interact), HUD/inventario/craft, máquinas/colonos/ship-builder (lógica), persistencias B10.

### Legado dormido (NO borrar, NO referenciar en contenido nuevo)
Clases scout/warrior/mage, habilidades, oleadas survival, AI hostiles — permanecen para fauna/peligro futuro (ver §10).

---

## §1 ⭐ MODELO VIGENTE — TOP-DOWN 3/4 (13 jul 2026)

> **CAMBIO MAYOR (13 jul):** el juego migró de vista lateral (side-scroller/
> Terraria) a **top-down 3/4 (Stardew/LimeZu)**. Detalle completo, fases y
> reglas de Z-Levels en **`MIGRATION_PLAN.md`** (raíz). Núcleo:
> `ha-shared/topdown.rs`; render `ha-game/world2d.rs` (grilla 2D, y-sorting,
> `render_xy(x,z)=(x,-z)`). Suelo = plano XZ (unifica con la vista orbital SFS,
> que ya era top-down). El texto histórico de §1 abajo (tira/arco `s`) quedó
> **SUPERADO** — se conserva por contexto pero el modelo vigente es top-down:
> - Superficie = grilla 2D de tiles `(tx,tz)` por cuerpo; movimiento 8-dir sin
>   gravedad; cavar/colocar por celda. Cuevas = **Z-Levels** discretos (z<0) por
>   portales (entrada de mina/escaleras); oscuridad + vetas densas abajo.
> - Espacio = mismo plano XZ visto desde arriba (planetas = círculos reales,
>   `orbital2d`). Despegar/aterrizar = **zoom** de cámara (no cambio de coords).
> - Assets LimeZu vía capa drop-in (`art2d.rs`, `assets/art/`, `sync:art`).

### (Histórico — modelo de tira, SUPERADO por top-down)

**Esto es el corazón del diseño. Léelo antes de tocar mundo/vuelo/cielo/planetas.**

- **Un solo plano/universo.** El sistema solar vive en el plano XZ de siempre (`solar_ephemeris.rs`, Tierra ORIGEN FIJO, Luna orbita, todos coplanares) — y ahora ese plano ES la pantalla: render orbital `(x, -z)`. Los cuerpos son **círculos reales 2D** (`orbital2d.rs`) con posiciones vivas por efeméride, no fondos por-escena.
- **Superficie = tira de tiles cavable (Terraria).** Cada cuerpo caminable es su rebanada z=0: coordenada de arco `s` (envuelve la circunferencia `2πR_walk` — caminás y das la vuelta al planeta) × altura `h`. Terreno determinista por seed del cuerpo (`ha-shared/flat2d.rs`: heightline + estratos + vetas), cavable hasta `DIG_DEPTH_TILES`; ediciones sparse persistidas (`tiles.json`, B10). Render: chunks `Mesh2d` de 32 tiles con vertex colors (`world2d.rs`), decor/vetas = Sprites. **Cavar ES la cueva**: los gem_red lunares están embebidos en profundidad; la instancia Cave(2) quedó deprecada.
- **Continuidad exacta despegue↔aterrizaje (sin corte).** `flat_surface_to_orbital` / `orbital_to_flat_surface` (con velocidad) son inversas exactas: `θ = s/R`, `r = R+h`. El vuelo (`rocket_flight.rs`) simula SIEMPRE en el plano mundial; por debajo de `escape_alt` se renderiza en el marco superficie (subís desde el pad), por encima pasa a la vista orbital con zoom continuo de cámara (`camera.rs`). B8 intacto: una transición es un cambio de marco, JAMÁS una cutscene.
- **Cielo por región.** Tierra: ciclo día/noche 720 s con ClearColor + overlay nocturno (`sky2d.rs`); al subir (creativo o cohete) el cielo se funde a vacío (§11). Cuerpos airless y espacio: negro + **estrellas que siguen a la cámara** (nunca geometría finita cruzable — la regla del viejo `LunaSkyShell` sobrevive en 2D).
- **Mapa del sistema** (`game_menu.rs`, botón MENU): cenital del plano, igual que siempre — en 2D es literalmente la misma proyección que la vista orbital.
- **Cámara-módulo (futuro).** El monitor de vessels (`telemetry.rs`) quedó; la vista por cámara remota se recablea en 2D más adelante.

### 🚫 ANTI-REGRESIÓN — NO volver a esto
- NO reintroducir el **render 3D esférico** (cámara FP, esferas caminables, toon/outline, domo Ghibli, floating-origin, AssetFactory de meshes) — se borró en `rewrite-2d` por los 80-180 ms de frame GPU en el Redmi.
- NO el **walk-hub orbital caminable** (el espacio es vuelo continuo).
- NO cielos **por-escena** ni cuerpos lejanos forzados como skybox: culling por región + círculos reales en la vista orbital.
- NO "cielos" como geometría finita cruzable: estrellas/fondo siguen a la cámara SIEMPRE.
- NO bloquear input/cámara ni patadas de velocidad en transiciones (B8).

---

## §2 Arquitectura (capas)

```
packages/shared/   VERDAD DEL JUEGO (server) — GameSimulation, catálogos, zod (TS puro, isla 96×96)
server/            SurvivalRoom Colyseus + auth + Drizzle/Postgres + /updates
client/            LANDING ONLY — descarga APK + AppImage (sin motor, sin R3F)
native/
  crates/ha-shared/  Puerto Rust: constants, cuerpos celestes, superficies esféricas, vuelo, efemérides
  crates/ha-game/    Cliente Bevy ECS — render, input touch, sim local 20 Hz, regiones, mapa
```
**Dependencias:** `client→shared` (solo tipos manifest) · `server→shared` · `ha-shared` = espejo numérico de shared · `shared→NADA`.
**OJO shared vs native:** el sim de `packages/shared` sigue siendo la **isla plana 96×96** (fuente del servidor Colyseus). El cliente NATIVO usa el **modelo esférico/continuidad** (§1). No confundir: reglas de juego se espejan en `ha-shared`, pero la geometría de mundo del cliente nativo es su propia capa.

---

## §3 Reglas inviolables + Guards

**Reglas (sim/red):**
1. Sim solo en `packages/shared`; Rust = port numérico en `ha-shared` (debe coincidir).
2. Server input-only — clientes envían `InputState`, nunca posiciones/HP.
3. Determinismo TS (RNG con seed; vitest en `shared`).
4. Zod en fronteras WS/HTTP.
5. Sin IPs hardcodeadas — `VITE_SERVER_HOST`, `HA_SERVER_HOST`, `UPDATE_BASE_URL`.
6. Landing ≠ juego — `client/` solo descarga.
7. No reintroducir React/R3F en el cliente de juego (motor = `native/`).
8. Assets procedurales: def en `packages/shared/src/content/`; espejo Rust en `ha-shared/src/assets/`; meshes en `ha-game/src/assets/`.
9. Cliente 2D sin shaders custom ni assets en disco: tiles = vertex colors, ítems/mundo = íconos procedurales en runtime (`item_icons.rs`/`icon_factory.rs`). No agregar dependencias a `assets/` del APK.
10. Cambios de reglas → Rust **y** TS; regenerar paridad `UPDATE_CATALOG_PARITY=1 npm test`.

**Guards B1–B10 (cliente nativo — obligatorios en sistemas nuevos):**
- **B1** Queries mutables conflictivas (Bevy B0001): `Transform`/`Node`/`BackgroundColor` en el mismo sistema → `ParamSet` o queries disjuntas.
- **B2** Geometría de mundo visible → `AssetFactory.require` + material apropiado (Android: unlit GLES-safe).
- **B3** Singletons (océano, domo de cielo, mapa) — verificar query antes de spawn; no duplicar.
- **B4** Sistemas por contexto: primera línea `if ctx != Expected { return; }`. Ej.: `surface_patch` (solo el cuerpo activo), impostores celestes (solo vacío/remoto), partículas fireflies (solo Tierra).
- **B5** Paneles UI (BAG, craft, CONTROL PANEL, MAPA) — spawn/despawn o toggle de `Visibility`, nunca `Display::None` acumulativo.
- **B6** Al salir de una región, despawn de decor (`celestial_cleanup` con histéresis 2 s). Entidades de un cuerpo no viven en otro.
- **B7** Floating origin (`floating_origin.rs`): render = mundo − `shift`, relativo al **observador** (`ViewTarget`); shift se activa >~22 km del ancla. El mundo caminable también pasa por floating-origin vía `surface_frame.rs`.
- **B8** Cambio de `WorldContext`/`RegionId` solo en las transiciones reales de `rocket_flight.rs` (escape gameplay→orbital al superar `escape_alt`/vacío; touchdown al aterrizar). Para la Tierra el remap es identidad. **Una transición es un cambio de marco de referencia, NUNCA una cutscene: jamás bloquea input ni cámara, ni aplica patadas de velocidad.** El modo creativo **no** fuerza vuelo: se eliminó `creative_fly_to_space` (encendía `RocketFlight` a 95 m con `vel=up*95` + `blocks_walking` = el "corte feo" que trababa cámara/menú). En creativo se vuela con la locomoción a pie libre (`step_surface_creative_fly`: JUMP sube, sprint baja, sin gravedad) — ver §11.
- **B9** Sin alloc en hot path (`Update`/`FixedUpdate`) sin capacidad pre-reservada.
- **B10** `inventory_persist` / `world_persist` / `nav_data` flush síncrono al pausar (`nativeSetPaused`) y en transiciones de vuelo.

Validar: `npm run validate:arch`.

---

## §4 Sistemas actuales (mapa conciso)

| Tema | Archivos |
|------|----------|
| **Cuerpos celestes** (catálogo, radios, SOI, gravedad) | `ha-shared/celestial_bodies.rs`, `cosmic_scale.rs` |
| **Efemérides** (órbitas vivas, Tierra fija, Luna orbita Tierra) | `ha-shared/solar_ephemeris.rs` (`body_center_at`, `earth_center_at`) |
| **Mundo 2D** (tira de tiles por cuerpo: terreno determinista, cavar/colocar, chunks Mesh2d, decor/vetas, `tiles.json`; **8 variedades de árbol** por celda vía `pick_tree_species` — slots `decor/tree_{roble,pino,abedul,arce,sauce,palmera,arbusto,seco}`, fallback de color por especie, mismo `InteractTarget::Tree`) | `ha-shared/flat2d.rs`, `ha-game/world2d.rs` |
| **Locomoción plana** (plataformero: gravedad por contexto, salto/coyote, wrap circunferencia, vuelo creativo) | `ha-shared/flat2d.rs` (`step_flat_movement`), `ha-game/sim.rs` |
| **Vuelo 2D SFS** (thrust/coast/retro, fuel real, drag, paracaídas, SOI dinámica, aterrizaje, relanzamiento off-world, remap exacto superficie↔órbita; **telemetría H/V** `vertical_speed`/`horizontal_speed` + **radar** de superficie; **autopilot hover** ~500 m grueso) | `ha-game/rocket_flight.rs`, `ha-shared/flight_physics.rs`, `flat2d.rs` |
| **Vista orbital** (cuerpos como círculos reales + halo atmósfera + Sol) | `ha-game/orbital2d.rs` |
| **Regiones + jugador** (RegionId/WorldContext, spawn, teleports MENU) | `ha-game/scene.rs` (`teleport_to_body`) |
| **Interior de nave + cabina + recarga** (cuarto 2D abordable desde el pad; Z-level dedicado 64 sobre roca sólida = cuarto tallado en WoodFloor; abordar/salir = cambio región+z+teleport B8; `CockpitSeat` → `PreFlightUI` DESPEGAR/AUTOPILOTO/LEVANTARSE bloquea al walker; `FuelSource` + `RefuelEvent` en sala de máquinas = combustible gestionado) | `ha-game/ship_interior.rs` (`ShipInteriorPlugin`, `board_ship`/`leave_ship`, `CockpitState`, `RefuelEvent`), `RegionId::ShipInterior`, `TileWorld::force_tile` |
| **Cielo 2D** (día/noche 720 s, overlay nocturno, fade a vacío, estrellas que siguen la cámara) | `ha-game/sky2d.rs` |
| **Cámara 2D** (SIEMPRE centrada en cohete/jugador; zoom **controlado por el usuario** — pellizco móvil / scroll desktop, `CameraZoom`; default cercano al entrar a vuelo, ya NO auto-aleja a 9 km; shake) | `ha-game/camera.rs` |
| **Targeting 2D** (mira libre `AimCursor` movida por el stick derecho/mouse; tiles block_cell/place_cell; resalte de celda `CursorHighlight`) | `ha-game/targeting.rs`, `input/joystick.rs` (`AimCursor`, `AIM_MAX_REACH`), `input/desktop_input.rs` (`desktop_aim_cursor`), `world2d.rs` (`CursorHighlight`), `input/look_interact.rs` (USE→hold) |
| **Mapa del sistema / menú unificado** (MENU fullscreen: minimapa DINÁMICO — terreno top-down en superficie, orbital en espacio — + panel LIMPIO: viaje TIERRA/LUNA/ESPACIO, CREATIVO/PERF, EDITAR UI, AJUSTES; los selectores del editor viven en `EditToolbar`, visible solo en modo edición) | `ha-game/game_menu.rs` (`OrbitalLayer`/`TerrainLayer`, `update_terrain_minimap`, `EditToolbar`), `ui_editor.rs` |
| **Construcción 2D** (piezas en la pestaña CONSTRUIR de la BOLSA — pantalla limpia; elegir pieza entra modo build + cierra bolsa; ghost en la celda apuntada; colocar con USE cobrando recursos; quitar = cavar; chip mínimo "CONSTRUIR (salir)") | catálogo `ha-shared/topdown.rs` (`BUILD_PIECES`, tiles WoodWall/StoneBrick/Door), `ha-game/build.rs` (`BuildPlugin`, `SelectedBuildPiece`, `apply_build_hold`, ghost, chip), tab en `hud.rs` (`BagTab::Build`, `BagBuildSection`); `targeting.rs` set `place_cell` en build mode |
| **Navegación/sondas** (mapear cuerpos, gate de autopilot) | `ha-game/nav_computer.rs`, `ha-shared/nav_data.rs` |
| **Máquinas + inventario** (sprites por ícono procedural, por-cuerpo, banking off-body) | `ha-game/machines.rs`, `inventory_persist.rs`; sim `packages/shared/src/sim/machines.ts` |
| **Survival stats** (hambre/sed/O₂, comer, respawn amable) | sim `packages/shared/src/sim/survivalStats.ts` → espejo `ha-shared/survival.rs`; cliente `ha-game/survival_stats.rs`, barras `hud/survival_bars.rs` (ver §12) |
| **Ship builder** (piezas crafteables → contadores → stats emergen) | sim `shipMass.ts` (`ShipConfig`, `shipConfigStats`) → espejo `ha-shared/ship_mass.rs`; cliente `ha-game/ship_builder.rs` (tab BAG→SHIP, `ship.json`), extras aplican vía `launch_pad.rs::sync_ship_loadout`; motores extra multiplican thrust en `rocket_flight.rs` |
| **Colonos v1** (Acto V: habitat → enviar colonos, boost taladros) | sim `colony.ts` → espejo `ha-shared/colony.rs`; cliente `ha-game/colony.rs` (`colony.json`, botones COLONO/VÍVERES en el mapa gated por antena, cuestan manzanas); boost ×1.5 a los primeros N taladros lunares en `machines.rs::tick_machines`; estado en línea BASE LUNA |
| **Base lunar** (Acto II: taladro regolito, antena, monitoreo) | máquinas por CUERPO (`ha-game/machines.rs`: gates `machine_allowed_on`, `machines.json`, respawn `sync_machine_visuals` B6); off-body banquean `stored_output` (se cobra al volver); campamento = O₂ tether en `survival_stats.rs`; antena → línea "BASE LUNA" en mapa orbital (`game_menu.rs::update_luna_base_monitor`) |
| **Gamepad** (stick izq mover+sprint; A salto/thrust, RT acción/romper, LT colocar/comer/retro, Start mapa, B cerrar, D-pad bag/hotbar) | `input/gamepad.rs` — alimenta los MISMOS intents que touch/teclado; prioridad touch > pad > teclado |
| **HUD / UI / editor** (twin-stick: izq mover, der mira; USE/JUMP en arco arriba del stick der; editor de UI fusionado en el menú) | `ha-game/hud.rs`, `ui_editor.rs`, `input/actions.rs` (botones USE/JUMP), `input/joystick.rs` (sticks), `ui/icon_factory.rs`, `hud/item_icons.rs`, `ui_theme.rs` |
| **Partículas 2D + squash** (harvest juice) | `ha-game/render/particles.rs` |
| **Assets 2D drop-in** (PNG packs sobre arte procedural; `sync:art` genera el manifest) | `ha-game/art2d.rs`, `assets/art/` (+README), `scripts/sync-art.mjs` |
| **Debug/perf + verificación** (perf HUD, screenshots, autodrive end-to-end) | `ha-game/debug/perf.rs` (MENU→PERF / F3 / F12 screenshot), `debug/autodrive.rs` (`HA_AUTODRIVE=1`), `cpu_span.rs` (ha-span), `crash.rs` |
| **Legado 3D (borrado en rewrite-2d, recuperable en git `main`)** | surface_patch, floating_origin, view_target, flight_celestials, ghibli_atmosphere, materials(toon), earth/moon/planet_world, block/building_world, assets/, space_travel, jetpack_transit, world_travel, sky_clock, solar_lighting, quality, asset_dev |

`WorldContext` (gravedad/locomoción): Overworld / Mission(cueva) / Luna / Planet / Space. `RegionId` (visibilidad/render): Overworld / Cave / Space / Luna / Planet / Transit. **No confundirlos.**

---

## §5 Comandos + build + flujo

```bash
npm install
npm run play              # APK + server + landing — user abre URL en el celu
npm run ship:android      # build + publica APK (default arm64; ANDROID_ABIS="arm64-v8a,armeabi-v7a" para 32-bit)
npm run dev:server        # :2567
npm run test              # vitest shared
npm run test:native       # cargo test ha-shared
npm run validate:arch     # límites de arquitectura
cd native && make run     # cliente desktop (o cargo run -p ha-game --features desktop)
```
- **Flujo jugador:** `npm run play` → URL en el celu → descargar APK → instalar → jugar. OTA: manifest `mandatory` → auto-update al relanzar.
- **Bump versión:** `ship:android` bumpea por defecto (`--no-bump` para no). Bumpear para que el celu detecte el update.
- **Build pesado / OOM:** limitar con `CARGO_BUILD_JOBS=1` si el compilado de ha-game (Bevy) muere por memoria.
- **Env:** `VITE_SERVER_HOST` / `HA_SERVER_HOST` (LAN, nunca IP hardcodeada), `UPDATE_BASE_URL`.

---

## §6 Perf — camino a 60 fps (cliente nativo, Redmi)

**La palanca grande YA se ejecutó (12 jul): el rewrite 2D.** El diagnóstico histórico era frame 100% GPU (80-180 ms con lógica en µs): domo de cielo fullscreen, esferas 3D, toon, ~58 draws 3D. El cliente 2D elimina ese costo por diseño: tiles = ~11 meshes de vertex-colors sin textura, decor = sprites batcheados, cielo = ClearColor + overlay, sin luces ni sombras ni post.

**Dato desktop post-rewrite (NVK, NO representativo):** `draw3d~11`, `ents~223`, `total_update ≈ 30-120 µs`. **Pendiente: medir en el Redmi** con input real (MIUI throttlea sin input; ver memoria "pruebas perf Redmi").

**Si el Redmi aún no da 60 fps, palancas restantes:**
1. **Resolución de superficie Android** (`ANativeWindow_setBuffersGeometry`, commit a9317df en `main`) — el experimento quedó fuera del build 2D (render_scale borrado); reintroducir SOLO si hace falta (con 2D probablemente no).
2. **UI draws** (`ui≈98`): hotbar/corazones/texto — batching de nodos si pesa.
3. **Memoria/heap**: el 2D baja mucho el heap (sin meshes 3D ni texturas de cielo); vigilar igual (kswapd histórico).

**Método:** medir SIEMPRE en el Redmi jugando. Desktop 940M/NVK solo para correctitud. Herramienta: MENU→PERF (móvil) / F3 (desktop); spans en `debug/cpu_span.rs`.

---

## §7 Troubleshooting

- **APK no actualiza / "no muestra cambios":** confirmar versionCode bumpeado + manifest publicado; relanzar la app (OTA `mandatory`). Es Starship (Bevy), no Godot.
- **MIUI `INSTALL_FAILED_USER_RESTRICTED`:** activar "Instalar apps vía USB" en Opciones de desarrollador, o instalar por URL (`http://<host>:5173/downloads/hunger-arena.apk`) tocando "Instalar" en el celu.
- **Disco se llena en cargo:** `rm -rf native/target/debug` (el build Android usa otro target dir).
- **Lock de cargo ("Blocking waiting for file lock"):** matar cargo zombie; borrar `native/target/**/.cargo-lock`.
- **z-fighting de cuerpos lejanos:** far plane de vuelo grande + near razonable (GLES3 sin reversed-Z).
- **"Aparece 3D viejo / no aparecen mis cambios en el APK":** el APK puede salir como **fat APK** con `.so` rancios en `native/android/app/src/main/jniLibs/<abi>/`. `ship:android` solo compila arm64 por defecto, pero **gradle empaqueta lo que haya en `jniLibs/`** — un `libha_game.so` viejo de `armeabi-v7a` (pre-purga 2D) se cuela y un equipo 32-bit corre el motor 3D viejo. Fix: borrar `jniLibs/armeabi-v7a` (o rebuildear ambos ABIs con `ANDROID_ABIS="arm64-v8a,armeabi-v7a" --rebuild-rust`). Verificar ABIs: `unzip -l APK | grep lib/`. Casi todo equipo moderno (Redmi 10+) es arm64.
- **Fuente del juego = ASCII-only:** el `default_font` NO renderiza `· ↕ ↔ ● ○ ☀ ó — ` (salen cajas □). Toda cadena de UII/HUD debe ser ASCII (usar `-` en vez de `·`/`—`, `vert/horiz` en vez de flechas, sin acentos). Ver [[game-font-ascii-only]].

---

## §8 Visión (resumen — detalle en `native/docs/SPACE_SURVIVAL_VISION.md`)

Loop: **construir base en la Tierra → automatizar recolección → naves prefab (vuelo arcade) → bases off-world → colonos.** Dirección: Tierra (actual) → naves → off-world → colonos. Sin física orbital; 30+ FPS antes de más features espaciales. Naves = módulos prefab (no Kerbal). Anti-goals: nada de ensamblaje pieza-por-pieza tipo KSP, nada de física orbital real.

---

## §9 Skills / docs / herramientas para IA

`ps1-retro-graphics` (paleta), `fstyle-brand` (branding), `CLAUDE.md` (resumen agente), `native/README.md` (build), `native/docs/SPACE_SURVIVAL_VISION.md` (visión — el loop sigue válido; el render descrito ahí es el viejo 3D), `context_to_ai.md`. Retirados con el 3D: `ghibli-toon-graphics`, `hybrid-bd-ghibli-graphics`, `procedural-asset-factory` (meshes), `tools/blender`, `tools/asset-viewer`, `tools/sky_art`.

**graphify (grafo de código) — USAR ANTES de cambios grandes.** Grafo en `graphify-out/` (`graph.json`, `GRAPH_REPORT.md`). Comandos: `graphify explain "Nodo"` (nodo + vecinos), `graphify path "A" "B"` (camino entre dos), `graphify update .` (refrescar tras cambios, sin costo API). **Regla:** antes de tocar un sistema con muchas dependencias (render, marco de mundo, cámara, cielo), consultar el grafo para ver qué depende de eso — así se evitan roturas cruzadas (ej: el refactor `surface_frame` rompió la Luna por no ver que el `LunaSkyShell`/cámara asumían el marco estático). El `GRAPH_REPORT.md` lista god-nodes (`CelestialBody`, `RocketFlight`, `make_material`), ciclos de import (hud↔inventory_ui, varios con scene.rs) y comunidades.

---

## §10 Legado dormido (auditado, NO remover, NO referenciar en contenido nuevo)

Sistemas de la versión combate/OASIS que siguen en el repo por reuso futuro (fauna/peligro): clases + habilidades + AI hostiles + oleadas survival (`packages/shared/src/sim/` combat/ai/survival), schema Colyseus. **No** son parte del concepto nuevo pero **no se borran**. Tampoco reintroducir lo ya retirado: `apps/mobile`, `apps/desktop`, `packages/client-core`, `packages/client-gfx`, React Three Fiber, Expo, Tauri; ni el walk-hub orbital / cielos por-escena / split Tierra 480-8000 (ver §1 anti-regresión).

---

## §11 Movimiento: creativo, jetpack, políticas (actualizado 10 jul 2026)

**Creativo = tipo Minecraft (build/craft/máquinas GRATIS + vuelo a pie + noclip):** con
`UiEditorState.creative` ON (botón CREATIVO del menú):
- **Build/craft/máquinas GRATIS:** construir (`build.rs::apply_build_hold`), craftear
  (`hud.rs::bag_panel_system` — cualquier receta, otorga el output sin consumir) y colocar
  máquinas (`machines.rs::try_place_on_hold`) sin costo de recursos.
- **Vuelo libre del PERSONAJE por el espacio (`creative_fly.rs`, módulo separado):** parado en
  una superficie, **JUMP** te eleva a la vista orbital SIENDO el personaje (tu sprite, NO un cohete
  — se probó y descartó re-encender `RocketFlight`, que es god-node degree 61 / el "corte feo" del
  §8). `CreativeFlight { active, pos }`: te movés **libre en 2D** por el plano orbital (velocidad
  escala con el zoom), sin fuel ni gravedad. La cámara te centra (+ pellizco). **Aterrizás** al
  tocar un cuerpo (o USE = aterrizar en el más cercano) reusando `teleport_to_body` → coords de
  superficie siempre correctas, **sin remap inverso riesgoso**. Reemplaza el parche de botones
  TIERRA/LUNA/ESPACIO. Render: `sim.rs::interpolate_player_visual` dibuja al personaje en `pos`
  escalado por el zoom; cámara: `camera.rs::follow_player_camera` apunta a `pos` cuando `fly.active`.
- **Locomoción a pie:** noclip 8-dir (`sim.rs`: sin colisión, atraviesa agua/roca; `sim.body.y=0`).
- `creative_topup_system` sigue dando herramientas.

(El texto de abajo describe el vuelo vertical del viejo side-scroller — superado por top-down.)

**Modo creativo = modo dios (hoy es la ÚNICA forma de volar — el cohete construible aún no existe).**
- Se vuela con la locomoción a pie libre: `sim.rs` en creativo llama `step_surface_creative_fly` (JUMP sube, sprint baja, **sin gravedad**). El controlador de movimiento es el mismo en Tierra/Luna/planeta — el contexto cambia solo para render/culling/efemérides.
- **Es continuo por construcción:** en espacio-tangente `body.y` crece sin límite y se renderiza a lo largo de la normal (`interpolate_player_visual`), así subir recto desde la Tierra es un ascenso mundial correcto a cualquier altura, sin corte ni cambio de cámara.
- **NO** hay ignición forzada de `RocketFlight` (se borró `creative_fly_to_space`). Para saltar a otro cuerpo en creativo, usar los botones **LUNA/ESPACIO/TIERRA** del MENU (teleport deliberado, no una animación involuntaria).
- **Fade atmosférico al subir (2D).** En `sky2d.rs` `sync_sky`: sobre la Tierra la altura del jugador rampa 150→1200 m el ClearColor hacia vacío y enciende las estrellas (que siguen a la cámara). Continuo y reversible al bajar. Banda a tunear en el Redmi.

**Jetpack = herramienta universal.** El boost de jetpack (`sim.rs`, item `jetpack`: JUMP para saltar + mantener para sostener el ascenso) funciona en **cualquier cuerpo caminable** (Tierra/Luna/planetas), no solo en el overworld. Regla general del proyecto: nada de mecánicas encerradas por escena — "todo se usa en todo el juego". (El escape-transit `jetpack_transit.rs` hacia la Tierra sigue acotado a la Luna: es un "volver a casa" de contenido, no el boost de movimiento.)

**Transiciones de contexto (B8):** cambian el marco de referencia, **nunca** bloquean input ni cámara ni aplican patadas. Si algo "salta" al despegar/aterrizar, hay un clamp/teleport/ignición forzada en el camino — se elimina, no se parchea con un fade.

---

## §12 Survival stats — hambre / sed / oxígeno (añadido 11 jul 2026)

**Fuente de verdad:** `packages/shared/src/sim/survivalStats.ts` (constantes en `constants.ts`) → espejo numérico `ha-shared/survival.rs` (tests en ambos lados; mantener sincronizados).

- **Barras** en `SimState.survival` (cliente `ha-game/survival_stats.rs`, tick 20 Hz):
  - **Hambre** decae 0.12/s; **sed** decae a mitad de ritmo (0.09/s) hasta que existan fuentes de agua — la fruta hidrata.
  - **Oxígeno** solo drena **expuesto al vacío** (Luna, Space, planeta con `atmo_top_m == 0`) y **fuera de la cápsula** (`RocketFlight.active` = presurizado). Recarga 25/s con suministro: atmósfera, cápsula, o a <14 m de un beacon de pad/lander/dock (`is_supply_beacon`).
  - Hambre o sed en 0 → 1.5 dmg/s; asfixia → 4 dmg/s.
- **Comer:** comida equipada + sin target → **HOLD para comer** (touch look-zone y clic derecho desktop; `HoldInteractState.eating`, consumidor en `survival_stats.rs` ANTES de `update_interact_hud`). Tabla de restauración `food_restore` (apple 30/8, pie 60/5, soup 40/35…).
- **Fuentes de comida:** manzana = drop determinista de 1 de cada 3 árboles por hash de celda (`hud.rs` grant); receta 13 **Fruit Pie** (3 apple + 1 wood). Ítems food en catálogo TS + iconos procedurales (`item_icons.rs::draw_fruit/draw_meal`).
- **Muerte amable** (visión): hp 0 → barras e hp llenos, inventario intacto; en Tierra despertás en el pad de la agencia; off-world te levantás en el lugar (el tether de O₂ del beacon lo hace sobrevivible). Sin game over duro.
- **Persistencia:** `survival.json` (hp + barras), flush-on-pause + debounce 20 s — patrón B10.
- **HUD:** 3 barras finas bajo los corazones (`hud/survival_bars.rs`); la de O₂ solo aparece en vacío o si no está llena.
- **Regla:** cualquier cambio numérico va a TS **y** Rust (regla 10 de §3).

---

## §13-bis ⭐ REWRITE 2D (12 jul 2026) — estado

> **DECISIÓN FINAL del usuario (12 jul):** el 2D es definitivo. `rewrite-2d` se mergeó a `main` (ff, commits fa8f197 + 22e4c09 purga total: shaders/GLB/Blender/skills toon). El 3D existe SOLO en el historial git (hasta a9317df). No se conserva ni se reintroduce.

**Hecho en la rama (compila, tests verdes 90 Rust + 101 TS, boot desktop limpio):**
- `ha-shared/flat2d.rs`: terreno determinista por cuerpo (wrap circunferencia), locomoción plataformera, remap exacto superficie↔órbita (7 tests).
- Cliente 2D completo: `world2d` (tiles+decor+vetas+persistencia), `sim` (tick plano), `camera` (zoom continuo), `scene`, `sky2d`, `targeting` (puntero), `rocket_flight` (vuelo plano + relanzamiento), `orbital2d` (círculos), `launch_pad`/`machines` (sprites 2D), partículas 2D.
- Reusado intacto: HUD/inventario/craft (2813 líneas), survival stats, colonos, ship builder, nav computer, harvest, gamepad, persistencias, menú/mapa, boot UI, perf/crash.
- Borrado: ~25 módulos 3D (recuperables en `main`).

**Pendiente al retomar:**
1. Probar en el Redmi (APK 2D ya publicado desde `main`) — objetivo 60 fps; §6.
2. Pulido jugable: agua (visual-only hoy), depósitos re-minables (sin agotamiento), cámara remota de telemetry (no-op).
3. Retune de contenido para la tira 2D (distancias de depósitos, alturas de vuelo, banda del fade §11).
4. ~~Merge a `main`~~ HECHO (decisión del usuario, sin esperar al Redmi).

### §13-ter — Assets drop-in + verificación end-to-end (13 jul 2026)

**Capa de arte drop-in (para packs comprados Modern Exteriors/Interiors/UI):**
- `assets/art/<categoría>/<clave>.png` + `manifest.json` (generado por `npm run sync:art`, embebido en compile-time). `art2d.rs::ArtLibrary` carga y da fallback al arte procedural. `items/<id>.png` reemplaza el ícono en hotbar/bolsa/craft/máquinas. Consumidores conectados: decor (árbol/roca/vetas, variante `_moon`/`_mars`), máquinas, jugador (`player/astronaut`), cohete (`ship/rocket`). Ver `assets/art/README.md`. Tiles/UI/fondos = futuro.

**Arnés de verificación runtime (`debug/autodrive.rs`, gate `HA_AUTODRIVE=1`+`HA_AUTOPLAY=1`):**
- Maneja el juego por el pipeline REAL de input (ButtonInput virtual en PreUpdate) y loguea `ha-verify: <sistema> PASS/FAIL`. Capturas: `debug_screenshot` (auto a ~7s + F12). `HA_AUTOPLAY` saltea PLAY y fuerza updates continuos (winit no throttlea sin foco).
- **11/11 PASS (13 jul):** walk · dig · chop+targeting · craft · machine-place · automation · ignition · space-flight · moon-strip · oxygen-vacuum · place-tile. Extender este arnés al agregar sistemas.

**Bugs reales encontrados y corregidos por el arnés/capturas:**
- **Terreno duplicado (reporte del usuario):** el HUD tenía una 2ª `Camera2d` (`spawn_hud_camera`) que re-renderizaba TODO el mundo 2D como mini-overlay flotante. Fix: una sola cámara con `IsDefaultUiCamera` (mundo+UI). Eliminado `spawn_hud_camera`/`HudCamera`.
- **Vertex colors lavados:** los quads de tiles usaban sRGB directo en un pipeline lineal → se veían pálidos vs los sprites. Fix: `Color::srgba(...).to_linear()` en `build_chunk_mesh`.
- **Grant de inventario roto (dig y place):** consumían/flusheaban desde `inv_ui` (desincronizado) en vez de tratar el inventario como verdad. Fix: `inv.add`/`inv.remove` directo (mismo patrón que máquinas); el sync del HUD mergea a los slots.
- **Relanzamiento espurio del cohete al colocar bloque en la Luna:** un hold activo pone `actions.action=true`; al completar el hold de place, ese `action` queda residual un frame con `hold.active` ya false y el target con `place_cell` (no `block_cell`) → `try_surface_launch` no encontraba guarda y encendía el lander. Fix: la guarda ahora incluye `target.place_cell.is_some()` y `hold.completed`.
- **Input bloqueado en vuelo (B8):** `sync_ui_gameplay_block` marcaba `blocked` cuando `flight.blocks_walking()`, matando thrust/steering. Fix: sólo UI real (bag/editor) bloquea; el vuelo nunca.
- **Pad no re-aparecía al volver a la Tierra (B6):** las entidades del pad no se despawneaban off-Earth. Fix: marker `PadDecor` + `sync_pad_visuals` (despawn fuera de Tierra, respawn del pad+nave al volver).

### §13-quater — Rework de UI hacia el sistema de construcción (17 jul 2026)

Preparación para el futuro sistema de construcción (mezcla con máquinas). Detalle
por fase en `MIGRATION_PLAN.md` (Fases 8-11, todas ✅ y verificadas con capturas).

- **Twin-stick (Fase 8):** stick IZQUIERDO = mover, DERECHO = mira libre. El
  cursor es un `AimCursor` (world x,z) que el stick integra por velocidad y se
  clampa a `AIM_MAX_REACH` del jugador (`input/joystick.rs`). En desktop la mueve
  el mouse (`desktop_aim_cursor`). El targeting lee el cursor (entidad + celda).
  El **retículo visible** (cruz) sigue al cursor libre en el mundo
  (`world2d.rs::PointerReticle`, sin snap) — ver §13-sexies. La vieja cruz fija en
  el centro (`look_zone_ui.rs`, legado FP) fue eliminada.
- **Resalte de celda (Fase 8):** `CursorHighlight` en `world2d.rs` — corchetes
  amarillos en las esquinas de la celda bajo la mira. **Es la base del sistema de
  construcción** (feedback de "acá va a caer la pieza").
- **USE dispara el hold (Fase 8):** la zona derecha ya es el stick, así que el
  botón USE conduce el hold-interact (`use_button_hold_interact` en look_interact).
- **Menú unificado (Fase 9):** UN solo panel (`game_menu.rs`) con TODO: mapa,
  viaje, modos y los controles del editor (antes abrían panel aparte). EDITAR UI
  vuelve el mismo menú see-through (`sync_menu_edit_mode`) para arrastrar los
  controles; calibración = sub-diálogo desde AJUSTES. Sin menú anidado.
- **USE/JUMP reubicados (Fase 10):** en arco ARRIBA del stick derecho (use
  0.72,0.60 · jump 0.88,0.50). Bug corregido: `spawn_action_ui` estaba compilado
  solo-Android → no salían en desktop; ahora usa `uses_touch_gameplay_ui()`.
- **Minimapa dinámico (Fase 11):** el mapa del menú cambia según región —
  superficie = minimapa top-down del terreno (grilla 24×24 muestreando tiles en
  tiempo real, título = nombre del cuerpo, marcador del jugador); espacio = vista
  orbital de siempre. `OrbitalLayer`/`TerrainLayer` + `update_terrain_minimap`.

### §13-quinquies — Construcción + correcciones de UI (17 jul 2026, 2ª ola)

Detalle por fase en `MIGRATION_PLAN.md` (Fases 13-17, todas ✅). Publicado en
v1.0.153. **Acceso remoto:** APK buildeado con la IP Tailscale (`--host
100.83.97.54`) — el Redmi ya está en la tailnet, así que con Tailscale ON
conecta al server desde fuera de la LAN sin exponer puertos.

- **Menú limpio (F13):** los selectores del editor (MOVER/USE/JUMP) confundían
  en el menú (parecen controles del jugador). Ahora el menú principal deja solo
  funciones de menú; los selectores pasan a `EditToolbar`, visible solo en
  "EDITAR UI".
- **Hotbar táctil (F14):** bug — el hotbar (abajo-centro) solapa las zonas de
  los joysticks y los sticks robaban el tap (el ítem nunca se equipaba). Fix de
  ORDEN: `touch_move_stick`/`touch_look_stick` corren `.after(
  hud::append_hud_touch_blocks)`, así `TouchBlockRects` ya tiene el rect del
  hotbar antes de que los sticks decidan. Regla: **la UI táctil sensible debe ir
  arriba-izquierda (fuera de zonas de stick) o registrarse en TouchBlockRects.**
- **Construcción (F15-16):** catálogo `BUILD_PIECES` (Piso/Muro madera/Muro
  piedra/Puerta) + tiles nuevas; modo build (toggle CONSTRUIR / tecla B), paleta,
  ghost verde/rojo en la celda apuntada (base = `CursorHighlight`), colocar con
  USE cobrando recursos. Quitar = cavar (dug_item refunda). El martillo fuera de
  build sigue colocando máquinas. Verificado: autodrive "build PASS".

### §13-sexies — Batería en background, puntero libre, limpieza de cámara (18 jul 2026)

Tres pedidos del usuario, aplicados sobre `rewrite-2d`:

- **Batería al minimizar/cerrar:** el cliente nativo no tiene hilos, audio ni red
  (verificado), y la persistencia YA hace flush en el flanco de pausa
  (`*_persist.rs`, patrón `was_paused` / `is_app_paused()`), así que un cierre por
  deslizamiento conserva el estado. Lo único que faltaba era frenar el render.
  `platform/mod.rs::sync_lifecycle_pause` ahora, **solo en Android**, además de
  pausar `Time<Virtual>`, reescribe `WinitSettings` en el flanco pausa/reanuda:
  al pausar → `reactive_low_power(3600 s)` (el loop se estaciona y despierta solo
  con un evento de ventana/usuario, o sea la reanudación); al reanudar →
  `WinitSettings::mobile()`. Efecto: ~0 draws en segundo plano.
- **Puntero libre (estilo Terraria):** se eliminó la **cruz fija en el centro**
  (legado FP — `input/look_zone_ui.rs` borrado, junto con `FpCrosshair*`). El
  puntero es ahora un retículo **en el mundo** que sigue EXACTAMENTE a `AimCursor`
  (`world2d.rs::PointerReticle` + `sync_pointer_reticle`, sin snap a la grilla),
  y se ilumina cuando hay `LookTarget`. Los corchetes amarillos (`CursorHighlight`)
  siguen **resaltando la celda** apuntada (snap), que es lo correcto. El stick
  derecho mueve el cursor libremente; la grilla solo se resalta y activa funciones.
- **Cámara 1ª/3ª eliminada:** en 2D no hay perspectivas. Se quitó el botón `CAM`
  (`ui_editor.rs`: `UiCamButton`, `cam_button_system`, spawn y layout), el recurso
  muerto `CameraCycleRequest` (`camera.rs`) y el ciclo del botón North del gamepad
  (`input/gamepad.rs`). **Se conserva** el botón "REMOTE CAM" del panel de
  telemetría (tecla T): es monitoreo remoto de bases (feature futura, dormida), no
  1ª/3ª persona.

### §13-septies — Bucle de nave, Fase 1: interior abordable (18 jul 2026)

Base para el bucle nave (abordaje → cabina → recarga → vuelo). Reusa el mundo de
tiles en un **Z-level dedicado positivo** (`SHIP_INTERIOR_Z = 64`): ahí el terreno
natural es roca sólida, así que un cuarto tallado en `WoodFloor` queda **encerrado
sin poner paredes**. Módulo `ha-game/ship_interior.rs`:
- `RegionId::ShipInterior` (nuevo) — reconocido por `surface_region_active` para que
  el streamer de chunks dibuje su z-level; `ctx = Mission` (gravedad 1, sin vacío/O₂).
- **Abordar/salir = cambio de marco discreto (B8):** parándose en la celda de la nave
  del pad `(28,6)` entra; parándose en la puerta `(305,307)` del cuarto sale. Latch
  `armed` (igual que el `layer_transition` de la mina) evita ping-pong. Nunca bloquea
  input ni cámara.
- El cuarto NO se persiste: `TileWorld::force_tile` no marca `dirty` y
  `debounced_tiles_save` filtra `z == SHIP_INTERIOR_Z`.

**Fase 2 — cabina (`CockpitSeat`) + HUD de pre-vuelo (`PreFlightUI`):** parándose en
la celda del asiento `(305,300)` el jugador se sienta (`CockpitState::seated`): se
**bloquea el movimiento** (extendí el único escritor `sync_ui_gameplay_block` en
`hud.rs` con `|| cockpit.seated`, sin romper la invariante de un solo escritor) y
aparece el panel `PreFlightPanel` con lectura de combustible en vivo y 3 botones —
**DESPEGAR** (vuelo manual), **AUTOPILOTO** (marca `flight.autopilot`, aproximación/
hover llega en F4) y **LEVANTARSE**. Despegar reusa la ignición del pad
(`launch_pos_vel(Earth)` + `begin_at` + `vel`); hoy llena el tanque como el pad —
en F3 el combustible pasa a ser recurso gestionado (se llena en la sala de máquinas).
Marcador del asiento = slot `decor/cockpit_seat` (fallback de color).

**Fase 3 — estación de recarga (`FuelSource`) + bucle de combustible por eventos:**
el combustible pasa a ser recurso gestionado — el despegue de cabina YA NO auto-llena
el tanque (a diferencia del pad); se llena en la **sala de máquinas** del interior.
Celda `FUEL_SOURCE_CELL (300,300)`: al acercarse (≤1.8 m) con tanque no lleno se emite
`RefuelEvent` (Bevy `EventWriter`→`EventReader`, edge-triggered por `armed` = un
acercamiento, una recarga); `apply_refuel` llena a capacidad. Loop intencionado:
abordar → sala de máquinas (recargar) → cabina → DESPEGAR. Marcador = slot
`decor/fuel_source` (fallback de color). (El pad de Tierra sigue siendo un 2º punto de
recarga, vía su hold-USE de ignición.)

**Fase 4 — telemetría de vuelo + hover del autopilot:** el HUD de vuelo
(`build_interact_prompt` en `hud.rs`) ahora muestra **radar de superficie** (`altitude()`
= distancia a la superficie, no al centro), **vector vertical** `↕` (velocidad radial
subida/bajada) y **vector horizontal** `↔` (velocidad tangencial) — nuevos métodos
`RocketFlight::vertical_speed/horizontal_speed`. **Autopilot 80% asistido** (`drive_rocket_flight`):
al activarlo apunta la nariz radial-out y auto-empuja para sostener ~`AUTOPILOT_HOVER_ALT_M`
(500 m) sobre la superficie / frenar caídas (reusa el mismo path de aceleración; el
input manual lo desengancha). Altitude-hold **grueso** (bambolea alrededor del set-point):
el intercept preciso + hover exacto se **tunean en device** (§6: nunca tunear vuelo a ciegas).

- **Pendiente:** texturas del interior/marcadores (`decor/cockpit_seat`, `decor/fuel_source`)
  = slots drop-in a asignar (LimeZu Interiors), hoy fallback de color; tuning de hover +
  sensaciones del bucle nave en el Redmi.

### §13-octies — Fixes tras prueba en device (18 jul 2026)

Primera prueba real del bucle de nave en el Redmi (capturas). Correcciones:
- **Empaquetado APK (crítico):** un `libha_game.so` **armeabi-v7a del 6 jul (motor 3D
  pre-purga)** quedó en `jniLibs/` y gradle lo metía al APK → en 32-bit corría el juego
  3D viejo sin la nave. Borrado; APK ahora arm64-only. Ver §7.
- **Fuente ASCII-only:** todas las cajas □ del HUD/menú eran `· ↕ ↔ ● ☀ ó —` que la fuente
  no tiene. Pasado a ASCII (HUD de vuelo, leyenda del menú, panel de cabina). Regla nueva en §7.
- **Vuelo injugable por combustible:** `ROCKET_FUEL_BURN_PER_S` 8.0→3.5 (native-only, sin
  paridad) → ~28 s de empuje con tanque lleno (antes ~12 s, se agotaba a media subida); y el
  despegue de cabina vuelve a llenar el tanque (el `FuelSource` queda como top-up off-world).
- **Minimapa del menú en blanco:** el `MapViewport` con `Val::VMin(70)` colapsaba a ~0 →
  cambiado a `Val::Vh(52)` (tamaño explícito) + borde/fondo visibles. *(A confirmar en device.)*
- **Pendiente/observado:** el planeta en vuelo se ve como círculo 2D (SFS correcto) — con más
  combustible ya se puede subir y ver el cuerpo completo + aproximar/aterrizar. Abordaje de la
  nave (pisar `(28,6)`) puede ser finicky → si molesta, radio + cartel.

**Cámara en vuelo (reporte device, v1.0.160):** el zoom **auto-alejaba a 9 km** (`SURFACE_VIEW_H +
alt*2.2`) → el cohete quedaba un puntito lejísimos, imposible pilotar. Rework en `camera.rs`:
recurso `CameraZoom` **controlado por el usuario** (pellizco 2-dedos en móvil / rueda en desktop),
la cámara **siempre centrada** en el cohete (vuelo) o el jugador (superficie), con default cercano
(`FLIGHT_DEFAULT_H = 320 m`) al entrar/salir de vuelo. El pellizco se **gatea a vuelo/espacio**
(en superficie los 2 sticks usan 2 dedos y se leería como falso pinch). Clamps `[18, 12000] m`.
Necesario para poder testear salida/aterrizaje.

## §13 Estado del plan maestro + CONTINUACIÓN (12 jul 2026 — leer si retomás a mitad)

**Hecho y shipeado (v1.0.147, commits 792b98d..793be81):** F0 tests verdes · F1 survival core (§12) · F2 ship builder Fase 1 (contadores) · F3 base lunar (taladro/antena/monitoreo, máquinas por cuerpo) · F4 gamepad · F6 APK publicado. F5 perf: **medición bloqueada hasta conectar el Redmi** (plan en memoria del agente + §6).

**Fases restantes, EN ESTE ORDEN (aprobadas por el usuario 12 jul):**

1. **Ship Builder Fase 2 — piezas estructurales + nave visible** ✅ HECHO (12 jul):
   - Ítems `part_fin` (aletas: +estabilidad = menos drift/wobble en vuelo manual) y `part_nose` (cono: −drag atmosférico = ascenso más rápido). Recetas 17/18. TS `shipMass.ts` → espejo `ship_mass.rs`: `ShipConfig` gana `fins: u32 (0..=4)` y `nose: u32 (0..=1)`; `ShipConfigStats` gana `drag_factor` y `stability`.
   - Efecto en vuelo: `rocket_flight.rs` — drag atmosférico × `drag_factor` (nose 1.0→0.75), damping de rotación × stability.
   - **La nave del pad refleja la config** (el payoff SFS): `launch_pad.rs` respawnea el stack visual al cambiar config — tanques apilados (altura = 1+tanks), motores en anillo (n=engines), aletas, cono. Marker `ShipConfigVisual` + sistema `sync_ship_config_visual` (compara `builder.is_changed()`).
   - Tab SHIP: filas FINS y NOSE. NO editor 3D libre en móvil (desviación deliberada de la Fase 2 del design doc — riesgo UX; anotar en SHIP_BUILDER_DESIGN.md).
2. **Interior lunar (cueva)** ✅ HECHO (12 jul) — reusó el sistema de cueva Tierra (`scene.rs` ~470-530: portal → teleport a `CAVE_INTERIOR_ORIGIN`, contexto Mission, `cave_surface_return`): portal "boca de mina" en Luna (tx,tz fijos cerca del spawn), la instancia asigna recurso raro (gem_red denso) + oscuridad (linterna manda). `RegionRuntime` necesita saber A QUÉ cuerpo vuelve la cueva (hoy asume Tierra — agregar `cave_return_body`).
3. **Colonos v1** ✅ HECHO (12 jul) — sim TS `packages/shared/src/sim/colony.ts` PRIMERO + espejo `ha-shared/colony.rs`: `Colonist {o2, food, assigned_machine_uid}`, tick barras, boost ×1.5 al output de la máquina asignada. Máquina `habitat` (catálogo TS+Rust+parity). Solicitud desde Tierra vía panel del mapa (gated: antena + habitat + stock comida). Persistencia `colony.json`. Línea en monitor BASE LUNA.
4. **Tierra fases 2-3** (retune contenido radio 1500 + pase visual) — pendiente de sensaciones en device, NO hacer a ciegas.
5. **Ship Builder Fase 3** (bisagras/pistones) — al final, pesado.

**Al cerrar cada fase:** tests ambos lados + `UPDATE_CATALOG_PARITY=1 npm test` si tocaste catálogos + fila en §4 + `graphify update .` + commit. Publicar con `npm run ship:android` (URLs LAN :5173/:2567; servers ya corren).
