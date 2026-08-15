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
    /// **Hacia dónde MIRA** el objeto, en cuartos de vuelta: 0 = sur (de frente a
    /// cámara, el reposo), 1 = oeste, 2 = norte, 3 = este.
    ///
    /// No se rota el sprite: en pixel art girar los píxeles 90° deja una cama
    /// tumbada de costado. La orientación elige una POSE del slot
    /// (`sur`/`oeste`/`norte`/`este`), que el usuario recorta en el editor como ya
    /// recorta las de las paredes; sin esas poses el objeto se dibuja como
    /// siempre y sólo cambia su huella. `#[serde(default)]` = las partidas viejas
    /// entran como orientación 0.
    #[serde(default)]
    pub rot: u8,
}

/// Nombre de pose de cada cuarto de vuelta. En castellano, como el resto de lo
/// que el usuario ve en el editor.
pub const FACING_POSES: [&str; 4] = ["sur", "oeste", "norte", "este"];

/// Huella `(w, h)` de un objeto `w×h` orientado: los cuartos impares la giran.
pub fn footprint_rotated(w: i64, h: i64, rot: u8) -> (i64, i64) {
    if rot % 2 == 1 {
        (h, w)
    } else {
        (w, h)
    }
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
    /// **El suelo que el asset ESTAMPA bajo su huella.**
    ///
    /// Antes un asset colocado se limitaba a dibujarse ENCIMA: plantabas una casa
    /// sobre el bosque y abajo seguía habiendo pasto y árboles asomando por los
    /// bordes. Con esto la colocación reemplaza el terreno de todas las celdas que
    /// ocupa — y como la huella sale del recorte del slot, **el solar se adapta
    /// solo al tamaño del asset**: una universidad estampa su solar entero y una
    /// maceta no toca nada.
    ///
    /// `None` = no toca el terreno (comportamiento histórico, para props sueltos).
    #[serde(default)]
    pub ground: Option<Tile>,
    /// Huella del asset en TILES, tal como la vio el editor al colocarlo
    /// (`ArtManifest::slot_cells`). Se guarda en vez de re-deducirla en el juego
    /// para que recortar el sprite distinto MÁS TARDE no mueva solares ya
    /// autorados. `None` = deducir de `size`.
    #[serde(default)]
    pub cells: Option<[u32; 2]>,
    /// Celda ancla dentro de la huella (`ArtManifest::anchor`). Misma razón.
    #[serde(default)]
    pub anchor: Option<[u32; 2]>,
}

impl PlacedDecor {
    /// Huella en tiles: la guardada por el editor, o la deducida de `size`
    /// redondeado (1 tile = 1 m). Nunca menor que 1×1.
    pub fn footprint_cells(&self) -> (u32, u32) {
        if let Some([w, h]) = self.cells {
            return (w.max(1), h.max(1));
        }
        let s = self.size.unwrap_or([1.0, 1.0]);
        (s[0].round().max(1.0) as u32, s[1].round().max(1.0) as u32)
    }

    /// Celda ancla dentro de la huella: la guardada, o la de por defecto (columna
    /// del medio, fila del pie) — la misma regla que `CollisionFootprint::default_anchor`,
    /// replicada acá porque `ha-shared` no depende de `ha-assets`.
    pub fn footprint_anchor(&self) -> (u32, u32) {
        let (w, h) = self.footprint_cells();
        match self.anchor {
            Some([ax, az]) => (ax.min(w - 1), az.min(h - 1)),
            None => ((w - 1) / 2, h - 1),
        }
    }

    /// Celdas `(x, z)` del mundo que el asset ocupa. Es la huella alineada al
    /// grid: `[x-ax, x-ax+w) × [zc-az, zc-az+h)`, la MISMA regla que
    /// `ha_assets::placement_min_tile` usa para dibujar y colisionar.
    pub fn covered_cells(&self) -> impl Iterator<Item = (i64, i64)> + '_ {
        let (w, h) = self.footprint_cells();
        let (ax, az) = self.footprint_anchor();
        let (min_x, min_z) = (self.x - ax as i64, self.zc - az as i64);
        (0..h as i64).flat_map(move |dz| (0..w as i64).map(move |dx| (min_x + dx, min_z + dz)))
    }
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
    /// **Las ZONAS que pintaste** (ver [`crate::zonas`]). Van acá y no en un
    /// archivo aparte porque son lo mismo que el resto: contenido autorado que el
    /// juego aplica sobre la generación. Un solo archivo, un solo guardado.
    #[serde(default)]
    pub zonas: Vec<crate::zonas::Zona>,
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
        // Un asset que ESTAMPA suelo (una casa, una universidad) también reclama
        // sus celdas: si no, el bosque procedural seguiría creciendo por dentro y
        // asomando entre las paredes. Los props que no tocan el terreno
        // (`ground: None`) no reclaman nada — una maceta no despeja un bosque.
        let stamped = self
            .decor
            .iter()
            .filter(|d| d.ground.is_some())
            .flat_map(|d| d.covered_cells().map(move |(x, z)| (d.body, d.z, x, z)));
        painted.chain(erased).chain(stamped)
    }

    /// Tiles que las colocaciones de assets ESTAMPAN en el terreno. El juego los
    /// aplica igual que los `tiles` pintados a mano — misma capa, mismo
    /// `author_tile` —, así que un solar autorado y uno pintado son la misma cosa
    /// para todo lo que viene después (colisión, decor, persistencia).
    pub fn stamped_ground(&self) -> impl Iterator<Item = TileEdit> + '_ {
        self.decor.iter().filter_map(|d| d.ground.map(|t| (d, t))).flat_map(|(d, tile)| {
            d.covered_cells().map(move |(x, zc)| TileEdit { body: d.body, z: d.z, x, zc, tile })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_edits_roundtrip() {
        let w = WorldEdits {
            tiles: vec![TileEdit { body: 0, z: 0, x: 5, zc: -3, tile: Tile::Water }],
            zonas: vec![crate::zonas::Zona {
                nombre: "Barrio Norte".into(),
                body: 0,
                min: (100, -40),
                size: (80, 60),
                clase: crate::zonas::ClaseZona::Residencial,
            }],
            structures: vec![PlacedStructure {
                body: 0,
                z: 0,
                cx: 10,
                cz: 10,
                w: 4,
                h: 4,
                art: "structure/monument".into(),
                solid: vec![(1, 1), (2, 1)],
                rot: 3,
            }],
            decor: vec![PlacedDecor {
                body: 0,
                z: 0,
                x: 3,
                zc: 4,
                key: "decor/tree_roble".into(),
                size: Some([3.0, 3.2]),
                ground: None,
                cells: None,
                anchor: None,
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

    /// El asset colocado ESTAMPA su solar: el suelo se adapta al tamaño del
    /// asset, no al revés. Una casa de 6×4 tapa 24 celdas de terreno, ni una más.
    #[test]
    fn a_placed_asset_stamps_the_ground_under_its_whole_footprint() {
        let uni = PlacedDecor {
            body: 0,
            z: 0,
            x: 100,
            zc: 200,
            key: "building/universidad".into(),
            size: Some([6.0, 4.0]),
            ground: Some(Tile::Stone),
            cells: Some([6, 4]),
            anchor: None, // → default: columna del medio (2), fila del pie (3)
        };
        let cells: Vec<_> = uni.covered_cells().collect();
        assert_eq!(cells.len(), 24, "la huella tiene que ser 6x4 celdas");
        // Ancla en (100,200) con anchor (2,3) ⇒ esquina min en (98, 197).
        assert!(cells.contains(&(98, 197)), "esquina min");
        assert!(cells.contains(&(103, 200)), "esquina max");
        assert!(!cells.contains(&(104, 200)), "no se pasa de la huella");

        let w = WorldEdits { decor: vec![uni], ..Default::default() };
        let stamped: Vec<_> = w.stamped_ground().collect();
        assert_eq!(stamped.len(), 24);
        assert!(stamped.iter().all(|t| t.tile == Tile::Stone));
        // Y esas 24 celdas dejan de generar decor procedural: nada de árboles
        // creciendo dentro de la universidad.
        assert_eq!(w.decor_free_cells().count(), 24);
    }

    /// Un prop suelto (`ground: None`) NO toca el terreno ni despeja el bosque —
    /// es el comportamiento histórico y hay que conservarlo.
    #[test]
    fn a_prop_without_ground_leaves_the_world_alone() {
        let w = WorldEdits {
            decor: vec![PlacedDecor {
                body: 0,
                z: 0,
                x: 3,
                zc: 4,
                key: "nature/flores_rojas".into(),
                size: Some([1.0, 1.0]),
                ground: None,
                cells: None,
                anchor: None,
            }],
            ..Default::default()
        };
        assert_eq!(w.stamped_ground().count(), 0);
        assert_eq!(w.decor_free_cells().count(), 0);
    }
}
