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

/// Entrada completa (formato nuevo).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtEntry {
    pub png: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anim: Option<ArtAnim>,
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
            ArtValue::Path(png) => ArtEntry { png, anim: None },
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
    pub fn set(&mut self, key: &str, png: String, anim: Option<ArtAnim>) {
        self.sprites.insert(key.to_string(), ArtValue::Full(ArtEntry { png, anim }));
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
