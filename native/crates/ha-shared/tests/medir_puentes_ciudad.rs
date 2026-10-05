//! Cuenta los puentes que cruzan agua dentro de los cascos urbanos (canales y ríos angostos).
use ha_shared::celestial_bodies::CelestialBody;
use ha_shared::geography::{civil_at, settlement_in, Civil, SettlementTier};

#[test]
#[ignore]
fn puentes_dentro_de_las_ciudades() {
    let body = CelestialBody::Earth;
    for sx in -12..12 {
        for sz in -12..12 {
            let Some(s) = settlement_in(body, sx, sz) else { continue };
            if s.tier != SettlementTier::City {
                continue;
            }
            let (mut n, mut primero) = (0, None);
            let r = s.radius;
            for dz in (-r..=r).step_by(2) {
                for dx in (-r..=r).step_by(2) {
                    let (x, z) = (s.center.0 + dx, s.center.1 + dz);
                    if civil_at(body, x, z) == Some(Civil::Bridge) {
                        n += 1;
                        primero.get_or_insert((x, z));
                    }
                }
            }
            println!("ciudad {:?} {:?}: {n} celdas de puente, primero en {:?}", s.name, s.center, primero);
        }
    }
}

#[test]
#[ignore]
fn una_celda_de_puente() {
    use ha_shared::geography::{civil_kind_at, natural_tile};
    let body = CelestialBody::Earth;
    for (x, z) in [(-16666i64, -9597i64), (-16666, -9600), (-16666, -9594)] {
        println!("({x},{z}) civil={:?} kind={:?} nat={:?}", civil_at(body, x, z), civil_kind_at(body, x, z), natural_tile(body, x, z));
    }
}
