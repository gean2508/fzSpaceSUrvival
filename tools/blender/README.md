# Blender asset pipeline (external tool)

**Not part of the game runtime.** This folder is dev-only tooling to generate `.glb` drop-ins for Bevy.

## Layout

| Path | Purpose |
|------|---------|
| `generate_assets.py` | Run inside Blender (Scripting tab or headless) |
| `run_generate.sh` | Headless batch export → game models dir |
| `staging/` | GLBs whose kinds are not in `catalog-kinds.txt` yet |
| `../../native/crates/ha-game/assets/models/` | **Game drop-in dir** (`{catalog_kind}.glb`) |

Generated `.glb` files are **gitignored** (see repo `.gitignore`). Procedural meshes in `builders.rs` remain the fallback when a file is missing.

## Install Blender (already on this machine)

```bash
blender --version   # Blender 5.0.1 @ /opt/blender-5.0
```

## Generate all assets

```bash
npm run generate:assets
# or
./tools/blender/run_generate.sh
# or
blender --background --python tools/blender/generate_assets.py
```

## Preview generated GLBs

```bash
npm run view:assets   # http://localhost:5199 — tools/asset-viewer/
```

## Catalog naming

The game loads **`{kind}.glb`** where `kind` must match `native/crates/ha-game/assets/models/catalog-kinds.txt`.

The script maps generator names → catalog kinds (e.g. `tree_oak` → `flora_tree_oak`). Assets without a catalog entry export to `tools/blender/staging/` only.

## Procedural animation nodes (Bevy)

Child object names in GLB hierarchy (for future runtime animation):

`canopy`, `rotor`, `arm`, `flame`, `door`, `panel`, `dish`, `wheel_fl`, `wheel_fr`, `wheel_rl`, `wheel_rr`

## Rules

- 1 Blender unit = 1 meter
- Origin at base (Z min of bounding box)
- No PNG textures — Principled BSDF solid colors
- Target ≤ 350 triangles per asset
- Cylinder segments: 8 max; icosphere subdiv: 1 max

See comment block at top of `generate_assets.py`.
