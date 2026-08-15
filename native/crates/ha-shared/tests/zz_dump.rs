use ha_shared::relief::{relief_cell, relief_level, ReliefCell};
use ha_shared::CelestialBody;
#[test]
fn dump_relieve() {
    let b = CelestialBody::Earth;
    for tz in 3100..3120 {
        let mut s = String::new();
        for tx in 1360..1400 {
            s.push(match relief_cell(b, tx, tz) {
                ReliefCell::Cliff => 'M',
                ReliefCell::Pared => 'm',
                ReliefCell::Ramp => 'r',
                ReliefCell::Flat => '.',
            });
        }
        let mut n = String::new();
        for tx in 1360..1400 {
            n.push(char::from_digit((relief_level(b, tx, tz).max(0) as u32) % 10, 10).unwrap());
        }
        println!("{tz} {s}  {n}");
    }
}
