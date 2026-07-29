# DISEÑO — Editor de Mundo / Mapa (APROBADO 2026-07-25)

> **Estado: APROBADO por el usuario** (con las aclaraciones de §9). Plegado a
> `PLAN_MAESTRO §3.H` + fases §7. Se ejecuta después de cerrar las fases previas y
> ANTES de la IA frugal (fase 8, que va al último).

## 9. Aclaraciones del usuario (2026-07-25) — lo que ajusta el diseño

**No pintar todo el mapa.** El mundo sigue siendo **procedural**; el editor es para
**cosas puntuales** y para **corregir** lo que el código pone mal.

1. **Ciudades/barrios por PLANTILLA, no pintadas celda a celda.** El problema real:
   las calles/barrios generados por código salen raros (no conectan). Solución: vos
   **autorás una PLANTILLA de ciudad** (layout de calles+edificios+plaza, una vez,
   bien conectada) en el editor, y el **código la LEE y la UBICA** donde encaje
   (terreno apto, sin pisar agua, orientada). Sigue siendo procedural ("dónde"), pero
   el "cómo se ve la ciudad" lo definís vos → calles que conectan como deben.
2. **Editar la ciudad base generada:** si el código genera una ciudad base y no te
   gusta un edificio, en el editor lo **quitás / movés / reemplazás**; si no te gusta
   cómo se ve, le cambiás la **textura** (eso ya en el editor de assets). Edición
   QUIRÚRGICA de objetos individuales, no repintar todo.
3. **Ediciones puntuales del terreno:** ej. dos lagos y querés una **cascada** en el
   medio → editás sólo esos bloques y ponés agua/cascada. Cambiar UNA cosa específica.
4. **Capas por asset (estilo Godot tilemap):** cada asset/tilemap (p.ej. una casa)
   declara sus **capas**: `suelo`, `subsuelo`, **`luces`**, `techo` (se oculta cuando
   el jugador pasa atrás), `puerta`. Para ocultar el techo, poner colisiones exactas,
   y las **luces**. Todo controlado desde el editor, no por código.
5. **Colisión exacta pintada sobre el asset — ✅ HECHO** en el editor de assets
   (sección "🗺 huella": se ve el sprite y pintás las celdas sólidas encima;
   clic-derecho = ancla). Era el paso Godot-style que pediste.
6. **Día/noche + luces por asset:** de noche el ambiente del mundo se **oscurece**;
   si una casa tiene una capa de **luz** (un foquito exterior), esa luz **ilumina sólo
   su entorno** (glow aditivo), no todo el mundo. Necesita: (a) capa `Light` por
   asset, (b) oscurecimiento ambiental nocturno (ya hay `sky2d::DayClock`), (c) render
   aditivo de las luces. → Nueva sub-fase del editor/lighting.

**Complemento (mi aporte):** la "plantilla de ciudad" es lo que la industria llama
**structure template / prefab de asentamiento con reglas de encaje** (jitter,
rotación, snapping a terreno). Lo modelamos como un `world_edits.json` "region:
city_template" + un colocador determinista que respeta terreno. Y las capas se
apoyan en `ArtLayer` (ya existe Floor/Wall/Object/Roof) extendido con
`Basement`/`Light`.

## 0. Tu idea (como la entendí)
Un programa aparte —hermano del editor de assets y del mini-DAW de sonido— donde
vos **dibujás/editás el mapa** sobre el mundo que YA tenemos, poniendo cosas donde
quieras, y eso **se pasa al juego**. WYSIWYG: lo que ves en el editor, es lo que
hay en el juego.

## 1. El insight clave (lo que complemento)
Nuestro mundo **no es un mapa pintado a mano fijo**: es **procedural y
determinista** (por semilla: biomas, terreno, árboles/rocas por hash de celda) +
**capas de override que persisten** (`tiles.json`, `harvested.json`,
`roofs.json`, `structures.json`). Terraria/Minecraft/Starbound hacen igual: mundo
procedural + plantillas de estructura + reglas de bioma; **nadie dibuja a mano un
mundo infinito** — se autoran POIs, regiones y plantillas, y lo procedural rellena.

**Conclusión de diseño:** "dibujar el mapa" = **autorar esas capas como DATOS** que
el juego carga en boot, EXACTO como `manifest.json` (arte) y `audio.json` (sonido).
El editor de mundo es la **tercera herramienta data-driven**, y reusa TODA la infra
que ya construimos (tiles, prefabs, estructuras A2, huellas A1, catálogo de slots,
formatos de persistencia). No inventa nada nuevo del motor: solo **autora** lo que
hoy el jugador coloca en runtime, pero con poderes de dios, pinceles y sin costo.

**Dos modos de autoría complementarios:**
1. **Overrides puntuales** — pintás tiles/decor/estructuras/edificios/spawns en
   celdas concretas (como el sistema de construcción del jugador, pero como editor).
2. **Regiones/zonas** — dibujás un rectángulo y le asignás una regla: "esto es un
   pueblo/hub", "acá el bioma es agua/desierto", "spawn del jugador acá", "POI/quest
   acá", "acá NO generar nada". Esto **escala** (no pintás lo infinito; pintás POIs
   + regiones y lo procedural rellena).

## 2. Por qué conviene hacerlo AHORA (y qué fases absorbe)
El editor de mundo es la forma CORRECTA de crear contenido, y **subsume varias
fases pendientes**:
- **A3 hub terrestre** y **A4 ruinas off-world** → dejan de ser generadores
  procedurales a codear: los **pintás a mano** en el editor. Más control, menos código.
- Interiores/decorado de **verticalidad (C)** → también se autoran.
Por eso es sabio construir el editor **antes** de hardcodear hub/ruinas.

## 3. Modelo de datos — `world_edits.json` (o `.ron`)
Fuente de verdad editable, embebida al compilar (`include_str!`) o cargada en
runtime. Si falta o está rota → **mundo 100% procedural** (nunca rompe).
```
{
  "seed": <opcional, override de semilla global>,
  "tiles":      [ {body, z, x, z, tile} ],           // overrides de tile
  "decor":      [ {body, x, z, slot_id} ],           // props/flora/decor colocado
  "structures": [ PlacedStructure ],                 // reusa formato A2
  "buildings":  [ {body, z, min, prefab_id} ],       // reusa prefab + techo F4
  "spawns":     [ {kind, body, x, z, params} ],      // spawn jugador/NPC, POIs
  "regions":    [ {body, rect, kind, params} ]        // bioma/pueblo/no-spawn/quest
}
```
Determinista + autorado conviven: lo autorado **gana** donde exista; lo demás lo
pone la generación. Cero costo cuando no hay archivo.

## 4. UX del editor (egui standalone, como `editor_assets`)
- **Canvas del mundo**: renderiza el mapa —pan (arrastre), zoom (scroll)— dibujando
  lo procedural (editás SOBRE el mundo real) + tus overrides encima. Render por
  chunks para fluidez.
- **Pinceles/herramientas**: pintar tile (tipo + tamaño de pincel), colocar
  decor/prop/estructura/edificio (del MISMO catálogo `game_slots()` que usa el
  juego, con footprint A1), borrar/volver-a-procedural, herramienta de región
  (arrastrás un rect → kind), marcadores (spawn/POI/NPC/quest), gotero (samplear celda).
- **Panel de capas**: terreno / decor / estructuras / edificios / lógica
  (spawns+regiones) — visibilidad y lock.
- **Panel coords/semilla**: saltar a (x,z), elegir cuerpo (Tierra/Luna/Marte) y
  z-level (superficie/cuevas/interiores), fijar semilla.
- **Guardar**: escribe `world_edits.json` y verifica releyéndolo (patrón de los
  otros editores). Recordatorio "recompilá para verlo" (o hot-reload, F5).
- **Reuso total**: mismo `ArtLibrary`/manifest para los sprites, mismo catálogo de
  slots para la paleta, mismos colores de `Tile`, mismas convenciones
  `render_xy`/`sort_z` → WYSIWYG real.

## 5. Motor: cómo el editor ve el MISMO mundo (decisión de ingeniería)
La generación determinista ya vive en **`ha-shared::topdown`** (pura, sin Bevy).
El editor **importa `ha-shared` (gen) + `ha-assets` (slots/manifest)** y **rasteriza
a su propio canvas** (CPU → textura egui): mismos datos → mismo look, sin correr el
juego entero. (Alternativa: correr el juego headless para renderizar — más pesado;
se descarta.) El juego, en boot, carga `world_edits.json` y aplica overrides sobre
la gen (tiles, spawnear decor/estructuras/edificios, registrar regiones/spawns).

## 6. Fases propuestas
- **F0 — Datos + loader**: `world_edits.json` + carga en el juego (overrides sobre
  gen, fallback seguro). Tests. (Bajo riesgo, no toca UI.)
- **F1 — Shell del editor**: canvas que dibuja el mundo procedural (pan/zoom,
  cuerpo/z/semilla), read-only. Es la parte difícil (WYSIWYG del rasterizador).
- **F2 — Pinceles de colocación**: tile + decor/estructura/edificio (reusa catálogo
  + huellas A1) → escribe `world_edits.json`.
- **F3 — Regiones + spawns/POIs + marcadores**.
- **F4 — Lanzador** (`./editor_assets mundo` / `./build-y-publicar.sh mapa`) + docs;
  **autorar el hub (A3) y ruinas (A4)** como contenido (subsume esas fases).
- **F5 — Pulido**: minimapa, undo/redo, copy/paste de stamps, simetría, hot-reload.

## 7. Preguntas para afinar (antes de ejecutar)
1. **Alcance del mapa**: ¿solo la Tierra (overworld), o TODOS los cuerpos + z-levels
   (cuevas/interiores) desde ya?
2. **Mundo infinito** (procedural + POIs/regiones autoradas) vs **mapa acotado**
   totalmente dibujado. (Recomiendo infinito + autoría de POIs/regiones — es lo que
   escala y respeta el estilo Terraria/Minecraft.)
3. **Hot-reload** en el juego corriendo vs **recompilar** (como el arte). ¿Prioridad?
4. **Undo/redo y copy/paste** de stamps: ¿desde F2 o los dejamos a F5?
5. ¿El editor debe poder **editar la semilla/parámetros de bioma** o solo overrides
   puntuales + regiones?

## 8. Recomendación
Sí conviene y encaja perfecto con la arquitectura data-driven. Empezaría por **F0
(datos+loader, bajo riesgo)** y **F1 (canvas WYSIWYG)** — ese canvas es el 60% del
valor y del riesgo. Con eso arriba, F2–F4 son incrementales y ya te dejan **pintar
el hub y las ruinas a mano** en vez de codearlas.
```
```
