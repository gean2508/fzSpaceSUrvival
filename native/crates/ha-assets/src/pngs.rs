//! Listado de PNGs de un árbol de assets — lo consume el selector de hojas de
//! HA Studio (`ha-asset-tools`).
//!
//! Antes esto era `scan.rs`, que además barría un pack LimeZu y escupía un
//! `manifest.ron` esqueleto. Ese sistema quedó muerto (0 assets, nunca se usó:
//! el pixel se cura a mano en el editor) y se borró junto con `manifest.rs`,
//! `catalog.rs` y `registry.rs`. Ver biblia.md §8.

use std::path::{Path, PathBuf};

/// Lista los PNG bajo `root` (recursivo, ordenado), sin decodificarlos.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_pngs_recursively_skipping_noise() {
        let dir = std::env::temp_dir().join(format!("ha_pngs_{}", std::process::id()));
        let sub = dir.join("Beach");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("ball.png"), b"x").unwrap();
        std::fs::write(sub.join("Palette_stuff.png"), b"x").unwrap();
        std::fs::write(sub.join("notes.txt"), b"x").unwrap();

        let found = list_pngs(&dir);
        assert_eq!(found.len(), 1, "sólo el PNG que no es ruido");
        assert!(found[0].ends_with("ball.png"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
