//! Tipos de datos del Editor de Mundo (`world_edits.json`), COMPARTIDOS entre el
//! juego (que los aplica sobre la generación en boot) y el editor `mundo` (que los
//! escribe). Puros datos serde, sin Bevy → el editor los usa sin depender de
//! ha-game. Ver `ha-game::world_edits` (loader) y `ha-asset-tools::world_gui`.

use serde::{Deserialize, Serialize};

use crate::topdown::Tile;

/// Override de tile en una celda concreta (cuerpo + capa z + x,z).
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct TileEdit {
    pub body: u8,
    pub z: i32,
    pub x: i64,
    pub zc: i64,
    pub tile: Tile,
}

/// Un asset grande colocado (A2) — también lo autora el editor de mundo.
/// `(cx,cz)` = esquina MIN de la base `w×h`. `solid` = celdas reales de colisión
/// (huella pintada); vacío = base completa.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct PlacedStructure {
    pub body: u8,
    pub z: i32,
    pub cx: i64,
    pub cz: i64,
    pub w: i64,
    pub h: i64,
    pub art: String,
    #[serde(default)]
    pub solid: Vec<(u32, u32)>,
}

/// Un decor autorado (árbol/roca/flora/prop) colocado a mano en una celda desde el editor.
/// `key` = ID lógico de arte (`decor/*`, `nature/*`, `prop/*`). El juego lo pinta al
/// streamear el chunk; el editor lo dibuja. Complementa el decor PROCEDURAL.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct PlacedDecor {
    pub body: u8,
    pub z: i32,
    pub x: i64,
    pub zc: i64,
    pub key: String,
    /// Tamaño en METROS (1 tile = 1 m). El editor lo deduce del recorte del asset
    /// (una casa de 5×4 celdas mide 5×4) y te deja escalarlo. `None` = el tamaño
    /// de la tabla de especies, o 1.5×1.5 — que era lo único que había y hacía
    /// que una casa se viera como un arbusto.
    #[serde(default)]
    pub size: Option<[f32; 2]>,
}

/// Una celda "limpia": el decor PROCEDURAL (árboles/rocas/flora/props) no se
/// genera ahí. Es lo que escribe la goma de decor del editor cuando querés sacar
/// un árbol SIN cambiar el terreno. Pintar un tile ya limpia su celda sin
/// necesidad de esto (ver `WorldEdits::blocks_procedural_decor`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClearedCell {
    pub body: u8,
    pub z: i32,
    pub x: i64,
    pub zc: i64,
}

/// La capa autorada completa. Campos `#[serde(default)]` → compatible hacia
/// adelante (agregar campos no rompe archivos viejos).
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct WorldEdits {
    #[serde(default)]
    pub tiles: Vec<TileEdit>,
    #[serde(default)]
    pub structures: Vec<PlacedStructure>,
    #[serde(default)]
    pub decor: Vec<PlacedDecor>,
    /// Celdas donde el decor procedural queda suprimido sin tocar el terreno.
    #[serde(default)]
    pub cleared: Vec<ClearedCell>,
}

impl WorldEdits {
    /// Celdas donde el juego NO debe generar decor procedural: **toda celda con
    /// terreno autorado** (lo que pintás en el editor reemplaza al mundo — si
    /// ponés un rectángulo de pasto, el bosque no vuelve a crecer encima) más
    /// las borradas a mano con la goma de decor.
    pub fn decor_free_cells(&self) -> impl Iterator<Item = (u8, i32, i64, i64)> + '_ {
        let painted = self.tiles.iter().map(|t| (t.body, t.z, t.x, t.zc));
        let erased = self.cleared.iter().map(|c| (c.body, c.z, c.x, c.zc));
        painted.chain(erased)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_edits_roundtrip() {
        let w = WorldEdits {
            tiles: vec![TileEdit { body: 0, z: 0, x: 5, zc: -3, tile: Tile::Water }],
            structures: vec![PlacedStructure {
                body: 0,
                z: 0,
                cx: 10,
                cz: 10,
                w: 4,
                h: 4,
                art: "structure/monument".into(),
                solid: vec![(1, 1), (2, 1)],
            }],
            decor: vec![PlacedDecor {
                body: 0,
                z: 0,
                x: 3,
                zc: 4,
                key: "decor/tree_roble".into(),
                size: Some([3.0, 3.2]),
            }],
            cleared: vec![ClearedCell { body: 0, z: 0, x: 9, zc: 9 }],
        };
        let j = serde_json::to_string(&w).unwrap();
        let back: WorldEdits = serde_json::from_str(&j).unwrap();
        assert_eq!(back.tiles.len(), 1);
        assert_eq!(back.tiles[0].tile, Tile::Water);
        assert_eq!(back.structures[0].solid.len(), 2);
        // Compat: archivo vacío / campos ausentes → default vacío.
        let empty: WorldEdits = serde_json::from_str("{}").unwrap();
        assert!(empty.tiles.is_empty() && empty.structures.is_empty());
    }

    #[test]
    fn painted_terrain_frees_its_cell_from_procedural_decor() {
        // Lo que el usuario pinta MANDA: la celda con tile autorado deja de
        // generar árboles/rocas (si no, pintás pasto y el bosque sigue ahí).
        let w = WorldEdits {
            tiles: vec![TileEdit { body: 0, z: 0, x: 5, zc: -3, tile: Tile::Grass }],
            cleared: vec![ClearedCell { body: 0, z: 0, x: 40, zc: 40 }],
            ..Default::default()
        };
        let free: Vec<_> = w.decor_free_cells().collect();
        assert!(free.contains(&(0, 0, 5, -3)), "el tile pintado limpia su celda");
        assert!(free.contains(&(0, 0, 40, 40)), "la goma de decor tambien");
        assert_eq!(free.len(), 2);
        // Compat: un archivo sin `cleared` sigue cargando.
        let old = r#"{"tiles":[],"structures":[],"decor":[]}"#;
        assert!(serde_json::from_str::<WorldEdits>(old).unwrap().cleared.is_empty());
    }
}
