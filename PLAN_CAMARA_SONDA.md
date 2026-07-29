# PLAN — Cámara-sonda (F7 del vivario)

> Sub-plan de `PLAN_VIVARIO_VIDA_EMERGENTE.md` §F7. Ver otro planeta EN REMOTO con una
> cámara que es **una pieza más de la nave**. Reusa: `ship_builder` (piezas), la sonda de
> `nav_computer` (`probe` + `LaunchProbe`), `rocket_flight`, y la **activación por
> cuerpo/fuente de F2** (`fauna.rs::manage_vivarium`, `society.rs::manage_society`).

## 0. Visión (usuario, 2026-07-25)

La cámara es **una pieza de cohete**: armás un Voyager/rover, le ponés la pieza cámara + una
**antena**, lo mandás a cualquier planeta, y con antena **te conectás desde la Tierra (o
donde estés)** y ves renderizado lo que la cámara enfoca allá (fauna/nativos/entorno del
cuerpo remoto). Es gameplay: explorás sin ir en persona; la antena habilita el enlace.

## 1. Idea rectora: la cámara es una 2da FUENTE DE ACTIVACIÓN

Hoy la fauna residente (F2) y la sociedad (F6) se **instancian/simulan sólo en el cuerpo
donde está el JUGADOR**. La cámara-sonda agrega una **segunda "presencia"**: un punto en un
cuerpo REMOTO que también activa (simula + renderiza) la vida cercana. Conectarte = la
cámara pasa a "foco de render" y la pantalla muestra ese cuerpo en vez del tuyo.

## 2. Piezas (ship_builder) y estado

- **Pieza `camera`**: parte de nave (como las otras piezas del builder). Una sonda con
  cámara puede lanzarse a un cuerpo (reusa `probe`/`LaunchProbe` de `nav_computer`).
- **Pieza `antena`**: habilita el ENLACE remoto. Sin antena, la sonda llega pero no
  transmite (sólo deja nav data, como hoy). Con antena → conectable.
- **Recurso `Probes`**: sondas desplegadas `{ body, x, z, has_camera, has_antenna }`,
  persistente (como nav data). Ya hay lanzamiento de sondas → extender su payload.

## 3. Fases

- **F7.1 — Piezas + payload**: agregar piezas `camera`/`antena` al `ship_builder`; al lanzar
  una sonda, registrar su payload (cámara/antena) y cuerpo destino en un recurso persistente
  `DeployedProbes`.
- **F7.2 — Enlace/vista remota**: UI "Conectar cámara" (lista de sondas con antena); al
  conectar, la **cámara sigue a la sonda** en el cuerpo remoto y se **activa** la vida de
  ese cuerpo (fauna F2 + sociedad F6) alrededor del punto de la sonda — reusar la activación
  cambiando la "fuente" de jugador→sonda. Desconectar vuelve a tu cuerpo.
- **F7.3 — Señal/limitaciones**: latencia/estática por distancia, energía/uptime de la
  antena, campo de visión acotado (una ventana, no todo el planeta) → sabor "telemetría".
- **F7.4 — Descubrimiento**: lo que ves por cámara alimenta el "codex" del planeta (fauna,
  temperamento, nativos) → prepararte antes de ir en persona (loop de exploración de F5).

## 4. Reto técnico clave

La activación de F2/F6 asume **una sola fuente = el jugador** (`active_body`). Para la
cámara hay que generalizar a **N fuentes de activación** (jugador + sondas conectadas): cada
fuente instancia la vida de SU cuerpo alrededor de SU punto. Refactor acotado de
`manage_vivarium`/`manage_society` a "activación por fuentes". Empezar con **1 cámara activa
a la vez** (conectás a una) para no explotar el costo.

## 5. Estado

- [x] F7 v2 (**modelo correcto: cámara de vigilancia colocada en un PUNTO**) — reescrito.
  La cámara se COLOCA en un spot (creativo/en persona; la versión "pieza de nave + antena"
  reusa este mismo feed). No muestra el planeta entero: muestra **su área** (radio
  `CAM_VIEW_R=22 m`) de forma **VISUAL** (puntos de los seres reales, no telemetría).
  - `sightings.rs`: `WorldSightings` (avistamientos reales por cuerpo). Productores:
    `fauna.rs::update_fauna_sightings` (residentes: remotos desde `records`, actuales desde
    entidades vivas) y `society.rs::update_npc_sightings` (NPCs, color por reputación).
  - `camera_feed.rs`: `PlacedCameras` (persiste `cameras.json`), placement por paleta
    creativa ("Camara"), botón **CAM** → ventana visual de los seres cerca del punto
    (dejás animales/NPCs en un lugar y los ves de remoto). Movimiento leve para verse vivo.
  - Datos/telemetría → **sólo creativo** (overlays de vivario/sociedad ya lo eran). Compila
    verde. Pendiente Redmi.
- [x] F7.2b (v1) — cámara REMOTA por sonda: cada cuerpo mapeado con `probe` (antena
  implícita) obtiene una cámara automática donde haya vida (`auto_cameras_from_probes`) →
  ves un planeta remoto sin ir. Falta la pieza EXPLÍCITA en `ship_builder` + antena como
  parte + dejarla orbitando el Sol a voluntad. Compila verde.
- [ ] F7.3b — feed en vivo REAL (simular los seres alrededor de la cámara aunque estés lejos:
  activación multi-fuente jugador+cámara). Hoy remoto = posiciones del último guardado.
- [ ] F7.3 — señal/limitaciones (latencia/estática/uptime de antena, FOV acotado).
- [ ] F7.4 — codex/descubrimiento (lo visto por cámara prepara la ida en persona).
</content>
