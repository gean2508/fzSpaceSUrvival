//! **Los personajes viven aparte de los mundos.**
//!
//! Decisión del usuario el 15 ago 2026, y es la que ordena todo el menú: *"el
//! personaje es uno y los mundos son otros"*. Un personaje no pertenece a una
//! partida — entra a la que elijas, y en un mismo mundo pueden vivir varios.
//!
//! Por eso esta lista **no** se guarda en `persist_dir()` (que desde el
//! multi-partida apunta adentro del mundo abierto) sino en
//! `inventory_persist::base_dir()`: borrar una partida no puede llevarse a nadie
//! puesto, y ése es justamente el modo de falla que el usuario pidió evitar
//! cuando pidió la papelera.
//!
//! ## Qué se guarda
//!
//! El `LookDef` completo, que es exactamente lo que el juego ya sabe componer:
//! `scene.rs::spawn_player` arma el sprite del mundo con esas mismas capas. Ese
//! hilo —crear un personaje y que el que camina sea ése— ya funcionaba con UN
//! personaje en `savegame.json`; acá se generaliza a una lista sin cortarlo.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::game_phase::SelectedCharacter;

/// Cuántos caben. Es el número que el usuario tenía que decidir y que el
/// documento dejó abierto ("dibujé ocho personajes"): se toma el del mockup.
/// Ocho entran en la grilla de los mockups sin scroll en el apaisado del Redmi.
pub const MAXIMO: usize = 8;

const ARCHIVO: &str = "personajes.json";

/// Una entrada de la lista. `look` es lo que dibuja al personaje; `nombre` es lo
/// que se lee en la fila.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Personaje {
    pub id: String,
    pub nombre: String,
    /// Las capas del chargen, tal cual las modela `character_create`. `None` es
    /// un personaje de un savegame viejo, que se dibuja con `sheet`.
    #[serde(default)]
    pub look: Option<crate::chargen::LookDef>,
    /// El spritesheet de respaldo, para los que no traen capas.
    #[serde(default = "hoja_por_defecto")]
    pub sheet: String,
    /// Tinte RGBA lineal. `[1,1,1,1]` = sin tinte.
    #[serde(default = "sin_tinte")]
    pub tint: [f32; 4],
    /// Segundos desde el epoch: ordena la lista y desempata los nombres iguales.
    #[serde(default)]
    pub creado: u64,
}

fn hoja_por_defecto() -> String {
    "art/player/character.png".to_string()
}

fn sin_tinte() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}

fn ahora() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn path() -> PathBuf {
    crate::inventory_persist::base_dir().join(ARCHIVO)
}

/// **Todos los personajes, el más nuevo primero.**
pub fn listar() -> Vec<Personaje> {
    let Ok(txt) = std::fs::read_to_string(path()) else {
        return Vec::new();
    };
    let mut v: Vec<Personaje> = serde_json::from_str(&txt).unwrap_or_default();
    v.sort_by(|a, b| b.creado.cmp(&a.creado));
    v
}

fn escribir(v: &[Personaje]) {
    let p = path();
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(v) {
        Ok(body) => {
            if let Err(e) = std::fs::write(&p, body) {
                eprintln!("ha-personajes: no se pudo guardar ({e})");
            }
        }
        Err(e) => eprintln!("ha-personajes: lista ilegible ({e})"),
    }
}

/// **Guardar uno.** Si el id ya existe lo pisa (eso es "editar"); si no, lo
/// agrega. Devuelve `false` cuando la lista está llena y el personaje es nuevo:
/// el menú lo usa para no dejar crear el noveno en silencio.
pub fn guardar(p: &Personaje) -> bool {
    let mut v = listar();
    if let Some(slot) = v.iter_mut().find(|x| x.id == p.id) {
        *slot = p.clone();
    } else {
        if v.len() >= MAXIMO {
            return false;
        }
        v.push(p.clone());
    }
    escribir(&v);
    true
}

/// **Borrar uno.** No toca ninguna partida: el mundo donde vivió queda entero.
pub fn borrar(id: &str) {
    let v: Vec<Personaje> = listar().into_iter().filter(|p| p.id != id).collect();
    escribir(&v);
}

pub fn buscar(id: &str) -> Option<Personaje> {
    listar().into_iter().find(|p| p.id == id)
}

/// Un id nuevo. El reloj alcanza: no se crean dos personajes en el mismo
/// segundo con el pulgar.
pub fn nuevo_id() -> String {
    format!("c{}", ahora())
}

/// Pasa un personaje de la lista al que va a jugar. Es el hilo que hay que no
/// romper: `scene::spawn_player` compone el sprite del mundo con este `look`.
pub fn a_seleccionado(p: &Personaje) -> SelectedCharacter {
    SelectedCharacter {
        name: p.nombre.clone(),
        sheet: p.sheet.clone(),
        tint: p.tint,
        look: p.look.clone(),
    }
}

/// El camino de vuelta: lo que la pantalla de creación produce, guardado en la
/// lista. `id` vacío = personaje nuevo.
pub fn de_seleccionado(id: &str, ch: &SelectedCharacter) -> Personaje {
    Personaje {
        id: if id.is_empty() { nuevo_id() } else { id.to_string() },
        nombre: ch.name.clone(),
        look: ch.look.clone(),
        sheet: ch.sheet.clone(),
        tint: ch.tint,
        creado: ahora(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **La lista no vive adentro de una partida.** Es la invariante de la
    /// decisión "el personaje es uno y los mundos son otros": si este archivo
    /// cayera en `persist_dir()`, borrar una partida se llevaría a los
    /// personajes puestos — que es justo lo que el usuario pidió que no pasara.
    #[test]
    fn los_personajes_no_viven_adentro_de_una_partida() {
        let p = path();
        assert!(
            !p.to_string_lossy().contains("partidas"),
            "personajes.json quedó dentro de partidas/ ({})",
            p.display()
        );
    }
}
