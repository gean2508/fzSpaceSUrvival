# Ship Builder — diseño (estilo Spaceflight Simulator, adaptado)

> Estado: **Fase 1 IMPLEMENTADA (11 jul 2026)** — piezas crafteables (`part_tank`/`part_engine`/`part_lander`, recetas 14–16), tab **BAG → SHIP** con contadores [−]/[+] que consumen/devuelven piezas del inventario, stats en vivo (masa/fuel/burn/thrust/autonomía), persistencia `ship.json`, extras aplicados al `ShipLoadout` cuando el pad está completo; motores extra multiplican thrust (`thrust_factor`, cap x1.45). Desviación deliberada: el casco no es ítem — sigue siendo el módulo 1 del pad. Fases 2–3 pendientes.

## La idea (capturada del usuario + SFS)

En SFS lo genial es: **vos construís tu cohete como querés**, apilando/pegando piezas
(cápsula, tanques, motores, estructura, aerodinámica; y avanzadas: bisagras, pistones,
ruedas). Los stats de la nave (empuje, combustible, masa, TWR) **emergen** de cómo la
armaste. No hay una nave fija: hay piezas y vos decidís.

Nuestra versión toma eso pero lo cierra en un **loop de survival**: juntás recursos
(a mano y **automatizados con máquinas**) → crafteás piezas → ensamblás tu nave en el
pad → volás. Nave más grande = más capacidad pero más masa/combustible = más planificación.

## Lo que YA existe (base sobre la que se construye — no reinventar)

| Pieza del sistema | Dónde | Estado |
|---|---|---|
| **Máquinas que auto-producen recursos** | `ha-shared/machines.rs` (wood/stone extractor) | ✅ el "automatizar recursos" ya arranca aquí |
| **Módulos de nave con masa fija** | `ha-shared/ship_mass.rs` (hull 3.0, tank 2.0, engine 2.5, O₂ 1.5, cargo 2.0, lander 1.0, scanner 0.8) | ✅ |
| **`ShipLoadout`**: deriva masa/fuel_max/burn_factor de los módulos ensamblados | `ha-game/rocket_flight.rs`, `launch_pad.rs::sync_ship_loadout` | ✅ (hoy fijo a hull+tank+engine) |
| **Pad con slots de ensamblaje** | `launch_pad.rs` (3 slots hold-interact) | ✅ parcial |
| **Vuelo con masa/combustible reales** | `rocket_flight.rs` (mass tax en burn, sin escalar aceleración) | ✅ |
| **Consumo por tránsito + autonomía en HUD** | `hud.rs` | ✅ |

**Conclusión:** el 60% del backend ya está. Falta la parte de **elección del jugador**
(qué piezas y cuántas) + la UI + las piezas como ítems crafteables.

## Fase 1 — Ensamblaje básico (primer slice, sin bisagras/pistones)

1. **Piezas como ítems crafteables** (`ha-shared` catálogo + recetas + espejo Rust + paridad):
   - `part_hull` (cápsula/mando — obligatoria, 1×)
   - `part_tank` (tanque — +100 fuel_cap, +2 t)
   - `part_engine` (motor — +thrust, +2.5 t)
   - `part_lander` (tren de aterrizaje — mejora aterrizaje asistido)
   - Coste en hierro/oro/piedra; producibles pasivamente con máquinas.
2. **Ensamblaje en el pad**: en vez de 3 slots fijos, un **panel BUILD SHIP** donde
   elegís cuántos tanques/motores agregar (v1: contador simple, no colocación libre 3D).
   `ShipLoadout` ya deriva stats — solo hay que alimentarlo con la config elegida.
3. **Feedback de stats en vivo**: masa total, fuel_cap, TWR (thrust/masa), autonomía
   estimada de crucero — el jugador ve el trade-off al agregar piezas (como el panel de SFS).
4. **Persistencia**: la config de nave guardada (`ship.json`, patrón flush-on-pause B10).

## Fase 2 — Colocación libre + piezas estructurales (más SFS)

- Colocación en una grilla vertical 2D/3D (apilar tanques, motores abajo, mando arriba).
- Piezas estructurales y aerodinámicas (afectan drag/estabilidad).
- Múltiples motores → empuje sumado; balance de masa.

## Fase 3 — Piezas avanzadas (bisagras, pistones)

- **Bisagra** (`part_hinge`): articula sub-ensambles con un ángulo controlable (input).
- **Pistón** (`part_piston`): extiende/retrae (input). Habilitan tren desplegable,
  paneles, brazos — la parte "creativa" de SFS.
- Requiere un mini rig de articulaciones en el cliente (transform jerárquico +
  control por input). Es el bloque más pesado; va al final.

## Reglas que respeta

- Sim/definición de piezas y costes en `packages/shared` + espejo `ha-shared` (§2).
- Física arcade (§23): masa FIJA por config, sin ecuación del cohete, sin masa variable
  en vuelo. Empuje/fuel como recursos, no simulador rígido.
- Nada rompe el vuelo continuo SFS (v1.0.134): el builder solo alimenta `ShipLoadout`.

## Loop de juego resultante (identidad del juego)

```
Tierra: juntar recursos (mano + máquinas auto) → craftear piezas → ARMAR nave en pad
  → despegar → volar libre por el sistema solar → aterrizar en otro cuerpo
  → montar base + máquinas allá → más recursos/piezas → naves mejores → más lejos
```

Esto le da al juego el "por qué juego": **explorar el sistema solar con naves que vos
diseñás, sostenido por una economía de recursos que automatizás.**
