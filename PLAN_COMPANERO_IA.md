# PLAN_COMPAÑERO_IA — La "mano derecha" (IA frugal, maestro→aprendiz)

> **Estado: F1 IMPLEMENTADO 2026-07-26.** Compañero persistente controlado por la
> IA frugal del usuario (IAFZ-64 / `iafz-sys`), que **sigue al jugador**, **aprende
> conocimiento base del maestro** (el tester-IA o el jugador) y **evoluciona solo**
> reusando la evolución que el juego YA tiene (genoma + mutación + persistencia).
> Módulo `crates/ha-game/src/companion.rs`.

## 0. Qué pediste

Un compañero-IA que sea mi **mano derecha**, controlado por **mi IA frugal**. El
**tester** (u yo) es su **maestro**: le da el **conocimiento base** y de ahí el
compañero **evoluciona por sí mismo** — "algo que el juego ya tiene" (la fauna
evolutiva: come/reproduce/hereda genoma mutado/muere = selección natural in-game).

## 1. La clave: no es un sistema nuevo, es reusar la IA frugal + su sensor

El juego ya tiene todo el aparato:
- **`iafz-sys`** (IAFZ-64, FFI a C): `think(id, sensor32)->f32`, `mutate(src,tgt,rate)`,
  `crossover`, `load/save`. `MAX_AGENTS=1000` cerebros. La fauna usa ids `0..72`.
- **`ha_shared::fauna_brain::sense(...)`**: codifica percepción en 32 bits (paredes,
  bioma, **dirección a la "comida"**, energía, vecinos). Es la MISMA fuente de verdad
  que el gym que entrenó el genoma → la conducta aprendida transfiere.
- **`FAUNA.adn`**: genoma **ya entrenado por neuroevolución** para forrajear (buscar la
  comida por el canal de dirección) y evitar obstáculos. **Ese es el "conocimiento
  base" que sembró el maestro-gym.**

**Truco de diseño (elegante, sin ML nuevo):** al compañero le pasamos la **posición
del JUGADOR como la única "comida"** en `sense()`. El genoma base ya sabe "andá hacia
el sector de la comida y esquivá paredes" → **el compañero sigue al jugador con su
conocimiento base**. El jugador/tester se vuelve, literalmente, el objetivo que la IA
frugal persigue: **el maestro**.

## 2. Maestro → aprendiz → evolución sola (el bucle)

- **Cerebros reservados:** `CHAMPION_ID = 990` (el genoma vigente) y `TRIAL_ID = 991`
  (una copia mutada en prueba). Fuera del rango de la fauna (0..72), dentro de 1000.
- **Fitness (qué premia ser buena mano derecha):**
  `prox` (cercanía sostenida al jugador) `+ imit` (que su velocidad apunte parecido a
  la del jugador = imitar al maestro) `− stuck` (chocar paredes). Todo se acumula en
  una ventana de `EVAL_SECS`.
- **Hill-climb online:** cada `EVAL_SECS`, si el `TRIAL` acumuló MÁS fitness que el
  `CHAMPION`, el trial **se promueve** a champion (`mutate(trial→champion, 0)` = copia)
  y se **guarda a disco** (`companion.adn`). Luego se genera un nuevo trial
  `mutate(champion→trial, RATE)`. El compañero **corre con el champion**; el trial se
  evalúa en paralelo cada cierto ciclo (round-robin barato). Así **aprende y evoluciona
  por su cuenta**, sembrado por el conocimiento base y guiado por el maestro.
- **Persistencia:** `companion.adn` (junto al vivario). El conocimiento **se acumula
  entre sesiones** — no empieza de cero cada vez.

## 3. Anti-regresión / presupuesto

- **Barato:** un solo `think()` por frame (throttle a ~15 Hz como la fauna). Cero costo
  de render extra sin arte (quad de color; slot lógico `npc/companion` para cuando lo
  texturices).
- **Aislado:** módulo propio; NO toca `fauna.rs` ni el ecosistema (ids y archivo
  separados). Si algo falla, el compañero no arrastra al resto.
- **Determinista** del lado sim: usa el mismo `sense()` compartido. El server survival
  no ve al compañero (es cliente, como el vuelo Ki).
- **Sin mínimos locales:** el hill-climb sólo ACEPTA mejoras; para no congelarse, cada
  N ciclos sin mejora sube `RATE` (más exploración) — igual que la salvaguarda del gym.

## 4. Fases

- **F1 (HECHO):** spawn del compañero cerca del jugador; sigue por el genoma base;
  hill-climb online + persistencia; render quad (slot `npc/companion`).
- **F2:** acciones de asistencia (talar/picar/recoger cerca del jugador; alertar de
  peligro) — se suman como bits de acción del `think` y como términos de fitness.
- **F3:** que el compañero **se reproduzca/herede** dentro del ecosistema de fauna
  (colmena): su genoma alimenta el pool de colonos (§8 B4 del PLAN_MAESTRO).
- **F4:** generalizar a **colonos/NPCs** (un genoma por rol) — la mano derecha es el
  primer colono con cerebro; el resto de la ciudad sale de acá + estigmergia.

## 5. Cómo verlo

Desktop: corre con `HA_TESTER=1` (el tester es el maestro) — el compañero aparece al
lado del jugador y lo sigue. Log `ha-companion: champion mejorado fit=...` cada vez que
evoluciona. Genoma en `vivarium/companion.adn`.
