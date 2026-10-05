//! Mide cuántas ciudades tienen línea de tren, cuánto cuesta calcularla y que las vías queden sobre suelo llano.
use ha_shared::celestial_bodies::CelestialBody;
use ha_shared::rail::{celda_firme, lineas_cerca, riel_en};
use std::time::Instant;

#[test]
#[ignore = "medición: cuesta unos segundos"]
fn medir_lineas() {
    let body = CelestialBody::Earth;
    let t = Instant::now();
    let v = lineas_cerca(body, 0, 0, 9);
    println!("lineas={} en {:?}", v.len(), t.elapsed());
    for l in &v {
        println!("  id={} ciudad={:?} r={} z={} paradas={:?} largo={}", l.id, l.ciudad, l.radio, l.z, l.paradas, l.largo());
        // Toda celda de vía sobre suelo firme.
        let mut mal = 0;
        for x in (l.x_oeste() - 24)..=(l.x_este() + 24) {
            for via in 0..2 {
                let z = l.fila_via(via);
                if riel_en(body, &v, x, z).is_none() || !celda_firme(body, x, z) {
                    mal += 1;
                }
            }
        }
        println!("    celdas de vía mal: {mal}");
        assert_eq!(mal, 0);
    }
}

#[test]
#[ignore = "diagnóstico"]
fn diag_ciudades() {
    use ha_shared::geography::{settlement_in, SettlementTier};
    let body = CelestialBody::Earth;
    let mut v = vec![];
    for sx in -30..30 { for sz in -30..30 {
        if let Some(s) = settlement_in(body, sx, sz) { if s.tier == SettlementTier::City { v.push(s); } }
    } }
    println!("ciudades={}", v.len());
    for a in &v {
        let este: Vec<_> = v.iter().filter(|b| b.center.0 > a.center.0 && b.center.0 - a.center.0 < 14_000 && (b.center.1 - a.center.1).abs() < 14_000).map(|b| (b.center.0 - a.center.0, b.center.1 - a.center.1)).collect();
        println!("{:?} r={} este={:?}", a.center, a.radius, este);
    }
}

#[test]
#[ignore = "diagnóstico"]
fn diag_corredor() {
    use ha_shared::geography::{settlement_in, SettlementTier};
    use ha_shared::topdown::topdown_tile;
    let body = CelestialBody::Earth;
    for (sx, sz) in [(-3, -1), (-2, 1), (3, -3)] {
        let Some(s) = settlement_in(body, sx, sz) else { continue };
        if s.tier != SettlementTier::City { continue; }
        let (cx, cz) = s.center;
        let (x0, x1) = (cx - s.radius - 500, cx + s.radius + 500);
        println!("ciudad {:?} r={}", s.center, s.radius);
        for lado in [1i64, -1] {
            for k in [0i64, 5, 10, 20, 30] {
                let z = if lado > 0 { cz + s.radius + 70 + k * 6 } else { cz - s.radius - 70 - k * 6 - 6 };
                let mut mal = 0; let mut primero = None; let mut tipos = std::collections::BTreeMap::new();
                for x in x0..=x1 {
                    let t = topdown_tile(body, 0, x, z);
                    if !ha_shared::rail::celda_firme(body, x, z) { mal += 1; if primero.is_none() { primero = Some((x, format!("{t:?}"))); } *tipos.entry(format!("{t:?}")).or_insert(0) += 1; }
                }
                println!("  lado={lado} k={k} z={z} malas={mal}/{} primero={primero:?} tipos={tipos:?}", x1 - x0 + 1);
            }
        }
    }
}
