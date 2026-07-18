# ASSET_EDITOR_PLAN — editor de assets del juego (ha-asset-tools)

Plan por fases para evolucionar el editor de "asignar 1 PNG por slot" a un
**slicer de sprite-sheets** con grilla, selección por arrastre, poses
direccionales y animación por rango de celdas — con preview 1:1 al juego.

> Documento de retome: si nos quedamos sin tokens, seguí desde la primera fase
> sin ✅. Cada fase deja el crate compilando (`cargo build -p ha-asset-tools`) y,
> si toca motor, `cargo check -p ha-game` verde.

## Estado actual (hecho)

- Crate `native/crates/ha-assets` (headless): `manifest.ron`/scan/catalog (LimeZu),
  `slots::game_slots()` (claves que usa el juego), `art_manifest` (modelo del
  `manifest.json` del juego, compat viejo string + nuevo objeto con `anim`),
  `registry`, overlay de mods. Tests verdes.
- Editor `native/crates/ha-asset-tools` (egui, standalone, corre aparte):
  lista de slots en árbol colapsable, PNG picker recursivo, preview candidato +
  "cómo se ve en el juego", CRUD de slots, filtro "sin PNG", selector de carpeta
  nativo (rfd/xdg-portal), animación por slot (frames/fps — metadata).
- Motor `ha-game/art2d.rs`: lee el manifest nuevo; animación genérica por
  frame-strip (`ArtAnimated` + `setup_art_atlases` + `tick_art_animations`);
  reskin de decor/items/machines/player/ship y joysticks (UI).
- Lanzador `./editor_assets` (gui/scan/catalog/play).

## Objetivo nuevo

1. Fix UX de previews (no ocupar toda el área; candidato al lado, tamaño acotado).
2. Un PNG puede tener **muchas regiones** (poses/frames): grilla sobre la imagen,
   **arrastre** para elegir de-celda-a-celda, nombrar la región.
3. **Poses direccionales** para personajes: idle/run × up/down/left/right, +
   crear poses nuevas (sit, sleep, …).
4. **Animación por rango de celdas** (no escribir fps a mano): arrastrás el rango,
   los frames salen del rango; fps como slider con **preview reproducido**.
5. El motor consume regiones/poses/animación (para que se vea en el juego).

## Modelo de datos (evolución de `art_manifest.rs`)

Extender `ArtEntry` para soportar regiones dentro de una hoja. Compatible hacia
atrás (todo opcional):

```rust
struct ArtEntry {
    png: String,
    grid: Option<(u32,u32)>,           // celda px (default 32)
    rect: Option<CellRect>,            // recorte del sprite estático dentro del png
    anim: Option<ArtAnim>,             // { frames, fps } — o derivado de `strip`
    strip: Option<CellRect>,           // rango de celdas de una animación 1-fila
    poses: Option<BTreeMap<String, Pose>>, // "idle_down", "run_left", …
}
struct CellRect { col:u32, row:u32, w:u32, h:u32 }   // en celdas
struct Pose { rect: CellRect, anim: Option<ArtAnim> } // rect estático o tira animada
```

`CellRect` en celdas (no px) para que el arrastre en la grilla sea directo. El
editor guarda; el juego resuelve celda→px con `grid`.

## Progreso

- ✅ Fase A (previews acotados + lado a lado + tooltip fps)
- ✅ Fase B (canvas con grilla + zoom)
- ✅ Fase C (selección por arrastre snap → CellRect; guardar recorte)
- ✅ Fase D (poses: guardar como pose nombrada/estándar + CRUD) — falta consumo
  del jugador en motor (mapear poses → sistema direccional, ver Fase F).
- ✅ Fase E (animación por rango + preview reproducido)
- ◑ Fase F (motor: rect + strip ✅; **poses del jugador pendientes**)
- ☐ Fase G (mapear/CRUD de assets nuevos — parcial vía slots CRUD)
- ☐ Fase H (UI hearts/hotbar/botones + tiles/agua + docs + APK)

## Fases

### Fase A — Fix UX de previews (rápido)
- Acotar el tamaño de los previews (candidato y "en el juego") a ~128 px máx,
  preservando aspecto → la lista de PNGs mantiene scroll.
- Poner ambos previews **lado a lado** (actual | candidato); al cambiar la
  selección de la lista, se actualiza el candidato.
- Explicar `fps` en la UI (tooltip: "cuadros por segundo — velocidad").
- Archivos: `ha-asset-tools/src/gui.rs`.

### Fase B — Canvas de hoja con grilla + zoom/pan
- Panel/ventana que dibuja el PNG grande con una **grilla** (tamaño de celda
  configurable, default 32) encima, con zoom (rueda) y pan (arrastre medio).
- Usar `egui::Painter` sobre un `Image`; dibujar líneas de grilla y coordenadas.
- Archivos: `gui.rs` (+ posible `sheet_canvas.rs`).

### Fase C — Selección de región por arrastre (snap a grilla)
- Arrastre con el mouse sobre el canvas → rectángulo **snap** a celdas →
  `CellRect`. Resaltar la celda/rango. Mostrar `col,row,w,h`.
- Botón "Guardar región como…" con nombre → agrega a `poses`/`rect`.
- Archivos: `gui.rs`/`sheet_canvas.rs`, modelo en `ha-assets/art_manifest.rs`.

### Fase D — Poses direccionales (personajes)
- UI de poses: lista de direcciones estándar (down/up/left/right) × estados
  (idle/run/…); seleccionás una y marcás su `CellRect` en el canvas.
- "Crear pose" (nombre libre: sit, sleep). CRUD de poses.
- Archivos: `gui.rs`, `art_manifest.rs`, `slots.rs` (poses estándar sugeridas).

### Fase E — Animación por rango + preview reproducido
- Marcar un **rango de celdas** como animación (`strip`); frames = celdas del
  rango. `fps` como slider. **Preview que reproduce** la animación en el editor
  (timer egui, ctx.request_repaint) → 1:1 con el juego.
- Reemplaza el "frames/fps a mano" por el arrastre (deja fps editable).
- Archivos: `gui.rs`.

### Fase F — El motor consume regiones/poses/animación
- `art2d.rs`: al cargar, si un entry trae `rect`/`strip`/`grid`, construir el
  `TextureAtlas` (celda→px) y, para `poses`, exponer por dirección.
- Personaje: mapear las poses del manifest al sistema direccional existente
  (`scene.rs::spawn_player` + `sim.rs::face_player_sprite`) en vez del layout
  hardcodeado. Fallback al actual si no hay poses.
- Archivos: `ha-game/art2d.rs`, `scene.rs`, `sim.rs`. Verificar `cargo check`.

### Fase G — Mapear assets nuevos / CRUD de regiones
- El editor permite crear nuevas claves-slot con múltiples regiones y guardarlas;
  el juego las descubre. Aviso de PNGs/regiones sin usar.
- Archivos: `gui.rs`, `art_manifest.rs`.

### Fase H — Wire de UI restante + tiles + docs/APK
- Reskin de hearts/hotbar/botones/menú (patrón del joystick) en `hud.rs`/
  `actions.rs`. Texturizado de tiles (agua) — atlas de tiles.
- Actualizar `labiblia.md` + `ASSET_SYSTEM_ARCHITECTURE.md`; publicar APK.

## Notas de retome
- El editor NO corre con el juego (dev-only). Verificación visual = usuario en
  su desktop COSMIC/Pop!_OS; yo verifico por compilación + tests.
- Grid del juego = 32×32. Singles = 1 sprite por PNG; sheets = regiones por celda.
- Compat: todo campo nuevo del manifest es `Option` → el juego viejo no rompe.
