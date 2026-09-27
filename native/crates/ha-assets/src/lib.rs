//! `ha-assets` — modelo de datos del arte del juego.
//!
//! El código sólo conoce **IDs lógicos** (`"decor/tree_roble"`,
//! `"floor/piso#madera"`); los píxeles, rects, poses, huellas y capas viven en
//! `crates/ha-game/assets/art/manifest.json`, que escribe el usuario desde
//! HA Studio (`./editor_assets`). Este crate es **headless** (sin Bevy) para que
//! la CLI, el editor y los tests lo usen sin arrastrar el motor.
//!
//! - `art_manifest` — el formato del manifest (fuente de verdad del arte).
//! - `slots` — catálogo de qué IDs existen y cómo se llaman en castellano.
//! - `pngs` — listado de hojas para el selector del editor.
//! - `chargen_index` — capas del generador de personaje (creador + retratos).
//!
//! Ver `biblia.md` §8.

pub mod art_manifest;
pub mod chargen_index;
pub mod pngs;
pub mod slots;

pub use art_manifest::{
    Interaccion, Requisito, ESTADOS, VERBOS,
    namespace_layer, placement_center_tiles, placement_min_tile, split_variant, variant_key,
    ArtAnim, ArtEntry, ArtLayer, ArtManifest, ArtValue, CellRect, CollisionFootprint, Emission,
    ObjectDef, Pose, SliceBorder, StripDir, VariantDef, VARIANT_SEP,
};
pub use chargen_index::{ChargenIndex, LayerOption, LookDef};
pub use pngs::list_pngs;
pub use slots::{category_label, game_slots, interior_styles, EstiloInterior, SlotDef};
