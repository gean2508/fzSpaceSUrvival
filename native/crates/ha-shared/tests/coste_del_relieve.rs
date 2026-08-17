//! **CUÁNTO CUESTA PREGUNTARLE AL RELIEVE.**
//!
//! `relief_cell` es la consulta que hace el cliente por CADA tile de CADA chunk
//! que arma. Si en la montaña cuesta cien veces lo que en el llano, el mundo se
//! congela justo donde hay relieve — y lo que se ve es una pantalla vacía,
//! porque el chunk nunca llegó a armarse.
//!
//! ```sh
//! cargo test -p ha-shared --release --test coste_del_relieve -- --ignored --nocapture
//! ```

use ha_shared::relief;
use ha_shared::CelestialBody;
use std::time::Instant;

/// Un chunk del cliente. `world2d::CHUNK`.
const CHUNK: i64 = 32;

fn mide(nombre: &str, x0: i64, z0: i64, lado: i64) {
    let b = CelestialBody::Earth;
    // Sin contar la primera pasada: la caché de celdas se llena y no es lo que
    // se quiere medir (aunque tampoco salva, ver el resultado).
    let t = Instant::now();
    let mut n = 0u64;
    let mut niveles = [0u32; 14];
    for z in z0..z0 + lado {
        for x in x0..x0 + lado {
            let _ = relief::relief_cell(b, x, z);
            let l = relief::relief_level(b, x, z);
            niveles[(l.max(0) as usize).min(13)] += 1;
            n += 1;
        }
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    let chunks = (lado / CHUNK) * (lado / CHUNK);
    let alto = niveles.iter().enumerate().filter(|(l, _)| *l >= 5).map(|(_, c)| *c).sum::<u32>();
    println!(
        "{nombre:<24} ({x0},{z0}) {n} celdas en {ms:8.1} ms  = {:7.2} us/celda  \
         ≈ {:7.1} ms por chunk de 32x32   (celdas de nivel>=5: {alto})",
        ms * 1000.0 / n as f64,
        ms / chunks.max(1) as f64
    );
}

/// **DÓNDE SE VA EL TIEMPO.** `relief_cell` es lo que paga el cliente, pero por
/// dentro llama a `relief_level` cientos de veces y cada fallo de caché rehace
/// `compute_field` — que a su vez recorre la plantilla del mundo y TODOS los
/// tramos de río del bloque de 128×128. Este reparto dice cuál de las tres capas
/// hay que arreglar.
#[test]
#[ignore]
fn reparto_del_coste() {
    let b = CelestialBody::Earth;
    let lado = 128i64;
    for (nombre, x0, z0) in [
        ("llano del spawn", -64i64, -64i64),
        ("ladera de la sierra", 1699, -5151),
        ("cordillera central", 2000, -3000),
    ] {
        // 1. Sólo el nivel, una vez por celda (el mejor caso posible).
        let t = Instant::now();
        let mut acc = 0i64;
        for z in z0..z0 + lado {
            for x in x0..x0 + lado {
                acc += relief::relief_level(b, x, z) as i64;
            }
        }
        let nivel = t.elapsed().as_secs_f64() * 1e6 / (lado * lado) as f64;
        // 2. El campo crudo (sin caché útil: se pide en orden distinto).
        let t = Instant::now();
        let mut f = 0.0f32;
        for z in z0..z0 + lado {
            for x in x0..x0 + lado {
                f += relief::relief_field(b, x, z);
            }
        }
        let campo = t.elapsed().as_secs_f64() * 1e6 / (lado * lado) as f64;
        // 3. La celda entera.
        let t = Instant::now();
        let mut n = 0u32;
        for z in z0..z0 + lado {
            for x in x0..x0 + lado {
                if relief::relief_cell(b, x, z) == ha_shared::relief::ReliefCell::Pared {
                    n += 1;
                }
            }
        }
        let celda = t.elapsed().as_secs_f64() * 1e6 / (lado * lado) as f64;
        // 4. El alto de pared suelto.
        let t = Instant::now();
        let mut a = 0i64;
        for z in z0..z0 + lado {
            for x in x0..x0 + lado {
                a += relief::alto_de_pared(b, x, z);
            }
        }
        let alto = t.elapsed().as_secs_f64() * 1e6 / (lado * lado) as f64;
        println!(
            "{nombre:<22} relief_level {nivel:6.3} us | relief_field {campo:6.3} us | \
             alto_de_pared {alto:7.3} us | relief_cell {celda:7.3} us   (pared {n}, acc {acc}, f {f:.0})"
        );
    }
}

#[test]
#[ignore]
fn el_relieve_cuesta_lo_mismo_en_todas_partes() {
    println!("--- COSTE DE relief_cell ---");
    // El llano del spawn.
    mide("llano del spawn", -64, -64, 128);
    // La ladera donde el driver planta la primera captura.
    mide("ladera de la sierra", 1763 - 64, -5087 - 64, 128);
    // La rampa.
    mide("paso de la sierra", 1796 - 64, -5097 - 64, 128);
    // La cumbre del Monte Ceniza, donde la captura salió vacía.
    mide("cumbre Monte Ceniza", 15599 - 64, 9010 - 64, 128);
    // Un trozo de cordillera central.
    mide("cordillera central", 2000, -3000, 128);
}

/// **QUÉ PARTE DEL CAMPO DE ALTURA ES LA CARA.** Con la caché arreglada, lo que
/// queda es el coste EN FRÍO: la primera vez que se toca una celda hay que
/// calcularle el campo, y eso es plantilla + ruido + río. Este reparto dice cuál.
#[test]
#[ignore]
fn reparto_del_campo() {
    use ha_shared::celestial_bodies::body_terrain_seed;
    let b = CelestialBody::Earth;
    let lado = 200i64;
    for (nombre, x0, z0) in [
        ("llano del spawn", -100i64, -100i64),
        ("cordillera central", 2000, -3000),
        ("mar abierto", 25000, 0),
    ] {
        let t = Instant::now();
        let mut acc = 0.0f32;
        for z in z0..z0 + lado {
            for x in x0..x0 + lado {
                acc += ha_shared::world_map::platform(b, x, z);
            }
        }
        let plataforma = t.elapsed().as_secs_f64() * 1e6 / (lado * lado) as f64;

        // El ruido de las cuatro octavas se mide por su primo público: `moisture_at`
        // usa la misma familia de fbm y sirve de patrón de coste.
        let t = Instant::now();
        let _ = body_terrain_seed(b);
        let mut acc2 = 0.0f32;
        for z in z0..z0 + lado {
            for x in x0..x0 + lado {
                acc2 += ha_shared::geography::moisture_at(b, x, z);
            }
        }
        let ruido = t.elapsed().as_secs_f64() * 1e6 / (lado * lado) as f64;

        let t = Instant::now();
        let mut n = 0u32;
        for z in z0..z0 + lado {
            for x in x0..x0 + lado {
                if ha_shared::rios::perfil_del_cauce(b, x, z).is_some() {
                    n += 1;
                }
            }
        }
        let rio = t.elapsed().as_secs_f64() * 1e6 / (lado * lado) as f64;

        println!(
            "{nombre:<22} plataforma {plataforma:7.3} us | ruido(4 octavas) {ruido:7.3} us | \
             rio {rio:7.3} us   (acc {acc:.0}/{acc2:.0}, cauce {n})"
        );
    }
}

/// Cuántos tramos mira de verdad una celda. Si la rejilla no bajó el coste, o
/// las listas siguen largas o lo caro no es el bucle.
#[test]
#[ignore]
fn cuantos_tramos_mira_una_celda() {
    let b = CelestialBody::Earth;
    // El bloque de río es privado, así que se mide por diferencia: el coste de
    // `perfil_del_cauce` contra el de una consulta que sólo toca el memo.
    for (nombre, x0, z0) in [("llano del spawn", -100i64, -100i64), ("cordillera", 2000i64, -3000i64)] {
        let lado = 200i64;
        // Mismo bloque una y otra vez: aísla el bucle de candidatos del armado.
        let t = Instant::now();
        let mut n = 0u32;
        for _ in 0..5 {
            for z in z0..z0 + lado {
                for x in x0..x0 + lado {
                    if ha_shared::rios::perfil_del_cauce(b, x, z).is_some() {
                        n += 1;
                    }
                }
            }
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / (5 * lado * lado) as f64;
        println!("{nombre:<20} perfil_del_cauce repetido: {us:.3} us/celda (cauce {})", n / 5);
    }
}

/// **EL OTRO CAMINO CALIENTE: `biome_at`.** El cliente lo pregunta por celda
/// para elegir la losa del suelo, el material de la pared y la densidad de
/// vegetación — varias veces por celda. Si cuesta como costaba `relief_cell`,
/// el arreglo de la caché de niveles no alcanza.
#[test]
#[ignore]
fn cuanto_cuesta_el_bioma() {
    let b = CelestialBody::Earth;
    let lado = 128i64;
    for (nombre, x0, z0) in [
        ("llano del spawn", -64i64, -64i64),
        ("ladera de la sierra", 1699, -5151),
        ("cumbre Monte Ceniza", 15535, 8946),
    ] {
        // Caliente: los niveles ya están, como cuando el cliente rearma un chunk.
        for z in z0..z0 + lado {
            for x in x0..x0 + lado {
                let _ = relief::relief_level(b, x, z);
            }
        }
        let t = Instant::now();
        let mut n = 0u32;
        for z in z0..z0 + lado {
            for x in x0..x0 + lado {
                if ha_shared::geography::biome_at(b, x, z) == ha_shared::geography::Biome::Grassland
                {
                    n += 1;
                }
            }
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / (lado * lado) as f64;
        let t = Instant::now();
        let mut m = 0u32;
        for z in z0..z0 + lado {
            for x in x0..x0 + lado {
                if ha_shared::geography::biome_de_terraza(b, x, z)
                    == ha_shared::geography::Biome::Grassland
                {
                    m += 1;
                }
            }
        }
        let us2 = t.elapsed().as_secs_f64() * 1e6 / (lado * lado) as f64;
        println!(
            "{nombre:<22} biome_at {us:7.3} us/celda | biome_de_terraza {us2:7.3} us/celda \
             (pradera {n}/{m})  ≈ {:5.1} ms por chunk de 32x32",
            us * 1024.0 / 1000.0
        );
    }
}
