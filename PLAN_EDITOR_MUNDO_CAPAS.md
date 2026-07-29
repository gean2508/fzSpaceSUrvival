# PLAN — Editor de MUNDO por CAPAS + hacia editor de MOTOR

> Diseño (teoría primero, por si faltan tokens; después se ejecuta). Editor actual:
> `native/crates/ha-asset-tools/src/world_gui.rs` (egui, `./editor_assets mundo`). Reglas de
> rendimiento vigentes. Relacionado: `ASSET_SYSTEM_ARCHITECTURE.md`, `PLAN_HERRAMIENTAS_REMOTAS.md`.

## 0. Visión (usuario, 2026-07-26)

El editor de mundo debe ser un **inspector/editor de ESCENA por CAPAS**: no sólo el mapa de
tiles, sino **TODO el mundo** organizado por capa — suelo base, las capas encima, en qué capa
está el **jugador**, y que **casas, árboles, props, animales, absolutamente todo** aparezcan
en su capa correcta. Sirve para **verificar y acomodar** que cada cosa esté en la capa que
toca (bug real: un muro con el nombre del usuario quedó tapado por árboles → problema de
capas). A futuro: **un solo programa** que integre todas las herramientas (assets/mundo/
sonido), como un **motor de videojuegos**, y el juego pasa a ser un **MOD que consume el
motor**. Por ahora sólo el editor de mundo + capas + que carguen TODOS los assets.

## 1. Modelo de CAPAS/Z del juego (fuente de verdad para el editor)

El editor debe reflejar EXACTAMENTE cómo el juego organiza la profundidad (`ha-game`):

- **Z-LEVEL (piso/profundidad)** — `TileWorld.z`: `0` = superficie; `<0` = subsuelo (minas,
  hasta `MIN_Z_LEVEL`); `1..3` = pisos superiores interiores. Cada z-level es un mapa de
  tiles propio. El jugador está en un z-level a la vez.
- **DEPTH de render dentro de una capa** — `world2d::sort_z(world_z) = 2.0 + world_z*0.001`
  (Y-sort: lo más "al sur"/mayor z-mundo dibuja ADELANTE). Convenciones:
  - Suelo base (mesh/quads de terreno) y pisos (`floor/*`): depth BAJA (~1.0) → todo va encima.
  - Tiles sólidos con arte (muros `wall/*`, autotile) y **decor** (árboles/rocas/props):
    **Y-sort** (`sort_z(wz)`) → el jugador camina por detrás/delante según su z-mundo.
  - Cursor/retículo: depth alta (~6.5).
- **CATEGORÍAS de contenido por celda** (lo que el editor debe listar/mostrar por capa):
  1. **Terreno** (Tile natural + edición del jugador: grass/sand/water/rock/wood_wall/…).
  2. **Arte de tile** (autotile de agua/muros/pisos, `spawn_chunk_tile_art`).
  3. **Decor** (árboles/rocas/depósitos, `surface_decor_at`; flora/props `nature/*`,`prop/*`).
  4. **Estructuras/prefabs** (asentamientos generados + `world_edits.json` del jugador).
  5. **Entidades** (jugador, fauna residente/ambiental, NPCs, máquinas, pads).

**Regla de corrección de capas (bug de árboles/muro — YA arreglado en el juego):** el decor
**NUNCA** se spawnea sobre un tile SÓLIDO (natural o construido) — `spawn_chunk_decor` ahora
chequea `tiles.solid_at` (incluye ediciones). El editor debe **mostrar el decor/props** y
**evitar** que edites terreno donde hay decor cosechable sin avisar (paridad con el juego).

## 2. Qué está mal hoy (y por qué)

- El editor sólo pinta **tiles** y coloca **assets grandes** (`large_asset`); NO muestra
  decor, props, entidades, ni z-levels. Las "capas" eran H3/H4 pendientes.
- **No cargan todos los assets:** sólo ofrece `large_asset_count()` (assets grandes). Falta
  leer el **catálogo completo** (`manifest.json` de arte: tiles, decor, nature, prop, npc,
  ui) para poder ver/colocar CUALQUIER asset.
- No tiene en cuenta props/decor al editar → pisás cosas sin verlas (origen del bug).

## 3. Diseño del editor por capas

- **Panel de CAPAS (z-levels):** selector `subsuelo … 0(superficie) … pisos`, con indicador
  de **en qué capa está el jugador** (si hay una instancia de juego, vía un archivo de estado;
  si no, sólo edición). Cada capa se puede **mostrar/ocultar** y **aislar**.
- **Overlays por CATEGORÍA (toggle):** terreno / arte-tile / decor / estructuras / entidades.
  Cada celda muestra, apiladas, sus categorías → ves "acá hay grass + un árbol + (en juego)
  un animal". Resalta conflictos (decor sobre sólido, prop sobre muro).
- **Carga del catálogo COMPLETO:** leer `crates/ha-game/assets/art/manifest.json` → listar
  todos los slots por categoría; el pincel puede colocar tiles, decor, props, estructuras.
  Preview del PNG (recorte del atlas) como en el editor de assets.
- **Data-driven, sin inventar rects** (regla del proyecto): todo por ID lógico + manifest.
- **Rendimiento:** el editor es dev-tool (egui, desktop) — no afecta el juego. Pero la vista
  se limita a una **ventana** alrededor del cursor (no renderizar el mundo infinito); culling
  por región visible.

## 3.5 Enabler arquitectónico (clave para E3 y para el MOTOR)

Hoy la generación DETERMINISTA del mundo (terreno + colocación de decor/props: `topdown_tile`,
`surface_decor_at`, `pick_tree_species`, `nature/prop_decor_at`, hashes) vive en
**`ha-game::world2d`** (crate del juego, con Bevy). El editor (`ha-asset-tools`) NO puede
llamarla → por eso NO puede mostrar árboles/props/entidades por capa. **Paso clave:** extraer
esa lógica pura (sin Bevy) a un crate **compartido** (`ha-shared` o nuevo `ha-worldgen`) que
consuman **el juego Y el editor** (single source of truth de la generación). Entonces el
editor puede pintar exactamente lo que el juego generará (decor incluido) por celda y por
capa. **Este es el mismo paso que habilita el "motor"** (lógica compartida; el juego = mod).
Refactor grande pero de bajo riesgo (mover funciones puras + tests de round-trip). Requisito
de E3/E4 y de E5.

## 4. Hacia el editor de MOTOR (futuro)

Unificar `assets` + `mundo` + `sonido` en **un solo programa** con pestañas (comparten
catálogo/manifest), como un editor de motor. El juego = **mod** que consume el motor
(`ha-game` sobre `ha-engine`). El hub web (`PLAN_HERRAMIENTAS_REMOTAS.md`) es la versión
thin-client remota del mismo set. No rehacer lógica: ambos editan los mismos archivos
data-driven.

## 5. Fases (ejecución)

- [x] **E0 — Fix de capas en el JUEGO:** decor no spawnea sobre tiles sólidos/construidos
  (árboles ya no tapan el muro). Compila verde.
- [~] **E1 — Selector de CAPA (z-level)** etiquetado (subsuelo/superficie/pisos ▼▲⌂) en el
  editor. Hecho + compila verde. Falta el **indicador de la capa del jugador** (necesita un
  archivo de estado juego→editor) y ver el CONTENIDO de cada capa (E3, requiere el enabler §3.5).
- [x] **Enabler §3.5 HECHO:** generación de decor extraída a **`ha_shared::worldgen`** (sin
  Bevy, `[f32;2]`/`[f32;3]`); el JUEGO la consume (re-export en `world2d`) y el EDITOR también
  → fuente única (base del motor). Juego + editor compilan verde, comportamiento idéntico.
- [~] **E2/E3 (primer paso) HECHO:** el editor ahora **DIBUJA el decor** (árboles=verde,
  rocas=gris, flora/props=puntitos) desde `ha_shared::worldgen`, sólo en superficie y NUNCA
  sobre tiles sólidos/construidos (paridad con el juego) + leyenda. Ves qué generará el mundo
  y por qué tu muro no queda tapado. Falta: mostrar ENTIDADES (fauna/NPCs/máquinas) y
  colocar/editar decor+props desde el editor (requiere extender `world_edits` con decor).
- [ ] **E4 — Props/decor awareness al editar** (mostrar + evitar pisar sin avisar).
- [ ] **E5 — Editor de MOTOR unificado** (assets+mundo+sonido en un programa) + juego-como-mod.

## 6. Reglas

- Perf del juego intacta (el editor es aparte). Fuente de verdad de arte = `manifest`
  (no inventar rects/offsets). Fuente de conducta = `ha-shared`. UI ASCII donde aplique.
</content>
