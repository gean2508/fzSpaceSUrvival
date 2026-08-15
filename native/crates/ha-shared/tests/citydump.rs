//! **Volcado ASCII de una ciudad**, para mirar la trama con los ojos.
//!
//! No es un test: no afirma nada. Es la herramienta con la que se encontraron las
//! veredas de un solo lado, las calles que cambiaban de ancho a media cuadra, la
//! ciudad cortada en la frontera de su celda de asentamiento y los kilómetros de
//! puente sobre el océano — cuatro fallos que la suite entera daba por buenos
//! porque ninguno rompía un invariante que a alguien se le hubiera ocurrido
//! escribir. Mirar el mapa los delató en un minuto.
//!
//! ```sh
//! cargo test -p ha-shared --test citydump -- --ignored --nocapture
//! HA_DUMP=/tmp/ciudad.txt cargo test -p ha-shared --test citydump -- --ignored
//! ```

use ha_shared::celestial_bodies::CelestialBody;
use ha_shared::geography::{civil_at, Civil};
use ha_shared::topdown::{topdown_tile, Tile};

/// `#` calzada · `=` puente · `:` vereda · `M` pared · `o` ventana · `D` puerta
/// `.` piso · `+` cerca · `,` losa · `b` asfalto de parcela · `~` agua · `^` acantilado
fn glifo(body: CelestialBody, tx: i64, tz: i64) -> char {
    match civil_at(body, tx, tz) {
        Some(Civil::Road) => '#',
        Some(Civil::Bridge) => '=',
        Some(Civil::Pavement) => ':',
        None => match topdown_tile(body, 0, tx, tz) {
            Tile::WoodWall | Tile::StoneBrick => 'M',
            Tile::Window => 'o',
            Tile::Door => 'D',
            Tile::WoodFloor => '.',
            Tile::Fence => '+',
            Tile::Pavement => ',',
            Tile::Basalt => 'b',
            Tile::Water | Tile::WaterDeep => '~',
            Tile::Cliff => '^',
            t if t.solid() => 'X',
            _ => ' ',
        },
    }
}

#[test]
#[ignore = "herramienta de inspección, no un test"]
fn volcar_ciudades() {
    let body = CelestialBody::Earth;
    let mut out = String::new();
    for t in ha_shared::world_map::template(body).towns {
        let (cx, cz) = ha_shared::world_map::town_center(body, t);
        let r = t.radius;
        out.push_str(&format!(
            "\n===== {}  {:?}  centro=({cx},{cz}) r={r}\n",
            t.name, t.style
        ));
        for (etiqueta, ox, oz) in [
            ("CENTRO", cx - 60, cz - 35),
            ("BORDE E", cx + r - 90, cz - 35),
            ("BORDE N", cx - 60, cz - r - 10),
        ] {
            out.push_str(&format!("--- {etiqueta}  esquina=({ox},{oz})\n"));
            for z in 0..70 {
                out.push_str(&format!("{:7} ", oz + z));
                for x in 0..120 {
                    out.push(glifo(body, ox + x, oz + z));
                }
                out.push('\n');
            }
        }
    }
    match std::env::var("HA_DUMP") {
        Ok(ruta) => {
            std::fs::write(&ruta, out).unwrap();
            println!("volcado en {ruta}");
        }
        Err(_) => println!("{out}"),
    }
}

/// **Volcado de CERCAS: qué pose pide cada celda del perímetro de una parcela.**
///
/// Misma idea que [`volcar_ciudades`] y por el mismo motivo. El bug de la valla
/// horizontal en los tramos verticales no lo cazaba ningún invariante: había que
/// mirar. Esto imprime, celda por celda, la forma que la cerca le pide al arte,
/// y lo escribe en TSV para poder componer la parcela con los recortes de verdad
/// (`scripts/ver-cercas.py`) sin levantar el juego.
///
/// ```sh
/// HA_DUMP=/tmp/cercas.tsv cargo test -p ha-shared --test citydump -- --ignored volcar_cercas
/// ```
#[test]
#[ignore = "herramienta de inspección, no un test"]
fn volcar_cercas() {
    use ha_shared::cercas::{cadena, Vecinos};
    use ha_shared::city::afuera_de_cerca;
    use ha_shared::geography::settlement_covering;

    let body = CelestialBody::Earth;
    let t = &ha_shared::world_map::template(body).towns[0];
    let (cx, cz) = ha_shared::world_map::town_center(body, t);
    // Lejos del centro: en Downtown no hay parcelas con cerca. Ver el test
    // `una_cerca_vertical_no_usa_pieza_horizontal`, que aprendió lo mismo.
    let (ox, oz) = (cx + 380, cz + 380);
    // La CADENA entera, no sólo la primera: el juego prueba en orden hasta dar
    // con una pose curada, y quien mire el volcado tiene que poder hacer lo mismo
    // o va a ver agujeros donde el juego dibuja bien.
    let mut out = String::from("tx\ttz\tforma\tcadena\n");
    let mut mapa = String::new();
    for z in 0..90 {
        for x in 0..120 {
            let (tx, tz) = (ox + x, oz + z);
            if topdown_tile(body, 0, tx, tz) != Tile::Fence {
                mapa.push(glifo(body, tx, tz));
                continue;
            }
            let cerca = |dx: i64, dz: i64| {
                topdown_tile(body, 0, tx + dx, tz + dz) == Tile::Fence
            };
            let v = Vecinos::new(cerca(0, -1), cerca(1, 0), cerca(0, 1), cerca(-1, 0));
            let afuera =
                settlement_covering(body, tx, tz).and_then(|s| afuera_de_cerca(body, &s, tx, tz));
            let poses = cadena(v, afuera).join("|");
            // `|` tramo vertical · `-` horizontal · `+` codo o te
            mapa.push(match v.mascara() {
                "ns" => '|',
                "ew" => '-',
                _ => '+',
            });
            out.push_str(&format!("{tx}\t{tz}\t{}\t{poses}\n", v.mascara()));
        }
        mapa.push('\n');
    }
    println!("{mapa}");
    if let Ok(ruta) = std::env::var("HA_DUMP") {
        std::fs::write(&ruta, out).unwrap();
        println!("volcado en {ruta}");
    }
}
