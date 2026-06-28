# Atelier de mundo — isla, biomas y refugio

Carpeta para **briefs de diseño de mapa**, checklists de archivos y notas de rediseño.  
No es código ejecutable: es el punto de entrada cuando querés moldear la isla (o futuros cuerpos celestes) sin romper netcode, determinismo ni perf en Android.

**Relacionado:** [`labiblia.md`](../../labiblia.md) · [`planets-topology.md`](./planets-topology.md) · [`native/docs/PLAYER_WORLD_DESIGN.md`](../../native/docs/PLAYER_WORLD_DESIGN.md) · [`.cursor/skills/verify-before-done/SKILL.md`](../../.cursor/skills/verify-before-done/SKILL.md)

---

## Objetivo

Controlar cada milímetro del mapa y moldear el refugio virtual (distribución, biomas, secretos, límites) **operando sobre la infraestructura actual** del juego:

- Sim **input-only** — el server nunca acepta posiciones del cliente.
- **Determinismo** — mismo `seed` → mismo layout en TS (autoritativo) y Rust (cliente nativo).
- **Perf Android** — hoy ~13–16 FPS en Redmi; cualquier rediseño debe respetar streaming por chunks y batching (ver §21 en `labiblia.md`: 30+ FPS antes de naves/colonos).

---

## Archivos requeridos para rediseñar el mapa

Antes de proponer cambios, leé (o pegá en el chat de diseño) el contenido de estos archivos. Son la cadena completa **datos → espejo → render**.

### 1. La verdad matemática (TypeScript)

| Archivo | Rol |
|---------|-----|
| [`packages/shared/src/sim/world.ts`](../../packages/shared/src/sim/world.ts) | `generateWorld(seed)` — obstáculos e interactuables deterministas |
| [`packages/shared/src/sim/constants.ts`](../../packages/shared/src/sim/constants.ts) | `WORLD_HALF`, `OCEAN_HALF`, gravedad, parkour |
| [`packages/shared/src/sim/worldProps.ts`](../../packages/shared/src/sim/worldProps.ts) | Props estáticos derivados del path grid |
| [`packages/shared/src/sim/paths.ts`](../../packages/shared/src/sim/paths.ts) | Caminos/pradera procedural (mismo seed + obstáculos) |

**Estado actual (seed 42):**

- Isla **192×192** (`WORLD_HALF = 96`)
- **28** obstáculos de colisión, **55** nodos interactuables (`tree` / `rock` / `container`)
- Centro despejado (`CENTER_CLEAR = 8`)
- RNG independiente: `seed ^ 0x9e3779b9` (no contamina el stream principal de sim)

### 2. El espejo y la estética (Rust — `ha-shared`)

| Archivo | Rol |
|---------|-----|
| [`native/crates/ha-shared/src/world.rs`](../../native/crates/ha-shared/src/world.rs) | Puerto de `generateWorld` — **debe coincidir** con TS |
| [`native/crates/ha-shared/src/constants.rs`](../../native/crates/ha-shared/src/constants.rs) | Mismos números que `constants.ts` |
| [`native/crates/ha-shared/src/palette.rs`](../../native/crates/ha-shared/src/palette.rs) | Colores Ghibli + acentos neon (cielo, pradera, agua, outline) |

**Nota:** `OCEAN_HALF = 384` vive en **constants**, no en `palette.rs`. El océano visual es anillo en render; el jugador queda clamped en la isla.

**Paridad:** cualquier cambio en conteos o campos de contenido debe pasar por el harness genérico (`catalog-parity`) — ver skill `verify-before-done`.

### 3. El render y el streaming (Rust — `ha-game`)

| Archivo | Rol |
|---------|-----|
| [`native/crates/ha-game/src/world_spawn.rs`](../../native/crates/ha-game/src/world_spawn.rs) | Pradera, playa, acantilados, flora scatter, spawn de obstáculos/interactuables |
| [`native/crates/ha-game/src/chunk_manager.rs`](../../native/crates/ha-game/src/chunk_manager.rs) | Streaming decor en bloques **40×40** (`CHUNK_SIZE = 40`) |
| [`native/crates/ha-game/src/render/instanced_decor.rs`](../../native/crates/ha-game/src/render/instanced_decor.rs) | Batch de props por chunk + kind |
| [`native/crates/ha-game/src/render/ocean_horizon.rs`](../../native/crates/ha-game/src/render/ocean_horizon.rs) | Anillo de mar barato (perf) |p
| [`native/crates/ha-game/src/scene.rs`](../../native/crates/ha-game/src/scene.rs) | Atmósfera por zona (`Overworld` / `Cave`) |

**Estado actual render:**

- Seed mundo en cliente: `42` (`DEFAULT_WORLD_SEED` en `world_spawn.rs`)
- Flora scatter: **96** props Android / **220** desktop
- Ground tiles: step **4 m** Android / **2 m** desktop

---

## Reglas inviolables al rediseñar

1. **TS primero** — editar `packages/shared/src/sim/world.ts` (y constants si aplica), luego espejar en `ha-shared/src/world.rs`.
2. **No duplicar sim** en `ha-game` — solo spawn/visual; colisión viene de `generate_world`.
3. **Obstáculos climbables** — altura 0.5–3.0 m para parkour; >3 m = acantilado bloqueante (ver `PLAYER_WORLD_DESIGN.md`).
4. **Perf** — más props = más presión en `chunk_manager` + UI; medir con `scripts/smoke-android-perf.sh` y anotar en `perf-history.jsonl`.
5. **Legado combate** — dormido (§22); overworld pacífico salvo cueva/misión futura.

---

## Brief creativo — completar antes de codear

Respondé estas preguntas (podés editar esta sección directamente o pegar respuestas en el chat de diseño):

### Distribución y escala

- [ ] ¿La isla sigue siendo **una sola pieza 192×192** o querés sub-islas / anillos / bahías?
- [ ] ¿Dónde va el **spawn seguro** y el **refugio/base** inicial (coordenadas aproximadas o “norte de playa”)?
- [ ] ¿Más densidad en el interior o en la costa?

### Biomas (Acto I — Tierra)

- [ ] ¿Cuántas zonas distintas? (ej. pradera central, bosque denso, acantilado rocoso, playa, pantano)
- [ ] ¿Transiciones duras (cliff mesh) o suaves (blend de palette / altura)?
- [ ] ¿Recursos por bioma? (madera solo en bosque, piedra en acantilado, etc.)

### Secretos y landmarks

- [ ] ¿Puntos de interés fijos por seed o hand-placed en el generador?
- [ ] ¿Entrada a cueva / misión — dónde y cómo se descubre?
- [ ] ¿Algo “visible desde lejos” (torre, cohete futuro, naufragio) como brújula visual?

### Límites del mundo

- [ ] ¿Mar solo visual (GTA V) o quiero **gameplay en el agua** más adelante?
- [ ] ¿Pared invisible vs acantilado climbable vs instakill al salir de `WORLD_HALF`?

### Constraints de equipo

- [ ] Target de FPS mínimo en Redmi para este rediseño: ___ (default **30**)
- [ ] ¿Prioridad: belleza > densidad, o densidad de recursos > flora decorativa?

---

## Plantilla de entrega (para quien rediseña)

Cuando tengas el concepto claro, entregá:

1. **Respuestas al brief** (sección anterior)
2. **Boceto ASCII o lista** de zonas con radios/desde centro
3. **Tabla de cambios numéricos** (ej. `OBSTACLE_TARGET`, `NODE_TARGET`, nuevos `InteractableKind`)
4. **Lista de archivos a tocar** (TS → ha-shared → world_spawn → chunk_manager si aplica)
5. **Criterio de éxito** — screenshot mental + “puedo sprintear de spawn a X en Y segundos sin trabarse”

---

## Cómo usar esta carpeta

| Qué querés hacer | Dónde |
|------------------|--------|
| Nuevo brief de isla / planeta | Nueva nota aquí (`design/world/`) o ampliar este README |
| Contrato player ↔ geometría | `native/docs/PLAYER_WORLD_DESIGN.md` |
| Visión largo plazo espacial | `native/docs/SPACE_SURVIVAL_VISION.md` |
| Checklist pre-merge | `.cursor/skills/verify-before-done/SKILL.md` |

---

## Próximo paso

**Pegá el código o pedí diff de los archivos de la tabla** (empezá por TS `world.ts` + Rust `world.rs` si querés validar paridad antes de tocar arte).

Y contame: **¿cuál es la idea exacta para la distribución, biomas o secretos de esta isla?**  
Completá el brief de arriba — con eso se puede proponer un plan de archivos sin tocar pad de lanzamiento ni naves hasta que perf y diseño estén alineados.
