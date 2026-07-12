//! Machine catalog mirror — keep in sync with `packages/shared/src/content/machines/`.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MachineDef {
    pub id: &'static str,
    pub display_name: &'static str,
    pub asset_kind: &'static str,
    pub output_item_id: &'static str,
    pub output_per_second: f32,
    pub passive: bool,
    pub place_duration_ms: u32,
    pub cost_wood: u32,
    pub cost_stone: u32,
    pub cost_sticks: u32,
    pub cost_iron: u32,
}

pub const WOOD_EXTRACTOR: MachineDef = MachineDef {
    id: "wood_extractor",
    display_name: "Wood extractor",
    asset_kind: "machine_wood_extractor",
    output_item_id: "wood",
    output_per_second: 0.5,
    passive: true,
    place_duration_ms: 2000,
    cost_wood: 5,
    cost_stone: 2,
    cost_sticks: 1,
    cost_iron: 0,
};

pub const STONE_EXTRACTOR: MachineDef = MachineDef {
    id: "stone_extractor",
    display_name: "Stone extractor",
    asset_kind: "machine_stone_extractor",
    output_item_id: "stone",
    output_per_second: 0.35,
    passive: true,
    place_duration_ms: 2500,
    cost_wood: 3,
    cost_stone: 4,
    cost_sticks: 2,
    cost_iron: 0,
};

pub const LAUNCH_PAD: MachineDef = MachineDef {
    id: "launch_pad",
    display_name: "Launch pad",
    asset_kind: "machine_launch_pad",
    output_item_id: "wood",
    output_per_second: 0.0,
    passive: false,
    place_duration_ms: 4500,
    cost_wood: 40,
    cost_stone: 35,
    cost_sticks: 20,
    cost_iron: 15,
};

/// Act II — lunar camp: slow passive iron from regolith (airless bodies only).
pub const REGOLITH_DRILL: MachineDef = MachineDef {
    id: "regolith_drill",
    display_name: "Regolith drill",
    asset_kind: "machine_stone_extractor",
    output_item_id: "iron_ingot",
    output_per_second: 0.08,
    passive: true,
    place_duration_ms: 3000,
    cost_wood: 0,
    cost_stone: 8,
    cost_sticks: 0,
    cost_iron: 4,
};

/// Act II — uplink: enables remote base monitoring from Earth (no output).
pub const ANTENNA: MachineDef = MachineDef {
    id: "antenna",
    display_name: "Antenna",
    asset_kind: "machine_wood_extractor",
    output_item_id: "wood",
    output_per_second: 0.0,
    passive: false,
    place_duration_ms: 3500,
    cost_wood: 0,
    cost_stone: 4,
    cost_sticks: 0,
    cost_iron: 8,
};

/// Act V — sealed habitat: each one houses COLONISTS_PER_HABITAT colonists.
pub const HABITAT: MachineDef = MachineDef {
    id: "habitat",
    display_name: "Habitat",
    asset_kind: "machine_wood_extractor",
    output_item_id: "wood",
    output_per_second: 0.0,
    passive: false,
    place_duration_ms: 5000,
    cost_wood: 20,
    cost_stone: 0,
    cost_sticks: 0,
    cost_iron: 10,
};

pub const MACHINE_CATALOG: &[MachineDef] =
    &[WOOD_EXTRACTOR, STONE_EXTRACTOR, LAUNCH_PAD, REGOLITH_DRILL, ANTENNA, HABITAT];

pub fn get_machine(id: &str) -> Option<&'static MachineDef> {
    MACHINE_CATALOG.iter().find(|m| m.id == id)
}

pub fn require_machine(id: &str) -> &'static MachineDef {
    get_machine(id).unwrap_or_else(|| panic!("unknown machine: {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_matches_ts_wood_extractor() {
        let m = require_machine("wood_extractor");
        assert_eq!(m.cost_wood, 5);
        assert!(m.passive);
        assert_eq!(m.output_per_second, 0.5);
    }

    #[test]
    fn stone_extractor_produces_stone() {
        let m = require_machine("stone_extractor");
        assert_eq!(m.output_item_id, "stone");
        assert_eq!(m.cost_stone, 4);
    }

    #[test]
    fn launch_pad_is_non_passive() {
        let m = require_machine("launch_pad");
        assert!(!m.passive);
        assert_eq!(m.cost_iron, 15);
        assert_eq!(m.asset_kind, "machine_launch_pad");
    }
}
