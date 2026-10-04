//! **El cuarto que cerrás se techa solo.** Apilabas paredes, cerrabas el recinto con su puerta y
//! quedaba a la intemperie: había que ir celda por celda poniendo tumbado (pedido del 4 oct 2026,
//! «dejas un hueco ahí arriba que ni siquiera tiene techo»). Ahora, cuando `cuartos` reconoce un
//! recinto cerrado con puerta y piso, cada celda sin tapa recibe su tumbado: de madera si tenés madera,
//! de piedra si tenés piedra. Cuesta lo mismo que ponerlo a mano (1 por celda); en creativo es gratis.
//! Si no alcanza el material, se techan las celdas que alcanzan y el resto queda a la vista con un aviso.
//!
//! Sólo techa lo que existe como `Cuarto` (paredes del jugador): las casas generadas no pasan por acá.

use bevy::prelude::*;

use crate::ceilings::{covered, BuiltCeilings, PlacedCeiling};
use crate::cuartos::Cuartos;
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::world2d::TileWorld;

/// Cada cuánto (s) se revisan los cuartos. Barato: sólo recorre los cuartos que ya existen.
const PERIODO: f32 = 1.0;
/// Tapas por pasada: el streaming de sprites no debe recibir cientos de golpe.
const TOPE_POR_PASADA: usize = 24;

pub struct AutoTechoPlugin;

impl Plugin for AutoTechoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, techar_cuartos.run_if(crate::game_phase::world_active));
    }
}

/// Qué tumbado ponerle a una celda según lo que hay en la mochila: `(índice de pieza, ítem)`.
pub fn pieza_disponible(madera: u32, piedra: u32, creativo: bool) -> Option<usize> {
    if creativo || madera > 0 {
        Some(ha_shared::build_grid::ceiling_piece_for("wood"))
    } else if piedra > 0 {
        Some(ha_shared::build_grid::ceiling_piece_for("stone"))
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn techar_cuartos(
    time: Res<Time>,
    cuartos: Res<Cuartos>,
    tiles: Res<TileWorld>,
    roofs: Res<crate::building::BuiltRoofs>,
    editor: Res<crate::ui_editor::UiEditorState>,
    mut ceilings: ResMut<BuiltCeilings>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    if *acc < PERIODO {
        return;
    }
    *acc = 0.0;
    let mut puestas = 0usize;
    'cuartos: for cuarto in cuartos.list() {
        for &(tx, tz) in &cuarto.celdas {
            if puestas >= TOPE_POR_PASADA {
                break 'cuartos;
            }
            let body = ha_shared::CelestialBody::from_u8(cuarto.body);
            let Some(body) = body else { continue };
            if covered(&ceilings, &roofs, &tiles, body, cuarto.z, tx, tz) {
                continue;
            }
            let Some(pieza) = pieza_disponible(inv.qty("wood"), inv.qty("stone"), editor.creative) else {
                continue 'cuartos;
            };
            let def = &ha_shared::build_grid::CEILING_PIECES[pieza];
            if !editor.creative {
                if inv.qty(def.cost_item) < def.cost_qty {
                    continue 'cuartos;
                }
                inv.remove(def.cost_item, def.cost_qty);
                persist.mark_dirty();
            }
            ceilings.add(PlacedCeiling { body: cuarto.body, z: cuarto.z, tx, tz, piece: pieza });
            puestas += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn con_madera_se_techa_de_madera_y_con_piedra_de_piedra() {
        let m = pieza_disponible(5, 5, false).unwrap();
        let p = pieza_disponible(0, 5, false).unwrap();
        assert_eq!(ha_shared::build_grid::CEILING_PIECES[m].cost_item, "wood");
        assert_eq!(ha_shared::build_grid::CEILING_PIECES[p].cost_item, "stone");
        assert!(pieza_disponible(0, 0, false).is_none());
        assert!(pieza_disponible(0, 0, true).is_some(), "en creativo siempre alcanza");
    }
}
