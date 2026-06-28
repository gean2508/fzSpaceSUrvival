# Agencia Espacial Survival — Visión de diseño

> **Documento de dirección creativa** (jun 2026).  
> Referencia para features, assets y sim. **No es backlog de implementación inmediata** — ver precondiciones en `labiblia.md` §21.

---

## Elevator pitch

**Survival en primera persona** donde empezás en **Tierra** (la isla actual), construís una **agencia espacial casera**, ensamblás **naves prefabricadas**, explorás **espacio y el interior de planetas**, levantás **bases en otros mundos** y, al final, **enviás colonos humanos** a vivir allí.

Inspiración: *Kerbal Space Program*, *Space Flight Simulator* — pero **sin física orbital real**. Vuelo **arcade fluido**, controles **precisos e intuitivos** en touch y teclado, progresión por **craft + máquinas + expediciones**, no por calculadora de delta-v.

---

## Pilares de diseño

| Pilar | Qué significa |
|-------|----------------|
| **Explorar** | El premio es llegar a un lugar nuevo (órbita, superficie, cueva, núcleo) y traer recursos/datos |
| **Construir** | Bases modulares reutilizando el loop actual (máquinas, placement, automatización) |
| **Automatizar** | Extractores y fábricas pasivas — menos grind manual, más planificación |
| **Expandir** | De un jugador en Tierra → varias bases → colonos NPC con necesidades simples |
| **Fluidez** | 30+ FPS en móvil; vuelo sin simulación pesada; transiciones cortas entre modos |

---

## Referencias — qué tomar y qué NO

| De KSP / SFS | Tomamos | **NO** implementamos |
|--------------|---------|----------------------|
| Sensación de escala | Cielo estrellado, planetas grandes en horizonte, “viajé lejos” | Órbitas keplerianas, nodos ascendentes, maniobras Hohmann reales |
| Naves | Prefabs modulares (cápsula, tanque, motor, alas) **crafteados**, no voxel | Editor de cohete pieza a pieza con física de cada strut |
| Aterrizaje | Zona marcada, asistencia opcional, tensión visual | Simulación de gravedad/atmosfera con ecuaciones |
| Bases | Habitats, depósitos, antenas, generadores | Kerbal con trajes complejos y EVA physics puzzle |
| Progresión | Desbloquear cuerpos celestes y piezas de nave | Árbol tecnológico de 200 nodos antes de volar |

**Regla de oro:** si un sistema requiere **tutorial de física** para ser divertido, está fuera. Si se entiende en **30 segundos de juego**, entra.

---

## Loop principal (core loop)

```
Tierra (isla) ──► recolectar / craftear ──► ensamblar nave prefab ──► misión espacial
       ▲                                      │
       │                                      ▼
  colonos / carga ◄── base off-world ◄── aterrizar / explorar interior
       │                                      │
       └── monitoreo remoto (UI) ◄── automatizar con máquinas
```

1. **Preparación en Tierra** — laboratorio, pad de lanzamiento, craft de módulos de nave.
2. **Expedición** — vuelo arcade (espacio) o caminata FP (superficie/interior).
3. **Establecer foothold** — base mínima (oxígeno, energía, depósito).
4. **Automatizar** — extractores de regolito/hielo/minerales (reuso `machines/`).
5. **Escalar** — módulos de hábitat → solicitar **colonos** desde Tierra.
6. **Repetir** en otro cuerpo o profundizar en el interior.

---

## Dos modos de exploración

### A) Espacio — tránsito, órbita, aterrizaje

- **No** simulación orbital continua. Cada viaje es una **misión instanciada**:
  - Pre-flight checklist (combustible abstracto, oxígeno, módulos equipados).
  - Fase **ascenso** (opcional scripted): stick + throttle simplificado.
  - Fase **crucero**: autopilot con waypoints o mini-juego de “mantener en corredor” (barra de alineación).
  - Fase **aproximación / aterrizaje**: zona iluminada en superficie; controles arcade (pitch/yaw + throttle + botón “retro quemar”).
- **Combustible / oxígeno** = barras que bajan con el tiempo y con inputs — no masa variable.
- **Asistencia de aterrizaje:** 3 niveles (automático / asistido / manual puro) desbloqueables por progresión.
- **Fallo amable:** crash = respawn en última base o rescate en Tierra con penalización de carga, no game over duro.

### B) Superficie e interior planetario

- **Superficie:** mapas medianos por cuerpo (un bioma dominante: regolito lunar, hielo, desierto rojo, etc.).
  - Misma FP + parkour + gather que Tierra, con gravedad/atmósfera **como multiplicadores** (salto más alto en Luna, traje en vacío).
- **Interior:** instancias separadas (cuevas, lava tubes, cavernas de hielo, “núcleo” abstracto):
  - Iluminación limitada (linterna / colocar luces).
  - Recursos raros solo bajo tierra.
  - Peligros ambientales simples (frío, toxicidad, derrumbes scripted) — **no** combate PvP legado salvo fauna futura opcional.
- Transición superficie ↔ interior = **portal / boca de mina** crafteable, no mundo infinito vertical.

---

## Naves prefabricadas (sistema de módulos)

No se construye cohete voxel a voxel. Se **craftean y ensamblan módulos** en el pad:

| Módulo | Función |
|--------|---------|
| **Casco / cápsula** | Asientos, oxígeno base, punto de spawn en misión |
| **Tanque** | Capacidad de combustible abstracto |
| **Motor** | Thrust máximo en fase arcade (stat, no Newtons) |
| **Estabilizadores** | Reduce drift en vuelo manual |
| **Tanque O₂ / life support** | Duración de EVA y tripulación |
| **Bodega** | Slots de carga para traer recursos |
| **Aterrizador** | Piernas + retro — requisito para cuerpos sin puerto |
| **Scanner** | Revela puntos de interés en mapa de misión |
| **Habitat móvil** (tarde) | Permite dormir/save off-world |

**Ensamblaje:** UI tipo “slots” en pad (3–6 ranuras según tier). Validación simple: motor + tanque + casco mínimo.

**Variantes prefab de fábrica (late game):** “Scout”, “Carguero”, “Colonizador” — blueprints que autollen ranuras con stats balanceados.

Asset direction: meshes procedurales en catálogo (`ship_module_*`) — coherente con `AssetFactory`.

---

## Bases en otros planetas

Reutiliza el stack actual:

- **Placement** — martillo + hold (ya en `look_interact.rs`) para módulos de base.
- **Máquinas** — extractores por recurso del bioma (`regolith_extractor`, `ice_drill`, etc.).
- **Red de base** — energía (solar / nuclear crafteable), oxígeno, almacenamiento; stats en sim `@ 20 Hz`, no fluidos 3D.
- **Tiers de base:**
  1. **Campamento** — tienda, depósito, extractor 1
  2. **Puesto** — habitación sellada, craft bench, antena (link remoto a Tierra)
  3. **Colonia** — slots de colonos, granja hidropónica, producción en cadena

**Antena / uplink:** desbloquea pantalla de **monitoreo remoto** (estado máquinas, stock, alertas) desde Tierra — gameplay de “agencia”, no micromanagement en tiempo real obligatorio.

---

## Pipeline de colonos humanos

Fase tardía, después de hábitat estable:

1. **Habitat sellado** + **oxígeno estable** + **comida** (máquina o import desde Tierra).
2. **Solicitud de colonos** — UI en Tierra: elegir especialidad (minero, técnico, biólogo).
3. **Viaje en nave “Colonizador”** — misión especial con slots de NPC.
4. **En destino:** colonos son entidades sim simplificadas:
   - Necesidades: O₂, comida, refugio (barras).
   - Trabajo asignado a máquina (boost de output pasivo).
   - No pathfinding complejo al inicio — “ocupado en edificio X” abstracto.
5. **Crecimiento:** más colonos → más demanda → más cadena logística Tierra ↔ colonia.

---

## Progresión por actos (contenido)

| Acto | Ubicación | Meta |
|------|-----------|------|
| **I — Agencia casera** | Tierra (isla actual) | Pad, primera nave, wood/stone machines |
| **II — Luna cercana** | Superficie lunar pequeña | Primera base, regolito, retorno con carga |
| **III — Sistema interior** | 2–3 cuerpos (Marte-like, hielo, roca) | Bases múltiples, naves especializadas |
| **IV — Profundidad** | Interiores por planeta | Recursos raros, trajes tier 2 |
| **V — Colonización** | Cualquier base tier 3 | Colonos, cadena de suministros |
| **VI — Horizonte** | Estación orbital / cinturón (futuro) | Endgame cosmético + escala |

Cada acto = **1 mapa jugable nuevo** + **2–4 prefabs de nave/módulo** + **3–5 máquinas** — no big-bang.

---

## Física arcade — especificación

| Sistema | Comportamiento |
|---------|----------------|
| Gravedad | Constante por cuerpo (`gravity_scale`: Tierra 1.0, Luna 0.16, …) |
| Thrust | `velocity += forward * thrust_stat * dt` con cap máximo |
| Drag espacial | Amortiguación suave fuera de atmósfera — no inercia infinita confusa |
| Atmósfera | Solo efecto visual + ligero slow + shake en entrada — no aerodinámica |
| Rotación nave | Pitch/yaw suavizados; botón “estabilizar” alinea a horizonte local |
| Aterrizaje OK | Velocidad vertical < umbral + dentro de zona marcada |
| Combustible | `fuel -= thrust_input * cost_per_sec`; sin masa variable |

Todo determinista en `packages/shared` para replays/net futuro; `ha-shared` espejo numérico.

---

## Controles por contexto (misma filosofía touch)

| Contexto | Izquierda | Derecha | Acciones |
|----------|-----------|---------|----------|
| **FP suelo** (actual) | Stick mover | Look drag | USE hold/tap, BAG, MENU |
| **EVA / superficie vacío** | Igual + salto boost traje | Igual | Anclaje magnético (hold) |
| **Vuelo arcade** | Stick = throttle + yaw | Look = pitch/roll o “aim corredor” | Retro / estabilizar / autopilot toggle |
| **Interior estrecho** | Igual FP | Igual | Linterna, colocar luz |

**Principio:** nunca más de **2 sticks + 2 botones contextuales** visibles. Precisión > realismo.

---

## Integración con el código actual

| Ya existe | Reutilización en visión espacial |
|-----------|----------------------------------|
| Isla `world_spawn.rs` | **Tierra — Acto I** sin regenerar layout |
| `machines/` + placement | Extractores y módulos de base off-world |
| `generate_world(seed)` | Un seed por cuerpo celeste |
| `WorldContext` / regiones | `overworld` → Tierra; `mission` → instancia espacial/interior |
| `AssetFactory` + catálogo | `ship_*`, `base_*`, `planet_*` kinds |
| Parkour + FP | Superficies e interiores |
| Legado combate | Dormido; posible fauna hostil en interiores **mucho después** |

**Nuevos dominios sim (futuro):**

```
packages/shared/src/content/ships/     # módulos, blueprints, stats
packages/shared/src/content/bodies/    # cuerpos celestes, gravity, bioma
packages/shared/src/sim/flight.ts      # arcade flight tick (determinista)
packages/shared/src/sim/colony.ts      # colonos, necesidades, asignación
```

---

## Roadmap de implementación (orden estricto)

1. **Perf 30+ FPS** en Redmi — **bloqueante** para todo lo demás.
2. **Inventario + craft UI** persistente en Tierra.
3. **Pad de lanzamiento + ensamblaje 3 módulos** (casco/tanque/motor) — misión “suborbital” scripted.
4. **Mapa Luna** pequeño + aterrizaje asistido + base campamento.
5. **Antena + monitoreo remoto** UI.
6. **Interior lunar** (una instancia cueva).
7. **Segundo planeta + nave carguero**.
8. **Colonos v1** (abstractos, sin pathfinding).

No saltar fases. No implementar cohetes ni colonos antes de perf + loop Tierra sólido.

---

## Anti-goals (explícito)

- ❌ Editor orbital con mapa 2D de nodos y delta-v
- ❌ Construcción de cohete pieza a pieza estilo KSP
- ❌ Física rigid-body entre planetas en tiempo real
- ❌ Multijugador colonias antes de offline sólido
- ❌ Terraformación planetaria completa simulada
- ❌ Reintroducir PvP arena como foco principal
- ❌ Mobs animados / viewmodel FP antes de 30 FPS

---

## Estética

- **Tierra:** Ghibli + Blood Dragon dusk (actual).
- **Espacio:** cielo denso estrellas, nebulosa suave, siluetas planetas toon.
- **Otros cuerpos:** misma cel 3 bandas; paleta por bioma en `palette.rs`.
- **UI agencia:** fStyle Blood Dragon — sensores, scanlines, “CONFIRMACIÓN DE MISIÓN”.

---

*Operativa día a día: `labiblia.md`. Contrato movimiento/mundo: `PLAYER_WORLD_DESIGN.md`.*
