# La Biblia — Ki · Mundo Abierto

> **Documento único del proyecto.** Consolida y reemplaza a los ~30 `.md` que
> existían antes (planes, migraciones, auditorías, manuales de editores, notas de
> rendimiento). Todo lo que sigue es **lo vigente**; lo histórico se descartó a
> propósito, salvo el cementerio de §14, que existe para que nadie reintroduzca lo
> que ya se quitó. **Si esto contradice al código, gana el código** — pero avisá
> para actualizar el documento.
>
> Última consolidación: **2026-07-31**.

---

## §1 Identidad y dirección

| Campo | Valor |
|---|---|
| **Producto** | **Ki · Mundo Abierto** — sandbox de role-play. Repo interno `hunger-arena`; package Android `com.hungerarena.app`. |
| **Género** | Caja de arena: el juego no cuenta una historia, da herramientas. Superficie top-down 3/4 (minar, talar, construir, sociedad) + **vuelo Saiyan (Ki)** para salir del planeta y llegar a otros. |
| **Referencias** | **Top-Down 3/4** (Stardew / assets LimeZu Modern) en superficie · **Space Flight Simulator** para vuelo y planetas · Terraria/Minecraft para el loop de recolectar→craftear→construir. **Cero física orbital real**: siempre arcade. |
| **Home base** | Tierra, seed **42**. |
| **Plataformas** | APK Android arm64 · AppImage Linux · landing web de descarga · servidor Colyseus. |
| **Stack** | Rust/**Bevy 0.15 en 2D** (Sprites + Mesh2d, sin pipeline 3D) · Colyseus + Postgres/Drizzle · npm + Cargo workspaces. |
| **Estética** | 2D top-down 3/4 con assets LimeZu Modern; fallback procedural donde falte pixel. UI del juego ASCII-only. |
| **Versión** | `native/version.json` — nunca hardcodear. |
| **Objetivo de perf** | **60 FPS sin tirones en el Redmi**, conservando todo el contenido. |

**El loop:** explorar → recolectar/minar/talar → craftear y construir → encontrar
pueblos y convivir con ellos → cargar Ki y volar a otro cuerpo → repetir allá.

---

## §2 Reglas inviolables

Romper una de éstas es una regresión, no una decisión de diseño.

1. **NO 3D.** Ni cámara en primera persona, ni esferas caminables, ni shaders
   custom de iluminación 3D, ni toon/outline, ni domo de cielo. Se borró a
   propósito: costaba 80–180 ms de GPU por frame en el Redmi.
2. **60 fps del Redmi manda.** El terreno base son mallas de vertex-color; los
   sprites de tile sólo existen cuando hay arte asignado (**cero costo sin arte**).
   Nada nuevo debe costar antes de que el usuario le ponga pixel.
3. **La simulación determinista vive en `packages/shared` (TS)** y se espeja
   numéricamente en `ha-shared` (Rust). Nunca se bifurcan las reglas. Cambio de
   catálogo → `UPDATE_CATALOG_PARITY=1 npm test`.
4. **El servidor es autoritativo**; los clientes mandan `InputState`, nunca
   posiciones ni HP. Zod en las fronteras HTTP/WS.
5. **Sin IPs hardcodeadas** — `VITE_SERVER_HOST`, `HA_SERVER_HOST`,
   `UPDATE_BASE_URL`.
6. **`client/` es sólo la landing** de descarga. No hay motor de juego ahí.
7. **UI del juego ASCII-only.** La fuente por defecto no renderiza `·`, `—`,
   flechas, acentos ni emoji: salen cajas `□`. (El editor egui sí acepta unicode.)
8. **Los assets se referencian por ID lógico** (`slots.rs`). **Nunca generes PNGs
   ni inventes rects, offsets o pivotes**: eso lo pinta el usuario en el editor y
   vive en `art/manifest.json`.
9. **Una transición es un cambio de marco de referencia, jamás una cutscene.**
   No bloquea input ni cámara, no aplica patadas de velocidad, no tapa la pantalla.
10. **No romper** `WorldContext` / `RegionId` / `tiles.z` ni las transiciones
    superficie↔espacio. Consultá el grafo (`graphify`) antes de tocarlos y probá
    Tierra↔Luna↔espacio después.
11. **Construcción agnóstica al cuerpo**; sólo la *generación automática* de
    asentamientos es Tierra-only (gated por tabla).
12. **No se poda la conducta emergente.** Si de la evolución salen manadas,
    territorios, sociedades o guerras, se conservan.

### Guards del cliente nativo (obligatorios en sistemas nuevos)

- **B1** Queries mutables conflictivas (Bevy B0001): `Transform` / `Node` /
  `BackgroundColor` en el mismo sistema → `ParamSet` o queries disjuntas.
- **B3** Singletons (mapa, overlay de cielo): verificar la query antes de spawnear.
- **B4** Sistemas por contexto: primera línea `if ctx != Expected { return; }`.
- **B5** Paneles de UI: spawn/despawn o `Visibility`, nunca `Display::None`
  acumulativo.
- **B6** Al salir de una región, despawn del decor (histéresis de 2 s). Las
  entidades de un cuerpo no viven en otro.
- **B9** Sin allocs en el hot path (`Update`/`FixedUpdate`) sin capacidad
  pre-reservada.
- **B10** Persistencias con flush síncrono al pausar (`nativeSetPaused`) y en las
  transiciones de vuelo.

Validar la arquitectura: `npm run validate:arch`.

---

## §3 Arquitectura

```
packages/shared/   Sim determinista TS (isla 96×96) — verdad del servidor: GameSimulation,
                   catálogos, survival, colony, zod. Tests vitest.
server/            Colyseus SurvivalRoom + auth + Drizzle/Postgres + /updates (OTA).
client/            Landing ONLY — panel de descarga del APK/AppImage. Sin motor.
native/
  crates/ha-shared/   Puerto Rust de las reglas + geografía, ecología, relieve,
                      cuerpos celestes, efemérides, build_grid, fauna_brain.
  crates/ha-assets/   Modelo del manifest de arte (`art_manifest`) + catálogo de
                      slots (`slots.rs`) + huellas/capas/variantes.
  crates/ha-audio/    Spec + síntesis del audio (`native/assets/audio.json`).
  crates/iafz-sys/    FFI Rust↔C de la IA frugal (IAFZ-64, Deep BNN binaria).
  crates/ha-game/     Cliente Bevy ECS — render 2D, input táctil, sim local 20 Hz.
  crates/ha-asset-tools/  HA Studio: los editores (dev, corre aparte del juego).
```

**Dependencias:** `client→shared` (sólo tipos del manifest) · `server→shared` ·
`ha-shared` = espejo numérico de shared · `shared→nada`.

**Ojo shared vs native:** el sim de `packages/shared` sigue siendo la isla plana
96×96 que alimenta al servidor Colyseus. El cliente nativo tiene su propia capa de
geometría de mundo (§4). Las *reglas* se espejan; la *geometría* no.

---

## §4 El universo — un solo marco continuo

**Éste es el corazón del diseño.** Leerlo antes de tocar mundo, vuelo, cámara o
cielo. Módulo: `ha-game/src/universe.rs`.

El problema que resolvió: existían **dos marcos de coordenadas**. Al despegar, el
jugador se teletransportaba del marco de tiles al marco de efemérides (la Tierra
está a decenas de km del origen porque orbita al Sol); la cámara seguía uno y el
sprite el otro, y el salto se tapaba con nubes 100 % opacas. Resultado: cámara
rota, planetas invisibles, imposible volver.

1. **Un solo marco.** El origen de render es el centro del `ref_body`. El mundo de
   tiles de ese cuerpo ya vive ahí, así que el disco del planeta cae exactamente
   encima del terreno caminable. **El jugador nunca cambia de coordenadas** al
   despegar ni al aterrizar.
2. **Altura = zoom.** La superficie es cenital, así que la altura es perpendicular
   a la pantalla: subir no mueve el sprite, abre el encuadre (`view_h_for_alt`,
   continua y monótona de 30 m a 135 km).
3. **Altura efectiva = hipotenusa(vertical, hueco lateral al cuerpo).** Es lo que
   hace que la aproximación se regule sola: cuanto más cerca, menor la altura, más
   cierra el encuadre y más despacio vas, hasta posarte.
4. **Tres LOD del suelo que se funden** — tiles de 1 m → mapa satelital de 8 m
   (`TerrainLod`) → disco del planeta. Los tres se muestrean del **mismo**
   `topdown_tile` y comparten `world2d::tile_render_color`, así que color y forma
   coinciden. **Esa es la continuidad: no hay nada que tapar.**
   ⚠ El satelital lleva el color **en los vértices de una rejilla compartida**, no
   uno plano por quad. Un color plano por celda *es* un cuadrado, y como el lado
   del quad sigue al encuadre el damero medía siempre ~40 bloques de alto a
   cualquier altura: era el "el terreno me sale con cuadrados, como cuando no carga
   la textura". Interpolado se lee como un mapa pintado —la versión de pocos
   polígonos del mismo terreno— y encima cuesta menos (4× menos vértices y ~4× menos
   muestras de relieve por reconstrucción). Test: `la_malla_lod_no_es_un_damero`.
5. **Rebase del marco** al acercarte a otro cuerpo: jugador y discos se desplazan
   el mismo delta ⇒ en pantalla no pasa nada, y tus coordenadas pasan a ser las de
   la superficie del cuerpo nuevo. Se viaja y se aterriza sin un solo teletransporte.
6. **Región y contexto se derivan de la altura cada frame**
   (`creative_fly::sync_frame_region`). La clase entera de soft-locks ("aterricé
   pero sigo en el espacio") deja de ser representable: no hay estado que
   sincronizar, hay una función.

**Consecuencias:**
- `transition.rs` quedó reducido a la **bruma** atmosférica, techo `MAX_HAZE = 0.45`.
  ⚠ La cobertura **nunca** vuelve a ser opaca. Si algo necesita esconderse detrás
  de una pantalla, es un bug de continuidad.
- Las estrellas guardan posición y tamaño **normalizados** y van en `z = -400`
  (detrás de los discos). Antes se dibujaban sobre el regolito al pisar la Luna.
- El satelital se **recorta al radio del cuerpo**, o en cuerpos chicos (Luna
  r=420 m < los ±1024 m del mapa) el suelo desborda el planeta.
- `Flight::vertical_speed` / `horizontal_speed` son **relativas al cuerpo**; en
  absoluto arrastraban los ~565 m/s de la traslación terrestre.

### Escala y viaje

Radio de la Tierra **30 000 m** (60 km de punta a punta); radios ×20 y distancias
×8 respecto del sistema original; el Sol a 4 M. Cruzar el planeta a altura de
crucero (~1.4 km, donde todavía se distingue el terreno) son **~10 minutos**.
Períodos orbitales recalibrados a 30–100 min: la velocidad tangencial de un
planeta (~4 km/s) quedó **por debajo** del techo de crucero del jugador (30 km/s),
o un destino se te escapa y no hay forma de llegar.

**Ki**: drain 0.30/s; una carga da ~5.5 min de vuelo. Ida y vuelta a la Luna entra
holgada; Marte exige recargar. ⚠ El presupuesto de Ki y la escala del sistema
están **acoplados**; lo verifica el test
`a_full_ki_charge_covers_a_moon_round_trip`, no la próxima partida.

**Vuelo:** mantener JUMP carga el aura y despega; JUMP sube, USE baja, stick lateral
libre. Aura dorada procedural (`fx/aura_saiyan`). El Ki reemplaza al combustible.

**Espacio vivo** (`space_fx.rs`): halo radial translúcido (la misma pieza sirve para
atmósfera, corona solar y cabeza de cometa), terminador por cuerpo girando con su
órbita, cinturón de asteroides (220 rocas), cometas en elipses reales con la cola
en contra del Sol, polvo con parallax, calentamiento de reentrada.

⚠ **Dos trampas de render que costaron horas:** las texturas generadas necesitan
**muestreo lineal** (`ImageSampler::linear`) — un halo de 256 px estirado a 150 km
con vecino-más-próximo cuantiza el degradado en anillos concéntricos; y aun con
filtrado, el alfa de 8 bits **bandea**, hace falta **tramado** al generar el
degradado.

---

## §5 El mundo

### Geografía (`ha-shared/src/geography.rs`)

Relieve en cuatro escalas (continentes 9 km, cordilleras 1.4 km, colinas 110 m,
textura 22 m), humedad en dos, frío por latitud y altura. De ahí salen biomas como
**regiones que se atraviesan**: océano, playa, río, pantano, pradera, bosque,
sabana, desierto, sierra, cumbre, nieve, basalto, hielo, regolito.

Y civilización: asentamientos por rango (**aldea / pueblo / ciudad**), trama urbana
con calles y manzanas, edificios con fachada a la calle, **carreteras** que unen
núcleos vecinos con trazado curvo y **puentes** donde cruzan agua — así terminan
conectados dos continentes sin colocar nada a mano.

**Arquetipos por cuerpo:** Terran (Tierra), Oceanic (Titán, Neptuno), Desert
(Marte, Urano), Icy (Europa), Volcanic (Venus, Ío), Barren (Luna, Mercurio,
lunitas). Cada uno con su nivel del mar y su paleta — no la Tierra repintada.
⚠ Un arquetipo no puede salir casi todo sólido: aterrizar ahí sería quedar
enterrado. Lo vigila `every_world_is_walkable_somewhere`.

**Mundo finito:** más allá del radio caminable no hay mundo. La plataforma se hunde
hacia el borde, así el planeta termina en océano abierto en vez de cortarse a pico,
y el disco que ves desde el espacio es literalmente todo el terreno que existe.

**Coste:** la geografía es ~10× más cara por celda que la anterior. Las pruebas de
"¿esto es tierra?" usan una sola octava (`continental_at`) y los pozos de cueva se
calculan una vez por celda. Lo vigila `generating_a_chunk_is_cheap` (< 8 µs/celda).

### Cuevas y pisos, sin portales

Bocas de cueva repartidas por todo el mundo, una cada ~900 m donde hay galería
debajo y suelo pisable. **Se entra caminando sobre el agujero**: no hay prompt, ni
carga, ni teletransporte — conservás tus coordenadas y sólo cambia la capa que se
dibuja, con un fundido de 0.28 s (`world2d::LayerFade`).

Lo mismo para subir: `Tile::StairsDown/Up` hace el mismo swap instantáneo de
`tiles.z`. Hasta 3 pisos hacia arriba, hasta `MIN_Z_LEVEL` hacia abajo.

⚠ **Nada de cambios de escena para bajar a una cueva o subir un piso.** Si hace
falta una pantalla, la continuidad está rota.

### Pisos altos: el vacío (estilo Project Zomboid) — 1 ago 2026

**`z > 0` es `Tile::Air`, no `Bedrock`.** Antes era roca sólida, y como
`place_tile` rechaza toda celda sólida y `break_tile` no pica bedrock, **el
segundo piso no se podía construir**: lo único que existía arriba era la sala 7×7
que `carve_stair_room` tallaba al llegar, de la que no se podía salir ni ampliar.
Subir una escalera daba un cuarto fijo, no una planta.

- **El vacío deja ver abajo.** `tile_render_color` busca hacia abajo el primer
  nivel con algo y lo pinta atenuado (`VOID_DIM`). Tu planta nítida, el mundo de
  abajo apagado — sin esto el piso alto flota en negro y no sabés si el borde de
  tu losa da al patio o a la calle. Test: `el_hueco_del_piso_alto_muestra_el_
  suelo_de_abajo`.
- **Del vacío se CAE** (`world2d::fall_through_void`): salirte de tu losa —o
  quedarte sin ella— te baja un nivel en el mismo `(x, z)`, y sigue bajando hasta
  pisar algo. Sin daño, con el mismo impulso/polvo/golpe que descolgarse de un
  bordillo (`vault::VaultBob::drop_impact`). La alternativa era hacer sólido el
  vacío, que es exactamente el "choco con nada" que el jugador reporta como bug.
- **Apoyo** (`build_grid::floor_supported`): en altura una losa entra si hay algo
  debajo que la aguante —piso, pared de borde o estructura del nivel inferior, y
  también **las casas generadas del pueblo**, que es justo lo que uno quiere
  ampliar— o si se engancha a una losa vecina del mismo nivel (voladizos y
  balcones, encadenable como en Zomboid). La regla 1 es la que impide el edificio
  flotante; la 2, la que deja volar un alero.
- **Subir deja un rellano 3×3**, no una habitación amurallada: la grada de vuelta
  al centro y piso alrededor. El resto lo levantás vos, losa a losa.
- **La gente y los animales viven en la superficie**: fuera de la planta baja se
  ocultan (`society::tick_society`, `fauna::cull_fauna`), como ya hacían tiles y
  decor. Sin esto los vecinos caminaban por encima de tu segundo piso. La
  simulación sigue corriendo — el pueblo no se congela porque subiste una escalera.
- **Chapa de piso** (`build::sync_floor_indicator`): `PISO 2` / `SUBSUELO 1`,
  oculta en planta baja. Un solo eje para arriba y para abajo.

Lo cubre `HA_BUILDTEST` de punta a punta: piso → anillo de paredes → la pared sin
apoyo se rechaza → el muro frena → grada → rellano → losa sobre la casa → losa
flotante rechazada → salir de la losa te baja.

### Construcción por bordes (estilo Last Day on Earth)

La pared no es una celda: vive en el **borde** entre dos celdas. Un cuarto 5×4
tenía 6 tiles útiles rodeados de 14 de muro; ahora tiene 20 y el muro es una línea
fina.

- **Dos capas:** piso/escaleras = celda (`TileWorld`, autotile) · pared/puerta/
  ventana = arista `(tx, tz, axis)`, donde `H` es la línea `z = tz` y `V` la
  línea `x = tx`.
- **Reglas puras** en `ha-shared/src/build_grid.rs`: `nearest_edge` (snapping del
  dedo), `wall_supported` (necesita piso construido de algún lado),
  `wall_capsule_hits` (colisión contra rectángulos finos, `WALL_THICK = 0.26 m`),
  `prefab_edges`, catálogo `EDGE_PIECES`.
- **Estado y sprites** en `ha-game/src/walls.rs`: `BuiltWalls` persiste
  `walls.json` guardando la pieza **por id, no por índice**; streaming a 5 Hz
  alrededor del jugador; Y-sort por el pie del borde; puertas que se abren por
  proximidad.
  ⚡ **Indexado por borde, no barrido** (1 ago 2026). `blocks()` es la consulta más
  caliente del juego: la llama `wall_capsule_hits` por cada arista que toca la
  cápsula, para el jugador **y para cada animal y NPC**, todos los frames. Con la
  búsqueda lineal costaba `paredes × aristas × entidades`: gratis con la choza de
  la prueba, decenas de millones de comparaciones en cuanto alguien levanta el
  refugio que el sistema existe para levantar. Ahora es un `HashMap` por
  `(cuerpo, piso, celda, eje)`; `BuiltStructures` hace lo mismo con un set de
  celdas sólidas. Ojo con `swap_remove`: hay que reapuntar la entrada de la que se
  movió o queda una pared que se dibuja y se atraviesa (lo clava
  `demoler_una_pared_no_desindexa_a_las_demas`). Todo lo que se cargue de disco
  tiene que entrar por `add`, nunca asignando la lista.
- **Colisión** vía `topdown::step_topdown_movement_ex`, compartida por el jugador
  (`sim.rs`) y la fauna (`fauna.rs`) — el corral del vivario encierra de verdad.
- **UX:** apuntar a un borde vacío = colocar (ghost verde/rojo); apuntar a uno
  ocupado = demoler (ghost naranja) devolviendo el material. `BUILD_PIECES` quedó
  sólo con piso y escaleras; el resto son prefabs (`room_small/medium/large`,
  `casa_1`) que estampan piso + anillo de paredes con su puerta en un solo hold.
- **Estructuras grandes** (`structure/*`, `structures.rs`): registro persistente
  `structures.json`, sprites Y-sorted, colisión de base.

⚠ **Anti-regresión:** no volver a colocar paredes como tiles de celda. Los tiles
`WoodWall`/`Door`/`Window`/`StoneBrick` siguen existiendo **sólo** para los
asentamientos generados y las partidas viejas.

### Techos y tumbados — la tercera capa (2 ago 2026)

**Piso = celda · pared = borde · techo = celda, pero por encima**
(`ceilings.rs`, catálogo `build_grid::CEILING_PIECES`). Antes el único techo era
el rectángulo de un prefab, así que la casa que crece losa a losa —la que uno
construye de verdad— no tenía ninguno.

- **Tumbado** (`ceiling/madera`): la tapa lisa de adentro. Es "poner el piso como
  techo", que es como se pide cuando terminás el cuarto.
- **Techo modular** (`roof/teja`, `roof/chapa`): la cubierta de afuera, con
  **autotile por vecinos** — pintás la superficie y el juego resuelve faldones,
  cumbrera y esquinas. Una clave de arte, no nueve piezas para encajar a mano en
  un celular.
- **El suelo del 2º ES el techo del 1º**: toda celda con losa arriba se tapa sola
  (`ceilings::covered`), sin datos duplicados. Techar es la consecuencia de
  construir la planta siguiente, no una tarea aparte.
- Se transparenta al entrar, con la regla de siempre (§4: nunca una cutscene).
- Apoyo: se techa lo construido — piso o pared en la celda (`ceiling_supported`).

### Muebles que miran a algún lado (2 ago 2026)

`PlacedStructure.rot` (cuartos de vuelta, `#[serde(default)]` → las partidas
viejas entran como 0).

- **No se rotan píxeles.** En pixel art una cama girada 90° queda tumbada de
  costado y con la perspectiva 3/4 rota. La orientación elige una POSE del slot
  (`sur`/`oeste`/`norte`/`este`, `world_edits::FACING_POSES`), que el usuario
  recorta como ya recorta las de las paredes. Sin esas poses el objeto se dibuja
  como siempre y sólo cambia su huella: girar mejora con arte y nunca empeora sin
  él.
- **La huella gira con el objeto** (`footprint_rotated`, `rotate_cell`) — si no,
  el mueble se ve contra la pared y bloquea la celda de al lado (la "pared
  invisible" de §7).
- **Anclaje automático**: por defecto se planta de espaldas a la pared que tenga
  al lado (`auto_facing`). La chapa **GIRAR** (táctil, arriba-izquierda) y la
  tecla `R` fijan una orientación a mano; `AUTO` vuelve al automático.
- **Lo de adentro va adentro**: un `PlaceCat::Mueble` pide estar bajo techo o en
  un rincón de al menos dos paredes (`cell_is_indoors`). Las dos condiciones
  porque las dos son "adentro": un cuarto terminado tiene tapa y uno a medio
  hacer todavía no, pero ya tiene esquinas.

### Integridad: lo que se queda sin apoyo se cae (2 ago 2026)

`integrity.rs`. Sin esto la verticalidad mentía: picabas el piso de abajo y el
segundo piso quedaba flotando con sus paredes y su techo. Es la regla de
construir leída al revés — si una losa pierde el apoyo que `floor_supported`
exigía, se cae, y con ella las paredes que se quedaron sin piso a ningún lado y
las tapas sin nada debajo. Se propaga hacia arriba planta por planta.

Los materiales **se devuelven**: un derrumbe es una consecuencia física, no una
trampa. Cuesta cero mientras no toques una losa (`TileWorld::take_removed_floors`
sólo se llena al sacar una).

Y **picar arriba abre un agujero**: en `z > 0` `break_tile` deja `Air`, no
`CaveFloor` — sacar el piso tiene que dejar por dónde caerse.

### Otros sistemas de superficie

- **Césped que rebrota:** tierra expuesta sobre césped natural vuelve a Grass en
  45–120 s por celda (hash). No rebrota sobre edificios generados ni donde el
  jugador construyó, ni sobre terreno autorado en el editor.
- **Mapa como ítem** (`map_item.rs`): se craftea (`map_small`/`map_large`), se abre
  en la mano con tu posición marcada y se **cuelga** en la pared con USE,
  retratando el sector donde estabas. Colgar uno por sector arma el mapa del mundo.
- **Survival** (`survivalStats.ts` → `ha-shared/survival.rs`): hambre 0.12/s, sed
  0.09/s, oxígeno sólo en vacío. Hambre o sed en 0 → 1.5 dmg/s; asfixia → 4 dmg/s.
  Comer = HOLD con comida equipada y sin target. Muerte amable: barras e hp llenos,
  inventario intacto, sin game over duro. Persistencia `survival.json`.
- **Máquinas e inventario** por cuerpo, con banking off-body del `stored_output`.
- **Bordillos de terreno:** un borde de terraza en terreno llano se trepa y se baja
  de un impulso en cualquier dirección; la falda de una cordillera exige rampa. Lo
  decide la **pendiente** (`relief::climbable_edge`, umbral `SCARP_GRADIENT`), no
  la altura de la cara — el 99.8 % de los acantilados de la Tierra tiene exactamente
  un escalón, así que la altura no distinguía nada. Reparto actual: ~54 % bordillos
  / ~46 % paredes. Arte propio `ledge/{roca,pasto,arena,nieve,regolito,basalto}`.
  **Cruzarlo se SIENTE** (`ha-game/src/vault.rs`): trepar iza el sprite en un arco
  con un roce y tres motas de polvo; descolgarse lo deja caer, revienta una ráfaga
  de polvo a los pies, suena un golpe seco y sacude la cámara un pelo. El polvo se
  tiñe con el color del suelo pisado (`tile_render_color`), así que sirve en todos
  los cuerpos sin arte nuevo. Sin esto la regla era invisible y cruzar un bordillo
  se leía como **atravesar una pared** — el reporte literal. Es feedback puro: no
  toca la sim determinista, no bloquea el control y el offset va sólo en el dibujo,
  nunca en el Y-sort (los pies siguen en su celda). Las rampas (`Tile::Slope`) no
  disparan nada: subir una carretera de montaña no es un esfuerzo puntual.

---

## §6 La vida — el mundo vive sin vos

### Ecología (`ha-shared/src/ecology.rs`)

**El problema que resolvió:** la fauna se sembraba alrededor del jugador y el
animal que se alejaba más de 42 m se teletransportaba al borde opuesto; los NPCs se
sembraban en tu punto de aterrizaje. Garantizaba tener vida y gente al lado, y por
eso mismo mataba la exploración: el mundo no tenía lugares, tenía un radio.

- El mundo se parte en **regiones de 64 m**.
- Cada región tiene una **capacidad de carga** sacada del bioma: forraje comestible
  + escondite para el depredador. Un bosque hierve; un desierto está vacío de verdad.
- Sobre esa capacidad corre una **cadena alimenticia** de tres niveles (plantas →
  herbívoros → carnívoros; Lotka–Volterra logístico + Holling II) más **difusión
  entre regiones vecinas**.
- Una región nunca visitada **vale su equilibrio**: la vida ya estaba ahí, gratis y
  sin guardar nada. Lo que se aparta del equilibrio es lo que **hiciste**, y eso sí
  se persiste (`vivarium/ecology.json`) y se poda al volver a su estado natural.

`fauna.rs` **sólo materializa**: la población es la fuente de verdad y los animales
son la vista de las regiones cercanas (ventana 3×3). Nada se teletransporta; el que
se aleja vuelve a ser número en su región y sigue vivo. Nacer, morir cazado y cruzar
un borde mueven el censo, así que el mundo cuadra siempre.

⚠ **Anti-regresión:** no volver a sembrar vida en función de dónde está el jugador.
Si algo tiene que estar cerca, que lo esté porque el mundo lo puso ahí.

### Sociedad (`society.rs`, `dialog.rs`, `npc_menu.rs`)

La gente vive en los **asentamientos de `geography`** (aldea / pueblo / ciudad, con
sitio fijo en el planeta) y se siembra al **entrar** en uno (`discover_settlements`).
Fuera de ellos no hay un alma: encontrar gente es explorar. El pueblo no se congela
cuando te vas — los NPCs lejos siguen su horario en LOD barato (`advance_far`),
también en otros cuerpos, y la economía corre para todos (`colony_step`, roles,
`Stock`, `try_trade`).

Hay **religión fundable**, memoria de hechos (si le matás el padre a alguien, esa
familia te recuerda), reputación por cuerpo (`body_rep`) y chat por contexto.
Tocar un NPC abre un panel con los últimos mensajes y botones por rol; las
burbujas tienen presupuesto (`MAX_BUBBLES=3`), dueño y prioridad.

**La secta es del pueblo, no tuya — ningún NPC te sigue.** Esto es un pedido
explícito y ya se rompió una vez, así que está clavado con un test
(`society::religion_tests::ningun_fiel_apunta_al_jugador`):

- **Fundás predicando.** Menú del NPC → `PREDICAR`; el primer convencido funda el
  culto (`Religion::found`). Antes se fundaba juntando un rebaño de aldeanos
  alrededor tuyo, que es justo la conducta que no queremos premiar.
- **Crece solo, de vecino a vecino.** Un fiel contagia fe a los no-conversos
  dentro de `PREACH_RADIUS`, estés donde estés. Ya **no** hay ganancia de fe por
  estar cerca del jugador: pararse al lado de un aldeano no le hace nada.
- **El fiel hace su vida.** Ser converso no cambia a dónde va: mismo horario
  (trabajo / plaza / casa) y misma velocidad que cualquier vecino. Lo único que
  cambia es que a la hora de la plaza se aparta al **santuario** de SU pueblo
  (`shrine_of`, a un costado de la plaza) — ahí se ve la secta, un corro dorado
  rezando todos los días, exista o no el fundador.
- Lo único que todavía camina hacia el jugador es quien te **odia**, y eso es
  consecuencia de matarle la familia, no de fe.

### IA frugal (IAFZ-64) y evolución

La IA frugal del usuario (HIVE_OS / IAFZ-64) es una **Deep Binary Neural Network**
(MLP de 3 capas, `XNOR + popcount`, sin float pesado ni backpropagation) que aprende
por **neuroevolución offline** en C. El genoma es un `.adn` de 6.5–9.6 KB.

**Qué es y qué no:** es un **controlador reactivo**, no una mente cognitiva. Da
instinto y movimiento orgánico (locomoción, steering, evasión, persecución); no da
planes ni metas de largo plazo. E/S angosta (~32 bits de entrada).

**Arquitectura adoptada — cerebro híbrido en capas:**
- **Capa alta — psicología** (barata, determinista, en `ha-shared`): necesidades +
  utilidad + memoria simple deciden la **meta**. De ahí sale el "tiene vida
  interior", que es el motor de retención.
- **Capa baja — instinto (IAFZ)**: ejecuta la meta a nivel motor. Un genoma por
  especie/rol = literalmente un cerebro colmena.

**Estado:** `iafz-sys` (FFI, cross-compila a aarch64) ✓ · gym de neuroevolución
(`./editor_assets` espacio Fauna) con fitness 347→1452 en 160 generaciones ✓ ·
evolución **en el mundo** (comen, se reproducen heredando el genoma mutado, mueren →
selección natural in-game) ✓ · sensor compartido gym↔juego (`ha_shared::fauna_brain`)
con percepción social ✓ · **estigmergia** (campo de feromonas `SignalField` que
decae: alarma/comida/territorio/apareamiento) ✓ · gym multiagente (K=5) ✓.

**Compañero-IA** (`companion.rs`): la "mano derecha" del jugador, controlada por la
misma IA frugal, que lo sigue y evoluciona sola (hill-climb + persistencia).

---

## §7 Presentación

### Cámara

- **Siempre centrada** en el jugador. El zoom **nunca** mueve al personaje.
- **Auto-zoom por altitud** (`view_h_for_alt`, §4) con un **multiplicador de
  usuario** encima (pellizco móvil / scroll desktop). El pellizco es cámara pura:
  mueve el encuadre vivo, pero el personaje se dimensiona con el encuadre
  **vivo**, así hacer zoom te acerca sin que parezca que descendés.
- ⚠ **La altura es un PISO del encuadre, jamás un valor guardado.** El encuadre de
  cada frame se calcula entero (`camera::desired_frame_h`) y no se escribe de
  vuelta en el zoom del jugador. Guardarlo era el bug de "el botón de bajar no
  baja": subir trinqueteaba `view_h` hacia arriba y bajar ya no podía cerrarlo, así
  que el descenso era invisible (el personaje quieto, la altura cayendo sin que
  nada se moviera) hasta que al tocar suelo el clamp de superficie devolvía la
  cámara de golpe — el "de repente aparezco en el suelo". Bajar tiene que deshacer
  subir, y eso sólo se garantiza si el frame no recuerda nada
  (`camera::tests::bajar_deshace_subir`).
- La **velocidad de vuelo se ata a la altura, no al zoom** (hacer zoom no puede
  acelerarte).
- **Encuadre de aproximación:** volando, la cámara se adelanta hacia el cuerpo
  destino (hasta ¼ de pantalla), así el planeta crece de frente y entrás en él en
  vez de orbitar su centro fuera de cuadro.
- Pellizco a pie en la **banda superior** de la pantalla (`PinchActive` suelta los
  sticks) → `view_h` 12–45.

### Luz y cielo

- **Ciclo día/noche de 1 h** (25 min día · 5 atardecer · 25 noche · 5 amanecer).
  Ningún astro se dibuja como disco: en vista cenital un sol se lee como un manchón
  blanco sobre los techos. El sol existe como `SkyLight` (dirección/largo/color de
  las sombras) y su posición se lee en el mini-dial del reloj.
- **Luz 2D por asset** (`light2d.rs` + campo `emission` en el manifest): la
  oscuridad se **agujerea**, no se tapa con naranja. `night_veil.wgsl` +
  `NightVeilMaterial`; `NightOverlay` es `Mesh2d`; hasta 16 luces. Emisor con
  núcleo + halo + mist; luciérnagas con `Light2d` chico; `light_sprites` aclara a
  jugador/NPCs cerca del foco.
  ⚠ `light_sprites` está **partido en dos pasadas** (pasada cara a 20 Hz +
  re-aplicación del tinte a 60 Hz) porque vive en `PostUpdate` para pisar a
  fauna/NPCs/máquinas, que repintan `sprite.color` cada frame. **No unificarlas**:
  bajarlo a 20 Hz a secas reintroduce parpadeo.
  Probar de noche con `HA_DAYPHASE=0`. El `NIGHT_VEIL` alto es a propósito.
- **Estrellas** que siguen a la cámara, siempre; nunca geometría finita cruzable.

### Audio (`audio.rs` + crate `ha-audio`)

**Todo sintetizado en memoria al arrancar** (8/16-bit): cero assets de audio, cero
peso en el APK. Data-driven desde `native/assets/audio.json`, embebido con
`include_str!` (fallback al default si el JSON rompe).

- **22 SFX** vía evento `PlaySfx` con pitch ±5 %: cavar, colocar, stamp, pasos
  (suaves/duros ×2 por material), bolsa, craft, comer/beber/daño, salto, puertas,
  gradas, despegue/aterrizaje, loop de empuje, click de UI, pickup.
- **Música estilo C418**: pentatónica procedural determinista, pistas de
  superficie / espacio / subsuelo, director por contexto con **90–240 s de silencio
  entre pistas**. No meter loops continuos ni assets de audio.
- Volúmenes SFX/Música en el panel de AJUSTES.

### Arranque de partida: creador de personaje → pantalla de carga

El arranque es `Splash → MainMenu → CharacterCreate → Loading → Playing`
(`game_phase.rs`). El mundo **no existe** hasta `Loading`: `scene::spawn_player` y
`universe::spawn_universe` corren ahí, y sin `Player` el streaming de chunks
retorna temprano, así que en el menú no hay worldgen.

**LA ELECCIÓN ES UNA CUADRÍCULA, NO UNA RUEDA** (2 ago 2026). El creador tenía
cinco selectores `< ROPA 20 >`: para ver un gorro había que pasarlos de a uno y
leer un número — elegir ropa mirando texto. Ahora arriba van las fichas de capa
(PIEL/OJOS/ROPA/PELO/GORRO) y abajo la **cuadrícula de esa capa**, con el dibujo
de cada prenda; tocás y se selecciona (borde y fondo magenta en la elegida, no
sólo el borde: en el teléfono a plena luz 2 px no se distinguen de 1).

- La celda muestra el frame **de frente** de la capa sola, con el mismo
  `ImageNode` + atlas que la vitrina: no compone ni hornea nada, así que 200
  celdas cuestan lo mismo que 200 sprites.
- El accesorio abre con una celda **NINGUNO**: "sin gorro" es una elección, no un
  hueco.
- El layout de atlas se arma UNA vez (`CcSheetLayout`) y lo reusan todas las
  celdas — uno por celda serían 200 `TextureAtlasLayout`.
- **Acá el scroll SÍ va**: la cuadrícula es una lista de ~200 prendas, no un
  menú. Lo que no lleva scroll es el menú (ver abajo).

**EL JUEGO SE INSTALA EN LA COMPU** (`instalar-en-esta-pc.sh`, 2 ago 2026).
Compila el perfil `release-native` y deja binario + assets en
`~/.local/share/ki-mundo-abierto/`, el icono en `hicolor` y una entrada
`.desktop`: aparece en la bandeja de aplicaciones como cualquier programa, sin
root. El lanzador hace `cd` a la carpeta de datos antes de ejecutar — el juego
busca `assets/` relativo a donde corre, y sin eso arranca sin arte. Trae
`--desinstalar` (no toca la partida guardada).

**NADA DE SCROLL EN LOS MENÚS** (1 ago 2026). Tanto el creador de personaje como
el menú de MENU tenían contenido abajo del pliegue; se probó ponerles scroll y el
pedido fue explícito: **en un menú entra todo o está mal armado**. La app está
clavada en apaisado (`AndroidManifest`, `screenOrientation="landscape"`), así que
lo que sobra es ANCHO y los dos pasaron a ser de DOS COLUMNAS:

| pantalla | izquierda | derecha |
|---|---|---|
| creador (`character_create.rs`) | vitrina + retrato + el globo | las cinco ruedas + NOMBRE |
| MENU (`game_menu.rs`) | mapa orbital (`Vh(76)`) | título + botones (`FlexWrap::Wrap`) |

Las medidas salen del alto disponible y las columnas se reparten el ancho con
`flex_grow`, así que se acomodan solas. El scroll sigue existiendo donde SÍ va: en
listas largas (mochila, cofre), que es otra cosa.

**El nombre se puede escribir en el teléfono** (`CcKeyboard`). Era un campo
decorativo: sólo lo leía `name_input` desde `KeyboardInput`, o sea únicamente en
escritorio — en el celu te quedabas con el que vino sorteado. Ahora el recuadro es
un botón que abre un **teclado en pantalla** dibujado por nosotros. Se descartó
pedirle el teclado del sistema a Android (`show_soft_input`): tapa media pantalla
y depende de que el IME entregue teclas, que en `android-activity` no es seguro.
`el_teclado_cubre_lo_que_el_nombre_acepta` clava que las teclas dibujadas y lo que
`name_input` acepta no se separen.

**Y el nombre SOBREVIVE al menú**: `society::npc_line` lo recibe y el saludo neutro
te nombra ("Buen dia, <vos>."). Antes `SelectedCharacter.name` se guardaba en el
savegame y no lo leía nadie más.

**El retrato opina** (`PortraitQuip`). Cada vez que le cambiás pelo, color, ropa,
piel o gorro, el personaje comenta lo que le estás haciendo — rompiendo la cuarta
pared a propósito: la partida no empezó, así que es el único momento en que te
puede hablar a VOS y no al mundo. ASCII-only (regla 6), lo clava
`los_comentarios_son_ascii`.

**Creador de personaje** (`character_create.rs`) — es el *generador por capas* del
pack de LimeZu adentro del juego: PIEL, OJOS, ROPA (+color), PELO (+color) y
GORRO/accesorio (+color), más `AL AZAR` y el nombre. La vista previa **no compone
nada**: son cinco `ImageNode` apilados, así que tocar `>` es instantáneo. Al lado
va tu **retrato**, la cara con la que vas a aparecer en los diálogos: elegís
mirándote como te van a ver. Lo elegido se guarda como `SelectedCharacter.look`
(cinco índices) en el savegame; una partida vieja sin `look` sigue andando con su
`sheet`.

**La vitrina son las CUATRO direcciones, caminando en el lugar** (`SHOWCASE` +
`animate_showcase`). Antes se veía una sola pose quieta de perfil derecho —la
celda 0 de la hoja— y era engañoso: elegías mirándolo de costado y en el juego lo
ves casi siempre de frente o de espaldas, que es donde se leen el pelo, el gorro
y la cara. Los cuatro muñecos comparten un reloj y se les escribe el índice de
atlas sólo cuando cambia el cuadro (6 escrituras por segundo, no 60). La celda
sale de `char_cell()`, así que el orden de direcciones de la hoja (`R,U,L,D`)
vive en un solo sitio.

La pantalla está en tres tarjetas —vitrina, opciones, nombre— con la columna de
rótulos de ancho fijo: las cinco ruedas comparten grilla y el hueco del COLOR se
reserva aunque la capa no tenga (piel y ojos), o las flechas quedan escalonadas.
En la barra de abajo **JUGAR es el único botón relleno**: con los tres iguales,
la acción principal pesaba lo mismo que "volver".

**Pantalla de carga** (`loading.rs`) — el único momento del juego donde se puede
gastar de más sin que se note. Antes el mundo nacía CON vos adentro: el suelo
aparecía a pedazos y el frame saltaba justo cuando más se mira. Ahora, detrás de
una cortina opaca:

1. se spawnea el jugador y el universo, y todo lo que construye mundo sigue
   corriendo (`game_phase::world_active` = `Loading` + `Playing`);
2. el presupuesto de chunks sube de 2 a 12 por frame (`world2d::ChunkBudget`) —
   un frame de 200 ms detrás de la cortina no es un tirón, es trabajo adelantado;
3. se compone tu personaje a UNA textura (`chargen.rs`);
4. **se esperan las texturas.** Los PNG del manifest se piden en `PreStartup`
   pero se decodifican en hilos de IO: la cortina se levantaba con el mundo
   armado y el arte todavía viajando, así que la subida a GPU de cada textura
   caía DENTRO del gameplay y los sprites aparecían de golpe unos frames
   después. Ahora `loading.rs` recorre `ArtLibrary::image_handles()` y no sale
   hasta que están todas (un PNG que falta cuenta como lista: para eso está el
   respaldo procedural);
5. la cortina se levanta cuando pasaron las tres etapas —texturas, anillo
   (`world2d::ChunkBacklog == 0`), personajes— y hay 8 frames seguidos por
   debajo de 34 ms. Tope duro de 20 s: nunca te deja encerrado. La barra reparte
   el recorrido entre las tres etapas y el texto dice en cuál está.

⚠ Esto **no** contradice la regla "una transición es un cambio de marco, jamás una
cutscene" (§2.8): esa regla es para las transiciones EN JUEGO —despegue, órbita,
entrar a un planeta—, que siguen sin cortar nada. Esto es el arranque de la
partida, antes de que exista el jugador. **No agregar pantallas de carga entre
planetas.**

### Retratos de diálogo

El panel de NPC (`npc_menu.rs`) abre con un **retrato de 64×64** al lado del
nombre. Dos cosas lo hacen valer:

- **Es SU cara.** El retrato se compone con las mismas capas con las que se horneó
  su cuerpo (`chargen/index.json` guarda el look de cada `npc/char_XX.png`), así
  que el vecino de barba y gorra habla con cara de barba y gorra. Lo verifica
  `chargen::tests::los_looks_horneados_coinciden_con_los_del_juego`.
- **La expresión es información**: hostil = enojada, sin dormir = dormida, con
  hambre = triste, fiel = contenta (`chargen::Expression::for_npc`). Se te nota en
  la cara del pueblo cómo lo estás tratando antes de que abra la boca.

Los retratos se componen una vez y quedan cacheados por look (`ChargenJobs`), con
presupuesto de 2 por frame.

### Controles — el dedo ES el puntero

Desapareció el stick derecho, desapareció la mira y desapareció el botón USAR en
tierra. **Tocás el árbol y lo talás**; con un bloque en la mano tocás el suelo y lo
colocás. Minecraft Pocket aplicado al cenital. La mira era un intermediario: había
que conducir un cursor con el pulgar hasta el objeto y recién ahí apretar USAR.

| Pieza | Dónde |
|---|---|
| Dedo → punto del mundo + gesto | `input/world_touch.rs` (`AimCursor`, `track_world_touch`, `world_touch_gesture`, `resolve_hold`, `place_point`) |
| Orden puntero → objetivo → gesto | `input::InputSet::{Pointer, Gesture}`; `targeting.rs` corre entre los dos |
| Stick único (izq = mover) | `input/joystick.rs` (`StickState`) |
| USAR sólo volando (= bajar) | `input/actions.rs` (`UseButtonActive`) |

**Reglas del gesto** (una sola, y el contexto decide):

1. **Hay algo tocado** → el mantener arranca en el acto, sin retardo. Soltar antes
   cancela. (Los 200 ms de retardo quedan sólo para lo que se usa "en la mano" sin
   objetivo: comer, plantar cofre/antorcha/mapa/máquina.)
2. **Un contacto, una intención.** Una vez que el dedo empezó a COLOCAR no puede
   pasar a PICAR sin levantarse. Sin esto, arrastrar una hilera de bloques sobre un
   muro existente lo demolía.
3. **La cadena sólo avanza a un objetivo distinto** (`target_key`): se pica una
   veta entera arrastrando, pero no se comen tres manzanas ni se paga dos veces el
   mismo piso.
4. **Toque corto que no interactuó con nada** → golpe de la clase
   (`AbilityTapPulse`).
5. **Un objeto COLOCADO tiene dos verbos y el gesto los separa solo**: **toque =
   usar** (el cofre se abre), **mantener = levantar** (vuelve a la mochila y lo
   replantás donde quieras). Es la misma gramática que picar un bloque —tocar es
   interactuar, mantener es desarmar—, así que no hay nada nuevo que aprender.
   La señal es `HoldInteractState::tapped`, que ponen los tres caminos de entrada
   al soltar sin arrastrar (`HoldInteractState::release`). Un cofre sólo se
   levanta **vacío** y con hueco en la mochila: en cualquier otro caso se queda
   donde está, que es lo único que no puede salir mal.

**Lo que se coloca, se coloca donde tocaste** (`input::place_point`). **Sin dedo no
hay objetivo** (`AimCursor::targeting`): no hay resaltado ni texto hasta que tocás.
Dos apoyos: una acción en curso **congela** su objetivo (el temblor del dedo no
transfiere el hachazo al arbusto de al lado), y levantar el dedo con un `completed`
sin consumir sostiene el punto unos frames.

**Picado por rectángulo:** los sprites con `custom_size` se pican por su rectángulo
y el alcance se mide **a su pie**, no a su centro — tocar la copa de un árbol agarra
el árbol.

**Menos texto en pantalla:** el bloque de estado nace oculto y sólo habla cuando hay
algo que decir. La chuleta de controles es documentación: **sólo en modo CREATIVO**,
igual que los overlays SOCIEDAD/VIVARIO.

**Mando:** sin dedo, el punto se para 2,2 m al frente y el stick derecho lo gira con
el rumbo (`gamepad_aim_cursor`). Prioridad: touch > pad > teclado.

⚠ **Anti-regresión:** no volver a introducir un cursor conducido por stick ni un
botón USAR en tierra. Si algo necesita "apuntar", se toca.

**UI táctil:** los botones van arriba-izquierda o registrados en `TouchBlockRects`,
o los joysticks roban el tap.

### La mochila: tres pestañas y una paleta que se puede recorrer

La ventana ocupa la pantalla entera menos un borde. Tres pestañas
(INVENTARIO · CRAFTEAR · CONSTRUIR) que se conmutan por `Display`, y la activa se
marca con **borde de acento + texto encendido**, no sólo con el fondo: en la
pantalla del teléfono a plena luz las tres se veían casi iguales.

**El botón MENU NO vive acá.** Estuvo un tiempo metido en el header de la mochila
y no funcionó: para pausar o mirar el mapa había que abrir primero la bolsa, dos
toques y un rodeo. Volvió a **arriba al centro** del HUD (`hud::spawn_menu_button`),
que es la única zona libre —vida a la izquierda, energía y reloj a la derecha,
hotbar abajo—. Su rect se registra en `HudTouchBlocks` vía
`game_ui::menu_button_rect` para que el toque no se lo lleve además el mundo.

**El scroll se cortaba por el LAYOUT, no por el tope** (1 ago 2026). El reporte
—"los ítems se quedan recortados y no me deja seguir bajando", en CONSTRUIR— eran
dos bugs de flex encima del tope ya arreglado:

1. A `BagBuildSection` le faltaba **`min_height: Val::Px(0.0)`** (las otras dos
   pestañas sí lo tenían). En una columna flex, un hijo con `flex_grow` tiene
   `min-height: auto` y **no puede encogerse por debajo de su contenido**: la
   sección crecía con las tarjetas, su `BuildScroll` heredaba ese alto y quedaba
   sin nada que recortar. Lo que se veía cortado abajo lo cortaba el PANEL, y
   `ScrollPosition` no movía nada.
2. A los nodos `ScrollContent` les faltaba **`flex_shrink: 0.0`**: dentro de un
   marco `scroll_y` el hijo se encoge hasta entrar, así que lo medido era el alto
   del marco y `max_scroll` daba ~0.

Los dos son la misma trampa de flexbox y aparecen juntos siempre que se arma un
panel que scrollea. Se comprueban sin teléfono con
`bolsa construir agua` + `scroll 99999` del driver `HA_TESTER` (skill `swarm-sim`).

**Además el nombre ya no empuja al costo fuera de la tarjeta.** `spawn_icon_card`
tiene alto fijo y `Overflow::clip()`: un nombre de tres renglones ("Muelle de
tablones") sacaba el costo del recuadro y desaparecía. Ahora el nombre vive en una
caja con `flex_grow` que recorta lo que no entra, y el costo lleva
`flex_shrink: 0.0` — el costo es la mitad de la información de la tarjeta
("¿me alcanza?").

**El tope del scroll sale de MEDIR el contenido** (`hud::max_scroll` + los
marcadores `BagScrollView`/`ScrollContent`), y lo comparten los CUATRO paneles que
scrollean: mochila, cofre, creador de personaje y **el menú de MENU**
(`game_menu::scroll_game_menu`, agregado el 1 ago 2026 — el mapa orbital mide
`Vh(52)` y con la leyenda, el monitor de la base y las tres filas de botones el
menú pasa del alto de la pantalla en apaisado: **AJUSTES y CERRAR quedaban abajo
del pliegue sin forma de llegar**, porque la raíz no tenía `overflow` ni
`ScrollPosition`. Detalle que importa al copiar el patrón: el nodo de contenido
necesita `flex_shrink: 0.0`, si no se encoge hasta entrar en el marco y no hay
nada que recorrer). Era `clamp(0.0, 900.0)`, un número
elegido cuando la lista eran doce recetas: con el pack importado la paleta de
CONSTRUIR pasó a ~350 tarjetas (≈3500 px) y **dos tercios de los objetos del juego
quedaron fuera del alcance del dedo**. Estaban declarados, con nombre e icono, y
el menú no dejaba llegar. Es el bug de "agregaste los ítems y no los veo".
`el_scroll_llega_hasta_el_final_de_la_lista` lo clava.

Dos primos del mismo bug, cazados con él:
- **El INVENTARIO no scrolleaba nada.** Le faltaba `min_height: 0`: en una
  columna flex, un hijo con `flex_grow` tiene `min-height: auto` y no puede
  encogerse por debajo de su contenido, así que la sección crecía hasta contener
  las 8 filas y `scroll_y` no tenía qué desplazar.
- **El creador de personaje tenía `scroll_y` y ningún sistema que lo moviera**
  desde siempre: en un alto lógico corto, el nombre quedaba abajo del pliegue sin
  forma de llegar. Ahora arrastra igual que los otros dos.

**CONSTRUIR se navega por fichas, no scrolleando**: PIEZAS · PAREDES · PREFABS ·
las 8 categorías de `PlaceCat` · FAUNA (`build_groups`). La categoría del
colocable ya existía en los datos —"es la pestaña del menú CONSTRUIR" decía su
propio comentario— pero el menú la usaba de encabezado dentro de una tira única.
Cambiar de ficha **rehace la lista** en vez de esconderla: así abrir la mochila
spawnea entre 6 y 60 tarjetas, no 350 de una sentada. Los índices de paleta
siguen siendo los de las tablas, o sea que `build_selection` no se entera.
Lo cuidan `ningun_grupo_de_construir_esta_vacio`,
`todos_los_colocables_tienen_ficha` y `toda_categoria_usada_esta_en_el_menu`.

**Lo colocado se puede levantar.** Todo objeto colocado nace con un
`InteractTarget` (`structures.rs`), así que el dedo lo encuentra; mantener sobre
él —el MISMO gesto que picar un bloque o levantar un cofre— lo saca del mundo y
devuelve su material (en creativo no devuelve nada: costó cero). El cartel dice
su nombre real ("Manten para levantar Heladera") porque el sprite lleva su
etiqueta del catálogo. El `id` del target empaqueta `(cx,cz)` en 16 bits por eje
y `la_celda_vuelve_igual_de_como_se_empaqueto` lo cuida: con coordenadas
negativas —media Tierra— un empaquetado ingenuo devuelve un positivo enorme y el
objeto se ve tocable pero no se levanta nunca.

☐ **Después**: el editor va a decir POR OBJETO si es interactuable y con qué
acción; hoy todos comparten "levantar", que es lo único que tiene sentido sin esa
tabla.

☐ **Pendiente**: 85 de los 295 colocables no tienen PNG asignado y salen con el
respaldo procedural (25 son `building/*`: casas, escuela, comisaría…). El arte
está en el pack; falta agregarlos a `scripts/pack-objects.tsv` y reimportar.

### Transferir cosas (cofre) — dos columnas y un solo gesto

El panel del cofre era una TORRE vertical (cofre arriba, mochila abajo, a hacer
scroll) y eso rompe lo único que importa en una pantalla de intercambio: **ver
los dos lados a la vez**. Sin eso no sabés si te cabe lo que estás moviendo ni
qué te falta guardar, y cada intercambio cuesta dos scrolls.

Ahora **MOCHILA a la izquierda, COFRE a la derecha**, con las acciones abajo
(`GUARDAR TODO →`, `← SACAR TODO`, `RECOGER`, `CERRAR`). La celda se calcula a
partir del ancho disponible para que las dos rejillas entren: es preferible una
celda más chica que perder de vista la mitad del intercambio.

**Un gesto continuo cubre 1, la mitad y todo** (`chests::split_qty`):

- **Toque** → la pila entera. Es el 90 % de las veces; vaciar un cofre no puede
  costar una ceremonia.
- **Mantener** → pasados 0.22 s una barra recorre la pila **de 1 hasta el total**
  y soltás donde quieras. Arranca en 1 a propósito: "quiero sacar un solo palito"
  era justo lo que no se podía hacer, y es lo primero que ofrece el gesto.

La barra se pinta sobre la propia celda (`ChestSplitBar`) — sin ese feedback
"mantener" sería adivinar. El reloj vive en un sistema aparte (`tick_chest_press`)
porque `Interaction` sólo avisa de los CAMBIOS y el dedo quieto no genera
ninguno. **Nada se pierde en el camino**: si el destino está lleno, sólo se
descuenta lo que realmente entró.

### Escalas

Jugador **1.9 m**; árboles 5.6–8 m de alto y anchos; casas por tiles a 1 m/tile.
La proporción está calibrada: no retunear por estética.

**La referencia es el JUGADOR, no el tile.** Una pared/puerta se dibujaba a
`WALL_RENDER_H = 1.5` tiles, o sea **más baja que quien la cruza** — de ahí que
la puerta se viera diminuta en el mundo. Ahora `WALL_RENDER_H = 2.4` (≈1.26× la
altura del jugador, la proporción de un umbral real) y las puertas de TILE usan
la misma medida que las de borde vía `building::door_render_size`, apoyadas en su
pie. No toca la colisión: el grosor caminable sigue siendo `WALL_THICK`.

☐ **Pendiente**: escala por asset editable desde el editor. Hoy el tamaño sale de
las celdas del recorte, así que un asset cuyo arte no está dibujado a 1 m/tile
(casas de packs, sobre todo) queda chico y no hay forma de corregirlo sin
reexportar el PNG.

### Colisión: no chocar con nada y no comerse las esquinas

Dos problemas distintos que se sentían igual de mal:

- **Quedarse trabado en una arista.** Caminabas pegado a una pared, el pico de un
  bloque te rozaba dos centímetros, el eje se cancelaba y te quedabas empujando
  sin avanzar. No es que no quepas: te sobra el 93 % del ancho, te falta un pelo.
  `topdown::corner_slip` corre al jugador lo mínimo para librar el pico —como los
  plataformeros—, probando de menos a más y quedándose con el desplazamiento más
  chico que funciona. Tope `CORNER_SLIP = 0.16 m` y nunca más de lo que avanzás
  en ese frame, así que **no puede meterte por un hueco que no existe**: si hace
  falta más que un roce, chocás como siempre. Lo fijan dos tests
  (`a_corner_does_not_swallow_you`, `corner_slip_never_opens_a_solid_wall`).
- **Pared invisible.** La colisión que no coincide con lo que se ve. La respuesta
  es de AUTORÍA, no de código: pintá la huella con el pincel de ¼ de tile
  (`Assets → 🗺 huella`), que perfila la silueta en vez del tile entero. Sin
  huella pintada el asset bloquea su celda ancla completa, y ahí sí se siente un
  muro donde no hay nada.

---

## §8 Assets y editores — HA Studio

`./editor_assets` abre **HA Studio**: una sola ventana con cinco espacios de trabajo
(como los de Blender). `F1`…`F5` cambian de espacio.

| espacio | tecla | qué escribe | quién lo lee |
|---|---|---|---|
| 🧩 **Assets** — objetos, arte, variantes | `F1` | `native/crates/ha-game/assets/art/manifest.json` (+ PNG en `assets/art/`) | el juego (`art2d.rs`) y el espacio Mundo |
| 🖌 **Imagen** — compositor multi-PNG, píxeles, pincel de LUZ | `F2` | PNG en `assets/art/` | el espacio Assets |
| 🗺 **Mundo** — mapa WYSIWYG | `F3` | `native/crates/ha-game/assets/world_edits.json` | el juego (`world_edits.rs`) |
| 🔊 **Sonido** — mini-DAW procedural | `F4` | `native/assets/audio.json` | el juego (`audio.rs`) |
| 🧬 **Fauna** — ADN de fauna/NPC | `F5` | genomas `.adn` | el juego (`fauna.rs`, `society.rs`) |

Están conectados: al entrar a Mundo se relee el manifest de Assets, así el objeto
que acabás de crear ya aparece en la paleta. Cada editor se abre también solo:
`./editor_assets assets | mundo | sonido | gym-gui`.

Los archivos van **embebidos al compilar** (`include_str!`): después de guardar hay
que recompilar (`./editor_assets play`).

El botón **📄 Catálogo** del espacio Assets vuelca a `native/assets/CATALOGO.md` la
lista de slots con arte + la tabla de objetos con variantes. Es un artefacto
**regenerable** (gitignoreado) para pegarle a la IA; **no** es fuente de verdad —
la verdad son `slots.rs` (qué IDs existen) y `art/manifest.json` (qué pixel tiene
cada uno).

> Hubo un segundo sistema de assets basado en `manifest.ron` (`ha-assets::{manifest,
> catalog, registry, scan}` + subcomandos `scan`/`catalog`) que nunca se usó: el
> manifest quedó con 0 assets y `ha-game` jamás lo leyó. **Borrado el 31 jul 2026.**
> Si ves referencias a `manifest.ron` en algún lado, son de esa etapa.

⚠ El **Image Lab** aplana siempre a **UN** PNG (`art/edited/`): el runtime nunca
carga varios PNG por slot para componer.

### Generador de personaje (capas + retratos)

Dos packs comprados alimentan a la gente del juego, y los dos se **hornean con la
herramienta**, nunca a mano:

```bash
./editor_assets chargen --capas          # personajes + capas + retratos
```

| Qué hace | Sale en |
|---|---|
| Hornea 20 aldeanos + el jugador default (personajes CERRADOS) | `art/npc/char_XX.png`, `art/player/character.png` |
| Exporta las CAPAS sueltas recortadas | `art/chargen/{cuerpo,ojos,ropa,pelo,accesorio}/NNN.png` |
| Exporta los RETRATOS alineados por índice | `art/retrato/{piel,ojos,pelo,accesorio}/NNN.png` |
| Escribe el índice (etiquetas en castellano + look de cada aldeano horneado) | `art/chargen/index.json` |

Decisiones que **no** hay que deshacer:

- **TODO el arte del juego es de 32×32. Mezclar con 16×16 está prohibido.**
  (1 ago 2026, pedido explícito.) El síntoma que lo destapó: en el editor, una
  hoja con grilla de 32 mostraba DOS cuadros por celda — porque esa hoja estaba
  dibujada a 16. Los props ya salían de las carpetas `32x32` del pack; lo único
  que quedaba en 16 era el personaje, que se exportaba de `Character_Generator/
  */16x16` (el downscale del autor) con celda 16×32. Ahora:
  `chargen_index::CELL = [32, 64]`, el catálogo de `--capas` se lee de `32x32` y
  `--scale` vale 1 (sin downscale). El personaje **se dibuja igual de grande** —su
  tamaño en el mundo lo fija `custom_size` en metros, no la textura—, lo que
  cambia es que tiene el doble de píxeles por lado.
  - Se regenera con `./editor_assets chargen --capas --player` (20 s). Verificado:
    tiras `1792×1280`, horneados `1792×1312`, `index.json` con `cell [32,64]`, y
    el `manifest.json` sin una sola grilla de 16 (58 de `32×32` + 21 de `32×64`).
  - **Costo pendiente de medir en el teléfono**: la textura compuesta de un
    personaje pasa de 896×640 (2,3 MB RGBA) a 1792×1280 (9,2 MB). Con los 20
    aldeanos horneados residentes eso son ~190 MB de textura en vez de ~47 MB.
    Si el Redmi no lo aguanta, **el knob es `STRIP_ROWS`**: hoy se exportan las 20
    filas del pack y el juego sólo reproduce `still`/`idle`/`walk` (filas 0-2).
    Bajarlo a 3 deja el arte en 32×32 y la memoria por debajo de la de antes.
- **Las capas se exportan enteras: 56×20 celdas** (`chargen_index::STRIP_*`),
  o sea el pack completo (dormir, sentarse, teléfono, pelear…). Antes eran 28×3 y
  el resto se tiraba; esas animaciones son gameplay futuro. Ver arriba: son
  también la palanca de memoria si hace falta.
- **El retrato es 64×64, no 32×32.** La carpeta del pack se llama `32x32` por el
  TILE, pero una cara ocupa 2×2 tiles — medido abriendo la hoja, no supuesto por el
  nombre. Se exportan **las 30 caras** del pack (`PORTRAIT_EXPRESSIONS`): antes se
  recortaban 6 para ahorrar 4 MB, pero el peso no es el problema y ahora las
  burbujas muestran retrato, así que conviene el repertorio entero.
- **Las caras del pack NO vienen etiquetadas.** Mirando la hoja al 10× la mayoría
  son variaciones finas de mirada y boca, no emociones: sólo seis se leen sin
  ambigüedad (NEUTRA 0, DORMIDA 2, CONTENTA 4, ENOJADA 12, TRISTE 15, DE ESPALDAS
  24) y ésas tienen nombre; el resto queda como `CARA NN` en vez de inventarle una
  emoción que no se puede verificar. El valor de `chargen::Expression` **es** la
  columna del atlas — `las_expresiones_apuntan_a_su_columna` ata el enum al
  catálogo para que reordenar la lista no le cambie la cara a nadie en silencio.
- **El exportador borra las etiquetas guía** del pack (`erase_guide_labels`): el
  recorte viejo de 6 columnas las tapaba por casualidad, con la hoja entera
  quedaban letras blancas dentro de las caras.
- **El índice ES la identidad.** `pelo[7]` es el mismo peinado en el catálogo, en
  el retrato y en tu savegame. Reordenar las listas le cambiaría la cara a todos
  los personajes guardados: la herramienta ordena alfabéticamente y nunca filtra.
- **El sorteo vive en `ha-assets`** (`look_from_seed`), no duplicado en la
  herramienta y el juego: si se bifurcara, el aldeano hablaría con la cara de otro.
- El índice va **incrustado con `include_str!`**: es texto chico y así el arranque
  no depende de una carga asíncrona en Android.

El juego compone en runtime (`ha-game/src/chargen.rs`) y cachea por look, con
presupuesto por frame; el resultado es **una** textura, así que el sprite del
personaje cuesta lo mismo que antes. La regla de "un PNG por slot" sigue en pie
para el arte del mundo: esto es un generador de personajes, no un slot.

### La lista se lee en castellano

Cada objeto se muestra por su **nombre** (Puerta, Pared de piedra, Peñasco grande
2x2), con la clave del código debajo en chico. El buscador acepta las dos. Los
nombres viven en `ha-assets/src/slots.rs` (campo `label`) y el test
`every_slot_has_a_human_label` no compila si falta uno.

**Toda la construcción está en la lista** — piezas de celda, de borde, prefabs y
estructuras grandes. Lo garantizan los tests de
`ha-asset-tools/src/slot_audit.rs`: si entra una pieza nueva sin slot, falla el
test y no el usuario un mes después. **Sin arte nada queda en blanco**: una puerta
es un cuadrado café, el piso de madera uno marrón claro.

### Animaciones

La matriz cubre quieto, caminar, correr, disparar, apuntar, cavar, talar, dormir,
sentarse, comer, saludar… por **4 u 8 direcciones**. Cada casilla `acción ×
dirección` es una animación con nombre canónico (`walk_down`, `shoot_up_left`,
`sleep`).

1. Elegí el slot y su PNG · 2. marcá el **primer frame** de una fila · 3. `toda la
fila →` completa la cantidad · 4. tocá la casilla y 💾.

**⚡ auto-slice:** marcá el primer frame de la primera fila, elegí el orden del
atlas (`acción→direcciones` o `dirección→acciones`) y la cantidad de filas, y se
escriben todas de una.

El juego cae con gracia: pide `walk_down_left`, si no está usa `walk_left`, después
`idle_left`, por último `idle_down`. Si sólo sliceaste un lado, el opuesto se
espeja solo.

**Varios PNG por asset:** cada pose puede traer su propio PNG y su propia grilla.
`📥 importar VARIOS PNG` deduce del nombre del archivo qué animación es
(`Body_Idle_Left` → `idle_left`) y de sus dimensiones cuántos frames tiene.

**Estados (puertas, cofres):** panel 🚪 con la cadena `cerrada → abriéndose →
abierta → cerrándose → cerrada` más `bloqueada`. Las dos transiciones son
opcionales; si las guardás se reproducen enteras (modo `once`) y la duración sale
de los frames de la propia animación. Para varios estados en un PNG usá **rango de
frames** (`del 1 al 6`, `del 7 al 12`).
> ⚠ No uses `Guardar como animación ÚNICA (strip)` para esto: ése escribe la
> animación **base**, y hay una sola.

**Colisión de la puerta:** en el sistema de bordes una puerta **nunca** frena, ni
cerrada. Es deliberado: abrirla no puede dejarte encerrado. Para una puerta que
bloquee, marcala *bloquea SIEMPRE* y usá el estado 🔒.

### Huella (colisión) ≠ capa (qué te tapa)

Son dos ejes distintos **a propósito**, y de esa separación salen todos los efectos
de profundidad. El agua te frena y no te tapa; un techo te tapa y no te frena.
Confundirlos era el bug de "el agua me tapa al jugador": el juego usaba
`Tile::solid()` —que significa *bloquea el paso*— como si dijera *es alto*.

| capa | dónde se dibuja | para qué |
|---|---|---|
| **Suelo** | siempre bajo el jugador | agua, pisos, caminos, alfombras, decals |
| **Subsuelo** | bajo el piso actual | sótanos asomando |
| **Objeto** *(default)* | Y-sort: te tapa si está al sur tuyo | muebles, props, árboles, rocas |
| **Pared** | Y-sort por su fila del **pie** | muros |
| **Techo** | encima de todo, se transparenta al pasar debajo | techos, pisos superiores |
| **Luz** | glow, no tapa nada | ventanas encendidas, halos |

Sin etiquetar = **Objeto**. Por convención `floor/*` y `stairs/*` son Suelo aunque
nadie los toque. La escalera completa de profundidad vive en un archivo:
`ha-game/src/layers.rs`.

**Los dos efectos que se buscaban:**
- *Pasar por detrás de la parte alta de una pared*: marcá el asset **Pared** y
  pintá sólida **sólo la fila de la base** (`⟟ perfilar la base`).
- *Casa de tres pisos donde sólo se oculta el techo*: son varios assets apilados;
  los de abajo van Pared/Objeto y el techo va **Techo** → se desvanece solo
  (`layers::Overhead`, genérico, sin código por edificio).
- *Pisos como niveles reales* es el otro mecanismo: el `z` del editor de mundo
  (`▲ ▼ capa`), que cambia el nivel entero, no el orden de dibujo.

**La huella** se pinta en **Assets → 🗺 huella**, encima del sprite, con pincel a la
vista (🟥 sólido · 🟨 escalón · 🧽 borrar) y barra de zoom:
- `detalle` = sub-celdas por tile: `tile`, `½`, `¼`. Con ¼ la caja perfila la
  silueta: un farol frena en su poste y no en el metro entero.
- `⟟ perfilar al sprite` / `⟟ perfilar la base` la deducen de los píxeles opacos.
  La **base** es la correcta en cenital: el tronco frena, la copa se atraviesa.
- Arrastre pinta, `Ctrl`+arrastre borra, clic derecho fija el ancla.
- 🟨 **escalón** = desnivel que se sube y se baja a pie; se sube y se baja por el
  mismo sitio **a propósito** (un escalón de una sola dirección es una trampa).
- En el juego la colisión del decor se prueba **en metros** contra esas cajas
  (`world2d::decor_hits_box`), no celda por celda. Sólo colisiona lo que declara
  huella o "bloquea el paso" — plantar flores no levanta un muro.

### Variantes (materiales / skins)

Un piso no es un objeto por material: es **un** objeto `floor/piso` con variantes
(madera, piedra, metal…), cada una con su asset. `◻ genérico` edita el fallback.
`◀ ▶` ordenan. En la lista, `⟨2/3⟩` = 3 declaradas, 2 con asset.

En el manifest la variante es la clave `objeto#variante` más su declaración en
`objects`. Del lado del juego:

```rust
art.variants("floor/piso")                     // qué materiales ofrecer
art.resolved_key("floor/piso", Some("madera"))  // clave a dibujar (cae al genérico)
art.variant_key_seeded("decor/rock", celda)     // variante fija por celda: variedad
```

El decor del mundo ya usa la forma `seeded`: cargale 3 variantes a `decor/rock` y el
terreno se ve variado sin tocar la generación ni guardar nada por instancia.

**Nunca inventes claves de variante** — salen del editor.

### Editor de mundo

La paleta sale de `art/manifest.json`: todo lo que tenga arte aparece, con la
miniatura real. `↻ assets` recarga sin cerrar.

- `🌳 Colocar asset` — clic o arrastre. El fantasma muestra el sprite real a su
  tamaño real; el **tamaño sale del recorte** (una casa de 5×4 celdas mide 5×4
  tiles). El campo `escala` lo multiplica.
- **Cae donde está el mouse:** cada asset tiene una **celda ancla** (por defecto la
  columna del medio de su fila del pie) que queda sobre la celda apuntada. La
  cuenta vive en `ha_assets::placement_center_tiles` y la usan el editor **y** el
  juego, así que el fantasma no puede mentir.
- **El amarillo es la colisión, no el tile.** Si el asset no tiene huella se marca
  el ancla con una cruz: ese asset se pisa.
- `🍃 Borrar asset/árbol` saca lo puesto y marca la celda para que el juego tampoco
  genere el árbol ahí. `Ctrl+Z` deshace por trazo.
- **Lo que pintás reemplaza al mundo:** una celda con terreno autorado no genera
  decor procedural (`WorldEdits::decor_free_cells`), igual en el editor y en el
  juego. Se aplica con `TileWorld::author_tile`, que respeta la capa, pisa
  cualquier cosa y no programa rebrote.

### Formato del manifest

Campos opcionales — los manifests viejos cargan igual.

```json
"npc/cuerpo": {
  "png": "art/atlas/cuerpo_izq_1a2b3c4d.png",
  "grid": [32, 32],
  "poses": {
    "idle_left": { "rect": {"col":0,"row":0,"w":1,"h":1} },
    "idle_down": { "rect": {"col":0,"row":0,"w":1,"h":1},
                   "png": "art/atlas/cuerpo_abajo_5e6f7a8b.png", "grid": [16,16] }
  }
}
```

Huella: `res` = sub-celdas por tile (ausente = 1); `solid` va en sub-celdas de la
grilla `(w*res) × (h*res)`; `ax`/`az` son la celda ancla en **tiles**.

```json
"prop/farol": {
  "png": "art/prop/farol.png",
  "footprint": { "w":1, "h":2, "ax":0, "az":1, "res":4,
                 "solid": [[1,6],[2,6],[1,7],[2,7]] }
}
```

`world_edits.json` — `decor[].size` en metros y `cleared[]` (celdas sin decor
procedural):

```json
{
  "decor":   [{ "body":0, "z":0, "x":12, "zc":-4, "key":"building/casa_chica", "size":[5.0,4.0] }],
  "cleared": [{ "body":0, "z":0, "x":13, "zc":-4 }]
}
```

Las hojas se copian **direccionadas por contenido** (`art/atlas/<nombre>_<hash>.png`):
dos slots que recorten el mismo atlas comparten archivo y el APK no se infla.

---

## §9 Build, publicación y comandos

**El atajo** — un solo comando, pensado para correrlo sin la IA:

```bash
./build-y-publicar.sh              # build completo + tests + publica + sirve
./build-y-publicar.sh sin-tests    # salta los gates de test (evita OOM en PC de poca RAM)
./build-y-publicar.sh remoto       # hornea la IP de Tailscale en el APK (jugar fuera de la LAN)
./build-y-publicar.sh remoto sin-tests
./build-y-publicar.sh rapido       # NO recompila: sirve el APK ya publicado
./build-y-publicar.sh editor       # abre sólo HA Studio
./build-y-publicar.sh sonido       # abre sólo el editor de sonido
./build-y-publicar.sh mundo        # abre sólo el editor de mundo
./build-y-publicar.sh herramientas # hub web de herramientas sobre Tailscale
```

Cuando termina imprime la URL. Abrís esa URL en el celu (misma WiFi), descargás el
APK, lo instalás y jugás. Dejá la terminal abierta: mantiene vivos el servidor y la
landing.

### Comandos npm

```bash
npm install
npm run play              # APK + server + landing
npm run play -- --no-ship # sólo levanta server + landing (APK ya publicado)
npm run ship:android      # sólo build + publicar APK
npm run dev:server        # :2567
npm run dev:client        # landing :5173
npm run test              # vitest de packages/shared
npm run test:native       # cargo test (ha-shared, ha-assets, ha-game)
npm run validate:arch     # límites de arquitectura
npm run qa:suite          # corre todos los drivers de QA y agrega bugreport.jsonl
cd native && make run     # cliente desktop
cd native && cargo check -p ha-game
node scripts/lib/free-disk.mjs   # purgar caché de compilación a mano
```

### El disco se llena — y el síntoma engaña

101 GB en total y `native/target` sola se come ~24 GB. Cuando llega a cero **no
sale "no space left"**: el linker muere con `ld terminated with signal 7 [Bus
error]`, cargo se cae sin mensaje y los procesos de fondo aparecen "killed". Es
media hora buscando un bug de compilación que no existe. Ante cualquiera de esos
tres síntomas: **`df -h /` primero**.

`npm run ship:android` ahora purga solo la caché incremental al terminar
(`scripts/lib/free-disk.mjs`) y avisa fuerte si quedan menos de 5 GB. Sólo borra
lo regenerable: los `deps` de cada perfil no se tocan, porque borrarlos no es
limpieza sino una recompilación completa de castigo. Si hace falta el grueso,
`cargo clean` es manual y a conciencia.

### Requisitos (una vez)

Node 18+, Rust con `rustup target add aarch64-linux-android` + `cargo install
cargo-ndk`, Android SDK + NDK (`scripts/lib/android-env.mjs` los autodetecta).

### Paso a paso manual (si el script falla)

```bash
node scripts/detect-lan-ip.mjs                                    # a) IP de la LAN
node scripts/ship-native-android.mjs --host 192.168.1.X --skip-tests   # b) APK + publicar
VITE_SERVER_HOST=192.168.1.X UPDATE_BASE_URL=http://192.168.1.X:2567 npm run dev:server &
VITE_SERVER_HOST=192.168.1.X npm run dev:client                   # c) servidor + landing
curl -s http://192.168.1.X:2567/updates/manifest                  # verificación
```

### Acceso remoto (Tailscale)

El Redmi ya está en la tailnet. `./build-y-publicar.sh remoto` (o
`--host 100.83.97.54`) hornea la IP de Tailscale en el APK: se descarga y se juega
dentro o fuera de casa, con Tailscale ON en el celu.

**Flujo de actualización:** `ship:android` bumpea el `versionCode` por defecto
(`--no-bump` para no hacerlo); el manifest OTA es `mandatory`, así que relanzar la
app en el celu actualiza sola.

---

## §10 Rendimiento

Escrito para leerse en frío. Objetivo: **60 FPS sin tirones en el Redmi**.

### RESUELTO (1 ago 2026) — el suelo en `Blend` era el 65 % del frame

La ablación por capas (rondas 1-3, `debug/ablate.rs`) cerró la atribución. Medido
en el Redmi, misma escena, configuraciones alternadas cada 12 s:

| cfg | qué cambia | fps | frame_ms |
|---|---|---|---|
| 0 | base (suelo `Blend`) | 12,5 | **81,0** |
| 2 | escala de render 0,7 | 22,4 | 45,1 |
| 1 | **suelo `Opaque`** | 35,6 | 28,3 |
| 3 | las dos | 55,2 | 18,2 |

`spr_vis` (~260) y `lotes` (~90) quedaron IGUALES en las cuatro. No era cantidad
de sprites ni draw calls: era **el modo de mezcla de la malla que tapa la pantalla**.
El suelo pedía `Blend` las 24 h para usar la transparencia sólo durante el fundido
al satelital; a pie su alpha vale 1,0 y la mezcla no mezclaba nada, pero igual
forzaba leer el framebuffer por fragmento y prohibía descartar los tapados.

**Arreglo** (`world2d::fade_surface_lod`): el modo se elige según `fading`, y se
escribe SÓLO al cruzar el umbral. Verificado con el cable, vuelta completa
superficie→órbita→superficie: **a pie 60-62 fps / 16,2 ms**, en vuelo 43-45 fps,
jitter 0,0-3,0 ms en la transición (la respecialización de pipeline que se temía
no aparece). El fundido de despegue se ve igual que antes.

**La escala de render 0,7 NO se aplicó**: se ve más borroso y el opaco solo ya
alcanza el objetivo. Sigue disponible si algún día hace falta.

Detalle que salió mirando esto: la malla del suelo se arma con POSITION y COLOR
por vértice y nada más (`world2d.rs`, `insert_attribute`), sin UV y con un
`ColorMaterial` vacío — o sea que **el suelo hoy no usa arte de tile, se dibuja
por color plano**. Es lo que vuelve el opaco seguro a pixel. Si algún día se le
pone textura, seguirá andando mientras los tiles de suelo sean sólidos; una
textura con pixeles semitransparentes obligaría a repensar esto.

> **La ablación quedó APAGADA** (`ablate::enabled`). Estaba cableada a encenderse
> sola en todo build de Android y **pisa el `alpha_mode` del suelo en `PostUpdate`
> cada 12 s**: dejarla prendida devuelve el juego a los 81 ms una fase de cada dos
> y arruina cualquier medición posterior. Pasó exactamente eso al verificar este
> arreglo — el fix parecía no funcionar y era la ablación peleándolo.

### RESUELTO (1 ago 2026, tarde) — el bajón de los primeros minutos era el mapa de los doce planetas

**Reporte:** después de la pantalla de carga el juego se laggea **como dos
minutos** y después levanta solo. El mundo ya está armado detrás de la cortina y
aun así ese rato cuesta un orden de magnitud más que el resto de la partida.

**Es `universe::bake_body_textures`.** Medido con la sonda nueva (`ha-sys`, abajo):
**125–384 ms por frame**, o sea el frame ENTERO, durante ~190 frames seguidos.

El disco de cada planeta se pinta con su propio terreno: 512² píxeles × 4 muestras
de `topdown_tile` = **un millón de evaluaciones de ruido por cuerpo**, repartidas
de a 32 filas por frame (16 frames por disco). Eso estaba bien pensado. Lo que
estaba mal era **cuándo se encolaba**:

```rust
let on_screen = body_radius(disc.body) * 2.0 / zoom.live_view_h;
if on_screen > 0.02 { bake.request(...) }     // ← el test viejo
```

`on_screen` compara el TAMAÑO del cuerpo con el encuadre y **no mira dónde está**.
Parado en el pasto, con un encuadre de ~20 m, Neptuno da `on_screen` ≈ 1 000 000:
pasaba el test, igual que los otros once cuerpos. O sea que al entrar a jugar se
encolaban **los doce discos de una vez** — 12 × 16 = 192 frames de 125–384 ms, a
3-4 fps: los dos minutos del reporte, exactos. Y "levanta solo" es la cola
vaciándose.

**Arreglo** (`universe::sync_universe`): al test de tamaño se le suma el de
posición — el disco tiene que estar además DENTRO del cuadro
(`dist < live_view_h * 1.5 + r`). Parado en la Tierra se hornea un solo disco: el
que estás pisando. Los demás se cocinan cuando te acercás, que es lo que el diseño
original decía y el test no comprobaba.

**Y la cortina ahora lo espera** (`loading.rs`): `DiscBakeQueue::pending()` entró
en la condición de salida junto a los chunks y las texturas, y detrás de la
cortina el horneado va a `LOADING_DISC_ROWS` (128) en vez de 32 — mismo trato que
`ChunkBudget`. Un frame de 200 ms detrás de la cortina no es un tirón.

Verificado en escritorio (`HA_STAGES=1 HA_TESTER=1 HA_AUTOPLAY=1`), 1280×720:

| | carga | primer segundo jugable | `update` |
|---|---|---|---|
| antes | 6,0 s | 100–400 ms/frame durante ~50 s | **250 ms** |
| después | 9,0 s | 47–58 ms/frame desde el frame 1 | **5–11 ms** |

3 s más de cortina a cambio de ~50 s de juego injugable.

> **Lo que queda caro y NO es esto:** `universe::sync_terrain_lod` marca
> **318 ms/frame** cuando rehace su malla de 37 000 quads (al cambiar de escalón
> de LOD o al recentrar, o sea volando). Es el tirón del vuelo, no el del
> arranque, y sigue pendiente — misma receta que los chunks: repartirlo o sacarlo
> del hilo principal.

### La sonda que faltaba: `ha-stages` / `ha-sys`

**La conclusión vieja "spans ≈ 0 y `frame` enorme ⇒ es la GPU, el escritorio no
puede responder esta pregunta" era falsa, y así se perdió esta caza.** El motivo:
`ha-perf` imprime seis spans (`chk/terrain/lit/atm/ptc/hud`) pero **sólo tres
están cableados** (`Chunk`, `Lights`, `Hud`). `terrain`, `atmo` y `part` salen 0
porque nadie los escribe. Un sistema sin `span_guard` es invisible **exactamente
igual** que uno barato — y `bake_body_textures` no tenía guard.

`debug/stages.rs` (`HA_STAGES=1`) no depende de que alguien se acuerde de poner un
guard:

- **`ha-stages`** parte el frame por etapa metiendo schedules-marca entre las del
  `Main` (`MainScheduleOrder`): `first/pre/fixed/update/post/last` y **`resto`** =
  lo que pasa entre el fin de `Last` y el `First` siguiente, o sea extracción,
  render, present y vsync. Con una corrida separa CPU de gameplay (`update`), CPU
  de motor (`post`) y GPU/driver (`resto`).
- **`ha-sys`** es el top-12 de sistemas por ms/frame. Se instrumenta con una línea
  al principio del sistema: `let _probe = crate::debug::probe("nombre");`.

```bash
cd native && cargo build -q -p ha-game
HA_STAGES=1 HA_TESTER=1 HA_AUTOPLAY=1 WGPU_BACKEND=gl ./target/debug/ha-game 2>&1 \
  | rg "ha-stages|ha-sys|loading:"
```

**`HA_RES=160x90`** es la otra palanca nueva: achica la ventana 64 veces. Si el
frame no baja, el gasto NO es fill-rate. Fue lo que descartó la GPU en dos minutos
— el frame seguía en 250 ms a 160×90, con el proceso al 100 % de UN core.

**El escritorio SÍ puede responder por CPU.** Lo que no puede es responder por FPS
absolutos (ver "Límites de lo medido"): ahí la 940M/NVK manda y `resto` sale
30–43 ms. Pero `update` es CPU real y comparable con el teléfono.

### La sonda de arranque (`ha-hitch`)

`ha-perf` promedia 5 s y `ha-span` promedia 60 frames — que a 4 fps son 15 s, o sea
que **un bajón entero cae dentro de una sola muestra promediada**. Por eso existe
la sonda de arranque (`debug/perf.rs`, `STARTUP_PROBE_SECS`): los primeros 12 s de
`Playing` se loguean **frame por frame**, con los spans en **delta de ese frame**
(antes eran acumulados desde el último reinicio: durante el bajón el número era la
suma de medio minuto y señalaba al sistema equivocado — se vio salir
`chunk=4136 ms` en un frame de 996 ms).

```bash
adb logcat -c && adb logcat | rg "ha-hitch|ha-stages|ha-sys|loading:|ha-perf|ha-crash"
```

- **`loading:`** dice cómo salió la cortina — `listo en X s` (mundo armado) o
  `se acabo el tiempo` (entró con backlog: ahora también dice cuántos discos
  quedaban).
- **`ha-hitch` con `chunk=` alto** → construir chunks después de la cortina.
- **spans ≈ 0 y `frame` enorme** → **no concluir nada**: encender `HA_STAGES=1` y
  mirar `ha-sys`. Ese atajo ya mintió una vez.

Adivinar ya falló varias veces — ver las correcciones abajo.

**Dónde mirar primero si algo se ve mal:** el overlay de PERF (desktop **F3**,
teléfono **MENU → PERF**) tiene una línea `blit`: `blit off` = la cámara de mundo
dibuja directo a la ventana; `blit 0.80 1920x864` = la escala de render está activa.
**Si el mundo se ve negro**, esa línea dice si el culpable es la escala de render.

### El costo de construir un chunk — la ganancia grande que queda

| Sistema | µs por frame (escritorio) |
|---|---|
| `stream_tile_chunks` | **59 583** (picos 124 817) → 14 272–19 059 tras el arreglo |
| luces (`light2d`) | 92–424 |
| HUD | 16–19 |

**Cada chunk cuesta ~6–12 ms de construir.** Con el presupuesto viejo en 10
chunks/frame, llenar el anillo se iba a 60–124 ms: un solo sistema comiéndose de 4 a
8 frames enteros, mientras luces y HUD juntos no llegaban al 1 % de eso. Se bajó
`CHUNK_BUILD_BUDGET_PER_FRAME` de 10 a 2 — **contiene el síntoma, no lo cura**.

Pendiente de fondo: `world2d.rs::spawn_chunk_decor` recorre las 256 celdas del
chunk y por cada pieza llama a `light2d::attach`, que spawnea entidades hijas.
→ `spawn_batch` en vez de spawns individuales · no adjuntar luz a lo que no emite ·
construcción de malla fuera del hilo principal (`AsyncComputeTaskPool`) · cachear la
malla del chunk.

### El "titila verde" — el anillo se armaba justo al borde de la pantalla

`range` se calcula para llenar exactamente la pantalla (`live_view_h * 1.25`) y
los chunks se construían justo hasta ahí, a 2 por frame. O sea que **un chunk se
armaba el frame en que ya se veía**: caminando, el jugador miraba nacer los
rectángulos de suelo en el borde. Se lee como un parpadeo verde porque el pasto
sin arte asignada es un quad de color plano, así que cada chunk nuevo es un
rectángulo verde que aparece de la nada.

Arreglo: `world2d::BUILD_MARGIN` (1 chunk). El suelo se arma **un anillo antes de
entrar en cuadro**; el envolvente de supervivencia va un paso más afuera para que
siga habiendo histéresis y el borde no se construya/destruya en frames alternos.

- Cuesta memoria: con `range` 3 pasa de 7×7 = 49 chunks a 9×9 = 81, y la
  decoración medida en escritorio subió de 3691 a ~4335 sprites (+17 %). **Si el
  Redmi no llega, ese es el knob** — `BUILD_MARGIN = 0` restaura lo de antes con
  el pop-in incluido.
- El tope duro (`MAX_CHUNK_RANGE = 7`) manda por encima del margen: pasados
  ~77 m de vista el anillo ya es más chico que la pantalla **a propósito**,
  porque ahí el detalle fino se funde contra el satelital (`FINE_FADE_VIEW_H`).
- Lo clava `world2d::streaming_tests::el_suelo_se_arma_fuera_de_cuadro`.

### Batching — la otra mitad

El teléfono reportó **2916 sprites de decoración** con frames de 356 ms y jitter de
sólo 13.5 ms. Jitter bajo sobre un frame enorme = costo parejo, la firma de miles de
draw calls, no de un tirón puntual. Bevy 2D agrupa sprites que comparten textura **y
son contiguos en el orden de dibujo**; con orden Y y muchas texturas intercaladas,
los lotes se rompen. La solución de fondo es **un atlas común para la decoración**.
Mientras tanto se bajó el conteo con `DECOR_FULL_RANGE`.

### Cambios aplicados

| # | Cambio | Archivo |
|---|---|---|
| 1 | `UpdateMode::Continuous` en Android. Era `WinitSettings::mobile()` = `reactive`, que actualiza **con cada evento de entrada**: con el dedo en el joystick (táctil a 120–240 Hz) el juego corría 2–4× más seguido de lo que presenta a 60. **Ésa era la causa del parpadeo**, y por eso empeoraba justo al tocar. | `lib.rs` |
| 2 | `Msaa::Off` en las dos cámaras. `Camera` exige un `Msaa` y su `Default` es `Sample4`: corríamos con multisampling 4× en un juego de sprites `nearest`. | `camera.rs`, `render/scale.rs` |
| 3 | Fin del relayout de UI por frame (reloj, botones USE/JUMP y su `font_size`, sección CONSTRUIR). Tocar un `Node` lo marca sucio y taffy recalcula el árbol entero. | `hud.rs`, `input/actions.rs`, `build.rs` |
| 4 | Luces sin cambios de arquetipo por frame; pasada cara a 20 Hz + tinte a 60 Hz. | `light2d.rs` |
| 5 | Anillo de vuelo 12→7 chunks, techo `MAX_LIVE_DECOR`, presupuesto 10→2, `DECOR_FULL_RANGE` (2 en Android). | `world2d.rs` |
| 6 | Escala de render: mundo a 0.8× con el **HUD a resolución nativa** encima. | `render/scale.rs`, `input/calibration.rs` |
| 7 | APK en `assembleRelease` (era `assembleDebug`, o sea `debuggable=true`); sin features 3D (`bevy_pbr`/`bevy_gltf`/`bevy_scene`). | `scripts/build-android.sh`, `android/app/build.gradle`, `Cargo.toml` |
| 8 | `world2d` y `light2d` condicionados a `GamePhase::Playing`. | `world2d.rs`, `light2d.rs` |
| 0 | Instrumentación: capturas automáticas nunca en Android (eran 2 tirones garantizados por partida); `CpuSpanStats` con atómicos (con `ResMut` el profiler **serializaba lo que medía**); `sample_frame_timing` con `Time<Real>`; overlay a 4 Hz. | `debug/perf.rs`, `debug/cpu_span.rs` |

**El mundo negro (arreglado).** Dos causas en `render/scale.rs`: (1)
`Bgra8UnormSrgb` como render target funciona en escritorio pero en GPUs móviles
(Adreno/Mali) los formatos BGRA suelen existir sólo para la swapchain → ahora es
`Rgba8UnormSrgb`; (2) `ClearColorConfig::None` en la cámara de blit dejaba el
framebuffer sin inicializar al fallar el blit → ahora limpia cuando el blit está
activo y **no** limpia con escala 1.0.

⚠ **`IsDefaultUiCamera` vive en la `BlitCamera`**, no en la cámara de mundo. La
cámara de blit se crea siempre (con escala 1.0 sólo se salta el quad), así que la UI
siempre tiene dueño. Si alguna vez se elimina ese módulo, hay que devolverle
`IsDefaultUiCamera` a `GameCamera`.

### Suposiciones que resultaron FALSAS — no volver a "arreglarlas"

0. **"Los spans dan casi cero, así que es la GPU."** Es la que más caro salió: dos
   meses de sección apuntando al render cuando el culpable era un sistema de CPU
   sin instrumentar (`bake_body_textures`, 250 ms/frame). **Un span en 0 no
   significa barato, significa que puede que nadie lo mida.** Antes de concluir
   nada: `HA_STAGES=1` (dice si es `update` o `resto`) y `HA_RES=160x90` (dice si
   es fill-rate).
1. **Tonemapping y deband no costaban nada.** En 2D vienen apagados de fábrica. De
   las tres "cosas caras que no aportan", **sólo MSAA era real**.
2. **El joystick ya estaba optimizado** (`bypass_change_detection` + `set_changed`).
   Faltaba extender ese patrón a los botones de acción y al reloj.
3. **El contador de FPS mentía.** Usaba `Res<Time>` (reloj virtual), que recorta el
   delta a 250 ms: **no podía reportar peor que 4 FPS** y se clavaba justo cuando el
   frame se iba de verdad. Ya usa `Time<Real>`.
4. **El medidor se medía a sí mismo.** Reescribir su texto cada frame obliga a
   re-medir tipografía y relayoutear ~120 nodos. Tener el overlay abierto —lo que
   uno hace justo para diagnosticar— empeoraba el bajón. Ahora va a 4 Hz.
5. **Bajar `light_sprites` a 20 Hz a secas reintroduce parpadeo** (ver §7).
6. **El plan original estaba mal enfocado, y sólo se supo al medir.** Atacaba costos
   de microsegundos; el gasto real era el streaming de chunks, en decenas de
   milisegundos. Instrumentar primero no es ceremonia.
7. **Subir las muestras del mapa satelital no es gratis.** La malla adaptativa
   (`universe.rs`, `LOD_QUADS² = 37 000` quads) se rehace de golpe en un frame. Al
   probar subir `taps`, `HA_FLYTEST` pasó de `fps=4 / chk=54 ms` a `fps=1 / 1399 ms`.
   **Queda en 2.** Lo que hacía falta para leer el mundo desde el aire era el
   sombreado compartido, no más muestras.

### Límites de lo medido

Las mediciones de CPU son de **escritorio con GeForce 940M + driver NVK (Mesa
experimental)**: ahí los frames de 100–400 ms son de esa GPU, no del juego (el CPU
medido eran 14–19 ms). **Los números de CPU y de entidades son válidos y comparables
entre sí; los FPS absolutos de escritorio no dicen nada sobre el Redmi.**

`DECOR_FULL_RANGE` y el presupuesto de chunks son palancas elegidas por **evidencia
indirecta**, no diagnósticos confirmados. El logcat las confirma o las descarta.

**El cierre al volar nunca se confirmó.** Se atacó por hipótesis (techo del anillo
de chunks + `MAX_LIVE_DECOR`). Si vuelve a pasar: `adb logcat -s ha-crash` — el
reporte ya se escribe (`debug/crash.rs`).

---

## §11 Verificación

```bash
npm test                                  # TS (packages/shared) — 101 al 31 jul 2026
npm run test:native                       # ha-shared + ha-assets + ha-game — 221
npm run validate:arch                     # límites de arquitectura
UPDATE_CATALOG_PARITY=1 npm test          # tras tocar catálogos

# Los DOS builds, siempre: hay código gateado por cfg y uno solo no alcanza.
cd native
cargo check -p ha-game --features desktop
cargo ndk -t arm64-v8a check -p ha-game --features android
```

### Drivers de QA — el juego se prueba solo, en la compu

Manejan el juego por el **pipeline real de input** (ButtonInput virtual en
`PreUpdate`) y afirman invariantes; sacan capturas PNG al scratchpad. Sirven para
depurar sin el Redmi. Skill: `swarm-sim`.

| Driver | Qué prueba |
|---|---|
| `HA_FLYTEST=1` | Despegue → órbita → abort a mitad del descenso → aterrizaje. Exige zoom continuo e invariantes de aterrizaje. |
| `HA_MOONTEST=1` | Órbita lunar sin white-square loop. |
| `HA_CYCLETEST=1` | Luna/Venus/Marte × 2 aterrizajes por el camino real. |
| `HA_TRIPTEST=1` | Tierra → Luna → aterrizar → despegar → Tierra, todo con teclas, cero saltos de encuadre. |

⚠ **Los presupuestos de tiempo de los drivers cuelgan de la ESCALA del mundo.** El
`HA_FLYTEST` daba 40 s para subir y 60 s en total: números calibrados con la Tierra
vieja de 1.5 km de radio. Con los 30 km actuales la etapa 0 (subir un radio) tarda
~60 s y el driver se declaraba `STUCK` sobre un vuelo perfectamente sano — un test
que falla siempre deja de leerse, que es peor que no tenerlo. Hoy ambos salen de
`CLIMB_BUDGET_S` (`debug/autodrive.rs`). Si se vuelve a mover la escala, se mueve
ese número.
| `HA_BUILDTEST=1` | Levanta un cuarto por el camino real del jugador; comprueba piso, anillo de paredes, rechazo de pared sin piso y que el muro frena. |
| `HA_ECOTEST=1` | Planta al jugador en un yermo, en el mejor bosque y en una ciudad. Afirma: yermo vacío · bosque con manada + depredador + forraje · la cadena no se come a sí misma · la ciudad se puebla sola. (`HA_ECOSETTLE=90` alarga la observación.) |
| `HA_AUTODRIVE=1` | Arnés end-to-end: walk, dig, chop, craft, machine-place, automation, vuelo, oxígeno, place-tile. |
| `HA_TESTER=1` | Tester-IA continuo: juega en bucle (roam/cava/construye/vuelo Ki) buscando crashes y bajones. Comandos on-demand por `tester-cmd.txt`; se mata con `pkill -x ha-game`. |

Complementos: `HA_AUTOPLAY=1` saltea el menú y fuerza updates continuos (winit
throttlea sin foco) · `HA_DAYPHASE=0` fuerza noche · `HA_RENDER_SCALE=0.7` ejercita
el camino de blit en la PC · `HA_NPCSHOT=/tmp/npc.png` captura el panel de NPC (con
su retrato) · `HA_CCSHOT=/tmp/cc` captura el creador de personaje y la pantalla de
carga (`_creacion.png` y `_carga.png`) · **`HA_HITCH=40`** imprime una línea por
cada frame que pase de esos ms una vez en `Playing`, con el segundo desde que se
levantó la cortina y el desglose de CPU. El log periódico promedia cada 5 s y ahí
adentro un tirón de 200 ms desaparece; para cazar "los lagazos al empezar a
jugar" hay que ver el frame suelto.

`npm run qa:suite` corre todos y agrega `bugreport.jsonl` (una línea JSON por
invariante roto, con mecánica, semilla, estado y repro).

**Una fase sin su verificación no está hecha.**

### graphify — el grafo del código

Grafo en `graphify-out/` (`graph.json`, `GRAPH_REPORT.md`). `graphify explain
"Nodo"` · `graphify path "A" "B"` · `graphify update .` (sin costo de API).

**Regla:** antes de tocar un sistema con muchas dependencias (render, marco de
mundo, cámara, cielo), consultá el grafo para ver qué depende de eso. Un refactor de
marco rompió la Luna por no ver que la cámara asumía el marco estático.

---

## §12 Troubleshooting

- **El APK no muestra los cambios.** El `versionCode` se bumpea solo al publicar;
  relanzá la app (la OTA es `mandatory`). Reinstalá si hace falta.
- **APK gordo / crash en equipos 32-bit — ARREGLADO 31 jul 2026.** Dos causas que se
  sumaban: `build.gradle` tenía `abiFilters` fijo en `'arm64-v8a', 'armeabi-v7a'`
  mientras `ship:android` compila **sólo arm64**, y `copyLibcxxShared` copiaba el
  `libc++_shared.so` de **los dos** ABIs sin mirar cuáles se habían compilado. El
  APK salía con un `libc++_shared.so` de 32 bits **sin** su `libha_game.so`: 1.3 MB
  muertos y crash al cargar el motor en ese equipo.
  Ahora `abiFilters` se deriva de `-PabiFilters` (que el script pasa desde
  `ANDROID_ABIS`) y `copyLibcxxShared(ndk, jniLibs, ABIS)` **poda** los directorios
  de ABIs que no se compilan. Verificar igual con `unzip -l APK | grep lib/`: si
  aparece un ABI sin su `libha_game.so`, está mal.
  ⚠ Gradle cachea esto agresivamente. Si tenés que forzar un reempaquetado, borrá
  `build/intermediates/{merged_jni_libs,merged_native_libs,stripped_native_libs}`
  **y** `build/outputs/apk`, o `packageRelease` queda `UP-TO-DATE` y republica lo
  viejo sin avisar.
- **MIUI `INSTALL_FAILED_USER_RESTRICTED`.** Activar "Instalar apps vía USB" en
  Opciones de desarrollador, o instalar por URL desde el celu.
- **El celu no ve la web.** Misma WiFi + la IP de `detect-lan-ip.mjs`, no
  `localhost`.
- **Build "colgado" 10+ min en "Compiling ha-game".** Es normal (Bevy es grande).
  Verificá que sigue vivo con `ps aux | grep cargo` **antes** de matarlo. Si hace
  OOM, usá `sin-tests` o `CARGO_BUILD_JOBS=1`.
- **"No space left on device" durante el build.** `rm -rf native/target/debug` (el
  build de Android usa otro target dir). El incremental solo son varios GB.
- **Lock de cargo ("Blocking waiting for file lock").** Matar el cargo zombie;
  borrar `native/target/**/.cargo-lock`.
- **Cajas `□` en el HUD.** Cadena no-ASCII: la fuente del juego no las renderiza
  (regla 7 de §2).
- **Dumps de save en la raíz del repo** (`inventory.json`, `tiles.json`, `society/`,
  `vivarium/`, `ha-shot-*.png`): son de correr una herramienta desde la raíz en vez
  de `native/`. El save real vive en `native/`. Están gitignoreados; se borran.

---

## §13 Pendientes

### Bloqueado en hardware
- **Medir en el Redmi** (§10) — heap, fps por región, tirones. Es el único
  pendiente real de rendimiento. El usuario avisa cuando conecte el celu.
- **Asignación de texturas** a los slots — trabajo del usuario en HA Studio.

### Sociedad y NPCs (el plan más activo)
| Fase | Qué | Estado |
|---|---|---|
| S1 | Persistencia sana (un solo camino de guardado, `.adn` huérfanos, `Religion`, `home_x/z`, poda de `family_rep`) | ✅ |
| S2 | Higiene de gameplay (salud y alcance del golpe, cooldowns, testigos, cierre del menú por distancia, zonas de stick) | ✅ |
| S3 | Presentación (presupuesto de burbujas, nombre + apellido, **poses direccionales de NPC**) | ◐ falta `dir4(heading)` en `spawn_npc` |
| S4 | Censo, `center` fijo, siembra, `body_rep` | ✅ |
| S5 | **Rutina diaria + necesidades + luces del pueblo al atardecer** | ☐ |
| S6 | Tick abstracto de colonia, roles por escasez, obras, comercio real | ◐ falta obras (`Builder` colocando estructura real), humor de colonia, migración |
| S7 | Refactor `persistent_agents.rs` (fauna + residentes + NPCs comparten patrón) | ☐ |
| S8 | Linaje: parejas, `crossover`, etapas, `traits`, hijos NPC×NPC | ☐ |
| S9 | Hijos del jugador, herencia, cultos múltiples, migración | ☐ |

### Construcción — el refugio propio (plan activo, 1 ago 2026)

La meta: **armarte el refugio como en Last Day on Earth y crecer a lo alto como en
Project Zomboid**, porque las casas del mapa no alcanzan. C1 ya está; el resto es
lo que falta para que el sistema aguante el roleplay que viene detrás.

| Fase | Qué | Estado |
|---|---|---|
| C1 | **Pisos altos de verdad**: vacío en `z>0`, apoyo, caída, rellano, actores por capa, índice de paredes, chapa de piso | ✅ |
| C2 | **Techos y tumbados por celda** + el suelo del 2º como techo del 1º (§5) | ✅ |
| C3 | **Muebles orientados**: pose por rumbo, huella girada, anclaje a pared, "esto va adentro" (§5) | ✅ |
| C4 | **Demoler con el gesto**: mantener sobre piso/grada lo saca y devuelve material (ya lo hacía `break_tile`); las estructuras por `structures::pick_up_structure`. Cubierto por driver. | ✅ |
| C5 | **Integridad** (`integrity.rs`): el derrumbe se propaga hacia arriba y devuelve los materiales (§5) | ✅ |
| C6 | **Sótano con paredes de BORDE**: bajar por una grada ya no estampa muros como tiles — el sótano nace siendo una habitación normal, que se amplía y se demuele como cualquier otra. La cueva natural no talla nada (nunca lo hizo). | ✅ |
| C7 | **Perf en el Redmi con una base grande**: 500+ piezas colocadas. El índice de C1 saca el costo por consulta; falta medir el streaming de sprites de pared/techo y el rebuild de chunks al editar. | ☐ bloqueado en hardware |

Con C2–C6 cerrados, lo que sigue es lo que el mapa está pidiendo: **oficios y
minijuegos en los edificios que ya existen** (vender en el restaurante, atender en
el hospital, el aeropuerto) para que el pueblo se juegue y no sólo se mire. Ya
tiene debajo lo que necesitaba: locales con interior, mobiliario que se coloca
donde corresponde y puertas que se abren.

⚠ **Falta el pixel.** Las tres claves nuevas (`ceiling/madera`, `roof/teja`,
`roof/chapa`) y las poses de rumbo de los muebles (`sur`/`oeste`/`norte`/`este`)
existen como slots vacíos: hasta que se les asigne arte en HA Studio, el techo es
un cuadrado de color y los muebles no cambian de dibujo al girar (sí de huella).
Es el flujo de siempre (§8, regla 7): el código crea el slot, el pixel lo ponés
vos.

### Editor de mundo
- H2.1: pinceles de edificios/prefabs, mover/reemplazar estructura.
- H3: regiones + **plantillas de ciudad** (autorás el layout una vez y el código la
  ubica donde encaja) + spawns/POIs. **Subsume** el hub terrestre y las ruinas
  off-world: se pintan, no se codean.
- H4: render **aditivo** de la capa Luz (una casa ilumina su entorno de noche) +
  render del Subsuelo.
- H5: minimapa, undo/redo global, copy/paste de stamps, hot-reload.
- Sliders de generación base para esculpir el suelo ya definido de cada planeta.

### Vida
- **Gym**: entrenar buscar-agua y huir del depredador. Hoy beber y huir son
  oportunistas, no gym-optimizados. Falta sumar agua + amenaza al sensor
  (`fauna_brain::sense`) y a la arena del gym, y reentrenar `FAUNA.adn`.
- Generalizar el genoma a colonos/enemigos (uno por rol).

### Perf / render
- **Componer también el cuerpo de los aldeanos** (hoy sólo el jugador y los
  retratos). Las 20 hojas horneadas de `art/npc/` pesan 6,9 MB de APK y suben
  896×656 px cada una a la GPU (≈2,3 MB de VRAM × 20) para usar TRES cuadros; la
  tira compuesta mide 448×96 (172 kB). Sale ~45 MB de VRAM y 6,9 MB de APK, y de
  paso cada vecino sería único en vez de repetirse cada 20. Falta: vestir al NPC
  en un sistema aparte (`spawn_npc` ya está en el tope de parámetros de Bevy) y
  limpiar las entradas `npc/char_N` del manifest.
- Atlas común para la decoración (batching, §10).
- `spawn_batch` + malla de chunk fuera del hilo principal.
- **`universe::sync_terrain_lod`: 318 ms en el frame que rehace la malla** (37 000
  quads de una, al cambiar de escalón de LOD o al recentrar). Medido con `ha-sys`
  el 1 ago 2026; es el tirón del vuelo. Misma receta que los chunks: repartir el
  trabajo por frames o sacarlo del hilo principal (§10).
- **Origen flotante**: a distancias enormes la precisión f32 de los transforms se
  degrada; hace falta rebasear el mundo alrededor del jugador.

### Menor
- **El menú principal se desborda en el teléfono** (medido en el Redmi, 2400×1080,
  2 ago 2026): "KI" queda cortado arriba y "SALIR" abajo. La causa es el mismo
  desajuste **físico vs. lógico** que ya mordió en el mapa orbital (§7):
  `boot_ui::ui_scale` calcula `s` con `touch.height` (1080) pero el layout se
  resuelve en otro espacio — los botones salen ~2,6× más grandes de lo que dice
  el código (`menu_button` pide 60 px y en pantalla miden ~156 físicos).
  **No ajustar el `clamp` a ojo**: ese mismo `s` lo comparten el splash, el menú,
  los ajustes y el creador, así que tocarlo a ciegas arregla una pantalla y rompe
  cuatro. Hay que MEDIR el factor (regla: calibrar midiendo, no suponiendo) y
  corregirlo en `ui_scale`, una vez, para todas.
- **La cuadrícula del creador todavía es plana**: ~200 prendas seguidas sin
  agrupar. Las celdas ya son grandes (dos o tres por fila), falta la agrupación
  por categorías con encabezados dentro de cada capa.
- Escalas por asset editables desde el editor.
- Audio: sonidos por bioma, pájaros/viento ambientales, variación día/noche.
- ~~Panel UI 9-slice~~ **hecho (2 ago 2026)**: `art2d::ui_frame_node` corta el
  marco en 9 con `TextureSlicer` (borde = una celda de la grilla del recorte) y
  `skin_ui_from_art` lo aplica a paneles y a TODOS los botones. El arte de
  `ui/button` / `ui/panel` / `ui/slot_empty` ya estaba recortado en el manifest y
  no lo consumía nadie — el HUD entero se seguía dibujando con los colores planos
  de `ui_theme`. Los slots que el usuario todavía no recortó caen al marco
  genérico en vez de quedarse sin arte.
- Normal maps reales para los sprites, si el relieve falso no alcanza.

### Deuda conocida
- **El APK se firma con el certificado de debug de Android**, no con un keystore de
  release (lo avisa `verify-apk-pipeline.mjs`). Sirve para instalar de costado; no
  sirve para Play Store ni para actualizar sobre una instalación firmada distinto.
- El decor sigue sin atlas común (§10, batching).
- Quedan avisos de `dead_code` en `ha-game` (helpers de `ui_theme`, `harvest`,
  `light2d`, `desktop_input`…). **No los borres a ciegas:** buena parte sale de que
  el build de Android compila igual los módulos de escritorio, así que "muerto en
  Android" no quiere decir muerto. Antes de sacar algo, verificá los **dos**:
  `cargo check -p ha-game --features desktop` y
  `cargo ndk -t arm64-v8a check -p ha-game --features android`.

### Descartado a propósito
Time-warp · escala real del sistema solar (**diversión > realismo**) · ensamblaje de
naves pieza por pieza tipo KSP · física orbital real.

---

## §14 Cementerio — lo que ya se quitó, no reintroducir

**Render 3D esférico** (cámara FP, esferas caminables por píxel, AssetFactory de
meshes, toon/outline, domo de cielo Ghibli, floating-origin 3D, shaders custom,
GLB/Blender). Existe sólo en el historial de git. Costaba 80–180 ms de GPU por frame.

**El programa espacial entero**: ship builder, plataforma de lanzamiento, interior de
nave, física de cohete, `RocketFlight`. Ahora volás vos con Ki. `Flight` quedó como
espejo pasivo que escribe `creative_fly.rs`. Se conservan universo, sondas
(`nav_computer`) y efemérides.

**El swap superficie↔órbita** con su cobertura opaca, `orbital2d`, `ORBIT_REVEAL_FACTOR`
y `transition_profile.swap_up/down`. Reemplazado por el marco único (§4).

**El anillo de vida alrededor del jugador** (fauna sembrada al lado y reubicada al
borde opuesto; NPCs sembrados en tu punto de aterrizaje). Reemplazado por la
ecología por regiones (§6).

**La mira y el stick derecho** en tierra. Reemplazados por el toque directo (§7).

**Paredes como tiles de celda.** Reemplazadas por la construcción por bordes (§5).

**Portales / pantallas de carga** para cuevas y pisos. Reemplazados por el swap de
capa con fundido (§5).

**React Three Fiber, Expo, Tauri, `apps/mobile`, `apps/desktop`,
`packages/client-core`, `packages/client-gfx`.** El cliente de juego es `native/`.

**El walk-hub orbital caminable** y los **cielos por escena**.

### Legado dormido — no borrar, no referenciar en contenido nuevo

Sistemas de la versión combate/OASIS que siguen en el repo por reuso futuro
(fauna/peligro): clases scout/warrior/mage, habilidades, oleadas survival, AI
hostiles (`packages/shared/src/sim/`), schema Colyseus. No son parte del concepto
actual pero no se borran.
