//! Medidor: dibuja en texto el plano de una casa de la ciudad (`cargo test -p ha-shared --test ver_casa -- --ignored --nocapture`).
use ha_shared::{city, geography, topdown, CelestialBody};

#[test]
#[ignore]
fn plano_de_casas() {
    let b = CelestialBody::Earth;
    let mut n = 0;
    'o: for sz in -3..3 {
        for sx in -3..3 {
            let Some(s) = geography::settlement_in(b, sx, sz) else { continue };
            let r = s.radius.min(500);
            for dz in (-r..r).step_by(7) {
                for dx in (-r..r).step_by(7) {
                    let Some(l) = city::lot_at(b, &s, s.center.0 + dx, s.center.1 + dz) else { continue };
                    if l.program().district != city::District::Residential || l.min.0 % 1 != 0 {
                        continue;
                    }
                    let mut tiene = false;
                    let mut txt = String::new();
                    for z in l.min.1..l.min.1 + l.h {
                        for x in l.min.0..l.min.0 + l.w {
                            let c = match topdown::topdown_tile(b, 0, x, z) {
                                topdown::Tile::WoodWall => { tiene = true; '#' }
                                topdown::Tile::WoodFloor => '.',
                                topdown::Tile::Window => 'W',
                                topdown::Tile::Door => 'D',
                                topdown::Tile::Fence => 'f',
                                topdown::Tile::Water => '~',
                                topdown::Tile::Pavement => 'p',
                                _ => ' ',
                            };
                            txt.push(c);
                        }
                        txt.push('\n');
                    }
                    if tiene && n < 4 && l.min.0 % 3 == 0 {
                        println!("{} {:?}\n{}", l.program().key, l.min, txt);
                        n += 1;
                        if n >= 4 { break 'o; }
                    }
                }
            }
        }
    }
}
