---
name: hybrid-bd-ghibli-graphics
description: Apply the Hunger Arena hybrid in-game art style — Ghibli cel-shaded painted world plus Far Cry 3 Blood Dragon neon dusk accents. Use when creating or editing 3D game visuals (sky, lighting, materials, bloom, world shaders). UI/menus stay Blood Dragon only (BD_UI tokens); this skill covers the 3D world.
---

# Hybrid BD + Ghibli Graphics (in-game world)

Like FC3: Blood Dragon — **retro neon HUD**, but the **playable world** is warm painted anime
with synthwave dusk: purple zenith, pink/cyan horizon grid, cel grass, ink outlines.

## Split responsibilities

| Layer | Style | Source |
|-------|-------|--------|
| Menus, HUD, landing HTML | Blood Dragon synthwave | `packages/client-core/src/theme/bloodDragon.ts` (`BD`, `BD_UI`, `BD_COPY`) |
| In-game 3D world | Hybrid Ghibli + BD neon | `@hunger-arena/client-gfx` (`PALETTE`, `HYBRID_LIGHTS`) |

Never put neon grid / laser pink on menu buttons. Never use flat PBR gray on world meshes.

## When to use

- Adding/editing sky, fog, lights, ground shader, entity materials, bloom, camera backdrop.
- Reviewing scenes that look either too flat/anime-only or too cyberpunk/washed out.
- Pair with `.cursor/skills/ghibli-toon-graphics/SKILL.md` (cel rules) and
  `.cursor/skills/procedural-asset-factory/SKILL.md` (geometry).

## Core rules (always follow)

1. **Cel world, neon atmosphere.** `MeshToonMaterial` + stepped ramp (`makeCelMaterial`).
   Shadows tint purple (`toon.ts` ramp). Outlines use `PALETTE.night` / bark.
2. **Palette only.** World colors from `packages/client-gfx/src/art/palette.ts`:
   meadow greens, warm earth, dusk `skyZenith`/`voidPurple`, accents `neonPink`/`neonCyan`.
3. **Hybrid sky.** Use `createHybridSkyMaterial()` + `tickHybridAtmosphere()` +
   `applyHybridSkyUniforms()` — never raw drei `<Sky>` or hardcoded RGB in `AnimeSky`.
4. **Scene lighting.** Copy `HYBRID_LIGHTS` in `GameScene`: purple ambient, cool hemi sky,
   deep green ground bounce, sunset sun lerped ~28% toward `neonPink`.
5. **Fog = painted dissolve.** `FogExp2` with `PALETTE.fog`; each frame sync fog/background
   to `atmos.horizonColor` from `tickHybridAtmosphere`.
6. **Bloom for neon only (desktop/web).** `EffectComposer` + `Bloom`:
   intensity ~0.38–0.42, `luminanceThreshold` ~0.72–0.74 (catch horizon grid, not whole scene).
   Mobile skips postprocessing — rely on shader neon band.
7. **No duplicate logic.** Apps re-export atmosphere from `@hunger-arena/client-gfx`;
   do not fork sky tick code into `apps/*/src/gfx/atmosphere.ts`.

## File map

```
packages/client-gfx/src/art/palette.ts       — PALETTE + HYBRID_LIGHTS
packages/client-gfx/src/art/hybridAtmosphere.ts — tick + uniform push
packages/client-gfx/src/shaders/skyMaterial.ts — createHybridSkyMaterial()
packages/client-gfx/src/art/toon.ts          — cel ramp + outlines
apps/*/src/gfx/AnimeSky.tsx                  — sky + clouds (thin wrapper)
apps/*/src/game/GameScene.tsx                — lights, fog, atmos ref
apps/desktop/src/ui/Game.tsx                 — Bloom + Vignette
```

## How to apply a new world feature

1. Pick colors from `PALETTE` (add new keys there if needed — not in app code).
2. Material: `makeCelMaterial(PALETTE.*)` + `makeOutlineShell` for 360° props.
3. If it glows (pickup, ability VFX): use `neonCyan` / `neonPink` / `laser` sparingly;
   let Bloom pick it up on desktop.
4. Wire scene through existing `createAtmosphereState()` ref; do not animate sky in the feature.
5. UI labels for the feature: `BD_UI` tokens + `BD_COPY` strings — not world palette.

## Tuning targets

- **Readability:** grass stays green readable; neon lives at horizon + accents, not on terrain base.
- **Mobile budget:** no EffectComposer; keep `dpr={[1, 2]}`, instancing, low light count.
- **Feel:** "Saturday-morning meadow at synthwave sunset" — not full Tron grid floor.

## Anti-patterns

- `MeshStandardMaterial` on stylized world objects.
- Hardcoded `#ff2a6d` in React components (use `PALETTE.neonPink`).
- Copy-pasting old `createSkyMaterial` / manual `dayPhase` math in apps.
- Blood Dragon CSS/grid on in-game meshes (BD is UI-only).
