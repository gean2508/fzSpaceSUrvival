//! **Frutales: la comida que vuelve a crecer.**
//!
//! Un tercio de los árboles (los que ya tiraban una manzana al talarlos) son frutales. Con la
//! mano vacía NO se talan: se sacuden, dan manzanas y vuelven a dar a los diez minutos. Antes la
//! manzana era un premio de un solo uso que costaba el árbol, y en los primeros minutos de una
//! partida no había de dónde comer de forma sostenida (reporte del 1 oct 2026). Con hacha (o
//! cualquier herramienta) el árbol se tala como siempre.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::hud::inventory_ui::{add_item_to_slots, flush_slots_to_inventory, InventoryUiState};
use crate::input::{EquippedItem, HoldInteractState};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::targeting::LookTarget;

/// Segundos hasta que un frutal vuelve a dar fruta (un sexto del día de 3600 s).
pub const REBROTE_SECS: f64 = 600.0;
/// Manzanas por sacudida.
pub const MANZANAS: u32 = 2;

/// ¿Este árbol (por el id de su celda) es un frutal? Es la misma regla que ya existía para la
/// manzana suelta: el azar sale de la celda, así que el mismo árbol es siempre el mismo.
pub fn es_frutal(id: u32) -> bool {
    id % 3 == 0
}

#[derive(Resource, Default)]
pub struct Frutales {
    /// Id de celda → instante (reloj real, s) en que vuelve a tener fruta.
    listo_en: HashMap<u32, f64>,
}

impl Frutales {
    /// Intenta cosechar. `true` si había fruta (y la deja en cooldown).
    pub fn cosechar(&mut self, id: u32, ahora: f64) -> bool {
        if self.listo_en.get(&id).is_some_and(|&t| ahora < t) {
            return false;
        }
        self.listo_en.insert(id, ahora + REBROTE_SECS);
        true
    }
}

pub struct FrutalesPlugin;

impl Plugin for FrutalesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Frutales>().add_systems(
            Update,
            sacudir_frutal
                .before(crate::hud::update_interact_hud)
                .run_if(crate::game_phase::world_active),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn sacudir_frutal(
    mut commands: Commands,
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
    equipped: Res<EquippedItem>,
    time: Res<Time<Real>>,
    mut frutales: ResMut<Frutales>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut persist: ResMut<InventoryPersistState>,
) {
    if !hold.completed || target.kind != Some(ha_shared::InteractTargetKind::Tree) || equipped.item_id != "hand" {
        return;
    }
    let Some((_, _, id)) = target.flora_cell else { return };
    if !es_frutal(id) {
        return;
    }
    hold.completed = false;
    let p = target.hit_point;
    if frutales.cosechar(id, time.elapsed_secs_f64()) {
        add_item_to_slots(&mut inv_ui, "apple", MANZANAS);
        flush_slots_to_inventory(&inv_ui, &mut inv);
        persist.mark_immediate();
        crate::granja::globo(&mut commands, p.x, -p.y, format!("+{MANZANAS} Manzana"));
    } else {
        crate::granja::globo(&mut commands, p.x, -p.y, "Todavía no hay fruta".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_frutal_da_fruta_y_tarda_en_volver() {
        let mut f = Frutales::default();
        assert!(f.cosechar(3, 0.0));
        assert!(!f.cosechar(3, 10.0), "recién sacudido");
        assert!(f.cosechar(3, REBROTE_SECS + 1.0), "ya rebrotó");
        assert!(f.cosechar(6, 5.0), "otro árbol no comparte reloj");
    }

    #[test]
    fn un_tercio_de_los_arboles_es_frutal() {
        let n = (0..3000u32).filter(|&i| es_frutal(i)).count();
        assert_eq!(n, 1000);
    }
}
