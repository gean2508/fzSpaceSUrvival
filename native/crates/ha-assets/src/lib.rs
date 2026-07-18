//! `ha-assets` — sistema de assets data-driven de Starship.
//!
//! Fuente de verdad = `manifest.ron`. El código sólo conoce IDs lógicos
//! (`"flora/arbol_roble"`); píxeles, rects y pivotes viven en datos. Este crate
//! es **headless** (sin Bevy) para que la CLI, el server y los tests lo usen.
//!
//! F1 (este archivo): spec del formato + structs serde + round-trip.
//! F2+: `scan`, `catalog`, `registry` se apoyan en estos tipos.

pub mod art_manifest;
pub mod catalog;
pub mod manifest;
pub mod registry;
pub mod slots;

#[cfg(feature = "scan")]
pub mod scan;

pub use art_manifest::{ArtAnim, ArtEntry, ArtManifest, ArtValue, CellRect, Pose};
pub use catalog::generate_catalog;
pub use manifest::{
    AnchorKind, Anim, AnimLayout, Asset, Brush, Flags, Footprint, Hitbox, Manifest, ManifestError,
    Pivot, Rect, Sheet, Source, YSort, CURRENT_MANIFEST_VERSION,
};
pub use registry::{AssetRegistry, PxRect, ValidationIssue};
pub use slots::{game_slots, SlotDef};

#[cfg(feature = "scan")]
pub use scan::{list_pngs, scan_limezu_directory, ScanOptions, ScanReport};
