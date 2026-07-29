# PLAN — Vivario: vida emergente PERSISTENTE (datos en todos lados, render cerca)

> Diseño de subsistema. Anexo de `PLAN_MAESTRO.md` §8/N7. Fuente de verdad de la
> conducta: `native/crates/ha-shared/src/fauna_brain.rs` (sensor compartido gym↔juego).
> No podar la conducta emergente (memoria `ai-life-emergence-no-prune`). Interfaz de
> herramientas remotas: `PLAN_HERRAMIENTAS_REMOTAS.md`.

## 0. Visión (del usuario, 2026-07-25, refinada)

En **modo creativo**, spawnear entidades (fauna; luego colonos/NPCs) y **cerrar la puerta
de una casa para encerrarlas y observar cómo evoluciona su conducta** cuanto más viven en
el mundo. Gyms enseñan las BASES; de ahí, emergencia. Salvaguarda: **prevenidos ante
mínimos/máximos locales**. Refinamientos clave:

- **NO al "círculo de streaming".** Los animales (sobre todo los **domésticos de mi
  granja**) **no deben desaparecer/respawnear cuando me alejo**. Existen como **DATOS
  persistentes en su lugar del mundo**; sólo se **renderizan/simulan en detalle cuando
  estoy cerca** (o cuando una cámara los enfoca). Me alejo, vuelvo, siguen ahí.
- **Ecosistemas por planeta.** Cada cuerpo (Tierra, Luna, y los planetas que inventemos en
  el universo) tiene **su propia fauna** — animales inventados por bioma/planeta, no los
  terrestres en todos lados.
- **Máquina "cámara".** Una máquina colocable que se envía a otro cuerpo (ej. la Luna) y,
  estando yo en la Tierra, me **conecto y VEO renderizado** lo que la cámara enfoca allá
  (los animales lunares, etc.). Es una segunda fuente de "activación/render" además del
  jugador.

Adaptar al proyecto, no tomar al pie de la letra.

**Aclaración (usuario, 2026-07-25):** el spawn en creativo + "cerrar la puerta con seguro"
es un **atajo de DEBUG** (analogía: el huevo de spawn del modo creativo de Minecraft) — no
una mecánica obligatoria. Normalmente los animales viven **LIBRES en su hábitat**, haciendo
lo que el cerebro colmena decida (mundo vivo). El encierro es sólo para OBSERVARLOS al
testear. En el diseño esto ya se cumple: al residente sólo lo confinan **paredes/puertas
reales**; sin muros, roaming libre. El "seguro de puerta" (que una puerta no se abra) es una
comodidad de debug opcional, no el corazón del sistema.

## 0.5 Principios de diseño — MUNDO VIVO (Minecraft + Project Zomboid)

Norte (usuario, 2026-07-26): **la vida y los biomas están AHÍ y viven SIN el jugador**; el
jugador llega, interactúa y se divierte. Es un JUEGO → **diversión > realismo** (el realismo
mata la diversión). Esto es el ESQUELETO (MVP); se pule después. Diseñar con patrones, como
ingeniero + game designer.

**Patrones/reglas que hacen que el mundo se sienta vivo y ESTABLE (no "desaparece de la nada"):**
1. **Mortalidad separada del metabolismo.** El metabolismo (hambre/sed/energía) sesga la
   conducta pero **NUNCA mata** (piso de energía `STARVE_FLOOR`). Como en Minecraft, los
   bichos no se esfuman solos. → *arregla el bug "desaparecen de la nada".*
2. **La muerte viene de causas VISIBLES:** **PREDACIÓN** (un depredador caza a una presa
   cercana) y el **jugador**. Eso da la **selección** para la evolución + gameplay, sin
   desapariciones misteriosas. Los NPCs igual: sólo mueren si el jugador los mata.
3. **Datos-en-todos-lados + activación por proximidad** (F2, patrón "chunk activation"):
   los seres persisten como datos en TODO el mundo; se simulan/renderizan sólo cerca del
   jugador (o de una cámara). Barato + el mundo no se pierde.
4. **Fuente única de percepción** (`fauna_brain.rs`): el gym y el juego comparten el sensor
   o la conducta no transfiere. Determinismo.
5. **Emergencia, no scripting** (estigmergia + cerebros IAFZ): las manadas/huidas/
   territorios/sociedades surgen; no se podan ([[ai-life-emergence-no-prune]]).
6. **Creativo = laboratorio de DEBUG:** spawnear/encerrar/feeder/depredador/overlays son
   herramientas para VER que se comporten y evolucionen bien — NO mecánicas de gameplay. En
   juego normal los seres sólo se ven y se interactúa con ellos cuando el jugador quiere.

## 1. Estado actual (reusar, no reinventar)

- **`fauna.rs`** — ecosistema AMBIENTAL efímero: la fauna se _streamea_ (la que se aleja se
  reubica al borde), población acotada, cerebro a **15 Hz** + pausa en background. Genoma
  **IAFZ-64**; come / reproduce (hereda mutado) / muere → selección natural. Ids `0..48`.
  *Esto se queda como "vida de fondo" barata; NO es lo persistente.*
- **`fauna_brain.rs`** — sensor 32 bits COMPARTIDO gym↔juego (paredes, bioma, comida,
  energía, social). Fuente única. Bits nuevos van acá o no transfieren del gym.
- **Estigmergia** (`SignalField`) — feromonas → manadas/huidas/territorio emergentes.
- **`iafz-sys`** — `load_bytes/init/think/mutate/crossover/reset_echo/save/load`,
  `MAX_AGENTS=1000`. `save/load` a archivo → **persistencia del genoma evolucionado**.
- **`editor.creative`**, **`build.rs`** (paleta → celda apuntada → hold USE, costo saltado
  en creativo). Terreno sólido = paredes/puertas **ya confinan**.
- **Persistencia** — patrón `crate::inventory_persist::persist_dir()` + serde JSON (ej.
  `tiles.json`): dir por plataforma (Android files / env / `.`).

## 2. Modelo: fauna RESIDENTE = registro persistente + activación por proximidad

La residente (la que colocás y tu granja) NO es una entidad efímera: es un **REGISTRO de
datos** (`ResidentRecord`: cuerpo, x, z, rumbo, especie, needs, genoma) que vive en el
mundo **siempre**, en TODOS los cuerpos a la vez. Una **entidad Bevy renderizada** existe
sólo mientras el registro está **ACTIVO**:

- **Activación** = el registro está a ≤ `RENDER_R` del jugador **O** de una cámara activa
  (§0). Activo → spawnea sprite + corre cerebro/metabolismo/repro. Inactivo → sin sprite,
  **congelado** (o simulado grueso), pero el registro (posición/genoma) **se conserva**.
- **Persistencia a disco** (`vivarium/…`): registros en JSON + genoma por `iafz_sys::save`.
  Me alejo → se guarda; vuelvo (o reinicio el juego) → se re-spawnea en su lugar con su
  genoma evolucionado. **Nunca desaparecen.**
- **Por cuerpo**: sólo se instancian entidades de los registros del cuerpo donde estás (o
  donde mira una cámara). La granja lunar sigue existiendo como datos mientras estás en la
  Tierra.

Comparte `Animal/Needs/FaunaPose` y el mismo `tick_fauna` (metabolismo/cerebro/repro): no
se duplica la lógica. La única diferencia: la residente **no se streamea**, tiene su pool
de ids/censo, **se activa por proximidad** y **persiste**.

## 3. El bucle de aprendizaje (gym → mundo → persistencia)

1. **Gym** enseña las BASES (forrajeo, evitar paredes, agua, amenaza) → `FAUNA.adn`.
2. En el hábitat residente la evolución CONTINÚA bajo la presión que diseñás (cuarto
   cerrado, comida escasa, agua, depredador).
3. **Persistir el genoma** → la conducta acumula divergencia del seed entre sesiones:
   cuanto más viven, más se alejan de lo que el gym enseñó.

## 4. Salvaguardas anti mínimos/máximos locales

Población chica y encerrada → converge/estanca (mínimo local) o queda atrapada en un pico
(máximo local). Mecanismos baratos, con perilla, que **preservan diversidad sin imponer
conducta**:

1. **Piso de mutación** (`MUTATE_RATE≥0.06`, nunca 0). *(F1 ✓)*
2. **Hipermutación esporádica** (~1/5 nacimientos, tasa ×4 → salto exploratorio). *(F1 ✓)*
3. **Inmigración del seed** (cada N nacimientos el hijo nace del gym, no del padre). *(F1 ✓)*
4. **Arca de diversidad** — pool de genomas DISTINTOS (`iafz_sys::save`), se reinyecta y
   persiste. *(F3)*
5. **Diversidad + temperatura adaptativa** — firma barata del genoma; si la varianza cae,
   subir mutación/inmigración hasta recuperar. *(F3)*
6. **Catástrofes / estacionalidad** (sequía, invierno, pulso de depredador). *(F3/F6)*
7. **Nichos por planeta (island model)** + `crossover` ocasional entre linajes. *(F3)*

## 5. Fases

- **F1 — Spawn creativo de residente** ✅: paleta creativa + `Resident` (no streamea) +
  pool/censo propio + confinamiento + evolución en el lugar + salvaguardas baratas.
- **F2 — Persistencia + activación por proximidad** *(este pedido)*: `ResidentRecord`
  persistente por cuerpo (`vivarium/` JSON + genomas `iafz_sys::save/load`); las residentes
  NO desaparecen al alejarte — se guardan como datos y se **renderizan/simulan sólo cerca**
  (cull por `RENDER_R`); se re-spawnean al volver o reiniciar. Reemplaza el miedo al
  "círculo de streaming" para la fauna que te importa.
- **F3 — Salvaguardas completas**: arca de diversidad + medida de diversidad + temperatura
  adaptativa + crossover/nichos.
- **F4 — Observación (creativo)**: overlay barato (censo, generación, diversidad, edad).
- **F5 — Fauna POR PLANETA (inventada + temperamento)**: cada cuerpo tiene un
  **TEMPERAMENTO** (amigable / neutral / tenebroso / hostil) que, con el bioma, define su
  fauna INVENTADA (color/tamaño/agresividad, determinista por cuerpo → variedad en todo el
  universo sin arte a mano; fallback a quad tintado). La fauna **responde al entorno**: en
  un planeta hostil te acosa → si no vas preparado, te obliga a **retirarte y volver
  listo** (rompe la monotonía; la exploración pesa). La ambiental spawnea la fauna del
  cuerpo donde estás (no vacas en Marte). *(este turno)*
- **F6 — SOCIEDAD viva (colonos / NPCs con MEMORIA)**: NPCs por planeta con simulación de
  **sociedad/vida** — evolucionan y **RECUERDAN**: si matás al padre de alguien, esa
  familia te recuerda y reacciona (rencor, reputación, relaciones/familias). Reusa el
  cerebro IAFZ + estigmergia + persistencia; agrega **memoria episódica por-NPC del jugador**
  (quién sos para ellos) y **grafo de parentesco/reputación**. Grande — sub-plan propio.
- **F7 — Cámara = PIEZA de nave (sonda)**: la cámara es **una pieza más del ship builder**
  (como armar un Voyager/rover); le sumás una **antena** y mandás la sonda a cualquier
  planeta; con antena te **conectás desde la Tierra (o donde estés)** y ves renderizado lo
  que enfoca allá (fauna/entorno del cuerpo remoto). Reusa: `ship_builder` (pieza+antena),
  el sistema de sonda/vuelo, y la **activación de F2** (la cámara es 2da fuente de
  activación → simula+renderiza los residentes/fauna del cuerpo remoto).
- **F8 — Herramientas de ecosistema (debug creativo)**: colocar fuente de comida / agua /
  un depredador para **diseñar la presión selectiva** del hábitat y testear conducta. Reusa
  la paleta creativa (como el spawn). No depende de decisiones del usuario.

## 6. Riesgos / no-regresión

- **Perf/batería**: residentes acotados (`MAX_RESIDENTS`), aparte del `MAX_POP` ambiental;
  `think` a 15 Hz + pausa background; inactivos NO consumen (congelados). Validar en Redmi
  (memoria `pending-redmi-perf-tests`).
- **Determinismo del sensor**: bits nuevos en `fauna_brain.rs` o no transfiere del gym.
- **No podar lo emergente**: salvaguardas = diversidad, no conducta impuesta.
- **UI ASCII-only** (memoria `game-font-ascii-only`). Ids: ambiental `0..48`, residentes
  `48..48+MAX_RESIDENTS` (cabe en `MAX_AGENTS=1000`).

## 7. Estado

- [x] **F1** — spawn creativo de residente + confinamiento + evolución + salvaguardas
  baratas. Compila verde. Pendiente Redmi.
- [x] **F2** — persistencia + activación por proximidad. `ResidentRecord` persistente por
  cuerpo en `vivarium/residents.json` + genomas `g_<uid>.adn` (`iafz_sys::save/load`);
  `manage_vivarium` instancia sólo el cuerpo activo (snapshot+despawn al salir, respawn al
  entrar) → los residentes NO se pierden; `cull_residents` los oculta y `tick_fauna` los
  congela lejos del jugador (render/sim sólo cerca); guardado periódico + al pausar.
  Compila verde. Pendiente Redmi.
- [x] **F3** — termostato de diversidad (temperatura evolutiva adaptativa): mide la
  entropía de conducta de los residentes (acción×rumbo) y sube mutación/inmigración cuando
  la diversidad colapsa → escapa de mínimos/máximos locales. Compila verde. (Arca de
  diversidad + crossover/nichos quedan como refinamiento futuro — el gym cubre esa mitad.)
- [x] **F4** — overlay de observación (DEBUG, sólo creativo): censo/nac/temp/diversidad/
  energía arriba-izquierda, ASCII. Compila verde. Pendiente verificación visual.
- [x] **F5** — fauna por planeta (inventada + temperamento): `Temperament` por cuerpo
  (Tierra=amigable, Luna=tenebrosa, planetas deterministas), `FaunaCatalog` genera especies
  por cuerpo (color/tamaño/agresividad; fallback quad tintado), la ambiental spawnea la
  fauna del planeta donde estás, y la HOSTIL acecha al jugador. Compila verde. Pendiente
  Redmi. (Daño/combate real de la hostil = follow-up; el spawn creativo usa slots genéricos
  que resuelven contra la fauna del planeta.)
- [x] F6 — SOCIEDAD viva con memoria (F6.1–F6.4 hechas): `society.rs` — NPCs con cerebro +
  familia + **reputación persistente del jugador** + evento de daño=rencor; **evolución**
  (genoma persiste/muta, hijos heredan familia+memoria); **roles/hostilidad de grupo** +
  recambio generacional; **nativos por planeta** con disposición según temperamento (F5).
  Sub-plan `PLAN_SOCIEDAD_MEMORIA.md`. Falta sólo pulido (comercio, F6.x). Compila verde.
- [~] F7 — cámara-sonda: **v1 hecho** (`camera_feed.rs`): las sondas que mapean un planeta
  dejan una CÁMARA; botón CAM → feed de telemetría con la fauna del planeta remoto +
  temperamento. Sub-plan `PLAN_CAMARA_SONDA.md`. Falta F7.2b (vista de mundo real remoto),
  F7.3 (señal), F7.4 (codex). Compila verde.
- [x] F8 — herramientas de ecosistema (paleta creativa): **comida** (feeder `PlacedFood`),
  **agua** (tile de abrevadero), **depredador** (residente forzado hostil que acecha) →
  diseñás la presión del hábitat para testear conducta. Compila verde.
</content>
