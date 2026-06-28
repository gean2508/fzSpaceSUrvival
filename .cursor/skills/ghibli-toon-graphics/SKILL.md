---
name: ghibli-toon-graphics
description: Apply the Hunger Arena hybrid in-game art style (Ghibli cel-shaded world + Blood Dragon neon dusk). Use when creating or editing 3D visuals (materials, lighting, sky, outlines, postprocessing, shaders). UI stays Blood Dragon — see hybrid-bd-ghibli-graphics skill for the full split.
---

# Ghibli Toon Graphics (hybrid in-game)

**Primary guide:** `.cursor/skills/hybrid-bd-ghibli-graphics/SKILL.md` for the full
Ghibli + Blood Dragon world split. This skill covers cel-shading mechanics.

A code-only (no texture assets) art style for the R3F client. Warm, soft, hand-painted
anime: flat cel bands, ink outlines, painted sky with a neon dusk horizon, subtle bloom.

## When to use
- Adding/editing any in-world 3D mesh, material, light, camera, sky, or effect.
- Reviewing visuals that look flat, plasticky, neon, or "Paint-like".
- **Also read** `.cursor/skills/procedural-asset-factory/SKILL.md` when creating
  terrain, flora, props, or items (geometry classification + registry).

## Core rules (always follow)
1. **Cel shading, not PBR.** Use `MeshToonMaterial` + a stepped `gradientMap`
   (`makeCelMaterial` / `makeCelRamp` in `packages/client-gfx/src/art/toon.ts`).
   3–4 bands. Never `MeshStandardMaterial` for stylized world objects.
2. **Ink outlines.** Give characters/props an inverted-hull outline
   (`makeOutline`) in dark bark `#2b2f3a`. Cheaper + more reliable on mobile than
   an edge postprocess pass.
3. **Palette only.** Pull every color from `PALETTE` / `HYBRID_LIGHTS` in
   `packages/client-gfx/src/art/palette.ts` (apps re-export the same module).
4. **Hybrid dusk light.** `HYBRID_LIGHTS` in `GameScene`: purple-tinted ambient,
   warm sunset sun with slight neon pink mix, hemisphere sky→ground.
5. **Painted + neon atmosphere.** `createHybridSkyMaterial` + `tickHybridAtmosphere`
   in `AnimeSky`; fog/background follow `horizonColor`. Neon grid band at horizon only.
6. **Subtle postprocessing (desktop/web).** `<Bloom>` intensity ~0.4,
   `luminanceThreshold` ~0.72 so horizon neon glows — not a full-scene wash.
7. **Orthographic, trailing camera.** Top-down twin-stick feel. Keep the
   `FollowCamera` rig (ortho, smooth frame-rate-independent lerp).
8. **Mobile budget.** Cap `dpr={[1, 2]}`, reuse cached gradient maps, keep light
   count low, prefer instancing for many identical entities.

## How to apply
- Hybrid atmosphere + sky: `createHybridSkyMaterial`, `tickHybridAtmosphere` from `@hunger-arena/client-gfx`.
- Materials/outlines: `makeCelMaterial`, `makeOutlineShell` from `@hunger-arena/client-gfx`.
- Asset geometry: `AssetRegistry` + builders — see procedural-asset-factory skill.
- Scene wiring: `apps/*/src/gfx/AnimeSky.tsx` + `GameScene.tsx`; UI uses `BD_UI` only.

See `reference.md` for ready-to-paste snippets and tuning notes.

## Optional: baked gradient maps
The runtime generator is the default. If you want hand-tuned PNG ramps, run:

```bash
node .cursor/skills/ghibli-toon-graphics/scripts/gen-gradient-maps.mjs
```

It writes 3/4/5-step ramps to `client/public/gradients/`. Load them with a
`THREE.TextureLoader` and set `minFilter/magFilter = NearestFilter`.
