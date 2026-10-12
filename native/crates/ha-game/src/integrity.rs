//! **Integridad estructural** — lo que se apoyaba en la losa que sacaste se cae.
//!
//! Sin esto la verticalidad mentía: picabas el piso de la planta baja y el
//! segundo piso quedaba flotando en el aire, con sus paredes y su techo, colgado
//! de nada. El jugador se caía (eso ya andaba, `world2d::fall_through_void`) pero
//! la OBRA no, y quedaba un edificio imposible que además seguía frenándote.
//!
//! La regla es la misma que la de construir, leída al revés: una losa en altura
//! necesita apoyo (`build_grid::floor_supported`); si lo pierde, se cae. Y con
//! ella se van las piezas que ella misma sostenía —paredes de borde que ya no
//! tienen piso a ningún lado, tapas sin nada debajo—, así que un derrumbe se
//! propaga hacia arriba planta por planta hasta que todo lo que queda vuelve a
//! estar apoyado.
//!
//! **No cobra ni castiga**: los materiales se devuelven, como demoler a mano. El
//! derrumbe es una consecuencia física, no una trampa.
//!
//! Coste: cero mientras no toques una losa. Sólo corre cuando `TileWorld` avisa
//! que una dejó de existir, y trabaja sobre esa vecindad, no sobre el mundo.

use bevy::prelude::*;

use ha_shared::build_grid::{edge_cells, floor_supported, is_build_floor, WallAxis};
use ha_shared::CelestialBody;

use crate::ceilings::BuiltCeilings;
use crate::inventory_persist::PlayerInventory;
use crate::structures::BuiltStructures;
use crate::walls::BuiltWalls;
use crate::world2d::TileWorld;

/// Tope de plantas que puede recorrer un derrumbe en un frame. La verticalidad
/// llega a 3 pisos; el tope es el seguro de que un caso raro no se coma el frame.
const MAX_LEVELS: usize = 8;

pub struct IntegrityPlugin;

impl Plugin for IntegrityPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, collapse_unsupported);
    }
}

/// Las cuatro aristas de una celda, con su eje.
fn cell_edges(tx: i64, tz: i64) -> [(i64, i64, WallAxis); 4] {
    [
        (tx, tz, WallAxis::H),
        (tx, tz + 1, WallAxis::H),
        (tx, tz, WallAxis::V),
        (tx + 1, tz, WallAxis::V),
    ]
}

fn collapse_unsupported(
    mut tiles: ResMut<TileWorld>,
    mut walls: ResMut<BuiltWalls>,
    mut ceilings: ResMut<BuiltCeilings>,
    structures: Res<BuiltStructures>,
    mut inv: ResMut<PlayerInventory>,
) {
    let mut pending = tiles.take_removed_floors();
    if pending.is_empty() {
        return;
    }
    for _ in 0..MAX_LEVELS {
        if pending.is_empty() {
            break;
        }
        for (body_u8, level, tx, tz) in pending.drain(..) {
            let Some(body) = CelestialBody::from_u8(body_u8) else {
                continue;
            };
            // 1. Las piezas de BORDE de la propia celda que se quedaron sin piso
            //    a ninguno de sus dos lados.
            for (ex, ez, axis) in cell_edges(tx, tz) {
                if walls.at(body, level, ex, ez, axis).is_none() {
                    continue;
                }
                let apoyada = edge_cells(ex, ez, axis)
                    .iter()
                    .any(|&(cx, cz)| is_build_floor(tiles.tile_at_level(body, level, cx, cz)));
                if apoyada {
                    continue;
                }
                if let Some(w) = walls.remove(body, level, ex, ez, axis) {
                    let def = w.def();
                    inv.add(def.cost_item, def.cost_qty);
                }
            }
            // 2. La tapa de la celda: sin piso ni paredes alrededor no se sostiene.
            if ceilings.at(body, level, tx, tz).is_some()
                && !is_build_floor(tiles.tile_at_level(body, level, tx, tz))
                && !walls.any_edge_of_cell(body, level, tx, tz)
            {
                if let Some(c) = ceilings.remove(body, level, tx, tz) {
                    if let Some(def) = ha_shared::build_grid::ceiling_piece(c.piece) {
                        inv.add(def.cost_item, def.cost_qty);
                    }
                }
            }
            // 3. La losa de ARRIBA, si esta celda era lo único que la aguantaba.
            let above = level + 1;
            if above <= 0 || !is_build_floor(tiles.tile_at_level(body, above, tx, tz)) {
                continue;
            }
            let sostiene = |cx: i64, cz: i64| {
                tiles.supports_upper_floor(body, above - 1, cx, cz)
                    || walls.any_edge_of_cell(body, above - 1, cx, cz)
                    || structures.blocks(body, above - 1, cx, cz)
            };
            // El vecino del MISMO nivel sólo cuenta si él mismo está apoyado; si
            // no, dos losas huérfanas se sostendrían mutuamente en el aire y el
            // derrumbe se frenaría justo donde tiene que seguir.
            let vecino_firme = |cx: i64, cz: i64| {
                is_build_floor(tiles.tile_at_level(body, above, cx, cz)) && sostiene(cx, cz)
            };
            if floor_supported(above, &sostiene, &vecino_firme, tx, tz) {
                continue;
            }
            tiles.clear_floor_at_level(body, above, tx, tz);
            inv.add("wood", 1); // la losa vuelve a la mochila, como al demolerla
        }
        // `clear_floor_at_level` volvió a anotar lo que se cayó: ésa es la planta
        // siguiente del derrumbe.
        pending = tiles.take_removed_floors();
    }
}
