# Auditoría + Plan Maestro — Agencia Espacial Survival

> **Fecha:** 2026-07-11 · **Estado:** EJECUTADO (mismo día)
>
> **Progreso:** ✅ F0 (working tree commiteado, tests verdes) · ✅ F1 survival core (hambre/sed/O₂, comer, respawn amable — biblia §12) · ✅ F2 ship builder (piezas + BAG→SHIP + stats en vivo) · ✅ F3 base lunar (taladro regolito, antena, monitoreo, máquinas por cuerpo) · ✅ F4 gamepad completo · ⚠️ F5 perf: estático hecho; **medición 60 FPS bloqueada hasta conectar el Redmi** (plan en memoria del agente) · F6 APK publicado vía `ship:android`.
> Nota de orden: F4 (gamepad) se ejecutó antes que la fase de perf, sin impacto.
> Visión de referencia: `SPACE_SURVIVAL_VISION.md` · builder: `SHIP_BUILDER_DESIGN.md` · movimiento: `PLAYER_WORLD_DESIGN.md`
> Pedido del usuario: juego 3D Android (Redmi 10+, 60 FPS objetivo), mezcla KSP/SFS (arcade, sin física pesada)
> + survival Zomboid/Minecraft/Terraria, 1ª/3ª persona, controles móvil/PC/mando, documentación exhaustiva.

---

## 1. Veredicto ejecutivo

**El juego ya existe y está avanzado (~18.500 líneas en `ha-game` + sim compartida `ha-shared`).**
No hay que crear nada desde cero: hay que **cerrar 5 huecos** y **verificar perf en el Redmi**.

| Pilar del pedido | Estado real |
|---|---|
| Cohetes: construir y volar | 🟡 Vuelo completo con masa/fuel real; **ensamblaje del jugador pendiente** (diseñado en `SHIP_BUILDER_DESIGN.md`) |
| Salir del planeta, espacio, otros planetas | ✅ Tierra → espacio → Luna → planetas; efemérides solares, nav computer, aterrizaje |
| Astronauta 1ª/3ª persona con toggle | ✅ Ciclo FP → 3ª atrás → 3ª frente (`camera.rs:71`) |
| Survival Minecraft/Terraria | 🟡 Bloques voxel ✅, harvest ✅, craft ✅, máquinas ✅ — **falta hambre/comida/oxígeno** |
| Survival Zomboid (presión, necesidades) | ❌ Solo HP + stamina; sin barras de necesidad |
| Controles móvil/PC | ✅ Touch (2 sticks + contextuales) + teclado/ratón |
| Controles mando (gamepad) | ❌ No implementado |
| 60 FPS Redmi 10 | ❓ **Sin verificar** — tiers de calidad existen (`quality.rs`), pruebas en dispositivo pendientes |

---

## 2. Auditoría — lo que YA está construido

### 2.1 Mundo y planetas
| Sistema | Archivo(s) | Estado |
|---|---|---|
| Tierra esférica radio 1500, marco único | `earth_world.rs`, fase 1 continuidad hecha | ✅ (fases 2 retune + 3 visual pendientes) |
| Luna: terreno, locomoción baja gravedad, sky-shell | `moon_world.rs`, `lunar_surface.rs`, `ha-shared/luna_*` | ✅ |
| Planetas adicionales + biomas | `planet_world.rs`, `ha-shared/celestial_bodies.rs`, `surface_biome.rs` | ✅ base |
| Streaming de chunks + floating origin | `world_stream.rs`, `floating_origin.rs`, `surface_patch.rs` | ✅ |
| Ciclo día/noche, sol, atmósfera Ghibli | `sky_clock.rs`, `solar_lighting.rs`, `ghibli_atmosphere.rs` | ✅ |
| Depósitos minerales + flora | `deposit_spawn.rs`, `flora_spawn.rs`, `surface_deposits.rs` | ✅ |

### 2.2 Cohetes y espacio
| Sistema | Archivo(s) | Estado |
|---|---|---|
| Vuelo arcade con masa/fuel derivados de módulos | `rocket_flight.rs` (619 líneas), `ha-shared/flight_physics.rs`, `ship_mass.rs` | ✅ |
| Pad de lanzamiento con 3 slots hold-interact | `launch_pad.rs` (478 líneas) | 🟡 slots fijos, sin elección del jugador |
| Tránsito interplanetario + celestes en vuelo | `space_travel.rs`, `flight_celestials.rs`, `solar_ephemeris.rs` | ✅ |
| Nav computer + beacons + targeting | `nav_computer.rs`, `flight_beacon.rs`, `targeting.rs` | ✅ |
| Jetpack universal (EVA) | `jetpack_transit.rs` | ✅ |
| Vessels / base espacial | `ha-shared/vessel.rs`, `space_base.rs` | ✅ base |

### 2.3 Survival y construcción
| Sistema | Archivo(s) | Estado |
|---|---|---|
| Bloques voxel romper/colocar (Minecraft-style) | `block_world.rs` + `ha-shared/blocks.rs` | ✅ por cuerpo celeste |
| Harvest + inventario + recetas de craft | `harvest.rs`, `ha-shared/inventory.rs`, `recipes.rs` | ✅ |
| Máquinas auto-productoras (extractores) | `machines.rs`, `ha-shared/machines.rs`, `power.rs` | ✅ |
| Placement de estructuras | `surface_placement.rs`, `building_world.rs` | ✅ |
| Persistencia (inventario, mundo, bloques) | `inventory_persist.rs`, `world_persist.rs` | ✅ flush-on-pause |
| Parkour: salto, escalada, mantle, step-up | `ha-shared/parkour.rs` + contrato en doc | ✅ |
| HP + stamina | `sim.rs:51`, `constants.rs:41-49` | ✅ |
| **Hambre / comida / cocina** | — | ❌ no existe |
| **Oxígeno / soporte vital** | — | ❌ no existe (solo módulo O₂ como masa) |

### 2.4 Presentación y plataforma
| Sistema | Archivo(s) | Estado |
|---|---|---|
| Cámara 1ª/3ª persona (3 modos, toggle) | `camera.rs` | ✅ |
| Touch: 2 sticks, hold-interact, calibración | `input/joystick.rs`, `look_interact.rs`, `calibration.rs` | ✅ |
| Teclado/ratón desktop | `input/desktop_input.rs` | ✅ |
| **Gamepad/mando** | — | ❌ (bevy_input lo trae gratis, sin mapear) |
| HUD completo + inventario UI + iconos | `hud.rs` (2.717 líneas), `hud/inventory_ui.rs` | ✅ |
| Shaders toon/cel + cielo Ghibli | `assets/shaders/toon.wgsl`, `ghibli_sky.wgsl` | ✅ |
| Tiers de calidad Android vs desktop | `quality.rs`, `render/render_scale.rs`, `mesh_batch.rs` | ✅ estructura |
| Telemetría + perf spans + crash log | `telemetry.rs`, `debug/perf.rs`, `debug/crash.rs` | ✅ |
| Android nativo (insets, pause, APK pipeline) | `platform/android.rs`, `npm run play` | ✅ |

### 2.5 Deuda registrada (memoria del proyecto)
- **Pruebas perf en Redmi pendientes** — paso 0: heap + FPS por región + tirones (bloqueante declarado del roadmap).
- **Tierra fases 2–3** — retune de contenido para radio 1500 y pase visual.
- **`labiblia.md` desactualizada** — describe arquitectura pre-SFS; el código manda.
- Cambios grandes sin commitear en working tree (celestial refactor: `solar_system.rs`/`space_world.rs`/`celestial_sphere.rs` borrados, ~40 archivos tocados).

---

## 3. Huecos a cerrar (lo único que falta del pedido)

1. **Perf 60 FPS Redmi** — sin datos de dispositivo; todo lo demás está bloqueado por esto (regla del roadmap).
2. **Survival core**: hambre + comida (recolectar/cultivar/cocinar) y oxígeno como barra en vacío/espacio.
3. **Ship Builder Fase 1**: piezas crafteables + panel BUILD SHIP + stats en vivo (ya diseñado, backend 60% hecho).
4. **Gamepad**: mapear `bevy_input::gamepad` sobre el sistema de acciones existente (`input/actions.rs`).
5. **Bases off-world jugables**: campamento lunar (O₂ + energía + extractor) cerrando el loop del Acto II.

---

## 4. Plan maestro por fases (orden estricto)

> Regla heredada del roadmap: **no saltar fases; perf primero**. Cada fase termina con build en el Redmi + commit.

### Fase 0 — Estabilizar y medir (bloqueante)
- **0.a** Commitear/ordenar el working tree actual (refactor celestial en curso, 40+ archivos).
- **0.b** Pruebas en Redmi 10 (pendiente de memoria): heap, FPS por región (Tierra/vuelo/Luna), tirones.
- **0.c** Presupuesto de frame: si no da 60, decidir mix (render scale dinámico, radio de chunks, sombras, densidad flora). Meta: **60 FPS estable en Tierra e interiores; mínimo 45 en vuelo**.

### Fase 1 — Survival core (Zomboid/Minecraft)
- **1.a** Hambre: barra + drain lento; comida como ítems (baya/pescado/cultivo) + cocina en fogón/máquina. Sim en `ha-shared` (espejo de `/shared` TS), UI en HUD junto a stamina.
- **1.b** Oxígeno: barra activa solo en vacío (Luna/espacio/EVA); recarga en cápsula/base/tanque O₂. El módulo O₂ del cohete pasa de masa muerta a stat útil.
- **1.c** Muerte amable: respawn en última cama/base con pérdida parcial de carga (patrón "fallo amable" de la visión).

### Fase 2 — Ship Builder Fase 1 (SFS)
Implementar tal cual `SHIP_BUILDER_DESIGN.md` Fase 1:
- Piezas como ítems crafteables (`part_hull/tank/engine/lander`) con recetas.
- Panel **BUILD SHIP** en el pad: contador de piezas → `ShipLoadout` (ya deriva stats).
- Stats en vivo: masa, fuel, TWR, autonomía — el trade-off visible como en SFS.
- Persistencia `ship.json`.

### Fase 3 — Acto II jugable: base lunar
- Campamento: depósito + extractor de regolito + generador + recarga O₂.
- Antena + pantalla de monitoreo remoto desde Tierra (gameplay "agencia").
- Misión de retorno con carga (bodega llena → aterrizar en Tierra → recompensa).

### Fase 4 — Controles completos
- Gamepad: stick izq = mover, der = cámara, mapping de USE/BAG/MENU y modo vuelo; detección hot-plug; glifos en HUD según dispositivo activo.
- Remapeo básico y pulido desktop.

### Fase 5 — Profundidad
- Interior lunar (una cueva instanciada, linterna, recurso raro).
- Segundo planeta con bioma propio + nave carguero (blueprint prefab).
- Tierra fases 2–3 pendientes (retune + visual) se intercalan aquí.

### Fase 6 — Colonos v1 (visión Acto V)
- Colonos abstractos: barras O₂/comida/refugio, asignación a máquina como boost. Sin pathfinding.

---

## 5. Reglas que el plan respeta

- Sim determinista solo en `/shared` (TS) espejada en `ha-shared` — nada de reglas nuevas solo en cliente.
- Servidor autoritativo; cliente manda inputs.
- Sin física orbital real: gravedad constante por cuerpo, thrust arcade (spec §Física arcade de la visión).
- Anti-goals vigentes: nada de editor delta-v, ni rigid-body interplanetario, ni multijugador de colonias antes de offline sólido.
- Máx. 2 sticks + 2 botones contextuales visibles en touch.

## 6. Qué apruebo con este documento

1. El **orden de fases** (0 → 6) y que Fase 0 (perf Redmi) bloquea el resto.
2. Que survival core (hambre/O₂) va **antes** que el ship builder.
3. Que el mando (gamepad) espera hasta Fase 4.

Cualquier cambio de orden se anota aquí antes de ejecutar.
