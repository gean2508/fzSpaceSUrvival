---
name: ghibli-toon-graphics
description: Apply the Hunger Arena anime / Studio-Ghibli-inspired toon art style to React Three Fiber scenes. Use when creating or editing 3D visuals (materials, lighting, sky, outlines, postprocessing, shaders) so the game keeps its warm, painterly, cel-shaded look instead of default PBR or a "made in Paint" feel.
---

# Ghibli Toon Graphics

A code-only (no texture assets) art style for the R3F client. The goal is a warm,
soft, hand-painted anime look: flat cel-shaded bands, clean ink outlines, painted
sky and clouds, gentle warm light, and subtle bloom — all cheap enough for mobile.

## When to use
- Adding/editing any in-world 3D mesh, material, light, camera, sky, or effect.
- Reviewing visuals that look flat, plasticky, neon, or "Paint-like".

## Core rules (always follow)
1. **Cel shading, not PBR.** Use `MeshToonMaterial` + a stepped `gradientMap`
   (`makeToonMaterial` / `makeToonGradient` in `client/src/game/art/toon.ts`).
   3–4 bands. Never `MeshStandardMaterial` for stylized world objects.
2. **Ink outlines.** Give characters/props an inverted-hull outline
   (`makeOutline`) in dark bark `#2b2f3a`. Cheaper + more reliable on mobile than
   an edge postprocess pass.
3. **Palette only.** Pull every color from `client/src/game/art/palette.ts`
   (mirrors the CSS tokens in `index.css`). No raw hex in components.
4. **Warm soft light.** `hemisphereLight` (sky→ground) + a warm sunset
   `directionalLight` with `SoftShadows` (PCF). Avoid harsh white key lights.
5. **Painted atmosphere.** drei `<Sky>` + `<Clouds>/<Cloud>` + `<fog>` in the fog
   color. Background = fog color so the horizon dissolves softly.
6. **Subtle postprocessing.** `<EffectComposer>` with gentle `<Bloom>`
   (intensity ~0.4, high luminance threshold) + soft `<Vignette>`. Never blow out
   the scene; bloom is a glow accent, not a wash.
7. **Orthographic, trailing camera.** Top-down twin-stick feel. Keep the
   `FollowCamera` rig (ortho, smooth frame-rate-independent lerp).
8. **Mobile budget.** Cap `dpr={[1, 2]}`, reuse cached gradient maps, keep light
   count low, prefer instancing for many identical entities.

## How to apply
- Materials/outlines: import from `client/src/game/art/toon.ts`.
- Colors: import `PALETTE` from `client/src/game/art/palette.ts`.
- Scene atmosphere (sky/clouds/light/fog/ground): see `client/src/game/GameScene.tsx`.
- Effects + camera: see `client/src/ui/Game.tsx`.

See `reference.md` for ready-to-paste snippets and tuning notes.

## Optional: baked gradient maps
The runtime generator is the default. If you want hand-tuned PNG ramps, run:

```bash
node .cursor/skills/ghibli-toon-graphics/scripts/gen-gradient-maps.mjs
```

It writes 3/4/5-step ramps to `client/public/gradients/`. Load them with a
`THREE.TextureLoader` and set `minFilter/magFilter = NearestFilter`.
