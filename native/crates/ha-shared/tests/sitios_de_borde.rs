//! Lista coordenadas REALES de cada tipo de borde de acantilado para
//! fotografiarlas con `HA_VISTATEST="x,z;x,z"` (ver biblia §11).
//!
//! ```sh
//! cargo test -p ha-shared --test sitios_de_borde -- --ignored --nocapture
//! ```

use ha_shared::celestial_bodies::CelestialBody;
use ha_shared::relief::{alto_de_pared, relief_cell, relief_level, ReliefCell};
use ha_shared::topdown::{topdown_tile, Tile};

const VENTANAS: [(i64, i64); 6] = [
    (1_900, -4_900),
    (4_092, -4_960),
    (-3_450, 3_150),
    (0, 0),
    (2_400, -1_800),
    (1_500, 3_300),
];

fn natural(t: Tile) -> bool {
    matches!(
        t,
        Tile::Grass | Tile::Dirt | Tile::Sand | Tile::Water | Tile::WaterDeep | Tile::Regolith
            | Tile::Ice | Tile::Basalt | Tile::RockRed | Tile::Cliff | Tile::Slope | Tile::Rock
            | Tile::Stone | Tile::Subsoil | Tile::RockDark | Tile::Bedrock | Tile::CaveFloor
    )
}

#[test]
#[ignore]
fn sitios() {
    let b = CelestialBody::Earth;
    let lv = |x: i64, z: i64| relief_level(b, x, z);
    let mut hallados: Vec<(String, i64, i64)> = Vec::new();
    let mut cuenta = std::collections::HashMap::<String, u32>::new();
    let mut poner = |que: &str, x: i64, z: i64, max: u32, v: &mut Vec<(String, i64, i64)>| {
        let bio = format!("{:?}", ha_shared::geography::biome_at(b, x, z - 3));
        let que = format!("{que}-{bio}");
        if bio == "Snow" || bio == "Ocean" { return; }
        let c = cuenta.entry(que.clone()).or_insert(0);
        if *c < max {
            // Separados entre sí: no repetir el mismo tramo.
            if v.iter().any(|(q, a, c2)| *q == que && (a - x).abs() + (c2 - z).abs() < 60) {
                return;
            }
            *c += 1;
            v.push((que, x, z));
        }
    };
    for (cx, cz) in VENTANAS {
        for tz in cz - 150..cz + 150 {
            for tx in cx - 150..cx + 150 {
                let n = lv(tx, tz);
                if n <= 0 {
                    continue;
                }
                let baja = |dx: i64, dz: i64| {
                    let m = lv(tx + dx, tz + dz);
                    m >= 0 && m < n
                };
                let (s, e, w, nn) = (baja(0, 1), baja(1, 0), baja(-1, 0), baja(0, -1));
                if relief_cell(b, tx, tz) == ReliefCell::Cliff {
                    let alto = alto_de_pared(b, tx, tz);
                    if s && !e && !w && alto >= 3 {
                        poner("cara-sur-alta", tx, tz + alto.min(6) + 3, 3, &mut hallados);
                    }
                    if e && !s && !nn {
                        poner("cara-este", tx, tz + 3, 3, &mut hallados);
                    }
                    if w && !s && !nn {
                        poner("cara-oeste", tx, tz + 3, 3, &mut hallados);
                    }
                    if nn && !s && !e && !w {
                        poner("cara-norte", tx, tz + 3, 3, &mut hallados);
                    }
                    if s && e && alto >= 2 {
                        poner("esquina-se-alta", tx, tz + alto.min(6) + 3, 3, &mut hallados);
                    }
                    if s && w && alto >= 2 {
                        poner("esquina-sw-alta", tx, tz + alto.min(6) + 3, 3, &mut hallados);
                    }
                    if nn && e {
                        poner("esquina-ne", tx, tz + 3, 2, &mut hallados);
                    }
                    if nn && w {
                        poner("esquina-nw", tx, tz + 3, 2, &mut hallados);
                    }
                    if s && e && w {
                        poner("peninsula", tx, tz + alto.min(6) + 3, 2, &mut hallados);
                    }
                    // Algo edificado a menos de 4 celdas de un borde.
                    let mut cerca = false;
                    'r: for dz in -4..=4i64 {
                        for dx in -4..=4i64 {
                            if !natural(topdown_tile(b, 0, tx + dx, tz + dz)) {
                                cerca = true;
                                break 'r;
                            }
                        }
                    }
                    if cerca {
                        poner("junto-a-estructura", tx, tz + 2, 12, &mut hallados);
                    }
                } else if !(baja(1, 1) && false) && n > 0 {
                    // cóncava: ningún ortogonal baja, alguna diagonal sí
                    let d = |dx: i64, dz: i64| {
                        let m = lv(tx + dx, tz + dz);
                        m >= 0 && m < n
                    };
                    if d(1, 1) || d(-1, 1) || d(1, -1) || d(-1, -1) {
                        poner("concavo", tx, tz + 3, 3, &mut hallados);
                    }
                }
                if ha_shared::agua::cascada_at(b, tx, tz).is_some() {
                    poner("cascada", tx, tz + 5, 4, &mut hallados);
                }
            }
        }
    }
    for (q, x, z) in &hallados {
        println!("SITIO {q} {x},{z}");
    }
}
