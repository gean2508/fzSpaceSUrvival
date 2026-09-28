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
| **Estética** | 2D top-down 3/4 con assets LimeZu Modern; fallback procedural donde falte pixel. UI en castellano con acentos (fuente propia). |
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
7. **La UI se escribe en castellano de verdad.** El juego embarca su fuente
   (`ha-game/assets/fonts/ui.ttf`, ver `fuente.rs`), así que acentos, `¿` y `¡`
   van. Lo que esa fuente no traiga (emoji, `▲`, `←`) sigue saliendo caja `□`:
   eso se dibuja como nodo de UI, no como texto. Lo clava
   `la_fuente_dibuja_todo_el_texto_de_la_ui`, que compara contra los glifos
   reales de la fuente y no contra el rango ASCII. La vieja regla "ASCII-only"
   era un parche por la fuente pelada de Bevy, no una decisión estética.
8. **Los assets se referencian por ID lógico** (`slots.rs`). **Nunca generes PNGs
   ni inventes rects, offsets o pivotes a ojo**: el pixel sale del pack y la
   geometría del recorte, de un importador que la MIDE. El resultado vive en
   `art/manifest.json`.
   > Esta regla decía "eso lo pinta el usuario en el editor". Ya no: ver la
   > regla 13. Lo que sigue prohibido es INVENTAR —dibujar pixeles a mano o
   > escribir un rect a ojo—, no asignar.
9. **Una transición es un cambio de marco de referencia, jamás una cutscene.**
   No bloquea input ni cámara, no aplica patadas de velocidad, no tapa la pantalla.
10. **No romper** `WorldContext` / `RegionId` / `tiles.z` ni las transiciones
    superficie↔espacio. Consultá el grafo (`graphify`) antes de tocarlos y probá
    Tierra↔Luna↔espacio después.
11. **Construcción agnóstica al cuerpo**; sólo la *generación automática* de
    asentamientos es Tierra-only (gated por tabla).
12. **No se poda la conducta emergente.** Si de la evolución salen manadas,
    territorios, sociedades o guerras, se conservan.
13. **NO HUMAN IN THE LOOP.** El proyecto asume que el agente hace el trabajo
    entero y que el usuario **decide**, no ejecuta. (Pedido textual, 14 ago 2026:
    *"al principio se hacía así; ahora me di cuenta de que vos podés hacerlo,
    entonces tratá de hacer todo tuyo, la mayoría de lo que puedas; si realmente
    no podés, ahí intervengo yo. Que la filosofía del proyecto ahora sea no human
    in the loop"*.)

    Qué cambia en la práctica:

    - **Asignar arte es tarea del agente.** Donde antes la respuesta era "creá el
      slot y que el usuario le ponga el pixel desde HA Studio", ahora es *"creá
      el slot y escribí el importador que le ata el pixel del pack"*. Los
      importadores de `scripts/` son la forma canónica: recortan del pack,
      **miden** lo que recortaron (y se plantan si el pack cambió), tiñen
      derivando del arte que ya está en uso y reescriben el manifest.
      `import-suelo-bordes.py` es el ejemplo corto; `import-acantilado.py`, el
      largo.
    - **Un slot vacío es una tarea abierta, no un estado válido.** Si falta arte,
      el orden es: buscarlo en el pack → derivarlo de lo que ya está asignado
      (§8, *"derivar arte de la textura en uso"*) → y sólo si ninguna de las dos
      da, avisarle al usuario qué falta y por qué no se pudo.
    - **HA Studio no se retira**: sigue siendo donde el usuario mira, corrige y
      manda. Lo que se retira es la *dependencia* de que abra el editor para que
      el juego se vea completo.
    - **Y el agente verifica lo suyo.** Sin humano en el bucle, "lo hice" no lo
      dice nadie más: se mide (§11), se corre el driver y se **mira la captura**.

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
   continua y monótona de 30 m a 26 Mm). Cerca del suelo la escala es **honesta,
   1:1**: a 200 m de altura ves 200 m de mundo. Lejos es proporcional al radio del
   cuerpo, y el cruce entre las dos ramas —el momento en que tu mundo se convierte
   en un disco— es el tramo más rápido de todo el vuelo (~×2.6/s). Que se note está
   bien; que parezca un corte, no. Ver §7 (Cámara) para las cotas y sus tests.
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

**Vuelo:** mantener SALTAR carga el aura y despega; SALTAR sube, BAJAR baja, stick
lateral libre. Aura dorada procedural (`fx/aura_saiyan`). El Ki reemplaza al
combustible. **Salvo cuando el botón dice TREPAR** —hay un borde delante bajo
tierra—: ahí no carga nada, porque el botón hace lo que dice (§5).

**Espacio vivo** (`space_fx.rs`): halo radial translúcido (la misma pieza sirve para
atmósfera, corona solar y cabeza de cometa), terminador por cuerpo girando con su
órbita, cinturón de asteroides (220 rocas), cometas en elipses reales con la cola
en contra del Sol, polvo con parallax, calentamiento de reentrada.

⚠ **Dos trampas de render que costaron horas:** las texturas generadas necesitan
**muestreo lineal** (`ImageSampler::linear`) — un halo de 256 px estirado a 150 km
con vecino-más-próximo cuantiza el degradado en anillos concéntricos; y aun con
filtrado, el alfa de 8 bits **bandea**, hace falta **tramado** al generar el
degradado.

#### El limbo tiene su propio color, y NO es el de las nubes (3 ago 2026)

El halo de los cuerpos con aire se teñía con `transition_profile(body).cover_color`.
Para la Tierra eso es `[0.86, 0.89, 0.95]`, casi blanco, y como las capas eran
concéntricas y parejas el planeta terminaba con un **aro blanco** del mismo grosor y
brillo todo alrededor: se leía como una calcomanía pegada al disco, no como aire.

Son dos cosas distintas que compartían un valor:

- `cover_color` = el color de las **nubes vistas desde abajo**. Alimenta las bandas
  de transición, **y ésas no se retunean por estética** (regla vieja, sigue en pie).
- `limb_color(body)` (`universe.rs`) = el color del **aire visto de canto desde el
  espacio**. La Tierra tiene el limbo AZUL.

Y tres capas en vez de dos: dispersión ancha muy tenue, limbo ceñido, y un arco fino
que se **corre hacia el Sol** (`BodyLimb`, posicionado en `sync_terminators`). Ese
corrimiento —0.09 radios, poco a propósito— es lo que rompe la lectura de "aro": el
aire brilla del lado iluminado y se apaga hacia el terminador, como en un planeta de
verdad. Con más desplazamiento el halo se despega del disco por el lado oscuro y se
ve el truco.

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

**La calzada es `Tile::Asphalt`, no basalto** (3 ago 2026). Durante mucho tiempo
las carreteras se pintaban con el tile de la roca volcánica lunar por no tener
uno propio: caminable, pero gris plano, así que ninguna calle del juego se veía
como una calle. Ahora la calzada tiene su tile, su arte (`floor/asfalto`, ocho
losas con grietas y tapas de alcantarilla que el mundo alterna por celda), la
vereda tiene la suya (`floor/vereda`) y encima va la **línea pintada**
(`road/linea`): no es un autotile de región como el agua sino una RED, y las
poses del pack se eligen por CONEXIÓN (`ns`, `ew`, `nse`, `nsew`…).
`roads::lane_mark_at` la coloca sobre el eje de la calzada, que se **mide** —una
celda es eje si tiene la misma calzada a un lado que al otro—, así que sale igual
en la retícula de una ciudad que en una ruta que serpentea. Lo cuidan
`las_calzadas_llevan_linea_pintada` y `la_linea_no_sale_doble`.

**La vereda tiene cosas encima** (`worldgen::vereda_decor_at`): bancos, faroles,
tachos, hidrantes, macetas, carteles. Los 716 sprites de `3_City_Props` existían
desde el primer día y no se sembraban en ninguna parte — por eso las ciudades se
leían como maquetas.

### Las cinco reglas de una ciudad que se sostiene (4 ago 2026)

El reporte fue *"veredas cortadas, calles incoherentes; hay que pulir el mapa
para que sea jugable"*. Mirando el volcado ASCII (`cargo test -p ha-shared --test
citydump -- --ignored`) aparecieron cinco fallos que la suite entera daba por
buenos, porque ninguno rompía un invariante que a nadie se le había ocurrido
escribir. **Ninguno se encontró leyendo código: se encontraron mirando el mapa.**

1. **La calzada va CENTRADA en la línea de la retícula.** Ocupaba los primeros
   `ancho` tiles de la manzana, o sea entera de un lado de la línea: cada calle
   tenía vereda al norte y al oeste y al sur y al este el asfalto lindaba con el
   pasto. *Media vereda en toda la ciudad* — el reporte, literal. Al centrarla
   sale a los dos lados sola, sin caso especial. Y el callejón, que llevaba
   banquina 0.5 y redondeaba a CERO, ahora lleva 1.0.
   → `toda_via_tiene_vereda_a_los_dos_lados`
2. **La clase de una calle sale de la LÍNEA por la que corre, no de la manzana**
   (`roads::axis_class`). Con `street_class(ix, iz)` la misma calle era avenida
   mientras bordeaba una manzana y callejón en la siguiente: recorriéndola se
   ensanchaba de 7 a 3 tiles y volvía.
   → `una_calle_mide_lo_mismo_de_punta_a_punta`
3. **La ciudad se busca en las NUEVE celdas vecinas, no en cuatro.** La celda de
   asentamiento mide 2.6 km y una metrópolis 3.6: es más ancha que la casilla
   donde está anotada. `civil_kind_at` miraba sólo las cuatro que necesita la red
   de enlaces (este y sur), así que **desde la frontera hacia el norte y el oeste
   no había una sola calle — pero sí los edificios**, porque ésos ya buscaban en
   las nueve. Barrios enteros de casas en el campo, con una línea recta donde
   empezaba el asfalto. Además gana el núcleo MÁS GRANDE de los que cubren la
   celda: una aldea fundada pegada al borde le comía 130 tiles a la metrópolis.
   → `la_ciudad_no_se_corta_en_la_frontera_de_su_celda`
4. **La ciudad se acaba en la orilla.** El casco es un cuadrado y a una ciudad
   costera medio cuadrado le cae en el mar: la retícula pintaba calles allá
   afuera y `civil_at` las ascendía a PUENTE. Cada ciudad de la costa tenía
   kilómetros cuadrados de viaducto sobre el océano abierto, con las veredas
   flotando entre ellos. Se recorta en `civil_at` y no en la trama, porque **la
   trama no puede mirar el terreno** (el relieve la consulta a ella para abrir
   rampas; preguntarse mutuamente desbordaba la pila). ⚠ El primer intento usó la
   prueba continental de una octava y NO sirvió: discrepa con el relieve de verdad
   en toda la franja de costa. Manda el tile que se dibuja (`geography::hay_agua`,
   que es el único predicado de "mojado" que vale — había dos y no coincidían).
   → `ninguna_calle_ni_vereda_sobre_el_agua`
5. **Toda carretera desemboca en algo: la circunvalación** (`roads::ring_class`).
   El enlace interurbano llegaba al borde del casco y ahí se acababa salvo que su
   punto de entrada cayera justo sobre una calle de la retícula, o sea casi nunca:
   la carretera moría contra el jardín de alguien. El anillo del perímetro las
   recoge a todas, y de paso le da a la ciudad un límite que se ve y se conduce.
   → `el_casco_tiene_circunvalacion`

Y una sexta, que no es de calles sino de manzanas: **reservar no es ocupar**. Un
hospital reserva 2×2 manzanas y llena una; las otras tres reciben ahora su propio
lote chico mientras no se pisen con él (`Lot::overlaps`). Además el sorteo de
programa se hace **entre los que CABEN** en el tamaño de ancla, en vez de sortear
entre todos y descartar si no cabe — que era rechazo puro y dejaba más de la mitad
de las manzanas del centro sin nada. Y como la calzada centrada hace que la
manzana pague la banquina de sus cuatro calles y no de dos, los `block_size`
subieron ~8 tiles para que el hueco edificable siguiera siendo el de antes.
→ `la_ciudad_no_esta_llena_de_baldios` (en las cuatro ciudades: el agujero se
escondió una vez en el puerto, cuyas manzanas no admitían ningún programa de
centro), `una_manzana_da_para_construir`

**Y de paso, §10:** `settlement_in` tiene memo por hilo. Cada tile de ciudad
pregunta por sus nueve celdas vecinas y un chunk de 32×32 son ~30.000 consultas
sobre las mismas nueve respuestas. Pintar un chunk de ciudad pasó de ~9 ms a
~2-4, y la suite de `ha-shared` de 190 s a ~100 con ocho tests más.

**Y una séptima, del 7 ago 2026: TODA CASA TIENE POR DÓNDE ENTRAR.** Es regla del
jugador y es innegociable — una casa sin puerta es decorado, no un sitio. No
alcanzaba con que `residential_tile` dibujara una: lo que se ve en el mundo es la
composición (`topdown_tile`), donde **la calzada gana sobre lo edificado**, así
que una carretera que rozara la fachada podía borrar la única celda de puerta y
dejar la casa cerrada con llave para siempre. El portal pasó a ser de **dos tiles
de ancho** (casa y edificio macizo), y el patio interior de los edificios grandes
—un rectángulo de losa rodeado de ladrillo, al que no se llegaba por ningún lado—
tiene ahora su propio paso. El barrido lo comprueba sobre el tile FINAL, no sobre
el del lote.
→ `toda_casa_generada_tiene_por_donde_entrar`

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

### Las cuevas se cavan — y las que hay, se encuentran (3 ago → 4 ago 2026)

**Se borró el sistema de cuevas viejo.** No hay galerías procedurales repartidas
por todo el espesor del mundo ni pozos con escalera puestos por el generador. De
`−2` hacia abajo el subsuelo es **piedra maciza** hasta `MIN_Z_LEVEL` (−16), y ahí
el único hueco que existe es el que abriste con el pico (`TileWorld::dig_down`).
Lo vigila `bajo_las_cuevas_el_subsuelo_es_macizo`.

⚠ **Al día siguiente volvieron las cuevas, pero SÓLO en `z = −1` y con boca
caminada** (ver "Las cuevas vuelven, con boca caminada" más abajo). Lo que sigue
valiendo de esta sección es el porqué: entrar no puede sentirse un cambio de
escena. Lo que ya no vale es "no hay cuevas antes que vos".

Por qué, en las palabras del jugador: *"odio las cuevas y cómo entramos a la cueva
actualmente"*. Entrar era caminar sobre un agujero que puso el generador, y a
partir de ahí **la pantalla entera pasaba a dibujar otra cosa**. Técnicamente era
un swap de capa; para el que juega era cambiar de escena y de sitio — justo lo que
§4 promete que no pasa nunca.

**El túnel se dibuja EN EL MISMO MUNDO.** `TileWorld::render_tile` separó dos
cosas que estaban pegadas: *dónde estás parado* y *qué se dibuja*. Caminás por el
subsuelo mirando TU nivel para la colisión —todo es macizo menos lo que picaste—,
y lo que se pinta lo decide `render_tile`. Ver y estar son cosas distintas.

#### Bajo tierra se ve el SUBSUELO (corrección del 7 ago 2026)

Hasta el 7 de agosto esto decía lo contrario: *bajo tierra seguís viendo la
superficie, y lo único que se abre es tu galería, como el corte de un
hormiguero*. La intención era buena —no cortar la escena— pero el mecanismo era
el equivocado, y el jugador lo rechazó con todas las letras: **caminabas por el
subsuelo rodeado de pasto a pleno sol, con árboles y mariposas, y un agujero
negro en el medio.** No se leía "estoy debajo de mi casa"; se leía "el mapa está
roto".

Lo que hay ahora, y por qué no reintroduce el corte:

- `render_tile` devuelve **la planta en la que estás**, en los tres casos. Bajo
  tierra: [`Tile::Subsoil`] —tierra maciza— hasta donde llega la vista, y
  recortado en ella lo que se excavó. Lo vigila
  `bajo_tierra_se_ve_el_subsuelo_y_no_el_pasto`.
- **La continuidad nunca la dio el dibujo.** La dan los cuatro escalones que se
  andan (`escaleras.rs`) y el gradiente de encierro (`interior.rs`), y los dos
  siguen intactos: el driver sigue midiendo `encierro máx 1.00, salto máx ≈ 0.18`.
  Nadie se teletransporta y nada tapa la pantalla.
- **El macizo es TIERRA, no roca gris**, y sólo los tres primeros niveles
  (`topdown::SUELO_DE_TIERRA`); más abajo empieza la piedra, que al picarla sigue
  dando `stone`. El cambio de material cuenta algo en vez de ser un gris parejo.
- **El suelo de la galería es un piso de verdad** (`floor/cueva`, losa de Modern
  Interiors) y **no se dibuja como pozo**. Dibujarlo como pozo —lo de antes— era
  lo que convertía la cueva en una mancha de cuadros negros.
- **Las vetas se ven donde están expuestas** (`world2d::expuesta`): en la pared de
  tu galería o en el suelo abierto. Antes se dibujaba el 1,4 % de TODAS las celdas
  del nivel y el macizo salía salpicado de cuadraditos de colores hasta el
  horizonte. Y sin arte asignado la veta son pepitas dibujadas por código, no un
  cuadrado (`world2d::vetear`).
- **El subsuelo es SU PROPIO sitio, no el overworld con otro suelo** (8 ago 2026).
  Corolario tardío de este mismo cambio, y el jugador lo cazó con una captura: si
  lo que se DIBUJA es el subsuelo, lo que se MUEVE también tiene que serlo. Bajo
  tierra dejan de verse la fauna (`fauna::cull_fauna`), los vecinos
  (`society`), las mariposas y las luciérnagas (`ambient`) y deja de llover
  (`weather::weather_allowed`) — los cuatro miraban `z > 0` y ahora miran
  `z != 0`. Estaban colgados de la regla vieja: cuando bajo tierra se veía el
  pasto tenía sentido ver también sus bichos; ahora quedaban revoloteando sobre la
  tierra maciza.
  Lo que pueble el subsuelo lo tiene que poner el subsuelo, y hoy no pone nada:
  está vacío a propósito, no por descuido.
- Al CAER por un hueco hay 0.28 s de `LayerFade` — los de siempre, ahora también
  hacia el subsuelo. **Y al bajar por una escalera también hay fundido, desde el
  8 ago 2026**: ver "El tramo se oscurece" más abajo. La regla vieja ("no hay nada
  que fundir") cayó.
- Y siguen caídas las reglas que trataban el subsuelo como "otro lugar": no se
  apaga el día por decreto (lo apaga la distancia a la abertura), no se borra el
  cielo, y el clima sigue corriendo en la superficie.

**Y desde arriba se ve la calle.** El otro lado de la misma regla: por el vacío
del borde de un piso alto se ve el nivel de abajo atenuado — el color del suelo
(`VOID_DIM`) y también **los árboles, los coches y los faroles**, apagados con el
mismo factor (`world2d::atenuar_sprite`). Salir a la azotea es asomarse al barrio;
antes era asomarse a una losa flotando en gris.

El pozo se DIBUJA como agujero, no como mancha: corte de tierra → pared → fondo
(`world2d::pit_quads`), tres tonos concéntricos que es como se lee un hueco desde
arriba cuando no hay perspectiva. **El borde va sólo en la ORILLA del hueco**: en
medio de un vacío grande —el que rodea a una azotea, o una galería ancha— un
anillo por celda pintaba una cuadrícula encima del mundo de abajo.

**Cavar es picar, con lo que tengas** (`targeting::can_dig_down`): vale cualquier
cosa que ya sepa picar un bloque, la mano incluida, y cada una tarda lo suyo
(mano 750 ms, hacha 450, pico 350). Pedía pico y estaba mal: el jugador no tiene
cómo adivinar que la herramienta cambia lo que SIGNIFICA apuntar al suelo, y el
resultado fue *"no puedo picar y no veo los voxels"* — la mecánica existía y era
invisible.

### La escalera se camina — 4 ago 2026

**Se borró la grada-teletransporte entera.** `Tile::StairsUp/StairsDown`,
`layer_transition`, el pestillo `LayerLatch`, la sala 7×7 de `carve_stair_room` y
el `shaft_exit` que trepaba un pozo de una: nada de eso existe. El problema no era
el fundido — era que **un tile no es un recorrido**. Pisar una baldosa y aparecer
en otra planta se lee como un teletransporte por suave que sea la transición.

Ahora un tramo es **cuatro escalones en fila** (`ha_shared::escaleras::Tramo`) y
subir es andarlos:

```
         paso 0      1      2      3
        ┌──────┬──────┬──────┬──────┐
plantaP │ ▁▁▁▁ │ ▂▂▂▂ │ ▃▃▃▃ │ ▄▄▄▄ │ → planta alta
baja    └──────┴──────┴──────┴──────┘
s =     0     .25    .50    .75     1
                             ▲ acá cambia de planta: al pisar
                               el ÚLTIMO escalón, no antes
```

- **Todo sale de un escalar**: `progreso(tramo, x, z)`, cuánto llevás andado. La
  planta cambia cuando cruza `CIMA` (0.75) y vuelve cuando baja de `PIE` (0.25).
  Medio tramo de franja muerta **reemplaza al pestillo**: para cambiar de planta
  hay que *andar*, así que quedarse quieto no puede hacerla parpadear, y desandar
  a mitad te devuelve de verdad.
- **El personaje se eleva con el paso** (`escaleras::elevacion`) y el salto que da
  el dibujo al cambiar de planta cancela EXACTAMENTE el que da el mundo. Test:
  `el_cambio_de_planta_no_hace_saltar_al_personaje`. Eso arregla el CUERPO; lo que
  faltaba era el mundo, y lo pone "El tramo se oscurece" (más abajo).
- **Las celdas del tramo existen en las DOS plantas** (`Step` abajo, `Stairwell`
  arriba), así que cambiar de planta jamás mueve al jugador. No hay a dónde
  teletransportarlo.
- **Se coloca entero, con su rellano** (pieza "Escalera", coste × 4). Media
  escalera —sin rellano, o cruzada con otra— deja a mitad de camino sin poder
  seguir ni volver, así que o entra completa o no entra.
- **Del pozo se sale trepando** (`escaleras::trepar_borde`): mirá el borde y
  **pulsá TREPAR**. Es la trepada del bordillo (`vault::VaultBob`) aplicada a la
  verticalidad.

⚠ **Nada de cambios de escena para bajar a una cueva o subir un piso.** Si hace
falta una pantalla, la continuidad está rota.

### Trepar se pulsa, no se acumula — 8 ago 2026

Hasta hoy trepar el borde era **empujar contra la pared 0.45 s** y el juego te
izaba solo. Dos reportes lo tumbaron, y son el mismo:

> *"cuando estoy en el subsuelo me salta y sube al overworld"* ·
> *"no me gusta que me apego y solito sube"*

En el fondo de un pozo hay pared **por los cuatro lados**, así que caminar dentro
del hueco disparaba la trepada sin pedirla y te devolvía a la superficie: el pozo
dejaba de ser un pozo. Y aunque no te sacara de sitio, lo que se decide se pulsa.

- **El botón SALTAR pasa a decir TREPAR** cuando hay borde delante
  (`input::actions::update_jump_button_label`, `escaleras::Trepada::hacia`), y al
  alejarte vuelve a SALTAR. **El texto ES el detector**: sin él el botón tendría un
  segundo significado invisible.
- **Se busca por hacia dónde MIRÁS**, no hacia dónde empujás. Con botón alcanza
  mirar, y así el aviso se enciende al acercarte —que es cuando hay que leerlo— y
  se apaga al darte vuelta.
- **El botón hace lo que dice**: mientras diga TREPAR **no carga el vuelo Ki**
  (`creative_fly::creative_fly_takeoff`). Es el mismo dedo para las dos cosas, y en
  el fondo de un pozo —donde siempre hay pared— pulsar para salir te despegaba y te
  ibas atravesando el subsuelo en vuelo. Lo cazó el driver `HA_BUILDTEST`, que
  empezó a recorrer 148 celdas de roca en línea recta.
- Sigue valiendo sólo **bajo tierra** y sólo hacia una celda **pisable**: trepar
  hacia dentro de la roca es el soft-lock al revés.

⚠ **El hueco que cavaste no se camina más allá de su tamaño.** Las vecinas del
fondo son tierra maciza y frenan (`el_hueco_que_cavaste_esta_rodeado_de_pared`).
Para andar más, hay que picar más. Lo que parecía un agujero en la colisión era
la trepada automática sacándote sola.

### El tramo se oscurece — 8 ago 2026

**Hay fundido al cambiar de planta, y es la escalera.** La regla vieja decía que no
hacía falta porque el cuerpo ya subía liso; el jugador miró la grada y dijo *"como
que me teletransporta"*, y tenía razón: el cuerpo era continuo pero **el mundo
cambiaba de golpe**. Lo que pidió, textual:

> *"cuando estás de abajo para arriba, ya antes de llegar al final, que se vaya
> aclarando el escenario, el overworld, hasta que aparece completamente. Y
> viceversa."*

No se hace con una cortina (§4 sigue en pie: nada tapa la pantalla ni bloquea el
input). Se hace con lo que ya existía — **el encierro** (`interior.rs`) — leído
desde el tramo:

```
  s =  0        PIE                CIMA        1
       │         ┌──────────────────┐          │
       │        ╱   sombra del hueco ╲         │
       └───────┘                      └────────┘
   pie del      el cambio de planta cae      rellano
   tramo        en CUALQUIERA de los dos     de arriba
                bordes de la meseta
```

- **`Subida::tramo` publica el progreso** y `medir_encierro` cruza el encierro de
  las DOS plantas con él: bajando, el overworld se apaga a lo largo de los cuatro
  escalones; subiendo, se enciende. Es una función de dónde estás parado, así que
  desandar medio escalón devuelve la luz — no es una animación disparada.
- **Encima va la sombra del hueco de escalera** (`interior::sombra_del_tramo`,
  tope 0.92). Su meseta cubre exactamente la franja muerta `[PIE, CIMA]`, que es
  donde caen los dos únicos puntos en que `caminar_tramo` toca `tiles.z`. O sea:
  **el mundo se cambia siempre en el instante más oscuro del recorrido**, sin
  tocar la histéresis que evita el parpadeo. Tests:
  `la_sombra_tapa_los_dos_puntos_donde_cambia_la_planta`,
  `el_tramo_abre_y_cierra_en_sus_puntas`, `la_sombra_del_tramo_es_un_gradiente`.
- **Y no es una cortina, es que un hueco de escalera está oscuro.** Eso es verdad
  y por eso se puede dibujar. La sombra se escala por el SALTO entre las dos
  plantas: del salón a tu primer piso no oscurece nada (no hay nada que disimular),
  del pasto a la mina sí.
- **Y la planta se DESVANECE, no sólo se oscurece** (`LayerFade::tramo`). Es la
  otra mitad, y vale para **cualquier par de plantas**, no sólo para la cueva:
  *"sobre planta 0 planta 1 hacé igual que al subir por la escalera: a cierto rango
  se va difuminando el piso 0 hasta que el player termina de subir toda la escalera
  y ahí ya el piso 1 está 100 % render, piso 0 ya no se ve"*.

  `LayerFade` pasó a tener **dos fuentes** y se combinan con `max`:
  - `pulso` — **caer** por un hueco. Instantáneo por definición, así que lo único
    posible es un cuarto de segundo por detrás. Se dispara y decae solo.
  - `tramo` — **andar** una escalera. Acá no hay nada que perseguir: el cambio dura
    lo que dura el paso, así que el fundido es **función de dónde estás**
    (`meseta_del_tramo`), no un temporizador. Sube a `FUNDIDO_TRAMO` = 0.85 en el
    instante del cambio y vuelve a 0 en el rellano.

  Que sea función de la posición trae gratis lo que un temporizador no puede:
  **desandar medio escalón devuelve el dibujo**. No hay una animación disparada
  que haya que esperar a que termine.

  0.85 y no 1.0 a propósito: a 1.0 la planta se borra del todo y lo que queda es el
  color de fondo de la cámara — un fotograma de vacío, que es la "cortina" que §4
  prohíbe. A 0.85 se lee como un disolvido.

- **Una sola curva para las tres cosas.** La luz (`sombra_del_tramo`), el dibujo
  (`LayerFade::tramo`) y el cambio de `tiles.z` salen todos de
  `ha_shared::escaleras::meseta_del_tramo`. Por eso el instante más oscuro, el más
  borrado y el del cambio son **el mismo instante**, y no tres que haya que
  mantener alineados a mano. Tests en `ha-shared`:
  `la_meseta_cubre_los_dos_cambios_de_planta`, `la_meseta_no_tiene_escalones`.
- **El driver mide la bajada beat a beat** (`HA_CUEVATEST` → `cueva-baja-5..9.png`):
  beat 6 pleno sol, beat 7 el cambio en penumbra, beat 8 la galería abriendo. La
  invariante "el encierro no da saltos" ahora **excluye los beats del tramo**: ahí
  la luz SÍ cambia rápido, y tiene que hacerlo.

**Y en el hueco no hay escalera, así que no hay fundido de tramo.** Caer es
instantáneo por definición, y ahí lo que hay son los 0.28 s de `LayerFade` de
siempre. Se decidió **no** dibujar "el pedacito de overworld del agujero" estando
abajo: sería reabrir el mapa-de-dos-capas que se borró el 7 ago (ver §5), y cuesta
una segunda pasada de mundo por frame en el Redmi. Lo que te dice por dónde caíste
es la **luz que entra por el agujero** (`interior::luz_de_boca`) y el borde del
pozo dibujado desde abajo — no una ventana recortada.

### Las cuevas vuelven, con boca caminada — 4 ago 2026

El 3 ago se borraron enteras porque *"odio las cuevas y cómo entramos a la cueva"*.
Lo que estaba roto era **la entrada**, no que existieran. Vuelven en `cuevas.rs`
con tres decisiones que son la misma:

1. **La boca es un tramo de escalera**, no un portal: se baja andando los cuatro
   escalones de siempre, y desde la superficie se ve como lo que es (el arte
   `stairs/down` cortado en el terreno).
2. **Toda la cueva está en UN nivel** (`z = −1`). Se cambia de planta una sola vez,
   al entrar; adentrarse después es sólo caminar. No hay un segundo cambio de capa
   esperándote en el fondo.
3. **La forma es un segmento y un disco** — pasillo de 16-30 tiles + cámara de
   radio 4-7 —, así que "¿esta celda es cueva?" son dos distancias y entra de
   sobra en el presupuesto del generador (test `preguntar_por_la_cueva_es_barato`,
   < 2 µs/celda contra los 8 del chunk entero). El ruido con umbral del sistema
   viejo costaba varias octavas por celda **y por nivel**.

Una boca cada ~160 m, una de cada tres celdas gruesas, y **después** de la obra
civil y de los asentamientos: una escalera abierta en mitad de la avenida es justo
lo que hace impasable un mapa generado. Si una parcela le cae encima, esa cueva se
queda sin esa entrada y se llega cavando.

### El encierro es un gradiente, no un interruptor — 4 ago 2026

`interior.rs` calcula **un número**, `Interior::encierro`: 0 a cielo abierto, 1
enterrado sin ver la luz. Es `smootherstep` de la **distancia a la abertura más
cercana** (5 m → 16 m), más un castigo por cada nivel de profundidad.

```
 boca                    pasillo                      cámara
 ┃ encierro 0 ──────────────────────────────────────► 1
 ┃ ░░░░░░░▒▒▒▒▒▒▒▒▓▓▓▓▓▓▓▓█████████████████████████████
 ┃  luz del día       se apaga           sólo tu antorcha
 ▲ por acá entra la luz (`luz_de_boca`)
```

- **Por qué un número y no un modo "estás en una cueva".** Un modo hay que entrar
  y salir de él, y esos dos bordes SON el corte que el jugador ve. El sistema
  viejo apagaba el día de golpe al bajar un escalón, y eso —más que el fundido—
  era lo que se leía como cambiar de escena.
- **Una abertura es lo mismo en los tres casos**: `Tile::Air` en superficie (tu
  pozo), `Tile::Stairwell` (boca de cueva o bajada al sótano). Por eso no hay un
  sistema de cuevas y otro de sótanos: hay aberturas, y la luz entra por ellas.
- **Mueve el velo de noche** (`light2d::darkness` toma `max(día, encierro)`), o
  sea que adentrarse enciende el mecanismo que ya existía y **la antorcha vuelve a
  ser lo que decide si ves**. Ninguna regla nueva.
- **La luz de la boca** (`interior::luz_de_boca`): una sola `Light2d` blanco-fría
  en la abertura, con la intensidad del sol de arriba. Por eso la entrada está
  clara a mediodía y negra a las tres de la mañana sin que nadie lo haya escrito,
  y una tormenta la oscurece (`weather::daylight_mul` ya entra en `daylight()`).

### Voxel 2D: al sacar un bloque queda la cavidad — 3 ago 2026

El mapa dejó de ser un plano con niveles sueltos y pasó a ser **materia**: cada
celda de cada `z` está llena o hueca, y sacar lo que la llena abre el paso.

- **Picar** (`TileWorld::break_tile`) saca el bloque de al lado y te deja la celda
  abierta en TU nivel: es cavar en horizontal, y era lo único que existía.
- **Cavar** (`TileWorld::dig_down`) saca el suelo que PISÁS: la celda queda en
  `Tile::Air` —el hueco, que se ve y por el que se cae— y en el nivel de abajo se
  abre esa misma celda: la **cavidad**. Bajar es ahora una decisión tuya, no
  encontrar la boca de cueva que el mundo puso.

Tres cosas sostienen la regla, y las tres existían ya para el piso alto — no hay
sistema nuevo, hay uno solo que ahora vale para todo `z`:

- **El hueco se ve**: `tile_render_color` busca hacia abajo el primer nivel con
  algo y lo pinta atenuado (`VOID_DIM`). El pozo en el pasto muestra el fondo de
  la cueva apagado, igual que el vacío del segundo piso muestra la calle. Sin
  esto un agujero se pinta negro y se lee "acá no hay mundo".
- **Por el hueco se cae**: `fall_through_void` ya no se limita a `z > 0`. El pozo
  que abrís en la superficie te deja en la galería de abajo con el mismo fundido
  de 0.28 s que la boca de cueva natural — **cavar y las cuevas son lo mismo**, y
  ninguno de los dos cambia de escena (§4).
- **Del pozo se sale TREPANDO** (`escaleras::trepar_borde`): mirá el borde, el
  botón pasa a decir TREPAR y lo pulsás. Un pozo a pico sin salida es un soft-lock
  y acá no hay salto. Antes el fondo estampaba una grada que subía el hueco entero
  de una; eso se borró con el resto del teletransporte (§14). Y hasta el 8 ago la
  trepada se disparaba sola al empujar, que era el bug contrario: te sacaba del
  pozo sin pedirlo (ver "Trepar se pulsa, no se acumula").
- **Y el pozo confina**: las vecinas del fondo son macizas y frenan. Para caminar
  más allá del hueco hay que picar más (`el_hueco_que_cavaste_esta_rodeado_de_pared`).

Cavar pide **pico** (`targeting::can_dig_down`). No es progresión: el suelo está
en todas partes, y con la mano abierta cada toque perdido en el pasto abriría un
pozo por el que te caés a la cueva.

Se cava el piso —tierra, roca de superficie, suelo de cueva, tu propia losa— y no
el líquido (se rellena, no se pica), ni el vacío (ya es hueco), ni contra el
lecho de roca del fondo de la mina (`MIN_Z_LEVEL`). El pozo se rellena colocando
bloques encima como cualquier celda abierta, y persiste en `tiles.json` como
cualquier otra edición. Lo cubre `world2d::dig_tests`.

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
- **La escalera trae su propio rellano** (una losa al final del tramo), no una
  habitación amurallada. El resto lo levantás vos, losa a losa.
- **La gente y los animales viven en la superficie**: fuera de la planta baja se
  ocultan (`society::tick_society`, `fauna::cull_fauna`), como ya hacían tiles y
  decor. Sin esto los vecinos caminaban por encima de tu segundo piso. La
  simulación sigue corriendo — el pueblo no se congela porque subiste una escalera.
- **Chapa de piso** (`build::sync_floor_indicator`): `PISO 2` / `SUBSUELO 1`,
  oculta en planta baja. Un solo eje para arriba y para abajo.

Lo cubre `HA_BUILDTEST` de punta a punta: piso → anillo de paredes → la pared sin
apoyo se rechaza → el muro frena → **tramo de escalera colocado y ANDADO** →
rellano firme → losa sobre la casa → losa flotante rechazada → salir de la losa te
baja → pozo cavado → caída → **salida trepando el borde**.

⚠ **El driver empuja SOSTENIDO, no a pulsos** (`BuildTestState::empuje`). Todo lo
que se mide por tiempo de juego —andar un tramo— fallaba de forma intermitente
cuando la actuación duraba un frame de cada tres: el fallo hablaba de cuán ocupada
estaba la máquina, no del juego.

⚠ **Pero el botón de TREPAR sí va a pulsos** (`BuildTestState::saltar_hasta`), y
con el hueco largo: la trepada entra por el FLANCO del sostenido, y `hold_jump`
lo bufferea 0.16 s. Ciclo 0.2 s pulsado / 0.4 s suelto, o no hay flanco nuevo y
el paso no se puede reintentar.

⚠ **El driver escribe en `persist_dir()`, que en escritorio es el cwd** — o sea
`native/tiles.json`. Una corrida anterior que dejó un pozo a medio cavar hace
fallar a la siguiente por estado, no por código. Para un veredicto limpio, corré
con el guardado aislado:

```bash
env HA_UI_LAYOUT_FILE=/tmp/qa/ui.json HA_BUILDTEST=1 HA_AUTOPLAY=1 \
    WGPU_BACKEND=gl ./target/debug/ha-game
```

### Zonas: vos repartís los barrios — 7 ago 2026

El generador de ciudad ya sabía llenar un barrio (distritos + catálogo de
parcelas con su tamaño y su peso, `city::LOT_PROGRAMS`). Lo que no sabía es
**dónde querés vos que esté cada barrio**: lo decidía un anillo alrededor del
centro más un sorteo, y por eso todas las ciudades salían con la misma cebolla.

Una zona es **un rectángulo con un carácter** (`ha_shared::zonas`), y se dibuja
con la herramienta 🗺 del editor de mundo. Se guarda en `world_edits.json` con el
resto de lo autorado — un archivo, un guardado.

```
 ┌──────────────────── RESIDENCIAL ─────────────────────┐
 │   ┌──── COMERCIAL ────┐        ┌── PARQUE ──┐        │
 │   │  tiendas, mercado │        │  plaza     │        │
 │   └───────────────────┘        └────────────┘        │
 │            casas, dúplex, villas                     │
 └──────────────────────────────────────────────────────┘
```

- **Las sub-zonas salen gratis: gana la más chica.** No hay campo "padre" ni árbol
  que mantener — dibujar un rectángulo dentro de otro ES anidarlo. La jerarquía es
  la geometría, que es lo que estás mirando mientras dibujás, y mover una zona no
  puede dejar el árbol mintiendo. Test: `la_zona_mas_chica_gana`.
- **Una zona no coloca nada.** Le cambia el carácter a las manzanas que cubre
  (`city::district_at` la consulta primero) y el generador de siempre hace el
  resto con su propio sorteo. Por eso dos zonas residenciales iguales no salen
  iguales, y pintar sobre una ciudad ya generada reparte distinto en vez de
  borrar. Test: `pintar_una_zona_cambia_lo_que_se_construye_ahi`.
- **Se ve mientras dibujás.** El editor reinstala las zonas en el generador con
  cada trazo (`sync_zonas`), y el lienzo rasteriza el MISMO mundo que el juego:
  pintás un barrio comercial y los edificios se reordenan ahí mismo.
- **Coste cero cuando no hay zonas**: `zonas::clase_en` sale por una carga atómica
  antes de tocar candado o lista. El mundo por defecto no paga nada.

⚠ El registro de zonas es **global al proceso** (el generador son funciones puras
llamadas desde veinte sitios). Los tests que instalan zonas usan un cuerpo que
nadie más toca (Neptuno) para no cambiarle los distritos al resto de la suite.

### Construcción por bordes (estilo Last Day on Earth)

La pared no es una celda: vive en el **borde** entre dos celdas. Un cuarto 5×4
tenía 6 tiles útiles rodeados de 14 de muro; ahora tiene 20 y el muro es una línea
fina.

**El muro de adelante se dibuja BAJO** (`walls::ALTO_MURO_CERCANO`, 0.9 tiles).
Sin eso un cuarto de 3×3 era un bloque macizo de tablas: los tres muros del sur,
de 2.4 tiles cada uno, se dibujan hacia arriba en pantalla —o sea encima del
cuarto— y lo tapaban entero. `wall_pose` ya elegía `back`/`right` para esas caras
esperando un muro bajo del arte; el pack no lo trae, así que la altura la pone el
juego. Es la convención de cualquier cenital 3/4: **muro del fondo entero, muro de
adelante a la cintura**. La colisión no cambia.

**Y la pared que te tapa se transparenta**, con el mismo mecanismo que un techo
(`layers::Overhead`): cubrís su huella, se desvanece. Que sea el mismo para las
dos cosas importa — un cuarto con techo y paredes tiene que abrirse de una sola
manera. Los techos usan `Overhead::techo` (margen 1: se abre un paso antes) y las
paredes `Overhead::pared` (margen 0: pasar por la vereda de una casa no puede
volverla translúcida).

- **Dos capas:** piso/escaleras = celda (`TileWorld`, autotile) · pared/puerta/
  ventana = arista `(tx, tz, axis)`, donde `H` es la línea `z = tz` y `V` la
  línea `x = tx`.
- **Cada material trae el JUEGO COMPLETO** (7 ago 2026): pared, **vano**, puerta y
  ventana en madera, y las mismas cuatro en piedra, más su tumbado. Antes había
  dos paredes pero una sola puerta y una sola ventana —las dos de madera—, así que
  construir de piedra era terminar la casa con carpintería prestada. Lo cuida
  `el_juego_de_piezas_es_el_mismo_en_cada_material`.
  El **VANO** (`WallKind::Doorway`) es la pared con el hueco de la puerta y sin
  hoja: cierra el recinto y **cuenta como entrada** (`is_entrance`), pero se cruza
  sin abrir nada. Es el umbral entre dos ambientes de una casa, que con una puerta
  cada vez es un fastidio. Su pixel estaba en el pack desde el primer día
  (`Room_Builder_Arched_Entryways`) y lo importa `scripts/import-construccion.py`
  emparejando el vano con la pared **por color**, para que sea la MISMA textura.
  Sin arte asignado se dibuja por código —dos jambas y un dintel
  (`walls::marco_de_vano`)— y nunca como un cuadrado, que se vería igual que una
  pared.
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
- **UX:** apuntar a un borde vacío = colocar (fantasma verde/rojo); apuntar a uno
  ocupado = demoler (fantasma naranja) devolviendo el material. `BUILD_PIECES` quedó
  sólo con piso y escaleras; el resto son prefabs (`room_small/medium/large`,
  `casa_1`) que estampan piso + anillo de paredes con su puerta en un solo gesto.
- **Apuntar y colocar son DOS gestos** (8 ago 2026): ver abajo.
- **La casa tiene FRENTE** (8 ago 2026): ver abajo.

### La casa tiene frente — 8 ago 2026

**Los dos packs dibujan un edificio en proyecciones distintas**, y de ahí salía el
problema entero. Modern Interiors (`Room_Builder_*`) lo corta en cenital: paredes
finas vistas **desde adentro**. Modern Exteriors lo dibuja de **elevación**: el
frente de la casa —paño, vidriera, cornisa— y el techo desde arriba; no trae
costados, sólo el frente. La construcción por bordes estaba pintada entera con las
primeras, así que **una casa del jugador, mirada desde la vereda, era un interior
dado vuelta**. El pedido fue literal: *"tendríamos que construir sólo el frente, y
de ahí con el modern interior hacer el interior, pero de la forma que lo estamos
haciendo se nos hace imposible con los assets que tenemos"*.

No hizo falta un sistema nuevo. `walls::Cara` **ya** elegía una pose por cara, así
que un material puede tener su cara de afuera de un pack y su cara de adentro del
otro. Lo que faltaba era el pixel y tres arreglos de render.

**El bug que apareció midiendo, y que existía desde antes.** Toda tapa se planta
plana en `layers::OVERHEAD` (6.0) y las paredes Y-sortean en `[2.0, 5.0]`. El sprite
de una pared sur **crece hacia el norte**, o sea justo sobre las celdas que tienen
tumbado. O sea: **con el tumbado puesto, la pared del sur de una casa construida ya
era invisible desde la calle**, con `wall/wood` de 2 m igual que con una fachada de
3. No lo notó nadie porque no había nada que mirar ahí. La regla que sale de eso, y
que ahora es un test (`la_cara_exterior_va_por_delante_del_tumbado`):

> **Nada de adentro de una casa —su tumbado, su techo, sus muebles— puede
> dibujarse delante de su propia cara exterior.**

- **Banda nueva** `layers::FACHADA_BASE` (10.0) + `layers::fachada(z)`, que Y-sortea
  con el mismo span: dos casas a menos de tres tiles no se pelean. Y no se pierde
  nada por subirla, porque el sprite **nunca** crece hacia el sur: no puede taparle
  la cara a nadie parado en la calle, delante de él. Con ella subieron el fantasma
  de obra y los corchetes del cursor (`layers::OBRA`, 19) y los globos de diálogo
  (`layers::GLOBO`, 20) — un fantasma detrás de la fachada que acabás de poner es
  invisible justo cuando lo necesitás.
- **`Cara` se partió en cinco.** `Fondo` metía en la misma bolsa el TABIQUE interior
  (piso de los dos lados) y la ESPALDA del edificio (piso sólo al sur), y
  `Lateral { este: false }` metía además el tabique norte-sur. Ahora hay `Tabique`,
  `Fondo` (espalda), `Cercano` (frente) y `Lateral { este, interior }`, y
  `Cara::es_exterior()` es lo único que decide la banda. Nada se persiste: la cara
  se re-deriva en el streaming (`cara_de_borde`), así que no hubo migración.
  - `back` **ya significaba "la cara de afuera"** en este repo
    (`import-construccion.py` la recorta con `afuera()`), así que el frente siempre
    pidió la pose correcta.
  - La espalda estrenó la pose `espalda`, **opcional**: sin recorte propio cae a
    `front` por la cadena y se ve como se veía.
- **Doble fundido.** Con la fachada por encima del tumbado, adentro las dos capas se
  multiplican: dos veces 0,30 no dan 0,30, dan `1 − 0.7²` ≈ 0,51, y la franja de la
  entrada quedaba turbia. `Overhead` estrenó `alpha_dentro` y las caras exteriores
  usan `layers::FACHADA_ALPHA` (0,14). **El número está sin confirmar en el celu.**
- **Y el frente se abre A LO ANCHO** (`Overhead::margen_x`, `FRENTE_QUE_SE_ABRE` = 4).
  Un muro se funde por columna, y con paredes de 2 m alcanzaba: la que te tapa se
  abre y listo. Con una cara de 4 m no — parado adentro se abría la columna de
  encima y las vecinas seguían opacas, así que **el cuarto se leía en franjas, como
  por una ranura**. Es un eje aparte del `margen` de siempre porque el vertical
  tiene que seguir en 0: con margen en z, pasar por la vereda volvería translúcida
  la casa desde afuera, que es justo el bug que `Overhead::pared` vino a arreglar.
- **Cambiar de pose ya no borra el fundido.** `building::set_door_pose` hacía
  `*sprite = s`, alfa incluido, y `fade_overhead` tarda unas décimas en volver a
  bajarlo: la puerta de una fachada —que se abre justo cuando estás pegado a ella—
  **destellaba opaca, de 4 m, en la cara de la casa** cada vez que se abría o se
  cerraba. Ahora se conserva el alfa. Lo cazó el driver midiendo desde adentro:
  0,14 en dos pasadas y 0,74 en la tercera, la que caía después de la apertura.
- **`medida_de_pieza` tenía una mentira dormida.** El doc promete que la medida sale
  de la misma pose que dibuja el sprite; no era cierto cuando la pose faltaba, porque
  `ArtLibrary::celdas_de(key, Some(pose))` **no devuelve `None`: cae al rect del
  slot**. La cadena de la medida cortaba siempre en la primera vuelta y la del sprite
  seguía buscando → el pixel de una cara estirado al talle de otra. No se veía con
  `wall/wood` (el rect del slot ES el de `front`); con una fachada (`back` 1×4,
  `front` 1×2) sí. Guardado con `has_pose`.
- **La puerta era la única pieza que ignoraba la cara.** `animate_wall_doors` pedía
  `"open"`/`"closed"` a secas, así que una puerta puesta en el frente se dibujaba con
  la hoja de interior en cuanto alguien se le acercaba. Ahora `walls::pose_de_puerta`
  arma la cadena por cara y estado, y `building::set_door_pose`/`door_transition`
  reciben una CADENA en vez de una pose (antes salían sin hacer nada si faltaba: la
  puerta se congelaba, callada). Dos reglas: sólo el FRENTE tiene hoja de fachada, y
  **una cara nunca cae en otra cara** — sin cuadro abierto la hoja se queda dibujada
  cerrada, que es raro pero es de la misma casa.
- **Y los techos de aguas volvieron a la paleta** (`roof/teja`, `roof/chapa`, con el
  autotile que estaba escrito y sin usar). Salieron el 4 ago con un motivo escrito
  —"lo que uno construye a mano son CAJAS"— y ese motivo caducó: una casa con frente
  de edificio y tapa lisa se ve mitad y mitad. Van DESPUÉS de los tumbados en
  `CEILING_PIECES` para que el prefab de habitación siga entregando tumbado.

### Los dos materiales de fachada, y lo que el pack NO tiene — 8 ago 2026

`scripts/import-fachadas.py` cura `wall|doorway|door|window / fachada_local` y
`… / fachada_portal` a una página propia (`art/pack/fachadas.png`: 7 páginas
distintas serían hasta 7 lotes de dibujo, y el Redmi es sprite-bound — §10).

**Lo que aprendí midiendo, para no repetirlo:**

- **Los frentes de local del pack son DIBUJOS ENTEROS.** Rebanar un
  `planta_baja_tienda_*` de 7 celdas cada 32 px da media vidriera, medio maniquí y
  medio cajero. Corrí un clasificador de uniformidad sobre las 417 columnas de la
  familia: el "paño más liso" que elegía era la **persiana** de un local y la
  "ventana" era la **pantalla de un ATM**. Es lo mismo que §8 dice de los interiores,
  y vale igual para los exteriores.
- **Lo repetible es lo que el artista publicó de UNA celda de ancho**:
  `piso_modular_medio` (planta media, 1×4) y `planta_baja_*_modular` (planta baja,
  1×3). De ahí y de ningún otro lado sale una pieza de borde.
- **Las variantes (`#1..#30`) no se filtran.** El importador masivo agrupó las 31
  plantas medias como `piso_modular_medio` + variantes; descartar las que llevan `#`
  dejaba UNA y el material de vivienda se quedaba sin ventana.
- **Un trasto apoyado en la vereda pasa por puerta** si sólo se mide "el hueco llega
  al piso". El cajero automático ganó el puesto de entrada del edificio con todos los
  filtros en verde: llega al piso, es alto y es ancho. Lo que lo delata es la
  cantidad de colores DENTRO del hueco (`variedad_hueco`): un paño de vidrio da 11, el
  cajero 17.
- **El pack no trae entrada de vivienda en el set repetible, ni vano de fachada.**
  Medido: de las 123 columnas modulares, la única que pasa el filtro de abertura es la
  puerta vidriada de local. Así que `fachada_portal` **pide prestada** esa puerta (y
  lo imprime), y el vano queda **sin pixel** — el juego le dibuja el marco por código
  (`walls::marco_de_vano`), que es para lo que existe. `toda_pieza_de_construccion_tiene_pixel`
  saltea los `Doorway` por eso.
- Los dos materiales comparten el paño (ladrillo rojo) porque es el único con
  entradas, y se diferencian por la abertura. Es cómo se ve una cuadra de verdad: el
  mismo ladrillo, abajo el local y al lado la entrada de los departamentos.
- La cara de adentro (`front`) y `center` apuntan al paño de **piedra** de
  `construccion.png` — una `Pose` admite su propio `png`/`grid`
  (`art_manifest::Pose::png_or`), así que un slot mezcla las dos hojas sin copiar un
  pixel. **Ése es el punto entero del trabajo: afuera Modern Exteriors, adentro
  Modern Interiors.** Los costados son una tira fina del paño de afuera: desde la
  calle, el lado de una casa es ladrillo, no empapelado.

⚠ **Las fachadas van al FINAL de `EDGE_PIECES`.** `build::edge_piece_of_kind`
devuelve el primer `(kind, cost_item)` que calce y de él salen los anillos de los
prefabs: con la fachada de piedra antes que `edge_wall_stone`, `room_small` y
`casa_1` se estamparían de fachada. Lo clava `los_prefabs_no_estampan_fachadas`.
Cuestan `stone` a propósito (una terminación, no un recurso nuevo), y por eso la
familia la cuida `la_fachada_trae_el_juego_completo` y no el test por `cost_item`.

### Construir se previsualiza — 8 ago 2026

**Tocar el mundo dejó de colocar: APUNTA.** Hasta hoy el dedo era irreversible y
el fantasma era un rectángulo de color siguiendo al cursor. En un teléfono eso
significa que **nunca ves dónde va a caer la pieza**: el dedo tapa justo la celda
que estás apuntando, y para cuando lo levantás la pared ya está puesta. El pedido
fue literal — *"yo toco el tile y se añade un fantasma de la pieza y objeto a
colocar, de ahí me salen los botones visto bueno para colocar, x de cancelar, y el
de rotar"*.

Ahora son dos gestos (`build::MiraDeObra`):

1. **Tocás** → el fantasma se CLAVA ahí y se dibuja con **el sprite de verdad de
   la pieza**, traslúcido y teñido (verde entra · naranja demuele · rojo no se
   puede). Ya no sigue al dedo, así que lo podés mirar con la mano fuera de la
   pantalla.
2. **COLOCAR · CANCELAR · GIRAR** → confirmás, descartás o rotás. Las tres chapas
   viven en la misma columna arriba-izquierda y **sólo aparecen con un fantasma
   clavado**. En escritorio, `Enter` y `Esc`.

Y después de colocar, **el fantasma se corre solo** a la celda siguiente: levantar
un muro de seis metros es tocar una vez y aceptar seis (*"que el fantasma se vaya
hacia el lado así si quiero seguir construyendo solo sigo aceptando"*). La
dirección no se elige en ningún menú:

- se **aprende del segundo toque** (la dirección en la que te moviste, componente
  dominante — dos toques en diagonal no dejan un avance diagonal);
- la primera vez sale del **rumbo del jugador**: la obra se aleja de vos;
- y una **pared avanza siempre a lo largo de su propia línea**, porque un muro se
  levanta en fila (`una_pared_avanza_por_su_propio_eje`).

Detalles que parecen chicos y no lo son:

- **El fantasma de una pared ya no es la tirita de colisión.** Se dibuja como
  pared —su sprite de frente, al tamaño real, apoyado en el pie—, porque lo que
  hace falta ver antes de aceptar no es dónde va a frenar sino **qué te va a tapar
  la vista**. Con la escala nueva (una pared mide 1,5 × 3 m y sobresale de su
  celda) eso importa el doble.
- **Se avanza aunque la pieza NO entre.** Es lo que se quiere al llegar a una
  esquina: seguís aceptando, el fantasma sigue de largo poniéndose rojo, y vuelve
  a verde cuando vuelve a poderse.
- **CANCELAR devuelve GIRAR a AUTO.** Un giro que hiciste para una colocación que
  descartaste, arrastrado a la siguiente, es la clase de estado invisible que hace
  que el juego "haga cosas raras".

⚠ **Y el driver de QA tiene que MIRAR antes de aceptar** (`confirmar_si_apunta`).
Aceptar a ciegas en un bucle de reintentos es un bug garantizado: al colocar, el
fantasma se corre solo, así que la segunda confirmación pone la pieza en el borde
de al lado. Se vio exactamente así — *"11 paredes en el anillo, 12 en el mundo"*,
una fuera de la caja. El driver ahora apunta siempre y confirma sólo cuando el
fantasma ya está donde lo quiere, que es además lo que hace una persona.
- **Estructuras grandes** (`structure/*`, `structures.rs`): registro persistente
  `structures.json`, sprites Y-sorted, colisión de base.
- **Alto por pieza**: sale del ARTE (`walls::medida_real_de` → `tamano_real`, 8 ago
  2026), no de una constante. El de `wall/*` es de 1×2 celdas → 1,5 × 3,0 m, o sea
  más alto que el jugador. `EdgePiece::render_h` (2,4 la pared, 1,5 la baranda)
  quedó **sólo como respaldo** para un slot todavía sin pixel asignado. La colisión
  no cambia: todo borde es el rectángulo fino de `WALL_THICK`. Ver §8, "La escala
  del pack".

⚠ **Anti-regresión:** no volver a colocar paredes como tiles de celda. Los tiles
`WoodWall`/`Door`/`Window`/`StoneBrick` siguen existiendo **sólo** para los
asentamientos generados y las partidas viejas.

### El "Piso piedra" que era una pared — 4 ago 2026

La pieza `build_stone_floor` de la paleta colocaba `Tile::StoneBrick`, que está
en `Tile::solid()` **y** en `Tile::upright()`. O sea: ponías tu piso de piedra y
quedabas parado sobre un muro — no se camina, te tapa lo de atrás, se dibuja con
`wall/stone` y encima no se puede reemplazar (`place_tile` rechaza sobre sólido).
El slot `floor/stone` existía en el catálogo y ningún tile lo pedía.

Ahora hay `Tile::StoneFloor`: se camina, dibuja `floor/stone`, devuelve piedra al
picarlo. `StoneBrick` se queda como estaba — lo usan los asentamientos generados
y las partidas viejas, que no se migran.

Y `build_grid::is_build_floor` acepta **los dos pisos de la paleta**, no sólo el
de madera. Mientras el de piedra era un ladrillo sólido eso no se notaba; con una
losa que se camina, dejarla afuera significaba que sobre tu piso de piedra no
entra ni una pared, ni un techo, ni la losa de arriba — una regla que depende de
con qué material empezaste. La vereda NO entra: es calle, no cimiento.

### La calle dejó de ser una sola baldosa — 4 ago 2026

`world2d::spawn_chunk_tile_art` pedía `tile_sprite(clave_base)` y nunca pasaba
por `variant_key_seeded`. Con eso, las 6 variantes de vereda y las 8 de asfalto
que ya había curado `import-calles.py` **no se dibujaban jamás**: la ciudad
entera con la misma losa repetida cada metro. Ahora la variante se elige por
celda (`celda_semilla`, determinista y sin guardar nada), el mismo camino que ya
usaban las rocas y los árboles.

`import-construccion.py` además engancha las losas del pack que estaban
importadas y sin pedir: de las 166 claves `floor/*`, **16 son losa repetible**
(1×1, opaca entera y pareja) y entran como variantes de la calzada y la acera;
las 119 restantes son rects grandes —una entrada de garaje, una senda peatonal,
un cordón— o sea composiciones, no baldosas, y se pintan desde el editor de
mundo (§8: `floor` no es colocable a propósito).

### La esquina: la línea se corta y la senda va en los cuatro brazos — 8 ago 2026

Dos cosas se veían mal en cualquier cruce, y las dos eran de dibujo, no de trama:
la raya discontinua del centro **seguía derecho por la intersección** —dos rayas
cruzándose en el aire, algo que no existe en ninguna calle— y de senda peatonal
había **una sola**, en un brazo.

Lo que faltaba era saber dónde está el cruce, y eso **no se distingue por el
ancho**: una calle de tres y un cruce de tres por tres miden lo mismo. Lo que los
separa es que en la calle recta la calzada **se termina** a un lado y al otro (a
media calzada chocás con la vereda) y adentro del cruce no se termina en ninguno
de los cuatro rumbos, porque a cada lado empieza la otra calle. Se mide con la
misma vara que ya usaba el eje (`calzada_seguida` hasta `ALCANCE_EJE`):
`roads::es_cruce_con`. Una explanada de asfalto da lo mismo, y está bien —
tampoco se le pinta una línea divisoria por el medio.

Con eso, `roads::Pintura` reemplaza al viejo "marca de línea": adentro del cruce
no se pinta nada, la fila (o la columna) de calzada **pegada** al cruce es senda —
por eso son cuatro brazos y no uno: se pregunta por la celda vecina, no por el
eje— y el resto lleva línea. Slots nuevos `road/senda_ns` y `road/senda_ew`, con
respaldo dibujado por código (`procedural_art::zebra`) hasta que se les asigne el
pixel: las cebras van **paralelas al tránsito** que cruzan.

### El fleco de la vereda — 8 ago 2026

El borde exterior de la acera era una recta perfecta de kilómetros, y del otro
lado empezaba el bioma de golpe: retazos duros de pasto, de arena y de piedra
pegados a la vereda, como recortados con tijera. `geography::fleco` le suma 0 o 1
tile sorteado a lo largo de la calle, en rachas de dos (pixel a pixel se ve ruido;
de a dos se lee como una baldosa suelta).

**Sólo crece hacia AFUERA**, nunca hacia adentro: la calzada no se toca y la
vereda no puede desaparecer de un lado. Por eso siguen valiendo
`toda_via_tiene_vereda_a_los_dos_lados` y `una_calle_mide_lo_mismo_de_punta_a_punta`
— es lo que hace seguro tocar esto.

### El edificio modular: se coloca ENTERO — 8 ago 2026

Hasta acá una casa se levantaba pieza por pieza sobre la grilla de bordes, y eso
está bien para tu refugio. Para un edificio de ciudad no alcanza, y no por
comodidad: **la banda del pack mide nueve celdas y es UN dibujo**. Colocarla borde
por borde obliga a rebanarla en columnas de una celda, que es exactamente el
destrozo que dejó la primera importación (media ventana por pared).

Así que hay un segundo camino, con su propia geometría en `ha-shared/edificio.rs`
y su dibujo en `ha-game/edificios.rs`. Las cuatro cosas que sostiene:

- **La planta son cuartos**, no celdas sueltas. Ampliar es pegarle otro cuarto; no
  hay que redibujar nada.
- **El contorno se agrupa en TRAMOS**: corridas de bordes seguidos sobre la misma
  línea y con la misma cara. Un tramo es la unidad de dibujo — uno de nueve es una
  banda, uno de dieciocho son dos. `Tramo::modulos()` devuelve `(enteros, resto)`,
  que es lo que el dibujante necesita para no inventar pixel.
- **Los tabiques salen solos**: un borde con edificio de los dos lados pero con un
  cuarto distinto a cada lado es una pared interior. Nadie los coloca.
- **Las aberturas son UNA sola lista** indexada por borde. La elevación de la calle
  y el muro de adentro leen el MISMO dato, así que es imposible que una puerta esté
  abierta de un lado y tapiada del otro. Ése era el bug que el módulo existe para
  no tener.

El dibujo se planta sobre el **frente** (la cara sur) y crece hacia arriba:
planta baja (3 celdas, con la entrada) + N pisos (4) + azotea (7). No hay dibujo
de costado ni de espalda en el pack y **no se inventa**: esas caras las sigue
poniendo `walls` con los materiales `fachada_*`. Un edificio se mira de frente,
igual que en la hoja.

La paleta es la ficha **EDIFICIO** de la mochila: Ambiente · Ampliar · Una planta
más/menos · Puerta · Vano · Ventana · Tapiar · Cambiar tema. Se engancha por
`edificios::PALETTE_BASE` (20.000), el mismo mecanismo que la fauna — y se
pregunta ANTES que ella, porque 20.000 también es `>= SPAWN_PALETTE_BASE` y al
revés cada toque spawnearía un bicho.

⚠ **Una celda del pack se dibuja como UN TILE acá**, no como 0,95 m. Es la misma
excepción que ya vale para las piezas de obra (`medida_de_obra`): la banda
embaldosa a lo largo del frente, y con la escala de objeto un módulo de nueve
daría 8,55 m sobre 9 tiles — cada edificio montado medio metro sobre el de al lado.

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
- **Y el cuarto estampado de una pieza también está techado** (7 ago 2026). Su
  tapa vive en otro registro (`building::BuiltRoofs`), y hasta ahora
  `ceilings::covered` no la miraba: la habitación prefabricada **se veía** techada
  pero para el resto del juego estaba a la intemperie — adentro daba el sol y no
  se podían plantar muebles. Ahora las tres clases de tapa contestan a la misma
  pregunta, que es lo que hace que "techado" signifique una sola cosa.
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

- **Densidad de árboles: se calcula, no se elige** (`worldgen::surface_decor_at`,
  3 ago 2026). La copa media de `TREE_SPECIES` mide ~34 m² y la celda mide 1 m²:
  con el 14 % de celdas arboladas que había, cada metro de bosque estaba cubierto
  por **cinco copas superpuestas**. Eso no es un bosque, es una plancha verde — el
  reporte fue "los árboles son muy invasivos, hasta las casas están con árboles".
  El techo ahora es `TREE_PERMILLE_MAX = 30 ‰` (bosque), elegido para que la
  cobertura dé ~1.0: las copas se tocan y desde el aire sigue leyéndose bosque,
  pero se ve el suelo y lo que hay detrás. De regalo, ~5× menos entidades de
  decoración por chunk. **Si alguien vuelve a tocar este número, que sea contra la
  cobertura (densidad × área de copa), no a ojo.**
- **Dentro del pueblo el monte no crece solo.** Una parcela (`city::lot_at`) es de
  su casa: jardín, patio, entrada y vereda son parte de su programa aunque no
  pinten tile, y un árbol de 6 m plantado ahí —se dibuja con el pie en su celda y
  la copa hacia el norte— le comía la fachada. Ahora ninguna parcela genera árbol
  salvo `lot/arboleda`, cuyo programa a propósito no pinta ningún tile porque los
  árboles los siembra la decoración. En el resto del casco (manzanas vacías,
  descampados) crece un cuarto de lo normal: un pueblo tiene árboles, pero no es un
  claro dentro del bosque.
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
- **Bordillos de terreno:** un borde de terraza de UN escalón se trepa y se baja
  de un impulso en cualquier dirección; de dos para arriba es pared y hay que
  buscar el paso. Lo decide la altura de la cara y nada más
  (`relief::climbable_edge` = `alto_de_pared == 1`). Reparto medido: ~86 %
  bordillos / ~14 % paredes, y eso es correcto — dos tercios del planeta viven en
  las dos primeras terrazas y ahí el relieve tiene que dejar caminar. Arte propio
  `ledge/{roca,pasto,arena,nieve,regolito,basalto}` (dos celdas).
  **Cruzarlo se SIENTE** (`ha-game/src/vault.rs`): trepar iza el sprite en un arco
  con un roce y tres motas de polvo; descolgarse lo deja caer, revienta una ráfaga
  de polvo a los pies, suena un golpe seco y sacude la cámara un pelo. El polvo se
  tiñe con el color del suelo pisado (`tile_render_color`), así que sirve en todos
  los cuerpos sin arte nuevo. Sin esto la regla era invisible y cruzar un bordillo
  se leía como **atravesar una pared** — el reporte literal. Es feedback puro: no
  toca la sim determinista, no bloquea el control y el offset va sólo en el dibujo,
  nunca en el Y-sort (los pies siguen en su celda). Las rampas (`Tile::Slope`) no
  disparan nada: subir una carretera de montaña no es un esfuerzo puntual.

### La escalera del mundo: paredes de hasta diez pisos (10 ago 2026)

**El mapa y los acantilados se hicieron en fechas distintas y no encajaban.** El
relieve cuantizaba un campo de altura continuo en doce terrazas *de la misma
altura*, y el campo no puede cambiar más de `MAX_FIELD_STEP_PER_TILE` (0,020) por
tile contra los 0,047 que medía un escalón. Consecuencia: **ninguna pared del
planeta podía medir más de un escalón**. Medido, las 7.832 caras de la Tierra
bajaban exactamente uno.

Todo lo que se construyó encima era un parche de eso: la cascada con tope de
cuatro pisos que siempre salía de uno, el kit de tres celdas fijas, y el bordillo
decidido por una *cota trepable regional* — un campo de ruido de período 700
inventado porque, si todas las caras miden lo mismo, la altura no distingue nada.
La queja del usuario es literal: *"si hay una cascada de tres pisos, obligas a la
cascada a que se vea de acuerdo a un piso de acantilado"*.

**El arreglo es una sola constante**: `relief::SALTO_DE_TERRAZA`, cuánto se
levanta cada terraza sobre la anterior.

| terraza | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10+ |
|---|---|---|---|---|---|---|---|---|---|---|
| escalones de SU pared | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
| altura acumulada | 1 | 3 | 6 | 10 | 15 | 21 | 28 | 36 | 45 | 55… |

**La escalera sube de a uno, y eso es el arreglo** (14 ago 2026). Decía
`1 2 3 4 6 8 10 10 10 10 10 10`, o sea que de la terraza 7 para arriba **todas
las paredes medían exactamente 10** y los valores 5, 7 y 9 no existían en ningún
lado del planeta. Medido con
`medir_acantilados::cuanto_recortan_los_topes_de_la_pared` (Tierra, 800×800), el
mundo salía **bimodal**: `[698, 1473, 346, 38, 42, 40, 17, 26, 30, 515]` — el
96 % de las paredes medía 1-3, después un desierto, y un pico de 515 en el
máximo. Dos tamaños, no variedad. El `10` nunca fue una regla sino un techo
aproximado que el usuario tiró de palabra; el techo se queda, el amontonamiento
se fue. Total: 75 escalones ≈ 188 m (antes 84 ≈ 210).

Los números salen de **medir el planeta**, no de elegirlos lindos
(`relief::histograma_de_terrazas`): la Tierra reparte su superficie en 0 (10 %),
1 (43 %), 2 (24 %), 3 (5 %) y una cola de 4 a 9 que es la cordillera. La primera
versión puso los farallones en las terrazas 10-12 —que no existen— y el mundo
entero salió de bordillos.

> ⚠ **DOS POBLACIONES NO SE DIVIDEN ENTRE SÍ.** Ese mismo histograma tuvo un bug
> que costó una sesión entera de trabajo sobre un problema inexistente: contaba
> la SUPERFICIE en una rejilla ancha (±4.420 tiles, paso 17) y las CARAS celda a
> celda en diez ventanas de 400×400, dos de ellas en la meseta de nivel 8 de la
> cordillera, **fuera** del cuadrado de la rejilla. Leídos como una sola muestra
> decían *"el 13 % del planeta (niveles 4-7) no cuelga una sola cara"*, y es
> falso: en la rejilla no hay nivel 8 (0,02 %) y en esas ventanas no hay niveles
> 4-6 (cero celdas). Medido donde esos niveles de verdad están
> (`medir_acantilados::los_niveles_4_a_7_donde_estan`) cuelgan **una cara cada
> 115-367 celdas**, el mismo orden que las terrazas 1-2, y
> `cuanto_dura_cada_nivel` confirma que son terrazas de verdad (mediana de 4 a 8
> tiles seguidos, como el resto). Hoy el test cuenta las dos cosas sobre las
> mismas celdas e imprime aparte, rotulada, la rejilla ancha.

De ahí sale todo lo demás sin tocar nada más:

| concepto | quién lo decide |
|---|---|
| alto de la cara, en escalones | `relief::alto_de_pared` — **fuente única** |
| filas de roca que tapa | `alto + 1` (labio + cuerpos + pie) |
| celdas sólidas al pie | `ReliefCell::Pared` (`relief::pie_de_pared`) |
| pisos de la cascada | `agua::salto_desde` = `alto_de_pared` |
| filas del hombro de la garganta | `agua::filas_de_hombro(pisos)` |
| ¿bordillo o pared? | `alto == 1` |
| largo del paso | las mismas `alto + 1` celdas, abiertas |

**El cuerpo de la pared ahora es sólido.** Mientras toda pared medía un escalón,
la roca cubría dos celdas caminables y el jugador se metía dentro del dibujo sin
que se notara; con paredes de diez pisos, once celdas de roca atravesable son un
agujero en el mundo. `ReliefCell::Pared` marca esas filas: mismo tile de
acantilado (se pican igual), no se caminan y **no dibujan nada por su cuenta** —
la pared entera la cuelga su canto de una pieza (`world2d::pared_apilada`, que
existía y devolvía `false` siempre porque nunca había un desnivel de dos).

**Y el paso se recorre.** Donde la franja de paso cruza un muro, sus `alto + 1`
filas son rampa en vez de roca: subir un farallón de diez pisos cuesta once
celdas de corredor entre dos espolones de roca, no un tile mágico (biblia §5,
*nada de teletransporte por tile*).

**NUNCA AL SUR** (`relief::cara_al_sur`). Regla del usuario, textual: *"en el sur
no se ocupa rampa, prohibido para los acantilados usar rampas en caras del sur —
o sea el down en rampas; sólo usar izquierda, derecha, top"*. El motivo se ve en
una captura: la cara sur es la única que se enseña ENTERA, así que un paso ahí es
un tajo recto que parte el muro de arriba abajo, y cuanto más alta la pared más
largo el tajo. Las otras tres se ven de canto y ahí el paso es una muesca en el
filo. No cierra nada: el borde de una terraza es una curva cerrada y siempre
tiene caras al este, al oeste o al norte — lo que cambia es que hay que ir a
buscar la subida. Guardián: `relief::ninguna_rampa_mira_al_sur`.

**La cascada se arma de la más alta que dibujó el pack** (`import-agua.py::
CASCADAS_ALTAS`). El pack sólo publicó las piezas sueltas de la cascada 1; para
las otras dos el importador las recortaba de la hoja de tres filas, donde la
caída y el remate están fundidos — y en la cascada 3 ese remate **son las rocas
donde revienta el chorro**. Resultado: un salto de diez pisos con diez filas de
rocas cayendo por el aire. Ahora se recortan de la hoja de CUATRO filas (boca ·
caída · remate · poza), donde la fila 1 es agua y nada más y por eso se puede
repetir. Misma idea para el paso: `slope/*` estrena `labio`/`cuerpo`/`pie` y la
escalinata se apila igual que la pared.

**Las tres cuentas tienen que dar lo mismo o el mundo se rompe** de una de dos
formas: pared invisible (celda que frena sin dibujo encima) o roca fantasma
(celda que se camina con un farallón pintado). Por eso `pie_de_pared`,
`pared_apilada` y el guardián `relief::cada_fila_que_tapa_un_muro_es_roca` aplican
la MISMA poda: el muro cubre la terraza de abajo mientras siga siendo la misma, y
se corta en cuanto el nivel cambia (otro canto que baja, o el suelo que vuelve a
subir en el fondo de un valle).

Medido después: la pared más alta que encuentra el driver mide **10 escalones**, y
entre dos paredes hay 55 tiles de terraza (mediana; p10 = 9, y ahí la poda
recorta). Guardianes: `hay_paredes_de_varios_pisos`,
`cada_fila_que_tapa_un_muro_es_roca`, `the_world_has_both_ledges_and_walls`.

### El agua baja de la montaña (`ha-shared/src/agua.rs` + `geography::river_here`)

> ⚠ **TODO LO QUE ESTA SECCIÓN DICE DEL CAUCE ES HISTORIA** (14 ago 2026). El
> entramado de ruido, sus semianchos y el tirón del sumidero se retiraron enteros:
> medido, **ninguna de las 34 cabeceras del planeta llegaba al mar ni a un lago**.
> Hoy el cauce se TRAZA — ver §"El río se traza, no se pinta". Lo que sigue
> vigente de acá abajo es la **cascada**: cómo se ancla y por qué su alto lo da la
> montaña.

El cauce era una cresta de ruido cuyo ancho lo daba `lowland = 1 − (h−mar)/0.22`,
que se satura **medio escalón por encima del mar**: de la segunda terraza para
arriba el río quedaba en su mínimo y el entramado tenía período 2.600 tiles (el
ancho de una celda de asentamiento entera). Medido sobre la sierra: **59 celdas
de río en 750.000**. Sin río en la sierra no había nada de lo que cuelga de él —
ni cascada, ni orilla de piedra, ni un torrente que seguir hasta la playa.

Ahora el ancho lo gobierna la **altura**, repartida sobre toda la escalera de
tierra (`RIVER_HALF_CUMBRE` … `RIVER_HALF_BOCA`), con período 650. Los dos
semianchos están **medidos, no supuestos**: `ancho ≈ semiancho × 1.786`, y el
primer intento calculado a ojo desde el período dio un río de **48 tiles de ancho
medio** — un lago con forma de serpiente. Reparto actual del planeta:

| | río sobre la tierra | ancho medio | orillas pasto/arena/roca | cascadas |
|---|---|---|---|---|
| sierra (terraza ≥5) | 4.4 % | 27 tiles | 289 / 0 / **1.435** | una cada 203×203, de 1 a 4 filas |
| llano | 3.3 % | 20 tiles | 7.018 / 4.496 / 0 | una cada 155×155, de 1 fila |

> ⚠ **Esta tabla es de agosto y dos de sus columnas ya no dicen la verdad.** La
> orilla de ROCA se quitó (§"No hay agua sin césped": quedan dos familias, pasto
> y arena) y la cascada dejó de medir "de 1 a 4 filas" cuando el relieve aprendió
> a apilar. Los números vigentes se sacan corriendo `medir_rios` y
> `medir_cascadas`; el reparto del 14 ago 2026 está más abajo, en
> §"El río sabe que hay lagos".

**La cascada.** `cascada_at` pedía agua a los DOS lados de cada celda de caída, y
entonces dos paredes seguidas se anulaban entre sí: **99 saltos en 1.800×1.800
tiles, todos de un escalón**. Arreglar eso no alcanzaba: el terreno **no puede
apilar escalones** —el campo de altura cambia menos de medio escalón por tile—, y
medido sobre la sierra, de una pared que mira al sur a la siguiente hay **entre 8
y 20 tiles**. Una cascada de tres escalones apilados no existe en este mundo. Así
que el alto lo da **la montaña**: el chorro se dibuja tan largo como alto esté el
labio sobre el mar, tapando el tramo de torrente que sigue cuesta abajo. Montaña
más grande, cascada más grande, sin autorar ninguna.

> ⚠ **"Una cascada de tres escalones apilados no existe en este mundo" DEJÓ DE SER
> CIERTO** (10 ago 2026, verificado el 14). Se escribió cuando todas las terrazas
> medían lo mismo, y ahí era verdad por construcción. Con
> `relief::SALTO_DE_TERRAZA` la terraza crece de 1 a 10 escalones con la altura,
> así que **la pared que separa dos terrazas altas mide diez pisos y la cascada
> que cae por ahí también**. Lo que sigue siendo verdad es la premisa: dos celdas
> pegadas nunca se llevan más de UNA terraza —el campo no puede cambiar tanto por
> tile— y por eso `medir_acantilados::cuantos_escalones_baja_una_cara` imprime
> `{1: 7779}`. Ese número es el salto de TERRAZA, no el alto de la PARED, y
> confundir los dos es lo que dejó la doctrina vieja escrita en tres sitios. El
> test imprime los dos desde el 14 ago.

**Y se dibuja en METROS, no en escalones** (ago 2026). El alto salía de
`(terraza + 1) / 2` y encima se recortaba por la poza. Medido sobre el planeta
entero: **48 sitios de caída, 42 en la terraza 1** → con la división por dos casi
todas las cascadas del mundo medían UN tile y el arte animado del pack no se veía
nunca entero. Ahora `alto = terraza × STEP_M` (2,5 m por escalón, un tile por
metro) con `SALTO_MAX = 10`: hasta el salto más humilde baja tres tiles y el de la
sierra baja siete. **El salto sigue siendo un escalón de terraza** — lo que mide
cinco tiles es el dibujo, no el desnivel que caminás. La poda de la poza se queda
y no cuesta casi nada: medida, el agua al pie va de 1 a 20 tiles con 8 de mediana.

**AL RAS DE LA MESETA — y el alto vuelve a ser el desnivel** (9 ago 2026). Lo de
arriba se revirtió: medir el chorro en metros sobre el mar lo desató del terreno.
`medir_acantilados::corte_de_una_cascada` volcó el corte vertical de tres saltos
de sierra y los tres bajan **un escalón** (terraza 9 → 8) con **6 y 10 filas de
chorro dibujadas**. En pantalla eso es una cinta de agua colgando en mitad del
río, al lado de una pared de tres celdas. Encima la boca se ponía en `tz-1` —una
fila POR ENCIMA del canto—, así que el labio de roca del recorte flotaba dentro
del lago de arriba.

Regla del usuario: *"el principio de la cascada desde arriba tiene que estar al
ras de donde está la meseta, y abajo al ras del suelo"*. `cascada_at` devuelve
**pisos de meseta** y las filas son las mismas que las de la pared que el kit
cuelga al lado:

| fila | cascada | pared (`world2d::pared_apilada`) |
|---|---|---|
| `tz` | `boca` | `labio` — al ras de la meseta |
| `tz+1 … tz+n` | `tramo` ×n | `cuerpo` ×n (n = pisos) |
| `tz+n+1` | `poza` (1.ª) | `pie` — al ras del suelo de abajo |
| `tz+n+2` | `poza` (2.ª) | — el reventón, ya sobre el agua |

**Y el tope de cuatro pisos se fue con la escalera del mundo** (10 ago 2026, ver
más arriba). Se puso cuando toda pared medía un escalón: daba igual poner cuatro o
cuarenta porque la resta siempre daba uno. Hoy `salto_desde` devuelve
`relief::alto_de_pared` sin recortar y un río que cruza el borde de una terraza
alta cae los diez pisos que baja el terreno, con los dos hombros de roca a la
misma altura. La variedad de tamaño no se perdió al soltar la regla de "montaña
más grande, cascada más
grande": la da el río. Medido con `medir_acantilados::saltos_encadenados`, **de
32 anclas del planeta 9 tienen otra río abajo**, a entre 8 y 40 tiles — la sierra
se baja encadenando saltos separados por rápidos, que es como baja un río de
verdad. Guardianes: `agua::la_cascada_mide_lo_que_baja_el_terreno` y
`agua::hay_cascadas_encadenadas_bajando_la_sierra`.

Y el cierre de esquinas cóncavas también calla dentro de la garganta
(`agua::en_la_garganta`, la variante de `garganta_de_cascada` sin el filtro de
"tiene que ser pared"): esas celdas son terraza llana por definición, así que la
pregunta con `pared` les decía que no y el relieve colaba su canto dentro del
recorte del pack.

**El agua no lleva sombras** (ago 2026). La onda, la hondura del fondo, la
burbuja, el cardumen y el nenúfar salían Y-sorteados como si se levantaran del
agua, así que `light2d::attach_ground_shadows` les colgaba su mancha: mar abierto
lleno de manchones oscuros sin nada que los proyecte. No están SOBRE el agua —
**son** el agua, y el nenúfar flota plano como una moneda. Ahora van en la banda
de piso (`agua::TEXTURA_DE_AGUA`) y no proyectan nada. Lo que sí tiene volumen —la
roca flotante, el tronco a la deriva— no está en la lista y conserva su sombra,
que es exactamente la regla que se pidió.

**El chorro se anima de verdad** (ago 2026). Las cuatro poses de la cascada miden
3 celdas de ancho y eran las **únicas poses animadas multi-celda del manifest**;
`art2d::load_art_manifest` pisaba el paso de la tira con el ancho del "bloque",
que sin `strip` declarado vale 1 celda. Resultado: la animación avanzaba 32 px en
vez de 96 y el chorro se arrastraba por encima de sus propios frames — en pantalla,
seis dibujos encimados y quietos. El paso de bloque ahora sólo se aplica cuando el
entry lo declara, y el test `los_frames_de_una_pose_no_se_pisan` recorre el
manifest entero para que no vuelva a pasar.

Dos reglas más que salieron de mirar las capturas:

- **Una caída, un dibujo, centrado.** El recorte del pack es una GARGANTA —roca a
  los dos costados y el chorro en el medio—, no un tramo repetible. Anclando cada
  tres celdas, un río de veinte tiles se llenaba de **columnas de roca alineadas
  en mitad del agua**. El resto del desnivel sigue siendo pared con agua arriba y
  abajo, o sea un rápido.
- **Las paredes corren en diagonal**, así que tres columnas que empiecen en la
  misma fila casi no existen: `salto_vecino` tolera un tile de desnivel entre
  columnas vecinas.

### El acantilado sabe hacia dónde se cae

Un borde de terraza es una banda de UN tile, y el autotile lo comparaba **con el
tile de al lado**: sus vecinos de los costados eran acantilado (igual) y los de
arriba y abajo no → lados opuestos distintos → `center`. Las 32.000 caras del
planeta pedían la misma pieza y daba igual hacia dónde miraba el precipicio.

Ahora, para `Cliff` y `Slope`, "igual" significa **"no se cae para ese lado"**
(`relief_level(vecino) >= relief_level(celda)`), así que el nombre de la forma ya
dice dónde va la cara: `edge_s` es la que da a la terraza de abajo. Vale para el
arte que dibujes en HA Studio con los nombres de siempre y para el respaldo de
`procedural_art`, que ahora genera **una hoja por forma** con la meseta del color
del SUELO de arriba (pintarla del marrón de la roca dejaba una banda oliva
atravesando el prado) y la cara sólo hacia el sur, que en 3/4 es la única que se
ve de frente. El bordillo (`ledge/*`) estrena respaldo propio: cara baja y clara,
para poder distinguir de un vistazo lo que se trepa de lo que hay que rodear.

Y el material de la pared ya no se lo pregunta al agua: un escalón que da a un
río salía con bioma `River`, o sea "roca", y en mitad de una pradera aparecían
**placas grises sueltas entre paredes verdes**. `cliff_material` busca la tierra
de al lado en anillos — la misma lección que `agua::orilla_de`.

### El muro se cierra: las esquinas cóncavas (9 ago 2026)

`relief_cell` marca `Cliff` sólo la celda de la que se puede **caer caminando**:
la que tiene un vecino ortogonal más abajo. La celda alta cuyos cuatro vecinos
están a su nivel y sólo la DIAGONAL cae no es acantilado, así que nadie le pedía
pieza — y como los bordes de terraza bajan casi siempre en diagonal, el muro
salía alternando pieza y hueco (`M?M?M?`): en pantalla, **una línea de puntos de
roca bajando por la ladera**. Eso era el "acantilado cortado en seco".

Medido antes de tocarlo (`tests/medir_acantilados.rs`, Tierra, 800×800):

| | |
|---|---|
| caras de acantilado | 3.154 |
| esquinas cóncavas SIN pieza | **1.522 (48 % del perímetro)** |
| caras que piden una forma orientada | 99,4 % (el autotile no era el problema) |
| tierra alta en mesetas de 400+ tiles | **99,3 %** |

Ese último número mató la otra hipótesis de una: **las mesetas ya se cierran en
360°**. Son conjuntos de nivel de un campo continuo, o sea curvas cerradas por
construcción; no hacía falta ningún *falloff* ni umbral sobre el ruido. Lo que
faltaba era la pieza del autotile.

La dibuja `world2d::spawn_chunk_canto_concavo`, y es **dibujo y nada más**: la
celda sigue siendo terraza llana, no colisiona y `relief_cell` no se entera —
cerrar el muro no puede cerrarle el paso a nadie. La pieza que va ahí **no es una
pared**: la vecina ya cuelga la suya y debajo hay terreno alto por el que se
camina, así que una pared de tres celdas taparía meseta. Es el CANTO, una celda:
la cresta si la diagonal cae al norte, el labio si cae al sur
(`import-acantilado.py::CARA_CANTO`).

### El asset se usa como está dibujado (9 ago 2026)

Dos intentos de "arreglar" la pared inventando reglas propias, y los dos peores
que el problema:

1. **Vaciarle el suelo dibujado a la pieza** para que la meseta no desentonara
   con el pasto. Resultado: paredes de roca pelada sin remate.
2. **Poner cuerpo de pared en todas las caras** para que ninguna quedara fina.
   Resultado: el perímetro de cada terraza hecho bloques de piedra.

El pack ya resolvió cómo se ve un acantilado y trae **un recorte distinto para
cada sitio**: la cara que mira al sur lleva labio + cuerpo + pie, la que mira al
norte es la cresta lejana de una celda, y la que se ve de canto es la meseta con
su filo más el costado modular. Frente, lado y atrás **no son intercambiables**.
El trabajo es colocarlos bien, no redibujarlos.

**Si el color no pega, se tiñe; no se borra.** El mundo tiene cinco pastos —el de
referencia más los cuatro de clima de `world2d::pasto_de_bioma`— y el relieve
tenía una sola variante: la franja de arriba de la pared salía de otro verde que
la losa de al lado (medido en la captura del campo: **(86,165,92) contra un pasto
de sabana de (135,147,89)**). Ahora hay un `cliff/*`, un `ledge/*` y un `slope/*`
por clima, y el tinte se **mide de la losa** que el usuario asignó en HA Studio y
se **ancla en el tono dominante** de la meseta (`import-acantilado.py::
mapa_por_losa`), así que la parte plana de la pieza sale idéntica al pasto que
tiene pegado. Verificado en captura: los dos son (71,151,87).

### Un muro, una pieza (9 ago 2026)

El bordillo se dibuja con dos celdas y la pared con tres, así que cada vez que
dos celdas vecinas del mismo muro contestan distinto aparece un escalón de piedra
que se corta y vuelve — *"se mezclan piezas de distintos sets en el mismo tramo"*.

La causa estaba en `climbable_edge`: decidía con la **pendiente del campo a ±4
tiles**, que es una medida local de un campo continuo, así que dos celdas vecinas
caían a los dos lados del umbral. Medido sobre ocho ventanas de la Tierra:

| regla | parpadeo | bordillos |
|---|---|---|
| pendiente ±4 (la vieja) | **8,86 %** | 55 % |
| pendiente ±24 | 4,4 % | 58 % |
| **cota regional por nivel** | **0,08 %** | 60 % |

Ensanchar el estencil no arregla nada: el parpadeo es inherente a poner un umbral
sobre un campo continuo. Lo que **no puede** parpadear es una regla que dependa
del NIVEL, porque un tramo de muro es el borde de un conjunto de nivel y todas
sus celdas están en el mismo escalón.

> ⚠ **`cota_trepable` ya no existe** (verificado el 14 ago 2026). Era un campo de
> ruido de período 700 que decía hasta qué terraza se trepa en cada comarca, y
> hacía falta **porque todas las paredes del planeta medían un escalón**: si la
> altura no distingue una de otra, hay que inventarle al muro otro criterio. Con
> `relief::SALTO_DE_TERRAZA` la altura volvió a ser el dato, así que un bordillo
> es sencillamente **una pared de un escalón** (`alto_de_pared <= 1`) y lo deciden
> `climbable_edge` para la colisión y `terrain_art_key` para el dibujo, los dos
> mirando el mismo número. El parpadeo no volvió: el alto de una pared también es
> constante a lo largo de un tramo de muro, por el mismo motivo que el nivel.

**Ojo al medir:** una sola ventana de 1.200×1.200 puede caer entera en una comarca
alta —toda paredes—, que es lo que la regla promete, y contestar "acá no se trepa
nada". Los tests que preguntan por el reparto barren ocho ventanas repartidas por
el planeta, no un cuadrado en el origen.

### Nada edificado se planta a caballo de un escalón (9 ago 2026)

El tile edificado GANA sobre el natural (`topdown_tile`), así que donde una casa,
una acera o un lote cruzaba un borde de terraza **la pared del mundo desaparecía**
y el desnivel quedaba cortado a la altura del ladrillo. Medido sobre 63 núcleos:

| | antes | después |
|---|---|---|
| caras pisadas por algo edificado | 32.572 | **1.122** |
| de ellas, calzada o acera | 21.450 | **184** |
| parcelas con una cara adentro | 5.083 | 1.018 |

Tres cambios, y el del medio es el que importa:

1. `civil_at` no pinta ACERA sobre una cara. La calzada sí manda —donde pasa un
   camino el relieve ya abre rampa, así que nunca cae sobre una cara.
2. `city::retiro_del_desnivel`: **el escalón no vacía la manzana, la achica.**
   Rechazar la parcela entera fue el primer intento y dejó la metrópolis en 124
   manzanas de 207 —un descampado—, porque una manzana mide 34 tiles y un borde
   cae cada ~20: casi todas rozan uno. Se retiran: la parcela se queda con la
   parte llana (de los cuatro recortes posibles, el de más superficie) y el borde
   pasa por el jardín, con un tile de margen.
3. `building_in_block` mira la huella ENTERA y no sólo la esquina noroeste.

**Y la respuesta no puede depender de quién pregunta.** `lot_at` corre por tile;
el atajo de "si el candidato no me contiene, lo apunto como ocupado y sigo" hacía
que para unas celdas el rectángulo grande existiera y para otras no, y la parcela
chica que se metía en el hueco salía **dibujada a trozos y sin puerta** (9 casas
de 1.977, lo cazó `toda_casa_generada_tiene_por_donde_entrar`). Ahora el terreno
se mira siempre, contenga o no a la celda, con memo por rectángulo para que no
cueste.

### La cascada manda en su garganta (9 ago 2026)

El recorte del pack no es un chorro suelto: es una **garganta** de tres celdas,
roca a los dos costados y el agua en el medio. Dibujar además la pared del
relieve en esas celdas es pintar dos rocas en la misma celda, y no coinciden — el
muro mide tres celdas fijas y el chorro mide lo que mida la montaña. Sobre todo,
`salto_vecino` tolera **un tile de desnivel** entre las tres columnas (sin esa
tolerancia se anclaba una cascada de cada cinco, porque las paredes de terraza
corren en diagonal), y el dibujo es un rectángulo rígido: la columna que arranca
una fila más arriba dejaba su labio de roca **asomando por encima del agua**.

`agua::garganta_de_cascada` calla al relieve donde cae un salto.

### La garganta se ancla AL RAS, y se viste del material de su pared (9 ago 2026)

Dos arreglos de la misma sesión, los dos salidos de medir: el driver está en
`ha-shared/tests/medir_cascadas.rs` (`cargo test -p ha-shared --test
medir_cascadas -- --ignored --nocapture`).

**1. La deriva se acumulaba y el dibujo no se enteraba.** `salto_vecino` tolera
un tile por columna y **sigue desde la fila nueva**, así que a lo ancho de las
tres celdas del recorte la deriva llega a ±2; el dibujo, en cambio, estampaba su
rectángulo rígido en la fila del ancla. Medido sobre 1.800×1.800 tiles:

| | antes | ahora |
|---|---|---|
| columnas del chorro que caen en la fila de la boca | 50 de 81 (62 %) | **33 de 33 (100 %)** |
| paredes que flanquean la garganta con su canto ahí | 17 de 54 (31 %) | 15 de 22 (68 %) |
| …sin ninguna pared en ±2 filas | **10 de 54** | **0** |
| anclas en la región | 27 | 11 |

Eso era el chorro colgado *al lado* del acantilado en vez de dentro de él. Ahora
`agua::tramo_de_caida` reconstruye la fila de cada columna del tramo y busca una
ventana de tres **que caigan las tres en la misma fila** — la más centrada del
tramo. Si no hay ninguna, el desnivel se queda como pared con agua arriba y
abajo, o sea un rápido, que es una lectura correcta del terreno. La tolerancia no
se fue: sigue haciendo falta para SEGUIR el cauce (las paredes corren en
diagonal), sólo que ya no decide dónde se dibuja. Se pagan cascadas —de 27 a 11—
y se cobran once que se ven bien en vez de veintisiete torcidas. El ±1 que queda
en las paredes flanqueantes es el escalón real del terreno y no molesta: el
recorte trae su propia roca a los dos lados. Guardián:
`agua::la_garganta_cae_al_ras_de_sus_tres_columnas`.

**2. El chorro era una cicatriz naranja en un muro verde.** El recorte del pack
trae su garganta dibujada en roca **terracota con estrías**; el kit de acantilado
se tiñe con la losa que corona y sale un **canto rodado gris malva**. Y medido,
las cascadas caen justo donde peor pega: de 94 paredes contiguas, **91 son de
pasto**, 2 de sabana y 1 de arena.

`import-agua.py` corta ahora una copia teñida por material
(`agua/cascada#<variante>_<material>`, los nueve de `import-acantilado.py`), y el
color no se elige a ojo ni sale de una segunda tabla: **sale de
`cliff/<material>`, el mismo dibujo que va a quedar pegado a la garganta**. Su
meseta da el color de los verdes del recorte, su `cuerpo` da la paleta de las
tierras cálidas, y el agua no se toca — es la misma que la del río. La paleta se
parte midiendo (los costados del recorte contra su columna central) y sale sin
solapamiento: 9 tonos de agua, 9 de roca, 4 de césped.

**Se tiñe la hoja del pack ENTERA antes de recortar**, así que salen los seis
frames de cada pose. Teñir sólo el primero habría hecho parpadear de color la
animación. `world2d::material_de_cascada` elige el material con el mismo criterio
que `terrain_art_key` usa para la cara de al lado — que sea el mismo criterio es
todo el punto. Sin la copia teñida se cae al recorte crudo y se dibuja igual.

Coste: la hoja pasó de 1024×1248 a **1152×2880 px** (+2,1 Mpx sobre los 169 Mpx
de atlas del proyecto). Dos detalles que lo hicieron entrar: la cascada `entera`
de 3×3 no se guarda teñida (es la vista previa del editor, el juego no la pide) y
`HOJA_COLS` pasó de 32 a **36**, porque una tira de cascada mide 18 celdas y dos
no entraban en 32 — el empaquetador le daba un renglón a cada una y la hoja se
iba a 1024×**7008**, por encima del máximo de textura de buena parte de las GPU
móviles.

**3. Y tres cosas más que sólo se vieron en la captura del driver**
(`HA_AGUATEST=1`, `agua-2-cascada.png`). Vale anotar el método: los tres puntos
anteriores salieron de medir, éstos de MIRAR. Ninguna medición los iba a cazar
porque los tres son "está dibujado donde toca, pero se ve mal".

- **El chorro llevaba sombra.** `light2d::attach_ground_shadows` cuelga su mancha
  a todo lo que vive por encima de `layers::YSORT_MIN`, y la cascada está ahí
  arriba porque tiene que Y-sortear (el que camina bajo la poza pasa por delante
  del chorro). Pero **es agua**, y el agua no proyecta (§"El agua no lleva
  sombras"): se veía el rectángulo tumbado sobre la poza. La onda y el nenúfar
  cumplen la regla bajando de capa; la cascada no puede, así que estrena la
  puerta explícita `light2d::sin_sombra`.
- **El chorro no era del color del río.** El agua del recorte va de (42,112,155)
  a (90,204,195) con dominante (54,144,174), y el río es un plano de
  **(60,163,178)**: la cascada caía más turquesa y más clara que el agua de la
  que sale y en la que revienta. Se ancla igual que la roca —el tono dominante al
  del río, el resto por luminancia— así que la espuma sigue siendo espuma. Va en
  TODAS las copias, la de respaldo incluida: es el mismo líquido del mismo cauce.
  El color del río se lee del **pack**, no del manifest, porque el manifest es lo
  que este script escribe y leer la corrida anterior es cómo se arma una deriva.
- **El río de arriba se cortaba con una ribera cruzada sobre el salto.** La celda
  de la boca es acantilado, o sea tierra, así que el autotile del agua le pedía
  `edge_s` y le pintaba su banda de barro y pasto **atravesada por encima de la
  cascada**: el lago terminaba en una orilla y el chorro empezaba debajo, sin que
  una cosa llevara a la otra. Regla del usuario: *"es el único lugar por ahora
  donde la tierra/pasto y el agua no ponen su borde, sino el asset de la
  transición que hace que la cascada empiece"*. Y esa transición ya existía — es
  la boca del propio recorte. `agua::boca_de_cascada` la declara agua para el
  autotile y el río desemboca en su salto.

### LA GARGANTA ES UN MÓDULO: chorro + dos hombros iguales (10 ago 2026)

Regla del usuario sobre la captura del salto: *"las líneas punteadas es donde
debería estar el acantilado y el rectángulo es donde está; las flechas son en qué
dirección debería crecer, y ahí ya estaría perfectísima la cascada. Quiero que
sea algo repetible, ya como un módulo que se pueda usar sin bugs."*

El chorro ya se anclaba al ras (arriba), pero **la pared de al lado seguía siendo
la que le tocara al relieve**, y el borde de terraza serpea. Medido con
`medir_cascadas::pared_junto_a_la_garganta` y `encaje_de_la_cascada` sobre
1.800×1.800 tiles, de los **22 hombros** de los 11 saltos del recorte:

| | antes |
|---|---|
| hombros que ni siquiera son pared | **9 de 22** |
| de los que sí, con el canto desfasado ±1 fila | 7 de 13 |
| dibujados como bordillo (2 celdas contra 4 del chorro) | 3 |

O sea: al lado del salto o no había roca, o empezaba y terminaba a otra altura.
Eso es la banda suelta y más corta que el chorro de la captura.

Ahora el hombro **se arma**, no se espera: `agua::hombro_de_garganta` contesta
por las columnas `ancla − 1` y `ancla + ANCHO_CASCADA` en la fila del ancla, y
`world2d::spawn_chunk_hombros_de_garganta` cuelga ahí las mismas filas sueltas
que apila `pared_apilada`, con el material de `material_de_cascada`:

```text
  fila tz            labio   ← al ras de la meseta, la fila de la boca
  filas tz+1..tz+n   cuerpo     (n = pisos que baja el salto)
  fila tz+n+1        pie     ← donde revienta la poza
```

La pieza que le tocaría por relieve se calla en esas dos celdas, como ya se
callaba dentro de la garganta. **Es dibujo y nada más**: `relief_cell`, la
colisión y la navegación no se enteran, así que un hombro no encierra a nadie.
Guardián —y éste no es una medición, es un test de verdad—:
`medir_cascadas::toda_garganta_tiene_sus_dos_hombros`.

### EL SUELO NO TAPA AL JUGADOR, y una cascada es suelo (10 ago 2026)

Regla del usuario, y va por encima de cualquier ajuste de capa: *"el jugador
siempre está sobre el suelo, nunca el suelo sobre el jugador; nada debe ser
tapado por el suelo en tema de capas a menos que sea necesario y de manera
explícita se te indique."*

Las tres piezas del salto Y-sorteaban por el **pie del dibujo entero**, así que
la poza —que mide dos filas y termina abajo del todo— se ordenaba como si fuese
una pared de dos metros: **el que se metía en la poza quedaba detrás del
reventón**, tapado por agua que le llegaba a los tobillos. Igual la boca contra
el que vadea el río de arriba.

Reparto nuevo, y el criterio es el de siempre (§"Capa ≠ colisión"): **plano es
plano**. La boca y la poza son líquido acostado en el suelo → `layers::FLOOR`,
como el agua del río, y por definición no tapan a nadie. El único trozo que se
levanta es el chorro, y ése Y-sortea **por su propia fila**, no por el pie del
conjunto: pararse al lado de una fila de caída te pone delante de ella.

### EL PEINE DE BARRITAS: los pasos anchos repetían su filo de roca (10 ago 2026)

Lo que el usuario tachó en la cuarta captura —*"se está repitiendo la textura y
se ve horrible"*— eran tiras verticales de roca cada metro sobre el pasto. No era
una siembra: era **el paso**, `slope/*`.

Los escalones se cortaban por celda (columnas 1 y 2 de
`Four_Sand_Mountains_1_Bottom`) y cada uno se llevaba pegado el filo de roca del
espolón vecino: medido sobre la hoja, la roca ocupa los píxeles **0-37 y 90-127**,
o sea los 6 primeros de la columna 1 y los 6 últimos de la 2. Con pasos de una
celda no se notaba; desde que se ensancharon (§5, *el paso se cruza, no se
enhebra*) miden cuatro y cinco celdas y las cuatro dibujaban su tira. Las únicas
rocas de un paso son sus dos mejillas, y ésas ya van aparte en `cliff/*`.

Ahora el escalón se corta **en píxeles, no en la grilla** (`ventana()`): la franja
38-89 es escalón puro, y salen dos ventanas corridas entre sí para que un paso
ancho no repita el mismo peldaño celda a celda.

Y en el mismo viaje se cayó el argumento que metió las hojas `_Left` y `_Right`
el 9 ago: sus columnas del medio **no** son "tres celdas repetibles" — la banda de
roca cruza las 4×4 **en diagonal**, así que cualquier columna cortada es un trozo
de esa diagonal que no empalma con la de al lado. De un paso que baja de lado, en
3/4, se ve el filo de la meseta y nada más. Misma corrección en el acantilado:
`edge_e`/`edge_w` eran el filo **más el costado modular colgando dos celdas**, y
una corrida norte-sur lo repetía cada metro. Una celda.

### LAS LÍNEAS BLANCAS: la falda de arena viajaba en cada tinte (10 ago 2026)

Regla del usuario: *"como copiamos los acantilados de la playa de arena, se vino
con la textura unas líneas blancas. Eliminalas de casi todos, dejando el original
de arena con esa textura. La textura que deberían tener los teñidos es la de los
tiles vecinos que ocupan."*

El kit dibuja, entre la meseta y los cantos rodados, una **orilla clara de arena**.
Esos dos tonos —(200,155,132) y (161,121,108)— viven también dentro de la pared
como brillo de los cantos, así que `mapa_de_tinte` los clasificaba como ROCA, y la
roca no se tiñe. Medido sobre `cliff/pasto` contra `cliff/arena`: de los 3.072
píxeles de `edge_s`, **616 quedaban sin teñir y son exactamente esos dos tonos**
(312 + 304); en `labio`, los 148 sin teñir son ésos y nada más. En pantalla: una
línea crema cruzando el canto de todo acantilado de pasto y bajándole por el filo.

No se pueden teñir a lo bruto —son también el brillo de la roca, y pintarlos de
verde deja la pared con manchas de musgo—, así que se tiñen **sólo donde tocan el
suelo**: `import-acantilado::tenir` crece 4-vecinos desde los píxeles ya teñidos
de terreno y arrastra la orilla con el color del SUELO (no el de la roca), buscando
el tono de luminancia más parecida para que conserve su volumen. Resultado:
`labio` y `edge_e` quedan en 0 píxeles de falda, `edge_s` baja de 616 a 452 — y
esos 452 son el brillo de los cantos, que no toca la meseta y no debe cambiar.
La arena se queda como está: ahí la orilla no es una línea fuera de lugar, es el
médano derramándose sobre la roca.

### LA ONDA DE DUNA: la meseta tenía textura y el relleno no (22 ago 2026)

Pedido del usuario, textual: *"ese borde de césped tiene un patrón dibujado,
blanco sobre el césped... originalmente este asset es de arena. Se nota un cambio
horrible desde la textura del acantilado con líneas al terreno sólido de la
meseta. Quiero que se conserve el borde del acantilado y que con el relleno
sólido de la meseta sea un solo color."*

No es la falda de arena de la sección anterior —ésa vive entre la meseta y los
cantos y ya estaba resuelta—. Es el plano de ARRIBA: LimeZu le dibujó al *Sand
Mountain* las **ondas del viento sobre la duna**, bandas horizontales onduladas
más claras que el resto. Medido sobre las doce hojas del kit son **15.152
píxeles** en exactamente **tres tonos**, y los tres son los únicos tonos de suelo
por encima del dominante: luminancia 1,037, 1,057 y 1,081 contra `ARENA_ESPERADA`.

En un médano eso es la duna. Teñido de verde es un rayado raro sobre el pasto —y,
sobre todo, **muere de golpe donde empieza el relleno**, que es una losa de color
plano (`tiles/grass` son 1.024 píxeles de un solo color). Ahí no fallaba el
recorte ni el tinte: fallaba que la pieza tuviera textura y el relleno no.

La salida NO es texturar el relleno (el suelo base es color plano; lo que se lee
son las orlas, las variantes y la sombra). Es **aplanar la onda**: los tres tonos
se mandan al dominante, que `mapa_por_losa` ya manda al color exacto de la losa.
Todo lo demás vive por DEBAJO del dominante —el filo, el contorno, la sombra de
contacto, la falda— y no se entera, así que el borde del acantilado queda entero.

Medido sobre la celda de meseta de `cliff/pasto`: pasa de 436 a **740 píxeles del
color exacto de la losa** (71,151,87) y desaparecen los tres tonos claros. Meseta
y relleno son ahora indistinguibles.

**Menos la arena, y es decisión del usuario.** Preguntado en la misma sesión,
eligió *dejar la arena con sus ondas*: ahí el rayado no está fuera de lugar —es
la duna que el kit vino a dibujar— y vale más el asset como LimeZu lo hizo que la
costura contra la losa. Es el mismo criterio que ya valía para su falda. Medido:
los ocho materiales teñidos quedan en **0 píxeles más claros que su losa**;
`cliff/arena` conserva sus **304**.

`import-acantilado.py::verificar_onda` lo vuelve a medir en cada corrida: si el
pack cambia y aparece un cuarto tono claro, el importador se planta en vez de
dejar medio rayado puesto.

### NADA SE SIEMBRA PEGADO AL FILO (22 ago 2026)

Misma sesión, el otro medio del problema: *"estamos colocando árboles sobre el
borde... lo coherente es que se detecte el borde dos cuadros antes, hacia el
centro de la meseta, y se deje un límite de dónde no pueden estar cosas ni props,
porque si no es un árbol grande y queda la mitad afuera flotando."*

El canto ya era `Tile::Cliff` y ninguna tabla sembraba sobre él. Lo que faltaba
era el retiro **hacia adentro**: la celda pegada al canto es terreno llano y
edificable, así que el sorteo le plantaba el roble ahí y medio tronco quedaba
colgando sobre la pared que cae.

`relief::retiro_del_canto` responde si el terreno CAE dentro de
`RETIRO_DEL_CANTO` (2 celdas, Chebyshev). El número es el tamaño de lo que se
planta: un árbol del pack mide más de una celda y se dibuja hacia arriba desde su
pie. Lo consultan `worldgen::surface_decor_at`, `nature_decor_at` y
`prop_decor_at`, o sea las tres tablas que siembran sobre meseta.

**El llano no paga nada**: la pregunta sale por nivel antes de mirar una sola
vecina, así que las celdas de nivel 0 —la enorme mayoría del mundo— cuestan una
consulta a la caché y se van (mismo patrón que `es_paso`).

Medido (`medir_acantilados::props_pegados_al_filo`, Tierra 1.200×1.200):
**1.434.999 celdas de tierra alta, 50.380 dentro del retiro (3,5 %)** y **0 props
sembrados ahí** contra los que había antes.

### EL ÁRBOL CORTADO ERA UNA COPA SUELTA (10 ago 2026)

*"¿Qué carajos es ese árbol cortado? ¿Está mal importado, o es un árbol modular?
Si es así, construilo bien; si no, borralo y purgalo del juego."*

Es modular. El objeto sembrado era `ME_Singles_Camping_32x32_Tree_Props_20` y
familia: los 24 recortes de `Tree_Props_*` miden **4×2 celdas** y son la copa con
el arranque del tronco, cortada a ras arriba y abajo — van encima de un `Tree_N`,
que es el árbol entero. No hay pieza de tronco que los complete en todo el pack
(los `Tree_N` son árboles completos de 64×64 a 128×160), así que no había nada que
componer: se purgan de la siembra y quedan como colocables del editor.

Entraron cinco al bosque (`nature/arbol_4`, `_10`, `_16`, `_19`, `_22`) porque
`MARCAS_MODULARES` no tenía marca para ellos. Ahora la tiene —
`(?:^|_)tree_props(?:_|$)`—, y **ojo con generalizar a `props` a secas**:
`City_Props` es el nombre de un TEMA y ahí dentro hay 900 objetos enteros.

### TODA MESETA SE CIERRA CON UNA RAMPA (9 ago 2026)

Regla del usuario: *"toda meseta como mínimo debe tener una rampa para poder
subir en una dirección coherente. De ahí se puede tener cuatro […] pero como
mínima, una. Todas las mesetas se cierran."*

`relief::every_terrace_edge_has_a_pass` decía que sí y miraba el **ratio global**
de rampas contra caras. Un promedio sano no dice nada de la meseta concreta a la
que te estás acercando: medido componente por componente con un BFS de
caminabilidad encima (`medir_cascadas::mesetas_selladas`), **12 de 48 mesetas
eran islas** — ni rampa ni bordillo en todo su perímetro, y tampoco se llegaba a
ellas desde la terraza de arriba. De 3 a 238 celdas.

No es mala suerte, es perímetro: las franjas de paso son campos globales de
período 60 y 41, así que cruzan seguro una meseta de 200 tiles de lado y se le
escapan a una de 15. El barrido:

| familias de franja | pared | rampa | % paso | sin rampa propia | inalcanzables |
|---|---|---|---|---|---|
| 60, 41 (antes) | 3.154 | 1.249 | 28,4 % | 11 | 12 |
| + 23 | 2.575 | 1.828 | 41,5 % | 4 | 5 |
| + 23, 13 | 2.013 | 2.390 | **54,3 %** | 0 | 0 |
| + 23 y paso de meseta chica | 2.554 | 1.849 | 42,0 % | **0** | 1 |

La cuarta familia llega a cero y se lleva por delante un tercio de los muros:
más paso que pared en todo el planeta, o sea pagar en la cordillera un problema
que sólo tienen las lomas. Así que la loma se arregla en la loma:
`relief::paso_de_meseta_chica` rellena la meseta a la que pertenece la celda
—cortando en cuanto pasa de `MESETA_CHICA = 96`, que es lo que la hace barata— y
si de verdad es chica le abre **UN** paso, en la celda de su borde que le toca por
sorteo del ancla. Determinista y sin estado: todas las celdas de la meseta
contestan lo mismo. Media décima de ratio, y las mesetas selladas se acaban.

Queda **una** bolsa inalcanzable de las 48 (29 celdas, nivel 2): tiene su rampa,
pero baja a otra bolsa cerrada. La regla que se pidió —una rampa por meseta— está
en cero incumplimientos; esto es una cadena de dos y se deja anotado.
Guardián: `relief::toda_meseta_tiene_por_donde_subir`, que ya no mira el promedio
sino meseta por meseta.

### SE CAMINA HASTA LA CIMA — los tres montes sueltos (14 ago 2026)

Regla del usuario: *"la idea de todo esto es poder caminar hasta la cima de las
montañas, que haya caminos acantilados que nos lleven a la cima, otros que no nos
lleven a nada; el mapa está inspirado en GTA V, el monte Chiliad"*.

Faltaban las dos cosas: **el monte** y **el camino**.

**El monte.** La plantilla tenía una cordillera (Monte Altísimo) y una sierra, y
las dos son BARRERAS: se rodean o se cruzan por un puerto. Lo que no había era la
montaña que se ve entera desde el valle y se sube porque está ahí. Ahora hay tres,
declaradas como el Monte Olimpo marciano —espina cortísima para que salga un cono
y no una sierra— en `world_map::EARTH.ranges`:

| | dónde | escalón de la cumbre |
|---|---|---|
| Cordillera Monte Altísimo | cruza el continente | 9 |
| **Monte Solitario** | Campos de Chronos, lóbulo NE | 10 |
| **Pico del Cuervo** | llano del oeste | 11 |
| **Monte Ceniza** | lóbulo SE, a un valle de Villa Sur | 10 |

Los sitios no se eligieron a ojo: el primer intento los puso en el borde del
lóbulo y `donde_estan_las_montanas` los midió en 7 y 6 —el `falloff` del
continente les comía la base—. Movidos al núcleo de su masa de tierra, llegan a
10 y 11. Ninguno cae dentro del radio de una ciudad
(`named_cities_stand_on_habitable_ground` no admite pueblo por encima del 6).

**El camino.** La garantía que había —`toda_meseta_tiene_por_donde_subir`, en cero
incumplimientos— es **por meseta**, y un cono de doce terrazas son doce anillos
encadenados. Que cada anillo tenga su rampa en algún punto de su perímetro no dice
que los doce estén enhebrados: se podía subir tres escalones y quedarse dando
vueltas. `world_map::sendero_de_cumbre` los enhebra, y engancha exactamente donde
ya enganchaba la carretera (`relief::in_pass_band` sale por lo alto si
`roads::is_drivable_at`).

Es una **espiral de tres brazos**: la fase avanza con la distancia a la cresta y
retrocede con el rumbo, así que cada anillo del cono se cruza tres veces, a 120°.
Tres y no uno porque `es_paso` no abre nunca una cara que mire al sur —regla del
usuario— y un sendero de un solo brazo se sella la primera vez que su vuelta cruza
el flanco sur. **Y de ahí salen gratis los caminos que no llevan a nada**: el
brazo que se corta contra la cara sur es el ramal muerto, y hay uno por vuelta.

El sendero **propone, no impone**: `es_paso` sigue mandando, así que el sendero no
puede abrir un paso donde el dibujo no lo admite. Guardián:
`relief::se_camina_hasta_la_cumbre` — un relleno de caminabilidad desde el llano
de alrededor hasta el escalón de la cumbre, monte por monte, en cero
incumplimientos.

### LA CUMBRE ES BLANCA, y se decide por ESCALÓN (14 ago 2026)

`Biome::Snow` salía del término de altitud de `coldness_at`
(`altitude = (h − 0.74) / 0.20`), o sea del campo CRUDO. Era el último sitio del
módulo donde el bioma y la geometría podían discrepar, y discrepaban: el escalón
no sale del campo directo sino de `relief::level_from_field`, así que la nieve
caía donde el ruido pasaba un umbral y no donde el terreno levantaba una cumbre.

Ahora es la tercera grada de la misma escalera que ya usaban la sierra y la alta
montaña: `geography::NIVEL_NIEVE = 9`, medido —la cordillera corona en 9 y los
tres montes en 10-11, y la superficie del planeta se reparte casi entera entre los
escalones 0 y 3—. Con 9 la nieve es **el techo del mundo y nada más**.

**Y el pixel.** `Tile::Ice` no tenía losa: la cumbre habría salido como un
rectángulo celeste plano. El pack no trae ni un tile de nieve —buscado: lo único
blanco son props de heladería y de navidad—, así que se TIÑE, que es la puerta por
la que ya entraron la sabana y el pantano (`atlas-suelo.py::derivar` traslada la
losa de pasto a otro color sin aplanarle la textura). El destino no se eligió a
ojo: es la rampa con la que `import-acantilado.py` ya tiñe `cliff/nieve`, así que
**la cumbre y su acantilado son literalmente el mismo blanco**, y coincide con el
color declarado de `Tile::Ice`. Cerrado el bucle, `LOSA_DEL_MATERIAL["nieve"]` por
fin encuentra su losa y la meseta del acantilado nevado se cura con ella.

**Y DOS COSAS QUE SÓLO SE VIERON EN LA CAPTURA** (`relieve-3-cumbre.png`).
Ninguna medición las iba a cazar, porque las dos son "está dibujado donde toca,
pero se ve mal" — el método de siempre.

- **La cumbre salía como una plancha lisa.** `world2d::tile_render_color` aclara
  el suelo un 3,5 % por escalón para que el relieve se lea desde el aire, y eso es
  una MULTIPLICACIÓN sobre el color de vértice que `ColorMaterial` cruza con la
  losa. Con suelos oscuros sobra margen —el pasto es (0.36, 0.62, 0.29) y aguanta
  el +40 % sin despeinarse—, pero la nieve es (0.82, 0.88, 0.93) y en el escalón
  10 sale `0,93 × 1,38 = 1,28`: todo lo claro de la textura cae en el mismo blanco
  recortado y **la losa se aplana entera**. Ahora el aclarado se topa contra el
  canal más alto (`shade.min(1/max)`), que además no mueve el TONO — recortar
  canal por canal viraría la nieve al gris.
- **El pico nevado estaba FLORECIDO.** `worldgen::nature_decor_at` sorteaba el
  4 % de todas las celdas de la Tierra sin mirar el bioma —sólo esquivaba el
  asfalto—, así que el arbusto de jardín y la mata de bosque salían igual en la
  pradera que en la cumbre. No se notaba mientras la nieve no existía casi.
  Guardián: `worldgen::en_la_nieve_no_crece_nada`.
- **Y con la flora fuera, se vio la segunda:** cada piedra de la cumbre tenía un
  LUNAR VERDE debajo. `decor/rock` es el único que tiene pixel —`rock_moon` y
  `rock_mars` son slots vacíos— y viene con su óvalo de pasto dibujado, que es lo
  correcto sobre pradera. No se le recorta la base (memoria: *el asset se usa como
  está dibujado*): `rock_permille` pasa a 0 en `Snow`/`Ice` y la cumbre queda
  limpia. El día que `rock_moon` tenga pixel —una piedra con base neutra— la
  cumbre lo pide y vuelve a 30, porque piedra asomando entre la nieve **es** lo
  que tiene que verse.

### LA LÍNEA DE NIEVE ES UNA CURVA DE NIVEL, NO UNA GRECA (16 ago 2026)

La sección de arriba dejó la nieve donde tenía que estar —el escalón 9, medido— y
con su losa. Lo que no arregló, porque no se veía hasta que los chunks se armaron
enteros (§10, el arreglo del coste de `relief_cell`), es **la forma de la
frontera**. En `relieve-0-pared.png` fresco, la cresta de la cordillera salía así,
medido con `ha-shared/tests/mapa_de_la_nieve.rs` sobre la pantalla de 80×40 tiles:

| | |
|---|---|
| nieve / montaña en el cuadro | **73 % / 27 %, entreveradas** |
| celdas dentro del damero de transición | **33,9 % de la pantalla** |
| ancho de la franja al CRUZARLA | 1 tile de mediana, 95 % por debajo de 8 |

Las dos últimas filas juntas son el diagnóstico, y son lo contrario de lo que
parecía. **El ancho de la franja estaba perfecto** —`AJEDREZ_EN_PASOS` cumple: se
cruza en ocho pasos—; lo que pasaba es que **la frontera pasaba por todas
partes**. Un tercio de la pantalla estaba a menos de dos tiles de un borde. En
pantalla: confeti blanco y verde de horizonte a horizonte, la misma queja que
`ANCHO_DE_TRANSICION` documenta haber perseguido tres veces bajando un número.

**Por qué, y por qué bajar el número nunca lo iba a arreglar.** El campo de altura
lleva la octava de lomas de 34 m y la local de 110 m (`relief::terrain_noise`).
Sólo la de 110 m mueve el terreno **0,055 unidades de campo**, y un escalón mide
0,047: una sola octava fina hace cruzar la cota más de un escalón entero. La
frontera de *cualquier* umbral sobre un campo así no es una curva de nivel, es una
greca fractal — y donde el campo ronda la cota en una zona ancha (la cresta de la
cordillera es exactamente eso), la greca **llena la zona**. Ninguna calibración del
ancho toca eso, porque el problema no es el ancho.

**El arreglo: la nieve tiene su propio campo.** `relief::campo_de_cordillera` es
la plataforma del mundo más las dos octavas GRANDES (continental y regional),
renormalizadas — o sea el mismo terreno sin el temblor de las lomas. Sobre ese
campo la frontera vuelve a ser una curva de nivel, y el ajedrez de Bayer se dibuja
alrededor de ella (`geography::pasa_la_linea_de_nieve`, con el mismo ancho medido
caminando). Resultado en el sitio de la captura: **0 % de damero**, y una diagonal
limpia con su franja de ocho pasos en `relieve-4-linea-de-nieve.png`.

**Sólo la nieve, y por un motivo concreto.** Sierra (5) y alta montaña (8) pintan
el MISMO material en un mundo templado (`biome_tile` → `Tile::Grass` con la losa
`pasto_sierra`), así que su frontera no se ve y da igual por dónde vaya. La nieve
pinta `Tile::Ice`: blanco contra verde. Es la única grada cuya frontera el ojo lee.

**No toca la geometría.** Escalones, acantilados y rampas siguen saliendo del campo
completo; lo único que decide el campo suave es de qué está hecho el suelo — que es
lo que ya hacía el difuminado.

**Lo que costó, dicho sin maquillaje.** La nieve pasa del **2,52 % al 4,73 % de la
tierra firme** (medido con las dos reglas completas, con su ajedrez, sobre la
Tierra entera). No es un efecto del sesgo del difuminado —se probó centrarlo y
sólo movió 0,05 puntos—: es que una ladera tiene más superficie por debajo de su
línea de nieve que por encima, así que cualquier franja de transición ancha suma
neto. Se acepta a propósito:

- la regla de la casa es cualitativa —*la nieve es el techo del mundo, no aparece
  en ninguna loma*— y el campo suave la cumple MEJOR, porque sólo puede nevar donde
  la plataforma levanta cordillera, nunca donde un montículo cruzó el umbral;
- subir el umbral a 10 la devolvería al 0,53 % **y dejaría la cordillera Monte
  Altísimo sin nieve** (corona en el escalón 9), que es justo lo que la sección de
  arriba fue a buscar.

Guardianes de medida (no son asserts, son números para leer):
`mapa_de_la_nieve::cuanto_de_la_pantalla_es_damero`,
`::las_dos_reglas_de_nieve_lado_a_lado`, `::a_que_escalon_va_la_linea_de_nieve`.

### El río sabe que hay lagos (14 ago 2026)

> ⚠ **SUPERADA EL MISMO DÍA.** El tirón del sumidero que describe esta sección
> subió los lagos con afluente de 11 a 20 de 36 y **no movió el descenso**, que
> era el otro medio pedido: los ríos seguían sin bajar y sin llegar a ninguna
> parte. Se retiró junto con el resto del entramado de ruido — ver §"El río se
> traza, no se pinta", que es donde vive la regla ahora. Queda por la lección de
> método: *deformar el DOMINIO no cambia la densidad del campo; mezclar dos campos
> independientes, sí*.

Regla del usuario: *"hay ríos que terminan en lagos, no todos"*.

Los cauces eran una cresta de ruido y las lagunas una cuenca que hunde el terreno:
**dos campos que se suman en la altura y no se hablan**. Medido antes de tocarlo
(`tests/medir_rios.rs`, nuevo):

| | antes | ahora |
|---|---|---|
| lagunas con agua que tienen un afluente | **11 de 36** | **20 de 36** |
| cauce sobre la tierra, llano | 2,33 % | 2,21 % |
| cauce sobre la tierra, sierra | 1,47 % | 1,32 % |
| cauce sobre la tierra, cuenca oeste | 7,29 % | 5,92 % |

Los lagos casi duplican sus afluentes **y el reparto de cauce del planeta no se
mueve**, que es la mitad que importa: lo que cambió es hacia dónde van los ríos,
no cuántos hay.

Lo hace **una** deformación del DOMINIO —nunca de la respuesta—, sumada al warp de
ruido que el cauce ya tenía: `geography::tiron_del_sumidero`.
`world_map::sumidero_cercano` devuelve el lago o cuenca que manda en esta celda y
cuánto tira (1 pegado al agua, 0 a cinco radios), y el punto de muestreo se
desplaza hacia AFUERA esa cantidad, con lo que el entramado se contrae sobre el
lago. El que pasa cerca entra; el que pasa lejos sigue de largo — que es lo que
deja lagos sin afluente, como se pidió. El tirón está barrido, no elegido: 220
tiles dan 17 lagos, 350 dan 20 y de ahí para arriba no se gana ninguno y el
reparto de cauce se empieza a mover.

**Por qué se deforma el dominio y no la fase.** Costó dos mediciones y vale
anotarlo. El primer intento fue mezclar la fase del entramado con una fase
RADIAL —cerca de un lago las líneas de flujo son sus radios— con el peso del
sumidero. Funcionó para los lagos y destrozó el resto del mundo: el llano pasó de
2,33 % de cauce a **0,15 %**, y el segundo intento (la fase radial como fbm en
coordenadas de embudo, para que la distribución coincidiera) lo dejó en **0,02 %**.
Mezclar dos campos independientes no conserva la longitud de su conjunto de
nivel, y la banda del río ES un conjunto de nivel: las dos crestas se pelean.
Deformar el dominio no puede tener ese problema — es el mismo campo, con la misma
estadística, muestreado en otro punto.

Y una vuelta más de tuerca: **el signo del tirón estaba al revés**. Empujar la
muestra hacia el centro DILATA el patrón alejándolo del lago (11 → 7 afluentes);
empujarla hacia afuera lo contrae sobre él (11 → 17, y 20 con el tirón barrido a
350 tiles).

**Lo que NO se consiguió, con los números delante.** El pedido también decía que
el río *baje* de la montaña, y **eso sigue abierto**: siguiendo el cauce aguas
abajo, la bajada media es de **0,01 a 0,19 escalones** y **todas** las cabeceras
mueren en seco a unos doce tiles, igual que antes.

Se probó y no entró un `sesgo_de_pendiente`: deformar el dominio a favor de la
pendiente de `world_map::platform`, desplazando la muestra cuesta arriba en
proporción a la altura, para que la cresta se alargara cuesta abajo. Medido, **no
movió el descenso ni un ápice** (misma bajada media, mismas cabeceras en seco); lo
único que movió fue la cantidad de cauce en la sierra (1,32 → 3,89 %), que además
puede ser fragmentación y no ríos, porque el gradiente normalizado da tumbos donde
la plataforma es plana. Cuatro consultas de plataforma por celda de tierra en el
camino de `biome_at`, un 4-6 % de la generación del mundo, a cambio de nada
demostrado: se sacó. Si hace falta más río de montaña, la perilla barata es
`RIVER_HALF_CUMBRE`.

Un cauce que baje de verdad de la cumbre al mar **no se saca deformando un campo
de ruido — hay que trazarlo**. Anotado en §13.

**Y el que mueve los ríos rompe todo lo que colgaba de ellos.** El tirón dejó tres
tests en rojo y ninguno era un bug del tirón:

- `la_cascada_mide_lo_que_baja_el_terreno` comparaba el alto del salto contra la
  resta de NIVELES, que nunca pasa de uno; pasaba sólo porque las cascadas de su
  ventana caían todas en la terraza 1. Movidos los cauces apareció una en terraza
  alta: *"dibuja 4 pisos y el terreno baja 1"*, las dos cosas ciertas. Ahora
  pregunta por `relief::alto_de_pared`, y de paso el reparto que imprime enseña
  saltos de **2, 3 y 4 pisos** — la variedad de tamaño que se pedía, medida.
- `la_garganta_cae_al_ras_de_sus_tres_columnas` barría 800×800 alrededor del
  origen y se quedó sin una sola cascada. Barre las `VENTANAS` como sus hermanos.
- `una_calle_mide_lo_mismo_de_punta_a_punta`: un arroyo entró en Metrópolis Norte
  y le comió una fila de asfalto (dentro de un casco la calzada no puentea, §"la
  ciudad se acaba en la orilla"), así que el ancho medido bajaba de 3 a 2. Eso no
  es la calle cambiando de clase, que es lo único que ese test defiende.

Guardián: `geography::hay_lagos_con_afluente_y_lagos_sin_el`, con las dos mitades
—que al menos un cuarto tenga afluente y que **no lo tengan todos**—.

### La cascada encaja con la orilla (14 ago 2026)

Tres defectos que producían la costura del pie del salto:

1. **Faltaba la exención de la POZA.** `agua::boca_de_cascada` existía desde el 9
   de agosto y por eso el río de arriba desemboca limpio en el salto; **no había
   simétrica abajo**. Las celdas de agua bajo el reventón calculaban su autotile
   normal y, como a los costados hay tierra (los hombros y el pie de la pared), le
   pedían `edge_e`/`edge_w`/`corner_*` y pintaban su ribete de barro **debajo del
   sprite de la poza**: el chorro reventaba contra un borde de orilla en vez de
   contra su propia agua. La transición ahí ya existe —es la poza del recorte, con
   la espuma dibujada—, así que `agua::poza_de_cascada` la declara agua para el
   autotile, igual que la boca. Es barata por el mismo truco que
   `bajo_el_chorro`: pared primero (caché), `cascada_at` sólo si la hay.
2. **Empate de z.** El tile de agua y la boca/poza pedían los dos `layers::FLOOR`
   exacto, sin desempate: funcionaba de casualidad, porque `spawn_chunk_cascadas`
   corre después del bucle de tiles del chunk, y dejaba de funcionar en cuanto la
   poza caía del otro lado de una frontera de chunk. Estrena
   `layers::SOBRE_EL_PISO`, muy por debajo de `YSORT_MIN`: sigue siendo suelo y
   sigue sin tapar a nadie.
3. **`POZA_FILAS` contra el alto real del recorte.** El cliente leía la altura del
   manifest con un `unwrap_or(1)` justificado en que "las variantes derivadas de
   la hoja entera miden uno". Medido: **las 30 miden dos**. El respaldo ahora es
   `agua::POZA_FILAS` y hay guardián que recorre el manifest
   (`art2d::la_poza_del_pack_mide_lo_que_dice_la_sim`).

**Y la orilla del lago dejó de ser una playa.** `agua::orilla_de` salía por lo alto
con `relief_level < 0 → Orilla::Arena` pensando en el mar, y **un lago también
está bajo el nivel del mar**: toda laguna del planeta salía ribeteada con el panal
de arena, incluidas las del bosque y las de la sierra. Es justo el "panal fuera de
la playa" que se pidió sacar. Ahora el agua de tierra adentro
(`world_map::agua_interior`, O(1)) sigue de largo y se le pregunta a la tierra de
al lado, igual que a un río. El test que debía cazarlo no podía: clasificaba
"sierra" por el bioma de la celda MOJADA, que nunca es `Highland` — es `River` o
`Ocean` —, así que su contador salía 0 pasara lo que pasara.

### La fila de llano que se había perdido (14 ago 2026)

`SUELO_AL_PIE = 3` significaba "dos filas que tapa la roca **más una de llano**".
Al reescribirlo en función del alto real de cada pared
(`relief::bajo_el_dibujo_de_una_pared`) se conservaron las dos primeras y se
perdió la tercera. Ningún guardián lo cazó —el que hay pregunta por lo que el
dibujo tapa, no por lo que se ve debajo— y la biblia siguió anotando el número
viejo. Lo cazó la medición: `medir_cascadas::pie_de_pared_contra_agua` daba
**110 caras de 3.666 con el agua pegada al pie** donde acá decía 0.

Arreglado, quedan **2** (0,05 %), las dos en paredes altas donde el dibujo se poda
contra un canto de más abajo. Guardián nuevo con el número medido, no con un
porcentaje: `agua::queda_una_fila_de_llano_al_pie_de_toda_pared`.

### LA MESETA SE APILA: la pared mide lo que mide su macizo (14 ago 2026)

La queja, con captura tachada con una X: *"solo se está poniendo el borde donde
está el acantilado y ya; sigue siendo el mundo base y sólo lo divide ese borde.
No es como que el mundo base fuera una superficie, se crea la meseta sobre, el
siguiente nivel, se crea la siguiente superficie… no sólo el borde y lo demás
vacío"*. Con dibujo: tortitas apiladas, cada una con su tapa y su canto.

Y la causa, que ninguna media cazaba porque las medias estaban bien. Volcado de
una ladera de la sierra (`medir_acantilados::mapa_de_zona` sobre 1763,-5086):

```text
   ...MMMMM#.....#MMMMMMMM   8888889999988888899999999
   ...#######################   88888888888888888888888
```

Esos `MMMMM` son **manchas de nivel 9 de dos filas de fondo** —el campo de altura
rozando el umbral del escalón, un montículo— y `SALTO_DE_TERRAZA` dice que entre
la terraza 8 y la 9 hay **diez escalones**. O sea que cada pimpollo de 5×2 celdas
colgaba un farallón de once filas de roca; como la ladera está llena de ellos, los
muros se solapaban hasta tapar la llanura entera. Eso es la pared gris que se come
media pantalla en `relieve-0-pared.png` y el marrón que llena la isla de punta a
punta en la captura del usuario.

Tres topes, los tres en `relief.rs`, y ninguno toca el trazado del borde (la forma
la sigue mandando el terreno, que es lo que se pidió):

| Tope | Regla |
|---|---|
| `meseta_detras` | una pared no puede ser más alta que filas tiene de meseta detrás del canto |
| `hueco_al_sur` | y le deja a la terraza de abajo `TERRAZA_VISIBLE` = 2 filas de suelo propio |
| `pie_de_pared` | usa el MISMO alto que dibuja el canto (antes usaba el bruto → roca invisible) |

Medido (`medir_acantilados::cuanto_recortan_los_topes_de_la_pared`): las caras de
diez escalones bajan de **812 a 568** y el resto se reparte por 4-9; 8 % menos
filas de roca en total. Guardianes: `una_pared_no_es_mas_alta_que_su_meseta` y
`la_terraza_de_abajo_siempre_enseña_su_suelo`.

### LA SOMBRA CAÍA DENTRO DE LA ROCA (14 ago 2026)

La otra mitad de la misma queja, y la más barata de arreglar. `terrace_shadow`
preguntaba por el vecino del NORTE — pero el vecino del norte de la primera celda
visible de una terraza no es el canto de arriba: es el **pie del muro**, que tiene
el nivel de la terraza de abajo porque es la huella que el dibujo tapa
(`ReliefCell::Pared`). La comparación daba cero y la franja oscura se pintaba
entera debajo del sprite del acantilado. En pantalla: dos praderas del mismo verde
separadas por una banda de roca, sin ninguna pista de cuál está más alta.

Ahora la sombra la calcula `world2d::sombra_de_muro_en_la_columna`, que barre la
columna del chunk de norte a sur, y cada canto escribe su franja **desde el pie de
su dibujo** (`relief::pie_del_muro`) hacia abajo. Por columna y no por celda: la
versión por celda buscaba el canto hacia el norte hasta quince filas y el chunk
tardaba tanto en hornearse que la captura del driver salía verde lisa.

Y dos cosas del color, que iban en la misma dirección:

- El aclarado por escalón era **3,5 %**, o sea menos que el ±4,5 % de grano por
  celda que se aplica justo arriba: el escalón quedaba por debajo del grano del
  suelo. Ahora **4,5 %**.
- El recorte anti-blanco (`shade.min(1/alto)`) recortaba `shade` **entero, grano
  incluido**, así que en la nieve todas las celdas caían en el mismo tope y la
  cumbre seguía siendo una plancha lisa — sólo que sin saturar. Se recorta el
  ACLARADO y el grano modula alrededor.

Guardián: `world2d::la_sombra_del_muro_cae_donde_se_ve` — ninguna celda sombreada
puede ser cuerpo de pared.

### LA MESETA ES TIERRA SOBRE EL PISO BASE (14 ago 2026)

Regla del usuario, textual: *"solo al picar el piso base llegamos a las cuevas, no
el piso de las mesetas, ya que en teoría es como que hay tierra sobre el piso
base; para llegar a la cueva cavás toda esa tierra"*. Las capas del piso base, de
arriba abajo: **tierra, piedra, cuevas, piedra, bedrock** (irrompible).

Antes el subsuelo era plano: un tile de pico en la cumbre de la cordillera te
dejaba a la misma profundidad que uno en la playa — el relieve terminaba en la
superficie y por debajo el mundo se olvidaba de la montaña. Ahora
`topdown::topdown_tile` mide la profundidad desde el piso base
(`zb = z + tierra_de_meseta`), con `TIERRA_DE_MESETA_MAX = 6` de tope y
`LECHO_DE_ROCA = -16` como profundidad absoluta del bedrock.

**La cueva va primero y sigue pegada a su boca.** Moverla de nivel con el terreno
dejaría colgado el tramo de escalera que baja desde la boca, y como el 43 % de la
tierra es terraza 1, enterrarlas las volvería inencontrables en casi todo el
mundo. Lo que cuesta es cavar **a ciegas**, que es de lo que hablaba la regla.

Guardián: `topdown::cavar_en_la_meseta_cuesta_su_tierra`.

### EL RÍO SE TRAZA, NO SE PINTA (`ha-shared/src/rios.rs`, 14 ago 2026)

Pedido del usuario: *"que surja de por ahí de algún lado alto y de ahí vaya
bajando, se bifurque, llegue al lago, llegue al mar"*. Las cuatro son propiedades
de un TRAZADO, y ninguna se puede sacar de un campo de ruido.

**Lo que había medía así** (`medir_rios`, Tierra, tres ventanas de 1.200×1.200):

| | entramado de ruido | trazado |
|---|---|---|
| cabeceras seguidas aguas abajo | 34 | 491 ríos trazados |
| **que llegan al mar o a un lago** | **0** | **383 (78 %)** — 141 al mar, 242 a un lago |
| bajada media siguiendo el cauce | 0,01–0,19 escalones | monótona por construcción |
| cauce sobre la tierra (llano / sierra) | 2,21 % / 1,32 % | 2,97 % / 4,18 % |

Cero. **Todas morían en seco y ninguna bajaba**, y no era cuestión de calibrar:
se probaron el tirón del sumidero, dos variantes de fase radial y un sesgo por la
pendiente de la plataforma, y ninguno movió el descenso ni una décima. *Un campo
de ruido no tiene aguas abajo* — cruza laderas en diagonal porque no sabe qué es
una ladera. El cementerio completo, con los números de cada intento, quedó en el
docstring de `geography::river_here`.

**Cómo se traza sin generar el planeta.** El mundo se genera por celda y bajo
demanda, así que no hay dónde correr una hidrología. El trazado es determinista y
local:

1. El planeta se parte en cuencas de 900 tiles; cada una sortea **cuatro**
   nacientes candidatas.
2. Vale la que cae en la **ladera alta** (0,30 … 0,86 de la escalera de tierra).
   Es literal del "no desde el top de la montaña": el pico es nieve y roca.
3. Desde ahí se baja por **máxima pendiente de `world_map::platform`** —la
   plantilla autorada— a zancadas de 48 tiles, hasta el mar, hasta una laguna o
   hasta agotar 80 zancadas (3.840 tiles).
4. En un punto sorteado del tercio medio **se bifurca**: sale un brazo con sesgo
   lateral que baja por su cuenta.

Se baja por la PLATAFORMA y no por el campo completo porque el campo lleva el
ruido encima y el ruido tiene mínimos locales cada pocos tiles: un descenso por
ahí se mete en el primer hoyo y se queda.

**Y el río impone su cota, no resta una hondura.** Éste fue el segundo error y
vale anotarlo: el primer intento restaba 0,03 al campo donde había cauce. La
amplitud del ruido de terreno es **0,42**, o sea catorce veces más, así que el
fondo del río seguía subiendo y bajando con el terreno y seguirlo aguas abajo
moría a las dos celdas — el mismo "muere en seco", con otra causa. Ahora cada
tramo trae la cota de la plataforma en sus extremos, **monótona decreciente por
construcción**, y `relief::compute_field` lleva el campo hacia ella con un peso
que vale 1 en el eje del agua y se apaga en el borde del valle (tres veces el
ancho del cauce). El fondo es monótono y la orilla empalma suave.

Perillas, todas medidas y comentadas en `rios.rs`: `PASO`, `PASOS_MAX`,
`RADIO_CUENCAS`, `MEANDRO` (sin él el cauce sale recto: 45 tiles de desvío en
1.536 de avance), `TIRON_DEL_AGUA` y `ESTANCADO_MAX` (un collado no termina un
río, se cruza).

> **Lo que costó**: dos guardianes tuvieron que cambiar de PREGUNTA, no de umbral.
> `hay_lagos_con_afluente_y_lagos_sin_el` sondeaba la orilla de cada laguna
> buscando la primera celda seca — y la desembocadura de un río trazado está
> **por debajo** del nivel del líquido, así que la descartaba por mojada: daba
> 1 de 36 mientras el trazador informaba 49 % de ríos terminando en laguna.
> Ahora le pregunta al trazador (`rios::destinos`).
> `la_garganta_cae_al_ras_de_sus_tres_columnas` se quedó sin cascadas en sus
> ventanas fijas porque los saltos se concentraron **donde tienen que estar**, en
> la montaña; ahora barre buscando sierra, como `medir_cascadas`.

Herramientas: `cargo test -p ha-shared --test medir_rios -- --ignored --nocapture`
(`donde_termina_cada_rio` es la medición que importa).

### LA COSTURA DEL SUELO: orillas y difuminado de biomas (14 ago 2026)

Dos quejas del usuario que resultaron ser el mismo defecto: *"la cumbre nevada se
ve como un rectángulo blanco pegado sobre el prado"* y *"el pozo que cavo sale con
unas líneas negras alrededor"*.

**La causa, medida:** el suelo lo pinta la malla del chunk con **una losa plana
por celda** —`tiles/grass` son 1.024 píxeles de (71,151,87) y ni uno más—, así que
donde dos materiales se tocan hay un corte a escuadra entre dos colores. No había
ninguna pieza de transición en todo el suelo del juego.

Tres cosas lo arreglan, y las tres salen de pixel que el pack ya tenía:

1. **Las orillas** (`scripts/import-suelo-bordes.py` → `borde/tierra`,
   `borde/arena`, `borde/nieve`, más una variante por clima de pasto). `Grass_3`
   de `1_Terrains_and_Fences` es un **autotile entero de tierra contra pasto**, 22
   recortes, y estaba sin usar. El importador lo clasifica midiendo cada pieza en
   una grilla de 3×3 (y se planta si el pack cambia), tiñe el lado del material a
   arena o a nieve y el lado del pasto al verde del clima vecino.
   `world2d::spawn_chunk_orilla` las siembra **sólo en las celdas de borde** — el
   relleno lo sigue dibujando la malla, un draw call por chunk (regla 2).

   > **Y desde el 21 ago 2026, también entre dos PASTOS** (`borde/pasto_bosque`,
   > `_seco`, `_pantano`, `_sierra` y sus cruces: 10 claves nuevas). Es la misma
   > pieza con el lóbulo de material teñido a otro verde en vez de a un marrón,
   > con el grano comprimido ×0,53 —cociente medido de los rangos de luminancia de
   > los dos lóbulos del kit— para que no salga empedrada. Faltaba, y era la mitad
   > del bug "las mesetas no tienen su terreno": la frontera sierra/pradera es un
   > cambio de losa tan visible como la arena contra el pasto y salía a escuadra.
   > La dibuja el pasto de mayor rango (`world2d::rango_de_pasto`) — una costura,
   > un solo dueño. Detalle en §5.
2. **El sólido**. `tiles/sand`, `tiles/dirt` y `tiles/ice` no tenían losa: la
   búsqueda por color de `atlas-suelo.py` no encontraba nada a menos de
   `DELTA_MAX` y se quedaban en color plano inventado. Ahora los tres salen de
   `Grass_1` trasladado al tono del pack (la arena del mundo es **(230,174,85)**,
   medida en `Sea_Water_Tileset` y en la meseta del kit de acantilado: son el
   mismo amarillo), y **con sus siete variantes**, que es lo que le da grumos a la
   nieve y grano a la arena. Sin eso la cumbre era una plancha lisa.
3. **El difuminado de biomas** (`geography::pasa_el_escalon`). La nieve, la
   montaña y la sierra se decidían con un `>=` sobre el escalón, y un escalón es
   una meseta entera. Ahora el borde se reparte con una matriz de Bayer 4×4 según
   cuánto le falta a la celda para subir (`relief::nivel_fraccion`): abajo casi
   ninguna celda pasa, en el medio pasa una de cada dos —damero— y arriba pasan
   casi todas. Es literal del pedido: *"intercalando tipo ajedrez, difuminándose
   en pixeles"*.

   **La franja se mide CAMINANDO, no en altura** (14 ago 2026). Ése era el bug del
   confeti verde y blanco que salía en toda captura del relieve. El ancho se
   definía como "el N % de arriba del escalón", y un porcentaje de ALTURA no
   significa lo mismo en los dos sitios: en una ladera empinada el 7 % son dos
   pasos, pero en la pradera casi plana de `relieve-0-pared.png` **son cientos de
   tiles de tablero**. Por eso bajar el número tres veces (0,45 → 0,12 → 0,07)
   nunca lo arregló: el ancho nunca fue el problema, la unidad sí.
   `pasa_el_escalon` calcula ahora la pendiente local de la fracción y de ahí saca
   cuánta altura son `AJEDREZ_EN_PASOS` (8) pasos AQUÍ. Y la frontera alrededor de
   la que se teje el ajedrez dejó de ser una curva de nivel perfecta: la ondula
   `ondula_el_borde` con ruido suave, así la nieve entra y sale en lengüetas. El
   ajedrez se queda —se pidió— pero pegado al borde, no de pantalla a pantalla.

   > **Y el ajedrez se quedó SÓLO en la nieve** (21 ago 2026). Sierra (5) y
   > montaña (8) pasan por `pasa_la_grada`, que es la misma frontera ondulada
   > resuelta con una LÍNEA en vez de con la trama. Dos motivos: entre dos verdes
   > parecidos el damero no se lee como degradado sino como suciedad, y —el de
   > fondo— la costura entre dos pastos ahora la dibuja el pack, y una orla
   > necesita una frontera con un adentro y un afuera, no una nube de celdas
   > sorteadas. El área se conserva: 0,5 es el promedio de la matriz de Bayer.

> **El acantilado NO se difumina.** Una pared es una pieza de kit de tres celdas
> de alto y su material sale del suelo que corona: con el bioma difuminado saldría
> a rayas. La trama es del suelo; la pared es de la terraza.
>
> Desde el 21 ago 2026 eso **no lo decide quien pregunta**: lo decide
> `geography::superficie_de`, que sobre relieve contesta con el bioma liso y en
> llano con el difuminado, y es la MISMA función con la que la malla pinta esa
> celda. `biome_de_terraza` sigue existiendo pero ya no la llama el juego, sólo
> las mediciones — tener dos entradas era justamente el bug del anillo (§5).

**Y el sombreado volvió al suelo texturizado.** `build_chunk_mesh` ponía el color
de vértice en BLANCO cuando la celda tenía losa —correcto, para no teñir la
textura con el color del material— y con eso se iba también el grano por celda, el
aclarado por escalón y la sombra de terraza. Era el motivo de fondo de que la
cumbre se viera como una plancha: la nieve tiene losa. Ahora el vértice lleva el
sombreado **en gris** (`world2d::tile_render_color_y_sombra`), que modula la losa
sin virarle el tono.

**Las líneas blancas del acantilado.** El kit de relieve sale del *Sand Mountain*
del tema de playa y trae una orilla clara de arena entre la meseta y los cantos.
Había un mecanismo para teñirla que sólo alcanzaba a la que TOCA el suelo: medido
sobre la hoja publicada, **el 8,74 % de los píxeles de `cliff/pasto` seguían siendo
exactamente esos dos tonos de médano**. Ahora la que no toca suelo se manda a la
roca por luminancia (`import-acantilado.py::falda_a_roca`) y queda en 0,00 % en
todos los materiales **menos la arena**, que es el kit original y no se toca.

### UNA PARTIDA NUEVA ES UN MUNDO NUEVO (`ha-game/src/partida.rs`, 14 ago 2026)

Pedido: *"cada que abra una partida, a menos que la guarde, que se abra un mundo
nuevo; no me gusta que cosas que coloqué antes estén puestas"*. Eran dos cosas
eternas:

- el terreno salía de `celestial_bodies::body_terrain_seed`, una **constante
  compilada**, así que la Tierra era siempre la misma;
- y el estado se autoguardaba en `inventory_persist::persist_dir()` sin que nadie
  lo borrara nunca.

Ahora **NUEVA PARTIDA** sortea una semilla y tira el estado anterior; **CARGAR**
recupera la semilla de `partida.json` y no borra nada. La semilla se mezcla en
`body_terrain_seed`, un solo sitio por el que pasa todo el ruido del planeta:
relieve, biomas, ríos, cuevas y los pueblos sorteados. La **plantilla** del cuerpo
no se mueve —continentes, cordillera central y ciudades con nombre son datos
autorados, la geografía que el juego promete—; lo que cambia es el ruido que la
rellena.

> ⚠ **`persist_dir()` es `"."` en escritorio**, o sea el repositorio. El borrado
> enumera **exactamente** los archivos que el juego escribe (`partida::ARCHIVOS` y
> `CARPETAS`) y no toca nada más. El test
> `ningun_estado_de_partida_se_escapa_del_borrado` lee el propio código fuente
> buscando `persist_dir().join("…")` y falla si alguien agrega un archivo nuevo
> sin ponerlo en la lista — el modo de falla que cuida es silencioso: un mundo
> nuevo con un trozo del viejo pegado.
>
> Las cachés de mundo (`relief::LEVEL_CACHE`, `geography::SITIOS`, `city`,
> `cuevas`) llevan la **época** de la semilla en su clave (`ha_shared::partida`):
> sin eso, la primera partida nueva salía con parches del mundo anterior donde la
> caché seguía caliente.

### EL AGUA DE MAR NO ES EL AGUA DE ADENTRO (14 ago 2026)

`familia_de_bioma` mandaba la sabana a `Orilla::Arena`, y la sabana tiene suelo de
**pasto** (`biome_tile(Savanna) → Grass`). Resultado, visible en
`agua-3-playa.png`: un río de llanura con las dos riberas de panal de playa
cruzando un campo verde. Es el mismo bug de las dos tablas paralelas que ya se
arregló del lado del acantilado, ahora del lado del agua. La sabana bebe con
pasto; el panal se queda para el mar y para el médano de verdad (`Desert`, que sí
tiene suelo de arena).

### EL CHARCO DE UN TILE NO SE PUEDE DIBUJAR (14 ago 2026)

Los cuadrados celestes que el usuario marcó en círculo rojo por todo el borde de
la laguna. El pantano sorteaba sus charcos con ruido de **período 9** —o sea casi
celda a celda— y una celda de agua con tres o cuatro lados en tierra no tiene
pieza en el pack: `autotile_shape` cae a `center`, que es el interior del agua,
sin orilla.

Se arregla en el origen y en dos pasos, porque hacen falta los dos:
`CHARCO_PERIODO` sube a **14** y `geography::orilla_dibujable` le exige a la celda
que tenga con qué casar. Medido
(`medir_acantilados::agua_que_no_se_puede_dibujar`): de **1,70 % a 0,36 %**.

El período está medido, no elegido: con 22 baja a 0,27 % pero el charco se agranda
tanto que el pantano pierde perímetro y `las_dos_orillas_aparecen_donde_corresponde`
se cae. Y el cliente lleva una red: `autotile_shape` ya no cae al interior sino a
la esquina que enseñe orilla por los dos lados que la tienen.

### NINGUNA PARED CON LOS PIES EN EL AGUA (9 ago 2026)

Regla del usuario, textual: *"siempre tiene que haber un borde donde termina la
pared, un borde más de bloques de llano, que es el borde que da de llano con
agua — eso no lo estás haciendo, estás dejando acantilado flotando sobre el agua
e incoherente"*.

La regla vieja (`ningun_acantilado_toca_el_agua`) miraba los **cuatro vecinos**, y
eso alcanzaba mientras la pared se dibujara dentro de su celda. No se dibuja ahí:
la cara que mira al sur **cuelga tres filas** sobre la terraza de abajo. Con el
río a dos tiles, la roca salía pintada encima del agua.

`relief::SUELO_AL_PIE = 3` (dos filas que tapa la roca + la del llano que se ve).
El líquido viene de dos sitios y por eso hay dos mecanismos:

| líquido | por qué | qué hace |
|---|---|---|
| mar y lagos (nivel < 0) | no se pueden secar: sería tierra flotando | **se retira la pared** (`relief::relief_cell`) |
| río y charco de pantano (nivel ≥ 0) | corren sobre tierra | **se seca el pie** (`relief::bajo_el_dibujo_de_una_pared` → `geography::natural_tile`) |

**La garganta de una cascada está exenta**: ahí el cauce cruza el escalón y el
agua tiene que pasar. Y ahí está la trampa que costó una vuelta: para saber si
está exenta hay que preguntarle a `agua::en_la_garganta`, que para responder
mira si hay agua… que es lo que estamos decidiendo. Se corta con
`agua::cauce()`: un salto pregunta por el **cauce** (nivel bajo cero o
`geography::hay_rio`), no por el tile pintado. El tile pintado sólo lo mira la
decoración, que ya no está en el ciclo.

Coste medido: la suite de `ha-shared` pasó de 204 s a 209 s. Guardianes:
`agua::ninguna_pared_se_dibuja_sobre_el_agua` y
`agua::el_rio_sigue_corriendo_despues_de_secar_los_pies` (secar no puede dejar el
mundo sin ríos).

**Y el borde que pidió, verificado** (9 ago 2026). Se volvió a preguntar por esta
regla en su forma completa —*"mínimo uno o dos bloques de borde: uno el que va de
agua a césped, y de ahí otro que sea sólo césped, y ese es el que toca el borde
de abajo del acantilado"*— y esta vez se midió en vez de suponer
(`medir_cascadas::pie_de_pared_contra_agua` y `::pared_junto_a_la_garganta`).
**Ya se cumple, y no hacía falta tocar nada:**

- de 2.783 caras que cuelgan al sur, **ninguna** tiene el agua pegada al pie;
- las 75 que tienen líquido cerca dejan exactamente **una fila de césped puro**
  entre el pie de la roca y la celda de agua;
- y esa celda de agua es la que dibuja la transición: el autotile de la orilla
  vive **en la celda mojada** (`tiles/water` `edge_*` trae el verde, el barro y
  el agua en el mismo tile), no en la de tierra. O sea que las dos celdas que se
  pidieron están ahí: el césped puro contra la roca y la orilla contra el agua.
- la exención de la garganta tampoco se escapa: de las paredes que flanquean una
  cascada, **0** tienen el pie sobre el agua.

Vale la pena dejarlo escrito porque la cuenta no es obvia: `SUELO_AL_PIE = 3` da
dos filas tapadas por la roca **+ una de césped visible**, y la orilla no gasta
una cuarta porque se dibuja del lado del agua.

### El acantilado es una PARED, no una transición (`scripts/import-acantilado.py`)

Primer intento: ponerle al relieve el autotile de terreno del pack (pasto sobre
llano). Arregló el color y no la lectura — una transición de terreno es plana, y
lo que se pidió es meseta arriba, roca cayendo y falda al pie.

**Ese kit existe y es UNO SOLO en todo Modern Exteriors**: el `Sand_Mountain` del
tema 21. Se buscó en serio antes de darlo por incompleto — los 6.142 recortes
sueltos de los 24 temas por familia, los 26 autotiles, la lámina de referencia
comparada contra cada hoja, y un barrido de TODOS los PNG del pack buscando la
paleta exacta de la pared. Salieron 76 archivos y son todos el mismo kit (que el
pack además duplica dentro de la hoja de Terrains). Las otras familias de relieve
son `Mound_1/2` (montículos de un tile) y `Wall_1/2/3` (muros de ladrillo de
jardín).

#### Las tres filas que hay que acertar

La meseta chica (`Base_Single_Small_Sand_Mountain`, 3×5 celdas) tiene **dos
bordes distintos**, y confundirlos fue el bug de las capturas del 9 ago 2026:

```
  fila 0   CRESTA: el canto LEJANO, el que se ve mirando al norte
  fila 1   meseta limpia, con el filo lateral en las columnas 0 y 2
  fila 2   meseta + el labio de la pared asomando      ← BORDE SUPERIOR
  fila 3   la PARED de cantos rodados                  ← CUERPO
  fila 4   la pared + la falda al pie                  ← BORDE INFERIOR
```

La versión vieja armaba las caras que miran al SUR con `fila 0 + filas 3-4`: le
pegaba la cresta lejana encima de la pared. En el juego salía como **una franja
marrón oscura flotando sobre la meseta**, y como el teñido a pasto sólo alcanzaba
a la arena (que en esa columna era una tira de cuatro píxeles), el acantilado de
pasto y el de arena se veían **iguales**. Ahora cada cara pide su fila:

| mira al | filas | alto |
|---|---|---|
| SUR (la única que se ve entera en 3/4) | 2, 3, 4 — labio + cuerpo + pie | 3 celdas |
| NORTE | 0 — el canto de enfrente, sin pared | 1 celda |
| ESTE / OESTE | 1 + el costado modular colgando | 3 celdas |

El bordillo (`ledge/*`) es el mismo dibujo sin el cuerpo: filas 2 y 4.

#### El cuerpo de un muro alto NO es una sola baldosa (14 ago 2026)

`SALTO_DE_TERRAZA` llega a diez, así que `world2d::pared_apilada` apila hasta
nueve filas de `cuerpo`. Con **una sola celda** (la `fila 3` de la columna 1) eso
no se lee como roca: se lee como una **cuadrícula de gofre**, la misma piedra
repetida nueve veces por el ancho de la corrida. Es el *"se lee como relleno"* de
las capturas.

Estaba escrito que el pack no daba para más —*"una sola variante de cuerpo por
familia"*— y medido es falso. `Middle_Modular_Single_Small_Sand_Mountain` es la
fila que **el kit dibujó justamente para repetirse**, es 3×1 de roca pura y sus
tres celdas son tres dibujos distintos; la `Vers_2` (con musgo) da otras tres. Esa
hoja ya estaba abierta en el importador, pero sólo como paleta de referencia para
saber qué colores NO se tiñen.

Hoy salen como `cuerpo_2`/`cuerpo_3`/`cuerpo_4` y el muro reparte entre las cuatro
**por celda** —columna y fila, no sólo fila, o una corrida larga sigue enseñando
una banda horizontal idéntica—. Dos guardianes: `import-acantilado::cuerpos_distintos`
se planta si el pack colapsa las variantes (mínimo 64 píxeles de 1.024 de
diferencia entre pares) y `art2d::el_cuerpo_de_una_pared_tiene_variantes` se planta
si un `cliff/*` vuelve a quedar con una sola.

**Lo que esto NO arregla**, y hay que saberlo antes de pedirle más al pack: el kit
nunca dibuja más de **tres filas de roca**, y su montaña alta
(`Example_Single_Big_Sand_Mountain`) no es un slab vertical sino una silueta que
se ENSANCHA al bajar, con hombros diagonales. Un farallón de nueve filas no va a
leerse como la montaña del pack por muchas variantes que tenga: eso ya no es
recorte, es geometría, y la decisión (bajar el techo o componer el farallón como
mesetas apiladas) es del usuario.

#### El perímetro se cierra con el ENLACE del pack (9 ago 2026)

Tapar las esquinas cóncavas con el canto RECTO cerró el agujero y dejó el borde
hecho de trozos que no empalman. Volcado con `medir_acantilados::piezas_del_muro`,
un borde de meseta baja en diagonal y alterna `M?w9W?w?` — pared de tres celdas,
canto de una, pared, canto: en pantalla, un gusano de roca partido.

El enlace estaba dibujado y sin usar: los cuatro `Top_Small_Sand_Mountain_Big_*`
(2×1 celdas) traen la roca **curvándose** alrededor de la esquina. De cada hoja
sirve una sola celda —la de la curva— y cuál cambia según la hoja
(`import-acantilado.py::CANTO_CURVO`). El canto recto queda de respaldo.

#### Cada paso sale de SU kit (9 ago 2026)

El pack dibujó cuatro escalinatas, una por dirección, y entraba sólo la del sur:
un paso que bajaba al este se dibujaba con los escalones **de frente**, o sea
vistos desde una cara que en 3/4 se ve de canto.

| baja al | hoja | qué se corta |
|---|---|---|
| SUR | `Four_Sand_Mountains_1_Bottom` | columnas 1-2, filas 1-3 |
| NORTE | `Four_Sand_Mountains_1_Top` | la muesca vista desde el aire, 1 celda |
| OESTE | `Four_Sand_Mountains_1_Left` | columna 1, filas 1-3 |
| ESTE | `Four_Sand_Mountains_1_Right` | columna 2, filas 1-3 |

El argumento para no meter los de lado era que son "una composición diagonal de
4×4 que no se puede cortar por celda". Lo es la hoja entera; sus columnas del
MEDIO no — son tres celdas repetibles (roca, rayas, filo). Las 0 y 3 sí son
puntas de la corrida y por eso siguen fuera.

**Y el relieve mide por POSE, no por slot** (`spawn_chunk_tile_art`): sus formas
no miden todas lo mismo, y preguntando por el slot salían todas del alto de la
primera — el canto lejano se dibujaba estirado sobre la meseta.

#### Cinco materiales, y el musgo es del pack

`arena` sale sin teñir. `pasto` usa la **`Vers_2` del kit**, que es la misma pared
con musgo entre las piedras: arte de LimeZu, no un tinte, y es lo que hace que un
acantilado de bosque no se vea como uno de playa repintado. `nieve`, `regolito` y
`basalto` tiñen **también la roca** (en la Luna la pared no es roca terrestre);
en la Tierra la roca no se toca — es roca en el prado y en el médano.

Qué es arena y qué es roca lo decide el **tramo repetible**, que es pared pura sin
un píxel de arena: todo color del kit que no esté ahí es suelo. Restando en cambio
las FILAS de pared —lo que se hacía antes— el filo naranja quedaba fuera del
teñido y el acantilado de pasto salía **con el contorno de arena de la playa**.

#### El material sale del SUELO, no del bioma

Regla del usuario: *"donde haya pasto el acantilado de pasto y con arena el de
arena"*. Se rompía por tener **dos tablas paralelas** —bioma→material para la
pared y bioma→tile para el piso— que discrepaban en cuanto un bioma no era del
color de su nombre. La sabana es el caso: `biome_tile(Savanna)` da `Grass` y la
tabla vieja daba `"arena"`, así que media pradera salía coronada por un médano.
Hoy hay una sola fuente, `world2d::suelo_del_relieve`.

Esa misma función arregló las **manchas grises salpicadas por cada desnivel**:
`Tile::Cliff` y `Tile::Slope` son GEOMETRÍA, no material, pero su color de tile es
piedra y `cliff/roca` no tiene losa, así que la malla del chunk pintaba un
cuadrado gris pardo por cada celda de borde — celdas por las que se camina. El
piso de una celda de relieve es el que le tocaría por bioma si el relieve no
existiera.

**Y una pared crece hacia arriba mientras que un acantilado CUELGA.** Son el
mismo truco al revés y la diferencia está en dónde vive la celda dentro del
dibujo: en una pared la celda es el pie, en un acantilado es el canto. Dibujado
como pared —que es lo que hacía `spawn_chunk_tile_art` con todo lo `upright()`—
la roca subía tapando la meseta que la sostiene. El Y-sort no cambia, sigue
siendo el de su celda, y eso es lo que hace que el que camina por la terraza de
abajo pase POR DELANTE de la pared en vez de meterse dentro.

#### Nada de apilar tramos: la pared mide un escalón

Medido antes de construir la maquinaria (Tierra, 800×800 tiles): las **1.276
caras que miran al sur bajan exactamente UN escalón**, ninguna dos, y el 96 %
tiene 11 celdas o más de terraza libre debajo. Tres celdas de alto es lo que MIDE
el desnivel (`STEP_M` = 2,5 m sobre tiles de 1 m); una pared más alta taparía
terreno por el que se camina. Si algún día hace falta un muro de dos escalones,
lo que hay que tocar es la pendiente del relieve, no el arte — misma lección que
las cascadas apiladas.

### El paso se cruza, no se enhebra (rampas del kit)

El kit trae la escalinata entera (`Four_Sand_Mountains_1_Bottom`, 4×4): columna 0
= la pared con el espolón izquierdo, columnas 1-2 = las rayas por las que se
sube, columna 3 = la pared con el espolón derecho. Los espolones son **PARED**, no
paso: viven en `cliff/*` como poses `rampa_o` / `rampa_e` y los pide
`world2d::mejilla_de_rampa`. Sin ellos el paso se abría como un agujero cuadrado
y las dos caras se quedaban mirándose. Es la regla 8 aplicada al relieve — la
escalinata es un kit, no una losa suelta.

Para que entrara hubo que **ensanchar los pasos**. Medido: de 520 pasos, **465
medían UNA celda**. Un hueco de un tile en una pared es una aguja — se lee como
un error de dibujo antes que como un camino. Ensanchar la franja de ruido
(`PASS_HALF`) habría hecho pasos más anchos Y más pasos, o sea menos pared; en su
lugar se DILATA el ancho (`relief::PASS_DILATE`): misma cantidad de pasos, en los
mismos sitios, con tres celdas de luz. Resultado: 167 pasos, mediana de 6 celdas.

Los pasos que bajan al norte son la muesca vista desde el aire (`1_Top`, una
celda). Los de este/oeste del kit son una composición diagonal de 4×4 que no se
puede cortar por celda sin saber en qué parte de la corrida estás: ésos se dibujan
con el cuerpo rayado, que se lee igual de bien como "por acá se sube".

### El acantilado de ciudad: escaleras, caños y rejas

Regla del usuario: *"los tubos y escaleras en un contexto más cercano a la ciudad,
para que tengan sentido; para bosques y naturaleza los acantilados normales"*. La
misma pared cuenta dos cosas según dónde esté, y lo que la cuenta es lo que le
colgaron. Los siembra `world2d::spawn_chunk_cliff_urbano` con
`cliff/{escalera,tubo,reja}` (escaleras de jardín, caños de condominio, rejas y
tapas de alcantarilla del pack), una de cada seis celdas de cara sur.

**"Cerca de gente" es DENTRO DEL CASCO** (`settlement_covering`), y se pregunta
por CELDA, no por chunk: una ladera cae casi siempre en el BORDE del pueblo, y
mirando sólo el centro del chunk la mitad de las laderas urbanas quedaban peladas.
Sale caro por celda y no importa — para llegar a esa pregunta la celda tiene que
ser cara sur y haber ganado el sorteo, o sea menos de una consulta por chunk.

La pieza **se apoya en el pie y asoma lo que le sobre**. Recortarla al alto exacto
del desnivel dejaba, en un bordillo —que es casi todo el relieve urbano, porque
las ciudades se fundan en llano—, dos peldaños sueltos que no se leían como nada.
Una escalera apoyada contra un muro bajo asoma por encima del canto: eso es lo que
hace una escalera de verdad.

### La rampa del relieve (`scripts/import-relieve.py`)

La RAMPA no es una pared: es el paso, y por el paso hay tierra pisada. Ese pixel
sale de los autotiles de terreno del pack
(`Autotiles_32x32/Game_Maker_Studio_Autotiles_Singles_32x32`, 26 hojas de 12×4
celdas), tomando de cada familia su celda de **fondo puro** — el terreno sin
pasto encima:

| hoja | fondo medido | slot |
|---|---|---|
| `..._1` **el Llano** | (170,143,90) tierra apagada | `slope/pasto` |
| `..._2` **la Arena** | (202,136,84) arena naranja | `slope/arena` |

El número de hoja no dice nada (son archivos numerados), así que cada familia
declara el color que su celda de fondo tiene que medir: si LimeZu reordena el
pack, el script se planta en vez de importar la cancha de tenis como si fuera el
llano. La maquinaria que puntúa las trece formas de autotile sigue ahí porque es
la que VERIFICA que la hoja sea de transición — una hoja lisa no llega al mínimo
y falla.

`cliff/roca` **sigue con el respaldo por código** y ya no lo pide nadie:
`material_de_suelo` no devuelve "roca" nunca. `nieve`, `regolito` y `basalto` sí
tienen pixel desde el 9 ago 2026 (teñidos del mismo kit, roca incluida). Y en la
Tierra la sierra y la montaña son "pasto", porque
`geography::natural_tile` le sacó a `Biome::Mountain` el suelo `Tile::Rock`
(quedaba una cordillera empedrada de gris hasta el horizonte). **La superficie
viva del mundo es pasto**; la altura se lee por el relieve.

> Decía "la cumbre incluida", y desde el 14 ago 2026 ya no: por encima de
> `geography::NIVEL_NIEVE = 9` la superficie es NIEVE (§"La cumbre es blanca"). La
> regla no cambió de fondo —la altura sigue leyéndose por el relieve y no pintando
> de gris la cordillera entera—; lo que se agregó es el techo del mundo, que son
> unas pocas décimas de por ciento de la superficie y sólo el remate de los cuatro
> macizos con nombre.

### EL ANILLO DE LA MESETA: el sprite no llevaba el gris de su celda (21 ago 2026)

Reporte del usuario, con capturas del celu: *"todos los bordes se ven con una
textura y el relleno se ve con otra; es como si las mesetas no tuvieran su
terreno, como si se usaran sólo como margen y el terreno de relleno fuera el
mismo del de abajo"*.

**No era el material: era el TINTE.** La pieza de acantilado del pack **trae su
meseta dibujada** (filas 0-2 del recorte, ver `scripts/import-acantilado.py`), y
el importador la tiñe al color **crudo** de la losa que la corona. Pero la malla
del chunk no pinta la losa cruda: la multiplica por

```
gris = grano × void_dim × (1 + nivel × 4,5 %) × sombra_de_terraza × sombra_de_muro
```

o sea hasta **×1,54** en la cumbre y hasta ×0,66 al pie de un muro. El sprite se
spawneaba **sin `color`**, así que en el escalón 9 el relleno salía un **40 % más
claro** que la franja de meseta del sprite pegado al lado: dos verdes distintos
para el mismo pasto, con la frontera calcada al canto de la terraza. Eso es el
anillo, y por eso la meseta se leía como un margen dibujado encima del suelo de
abajo.

El arreglo es una sola fuente, `world2d::gris_de_celda`, que aplican **todos** los
que tapan una celda que la malla ya pintó: el acantilado suelto, el bordillo, la
rampa, el muro apilado, el canto cóncavo, la orilla y lo que la ciudad le cuelga
a la ladera (escalera, caño, reja). Tres detalles que costaron:

- **`atenuar_sprite` cortaba en `k < 1.0`** (existía para el `VOID_DIM` del
  pozo), así que aclarar era imposible — y el gris de una cumbre vale 1,4. De ahí
  sale `tenir_sprite`, que también aclara (corta sólo en `k == 1.0`, que es el
  caso de todo sprite que no es relieve). Pasarse de 1 no rompe nada: la malla ya
  lo hace y el recorte a blanco lo hace el shader, igual para los dos.
- **El muro apilado se tiñe por fila**: el `labio` con el gris del CANTO (arriba
  es donde meseta toca meseta) y el `pie` con el de la celda que pisa (abajo la
  terraza baja está varios escalones más oscura). Dos consultas por pared, no una
  por fila; el espolón del paso va con el mismo gris.
- **El gris del pie va sin la sombra de columna**: esa franja oscura la tira
  ESTE muro, y un muro no se sombrea a sí mismo.

Lo cuida `world2d::gris_tests::el_muro_empieza_en_el_gris_del_canto_y_termina_en_el_del_pie`.

### UNA SOLA FUENTE DE SUPERFICIE — `geography::superficie_de` (21 ago 2026)

El anillo tenía una segunda mitad, y es la clase de bug que ya mordió tres veces
(la sabana coronada de médanos, el acantilado con el pie mojado, el agua sin
familia de ribera): **dos respuestas para la misma pregunta y nadie obligando a
que coincidan.**

| pregunta | fuente 1 | fuente 2 |
|---|---|---|
| ¿de qué material es esta celda? | `biome_de_terraza` (liso) para la pared | `biome_at` (difuminado) para la losa |

En la franja del ajedrez de `pasa_el_escalon` eso es una pared de sierra plantada
sobre una losa de pradera, y la misma corrida de canto alternando de variante
celda a celda.

Ahora hay **una**: `geography::superficie_de(body, tx, tz) -> Superficie { tile,
pasto }`, con caché posicional y época de partida en la clave (la misma tabla
directa que usa `relief`, sin hash, para que un recorrido por filas no se pise a
sí mismo). La regla que la hace correcta:

> **El difuminado se suspende sobre el relieve.** Un canto, el cuerpo de su pared
> y su rampa son UNA pieza de kit: si la trama del ajedrez entra ahí, la pieza
> sale a rayas. Y como la malla pinta esa misma celda con esta misma función,
> borde y relleno no pueden discrepar — no por disciplina, por construcción.

Reemplaza como entrada del juego a `world2d::suelo_del_relieve` (que queda como
envoltorio de una línea), a `world2d::pasto_de_bioma` y a
`geography::biome_de_terraza` — esta última sigue pública **sólo para las
mediciones**, que necesitan comparar el bioma liso con el difuminado. De paso, el
coste BAJA: una celda de canto evaluaba el bioma tres veces y ahora una.

Y dos cosas que se aprendieron al hacerlo:

- **El pasto es del CLIMA, no del tile.** Atarlo al tile dejaba sin verde propio a
  la orilla de todo el pantano: un charco no pinta pasto, pero la orla que lo
  separa del prado sí lleva el verde del pantano.
- **`Biome::River` sólo en el camino difuminado.** Una celda de acantilado con río
  daba River → `Tile::Water` → pasto forzado → material `"pasto"`, o sea
  **acantilados con el kit de pasto en plena zona de nieve**, que es lo que se ve
  alrededor del cauce en la captura de la sierra. El río es un trazado sobre el
  terreno; de qué está hecha la ladera que cruza lo dice la ladera.

Guardián: `geography::sobre_relieve_manda_la_terraza`, que además **exige que la
muestra contenga casos donde las dos respuestas difieren** — sin eso la
afirmación se cumpliría sola y no guardaría nada.

### LA COSTURA ENTRE DOS PASTOS (21 ago 2026)

`spawn_chunk_orilla` sólo conocía tres materiales —tierra, arena, nieve— y su
`same()` comparaba `Tile`. A los dos lados de la frontera sierra/pradera el tile
es `Tile::Grass`, así que **nadie dibujaba nada**, aunque las losas sean
(106,148,88) contra (71,151,87): dos verdes claramente distintos cortados a
escuadra, siguiendo el borde de la terraza. La otra mitad de "la meseta parece un
margen".

La pieza no hubo que dibujarla: es **el mismo autotile `Grass_3` del pack** con el
lóbulo de material teñido a otro VERDE en vez de a un marrón. `import-suelo-bordes.py`
ya separaba los dos lóbulos (material y pasto) y los teñía por caminos distintos,
así que fueron cuatro líneas de tabla. Medido en la hoja publicada:
`borde/pasto_sierra` interior = (106,148,88), **exactamente** `tiles/pasto_sierra`,
y su fleco (71,151,87), exactamente `tiles/grass`.

Dos decisiones que el importador MIDE en vez de suponer:

- **El grano se comprime ×0,53**, que es el cociente de los rangos de luminancia
  de los dos lóbulos del kit. El lóbulo de material es tierra con piedritas y el
  pasto es mucho más liso: teñido tal cual, la costura entre dos pastos salía
  empedrada. El número lo imprime cada corrida; si el pack cambia, cambia con él.
- **Una costura, un solo dueño** (`world2d::rango_de_pasto`): la dibuja el pasto
  de mayor rango y su fleco se tiñe del de menor. Sin un orden total los dos lados
  dibujarían su fleco, uno encima del otro — dos dibujos para el mismo metro de
  mundo. El orden es pradera < bosque < sabana < pantano < sierra, y el importador
  sólo genera las combinaciones en ese sentido (10 claves nuevas).

Y por eso mismo **las gradas 5 y 8 dejaron de ir con ajedrez**
(`geography::pasa_la_grada`): una orla necesita una FRONTERA —una línea con un
adentro y un afuera—, no una nube de celdas sorteadas. El damero se queda en la
línea de nieve, que es blanco contra verde y ahí sí se lee como difuminado; entre
dos verdes parecidos se leía como suciedad. El comentario que justificaba lo
contrario —*"sierra y montaña pintan el mismo material, su frontera no se ve"*—
llevaba desactualizado desde que `pasto_sierra` se volvió losa propia.

### EL PASO SE COLOCA ENTERO, Y LA ROCA DEL PASTO ES PIEDRA (21 ago 2026)

Dos cosas del mismo reporte, mirando la lámina de ejemplo del pack:
*"usar las rampas completas de forma modular, no sé por qué recortás sólo un
pedazo y usás eso de rampa"* y *"en morado los assets que unen acantilados y
rampas"*.

#### La roca del pasto era mostaza

Y ésta era la de fondo. El kit de acantilado del pack es una **montaña de
arena**, y a la familia "pasto" nunca se le dio rampa de roca —nieve, regolito y
basalto sí la tienen—, así que su pared se quedaba con el marrón rosado del
médano. Sobre verde eso no se lee como piedra: se lee como una mancha
**mostaza**, y es literalmente el *"el relleno se ve con otra textura"* que el
usuario venía marcando desde las capturas del 16 ago. Puestas al lado,
`cliff/nieve` sale roca limpia y `cliff/pasto` sale mostaza.

Los cuatro tonos de [`ROCA_DE_PASTO`] **no son un gris inventado**: son colores
del núcleo de 501 del pack, elegidos en las **mismas cuatro luminancias que la
piedra del kit** —63, 93, 129 y 163, medidas sobre el tramo repetible— así que el
canto rodado conserva su volumen y sólo cambia de tono. Y el más oscuro va
**tintado**, nunca gris neutro: es la regla medida del pack (246 de 250 sprites,
skill `pixel-art`).

#### El kit del paso, importado y colocado

`paso/<material>`, 12 piezas por material: boca, tramo repetible ×2, pie, los
pasos que bajan al este y al oeste, y **los cuatro enlaces diagonales** —las
piezas que el usuario marcó en morado, las que unen el paso con la pared—.

Tres decisiones que costaron su medición:

- **Hoja propia** (`art/pack/paso.png`, 1.024×1.120). Metidos en el atlas del
  acantilado, los kits de 4×4 entre piezas de 1×1 lo inflaban de 6,75 a **15,75
  MB de VRAM**, casi todo aire de empaquetado. Aparte, y empaquetando por altura,
  la hoja mide exactamente las 1.080 celdas que dibuja. El atlas del acantilado
  quedó byte por byte igual.
- **`_1_Top` y `_1_Bottom` no se publican**: son la familia modular ya compuesta.
  Medido buscando cada recorte dentro del plato de ejemplo, `1_Bottom` y
  `Central_Bottom_Part` arrancan en el mismo píxel.
- **Los peldaños conservan su tierra.** Sus tonos son **exactamente los catorce de
  la meseta, cero exclusivos** (medido), así que separarlos por color es
  imposible: se hace por la ventana de píxeles que el importador ya tenía medida.
  Teñidos quedaban verde sobre verde, que es la queja vieja *"odio cómo usás las
  rampas"*.

Del lado del juego, **todavía no se coloca**, y el intento está contado abajo
porque el resultado importa: se escribió `spawn_chunk_paso` —una pieza por paso,
centrada en la corrida, callando la rampa embaldosada y la pared bajo sus
espolones— y **la captura del driver mostró el terreno destrozado**: paredes
partidas en fragmentos sueltos y claros donde antes había muro. La supresión se
comía celdas de acantilado que ningún kit alcanzaba a cubrir, porque el kit se
apoya donde puede y el canto zigzaguea. Se revirtió entero.

#### Lo que NO se pudo, y está medido

El kit necesita **cuatro celdas de canto seguidas al mismo nivel**. Medido:

| | |
|---|---|
| cantos de 1 celda recta | **86,2 %** de todos los del mundo |
| cantos de 4 o más | 4,4 % |
| pasos de 1 celda de ancho | 88,9 % |

Los bordes de terraza **son escaleras diagonales, no líneas**. Se probaron dos
salidas y las dos se revirtieron con su número:

1. **Ensanchar el paso** a lo largo del canto: 88,9 % → 89,4 % de pasos de una
   celda, con un 22 % más de celdas de paso a cambio de nada.
2. **Tallar una muesca** en el campo de altura (aplanar bloques de 4×4 en la
   franja de paso): los cantos rectos pasaron de 4,4 % a 5,6 % preguntando por el
   ancla, y **empeoraron a 2,8 %** preguntando por la celda.

Por eso el kit se coloca **como sprite**, colgado sobre las celdas igual que la
pared (`pared_apilada`): que el borde zigzaguee cambia cuánto se CAMINA, no cómo
se dibuja. Enderezar el canto de verdad es trabajo de generación y queda en §13.

### EL RÍO SE TALLA EN ESCALONES, NO EN DECIMALES (21 ago 2026)

Segunda captura del usuario: en la sierra, *"los ríos y los acantilados se pelean
el terreno en vez de distribuirlo"* — farallones en zig-zag cortando el cauce,
cascadas sueltas y orlas secas marrones alrededor del agua. Medido antes de tocar
nada (`medir_rios`, ventana de sierra de 1.200 tiles): **el 21,75 % de las celdas
de cauce eran roca**.

Tres causas, y las tres eran números que nadie había medido:

1. **`HONDURA` valía 0,030 de campo y un escalón mide 0,0467.** La talla no bajaba
   el cauce ni un nivel: sólo corría la frontera de cuantización, así que el nivel
   de la orilla parpadeaba a lo largo del curso. Ahora la hondura se pide **en
   escalones** (`HONDURA_EN_ESCALONES = 1,0`) y se convierte con el paso real del
   cuerpo.
2. **El empalme del valle pegaba un campo liso contra uno rugoso.** Era
   `base × (1−peso) + cota × peso`, y como `base` lleva el ruido dentro, en el eje
   del cauce el terreno quedaba perfectamente plano y a 3,6 tiles volvía entero:
   hasta **0,06 de campo por tile, el triple de `relief::MAX_FIELD_STEP_PER_TILE`**
   —y esa cota no es decorativa, el descarte barato de `near_step_boundary` se
   apoya en ella—. **El propio río fabricaba los farallones que después lo
   cortaban.** Ahora la plataforma se lleva a la cota y el ruido se atenúa aparte
   (`RUIDO_EN_EL_CAUCE = 0,25`), y el valle nunca mide menos de `VALLE_MINIMO = 8`
   tiles: lo que gobierna el dibujo no es la proporción, es la pendiente.
3. **`es_canto` no conocía los ríos.** Se retiraba del mar (`nivel < 0`) pero un
   río corre en nivel ≥ 0, así que plantaba canto dentro del cauce; y como
   `relief_tile` gana sobre el bioma, esa roca además SECABA hasta tres filas más
   (`bajo_el_dibujo_de_una_pared`) — las orlas marrones. Ahora se retira del cauce
   igual que del mar, **salvo en la garganta**.

> **"Cruzar" son TRES cosas, no dos.** La excepción de garganta pedía agua arriba
> y agua abajo, y con eso un río que corre de norte a sur por un llano la cumple
> SIEMPRE: la excepción se tragaba el retiro entero y el cauce seguía cortado en
> el 7,4 % de la sierra. Falta la tercera: **que el terreno baje ahí**. Un salto
> sin desnivel no es un salto.

Resultado, todo medido con `medir_rios` (mismas ventanas, antes → después):

| | antes | después |
|---|---|---|
| cauce cortado por roca (sierra) | 21,75 % | **4,12 %** |
| cauce cortado por roca (llano) | 0,52 % | **0,01 %** |
| salto de campo p99 dentro del valle | 0,0317 | **0,0199** (cota: 0,020) |
| cauce por debajo de su ribera (sierra) | 83,6 % | **96,1 %** |
| anclas de cascada en la sierra | 49 | **56** |

El 4,12 % que queda **son los cruces**, y son correctos: donde el cauce baja un
escalón hay roca a los lados, con cascada o con rápido.

#### LA CASCADA ES DE FRENTE, ASÍ QUE EL RÍO TIENE QUE BAJAR (21 ago 2026)

Reporte del usuario mirando las capturas: *"las cascadas sólo tenemos assets de
frente, tipo cuando el río viene de arriba hacia abajo, no de izquierda a derecha
horizontal; veo que pones cascadas ahí y eso está mal"*.

Tenía razón, y el número es peor de lo que parecía. `agua::salto_desde` pedía tres
cosas —agua encima, pared que baja al sur, agua al pie— y **ninguna dice hacia
dónde corre el agua**. Las tres las cumple un río que pasa de este a oeste con su
ribera sur cayendo. Medido con `medir_rios::rumbo_del_cauce_en_los_saltos` sobre
las tres ventanas de cordillera:

| | (−6.100,−6.100) | (−7.000,−6.100) | (−4.300,−6.100) |
|---|---|---|---|
| candidatos a salto | 2.408 | 1.489 | 1.185 |
| con el cauce hacia el **norte** | **1.998** | 1.067 | 575 |
| hacia el sur | 410 | 422 | 610 |
| dentro de un cono de 45° del sur | 2 | 0 | 0 |

O sea que **la mayoría de las cascadas del mundo caían contra la corriente**.

El arreglo es preguntarle el rumbo al TRAZADO, que es quien lo sabe:
`rios::rumbo_del_cauce` devuelve el vector unitario del tramo más cercano (el
sentido sale gratis, los tramos se generan aguas abajo). `salto_desde` exige
`dz > 0`.

**Y no un cono, el SENTIDO.** Con 45° sobrevivían dos cascadas en todo el macizo:
el trazado avanza a zancadas de 48 tiles con meandro, así que casi ningún tramo es
verticalísimo. Exigir el cono deja el mundo sin saltos; exigir el sentido saca los
que estaban al revés. Anclas por ventana: 68 → **15**, 39 → **26**, 23 → **20**.

**La laguna está exenta**: `cauce` también cuenta el agua quieta, y ahí no hay
trazado que preguntar (`rumbo_del_cauce` devuelve `None`). Un vertedero de laguna
cayendo de frente está bien dibujado igual.

#### El llano se quedó sin cascadas, y está bien

`agua::hay_cascadas_donde_el_rio_cruza_un_escalon` barría 1.800×1.800 tiles
centrados en el spawn —que es llano— y empezó a fallar. Los saltos que encontraba
ahí **eran el defecto**: el cauce cruzaba una frontera de cuantización cada
treinta tiles porque el valle pegaba un campo liso contra uno rugoso, y la pradera
tenía cascadas de diez pisos en mitad del prado. Un salto es de montaña. El
guardián ahora mira tres ventanas de cordillera **medidas** con
`medir_rios::donde_estan_las_cascadas` —(−6.100,−6.100), (−7.000,−6.100),
(−4.300,−6.100)— y no "la primera terraza ≥ 5", que cae en cualquier loma alta y
una loma no tiene río: para que haya salto hacen falta las dos cosas juntas.

#### Y la ciudad desmintió un número

Se probó con `HONDURA_EN_ESCALONES = 1,15` —la idea era que el redondeo no dejara
tramos empatados con su orilla— y lo tiró abajo `city::la_ciudad_no_esta_llena_de_baldios`:
con esa hondura la ribera mete un borde de terraza más en el casco de Ciudad
Central, y `lot_at` no planta parcelas a caballo de un escalón. **135 de 218
manzanas**, por debajo del dos de cada tres que separa una ciudad de un
descampado. Con 1,0 pasan las dos cosas — y el río sale mejor en las tres
mediciones. Es la regla de siempre: *calibrar midiendo, no suponiendo*.

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

#### El presupuesto de gente es LOCAL, no global (3 ago 2026)

`MAX_NPCS = 48` son **cerebros**, no habitantes. Antes se repartían por orden de
llegada y no se soltaban nunca, y eso producía tres fallas encadenadas —las tres
las cazó el invariante `ciudad poblada` de `HA_ECOTEST`, que daba 0 vecinos parado
en el medio de una ciudad:

1. **La ciudad quedaba desierta para siempre.** El primer pueblo que pisabas se
   quedaba los 48; al llegar después a una ciudad a 2 km, `discover_settlements`
   veía `alive >= MAX_NPCS` y cortaba antes de sembrar.
2. **Se perdía el pueblo.** Al ENTRAR a un cuerpo se sacaban todos sus registros de
   `soc.records` y se spawneaban hasta agotar los cerebros; el `break` descartaba el
   resto, que ya no estaba en ningún lado. Un guardado real se derritió de 90 NPCs a
   42 así.
3. **Al SALIR se barrían los virtualizados**, con un `retain(|r| r.body != old)` que
   valía sólo mientras "en el cuerpo activo todos están vivos".

Ahora hay **streaming de gente** (`stream_npcs`): quien queda a más de `VIRT_OUT_R`
vuelve a ser `NpcRecord` —sigue su rutina en `advance_far`, sin entidad ni cerebro— y
se materializa de nuevo bajo `VIRT_IN_R`. Con histéresis, para que caminar sobre el
borde no haga spawn/despawn cada frame.

**Invariante nuevo, y de él cuelga todo:** si alguien está en `soc.records`, NO tiene
entidad. Vale para todos los cuerpos, incluido el activo. Por eso
`tick_society_remote` ya no saltea el cuerpo activo, y por eso el guardado ya no
puede tirar registros suyos.

⚠ Nadie desaparece nunca: deja de estar renderizado, que es otra cosa. Si en el
futuro algo poda `records`, tiene que ser por una razón de MUNDO (murió), jamás por
presupuesto de cerebros.

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

### La interfaz de Ki — el kit del pack y las imágenes de referencia

**La interfaz entera se rehizo el 15-16 ago 2026** siguiendo la propuesta que el
usuario aprobó (*Interfaz de Ki*). La consigna fue textual: *"quiero que el juego
se vea tal cual en las imágenes de referencia"*, *"no quiero variaciones"*.

**Las imágenes viven en el repo**: `scripts/maqueta/` — los PNG/GIF aprobados y
los scripts de Python que los compusieron. Ese directorio es la verdad de cómo
tiene que verse cada pantalla; ver su `LEEME.md` para los comandos de captura.

#### Las tres piezas del sistema

1. **`scripts/import-ui-kit.py`** recorta del pack (Modern UI Style 1) los ocho
   marcos de 9 partes y los botones-icono, y los publica como `ui/marco_*` y
   `ui/ic_*`. Los recortes NO están alineados a la grilla de 32 —el panel empieza
   en el pixel 116— así que la página va con `grid: [1,1]` (rects en píxeles) y
   cada marco declara su **borde** (`slice`), que es lo que `art2d::ui_frame_node`
   necesita para estirar el centro sin deformar las esquinas. El script verifica
   el SHA-256 de la hoja del pack: si el pack cambia, se planta.
2. **`ha-game/src/ui_pack.rs`** es el gemelo en Bevy de `scripts/maqueta/uikit.py`:
   marcos, cartel, botón, texto anclado, iconos, la barra de estado, el muñeco por
   capas, la viñeta y el halo.
3. **Las coordenadas son las del mockup.** Todo se dibuja en un lienzo de
   **2400 × 1080** —el Redmi apaisado— y un solo factor `ui_pack::k = ancho / 2400`
   lo escala. El lienzo va **anclado arriba**: centrado dejaba una franja vacía
   entre el borde de la pantalla y la barra de estado.

> Si algo no entra, el número está mal en la maqueta y se cambia en la maqueta.
> Acá no se corrigen números a ojo.

#### Trampas que ya costaron una vuelta

- **`hud::skin_ui_from_art` le pone `ui/button` a todo botón sin imagen.** Los
  botones del kit ya traen su marco; el reskin se los pisaba y todo salía con el
  interior marrón oscuro de `ui/button` (107,80,82). Ahora se saltea al que ya
  tiene `ImageNode`, y un área táctil invisible se marca con `ui_pack::SinMarco`
  (si no, el velo de un modal sale como una tabla de madera a pantalla completa).
- **Las grillas van con `Display::Grid`, no con `flex_wrap`.** 7 × 130 + 6 × 12 da
  982 clavados: basta que taffy redondee un pixel para que la séptima casilla se
  caiga a la fila de abajo.
- **El texto va con `ZIndex` por encima del marco.** Los rótulos chiquitos
  ("NOMBRE", "SEMILLA") caen justo arriba de su campo, que se dibuja después.
- **La miniatura de una casilla del creador necesita `CcThumb` en la casilla y
  `CcThumbSlot` en un hijo** — `stream_thumbs` los busca en padre/hijo. En el
  mismo nodo, la grilla entera sale con la misma prenda.

#### Cómo se mira sin el teléfono

```bash
cd native
HA_MENUSHOT=/tmp/menu.png            ./target/release-native/ha-game
HA_LISTASHOT=/tmp/lista              ./target/release-native/ha-game
HA_CCSHOT=/tmp/cc                    ./target/release-native/ha-game
HA_AUTOPLAY=1 HA_BAGSHOT=/tmp/juego  ./target/release-native/ha-game
```


### Cámara

- **Siempre centrada** en el jugador. El zoom **nunca** mueve al personaje.
- **UN PRODUCTO, y nada más:** `encuadre = view_h_for_alt(cuerpo, altura) ×
  user_mult`. La altura pone la escala; el pellizco (móvil) / scroll (desktop)
  pone un multiplicador **persistente** encima. `camera::desired_frame_h` es esa
  cuenta, pura y sin estado.
- ⚠ **Subir tiene que VERSE.** En vista cenital el personaje no se mueve en
  pantalla: lo único que cuenta que estás subiendo es que el suelo se aleje. El
  4 ago 2026 se probó sacarle la altura a la cámara entera para matar el tirón del
  despegue — funcionó, y de paso mató el vuelo: cruzabas la atmósfera y en pantalla
  no pasaba **nada**. No volver a hacerlo: el tirón se arregla en la CURVA, no
  desconectándola (`camera::tests::el_despegue_no_se_aleja_de_golpe` mide el ritmo
  por los dos lados, techo **y** piso).
- ⚠ **El ritmo de la subida es constante por construcción.** `climb_rate` y
  `view_h_for_alt` están atadas (`universe::CLIMB_BASE = GROUND_VIEW_H / 2`,
  `NEAR_ZOOM_RATE = 1`) para que el suelo se aleje siempre a **×1.65/s**, de pisar
  el pasto a ver el planeta entero. Con los valores viejos (30 m/s y 2.4) el primer
  segundo de vuelo se alejaba ×11 — ése era, textual, "al darle a volar la cámara se
  alejaba de golpe". Tocar una de las dos sin la otra rompe la propiedad.
- ⚠ **El empuje vertical entra con rampa** (`creative_fly::THRUST_RAMP_S`, 0.35 s):
  pasar de quieto a `climb_rate` en un frame es una discontinuidad de velocidad, y
  como la altura encuadra, se ve. Al soltar deja ~9 % de inercia, que se lee como
  cuerpo, no como bug.
- ⚠ **Nada se guarda entre frames.** El encuadre no se escribe de vuelta en el zoom
  del jugador. Guardarlo era el bug de "el botón de bajar no baja": subir
  trinqueteaba `view_h` hacia arriba y bajar ya no podía cerrarlo, así que el
  descenso era invisible hasta que al tocar suelo el clamp devolvía la cámara de
  golpe — el "de repente aparezco en el suelo". Bajar deshace subir porque `auto` es
  una función pura de la altura (`camera::tests::bajar_deshace_subir`).
- **Despegar y aterrizar son continuos por construcción:** `view_h_for_alt(b, 0) ==
  SURFACE_VIEW_H`, o sea `auto = ×1` a ras de suelo. Cruzar el límite vuelo↔pie no
  puede mover el encuadre, y el multiplicador del jugador **nunca** se resetea, así
  que el encuadre que elegiste sobrevive el despegue, el viaje y el aterrizaje
  (`camera::tests::despegar_y_aterrizar_no_dan_un_salto`).
- El personaje se dimensiona con el encuadre **vivo** (`sim.rs`), o sea que se dibuja
  a tamaño de pantalla constante: vos te quedás, el mundo se achica.
- La **velocidad de vuelo se ata a la altura, no al zoom** (hacer zoom no puede
  acelerarte).
- **Encuadre de aproximación:** volando, la cámara se adelanta hacia el cuerpo
  destino (hasta ¼ de pantalla), así el planeta crece de frente y entrás en él en
  vez de orbitar su centro fuera de cuadro.
- Pellizco a pie en la **banda superior** de la pantalla (`PinchActive` suelta los
  sticks). Un solo rango de zoom, vueles o no.

### Luz y cielo

- **Ciclo día/noche de 1 h** (25 min día · 5 atardecer · 25 noche · 5 amanecer).
  Ningún astro se dibuja como disco: en vista cenital un sol se lee como un manchón
  blanco sobre los techos. El sol existe como `SkyLight` (dirección/largo/color de
  las sombras) y su posición se lee en el mini-dial del reloj.
- **Luz 2D por asset** (`light2d.rs` + campo `emission` en el manifest): la
  oscuridad se **agujerea**, no se tapa con naranja. `night_veil.wgsl` +
  `NightVeilMaterial`; `NightOverlay` es `Mesh2d`; hasta **40** luces
  (`VEIL_LIGHTS`). Emisor con núcleo + halo; luciérnagas con `Light2d` chico;
  `light_sprites` aclara a jugador/NPCs cerca del foco.
  ⚠ `light_sprites` está **partido en dos pasadas** (pasada cara a 20 Hz +
  re-aplicación del tinte a 60 Hz) porque vive en `PostUpdate` para pisar a
  fauna/NPCs/máquinas, que repintan `sprite.color` cada frame. **No unificarlas**:
  bajarlo a 20 Hz a secas reintroduce parpadeo.
  Probar de noche con `HA_DAYPHASE=0`. El `NIGHT_VEIL` alto es a propósito.
- **Estrellas** que siguen a la cámara, siempre; nunca geometría finita cruzable.
- **Bichos de ambiente** (`ambient.rs`): luciérnagas de noche, **mariposas de día**.

#### LA NOCHE SE ENCIENDE — el motor estaba entero y no tenía a quién iluminar (28 ago 2026)

El pedido llegó con cinco capturas de un juego de festival nocturno y una sola
consigna: **que Ki se vea así, sin perder fps**. Lo que se encontró al medir no
fue un problema de shader.

**La causa: de los 14.440 sprites del manifest, CUATRO declaraban `emission`.**
`prop/farol`, `furniture/campfire`, `items/lantern` y un botón de la UI. O sea que
el pack tenía faroles, velas, fogatas, linternas de camping, letreros y torres de
reflector dibujados, el motor tenía dos pasadas por píxel con pozo, bulbo, sombra
direccional y tinte de sprites… y **el pueblo de noche estaba negro porque nadie
había dicho quién emite**. Es el caso de manual de la memoria "el pixel suele
estar; la cadena no".

##### `scripts/import-emisiones.py` — la emisión se MIDE

Nuevo importador, con el contrato de siempre (regla 7): idempotente vía
`scripts/emisiones-import-index.json`, y **nada escrito a ojo**. Por cada clave
candidata recorta el sprite de su hoja, busca el **núcleo caliente** (componentes
conexas de píxeles por encima de un piso de luminancia y del percentil alto del
propio recorte) y de ahí saca:

| dato | de dónde sale |
|---|---|
| `color` | media del núcleo **más su anillo de 2 px** — el centro de una llama es blanco, el ámbar está alrededor |
| `offset` | centroide del núcleo respecto del centro del sprite, en metros: la luz sale del vidrio del farol, no de su poste |
| `radius` | alcance base de la familia escalado por `√área` del núcleo (o por la EXTENSIÓN del conjunto, en los multifoco) |

`intensity`, `flicker` y `day` son de la FAMILIA y van declarados: no se puede
medir en un cuadro quieto si una luz late.

**Si no hay núcleo, el asset no emite**, y ése es el filtro honesto: entra
`prop/farol_de_calle_5` (36 px de vidrio blanco) y no entra `prop/fogata_1` (leña
sin encender, 0,686), ni la chimenea de ladrillo (0,504), ni el letrero pintado
(0,429). **Y no entró `prop/farol`**, el candidato obvio por nombre: medido, está
dibujado APAGADO — por eso el alumbrado público usa `farol_de_calle_5`.

El piso de luminancia quedó en **0,70 y no 0,72** por una razón de PALETA: el
color más claro y saturado del lado del rojo que tiene LimeZu es (255,160,160),
que mide 0,707. Un piso en 0,72 no dice "esto está apagado", dice "en este pack no
puede existir una luz roja".

Resultado: **102 emisores medidos** (de 4 que había), 66 de ellos ya sembrados por
las tablas del pack sin tocar una sola línea de siembra.

##### El alumbrado público y las guirnaldas (`worldgen.rs`)

Dos colocaciones nuevas, las dos **geométricas y no por sorteo**, como el semáforo
de `esquina_decor_at`:

- **`alumbrado_at`** — farol en el CORDÓN (la vereda que toca la calzada), cada
  `PASO_ALUMBRADO` = 14 tiles, con el compás corriendo a lo largo de la acera.
  Empezó en 8 y la captura de `HA_NIGHTTEST` mostró por qué está mal: con el pozo
  del farol viejo (radio 7 × `LIGHT_REACH` 1,5 = 10,5 m) se superponían diez pozos
  por celda, `cover` llegaba a cero y **la calle de medianoche se veía como un
  mediodía nublado**. Un charco sólo se lee como charco si al lado hay oscuridad.
- **`guirnalda_at`** — guirnaldas de luz en la acera de adentro, dentro del casco
  (`settlement_covering`), en una retícula de 9×7 tiles. El paso en x es mayor que
  la guirnalda más larga, así que dos nunca se pisan.

Las guirnaldas **no existen en el pack** (se buscó en el manifest en castellano y
en inglés, y en los PNG crudos: lo único que aparece es `prop/luces_calabaza_*`,
que son calabazas de Halloween con una vela adentro). Se dibujaron con la skill
`pixel-art` en `scripts/dibujar-guirnaldas.py` → `art/gen/prop/guirnaldas.png`,
cuatro vanos y cuatro colores, y su emisión la mide el mismo importador que la de
cualquier asset comprado.

##### Lo que se agregó al shader, sin pagar un pase más

Las dos pasadas de `night_veil.wgsl` ya barren la pantalla entera: todo lo que se
sume ahí adentro es ALU y cero ancho de banda.

1. **Halo** — un lóbulo ancho `(1-x)²` sumado con poca ganancia alrededor del
   bulbo. Es el "bloom" de cada luz sin mip-chain de bloom, que en una GPU de
   tiles es justo el recurso escaso.
2. **Viñeta** — la pasada de sombra YA es un multiplicador a pantalla completa.
   Va atada a `night` y vale 0 de día **a propósito**: `toggle_light_passes` apaga
   las dos pasadas a mediodía, y si la viñeta valiera algo justo ahí, las esquinas
   pegarían un salto al apagarse el pase.
3. **Bitono** frío/cálido: la sombra se enfría, el pozo no. Ámbar contra azul.

##### Y salió MÁS BARATO que antes (medido con `HA_NIGHTTEST`)

El presupuesto subió de 24 a 40 luces y el costo BAJÓ. Tres cambios:

- **`lights[i].z` ya no es el alcance sino `1/alcance²`**, y `tints[i].w` es
  `1/(radio·BULB_R)`. Rechazar una luz fuera de alcance —el 90 % de los casos, 40
  veces por píxel— pasó a costar una resta, un `dot`, una multiplicación y una
  comparación: **ni raíz cuadrada ni división**. La raíz sólo se paga por las
  luces que de verdad tocan el píxel.
- El mismo rechazo al cuadrado en los dos bucles de CPU (`light_sprites` y
  `cast_shadows`), que son bucles objeto × luz.
- **El arte PLANO ya no se tiñe por sprite.** Todo lo que está bajo
  `layers::YSORT_MIN` lo ilumina el shader por píxel con un degradado continuo;
  teñirlo además por sprite le pone un valor único a toda la baldosa. Y son la
  mayoría: 3.915 sprites de tile contra ~560 objetos en la captura del barrio.

| `span[lit=…]` en la vista de barrio | µs |
|---|---|
| antes (24 luces) | 2.461 |
| con 40 luces + rechazo al cuadrado | 1.219 |
| \+ sin teñir el arte plano | **503** |

#### FUEGOS ARTIFICIALES (`render/fireworks.rs`, 28 ago 2026)

La pieza que le faltaba a la noche: con los faroles encendidos el pueblo ya se lee,
pero **una noche con luz fija es una postal**. Lo que la vuelve una escena es que
algo cambie la luz — durante un estallido la fuente dominante deja de ser el farol
y pasa a ser el cielo.

Tres piezas: el **cohete** (sube frenándose, con rastro y su propia `Light2d`
chica), el **estallido** (44 chispas con velocidad radial, gravedad y ARRASTRE —
el arrastre es lo que las hace abrirse de golpe y quedar colgando, que es la
silueta que uno reconoce) y el **destello**, una `Light2d` grande y efímera que es
lo que ilumina el suelo.

Presupuesto: máximo 3 estallidos vivos, sólo de noche, sólo en superficie y sólo
**dentro del casco** de un poblado — en el campo no hay quién los tire. De día son
cero entidades y un `if` por frame. `HA_PIROTECNIA=1` acorta la espera para que un
driver de QA los vea en su corrida (si no, se verificarían por el log y nunca por
la imagen).

#### Las mariposas DECIDEN (3 ago 2026)

Eran decoración: 22 sprites recicladas en un anillo alrededor tuyo, movidas por una
senoide con separación y cohesión. Bonito, pero cualquiera que las mirara diez
segundos veía que no les pasaba nada.

Ahora tienen **pulsiones** y una `energy` que las ordena:

| Pulsión | Cuándo | Qué hace |
|---|---|---|
| `Huir` | jugador a menos de `FLEE_R` | se aparta; manda sobre todo lo demás |
| `Buscar` | `energy < HUNGRY` | va a una flor concreta |
| `Libar` | llegó a la flor | quieta, repone energía |
| `Descansar` | `energy` alta, sobre una planta | posada, casi no gasta |
| `Volar` | resto | el boids de siempre (jugar, perseguirse) |

Volar gasta, huir gasta más, libar repone: el lazo se autorregula solo y la energía
de la bandada oscila sola. Censo en vivo con **`HA_MARIPOSAS=1`**.

Las flores son **reales**: salen de `ha_shared::worldgen::nature_decor_at`, que es
determinista, así que la mariposa "ve" flores por celda **sin ningún índice
espacial** y sin depender de que el sprite esté materializado. La búsqueda va por
anillos crecientes con salida al primer acierto, y el re-pensado está escalonado por
bicho (`RETHINK_EVERY`), así que en una pradera cuesta unas decenas de hashes.

⚠ Cero cerebros IAFZ a propósito: el presupuesto de cerebros ya está repartido entre
fauna, residentes y NPCs, y los 60 fps del Redmi mandan. Engancharles IAFZ es una
fase posterior, y sólo si sobra margen medido en el celu.

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
puede hablar a VOS y no al mundo. Los acentos van; lo clava
`la_fuente_dibuja_los_comentarios`.

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

### Todo el mundo se deja tocar (15 ago 2026)

Hasta acá el dedo sólo encontraba **árboles, rocas, vetas, cofres, NPCs y lo que
vos hubieras colocado**. El banco de la vereda, el farol, el cajón del galpón, el
tronco flotante y las 7.821 claves del pack eran **pintura de fondo**: se veían,
tapaban, daban sombra, y no contestaban. Un mundo donde la mitad de lo que ves no
responde al dedo se siente roto.

Tres cambios, y los tres al servicio de la misma frase del usuario — *"lo mejor
sería ir directamente a tocar"*:

1. **Todo prop declara objetivo.** `spawn_chunk_small_decor` y el decor autorado
   ponen `InteractTarget { kind: Prop }` + `PropArt` (la clave de arte, en `Cow`
   para no asignar nada en el camino caliente). Se levanta con el MISMO gesto que
   todo lo demás —mantener— y lo resuelve `harvest::pick_up_prop`, que anota la
   baja en el set de removidos (así no resucita al re-streamear) y devuelve el
   material si el objeto está en el catálogo de colocables. El cartel dice su
   nombre real, sacado de `ha_assets::slots::slot_label`.
   - **Menos lo que es PISO.** Charcos, musgo, hojas caídas, alfombras y la
     textura del agua están marcados `Floor` porque no se levantan de la baldosa:
     **son** la baldosa. Si fueran objetivo le robarían el toque a la celda y
     cavar/colocar dejarían de funcionar sobre cualquier suelo decorado. El
     portero es `layers::es_piso`.
2. **Margen de perdón del dedo** (`targeting::PICK_FORGIVENESS`, 0,55 m), y con
   la cuenta hecha: a pie la cámara encuadra 30 m sobre ~1080 px → **36 px por
   metro**, un tile es 1 m, y la yema apoya 40-60 px. **El dedo es más grande que
   el banco.** Pero es una SEGUNDA pasada, no un imán: `grado_de_toque` devuelve
   0 si caíste dentro del rectángulo y 1 si entraste por el margen, y el grado
   ordena antes que la distancia. De tres árboles pegados **gana el que tocaste**
   — eso es lo que no se negocia. Y en **modo CONSTRUIR el margen no juega**: ahí
   la intención es la celda, y un prop que se cuela por el margen no es una ayuda,
   es una pieza que no se colocó (el rectángulo exacto sí sigue valiendo, así se
   saca del medio el arbusto que estorba sin salir del modo).
3. **El resaltado abraza al OBJETO, no a la celda.** Los corchetes amarillos se
   estiran al rectángulo del sprite apuntado (`sync_cursor_highlight`), con los
   brazos a grosor constante — escalar el nodo engordaba el trazo y sobre una casa
   de cuatro tiles quedaba un marco gordo. Sin objetivo con cuerpo (cavar, colocar
   un bloque) vuelven a la celda, que ahí sí es lo que va a pasar.

⚠ **Anti-regresión:** el margen NO puede crecer hasta elegir por el jugador, y
nunca puede haber un "seleccionar el más cercano" ni un ciclador de objetivos: si
hay que apuntar, se toca. Lo cuidan `el_rectangulo_tocado_le_gana_al_margen_del_vecino`
y `el_margen_tiene_final` (`targeting.rs`).

**Driver:** `HA_PROPTEST=1` planta el dedo sobre cada prop al alcance, uno por
frame, y exige que TODOS se resuelvan como objetivo (cobertura) y que salga el
apuntado y no un vecino (precisión). Deja `prop-1-sin-dedo.png` y
`prop-2-resaltado.png`.

**Lo que todavía NO se toca:** las **máquinas** colocadas (`machines.rs` no tiene
`InteractTarget` ni API de baja en `MachineWorld`) y la **arquitectura generada**
—fachadas, muros y techos de `edificios.rs`/`walls.rs`/`building.rs`—, que se
tocan como tiles (`Block`) y no como objetos.

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
spawnea 48 tarjetas, no 350 de una sentada. Los índices de paleta
siguen siendo los de las tablas, o sea que `build_selection` no se entera.
Lo cuidan `ningun_grupo_de_construir_esta_vacio`,
`todos_los_colocables_tienen_ficha` y `toda_categoria_usada_esta_en_el_menu`.

Y **la ficha se pagina** (`BUILD_POR_PAGINA = 48`, fila ANTERIOR/SIGUIENTE). Con
el pack entero importado MUEBLES tiene 2.878 colocables: escupirlos juntos son
2.878 botones con icono y texto en un frame —el mismo tirón que las fichas
vinieron a evitar— y 28.000 px de scroll que no se recorren con el dedo. Con
página el costo de abrir una ficha es constante, se mire el catálogo que se mire
(`ninguna_pagina_de_construir_es_gigante`).

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

**El catálogo son 7.821 objetos, no 295** (3 ago 2026). Lo curado a mano llegaba
a 176 objetos del pack sobre 11.691 sprites comprados: el 98 % del arte no estaba
en el juego. Ahora entra entero con `scripts/import-pack-masivo.py` — ver §8,
"El pack entero".

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
  es de AUTORÍA, no de código: pintá la huella con el detalle de ¼ de tile
  (`Assets → huella y zona`, o el botón `sólido = silueta`), que perfila la
  silueta en vez del tile entero. Sin
  huella pintada el asset bloquea su celda ancla completa, y ahí sí se siente un
  muro donde no hay nada.

### EL ACANTILADO NO PROYECTABA SOMBRA DE SOL (21 ago 2026)

Reporte del usuario: *"las paredes/acantilados o cualquier terreno no generan
sombra como las rocas, me refiero a ese sol que genera la sombra"*. Tenía razón,
y eran DOS bugs en `light2d::attach_ground_shadows`/`cast_shadows`, el sistema
que le cuelga a cada sprite del mundo una `GroundShadow` —un quad que
`cast_shadows` gira y estira según de dónde venga el sol (`sky2d::SkyLight`)—.
Confirmado con el flag que el propio sistema ya traía para esto,
`HA_SHADOW_DEBUG=1` (pinta la sombra en magenta opaco: a su alpha real se
confunde con la roca oscura y no se ve si está o no).

1. **El LARGO salía del ANCHO, no del alto.** `length = size.x * 0.62` — para un
   decorado más o menos cuadrado (un roble, una roca) da lo mismo, pero un
   acantilado es una columna de **una** celda de ancho que puede medir **diez**
   de alto (`relief::SALTO_DE_TERRAZA`): la sombra salía del porte de un
   guijarro sin importar si el muro medía uno o nueve pisos. Arreglado con
   `size.x.max(size.y)`: a un decorado normal (alto ≤ ancho) no le cambia un
   pixel: al que es angosto y alto, le da su tamaño real.
2. **La pared apilada no tiene UN sprite, tiene N.** `pared_apilada` dibuja cada
   fila (labio, cuerpo, pie) como su propia entidad de 1×1 —es lo que permite
   que la pared sortee como un solo objeto y que la fila del medio tenga
   variantes—, pero eso significa que `attach_ground_shadows`, que no sabe nada
   de paredes y sólo mide `custom_size`, le colgaba a un farallón de nueve pisos
   **nueve sombritas idénticas apiladas**, cada una del porte de un ladrillo. Se
   arregla con un ancla invisible (`Sprite::from_color(Color::NONE, ...)`, el
   mismo truco que ya usa la veta sin arte) del tamaño de la pared ENTERA
   —`TILE_M` de ancho por `(filas+1) × TILE_M` de alto—, y las filas visibles se
   marcan `light2d::sin_sombra` para no duplicar la mancha.

Verificado con `HA_SHADOW_DEBUG=1` antes y después (la cuña magenta pasa de una
línea fina a un bloque sólido) y con una captura normal (las franjas
diagonales del sol cruzando la nieve, donde antes no había nada). Ningún test
de mundo se movió: el cambio es puro render, no toca `relief.rs`.

---

## §8 Assets y editores — HA Studio

### Todo slot del juego tiene pixel (14 ago 2026)

Medido con `art2d::que_le_falta_pixel_al_juego`, el catálogo canónico
(`ha_assets::game_slots`) tenía **142 claves de 8.792 sin una sola entrada** en el
manifest: los 36 iconos del inventario, 18 de interfaz, la fauna, las máquinas, el
cohete, los muebles, las cercas… Todo eso se dibujaba como un cuadrado de color
plano, y son justo los que el jugador mira todo el tiempo (la mochila y la barra
rápida están en pantalla siempre).

Hoy son **8.792 de 8.792**, y lo sostiene un guardián:
`art2d::ningun_slot_se_queda_sin_pixel`. Agregar un slot sin darle arte falla en
la compu, no en el teléfono.

> ⚠ **Y se dibujó de más: 46 de las 132 no hacían falta.** El arte del pack no
> sólo existía — **ya estaba importado en el manifest**, con el nombre traducido
> al castellano por el importador masivo. Se dibujó `vehicle/bus` teniendo
> `vehicle/colectivo_abajo`, `agua/estrella_mar` teniendo seis estrellas de mar,
> `furniture/inodoro` teniendo los 180 muebles de baño de Modern Interiors. Las
> dos causas: buscar **en inglés** cuando el importador bautiza en castellano, y
> buscar sólo en `PRUEBAS-CODIGOS/` (los PNG crudos) en vez del manifest, que es
> donde está lo que el juego ya sabe dibujar. Corregido el 15 ago 2026:
> `scripts/asignar-del-pack.py` apunta 46 slots canónicos a la clave que ya
> estaba, y esas recetas se borraron de los generadores. **Un slot vacío casi
> nunca significa "falta pixel": significa que nadie dijo cuál de las 13.413
> claves le toca.** Las dos herramientas que lo evitan —y que hay que usar antes
> de dibujar— son `scripts/ver-claves.py` (lo ya importado) y
> `scripts/ver-pack.py` (los PNG crudos).

**El orden con el que se cerraron es el de la skill `pixel-art`**, y vale para
todo lo que venga:

1. **Asignar lo que ya está** — `npc/colonist`, `aldeano` y `comerciante` no se
   dibujaron: el juego ya tenía veinte hojas de personaje del pack
   (`art/npc/char_*.png`) y sólo faltaba decir cuál le toca a cada oficio.
2. **Derivar** — `cliff/roca` / `slope/roca` / `ledge/roca` salieron de sumarle un
   material al kit de acantilado; `tiles/regolith` y `tiles/basalt`, de anclar una
   losa del pack al color de su propia rampa; los `ui/*_empty`, de apagar su
   `*_full` (que además garantiza que la pipa vacía comparta silueta con la
   llena — el corazón oscuro del pack mide 176 px opacos contra 292 del lleno y
   no servía).
3. **Adaptar** lo que el juego pide.
4. **Dibujar**, que dejó de estar prohibido. La regla 7 decía "nunca generes
   PNGs" y el usuario la levantó: *"si logras perfeccionar eso podríamos borrar
   esa regla. Pero si vas a poner un solo color sólido todo feo, mejor adapta el
   juego"*. O sea que la prohibición era por CÓMO salían los dibujos, no por
   dibujar.

**El estilo es uno y está medido**, no descrito: 32×32, 9 colores por sprite (30
de techo), cero dithering, cero contorno negro (el borde es el tono oscuro del
propio material), paso de 12 de luminancia entre tonos, el oscuro tintado,
saturación 0,28, luz suave de arriba-izquierda (+5/+4) y sombra de contacto en
**(58,58,80) a alfa 100** — todos números sacados de 250-400 singles del pack y de
las 168 páginas que el juego dibuja (75,9 M de píxeles). El corsé que los hace
cumplir es `scripts/lib/pixel.py`: cada color pasa por la paleta de 501 tonos
medida del arte en uso y `verificar()` se planta si la pieza tiene damero, negro
neutro, alfa raro, demasiados colores o queda flotando.

Lo dibujado vive en **`art/gen/`** y lo comprado en **`art/pack/`**: se
distinguen de un vistazo justamente para poder reemplazar un dibujo por arte real
cuando aparezca. El manifest manda, así que ese reemplazo no toca código.

Los generadores son `scripts/dibujar-items.py` (35 iconos),
`scripts/dibujar-ui.py` (14 dibujados + 3 derivados) y
`scripts/dibujar-mundo.py` (37 dibujados + 4 asignados) — **86 piezas dibujadas,
no 132**: lo que queda es ciencia ficción (máquinas, cohete, minerales), fauna de
granja, fondos de cielo, iconos de inventario y un puñado de piezas urbanas que
LimeZu no dibujó. Todo lo demás salió del pack.

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

### El pack entero — `import-pack-masivo.py` (3 ago 2026)

Los packs comprados (Modern Exteriors + Modern Interiors, `PRUEBAS-CODIGOS/`)
traen **11.691 sprites**. Hasta esta fecha estaban en el juego **176**: el
importador viejo (`import-pack-objects.py`) leía una tabla escrita a mano
(`pack-objects.tsv`, 178 líneas) donde alguien miraba la hoja de contacto y le
ponía nombre a un sprite. El 98 % del arte comprado no se dibujaba en ninguna
parte — las calles, sin ir más lejos, salían con el color plano del basalto.

```bash
python3 scripts/import-pack-masivo.py     # el pack entero → slots + manifest + siembra
python3 scripts/import-calles.py          # asfalto, vereda y línea de carril
python3 scripts/import-emisiones.py       # QUIÉN ilumina de noche, medido del dibujo
python3 scripts/dibujar-guirnaldas.py     # lo único de luz que el pack no tiene
```

⚠ **`import-emisiones.py` va DESPUÉS de cualquier importador que agregue arte**, y
también después de `dibujar-guirnaldas.py`: mide sobre lo que ya está publicado en
el manifest. Es idempotente (`scripts/emisiones-import-index.json`) y **respeta la
emisión que curó el editor a mano**. El detalle está en §7 ("La noche se
enciende").

Lo que antes era criterio humano ahora sale **del pixel**:

| Pregunta | Cómo se responde |
|---|---|
| ¿Qué es un objeto? | Los singles de interiores ya son objetos enteros; los de exteriores vienen **partidos** (`Condo_1_1..10` son diez tajadas de un edificio). Cada single se ubica en la hoja de su tema por hash de celda y las piezas pegadas se recortan juntas. |
| ¿Variante u otro objeto? | Misma silueta (canal alfa idéntico) + índices consecutivos = el mismo mueble en otro color → variantes `objeto#N`. 556 PNG de dormitorio son 205 objetos. |
| ¿Cuánto mide? | Del tamaño del PNG × 1,5 (32 px = 1 tile de LimeZu ≈ 1,5 m del juego). |
| ¿Cómo se llama? | `scripts/pack_lexico.py`: frases hechas primero, adjetivo detrás del sustantivo y concordancia de género. Los interiores no traen nombre en el pack y salen como "Dormitorio 137". |

Resultado: **7.821 objetos, 10.918 sprites únicos** (348 repetidos del pack se
deduplican por hash), en **125 páginas de atlas de 1024²** que pesan 9,6 MB en
disco. Escribe `manifest.json`, los bloques `PACK:START/END` de `slots.rs` y
`placeables.rs`, y `ha-shared/src/pack_siembra.rs`.

**Reimportar no pisa lo curado**: cada script anota lo suyo en su índice
(`scripts/pack-import-index.json`, `scripts/calles-import-index.json`) y en la
corrida siguiente borra exactamente eso.

**Un slot declarado y no sembrado no existe.** Por eso el importador también
genera `pack_siembra.rs`: nueve tablas por SITIO (vereda, calle, parque, playa,
cementerio, bosque, campamento, obra, patio) de 60 objetos cada una, que consume
`worldgen`. Es el puente entre "está importado" y "se ve caminando".

**Carga perezosa (obligatorio con este volumen).** 12.000 claves × textura en
boot son ~500 MB: OOM instantáneo en el Redmi. `art2d::ArtLibrary` guarda RUTAS y
pide el PNG la primera vez que alguien dibuja la clave; el log escupe una línea
por hoja (`ha-art: hoja art/pack/city_props_4.png (viven 49)`), que es el número
que hay que mirar. Medido en una ciudad: **11 páginas del pack vivas de 125**.
La pantalla de carga precarga las tablas de siembra (`loading::claves_del_mundo`)
para que la cortina no se levante antes que los props.

### Lo que el pack dibujó ANIMADO — `import-animados.py` (9 ago 2026)

`import-pack-masivo.py` mete los recortes SUELTOS. Los dos packs traen además
carpetas de **hojas de animación** que ese script no mira, y ahí estaba la vida
del mundo sin usar: fuentes, fuego, agua que se mueve, molinos, banderas,
gaviotas, barreras, maquinaria. **842 hojas compradas que no veía nadie.**

```bash
python3 scripts/import-animados.py --dry-run
python3 scripts/import-animados.py     # 26 páginas + manifest + slots + colocables
```

**No entran las 842 enteras, y el número lo dice todo**: suman 536 megapíxeles =
511 páginas de atlas ≈ 2 GB de textura. En un teléfono de 3,8 GB eso es el mismo
OOM que ya documentó el importador masivo. Se recorta con tres reglas medidas:

| filtro | cuántas saca | por qué |
|---|---|---|
| puertas fuera | 195 | el juego ya tiene puertas con poses `open`/`closed` (`building::DoorArt`) — regla 8: ¿pieza de un kit que ya existe? |
| fotograma ≤ 8 celdas | 90 | por encima no es un objeto, es una escena (hay una hoja de 798 fotogramas) |
| no cae en la grilla de 32 | 8 | hoja rota |

Quedan **548 objetos animados**, 26 páginas, 12 MB en disco. Los fotogramas se
capan en 8: un bucle se lee con eso.

**Cuántos fotogramas tiene una hoja no lo dice ni el nombre ni el tamaño.** Se
probó lo obvio —"el objeto mide de ancho lo que la hoja mide de alto"— y falla:
el molino es una hoja de 16×8 celdas con CUATRO molinos, no dos. Lo que funciona
es buscar el **período** (`import-animados.py::periodo`): la parte QUIETA de una
animación —el marco, la torre, el pilón— se repite idéntica cada fotograma, así
que se prueba cada ancho que divida a la hoja y gana el más chico cuyos
fotogramas se parecen entre sí.

Escribe `anim` (no `frames`/`fps` sueltos): es el nombre del campo en
`ha_assets::art_manifest::ArtEntry` y serde ignora en silencio lo que no
reconoce — una tira con los campos mal puestos entra como sprite quieto y nadie
se entera. Y escribe **también** el bloque de `placeables.rs`: un slot sin
colocable existe en el editor y no lo pone nadie.

**Lo que sigue sin usarse, a propósito**: el pack de UI
(`modernuserinterface-win`, 304 archivos — es arte de HUD, no de mundo),
`6_Home_Designs` (132 láminas de ejemplo, no assets) y los autotiles sueltos (26,
ya cubiertos por los importadores curados de agua, calles y acantilado).

### Las 54 paredes de interior, y por qué no se usaban — 24 ago 2026

El reporte fue *"quiero que importe bien los assets de las paredes interiores…
busca dónde se encuentra el bug de por qué aún no se usan"*. Las dos mitades
resultaron ser cosas distintas, y la segunda es la interesante.

**Lo que faltaba importar.** `Room_Bulder_subfiles_32x32/` trae **nueve** hojas y
el juego tocaba tres. De la de paredes, `import-construccion.py` saca dos
estilos —el más de madera y el más de piedra— de los **54** que dibujó LimeZu.
Los otros 52 estaban en el disco desde el primer día, y con ellos la cocina, el
baño y el dormitorio de toda la ciudad eran el mismo cuarto pintado del mismo
marrón. `scripts/import-paredes-interiores.py` los importa todos: cada estilo con
sus cuatro caras, **su vano** (la pared con el hueco, de la hoja de arcos) y **su
losa**, emparejados por COLOR y no por orden de hoja. Más las cinco familias de
remate que nadie había tocado: zócalos, orlas, empalmes de piso, sendas y
sombras al pie. Son 945 claves nuevas.

Los nombres salen del pixel, no de un archivo: el pack no bautiza estas celdas,
así que el tono y la claridad medidos dan "Pared verde clara 2". Es el mismo
criterio con el que `import-construccion.py` decide cuál bloque es "de madera".

⚠ **El friso NO se conserva, tampoco en la cara de adentro.** Es tentador
quedárselo —"el cielorraso se lee desde el cuarto"— y es un error: esos 10 px de
blanco son el techo del cuarto **de la hoja**, el mockup con el que se dibujó el
catálogo. La primera versión de este importador se los quedó y el guardián lo
cazó en 108 recortes. `verificar_pagina` es el mismo de `import-construccion.py`,
heredado a propósito.

**Y ahora el bug de verdad, que no era de arte.** Las paredes no se usaban por
tres motivos encadenados, y ninguno era "falta pixel":

1. **La ciudad generada no tiene una sola pared de interior.** `residential_tile`
   hacía `if !perimeter { return WoodFloor }`: el interior de toda casa del mundo
   era un galpón. Una casa de campo de 23×12 era un salón de 21×10 sin una
   división. No es que la pared de interior se viera mal — **no había dónde
   dibujarla**. Ahora `city::tabique_de_planta` parte la planta en cuartos.
2. **`center` era `front`.** Un tile de pared pide su forma de autotile
   (`edge_n`, `corner_ne`…), ninguna pared tiene esas poses, y `tile_sprite` cae
   **siempre** a `center` — que apuntaba al paño de adentro, con su zócalo. O sea
   que las casas de todas las ciudades se dibujaban con el empapelado del cuarto
   puesto para la calle. Desde afuera, una pared es su paño liso.
3. **La pose `espalda` no existía en ningún asset.** `walls::Cara::Fondo` la pide
   desde el 8 ago y la cadena caía a `front`, así que la espalda de toda casa
   construida también salía empapelada. Ahora la escriben los dos importadores.

El que las conecta es `world2d::pared_de_interior`: elige el estilo **por casa**
(hash del rect edificado, así los cuatro muros de un cuarto combinan y la casa lo
conserva entre partidas sin guardar nada) y la **cara por geometría** —en cenital
3/4 la pared del norte enseña su lado de adentro y la del sur el de afuera—, que
es la misma regla que `walls::Cara` ya aplicaba a lo que construye el jugador.
Lo que faltaba era aplicarla a lo que genera la ciudad.

**Los tabiques y la veda.** `tabique_de_planta` es una partición binaria por el
lado largo, iterativa y sin memoria (quien pregunta es un `topdown_tile` por
celda). Lo único delicado es que **un vano abre contra la fila que tiene
enfrente**: si un corte posterior planta su tabique justo ahí, el cuarto se
sella. Medido: 2.613 plantas de 16.000 con habitaciones inalcanzables. Con la
veda —cada vano prohíbe su coordenada a los cortes que vengan después— son 0.
Lo cuidan `ningun_cuarto_queda_sellado` y `todo_cuarto_da_para_amueblarse`.

**Una abertura, una puerta.** La fachada del pack ya trae su puerta dibujada y
animada, y el sim además estampaba su `Tile::Door` en la misma cara: la cara a la
calle de toda casa generada tenía **dos puertas encimadas**, con pose y posición
distintas. Manda la fachada, que es la que tiene el dibujo bueno; se calla sólo
el SPRITE del tile, no el tile — se sigue cruzando por ahí, y las otras tres
caras conservan la suya. Es el mismo bug que `edificio.rs` documenta como razón
de existir ("las aberturas son UNA sola lista"), que en la ciudad generada nadie
estaba evitando.

⚠ **Lo que NO entró: las plantas altas.** El pedido incluía "fijate cuántos pisos
tiene la casa por fuera y hacé los mismos por dentro". Se midió y **la fachada no
lo dice**: derivar la altura de elevación del alto del PNG menos la huella da
negativo en 38 de 45 programas, porque esos sprites no son elevaciones
consistentes (hay plantas, hay 3/4, hay recortes al talle del lote). Inventar un
número ahí sería justo lo que §2 prohíbe. Y el resto de la cadena tampoco está:
`topdown_tile` devuelve `Air` en todo `z > 0`, `building::stream_roofs` sólo
genera techos con `z == 0` (subís y la ciudad se queda sin fachadas), y fauna,
vecinos, clima y cofres están apagados en `z != 0` con brocha gorda. Es un
trabajo de su propio tamaño: sim en `z>0`, un `city::tramo_de_casa` colgado de
`Escaleras::en` como ya lo hace `cuevas::tramo_natural`, y desactivar el gate de
`z != 0` para las plantas de casa —que no son el subsuelo.

### El kit de cuarto: `Room_Builder_3d_walls`, y por qué la entrada de arriba estaba mal — 24 ago 2026

El reporte fue *"¿qué son esas porquerías? ¿por qué no usás las paredes
verticales, las que tienen ángulo?"* — y tenía razón. La entrada de acá arriba
("Las 54 paredes de interior") describe un trabajo con **la hoja equivocada** y
con dos suposiciones que no aguantaron la medición. Queda escrita porque el
diagnóstico de fondo —"no faltaba pixel, faltaba quien lo pidiera"— sigue siendo
cierto, pero lo de abajo la corrige.

**Hoja equivocada.** `Room_Builder_Walls` dibuja el paño de frente y nada más. Un
cuarto no se arma con eso: un muro norte-sur salía como una hilera de carteles de
frente puestos de canto, y no hay pieza para las esquinas. La hoja que LimeZu
dibujó PARA armar un cuarto es **`Room_Builder_3d_walls_32x32.png`**, que estaba
sin importar: trae el muro horizontal con su tapa, los verticales vistos de canto
y las esquinas en ángulo.

**La grilla se midió contra las casas del propio artista.** El pack trae
`6_Home_Designs/` — casas armadas por LimeZu con este kit. Comparando celda por
celda (hash del pixel) qué recorte usó y en qué lugar del cuarto, el uso se
amontona en un bloque de **8×7 que se repite cada 8 columnas y cada 7 filas**:
223 usos de la celda local (3,3) y 178 de (3,2) contra 0 de sus vecinas. De ahí
salen las piezas, ninguna escrita a ojo:

| pose | pieza local | qué es |
|---|---|---|
| `edge_n` | (3,2)+(3,3) | el muro horizontal: tapa arriba, paño abajo |
| `edge_w` / `edge_e` | (1,2) / (6,2) | los verticales: de un muro norte-sur se ve el canto |
| `edge_s` | (3,5) | el que da a la calle: se le ve la tapa y nada más |
| `corner_nw` / `corner_ne` | (2,2) / (5,2) | las cuñas en ángulo |
| `end_w` / `end_e` | (0,0)+(0,1) / (0,3)+(0,4) | las jambas de un vano |

**Un vano es un HUECO, no un arco.** El kit no dibuja "la pared con el agujero":
deja pasar el piso y remata el muro a cada lado con su jamba. Así que
`Tile::Door` de tabique devuelve la LOSA y el hueco lo dibujan los dos muros que
lo flanquean. La hoja de arcos sigue importada para colocarla a mano desde HA
Studio, pero el mundo generado ya no la pide.

⚠ **Corrección: NO hay cara de afuera en `Room_Builder_*`.** El importador viejo
publicaba la columna 5 como `back`/`espalda`/`center` creyendo que era "el paño
liso de la vereda". Está medido que es mentira: color medio (204,208,211) contra
(208,212,215) del frente — **el mismo empapelado**, sólo que sin la tapa. Con eso
toda casa generada del mundo enseñaba el empapelado del dormitorio mirando a la
calle, que es justo lo que el punto 2 de la entrada anterior decía haber
arreglado. Ahora el importador **no emite `back` ni `espalda`**, y el muro que da
a la calle contesta con su tapa: lo que mira al frente es la fachada de Modern
Exteriors.

⚠ **Corrección: el friso SÍ va — es la TAPA del muro.** La entrada anterior lo
tiraba ("son 10 px del mockup"). Sin tapa el paño queda flotando contra el piso y
un cuarto se lee como una mancha de color. Las casas de `6_Home_Designs` tienen
tapa en **todas** sus paredes. El guardián de salida quedó dado vuelta:
`verificar_tapas` se planta si el muro horizontal perdió su celda de arriba.

**Se probó y NO sirve como guardián**: pedirle a la tapa que sea más clara que su
paño (la distancia de color entre las dos va de 4,2 a 150,6 en los 19 estilos, y
cinco tienen la tapa del mismo tono), ni pedirle una línea de contorno oscura
arriba (la traen 15 de 19). Lo que sí vale para los 19: el recorte es opaco de
punta a punta y su celda de arriba **no es el mismo dibujo** que la de abajo
(cambian de 456 a 1.024 píxeles de 1.024). Ese es el test.

**La pieza sale de dónde está el CUARTO, no de qué borde del rect es.** Un
tabique y una pared de borde son la misma pared mirada desde el mismo lado; lo
que decide qué se ve de ella es para dónde le queda el piso
(`world2d::cara_de_muro`). La regla vieja —"si es perímetro, `back`"— le pedía
piezas distintas al tabique norte-sur y al muro oeste de la casa, siendo la misma
pared.

**El emparejado del piso por color, corregido.** Elegir para cada pared la losa
MÁS parecida es exactamente lo que no se puede hacer: medido sobre
`azul_clara_4`, pared (208,212,215) y piso (206,214,220) dan 8 de distancia y el
cuarto entero sale de un solo azul. Ahora se pide un mínimo de separación de
brillo (22, la mitad de los 45 que usa `Generic_Home_1` entre su pared gris y su
piso de madera) y entre las que lo cumplen gana la más parecida: el cuarto sigue
combinando, un tono más abajo.

**La maqueta.** `scripts/maqueta-casa.py` dibuja una casa entera con los rects del
manifest y la misma regla de apoyo que usa el chunk, sin compilar el juego. No
reemplaza a la captura: reemplaza a *adivinar*. Un recorte mal elegido se ve en un
segundo y no en veinte minutos de `cargo`.

**Colisión: ya era la de abajo.** El pedido de *"controlar la colisión sólo en la
parte de abajo, para que dé la ilusión de pasar detrás de las paredes"* no
necesitó código: un muro ocupa UN tile de suelo y el sprite mide dos celdas hacia
arriba, así que lo que frena es el pie y lo que tapa es el cuerpo. Lo que faltaba
era el arte — con el paño de frente en los cuatro lados no había ilusión que
mostrar.

⚠ **Lo que NO entró: sótanos y plantas altas.** El pedido incluía "tené en cuenta
las puertas de sótanos y para segundos y más pisos". El mecanismo de andar entre
plantas existe (`ha_shared::escaleras` + `escaleras.rs`), pero **nadie planta un
tramo dentro de una casa generada**: `city.rs` no coloca escaleras y el resto de
la cadena sigue como lo describe la entrada anterior (`topdown_tile` devuelve
`Air` en `z > 0`, `stream_roofs` sólo genera techos en `z == 0`). Es el mismo
trabajo de su propio tamaño que ya estaba anotado, ahora con el kit de cuarto
listo para vestirlo.

### Las hojas que el importador masivo NO podía ver — 4 ago 2026

`import-pack-masivo.py` recorre las carpetas `*_Singles_32x32` del pack. Las
hojas que **no tienen carpeta de singles** nunca entraron — y son, exactamente,
las de construir:

- `moderninteriors/1_Interiors/32x32/Room_Bulder_subfiles_32x32/` (paredes,
  pisos, zócalos, bordes, arcos: 9 hojas),
- `Theme_Sorter_32x32/17_Visibile_Upstairs_System_32x32.png` (gradas y pisos
  altos).

Por eso `wall/wood`, `wall/stone`, `window/basic`, `stairs/up`, `stairs/down`,
`floor/stone`, `ceiling/madera`, `roof/teja` y `roof/chapa` tenían slot con
nombre y **ningún PNG**: levantabas una pared y salía el cuadrado de
`walls::fallback_color`. Doce mil sprites importados y la cosa que más se toca
del juego dibujada a color plano.

```bash
python3 scripts/import-construccion.py --dry-run   # informa qué bloque eligió
python3 scripts/import-construccion.py             # página + manifest
```

Elige **midiendo**, no a ojo: la hoja de paredes son bloques de 10×2 celdas
repetidos 54 veces y la de pisos bloques de 3×2; cuál es "madera" y cuál
"piedra" sale del color medio en HSV del **centro del paño** (el friso de arriba
y el zócalo de abajo son iguales en todos los estilos y le regalaban textura a
las paredes lisas: una pared pintada de verde salía "de piedra" por su zócalo).
La geometría se verifica contra el pixel antes de cortar (`verificar_hojas`), así
que si LimeZu reordena una hoja el script se planta en vez de escribir recortes
que apuntan a cualquier cosa.

La ventana es la pared **con el paño de vidrio del pack encima** (capas
modulares, que es como el pack está pensado): recortado a 20 px de los 32 para
que se siga viendo pared a los costados — si no, no se lee "pared con ventana"
sino "vidrio del tamaño del tile". Las gradas salen del *Visible Upstairs
System*: madera para subir, piedra para bajar, porque en una celda de 32 px un
tramo de escalones se ve igual en las dos direcciones y el material las separa de
un vistazo.

⚠ **Anti-regresión**: `slot_audit::toda_pieza_de_construccion_tiene_pixel` falla
si una pieza de `EDGE_PIECES`, `CEILING_PIECES` o `BUILD_PIECES` se queda sin
entrada en el manifest. El decorado del mundo NO entra en ese test a propósito:
ése sí puede esperar al pixel.

### Arte que estaba y nadie pedía — `slots-colgados.py` (4 ago 2026)

Cuando el importador masivo regeneró el bloque `PACK:START/END`, los objetos que
había curado el importador viejo **perdieron su slot y su colocable**; el arte se
quedó en el manifest. Quedaron **195 claves con PNG que ninguna línea de código
nombraba**.

```bash
python3 scripts/slots-colgados.py             # informa
python3 scripts/slots-colgados.py --limpiar   # borra SÓLO las copias
```

Copia **no** es "se llama parecido" — el importador bautiza automático y erra
seguido. Se compara el pixel: mismo dibujo (recortado el aire de alrededor,
mismo tamaño) **y** mismos rasgos (animación, huella, capa, emisión,
interacción). `grid` queda afuera de esa comparación a propósito: es cómo se lee
el `rect` dentro de la hoja, no una propiedad de la pieza, y meterlo declaraba
distinto TODO par (el arte viejo venía en PNG suelto, sin `grid`).

Resultado: **168 eran el mismo dibujo con otro nombre** (borradas; el dibujo
sigue vivo bajo su clave del pack) y **27 no** — peceras y amplificadores
animados, fogata, cascada, burbujas, cardumen, rocas flotantes, un cuervo con
seis cuadros de aleteo. Ésas volvieron al juego en `slots::RESCUE_KEYS`,
`PLACEABLES` y `worldgen::WATER_DECOR`, **fuera** del bloque PACK para que la
próxima importación no se las lleve otra vez.

De yapa, el script ajusta los recortes que declaran más celdas de las que tiene
su PNG (4 casos, de arrastrar una celda de más en HA Studio): el juego los
dibujaba pegados arriba con un hueco debajo.

**Y lo animado ahora se anima**: `world2d` pedía el sprite sembrado con
`world_sprite_seeded` y tiraba la animación que le devolvía. El pack traía las
peceras, el fuego, el cardumen y el cuervo con sus cuadros recortados, y el mundo
los plantaba congelados.

### El suelo se dibuja con el pack — 4 ago 2026

La calle se veía como una banda de color plano con la línea de carril flotando
encima. La causa: `art2d::tile_has_art` sólo dice que sí con poses, animación o
tira, y `floor/asfalto`/`floor/vereda` tenían su recorte y **ninguna pose**, así
que `spawn_chunk_tile_art` los salteaba. Lo único con poses era `road/linea` — de
ahí "la calle es pintada base con el asset en medio".

Ahora **el suelo lo dibuja la malla del chunk con UVs** contra una hoja única
(`art/pack/suelo.png`, la arma `scripts/atlas-suelo.py`):

- **Un solo draw call por chunk**, igual que antes. No se spawnea un sprite por
  tile — que es justo lo que la nota de `tile_has_art` existía para evitar.
- La celda (0,0) de la hoja es **blanca**: `ColorMaterial` multiplica textura ×
  color de vértice, así que lo que no tiene losa se sigue viendo con su color de
  siempre. Es lo que permitió texturizar sin tocar el aspecto de lo demás.
- La losa de cada celda sale de `ArtLibrary::suelo_uv_seeded`, que elige la
  variante **sin construir la clave**. Con `variant_key_seeded` eran 256 `String`
  por chunk cada vez que se reconstruye: medido en el Redmi, `build_chunk_mesh`
  se comía 4 ms de frame.
- La hoja mide 1024×256 px **fijos** y ese número está en los dos lados
  (`SUELO_PAGINA_PX` y el script): la malla se arma antes de que Bevy termine de
  cargar la textura, así que no hay a quién preguntarle el tamaño.

El script elige la losa **por color**, contra el que ya declara `Tile::color()`,
y con dos filtros que salen del pixel: los cuadrantes de una losa repetible se
parecen entre sí (descarta bordes y esquinas del autotile) y el material se ata a
su ORIGEN (terreno de exteriores vs. pisos de Room_Builder) porque la tierra y la
madera son el mismo marrón. Lo que no encuentra losa parecida se queda en color
plano: más vale el color de antes que una textura que no es.

⚠ **Sólo materiales de la Tierra POR PARECIDO.** Por color, la vereda "calza" con
el regolito lunar y el asfalto con el basalto — y eso pondría una acera de ciudad
en la Luna. La búsqueda por parecido sigue atada a la Tierra por eso.

**Pero eso no obliga a dejar la Luna en color plano** (14 ago 2026). Esa regla se
escribió antes de que existiera `derivar`, y con `derivar` deja de valer por el
mismo motivo por el que dejó de valer para la nieve: no hace falta que el pack
DIBUJE regolito, hace falta que dibuje un grano repetible que se pueda **anclar
por luminancia** al color del regolito. Lo dibujó. Así que `tiles/regolith` y
`tiles/basalt` entran por la puerta de la nieve —losa `Grass_1` con sus siete
variantes, trasladada al tono de destino— y el destino no se elige a ojo: son los
dos tonos de en medio de la rampa con la que `import-acantilado.py::MATERIALES` ya
tiñe `cliff/regolito` y `cliff/basalto`. Que el suelo y la pared salgan de la
misma rampa es todo el punto: en la Luna, el llano y la cara del cráter son el
mismo gris y no dos.

Es el sitio donde más se notaba, además: la Luna y Mercurio no tienen vegetación
que rompa el llano, así que el suelo es **todo** lo que hay para mirar, y era un
plano gris de horizonte a horizonte. Queda un solo material sin losa,
`tiles/subsuelo`, y ahí el color plano sigue siendo la respuesta correcta: un
macizo sin dibujo se lee como masa, que es lo que es.

### Desde la vereda no se ve el interior — 23 ago 2026

El reporte fue *"adentro estaba mal colocado, sobresalía un borde"*, y era literal:
en media ciudad se veía el piso de madera y los muebles de las casas **desde la
calle**. Tres causas distintas, las tres arregladas.

**1. El lote CRECE, el edificio crecía con él.** Un lote se agranda para llenar
manzanas enteras (`blocks_needed`: una universidad ocupa cuatro), así que el lote
colocado suele ser bastante más grande que el `cells` que declara su programa — el
dúplex declara 26×18 y en una manzana ancha se plantaba en 26×24. `plain_building_tile`
construía *todo el lote menos la vereda*, mientras `import-edificios.py` le había
elegido la fachada para el tamaño DECLARADO. Con el edificio al doble de su dibujo
y el cliente sin estirar más de 1,4× (`building::AGRANDE_MAX`), la fachada tapaba
la mitad sur y por el resto se veía el interior. Ahora `city::edificado_de_lote`
construye lo que declara el programa, centrado y apoyado contra el frente, y lo
que sobra del lote queda de explanada.

**2. Y si la fachada no llega, se INFORMA — el edificio NO se achica.**
`import-edificios.py` compara, para cada programa, la huella contra el arte que le
tocó, y lista los que se quedan cortos (`reportar_huellas`). Eso es todo lo que
hace: imprime.

⛔ **Achicar el lote está PROHIBIDO desde el 24 ago 2026.** Hasta ese día esta
misma función reescribía el `cells` del `LotProgram` en `city.rs`, con el
argumento de que "achicar un edificio para que su propia fachada lo tape es una
corrección". No lo es, y el usuario lo cortó en seco:

> *"deja de estarme achicando edificios. Si te ocupa mil tiles, que ocupe los
> putos mil. Los assets están ahí, los usás del tamaño que son."*

El tamaño de un edificio es una decisión de DISEÑO y la toma él. Que el pack haya
dibujado una universidad más chica que su parcela es un problema de ARTE —hay que
componer, repetir tramos o dibujar más fachada— y nunca una excusa para encoger el
edificio. La corrida del 24 ago que achicó diez programas (universidad 44×32 →
22×30, hangar 30×24 → 14×16, correo 20×16 → 11×16…) se revirtió entera y
`ajustar_huellas` pasó a llamarse `reportar_huellas` para que el nombre no vuelva a
mentir. Ningún importador escribe `city.rs`.

**3. El garaje era una caja sin tapa.** La parcela residencial construía, aparte de
la casa, un rectángulo de 6×5 con paredes de ladrillo, piso de madera y un portón
— y como `roofed_rect` sólo tapa el edificio principal, se dibujaba **sin techo**:
un interior a la intemperie por parcela. Se le podría haber puesto una tapa, pero
no hay con qué: Modern Exteriors no dibuja un galpón de 6×5 en ningún lado. Lo que
sí dibuja son **casas con el portón en la fachada**. Así que el juego se adaptó al
arte (regla 7): el garaje pasó a ser el portón que la casa ya tiene dibujado, y de
él sale la entrada de auto hasta la calle. Se pierde un cuarto de 4×3 al que casi
nadie entraba; se gana que el barrio deje de tener una caja destapada por parcela.

**Y el que las tapaba a todas, que era un bug de una línea.** `kit_de_hilera`
preguntaba por el TAMAÑO de la pose `tramo` en vez de por su existencia, y
`celdas_de` cae al tamaño del slot cuando la pose falta —es lo correcto para quien
sólo quiere una medida—, así que **toda** fachada contestaba que era un kit
modular, entraba por ese camino, no encontraba `remate` que dibujar y se iba sin
dibujar nada: el barrio entero sin techos. Se ve en `barrio-2-duplex.png` antes y
después. Preguntar `has_pose` es la diferencia.

**Tres más, del mismo día, que salieron de mirar el APK en el celu.**

- **Colgarle una pose a un slot lo puede ROMPER.** Al darle la puerta animada a
  las fachadas enteras, esas 30 entradas pasaron a tener `poses` y ninguna
  representativa — y `world_sprite` busca `idle_down`/`center`/`base` y, si no
  hay, **se queda con la primera del mapa**, que en un `HashMap` es la que salga.
  O sea que la casa se dibujaba con el sprite de su propia puerta; y si le tocaba
  `opening` o `closing` (que llevan animación) ni siquiera recorta: estira la
  HOJA ENTERA de puertas sobre el lote. De ahí el "se ven todos los frames" y el
  "el exterior no cubre el interior". Todo slot al que se le agregue una pose se
  lleva también una `base` con su recorte entero.
- **Una tira de animación no se parte.** `armar_pagina` clavaba la página de
  puertas en 1024 px y `Additional_Houses_Door_2` mide 1344 (14 cuadros de 3
  celdas): `Image.paste` recorta sin avisar, así que los cuadros de `closing`
  apuntaban fuera de la imagen y salía un bloque plano encima de la fachada.
  La página mide ahora lo que mide la tira más ancha.
- **La cara exterior Y-SORTEA por su pie.** Las fachadas generadas iban a
  `layers::OVERHEAD`, que es una z PLANA. Una fachada mide catorce tiles de alto
  sobre una huella de diez, así que siempre se mete sobre el terreno del vecino de
  atrás — y con z plana, cuál tapa a cuál lo decidía el orden de spawn: aparecía
  media pared ajena flotando sobre la casa de adelante. Ahora van a
  `layers::fachada(z_del_frente)`, la banda que ya existía para las caras
  exteriores construidas, y la de más al sur va adelante como cualquier cosa del
  mundo.

### Una hilera son VARIAS viviendas, no una casa ancha — 23 ago 2026

El reporte fue una captura y una frase: *"estás colocando dos casas como si fuera
una"*. Y era exacto. La primera versión de la casa en hilera hacía un lote ancho
con **una** vivienda —un interior, una puerta— y encima le pintaba tres módulos
del pack. Desde la vereda se contaban tres casas; desde adentro, una. Eso no es un
detalle de arte: es el exterior mintiendo sobre el interior, que es justo lo que
este sistema existe para evitar.

Ahora el sim parte la hilera en viviendas de `city::ANCHO_DE_HILERA` tiles (10: el
módulo del pack, 10 celdas × 0,95 m, redondeado a la grilla). Cada tramo es una
casa entera —su medianera, su puerta al frente y otra al fondo— y el cliente le
dibuja **un módulo a cada una**. Que la constante viva en `ha-shared` y no en el
cliente es lo que hace que las dos mitades cuenten las mismas casas.

Dos cosas que costaron una corrida de tests cada una:

- **Se redondea hacia ABAJO, nunca hacia arriba.** Forzar un mínimo de dos
  viviendas hacía crecer la casa más allá de su jardín, así que su fila del frente
  —con las puertas— caía fuera del lote y la casa quedaba sellada.
- **La hilera necesita puerta en las dos PUNTAS.** Con puertas sólo al frente y al
  fondo, una calle que cruce la fila de este a oeste se come las dos: 1 sellada de
  1.743. Es la misma razón por la que la casa suelta ya tenía portal en las cuatro
  caras.

El módulo se dibuja estirado de su **paso** a una vivienda, no de su ancho: lo que
sobra es el sangrado que el pack dibujó para que se monte sobre el vecino.

### La parcela no es un cuadrado fijo — 23 ago 2026

Dos reportes del usuario, misma causa: *"el cerramiento llegó al límite de la
parcela e invadió la carretera"* y *"las parcelas varían el tamaño para dar
variedad"*.

El rectángulo del lote es `p.cells` acotado a la manzana, o sea **el mismo número
para las mil casas del barrio**, y la cerca se plantaba en su filo (`lx == 0 ||
lz == 0 || …`). Como el margen del lote sale de la calle más ancha que lo rodea
(`bounding_margin`), del lado de la calle angosta ese filo queda pegado al
asfalto — y ahí aparecía el cerramiento, cruzado en la calzada.

`jardin_de_la_parcela` mete un **retiro por lado**, distinto en cada lado y en
cada lote (0 a 3 tiles, de bits distintos del hash; el frente se retira menos,
que es la cara que da la línea del barrio). Resuelve los dos de una: la cerca
nunca nace en el filo y dos parcelas vecinas no miden lo mismo aunque el programa
sea el mismo. Lo que queda afuera es terreno natural. Se achica sólo hasta donde
la casa siga entrando.

**Y la cerca no siempre da la vuelta entera.** Una cuadra donde las mil parcelas
están cercadas igual se lee como una grilla. El borde se recorre como una recta
(`indice_de_borde`, sentido horario desde el noroeste) y la cerca es un
**intervalo** de esa recta: el largo sale de una fracción sorteada —entera, media,
un tercio, un cuarto— y el arranque se cuenta desde el frente, porque lo primero
que cerca cualquiera es la cara que da a la calle. La entera pesa más que el resto:
un barrio sin ninguna cerca completa tampoco se lee como un barrio.

Lo mira el driver **`HA_BARRIOTEST`** (`debug/autodrive.rs`): busca una casa de
cada programa de vivienda de la ciudad más cercana, retrata cada una desde su
vereda, después se para pegado a la puerta de la casa en hilera —para ver el
batiente abierto con la fachada todavía opaca— y termina adentro. Lo residencial
vive en las AFUERAS (`district_at`), así que barre hasta el borde del asentamiento:
barriendo sólo el centro te devuelve tiendas y torres, que fue lo que pasó la
primera vez.

```bash
cd native && cargo build -q -p ha-game --profile qa
timeout 480 env HA_BARRIOTEST=1 HA_AUTOPLAY=1 HA_SHOTDIR=/tmp/ha-qa \
  WGPU_BACKEND=gl ./target/qa/ha-game
```

### Las casas del mapa tienen fachada — 4 ago 2026

`building.rs` ya dibujaba la fachada encima de la casa y **la transparentaba al
entrar**; lo que faltaba era el pixel. `generated_building_art` adivinaba el arte
por el TAMAÑO y devolvía `building/casa_chica`/`casa_mediana`/`tienda`: tres
slots vacíos, así que todas las casas del mundo eran el cuadrado marrón de
respaldo.

- La clave sale ahora **del lote** (`city::roofed_rect` la devuelve junto al
  rect): el programa de la parcela ya sabía qué edificio es.
- `roof_sprite` cae al **sprite entero** cuando el slot no trae pose `Roof`. Las
  fachadas del pack son un dibujo único —techo, ventanas, toldo y puerta—, así
  que sin esa rama apuntar al arte correcto igual no se veía.
- `scripts/import-edificios.py` le asigna a cada lote la fachada de **su tema**
  que mejor calza por tamaño, midiendo, y apuntando al mismo `png`+`rect` del
  pack: no copia un pixel.

### Lo que se queda sin pixel NO se dibuja gris — 4 ago 2026

Al limpiar el catálogo (`slots-colgados.py` sacó 168 claves que eran el mismo
dibujo con otro nombre), las partidas que ya tenían colocada una de ésas
empezaron a mostrar **rectángulos grises** por todo el mundo: el quad de respaldo
de `structures.rs`. Se vio en el Redmi antes que en ningún test.

Ahora, al cargar, una estructura (o un decor autorado) cuya clave no tiene arte
**se descarta**, con una línea en el log que lo canta. Un cuadrado gris en medio
del campo es peor que no tener nada, y la pieza se puede volver a colocar con la
clave que sí existe.

### Las animaciones del personaje, sin recortar nada a mano — 4 ago 2026

`chargen_index::CHAR_ANIMS` tiene **las 20 animaciones medidas celda por celda**
(fila, cuadros, direcciones, fps) y `char_cell` da la celda exacta: es con lo que
el generador hornea `art/npc/char_XX.png`. Pero el manifest no tenía ni una pose
para esas hojas, así que `sim.rs` —que ya pedía `walk_down`, `idle_left`,
`hit_up`…— recibía `has_poses = false` y dibujaba un cuadro fijo. Las animaciones
andaban en el creador de personaje y en el juego no.

`art2d::registrar_poses_de_personaje` deriva **1.491 poses** del mismo índice al
cargar. No se inventa geometría: si mañana se rehornea la hoja, siguen calzando.

### En la calzada no va un cajero automático — 4 ago 2026

`pack_siembra::CALLE` se había generado con la misma regla que la vereda, así que
el asfalto recibía bancos, tachos, bebederos y cajeros. Ahora manda el ALTO
(`max_alto` en el importador, `scripts/calzada-plana.py` para la tabla ya
escrita): en la calzada sólo entra lo de una celda —tapas de alcantarilla,
rejillas, papeles, latas, marcas de frenada— y lo que se levanta del suelo se
queda en la vereda, que es su sitio. La regla es medible y no depende del nombre,
que en este pack no es de fiar.

### El kit modular de edificios, entero y sin cortar — 8 ago 2026

`5_Floor_Modular_Buildings_32x32.png` (32×259 celdas) es la hoja de la que sale
todo edificio de ciudad. `import-pack-masivo.py` la había recorrido por la carpeta
`*_Singles_32x32` y eso es **exactamente lo que no hay que hacer**: los "singles"
de esa carpeta son recortes de 1 celda de bandas que el artista dibujó anchas — 92
columnas de 1×3 y 31 de 1×4 en el manifest, y una pared que sale con media
ventana. Lo trae `scripts/import-modulares.py`, que lee la **hoja cruda**.

**Por islas de PIXEL, no de celda.** La primera versión de ese script segmentaba
por islas de CELDA y era el mismo error una capa más arriba: dos piezas que se
tocan en media celda quedaban pegadas, y el rótulo impreso de la hoja ("Floor-
Modular Buildings", "combine the pieces…", "BUILDING SHADOWS") se volvía un slot.
Con islas de pixel salen **337 piezas** y la gramática aparece sola.

Los tres números que sostienen todo se midieron y el script los vuelve a verificar
en cada corrida (`verificar_estructura`):

```
MODULO   =    9 celdas   el frente de un edificio modular
PERIODO  = 2432 px       lo que ocupa un TEMA completo
Y0       =  504 px       dónde arranca el primero
```

La hoja trae **tres temas** —ladrillo, azul y hormigón— dibujados uno debajo del
otro con la MISMA planta. De ahí sale el filtro que limpia todo lo demás:

> Una pieza pertenece al kit si su firma `(offset dentro del tema, x, ancho, alto)`
> aparece en **los tres temas**.

Eso barre de un saque el encabezado y la **ilustración de muestra con el
personajito** — que NO es una azotea, aunque mida 8×14,6 y lo pareciera. Y de la
misma comparación sale gratis el otro dato: si las tres copias tienen el mismo
pixel, la pieza es COMPARTIDA (un sprite); si difieren, es del tema (tres
variantes). Nadie escribe esa lista a mano.

**Las islas de 18 celdas son DOS módulos, no una banda de 18.** Miden 576 px
exactos y no repiten: la mitad izquierda y la derecha son dibujos DISTINTOS que se
tocan en el pilar del medio. Se parten al medio y a cada mitad se le recalcula su
bounding box, porque no empiezan a la misma altura (la azotea izquierda mide 9×6
justas y la derecha 9×6,25).

⚠ **`Image.getbbox()` mira SÓLO EL ALFA desde Pillow 10.** `difference(a, b)
.getbbox() is None` da "iguales" para dos paños opacos completamente distintos,
porque el alfa de la diferencia es cero. Era el patrón que usaba la función que
medía si una banda se repetía a sí misma, o sea que contestaba cualquier cosa. Se
compara con numpy. Está en tres scripts más: si alguno da un resultado que no se
entiende, mirar ahí primero.

Resultado: **54 slots · 184 sprites** en 5 páginas de 1024², la medida del resto
del pack. El pixel va **abajo y centrado** dentro de su bloque de celdas —la
convención que dejó `reparar-importados.py --solo asentar`—, así que no hace falta
repararlo después. Lo que no está en `GRAMATICA` no se exporta: se cuenta y se
lista al final de la corrida, para que nada se pierda en silencio. El guardián es
`slot_audit::el_kit_modular_mide_lo_que_dibujo_el_artista`.

**Y hay copias que la hoja dibuja PISADAS.** El cartel de carnicería y el de pesca
aparecen tres veces cada uno, y las dos copias de los costados quedan cortadas al
medio ("BUT…", "…ERY", "B… S…"). Son la misma pieza que ya se exporta entera:
sacarlas (`RECORTADAS`) es la única forma de no meter medio cartel en el juego. Se
ve en el contact sheet (`--hoja`), que **se mira antes de escribir nada**.

### La casa en HILERA: el otro kit modular — `import-casas.py` (23 ago 2026)

`24_Additional_Houses` no trae sólo casas enteras: trae un kit, y es la respuesta
del pack a *"esta misma casa, más grande o más chica"*.

```
┌──────────┬──────────┬────────────┐
│  tramo   │  tramo   │   remate   │   tramo  = 10 celdas, se repite
│ (10 c.)  │ (10 c.)  │  (12 c.)   │   remate = cierra la fila por la derecha,
└──────────┴──────────┴────────────┘             con su pared de costado
```

**Lo que estaba roto.** `import-pack-masivo.py` recorta la carpeta de singles sin
preguntarse para qué dibujó el autor cada archivo, e `import-edificios.py` le
buscaba a cada lote la fachada por TAMAÑO. Dos resultados, medidos:

- `building/casa_chica` —la casa más común del mapa— era
  `casa_base_en_terrazas_costado`: **las dos paredes de costado apiladas, sin
  techo**. El empaquetador dejó `Side_View_Base_1` y `_2` pegadas en un recorte de
  10×12 que apoya, llena su caja y pasa todos los filtros de pixel.
- Las cuatro `casa_en_terrazas_*` que le tocaban al barrio son REMATES de hilera
  plantados solos, con el techo cortado a cuchillo por la izquierda — que es por
  donde iba la casa vecina.

**Cómo quedó.** `scripts/import-casas.py` publica el kit COMO kit y `city.rs` le da
su propio programa de lote (`building/casa_hilera`). El cliente
(`building::spawn_fachada_modular`) reparte los módulos **de derecha a izquierda**:
primero el remate contra el borde de la casa —es el que trae la pared de costado—
y después tramos hasta cubrir; el último se recorta por la izquierda, que es
justo el borde que el pack ya dibuja al ras. Resultado: la fachada mide
**exactamente** lo que mide la casa, sin estirar un pixel.

Tres cosas que hubo que MEDIR y no se podían escribir:

1. **El paso entre módulos.** No es el ancho: el autor dibujó cada pieza con medio
   sangrado por lado, así que pegadas al ras dejan una ranura de una celda por la
   que se ve el pasto. `solapa()` pega las dos piezas con 0 y con 1 celda de
   solapa y se queda con la que no deja **fondo encerrado** en la costura (píxeles
   transparentes con dibujo a izquierda y derecha en la misma fila). La búsqueda
   llega hasta una celda porque el sangrado del pack es de media por lado — se ve
   en el perfil de opacidad: las columnas de las puntas están al 81–84 % y las del
   cuerpo al 88–96 %. Sin ese tope la medición se iba a dos y tres celdas
   persiguiendo el triángulo de cielo bajo el alero —que es dibujo, no ranura— y
   se comía el pilar que separa una casa de la otra.
2. **Cuánto mide un cuadro de la tira de puerta.** Las hojas animadas no separan
   los cuadros con aire. Lo primero que se probó —"el primer ancho que divida la
   tira y cuyo cuadro 0 aparezca en una casa"— está mal: `Door_2` mide 42 celdas y
   sus cuadros son de 3, pero las dos primeras celdas de un cuadro de 3 también
   contienen la puerta cerrada, así que 2 "calzaba" y la tira quedaba cortada en
   diagonal. Lo que separa el ancho bueno de sus tajadas es que **una animación es
   continua**: se elige el ancho que minimiza el salto entre cuadros vecinos.
3. **Cuál es el cuadro de "abierta".** No es el más distinto del cerrado: un
   batiente a medio abrir cruza el vano con su hoja y cambia más píxeles que uno
   abierto del todo, que es un agujero oscuro (medido en `Door_3`: del cuadro 2 al
   12 la diferencia es plana ±8 %, o sea que el máximo lo decidía el ruido). Lo
   que sí se mide es la FORMA: la tira es un palíndromo (va y vuelve), y si lo es
   —se comprueba— el vértice está en el medio.

**Y un offset en píxeles dentro del manifest, sin inventar un campo.** Las puertas
del pack no caen en la grilla de 32 (la de `Terraced_House_Modular_2` arranca en el
píxel 204). Una `Pose` ya puede declarar su propia `grid` —está para que convivan
hojas de 16 y de 32 px—, así que `puerta_en_tramo`, `puerta_en_remate`,
`paso_tramo` y `paso_remate` se publican con `grid: [1, 1]` y su rect **es** el
rect en píxeles. El cliente le resta el origen de la pieza y tiene el offset
exacto.

**La puerta se dibuja encima del paño, en el pixel donde el pack la dibujó
cerrada** (`PuertaDeFachada`), así que el cuadro 0 de la animación es idéntico al
pixel que tapa: no hay costura ni salto al abrirse. No reusa `DoorArt` porque
aquélla es la puerta de una CELDA y se dibuja alta y apoyada
(`door_render_size`) — correcto para un vano de obra, y lo que arruinaría el calce
acá.

**Y no sólo en la hilera.** Una casa entera se planta de un sprite, así que no
hay dónde meterle la puerta… salvo que el propio slot diga dónde está: la pose
`puerta_en_sprite` trae, en píxeles y desde la esquina del recorte, el sitio del
batiente, y `building::spawn_puerta_de_fachada` lo lleva a la escala con la que se
dibujó ese paño (`roof_layout` puede agrandarlo hasta 1,4×, y sin corregir por eso
la puerta cae donde estaría si la casa midiera lo que mide su pixel). Las fachadas
se buscan **por RECORTE y no por nombre**: el importador masivo bautiza automático
y el de edificios reparte alias, así que la misma casa vive bajo media docena de
claves (`building/casa_mediana#3`, `building/villa#7`…) y lo único que no miente es
a qué píxeles apunta cada una. Al 24 ago 2026: 48 claves tocadas y 40 fachadas con
puerta animada viva (Modern House, One Story, Post Apocalyptic, las Terraced y sus
alias) — ver "El ORDEN de la cadena de importadores" más abajo para por qué el
número baja de 48 a 40 y cómo se verifica. La japonesa y las
victorianas no calzan con su hoja —el pack las dibujó con otro color— y eso se
reporta en la corrida en vez de publicarse a medias.

Se abre a **3 tiles** (`PUERTA_FACHADA_DIST`) y no a 1,7 como una puerta de obra:
el techo empieza a desvanecerse en cuanto pisás la huella más un tile, así que con
el alcance corto la franja donde la puerta estaba abierta Y la fachada todavía
opaca medía una décima de tile — o sea que desde la vereda el batiente no se veía
abrir nunca.

### El pasto salía como una alfombra de puntitos — 24 ago 2026

Reporte literal: *"le pones un pasto con tanta mierda que parecen puntos… esos
forman figuritas como charquitos de huecos de césped"*, con el campo entero
empedrado de matas repetidas en grilla.

No era el arte: las losas base (`tiles/pasto_bosque`, `_seco`, `_sierra`,
`_pantano`) son **verde liso**, y la de matas es una VARIANTE (`#s1`). Era el
sorteo. En `art2d.rs`, al armar `suelo_variantes`:

```rust
if let Some(vs) = art.variant_art.get(&base) {
    uvs.extend(...);          // sólo las VARIANTES
}
if uvs.is_empty() { /* recién acá entraba la losa base */ }
```

O sea: **un material con variantes sorteaba sin la losa base**. Para el asfalto o
la vereda da igual —sus variantes son la misma baldosa con otro dibujo— pero
`pasto_bosque` tiene exactamente una variante y es la de matas, así que el 100 %
del campo salía con matas.

Arreglo: en el pasto (y sólo en el pasto, `PASTOS_DE_BIOMA`) la losa lisa entra al
sorteo repetida `PESO_PASTO_LISO = 7` veces por variante. El campo se lee sólido y
la mata aparece salteada, que es para lo que está: marcar que el suelo tiene
textura, no empedrarlo.

⚠ Si algún día el atlas publica las siete variantes de mata que `atlas-suelo.py`
contempla, el peso sigue valiendo (se multiplica por la cantidad de variantes),
pero conviene volver a mirarlo en pantalla.

### El ORDEN de la cadena de importadores — 24 ago 2026

Los importadores de `scripts/` no son independientes: **el masivo va primero y el
de casas va después del de edificios en lo que toca a las poses**. El orden bueno
es

```
import-pack-masivo.py  →  import-casas.py  →  import-edificios.py  →  podar-variantes-de-suelo.py
```

(el último desde el 27 sep 2026: el masivo agrupa por nombre y le cuelga a
`floor/vereda` 38 "variantes" que son otras baldosas —blancas, rojas, azules, con
bordes—, y la malla sortea una por celda: la vereda salía hecha un damero. El
podador MIDE cada variante contra la base —color medio y borde— y muda las que no
son la misma baldosa a `<base>_suelta#…`, fuera del sorteo.)

y saltárselo deja el árbol de arte a medias, en silencio, sin que ningún test lo
cace.

**Por qué.** Cada importador lleva su índice (`scripts/*-import-index.json`) con
las claves que publicó, y al re-correr borra las suyas viejas y las vuelve a
escribir. El masivo las escribe **enteras y peladas**:

```python
# import-pack-masivo.py::escribir_manifest
sprites[clave] = entrada        # dict nuevo: png + grid + rect, y NADA más
```

Ese `=` pisa la entrada completa, incluido el campo `poses`. Y `poses` es
justamente lo que `import-casas.py::colgar_puertas_en_fachadas` le cuelga a las
casas enteras: las cuatro poses del batiente más `puerta_en_sprite`. O sea que
**re-correr el masivo al final deja a las casas sin puerta animada**, que es
exactamente lo que pasó el 23 ago 2026 y hubo que rehacer al día siguiente.

El de edificios, en cambio, propaga en vez de pisar: sus alias por lote salen de
`sprites[destino] = dict(sprites[k])`, una copia de la clave del pack. Si casas ya
la enriqueció, la puerta viaja sola a `building/casa_mediana#3` y compañía — por
eso casas va ANTES que edificios y no al revés.

**Cómo se comprueba que quedó bien** (no hay test que lo haga; es un chequeo de
corrida):

```bash
python3 - <<'EOF'
import json
m=json.load(open('native/crates/ha-game/assets/art/manifest.json'))
c=json.load(open('scripts/casas-import-index.json'))
sp=m['sprites']
BAT=('opening','closing','open','closed','base')
print([k for k in c['poses_en'] if k in sp and not all(p in sp[k].get('poses',{}) for p in BAT)])
EOF
```

Tiene que dar lista vacía. Ojo con un falso positivo: usar `sp.get(k,{})` confunde
*ausente* con *sin poses*, y hay claves del índice que edificios elimina
legítimamente después (los alias `#n` de un slot que esta vez recibió una sola
fachada). Al 24 ago 2026: 48 claves tocadas por casas, 40 vivas con el batiente
completo y 8 borradas por edificios.

⚠ **Ningún importador escribe `city.rs`.** `import-edificios.py` lo LEE (para
`LOTES_MODULARES` y `AGRANDE_MAX`) y nada más: una corrida suya sólo puede aparecer
en el `git diff` del manifest. Si ves `city.rs` modificado después de un
importador, es un bug — ver "la fachada no llega, se INFORMA" más arriba en este §8.


### La escala del pack: una celda son 0,95 m — la mide el personaje — 8 ago 2026

El arte de LimeZu está dibujado a **32 px por celda**, y una celda del pack mide
**0,95 m** del juego (`art2d::ESCALA_ARTE`, el mismo `ESCALA_TILE` con el que
`import-pack-masivo.py` calcula las huellas). Si los dos números se separan, el
objeto se dibuja de un tamaño y frena de otro — lo cuidan los dos tests de
`art2d::tests`.

**Ese 0,95 no se elige: se divide.** El único objeto del juego cuyo tamaño real
conocemos es el personaje. Su marco es de 32 × 64 px —1 × 2 celdas— y se dibuja
de 0,95 × 1,90 m (`scene::TAMANO_JUGADOR`). 1,90 ÷ 2 = 0,95 m por celda. Cualquier
otra escala es afirmar que el artista dibujó a la gente con una proporción y a los
bancos con otra.

⚠ **Antes decía 1,5, y ese número no lo midió nadie.** El argumento era "LimeZu
dibuja a 32 px por tile de ~0,66 m", un dato del pack que nunca se comparó contra
el personaje del juego. Con 1,5 los 12.000 objetos importados entraron un **58 %
más grandes que la gente que camina entre medio**: bancos de tres metros de largo,
tachos de basura más altos que el jugador, latas de gaseosa de metro y medio. Se
veía en cualquier captura de calle (`HA_CITYTEST=1`), y es la mitad de lo que se
sentía como "los assets están mal importados". La otra mitad es el aire debajo del
dibujo — ver *Los props flotaban sobre su celda*, más abajo.

Reparar lo ya escrito no es volver a importar: `scripts/reparar-importados.py`
recalcula las huellas de `pack_siembra.rs` y `placeables.rs` **desde el manifest**,
así que es idempotente y corre en segundos.

De ahí salen las **dos** reglas de tamaño que hay, y la línea entre ellas es
*superficie* contra *objeto*:

| Qué | Con qué tamaño se dibuja | Por qué |
|---|---|---|
| **SUPERFICIE** (piso, vereda, agua, tumbado, techo modular) | exactamente 1×1 tile | es la textura de una celda: tiene que embaldosar, ni un pixel más |
| **OBJETO** (pared, puerta, ventana, mueble, cofre, auto, casa) | **el tamaño de su recorte** — `ArtLibrary::tamano_real` = px/32 × 0,95 | mide lo que mide |

⚠ **Un objeto NUNCA se dibuja más chico que su pixel** (8 ago 2026). Es regla del
usuario y no admite matices: *"los assets no los podés escalar a pequeño, usás el
tamaño real; dejá de modificar el tamaño de mis assets"*.

Hasta ese día las piezas de obra tenían su propia tabla de constantes y el
resultado era un mundo a tres escalas. Los números, porque son la queja entera:

- El arte de `wall/*` es de **1×2 celdas (32×64 px)** → su tamaño real es
  **0,95 × 1,90 m** (con la escala vieja daba 1,5 × 3,0). Se dibujaba a
  **1,0 × 2,4** (`TILE_M` × `WALL_RENDER_H`): más ancho y más alto de lo que
  mide su pixel.
- El jugador mide **1,9 m**. O sea que la pared de una casa era **más baja que el
  personaje**, y no por una ilusión de perspectiva: por una constante.
- El cofre estaba clavado en `Vec2::splat(1.1)` y la puerta de tile en
  `TILE_M × WALL_RENDER_H`. Cada sitio que dibujaba algo elegía "un tamaño que
  quedara bien".

Ahora todos pasan por `ArtLibrary::tamano_real`, que es donde vive la regla, y el
respaldo (la constante vieja) sólo entra si el slot **no tiene recorte declarado**
— el único caso en el que no hay tamaño real que respetar.

**Una pieza de obra mide su ranura, y con la escala corregida eso ya no pelea con
nada.** Con 1,5 una pared de una celda se dibujaba de 1,5 m sobre un borde de 1 m
y se montaba 0,25 m sobre cada vecina; hubo que darle a las piezas de obra su
propia regla (`medida_de_obra` = celdas × `TILE_M`) para que embaldosaran. Esa
regla sigue en pie —una pared tiene que medir su ranura exacta o abre costura— y
ahora **coincide** con la de los objetos salvo un 5 % (0,95 contra 1,00), o sea
cinco centímetros: invisible. **La colisión no se entera** en ningún caso: sigue
siendo el rectángulo fino de `WALL_THICK` sobre la arista. Lo fijan
`una_pieza_de_obra_no_se_monta_sobre_su_vecina` y
`una_pared_no_se_dibuja_mas_chica_que_su_pixel`.

Las **dos** excepciones, y las dos las pidió o las aceptó el usuario:

1. **La cara LATERAL de un muro** (`walls::GROSOR_LATERAL`). Esa cara no existe en
   el pack: se deriva estirando la columna central del muro de frente
   (`FRANJA_LATERAL`), así que no hay tamaño real que respetar. Es la *"pared de
   lado vertical que la hiciste vos"*.
2. **El antepecho** (`ALTO_MURO_CERCANO`), el muro que te queda entre la cámara y
   el cuarto. Y **no es un achicamiento, es un RECORTE**: se toma la base de la
   textura a la MISMA densidad de pixel que el muro entero. La fracción de textura
   y la de alto tienen que coincidir o sale chato — lo fija
   `el_antepecho_recorta_a_la_misma_densidad_que_el_muro`.

⚠ **El colocable NO se estira hasta su huella.** La huella es de tiles ENTEROS
—es lo que ocupa y lo que frena— y hasta el 4 ago 2026 el sprite se escalaba a
ella: una silla de 1×2 celdas, huella 2×2, se dibujaba con **el doble de ancho**.
Medido sobre la tabla entera: **2.713 de 7.710 colocables salían deformados**.
Ahora `structures.rs` dibuja con `ArtLibrary::draw_tiles` (el recorte del slot, o
el de la pose si la hay) y sigue anclando al PIE y centrado en la huella, así que
lo que se ve y lo que frena coinciden por abajo.

Las tablas de siembra (`worldgen`, `pack_siembra`) llevan tamaños fraccionarios
—salen de `celdas × 0,95`— así que ya eran proporcionales; las ocho entradas
curadas a mano que no lo eran (`prop/farol` iba 0,8×2,2 sobre un arte de 2×4
celdas) se corrigieron **conservando la dimensión grande** y sacando la otra de
la proporción del arte: se mantiene el tamaño que estaba elegido a ojo y se va el
achatamiento.

### Los props flotaban sobre su celda: el aire debajo del dibujo — 8 ago 2026

El empaquetador (`import-pack-masivo::empaquetar`) pega cada sprite en la esquina
de **arriba-izquierda** de un bloque redondeado a celdas enteras. Un objeto de
40 × 56 px cae en un bloque de 2×2 celdas (64 × 64) y queda con **24 px de aire
abajo** y 24 a la derecha. Como el mundo planta el decor con el pivote al pie —el
borde de abajo del quad es donde el objeto pisa (`world2d`)— ese aire se traduce
directo en objetos flotando sobre su propia celda y corridos hacia la izquierda.

Medido sobre el manifest entero: **8.553 sprites con el dibujo despegado del piso
de su bloque, 4.478 de ellos con 8 px o más** (hasta una celda completa). Es la
otra mitad de lo que se sentía como "los props están mal puestos".

**El arreglo mueve el pixel dentro de su bloque, no el bloque.**
`scripts/reparar-importados.py --solo asentar` recorta la tinta de cada bloque y
la vuelve a pegar **abajo del todo y centrada**. El bloque no cambia de tamaño ni
de sitio, así que no hay que reempaquetar nada ni tocar un solo `rect` del
manifest, y el aire que sobra pasa a estar ARRIBA — que para algo plantado en el
suelo es lo correcto: es aire sobre la cabeza. 7.718 sprites asentados en 112
páginas; verificado que ninguna página perdió un solo pixel opaco.

No se tocan tres familias, y por el mismo motivo: mover su pixel abriría una
costura. Lo que **embaldosa** (`floor/`, `tiles/`, `wall/`, `roof/`, `fence/`…),
lo que se **compone en piezas que encajan** (fachadas, marcos, escaleras) y lo
que tiene **varios cuadros en el bloque** (`strip`/`poses`: el rect describe uno
y los demás son sus vecinos). La lista vive en `INTOCABLES`, en el script.

### Una baldosa no es un objeto: los cuadrados verdes — 8 ago 2026

En cualquier captura de pueblo había **rectángulos verdes planos** tirados sobre
el pasto. No eran un fallback ni un error de textura: eran 29 entradas de las
tablas de siembra que **no son objetos**. Recortes de UNA celda de los autotiles
de terreno (`pasto_3`, `pasto_17`, `pasto_agua_5`, el empedrado, el piso de obra)
y props que traen el suelo pegado detrás (la pelota de playa sobre su cuadrado de
pasto, la boya sobre su cuadrado de agua). El importador los clasificó como
`nature`/`prop` porque miden lo que mide un objeto chico.

**La regla que los separa es medible y no depende del nombre**: un objeto suelto
tiene SILUETA —se recorta contra el fondo, por eso se puede plantar sobre
cualquier suelo—, y un recorte de una celda con el pixel al 100 % no la tiene. Es
superficie. Y el suelo del juego no se dibuja con sprites: lo dibuja la malla del
chunk con `suelo.png`. Vive en `reparar-importados::es_baldosa` y en el filtro
equivalente de `import-pack-masivo::elegir_siembra`, para que no vuelvan.

Se miraron los 29 uno por uno antes de echarlos: los 29 son superficie.

**Y el tamaño no tenía nada que ver — 8 ago 2026.** La regla miraba sólo los
recortes de 1×1, así que las superficies GRANDES seguían entrando: el entablado
del muelle (2×1), la ladera de arena modular (3×1), el suelo de obra, el panel de
pared de la cárcel, la puerta del faro con su muro rojo pegado atrás. Son el mismo
cuadrado plano, más grande.

El corte del multi-celda es **más duro** que el del 1×1 y eso se midió, no se
eligió por prudencia: en un recorte grande el aire de un borde pesa muy poco, así
que con 0,98 caían objetos de verdad —un banco de jardín de 3×1 da **0,9987**—
mientras que los que hay que echar dan **1,0000 clavado**, porque están dibujados
para embaldosar y llegan al borde de la celda por definición. La línea la puso el
pack; nosotros la leímos (`COBERTURA_BALDOSA_GRANDE = 0.999`).

⚠ Y se mide sobre el **bloque de celdas**, no sobre el dibujo apretado. Es la
diferencia entera: un tacho de basura tiene la silueta llena de pixel —su caja
apretada da 1,00— y lo que lo delata como objeto es el AIRE que le queda dentro de
la celda. Medir el recorte apretado echaba 216 cosas, casi todas bancos, mesas y
tachos.

⚠ `interior_siembra.rs` —los muebles de adentro— **se había quedado afuera** de la
corrección de escala: la calle ya estaba a 0,95 y los interiores seguían a 1,5,
con camas de metro y medio de alto. Son 480 tamaños. Las tablas generadas que
llevan metros son **dos**, y las dos están en `SIEMBRAS`.

### En el asfalto no crece nada — 8 ago 2026

`nature_decor_at` y `prop_decor_at` sorteaban sin mirar `civil_at`, así que la
flora de jardín del pack (girasoles, macetas, arbustos podados) y los props de
patio (reposeras, parrillas, escaleras de pileta) salían **encima de la calzada y
de la vereda**, en mitad del cruce. Las dos tablas tienen ahora la guarda: sobre
suelo urbanizado no siembra ninguna, porque lo urbano ya tiene las suyas.

Y `pack_siembra::CALLE` —la tabla que el importador arma con la regla del ALTO
justamente para la calzada— **se generaba y no la consumía nadie**. La calzada
quedaba pelada mientras la basura le entraba por la puerta de atrás. Ahora la
consume `calzada_decor_at`, al 0,8 % de las celdas de `Civil::Road`: una tapa de
alcantarilla cada tanto es un sitio, una lata cada dos pasos es un basural y una
entidad por celda que el Redmi paga por frame.

### ¿Es una PIEZA o es un OBJETO? — 8 ago 2026

**La regla, antes que nada.** Antes de colocar cualquier cosa del pack hay que
contestar dos preguntas **en este orden**:

1. **¿va sola?** ¿Es un objeto entero o una pieza de un kit?
2. **¿dónde va?** Y recién ahí, en qué sitio del mundo tiene sentido.

Saltearse la primera es lo que dejó el mundo lleno de medias cosas, y se ve de
lejos en cualquier captura: **postes de semáforo sin lámpara** en las esquinas,
**un tramo de cerca solo** en mitad de un patio, **arcos de seto** plantados a
campo abierto, regaderas y rastrillos tirados en el descampado.

**Medio pack es modular, y lo dice en el nombre del archivo:**

| lo que parece un objeto | lo que en realidad es | cómo se reconoce |
|---|---|---|
| `Traffic_Lights_Colours_Mod_2` | las tres lámparas, para pegar sobre el poste | `_Mod_` = **MOD**ULAR |
| `Fence_1_Bottom_Left` | una losa del autotile de la cerca | `Top/Bottom/Middle_Left/Middle/Right` |
| `Fence_2_Inner_Corner_3`, `Fence_2_Pass` | la esquina cóncava y el hueco del portón | `Inner_Corner`, `_Pass` |
| `Grass_Wall_1_Flowered` | un TRAMO de seto | `_Wall_` |
| `Brown_Wood_Storage_Door` / `_Roof` | la puerta y el techo del galpón | `_Door` / `_Roof` al final |
| `Traffic_Light_New_Front_Diagonal_Left_Down_Red` | un brazo del pórtico | `Diagonal` |

El filtro vive en `scripts/import-pack-masivo.py::MARCAS_MODULARES` y corre en
`elegir_siembra`, **antes** de mirar tamaños o contexto. Lo que caza **sigue
existiendo** como slot y como colocable —se puede poner a mano desde el editor,
que es donde una pieza modular tiene sentido—; lo único que no puede es salir
sorteada. Del lado del juego lo cuida el test
`worldgen::tests::ninguna_tabla_siembra_piezas_de_kit`, que audita todas las
tablas de `TABLAS_SEMBRADAS` contra las mismas marcas ya traducidas.

**Y si la pieza hace falta entera, se COMPONE.** Es lo que hace
`scripts/import-semaforos.py`: el semáforo del pack son ocho archivos, cuatro
caras del poste (`Off_Frontal_2`, `Back_2`, `Left_2`, `Right_2`) y cuatro
calcomanías de lámparas (`*_Mod_*`). Las caras van solas pero **vienen
apagadas**; el importador le pega `Colours_Mod_2` a la frontal y publica las
cuatro como `road/semaforo_{frente,atras,izquierda,derecha}`, 1×5 tiles cada
una. `worldgen::SEMAFOROS` le da una a cada esquina según a qué brazo detiene
(`roads::semaforo_at`) — no se sortea ninguna.

> Antes de esto la tabla apuntaba a los recortes crudos: `de_colores_2` (una
> calcomanía suelta: una caja de luces flotando sin poste), `atras_1` (la versión
> CORTA, 1×4, desentonando con las otras tres) y una sola esquina prendida de las
> cuatro.

**La segunda pregunta también estaba mal contestada.** `nature_decor_at` decidía
"cerca de gente" preguntando si había un asentamiento en la celda de
`SITE_PERIOD`, que mide **2.600 tiles de lado**: 6,7 km² de campo abierto
declarados jardín porque a cuatro kilómetros hay un pueblo. Ahora usa
`settlement_covering`, que mide el casco de verdad: el jardín termina donde
termina el pueblo.

### UNA CERCA ES UNA LÍNEA, NO UNA MANCHA — 15 ago 2026

El reporte fue literal: *"tenemos vallas muros verticales, no pongas horizontales
en vertical, se ve horrible"*. Y era peor de lo que suena: **los cuatro lados de
toda parcela del juego se dibujaban con el mismo recorte**.

La causa no era el arte, era **la pregunta**. `world2d::autotile_shape` contesta
*"¿de qué lado se acaba el material?"* y devuelve `edge_n`, `corner_ne`,
`inner_sw`… Eso describe el borde de un ÁREA — una mancha de tierra contra el
pasto — y está bien para lo que se inventó. A una cerca hay que preguntarle otra
cosa: **¿hacia dónde SIGUE la línea?** Como `Tile::Fence` pedía `edge_*` y ninguna
cerca tiene esas poses, `tile_sprite` caía al recorte del slot y estampaba el
tramo horizontal en los cuatro lados.

- **`ha-shared/src/cercas.rs`** es la pregunta correcta: `Vecinos` → máscara de
  conexión (`ns`, `ew`, `ne`, `nes`, `nesw`, `solo`). `cadena()` devuelve las
  poses a probar en orden, y **nunca cruza de eje**: antes un codo dibujado de
  tramo que un tramo horizontal en una línea vertical. Eso es un test
  (`un_tramo_vertical_nunca_cae_en_una_pose_horizontal`).
- **`scripts/import-cercas.py`** cura `Fence_1/2/3` y `Wall_1` de
  `1_Terrains_and_Fences`, que estaban en el manifest **atomizadas en 22 objetos
  sueltos sin etiqueta** (`fence/terreno_y_cercas_cerca_1`…`_22`): importadas, sí,
  pero sin forma de pedir "la vertical". Ahora son cuatro slots con una pose por
  forma, 58 en total.
- **El sufijo de lado (`ns@w`, `ew@s`).** El pack dibuja los dos costados de un
  cerco por separado: la pieza 4 lleva los postes contra el borde izquierdo de la
  celda y la 8 contra el derecho. Quién es cuál lo sabe el lote y nadie más
  (`city::afuera_de_cerca`): una parcela pegada a otra no deja adivinarlo mirando
  vecinos.

**Tres lecciones que costaron una vuelta cada una, y por eso están escritas:**

1. **Clasificar por costura no sirve para una cerca de estacas.** La primera
   medida automática preguntaba si el borde este de una pieza continúa en su
   propio borde oeste. Anda con los muros macizos y falla con las estacas, que no
   caen en el borde de celda. Y con los bordes vacíos da falso positivo —dos
   vacíos "coinciden" siempre—, que hacía pasar los cuatro postes sueltos de
   `Fence_1` por tramos horizontales.
2. **En 3/4 la base del sprite toca el borde sur por su propia sombra.** La
   segunda medida (componente conexa, por qué bordes sale la tinta) daba `esw` en
   un tramo horizontal puro. Como medida absoluta miente; como **verificación**
   es perfecta, y así quedó: la tabla dice la forma y la medida comprueba que
   **toda letra declarada aparezca**. Lo que sobre se perdona. Esa verificación
   cazó de entrada que las piezas 11 y 12 de `Fence_1` son TES y no tramos, que
   es como las había leído yo de la hoja de contacto.
3. **El tramo pelado tiene que existir.** El pack no dibuja el lado norte de una
   cerca horizontal —en 3/4 enseña siempre la misma cara—, así que la cadena
   `ew@n → ew → solo` se agotaba y **la mitad del perímetro seguía con el bug**.
   El importador publica `ew` como alias de `ew@s` cuando hay un solo costado.

**Cómo se mira sin levantar el juego** (un test verde con una captura fea es un
fallo): `citydump::volcar_cercas` vuelca la forma y la cadena de cada celda, y
`scripts/ver-cercas.py` compone la parcela con los recortes de verdad leyendo el
mismo `manifest.json`. Un segundo, contra quince minutos de build de Bevy.

`fence/seto` sigue sin curar a propósito: el pack lo dibuja como autotile de ÁREA
(`Grass_Fenced_1`, pasto ya cercado), que es otra cosa que una línea. Hasta que se
importe como corresponde lo dibuja `procedural_art`, que **ahora también tiene una
hoja por eje** — el respaldo no puede mentir donde el arte real dice la verdad.

### La superficie viva es PASTO; la tierra se ve al cavar — 8 ago 2026

`geography::biome_tile` devolvía `Tile::Dirt` en la sabana, en el pantano y en
toda tierra alta de un mundo templado — o sea en la mayor parte de la Tierra. Y
`tiles/dirt` **no tiene losa en el pack** (`atlas-suelo.py` lo informa como "SIN
LOSA parecida"), así que esas regiones salían de marrón plano, sin textura, de
horizonte a horizonte: el mundo se veía como un baldío.

La regla es la del pedido: **la superficie es pasto**, con el pixel del pack, y
la tierra sólo aparece donde alguien la destapó — al cavar, que deja un edit
`Dirt` y encima rebrota solo a los 45–120 s (`world2d::maybe_schedule_regrow`).
Los otros materiales (arena, hielo, roca, regolito, basalto) siguen igual: no son
tierra pelada, son otro suelo.

Los biomas no se pierden: lo que distingue una sabana de un bosque es la
DENSIDAD de lo que crece (`vegetation_density`) y su especie de árbol, no el
color del piso. Una sabana con cuatro árboles contados se lee sabana igual.

### El pasto tiene clima — 8 ago 2026

Con "la superficie viva es PASTO" el mundo dejó de ser un baldío, pero se comió
la diferencia entre biomas: la sabana, el bosque, el pantano y la sierra salían
**del mismo verde**, y lo único que los separaba era cuántos árboles crecían
encima. Ahora cada clima pide su propia losa (`world2d::pasto_de_bioma`), igual
que el relieve pide la suya (`cliff_material`): bosque, seco, pantano y sierra
tienen slot propio, y lo que no lo tiene cae al pasto de referencia — nunca queda
una celda sin dibujar.

**Lo que el pack tiene de verdad, medido.** Parece traer cuatro pastos
(`Grass_1`, `Grass_3`, `Grass_4`, `Grass_Water_*`, `Grass_Fenced_1`) y no los
trae: el RELLENO de todas esas familias es el mismo verde (71,148,86 ±2). Lo que
cambia entre ellas es la **transición** — a tierra khaki la 3, a tierra naranja
la 4, a agua las `Water` —, o sea que las "versiones" del pasto son piezas de kit
(regla 8) y sembrar una como relleno pinta cuñas verdes en cuadrícula por todo el
campo.

> ⚠ **Y eso es exactamente lo que estuvo pasando hasta el 22 ago 2026.** El
> párrafo de arriba decía que el filtro de losa (`CUADRANTES_MAX`, `BORDE_MAX`)
> las echaba. Echaba los BORDES —donde media celda es del otro material— y no las
> **esquinas internas**, que son 98 % pasto: mismo color medio, misma desviación,
> mismos cuatro cuadrantes, misma sombra por los cuatro lados. Pasaban limpias
> por los tres filtros.
>
> Las siete "variantes" de `tiles/grass` eran, píxel por píxel, las siete
> esquinas internas de `Grass_1` (`_9`…`_13`, `_17`, `_20`), cada una con su
> lengüeta de tierra naranja en un rincón. Y como `COLOR_DEL_TILE` ató a la misma
> familia a la tierra, la arena, la nieve, el regolito, el basalto y los cuatro
> pastos de bioma —justamente para heredarles esas "variantes"—, **diez
> materiales sembraban confeti naranja celda por celda**.
>
> Lo caza ahora `atlas-suelo.py::contamina`: una losa es relleno sólo si no trae
> píxeles de OTRO material, medido por [`croma`] contra los nueve rellenos que el
> pack declara (sus celdas de un solo color). El corte sale de
> `--calibrar`, no de la intuición.

**Y el pasto no tiene tres juegos de píxeles: tiene uno, y es un color plano.**
Medido sobre las cuatro carpetas de terreno celda a celda: el único relleno de
pasto del pack es `Grass_1_22` —**un único (71,151,87) en sus 1.024 píxeles**—,
repetido idéntico en nueve archivos (`Grass_2_21`, `Grass_3_22`, `Grass_4_21`,
`Grass_Fenced_1_22`, `Grass_Water_*_22/23`). La **única** variante de verdad que
existe es `Grass_Fenced_1_9` (= `Grass_Water_1_9` = `_2_9` = `_3_9`): el mismo
verde con cuatro matas, tres colores, cero costura. Entra por `pasto_bosque` y
`pasto_pantano`, que son los que apuntan a esas familias.

La variedad del campo NO sale de la losa, entonces: sale de la **siembra de
props** (`pack_siembra::BOSQUE` — `Props_Grass_8…13` y `Props_Dirt`, el 4 % de
las celdas) y del grano por celda de `tile_render_color`, que desde el mismo día
va a la mitad (±2,25 %): sobre una losa de color plano, ±4,5 % no se leía como
grano sino como **damero**, con la frontera clavada en la grilla de 32 px.

El color del clima sale de correr esa losa hacia un color que el propio pack
declara en sus paletas planas (`derivar` + `PACK_KHAKI`/`PACK_AGUA`/`PACK_SETO`).
No se mezcla pixel a pixel —a 0,8 de mezcla las matas desaparecen y queda un
rectángulo liso, que es el baldío otra vez—: se mide la media de la losa y se
**traslada** entera, así la mata verde de la pradera llega a la sabana como mata
seca y no como mancha.

| clima | slot | de dónde sale | a dónde va |
|---|---|---|---|
| pradera | `tiles/grass` | `Grass_1` | — (es la referencia) |
| bosque | `tiles/pasto_bosque` | `Grass_Fenced_1` | 0,34 hacia el verde hondo del seto |
| sabana | `tiles/pasto_seco` | `Grass_1` | 0,65 hacia el khaki del pack |
| pantano | `tiles/pasto_pantano` | `Grass_Water_*` | 0,20 hacia el celeste del agua |
| sierra | `tiles/pasto_sierra` | `Grass_1` | 0,35 hacia el khaki |

La sabana va a 0,65 y no a fondo a propósito: a 1,0 la media aterriza en
(156,144,90), que **ya es tierra**, y volveríamos al baldío. A 0,65 queda en
(135,146,89) — oro seco con las matas todavía puestas.

Montaña, playa y desierto no están porque su suelo no es pasto: son roca y arena,
y ya tienen material. Y el día que le asignes un pasto de verdad a uno de esos
slots desde HA Studio, entra ése — el código crea el slot, el pixel lo ponés vos
(regla 7).

**El costo es cero cuando no se usa.** Preguntar de qué bioma es una celda cuesta
media docena de fbm (`geography::biome_at`) y la malla del chunk lo preguntaría
en casi todas: 256 por chunk, cada vez que uno entra al streaming. Por eso
`ArtLibrary::pasto_por_bioma` se calcula una vez al cargar el arte y, si no hay
losas de clima, la malla ni pregunta.

### Una vereda por ciudad — 8 ago 2026

El pack dibujó **seis aceras enteras** y el juego usaba una: todas las ciudades
del mundo tenían la misma vereda. Ahora cada asentamiento se queda con una pareja
completa —la baldosa `floor/vereda_tipoN` (malla del chunk) y el escalón
`road/cordon_N` (calcomanía sobre el asfalto)— y salen seis materiales de verdad:
crema (215,209,200), crema cálido (217,204,186), gris frío (144,147,151), gris
neutro (179,179,179), gris azulado (142,153,167) y gris cálido (150,144,144).

**La elección es de la CIUDAD, no de la celda.** Sale del centro del asentamiento
(`world2d::vereda_de_la_ciudad`), y se resuelve **una vez por chunk**: la malla y
`spawn_chunk_cordon` reciben el mismo número, así que no pueden discrepar. Si
cada uno la resolviera por su cuenta, en el borde del casco caería el cordón de
una familia sobre la baldosa de otra — que es exactamente el motivo por el que
durante meses hubo una sola vereda.

**Lo que estaba bloqueado era la tabla de índices.** El cordón salía de doce
números leídos a mano de `Sidewalk_1`, y no servían para las otras cinco: cada
familia numera sus piezas distinto (la 1 pone las esquinas en 11–14 y el resto en
13–16). Copiarlos seis veces es el trabajo que se equivoca solo, así que ahora la
forma **se lee del pixel** (`import-calles.py::forma_del_cordon`): la acera es lo
que está más claro que el asfalto liso de su propia familia, un lado entero es un
`edge_`, dos lados adyacentes un `corner_`, y una cuña suelta en una diagonal es
una esquina cóncava (el centroide dice cuál — son veinte píxeles y una ventana de
esquina fija no los caza).

La tabla a mano no se borró: **quedó como test del detector**. Si no la reproduce
entera sobre la familia 1, el script no publica nada. Hoy la reproduce 12/12.

**Y al leer el pixel salieron dos bugs viejos que nadie había visto.**

1. **Toda ciudad tenía una alcantarilla cada 95 cm.** La tabla a mano le había
   dado a `edge_n` la pieza 19, que no es un cordón liso: es el cordón **con la
   rejilla del sumidero** dibujada en la cara. Como el autotile la usa en cada
   celda de calzada con vereda al norte, cada cuadra del mundo era una fila
   continua de rejillas. El cordón liso (la 6) estaba al lado y no se usó nunca.
   Ahora la lisa es la pose normal y la rejilla entra con **periodo de 9 celdas**
   (`world2d::CORDON_REJILLA`), que es lo que una cuneta es: un desagüe cada
   ocho metros y medio, no un canal. Es la fila "alcantarilla" de §13, resuelta.
2. **Dos esquinas redondeadas y dos cuadradas.** La tabla usaba 13 y 14
   (redondeadas) para el noroeste y el noreste, y 15 y 16 (cuadradas) para el sur:
   la misma esquina se dibujaba redonda arriba y en escuadra abajo. El pack tiene
   las cuatro de cada tipo (11–14 redondeadas, 15–18 cuadradas) y el detector se
   queda con las redondeadas completas, que es como el pack las dibujó.

   La rejilla se reconoce sola por el pixel, y **no por contraste**: los barrotes
   están pintados con cuatro puntos de brillo de diferencia (68 contra 72) y
   cualquier corte por contraste los pierde — el primer intento los perdió. Lo
   que ninguna otra pieza de la hoja hace es **cambiar de tono ocho veces a lo
   ancho de una fila**: un borde vertical cambia dos, el asfalto liso ninguna, y
   una rejilla de ocho barrotes, quince.

`Sidewalk_6` queda afuera: el pack no le dibujó el `edge_n`. Media familia no se
publica —una ciudad con un hueco en cada cordón que mira al norte se ve peor que
usar la vereda de al lado—, y el script lo dice por consola. Del lado del juego
lo cuida el filtro de "pareja completa": si falta la baldosa o falta el cordón,
esa familia no se reparte.

### La cebra es del pack, no del código — 8 ago 2026

`road/senda_ns` y `road/senda_ew` los venía dibujando `procedural_art::zebra`
(cuatro barras por código) mientras el pack traía el paso de peatones en dos
orientaciones y tres tramos, en las seis familias. Ahora entran del pack.

**Sólo los tramos del MEDIO.** Los de los extremos traen su pedazo de acera
pintado, y esa acera ya la dibuja la malla con la familia de la ciudad y el
escalón ya lo pone `road/cordon`: superponerlos metería una vereda crema dentro
de una ciudad gris, y dibujaría el cordón dos veces. Recortados a pura pintura la
cebra deja de tener familia — es cal sobre asfalto, y sirve igual en las seis
ciudades.

El recorte va contra el asfalto liso de la familia (`recortar_contra`) y después
se queda sólo con lo claro: sobrevivían cuatro píxeles del cordón vecino que
sobre la calzada se leían como una mancha sucia en la esquina de cada senda.

### La vereda no puede ser del gris de la calle — 8 ago 2026

Tres bugs encadenados en `atlas-suelo.py` hacían que la acera se dibujara con el
asfalto de la calzada. Se vieron midiendo la hoja (`art/pack/suelo.png`), no
mirando: `floor/vereda` daba crema (211,205,196) y sus tres variantes daban
(85,81,83) — **el mismo gris que `floor/asfalto`**. Tres de cada cuatro celdas de
vereda eran calle.

1. **La deduplicación le robaba las losas a la familia.** Una baldosa que el pack
   repite en varios temas se guardaba una vez y **perdía el nombre** de las
   copias, así que el filtro `familia="Sidewalk_1_"` no las encontraba. Ahora la
   losa se guarda una vez y se queda con **todos** sus nombres.
2. **Con familia atada se salteaba el corte de color**, así que el asfalto de la
   propia familia `Sidewalk_1` entraba como "variante de vereda". Ahora las
   variantes se cortan contra la losa ELEGIDA (`DELTA_VARIANTE`), no contra el
   color declarado: una variante está para que el suelo no repita, no para
   cambiar de material.
3. **La única losa lisa de 32×32 de esa vereda es una ESQUINA** (`Sidewalk_1_9`):
   trae pintada la sombra del cordón en su borde de abajo y de la derecha, y
   repetida celda a celda dibujaba una cuadrícula oscura sobre toda la acera. Las
   limpias están dentro de los bloques de 2×2 (`_25`…`_28`), que el script no
   miraba porque sólo aceptaba singles de una celda. Ahora entra celda a celda
   todo single de hasta 2×2 —más grande son ESCENAS de ejemplo, y recortarles
   celdas metía trozos de barco y de fachada de palacio donde va suelo— y
   `medir` mide además la **asimetría de sombra entre los cuatro bordes**
   (`BORDE_MAX`): una losa de relleno se ve igual por los cuatro lados, una de
   borde no. El asfalto liso baja 33 por sus cuatro lados a la vez (eso es
   viñeteado, embaldosa perfecto); `Sidewalk_1_9` baja 33 abajo y 1 arriba.

Resultado: `floor/vereda` sale de `Sidewalk_1_27` con cinco variantes, todas
crema. El cordón (`road/cordon`, `import-calles.py`) sigue siendo de la misma
familia — ése es el contrato entre los dos scripts y no cambió.

### Si soltás la animación, se dibujan TODOS los cuadros — 4 ago 2026

`world_sprite` / `world_sprite_seeded` / `tile_sprite` devuelven
`(Sprite, Option<ArtAnimated>)`, y **cuando el slot tiene tira el `Sprite` viene
sin recorte a propósito**: el recorte lo pone `ArtAnimated` cuadro a cuadro. Por
eso `.map(|(s, _)| s)` no deja el objeto quieto — deja la HOJA ENTERA, los seis
cuadros a la vez, metidos en el cuadrito del objeto.

Se caza mirando el sitio de spawn: si el `Option<ArtAnimated>` no termina en un
`ent.insert(a)`, ese slot no puede tener animación. Estaba pasando en el decor
sembrado (peceras, fuego, cardumen, cuervo: el pack los traía animados y el
mundo los plantaba congelados o mostrando la tira), en el ítem en la mano (el
farol), en las máquinas y en el cofre. Los cuatro insertan la animación ahora.

Auditoría del manifest entero (7.961 claves): **0 tiras que no entren en su
hoja** y **0 recortes fuera de la imagen** — los cuatro que había
(`interior/amplifier`, `interior/bathroom_cabinet_blue_empty`,
`tiles/floating_trunk`, `tiles/waterfall_3`, todos de arrastrar una celda de más
en HA Studio) los ajustó `slots-colgados.py`. Las únicas grillas que no son de
32×32 son las 21 hojas de personaje, que son de 32×64 por diseño (una persona
mide dos tiles).

### Baranda: el borde del piso alto (Modern Interiors) — 4 ago 2026

Los pisos altos se construyen losa a losa desde el 1 ago, y el borde quedaba a
pico: nada avisaba dónde se termina el suelo, que en cenital es una caída sin
aviso. `railing/madera` es una pieza de borde más (`EDGE_PIECES`), frena como una
ventana —se ve a través— y se dibuja a `RAILING_RENDER_H` (1,5 = una celda del
pack, a la altura del pecho) en vez de los 2,4 de una pared: si se dibujara alta
taparía el piso que rodea.

Para eso `EdgePiece` estrenó `render_h`. No toca la colisión: todo borde sigue
siendo el rectángulo fino de `WALL_THICK`.

El arte sale del mismo sistema de pisos altos de Modern Interiors
(`17_Visibile_Upstairs_System`) que ya daba las gradas.

### Lo que el pack trae PARTIDO, y lo que no — 4 ago 2026

En los packs hay objetos dibujados en varios sprites sueltos (los diez tajos
verticales de un condominio). `import-pack-masivo.py` los junta: ubica cada
single en la hoja de su tema por hash de celda y recorta el bloque entero
(`componer`). Medido en la corrida de hoy: **1.819 de los 6.221 singles de
exteriores se ubican** en su hoja, y son los que se componen.

**En Modern Interiors no se puede hacer lo mismo, y no hace falta**, medido:

- sólo el **8 %** de los singles de interiores está alineado a la grilla (92/1.131
  en la muestra de seis temas). El otro 92 % viene recortado a su silueta, que es
  la forma que tiene el pack de decir "esto ya es un objeto entero";
- y los interiores **no traen nombre de familia** (`Kitchen_Singles_32x32_137`),
  así que no hay manera de distinguir "dos piezas de un mismo mueble" de "dos
  muebles vecinos en la hoja". Componer por adyacencia fusionaría la pava con la
  mesada de al lado.

Lo mismo aplica a los recortes de `import-construccion.py`: la pared es modular
por diseño (el pack la dibuja para repetir), el piso y el techo son superficies
que TIENEN que embaldosar, y la ventana sí se compone (el paño va sobre la
pared). La única que es un trozo es la grada: la escalera del pack mide 63 px de
ancho —dos celdas, con pasamanos a los lados— y la celda que se dibuja es el
tramo de escalones, que es la parte repetible. Un tile mide un tile.

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

Sin etiquetar = **Objeto**. La escalera completa de profundidad vive en un archivo:
`ha-game/src/layers.rs`.

**Convención por namespace** (`ha-assets::namespace_layer`) — la capa sale sola
donde la intención no es ambigua, para que el bug no reaparezca con el próximo
asset que se agregue:

| namespace | capa | por qué |
|---|---|---|
| `floor/*`, `stairs/*` | Suelo | un piso nunca va encima del jugador |
| `room/*` | Suelo | es una PLANTA: el cuarto visto desde arriba, piso y paredes en la misma imagen. Y-sorteado te tapaba entero al entrar |
| `roof/*` | Techo | tiene que poder desvanecerse; en la banda Y-sort no hay forma de entrar a lo que cubre |
| `wall/*` | Pared | y de ahí sale que **frene por defecto** sin tildar slot por slot |

El resto queda sin default (→ Objeto) porque ahí la intención sí es ambigua: un
`decor/*` puede ser un arbusto o un charco, y un `agua/*` es un bote o un nenúfar
—objetos que flotan—, no la superficie del agua.

**Colisión por defecto sin huella pintada:** la da la capa. Sólo **Pared** frena;
Suelo, Techo y Objeto no. Lo que tildes en el editor gana siempre sobre esto, y
una huella pintada gana sobre todo (se consulta primero).

**Los dos efectos que se buscaban:**
- *Pasar por detrás de la parte alta de una pared*: marcá el asset **Pared** y
  pintá sólida **sólo la fila de la base** (`⟟ perfilar la base`).
- *Casa de tres pisos donde sólo se oculta el techo*: son varios assets apilados;
  los de abajo van Pared/Objeto y el techo va **Techo** → se desvanece solo
  (`layers::Overhead`, genérico, sin código por edificio).
- *Pisos como niveles reales* es el otro mecanismo: el `z` del editor de mundo
  (`▲ ▼ capa`), que cambia el nivel entero, no el orden de dibujo.

**La huella** se pinta en **Assets → huella y zona**, encima del sprite. El panel es
un editor de dibujo chico, con dos ejes separados:

- **herramienta** (cómo elegís la celda): `libre` (a mano alzada, con grosor),
  `rectángulo`, `pluma` (un clic por vértice; cierra con Enter, doble clic o clic
  sobre el primer vértice; `Esc` cancela y clic derecho quita el último) y
  `balde` (rellena la región contigua que está igual).
- **material** (qué queda en la celda): **sólido** (frena), **escalón** (se sube
  y se baja a pie — por el mismo sitio **a propósito**: uno de una sola dirección
  es una trampa), **zona** (desde dónde se interactúa; no frena) y **borrar**
  (limpia las tres). Sólido y escalón se excluyen; la zona convive con ambos.

Visor: `Ctrl`+rueda hace zoom anclado al cursor, el botón del medio o
`Shift`+arrastre desplaza, `ajustar` reencuadra, `alto` estira el visor y
`ventana grande` abre el lienzo en una ventana aparte redimensionable.
`Ctrl`+arrastre borra y el clic derecho fija el ancla.

- `detalle` = sub-celdas por tile: `tile`, `½`, `¼`. **Una huella nueva abre en
  ¼**: a un sub-celda por tile la unidad de pintura ES el cuadrado donde se
  coloca el objeto, así que no hay silueta posible (era el motivo de "sólo puedo
  pintar el cuadrado, no el sprite").
- `del sprite:` deduce la forma de los píxeles opacos — `sólido = silueta`,
  `sólido = sólo la base` (la correcta en cenital: el tronco frena, la copa se
  atraviesa) y `zona = borde de lo sólido` (el anillo de alrededor, que es
  literalmente "desde dónde lo alcanzo"). Antes de leer, ajusta la grilla al
  tamaño real del sprite y el detalle a ¼, y lo avisa.
- El tamaño en tiles sale del recorte, la tira, la pose **o del PNG** si el arte
  es un archivo suelto (la mayoría del manifest). Sin eso, una casa de 5×5 abría
  en una grilla de 1×1 con el dibujo aplastado adentro.
- El guardado a disco espera a que sueltes el botón: antes reescribía el manifest
  entero en cada frame del arrastre.
- Pintar zona marca el objeto como interactuable sola (antes se descartaba al
  guardar si no tenía verbo, o sea que pintarla no hacía nada).
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

### La fauna de la Tierra se MODELA y se renderiza a pixel art (27 sep 2026)

Los animales dibujados a mano eran "arte basura" (usuario, con razón). Ahora
`scripts/fauna3d.py` arma cada uno como un modelo de elipsoides articulado (patas
en la cadera, cabeza en el cuello) y RENDERIZA el pixel art desde él: cámara
inclinada 38°, luz del pack, cuatro bandas por material sacadas de `rampa()`,
limpieza de píxeles huérfanos y `verificar()` del corsé. Salen 3 direcciones ×
idle/walk/run/eat (+bark del perro) para vaca, gallina, oveja, cerdo, perro,
cabra, burro, conejo y pato, en `art/gen/fauna/`. **`SpeciesDef.size` es el
CUADRO en metros que imprime el script** — si no coincide, el sprite se estira.

Las páginas del pack llevan además un retoque de luz horneado ese mismo día (un
escalón de rampa del lado en sombra, +5 del lado a la luz). El script que lo hizo
se borró con el resto del 3D (§14); las páginas originales están en el commit
`c531097`.

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

#### "El servidor local no anda con la IP que usábamos" (23 ago 2026)

El servidor andaba. Lo que no andaba era **la IP horneada en el APK**.

`scripts/detect-lan-ip.mjs` devolvía *"la primera IPv4 que no sea interna"*, y ese
orden no lo decide nadie: lo devuelve el sistema como se le da la gana. En esta PC
conviven la WiFi (`192.168.1.5`) y Tailscale (`100.83.97.54`), así que según el
día el APK salía apuntando al tailnet — y entonces el celu **en la WiFi de casa**
no encontraba nada. Desde afuera se ve idéntico a "el servidor no levantó".

Ahora el orden es explícito y está escrito: **LAN de verdad** (192.168/10/172.16-31)
→ Tailscale (100.64.0.0/10) sólo si no hay LAN → `127.0.0.1`. Y se saltean las
interfaces virtuales (docker, puentes, veth, VPN de trabajo), que nunca son
alcanzables desde el teléfono.

De paso, el cartel del final ahora imprime **las dos puertas**, porque las dos
están abiertas siempre (vite escucha con `host:true` y Colyseus en `0.0.0.0`):

```
║  1. Abrí una de estas en el Chrome del celu:
║  WiFi:      http://192.168.1.5:5173
║  Tailscale: http://100.83.97.54:5173
```

El puerto **5173** (landing) y el **2567** (servidor) no cambiaron ni van a
cambiar. Si la landing no responde, antes de tocar nada:

```bash
curl -s -o /dev/null -w "%{http_code}\n" http://192.168.1.5:5173/
ss -lptn 'sport = :5173'
```

#### "Compiló pero no levantó el servidor" (10 ago 2026)

Era el puerto, y se veía como cualquier cosa menos eso. Cerrar la terminal **no**
mata a `npm run dev:server` ni a `dev:client`: la corrida siguiente compilaba el
APK entero, lanzaba el servidor, y éste moría al segundo con un
`EADDRINUSE 0.0.0.0:2567` perdido entre el banner de Colyseus. Y como los dos
hijos estaban atados con un `Promise.all(...).then(shutdown)`, el que se caía se
llevaba al hermano: sin servidor **y** sin landing, después de veinte minutos de
compilación.

Tres cosas en `scripts/play.mjs`:

1. **Se pregunta por los puertos ANTES de compilar** (`liberarPuertos`). Si 2567 o
   5173 están tomados por un `node`, es la corrida anterior: se le manda SIGTERM
   y se espera a que suelte el socket de verdad. Si el dueño no es `node`, corta
   con el nombre y el PID en vez de fallar veinte minutos después.
2. **Que se caiga uno no se lleva al otro.** El que sobrevive sigue sirviendo y el
   que murió lo dice con todas las letras.
3. **Un build de APK roto ya no aborta el comando.** Antes tiraba la excepción y
   terminaba ahí — justo cuando más falta hacen el servidor y la web para mirar
   qué pasó.

Para verlo a mano: `ss -lptn 'sport = :2567'`.

### Antes de publicar: probalo en el celu sin cable

**Publicar en la web es el plan B, no el primer paso** (regla 11 de CLAUDE.md). El
Redmi se alcanza por tailnet, así que el APK se instala y se prueba directo desde
la PC — sin cable y sin tocar el teléfono. Es más rápido que compilar → publicar →
abrir la URL en el celu → bajar 152 MB → tocar instalar → aceptar el diálogo.

```bash
leo-celu conectar   # una vez por sesión: busca en el tailnet, después en la LAN
leo-celu probar     # instala, lanza, espera 15 s, saca captura y vuelca el log
```

`probar` devuelve 0 sólo si la app quedó viva, pero **mirá siempre la captura**
(`/tmp/leo-celu-prueba.png`): un proceso vivo con la pantalla en negro sigue siendo
un fallo, y es justo el modo de falla que más se escapa.

**MIUI pelea la instalación remota, de forma intermitente.** Hay dos vías y ninguna
es confiable sola:

- `adb install` suele morir con `INSTALL_FAILED_USER_RESTRICTED: Install canceled by
  user`. Es la restricción de Xiaomi, que quiere "Instalar vía USB" habilitado y
  cuenta Mi.
- `pm install` corriendo dentro del teléfono (empujando el APK a `/data/local/tmp`
  primero) esquiva ese chequeo... a veces.

Medido el 2 ago 2026 con diez minutos de diferencia: primero funcionó `pm install` y
falló `adb install`; después al revés. Por eso `leo-celu instalar` prueba `pm
install` y cae a `adb install` si falla. Si fallan **las dos**, ahí sí publicás con
`./build-y-publicar.sh` y lo bajás a mano.

**El logcat trae los fps de verdad.** Las líneas de `BufferQueueProducer` para la
Activity del juego incluyen `fps=`, que es el framerate real del compositor:

```bash
leo-celu volcado BufferQueueProducer 20
```

En la corrida del 2 ago 2026 daba `fps=52 / 47 / 38 / 34` en el menú. Eso es una
medición del aparato, no del escritorio — ver §10 antes de sacar conclusiones.

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

### RESUELTO (27 sep 2026) — "salgo del planeta: lag y no me puedo mover"

Reporte del usuario. Medido en la compu con `HA_FLYTEST` + `HA_STAGES` y, como la
sonda `ha-sys` no cubre todos los sistemas, con **gdb como perfilador**: se lanza
el juego bajo gdb y se lo interrumpe cada 1,5 s (`kill -INT`), contando en qué
función del juego está cada hilo. Con `ptrace_scope = 1` no se puede adjuntar a un
proceso ya corriendo; hay que lanzarlo desde gdb.

**Tres causas, una misma raíz: el trazado de ríos.**

1. **El juego se CONGELABA a los 4 s de jugar.** El primer tick de ecología
   (`ECO_TICK = 4 s`) recalculaba la capacidad de carga de TODAS las regiones del
   guardado (vienen sin ella): 3.001 regiones × 9 muestras de bioma, y cada muestra
   pasa por relieve → ríos. La pila en el cuelgue:
   `EcologyWorld::cap → carrying_capacity → biome_calc → relief::cached_cell →
   rios::perfil_del_cauce → trazar_brazo → world_map::platform`. Ahora la capacidad
   se calcula en una **cola con plazo** (`fauna::PLAZO_CAPACIDAD`, 2 ms por frame);
   lo que no está listo espera y la ventana de materialización se arma cuando sus
   nueve regiones tienen capacidad.
2. **Volando, 80-110 ms por frame.** 16 de 18 muestras de gdb dentro de
   `world_map::platform` llamada por los ríos: la malla del terreno visto desde el
   aire (`LodBake`) y el disco del planeta muestrean cada decenas de metros, cada
   muestra caía en un bloque de ríos nuevo y cada fallo recorría 121 trazados. Un
   río de 3-6 celdas no se ve a esa escala. Nació **`ha_shared::grueso`**: dentro de
   `grueso::con(..)` no hay ríos y **ninguna caché por celda se escribe** (un valor
   sin río no puede quedarle al que camina por la orilla; lo cuida
   `grueso::tests::el_grueso_no_ensucia_las_caches`). Lo usan sólo el LOD, el disco
   y la capacidad de la ecología. Resultado: **update de 80-110 ms → 10-15 ms**.
3. **Los tirones al caminar** (picos de 50-98 ms) eran el mismo trazado: las
   memorias de ríos eran `thread_local!` y Bevy corre los sistemas en 4 hilos, así
   que el mismo bloque se trazaba una vez por hilo. Ahora `rios::TRAZOS` y
   `rios::BLOQUES` son compartidas (`Mutex`, se calcula fuera del candado) y más
   grandes (1024 trazos, 128 bloques); `ULTIMO` sigue por hilo, sin candado.

**Y los paneles de depuración** ("VIVARIO…", "ECO…") salían encima de los
corazones para cualquiera que jugara en creativo: ahora arrancan apagados y se
prenden con `/depurar` o `HA_DEBUG_HUD=1` (`debug::paneles_visibles`).

**Pendiente con el teléfono en la mano**: la escala de render (`render/scale.rs`)
es LA palanca para celulares más débiles —el costo del Redmi es por píxel— y
sigue apagada en Android desde el "mundo negro" del 31 jul. Hay que reactivarla
midiendo con el cable, no a ojo.

### RESUELTO (24 ago 2026) — el disco de la Tierra se horneaba MIENTRAS CAMINABAS

El reporte fue *"el juego es un puto lag, antes no era así"*. Medido en el Redmi,
en release, caminando por el pasto:

```text
ha-sys: bake_body_textures=568.6 tick_fauna=1.6 stream_tile_chunks=0.6 … (ms/frame)
ha-perf fps=1 frame_ms=804.0 … span[chk=46513 lit=600]us
```

**Dos tercios del fotograma en texturar el disco de un planeta.** Y de paso: el
chunk 0,6 ms y las luces 0,6 ms, o sea que los dos sospechosos históricos de §10
—`stream_tile_chunks` y `light2d::attach`— estaban limpios. La sonda de sistemas
lo cerró en una corrida; sin ella habríamos vuelto a optimizar el chunk.

**Causa 1: el guard de visibilidad no cubría el caso de estar parado encima.** El
arreglo del 1 ago pedía `on_screen > 0.02 && in_frame` con
`in_frame = dist < live_view_h·1,5 + r`. Para el cuerpo que PISÁS, `dist ≈ r`, así
que el test se cumple solo, siempre. El disco vive en `z = −20`, detrás de todo el
suelo: se estaba cocinando un millón de píxeles que el terreno tapa por completo.
Ahora hay techo: `on_screen > DISCO_TAPADO_SOBRE` (12 pantallas de ancho) significa
"lo estás pisando", y ahí no se hornea.

**Causa 2: el plazo de 4 ms era decorativo, porque la unidad era una FILA.** El
bucle pintaba "al menos una fila y después miro el reloj". Una fila son 512 px × 4
muestras del resolvedor de celda completo (`suelo_a_la_vista`, que hoy pasa por
biomas, relieve, ríos **y ciudades**) — **medido: ~500 ms la fila en el Redmi, ~1 ms
el píxel**. Con ese piso, el plazo no podía acotar nada. El cursor pasó a ir por
PÍXEL (`next_px`) en tandas de `PX_POR_TANDA = 32`, así que entre dos miradas al
reloj hay unos pocos píxeles y el plazo manda de verdad.

Después: `bake_body_textures = 0,0 ms` a ras del suelo, y **4,5–6,2 ms** volando,
que es el presupuesto. El disco tarda más en estar listo; el fotograma dejó de
pagarlo.

**La lección, otra vez la misma de "un presupuesto en FILAS no acota nada"**: un
plazo sólo acota si la unidad de trabajo entre dos miradas al reloj es chica. Cada
vez que una muestra del mundo se encarece —y se encarece cada vez que la sim gana
una capa: ciudades, ríos, sociedad— toda unidad de trabajo que la contenga en
bloque vuelve a romper su presupuesto. Si agregás una capa al mundo, revisá quién
la muestrea en lote.

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

### RESUELTO (16 ago 2026) — el chunk costaba caro porque `relief_cell` costaba 8 µs

Ésta era la "ganancia grande que queda" de la sección de abajo, y no estaba en
`spawn_chunk_decor`: estaba en **preguntarle al relieve**.

`relief::relief_cell` es la consulta que el cliente hace por CADA tile de CADA
chunk, y varias veces (malla, decoración, orilla, sombra, cascadas, cordón).
Medido en release (`ha-shared/tests/coste_del_relieve.rs`):

| | antes | después |
|---|---|---|
| `relief_cell` (niveles ya cacheados) | **8–10 µs** | **0,31–0,48 µs** |
| `alto_de_pared` | 0,83–1,36 µs | 0,03–0,14 µs |
| chunk de 32×32, sólo consultar el relieve | **11–26 ms** | 0,4–5,9 ms |

Once a veintiséis milisegundos **por chunk y sólo para preguntar**, contra los
16 ms que dura un fotograma entero en el Redmi. En pantalla eso no se lee como
"va lento", se lee como **mundo roto**: el anillo llega a medio armar y lo que
falta es el color de fondo. La captura `relieve-3-cumbre.png` salía vacía por
esto —no por falta de arte: `tiles/ice` tiene su losa— y el driver
`HA_RELIEVETEST` se colgaba indefinidamente al teletransportarse a la ladera.

**Las tres causas, y las tres son de estructura, no de calibración:**

1. **La caché de niveles se mapeaba por HASH y tenía 1.024 ranuras.** Su
   comentario decía "el esténcil de 5 puntos de un chunk necesita ~3 filas
   vivas", y para `relief_level` suelto es cierto. Pero `relief_cell` no es un
   esténcil de 5 puntos: llama a `pie_de_pared` (11 filas al norte) y cada una a
   `alto_de_pared` (10 al norte por `meseta_detras` + 23 al sur por
   `hueco_al_sur`) — del orden de **300 consultas por celda**, repartidas por una
   ventana de 32×80 tiles. 2.560 celdas contra 1.024 ranuras, y encima chocando
   entre ellas. Arreglo: **el índice es la posición**, no un hash — los bits bajos
   de `(tx, tz)` sobre `CACHE_LADO` = 128, así cualquier ventana de 128×128 entra
   sin un solo choque se recorra en el orden que se recorra. Sigue siendo tabla
   directa: un acceso disperso sigue costando una evaluación y nada más.
2. **`alto_de_pared` y la clasificación se recalculaban once veces cada una.**
   `relief_cell(x, z)` pregunta por el canto que tiene al norte y `relief_cell(x,
   z+1)` pregunta por el mismo. Las dos son funciones puras de
   `(cuerpo, celda, época)`: se memorizan con el mismo índice espacial.
3. **`paso_de_meseta_chica` resolvía "¿ya vi esta celda?" con `Vec::contains`**
   dentro del bucle de vecinos: hasta 18.000 comparaciones por celda, y se
   pregunta por cada celda que iba a ser pared, o sea toda la ladera de una
   montaña. Ahora el "visto" es un mapa de bits de 97×97 en la pila.

Y una cuarta, en `rios.rs`, que era **la mitad del campo de altura en frío**
(1,3 µs de 2,0): `perfil_del_cauce` recorría TODOS los tramos del bloque de
128×128. Ahora el bloque trae una **rejilla** de 8×8 sub-celdas y la consulta
mira sólo su casilla. El alcance con que se indexa es el MÁXIMO del mundo
(`ANCHO_BOCA × VALLE`) y no el de cada tramo: así la poda es **exactamente
equivalente** a recorrer la lista entera —ningún tramo descartado puede ganarle
al más cercano— y el terreno no cambia ni un tile. Además `trazo_de` devolvía su
`Vec` de ~50 tramos **clonado** en cada acierto, y armar un bloque consulta 121
cuencas: ahora es un `Rc`.

**El mundo es idéntico**: 263 pruebas de `ha-shared` en verde, incluidos los
guardianes de terreno (`the_cheap_reject_never_hides_a_cliff`,
`every_terrace_edge_has_a_pass`, `everything_below_zero_is_liquid`).

Lecciones que valen más que el arreglo:

- **Una caché mapeada por hash no sirve para un recorrido espacial.** Si el
  acceso es una ventana, el índice tiene que ser la posición.
- **El comentario de una caché envejece con quien la usa.** El de ésta describía
  correctamente a su llamador de 2026-07; para cuando `relief_cell` creció a 300
  consultas, seguía ahí diciendo que tres filas alcanzaban.
- **Un coste de generación se ve como un bug de dibujo.** Nadie reporta "el
  chunk tarda": reporta "la montaña sale vacía".

### RESUELTO (16 ago 2026) — un presupuesto en FILAS no acota nada

Encontrado con `HA_STAGES=1` mientras se buscaba por qué el driver del relieve
no terminaba nunca:

```text
ha-stages: frames=1 frame_ms=59441.1 | update=59382.8
ha-sys: bake_body_textures=59268.6 stream_tile_chunks=198.2 spawn_chunk_decor=107.9
```

**Un fotograma de 59 segundos**, y el 99,8 % en `universe::bake_body_textures`.
La cortina de carga se rinde por tiempo ("se acabó el tiempo (20 s) con 45 chunks
y 1 discos pendientes — entro igual") y el jugador entra a un mundo a medio armar:
eso, y no un error de dibujo, era la cumbre vacía de `relieve-3-cumbre.png`.

El horneado ya estaba repartido por franjas — pero el presupuesto era
`DISC_BAKE_ROWS_PER_FRAME = 8` **filas**, y una fila no es una unidad de tiempo.
Cuesta 512 píxeles × 4 muestras de terreno, y cuánto vale una muestra de terreno
cambia con el build, con la máquina y con si ese trozo de planeta tiene relieve,
río o ciudad. Ocho filas son 4,5 ms en el Redmi en release y **casi un minuto** en
depuración: el mismo número, dos órdenes de magnitud de diferencia.

Arreglo: **se mira el reloj**. `DiscBakeRows` pasa a llevar un plazo
(`DISC_BAKE_MS_PER_FRAME = 4`, y 40 detrás de la cortina) y las filas quedan sólo
de techo por tanda. Se pinta al menos una —el disco siempre avanza— y se sigue
mientras quede plazo. El mismo código da doscientas filas en un escritorio, ocho
en el teléfono y una en un build lento, y en los tres respeta el fotograma. Con
eso el driver `HA_RELIEVETEST` terminó por primera vez y dejó sus capturas.

La lección es general y aplica a los otros presupuestos del juego
(`CHUNK_BUILD_BUDGET_PER_FRAME`, `DECOR_DESPAWN_BUDGET_PER_FRAME`): **contar
unidades de trabajo sólo acota el frame si todas las unidades cuestan lo mismo**,
y en un generador procedural nunca cuestan lo mismo.

Lo que NO cambia y sigue pendiente: un disco son 512² × 4 = **un millón de
muestras de terreno**, y las muestras están a metros unas de otras, así que la
caché de celdas no las ayuda (cada tap es un fallo). Repartirlo bien evita el
tirón; no baja el trabajo. Las salidas, por orden de menos a más invasiva:
hornear en `AsyncComputeTaskPool`, bajar los 2×2 taps a 1 (el `box_blur_rgb` ya
suaviza) o bajar `DISC_TEX_N`.

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

**Reaparecido el 3 ago 2026, con el pack completo** (misma firma: `frame=55ms`,
spans de CPU en cero, `ents=5532`, `draws=106`). Al entrar los 7.821 objetos, la
siembra pasó a poblar también la vereda y la playa y el conteo de sprites subió
otra vez. Tres palancas, en este orden:

1. **Un atlas para lo sembrado.** `import-pack-masivo.py` empaqueta lo que sale
   en `pack_siembra` en páginas propias `mundo_N` (3 páginas) en vez de repartirlo
   por tema — es el "atlas común para la decoración" que este apartado pedía,
   aplicado al arte nuevo.
2. **Densidad de siembra**: vereda 6 %→3 %, playa 3,5 %→2 %.

Con las dos, el aparato pasó de **20,8 a ~37 fps** en una aldea.

**Lo que se probó y NO sirvió**: aclarar la siembra en Android (uno de cada dos
adornos). Bajó de 4.813 a 4.665 entidades —148, un 3 %— sin mover los fps, a
cambio de que el teléfono muestre la mitad de la flora que el escritorio. El
adorno chico no es de donde salen las entidades de una aldea.

⚠ **El Redmi es sprite-bound, no draw-call-bound.** 106 draw calls no son nada;
lo que pesa son las ~4.700 entidades de sprite y su extracción por frame. Antes de
optimizar acá, mirá `ents=` en `ha-hitch`, no `draws=` — y acordate de que
`ha-hitch` **sólo loguea los frames lentos**, así que su `frame=` no es el típico:
el fps de verdad sale del `BufferQueueProducer` del logcat (`leo-celu volcado`).

☐ **Sigue abierto**: 37 fps no son 60. El siguiente paso real no es aclarar la
siembra sino **sacar el adorno estático del camino de sprites** — hornearlo en la
malla del chunk o instanciarlo—, que es el mismo pendiente que este apartado ya
tenía anotado desde el 31 jul.

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
8. **Y la ciudad desde el aire tampoco se arregla muestreando — 8 ago 2026.**
   `HA_CITYTEST` a 900 m salía "casi vacío": un degradado del bioma sin retícula,
   sin veredas y sin manzanas. **No era el streaming ni un bug**: a esa altura el
   quad del satelital mide `view_h/40` redondeado a potencia de dos ⇒ **64 m**, y
   una calle mide tres tiles, así que la probabilidad de que una de las cuatro
   muestras caiga sobre asfalto es de un 5 %. El promedio se come la ciudad.
   La salida barata es preguntar **una vez por vértice** (no por muestra) si ese
   punto cae dentro de un casco urbano —`settlement_covering`, memoizado— y teñir
   el color hacia el gris de la vereda (`world2d::tinte_urbano`, 0,45 en el centro
   y fundido en el último quinto del casco). Desde un avión una ciudad ES una
   mancha gris: lo que faltaba no era resolución, era que el color supiera que ahí
   hay ciudad.

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
npm run test:native                       # ha-shared + ha-assets + ha-game — 423 al 8 ago 2026
npm run validate:arch                     # límites de arquitectura
UPDATE_CATALOG_PARITY=1 npm test          # tras tocar catálogos

# Los DOS builds, siempre: hay código gateado por cfg y uno solo no alcanza.
cd native
cargo check -p ha-game --features desktop
cargo ndk -t arm64-v8a check -p ha-game --features android
```

#### El perfil `qa`: mirar una captura sin esperar trece minutos (23 ago 2026)

Los drivers de QA se corrían con `release-native`, que es el perfil del APK:
`lto = "thin"` y `codegen-units = 1`. Tocar una línea de `ha-game` y volver a
tener binario costaba **13 minutos**, y un driver visual se itera diez veces por
sesión — o sea que el ciclo de "cambio algo y miro la captura" era de dos horas.

`native/Cargo.toml` declara ahora un tercer perfil, `qa`: el mismo `release` sin
LTO y con `codegen-units = 16`. El juego corre igual de rápido para mirar una
captura y el rebuild baja a **4 min y medio**. **Nunca se publica con este
perfil** — el APK sale de `release-native`, que no se tocó.

```bash
cd native && cargo build -q -p ha-game --profile qa
timeout 480 env HA_BARRIOTEST=1 HA_AUTOPLAY=1 HA_SHOTDIR=/tmp/ha-qa \
  WGPU_BACKEND=gl ./target/qa/ha-game
```

Y una trampa que cuesta una corrida entera: **el driver tiene que respirar antes de
salir**. `save_to_disk` escribe la captura un par de frames después de pedirla, así
que un `process::exit(0)` inmediatamente después del último `shot()` deja el
archivo sin escribir — el driver "pasa" y no hay evidencia. `HA_BARRIOTEST` espera
1,5 s.

### Mirar el mapa — el volcado ASCII de una ciudad

```bash
cargo test -p ha-shared --test citydump -- --ignored --nocapture
HA_DUMP=/tmp/ciudad.txt cargo test -p ha-shared --test citydump -- --ignored
```

No afirma nada: dibuja las cuatro ciudades autoradas en ASCII (centro, borde este
y borde norte). Es la herramienta con la que salieron los cinco fallos de §5 —
veredas de un solo lado, calles que cambiaban de ancho, la ciudad cortada en la
frontera de su celda y kilómetros de puente sobre el océano—, **ninguno de los
cuales rompía un test**. Si vas a tocar la trama urbana, volcala antes y después:
un vistazo caza lo que ningún invariante escrito de antemano ve venir.

### Auditar el mapa — el mundo ENTERO, medido por PANTALLA (16 ago 2026)

```bash
cargo test -p ha-shared --release --test auditoria_del_mapa -- --ignored --nocapture
cargo test -p ha-shared --release --test coste_del_relieve  -- --ignored --nocapture
cargo test -p ha-shared --release --test mapa_de_la_nieve    -- --ignored --nocapture
```

Tampoco afirman nada: son números para leer. Existen por **tres errores de método
que costaron caro y que conviene no repetir**, y los tres son sobre CÓMO se mide,
no sobre qué:

1. **Medir sólo alrededor del spawn.** La Tierra son 60 km de lado (±30.000
   tiles); una ventana de ±1.500 es el **0,25 % del área** y encima la parte más
   llana. Medida ahí, la Tierra da 78 % sabana, 0 % bosque, 0 % montaña y 0,03 %
   océano — y todo "pasa". Con esa medida se estuvo a punto de reescribir
   `moisture_at` por una avería que no existía: barrido el planeta entero, la
   humedad se reparte bien (8 % desierto, 30 % sabana, 47 % pradera, 15 % bosque).
2. **Promediar sobre el planeta.** El jugador no ve el planeta, ve **una
   pantalla**. Una pantalla de ladera puede ser 90 % roca mientras la media del
   mundo dice 1,45 %. Por eso la medida que manda es *cuántas ventanas del tamaño
   de la cámara salen ilegibles* — hoy, de 2.150 ventanas, **ninguna pasa del
   22 % de roca de pared** y 2.102 están por debajo del 10 %.
3. **Barrer un cuadrado sobre un mundo que es un disco.** El borde se hunde en mar
   abierto desde `d = 0,80` del radio y la latitud del clima es la distancia al
   centro, así que las esquinas del cuadrado —**el 21,5 % de las muestras**— caen
   todas en el mismo sitio: el mar polar. Eso solo ponía `Ice` en el 42 % del
   censo. `auditoria_del_mapa::dentro_del_mundo` las descarta.

Y una medida que no es del mapa sino del generador: `coste_del_relieve` mide
cuántos microsegundos cuesta cada consulta y cuántos milisegundos sale un chunk.
Es el instrumento que encontró el bug de §10 (`relief_cell` a 8-10 µs). **Un coste
de generación se ve como un bug de dibujo** — nadie reporta "el chunk tarda",
reporta "la montaña sale vacía" — así que si algo del mundo se ve a medias, este
test va antes que mirar el dibujo.

### Drivers de QA — el juego se prueba solo, en la compu

Manejan el juego por el **pipeline real de input** (ButtonInput virtual en
`PreUpdate`) y afirman invariantes; sacan capturas PNG al scratchpad. Sirven para
depurar sin el Redmi. Skill: `swarm-sim`.

**Y si un driver no termina, la sonda es `HA_STAGES=1`**, no el depurador: imprime
en qué etapa del `Main` se va el frame y `ha-sys` lo reparte por sistema. Fue lo
que cazó el fotograma de 59 segundos de §10 en una sola corrida, con el proceso
imposible de adjuntar (`ptrace_scope = 1`).

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
| `HA_BUILDTEST=1` | Levanta un cuarto por el camino real del jugador; comprueba piso, anillo de paredes, rechazo de pared sin piso y que el muro frena. Y la **verticalidad**: coloca un tramo de escalera, lo ANDA hasta cambiar de planta, comprueba el rellano, cava un pozo, se cae, y sale TREPANDO el borde — mirándolo y **pulsando** el botón, que es lo único que puede hacer el jugador. |
| `HA_FACHADATEST=1` | **La casa tiene frente.** Levanta un cuarto de 3×3, le pone la cara sur con el material de fachada (con su puerta al medio) y lo tapa con un tumbado — la combinación exacta que estaba rota. Afirma que la cara exterior se dibuje **por delante de su propia tapa** (`z_frente > z_tapa`) y que **se funda al entrar**. Deja `fachada-calle.png` y `fachada-adentro.png`: hay que MIRARLAS, una es un edificio de ladrillo y la otra un cuarto con su tumbado. ⚠ Dos lecciones de escribirlo: (1) **contá lo que colocaste**, la primera versión no puso ni una fachada —los bordes ya estaban ocupados por un `walls.json` de otra corrida— midió las paredes de madera viejas y cantó DONE; (2) el alfa se mide con el **mínimo**, no el máximo: cada pieza tiene su huella, así que de tres columnas sólo se funde la que te tapa. |
| `HA_CASATEST=1` | **La casa MODULAR**, el hermano del anterior para la otra forma de construir: el edificio se coloca ENTERO. Funda un ambiente, le apila dos plantas, le abre la puerta y lo amplía — todo por la ficha EDIFICIO y por el camino real. Afirma: el frente sale de **un módulo entero** (`modulos == (1,0)`, que es lo que impide que alguien vuelva a rebanar las bandas), el **tabique aparece solo** al ampliar, hay entrada, las bandas se dibujan de verdad (se cuentan los sprites VIVOS) y van por encima de la banda de techos. Deja `casa-calle.png` y `casa-adentro.png`. ⚠ El fundido se afirma **comparando** las dos capturas, no contra un umbral: el alfa de un sprite también lo tocan la luz ambiental y el velo de la noche, así que un número fijo mide la hora del día y no la regla 8. |
| `HA_CUEVATEST=1` | Busca una boca de cueva de verdad en el mundo generado, se planta afuera y camina hacia adentro. Exige que se entre **andando** (bajar de nivel en el primer paso sería un portal) y que el encierro suba en **gradiente**, sin saltos de más de 0.5 por medio segundo — **descontando los beats en los que se anda el tramo**, donde la luz cambia rápido a propósito (`interior::sombra_del_tramo`). Deja `cueva-1-boca.png`, la serie `cueva-baja-5..9.png` (la bajada beat a beat, que es la prueba visual del fundido), `cueva-2-entrando.png` y `cueva-3-adentro.png`. |
| `HA_RELIEVETEST=1` | **El relieve**, buscado en el mundo de verdad: una pared que mira al sur en el campo, un paso (rampa), una ladera DENTRO de un casco urbano y **la cumbre del monte más alto de la plantilla** (`world_map::cumbres`, no una coordenada a mano). Deja `relieve-0-pared.png`, `relieve-1-rampa.png`, `relieve-2-urbano.png` y `relieve-3-cumbre.png`, y hay que MIRARLAS — ninguna de estas cinco cosas la puede afirmar un assert y todas fueron bugs: (1) la celda del acantilado es TERRAZA, no un cuadrado gris pardo; (2) el de pasto se ve de pasto y el de arena de arena, no el mismo dibujo con una tira verde de cuatro píxeles arriba; (3) la rampa tiene sus dos espolones de roca; (4) la ladera urbana lleva escaleras y caños y la del campo NO; (5) la cumbre es BLANCA —suelo y acantilado del mismo blanco, sin franja verde en el medio— y se le ve el sendero subiendo, que en el dibujo es una rampa cada tantos metros y no una pared cerrada. |
| `HA_AGUATEST=1` | **El agua**, con los seis sitios buscados en el mundo de verdad (`agua_pick_sites`): laguna, río, cascada, playa, sierra y **la boca de un afluente** — la celda donde un cauce entra a un lago, que hasta el 14 ago 2026 casi no existía (25 de 36 lagunas no tenían ninguno). Deja `agua-0-laguna.png` … `agua-5-afluente.png` y hay que MIRARLAS: (1) el chorro cae DENTRO de su garganta, no al lado; (2) al pie del salto no hay un ribete de orilla cruzado bajo la poza; (3) la laguna del bosque tiene ribera de PASTO, no el panal de arena de la playa; (4) la desembocadura se lee como una desembocadura y no como dos manchas de agua que se tocan. |
| `HA_NIGHTTEST=1` | **La noche**: se planta en la ciudad más cercana, clava el reloj en medianoche y captura el mismo sitio a tres encuadres (`night-0-close`, `night-1-block`, `night-2-district`). Es el ojo de `light2d` + `night_veil.wgsl`, y desde el 28 ago 2026 también del alumbrado, las guirnaldas y la pirotecnia. Hay que MIRARLAS: (1) **entre farol y farol tiene que haber oscuridad** — si la vereda es una sábana de luz, el pozo está muy grande o el paso muy corto (fue exactamente el bug de `PASO_ALUMBRADO = 8`); (2) el pozo conserva el albedo, no lava el pixel art a crema; (3) las esquinas del cuadro las cierra la viñeta. Complementos: `HA_NIGHTPHASE=0.02` mueve la hora, **`HA_PIROTECNIA=1`** acorta la espera de los fuegos artificiales para que entren en la corrida. Y el log deja `ha-pirotecnia:` por cada cohete y `span[lit=…]` con el costo real del sistema de luz, que es como se midió que 40 luces salieran más baratas que 24. |
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

### Los mismos drivers, EN EL CELU (3 ago 2026)

La regla 11 pide probar en el Redmi antes de publicar, pero hasta acá los drivers
sólo eran alcanzables en la compu: se prenden con variables de entorno y **una app
Android no tiene entorno de shell** (la lanza `zygote`, no tu terminal). Así que
"probado en el celu" quería decir "lo toqué a mano un rato".

Ahora hay una sola puerta que sirve de los dos lados, `crate::flags::var`:

```bash
HA_FLYTEST=1 ./target/debug/ha-game     # compu: el entorno, como siempre
adb shell setprop debug.ha.flytest 1    # celu: sin root, el prefijo `debug.` alcanza
```

El nombre se traduce solo (`HA_FLYTEST` → `debug.ha.flytest`) y el entorno siempre
gana. Arnés: **`./scripts/qa-android.sh`** (todos los drivers, o los que le nombres)
— instala nada, sólo maneja properties, lanza la activity, espera, y baja log y
capturas con `adb pull`.

Dos cosas que hubo que mover para que esto funcione de verdad:

- **Las capturas.** `FLYTEST_DIR` era una constante con una ruta `/tmp` del
  escritorio; en el celu no existe y la evidencia se perdía en silencio. Ahora sale
  de `flytest_dir()`: en Android va al directorio externo propio de la app
  (`/sdcard/Android/data/com.hungerarena.app/files/ha-qa`, sin permisos, y
  `adb pull` lo baja sin root). `HA_SHOTDIR` la pisa en las dos plataformas.
- **La actuación.** Los drivers sintetizan `ButtonInput<KeyCode>`, que es una
  superficie de ESCRITORIO. En Android `desktop_keyboard_actions` (Space/F = saltar
  y armar el vuelo Ki) ni se registraba, y `touch_move_stick` corría después de
  `keyboard_input` en la cadena escribiéndole ceros encima al movimiento. El driver
  arrancaba, no movía a nadie y moría en `STUCK phase=Ground armed=false`. Con
  `flags::driver_active()` el camino de teclado se habilita en el celu **sólo si hay
  un driver**, y el táctil le cede el stick. Con una persona jugando no cambia nada.

⚠ Si agregás un driver, sumalo a `DRIVERS` en `flags.rs` o `driver_active()` no lo
va a ver y en Android no va a poder mover al personaje.

### graphify — el grafo del código

Grafo en `graphify-out/` (`graph.json`, `GRAPH_REPORT.md`). `graphify explain
"Nodo"` · `graphify path "A" "B"` · `graphify update .` (sin costo de API).

**Regla:** antes de tocar un sistema con muchas dependencias (render, marco de
mundo, cámara, cielo), consultá el grafo para ver qué depende de eso. Un refactor de
marco rompió la Luna por no ver que la cámara asumía el marco estático.

---

## §12 Troubleshooting

- **"Se cierra solo después de un rato de jugar" — REPRODUCIDO Y MEDIDO (3 ago 2026).**

  ⚠ **La entrada anterior decía "es un pánico, no falta de memoria". Era al revés.**
  Se midió en el Redmi con la app lanzada a mano, sin driver:

  ```
  10:58:44  arranca
  11:24:10  Zygote: Process 28597 exited due to signal 9 (Killed)
  11:24:11  ActivityManager: ... has died: fg TOP          ← en PRIMER PLANO
  11:24:33  lowmemorykiller: ... critical pressure and device is low on memory
  ```

  Murió a los **25 min y medio**, por **SIGKILL**, y acto seguido el sistema siguió
  matando otras apps por presión de memoria. No hubo `SIGSEGV`, ni excepción Java, ni
  `ULTIMO CIERRE:`. Es un **OOM kill**. Que no aparezca el renglón del menú no prueba
  que sea un pánico — prueba lo contrario: con `signal 9` el proceso no llega a correr
  ningún hook.

  El perfil, que es lo que hay que atacar:

  | | |
  |---|---|
  | RAM del Redmi | 3.8 GB (quedaban 56 MB libres) |
  | Native heap del juego | **492 MB reservados** |
  | …de eso, residente | sólo 36 MB |
  | …de eso, swapeado a zram | **417 MB** |
  | Tirón medido a los 17 min | **1406 ms** |

  O sea el juego **reserva ~490 MB que casi no toca**. Android los comprime a zram, y
  traerlos de vuelta es el tirón de segundo y medio. **Son dos problemas distintos y
  hay que separarlos:**

  1. **Footprint estático (~430 MB ya a los 2 min).** Es lo que más pesa. Demasiado
     para un device de 3.8 GB compartido con WhatsApp, Instagram y el resto.
  2. **Crecimiento de ~4 MB/min encima.** Ese sí es un leak, y es el que decide si
     aguanta media hora o un día.

  **Descartados con medición** (no volver a sospecharlos sin datos nuevos): entidades
  ECS (estables en ~5170), ecología (topeada en `MAX_KNOWN_REGIONS`, poda activa),
  anillo de chunks (`MAX_CHUNK_RANGE = 7` lo acota), discos planetarios
  (`DISC_TEX_N = 512` → 1 MB por cuerpo, ~13 MB en total).

  **CAUSA ENCONTRADA: el arte estaba en memoria DOS veces.**

  Bevy carga las imágenes con `MAIN_WORLD | RENDER_WORLD` por defecto: sube la textura
  a la GPU **y** se queda con los píxeles en RAM. Medido con la palanca nueva
  `HA_RAMART=1`, que suma los bytes de `Assets<Image>`:

  ```
  ha-ramart: 306 imagenes en Assets<Image> = 238.8 MB de CPU
  ```

  238 MB de píxeles que **nadie lee**. El manifest referencia 271 PNGs distintos que
  decodificados son 222 MB, y **179 MB de eso es `npc/`**: cada `npc/char_*` es una
  hoja de 1792×1312 (9 MB) de la que el slot usa **una celda de 32×64**. Se pagaban
  9 MB para dibujar 2 KB, veinte veces.

  El único que tocaba `Assets<Image>` era `setup_art_atlases`, y sólo para `img.size()`.
  Cuando el slot declara `grid`, esa geometría ya viene del manifest, así que la imagen
  se carga **sólo-GPU** (`RenderAssetUsages::RENDER_WORLD`) y la copia de CPU se suelta
  al subirla:

  ```
  antes:  306 imagenes = 238.8 MB de CPU
  después: 253 imagenes =  26.7 MB de CPU     ← -212 MB
  ```

  Los slots **sin** `grid` conservan la copia de CPU a propósito: su celda se deriva del
  ancho de la imagen y sin ella no se puede armar el atlas. Son los livianos.
  `HA_ART_CPU=1` vuelve al comportamiento viejo para todos.

  ⚠ Al agregar arte: si un slot animado no declara `grid`, se sigue guardando en RAM. En
  una hoja grande eso es caro. `setup_art_atlases` avisa por `warn!` si alguna vez cae un
  sprite sólo-GPU sin grilla.

  *Sospecha investigada y NO confirmada — no volver a darla por hecha sin medir.*
  En Bevy 0.15, `commands.entity(e).insert(...)` y `with_children(...)` sobre una
  entidad **ya despawneada** son `panic!` (B0003), y los sistemas que *decoran*
  entidades ajenas (sombra de suelo, tinte de luz, `LodHidden`, pose de fauna,
  `SquashStretch`) las sacan de una `Query`, que refleja el mundo **antes** de
  aplicar los comandos del frame; si el streamer tiró ese chunk en el mismo frame,
  el comando cae sobre un muerto. Encaja perfecto con "crashea en algún momento"…
  pero **tres soaks del tester (10, 25 y 45 min; 54 vueltas
  despegue→órbita→aterrizaje) dieron el contador en 0 en los SIETE sitios
  instrumentados** (`lod/oculto`, `luz/tinte`, `sombra/marca`, `sombra/padre`,
  `pose/fauna`, `pose/sim`, `particulas/squash`). El orden en que se aplican esos
  comandos resulta favorable, así que la familia B0003 no explica el cierre.
  De paso, esos 45 min descartan otras dos hipótesis: el censo de entidades se
  queda clavado en ~5 000 (no hay fuga) y los fps no se degradan con el tiempo
  (varían con la escena, no con los minutos).

  *Dónde NO se buscó todavía.* El tester vuela, cava y construye, pero **nunca
  abre la mochila, ni craftea, ni usa un cofre, ni habla con un NPC** — y ahí es
  donde vive el resto de los indexados (`state.grid[i]`, `ui.hotbar[idx]`). Es la
  superficie que un jugador de verdad pisa a cada rato y el soak no. Primer lugar
  donde mirar si el reporte del teléfono no lo dice solo.
  Lo que quedó igual: `try_insert` en todos los sitios que tocan entidades ajenas y
  `render::try_add_child` en vez de `with_children` — seguro barato, coste cero.
  **Regla que sí vale: para tocar una entidad que no spawneaste vos en ese mismo
  sistema, `try_insert` y `render::try_add_child`.** `remove` y `despawn` ya
  toleran la ausencia.

  *Cómo diagnosticarlo la próxima vez.* Cada cierre se anexa a `ha-crashes.log`
  (en `/data/data/com.hungerarena.app/cache/`), que **no se borra nunca solo**, y
  los últimos tres se leen **dentro del juego, en el panel de AJUSTES** — hace
  falta porque con el APK por URL nadie tiene un cable para `adb pull`. El
  `ha-last-crash.json` sigue siendo el "recién pasó" que el menú consume y borra.
  El contador `ha-ecs muerto quien=<sitio> n=N` dice si alguna decoración estuvo a
  punto de reventar; en 0 para todos los sitios, el pánico viene de otro lado.
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
- **Cajas `□` en el HUD.** Un carácter que la fuente embarcada no trae (emoji,
  `▲`, `←`). Los acentos ya NO son el problema. Correr
  `cargo test -p ha-game la_fuente_dibuja` para ver cuál es y en qué línea.
- **Dumps de save en la raíz del repo** (`inventory.json`, `tiles.json`, `society/`,
  `vivarium/`, `ha-shot-*.png`): son de correr una herramienta desde la raíz en vez
  de `native/`. El save real vive en `native/`. Están gitignoreados; se borran.
- **"No se siente que subo" / "la cámara se aleja de golpe al volar".** Son la MISMA
  cuenta mal, por las dos puntas: el encuadre es `view_h_for_alt(cuerpo, altura) ×
  user_mult` (§7). Si el suelo no se aleja, alguien desconectó la altura; si pega un
  tirón, alguien desató `climb_rate` de `NEAR_ZOOM_RATE` o sacó la rampa de empuje.
  `cargo test -p ha-game el_despegue_no_se_aleja_de_golpe` mide las dos cosas —techo
  y piso del ritmo— sin abrir el juego. Para verlo:
  `timeout 90 env HA_FLYTEST=1 HA_AUTOPLAY=1 WGPU_BACKEND=gl ./target/debug/ha-game`
  y mirar las capturas: la altura y el `view_h` del log tienen que ir casi 1:1 abajo.
  ⚠ Si el driver reporta un salto justo después de una etapa que **fuerza** una
  altura, mirá primero si no es el propio driver: un teleport de ×100 hace que la
  cámara camine a su tope varios segundos, y eso no es un salto del juego
  (`GRACIA_INYECCION_S`, `encuadre_asentado`).

---

## §13 Pendientes

### ▶ PLAN PARA RETOMAR (escrito el 27 sep 2026, al cortar la sesión)

Estado: commit `67bb624` + los cambios de abajo marcados ✅ (ver `git log`). APK
**1.0.378** publicado (Tailscale `http://100.83.97.54:5173`) con todo hasta
`67bb624`, sin probar en el Redmi (no estaba conectado). Referencias visuales del
usuario en `design/referencias/` (memoria `referencias-visuales`).

**Hecho en esta sesión (verificado en la compu):** congelamiento a los 4 s por la
ecología (cola con plazo); vuelo de 80-110 → 10-15 ms (`ha_shared::grueso`);
tirones al caminar (memorias de ríos compartidas); sombra por esquina (sin
rectángulos grises); vereda/asfalto sin damero (`podar-variantes-de-suelo.py`);
paneles de depuración apagados (`/depurar`); botón CAM al lado de MENU;
✅ CASATEST pasa (la cáscara se abre entera: `Overhead::pie` + recinto en
`layers::fade_overhead`); ✅ tope de dt del vuelo 1/20 → 1/10 (un celu lento
volaba en cámara lenta). Estos dos últimos están commiteados pero NO en el APK.

**Pendiente, POR ETAPAS (orden del usuario, 27 sep 2026 noche):**

- **E1 · CALLES Y CIUDAD** — ✅ primera pasada hecha (27-28 sep 2026):
  interior de manzana = pasto de parcela (`topdown_tile`), sin fleco en la
  vereda (`geography::urban_civil`), ningún lago en un casco
  (`relief::rellenar_pueblo` + `world_map::pond_in`, guardián
  `grueso::ciudad_sin_lagos`), y listas blancas en las tablas de siembra
  (`import-pack-masivo.py` "solo": vereda = mobiliario urbano, calzada = piso de
  calle, parque = bancos/flores/fuentes, patio sin kit de pileta). Falta
  revisar con el usuario: lotes (qué edificio va en cada parcela) y esquinas.
  Antes: Una calle es, de afuera hacia adentro:
  parcela → vereda (con su ancho) → cordón/filo contra la calzada (o contra la
  alcantarilla) → calzada → línea central. Más las esquinas. Hoy "están todas
  rotas", y DENTRO de las parcelas de ciudad aparecen lagos, piezas sueltas y
  props repartidos al azar: sospecha de importación mal interpretada del pack
  (regla 8: pieza vs objeto). Ver §5/§8 (`pack_siembra.rs`, `roads.rs`,
  `city.rs`, `import-calles.py`, `import-marcas-calzada.py`).
- **E2 · ESQUINAS DE ACANTILADO** — ✅ hecho (28 sep 2026): en una pared de más
  de un escalón `pared_apilada` armaba las esquinas con las filas de la pared
  RECTA. Ahora `corner_se/sw` usan `labio_/cuerpo_/pie_{se,sw}`, que
  `scripts/derivar-esquinas-de-acantilado.py` parte del rect medido de la pieza
  1×3 (correrlo después de `import-acantilado.py`). Antes: en cada ángulo de 90° la pieza de la
  esquina sale mal y el borde superior (donde se pisa) no es continuo. Kit en
  `world2d.rs` (`corner_*`, `inner_*`, `edge_*`, `CARAS_QUE_CUELGAN`) e
  `import-acantilado.py`. Referencia: `design/referencias/terrazas-y-cascadas.png`.
- **E3 · LUZ DEL COLOR DE SU FUENTE** — ✅ hecho (28 sep 2026), falta verlo de
  noche con pantalla real (en Xvfb la noche no carga): `import-emisiones.py`
  pondera el color por croma (antes ganaba el núcleo blanco) y una llama sin
  color medible toma el ámbar `LLAMA`; en `night_veil.wgsl` el pozo y el halo
  suman el color saturado (potencia 1,6), el núcleo puede quemar a blanco.
  Antes: la antorcha es amarilla → su luz, amarilla;
  el farol blanco → blanca. `emission` del manifest / `light2d.rs`
  (`Light2d.color`). Referencia: `iluminacion-noche.png` (brillos neón,
  gradiente luz→oscuridad).
- **E4 · INVENTARIO** — ✅ hecho (28 sep 2026): la grilla de la mochila es flex
  con wrap y ancho exacto (con `Display::Grid` salía en una fila de rayitas) y
  las casillas no se encogen; la hotbar usa el mismo marco del kit (lo pisaba
  `ui/hotbar_slot` en `skin_ui_from_art`) sobre una base oscura. Verificado con
  `HA_BAGSHOT`. Antes: la grilla de la mochila sale como líneas en vez de
  casillas, y la hotbar no tiene el fondo oscuro que tiene el inventario
  (`hud.rs`, `casillas_del_kit`, `marcos_del_kit`).
- **E5 · PLANETAS EN PIXEL ART** — ✅ primera pasada (28 sep 2026):
  `universe::pixelar_disco` baja el disco a 128 px, terminador (luz de arriba a
  la derecha), cráteres en cuerpos sin aire, luz en 5 bandas, muestreo nearest.
  `cargo test -p ha-game --release --lib hornear_discos -- --ignored` deja los
  discos de TODOS los cuerpos en `HA_DISCO_DIR`. La cara de noche
  (`BodyNightSide`, gira con el Sol) usa `NightTexture` en bandas y el Sol su
  `SunTexture` pixelada; el horneado sólo pone el volumen simétrico. **Queda**:
  el anillo gris del borde (el "fin del mundo" caminable pintado como agua/roca)
  y Urano/Neptuno salen verdes como planetas de tierra en vez de gaseosos con
  bandas (lo decide la sim del terreno de cada cuerpo). Antes: los cuerpos vistos de lejos en el estilo de
  `luna-pixel-art.png` (mares, cráteres, terminador), no el disco actual.
- **E6 · FACHADATEST**: al entrar al cuarto construido se abren la cara sur pero
  no las norte/laterales (`layers::fade_overhead`, `walls::huella_que_tapa`).
- **E7 · Terrazas como la referencia** (cantos rectos ≥4 celdas; hoy escaleras de
  1 celda, ver "EL PASO NO USA EL KIT" más abajo) y **agua en la nieve** (pasto
  verde en cuadros junto a cascadas nevadas).
- **E8 · Redmi**: probar el APK (`leo-celu conectar` → `probar`), reactivar la
  escala de render en Android midiéndola.
- **E9 · Resto**: forraje (ECOTEST, cuadrados rojos planos), mapa del mundo como
  `distribucion-del-mapa.png`, fauna de otros planetas, crías, cultivos.

**Cómo se mide** (lo que funcionó): drivers `HA_<X>TEST=1 HA_AUTOPLAY=1
HA_FORCE_TOUCH=1 HA_SHOTDIR=… WGPU_BACKEND=gl ./target/release/ha-game`; sonda
`HA_STAGES=1`; y gdb como perfilador (lanzar el juego DESDE gdb e interrumpirlo
con `kill -INT` cada 1,5 s). En escritorio el tester queda encerrado en las
paredes que construyó en corridas viejas: no es bug del juego.


### El canal urbano: el río que cruza el pueblo ya lleva muro (15 ago 2026)

`1_Terrains_and_Fences` trae **tres familias `Wall_*`, 58 piezas**, y no las usaba
nadie: la 1 es el coronamiento de piedra visto en cenital (autotile) y la 2/3 el
paramento de ladrillo en tres alturas. Son exactamente el canal de las referencias
de LimeZu — el agua encajonada entre dos muros en vez de un arroyo de orilla de
pasto cruzando el centro.

- `scripts/import-muelle.py` publica el coronamiento como `quay/muro`. La pose
  sale **del pixel**, con la misma idea que el cordón: el LABIO (banda oscura) más
  el alfa son "el borde", y qué franjas ocupa dice si es `edge_*` o `corner_*`.
  Salen las ocho de `Wall_1_{1,4,5,8,9,10,11,12}`.
- **Cóncavas no hay.** Se buscaron: las piezas 13–20 son diagonales de media
  celda, no muescas. El cliente cae al borde recto del lado que corresponda —un
  rincón resuelto con un tramo recto no llama la atención; uno sin dibujar sí.
- `world2d::spawn_chunk_muelle` lo siembra sobre la celda de TIERRA pegada al
  agua, mirando al agua, **sólo dentro de un casco**
  (`geography::settlement_covering`, preguntado una vez por chunk). Fuera del
  pueblo el río sigue con su orilla de pasto, que es lo que le corresponde a un
  río de campo — y así el muro no cuesta una consulta en los millones de celdas
  de costa del planeta. Sobre la calzada tampoco: ahí el río se cruza por puente.

**El pack está importado ENTERO** — se midió el 15 ago 2026 con
`import-pack-masivo.py --dry-run`: 11.691 singles → 10.918 dibujos distintos (348
repetidos) → 7.821 objetos → 12.057 entradas de manifest. Lo que falta nunca es el
pixel: es el sistema que lo pide.

### El barrio residencial dejó de ser cajas de color (15 ago 2026)

Era lo más grande que quedaba, y no era falta de arte: el pack tiene 215 casas
sueltas (`24_Additional_Houses`) y 194 villas (`7_Villas`), y **estaban
importadas**. Lo que fallaba era la cadena de cuatro eslabones que va del pack a
la parcela, y los cuatro estaban rotos a la vez:

1. **`import-edificios.py` emparejaba por PREFIJO de clave** (`building/casas_…`).
   Cuando el importador masivo se volvió idempotente dejó de anteponer el tema, y
   ese prefijo pasó a emparejar **1 clave de 215**. Con un candidato,
   `casa_chica`, `casa_mediana`, `casa_grande` y `casa_country` recibían las
   cuatro la misma villa, y otros veinte lotes se quedaban sin fachada. Ahora el
   tema es la **PÁGINA de atlas**, que sale de la carpeta del pack y no de cómo se
   bautizó nada.
2. **Medía con `ESCALA_TILE = 1,5`** — el valor viejo, elegido a ojo — mientras el
   juego dibuja con 0,95. Cada fachada se medía un 58 % más grande de lo que se
   iba a ver, así que siempre elegía edificios demasiado chicos. El test
   `el_importador_usa_la_misma_escala` sólo miraba `import-pack-masivo.py`; ahora
   mira todos.
3. **El filtro del pie descartaba 23 de las 32 casas.** Pedía que la última fila
   del RECTÁNGULO estuviera llena, y una fachada en 3/4 casi nunca lo está (hay
   un cantero, un sendero, un farol que baja más que la pared). Se cambió por
   "¿sigue siendo ancho abajo?": el quinto inferior contra la fila más ancha del
   propio dibujo. Las 12 casas dan 0,66–1,00 y el toldo suelto 0,07.
4. **Se comparaba contra la PARCELA, no contra la casa.** La casa ocupa el 55 %
   del ancho del lote, así que para un lote de 16×12 la casa mide 8×6 — y se
   estaba eligiendo la fachada para 16×12. Encima el cliente ESTIRABA el sprite a
   la huella del suelo, aplastando un dibujo en 3/4 (que es techo **más** pared
   de frente) a la mitad de su alto: casas derretidas. Ahora la fachada se dibuja
   a su propia proporción y **apoya en el frente**, creciendo hacia arriba
   (`building::roof_layout`), con tope de ×1,4 para no ampliar el pixel.

Y encima de eso, lo que faltaba para que fuera un barrio: **cada slot se lleva
hasta doce fachadas distintas** como variantes y la parcela elige la suya por su
esquina (`variant_key_seeded`). Las parcelas residenciales se agrandaron para que
una casa del pack entre entera (una casa de LimeZu mide de 9 a 23 tiles de
frente) y `building/villa_6` —que era el nombre que el importador masivo le puso
a un recorte de UNA CELDA— se cambió por `building/villa`.

Lo cuida `art2d::ningun_lote_de_la_ciudad_se_dibuja_como_una_caja`: comprueba que
cada clave de `LOT_PROGRAMS` tenga pixel **y** que ese pixel mida como un
edificio. Ese guardián no existía —`ningun_slot_se_queda_sin_pixel` mira el
catálogo de slots, y los lotes apuntan a claves del pack, que no están ahí— y por
eso el agujero estuvo abierto sin que nada fallara.

### La calle: lo que quedó del pack sin consumir (8 ago 2026)

La vereda de ciudad ya entró con su **cordón** — el autotile de 12 poses de
`road/cordon`, recortado contra la losa lisa de su propia familia para que
sobreviva el relieve (bisel, cara del cordón y sombra de contacto). Y desde el
8 ago 2026 la BALDOSA es de verdad la de la vereda: sale del interior de los
bloques `Sidewalk_1_25…28` y ya no del asfalto de la misma familia (§8, "La
vereda no puede ser del gris de la calle"). Lo que el pack trae y todavía no usa
nadie:

**Cuatro de las cinco filas de esta tabla ya están hechas** (9 ago 2026, ver §8).
Y tres de ellas decían algo que no era: los tamaños se habían anotado de un
listado, sin abrir las piezas. Abiertas una por una, esto es lo que el pack tiene:

| Pieza | Qué es de verdad | Estado |
|---|---|---|
| Familias `Sidewalk_2…6` | seis aceras enteras, baldosa + cordón | ✅ **una vereda por ciudad** (§8). `Sidewalk_6` queda afuera: el pack no le dibujó el `edge_n` |
| `Sidewalk_N_29…36` (64×32 y 32×64) | **la cebra**, en dos orientaciones y tres tramos — no la alcantarilla, que es lo que decía esta tabla | ✅ entran los tramos del medio, recortados a pintura (§8) |
| Alcantarilla / cuneta | la **rejilla del sumidero**, dibujada en la cara del cordón norte (`Sidewalk_N_19`). No es una pieza aparte: es una variante de `edge_n` — y ya estaba en uso, sólo que en TODAS las celdas | ✅ ahora entra con periodo (§8) |
| Esquinas redondeadas | tampoco son de 2×2: `Sidewalk_1_11…14` son las esquinas **redondeadas** de 32×32 y `15…16` las cuadradas. La tabla a mano mezclaba dos de cada | ✅ el detector elige las cuatro redondeadas, que es lo que el pack dibujó como juego (§8) |
| `Sidewalk_N_37…41`, `42…46`, `48…54` | **isletas y bahías pintadas**: un círculo de 4×4, una isleta de 6×4 partida en tres, estacionamiento (`P`, 3×5 y 5×2) y parada de bus (`BUS`, 3×6, y el poste con su plataforma, 3×4) | ✅ la bahía de BUS y el cajón `P`, 15 ago 2026 (ver abajo). ☐ la isleta y el círculo |

#### La bahía de BUS y el cajón `P` — resueltos midiendo la calzada (15 ago 2026)

Esta fila decía que el bloqueo **no era de arte** sino de sitio: que una bahía de
3×6 no se podía sortear por celda sin dejar una `P` cruzada en mitad de un cruce,
y que hacía falta **H3, plantillas de ciudad** — un sitio autorado donde el
generador supiera que hay lugar. Resultó que no hacía falta autorar nada: **el
sitio ya está en la geometría de la calzada y sólo había que medirlo**, que es lo
mismo que ya hacían el eje de la línea divisoria (`es_eje_con`) y el cruce
(`es_cruce_con`).

- **El arte** lo recorta `scripts/import-marcas-calzada.py`, y **no tiene ni un
  número escrito a ojo**: la bahía se encuentra por sus manchas de amarillo vial,
  el cajón por su glifo `P` más el PASO entre dos `P` vecinas, y el fondo del
  banco por sus líneas blancas. Salen 3×6 y 7×3 la bahía, 3×5 y 5×3 el cajón. El
  script se planta si eso no coincide con lo que declara `roads.rs`.
- **Cada tile es una pose** (`ns_2_4` = columna 2, fila 4), recortada a
  calcomanía contra el asfalto liso igual que `road/linea`, así que debajo se
  sigue viendo la variante de asfalto de la celda.
- **La ubicación** la decide `roads::marca_rect_con`: la marca se pega al cordón,
  tiene que dejar libre al menos `ANCHO_MINIMO_LINEA` de calzada (con eso salen
  solas en las avenidas y no en las calles de manzana), se ancla cada
  `PASO_MARCA` tiles y se sortea.
- **Todo se decide en el ANCLA, no en la celda.** Es la parte que hay que
  entender: `calzada_seguida` se topa con `ALCANCE_EJE` y cerca de una esquina la
  calzada "sigue" hacia la calle que cruza, así que dos celdas del mismo
  rectángulo medían anchos distintos y la marca salía con agujeros. Lo cazó
  `las_marcas_de_calzada_van_sobre_el_asfalto_y_salen_enteras` con un cajón de 5
  de 15 casillas y una bahía de 3 de 18.

Lo que sigue pendiente de esta fila es la **isleta** y el **círculo del
retorno**: ésos sí van en el eje de la avenida, no contra el cordón, y su sitio
todavía no está medido.

### Mundo — relieve y agua (14 ago 2026)

- ~~**El río todavía no BAJA.**~~ **RESUELTO** — primero por el trazado
  (`rios.rs`, 14 ago 2026: el 78 % de los cauces llega al mar o a un lago) y
  después por la talla (21 ago 2026: el cauce va por debajo de su ribera en el
  96 % de las celdas de sierra, contra el 84 % de antes, y el 21,75 % de cauce
  cortado por acantilados bajó al 4,12 %). Detalle y números en §5, *"El río se
  talla en escalones"*.
- **Las 16 lagunas sin afluente** son correctas por diseño ("no todos los ríos
  terminan en lago") pero no están elegidas: son las que el entramado no alcanzó.
  Si algún día molesta, el sitio es `world_map::sumidero_cercano`.
- **Las 2 caras con el agua pegada al pie** (de 3.666) — ver §5. Son paredes altas
  donde el dibujo se poda contra un canto de más abajo.
- **La cumbre quedó LIMPIA, y le falta la piedra.** Sacar `decor/rock` de la nieve
  arregló los lunares verdes y dejó la meseta de la cima sin nada encima. Lo que
  hace falta es una copia TEÑIDA de la roca —el patrón que ya usan
  `agua/cascada#N_<material>` y `cliff/<material>`: misma pieza, otra paleta, sin
  recortarle nada—, o que el usuario le asigne pixel a `decor/rock_moon`. Con
  cualquiera de las dos, `rock_permille` de `Snow`/`Ice` vuelve a 30 y la cumbre
  recupera su piedra asomando.
- **EL ACANTILADO ES UNA MONTAÑA DE ARENA TEÑIDA, y el pack tiene el talud de
  pasto sin usar** (21 ago 2026). Reporte del usuario: *"no olvides los bordes de
  las terrazas, se ven horribles y ajenos al terreno; la idea es que todo el
  terreno esté en armonía"*.

  El tinte ya no es el problema —el labio del muro lleva el material del terreno
  que corona y el gris de su celda (§5, "el anillo de la meseta")— y desde hoy la
  roca del pasto es piedra y no mostaza. Lo que sigue siendo ajeno es **la
  FORMA**: el único kit de relieve que el juego importa es `Sand_Mountain`, o sea
  una montaña de PLAYA con canto rodado grueso. Teñida de verde sigue leyéndose
  como muro de castillo apoyado en el pasto.

  Y el pack tiene lo otro: la familia **`Mound_1`/`Mound_2`** de
  `1_Terrains_and_Fences` — nueve piezas de **talud de tierra con pasto asomando
  por el canto y matas al pie**, con bordes en diagonal y cuerpo repetible. No
  está importada ni una. `import-acantilado.py` las nombra y las descarta como
  *"montículos de un tile"*; miradas, `Mound_2` tiene cuerpo y costados, que es lo
  que hace falta para vestir un desnivel.

  Es un importador nuevo más el mapeo del autotile de 13 formas, y la decisión de
  qué material usa cuál: el talud de tierra para pasto y bosque, y la roca del
  `Sand_Mountain` donde de verdad hay roca (cumbre, sierra alta, basalto).

- **EL PASO NO USA EL KIT DEL PACK, y para usarlo hay que ENDEREZAR EL CANTO**
  (21 ago 2026). Pedido del usuario, mirando la lámina del pack: *"usar las
  rampas completas de forma modular; no sé por qué recortás sólo un pedazo y usás
  eso de rampa"*, y *"en morado los assets que unen acantilados y rampas"*.

  Hecho ya: el kit **está importado entero** (`paso/<material>`, 12 piezas — boca,
  tramo repetible, pie, los pasos que bajan de lado y los cuatro enlaces
  diagonales), en su propia hoja `art/pack/paso.png` de 1.024×1.120 sin una celda
  de aire. Falta **colocarlo**, y ahí está el problema de fondo, medido:

  | medición | resultado |
  |---|---|
  | `medir_acantilados::ancho_de_los_pasos` | **88,9 %** de los pasos mide 1 celda; 3,4 % llega a 4 |
  | `medir_acantilados::corridas_rectas_de_canto` | **86,2 %** de TODOS los cantos mide 1 celda recta; 4,4 % llega a 4 |

  O sea que **los bordes de terraza son escaleras diagonales, no líneas**, y el
  kit del pack mide 4 celdas de ancho: no se apoya en el 95,6 % de los cantos del
  mundo. Por eso el juego terminó autotileando celda a celda — es lo único que
  viste un borde que zigzaguea.

  Ensanchar el paso **no alcanza** y está probado: ensanchando a lo largo del
  canto se pasó de 88,9 % a 89,4 % de pasos de una celda, con un 22 % más de
  celdas de paso a cambio de nada. Se revirtió.

  Lo que falta es que **el paso talle una muesca RECTA de cuatro celdas al mismo
  nivel**, que es literalmente lo que el kit dibuja (`Four_Sand_Mountains_1_Top`
  ES una muesca en la meseta). Eso es trabajo de generación —`relief::es_paso` y
  la cuantización del campo—, no de dibujo, y mueve el mundo: los tests de
  caminar hasta la cumbre, de caminos y de manzanas van a moverse con él.

- **El muro de terraza se ve REPETITIVO**, y en una ladera empinada se ve peor:
  cada celda de canto cuelga su pared, así que un borde largo es la misma pieza
  del kit estampada tile a tile, y donde el terreno baja un escalón POR TILE
  —islas chicas, conos— sale un damero de bloques de roca. Se ve en
  `agua-4-sierra.png` (14 ago 2026). No es del agua ni de los montes nuevos: es el
  kit de acantilado desde que existe. Lo que le falta es variación por celda
  (`variant_key_seeded` ya existe y el kit no la usa) y una poda que, cuando el
  desnivel baja más rápido que el dibujo, deje UN muro alto en vez de N muros de
  una fila.
- **La ladera urbana del driver de relieve cae al fallback.** `relieve_pick_sites`
  no encontró cara sur dentro de un casco y capturó (0,0), así que
  `relieve-2-urbano.png` no enseña lo que promete. Es del driver, no del mundo.
- **La poda de la era esférica.** `planet_surface.rs`, `luna_terrain.rs`,
  `surface_{chunk,biome,locomotion,climate,deposits,flora}.rs`,
  `luna_locomotion.rs` y el stub `parkour.rs` son el sistema de altura del mundo
  3D: **cero consumidores en `ha-game`**, pero enredados entre ellos y con
  `movement.rs`, `world.rs`, `blocks.rs`, `flight_physics.rs` y `vessel.rs` dentro
  de `ha-shared`, así que el compilador los da por vivos. Es una poda aparte, con
  su propia verificación en los dos targets (§13, deuda conocida).

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
| C8 | **El edificio de ciudad se coloca ENTERO** (§5): kit modular importado sin cortar, `ha-shared/edificio.rs` (planta, tramos, tabiques automáticos, lista única de aberturas), ficha EDIFICIO de la mochila y dibujo de la cáscara. Driver `HA_CASATEST`. | ✅ 8 ago 2026 |

**Lo que le falta a C8**, en orden, y ninguna es urgente:

1. **El adentro.** La cáscara es la elevación de la calle; cruzar la puerta te
   deja sobre el suelo que había. Falta que fundar un ambiente estampe su piso y
   sus paredes de interior — la lista de aberturas ya está lista para eso, es de
   donde tienen que salir las dos caras.
2. **Costados y espalda.** El pack no los dibuja y no se inventan; hoy los pone
   `walls` con `fachada_*`. Si algún día molesta, la salida es derivar una tira
   fina del propio paño (memoria: el arte se recorta de lo que ya está asignado,
   no se le pide otra pose al usuario).
3. **Que la ciudad generada use el kit.** Hoy lo usa lo que colocás vos;
   `geography::building_in_block` sigue estampando la casa vieja por tiles.
4. **Las tiendas y los carteles.** Entraron al manifest (34 slots) y no los
   consume nadie todavía.

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

### Arte y 3D (27 sep 2026)
- **Cultivos por etapas** (plantita → cosecha), como la lámina de referencia que
  pasó el usuario. No hay sistema de cultivo: es mecánica nueva (sembrar,
  crecer, cosechar) más el arte de cada etapa.
- **Más fauna**: variantes de color (vaca overa colorada, cerdo manchado), crías,
  gallo, y la fauna de los otros planetas, que sigue siendo quad de color.

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
- Escalas por asset editables desde el editor.
- Audio: sonidos por bioma, pájaros/viento ambientales, variación día/noche.
- Panel UI 9-slice: el editor lo arma pero el HUD todavía no lo consume.
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

### Convertir el pixel art a 3D (27 sep 2026) — NO

Se intentó pasar los 14.000 assets a modelos 3D automáticamente (inflado de
silueta y después vóxeles por columna: tapa + frente) y un prototipo de motor 3D
en tercera persona. El usuario los vio y los descartó: *"una basura, una imagen
puesta sobre un bloque"*. **El motivo es de fondo**: un dibujo 3/4 no contiene
los costados ni el dorso; cualquier conversión los INVENTA, y el resultado nunca
es "el mismo objeto, sólo que en 3D". Si algún día el juego va a 3D, el arte
tiene que nacer 3D (un pack low-poly coherente), no convertirse. Todo se borró
(modelos, scripts de vóxeles, visor, rama `motor-3d`); quedaron los PNG del
juego y `scripts/fauna3d.py`, que usa un modelo sólo como paso intermedio para
renderizar pixel art.

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

**Portales / pantallas de carga** para cuevas y pisos, y **el swap de capa que los
reemplazó**: `Tile::StairsUp/StairsDown`, `layer_transition`, `LayerLatch`,
`carve_stair_room`, `shaft_exit`. Un tile con el poder de cambiar de planta
teletransporta por suave que sea el fundido. Reemplazados por la escalera que se
camina (§5, 4 ago 2026).

**Apagar el día entero al bajar un nivel.** Era el interruptor "estás en una
cueva", y sus dos bordes eran el corte. Reemplazado por el gradiente de encierro
(§5).

**React Three Fiber, Expo, Tauri, `apps/mobile`, `apps/desktop`,
`packages/client-core`, `packages/client-gfx`.** El cliente de juego es `native/`.

**El walk-hub orbital caminable** y los **cielos por escena**.

### Legado dormido — no borrar, no referenciar en contenido nuevo

Sistemas de la versión combate/OASIS que siguen en el repo por reuso futuro
(fauna/peligro): clases scout/warrior/mage, habilidades, oleadas survival, AI
hostiles (`packages/shared/src/sim/`), schema Colyseus. No son parte del concepto
actual pero no se borran.
