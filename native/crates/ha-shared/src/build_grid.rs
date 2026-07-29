//! Construcción por **cuadrícula + bordes** (estilo *Last Day on Earth*).
//!
//! Dos capas distintas, no una:
//! - **Pisos**: ocupan la CELDA (`Tile::WoodFloor` en el grid de `topdown.rs`).
//!   Se colocan libres, en cualquier parte del mapa; la grilla es global (1 tile
//!   = 1 m), así que no hay "zona de base" ni offsets relativos que mantener.
//! - **Paredes / puertas / ventanas**: ocupan el **BORDE** entre dos celdas, no
//!   la celda. Un borde se identifica por `(tx, tz, axis)`:
//!   - `WallAxis::H` = línea `z = tz`, tramo `x ∈ [tx, tx+1]` (separa la celda de
//!     arriba `(tx, tz-1)` de la de abajo `(tx, tz)`).
//!   - `WallAxis::V` = línea `x = tx`, tramo `z ∈ [tz, tz+1]` (separa `(tx-1, tz)`
//!     de `(tx, tz)`).
//!
//! Por qué bordes y no celdas: es lo que hace que una habitación de 3×3 tenga
//! 9 tiles de piso caminables (y no 1 útil rodeado de 8 de muro), que la pared
//! sea fina al caminar y que puerta/ventana caigan justo en la línea del muro.
//!
//! Este módulo es SÓLO reglas puras (sin Bevy, sin IO): catálogo de piezas,
//! snapping del borde más cercano, validación de apoyo y colisión cápsula-vs-
//! pared. El estado vivo (lo construido, su persistencia y sus sprites) es
//! `ha-game::walls`.

use serde::{Deserialize, Serialize};

use crate::topdown::{Prefab, Tile, PLAYER_RADIUS, TILE_M};

/// Grosor de COLISIÓN de una pared, en metros. Fino a propósito: la pared se
/// dibuja alta (`WALL_RENDER_H`) pero sólo roba esto de espacio caminable.
pub const WALL_THICK: f32 = 0.26;

/// Alto VISUAL de una pared en tiles. El sprite se ancla al pie del borde y
/// crece hacia arriba en pantalla (perspectiva 3/4).
pub const WALL_RENDER_H: f32 = 1.5;

/// Orientación de un borde de la cuadrícula.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WallAxis {
    /// Horizontal: corre este-oeste sobre la línea `z = tz`.
    H,
    /// Vertical: corre norte-sur sobre la línea `x = tx`.
    V,
}

impl WallAxis {
    pub fn as_u8(self) -> u8 {
        match self {
            WallAxis::H => 0,
            WallAxis::V => 1,
        }
    }
    pub fn from_u8(v: u8) -> Self {
        if v == 1 {
            WallAxis::V
        } else {
            WallAxis::H
        }
    }
}

/// Qué es la pieza que ocupa el borde.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WallKind {
    Wall,
    Door,
    Window,
}

impl WallKind {
    /// ¿Corta el paso? La puerta NO (se cruza; su sprite se abre por
    /// proximidad); pared y ventana sí.
    pub fn blocks(self) -> bool {
        !matches!(self, WallKind::Door)
    }
}

/// Una pieza de borde del catálogo (paleta de construcción).
#[derive(Clone, Copy, Debug)]
pub struct EdgePiece {
    pub id: &'static str,
    pub label: &'static str,
    /// Slot de arte (`wall/wood`): sus poses `front`/`back`/`left`/`right` las
    /// recorta el usuario en el editor de assets.
    pub art: &'static str,
    pub kind: WallKind,
    pub cost_item: &'static str,
    pub cost_qty: u32,
}

/// Catálogo ordenado de piezas de borde (el índice es el slot de paleta).
pub static EDGE_PIECES: &[EdgePiece] = &[
    EdgePiece {
        id: "edge_wall_wood",
        label: "Pared madera",
        art: "wall/wood",
        kind: WallKind::Wall,
        cost_item: "wood",
        cost_qty: 2,
    },
    EdgePiece {
        id: "edge_wall_stone",
        label: "Pared piedra",
        art: "wall/stone",
        kind: WallKind::Wall,
        cost_item: "stone",
        cost_qty: 2,
    },
    EdgePiece {
        id: "edge_door_wood",
        label: "Puerta",
        art: "door/wood",
        kind: WallKind::Door,
        cost_item: "wood",
        cost_qty: 3,
    },
    EdgePiece {
        id: "edge_window",
        label: "Ventana",
        art: "window/basic",
        kind: WallKind::Window,
        cost_item: "wood",
        cost_qty: 2,
    },
];

pub fn edge_piece_count() -> usize {
    EDGE_PIECES.len()
}

pub fn edge_piece(idx: usize) -> Option<&'static EdgePiece> {
    EDGE_PIECES.get(idx)
}

/// Índice de paleta de una pieza por id (para resolver lo persistido).
pub fn edge_piece_index(id: &str) -> Option<usize> {
    EDGE_PIECES.iter().position(|p| p.id == id)
}

/// Pieza de madera por defecto para lo que no resuelva (nunca falla).
pub fn default_edge_piece() -> &'static EdgePiece {
    &EDGE_PIECES[0]
}

/// Las dos celdas que el borde separa: `[antes, después]` en el eje del borde
/// (H → norte/sur; V → oeste/este).
pub fn edge_cells(tx: i64, tz: i64, axis: WallAxis) -> [(i64, i64); 2] {
    match axis {
        WallAxis::H => [(tx, tz - 1), (tx, tz)],
        WallAxis::V => [(tx - 1, tz), (tx, tz)],
    }
}

/// Centro del borde en coordenadas de mundo (x, z).
pub fn edge_center(tx: i64, tz: i64, axis: WallAxis) -> (f32, f32) {
    match axis {
        WallAxis::H => ((tx as f32 + 0.5) * TILE_M, tz as f32 * TILE_M),
        WallAxis::V => (tx as f32 * TILE_M, (tz as f32 + 0.5) * TILE_M),
    }
}

/// **Snapping**: borde de la cuadrícula más cercano al punto de mundo `(wx, wz)`.
/// Es lo único que traduce "dónde apunta el dedo" → "qué borde". Mira las 4
/// aristas de la celda apuntada y devuelve la más cercana.
pub fn nearest_edge(wx: f32, wz: f32) -> (i64, i64, WallAxis) {
    let tx = (wx / TILE_M).floor() as i64;
    let tz = (wz / TILE_M).floor() as i64;
    let fx = wx / TILE_M - tx as f32; // 0..1 dentro de la celda
    let fz = wz / TILE_M - tz as f32;
    // (distancia, borde)
    let candidates = [
        (fz, (tx, tz, WallAxis::H)),           // norte
        (1.0 - fz, (tx, tz + 1, WallAxis::H)), // sur
        (fx, (tx, tz, WallAxis::V)),           // oeste
        (1.0 - fx, (tx + 1, tz, WallAxis::V)), // este
    ];
    let mut best = candidates[0];
    for c in &candidates[1..] {
        if c.0 < best.0 {
            best = *c;
        }
    }
    best.1
}

/// ¿El tile cuenta como PISO de base para apoyar una pared? Sólo el piso
/// construido — así la primera pared exige haber puesto piso (regla LDoE).
pub fn is_build_floor(t: Tile) -> bool {
    matches!(t, Tile::WoodFloor)
}

/// **Validación de apoyo**: una pared/puerta/ventana necesita piso construido en
/// al menos UNA de las dos celdas que separa (así se pega al borde del suelo).
pub fn wall_supported(has_floor: &dyn Fn(i64, i64) -> bool, tx: i64, tz: i64, axis: WallAxis) -> bool {
    edge_cells(tx, tz, axis).iter().any(|&(cx, cz)| has_floor(cx, cz))
}

/// **Colisión**: ¿la cápsula del jugador centrada en `(x, z)` pisa alguna pared
/// que corte el paso? `blocked(tx, tz, axis)` responde por borde.
///
/// Cada pared es un rectángulo fino: V → `x ∈ [tx±WALL_THICK/2]`, `z ∈ [tz, tz+1]`;
/// H → `z ∈ [tz±WALL_THICK/2]`, `x ∈ [tx, tx+1]`. Se prueban sólo los bordes que
/// tocan el AABB de la cápsula (a lo sumo un puñado).
pub fn wall_capsule_hits(x: f32, z: f32, blocked: &dyn Fn(i64, i64, WallAxis) -> bool) -> bool {
    let h = WALL_THICK * 0.5;
    let (x0, x1) = (x - PLAYER_RADIUS, x + PLAYER_RADIUS);
    let (z0, z1) = (z - PLAYER_RADIUS, z + PLAYER_RADIUS);
    let (cx0, cx1) = (((x0 - h) / TILE_M).floor() as i64, ((x1 + h) / TILE_M).ceil() as i64);
    let (cz0, cz1) = (((z0 - h) / TILE_M).floor() as i64, ((z1 + h) / TILE_M).ceil() as i64);

    // Verticales: línea x = tx, tramo de una celda en z.
    for tx in cx0..=cx1 {
        let wx = tx as f32 * TILE_M;
        if x1 <= wx - h || x0 >= wx + h {
            continue;
        }
        for tz in cz0..=cz1 {
            let (za, zb) = (tz as f32 * TILE_M, (tz + 1) as f32 * TILE_M);
            if z1 > za && z0 < zb && blocked(tx, tz, WallAxis::V) {
                return true;
            }
        }
    }
    // Horizontales: línea z = tz, tramo de una celda en x.
    for tz in cz0..=cz1 {
        let wz = tz as f32 * TILE_M;
        if z1 <= wz - h || z0 >= wz + h {
            continue;
        }
        for tx in cx0..=cx1 {
            let (xa, xb) = (tx as f32 * TILE_M, (tx + 1) as f32 * TILE_M);
            if x1 > xa && x0 < xb && blocked(tx, tz, WallAxis::H) {
                return true;
            }
        }
    }
    false
}

/// Bordes que estampa un prefab colocado con su esquina mínima en
/// `(min_x, min_z)`: perímetro completo + tabiques interiores, con las puertas
/// en su lugar. El piso lo estampa aparte (toda la huella `w×h`).
///
/// Traduce el catálogo viejo (que describía la habitación por CELDAS de muro) al
/// modelo de bordes: una celda de muro del perímetro es la arista externa de la
/// habitación, y un tabique interior en la columna `dx` es la arista vertical
/// `x = min_x + dx`.
pub fn prefab_edges(p: &Prefab, min_x: i64, min_z: i64) -> Vec<(i64, i64, WallAxis, WallKind)> {
    let mut out = Vec::new();
    // Perímetro: dos líneas H (norte y sur) y dos V (oeste y este).
    for dx in 0..p.w {
        out.push((min_x + dx, min_z, WallAxis::H, WallKind::Wall));
        out.push((min_x + dx, min_z + p.h, WallAxis::H, WallKind::Wall));
    }
    for dz in 0..p.h {
        out.push((min_x, min_z + dz, WallAxis::V, WallKind::Wall));
        out.push((min_x + p.w, min_z + dz, WallAxis::V, WallKind::Wall));
    }
    // Tabiques interiores (columnas de muro) → aristas verticales.
    for &(dx, dz) in p.interior_walls {
        out.push((min_x + dx, min_z + dz, WallAxis::V, WallKind::Wall));
    }
    // Puertas: la del perímetro se ubica según en qué lado cae su celda; las
    // interiores parten el tabique vertical.
    let mut doors: Vec<(i64, i64, WallAxis)> = Vec::new();
    let (dx, dz) = p.door;
    doors.push(perimeter_door_edge(p, min_x, min_z, dx, dz));
    for &(dx, dz) in p.interior_doors {
        doors.push((min_x + dx, min_z + dz, WallAxis::V));
    }
    for (tx, tz, axis) in doors {
        // La puerta REEMPLAZA la pared de ese borde (no se apilan).
        out.retain(|&(ex, ez, ea, _)| !(ex == tx && ez == tz && ea == axis));
        out.push((tx, tz, axis, WallKind::Door));
    }
    out
}

/// Borde del perímetro donde cae una celda de puerta `(dx, dz)` del prefab.
fn perimeter_door_edge(p: &Prefab, min_x: i64, min_z: i64, dx: i64, dz: i64) -> (i64, i64, WallAxis) {
    if dz == p.h - 1 {
        (min_x + dx, min_z + p.h, WallAxis::H) // sur
    } else if dz == 0 {
        (min_x + dx, min_z, WallAxis::H) // norte
    } else if dx == 0 {
        (min_x, min_z + dz, WallAxis::V) // oeste
    } else {
        (min_x + p.w, min_z + dz, WallAxis::V) // este
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topdown::PREFABS;

    #[test]
    fn edge_snapping_picks_the_nearest_side() {
        // Cerca del borde norte de la celda (3,7).
        assert_eq!(nearest_edge(3.5, 7.05), (3, 7, WallAxis::H));
        // Cerca del borde sur → el H de la fila siguiente.
        assert_eq!(nearest_edge(3.5, 7.95), (3, 8, WallAxis::H));
        // Cerca del oeste / este.
        assert_eq!(nearest_edge(3.02, 7.5), (3, 7, WallAxis::V));
        assert_eq!(nearest_edge(3.97, 7.5), (4, 7, WallAxis::V));
        // Funciona en coordenadas negativas (mapa infinito).
        assert_eq!(nearest_edge(-0.1, -0.5), (0, -1, WallAxis::V));
    }

    #[test]
    fn edge_cells_and_center_are_consistent() {
        assert_eq!(edge_cells(4, 9, WallAxis::H), [(4, 8), (4, 9)]);
        assert_eq!(edge_cells(4, 9, WallAxis::V), [(3, 9), (4, 9)]);
        let (cx, cz) = edge_center(4, 9, WallAxis::H);
        assert!((cx - 4.5).abs() < 1e-5 && (cz - 9.0).abs() < 1e-5);
        let (cx, cz) = edge_center(4, 9, WallAxis::V);
        assert!((cx - 4.0).abs() < 1e-5 && (cz - 9.5).abs() < 1e-5);
    }

    #[test]
    fn walls_need_a_floor_on_one_side() {
        let floor_at_5_5 = |cx: i64, cz: i64| (cx, cz) == (5, 5);
        // Borde norte de (5,5): apoya (la celda sur tiene piso).
        assert!(wall_supported(&floor_at_5_5, 5, 5, WallAxis::H));
        // Borde de una celda sin piso a ninguno de los dos lados: no apoya.
        assert!(!wall_supported(&floor_at_5_5, 20, 20, WallAxis::H));
        assert!(is_build_floor(Tile::WoodFloor) && !is_build_floor(Tile::Grass));
    }

    #[test]
    fn capsule_collides_with_thin_walls_only_where_they_are() {
        // Pared vertical en x = 10, celda z = 3.
        let wall = |tx: i64, tz: i64, a: WallAxis| (tx, tz, a) == (10, 3, WallAxis::V);
        assert!(wall_capsule_hits(10.0, 3.5, &wall), "encima de la pared");
        assert!(wall_capsule_hits(9.8, 3.5, &wall), "el radio la toca");
        assert!(!wall_capsule_hits(9.0, 3.5, &wall), "lejos: libre");
        assert!(!wall_capsule_hits(10.0, 5.5, &wall), "otra fila: libre");
        // Pared horizontal en z = 3, celda x = 10.
        let wall = |tx: i64, tz: i64, a: WallAxis| (tx, tz, a) == (10, 3, WallAxis::H);
        assert!(wall_capsule_hits(10.5, 3.0, &wall));
        assert!(!wall_capsule_hits(10.5, 4.5, &wall));
    }

    #[test]
    fn doors_do_not_block_but_walls_and_windows_do() {
        assert!(WallKind::Wall.blocks());
        assert!(WallKind::Window.blocks());
        assert!(!WallKind::Door.blocks());
    }

    #[test]
    fn prefabs_stamp_a_closed_ring_with_one_door() {
        for p in PREFABS {
            let edges = prefab_edges(p, 0, 0);
            let doors = edges.iter().filter(|e| e.3 == WallKind::Door).count();
            assert_eq!(doors, 1 + p.interior_doors.len(), "{}", p.id);
            // Perímetro completo (2·w horizontales + 2·h verticales) + tabiques.
            // La puerta del perímetro reemplaza a su pared, así que no suma.
            let perimeter = 2 * (p.w + p.h) as usize;
            assert_eq!(
                edges.len(),
                perimeter + p.interior_walls.len() + p.interior_doors.len(),
                "{} no cierra el anillo",
                p.id
            );
            // Ningún borde repetido (la puerta reemplaza a su pared).
            let mut seen = std::collections::BTreeSet::new();
            for &(tx, tz, a, _) in &edges {
                assert!(seen.insert((tx, tz, a.as_u8())), "{} borde duplicado", p.id);
            }
        }
    }

    #[test]
    fn edge_catalogue_is_sane() {
        assert!(edge_piece_count() >= 3);
        for i in 0..edge_piece_count() {
            let p = edge_piece(i).unwrap();
            assert!(!p.art.is_empty() && p.cost_qty > 0, "{}", p.id);
            assert_eq!(edge_piece_index(p.id), Some(i));
        }
        assert!(edge_piece(edge_piece_count()).is_none());
        assert!(edge_piece_index("no_existe").is_none());
    }
}
