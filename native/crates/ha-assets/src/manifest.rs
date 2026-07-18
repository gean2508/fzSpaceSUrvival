//! Spec del formato `manifest.ron` + structs Rust (serde). Ver
//! `ASSET_SYSTEM_ARCHITECTURE.md` §2 para el schema y la justificación de RON.
//!
//! Los mapas son `BTreeMap` a propósito: salida ordenada → RON determinista →
//! diffs mínimos (apto para review y para el flujo vibecodeo).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Bump = migración incompatible. El loader rechaza manifests con una versión
/// mayor a la que entiende (nunca adivina un formato futuro).
pub const CURRENT_MANIFEST_VERSION: u32 = 1;

/// Raíz del `manifest.ron`. La clave de `assets`/`sheets`/`brushes` **es** el ID
/// lógico estable que referencia el código (`"flora/arbol_roble"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Versión del formato (no del contenido). Ver [`CURRENT_MANIFEST_VERSION`].
    pub manifest_version: u32,
    /// Pixels-per-unit por defecto (16 = grid LimeZu). Cada asset puede override.
    pub ppu: u32,
    /// Fuentes en orden de prioridad para overlays de mods (F6).
    pub sources: Vec<Source>,
    /// Hojas de sprites reutilizables, por ID.
    pub sheets: BTreeMap<String, Sheet>,
    /// ID lógico -> definición de asset.
    pub assets: BTreeMap<String, Asset>,
    /// Composiciones multi-asset (cama, refugio, etc.), por ID.
    pub brushes: BTreeMap<String, Brush>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            manifest_version: CURRENT_MANIFEST_VERSION,
            ppu: 16,
            sources: Vec::new(),
            sheets: BTreeMap::new(),
            assets: BTreeMap::new(),
            brushes: BTreeMap::new(),
        }
    }
}

/// Una raíz de assets. `base` (priority baja) es LimeZu importado; `mods`
/// (priority alta) puede overridear por ID (F6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub root: String,
    pub priority: i32,
}

/// Una hoja de sprites (PNG) con su grilla.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    /// Ruta relativa a la `root` de la [`Source`] que la contiene.
    pub path: String,
    /// Tamaño de celda en píxeles: `(w, h)`.
    pub grid: (u32, u32),
    /// Columnas de la hoja (para convertir `Rect::Cells` -> índice de atlas).
    pub columns: u32,
}

/// Definición de un asset lógico. Reajustar `rect`/`pivot`/`hitbox` es editar
/// datos — el código Rust que lo referencia por ID queda intacto.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    /// Referencia a una clave de `sheets`.
    pub sheet: String,
    /// Tamaño lógico en celdas (multi-tile real: árbol = 3×4).
    pub footprint: Footprint,
    /// Región dentro de la hoja: `Cells` (legible) o `Px` (exacto).
    pub rect: Rect,
    /// Punto de anclaje (los "pies" del sprite para top-down).
    pub pivot: Pivot,
    /// Pixels-per-unit de este asset.
    pub ppu: u32,
    /// Fila que define la profundidad en el orden top-down 3/4.
    pub ysort: YSort,
    /// `None` = no colisiona.
    pub hitbox: Option<Hitbox>,
    /// `None` = estático.
    pub anim: Option<Anim>,
    pub flags: Flags,
    pub tags: Vec<String>,
}

/// Tamaño lógico en celdas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Footprint {
    pub w: u32,
    pub h: u32,
}

/// Región de la hoja. `Cells` se lee/edita fácil; `Px` es el escape para packs
/// que no respetan la grilla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rect {
    Cells { col: u32, row: u32, w: u32, h: u32 },
    Px { x: u32, y: u32, w: u32, h: u32 },
}

/// Punto de anclaje del sprite.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Pivot {
    Anchor(AnchorKind),
    /// Offset exacto en píxeles desde la esquina superior-izquierda del rect.
    Px { x: f32, y: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchorKind {
    BottomCenter,
    Center,
    TopLeft,
    TopCenter,
    BottomLeft,
}

/// Profundidad para el Y-sort top-down.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum YSort {
    /// Ordena por el borde inferior de esta fila del footprint (0-indexado).
    Row(u32),
    /// Offset manual en unidades de mundo.
    Offset(f32),
}

/// Caja de colisión, en celdas, relativa a la esquina inferior-izquierda del
/// footprint.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Hitbox {
    Aabb { x: f32, y: f32, w: f32, h: f32 },
}

/// Animación por frames dentro de la hoja.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Anim {
    pub frames: u32,
    pub fps: f32,
    pub layout: AnimLayout,
    pub loop_: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimLayout {
    /// Frames consecutivos hacia la derecha en la misma fila.
    RowRight,
    /// Frames consecutivos hacia abajo en la misma columna.
    ColDown,
    /// Frames en grilla (wrap por `columns`).
    Grid,
}

/// Flags de gameplay/render. Se extiende sin romper (serde ignora campos nuevos
/// con default en el futuro; por ahora todos explícitos).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Flags {
    pub solid: bool,
    pub harvestable: bool,
    pub billboard: bool,
}

/// Composición multi-asset: coloca varios assets en un patrón (cama, refugio).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Brush {
    /// Tamaño del brush en celdas: `(w, h)`.
    pub size: (u32, u32),
    /// `(dcol, drow, asset_id)` — offset en celdas + asset a colocar.
    pub cells: Vec<(u32, u32, String)>,
}

// ---------------------------------------------------------------------------
// (De)serialización RON + guard de versión
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum ManifestError {
    Parse(ron::error::SpannedError),
    Serialize(ron::Error),
    UnsupportedVersion { found: u32, max: u32 },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Parse(e) => write!(f, "manifest RON parse error: {e}"),
            ManifestError::Serialize(e) => write!(f, "manifest RON serialize error: {e}"),
            ManifestError::UnsupportedVersion { found, max } => write!(
                f,
                "manifest_version {found} no soportada (este binario entiende hasta {max})"
            ),
        }
    }
}

impl std::error::Error for ManifestError {}

impl Manifest {
    /// Parsea un `manifest.ron` y valida la versión. Rechaza versiones futuras.
    pub fn from_ron_str(s: &str) -> Result<Self, ManifestError> {
        let m: Manifest = ron::from_str(s).map_err(ManifestError::Parse)?;
        if m.manifest_version > CURRENT_MANIFEST_VERSION {
            return Err(ManifestError::UnsupportedVersion {
                found: m.manifest_version,
                max: CURRENT_MANIFEST_VERSION,
            });
        }
        Ok(m)
    }

    /// F6 — funde `overlay` encima (los mods GANAN): sheets/assets/brushes con
    /// la misma clave se reemplazan; los nuevos se agregan.
    pub fn overlay(&mut self, overlay: &Manifest) {
        for (k, v) in &overlay.sheets {
            self.sheets.insert(k.clone(), v.clone());
        }
        for (k, v) in &overlay.assets {
            self.assets.insert(k.clone(), v.clone());
        }
        for (k, v) in &overlay.brushes {
            self.brushes.insert(k.clone(), v.clone());
        }
    }

    /// Aplica una pila de overlays en orden de prioridad ASCENDENTE (el último
    /// gana). `base` es la fuente `base/`; los mods se pasan de menor a mayor.
    pub fn stacked(mut base: Manifest, overlays: impl IntoIterator<Item = Manifest>) -> Manifest {
        for o in overlays {
            base.overlay(&o);
        }
        base
    }

    /// Serializa a RON legible (con nombres de struct, como en la doc/ejemplos)
    /// y ordenado (BTreeMap) → diffs mínimos.
    pub fn to_ron_string(&self) -> Result<String, ManifestError> {
        let cfg = ron::ser::PrettyConfig::new()
            .struct_names(true)
            .indentor("    ".to_string());
        ron::ser::to_string_pretty(self, cfg).map_err(ManifestError::Serialize)
    }
}

// ---------------------------------------------------------------------------
// Tests (F1: round-trip + parse de ejemplo escrito a mano + guard de versión)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> Manifest {
        let mut sheets = BTreeMap::new();
        sheets.insert(
            "limezu/exteriors_01".to_string(),
            Sheet {
                path: "LimeZu/ModernExteriors/exteriors_01.png".to_string(),
                grid: (16, 16),
                columns: 64,
            },
        );
        sheets.insert(
            "limezu/interiors_04".to_string(),
            Sheet {
                path: "LimeZu/ModernInteriors/interiors_04.png".to_string(),
                grid: (16, 16),
                columns: 64,
            },
        );

        let mut assets = BTreeMap::new();
        assets.insert(
            "flora/arbol_roble".to_string(),
            Asset {
                sheet: "limezu/exteriors_01".to_string(),
                footprint: Footprint { w: 3, h: 4 },
                rect: Rect::Cells { col: 12, row: 8, w: 3, h: 4 },
                pivot: Pivot::Anchor(AnchorKind::BottomCenter),
                ppu: 16,
                ysort: YSort::Row(3),
                hitbox: Some(Hitbox::Aabb { x: 1.0, y: 0.0, w: 1.0, h: 0.5 }),
                anim: None,
                flags: Flags { solid: true, harvestable: true, billboard: false },
                tags: vec!["flora".into(), "overworld".into(), "wood".into()],
            },
        );
        assets.insert(
            "mueble/cama_doble".to_string(),
            Asset {
                sheet: "limezu/interiors_04".to_string(),
                footprint: Footprint { w: 2, h: 3 },
                rect: Rect::Cells { col: 4, row: 20, w: 2, h: 3 },
                pivot: Pivot::Anchor(AnchorKind::BottomCenter),
                ppu: 16,
                ysort: YSort::Row(3),
                hitbox: Some(Hitbox::Aabb { x: 0.0, y: 0.0, w: 2.0, h: 1.0 }),
                anim: None,
                flags: Flags { solid: true, harvestable: false, billboard: false },
                tags: vec!["mueble".into(), "interior".into()],
            },
        );
        assets.insert(
            "npc/colono_idle".to_string(),
            Asset {
                sheet: "limezu/exteriors_01".to_string(),
                footprint: Footprint { w: 1, h: 2 },
                rect: Rect::Cells { col: 0, row: 0, w: 1, h: 2 },
                pivot: Pivot::Anchor(AnchorKind::BottomCenter),
                ppu: 16,
                ysort: YSort::Row(2),
                hitbox: Some(Hitbox::Aabb { x: 0.0, y: 0.0, w: 1.0, h: 0.5 }),
                anim: Some(Anim { frames: 4, fps: 6.0, layout: AnimLayout::RowRight, loop_: true }),
                flags: Flags { solid: false, harvestable: false, billboard: false },
                tags: vec!["npc".into(), "character".into()],
            },
        );

        let mut brushes = BTreeMap::new();
        brushes.insert(
            "refugio_cama".to_string(),
            Brush { size: (2, 3), cells: vec![(0, 0, "mueble/cama_doble".to_string())] },
        );

        Manifest {
            manifest_version: 1,
            ppu: 16,
            sources: vec![
                Source { id: "base".into(), root: "assets/art/".into(), priority: 0 },
                Source { id: "mods".into(), root: "assets/mods/".into(), priority: 100 },
            ],
            sheets,
            assets,
            brushes,
        }
    }

    #[test]
    fn roundtrip_in_memory() {
        let original = sample_manifest();
        let ron = original.to_ron_string().expect("serialize");
        let parsed = Manifest::from_ron_str(&ron).expect("parse");
        assert_eq!(original, parsed, "round-trip debe preservar el manifest exacto");
    }

    #[test]
    fn parses_hand_written_example() {
        // Escrito a mano (estilo de la doc), sin pasar por el serializer.
        let src = r#"
Manifest(
    manifest_version: 1,
    ppu: 16,
    sources: [
        Source( id: "base", root: "assets/art/",  priority: 0 ),
    ],
    sheets: {
        "limezu/exteriors_01": Sheet(
            path: "LimeZu/ModernExteriors/exteriors_01.png",
            grid: (16, 16), columns: 64,
        ),
    },
    assets: {
        "flora/arbol_roble": Asset(
            sheet: "limezu/exteriors_01",
            footprint: Footprint( w: 3, h: 4 ),
            rect:  Cells( col: 12, row: 8, w: 3, h: 4 ),
            pivot: Anchor(BottomCenter),
            ppu: 16,
            ysort: Row(3),
            hitbox: Some(Aabb( x: 1.0, y: 0.0, w: 1.0, h: 0.5 )),
            anim: None,
            flags: Flags( solid: true, harvestable: true, billboard: false ),
            tags: [ "flora", "overworld", "wood" ],
        ),
    },
    brushes: {},
)
"#;
        let m = Manifest::from_ron_str(src).expect("el ejemplo de la doc debe parsear");
        assert_eq!(m.manifest_version, 1);
        assert_eq!(m.ppu, 16);
        let roble = m.assets.get("flora/arbol_roble").expect("arbol_roble existe");
        assert_eq!(roble.footprint, Footprint { w: 3, h: 4 });
        assert_eq!(roble.rect, Rect::Cells { col: 12, row: 8, w: 3, h: 4 });
        assert_eq!(roble.pivot, Pivot::Anchor(AnchorKind::BottomCenter));
        assert!(roble.flags.harvestable);
    }

    #[test]
    fn rejects_future_version() {
        let src = "Manifest( manifest_version: 999, ppu: 16, sources: [], sheets: {}, assets: {}, brushes: {} )";
        match Manifest::from_ron_str(src) {
            Err(ManifestError::UnsupportedVersion { found: 999, max }) => {
                assert_eq!(max, CURRENT_MANIFEST_VERSION);
            }
            other => panic!("esperaba UnsupportedVersion, obtuve {other:?}"),
        }
    }
}
