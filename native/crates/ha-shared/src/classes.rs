//! Port of `packages/shared/src/sim/content/classes.ts`

#[derive(Clone, Debug)]
pub struct ClassDef {
    pub id: &'static str,
    pub name: &'static str,
    pub max_hp: f32,
    pub move_speed: f32,
    pub radius: f32,
    pub abilities: &'static [&'static str],
    pub tap_ability_slot: usize,
}

pub const DEFAULT_CLASS_ID: &str = "scout";

const SCOUT: ClassDef = ClassDef {
    id: "scout",
    name: "Explorador",
    max_hp: 90.0,
    move_speed: 4.0,
    radius: 0.35,
    abilities: &["dagger", "dash"],
    tap_ability_slot: 1,
};

const WARRIOR: ClassDef = ClassDef {
    id: "warrior",
    name: "Guerrero",
    max_hp: 140.0,
    move_speed: 3.8,
    radius: 0.38,
    abilities: &["cleave", "charge"],
    tap_ability_slot: 0,
};

const MAGE: ClassDef = ClassDef {
    id: "mage",
    name: "Mago",
    max_hp: 80.0,
    move_speed: 4.0,
    radius: 0.35,
    abilities: &["fireball", "frost_nova"],
    tap_ability_slot: 0,
};

pub fn get_class(id: &str) -> ClassDef {
    match id {
        "warrior" => WARRIOR.clone(),
        "mage" => MAGE.clone(),
        _ => SCOUT.clone(),
    }
}

/// Bevy `Capsule3d::new(radius, half_length)` — half_length along cylinder section.
pub fn capsule_half_length(radius: f32) -> f32 {
    radius * 2.2
}

/// World Y of capsule centre when feet are at `body.y == 0`.
pub fn capsule_center_above_feet(radius: f32) -> f32 {
    radius + capsule_half_length(radius)
}

pub fn eye_height_above_feet() -> f32 {
    crate::cosmic_scale::HUMAN_EYE_HEIGHT_M
}
