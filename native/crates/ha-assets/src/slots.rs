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

/// Decoración del mundo (referenciada en `world2d.rs`/`ship_interior.rs`). Sufijo
/// `_moon`/`_mars` = variante por cuerpo (cae a la genérica si no está). Las
/// especies de árbol (`tree_*`) las elige `pick_tree_species`; `tree` es el
/// fallback genérico.
const DECOR_KEYS: &[&str] = &[
    "decor/tree", "decor/tree_roble", "decor/tree_pino", "decor/tree_abedul", "decor/tree_arce",
    "decor/tree_sauce", "decor/tree_palmera", "decor/tree_arbusto", "decor/tree_seco",
    "decor/rock", "decor/rock_moon", "decor/rock_mars", "decor/ore_iron", "decor/ore_gold",
    "decor/gem_red", "decor/gem_blue", "decor/mine_hole", "decor/ladder", "decor/cockpit_seat",
    "decor/fuel_source",
];

/// Props de exterior (LimeZu `3_City_Props` / `17_Garden` / `11_Camping`).
/// Sembrados deterministas en superficie (Tierra) por `world2d.rs`; cero costo
/// si el slot no tiene arte asignado.
const PROP_KEYS: &[&str] = &[
    "prop/banco", "prop/farol", "prop/tacho_basura", "prop/cartel", "prop/hidrante",
    "prop/cerca", "prop/buzon", "prop/barril", "prop/caja", "prop/mesa_picnic",
    "prop/carpa", "prop/maceta", "prop/estatua", "prop/pozo",
];

/// Muebles de interior (LimeZu `Interiors_32x32.png`). Van dentro de edificios
/// (generados o construidos); pivote BottomCenter + Y-sort por la fila del pie.
const FURNITURE_KEYS: &[&str] = &[
    "furniture/cama", "furniture/mesa", "furniture/silla", "furniture/sofa",
    "furniture/cocina", "furniture/heladera", "furniture/pileta", "furniture/inodoro",
    "furniture/banera", "furniture/estante", "furniture/biblioteca", "furniture/lampara",
    "furniture/escritorio", "furniture/ropero", "furniture/mostrador", "furniture/planta",
];

/// Fachadas/edificios (LimeZu `4_Generic_Buildings` / `24_Additional_Houses`).
/// Cada edificio se recorta en POSES por capa: `base` (piso+paredes, Y-sort),
/// `roof` (techo, capa Roof → translúcido al entrar). Ver §techo en building.rs.
const BUILDING_KEYS: &[&str] = &[
    "building/casa_chica", "building/casa_mediana", "building/tienda",
    "building/deposito", "building/granero",
];

/// Assets GRANDES colocables (A2): un sprite único con footprint de colisión,
/// que el jugador coloca como una casa y rodea (Y-sort). Claves = `LARGE_ASSETS`
/// en `ha-shared::topdown`. El usuario asigna el pixel y pinta la huella (A1).
const STRUCTURE_KEYS: &[&str] =
    &["structure/boulder", "structure/statue", "structure/monument"];

/// Vehículos animados (LimeZu `Animated_32x32/Vehicles_32x32`).
const VEHICLE_KEYS: &[&str] =
    &["vehicle/auto", "vehicle/taxi", "vehicle/bus", "vehicle/helicoptero", "vehicle/tanque"];

/// Flora/decoración natural extra (LimeZu `17_Garden`). Complementa los
/// `decor/tree_*` existentes — no los duplica.
const NATURE_KEYS: &[&str] = &[
    "nature/arbusto", "nature/flores_rojas", "nature/flores_amarillas", "nature/pasto",
    "nature/tocon", "nature/tronco", "nature/hongo", "nature/cactus",
];

/// Piezas de construcción con AUTOTILE (LimeZu `Room_Builder_32x32.png`). El
/// editor guarda poses `center`/`edge_*`/`corner_*`/`inner_*` y el juego elige
/// la variante por vecinos (mismo pipeline que `tiles/water`). `door/*` además
/// admite poses `open`/`closed` (animación por proximidad).
const BUILD_ART_KEYS: &[&str] = &[
    "floor/wood", "floor/stone", "wall/wood", "wall/stone", "door/wood",
    "window/basic", "stairs/down", "stairs/up",
];

/// Plantillas de habitación prefab (LimeZu `6_Home_Designs`): arte opcional de
/// los presets de construcción (`room_small`…) — la LÓGICA vive en `ha-shared`.
const ROOM_KEYS: &[&str] =
    &["room/room_small", "room/room_medium", "room/room_large", "room/casa_1"];

/// Elementos de UI / HUD (hoy procedurales en `ui_theme.rs`/`hud.rs`/`hud/*`).
/// Audit completo: cada elemento visible tiene su slot; el HUD pide primero el
/// sprite y cae al dibujo procedural si no hay arte. Ítems y máquinas NO van acá
/// (ya tienen `items/*` / `machines/*`).
const UI_KEYS: &[&str] = &[
    // Paneles / botones base.
    "ui/panel", "ui/button", "ui/button_pressed", "ui/hotbar_slot", "ui/hotbar_slot_selected",
    "ui/slot_empty", "ui/close_button", "ui/tab_inventario", "ui/tab_craft", "ui/tab_construir",
    // Barras de survival (corazones + energía + hambre + sed + oxígeno).
    "ui/heart_full", "ui/heart_half", "ui/heart_empty", "ui/energy_full", "ui/energy_empty",
    "ui/hunger_full", "ui/hunger_half", "ui/hunger_empty", "ui/thirst_full", "ui/thirst_half",
    "ui/thirst_empty", "ui/oxygen_full", "ui/oxygen_empty",
    // Controles táctiles / acciones.
    "ui/joystick_base", "ui/joystick_knob", "ui/use_button", "ui/jump_button", "ui/bag_button",
    "ui/menu_button", "ui/crosshair", "ui/lantern_button", "ui/build_chip",
    "ui/hold_progress_bg", "ui/hold_progress_fill",
    // Iconos de acción / interacción.
    "ui/icon_atacar", "ui/icon_usar", "ui/icon_cavar", "ui/icon_interactuar", "ui/icon_craft",
    // Menú del juego / mapa.
    "ui/menu_tierra", "ui/menu_luna", "ui/menu_espacio", "ui/menu_creativo",
    "ui/menu_ajustes", "ui/minimapa_marco",
];

/// Fondos / cielo (hoy color + estrellas).
const BG_KEYS: &[&str] = &["backgrounds/sky_day", "backgrounds/sky_night", "backgrounds/stars"];

/// Lista canónica completa de slots del juego. `items/<id>` y `machines/<id>` se
/// generan de los catálogos; el resto son claves fijas documentadas.
pub fn game_slots() -> Vec<SlotDef> {
    let mut out = Vec::new();
    // Personaje / nave. `player/character` es la clave REAL que el juego dibuja
    // (PLAYER_ART_KEY en sim.rs): admite poses idle/walk × 4 direcciones y va
    // animado. `player/astronaut` queda como alias histórico.
    out.push(anim("player/character", "player"));
    out.push(anim("player/astronaut", "player"));
    // NPCs — mismo esquema de poses que el jugador (idle_down…). `npc/colonist`
    // se dibuja en la base lunar (uno por colono; ver world2d::sync_colonists).
    out.push(anim("npc/colonist", "npc"));
    out.push(anim("npc/aldeano", "npc"));
    out.push(anim("npc/comerciante", "npc"));
    // Fauna con cerebro IAFZ (FASE 8) — especies del pack LimeZu Modern Farm.
    // Poses del pack por dirección: idle/walk/eat/drink (perro: + run/bark).
    out.push(anim("npc/fauna_cow", "npc"));
    out.push(anim("npc/fauna_chicken", "npc"));
    out.push(anim("npc/fauna_sheep", "npc"));
    out.push(anim("npc/fauna_pig", "npc"));
    out.push(anim("npc/fauna_dog", "npc"));
    out.push(s("ship/rocket", "ship"));
    // Decoración.
    for k in DECOR_KEYS {
        out.push(s(k, "decor"));
    }
    // Props de exterior / muebles / edificios / vehículos / flora.
    for k in PROP_KEYS {
        out.push(s(k, "prop"));
    }
    for k in FURNITURE_KEYS {
        out.push(s(k, "furniture"));
    }
    for k in BUILDING_KEYS {
        out.push(s(k, "building"));
    }
    for k in STRUCTURE_KEYS {
        out.push(s(k, "structure"));
    }
    for k in VEHICLE_KEYS {
        out.push(anim(k, "vehicle"));
    }
    for k in NATURE_KEYS {
        out.push(s(k, "nature"));
    }
    // Piezas de construcción (autotile) + plantillas prefab.
    for k in BUILD_ART_KEYS {
        out.push(s(k, "build"));
    }
    for k in ROOM_KEYS {
        out.push(s(k, "room"));
    }
    // Contenedores con estado: el juego conmuta las poses `closed`/`open` y las
    // reproduce una sola vez (queda abierto mientras lo usás). Ver `chests.rs`.
    out.push(anim("props/chest", "props"));
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
        for expected in [
            "player/character", "player/astronaut", "decor/tree", "decor/tree_roble",
            "decor/ladder", "items/apple", "machines/wood_extractor", "tiles/water",
            "prop/banco", "furniture/cama", "building/casa_chica", "vehicle/auto",
            "nature/arbusto", "floor/wood", "wall/wood", "door/wood", "window/basic",
            "stairs/down", "room/room_small", "ui/hunger_full", "ui/thirst_half",
            "ui/oxygen_full", "ui/tab_construir", "ui/hold_progress_fill", "npc/aldeano",
        ] {
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
