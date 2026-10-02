//! Lo que comparten los agentes que persisten (fauna residente, ecología, NPCs): un archivo
//! JSON por sistema, leído tolerante y escrito de forma atómica. Una caída a mitad del
//! guardado nunca deja un archivo truncado que la próxima partida no pueda leer.

use std::path::Path;

use serde::{de::DeserializeOwned, Serialize};

/// Lee `path`. `None` si no existe o no se entiende (el llamador arranca de cero).
pub(crate) fn cargar_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Escribe `valor` en `path` por archivo temporal + renombrado (atómico en el mismo disco).
/// Crea la carpeta si hace falta. Devuelve si quedó guardado.
pub(crate) fn guardar_json<T: Serialize>(path: &Path, valor: &T) -> bool {
    let Ok(json) = serde_json::to_string(valor) else {
        return false;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, PartialEq, Debug, Default)]
    struct Caja {
        a: u32,
        b: Vec<String>,
    }

    fn dir(n: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ha-persist-{n}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn ida_y_vuelta_crea_la_carpeta_y_no_deja_temporales() {
        let d = dir("ida");
        let p = d.join("sub").join("x.json");
        let v = Caja { a: 7, b: vec!["hola".into()] };
        assert!(guardar_json(&p, &v));
        assert_eq!(cargar_json::<Caja>(&p), Some(v));
        assert!(!p.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn archivo_ausente_o_roto_es_none() {
        let d = dir("roto");
        let p = d.join("x.json");
        assert_eq!(cargar_json::<Caja>(&p), None);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(&p, "{ no es json").unwrap();
        assert_eq!(cargar_json::<Caja>(&p), None);
        let _ = std::fs::remove_dir_all(&d);
    }
}
