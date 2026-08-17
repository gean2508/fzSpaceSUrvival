//! **CUÁNTO OCUPA EL DAMERO DE LA NIEVE, EN TILES.**
//!
//! `geography::pasa_el_escalon` difumina el borde de los biomas de altura con una
//! matriz de Bayer, y el ancho de esa franja se calibró tres veces
//! (0,45 → 0,12 → 0,07) midiéndola en FRACCIÓN DE ALTURA, hasta que se cambió a
//! medirla caminando (`AJEDREZ_EN_PASOS`). En las capturas frescas del 16 ago
//! (`relieve-0-pared.png`, `relieve-1-rampa.png`) el damero vuelve a cubrir la
//! pantalla, así que hay que medirlo en la unidad en que se ve: **tiles**.
//!
//! ```sh
//! cargo test -p ha-shared --release --test mapa_de_la_nieve -- --ignored --nocapture
//! ```

use ha_shared::geography::{self, Biome};
use ha_shared::relief;
use ha_shared::CelestialBody;

/// Los dos sitios que el driver del relieve retrata, y la cumbre.
const SITIOS: [(&str, i64, i64); 3] = [
    ("pared (relieve-0)", 1763, -5087),
    ("rampa (relieve-1)", 1796, -5097),
    ("cumbre Monte Ceniza", 15599, 9010),
];

/// Una celda está EN EL DAMERO si su bioma difumina distinto que el de su
/// terraza: o sea, si `biome_at` y `biome_de_terraza` no coinciden. Es
/// exactamente lo que el ojo lee como tablero.
fn en_el_damero(b: CelestialBody, x: i64, z: i64) -> bool {
    geography::biome_at(b, x, z) != geography::biome_de_terraza(b, x, z)
}

#[test]
#[ignore]
fn cuanto_de_la_pantalla_es_damero() {
    let b = CelestialBody::Earth;
    // La pantalla del juego, medida en las capturas: 80×40 tiles.
    for (nombre, cx, cz) in SITIOS {
        let (mut damero, mut total) = (0u32, 0u32);
        let mut biomas: std::collections::BTreeMap<String, u32> = Default::default();
        for z in cz - 20..cz + 20 {
            for x in cx - 40..cx + 40 {
                total += 1;
                if en_el_damero(b, x, z) {
                    damero += 1;
                }
                *biomas.entry(format!("{:?}", geography::biome_at(b, x, z))).or_default() += 1;
            }
        }
        let mut v: Vec<_> = biomas.into_iter().collect();
        v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        let reparto: Vec<String> =
            v.iter().map(|(k, n)| format!("{k} {:.0}%", *n as f64 * 100.0 / total as f64)).collect();
        println!(
            "{nombre:<22} ({cx},{cz}) damero {:.1}% de la pantalla | {}",
            damero as f64 * 100.0 / total as f64,
            reparto.join(", ")
        );
    }
}

/// **EL ANCHO DE LA FRANJA, CAMINANDO.** Se recorre una columna larga y se mide
/// cada racha seguida de celdas en damero: eso es cuántos pasos dura el tablero
/// al cruzar la frontera. `AJEDREZ_EN_PASOS` dice que deberían ser ocho.
#[test]
#[ignore]
fn cuantos_pasos_dura_el_tablero() {
    let b = CelestialBody::Earth;
    let mut hist: std::collections::BTreeMap<usize, u32> = Default::default();
    let (mut total, mut suma, mut peor) = (0u64, 0u64, 0usize);
    // Columnas repartidas por la cordillera, que es donde está la nieve.
    for i in 0..200i64 {
        let x = -14_000 + i * 140;
        let mut racha = 0usize;
        for z in -7_000..-2_000 {
            if relief::relief_level(b, x, z) >= 0 && en_el_damero(b, x, z) {
                racha += 1;
            } else if racha > 0 {
                *hist.entry(racha.min(200)).or_default() += 1;
                total += 1;
                suma += racha as u64;
                peor = peor.max(racha);
                racha = 0;
            }
        }
    }
    println!("--- RACHAS DE DAMERO (cordillera, 200 columnas de 5.000 tiles) ---");
    println!(
        "rachas {total}, media {:.1} tiles, la peor {peor} tiles (AJEDREZ_EN_PASOS dice 8)",
        suma as f64 / total.max(1) as f64
    );
    let mut acum = 0u32;
    for (n, c) in &hist {
        acum += c;
        if *n <= 12 || *n % 20 == 0 || *n >= 200 {
            println!("  {n:>3} tiles: {c:>6}  (acumulado {:.1}%)", acum as f64 * 100.0 / total as f64);
        }
    }
    let _ = Biome::Snow;
}

/// **¿CUÁNTO MUNDO NIEVA, CON LA REGLA VIEJA Y CON LA NUEVA?**
///
/// La regla de la casa es que la nieve sea el TECHO del mundo y nada más
/// (biblia §5, "LA CUMBRE ES BLANCA"). El censo de
/// `auditoria_del_mapa::de_que_esta_hecha_la_tierra` dice que los escalones 9 y
/// más son el **2,20 % de la tierra firme**: ése es el presupuesto. Si trazar la
/// línea sobre el campo de cordillera lo dispara, el arreglo cambió un bug por
/// otro.
#[test]
#[ignore]
fn cuanta_tierra_nieva() {
    let b = CelestialBody::Earth;
    let r = 30_000i64;
    let paso = 23i64;
    let (mut tierra, mut vieja, mut nueva) = (0u64, 0u64, 0u64);
    let mut z = -r;
    while z < r {
        let mut x = -r;
        while x < r {
            let l = relief::relief_level(b, x, z);
            if l >= 0 {
                tierra += 1;
                // La regla vieja: el escalón de la celda.
                if l >= geography::NIVEL_NIEVE {
                    vieja += 1;
                }
                // La nueva: el bioma que sale de verdad.
                if geography::biome_at(b, x, z) == Biome::Snow {
                    nueva += 1;
                }
            }
            x += paso;
        }
        z += paso;
    }
    println!("--- CUÁNTA TIERRA NIEVA (Tierra entera, 1 de cada {}) ---", paso * paso);
    println!("tierra firme: {tierra} muestras");
    println!("  escalón >= {} (regla vieja): {:.2}%", geography::NIVEL_NIEVE, vieja as f64 * 100.0 / tierra as f64);
    println!("  Biome::Snow de verdad      : {:.2}%", nueva as f64 * 100.0 / tierra as f64);
}

/// **A QUÉ ESCALÓN DEL CAMPO DE CORDILLERA HAY QUE PONER LA LÍNEA.**
///
/// El campo suave no tiene la misma escalera efectiva que el completo: al quitarle
/// las octavas finas se le va la varianza, así que el mismo número de escalón deja
/// más tierra por encima. El umbral no se elige, se mide contra el presupuesto de
/// la casa (el 2,20 % de `escalón >= 9` sobre el campo completo).
#[test]
#[ignore]
fn a_que_escalon_va_la_linea_de_nieve() {
    let b = CelestialBody::Earth;
    let r = 30_000i64;
    let paso = 23i64;
    let mut tierra = 0u64;
    let mut por_umbral = [0u64; 14];
    let mut z = -r;
    while z < r {
        let mut x = -r;
        while x < r {
            if relief::relief_level(b, x, z) >= 0 {
                tierra += 1;
                let (n, _) = relief::escalon_de_cordillera(b, x, z);
                for (u, c) in por_umbral.iter_mut().enumerate() {
                    if n >= u as i8 {
                        *c += 1;
                    }
                }
            }
            x += paso;
        }
        z += paso;
    }
    println!("--- LÍNEA DE NIEVE sobre el campo de cordillera ---");
    println!("presupuesto de la casa: 2,20 % de la tierra firme (escalón >= 9 del campo completo)");
    for (u, c) in por_umbral.iter().enumerate().skip(7) {
        println!("  umbral {u:>2}: {:.2}% de la tierra firme", *c as f64 * 100.0 / tierra as f64);
    }
}

/// **LA COMPARACIÓN HONESTA: las dos reglas COMPLETAS, con su ajedrez.**
///
/// El primer intento comparó la regla nueva (con difuminado) contra la vieja SIN
/// difuminar, y por eso pareció que la nieve se había duplicado. El ajedrez sólo
/// AÑADE —`pasa_el_escalon` devuelve `true` de una si el escalón ya llegó, y sólo
/// sortea en el escalón de abajo—, así que cualquier regla dithered cubre más que
/// su umbral pelado. Hay que comparar dithered contra dithered.
#[test]
#[ignore]
fn las_dos_reglas_de_nieve_lado_a_lado() {
    let b = CelestialBody::Earth;
    let r = 30_000i64;
    let paso = 23i64;
    let (mut tierra, mut vieja, mut nueva) = (0u64, 0u64, 0u64);
    let mut z = -r;
    while z < r {
        let mut x = -r;
        while x < r {
            let l = relief::relief_level(b, x, z);
            if l >= 0 {
                tierra += 1;
                // La regla VIEJA completa: ajedrez sobre el escalón de la celda.
                if geography::pasa_el_escalon(b, x, z, l, geography::NIVEL_NIEVE) {
                    vieja += 1;
                }
                if geography::biome_at(b, x, z) == Biome::Snow {
                    nueva += 1;
                }
            }
            x += paso;
        }
        z += paso;
    }
    println!("--- LAS DOS REGLAS DE NIEVE, COMPLETAS ---");
    println!("tierra firme: {tierra} muestras");
    println!("  VIEJA (ajedrez sobre el escalón de la celda): {:.2}%", vieja as f64 * 100.0 / tierra as f64);
    println!("  NUEVA (ajedrez sobre el campo de cordillera): {:.2}%", nueva as f64 * 100.0 / tierra as f64);
}
