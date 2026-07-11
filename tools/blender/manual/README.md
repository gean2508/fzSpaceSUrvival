# Ajustes manuales en Blender

Si corriges posiciones en la GUI de Blender, el generador **no debe pisar** tu trabajo al volver a correr `npm run generate:assets`.

## Dos niveles de override

| Prioridad | Archivo | Cuándo usarlo |
|-----------|---------|----------------|
| 1 (máxima) | `manual/{kind}.blend` | Editaste mallas, materiales o jerarquía a mano |
| 2 | `manual/overrides/{kind}.json` | Solo moviste/rotaste/escalaste objetos por nombre |

Con cualquiera de los dos, `generate_assets.py` **salta el builder procedural** para ese kind (o aplica el JSON encima).

Usa `--force` para regenerar desde código e ignorar ambos.

## Flujo recomendado (solo posiciones)

```bash
# 1. Exporta un .blend editable desde el procedural
blender --background --python tools/blender/generate_assets.py -- \
  --only flora_tree_cherry --save-blend flora_tree_cherry

# 2. Abre en Blender y corrige en la GUI
blender tools/blender/editable/flora_tree_cherry.blend

# 3. Guarda transforms (con el .blend abierto, o headless):
blender tools/blender/editable/flora_tree_cherry.blend --background \
  --python tools/blender/generate_assets.py -- --capture flora_tree_cherry

# 4. Regenera todo — aplica tu JSON, no el layout procedural viejo
npm run generate:assets
```

El JSON queda en `manual/overrides/flora_tree_cherry.json`. Puedes versionarlo en git.

## Flujo completo (mallas editadas)

1. Trabaja en `editable/{kind}.blend` o copia a `manual/{kind}.blend`
2. **Parentea todo** bajo un Empty llamado exactamente `{kind}` (ej. `flora_tree_cherry`)
3. Guarda como `manual/{kind}.blend`
4. `npm run generate:assets` exporta ese .blend → GLB del juego

## Comandos útiles

```bash
# Solo un kind
npm run generate:assets -- --only flora_tree_cherry

# Forzar regeneración procedural (borra efecto de manual/JSON)
npm run generate:assets -- --force --only flora_tree_cherry

# Guardar .blend editable sin tocar el resto del catálogo
npm run generate:assets -- --only building_cottage --save-blend building_cottage
```

## Nombres de objetos

Los overrides JSON coinciden por **nombre de objeto** (`canopy`, `door`, `rotor`, etc.). No renombres nodos animables si quieres que Bevy los encuentre después.
