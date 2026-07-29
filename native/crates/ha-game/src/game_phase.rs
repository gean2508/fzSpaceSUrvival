//! Máquina de estados del arranque: **Splash → MainMenu → CharacterCreate → Playing**.
//!
//! Clave del pedido: **el mundo NO se genera hasta `Playing`**. `scene::spawn_player`
//! corre en `OnEnter(GamePhase::Playing)`; sin `Player`, `world2d::stream_tile_chunks`
//! retorna temprano (ver world2d.rs) → cero worldgen/streaming en el menú. El menú y
//! la creación de personaje muestran un fondo animado (secuencia de imágenes), no
//! gameplay vivo.

use std::path::PathBuf;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GamePhase {
    #[default]
    Splash,
    MainMenu,
    CharacterCreate,
    Playing,
}

/// El personaje elegido en la pantalla de creación. Lo lee `scene::spawn_player`
/// al entrar a `Playing`. Serializable → es también el contenido del savegame v1.
#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct SelectedCharacter {
    pub name: String,
    /// Ruta del spritesheet dentro de `assets/` (mismo layout 16×32 que el default).
    pub sheet: String,
    /// Tinte de ropa/piel aplicado al sprite (RGBA lineal 0..1). `[1,1,1,1]` = sin tinte.
    pub tint: [f32; 4],
}

impl Default for SelectedCharacter {
    fn default() -> Self {
        Self {
            name: "Viajero".into(),
            sheet: "art/player/character.png".into(),
            tint: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

impl SelectedCharacter {
    pub fn tint_color(&self) -> Color {
        Color::srgba(self.tint[0], self.tint[1], self.tint[2], self.tint[3])
    }
}

pub struct GamePhasePlugin;

impl Plugin for GamePhasePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<GamePhase>().init_resource::<SelectedCharacter>();
    }
}

// ── savegame v1 ─────────────────────────────────────────────────────────────
// De momento el savegame guarda el personaje elegido (nombre/sheet/tinte). El
// estado del mundo ya se persiste aparte (world_persist / inventory_persist);
// esto sólo materializa "Cargar partida" en el menú principal.

pub fn savegame_path() -> PathBuf {
    crate::inventory_persist::persist_dir().join("savegame.json")
}

pub fn has_savegame() -> bool {
    savegame_path().exists()
}

pub fn load_savegame() -> Option<SelectedCharacter> {
    let text = std::fs::read_to_string(savegame_path()).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_savegame(ch: &SelectedCharacter) {
    let path = savegame_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(ch) {
        Ok(body) => {
            if let Err(e) = std::fs::write(&path, body) {
                eprintln!("savegame write failed: {e}");
            }
        }
        Err(e) => eprintln!("savegame serialize failed: {e}"),
    }
}
