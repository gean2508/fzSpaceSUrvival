//! F2 — escaneo de un árbol LimeZu → `manifest.ron` esqueleto.
//!
//! Modo **singles** (por defecto, y el único robusto): cada PNG es UN sprite ya
//! recortado, así que emite un asset por archivo (rect = imagen completa,
//! footprint = dims / grid). Es la puerta única del píxel: a partir de acá la IA
//! sólo referencia el ID lógico generado. Determinista (orden estable) → un
//! re-scan produce diffs mínimos y **nunca pisa IDs ya curados**.
//!
//! Sólo se compila con la feature `scan` (arrastra el decoder `image`).

use std::path::{Path, PathBuf};

use crate::manifest::{
    Anim, Asset, Flags, Footprint, Hitbox, Manifest, Pivot, Rect, Sheet, YSort, AnchorKind,
};

/// Opciones del escaneo.
pub struct ScanOptions {
    /// Tamaño de celda en píxeles. El juego es 32×32 (packs LimeZu 32x32).
    pub grid: (u32, u32),
    /// Prefijo de categoría para los IDs generados (`"exteriors"`, `"muebles"`…).
    pub id_prefix: String,
    /// Corta el escaneo tras N archivos (útil para probar sin volcar 6k+ assets).
    pub max_files: Option<usize>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self { grid: (32, 32), id_prefix: "art".to_string(), max_files: None }
    }
}

/// Resumen de un escaneo — lo imprime la CLI.
#[derive(Debug, Default, Clone, Copy)]
pub struct ScanReport {
    /// PNGs con dimensiones leídas OK.
    pub scanned: usize,
    /// Assets nuevos añadidos al manifest.
    pub added: usize,
    /// Archivos saltados (ya curados, o dims ilegibles).
    pub skipped: usize,
}

/// Barre `root` y funde los assets nuevos sobre `existing` (o un manifest
/// vacío). Los IDs y sheets ya presentes se respetan: sólo se añaden PNGs que
/// aún no tienen una entrada de sheet.
#[cfg(feature = "scan")]
pub fn scan_limezu_directory(
    root: &Path,
    existing: Option<&Manifest>,
    opts: &ScanOptions,
) -> (Manifest, ScanReport) {
    let mut manifest = existing.cloned().unwrap_or_default();
    let mut report = ScanReport::default();

    let mut pngs = collect_pngs(root);
    pngs.sort(); // orden estable = diffs mínimos

    // Sheets ya presentes → no volver a añadir su PNG.
    let known_sheets: std::collections::BTreeSet<String> =
        manifest.sheets.values().map(|s| s.path.clone()).collect();

    for path in pngs {
        if let Some(max) = opts.max_files {
            if report.scanned >= max {
                break;
            }
        }
        let Ok((w, h)) = image::image_dimensions(&path) else {
            report.skipped += 1;
            continue;
        };
        report.scanned += 1;

        let rel = rel_path(root, &path);
        if known_sheets.contains(&rel) {
            report.skipped += 1; // ya curado
            continue;
        }

        let (gw, gh) = opts.grid;
        let fw = w.div_ceil(gw).max(1);
        let fh = h.div_ceil(gh).max(1);
        let sheet_id = format!("sheet/{rel}");
        let asset_id = make_id(&opts.id_prefix, &path, opts.grid);

        // Dos archivos pueden sanear al mismo ID lógico → el primero gana, el
        // resto se cuenta como saltado (no como añadido).
        if manifest.assets.contains_key(&asset_id) {
            report.skipped += 1;
            continue;
        }
        manifest
            .sheets
            .entry(sheet_id.clone())
            .or_insert(Sheet { path: rel.clone(), grid: opts.grid, columns: fw });
        manifest.assets.insert(
            asset_id,
            Asset {
                sheet: sheet_id,
                footprint: Footprint { w: fw, h: fh },
                rect: Rect::Px { x: 0, y: 0, w, h },
                pivot: Pivot::Anchor(AnchorKind::BottomCenter),
                ppu: gw,
                ysort: YSort::Row(fh.saturating_sub(1)),
                hitbox: None::<Hitbox>,
                anim: None::<Anim>,
                flags: Flags::default(),
                tags: derive_tags(&opts.id_prefix, &path),
            },
        );
        report.added += 1;
    }

    (manifest, report)
}

/// Lista los PNG bajo `root` (recursivo, ordenado), sin decodificarlos — para
/// el selector de PNGs del editor.
pub fn list_pngs(root: &Path) -> Vec<PathBuf> {
    let mut v = collect_pngs(root);
    v.sort();
    v
}

/// Recolecta `.png` recursivamente, saltando ruido conocido (paletas, viejos,
/// generadores de personajes/retratos que no son props de juego).
fn collect_pngs(root: &Path) -> Vec<PathBuf> {
    fn is_noise(p: &Path) -> bool {
        let s = p.to_string_lossy().to_lowercase();
        s.contains("palette")
            || s.contains("/old")
            || s.contains("_generator")
            || s.contains("desktop.ini")
    }
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|e| e.to_str()) == Some("png") && !is_noise(&p) {
                out.push(p);
            }
        }
    }
    out
}

fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// `<prefix>/<nombre_saneado>` — quita tokens de tamaño (`16x16`), pasa a
/// snake_case ascii, colapsa separadores. Determinista para el mismo archivo.
fn make_id(prefix: &str, path: &Path, grid: (u32, u32)) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
    let size_tok = format!("{}x{}", grid.0, grid.1);
    let cleaned: Vec<String> = stem
        .split(|c: char| c == '_' || c == '-' || c == ' ')
        .filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case(&size_tok) && !is_size_token(t))
        .map(sanitize_token)
        .filter(|t| !t.is_empty())
        .collect();
    let name = if cleaned.is_empty() { "asset".to_string() } else { cleaned.join("_") };
    format!("{}/{name}", prefix.trim_matches('/'))
}

/// `48x48`, `32x32`, etc.
fn is_size_token(t: &str) -> bool {
    let lower = t.to_lowercase();
    if let Some((a, b)) = lower.split_once('x') {
        return !a.is_empty() && a.chars().all(|c| c.is_ascii_digit())
            && !b.is_empty() && b.chars().all(|c| c.is_ascii_digit());
    }
    false
}

fn sanitize_token(t: &str) -> String {
    t.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

/// Tags derivados: el prefijo + los directorios padre saneados (contexto para
/// que el humano/egui filtre en F3).
fn derive_tags(prefix: &str, path: &Path) -> Vec<String> {
    let mut tags = vec![prefix.trim_matches('/').to_string()];
    if let Some(parent) = path.parent().and_then(|p| p.file_name()) {
        let t = sanitize_token(&parent.to_string_lossy());
        if !t.is_empty() && !tags.contains(&t) {
            tags.push(t);
        }
    }
    tags
}

#[cfg(all(test, feature = "scan"))]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    fn write_png(path: &Path, w: u32, h: u32) {
        let img = ImageBuffer::from_pixel(w, h, Rgba([255u8, 0, 0, 255]));
        img.save(path).unwrap();
    }

    #[test]
    fn scans_singles_into_skeleton() {
        let dir = std::env::temp_dir().join(format!("ha_scan_{}", std::process::id()));
        let sub = dir.join("Beach");
        std::fs::create_dir_all(&sub).unwrap();
        write_png(&sub.join("Beach_16x16_Ball.png"), 16, 16); // 1×1
        write_png(&sub.join("Condo_16x16_Roof.png"), 48, 112); // 3×7
        std::fs::write(sub.join("desktop.ini"), b"noise").unwrap(); // ignorado

        // Grid fijado en 16 para este caso (dims múltiplos de 16) — testea la
        // mecánica footprint = dims / grid con independencia del default (32).
        let opts = ScanOptions { grid: (16, 16), id_prefix: "exteriors".into(), max_files: None };
        let (m, report) = scan_limezu_directory(&dir, None, &opts);

        assert_eq!(report.scanned, 2, "dos PNGs válidos");
        assert_eq!(report.added, 2);
        assert_eq!(m.assets.len(), 2);

        let ball = m.assets.get("exteriors/beach_ball").expect("id saneado sin token de tamaño");
        assert_eq!(ball.footprint, Footprint { w: 1, h: 1 });
        assert_eq!(ball.rect, Rect::Px { x: 0, y: 0, w: 16, h: 16 });
        assert_eq!(ball.pivot, Pivot::Anchor(AnchorKind::BottomCenter));

        let roof = m.assets.get("exteriors/condo_roof").expect("segundo asset");
        assert_eq!(roof.footprint, Footprint { w: 3, h: 7 });
        assert_eq!(roof.ysort, YSort::Row(6));

        // Re-scan idempotente: no añade nada nuevo.
        let (m2, report2) = scan_limezu_directory(&dir, Some(&m), &opts);
        assert_eq!(report2.added, 0, "re-scan no re-añade");
        assert_eq!(m2.assets.len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn size_token_detection() {
        assert!(is_size_token("16x16"));
        assert!(is_size_token("48X48"));
        assert!(!is_size_token("Ball"));
        assert!(!is_size_token("3x")); // incompleto
    }
}
