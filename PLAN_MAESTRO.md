# PLAN MAESTRO — Agencia Espacial Survival (2D)

> **Este es el ÚNICO plan vigente.** Consolida y REEMPLAZA a todos los
> `PLAN_*.md` anteriores (vuelo, cámara, zoom, disco texturado, transición,
> colisiones/ciudades, assets/construcción, sonido, editor de sonido, tests).
> Aquellos se borraron: lo que seguía pendiente de cada uno vive acá (§3), y lo
> ya hecho quedó registrado en §1 + `labiblia.md`.
>
> **Inspiración de diseño:** Space Flight Simulator (vuelo/planetas/escala),
> Terraria (superficie, minar/talar, construcción por piezas), Minecraft (loop
> de recolección→craft→construcción, sonido escaso y procedural, mundo por
> chunks). Todo 2D top-down 3/4. **Regla de oro: no reintroducir 3D** (biblia §1).
>
> **Contexto obligatorio antes de tocar código:** `labiblia.md` (maestro),
> `CLAUDE.md` (reglas repo), `ASSET_SYSTEM_ARCHITECTURE.md` (assets),
> `native/docs/SPACE_SURVIVAL_VISION.md` (visión de producto), y **graphify**
> (`graphify explain "Nodo"`, `GRAPH_REPORT.md`; `graphify update .` tras cambios).

---

## 0. Loop de juego (la brújula)

Tierra (survival Terraria: talar, minar, craftear, construir base) → **vuelo Saiyan
(Ki)**: cargás el aura, despegás y volás vos mismo (sin naves) con transición
continua superficie↔espacio → off-world (Luna, Marte, planetas) → colonos/base
lunar. El servidor Colyseus es autoritativo para el survival multiplayer; el
cliente nativo (Bevy) es el juego.

> **CAMBIO 2026-07-26 — se eliminó el programa espacial (naves).** Ya no hay ship
> builder, plataforma de lanzamiento, interior de nave ni física de cohete. En su
> lugar **el personaje vuela como Dragon Ball** (`PLAN_VUELO_SAIYAN.md`): mantené
> JUMP para cargar el aura y despegar, JUMP sube / USE baja, el stick mueve en
> órbita. El **Ki** reemplaza al combustible (se drena volando, se recarga
> cargando o en tierra). El universo, los viajes interplanetarios (nav computer /
> sondas), las efemérides y la coreografía de transición se **conservan** intactos.

---

## 1. Estado actual — HECHO y verificado (no rehacer)

- **Rewrite 2D completo**: side-scroller→top-down 3/4 (LimeZu). Render por
  vertex-color chunk meshes baratos + sprites de tile SOLO con arte vivo
  (`spawn_chunk_tile_art`, cero costo sin arte). Objetivo 60 fps Redmi.
- **Sistema de assets data-driven**: editor `ha-asset-tools` (egui standalone),
  slots lógicos (`ha-assets/slots.rs`), consumo por ID en `art2d.rs` +
  `manifest.json`. El pixel entra UNA vez por el editor; el código referencia IDs.
  Etiquetado de **capas** (`ArtLayer { Floor, Wall, Object, Roof }`).
- **Construcción**: piezas 1-tile (piso/pared/puerta/ventana) + **prefabs
  multi-tile** (Cuarto 5×4/7×6, Salón 9×8, Casa 9×7) con stamp de área y ghost
  verde/rojo. **Agnóstico al cuerpo.** Generación auto de casas/props SOLO Tierra
  (gated por tabla por-cuerpo). **Colisión al construir**: no se edifica sobre
  árbol/roca/mineral (hay que talar/picar primero; ghost rojo).
- **Edificios sin cortes**: entrás por la puerta, el **techo se hace translúcido**
  (`building.rs`, fade por proximidad), misma escena/cámara. Puertas animadas
  (open/closed por proximidad).
- **Cosecha real**: talar árbol / picar roca / minar mineral **los hace
  desaparecer al instante** (despawn de entidad) y persiste el borrado por cuerpo
  (`harvested.json`); donde talaste podés caminar. Drops (madera, sticks, manzana).
- **Vuelo Saiyan (Ki) + transición superficie↔espacio** unificada por cuerpo
  (`ha-shared/transition.rs` + `crate::transition`): el **personaje** vuela
  (`creative_fly.rs`, fases Ground→Climb→Orbit→Descend), no una nave. Una sola
  coreografía (nubes/fundido/swap de marco/zoom) en todos los cuerpos.
  Despegue/aterrizaje libres (sin zonas fijas). Cap de velocidad orbital
  (~5500 m/s → Tierra→Luna ~10 s). Disco planetario **texturado** que crece al
  acercarse (SFS). Anillos de órbita + starfield. **Aura dorada** procedural que
  flamea al cargar/volar (`PlayerAura`, slot `fx/aura_saiyan`). Barra de **Ki** en
  el HUD. El recurso `crate::flight::Flight` es el espejo pasivo del estado de
  vuelo que leen cámara/sim/audio/HUD/persistencia. *(Cohete/ship builder/pad/
  interior de nave ELIMINADOS — ver §0 y `PLAN_VUELO_SAIYAN.md`.)*
- **Cámara (dirección VIGENTE, ver §2)**: ascenso/descenso en superficie
  auto-zoom (el terreno retrocede, personaje centrado a tamaño constante);
  órbita/espacio = zoom 100% del jugador (pinch) y la cámara **sigue al personaje**.
- **Sonido**: procedural 8/16-bit (`audio.rs` + crate `ha-audio` + `audio.json`
  editable + mini-DAW `./build-y-publicar.sh sonido`). Música escasa estilo C418.
- **Menús con scroll**: las 4 secciones de la bolsa (INVENTORY/CRAFT/CONSTRUIR/
  SHIP) hacen scroll con dedo/rueda.
- **UI ASCII-only**: saneados todos los strings renderizados (antes salían cajas
  □ por acentos/flechas/·). Regla: el texto del juego es ASCII (biblia §7); el
  editor egui sí acepta unicode.
- **QA drivers** (`debug/autodrive.rs`, skill `swarm-sim`): `HA_FLYTEST`,
  `HA_MOONTEST`, `HA_ROCKETTEST`, `HA_CYCLETEST`, `HA_AUTODRIVE` (smoke
  dig/place/build) — cierran ciclos con invariantes. Todos verdes.
- **Colonos / base lunar**, nav computer (probes/autopilot), survival
  (hambre/sed/o2), z-levels de cuevas, persistencia (tiles/inventory/roofs/…).

---

## 2. Cámara y vuelo — DIRECCIÓN VIGENTE (corrige planes viejos)

**REVERSIÓN 2026-07-26 (vuelo Saiyan):** el usuario **odiaba** el auto-zoom de
superficie ("se aleja sin motivo del personaje al entrar en vuelo y me bloquea el
pellizco"). Regla nueva y única:

- **Zoom 100% del jugador (pinch/scroll) en TODAS las fases de vuelo**
  (Climb/Orbit/Descend) y a pie. El juego **NUNCA** aleja la cámara solo. La
  cámara **siempre** sigue al personaje (`fly.pos` en órbita, el walker si no).
- Se **borró** el auto-zoom (`zoom.view_h = trans.view_h` en `follow_player_camera`)
  y el **bloqueo de pinch** por `trans.in_surface_frame()` (en `camera_zoom_input`).
  Modo `flying_view = fly.flying() || Space` → clamps amplios (`ZOOM_MIN_H..MAX_H`).
- Al entrar a vuelo NO se fuerza un zoom default: se mantiene el actual; pellizcás
  para ver más. El sprite crece con `view_h*0.006` (órbita) para seguir visible.
- **El "subir hacia el espacio"** ahora lo dan la **cortina de nubes/fundido** de la
  transición (por altitud), NO el zoom. La transición sigue intacta (no retunear
  bandas — memoria `atmosphere-wide-proportions`).

**NO reintroducir** `flight_lift`, auto-reveal de zoom, ni el auto-zoom de
superficie. (Memoria: `camera-zoom-surface-vs-orbit-split`, actualizada.)

---

## 3. PENDIENTE — staged por valor y riesgo

> Convención: 🟢 tractable/bajo riesgo · 🟡 medio · 🔴 grande/multi-sesión o
> gated por decisión de diseño (🛑 = requiere confirmación del usuario antes de
> ejecutar, porque cambia diseño o el marco de mundo).

### A. Colisión de "huella" (footprint) + assets grandes + ciudades 🟡🔴
De `PLAN_COLISIONES_Y_CIUDADES`. Hoy la colisión de decor es de **1 celda**
(ancla). Falta:
- **A1 🟡 Huella multi-celda** (`footprint: {w,h,anchor,solid:[[dx,dz]…]}` en el
  manifest + sección "🗺 huella" en el editor + marcado de tiles sólidos al
  spawnear). Habilita montañas sólidas y assets grandes que bloquean su base
  (anillo sólido + centro libre para cráteres caminables). **Encaja con el fix de
  construcción de esta sesión** (ya hay `cell_build_blocked`): generalizar de
  "¿hay decor?" a "¿qué celdas del footprint son sólidas?".
- **A2 🟡 Prefab "asset grande"** (sprite 4×4 + footprint) colocable con hold,
  como una casa.
- **A3 🔴🛑 Hub terrestre fijo** cerca del spawn (dojo/templo de entrenamiento Ki
  + 1-2 NPCs + gancho de tutorial). Aporta ancla narrativa. Requiere decidir tutorial.
- **A4 🔴 Ruinas off-world** (Luna/Marte) con loot + lore ambiental. Reusa
  generador de asentamientos + prefabs.
- **A5 🔴 Heightfield** (relieves reales, bajar a cráteres) — es terreno, no
  sprite. Posponer hasta que haya assets que lo justifiquen.

### B. IA colmena de colonos, alimentada por el swarm 🟡🔴
De `PLAN_VUELO_ESPACIO_Y_TESTS §3`, **reencuadrado** (2026-07-25): el objetivo NO
es solo QA. Los colonos deben **navegar, caminar, conducir, hacer de todo como un
jugador más**. Como para testear YA vamos a poner bots haciendo exactamente eso, el
**mismo dato** (trayectorias/decisiones/outcomes) se guarda y se convierte en el
**conocimiento de la IA colmena** de los colonos — un cerebro compartido, no uno
por NPC. Fases:
- **B1 🟢 `HA_SUITE` + `bugreport.jsonl`**: los drivers actuales emiten una línea
  machine-readable por fallo (mecánica, semilla, estado, invariante roto, repro) +
  un runner que corre todos y agrega el reporte. La IA lo lee y resuelve uno por
  uno. Bajo riesgo (reusa drivers). **Doble uso:** primer formato del log de
  comportamiento.
- **B2 🟡 Un driver por mecánica** (cavar/colocar, craft, cuevas, puertas, vuelo Saiyan,
  colonos, survival, construcción, transiciones), miles de repeticiones, semillas
  variadas, invariantes por mecánica → cobertura + más datos.
- **B3 🔴 `HA_SWARM=N` headless**: N agentes por el MISMO
  `step_topdown_movement`/colisión con políticas simples (wander/seek/flee), sin
  render, a máxima velocidad → estrés + **dataset** de comportamiento a disco.
- **B4 🔴 IA colmena reutilizando la "IA frugal" del usuario**: el usuario tiene
  una **IA frugal** que va a pasar al proyecto; la idea es evaluar si se puede
  **reusar** como el cerebro colmena que consume el dataset de B1–B3 y decide qué
  hacen los colonos (navegar/conducir/trabajar). **BLOQUEADO** hasta que el usuario
  integre esa IA. (Antes de asumir librerías/ML nuevas, esperar su IA.)

### C. Verticalidad: subsuelos / 2º piso 🔴🛑 (importante)
De `PLAN_ASSETS_Y_CONSTRUCCION F6`. Escalera dentro del edificio = swap instantáneo
de `tiles.z` a una sala cerrada acotada al footprint (reusa infra de z-levels de
cuevas, sin cutscene). **No romper** `WorldContext`/`RegionId`/`tiles.z` ni las
transiciones superficie↔espacio/Luna. 🛑 confirmar enfoque antes de tocar el marco.

### D. Perf device + texturas del usuario 🟡 (bloqueado en hardware/usuario)
- **D1** Perfilar en el **Redmi** (memoria `pending-redmi-perf-tests`): heap,
  fps por región, tirones; que props/casas/techos/tiles con arte no bajen de 60
  fps. Bloqueado hasta que el celu esté conectado.
- **D2** Asignación de texturas a los slots (trabajo del usuario en el editor).

### E. Audio — extensiones 🟢
De `PLAN_SONIDO`. Slider de volumen en Ajustes; sonidos por bioma;
pájaros/viento ambientales; variación de pista día/noche; jingle de logro.

### F. Escala del sistema — solo pulido opcional 🟢
La escala actual **le gusta al usuario** (2026-07-25) — NO buscamos escala real
(el juego es **diversión, no realismo**; ver §4.10). Único trabajo posible y
opcional: **acoplar un poco mejor** lo que ya hay (distancias/tiempos de viaje que
"se sientan" bien con el cap de velocidad actual), sin agrandar el sistema ni tocar
efemérides/SOI. Nada de multiplicadores globales de escala. **Time-warp: descartado**
(el usuario lo eliminó; no reintroducir).

---

## 4. Reglas inviolables (romper una = regresión)

1. **NO 3D**, ni cámara FP, ni shaders custom (biblia §1/§3). Todo 2D top-down.
2. **60 fps Redmi manda** (biblia §6). Terreno = vertex-color meshes; sprites de
   tile solo con arte vivo (cero costo sin arte). Nada nuevo cuesta sin arte.
3. **Sim determinista** solo en `/shared` (TS), espejada en `ha-shared` (Rust). El
   servidor es autoritativo; nada de IPs hardcodeadas (`HA_SERVER_HOST`).
4. **UI del juego ASCII-only** (la fuente no renderiza acentos/flechas/·/emoji →
   cajas). El editor egui sí acepta unicode.
5. **Assets por ID lógico** (`slots.rs`); NUNCA generar PNGs ni inventar
   rects/offsets/pivotes — eso lo hace el usuario en el editor.
6. **No romper** `WorldContext`/`RegionId`/`tiles.z` ni las transiciones — consultá
   graphify antes; probá Tierra↔Luna↔espacio tras cambios de mundo.
7. **Construcción agnóstica al cuerpo**; solo la GENERACIÓN auto es Tierra-only
   (gated por tabla).
8. **Cámara**: respetar el split superficie-auto / órbita-pinch de §2. No
   reintroducir `flight_lift` ni auto-reveal en espacio.
9. **Cambios quirúrgicos**: no tocar lo que ya funciona (física, perfiles de
   transición, aterrizaje libre) salvo que la tarea lo pida.
10. **DIVERSIÓN > realismo** (2026-07-25). El enfoque es que sea divertido, no
    simular la realidad. La escala del sistema está bien como está; **NO** buscar
    escala real ni time-warp (descartados). Ante una duda "¿realista o divertido?",
    gana divertido.

---

## 5. Verificación (comandos)

```bash
cd native
cargo check -p ha-game && cargo check -p ha-asset-tools   # rápido, sin OOM
cargo test -p ha-shared                                    # sim (101 tests)
cargo test -p ha-assets                                    # slots + manifest
# Drivers (desktop headed, capturas + invariantes):
cargo build -q -p ha-game
timeout 80 env HA_FLYTEST=1  HA_AUTOPLAY=1 WGPU_BACKEND=gl ./target/debug/ha-game
timeout 80 env HA_MOONTEST=1 HA_AUTOPLAY=1 WGPU_BACKEND=gl ./target/debug/ha-game
timeout 90 env HA_CYCLETEST=1 HA_AUTOPLAY=1 WGPU_BACKEND=gl ./target/debug/ha-game
timeout 120 env HA_AUTODRIVE=1 HA_AUTOPLAY=1 WGPU_BACKEND=gl ./target/debug/ha-game  # smoke
# Publicar APK (build + publish, sin servidor bloqueante):
npm run ship:android -- --skip-tests
# Tras cambios: refrescar grafo (sin costo API):
graphify update . --force
```
Si "No space left": `rm -rf native/target/debug/incremental` (libera ~2.5 GB sin
forzar recompilación de deps).

---

## 6. Checkpoints con el usuario (🛑 — no saltarlos)

Antes de EJECUTAR estos, presentar diseño concreto (listas/patrones/diagramas
ASCII) y esperar OK: **A3** (hub/tutorial), **C** (verticalidad — toca el marco de
mundo), **B4** (IA colmena — además depende de la IA frugal del usuario). El resto
(A1, A2, A4, B1, B2, E) puede ejecutarse por fases verificando compila+tests,
actualizando `labiblia.md` §13 y `graphify update .` al terminar cada una.

---

## 7. Orden de ataque por FASES (decisiones del usuario 2026-07-25)

- **FASE 1 — A1 huella de colisión multi-celda** ✅ *(HECHO 2026-07-25)*. Modelo
  `CollisionFootprint {w,h,ax,az,solid}` en el manifest (`ha-assets`, opcional/
  retro-compat, con tests) + colisión multi-celda en el juego (`world2d::
  decor_blocks_walk`, escaneo de vecindad SOLO si hay huellas → cero costo hasta
  pintar una) + `ArtLibrary::collision_footprint`/`has_footprints` + **editor**:
  sección "🗺 huella" (grilla cenital, clic=sólido, clic-der=ancla). El usuario
  pinta la huella en el editor. Habilita A2/A4.
- **FASE 2 — A2 assets grandes** ✅ *(HECHO 2026-07-25)*. Catálogo
  `LARGE_ASSETS` (ha-shared) + slots `structure/*` (editor) + registro
  persistente `BuiltStructures` (`structures.rs`: `structures.json`, streaming de
  sprites Y-sorted al suelo, colisión de base `w×h`) + paleta CONSTRUIR + ghost
  verde/rojo + colocación (no se apilan ni van sobre árbol/sólido) + colisión del
  jugador (`sim.rs`). El usuario asigna el pixel a `structure/boulder|statue|
  monument` y pinta la huella fina (A1). *Refinamiento futuro:* footprint
  per-celda para estructuras (hoy bloquean la base completa).
- **FASE 3 — B1 `HA_SUITE`/`bugreport.jsonl`** ✅ *(HECHO 2026-07-25)*.
  `autodrive::report_bug(mechanic, invariant, detail)` anexa una línea JSON a
  `HA_BUGREPORT` (default `bugreport.jsonl`), cableado en los invariantes de
  vuelo/luna/aterrizaje. Runner `scripts/ha-suite.mjs` (`npm run qa:suite`) corre
  todos los drivers, agrega el reporte y lo resume → la IA lo lee y resuelve.
  Doble uso: primer formato del log de comportamiento (semilla de B3/B4).
- **FASE 4 — E audio** ✅ *(HECHO 2026-07-25)*. Filas **SFX** y **Music** en el
  panel de AJUSTES (−/+, 0..100 %, en vivo) → `ui_editor` mueve `AudioSettings`.
- **FASE 5 — H EDITOR DE MUNDO** *(APROBADO — ver `TEORIA_EDITOR_MUNDO.md`)*. La
  herramienta data-driven para autorar el mundo (overrides + regiones + plantillas
  de ciudad + capas). **Subsume A3/A4** (el hub y las ruinas se PINTAN acá, no se
  codean). Sub-fases:
  - **H0** ✅ *(HECHO 2026-07-25)* `world_edits.json` (modelo `WorldEdits`:
    `tiles` + `structures`) + loader `world_edits.rs` (aplica overrides sobre la
    gen en boot; embebido `include_str!`; fallback: sin archivo = 100% procedural).
    `Tile` ahora serializa. Tests en ha-shared.
  - **H1** ✅ *(HECHO 2026-07-25)* shell del editor: subcomando `mundo`
    (`./build-y-publicar.sh mundo`) — canvas WYSIWYG que rasteriza el mundo
    procedural real (`topdown_tile`→`Tile::color`) con pan/zoom + selector de
    cuerpo/z + readout de celda. Read-only (pinceles = H2).
  - **H2** ✅ *(HECHO 2026-07-25)* pinceles en el canvas: **Mover/zoom**, **Pintar
    tile** (paleta curada; ej. cascada entre lagos), **Colocar asset** grande
    (LARGE_ASSETS, no se apila), **Borrar** (vuelve a lo procedural). Guarda en
    `world_edits.json` (verifica releyendo). Tipos movidos a `ha_shared::world_edits`
    (compartidos juego↔editor). *(Pendiente H2.1: pinceles de edificios/prefabs +
    mover/reemplazar estructura; leer huella del manifest al colocar.)*
  - **H3** regiones + **plantillas de ciudad** (autorás el layout una vez; el código
    la ubica donde encaja, calles que conectan) + spawns/POIs + marcadores.
  - **H4** **capas por asset** + **día/noche** + techo oculto/colisión exacta.
    - *Parcial ✅ 2026-07-25:* `ArtLayer` extendido con **`Basement`** (subsuelo) y
      **`Light`** (luces); selector en el editor de assets (Suelo/Subsuelo/Pared/
      Objeto/Techo/Luces) por pose; el juego resuelve poses `basement`/`light`.
      Colisión exacta pintada sobre el sprite ✅ (visible por defecto).
    - *RETENCIÓN VISUAL:* **ciclo día/noche** (ya existía: `DayClock` avanza +
      overlay oscuro + estrellas). **Luciérnagas ✅** *(2026-07-25)* — módulo
      `ambient.rs`: puntos cálidos que aparecen de noche, derivan y pulsan cerca
      del jugador (sobre superficie). **Arte de fauna ✅** slot `npc/fauna`.
    - *Pendiente:* render **aditivo** de la capa `Light` (una casa ilumina su
      entorno de noche) — necesita assets con pose `light`; render del `Basement`.
  - **H5** pulido: minimapa, undo/redo, copy/paste de stamps, hot-reload.
- **FASE 6 — C verticalidad** subsuelos/2º piso (🛑, toca el marco de mundo; se
  apoya en las capas H4).
- **FASE 7 — B2/B3 drivers por mecánica + swarm N-agentes** (dataset).
- **FASE 8 — B4 IA colmena** reusando la **IA frugal del usuario** (HIVE_OS/IAFZ-64,
  Deep BNN binaria; ver `ANALISIS_IA_FRUGAL.md`). **DESBLOQUEADA** (el usuario ya la
  pasó a `iagrufal-main/`, gitignorada).
  - **B4.0** ✅ *(HECHO 2026-07-25)* crate **`iafz-sys`**: FFI Rust↔C del kernel
    (`cc` compila `iafz_64.c`, cross-compila para Android vía cargo-ndk), bindings
    `init/think/load/save/mutate`, genoma entrenado `VENCEDOR_64.adn` vendorizado.
    Smoke test: `think` determinista + carga de genoma ✓.
  - **B4.1b GYM** ✅ *(HECHO 2026-07-25)* — **neuroevolución** (`ha-asset-tools gym`,
    `./editor_assets gym`): evoluciona OFFLINE un genoma que FORRAJEA en el mundo
    REAL (`topdown_tile`), evita obstáculos y es **bioma-consciente** (el bioma es
    sensor). Selección + mutación/crossover (iafz-sys). **Fitness 347 → 1452** en 160
    gens (≈14 comidas) → aprendizaje real. Guarda `genomes/FAUNA.adn`.
  - **B4.1+B4.2** ✅ — módulo **`fauna.rs`**: ecosistema con **EVOLUCIÓN EN EL MUNDO**
    (no conducta congelada): los animales **comen, se reproducen (heredan el genoma
    mutado) y mueren** según el mundo real → **selección natural in-game = aprenden
    por su cuenta**. Sensor COMPARTIDO gym↔juego (`ha_shared::fauna_brain`) con
    **percepción SOCIAL** (vecinos) → pueden emerger grupos/territorio/conflicto **sin
    enseñárselos**. **NO se poda la conducta emergente** (memoria
    `ai-life-emergence-no-prune`): si surgen sociedades/guerras, bienvenidas. El gym
    SIEMBRA competencia base (forrajeo). *Pendiente:* arte `npc/fauna`; gym MULTI-agente
    (para sembrar también lo social); recursos/territorio para más emergencia; por especie.
  - **B4.3** generalizar a colonos/enemigos (un genoma por rol). Telemetría del swarm
    calibra utilidades. La psicología (metas) se diseña a mano (más control de retención).
  - **B4.4 — COMUNICACIÓN NPC por STIGMERGIA** (diseño 2026-07-25, "sorpréndeme"):
    campo de señales tipo feromona (grid disperso que DECAE) — un animal deposita
    `alarma/comida/territorio/apareamiento` en su celda; los demás sienten el gradiente
    (bits nuevos del sensor). Así emergen manadas, huidas colectivas, territorios y
    **conflictos** SIN scriptear ("hacé una sociedad"): se da el canal, la evolución en
    el mundo decide. El **jugador se comunica** interactuando con el campo (deja comida
    = atrae; amenaza = alarma). Estructura: `Res<SignalField(HashMap<(i32,i32),
    [f32;4]>)>` con decaimiento; depósito + sensado en `fauna_brain`. *Pendiente de
    implementar.*
  - **B4.5** gym MULTI-agente (sembrar conducta social offline, no solo forrajeo).
- **PERF/BATERÍA** ✅ *(2026-07-25)*: fauna throttle a 15 Hz (4× menos cómputo/FFI);
  en segundo plano `Time<Virtual>` pausado + winit estacionado (reactive_low_power
  3600s) = casi cero draws. Sin ciclos infinitos.
- **BUG streaming lejos del origen** *(parcial ✅ 2026-07-25)*: `pack_cell` truncaba a
  ±32 km (i16) → la decoración desaparecía lejos por colisión con el removed-set;
  ahora usa hash completo. *Pendiente:* a distancias ENORMES, precisión f32 de los
  transforms (problema de **origen flotante**) — rebasar el mundo alrededor del jugador.
- **EDITOR DE MUNDO H3+ (pedido 2026-07-25):** sliders para esculpir el suelo ya
  definido de cada planeta (más/menos terreno, forma), cargar TODOS los assets para
  colocar cascadas/montañas/etc., y que el juego RELLENE lo procedural alrededor.
- **Descartado:** time-warp y escala real (§F). **Diversión > realismo.**

---

## 8. BACKLOG NUEVO (2026-07-25) — pulido, mundo vivo y controles

> Ideas del usuario + complementos (game-design). NO reemplaza lo de arriba; es la
> cola de trabajo a ejecutar/verificar. El usuario hará un recorrido para confirmar
> que TODO quede completo y sin bugs. Prioridad: que el mundo **se sienta vivo** por
> bioma/planeta, y controles fluidos.

### N1 — MUNDO VIVO: fauna por bioma con radio de streaming 🟢 (prioridad)
El usuario "perdió de vista" los animales: hoy se spawnean UNA vez cerca del spawn y
se quedan atrás. Diseño:
- Mantener una **población dentro de un RADIO** del jugador (medio-grande, ~cubre lo
  visible). Los que se alejan del radio se **reubican** al borde opuesto (streaming) →
  siempre hay vida alrededor, población **acotada** (no drena FPS/batería). La comida
  igual. Reusa el throttle 15 Hz + pausa en background que ya hay.
- **Densidad + especies POR BIOMA/planeta** (bosque vs desierto vs luna): tabla que a
  futuro se **define en el editor de mundo** (por eso el editor debe editar hasta la
  VIDA que se genera). Reaccionan al **día/noche** (ya existe) y a eventos atmosféricos
  (futuro). Todo se **persiste** lo que deba.
- Complemento: eventos ambientales ligeros (bandadas al alba, depredador que aparece de
  noche) — barato, refuerza "vivo".

### N2 — EDITOR DE MUNDO dentro del EDITOR DE ASSETS (UX / Material) 🟡
Integrar el editor de mundo como **pestaña** del mismo binario `ha-asset-tools`
(`Assets | Sonido | Mundo`), **compartiendo el catálogo de assets ya cargado** para
colocar props/cascadas/montañas con su ARTE real (no solo colores de tile). UI intuitiva
estilo Material (toolbar, iconos, tooltips). Complementos: **undo/redo, minimapa,
copiar/pegar stamps, hotkeys**. Si técnicamente no conviene fusionar (dos apps eframe),
al menos unificar la lista de assets y el look. Ya tiene: pincel con tamaño, rellenar
rect, bote por terreno, cuentagotas, colocar/borrar.

### N3 — EDITOR DE MUNDO: sliders de GENERACIÓN base 🟡
Esculpir el suelo BASE de cada planeta con **sliders** (cantidad de terreno, umbrales de
bioma, agua, montañas) → exige exponer parámetros de `topdown_tile` (semilla + factores
por-cuerpo) y regenerar/preview. Distinto de pintar overrides (que ya está). Que el juego
**rellene lo procedural** alrededor de lo autorado.

### N4 — ESCALAS de assets 🟢
Poder editar la ESCALA de cada asset (en el editor de assets / manifest) y que el juego
la respete (hoy el tamaño es fijo por código en varios lugares).

### N5 — JOYSTICK DERECHO: fluidez + contextual 🟡 (prioridad de feel)
- **Fluidez/precisión:** el usuario siente **desconexión/retraso** — la "crucecita" se
  mueve DENTRO del tile y luego salta de tile (apuntado cuantizado a celda). Fix: apuntado
  **CONTINUO** (posición libre, sin snap a tile) → velocidad + precisión (correr y pulsar
  un botón sin lag). Revisar `targeting.rs`/`AimCursor`.
  🟢 *IMPLEMENTADO (pendiente test Redmi):* `update_aim_cursor` pasó de integración de
  velocidad (trackball: el retículo iba a la deriva y **se quedaba atrás** al correr, sólo
  saltaba al chocar el clamp → sensación de cuantizado) a **mapeo directo/absoluto**: el
  offset retículo↔jugador mapea 1:1 con la deflexión del stick y queda anclado al jugador
  (acompaña al correr), con lerp leve (`AIM_FOLLOW_RATE=30`) anti-jitter. Persistencia al
  soltar (para construir) intacta. El `PointerReticle` continuo (world2d) ya dibujaba la
  posición exacta; ahora el INPUT también es continuo. Compila verde.
- **Contextual por item en mano:** arma → el joystick pasa de cuadradito a **barra que
  apunta** y las balas salen en esa dirección; **vuelo Ki** → JUMP sube / USE baja y el
  stick mueve en órbita (ya implementado en `creative_fly.rs`). El stick IZQUIERDO se
  deja como está (gusta).
- **Slider de cámara (zoom) vertical** arriba del joystick derecho, para reemplazar el
  pinch (a EVALUAR UX — el usuario duda; complementar: quizá un slider fino que no estorbe,
  o mantener pinch + slider opcional).

### N6 — ORIGEN FLOTANTE 🔴
A distancias ENORMES (millones de unidades) la precisión f32 de los transforms degrada
(texturas raras). Rebase del mundo alrededor del jugador. Grande, para cuando moleste.

### N7 — VIVARIO: spawn creativo + vida emergente persistente 🟡
Spawnear fauna (luego colonos/NPCs) en creativo para **encerrarla y observar cómo
evoluciona** su conducta cuanto más vive; gyms enseñan las BASES, de ahí emergencia, con
salvaguardas anti mínimos/máximos locales. La fauna que importa PERSISTE como datos en
todos lados y se renderiza sólo cerca (o cuando una cámara la enfoca); ecosistemas por
planeta. Diseño + fases en `PLAN_VIVARIO_VIDA_EMERGENTE.md`. **F1 y F2 hechos** (residente
confinable + evolución + salvaguardas baratas; persistencia por cuerpo + activación por
proximidad; compila verde, pendiente Redmi). Falta F3 salvaguardas completas, F4 overlay,
F5 fauna por planeta, F6 colonos, F7 máquina cámara, F8 herramientas de ecosistema.

### N8 — HERRAMIENTAS REMOTAS (web sobre Tailscale) 🟡
Interfaz web ÚNICA (assets + mundo + música) accesible por Tailscale desde el trabajo,
editando los mismos archivos data-driven (`manifest`, `world_edits.json`, `audio.json`).
Diseño + fases (FR1-FR5) en `PLAN_HERRAMIENTAS_REMOTAS.md`. Sin empezar.

### Reglas de trabajo (usuario 2026-07-25)
No gastar tokens en tareas poco necesarias; **siempre testear** (borrar cachés para
liberar GB y re-testear si hace falta) → **cero bugs**. Ejecutar por valor. **No podar la
conducta emergente** (ver [[ai-life-emergence-no-prune]]).

---

## 9. ESTADO Y PUNTO DE RETOME (2026-07-25, antes de /clear)

**Último APK publicado: 1.0.206** (server sirviendo en `192.168.1.5:5173`/`:2567`).

### Hecho y en el APK (FASE 8 / fauna, además de §1)
- **Ecosistema evolutivo**: fauna come/se reproduce (hereda genoma mutado)/muere →
  selección natural in-game. Genoma colmena `iafz-sys` (IAFZ-64, FFI a C, cross-compila
  aarch64). Gym `./editor_assets gym` (neuroevolución) + **gym multiagente** (K=5, evoluciona
  lo social). **Estigmergia** (campo de feromonas, `SignalField`) → alarma/presencia →
  emergencia (manadas/huidas) sin scriptear. **NO podar lo emergente**.
- **Mundo vivo**: streaming de fauna en radio ~42 m alrededor del jugador (los que se
  alejan se reubican) + comida; población acotada; throttle 15 Hz + pausa en background.
- **Compatible pack LimeZu Modern Farm**: 5 especies (`npc/fauna_cow/chicken/sheep/pig/dog`),
  animación direccional (idle/walk/eat/drink/run; perro: bark) via `apply_pose` (réplica de
  `face_player_sprite`). Acciones: eat (comida), drink (Tile::Water + sed), run (huida por
  alarma), bark+relay de alarma (perro). Fallback a quad de color sin arte.
- **Editor de mundo** (`./editor_assets mundo`): canvas WYSIWYG + pincel con tamaño, rellenar
  rectángulo, bote (flood-fill por terreno), cuentagotas, colocar/borrar → `world_edits.json`.
- **Retención visual**: luciérnagas de noche (`ambient.rs`) + día/noche (ya existía).
- **Fix bugs**: crash B0001 de luciérnagas (Without<Player>); `pack_cell` ±32km → hash.

### PENDIENTE (detallado; ver §3 y §8 para el resto)
- **GYM — entrenar buscar-agua + huir (depredador)**: hoy beber/huir son OPORTUNISTAS
  (beben si pasan cerca del agua; huyen si sienten alarma), NO gym-optimizados. Falta sumar
  **agua + amenaza al SENSOR** (`ha_shared::fauna_brain::sense`: dir-a-agua + sed + dir-a-amenaza)
  y a la **arena del gym** (scatter agua, un depredador que persigue) + reentrenar `FAUNA.adn`.
  Es la otra mitad de "entrenar todas las acciones del pack".
- **Escalas de assets** (§8 N4): editar la escala por asset y que el juego la respete.
- **Editor de mundo N2/N3**: fusionarlo como pestaña del editor de assets (compartir catálogo,
  UX Material) + sliders de generación base (esculpir el suelo del planeta) + cargar TODOS los
  assets para colocar cascadas/montañas + densidad de fauna POR BIOMA definida acá.
- **N6 origen flotante**: texturas a distancias enormes (rebase del mundo).
- Todo lo de §3 (huella multi-celda A1 hecho, A2 hecho; A3 hub, A4 ruinas, verticalidad C, etc.).

### >>> RETOMAR ACÁ DESPUÉS DEL /clear <<<
**PRÓXIMA TAREA: §8 N5 — Joystick DERECHO, fluidez.** El usuario siente
desconexión/retraso: el apuntado se **cuantiza a la celda** (cuadradito del tile + crucecita
que se mueve dentro y luego SALTA de tile). Fix: apuntado **CONTINUO** (posición libre, sin
snap a tile) para velocidad+precisión (correr y pulsar un botón sin lag). Revisar
`crate::input::AimCursor` + `targeting.rs` (`update_look_target`, `cursor.world`, `render_xy`)
y el HUD del retículo/cursor (`hud.rs`, `input/joystick.rs`, `targeting.rs`). El stick
IZQUIERDO se deja como está (gusta). Después: contextual por item (arma→barra que apunta;
vuelo Ki ya usa JUMP/USE+stick) y evaluar slider de cámara vertical.

> **NOTA 2026-07-26:** antes de N5 se ejecutó la **conversión a vuelo Saiyan**
> (`PLAN_VUELO_SAIYAN.md`): programa espacial eliminado, personaje vuela con Ki +
> aura. Compila verde, tests ha-shared verdes (104). Pendiente: validar drivers de
> vuelo en desktop y 60 fps en Redmi.
>
> **NOTA 2026-07-26 (sesión remote-control) — REFRAME SANDBOX + PURGA VISIBLE:**
> el juego deja de presentarse como "agencia espacial": es una **caja de arena de
> role-play** donde el personaje **vuela con Ki** (estilo Saiyan) entre Tierra y
> planetas. El universo/órbitas/transiciones/sondas quedan intactos; lo que se
> purgó es lo que el JUGADOR veía del viejo marco:
> - Boot: título `STARSHIP` → **`KI`**, subtítulo `AGENCIA ESPACIAL` → **`MUNDO
>   ABIERTO`**; "toca para lanzar la mision" → "toca para jugar" (`boot_ui.rs`).
> - Título de ventana `Starship` → **`Ki - Mundo Abierto`** (`lib.rs`).
> - Leyenda del mapa orbital "+ tu nave" → "+ tu" (`game_menu.rs`).
> - Prompts lunares/orbitales de "ACTION thrust/quemar" → **"JUMP para cargar Ki y
>   despegar / vuela con Ki"** (`hud.rs`).
> - La **antena** (uplink base remota) y la **cámara/sonda** (`camera_feed.rs`,
>   `nav_computer`) se conservan como **gadgets** del sandbox — NO son "agencia".
> - Botón **CREATIVO** conservado: es el toggle de spawn creativo/vivario, no nave.
> **Restos internos NO purgados a propósito** (invisibles al jugador, son el espejo
> determinista del server — tocarlos forkea `/shared` TS↔`ha-shared` y arriesga la
> sim; rule 9): `ha-shared/{launch_pad.rs,ship_mass.rs,vessel::PlayerShip}` y las
> variantes muertas `InteractActionKind::{Assemble,Launch}` /
> `InteractTargetKind::{LaunchModule,LaunchIgnition,SpaceStationModule}` (sus
> entidades ya NO se spawnean → código inalcanzable, sin efecto). Depurarlos es una
> tarea aparte que debe hacerse en TS y Rust a la vez.
> **NOTA 2026-07-26 (remote-control, parte 2) — cámara espacio, órbitas, compañero:**
> - **Cámara en el espacio = la de la Tierra** (pedido del usuario): se QUITÓ el
>   auto-zoom por altitud; `follow_player_camera`/`camera_zoom_input` usan el
>   `view_h` ABSOLUTO del usuario (pinch/scroll) en todas las fases, clamp amplio en
>   vuelo. El juego nunca aleja solo (el planeta queda fuera de cuadro hasta que
>   pellizcás). Esto ACOPLABA la velocidad orbital (`creative_fly_orbit` la sacaba de
>   `view_h`): se DESACOPLÓ → ahora escala con la ALTITUD (`ORBIT_SPEED_PER_ALT`, cap
>   5500). Zona de flip-flop: ver memoria `camera-zoom-surface-vs-orbit-split`.
> - **La Tierra ORBITA el Sol** (`earth_center_at`→heliocéntrico; en t=0 idéntico al
>   catálogo → despegue continuo intacto). **Mercurio y Venus reordenados** (estaban
>   más lejos que la Tierra → ahora entre Sol y Tierra). `ha-shared` 104 tests verde.
> - **Compañero-IA "mano derecha"** (`companion.rs`, `PLAN_COMPANERO_IA.md`): IAFZ-64
>   sigue al jugador (jugador=comida del genoma base FAUNA.adn) y **evoluciona solo**
>   (hill-climb + persist `vivarium/companion.adn`). El **tester-IA** es su maestro.
> - **Tester continuo** (`HA_TESTER`) + compañero corriendo en desktop; ciclo de
>   vuelo despegue→órbita→descenso→touchdown VERDE tras el fix de velocidad, 0 panics.
>
> **Drivers de vuelo Saiyan en desktop VERDES** (esta sesión): `HA_FLYTEST` (Tierra:
> despegue Ki→órbita→abort mid-descent→aterrizaje, ZOOM CONTINUITY + LANDING
> INVARIANTS PASS, EXIT 0) y `HA_MOONTEST` (órbita lunar CLARA, disco FAR/NEAR OK,
> EXIT 0). Nombre de log `ha-rocket:` → **`ha-ki:`**. Pendiente sigue siendo 60 fps
> en el Redmi.
