//! **UNA PARTIDA NUEVA ES UN MUNDO NUEVO.**
//!
//! Pedido del usuario, textual: *"cada que yo abra una partida, a menos que yo
//! guarde la partida, se abra un mundo nuevo, o sea todo nuevo, por ende no hay
//! nada de lo que yo haya colocado, porque no me gusta que cosas que coloqué
//! antes estén puestas"*.
//!
//! Hasta ahora el juego tenía UN mundo y UN estado, los dos eternos:
//!
//! - el terreno salía de `celestial_bodies::body_terrain_seed`, una constante
//!   compilada, así que la Tierra era siempre la misma;
//! - y todo lo que hacías se autoguardaba en `inventory_persist::persist_dir`
//!   sin que nadie lo borrara nunca, así que "Nueva partida" te devolvía a tu
//!   casa de la vez anterior.
//!
//! Acá se cierran las dos cosas, y con la misma decisión: **al pulsar NUEVA
//! PARTIDA se sortea una semilla y se tira el estado guardado**; al pulsar
//! CARGAR se recupera la semilla del disco y el estado queda donde estaba.
//!
//! ## Por qué no se borra el directorio entero
//!
//! `persist_dir()` es `"."` en escritorio — el repositorio. Borrarlo a lo bruto
//! sería catastrófico y no es teórico: es el primer atajo que se le ocurre a
//! cualquiera. Se borran **exactamente los archivos que el juego escribe**
//! ([`ARCHIVOS`] y [`CARPETAS`]); cualquier otra cosa del directorio no se toca.
//!
//! ## Guardar
//!
//! Guardar sigue siendo lo que ya era —`game_phase::save_savegame` con el
//! personaje— y ahora además deja la semilla en `partida.json`. Sin ese archivo,
//! cargar una partida vieja usa la semilla 0, que es el mundo de referencia de
//! siempre: las partidas anteriores a este cambio siguen abriendo su mundo.

use std::path::PathBuf;

use crate::inventory_persist::persist_dir;

/// Los archivos de estado que escribe el juego. **Es la lista completa** — si
/// alguien agrega uno nuevo y no lo pone acá, sobrevivirá a "Nueva partida" y el
/// mundo nuevo saldrá con un trozo del viejo pegado. Lo cuida
/// `ningun_estado_de_partida_se_escapa_del_borrado`.
const ARCHIVOS: &[&str] = &[
    "cameras.json",
    "ceilings.json",
    "chests.json",
    "colony.json",
    "edificios.json",
    "escaleras.json",
    "harvested.json",
    "inventory.json",
    "machines.json",
    "maps.json",
    "nav_data.json",
    "roofs.json",
    "savegame.json",
    "structures.json",
    "survival.json",
    "tiles.json",
    "torches.json",
    "vessels.json",
    "walls.json",
    "partida.json",
];

/// Las carpetas de estado. `vivarium` es la fauna y el compañero-IA; `society`,
/// la memoria de los NPC.
const CARPETAS: &[&str] = &["vivarium", "society"];

pub fn partida_path() -> PathBuf {
    persist_dir().join("partida.json")
}

/// **Arrancar una partida nueva**: semilla sorteada y disco limpio.
///
/// La semilla sale del reloj, que es la única fuente de entropía que no obliga a
/// meter una dependencia nueva para sortear un `u32`. Nunca devuelve 0: el 0 está
/// reservado para "el mundo de referencia" (el de los tests y los medidores), y
/// una partida que cayera ahí por azar sería la única indistinguible de ellos.
pub fn nueva() {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    // Mezcla para que dos arranques seguidos no salgan con semillas vecinas: los
    // nanos consecutivos comparten los bits altos y el ruido de terreno los usa.
    let mut s = n ^ (n >> 33);
    s = s.wrapping_mul(0xff51_afd7_ed55_8ccd);
    s ^= s >> 33;
    let semilla = ((s as u32) | 1).max(1);
    borrar_estado();
    ha_shared::partida::set_semilla(semilla);
    eprintln!("ha-partida: mundo nuevo, semilla {semilla}");
}

/// **Cargar la partida guardada**: se recupera su semilla y no se borra nada.
///
/// Sin `partida.json` (partida anterior a este cambio) queda la semilla 0, que es
/// el mundo que esa partida tenía cuando se guardó.
pub fn cargar() {
    let semilla = std::fs::read_to_string(partida_path())
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("semilla").and_then(|s| s.as_u64()))
        .unwrap_or(0) as u32;
    ha_shared::partida::set_semilla(semilla);
    eprintln!("ha-partida: partida cargada, semilla {semilla}");
}

/// Deja la semilla de la partida en curso en el disco. La llama quien guarda el
/// personaje (`game_phase::save_savegame`), para que las dos mitades del
/// savegame se escriban juntas y no puedan quedar desparejas.
pub fn guardar_semilla() {
    let path = partida_path();
    if let Some(parent) = path.parent() {
        if parent != std::path::Path::new("") && parent != std::path::Path::new(".") {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let body = format!("{{\n  \"semilla\": {}\n}}\n", ha_shared::partida::semilla());
    if let Err(e) = std::fs::write(&path, body) {
        eprintln!("ha-partida: no se pudo guardar la semilla ({e})");
    }
}

/// Borra el estado de la partida anterior. Sólo [`ARCHIVOS`] y [`CARPETAS`]: lo
/// demás que haya en el directorio no es del juego y no se toca (ver el
/// docstring del módulo — en escritorio ese directorio es el repositorio).
fn borrar_estado() {
    let dir = persist_dir();
    for f in ARCHIVOS {
        let _ = std::fs::remove_file(dir.join(f));
    }
    for d in CARPETAS {
        let _ = std::fs::remove_dir_all(dir.join(d));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **La lista de archivos a borrar tiene que estar completa.**
    ///
    /// El modo de falla que cuida es silencioso y feo: alguien agrega
    /// `persist_dir().join("loquesea.json")` en un módulo nuevo, nadie lo agrega
    /// acá, y a partir de ese día "Nueva partida" arranca con un trozo del mundo
    /// anterior pegado. Se lee el propio código fuente porque no hay registro
    /// central de archivos persistidos — y montar uno sería más ceremonia que
    /// esta comprobación.
    #[test]
    fn ningun_estado_de_partida_se_escapa_del_borrado() {
        let mut vistos: Vec<String> = Vec::new();
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut pendientes = vec![raiz];
        while let Some(p) = pendientes.pop() {
            let Ok(rd) = std::fs::read_dir(&p) else { continue };
            for e in rd.flatten() {
                let path = e.path();
                if path.is_dir() {
                    pendientes.push(path);
                    continue;
                }
                if path.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                let Ok(txt) = std::fs::read_to_string(&path) else { continue };
                for (_, resto) in txt.match_indices("persist_dir().join(\"") {
                    if let Some(fin) = resto[20..].find('"') {
                        vistos.push(resto[20..20 + fin].to_string());
                    }
                }
            }
        }
        let conocidos: Vec<&str> = ARCHIVOS.iter().chain(CARPETAS.iter()).copied().collect();
        let faltan: Vec<&String> = vistos
            .iter()
            .filter(|v| !conocidos.contains(&v.as_str()))
            .collect();
        assert!(
            faltan.is_empty(),
            "estos archivos se persisten y NO se borran al empezar de nuevo: {faltan:?} \
             — agregalos a partida::ARCHIVOS o a CARPETAS"
        );
    }
}
