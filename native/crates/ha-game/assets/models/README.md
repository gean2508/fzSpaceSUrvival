# Blender `.glb` drop-in overrides

Any catalog **kind** can be replaced by a glTF Binary (`.glb`) exported from Blender. No code changes — only file placement and naming.

## Where to put files

| Platform | Path | Notes |
|----------|------|--------|
| **Bundled (APK + desktop dev)** | `native/crates/ha-game/assets/models/{kind}.glb` | Gradle copies `assets/` into the APK; Bevy loads via `models/{kind}.glb`. |
| **Desktop hot-reload** | `./models/{kind}.glb` next to the executable, or `$HA_MODELS_DIR/{kind}.glb` | Overrides bundled file without recompiling. |

If a file exists → `AssetFactory` loads the GLB. If not → procedural mesh from `builders.rs` (unchanged behavior).

See **`catalog-kinds.txt`** in this folder for every valid `{kind}` string (mirrors `packages/shared/src/content/assets/catalog.ts` / `ha-shared` catalog).

## Export settings (Blender)

1. **Format:** glTF 2.0 Binary (`.glb`)
2. **Scale:** 1 Blender unit = 1 meter in-game
3. **Origin:** mesh origin at the **center of the base** of the object (feet on ground), not the geometric center
4. **No armatures or animations** in this iteration
5. **File name:** exactly the catalog kind, e.g. `machine_wood_extractor.glb`
6. **Destination:** this directory (`assets/models/`)

## Materials

- **Desktop / Linux:** GLB materials are used as exported (PBR `StandardMaterial`).
- **Android:** base color from the GLB is mapped onto the project **toon shader** for visual consistency with the rest of the world.

## Debug logging

When a GLB override loads successfully, debug builds log:

```text
ha-glb: loaded override for 'machine_wood_extractor' from …
```

Release builds fall back silently if the file is missing or invalid.

## Regenerating `catalog-kinds.txt`

Run the ha-game unit tests — `catalog_kinds_manifest_matches_shared_catalog` rewrites the file if the shared catalog changed:

```bash
cargo test -p ha-game catalog_kinds_manifest
```
