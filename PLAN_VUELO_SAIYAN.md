# PLAN_VUELO_SAIYAN — Eliminar el programa espacial, volar como Saiyan

> **Estado: IMPLEMENTADO + VERIFICADO 2026-07-26 (Ki con coste, aprobado).**
> Compila verde (0 errores); tests `ha-shared` 104/104. **Drivers de vuelo en
> desktop VERDES:** `HA_FLYTEST` (despegue→órbita→abort mid-descent→aterrizaje,
> ZOOM CONTINUITY + LANDING INVARIANTS PASS) y `HA_MOONTEST` (órbita lunar CLARA,
> sin white-square loop) → EXIT 0. **Cámara arreglada:** `view_h` constante toda
> la subida/órbita/bajada (ya NO se auto-aleja) y el pinch funciona en vuelo.
> Balance de Ki afinado (drain 1.0/s = ~100 s de vuelo; antes 3/s te tiraba a los
> ~25 s). Pendiente: 60 fps en el Redmi antes de shippear.
>
> **Qué se hizo:** Fase 1 `rocket_flight.rs`→`flight.rs` (`RocketFlight`→`Flight`
> espejo pasivo, física de cohete eliminada, helpers orbitales conservados) ·
> Fase 2 borrados `ship_builder.rs`/`launch_pad.rs`/`ship_interior.rs` + máquina
> LAUNCH_PAD + panel SHIP/cockpit del HUD · Fase 3 `creative_fly.rs` es el vuelo
> por defecto (mantené JUMP para cargar y despegar; Ki con coste; escribe el
> espejo `Flight`) · Fase 4 aura Saiyan (`PlayerAura`, slot `fx/aura_saiyan`),
> carga con SFX de aura, barra de **Ki** en el HUD, telemetría de vuelo Ki ·
> Fase 5 drivers QA adaptados (despegue por JUMP), docs (`PLAN_MAESTRO.md`,
> `labiblia.md`), memoria. El universo, sondas (nav computer), efemérides y la
> coreografía de transición quedaron **intactos**.
>
> **Decisiones §5:** (1) **Ki con coste** — aprobado por el usuario. (2)
> `nav_computer`/sondas **conservado** (viaje interplanetario coexiste con volar).
> (3) Interior de nave **eliminado** sin reemplazo por ahora.

## 0. Objetivo (lo que pediste)

- **Eliminar todo el "programa espacial"**: cohetes, constructor de naves,
  plataforma de lanzamiento, interior de nave, física SFS de propulsión, mezcla
  de masa/combustible/empuje.
- **Conservar el universo y los viajes interplanetarios**: cuerpos celestes,
  efemérides, el mapa orbital, la coreografía superficie↔espacio, la red de
  sondas / planificación de tránsitos (`nav_computer`).
- **Volar como Dragon Ball**: el **personaje** despega, vuela por el sistema
  solar, y aterriza por sí mismo — con **aura Saiyan** mientras carga/vuela.
  Sin naves, sin combustible: es Ki.

Esto **no reintroduce 3D** ni physics orbital real (biblia §1). Es el mismo
plano 2D SFS, pero el que sube es el personaje con aura en vez de un cohete.

---

## 1. La clave arquitectónica (por qué esto es limpio, no una reescritura)

`creative_fly.rs` **ya es el vuelo Saiyan** en germen: su propio doc dice *"the
CHARACTER (not a rocket) flies through the orbital view with a CONTINUOUS,
PLAYER-DRIVEN trajectory"*, con fases **Ground → Climb → Orbit → Descend**
(= despegar, volar, aterrizar), god-mode (sin órbitas, sin combustible, sin
gravedad), activado hoy por el botón **CREATIVO**. Reusa la coreografía
compartida (`ha_shared::transition`) — la MISMA que el cohete.

El nudo a desatar: **`RocketFlight` no es "la nave"**. Es la bandera universal
*"¿estoy volando?"* + estado de render que leen ~15 módulos:

```
camera, sim, hud, world2d, audio, survival_stats, world_persist,
input/{desktop,gamepad}, debug/{autodrive,perf}, machines
```
vía `.active`, `.blocks_walking()`, `.altitude_m()`, `.in_vacuum()`,
`.render_pos_2d()`, `.render_rot_2d()`. Y `rocket_flight.rs` también aloja
helpers **orbitales** que son "universo", no "nave":
`launch_pos_vel[_from]`, `orbital_start_above_pad`, `surface_x_to_angle`,
`escape_alt`. Esos deben sobrevivir.

**Decisión (recomendada):** un solo recurso de vuelo. Renombrar/depurar
`RocketFlight` → **`Flight`** (estado de vuelo del personaje), conservando su
superficie pública (`active`, `altitude_m`, `blocks_walking`, `render_pos_2d`…)
para que los 15 lectores casi no cambien. `creative_fly.rs` se vuelve el ÚNICO
controlador de vuelo y lo maneja. Se borran de él los campos de nave
(fuel, thrust, drag, `ShipLoadout`, `capacity/burn_rate/thrust_rate`).

Alternativa descartada: fusionar todo en `CreativeFlight` y migrar los 15
lectores — más limpio en papel, mucho más churn y riesgo. Si preferís esa, avisá.

---

## 2. Qué se ELIMINA vs qué se CONSERVA

| Área | Archivo | Acción |
|---|---|---|
| Constructor de naves | `ha-game/ship_builder.rs`, `ha-shared/ship_mass.rs` | **Borrar** |
| Plataforma / ignición | `ha-game/launch_pad.rs`, `ha-shared/launch_pad.rs` | **Borrar** |
| Interior de nave | `ha-game/ship_interior.rs` | **Borrar** |
| Física SFS del cohete | `ha-game/rocket_flight.rs` | **Depurar** → `flight.rs` (solo estado + helpers orbitales) |
| Loadout / vessel-nave | `ShipLoadout`, `vessel.rs::PlayerShip` | **Borrar / neutralizar** |
| Constantes de propulsión | `flight_physics.rs` `ROCKET_*`, `ship_config_*` | **Podar** las de nave |
| — | — | — |
| Universo (cuerpos, efemérides) | `celestial_bodies`, `solar_ephemeris`, `cosmic_scale`, `solar_system_clock` | **Conservar** |
| Mapa orbital | `orbital2d.rs` | **Conservar** |
| Coreografía superficie↔espacio | `transition.rs` (game + shared) | **Conservar** |
| Viaje interplanetario | `nav_computer.rs`, `nav_data.rs` | **Conservar** (renombrar "pad"→"punto de partida") |
| Vigilancia | `camera_feed.rs`, `sightings.rs` | **Conservar** |
| Helpers orbitales | `launch_pos_vel`, `orbital_start_above_pad`, `surface_x_to_angle`, `escape_alt` | **Mover** de `rocket_flight` a `flight.rs`/`orbital2d.rs` |

**Contenido survival intacto** (biblia §6): máquinas, base lunar, colonos, mundo
Terraria, fauna, sociedad — nada de eso se toca.

---

## 3. Fases de ejecución

### Fase 1 — Aislar y renombrar el estado de vuelo (sin romper nada)
1. Renombrar `rocket_flight.rs` → `flight.rs`; `RocketFlight` → `Flight`,
   `RocketFlightPlugin` → `FlightPlugin`, `tick_rocket_flight` → `tick_flight`.
   Ajustar los ~15 imports con un rename mecánico.
2. Mover los helpers orbitales (`launch_pos_vel*`, `orbital_start_above_pad`,
   `surface_x_to_angle`, `angle_to_surface_x`, `escape_alt`) intactos.
3. Podar de `Flight` los campos/métodos de nave: `fuel`, `thrust`, `drag`,
   `ShipLoadout`, `capacity()`, `burn_rate_factor()`, `thrust_rate_factor()`,
   `drag_rate_factor()`, `fuel_pct()`. **Compila verde** tras esta fase.

### Fase 2 — Borrar el programa espacial
1. Eliminar `ship_builder.rs`, `launch_pad.rs`, `ship_interior.rs` (game) y
   `ship_mass.rs`, `launch_pad.rs` (shared); quitar sus `mod`/`Plugin` en
   `lib.rs` y sus registros.
2. Neutralizar `vessel.rs::PlayerShip` (queda `OrbitalStation`/`LunaOutpost` para
   telemetría; el "PlayerShip" ya no existe como nave).
3. Limpiar HUD: quitar paneles de combustible/loadout/ignición
   (`hud.rs:817,1080`, botones de pad). Sustituir por medidor de **Ki**.
4. Podar `flight_physics.rs` de constantes `ROCKET_*`. **Compila verde.**

### Fase 3 — El personaje vuela (promover creative_fly)
1. `creative_fly.rs` deja de ser "modo debug CREATIVO" y pasa a ser el vuelo
   normal. El botón **CREATIVO** → **VOLAR** (o gesto de despegue). Un salto
   sostenido / carga inicia el despegue; USE desciende; soltar = hover.
   (Ya está casi todo: Ground/Climb/Orbit/Descend + transición compartida.)
2. Conectar `CreativeFlight` para que escriba el recurso `Flight` (mismo estado
   que hoy escribe el cohete): así cámara/sim/hud/audio siguen funcionando sin
   cambios de lógica, solo cambia quién es el piloto.
3. Autodrive/QA (`debug/autodrive.rs`, skill `swarm-sim`): reemplazar los
   `launch_pos_vel` del cohete por el arranque de vuelo del personaje para que
   los tests de despegue/vuelo/aterrizaje sigan verdes.

### Fase 4 — Aura Saiyan + Ki (lo nuevo)
1. **Aura VFX**: sprite/partículas alrededor del personaje mientras vuela o
   carga. Usa el sistema data-driven de assets (slot lógico `fx/aura_saiyan`
   en `art2d.rs`; el usuario texturiza con su slicer — **no generar PNGs**,
   ver memoria "asset-workflow-slots-not-textures"). Pulso/parpadeo procedural
   por código; capas: aura base + destellos. Color según carga.
2. **Carga (charge-up)**: mantener un botón acumula Ki (aura crece, temblor,
   SFX procedural tipo "power-up" vía `audio.rs` — ver memoria
   "audio-system-procedural"). La carga da el impulso de despegue y velocidad.
3. **Ki como recurso** (sustituye combustible): barra en HUD. Volar consume Ki
   lento; cargar lo repone; en superficie se regenera. Sin naves, sin repostar:
   es autosuficiente (encaja con god-mode actual, pero con un coste suave para
   que "cargar" tenga sentido). Reglas en `ha-shared` (mirror determinista) si
   el server las necesita; si es puramente cliente/cosmético, queda en game.
4. **Reuso EVA/RCS futuro**: mantener modulares los controles (memoria
   "future-eva-rcs-reuse-creative-controls").

### Fase 5 — Limpieza, biblia, ship
1. `cargo check` + `cargo clippy` verdes; correr `swarm-sim` (despegue/vuelo/
   aterrizaje/interplanetario) en desktop con capturas.
2. Actualizar biblia (`labiblia.md`) y `PLAN_MAESTRO.md`: marcar el programa de
   naves como retirado y el vuelo Saiyan como vigente (memoria
   "labiblia-outdated-vs-code").
3. Regenerar catálogo de assets si se añadieron slots.
4. **Validar 60 FPS en el Redmi** antes de shippear (memorias
   "pending-redmi-perf-tests", "dev-machine-gpu-nvk"). Ship con
   `./build-y-publicar.sh` (memoria "self-serve-build-no-tokens").

---

## 4. Riesgos / anti-regresión

- **B8 (biblia):** las transiciones son cambios de frame, nunca cutscenes — el
  vuelo Saiyan mantiene input y cámara vivos siempre. No tocar el retune de
  bandas de transición (memoria "atmosphere-wide-proportions").
- **No unificar zoom superficie vs órbita** (memoria
  "camera-zoom-surface-vs-orbit-split"): superficie = auto-zoom, órbita = pinch.
- **Persistencia:** `nav_data.json` (exploración) debe sobrevivir aunque
  desaparezca la nave — ya está desacoplado, verificarlo.
- **APK:** revisar que no queden `.so` rancios con el código de naves
  (memoria "apk-fat-stale-v7a-so").
- **Fuente ASCII-only** para cualquier texto de HUD nuevo (medidor Ki).

---

## 5. Preguntas abiertas (para tu aprobación)

1. **Ki con coste** (cargar tiene sentido) **vs god-mode puro** (volar gratis,
   aura solo cosmética). Propongo Ki con coste suave.
2. **`nav_computer`/sondas**: ¿se conserva tal cual (mandás sondas + planificás
   tránsitos) o ahora viajás vos volando a otros planetas y las sondas quedan
   solo para vigilancia remota? Propongo conservarlo y que el vuelo Saiyan sea
   otra forma de llegar.
3. **Interior de nave**: se borra. ¿Querés algo que lo reemplace (una "base"
   habitable) o simplemente desaparece?

---

**No ejecuto nada hasta tu OK.** Decime si aprobás el enfoque (o cambiás algo de
la §5) y arranco por la Fase 1.
