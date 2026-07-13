# Drop-in 2D art (Modern Exteriors / Interiors / UI, etc.)

Poné acá tus PNGs comprados y el juego los usa solo, con fallback al arte
procedural para todo lo que falte.

## Flujo

1. Copiá los PNG individuales a la categoría correcta (nombres abajo).
2. `npm run sync:art` — regenera `manifest.json` escaneando esta carpeta.
3. Rebuild (`npm run play` / `ship:android`) — el manifest va embebido y los
   PNGs viajan dentro del APK.

## Categorías y claves (la ruta ES la clave lógica)

| Carpeta | Clave de ejemplo | Lo usa |
|---------|------------------|--------|
| `items/` | `items/apple.png`, `items/pickaxe.png` | Ícono del ítem en hotbar/bolsa/craft y sprite de máquina — reemplaza el ícono procedural en TODOS lados. Un PNG por `item_id` del catálogo (`ha-shared/items.rs`). |
| `decor/` | `decor/tree.png`, `decor/rock.png`, `decor/rock_moon.png`, `decor/ore_iron.png`, `decor/gem_red.png` | Árboles, rocas y vetas del mundo. Sufijo `_moon`/`_mars` = variante por cuerpo (cae a la genérica si no está). |
| `machines/` | `machines/wood_extractor.png`, `machines/habitat.png` | Sprite de la máquina colocada (si falta, usa `items/<id>` y si no, procedural). |
| `player/` | `player/astronaut.png` | Sprite del jugador (1 frame por ahora; animación a futuro). |
| `ship/` | `ship/rocket.png` | Cohete en vuelo. |
| `tiles/` | *(futuro)* atlas + autotiling | Los tiles hoy son vertex-color; cuando queramos texturizarlos definimos acá el atlas 16px + reglas. |
| `ui/` | *(futuro)* paneles/botones Modern UI | El tema de UI hoy es procedural (`ui_theme.rs`). |
| `backgrounds/` | *(futuro)* `backgrounds/sky_day.png` parallax | El cielo hoy es color + estrellas. |

## Reglas

- Nombre de archivo = clave: `decor/tree.png` → clave `decor/tree`.
- Pixel art: tamaño libre (16/32/48px); el juego los escala al tamaño de
  mundo del objeto (filtro nearest, se ve nítido).
- Los atlases grandes de los packs NO van acá directo: exportá/copiá los PNG
  individuales que quieras usar, renombrados a la clave que toca.
