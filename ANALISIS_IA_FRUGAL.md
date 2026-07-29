# ANÁLISIS — ¿Sirve la IA frugal (HIVE_OS / IAFZ) para el cerebro colmena? (2026-07-25)

> Analicé `iagrufal-main/` (HIVE_OS SDK + IAFZ-64/128 + integración Godot en
> `caritoooocongemini` + gyms en `hive_os/simulators`). Veredicto y plan abajo.

## 1. Qué es (técnicamente)
- **IAFZ-64/128 = Deep Binary Neural Network** (MLP de 3 capas: oculta→estrategia→
  salida), **sin punto flotante pesado y sin backpropagation**. La activación es
  `XNOR + popcount` (compuertas lógicas), a velocidad de registro de CPU.
- **Genoma = archivo `.adn`** diminuto (6.5–9.6 KB por cerebro, `#pragma pack`).
- **Aprende por NEUROEVOLUCIÓN** (algoritmos genéticos) offline en C: millones de
  generaciones contra un "gym" con currículum + castigos → comportamiento emergente,
  no scripteado. Después se extrae el genoma élite y se corre solo **inferencia**
  (`think_iafz_64`) a latencia casi nula, con el aprendizaje apagado.
- **API C lista**: `init / think / mutate / crossover / reset_echo / earthquake /
  save / load`. **`MAX_AGENTS 1000`** (maneja mil cerebros en un array) y **memoria
  recurrente** (`MEMORY_BITS`, `reset_echo`) → NO es puramente reactiva: puede
  arrastrar estado entre ticks.
- **Ya probada en un juego** (Godot, como controlador de RigidBody).

## 2. Por qué ENCAJA muy bien (a favor)
- **Perf / Redmi 60 fps:** diseñada para microcontroladores/drones. Float-free,
  cerebro de KBs, inferencia a costo casi cero → mil agentes pensando es viable en
  el celu (justo lo que mata a los sistemas de NPC tradicionales).
- **Escala de colmena nativa:** 1000 agentes + un genoma compartido por especie/rol
  = literalmente un **cerebro colmena** (todos los lobos comparten el genoma-lobo).
  Eficiente y coherente con tu idea.
- **Determinista:** `think` es determinista (genoma+entrada → misma salida) → encaja
  con nuestra sim determinista (si corre server-side / en `ha-shared`, ver §6).
- **Reusa nuestro plan swarm (B3):** los gyms de HIVE_OS son exactamente lo que
  planeamos — nuestro `step_topdown_movement` + sensores del mundo ES el gimnasio.
  El "ejército de testers" (B1/B3) genera el dataset y entrena los genomas.

## 3. Los límites (honesto — dónde NO alcanza sola)
- **Es un CONTROLADOR reactivo (+ algo de memoria), no una mente cognitiva.** Mapea
  sensores→acción inmediata. Da **instinto y movimiento orgánico**, no "planes",
  metas de largo plazo, ni lenguaje/razonamiento (no es un LLM).
- **E/S angosta:** entrada de ~32 bits, salida de ~64/128 bits. Perfecto para
  **locomoción/steering/evasión/persecución**; incómodo para decidir METAS de alto
  nivel (¿trabajar, comer, socializar, huir?) directamente.
- **Entrenamiento = diseño de gyms + fitness.** Que un NPC sea "listo" exige diseñar
  el gym que premia esa conducta. Es trabajo real (pero es justo el plan swarm).

## 4. La arquitectura que RECOMIENDO (complemento) — cerebro HÍBRIDO en capas
El "se siente vivo / consciente" y la **retención** NO salen de la red sola; salen
de una capa de **psicología** encima. Propongo un cerebro de 2 capas por agente:

- **Capa ALTA — Psicología / cognición (barata, determinista, tuneable):** un
  sistema de **necesidades + utilidad** (hambre, energía, miedo, social, curiosidad)
  + memoria simple (dónde vi comida/peligro, con quién me llevo) que decide la
  **META** actual. Esto es lo que da "tiene vida interior, rutinas, relaciones" —
  el motor de retención (referencias: The Sims, RimWorld, Rain World, A-Life de
  S.T.A.L.K.E.R.). Vive en `ha-shared` (determinista, server-authoritative).
- **Capa BAJA — Instinto / cuerpo (IAFZ):** la IA frugal EJECUTA la meta a nivel
  motor: navegar hacia X, evadir obstáculos, perseguir/huir, moverse orgánico y
  no-scripteado. Un genoma por especie/rol (la "colmena"), evolucionado en el gym.

Así: la psicología dice "tengo hambre, voy a esa baya"; IAFZ resuelve el CÓMO
moverse hasta ahí de forma viva. Es como funcionan los juegos AAA (planner + steering).

## 5. Riesgos / decisiones
- **Determinismo/red:** el juego es server-authoritative (regla 2). Los cerebros de
  NPC deben correr **en la sim determinista (`ha-shared`) o en el servidor**, no como
  adorno de cliente, o divergen entre jugadores. IAFZ es determinista → compatible,
  pero hay que ubicarlo bien.
- **Alcance:** es multi-sesión (FFI Rust↔C + sensores + gyms + entrenamiento +
  spawn de NPCs con cerebro). Empezar por **UN** caso vertical (un animal con
  locomoción evolucionada) antes de generalizar.
- **Diversión > realismo:** no sobre-simular. La meta es que **se sienta** vivo, no
  simular biología. Si un comportamiento no aporta a la diversión/retención, fuera.

## 6. Integración técnica (camino concreto)
- **FFI Rust↔C:** crate `iafz-sys` (build con `cc`) que envuelve `iafz_64.c/.h`
  (`think/init/load/save`). Bevy/`ha-shared` llaman `think_iafz_64(id, sensores_u32)`.
- **Sensores→32 bits:** codificar el entorno LOCAL del agente (celdas/peligros/
  recursos/otros agentes relativos) en el bitfield de exterocepción. Salida (float/
  bits) → `MovementIntent` (mismo pipeline que el jugador y el swarm).
- **Genomas `.adn` como assets** (6.5 KB): uno por especie/rol, entrenados offline
  (Fase Cámara, en la compu, reusando el swarm/gym), embebidos como el arte/audio.
- **Corre en la sim determinista** → server-authoritative; 60 fps Redmi ✓.

## 7. VEREDICTO
**SÍ conviene** — es una base **excelente y frugal** para la **capa de instinto/
locomoción** del cerebro colmena (NPCs, enemigos, animales), y encaja con perf,
determinismo, escala y con nuestro plan swarm. **PERO** por sí sola no da "vivo/
consciente": hay que ponerle encima la **capa de psicología (necesidades/utilidad/
memoria)**, que es la que realmente mueve la retención. Juntas = mundo que se siente
vivo, corriendo en el celu.

Encaja en **PLAN_MAESTRO FASE 8 (B4)**, ahora desbloqueada. Sub-fases sugeridas:
- **B4.0** `iafz-sys` (FFI Rust↔C) + smoke de `think` en un test.
- **B4.1** UN animal vertical: sensores→32 bits, IAFZ mueve, gym en desktop
  (reusa swarm), genoma `.adn` embebido. Corre determinista en la sim.
- **B4.2** capa de psicología (necesidades/utilidad/memoria) que fija la meta.
- **B4.3** generalizar a especies/roles (colonos, enemigos, fauna) con un genoma
  por rol. Telemetría del swarm calibra utilidades.

## 8. Preguntas para vos
1. ¿Arranco por **B4.0 (FFI + smoke)** para dejar la IA compilando y llamándose
   desde Rust, y de ahí un animal vertical (B4.1)?
2. ¿La psicología (metas) la querés **evolucionada también** (otra red) o
   **diseñada a mano** (utilidad tuneable)? (Recomiendo mano: más control + tuneo
   de retención.)
3. ¿Primer sujeto de prueba: un **animal** (fauna, bajo riesgo) o directo los
   **colonos**? (Recomiendo un animal primero.)
