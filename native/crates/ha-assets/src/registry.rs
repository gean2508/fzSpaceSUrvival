//! F5 (núcleo headless) — `AssetRegistry`: lo que el juego carga en boot desde
//! `manifest.ron` para resolver un ID lógico a su rect en píxeles + metadatos.
//!
//! La cola Bevy (componente `AssetSlot`, `validate_and_bind_assets_system` con
//! placeholder magenta) vive en `ha-game` y se apoya en esto. Acá está toda la
//! lógica pura y testeable: lookup, resolución de rect y validación.

use crate::manifest::{Asset, Manifest, Rect, Sheet};

/// Rect resuelto en píxeles dentro de la hoja.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PxRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Un problema encontrado al validar el manifest (para loguear en boot; el juego
/// muestra placeholder magenta en vez de crashear).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationIssue {
    /// Un asset referencia una `sheet` que no existe.
    MissingSheet { asset: String, sheet: String },
}

/// Índice runtime del catálogo de assets.
pub struct AssetRegistry {
    manifest: Manifest,
}

impl AssetRegistry {
    pub fn new(manifest: Manifest) -> Self {
        Self { manifest }
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn len(&self) -> usize {
        self.manifest.assets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.manifest.assets.is_empty()
    }

    /// Definición por ID lógico, o `None` si no existe (el llamador usa placeholder).
    pub fn get(&self, id: &str) -> Option<&Asset> {
        self.manifest.assets.get(id)
    }

    /// Rect en píxeles del asset dentro de su hoja. `Cells` se resuelve con la
    /// grilla de la sheet; `Px` pasa directo. `None` si falta la sheet.
    pub fn resolve_px_rect(&self, id: &str) -> Option<PxRect> {
        let asset = self.get(id)?;
        let sheet = self.manifest.sheets.get(&asset.sheet)?;
        Some(px_rect(&asset.rect, sheet))
    }

    /// Recorre el manifest y devuelve los problemas (sheets faltantes). Vacío = OK.
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        for (id, asset) in &self.manifest.assets {
            if !self.manifest.sheets.contains_key(&asset.sheet) {
                issues.push(ValidationIssue::MissingSheet {
                    asset: id.clone(),
                    sheet: asset.sheet.clone(),
                });
            }
        }
        issues
    }
}

fn px_rect(rect: &Rect, sheet: &Sheet) -> PxRect {
    match *rect {
        Rect::Px { x, y, w, h } => PxRect { x, y, w, h },
        Rect::Cells { col, row, w, h } => {
            let (gw, gh) = sheet.grid;
            PxRect { x: col * gw, y: row * gh, w: w * gw, h: h * gh }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::*;
    use std::collections::BTreeMap;

    fn manifest_with(rect: Rect, sheet_present: bool) -> Manifest {
        let mut sheets = BTreeMap::new();
        if sheet_present {
            sheets.insert(
                "s".to_string(),
                Sheet { path: "s.png".into(), grid: (16, 16), columns: 8 },
            );
        }
        let mut assets = BTreeMap::new();
        assets.insert(
            "cat/thing".to_string(),
            Asset {
                sheet: "s".into(),
                footprint: Footprint { w: 2, h: 3 },
                rect,
                pivot: Pivot::Anchor(AnchorKind::BottomCenter),
                ppu: 16,
                ysort: YSort::Row(2),
                hitbox: None,
                anim: None,
                flags: Flags::default(),
                tags: vec![],
            },
        );
        Manifest { assets, sheets, ..Default::default() }
    }

    #[test]
    fn resolves_cells_via_sheet_grid() {
        let reg = AssetRegistry::new(manifest_with(
            Rect::Cells { col: 4, row: 20, w: 2, h: 3 },
            true,
        ));
        assert_eq!(
            reg.resolve_px_rect("cat/thing"),
            Some(PxRect { x: 64, y: 320, w: 32, h: 48 })
        );
        assert!(reg.validate().is_empty());
    }

    #[test]
    fn resolves_px_passthrough() {
        let reg = AssetRegistry::new(manifest_with(Rect::Px { x: 0, y: 0, w: 48, h: 112 }, true));
        assert_eq!(
            reg.resolve_px_rect("cat/thing"),
            Some(PxRect { x: 0, y: 0, w: 48, h: 112 })
        );
    }

    #[test]
    fn missing_sheet_is_flagged_not_panicked() {
        let reg = AssetRegistry::new(manifest_with(Rect::Px { x: 0, y: 0, w: 16, h: 16 }, false));
        assert_eq!(reg.resolve_px_rect("cat/thing"), None);
        assert_eq!(
            reg.validate(),
            vec![ValidationIssue::MissingSheet {
                asset: "cat/thing".into(),
                sheet: "s".into()
            }]
        );
        assert!(reg.get("no/existe").is_none());
    }

    #[test]
    fn overlay_mods_win() {
        let base = manifest_with(Rect::Px { x: 0, y: 0, w: 16, h: 16 }, true);
        let mut over = manifest_with(Rect::Px { x: 0, y: 0, w: 16, h: 16 }, true);
        // El mod cambia el footprint del mismo ID y agrega uno nuevo.
        over.assets.get_mut("cat/thing").unwrap().footprint = Footprint { w: 9, h: 9 };
        over.assets.insert(
            "mod/nuevo".to_string(),
            over.assets.get("cat/thing").unwrap().clone(),
        );
        let merged = Manifest::stacked(base, [over]);
        assert_eq!(merged.assets.get("cat/thing").unwrap().footprint, Footprint { w: 9, h: 9 });
        assert!(merged.assets.contains_key("mod/nuevo"));
    }
}
