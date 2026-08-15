//! Palancas de diagnóstico (`HA_*`) que funcionan EN LAS DOS plataformas.
//!
//! Todas las palancas del juego —los drivers de QA (`HA_FLYTEST`, `HA_ECOTEST`, …), las de
//! render (`HA_RES`, `HA_NO_LIGHTS`) y las de mundo (`HA_DAYPHASE`)— se leían con
//! `std::env::var`. En el escritorio eso alcanza, pero **una app Android no tiene entorno
//! de shell**: el proceso lo lanza `zygote`, no tu terminal, así que `HA_FLYTEST=1` nunca
//! llegaba y NINGÚN driver era alcanzable en el celu. La regla 11 del `CLAUDE.md` pide
//! probar en el Redmi antes de publicar; sin esto sólo se podía probar a mano.
//!
//! Acá hay UN camino que sirve en los dos lados:
//!
//! ```sh
//! # escritorio
//! HA_FLYTEST=1 ./target/debug/ha-game
//! # celu (sin root: el prefijo `debug.` es escribible por adb)
//! adb shell setprop debug.ha.flytest 1
//! ```
//!
//! El nombre se traduce solo: `HA_FLYTEST` → `debug.ha.flytest`.

use std::collections::HashMap;
use std::env::VarError;
#[cfg(target_os = "android")]
use std::sync::OnceLock;

/// Igual firma que `std::env::var` a propósito: los ~35 sitios que ya existían usan
/// `.is_ok()`, `.is_err()`, `.ok()`, `.map()` y `.unwrap_or_else()` sobre el `Result`, y así
/// migran con una sustitución textual en vez de reescribir cada uno.
pub fn var(name: &str) -> Result<String, VarError> {
    // El entorno manda siempre: en escritorio es el único camino, y en Android permite
    // forzar una palanca desde un runner sin tocar properties.
    if let Ok(v) = std::env::var(name) {
        return Ok(v);
    }
    #[cfg(target_os = "android")]
    {
        if let Some(v) = props().get(&prop_name(name)) {
            return Ok(v.clone());
        }
    }
    Err(VarError::NotPresent)
}

/// Los drivers de QA. Si alguno está prendido, el juego está siendo MANEJADO por un
/// programa, no por una persona.
const DRIVERS: [&str; 11] = [
    "HA_FLYTEST",
    "HA_MOONTEST",
    "HA_ROCKETTEST",
    "HA_NIGHTTEST",
    "HA_ECOTEST",
    "HA_CITYTEST",
    "HA_TRIPTEST",
    "HA_CYCLETEST",
    "HA_BUILDTEST",
    "HA_FACHADATEST",
    "HA_AUTODRIVE",
];

/// ¿Nos está manejando un driver de QA?
///
/// Sirve para una cosa muy concreta: en Android los drivers no podían actuar. Sintetizan
/// teclas (`ButtonInput<KeyCode>`), y ahí el táctil pisa `RawMovementIntent` con ceros cada
/// frame mientras que las acciones de teclado (saltar/volar) ni se registran. El resultado
/// era un driver que arrancaba, no movía a nadie y moría con `STUCK`.
///
/// Con esto el camino de teclado se habilita en el celu **sólo cuando hay un driver**, así
/// que el juego que toca una persona no cambia en nada.
pub fn driver_active() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| DRIVERS.iter().any(|d| var(d).is_ok()))
}

/// `HA_FLYTEST` → `debug.ha.flytest`. El prefijo `debug.` es el que Android deja escribir
/// por `adb shell setprop` sin root ni firma de plataforma.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn prop_name(name: &str) -> String {
    format!("debug.ha.{}", name.trim_start_matches("HA_").to_ascii_lowercase())
}

/// Volcado de properties, leído UNA vez. Se cachea porque esto lo consultan sistemas que
/// corren por frame (`debug/perf.rs`, `render/scale.rs`) y no vamos a pagar un `fork` ahí.
///
/// Se lee entero de una (`getprop` sin argumentos) en lugar de un `getprop <clave>` por
/// palanca: son 35 palancas y ese sería un proceso por cada una en el arranque.
#[cfg(target_os = "android")]
fn props() -> &'static HashMap<String, String> {
    static CACHE: OnceLock<HashMap<String, String>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let out = match std::process::Command::new("/system/bin/getprop").output() {
            Ok(o) => o.stdout,
            // Sin `getprop` no hay drivers, pero el juego tiene que arrancar igual: es una
            // palanca de diagnóstico, no una dependencia del gameplay.
            Err(e) => {
                eprintln!("ha-flags: no se pudo leer getprop ({e}) — palancas HA_* apagadas");
                return HashMap::new();
            }
        };
        parse_getprop(&String::from_utf8_lossy(&out))
    })
}

/// Formato de `getprop`: una línea `[clave]: [valor]` por property. Se filtra a `debug.ha.`
/// para no quedarnos con las ~1500 del sistema.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn parse_getprop(dump: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in dump.lines() {
        let Some((k, v)) = line.split_once("]: [") else { continue };
        let k = k.trim_start_matches('[').trim();
        let Some(v) = v.strip_suffix(']') else { continue };
        if k.starts_with("debug.ha.") && !v.is_empty() {
            map.insert(k.to_string(), v.to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traduce_el_nombre_de_la_palanca_a_property() {
        assert_eq!(prop_name("HA_FLYTEST"), "debug.ha.flytest");
        assert_eq!(prop_name("HA_ECOTEST"), "debug.ha.ecotest");
        // Sin prefijo `HA_` no se rompe: se usa tal cual, en minúscula.
        assert_eq!(prop_name("RES"), "debug.ha.res");
    }

    #[test]
    fn parsea_el_volcado_de_getprop_y_descarta_lo_ajeno() {
        let dump = "\
[debug.ha.flytest]: [1]
[debug.ha.dayphase]: [noche]
[debug.ha.vacia]: []
[ro.build.version.sdk]: [33]
basura sin formato";
        let m = parse_getprop(dump);
        assert_eq!(m.get("debug.ha.flytest").map(String::as_str), Some("1"));
        assert_eq!(m.get("debug.ha.dayphase").map(String::as_str), Some("noche"));
        // Una property vacía es "no seteada": si contara, `setprop x ""` prendería el driver.
        assert!(!m.contains_key("debug.ha.vacia"));
        assert!(!m.contains_key("ro.build.version.sdk"));
        assert_eq!(m.len(), 2);
    }

    /// En escritorio el entorno tiene que seguir mandando exactamente como antes.
    #[test]
    fn el_entorno_gana_en_escritorio() {
        std::env::set_var("HA_PRUEBA_FLAGS", "42");
        assert_eq!(var("HA_PRUEBA_FLAGS").as_deref(), Ok("42"));
        std::env::remove_var("HA_PRUEBA_FLAGS");
        assert!(var("HA_PRUEBA_FLAGS").is_err());
    }
}
