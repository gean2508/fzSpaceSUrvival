//! Mide cuánta obra (calzada, vereda, cerca) cae bajo el DIBUJO de un muro de acantilado en los cascos.
//! `cargo test -p ha-shared --release --test obra_bajo_muros -- --ignored --nocapture` (≈10 min).
use ha_shared::celestial_bodies::CelestialBody;
use ha_shared::geography::natural_tile;
use ha_shared::relief::{alto_de_pared, relief_cell, relief_level, ReliefCell};
use ha_shared::topdown::{topdown_tile, Tile};
use std::collections::BTreeMap;

fn huella(b: CelestialBody, tx: i64, tz: i64) -> Vec<(i64, i64)> {
    // celdas que dibuja una cara que cuelga al sur (canto + cuerpos + pie), más el propio canto
    let n = relief_level(b, tx, tz);
    let mut v = vec![(tx, tz)];
    if relief_level(b, tx, tz + 1) < n {
        let a = alto_de_pared(b, tx, tz).max(1);
        for j in 1..=a + 1 {
            v.push((tx, tz + j));
        }
    }
    v
}

#[test]
#[ignore]
fn obra_civil_bajo_el_dibujo_de_un_muro() {
    let b = CelestialBody::Earth;
    let mut tot = 0u32;
    let mut por_tile: BTreeMap<String, u32> = BTreeMap::new();
    let mut ejemplos: Vec<(i64, i64, String)> = Vec::new();
    let mut ventanas: Vec<(i64, i64, i64)> = Vec::new();
    for sx in -14..14 {
        for sz in -14..14 {
            if let Some(s) = ha_shared::geography::settlement_in(b, sx, sz) {
                ventanas.push((s.center.0, s.center.1, s.radius + 60));
            }
        }
    }
    println!("VENT nucleos {}", ventanas.len());
    for (cx, cz, r) in ventanas {
        for tz in cz - r..cz + r {
            for tx in cx - r..cx + r {
                if relief_cell(b, tx, tz) != ReliefCell::Cliff || topdown_tile(b, 0, tx, tz) != Tile::Cliff { continue; }
                for (x, z) in huella(b, tx, tz) {
                    let t = topdown_tile(b, 0, x, z);
                    let nat = natural_tile(b, x, z);
                    if (x, z) != (tx, tz) && t != nat && !matches!(t, Tile::Cliff | Tile::Slope) {
                        tot += 1;
                        *por_tile.entry(format!("{t:?}")).or_default() += 1;
                        if ejemplos.len() < 400 { ejemplos.push((x, z, format!("{t:?} sobre cara en ({tx},{tz})"))); }
                    }
                }
            }
        }
    }
    println!("INV total {tot}");
    for (k, v) in por_tile { println!("INV {k} {v}"); }
    for e in ejemplos { println!("EJ {:?}", e); }
}
