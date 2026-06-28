---
name: procedural-asset-factory
description: Build and extend Hunger Arena procedural 3D assets via the hybrid 360°/2.5D Asset Registry. Use when adding terrain, flora, props, equipment, consumables, characters, WorldRenderer, Mesh360Builder, MeshFlatBuilder, billboards, instancing, or any code-only mesh generation without .gltf files.
---

# Procedural Asset Factory (360° + 2.5D Hybrid)

All world art is **100% code-generated** — no `.gltf` / texture files. Assets are
classified, cached once, and drawn through `<WorldRenderer>` with instancing where
possible. Pair with the **ghibli-toon-graphics** skill for materials/lighting.

## When to use
- Adding/editing terrain, trees, rocks, houses, items, flowers, equipment drops.
- Choosing between full 3D vs billboard 2.5D for a new object type.
- Wiring sim interactables/obstacles to visuals.
- Optimizing draw calls (instancing, shadow budgets).

## Architecture

```
packages/client-gfx/          ← single source (Three.js only, no R3F)
  art/palette.ts              PALETTE colors
  art/toon.ts                   MeshToonMaterial + ink outlines
  factory/AssetRegistry.ts      dictionary + cache + instantiate()
  factory/builders/
    Mesh360Builder.ts           terrain boxes, trees, rocks, characters
    MeshFlatBuilder.ts          billboards, thin extrusions (items/flora small)
  factory/autotile.ts           path neighbor bitmasks
  factory/noise.ts              FBM for organic terrain edges

apps/{mobile,desktop}/src/gfx/
  WorldRenderer.tsx             R3F scene glue (instancing + billboards)
  shaders/groundMaterial.ts     grass tile shader (stays in app)
```

## Asset classes (never mix rules)

| Class | Examples | Rendering |
| ----- | -------- | --------- |
| `TerrainBox` | climb blocks, rocks, path tiles | 360°, toon + outline, shadows |
| `Flora360` | oak/pine trees | 360°, blob canopies, shadows |
| `FloraFlat` | flowers, mushrooms | 2.5D extruded shapes, **billboard**, no shadows |
| `PropLarge` | crates, houses, bridges | 360°, primitive CSG stacks |
| `ItemFlat` | swords, potions, food | 2.5D `THREE.Shape` + thin extrude, billboard |
| `CharacterBase` | players/NPCs | 360° capsule + outline |

**Rule:** If it is small, numerous, or floor loot → `*Flat` + billboarding.
If the camera orbits it → `*360`.

## How to add a new asset

1. Pick class from table above.
2. Add builder in `Mesh360Builder.ts` or `MeshFlatBuilder.ts`.
3. Register in `AssetRegistry.ts` with `AssetKind` string + `AssetRenderHints`.
4. Map sim type if needed (`instantiateInteractable` / obstacle instancing).
5. For billboards: call `tagBillboard(group)` — `WorldRenderer` rotates each frame.
6. Colors **only** from `PALETTE` via `makeCelMaterial`.
7. Verify draw calls with debug overlay (`OPTIMIZATION.md` budgets).

## API quick reference

```ts
import { AssetRegistry, PALETTE, makeCelMaterial } from "@hunger-arena/client-gfx";

// Interactable from sim
const tree = AssetRegistry.instantiateInteractable("tree", variantIndex);

// Direct kind
const potion = AssetRegistry.instantiate("item_potion");

// Obstacle (organic box)
const block = AssetRegistry.buildObstacleGroup(halfW, height, halfD, seed);

// Character pool
const { group, body } = AssetRegistry.buildCharacter(PALETTE.hero);
```

## WorldRenderer contract
- Ground: instanced shader tiles (`createGroundMaterial`).
- Obstacles: `InstancedMesh` + `getObstacleInstancedGeometry()` (noisy box).
- Interactables: registry prototypes, cloned once per id.
- Billboards: `userData.billboard` → Y-axis `lookAt(camera)` in `useFrame`.
- Never use `useState` for mesh transforms — `useRef` + `useFrame` only.

## Autotiling (paths)
Use `computePathMask(ix, iz, query)` + `autotileCornerWeights` when adding
path tiles. Blend grass↔dirt in the ground shader using corner weights.

## Checklist (every new asset PR)
- [ ] Registered in `AssetRegistry` with correct `AssetClass`
- [ ] `PALETTE` colors only; `makeCelMaterial` for shaded surfaces
- [ ] 360° assets: `makeOutlineShell` + shadows; Flat: no shadows
- [ ] Geometry cached (no per-spawn `new BufferGeometry` in hot path)
- [ ] Mobile draw-call budget respected (`OPTIMIZATION.md`)

## Related
- Visual style: `.cursor/skills/ghibli-toon-graphics/SKILL.md`
- Performance: `OPTIMIZATION.md`
- Sim world data: `packages/shared/src/sim/world.ts`

See `reference.md` for the full `AssetKind` list and scaling pattern.
