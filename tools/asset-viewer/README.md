# HA Asset Viewer

**Herramienta de desarrollo externa al juego.** Navega y previsualiza assets 3D del pipeline Blender y del repo.

## Qué encuentra

| Tipo | Ubicación típica |
|------|------------------|
| `.glb` staging | `tools/blender/staging/` |
| `.glb` drop-in | `native/crates/ha-game/assets/models/` (gitignored hasta generar) |
| `.gltf` | cualquier ruta del repo |
| `.blend` | cualquier ruta (hoy **no hay** `.blend` commiteados; solo el script Python) |

El escaneo recorre todo el monorepo (excluye `node_modules`, `target`, `.git`).

## Requisitos

- Node.js 18+
- Para abrir `.blend`: Blender en PATH (`blender` o `BLENDER=/ruta/blender`)

## Uso

```bash
# desde la raíz del repo
npm run view:assets

# o directo
./tools/asset-viewer/run.sh
```

Abre **http://localhost:5199** en el navegador.

## Features

- Lista filtrable por nombre, categoría y extensión
- Vista 3D de `.glb` / `.gltf` (Three.js, orbit controls)
- Stats: triángulos, vértices, bbox en metros
- Indica si el kind está en `catalog-kinds.txt`
- Wireframe, auto-rotate, re-encuadre, **Watch mode** (rescan cada 2.5s)
- Botón **Usar en juego** → escribe `dev/asset_overrides.json` (el cliente debug recarga solo)
- URL deep-link: `?kind=flora_tree_oak` o `?rel=tools/blender/staging/lunar_rover.glb`
- Botón «Abrir en Blender» para archivos `.blend`

## Integración con el juego (debug)

1. `npm run view:assets` — deja el visor corriendo
2. En el juego (build debug): **MENU → ASSETS**
3. **VIEWER** abre el navegador con el kind actual
4. En el visor: selecciona un `.glb`, escribe el kind, **Usar en juego**
5. En el juego: **PREVIEW** spawnea el mesh delante del jugador; **RELOAD** invalida caché

Overrides persisten en `tools/asset-viewer/dev/asset_overrides.json`. Si el archivo es inválido, el juego cae a procedural sin crashear.
