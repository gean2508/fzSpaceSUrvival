# PS1 Retro Graphics — Hunger Arena / Starship

**Visual layer only.** Gameplay, netcode, and sim stay unchanged.

## Direction

PlayStation 1 era fidelity — **ultra legible, hand-crafted, shadowed** — not low-effort pixel mush:

- Vertex snap (virtual ~180p) + affine-friendly silhouettes
- 3-band hard cel shading + directional shadows
- Bayer ordered dither on gradients
- Linear distance fog (muted blue-grey)
- 5-bit palette feel — rich but banded colors
- FF7 / MGS / Tekken menu DNA: bevel panels, yellow selection, navy void

**No synthwave neon** in world or native UI. Shaders add polish (fog, dither, snap) — they must harmonize with PS1, not fight it.

## Tokens (`ha-shared::Ps1Retro`)

| Token | Use |
|-------|-----|
| `VOID` | Boot void, clear margins |
| `PANEL` / `PANEL_HI` / `PANEL_LO` | Menu bevels |
| `TEXT` / `TEXT_DIM` / `SELECT` | Copy + cursor |
| `SKY_*` / `FOG` | Atmosphere |
| `GRASS`…`SNOW` | Terrain materials |
| `SNAP_RES`, `FOG_START`, `FOG_END`, `DITHER` | Shader uniforms |

## Where code lives

| Area | Path |
|------|------|
| Palette | `native/crates/ha-shared/src/ps1_palette.rs` |
| World shader | `native/crates/ha-game/assets/shaders/toon.wgsl` |
| Sky shader | `native/crates/ha-game/assets/shaders/ghibli_sky.wgsl` |
| Materials | `native/crates/ha-game/src/materials.rs` |
| Atmosphere | `native/crates/ha-game/src/ghibli_atmosphere.rs` |
| Native UI | `native/crates/ha-game/src/ui_theme.rs` |
| Boot screen | `native/crates/ha-game/src/ps1_boot.rs` |
| Landing web | `client/src/index.css`, `LandingBackdrop.tsx` |

## Rules

1. New 3D surfaces → `ToonMaterial` + `ps1_base_material()` / terrain helpers.
2. Set `ps1_settings`, `ps1_fog_color`, `camera_pos` via `apply_paint_uniforms` — do not duplicate per mesh.
3. UI: bevel borders (`PANEL_HI` outer, `PANEL_LO` inner), yellow pressed state.
4. Icons: CPU raster + optional 5-bit posterize in `icon_builders.rs`.
5. Do **not** reintroduce `HybridLights::SUN_NEON_MIX` or Blood Dragon tokens in native client.

## Landing

Web download page mirrors PS1: banded sky, scanlines, VT323 / Press Start 2P — not Orbitron synthwave.
