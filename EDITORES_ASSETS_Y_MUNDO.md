# Editor de Assets + Editor de Mundo — cómo se usan (y cómo se conectan)

Dos herramientas de DEV que ahora trabajan juntas:

| | comando | qué escribe | quién lo lee |
|---|---|---|---|
| Editor de **assets** | `./editor_assets` | `native/crates/ha-game/assets/art/manifest.json` (+ los PNG copiados a `assets/art/`) | el juego (`art2d.rs`) **y** el editor de mundo |
| Editor de **mundo** | `./editor_assets mundo` | `native/crates/ha-game/assets/world_edits.json` | el juego (`world_edits.rs`) |

Los dos archivos van **embebidos al compilar** (`include_str!`), así que después de
guardar hay que recompilar el juego: `./editor_assets play`.

---

## 1 · Un asset con MUCHAS animaciones (personajes)

La matriz de animaciones cubre un catálogo completo de acciones — quieto, caminar,
correr, disparar, apuntar, cavar, talar, dormir, sentarse, usar el celular, comer,
saludar… — por **4 u 8 direcciones**, más las acciones que agregues vos.

Cada casilla `acción × dirección` es una animación independiente. Se guarda con el
nombre canónico que lee el juego: `walk_down`, `shoot_up_left`, `sleep` (las poses
sin dirección no llevan sufijo).

**Flujo normal (atlas de una hoja):**

1. Elegí el slot (`player/character`, `npc/aldeano`…) y su PNG.
2. En el lienzo marcá el **primer frame** de una fila.
3. `toda la fila →` completa la cantidad de frames.
4. Tocá la casilla de la matriz y 💾.

**Flujo rápido — ⚡ auto-slice:** marcá el primer frame de la PRIMERA fila, elegí
el orden que tiene tu atlas (`acción→direcciones` o `dirección→acciones`), la
cantidad de filas, y se escriben todas de una. Es la diferencia entre 32 recortes
a mano y uno.

El juego usa las diagonales si existen: pide `walk_down_left`, y si no está cae a
`walk_left`, después a `idle_left`, y por último a `idle_down`. Nunca queda sin
sprite, y si sólo sliceaste un lado, el opuesto se espeja solo.

## 2 · Un asset armado con VARIOS PNG

El caso real de los packs: `cuerpo_idle_left.png` es un archivo, `cuerpo_idle_down.png`
es otro. Un slot ya no está atado a una sola hoja — **cada pose puede traer su
propio PNG y su propia grilla**.

- Guardar una pose desde otro PNG **ya no borra las demás** (era el bug: la entrada
  se reseteaba al cambiar de hoja).
- `📥 importar VARIOS PNG a este asset`: elegís la carpeta, el editor deduce de cada
  nombre de archivo qué animación es (`Body_Idle_Left` → `idle_left`) y de sus
  dimensiones cuántos frames tiene (128×32 = 4 frames de 32×32), te muestra la tabla
  para corregir, y recién ahí escribe.
- La lista de poses te dice de qué hoja sale cada una, y `✎ editar` trae esa hoja al
  lienzo (no la del slot).

En el juego, `ArtLibrary::pose_image` devuelve la hoja de cada pose y quien cambia de
pose reasigna `sprite.image` — jugador, fauna, colonos, techos de edificios y luces.

## 3 · El editor de mundo usa TUS assets

La paleta de la derecha sale de `art/manifest.json`: **todo lo que tenga arte**
aparece ahí, agrupado por categoría, con la miniatura real recortada de su hoja.
`↻ assets` la recarga sin cerrar nada — creás algo en el editor de assets, volvés,
recargás, y ya lo podés plantar.

- `🌳 Colocar asset` — clic o **arrastre** (una hilera, sin apilar en la misma celda).
  El fantasma bajo el cursor muestra el sprite real a su tamaño real.
- El **tamaño** sale del recorte del asset: una casa de 5×4 celdas mide 5×4 tiles.
  Antes todo lo colocado medía 1,5 × 1,5 m y una casa se veía como un arbusto.
  El campo `escala` lo multiplica si querés.
- `🍃 Borrar asset/árbol` — saca lo que pusiste y, si esa celda generaba un árbol del
  mundo, la marca para que el juego tampoco lo genere.
- El canvas dibuja los sprites reales (`ver sprites`), así que lo que ves es lo que
  el juego va a spawnear.
- `Ctrl+Z` deshace por trazo.

## 4 · Lo que pintás REEMPLAZA al mundo

Regla nueva, en el editor y en el juego a la vez: **una celda con terreno autorado
no genera decor procedural**. Pintás un rectángulo de pasto sobre un bosque y el
bosque desaparece ahí — en el editor lo ves al instante y en el juego pasa lo mismo,
porque las dos usan la misma regla (`WorldEdits::decor_free_cells`).

Además se arreglaron tres cosas que hacían que lo autorado no llegara al juego:

- Los tiles se escribían **en la capa activa**, no en la capa del edit → todo lo del
  subsuelo terminaba pisando la superficie.
- `place_tile` se **negaba sobre celdas sólidas** → no podías reemplazar roca por pasto.
- El rebrote de césped **deshacía tu diseño** al minuto de empezar la partida.

Ahora se aplican con `TileWorld::author_tile`, que respeta la capa, pisa cualquier
cosa y no programa rebrote.

## 5 · Formato

`art/manifest.json` — campos nuevos, todos opcionales (los manifests viejos cargan igual):

```json
"npc/cuerpo": {
  "png": "art/atlas/cuerpo_izq_1a2b3c4d.png",
  "grid": [32, 32],
  "poses": {
    "idle_left": { "rect": {"col":0,"row":0,"w":1,"h":1} },
    "idle_down": { "rect": {"col":0,"row":0,"w":1,"h":1},
                   "png": "art/atlas/cuerpo_abajo_5e6f7a8b.png",
                   "grid": [16, 16] }
  }
}
```

`world_edits.json` — `decor[].size` (metros) y `cleared[]` (celdas sin decor procedural):

```json
{
  "decor":   [{ "body":0, "z":0, "x":12, "zc":-4, "key":"building/casa_chica", "size":[5.0,4.0] }],
  "cleared": [{ "body":0, "z":0, "x":13, "zc":-4 }]
}
```

Las hojas se copian **direccionadas por contenido** (`art/atlas/<nombre>_<hash>.png`):
dos slots que recorten el mismo atlas comparten el archivo y el APK no se infla.
