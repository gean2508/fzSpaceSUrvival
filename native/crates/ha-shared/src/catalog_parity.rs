//! Generic catalog parity — compare Rust catalogs against TS snapshot JSON.
//! Add new modules to `rust_module_entries` when porting content to ha-shared.

use serde::Deserialize;
use std::collections::BTreeMap;

use crate::assets::ASSET_CATALOG;
use crate::machines::MACHINE_CATALOG;
use crate::ui::UI_ICON_CATALOG;
use crate::assets::{AssetCategory, AssetClass};

#[derive(Debug, Deserialize)]
pub struct ParityEntry {
    pub id: String,
    pub fields: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ParityModule {
    pub id: String,
    pub entries: Vec<ParityEntry>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogParitySnapshot {
    pub version: u32,
    pub modules: Vec<ParityModule>,
}

fn json_str(v: &str) -> serde_json::Value {
    serde_json::Value::String(v.to_string())
}

fn json_num(v: impl Into<serde_json::Number>) -> serde_json::Value {
    serde_json::Value::Number(v.into())
}

fn json_bool(v: bool) -> serde_json::Value {
    serde_json::Value::Bool(v)
}

fn asset_class_str(c: AssetClass) -> &'static str {
    match c {
        AssetClass::Terrain => "terrain",
        AssetClass::Flora360 => "flora360",
        AssetClass::FloraFlat => "floraFlat",
        AssetClass::Prop => "prop",
        AssetClass::Item => "item",
        AssetClass::Character => "character",
        AssetClass::Creature => "creature",
        AssetClass::Ui => "ui",
    }
}

fn asset_category_str(c: AssetCategory) -> &'static str {
    match c {
        AssetCategory::Terrain => "terrain",
        AssetCategory::Flora => "flora",
        AssetCategory::Rock => "rock",
        AssetCategory::Construction => "construction",
        AssetCategory::Equipment => "equipment",
        AssetCategory::Consumable => "consumable",
        AssetCategory::Material => "material",
        AssetCategory::Character => "character",
        AssetCategory::Creature => "creature",
        AssetCategory::Ui => "ui",
    }
}

fn ui_shape_str(s: crate::ui::UiIconShape) -> &'static str {
    match s {
        crate::ui::UiIconShape::Heart => "heart",
        crate::ui::UiIconShape::Drumstick => "drumstick",
    }
}

fn ui_variant_str(v: crate::ui::UiIconVariant) -> &'static str {
    match v {
        crate::ui::UiIconVariant::Full => "full",
        crate::ui::UiIconVariant::Half => "half",
        crate::ui::UiIconVariant::Empty => "empty",
    }
}

fn palette_key_str(k: crate::assets::PaletteKey) -> &'static str {
    match k {
        crate::assets::PaletteKey::Meadow => "meadow",
        crate::assets::PaletteKey::Earth => "earth",
        crate::assets::PaletteKey::Bark => "bark",
        crate::assets::PaletteKey::Hero => "hero",
        crate::assets::PaletteKey::Foe => "foe",
        crate::assets::PaletteKey::NeonPink => "neonPink",
        crate::assets::PaletteKey::NeonCyan => "neonCyan",
        crate::assets::PaletteKey::Night => "night",
        crate::assets::PaletteKey::Fog => "fog",
        crate::assets::PaletteKey::Gold => "gold",
        crate::assets::PaletteKey::Crystal => "crystal",
        crate::assets::PaletteKey::PotionRed => "potionRed",
        crate::assets::PaletteKey::PotionBlue => "potionBlue",
        crate::assets::PaletteKey::PotionGreen => "potionGreen",
        crate::assets::PaletteKey::Wood => "wood",
        crate::assets::PaletteKey::Stone => "stone",
        crate::assets::PaletteKey::Iron => "iron",
        crate::assets::PaletteKey::Leather => "leather",
    }
}

pub fn rust_module_entries(module_id: &str) -> Option<Vec<ParityEntry>> {
    match module_id {
        "assets" => {
            let mut entries: Vec<ParityEntry> = ASSET_CATALOG
                .iter()
                .map(|a| {
                    let mut fields = BTreeMap::new();
                    fields.insert("class".into(), json_str(asset_class_str(a.class)));
                    fields.insert("category".into(), json_str(asset_category_str(a.category)));
                    fields.insert("palette".into(), json_str(palette_key_str(a.palette)));
                    ParityEntry {
                        id: a.kind.to_string(),
                        fields,
                    }
                })
                .collect();
            entries.sort_by(|a, b| a.id.cmp(&b.id));
            Some(entries)
        }
        "machines" => {
            let mut entries: Vec<ParityEntry> = MACHINE_CATALOG
                .iter()
                .map(|m| {
                    let mut fields = BTreeMap::new();
                    fields.insert("passive".into(), json_bool(m.passive));
                    fields.insert(
                        "outputPerSecond".into(),
                        json_num(serde_json::Number::from_f64(m.output_per_second as f64).unwrap()),
                    );
                    fields.insert("placeDurationMs".into(), json_num(m.place_duration_ms));
                    fields.insert("assetKind".into(), json_str(m.asset_kind));
                    ParityEntry {
                        id: m.id.to_string(),
                        fields,
                    }
                })
                .collect();
            entries.sort_by(|a, b| a.id.cmp(&b.id));
            Some(entries)
        }
        "ui_icons" => {
            let mut entries: Vec<ParityEntry> = UI_ICON_CATALOG
                .iter()
                .map(|i| {
                    let mut fields = BTreeMap::new();
                    fields.insert("shape".into(), json_str(ui_shape_str(i.shape)));
                    fields.insert("variant".into(), json_str(ui_variant_str(i.variant)));
                    fields.insert("fillPalette".into(), json_str(palette_key_str(i.fill_palette)));
                    fields.insert("bgPalette".into(), json_str(palette_key_str(i.bg_palette)));
                    fields.insert("grid".into(), json_num(i.grid));
                    fields.insert("cellCount".into(), json_num(i.cells.len()));
                    ParityEntry {
                        id: i.kind.to_string(),
                        fields,
                    }
                })
                .collect();
            entries.sort_by(|a, b| a.id.cmp(&b.id));
            Some(entries)
        }
        // Items catalog not ported to Rust yet — TS snapshot validates; add here when ported.
        "items" => None,
        _ => None,
    }
}

pub fn assert_rust_matches_snapshot(snapshot: &CatalogParitySnapshot) {
    for module in &snapshot.modules {
        let Some(rust_entries) = rust_module_entries(&module.id) else {
            continue;
        };
        assert_eq!(
            rust_entries.len(),
            module.entries.len(),
            "entry count mismatch for module {}",
            module.id
        );
        for (rust_e, snap_e) in rust_entries.iter().zip(module.entries.iter()) {
            assert_eq!(rust_e.id, snap_e.id, "id order mismatch in {}", module.id);
            assert_fields_match(&rust_e.fields, &snap_e.fields, &module.id, &rust_e.id);
        }
    }
}

fn assert_fields_match(
    left: &BTreeMap<String, serde_json::Value>,
    right: &BTreeMap<String, serde_json::Value>,
    module: &str,
    entry_id: &str,
) {
    assert_eq!(left.len(), right.len(), "field count in {module}/{entry_id}");
    for (key, rv) in left {
        let lv = right.get(key).unwrap_or_else(|| panic!("missing field {key} in snapshot"));
        match (rv, lv) {
            (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
                let af = a.as_f64().unwrap_or(0.0);
                let bf = b.as_f64().unwrap_or(0.0);
                assert!(
                    (af - bf).abs() < 1e-4,
                    "numeric mismatch {key} in {module}/{entry_id}: {af} vs {bf}"
                );
            }
            _ => assert_eq!(rv, lv, "field {key} mismatch in {module}/{entry_id}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAPSHOT_JSON: &str =
        include_str!("../../../../packages/shared/catalog-parity.snapshot.json");

    #[test]
    fn rust_catalogs_match_ts_snapshot() {
        let snapshot: CatalogParitySnapshot =
            serde_json::from_str(SNAPSHOT_JSON).expect("parse catalog parity snapshot");
        assert_eq!(snapshot.version, 1);
        assert_rust_matches_snapshot(&snapshot);
    }

    #[test]
    fn snapshot_includes_items_module() {
        let snapshot: CatalogParitySnapshot =
            serde_json::from_str(SNAPSHOT_JSON).expect("parse catalog parity snapshot");
        let items = snapshot
            .modules
            .iter()
            .find(|m| m.id == "items")
            .expect("items module in snapshot");
        assert!(items.entries.len() >= 20);
    }
}
