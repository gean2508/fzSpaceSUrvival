//! Catálogo de **slots** del juego: las claves lógicas que el juego dibuja
//! (`decor/tree`, `player/astronaut`, `items/apple`, `machines/…`, `tiles/water`).
//! El editor siembra su lista con esto para que NUNCA esté vacío y para que
//! puedas reskinear lo que el juego usa. Derivado de los catálogos reales
//! (`ha-shared/items.rs`, `machines.rs`) + las claves documentadas en
//! `assets/art/README.md`.
//!
//! "en un futuro que actualice": agregá la clave nueva acá y aparece en el editor.

/// Un slot de arte del juego.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotDef {
    /// Clave lógica = ruta (`"decor/tree"`). Es lo que el juego pide a ArtLibrary.
    pub key: &'static str,
    /// Categoría para agrupar/filtrar en el editor.
    pub category: &'static str,
    /// Hint: suele ser animado (agua, npc). El usuario puede cambiarlo por slot.
    pub animated: bool,
}

const fn s(key: &'static str, category: &'static str) -> SlotDef {
    SlotDef { key, category, animated: false }
}
const fn anim(key: &'static str, category: &'static str) -> SlotDef {
    SlotDef { key, category, animated: true }
}

/// Ítems del juego (`ha-shared/items.rs`). Cada `items/<id>` reemplaza el ícono
/// procedural en hotbar/bolsa/craft y el sprite de la máquina.
const ITEM_IDS: &[&str] = &[
    "apple", "axe", "bread", "dirt", "gem_blue", "gem_red", "gold_ingot", "grape", "hand",
    "iron_ingot", "jetpack", "lantern", "leather", "meat", "orange", "part_engine", "part_fin",
    "part_lander", "part_nose", "part_tank", "pickaxe", "pie", "potion_blue", "potion_green",
    "probe", "sand", "shield_wood", "snow", "soup", "sticks", "stone", "stone_axe",
    "stone_pickaxe", "torch", "wood", "wood_axe", "wood_pickaxe",
];

/// Máquinas colocables (`ha-shared/machines.rs`).
const MACHINE_IDS: &[&str] = &[
    "wood_extractor", "stone_extractor", "launch_pad", "regolith_drill", "antenna", "habitat",
];

/// Decoración del mundo (README `assets/art/`). Sufijo `_moon`/`_mars` = variante
/// por cuerpo (cae a la genérica si no está).
const DECOR_KEYS: &[&str] = &[
    "decor/tree", "decor/rock", "decor/rock_moon", "decor/rock_mars", "decor/ore_iron",
    "decor/ore_gold", "decor/gem_red", "decor/gem_blue",
];

/// Elementos de UI / HUD (hoy procedurales en `ui_theme.rs`/`hud.rs`). Los listás
/// para poder reskinear con tus Sprites2D de UII (hotbar, barra de vida, botones…).
const UI_KEYS: &[&str] = &[
    "ui/panel", "ui/button", "ui/button_pressed", "ui/hotbar_slot", "ui/hotbar_slot_selected",
    "ui/heart_full", "ui/heart_half", "ui/heart_empty", "ui/energy_full", "ui/energy_empty",
    "ui/joystick_base", "ui/joystick_knob", "ui/use_button", "ui/jump_button", "ui/bag_button",
    "ui/menu_button", "ui/crosshair",
];

/// Fondos / cielo (hoy color + estrellas).
const BG_KEYS: &[&str] = &["backgrounds/sky_day", "backgrounds/sky_night", "backgrounds/stars"];

/// Lista canónica completa de slots del juego. `items/<id>` y `machines/<id>` se
/// generan de los catálogos; el resto son claves fijas documentadas.
pub fn game_slots() -> Vec<SlotDef> {
    let mut out = Vec::new();
    // Personaje / nave.
    out.push(anim("player/astronaut", "player")); // el jugador va animado a futuro
    out.push(s("ship/rocket", "ship"));
    // Decoración.
    for k in DECOR_KEYS {
        out.push(s(k, "decor"));
    }
    // Tiles animables / terreno texturizable a futuro.
    out.push(anim("tiles/water", "tiles"));
    out.push(s("tiles/grass", "tiles"));
    out.push(s("tiles/sand", "tiles"));
    out.push(s("tiles/regolith", "tiles"));
    // UI / HUD (Sprites2D de interfaz).
    for k in UI_KEYS {
        out.push(s(k, "ui"));
    }
    // Fondos.
    for k in BG_KEYS {
        out.push(s(k, "backgrounds"));
    }
    // Ítems (íconos + sprite de máquina).
    for id in ITEM_IDS {
        out.push(SlotDef { key: leak(format!("items/{id}")), category: "items", animated: false });
    }
    // Máquinas colocadas.
    for id in MACHINE_IDS {
        out.push(SlotDef { key: leak(format!("machines/{id}")), category: "machines", animated: false });
    }
    out
}

/// Los slots derivados de catálogos necesitan `&'static str`. Como `game_slots`
/// se llama una vez al abrir el editor (no en caliente), filtrar la cadena es
/// aceptable y mantiene `SlotDef` `Copy`.
fn leak(sfmt: String) -> &'static str {
    Box::leak(sfmt.into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_populated_and_has_known_keys() {
        let slots = game_slots();
        assert!(slots.len() > 50, "debe listar decenas de slots reales");
        let keys: Vec<&str> = slots.iter().map(|s| s.key).collect();
        for expected in ["player/astronaut", "decor/tree", "items/apple", "machines/wood_extractor", "tiles/water"] {
            assert!(keys.contains(&expected), "falta slot {expected}");
        }
        // El agua viene marcada animada por defecto.
        assert!(slots.iter().find(|s| s.key == "tiles/water").unwrap().animated);
    }

    #[test]
    fn keys_are_unique() {
        let slots = game_slots();
        let mut seen = std::collections::BTreeSet::new();
        for s in &slots {
            assert!(seen.insert(s.key), "clave duplicada: {}", s.key);
        }
    }
}
