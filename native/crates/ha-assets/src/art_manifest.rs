//! Modelo del **art-manifest del juego** (`native/crates/ha-game/assets/art/
//! manifest.json`): el mapeo clave-lógica → PNG (+ animación) que el juego lee
//! en boot para reskinearse. El editor escribe esto; `ha-game::art2d` lo lee.
//!
//! Compatible hacia atrás: el formato viejo era `{"key": "ruta.png"}` (string).
//! El nuevo permite `{"key": {"png": "...", "anim": {"frames": N, "fps": F}}}`.
//! Se leen los dos; el editor siempre escribe el nuevo.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Animación de un sprite tira-de-frames (single horizontal): N frames de igual
/// ancho reproducidos a `fps`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ArtAnim {
    pub frames: u32,
    pub fps: f32,
}

/// Rectángulo en **celdas** dentro de una hoja (no píxeles): el editor lo elige
/// arrastrando en la grilla; el juego lo resuelve a px con `grid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellRect {
    pub col: u32,
    pub row: u32,
    pub w: u32,
    pub h: u32,
}

impl CellRect {
    /// Cantidad de celdas del rango (usado como frames de una animación fila).
    pub fn cells(&self) -> u32 {
        self.w.max(1) * self.h.max(1)
    }
}

/// Una pose direccional/nombrada de un personaje (idle_down, run_left, sit…):
/// un recorte estático, o una tira animada si trae `anim`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub rect: CellRect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim: Option<ArtAnim>,
}

/// Entrada completa (formato nuevo). Todos los campos nuevos son opcionales →
/// el juego viejo y las entradas simples siguen funcionando.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtEntry {
    pub png: String,
    /// Tamaño de celda px de esta hoja (default 32 si falta).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<(u32, u32)>,
    /// Recorte estático del sprite dentro de la hoja (si es un sheet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<CellRect>,
    /// Animación de una tira (frames en fila). Con `strip`, frames = celdas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim: Option<ArtAnim>,
    /// Rango de celdas que forma la animación (1 fila).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip: Option<CellRect>,
    /// Poses direccionales/nombradas (personajes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poses: Option<std::collections::BTreeMap<String, Pose>>,
}

impl ArtEntry {
    pub fn new(png: String) -> Self {
        Self { png, grid: None, rect: None, anim: None, strip: None, poses: None }
    }
}

/// Valor de una clave: string suelto (viejo) u objeto (nuevo). `untagged` prueba
/// string primero y objeto después.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArtValue {
    Path(String),
    Full(ArtEntry),
}

impl ArtValue {
    pub fn png(&self) -> &str {
        match self {
            ArtValue::Path(p) => p,
            ArtValue::Full(e) => &e.png,
        }
    }
    pub fn anim(&self) -> Option<ArtAnim> {
        match self {
            ArtValue::Path(_) => None,
            ArtValue::Full(e) => e.anim,
        }
    }
    /// Normaliza a la forma completa (lo que escribe el editor).
    pub fn into_full(self) -> ArtEntry {
        match self {
            ArtValue::Path(png) => ArtEntry::new(png),
            ArtValue::Full(e) => e,
        }
    }
}

/// El art-manifest del juego.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ArtManifest {
    #[serde(default)]
    pub sprites: BTreeMap<String, ArtValue>,
}

impl ArtManifest {
    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }

    /// JSON legible y ordenado (BTreeMap) → diffs mínimos.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Asigna/actualiza un slot con PNG + animación opcional (forma completa).
    /// Conserva rect/strip/poses/grid si el slot ya existía con el mismo PNG.
    pub fn set(&mut self, key: &str, png: String, anim: Option<ArtAnim>) {
        let e = self.full_mut(key, png);
        e.anim = anim;
    }

    /// Devuelve la entrada completa del slot (creándola/normalizándola). Si el
    /// PNG cambió, resetea los recortes (eran de otra hoja).
    pub fn full_mut(&mut self, key: &str, png: String) -> &mut ArtEntry {
        let slot = self
            .sprites
            .entry(key.to_string())
            .or_insert_with(|| ArtValue::Full(ArtEntry::new(png.clone())));
        // Normalizar a Full.
        if let ArtValue::Path(p) = slot {
            *slot = ArtValue::Full(ArtEntry::new(std::mem::take(p)));
        }
        let ArtValue::Full(entry) = slot else { unreachable!() };
        if entry.png != png {
            *entry = ArtEntry::new(png);
        }
        entry
    }

    /// Marca un recorte estático (rect en celdas) para el slot.
    pub fn set_rect(&mut self, key: &str, png: String, grid: (u32, u32), rect: CellRect) {
        let e = self.full_mut(key, png);
        e.grid = Some(grid);
        e.rect = Some(rect);
        e.strip = None; // rect estático y strip son excluyentes
    }

    /// Marca una tira de animación (rango de celdas en 1 fila) a `fps`.
    pub fn set_strip(&mut self, key: &str, png: String, grid: (u32, u32), strip: CellRect, fps: f32) {
        let e = self.full_mut(key, png);
        e.grid = Some(grid);
        e.strip = Some(strip);
        e.anim = Some(ArtAnim { frames: strip.cells().max(1), fps });
        e.rect = None;
    }

    /// Agrega/actualiza una pose direccional/nombrada.
    pub fn set_pose(&mut self, key: &str, png: String, grid: (u32, u32), name: &str, pose: Pose) {
        let e = self.full_mut(key, png);
        e.grid = Some(grid);
        e.poses.get_or_insert_with(Default::default).insert(name.to_string(), pose);
    }

    pub fn remove_pose(&mut self, key: &str, name: &str) {
        if let Some(ArtValue::Full(e)) = self.sprites.get_mut(key) {
            if let Some(p) = e.poses.as_mut() {
                p.remove(name);
                if p.is_empty() {
                    e.poses = None;
                }
            }
        }
    }

    /// Quita el binding de un slot (vuelve a procedural en el juego).
    pub fn clear(&mut self, key: &str) {
        self.sprites.remove(key);
    }

    pub fn get(&self, key: &str) -> Option<&ArtValue> {
        self.sprites.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_old_string_format() {
        let old = r#"{ "sprites": { "decor/tree": "art/decor/tree.png" } }"#;
        let m = ArtManifest::from_json(old).unwrap();
        let v = m.get("decor/tree").unwrap();
        assert_eq!(v.png(), "art/decor/tree.png");
        assert_eq!(v.anim(), None);
    }

    #[test]
    fn reads_new_object_format_with_anim() {
        let new = r#"{ "sprites": { "tiles/water": { "png": "art/tiles/water.png", "anim": { "frames": 8, "fps": 6.0 } } } }"#;
        let m = ArtManifest::from_json(new).unwrap();
        let v = m.get("tiles/water").unwrap();
        assert_eq!(v.png(), "art/tiles/water.png");
        assert_eq!(v.anim(), Some(ArtAnim { frames: 8, fps: 6.0 }));
    }

    #[test]
    fn set_and_roundtrip() {
        let mut m = ArtManifest::default();
        m.set("decor/tree", "art/decor/oak.png".into(), None);
        m.set("tiles/water", "art/tiles/water.png".into(), Some(ArtAnim { frames: 4, fps: 8.0 }));
        let json = m.to_json().unwrap();
        let back = ArtManifest::from_json(&json).unwrap();
        assert_eq!(m, back);
        // El editor escribe forma completa (objeto), legible por el juego nuevo.
        assert!(json.contains("\"png\""));
    }
}
