# Procedural Asset Factory — Reference

## Catalog size (v1)

**97 procedural kinds** in `ASSET_CATALOG` — terrain, flora, rocks, construction, equipment, consumables, materials, characters, creatures, UI icons.

| Layer | Path |
| ----- | ---- |
| TS data (authoritative) | `packages/shared/src/content/assets/catalog.ts` |
| TS items (craft/gather) | `packages/shared/src/content/items/catalog.ts` |
| TS registry | `packages/shared/src/content/assets/registry.ts` |
| Rust data (native client) | `native/crates/ha-shared/src/assets/catalog.rs` |
| Rust mesh builders | `native/crates/ha-game/src/assets/builders.rs` |
| Lazy cache | `native/crates/ha-game/src/assets/factory.rs` (`AssetFactory`) |

## Asset classes

| Class | Examples | Perf |
| ----- | -------- | ---- |
| `terrain` | grass, dirt, path, cliff, rocks | `instanced: true` |
| `flora360` | oak, pine, cherry trees | mesh cached once |
| `floraFlat` | flowers, mushrooms, grass tufts | instanced + billboard |
| `prop` | cottage, barn, bridge, crate, fence | mesh cached once |
| `item` | swords, potions, food, ingots | instanced + billboard |
| `character` | adventurer, farmer, guard | capsule mesh |
| `creature` | slime, wolf, golem | capsule mesh |
| `ui` | heart, mana, inventory icons | billboard quads |

## TypeScript API

```ts
import {
  ASSET_CATALOG,
  ITEM_CATALOG,
  getAsset,
  getItem,
  assetKindForInteractable,
  assetKindForWorldProp,
  itemsByCraftTag,
} from "@hunger-arena/shared";

const oak = getAsset("flora_tree_oak");
const wood = getItem("wood");
const crafters = itemsByCraftTag("craft");
```

## Rust API (Bevy client)

```rust
use ha_shared::{get_asset, asset_kind_for_interactable, ASSET_CATALOG};
use crate::assets::AssetFactory;

// Lazy mesh + toon material (no per-spawn allocation)
let asset = factory.require("item_potion_red", &mut meshes, &mut materials);
commands.spawn((
    Mesh3d(asset.mesh.clone()),
    MeshMaterial3d(asset.material.clone()),
    Transform::from_xyz(x, y, z),
));
```

## Sim mapping

| Sim type | Visual kind |
| -------- | ----------- |
| `tree` | `flora_tree_oak` / `flora_tree_pine` |
| `rock` | `rock_medium` / `rock_boulder` |
| `container` | `prop_crate` / `prop_barrel` |
| `house` | `building_cottage` |
| `bridge` | `prop_bridge` |
| obstacle | `terrain_grass` |

## Performance rules

1. **Never** build geometry per frame — use `AssetFactory` cache.
2. Bulk terrain/flora → `hints.instanced` + `InstancedMesh` (future world renderer).
3. Floor loot / UI → `hints.billboard` + Y-billboard system (future).
4. Colors from `palette_rgb` / `PALETTE` only — cel shader compatible.
5. Add kinds in **both** TS + Rust catalogs (keep counts in sync; vitest checks items → assets).

## Adding a new asset

1. Row in `packages/shared/src/content/assets/catalog.ts`
2. Mirror row in `native/crates/ha-shared/src/assets/catalog.rs`
3. Extend `builders.rs` match arm if new shape family
4. Optional `ItemDef` in `items/catalog.ts` for crafting/inventory
5. Run `npm test -w @hunger-arena/shared`
