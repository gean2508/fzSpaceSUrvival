//! Port of `packages/shared/src/content/interactions.ts`

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractTargetKind {
    Rock,
    Tree,
    Water,
    Entity,
    Container,
    /// Cofre colocado por el jugador — abre su panel de almacenamiento
    /// (`ha-game/src/chests.rs`), no se "recoge" como un contenedor de worldgen.
    Chest,
    OreIron,
    OreGold,
    GemBlue,
    GemRed,
    Block,
    /// **Cualquier cosa que el mundo haya sembrado**: el banco de la vereda, el
    /// farol, la mata, el cajón del galpón, el nenúfar. No es `Entity` —eso es lo
    /// que colocó el jugador y vive en `BuiltStructures`— ni `Container`: es
    /// decorado del generador, y hasta el 15 ago 2026 no se podía ni tocar.
    ///
    /// Se levanta con el mismo gesto que todo lo demás y lo resuelve
    /// `harvest::pick_up_prop`.
    Prop,
    LaunchModule,
    LaunchIgnition,
    SpaceReentry,
    SpaceTransitLuna,
    SpaceTransitBody,
    SpaceStationModule,
    LunarReturn,
    PlanetReturn,
    ProbeConsole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractActionKind {
    Mine,
    Attack,
    Collect,
    Gather,
    None,
    Assemble,
    Launch,
    ReturnEarth,
    TravelLuna,
    TravelBody,
    LaunchProbe,
}

#[derive(Clone, Copy, Debug)]
pub struct InteractionRule {
    pub equipped_item_id: &'static str,
    pub target_kind: InteractTargetKind,
    pub kind: InteractActionKind,
    pub duration_ms: u32,
    pub item_id: Option<&'static str>,
    pub damage: Option<f32>,
}

pub const INTERACT_REACH: f32 = 4.5;
pub const GESTURE_HOLD_START_MS: u32 = 200;
pub const GESTURE_TAP_MAX_MS: u32 = 150;
pub const GESTURE_MOVE_CANCEL_PX: f32 = 14.0;
/// Looser drift tolerance while holding flight / launch beacons.
pub const GESTURE_FLIGHT_MOVE_CANCEL_PX: f32 = 36.0;
/// Wait before hammer placement hold — gives targeting time to lock flight beacons.
pub const GESTURE_MACHINE_HOLD_DELAY_MS: u32 = 420;

pub static INTERACTION_RULES: &[InteractionRule] = &[
    rule("pickaxe", InteractTargetKind::Rock, InteractActionKind::Mine, 800, Some("stone"), None),
    rule(
        "pickaxe",
        InteractTargetKind::OreIron,
        InteractActionKind::Mine,
        950,
        Some("iron_ingot"),
        None,
    ),
    rule(
        "pickaxe",
        InteractTargetKind::OreGold,
        InteractActionKind::Mine,
        1100,
        Some("gold_ingot"),
        None,
    ),
    rule(
        "pickaxe",
        InteractTargetKind::GemBlue,
        InteractActionKind::Mine,
        1000,
        Some("gem_blue"),
        None,
    ),
    rule(
        "pickaxe",
        InteractTargetKind::GemRed,
        InteractActionKind::Mine,
        1000,
        Some("gem_red"),
        None,
    ),
    rule("axe", InteractTargetKind::Tree, InteractActionKind::Gather, 1200, Some("wood"), None),
    rule(
        "sword_iron",
        InteractTargetKind::Entity,
        InteractActionKind::Attack,
        400,
        None,
        Some(18.0),
    ),
    rule(
        "sword_steel",
        InteractTargetKind::Entity,
        InteractActionKind::Attack,
        350,
        None,
        Some(26.0),
    ),
    rule(
        "watering_can",
        InteractTargetKind::Water,
        InteractActionKind::Collect,
        600,
        Some("supply"),
        None,
    ),
    rule("hand", InteractTargetKind::Tree, InteractActionKind::Gather, 2200, Some("wood"), None),
    // Placed voxel blocks — break refunds the block's own item (handler decides).
    rule("hand", InteractTargetKind::Block, InteractActionKind::Mine, 750, None, None),
    rule("pickaxe", InteractTargetKind::Block, InteractActionKind::Mine, 350, None, None),
    rule("axe", InteractTargetKind::Block, InteractActionKind::Mine, 450, None, None),
    // **Un prop se levanta con la mano, sin herramienta y rápido.** Sin `item_id`
    // a propósito: lo que devuelve sale del CATÁLOGO del propio objeto (una
    // heladera devuelve lo que costó), y eso lo sabe `harvest::pick_up_prop`, no
    // esta tabla estática. 550 ms es corto porque es lo que más se va a hacer:
    // el mundo está lleno de props y pelear medio segundo con cada uno cansa.
    rule("hand", InteractTargetKind::Prop, InteractActionKind::Gather, 550, None, None),
    rule("hand", InteractTargetKind::Rock, InteractActionKind::None, 0, None, None),
    rule("hand", InteractTargetKind::OreIron, InteractActionKind::None, 0, None, None),
    rule("hand", InteractTargetKind::OreGold, InteractActionKind::None, 0, None, None),
    rule(
        "hand",
        InteractTargetKind::GemBlue,
        InteractActionKind::Gather,
        2000,
        Some("gem_blue"),
        None,
    ),
    rule(
        "hand",
        InteractTargetKind::GemRed,
        InteractActionKind::Gather,
        2000,
        Some("gem_red"),
        None,
    ),
    rule(
        "hand",
        InteractTargetKind::Container,
        InteractActionKind::Gather,
        900,
        Some("supply"),
        None,
    ),
    // Abrir un cofre es instantáneo-ish y NO produce ítem: lo consume
    // `chests.rs` (registrado en `hold_target_handled_elsewhere`).
    rule(
        "hand",
        InteractTargetKind::Chest,
        InteractActionKind::Collect,
        320,
        None,
        None,
    ),
    rule(
        "hand",
        InteractTargetKind::LaunchModule,
        InteractActionKind::Assemble,
        3500,
        None,
        None,
    ),
    rule(
        "hand",
        InteractTargetKind::LaunchIgnition,
        InteractActionKind::Launch,
        2500,
        None,
        None,
    ),
    rule(
        "hand",
        InteractTargetKind::SpaceReentry,
        InteractActionKind::ReturnEarth,
        2200,
        None,
        None,
    ),
    rule(
        "hand",
        InteractTargetKind::SpaceTransitLuna,
        InteractActionKind::TravelLuna,
        1800,
        None,
        None,
    ),
    rule(
        "hand",
        InteractTargetKind::SpaceStationModule,
        InteractActionKind::Assemble,
        3000,
        None,
        None,
    ),
    rule(
        "hand",
        InteractTargetKind::LunarReturn,
        InteractActionKind::ReturnEarth,
        2000,
        None,
        None,
    ),
    rule(
        "hand",
        InteractTargetKind::SpaceTransitBody,
        InteractActionKind::TravelBody,
        1800,
        None,
        None,
    ),
    rule(
        "hand",
        InteractTargetKind::PlanetReturn,
        InteractActionKind::ReturnEarth,
        2000,
        None,
        None,
    ),
    rule(
        "hand",
        InteractTargetKind::ProbeConsole,
        InteractActionKind::LaunchProbe,
        2000,
        None,
        None,
    ),
    // --- Tiered tools (Minecraft progression) --------------------------------
    // Axes chop trees faster than bare hands.
    rule("wood_axe", InteractTargetKind::Tree, InteractActionKind::Gather, 1100, Some("wood"), None),
    rule("stone_axe", InteractTargetKind::Tree, InteractActionKind::Gather, 750, Some("wood"), None),
    // Wood pickaxe: stone + gems only. It has NO ore rule, so `resolve_interaction`
    // falls back to hand → None → ore stays gated until a stone pickaxe.
    rule("wood_pickaxe", InteractTargetKind::Rock, InteractActionKind::Mine, 900, Some("stone"), None),
    rule("wood_pickaxe", InteractTargetKind::GemBlue, InteractActionKind::Mine, 1000, Some("gem_blue"), None),
    rule("wood_pickaxe", InteractTargetKind::GemRed, InteractActionKind::Mine, 1000, Some("gem_red"), None),
    // Stone pickaxe: unlocks iron/gold ore.
    rule("stone_pickaxe", InteractTargetKind::Rock, InteractActionKind::Mine, 650, Some("stone"), None),
    rule("stone_pickaxe", InteractTargetKind::OreIron, InteractActionKind::Mine, 900, Some("iron_ingot"), None),
    rule("stone_pickaxe", InteractTargetKind::OreGold, InteractActionKind::Mine, 1050, Some("gold_ingot"), None),
    rule("stone_pickaxe", InteractTargetKind::GemBlue, InteractActionKind::Mine, 900, Some("gem_blue"), None),
    rule("stone_pickaxe", InteractTargetKind::GemRed, InteractActionKind::Mine, 900, Some("gem_red"), None),
];

pub fn interact_target_for_kind(kind: crate::world::InteractableKind) -> InteractTargetKind {
    use crate::world::InteractableKind;
    match kind {
        InteractableKind::Tree => InteractTargetKind::Tree,
        InteractableKind::Rock => InteractTargetKind::Rock,
        InteractableKind::Container => InteractTargetKind::Container,
        InteractableKind::OreIron => InteractTargetKind::OreIron,
        InteractableKind::OreGold => InteractTargetKind::OreGold,
        InteractableKind::GemBlue => InteractTargetKind::GemBlue,
        InteractableKind::GemRed => InteractTargetKind::GemRed,
    }
}

const fn rule(
    equipped: &'static str,
    target: InteractTargetKind,
    kind: InteractActionKind,
    duration_ms: u32,
    item_id: Option<&'static str>,
    damage: Option<f32>,
) -> InteractionRule {
    InteractionRule {
        equipped_item_id: equipped,
        target_kind: target,
        kind,
        duration_ms,
        item_id,
        damage,
    }
}

pub fn resolve_interaction(
    equipped_item_id: Option<&str>,
    target_kind: InteractTargetKind,
) -> Option<&'static InteractionRule> {
    let equipped = equipped_item_id.filter(|s| !s.is_empty()).unwrap_or("hand");
    if let Some(r) = find_rule(equipped, target_kind) {
        return Some(r);
    }
    if equipped != "hand" {
        return find_rule("hand", target_kind);
    }
    None
}

fn find_rule(equipped: &str, target: InteractTargetKind) -> Option<&'static InteractionRule> {
    INTERACTION_RULES
        .iter()
        .find(|r| r.equipped_item_id == equipped && r.target_kind == target)
}

pub fn interaction_duration_ms(equipped_item_id: &str, target_kind: InteractTargetKind) -> u32 {
    resolve_interaction(Some(equipped_item_id), target_kind)
        .map(|r| r.duration_ms)
        .unwrap_or(0)
}

/// Launch pad + orbital hub holds — wider reach and drift tolerance on client.
pub fn is_flight_hold_target(kind: InteractTargetKind) -> bool {
    matches!(
        kind,
        InteractTargetKind::LaunchModule
            | InteractTargetKind::LaunchIgnition
            | InteractTargetKind::SpaceReentry
            | InteractTargetKind::SpaceTransitLuna
            | InteractTargetKind::SpaceTransitBody
            | InteractTargetKind::SpaceStationModule
            | InteractTargetKind::LunarReturn
            | InteractTargetKind::PlanetReturn
    )
}

pub fn interact_reach_for(kind: InteractTargetKind) -> f32 {
    match kind {
        InteractTargetKind::LaunchIgnition
        | InteractTargetKind::SpaceReentry
        | InteractTargetKind::SpaceTransitLuna
        | InteractTargetKind::SpaceTransitBody
        | InteractTargetKind::LunarReturn
        |         InteractTargetKind::PlanetReturn => 6.2,
        InteractTargetKind::LaunchModule | InteractTargetKind::SpaceStationModule => 5.5,
        _ => INTERACT_REACH,
    }
}

pub fn interact_radius_for(kind: InteractTargetKind) -> f32 {
    match kind {
        InteractTargetKind::LaunchIgnition
        | InteractTargetKind::SpaceReentry
        | InteractTargetKind::SpaceTransitLuna
        | InteractTargetKind::LunarReturn => 2.8,
        InteractTargetKind::LaunchModule | InteractTargetKind::SpaceStationModule => 2.3,
        _ => 1.8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_count_stable() {
        // 28 base + 10 tiered tool rules (wood/stone axe + pickaxe, jul 2026)
        // + 1 abrir cofre (`InteractTargetKind::Chest`)
        // + 1 levantar prop del mundo (`InteractTargetKind::Prop`, 15 ago 2026).
        assert_eq!(INTERACTION_RULES.len(), 40);
    }

    #[test]
    fn block_rules_resolve() {
        let r = resolve_interaction(Some("pickaxe"), InteractTargetKind::Block).unwrap();
        assert_eq!(r.kind, InteractActionKind::Mine);
        assert!(r.duration_ms < 800);
        // Hand fallback also breaks blocks, slower.
        let hand = resolve_interaction(Some("hand"), InteractTargetKind::Block).unwrap();
        assert!(hand.duration_ms > r.duration_ms);
    }

    #[test]
    fn pickaxe_mines_rock() {
        let r = resolve_interaction(Some("pickaxe"), InteractTargetKind::Rock).unwrap();
        assert_eq!(r.kind, InteractActionKind::Mine);
        assert_eq!(r.duration_ms, 800);
        assert_eq!(r.item_id, Some("stone"));
    }

    #[test]
    fn hand_on_rock_none() {
        let r = resolve_interaction(Some("hand"), InteractTargetKind::Rock).unwrap();
        assert_eq!(r.kind, InteractActionKind::None);
    }

    #[test]
    fn duration_deterministic() {
        assert_eq!(interaction_duration_ms("pickaxe", InteractTargetKind::Rock), 800);
        assert_eq!(interaction_duration_ms("pickaxe", InteractTargetKind::Rock), 800);
    }
}
