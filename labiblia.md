# La Biblia — Starship (Agencia Espacial Survival)

> Contexto maestro para agentes IA. **Magra y VIGENTE** (reescrita 6 jul 2026, reemplaza la histórica de 1188 líneas). Si algo acá contradice al código, gana el código — pero avisá para actualizar. El historial viejo se descartó a propósito; lo único "histórico" que queda es la sección §10.

---

## §0 Identidad

| Campo | Valor |
|-------|-------|
| **Producto** | **Starship** (Agencia Espacial Survival). Repo interno `hunger-arena`; package Android `com.hungerarena.app`. |
| **Género** | Survival + construir base/laboratorio + automatizar recolección → naves prefab → off-world → colonos. |
| **Referencias** | KSP / Space Flight Simulator **solo en escala**; **cero** física orbital real (siempre arcade). |
| **Home base** | **Tierra** (seed **42**). |
| **Plataformas** | Android APK (arm64) + Linux AppImage (Bevy) · landing web · servidor Colyseus. |
| **Stack** | Colyseus · **Rust/Bevy 0.15** · Postgres+Drizzle · npm + Cargo workspaces. |
| **Sim** | Determinista TS (`packages/shared`); servidor **input-only**; `ha-shared` porta constantes a Rust. |
| **Estética** | 3D: Ghibli cel + outline. UI in-game: Borderlands (bordes ink + amarillo). Landing: synthwave. |
| **Versión** | ver `native/version.json` (no hardcodear). |

### Regla de migración (INVIOLABLE)
**No reescribir el motor.** Contenido nuevo sobre motor probado. Intacto: `native/crates/` (Bevy, cámara FP, input 3 gestos, AssetFactory, toon, perf), `server/` (Colyseus, auth, `/updates`), core de `packages/shared/src/sim` (tick 20 Hz, determinismo, parkour, `interactions.ts`).

### Legado dormido (NO borrar, NO referenciar en contenido nuevo)
Clases scout/warrior/mage, habilidades, oleadas survival, AI hostiles — permanecen para fauna/peligro futuro (ver §10).

---

## §1 ⭐ MODELO VIGENTE — CONTINUIDAD "UN SOLO UNIVERSO"

**Esto es el corazón del diseño espacial actual. Léelo antes de tocar espacio/vuelo/cielo/planetas.**

- **Un solo espacio/universo.** El espacio ES el escenario; Tierra, Luna, planetas, estrellas son objetos DENTRO, en órbitas **coherentes** (no a escala real; todos en el plano XZ) — la cantidad real de planetas/lunas para empezar. NO hay "cielo de la Tierra" y "cielo de la Luna" separados.
- **Tierra = marco unificado (sin "corte" al despegar).** `EARTH_GAMEPLAY_RADIUS_M = 1500`; radio caminable == orbital. La Tierra es el **ORIGEN FIJO** (`earth_center_at` no revoluciona; `FLIGHT_PLANE_Y = -1500 = -radio`, todos los cuerpos coplanares). Marco caminable == orbital ⇒ `remap_planet_gameplay_to_orbital_at(Earth)` es **identidad** ⇒ despegue/aterrizaje continuo. El mundo caminable se renderiza vía **floating-origin** (`surface_frame.rs`: `SurfaceAnchored` + `surface_live_offset`); Tierra fija = offset 0 (intacta). `view_target` calcula el observador MUNDIAL desde `SimState` (no del transform, que ya es render).
- **Culling por distancia + mapa 2D.** Lo lejano NO se renderiza en 3D desde una superficie (parado en la Luna NO ves la Tierra, y está bien). Un cuerpo se ve solo al **acercarte volando** (vacío) o por cámara remota (la esfera real aparece y crece hasta que aterrizás). Para navegar: **mapa 2D** (`game_menu.rs`, se abre con el botón MENU) — cenital del plano XZ, Sol al centro, anillos de órbita + puntos por efeméride (`body_center_at`), tu posición. Los cuerpos del cielo son **las mismas esferas reales**, no discos/skybox por-escena. El panel es fullscreen y **opaco** (no semitransparente: en el Redmi el blend fullscreen sobre el 3D vivo lo tiraba a ~4 fps). El viewport es **cuadrado** (`Val::VMin`) y los marcadores se posicionan en **porcentaje** (no en px medidos por `ComputedNode`, que devuelve px FÍSICOS mientras `Node` usa px LÓGICOS → en celu high-DPI eso empujaba todo el mapa fuera del panel a la derecha), con `overflow: clip`.
- **Atmósfera = SOLO la Tierra.** El "cielo" (domo azul Ghibli + nubes/niebla) es exclusivo de la Tierra: subís, la atravesás, llegás al espacio. Cuerpos SIN atmósfera (Luna/planetas airless) → **espacio directo** (negro + starfield), sin domo terrestre (`scene.rs` `sky_vis` oculta el domo en `on_surface`). El look de la Luna (esfera + cráteres) **se conserva, NO se toca**.
  - **El starfield de cuerpos airless NO es geometría finita cruzable.** En la Luna el shell de estrellas + puntos (`LunaSkyShell` en `moon_world.rs`) se **recentran en el jugador cada frame** (`follow_luna_sky_shell`) = skybox que nunca se puede atravesar. Antes estaba anclado en `moon_center` con R=`MOON_GAMEPLAY_RADIUS_M+460`: subiendo >460 m lo cruzabas y veías una esfera negra finita desde afuera (el "techo de cielo"). Cualquier "cielo" de un cuerpo airless debe seguir a la cámara o ir al far-plane, **nunca** una esfera fija.
- **Cámara-módulo (futuro).** Pieza de nave = observador remoto arbitrario en cualquier base. Esqueleto: `ViewTarget::RemoteCamera`. Todo el sistema está centrado en "observador arbitrario", justo para esto.

### 🚫 ANTI-REGRESIÓN — NO volver a esto
- NO el **walk-hub orbital caminable** (`dock_rocket_at_station` fuera del schedule; el espacio es vuelo continuo, no una estación donde caminás).
- NO cielos/hubs **por-escena** (esferas SFS hardcodeadas, `spawn_transit_planets`, `spawn_space_*_sphere`).
- NO el **split Tierra 480 vs 8000** ni `FLIGHT_PLANE_Y=-8000`.
- NO forzar cuerpos lejanos visibles desde una superficie (skybox) — culling + mapa 2D.
- NO cambiar el render de la Luna/esferas.

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
9. Shader Android: `toon.wgsl` embebido (`load_internal_asset!`), no depender de `assets/` del APK.
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
| **Superficie caminable esférica** (patch tangente streameado, half 120 m Android) | `ha-shared/planet_surface.rs`, `ha-game/surface_patch.rs`, `lunar_surface.rs` |
| **Marco continuidad** (walkable → floating-origin, offset vivo) | `ha-game/surface_frame.rs`, `floating_origin.rs`, `view_target.rs` |
| **Vuelo continuo** (thrust/coast/retro, fuel real, drag, paracaídas, SOI dinámica, aterrizaje) | `ha-game/rocket_flight.rs`, `ha-shared/flight_physics.rs` |
| **Regiones + atmósfera** (Tierra/Cueva/Luna/Planeta; domo solo-Tierra) | `ha-game/scene.rs` (`RegionId`, `sync_biome_visibility`, `sky_vis`) |
| **Mundos por cuerpo** (spawn/despawn) | `earth_world.rs`, `moon_world.rs`, `planet_world.rs`; limpieza `celestial_cleanup.rs` |
| **Cielo/impostores** (esferas reales visibles al acercarse; SkyMoon/SkySun estilizados en Tierra) | `ha-game/flight_celestials.rs` |
| **Mapa 2D del sistema** (navegación, MENU fullscreen + botones TIERRA/LUNA/ESPACIO/CREATIVO) | `ha-game/game_menu.rs` |
| **Navegación/sondas** (mapear cuerpos, gate de autopilot) | `ha-game/nav_computer.rs`, `ha-shared/nav_data.rs` |
| **Depósitos/flora** (procedurales por cuerpo, batch por kind) | `ha-shared/surface_deposits.rs` `surface_biome.rs`, `ha-game/deposit_spawn.rs` `flora_spawn.rs` |
| **Día/noche + luz solar** | `ha-game/sky_clock.rs` (ciclo 720 s), `solar_lighting.rs` |
| **Construcción / bloques** | `ha-shared/building.rs` `blocks.rs`, `ha-game/building_world.rs` `block_world.rs` |
| **Máquinas + inventario** | `ha-game/machines.rs`, `inventory_persist.rs`; sim `packages/shared/src/sim/machines.ts` |
| **Survival stats** (hambre/sed/O₂, comer, respawn amable) | sim `packages/shared/src/sim/survivalStats.ts` → espejo `ha-shared/survival.rs`; cliente `ha-game/survival_stats.rs`, barras `hud/survival_bars.rs` (ver §12) |
| **Ship builder** (piezas crafteables → contadores → stats emergen) | sim `shipMass.ts` (`ShipConfig`, `shipConfigStats`) → espejo `ha-shared/ship_mass.rs`; cliente `ha-game/ship_builder.rs` (tab BAG→SHIP, `ship.json`), extras aplican vía `launch_pad.rs::sync_ship_loadout`; motores extra multiplican thrust en `rocket_flight.rs` |
| **Base lunar** (Acto II: taladro regolito, antena, monitoreo) | máquinas por CUERPO (`ha-game/machines.rs`: gates `machine_allowed_on`, `machines.json`, respawn `sync_machine_visuals` B6); off-body banquean `stored_output` (se cobra al volver); campamento = O₂ tether en `survival_stats.rs`; antena → línea "BASE LUNA" en mapa orbital (`game_menu.rs::update_luna_base_monitor`) |
| **Cámara FP + targeting** (alineada a normal de superficie) | `ha-game/camera.rs`, `targeting.rs`, `input/look_interact.rs` |
| **Gamepad** (stick izq mover+sprint, der cámara; A salto/retro, RT acción/romper, LT colocar/comer/retro, Y cámara 1ª/3ª, Start mapa, B cerrar, D-pad bag/hotbar) | `input/gamepad.rs` — alimenta los MISMOS intents que touch/teclado; prioridad touch > pad > teclado |
| **HUD / UI / editor** | `ha-game/hud.rs`, `ui_editor.rs`, `ui/icon_factory.rs`, `ui_theme.rs` |
| **Calidad por plataforma** (patch step, radios, sombras, sky lite, suelo unlit Android) | `ha-game/quality.rs`, `render/render_scale.rs` |
| **Debug/perf** | `ha-game/debug/perf.rs` (MENU→PERF / F3), `cpu_span.rs` (ha-span), `crash.rs` |

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

**Diagnóstico (dato duro del HUD `ha-perf`/`ha-span`, 6 jul):** `total_update` CPU ≈ **20-25 µs** (¡microsegundos!) con frame de **80-180 ms**. → **La lógica del juego NO es el cuello. El frame es 100% GPU/render + present.** No optimizar sistemas ECS por FPS; atacar la GPU y la memoria.

**Breakdown típico Tierra:** `draw3d~58` (de los cuales `ground≈54` = batches flora/depósitos), `ui≈92` (draws de HUD), `ents≈210`. `outline=0`, `ocean=0`, `chunk=0`.

**Palancas por impacto (mayor → menor):**
1. **Resolución interna** (la más grande si se logra): renderizar el 3D a menos píxeles. El **blit offscreen REGRESÓ** en Redmi (el blit mismo era lento) → probar en cambio bajar la resolución de la SUPERFICIE/ventana Android nativa (que el compositor escale), no un blit extra. Bloqueado hasta encontrar método sin blit.
2. **Fill-rate / fragment** (cobertura de pantalla × costo por píxel):
   - Suelo casi-fullscreen: **unlit en Android** ✅ (`patch_material`).
   - **Domo de cielo Ghibli**: fullscreen aun en "lite" (`sky_shader_quality=0`) → sospechoso #1 ahora que el suelo es unlit. Considerar gradiente barato en Android.
   - Overdraw: outlines ya off (outline=0); nubes/transparencias mínimas.
3. **Memoria / swap** (causa histórica confirmada): el Redmi con ~285 MB heap nativo y poca RAM libre paga page-faults por frame (mesetas de ~90-250 ms por kswapd). Mitigado con meshes a `RenderAssetUsages::RENDER_WORLD` (libera copia CPU tras subir a GPU). **Vigilar el heap**; menos entidades/texturas ayuda directo al FPS.
4. **Draw calls** (`ground≈54`, `ui≈92`): re-batching flora/depósitos por grupos 3×3 ya reduce draws; UI con muchos nodos (hotbar iconos + corazones + texto). Importan menos que el fill-rate en este device, pero bajarlos ayuda al margen.

**Método:** medir SIEMPRE en el Redmi jugando con input real (MIUI throttlea apps foregrounded-por-adb sin input → FPS idle mentiroso). Desktop 940M/NVK **no es representativo** (driver open-source + throttle térmico) — solo para correctitud. Herramienta: MENU→PERF (móvil) / F3 (desktop); spans en `debug/cpu_span.rs`.

---

## §7 Troubleshooting

- **APK no actualiza / "no muestra cambios":** confirmar versionCode bumpeado + manifest publicado; relanzar la app (OTA `mandatory`). Es Starship (Bevy), no Godot.
- **MIUI `INSTALL_FAILED_USER_RESTRICTED`:** activar "Instalar apps vía USB" en Opciones de desarrollador, o instalar por URL (`http://<host>:5173/downloads/hunger-arena.apk`) tocando "Instalar" en el celu.
- **Disco se llena en cargo:** `rm -rf native/target/debug` (el build Android usa otro target dir).
- **Lock de cargo ("Blocking waiting for file lock"):** matar cargo zombie; borrar `native/target/**/.cargo-lock`.
- **z-fighting de cuerpos lejanos:** far plane de vuelo grande + near razonable (GLES3 sin reversed-Z).

---

## §8 Visión (resumen — detalle en `native/docs/SPACE_SURVIVAL_VISION.md`)

Loop: **construir base en la Tierra → automatizar recolección → naves prefab (vuelo arcade) → bases off-world → colonos.** Dirección: Tierra (actual) → naves → off-world → colonos. Sin física orbital; 30+ FPS antes de más features espaciales. Naves = módulos prefab (no Kerbal). Anti-goals: nada de ensamblaje pieza-por-pieza tipo KSP, nada de física orbital real.

---

## §9 Skills / docs / herramientas para IA

`ghibli-toon-graphics` (WGSL cel), `procedural-asset-factory` (AssetFactory/IconFactory), `CLAUDE.md` (resumen agente), `native/README.md` (build), `native/docs/SPACE_SURVIVAL_VISION.md` (visión), `native/docs/PLAYER_WORLD_DESIGN.md`, `context_to_ai.md` (mapa detallado de sistemas).

**graphify (grafo de código) — USAR ANTES de cambios grandes.** Grafo en `graphify-out/` (`graph.json`, `GRAPH_REPORT.md`). Comandos: `graphify explain "Nodo"` (nodo + vecinos), `graphify path "A" "B"` (camino entre dos), `graphify update .` (refrescar tras cambios, sin costo API). **Regla:** antes de tocar un sistema con muchas dependencias (render, marco de mundo, cámara, cielo), consultar el grafo para ver qué depende de eso — así se evitan roturas cruzadas (ej: el refactor `surface_frame` rompió la Luna por no ver que el `LunaSkyShell`/cámara asumían el marco estático). El `GRAPH_REPORT.md` lista god-nodes (`CelestialBody`, `RocketFlight`, `make_material`), ciclos de import (hud↔inventory_ui, varios con scene.rs) y comunidades.

---

## §10 Legado dormido (auditado, NO remover, NO referenciar en contenido nuevo)

Sistemas de la versión combate/OASIS que siguen en el repo por reuso futuro (fauna/peligro): clases + habilidades + AI hostiles + oleadas survival (`packages/shared/src/sim/` combat/ai/survival), schema Colyseus. **No** son parte del concepto nuevo pero **no se borran**. Tampoco reintroducir lo ya retirado: `apps/mobile`, `apps/desktop`, `packages/client-core`, `packages/client-gfx`, React Three Fiber, Expo, Tauri; ni el walk-hub orbital / cielos por-escena / split Tierra 480-8000 (ver §1 anti-regresión).

---

## §11 Movimiento: creativo, jetpack, políticas (actualizado 10 jul 2026)

**Modo creativo = modo dios (hoy es la ÚNICA forma de volar — el cohete construible aún no existe).**
- Se vuela con la locomoción a pie libre: `sim.rs` en creativo llama `step_surface_creative_fly` (JUMP sube, sprint baja, **sin gravedad**). El controlador de movimiento es el mismo en Tierra/Luna/planeta — el contexto cambia solo para render/culling/efemérides.
- **Es continuo por construcción:** en espacio-tangente `body.y` crece sin límite y se renderiza a lo largo de la normal (`interpolate_player_visual`), así subir recto desde la Tierra es un ascenso mundial correcto a cualquier altura, sin corte ni cambio de cámara.
- **NO** hay ignición forzada de `RocketFlight` (se borró `creative_fly_to_space`). Para saltar a otro cuerpo en creativo, usar los botones **LUNA/ESPACIO/TIERRA** del MENU (teleport deliberado, no una animación involuntaria).
- **Fade atmosférico al subir (hecho).** En `sky_clock.rs` `sync_sky_projection`: en creativo, sobre la Tierra y sin `launch_atmo`, la altura del jugador (`sim.body.y − ground`) rampa entre `CREATIVE_SKY_FADE_START_M`=150 y `END`=1200 un "night boost" (`sample.night.max(f)`) + tira `zenith/horizon` hacia los valores de cielo airless. **Reutiliza la maquinaria día/noche** (no una rampa nueva): el domo Ghibli **envuelve la cámara** (sigue al jugador) así que es el cielo y el ClearColor nunca se ve → no se pelea con guards; el domo se oscurece y las estrellas aparecen, continuo y reversible al bajar. NO se reusó `launch_atmo` (su `blend` está invertido/atado al cohete). Banda a tunear en el Redmi.

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
