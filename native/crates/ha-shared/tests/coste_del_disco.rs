//! **CUÁNTO CUESTA HORNEAR EL DISCO DEL PLANETA, MEDIDO DE VERDAD.**
//!
//! `universe::bake_body_textures` reportó (`HA_STAGES=1`, celu real, Tierra,
//! parado en pasto) 300-600 ms/frame SOSTENIDOS durante más de veinte segundos,
//! con `stream_tile_chunks` en 1-2 ms al mismo tiempo. La sospecha: hay varias
//! cachés pensadas para el puñado de sitios que un chunk local visita una y otra
//! vez (`geography::SITIOS`, 64 ranuras; `rios::MEMO_BLOQUES`, 16), y el horneado
//! del disco muestrea el PLANETA ENTERO (`fill_disc_rows`, 512×512 píxeles × 4
//! tomas) — un patrón de acceso disperso que no repite celda, así que cada toma
//! sería un fallo de caché.
//!
//! ```sh
//! cargo test -p ha-shared --release --test coste_del_disco -- --ignored --nocapture
//! ```

use ha_shared::geography::{civil_at, natural_tile};
use ha_shared::topdown::topdown_tile;
use ha_shared::CelestialBody;
use std::time::Instant;

const RADIO_CELDAS: i64 = 15_000; // ~30 km / 2 m por celda, mismo orden que body_walk_radius/TILE_M.

fn mide<F: FnMut(i64, i64)>(nombre: &str, local: bool, mut f: F) {
    let t = Instant::now();
    let mut n = 0u64;
    if local {
        for _ in 0..4 {
            for z in -64..64 {
                for x in -64..64 {
                    f(x, z);
                    n += 1;
                }
            }
        }
    } else {
        // El patrón de `fill_disc_rows`: filas completas a lo ancho del disco.
        let n_tex = 512i64;
        for py in 0..n_tex {
            let v = (py as f32 + 0.5) / n_tex as f32 - 0.5;
            for px in 0..n_tex {
                let u = (px as f32 + 0.5) / n_tex as f32 - 0.5;
                if (u * u + v * v).sqrt() > 0.5 {
                    continue;
                }
                let tx = (u * 2.0 * RADIO_CELDAS as f32) as i64;
                let tz = (-v * 2.0 * RADIO_CELDAS as f32) as i64;
                f(tx, tz);
                n += 1;
            }
        }
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "{nombre:<42} {n:>8} celdas en {ms:9.2} ms  = {:8.3} us/celda",
        ms * 1000.0 / n as f64
    );
}

#[test]
#[ignore]
fn desglose_del_costo_por_funcion() {
    let b = CelestialBody::Earth;
    mide("civil_at         (local)", true, |x, z| {
        let _ = civil_at(b, x, z);
    });
    mide("civil_at         (disco)", false, |x, z| {
        let _ = civil_at(b, x, z);
    });
    mide("natural_tile     (local)", true, |x, z| {
        let _ = natural_tile(b, x, z);
    });
    mide("natural_tile     (disco)", false, |x, z| {
        let _ = natural_tile(b, x, z);
    });
    mide("topdown_tile     (local)", true, |x, z| {
        let _ = topdown_tile(b, 0, x, z);
    });
    mide("topdown_tile     (disco)", false, |x, z| {
        let _ = topdown_tile(b, 0, x, z);
    });
}
