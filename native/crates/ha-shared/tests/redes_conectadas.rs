//! Ningún núcleo con otro cerca queda sin carretera (medido: antes 8 de 64 aislados).
use ha_shared::celestial_bodies::CelestialBody;
use ha_shared::geography::{link_clearance, settlement_in};

#[test]
fn todo_nucleo_con_vecino_cercano_tiene_carretera() {
    let body = CelestialBody::Earth;
    let (mut total, mut sin_camino) = (0, vec![]);
    for x in -40..=40i64 {
        for z in -40..=40i64 {
            let Some(s) = settlement_in(body, x, z) else { continue };
            let hay_vecino = (-3..=3i64).any(|dx| {
                (-3..=3i64).any(|dz| (dx, dz) != (0, 0) && settlement_in(body, x + dx, z + dz).is_some())
            });
            if !hay_vecino { continue; }
            total += 1;
            let m = link_clearance(body, s.center.0 as f32, s.center.1 as f32);
            if !matches!(m, Some(v) if v >= 0.0) { sin_camino.push((x, z)); }
        }
    }
    assert!(total > 10, "muestra chica: {total}");
    assert!(sin_camino.is_empty(), "sin carretera: {sin_camino:?}");
}
