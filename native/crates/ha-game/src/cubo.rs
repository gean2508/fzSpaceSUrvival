//! **El cubo** — ordeñar, juntar agua o lava, volcarla (pedido del usuario,
//! 29 sep 2026; modelo Minecraft).
//!
//! - Se craftea con tres lingotes de hierro (receta 30).
//! - **Vacío**: tocando una vaca o una cabra adulta, sale leche (`fauna.rs`);
//!   tocando agua, se llena de agua; donde el líquido del planeta es lava (los
//!   mundos volcánicos, `geography::Archetype::Volcanic`), de lava. El metano de
//!   Titán no se junta.
//! - **Lleno**: tocando suelo seco se vuelca. El agua deja un charco de verdad
//!   (sirve para regar la huerta: `granja::RADIO_AGUA`); la lava en un mundo
//!   volcánico queda lava, y en cualquier otro se enfría en roca.
//! - Leche y agua se BEBEN (mantener sin objetivo) y devuelven el cubo vacío.
//! - Un lago es infinito (como la fuente de Minecraft): lo que se saca es de un
//!   charco que volcaste vos, y ése sí se vacía.

use bevy::prelude::*;
use ha_shared::geography::Archetype;
use ha_shared::topdown::Tile;

use crate::hud::inventory_ui::{
    add_item_to_slots, consume_from_ui_slots, flush_slots_to_inventory, InventoryUiState,
};
use crate::input::{EquippedItem, HoldInteractState};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::targeting::LookTarget;
use crate::world2d::{active_surface_body, ActivePlanet, TileWorld};

/// **La mano también cuenta**: con las manos vacías se bebe de un lago o un río. Sin esto el
/// agua dependía de tres lingotes de hierro (el cubo) y en los primeros minutos no había forma
/// de calmar la sed (reporte del 1 oct 2026: *"no hay forma de conseguir alimento"*).
pub fn es_cubo(item: &str) -> bool {
    matches!(item, "bucket" | "water_bucket" | "lava_bucket" | "hand")
}

/// Cuánta sed calma un trago a mano (el cubo de agua da 40; un trago es menos).
pub const TRAGO: f32 = 25.0;

fn es_liquido(t: Tile) -> bool {
    matches!(t, Tile::Water | Tile::WaterDeep)
}

/// ¿Tiene sentido usar este cubo sobre esta celda?
pub fn celda_valida(item: &str, t: Tile) -> bool {
    match item {
        "bucket" | "hand" => es_liquido(t),
        "water_bucket" | "lava_bucket" => t.buildable_ground(),
        _ => false,
    }
}

/// El cartel del HUD con el cubo apuntando una celda.
pub fn ayuda(item: &str) -> Option<&'static str> {
    Some(match item {
        "bucket" => "Mantené para llenar el cubo",
        "hand" => "Mantené para beber",
        "water_bucket" => "Mantené para volcar el agua",
        "lava_bucket" => "Mantené para volcar la lava",
        _ => return None,
    })
}

/// Qué se lleva un cubo vacío de un líquido de este cuerpo.
pub fn liquido_de(body: ha_shared::CelestialBody) -> Option<&'static str> {
    match ha_shared::geography::archetype(body) {
        Archetype::Volcanic => Some("lava_bucket"),
        Archetype::Oceanic => None, // metano: no sirve
        _ => Some("water_bucket"),
    }
}

pub struct CuboPlugin;

impl Plugin for CuboPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            usar_cubo
                .before(crate::hud::update_interact_hud)
                .run_if(crate::game_phase::world_active),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn usar_cubo(
    mut commands: Commands,
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut tiles: ResMut<TileWorld>,
    mut equipped: ResMut<EquippedItem>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut persist: ResMut<InventoryPersistState>,
    mut player: Query<&mut crate::sim::SimState, With<crate::sim::Player>>,
) {
    if !hold.completed || !hold.usar_cubo {
        return;
    }
    hold.completed = false;
    hold.usar_cubo = false;
    let Some((tx, tz, _)) = target.place_cell else { return };
    let (tx, tz) = (tx as i64, tz as i64);
    let item = equipped.item_id.clone();
    let body = active_surface_body(&ctx, &planet);
    let t = tiles.tile_at(body, tx, tz);
    let (x, z) = (tx as f32 + 0.5, tz as f32 + 0.5);
    if item == "hand" {
        if !es_liquido(t) {
            return;
        }
        if liquido_de(body) != Some("water_bucket") {
            crate::granja::globo(&mut commands, x, z, "Esto no se puede beber".into());
            return;
        }
        if let Ok(mut sim) = player.get_single_mut() {
            sim.survival.thirst = (sim.survival.thirst + TRAGO).min(ha_shared::THIRST_MAX);
        }
        crate::granja::globo(&mut commands, x, z, "Bebiste agua".into());
        return;
    }
    if inv.qty(&item) == 0 {
        return;
    }
    let (sale, mensaje): (&str, String) = match item.as_str() {
        "bucket" if es_liquido(t) => {
            let Some(lleno) = liquido_de(body) else {
                crate::granja::globo(&mut commands, x, z, "Esto no se puede juntar".into());
                return;
            };
            // El charco que volcaste vos se vacía; un lago es infinito.
            if t == Tile::Water && tiles.is_edited(body, tx, tz) {
                tiles.place_tile(body, tx, tz, Tile::Dirt);
            }
            let msg = if lleno == "lava_bucket" { "Juntaste lava" } else { "Juntaste agua" };
            (lleno, msg.into())
        }
        "water_bucket" if t.buildable_ground() => {
            tiles.place_tile(body, tx, tz, Tile::Water);
            ("bucket", "Volcaste el agua".into())
        }
        "lava_bucket" if t.buildable_ground() => {
            if liquido_de(body) == Some("lava_bucket") {
                tiles.place_tile(body, tx, tz, Tile::Water);
                ("bucket", "Volcaste la lava".into())
            } else {
                // En un mundo que no es de lava, se enfría en el acto.
                tiles.place_tile(body, tx, tz, Tile::RockDark);
                ("bucket", "La lava se enfrió en roca".into())
            }
        }
        _ => return,
    };
    consume_from_ui_slots(&mut inv_ui, &[(item.as_str(), 1)]);
    add_item_to_slots(&mut inv_ui, sale, 1);
    flush_slots_to_inventory(&inv_ui, &mut inv);
    persist.mark_immediate();
    if inv.qty(&item) == 0 {
        equipped.item_id = sale.to_string();
    }
    crate::granja::globo(&mut commands, x, z, mensaje);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn con_la_mano_vacia_se_bebe_del_agua_y_de_nada_mas() {
        assert!(es_cubo("hand"));
        assert!(celda_valida("hand", Tile::Water));
        assert!(celda_valida("hand", Tile::WaterDeep));
        assert!(!celda_valida("hand", Tile::Grass));
        assert_eq!(ayuda("hand"), Some("Mantené para beber"));
    }

    #[test]
    fn el_cubo_sabe_que_hacer_en_cada_celda() {
        assert!(celda_valida("bucket", Tile::Water));
        assert!(celda_valida("bucket", Tile::WaterDeep));
        assert!(!celda_valida("bucket", Tile::Grass));
        assert!(celda_valida("water_bucket", Tile::Grass));
        assert!(!celda_valida("water_bucket", Tile::Water));
    }

    #[test]
    fn cada_mundo_da_su_liquido() {
        use ha_shared::CelestialBody as B;
        assert_eq!(liquido_de(B::Earth), Some("water_bucket"));
        assert_eq!(liquido_de(B::Io), Some("lava_bucket"));
        assert_eq!(liquido_de(B::Titan), None);
    }

    /// Beber un cubo devuelve el cubo (y se puede beber).
    #[test]
    fn leche_y_agua_se_beben() {
        assert!(ha_shared::food_restore("milk_bucket").is_some());
        assert!(ha_shared::food_restore("water_bucket").is_some());
        assert!(ha_shared::food_restore("lava_bucket").is_none());
    }
}
