//! **Los biomas del subsuelo.** Una sola capa bajo tierra (el nivel de las cuevas, `cuevas::NIVEL`) con
//! regiones de carácter propio, como las zonas de Core Keeper: lo que se encuentra al cavar depende de
//! DÓNDE se cava, y se nota a simple vista (el color de la roca) antes de picar.
//!
//! * El mapa es una **grilla de regiones** de [`REGION`] celdas con un punto sorteado en cada una; la
//!   celda pertenece al punto más cercano (diagrama de Voronoi). Sale de la semilla del cuerpo: dos
//!   mundos reparten distinto, y la respuesta a "¿qué bioma es esta celda?" cuesta nueve distancias.
//! * Cada bioma cambia **qué vetas hay** (`deposit_roll`), **qué suelta la roca** (`materias::extra_al_picar`)
//!   y **cómo se ve** (`tiles/subsuelo_<bioma>` y el tinte del piso de la galería).
//! * Sólo la Tierra tiene biomas; el subsuelo de los otros cuerpos es de un solo tipo.
//!
//! Función pura y determinista: no toca el estado del mundo, así que sirve igual al generar, dibujar y picar.

use crate::celestial_bodies::CelestialBody;
use crate::worldgen::hash_cell;

/// Lado de una región, en celdas. Con 72 un bioma se cruza en un par de minutos a pie: suficiente para
/// que tenga carácter y no tanto como para que toda la cueva sea de lo mismo.
pub const REGION: i64 = 72;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Bioma {
    /// El macizo gris de siempre: carbón, cobre y estaño.
    Piedra,
    /// Arcilla roja: la fuente de barro, adobe y cerámica.
    Arcilla,
    /// Cavernas de cristal: gemas, sal y brillos.
    Cristal,
    /// Gruta de musgo: caliza y humedad.
    Musgo,
    /// Veta de hierro: mena a montones.
    Hierro,
}

pub const BIOMAS: [Bioma; 5] = [Bioma::Piedra, Bioma::Arcilla, Bioma::Cristal, Bioma::Musgo, Bioma::Hierro];

impl Bioma {
    /// El nombre que ve el jugador.
    pub fn nombre(self) -> &'static str {
        match self {
            Bioma::Piedra => "Cueva de piedra",
            Bioma::Arcilla => "Arcillar",
            Bioma::Cristal => "Cavernas de cristal",
            Bioma::Musgo => "Gruta de musgo",
            Bioma::Hierro => "Veta de hierro",
        }
    }

    /// Sufijo de la clave de arte del macizo: `tiles/subsuelo` (piedra) o `tiles/subsuelo_<sufijo>`.
    pub fn clave_de_macizo(self) -> &'static str {
        match self {
            Bioma::Piedra => "tiles/subsuelo",
            Bioma::Arcilla => "tiles/subsuelo_arcilla",
            Bioma::Cristal => "tiles/subsuelo_cristal",
            Bioma::Musgo => "tiles/subsuelo_musgo",
            Bioma::Hierro => "tiles/subsuelo_hierro",
        }
    }

    /// El color del aire de la cueva (LINEAL, el mismo espacio que el velo nocturno): en la oscuridad
    /// casi cerrada que hay bajo tierra, lo único que dice en qué bioma estás es el matiz del fondo.
    pub fn velo(self) -> [f32; 3] {
        match self {
            Bioma::Piedra => [0.0008, 0.0038, 0.0034],
            Bioma::Arcilla => [0.0046, 0.0021, 0.0015],
            Bioma::Cristal => [0.0042, 0.0016, 0.0092],
            Bioma::Musgo => [0.0010, 0.0058, 0.0016],
            Bioma::Hierro => [0.0058, 0.0032, 0.0010],
        }
    }

    /// Tinte (multiplica el piso de la galería, que es una sola losa del pack): el aire del bioma.
    pub fn tinte_del_piso(self) -> [f32; 3] {
        match self {
            Bioma::Piedra => [1.0, 1.0, 1.0],
            Bioma::Arcilla => [1.12, 0.92, 0.88],
            Bioma::Cristal => [0.94, 0.92, 1.14],
            Bioma::Musgo => [0.90, 1.08, 0.92],
            Bioma::Hierro => [1.08, 0.96, 0.90],
        }
    }
}

/// El punto de la región `(rx, rz)`: sorteado dentro de ella, estable por semilla.
fn punto_de(semilla: u32, rx: i64, rz: i64) -> (i64, i64) {
    let h = hash_cell(semilla ^ 0x5B_10_3E, rx, rz);
    let ox = (h % (REGION as u32 - 12)) as i64 + 6;
    let oz = ((h >> 12) % (REGION as u32 - 12)) as i64 + 6;
    (rx * REGION + ox, rz * REGION + oz)
}

/// El bioma de una región (sale del punto, no de la celda: toda la región vecina del punto lo comparte).
fn bioma_de_punto(semilla: u32, rx: i64, rz: i64) -> Bioma {
    // Reparto: piedra 38 %, arcilla 20, hierro 18, musgo 14, cristal 10 (el cristal es lo raro).
    match hash_cell(semilla ^ 0xB1_03_A5, rx, rz) % 100 {
        0..=37 => Bioma::Piedra,
        38..=57 => Bioma::Arcilla,
        58..=75 => Bioma::Hierro,
        76..=89 => Bioma::Musgo,
        _ => Bioma::Cristal,
    }
}

/// **El bioma del subsuelo en la celda `(tx, tz)`.**
pub fn bioma_en(body: CelestialBody, tx: i64, tz: i64) -> Bioma {
    if body != CelestialBody::Earth {
        return Bioma::Piedra;
    }
    let semilla = crate::celestial_bodies::body_terrain_seed(body);
    let (rx, rz) = (tx.div_euclid(REGION), tz.div_euclid(REGION));
    let mut mejor = (i64::MAX, rx, rz);
    for dz in -1..=1 {
        for dx in -1..=1 {
            let (px, pz) = punto_de(semilla, rx + dx, rz + dz);
            let d = (px - tx) * (px - tx) + (pz - tz) * (pz - tz);
            if d < mejor.0 {
                mejor = (d, rx + dx, rz + dz);
            }
        }
    }
    bioma_de_punto(semilla, mejor.1, mejor.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn es_determinista_y_solo_la_tierra_tiene_biomas() {
        let b = CelestialBody::Earth;
        assert_eq!(bioma_en(b, 123, -456), bioma_en(b, 123, -456));
        assert_eq!(bioma_en(CelestialBody::Moon, 123, -456), Bioma::Piedra);
    }

    #[test]
    fn hay_de_todo_y_ninguno_domina() {
        let b = CelestialBody::Earth;
        let mut c: BTreeMap<Bioma, usize> = BTreeMap::new();
        for tx in (-1500..1500).step_by(11) {
            for tz in (-1500..1500).step_by(11) {
                *c.entry(bioma_en(b, tx, tz)).or_default() += 1;
            }
        }
        let total: usize = c.values().sum();
        for bi in BIOMAS {
            let f = *c.get(&bi).unwrap_or(&0) as f32 / total as f32;
            assert!(f > 0.04, "{bi:?} casi no existe: {f:.3}");
            assert!(f < 0.62, "{bi:?} domina el subsuelo: {f:.3}");
        }
    }

    #[test]
    fn los_biomas_son_regiones_y_no_ruido() {
        // Dos celdas vecinas casi siempre son del mismo bioma: es un mapa de zonas, no sal y pimienta.
        let b = CelestialBody::Earth;
        let (mut iguales, mut n) = (0, 0);
        for tx in (-600..600).step_by(3) {
            for tz in (-600..600).step_by(7) {
                n += 1;
                if bioma_en(b, tx, tz) == bioma_en(b, tx + 1, tz) {
                    iguales += 1;
                }
            }
        }
        assert!(iguales as f32 / n as f32 > 0.95, "{iguales}/{n}");
    }

    #[test]
    fn cada_bioma_tiene_clave_nombre_y_tinte() {
        for bi in BIOMAS {
            assert!(bi.clave_de_macizo().starts_with("tiles/subsuelo"));
            assert!(!bi.nombre().is_empty());
            assert!(bi.tinte_del_piso().iter().all(|c| (0.7..1.3).contains(c)));
            assert!(bi.velo().iter().all(|c| (0.0..0.02).contains(c)), "un velo tan claro deja de ser cueva");
        }
    }
}
