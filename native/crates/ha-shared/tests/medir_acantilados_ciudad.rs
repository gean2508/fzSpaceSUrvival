//! Mide cuánto acantilado cae DENTRO del casco de las ciudades (pedido del usuario: «acantilados tapando calles»).
use ha_shared::celestial_bodies::CelestialBody;
use ha_shared::geography::{settlement_in, SettlementTier};
use ha_shared::relief::{relief_cell, ReliefCell};

#[test]
#[ignore]
fn acantilados_dentro_de_las_ciudades() {
    let body = CelestialBody::Earth;
    let (mut ciudades, mut celdas, mut malas) = (0, 0u64, 0u64);
    for sx in -12..12 {
        for sz in -12..12 {
            let Some(s) = settlement_in(body, sx, sz) else { continue };
            if s.tier != SettlementTier::City {
                continue;
            }
            ciudades += 1;
            let (mut c, mut m) = (0u64, 0u64);
            let r = s.radius;
            for dz in (-r..=r).step_by(3) {
                for dx in (-r..=r).step_by(3) {
                    let (x, z) = (s.center.0 + dx, s.center.1 + dz);
                    c += 1;
                    if !matches!(relief_cell(body, x, z), ReliefCell::Flat) {
                        m += 1;
                        if m % 40 == 1 {
                            println!("   relieve en ({x}, {z}) a {} del centro; tile {:?}", dx.abs().max(dz.abs()), ha_shared::topdown::Tile::Grass);
                        }
                    }
                }
            }
            println!("ciudad {:?} en {:?} r={} : {m} de {c} celdas con relieve ({:.1} %)", s.name, s.center, r, m as f64 * 100.0 / c as f64);
            celdas += c;
            malas += m;
        }
    }
    println!("TOTAL {ciudades} ciudades: {malas} de {celdas} ({:.2} %)", malas as f64 * 100.0 / celdas.max(1) as f64);
}
