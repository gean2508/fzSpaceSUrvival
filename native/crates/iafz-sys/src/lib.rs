//! FFI seguro a la **IA frugal IAFZ-64** (HIVE_OS) — un cerebro Deep BNN binario
//! (activación `XNOR + popcount`, sin punto flotante pesado ni backpropagation)
//! con memoria recurrente de 32 bits. Entrenado por neuroevolución OFFLINE; en el
//! juego se corre sólo **inferencia** (`think`) a costo casi nulo → mil agentes
//! pensando en el Redmi. Es la capa de INSTINTO/locomoción del cerebro colmena de
//! NPCs/fauna/enemigos (ver `biblia.md` §6).
//!
//! El estado de los cerebros vive en un array global en C (`swarm[MAX_AGENTS]`),
//! indexado por `id`. No es thread-safe: llamalo desde UN sistema (o serializá el
//! acceso). Determinista dado (genoma, sensores, memoria) → apto para la sim.

use std::ffi::CString;
use std::os::raw::{c_char, c_float, c_int, c_uint};

/// Máximo de cerebros simultáneos (definido en el kernel C).
pub const MAX_AGENTS: i32 = 1000;
/// Tamaño exacto de un genoma `.adn` (Brain64).
pub const BRAIN_BYTES: usize = 9600;

extern "C" {
    fn init_iafz_64(id: c_int, seed: c_uint);
    fn think_iafz_64(id: c_int, exteroception_32: c_uint) -> c_float;
    fn mutate_iafz_64(source_id: c_int, target_id: c_int, rate: c_float);
    fn crossover_iafz_64(p1: c_int, p2: c_int, child: c_int);
    fn reset_echo_iafz_64(id: c_int);
    fn save_iafz_64(id: c_int, filename: *const c_char) -> c_int;
    fn load_iafz_64(id: c_int, filename: *const c_char) -> c_int;
    fn load_bytes_iafz_64(id: c_int, data: *const u8, len: c_int) -> c_int;
}

/// Carga un genoma desde MEMORIA (para embeberlo en el juego con `include_bytes!`,
/// sin archivo). `data` debe medir `BRAIN_BYTES`. `true` si ok.
pub fn load_bytes(id: i32, data: &[u8]) -> bool {
    if data.len() != BRAIN_BYTES {
        return false;
    }
    unsafe { load_bytes_iafz_64(id, data.as_ptr(), data.len() as c_int) != 0 }
}

/// Inicializa el cerebro `id` con un genoma pseudo-aleatorio derivado de `seed`.
pub fn init(id: i32, seed: u32) {
    unsafe { init_iafz_64(id, seed) }
}

/// Inferencia: 32 bits de sensores (exterocepción) → **acción analógica** (float,
/// p.ej. giro/steer). Actualiza la memoria recurrente interna del agente. Fuera de
/// rango `[0, MAX_AGENTS)` devuelve 0.
pub fn think(id: i32, sensors: u32) -> f32 {
    unsafe { think_iafz_64(id, sensors) }
}

/// Resetea la memoria recurrente del agente (nace "en blanco").
pub fn reset_echo(id: i32) {
    unsafe { reset_echo_iafz_64(id) }
}

/// Copia el genoma de `source` a `target` con mutación (`rate` 0..1). Para el gym.
pub fn mutate(source: i32, target: i32, rate: f32) {
    unsafe { mutate_iafz_64(source, target, rate) }
}

/// Cruza dos padres en un hijo (neuroevolución).
pub fn crossover(p1: i32, p2: i32, child: i32) {
    unsafe { crossover_iafz_64(p1, p2, child) }
}

/// Carga un genoma `.adn` (9600 bytes) en el agente `id`. `true` si ok.
pub fn load(id: i32, path: &str) -> bool {
    let Ok(c) = CString::new(path) else { return false };
    unsafe { load_iafz_64(id, c.as_ptr()) != 0 }
}

/// Guarda el genoma del agente `id` a un `.adn`. `true` si ok.
pub fn save(id: i32, path: &str) -> bool {
    let Ok(c) = CString::new(path) else { return false };
    unsafe { save_iafz_64(id, c.as_ptr()) != 0 }
}

/// Offset y largo de las 64 salidas de acción dentro de un genoma (`Brain64`): van después de
/// las dos capas binarias (2 × 64 × 9 B). Es el equivalente de `iafz::salidas_en_cero` del
/// proyecto dron (`~/Documents/mods/dron`), pero para el kernel de 1 salida de este juego.
pub const SALIDAS_INICIO: usize = 2 * 64 * 9;
pub const SALIDAS_LARGO: usize = 64 * 4;

/// Pone en cero las salidas de acción de un genoma volcado: la red emite exactamente 0 y la
/// conducta es la del código determinista que la rodea (la «vara honesta»). Es la semilla de un
/// residuo: la evolución arranca EN el clásico y sólo puede sumarle (bitácora 7.54 del dron).
pub fn salidas_en_cero(bytes: &mut [u8]) {
    if bytes.len() >= SALIDAS_INICIO + SALIDAS_LARGO {
        bytes[SALIDAS_INICIO..SALIDAS_INICIO + SALIDAS_LARGO].fill(0);
    }
}

/// Los 9600 bytes del genoma de `id` (vía un archivo temporal: el kernel sólo sabe guardar a
/// disco). `None` si falla.
pub fn dump(id: i32) -> Option<Vec<u8>> {
    let tmp = std::env::temp_dir().join(format!("iafz_dump_{}_{id}.adn", std::process::id()));
    let ruta = tmp.to_str()?;
    let ok = save(id, ruta);
    let bytes = if ok { std::fs::read(&tmp).ok() } else { None };
    let _ = std::fs::remove_file(&tmp);
    bytes.filter(|b| b.len() == BRAIN_BYTES)
}

/// Deja a `id` con las salidas en cero (conserva las capas binarias y la memoria).
pub fn poner_salidas_en_cero(id: i32) -> bool {
    let Some(mut b) = dump(id) else { return false };
    salidas_en_cero(&mut b);
    load_bytes(id, &b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn think_deterministic_and_genome_io() {
        // Determinismo: mismo cerebro + misma memoria + mismos sensores → igual.
        init(0, 12345);
        reset_echo(0);
        let a1 = think(0, 0xDEAD_BEEF);
        reset_echo(0);
        let a2 = think(0, 0xDEAD_BEEF);
        assert_eq!(a1.to_bits(), a2.to_bits(), "think debe ser determinista");

        // Cargar el genoma entrenado vendorizado (prueba que el .adn calza en Brain64).
        let genome = concat!(env!("CARGO_MANIFEST_DIR"), "/genomes/VENCEDOR_64.adn");
        assert!(load(1, genome), "debe cargar VENCEDOR_64.adn (9600 bytes)");
        let _ = think(1, 0x0F0F_0F0F); // no debe crashear

        // save→load roundtrip.
        let tmp = std::env::temp_dir().join("iafz_roundtrip.adn");
        let tmp = tmp.to_str().unwrap();
        assert!(save(1, tmp), "save");
        assert!(load(2, tmp), "load del guardado");
        let _ = std::fs::remove_file(tmp);
    }

    #[test]
    fn con_las_salidas_en_cero_la_red_emite_cero() {
        init(7, 99);
        assert!(poner_salidas_en_cero(7));
        reset_echo(7);
        for k in 0..64u32 {
            assert_eq!(think(7, k.wrapping_mul(2654435761)), 0.0);
        }
        // Y sigue siendo un genoma válido que se puede mutar hacia algo distinto de cero.
        mutate(7, 8, 1.0);
        reset_echo(8);
        assert!((0..64u32).any(|k| think(8, k.wrapping_mul(2654435761)) != 0.0));
    }
}
