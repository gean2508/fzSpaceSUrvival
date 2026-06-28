import type { AssetDef, AssetRenderHints } from "./types.js";

const TERRAIN: AssetRenderHints = {
  instanced: true,
  billboard: false,
  shadows: true,
  scale: 1,
};
const FLORA360: AssetRenderHints = {
  instanced: false,
  billboard: false,
  shadows: true,
  scale: 1,
};
const FLORA_FLAT: AssetRenderHints = {
  instanced: true,
  billboard: true,
  shadows: false,
  scale: 1,
};
const PROP: AssetRenderHints = {
  instanced: false,
  billboard: false,
  shadows: true,
  scale: 1,
};
const ITEM: AssetRenderHints = {
  instanced: true,
  billboard: true,
  shadows: false,
  scale: 0.85,
};
const CHAR: AssetRenderHints = {
  instanced: false,
  billboard: false,
  shadows: true,
  scale: 1,
};
const UI: AssetRenderHints = {
  instanced: false,
  billboard: true,
  shadows: false,
  scale: 1,
};

/** Full procedural catalog aligned with MVP sprite sheet (code meshes, not PNGs). */
export const ASSET_CATALOG: AssetDef[] = [
  // ── Terrain & pathing ──
  { kind: "terrain_grass", class: "terrain", category: "terrain", displayName: "Grass tile", palette: "meadow", hints: TERRAIN, craftTags: [] },
  { kind: "terrain_dirt", class: "terrain", category: "terrain", displayName: "Dirt tile", palette: "earth", hints: TERRAIN, craftTags: [] },
  { kind: "terrain_stone", class: "terrain", category: "terrain", displayName: "Stone tile", palette: "stone", hints: TERRAIN, craftTags: [] },
  { kind: "terrain_path_grass", class: "terrain", category: "terrain", displayName: "Grass path edge", palette: "meadow", hints: TERRAIN, craftTags: [] },
  { kind: "terrain_path_dirt", class: "terrain", category: "terrain", displayName: "Dirt path", palette: "earth", hints: TERRAIN, craftTags: [] },
  { kind: "terrain_path_stone", class: "terrain", category: "terrain", displayName: "Stone path", palette: "stone", hints: TERRAIN, craftTags: [] },
  { kind: "terrain_cliff", class: "terrain", category: "terrain", displayName: "Cliff face", palette: "earth", hints: TERRAIN, craftTags: [] },
  { kind: "terrain_slope", class: "terrain", category: "terrain", displayName: "Slope", palette: "meadow", hints: TERRAIN, craftTags: [] },
  { kind: "terrain_water", class: "terrain", category: "terrain", displayName: "Water tile", palette: "neonCyan", hints: { ...TERRAIN, instanced: true, shadows: false }, craftTags: [] },

  // ── Flora large ──
  { kind: "flora_tree_oak", class: "flora360", category: "flora", displayName: "Oak tree", palette: "meadow", hints: FLORA360, craftTags: ["wood"] },
  { kind: "flora_tree_oak_autumn", class: "flora360", category: "flora", displayName: "Autumn oak", palette: "gold", hints: FLORA360, craftTags: ["wood"] },
  { kind: "flora_tree_pine", class: "flora360", category: "flora", displayName: "Pine tree", palette: "bark", hints: FLORA360, craftTags: ["wood"] },
  { kind: "flora_tree_cherry", class: "flora360", category: "flora", displayName: "Cherry blossom", palette: "neonPink", hints: FLORA360, craftTags: ["wood"] },
  { kind: "flora_tree_red", class: "flora360", category: "flora", displayName: "Red maple", palette: "foe", hints: FLORA360, craftTags: ["wood"] },
  { kind: "flora_shrub", class: "flora360", category: "flora", displayName: "Shrub", palette: "meadow", hints: { ...FLORA360, scale: 0.6 }, craftTags: ["fiber"] },
  { kind: "flora_bush", class: "flora360", category: "flora", displayName: "Bush", palette: "meadow", hints: { ...FLORA360, scale: 0.75 }, craftTags: ["fiber"] },

  // ── Flora small ──
  { kind: "flora_flower_red", class: "floraFlat", category: "flora", displayName: "Red flower", palette: "foe", hints: FLORA_FLAT, craftTags: ["herb"] },
  { kind: "flora_flower_yellow", class: "floraFlat", category: "flora", displayName: "Yellow flower", palette: "gold", hints: FLORA_FLAT, craftTags: ["herb"] },
  { kind: "flora_flower_blue", class: "floraFlat", category: "flora", displayName: "Blue flower", palette: "neonCyan", hints: FLORA_FLAT, craftTags: ["herb"] },
  { kind: "flora_flower_white", class: "floraFlat", category: "flora", displayName: "White flower", palette: "fog", hints: FLORA_FLAT, craftTags: ["herb"] },
  { kind: "flora_grass_tuft", class: "floraFlat", category: "flora", displayName: "Grass tuft", palette: "meadow", hints: FLORA_FLAT, craftTags: ["fiber"] },
  { kind: "flora_mushroom_red", class: "floraFlat", category: "flora", displayName: "Red mushroom", palette: "foe", hints: FLORA_FLAT, craftTags: ["food", "alchemy"] },
  { kind: "flora_mushroom_brown", class: "floraFlat", category: "flora", displayName: "Brown mushroom", palette: "earth", hints: FLORA_FLAT, craftTags: ["food", "alchemy"] },

  // ── Rocks & crystals ──
  { kind: "rock_small", class: "terrain", category: "rock", displayName: "Small rock", palette: "stone", hints: { ...TERRAIN, scale: 0.4 }, craftTags: ["stone"] },
  { kind: "rock_medium", class: "terrain", category: "rock", displayName: "Rock", palette: "stone", hints: { ...TERRAIN, scale: 0.7 }, craftTags: ["stone"] },
  { kind: "rock_boulder", class: "terrain", category: "rock", displayName: "Boulder", palette: "stone", hints: TERRAIN, craftTags: ["stone"] },
  { kind: "rock_cave_entrance", class: "prop", category: "rock", displayName: "Cave entrance", palette: "night", hints: PROP, craftTags: [] },
  { kind: "crystal_yellow", class: "item", category: "rock", displayName: "Yellow crystal", palette: "gold", hints: ITEM, craftTags: ["crystal", "magic"] },
  { kind: "crystal_purple", class: "item", category: "rock", displayName: "Purple crystal", palette: "neonPink", hints: ITEM, craftTags: ["crystal", "magic"] },
  { kind: "crystal_blue", class: "item", category: "rock", displayName: "Blue crystal", palette: "neonCyan", hints: ITEM, craftTags: ["crystal", "magic"] },

  // ── Construction & props ──
  { kind: "prop_fence_wood", class: "prop", category: "construction", displayName: "Wood fence", palette: "wood", hints: PROP, craftTags: ["build"] },
  { kind: "prop_fence_stone", class: "prop", category: "construction", displayName: "Stone fence", palette: "stone", hints: PROP, craftTags: ["build"] },
  { kind: "prop_pillar", class: "prop", category: "construction", displayName: "Stone pillar", palette: "stone", hints: PROP, craftTags: ["build", "ruin"] },
  { kind: "prop_archway", class: "prop", category: "construction", displayName: "Stone arch", palette: "stone", hints: PROP, craftTags: ["build", "ruin"] },
  { kind: "prop_bridge", class: "prop", category: "construction", displayName: "Wooden bridge", palette: "wood", hints: PROP, craftTags: ["build"] },
  { kind: "prop_well", class: "prop", category: "construction", displayName: "Well", palette: "stone", hints: PROP, craftTags: ["build", "water"] },
  { kind: "machine_wood_extractor", class: "prop", category: "construction", displayName: "Wood extractor", palette: "wood", hints: { ...PROP, scale: 1.1 }, craftTags: ["machine", "wood", "automation"] },
  { kind: "machine_stone_extractor", class: "prop", category: "construction", displayName: "Stone extractor", palette: "stone", hints: { ...PROP, scale: 1.05 }, craftTags: ["machine", "stone", "automation"] },
  { kind: "machine_launch_pad", class: "prop", category: "construction", displayName: "Launch pad", palette: "iron", hints: { ...PROP, scale: 2.4 }, craftTags: ["machine", "space", "launch"] },
  { kind: "prop_stall", class: "prop", category: "construction", displayName: "Market stall", palette: "wood", hints: PROP, craftTags: ["build", "trade"] },
  { kind: "prop_barrel", class: "prop", category: "construction", displayName: "Barrel", palette: "wood", hints: PROP, craftTags: ["container"] },
  { kind: "prop_crate", class: "prop", category: "construction", displayName: "Crate", palette: "wood", hints: PROP, craftTags: ["container"] },
  { kind: "prop_crate_large", class: "prop", category: "construction", displayName: "Large crate", palette: "wood", hints: { ...PROP, scale: 1.3 }, craftTags: ["container"] },
  { kind: "prop_lantern", class: "prop", category: "construction", displayName: "Lantern", palette: "gold", hints: { ...PROP, scale: 0.5 }, craftTags: ["light"] },
  { kind: "prop_sign", class: "prop", category: "construction", displayName: "Hanging sign", palette: "wood", hints: PROP, craftTags: ["trade"] },
  { kind: "prop_pot_cooking", class: "prop", category: "construction", displayName: "Cooking pot", palette: "iron", hints: PROP, craftTags: ["cooking"] },
  { kind: "building_cottage", class: "prop", category: "construction", displayName: "Cottage", palette: "wood", hints: { ...PROP, scale: 2.2 }, craftTags: ["build", "house"] },
  { kind: "building_barn", class: "prop", category: "construction", displayName: "Barn", palette: "foe", hints: { ...PROP, scale: 2.8 }, craftTags: ["build", "house"] },
  { kind: "interior_bed", class: "prop", category: "construction", displayName: "Bed", palette: "wood", hints: { ...PROP, scale: 0.9 }, craftTags: ["furniture"] },
  { kind: "interior_table", class: "prop", category: "construction", displayName: "Table", palette: "wood", hints: PROP, craftTags: ["furniture"] },
  { kind: "interior_chair", class: "prop", category: "construction", displayName: "Chair", palette: "wood", hints: { ...PROP, scale: 0.6 }, craftTags: ["furniture"] },
  { kind: "interior_book", class: "item", category: "construction", displayName: "Book", palette: "earth", hints: ITEM, craftTags: ["furniture", "lore"] },
  { kind: "interior_scroll", class: "item", category: "construction", displayName: "Scroll", palette: "fog", hints: ITEM, craftTags: ["furniture", "lore"] },

  // ── Equipment ──
  { kind: "equip_sword_iron", class: "item", category: "equipment", displayName: "Iron sword", palette: "iron", hints: ITEM, craftTags: ["weapon", "melee"] },
  { kind: "equip_sword_steel", class: "item", category: "equipment", displayName: "Steel sword", palette: "stone", hints: ITEM, craftTags: ["weapon", "melee"] },
  { kind: "equip_sword_gold", class: "item", category: "equipment", displayName: "Gold sword", palette: "gold", hints: ITEM, craftTags: ["weapon", "melee"] },
  { kind: "equip_dagger", class: "item", category: "equipment", displayName: "Dagger", palette: "iron", hints: ITEM, craftTags: ["weapon", "melee"] },
  { kind: "equip_spear", class: "item", category: "equipment", displayName: "Spear", palette: "wood", hints: ITEM, craftTags: ["weapon", "melee"] },
  { kind: "equip_bow", class: "item", category: "equipment", displayName: "Bow", palette: "wood", hints: ITEM, craftTags: ["weapon", "ranged"] },
  { kind: "equip_staff", class: "item", category: "equipment", displayName: "Staff", palette: "crystal", hints: ITEM, craftTags: ["weapon", "magic"] },
  { kind: "equip_shield_wood", class: "item", category: "equipment", displayName: "Wood shield", palette: "wood", hints: ITEM, craftTags: ["armor", "shield"] },
  { kind: "equip_shield_metal", class: "item", category: "equipment", displayName: "Metal shield", palette: "iron", hints: ITEM, craftTags: ["armor", "shield"] },
  { kind: "equip_pickaxe", class: "item", category: "equipment", displayName: "Pickaxe", palette: "wood", hints: ITEM, craftTags: ["tool", "mining"] },
  { kind: "equip_axe", class: "item", category: "equipment", displayName: "Axe", palette: "wood", hints: ITEM, craftTags: ["tool", "woodcut"] },
  { kind: "equip_hammer", class: "item", category: "equipment", displayName: "Hammer", palette: "iron", hints: ITEM, craftTags: ["tool", "build"] },
  { kind: "equip_hoe", class: "item", category: "equipment", displayName: "Hoe", palette: "wood", hints: ITEM, craftTags: ["tool", "farm"] },
  { kind: "equip_watering_can", class: "item", category: "equipment", displayName: "Watering can", palette: "iron", hints: ITEM, craftTags: ["tool", "farm"] },

  // ── Consumables & materials ──
  { kind: "item_potion_red", class: "item", category: "consumable", displayName: "Red potion", palette: "potionRed", hints: ITEM, craftTags: ["potion", "heal"] },
  { kind: "item_potion_blue", class: "item", category: "consumable", displayName: "Blue potion", palette: "potionBlue", hints: ITEM, craftTags: ["potion", "mana"] },
  { kind: "item_potion_green", class: "item", category: "consumable", displayName: "Green potion", palette: "potionGreen", hints: ITEM, craftTags: ["potion", "stamina"] },
  { kind: "item_potion_purple", class: "item", category: "consumable", displayName: "Purple potion", palette: "neonPink", hints: ITEM, craftTags: ["potion", "magic"] },
  { kind: "item_potion_yellow", class: "item", category: "consumable", displayName: "Yellow potion", palette: "gold", hints: ITEM, craftTags: ["potion", "buff"] },
  { kind: "item_food_apple", class: "item", category: "consumable", displayName: "Apple", palette: "foe", hints: ITEM, craftTags: ["food"] },
  { kind: "item_food_orange", class: "item", category: "consumable", displayName: "Orange", palette: "gold", hints: ITEM, craftTags: ["food"] },
  { kind: "item_food_grape", class: "item", category: "consumable", displayName: "Grapes", palette: "neonPink", hints: ITEM, craftTags: ["food"] },
  { kind: "item_food_bread", class: "item", category: "consumable", displayName: "Bread", palette: "gold", hints: ITEM, craftTags: ["food", "cooking"] },
  { kind: "item_food_pie", class: "item", category: "consumable", displayName: "Pie", palette: "earth", hints: ITEM, craftTags: ["food", "cooking"] },
  { kind: "item_food_meat", class: "item", category: "consumable", displayName: "Cooked meat", palette: "earth", hints: ITEM, craftTags: ["food", "cooking"] },
  { kind: "item_food_soup", class: "item", category: "consumable", displayName: "Soup bowl", palette: "potionGreen", hints: ITEM, craftTags: ["food", "cooking"] },
  { kind: "item_mat_log", class: "item", category: "material", displayName: "Log", palette: "wood", hints: ITEM, craftTags: ["wood", "craft"] },
  { kind: "item_mat_sticks", class: "item", category: "material", displayName: "Sticks", palette: "wood", hints: ITEM, craftTags: ["wood", "craft"] },
  { kind: "item_mat_stone", class: "item", category: "material", displayName: "Stone", palette: "stone", hints: ITEM, craftTags: ["stone", "craft"] },
  { kind: "item_mat_ingot_iron", class: "item", category: "material", displayName: "Iron ingot", palette: "iron", hints: ITEM, craftTags: ["metal", "craft"] },
  { kind: "item_mat_ingot_gold", class: "item", category: "material", displayName: "Gold ingot", palette: "gold", hints: ITEM, craftTags: ["metal", "craft"] },
  { kind: "item_mat_leather", class: "item", category: "material", displayName: "Leather", palette: "leather", hints: ITEM, craftTags: ["leather", "craft"] },
  { kind: "item_mat_gem_red", class: "item", category: "material", displayName: "Red gem", palette: "foe", hints: ITEM, craftTags: ["gem", "craft"] },
  { kind: "item_mat_gem_blue", class: "item", category: "material", displayName: "Blue gem", palette: "neonCyan", hints: ITEM, craftTags: ["gem", "craft"] },
  { kind: "item_key", class: "item", category: "material", displayName: "Key", palette: "gold", hints: ITEM, craftTags: ["key"] },
  { kind: "item_scroll_map", class: "item", category: "material", displayName: "Map scroll", palette: "fog", hints: ITEM, craftTags: ["lore", "map"] },
  { kind: "item_coin", class: "item", category: "material", displayName: "Coin", palette: "gold", hints: ITEM, craftTags: ["currency"] },

  // ── Characters ──
  { kind: "character_adventurer", class: "character", category: "character", displayName: "Adventurer", palette: "hero", hints: CHAR, craftTags: [] },
  { kind: "character_farmer", class: "character", category: "character", displayName: "Farmer", palette: "earth", hints: CHAR, craftTags: [] },
  { kind: "character_elder", class: "character", category: "character", displayName: "Elder", palette: "fog", hints: CHAR, craftTags: [] },
  { kind: "character_guard", class: "character", category: "character", displayName: "Guard", palette: "iron", hints: CHAR, craftTags: [] },

  // ── Creatures ──
  { kind: "creature_slime", class: "creature", category: "creature", displayName: "Slime", palette: "potionGreen", hints: CHAR, craftTags: ["loot"] },
  { kind: "creature_plant", class: "creature", category: "creature", displayName: "Plant monster", palette: "meadow", hints: CHAR, craftTags: ["loot"] },
  { kind: "creature_wolf", class: "creature", category: "creature", displayName: "Wolf", palette: "night", hints: CHAR, craftTags: ["loot"] },
  { kind: "creature_golem", class: "creature", category: "creature", displayName: "Stone golem", palette: "stone", hints: CHAR, craftTags: ["loot"] },
  { kind: "creature_spirit", class: "creature", category: "creature", displayName: "Spirit", palette: "neonCyan", hints: CHAR, craftTags: ["loot"] },

  // ── UI icons (billboard quads for HUD/inventory) ──
  { kind: "ui_heart", class: "ui", category: "ui", displayName: "Heart", palette: "foe", hints: UI, craftTags: [] },
  { kind: "ui_mana", class: "ui", category: "ui", displayName: "Mana orb", palette: "neonCyan", hints: UI, craftTags: [] },
  { kind: "ui_slot_sword", class: "ui", category: "ui", displayName: "Skill: sword", palette: "iron", hints: UI, craftTags: [] },
  { kind: "ui_slot_shield", class: "ui", category: "ui", displayName: "Skill: shield", palette: "stone", hints: UI, craftTags: [] },
  { kind: "ui_slot_fire", class: "ui", category: "ui", displayName: "Skill: fire", palette: "foe", hints: UI, craftTags: [] },
  { kind: "ui_icon_inventory", class: "ui", category: "ui", displayName: "Inventory", palette: "wood", hints: UI, craftTags: [] },
  { kind: "ui_icon_quest", class: "ui", category: "ui", displayName: "Quest", palette: "gold", hints: UI, craftTags: [] },
  { kind: "ui_icon_map", class: "ui", category: "ui", displayName: "Map", palette: "earth", hints: UI, craftTags: [] },
  { kind: "ui_icon_settings", class: "ui", category: "ui", displayName: "Settings", palette: "stone", hints: UI, craftTags: [] },
  { kind: "ui_icon_chat", class: "ui", category: "ui", displayName: "Chat", palette: "neonCyan", hints: UI, craftTags: [] },
];

export const ASSET_KINDS = ASSET_CATALOG.map((a) => a.kind) as readonly string[];
