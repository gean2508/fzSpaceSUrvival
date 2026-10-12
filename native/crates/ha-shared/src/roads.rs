//! **LA RED VIAL** — caminos por los que de verdad se puede conducir.
//!
//! Antes toda calzada medía lo mismo (4 a 9 tiles según el rango del núcleo) y no
//! había jerarquía: una aldea y una metrópolis se unían con la misma cinta. Peor:
//! con el relieve por terrazas, una carretera podía chocar de frente contra un
//! acantilado y quedarse ahí, porque nadie le había dicho al relieve que por
//! debajo pasaba un camino.
//!
//! Este módulo arregla las dos cosas:
//!
//! ```text
//!   ═══════════ autopista 10 + banquina    une metrópolis
//!   ═══════     ruta 7                     une pueblos
//!   ══════      avenida 8                  eje de ciudad, cada 4 manzanas
//!   ════        calle 5                    manzana normal
//!   ══          callejón 3                 fondo de manzana
//! ```
//!
//! Y **el camino gana sobre el acantilado**: donde pasa una calzada, el borde de
//! terraza se abre en rampa ancha (ver `relief::in_pass_band`, que consulta
//! `civil_at`). Por eso se puede cruzar la cordillera conduciendo: hay puertos de
//! montaña donde la carretera los pide, no donde cayó el ruido.
//!
//! ## Para qué tanto ancho
//!
//! Porque el mapa tiene que servir a todas las funciones del juego, no sólo a
//! caminar: vehículos, helicóptero y NPCs que circulan. Un carril de 4 tiles con
//! un auto de 3 no deja adelantar ni girar. [`lane_at`] devuelve carril y sentido
//! justamente para que un NPC conductor sepa por dónde ir sin que nadie tenga que
//! trazar rutas a mano.

use crate::celestial_bodies::CelestialBody;
use crate::geography::{hash2, CityStyle, Settlement, SettlementTier};

/// Rango de una vía. Decide el ancho, y el ancho decide qué cabe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoadClass {
    /// Une ciudades grandes. Dos carriles por sentido y banquina.
    Highway,
    /// Une pueblos y aldeas con la red.
    Route,
    /// Eje interno de una ciudad.
    Avenue,
    /// Calle de manzana.
    Street,
    /// Fondo de manzana / servicio.
    Alley,
}

impl RoadClass {
    /// Semiancho de la CALZADA, en tiles.
    pub fn half_width(self) -> f32 {
        match self {
            RoadClass::Highway => 5.0,
            RoadClass::Route => 3.5,
            RoadClass::Avenue => 4.0,
            RoadClass::Street => 2.5,
            RoadClass::Alley => 1.5,
        }
    }

    /// Semiancho incluyendo la BANQUINA (grava/acera). Es lo que se despeja de
    /// vegetación: un camino con el bosque pegado a la calzada no se conduce.
    pub fn half_shoulder(self) -> f32 {
        self.half_width() + match self {
            RoadClass::Highway => 3.0,
            RoadClass::Route => 2.0,
            RoadClass::Avenue => 2.5,
            RoadClass::Street => 1.5,
            RoadClass::Alley => 0.5,
        }
    }

    /// ¿Caben dos vehículos cruzándose? Es la definición operativa de
    /// "transitable", y la que comprueba el test de conectividad.
    pub fn drivable(self) -> bool {
        self.half_width() >= 2.5
    }
}

/// La vía que une dos núcleos, según el mayor de los dos. Una autopista sale de
/// unir dos ciudades; a una aldea le llega una ruta.
pub fn link_class(a: SettlementTier, b: SettlementTier) -> RoadClass {
    match (a, b) {
        (SettlementTier::City, SettlementTier::City) => RoadClass::Highway,
        (SettlementTier::City, _) | (_, SettlementTier::City) => RoadClass::Route,
        _ => RoadClass::Route,
    }
}

/// La vía de una calle INTERNA, según la trama de la ciudad y qué línea de la
/// retícula es.
///
/// Que haya avenidas cada pocas manzanas es lo que da a una ciudad una estructura
/// legible —"salgo a la avenida y sigo"— en vez de una cuadrícula uniforme donde
/// todas las calles son iguales y no hay forma de orientarse. Es la otra mitad de
/// romper el damero (la primera es que cada ciudad tenga su [`CityStyle`]).
pub fn street_class(style: CityStyle, block_ix: i64, block_iz: i64) -> RoadClass {
    let (ix, iz) = (block_ix, block_iz);
    match style {
        // RETÍCULA ORTOGONAL: avenidas cada 4 manzanas en los dos ejes. La trama
        // de manual, la de una ciudad planificada sobre llano.
        CityStyle::Grid => {
            if ix.rem_euclid(4) == 0 || iz.rem_euclid(4) == 0 {
                RoadClass::Avenue
            } else if (ix + iz).rem_euclid(3) == 0 {
                RoadClass::Alley
            } else {
                RoadClass::Street
            }
        }
        // RADIAL: **anillos concéntricos** (la distancia de Chebyshev al centro,
        // que en una retícula es un cuadrado — el "anillo" de una ciudad
        // amurallada) **más avenidas radiales** por los ejes y las diagonales.
        // Es la trama del centro histórico: se sale del centro por una radial y
        // se rodea por un anillo.
        CityStyle::Radial => {
            let ring = ix.abs().max(iz.abs());
            if ring.rem_euclid(3) == 0 || ix == 0 || iz == 0 || ix.abs() == iz.abs() {
                RoadClass::Avenue
            } else if ring.rem_euclid(3) == 1 {
                RoadClass::Street
            } else {
                RoadClass::Alley
            }
        }
        // PUERTO: la trama sigue la costa. Se aproxima con manzanas ALARGADAS —
        // avenidas frecuentes en un eje (los paseos paralelos al agua) y raras en
        // el otro (los muelles que bajan al mar).
        CityStyle::Port => {
            if iz.rem_euclid(3) == 0 {
                RoadClass::Avenue
            } else if ix.rem_euclid(6) == 0 {
                RoadClass::Street
            } else if (ix + iz).rem_euclid(4) == 0 {
                RoadClass::Alley
            } else {
                RoadClass::Street
            }
        }
        // MODERNA: manzanas grandes de torre, avenidas anchas y frecuentes, sin
        // callejones (una torre no tiene fondo de manzana).
        CityStyle::Modern => {
            if ix.rem_euclid(3) == 0 || iz.rem_euclid(3) == 0 {
                RoadClass::Avenue
            } else {
                RoadClass::Street
            }
        }
        // RURAL: no hay avenidas ni callejones. Hay camino.
        CityStyle::Rural => {
            if ix.rem_euclid(6) == 0 && iz.rem_euclid(6) == 0 {
                RoadClass::Route
            } else {
                RoadClass::Street
            }
        }
    }
}

/// Separación entre calles dentro de un núcleo, en tiles. Sale de la TRAMA, no
/// del rango: una ciudad moderna tiene manzanas grandes de torre y un puerto las
/// tiene chicas y alargadas.
pub fn block_size(style: CityStyle, tier: SettlementTier) -> i64 {
    match style {
        CityStyle::Modern => 44,
        CityStyle::Radial => 36,
        CityStyle::Grid => 32,
        CityStyle::Port => 26,
        CityStyle::Rural => match tier {
            SettlementTier::Hamlet => 22,
            _ => 26,
        },
    }
}

/// Sentido de circulación de un carril, en octantes (`0` = este, sentido
/// antihorario), o `None` si esa celda no es calzada.
///
/// **Todavía no hay NPCs conduciendo**: esto existe para que cuando los haya no
/// tengan que inventarse la red. Un conductor sólo necesita "¿estoy en calzada y
/// hacia dónde apunta mi carril?", y eso es una consulta O(1) sobre el mismo mapa
/// determinista que dibuja las calles.
pub fn lane_at(body: CelestialBody, tx: i64, tz: i64) -> Option<u8> {
    let c = crate::geography::civil_at(body, tx, tz)?;
    if !matches!(c, crate::geography::Civil::Road | crate::geography::Civil::Bridge) {
        return None;
    }
    // El carril se deduce de la dirección en la que la calzada CONTINÚA: se mira
    // si hay más calzada al este/oeste que al norte/sur.
    let road = |dx: i64, dz: i64| {
        matches!(
            crate::geography::civil_at(body, tx + dx, tz + dz),
            Some(crate::geography::Civil::Road | crate::geography::Civil::Bridge)
        )
    };
    let horiz = road(3, 0) as u8 + road(-3, 0) as u8;
    let vert = road(0, 3) as u8 + road(0, -3) as u8;
    if horiz == 0 && vert == 0 {
        return None;
    }
    // Mano derecha: el carril de "ida" va según de qué lado del eje estés.
    let h = hash2(0x1a4e_0000, tx.div_euclid(64), tz.div_euclid(64));
    if horiz >= vert {
        Some(if (tz + (h % 2) as i64).rem_euclid(2) == 0 { 0 } else { 4 })
    } else {
        Some(if (tx + (h % 2) as i64).rem_euclid(2) == 0 { 2 } else { 6 })
    }
}

/// ¿Esta celda está sobre calzada? Lo usa el RELIEVE para abrir rampa donde el
/// camino cruza una terraza.
///
/// Consulta `civil_kind_at` y **no** `civil_at`: la segunda mira el terreno para
/// distinguir carretera de puente, y el terreno es justamente quien está
/// preguntando. Usarla acá era una recursión infinita.
pub fn is_drivable_at(body: CelestialBody, tx: i64, tz: i64) -> bool {
    matches!(
        crate::geography::civil_kind_at(body, tx, tz),
        Some(crate::geography::Civil::Road)
    )
}

/// Ancho de la calzada que sale de un núcleo, en tiles (semiancho).
pub fn settlement_road_half(s: &Settlement) -> f32 {
    match s.tier {
        SettlementTier::City => RoadClass::Highway.half_width(),
        SettlementTier::Town => RoadClass::Route.half_width(),
        SettlementTier::Hamlet => RoadClass::Street.half_width(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Toda vía por la que se espera conducir tiene ancho para dos vehículos.
    /// El callejón es la única excepción, y es a propósito.
    #[test]
    fn road_classes_are_wide_enough_to_drive() {
        for c in [RoadClass::Highway, RoadClass::Route, RoadClass::Avenue, RoadClass::Street] {
            assert!(c.drivable(), "{c:?} es demasiado angosta para conducir");
            assert!(
                c.half_shoulder() > c.half_width(),
                "{c:?} sin banquina: el bosque llegaría hasta el asfalto"
            );
        }
        assert!(!RoadClass::Alley.drivable(), "el callejón debería ser peatonal");
        // Y hay JERARQUÍA de verdad: una autopista es claramente más ancha.
        assert!(RoadClass::Highway.half_width() > RoadClass::Street.half_width() * 1.8);
    }

    /// Cada trama de ciudad reparte sus avenidas distinto. Si todas dieran la
    /// misma clase para las mismas manzanas, volveríamos al damero.
    #[test]
    fn city_styles_lay_out_different_street_grids() {
        let styles = [
            CityStyle::Grid,
            CityStyle::Radial,
            CityStyle::Port,
            CityStyle::Modern,
            CityStyle::Rural,
        ];
        let mut fingerprints = std::collections::HashSet::new();
        for s in styles {
            let mut f = Vec::new();
            for i in 0..12 {
                for j in 0..12 {
                    f.push(street_class(s, i, j));
                }
            }
            assert!(fingerprints.insert(f), "{s:?} tiene la misma trama que otra");
            // Y el tamaño de manzana también cambia.
            let _ = block_size(s, SettlementTier::City);
        }
        assert!(block_size(CityStyle::Modern, SettlementTier::City)
            > block_size(CityStyle::Port, SettlementTier::City));
    }

    /// Las autopistas sólo salen de unir ciudades; a una aldea le llega una ruta.
    #[test]
    fn only_cities_get_highways() {
        assert_eq!(
            link_class(SettlementTier::City, SettlementTier::City),
            RoadClass::Highway
        );
        for other in [SettlementTier::Town, SettlementTier::Hamlet] {
            assert_ne!(link_class(SettlementTier::City, other), RoadClass::Highway);
            assert_ne!(link_class(other, other), RoadClass::Highway);
        }
    }

    /// **NINGUNA CALZADA CHOCA CONTRA UN ACANTILADO.**
    ///
    /// Es el test que hace realmente transitable el mapa: con el relieve por
    /// terrazas, una carretera podía toparse de frente con una pared de 2.5 m y
    /// quedarse ahí. `relief::in_pass_band` abre rampa donde hay calzada, y esto lo
    /// comprueba sobre medio planeta, celda a celda.
    #[test]
    fn no_road_is_ever_blocked_by_a_cliff() {
        use crate::topdown::{topdown_tile, Tile};
        let body = CelestialBody::Earth;
        let r = crate::celestial_bodies::body_walk_radius(body) as i64;
        let mut checked = 0;
        // Muestreo fino en x (para no saltarse el cruce de una carretera) y más
        // espaciado en z: la red es continua, no hace falta mirar cada fila.
        for tz in (-r / 3..r / 3).step_by(17) {
            for tx in (-r / 3..r / 3).step_by(2) {
                if !is_drivable_at(body, tx, tz) {
                    continue;
                }
                checked += 1;
                let t = topdown_tile(body, 0, tx, tz);
                assert_ne!(t, Tile::Cliff, "acantilado sobre la calzada en ({tx},{tz})");
                assert!(
                    !t.solid(),
                    "la calzada en ({tx},{tz}) es intransitable: {t:?}"
                );
            }
        }
        assert!(checked > 300, "apenas {checked} celdas de calzada revisadas");
    }

    /// Un tramo de carretera que cruza una terraza tiene que ser CONTINUO: no
    /// vale abrir un agujero de un tile y dejar el resto amurallado.
    #[test]
    fn a_road_crossing_a_terrace_stays_continuous() {
        use crate::topdown::{topdown_tile, Tile};
        let body = CelestialBody::Earth;
        let r = crate::celestial_bodies::body_walk_radius(body) as i64;
        // Se busca un tramo largo de calzada y se camina entero.
        let mut runs = 0;
        for tz in (-r / 3..r / 3).step_by(419) {
            let mut run = 0;
            for tx in -r / 3..r / 3 {
                if is_drivable_at(body, tx, tz) {
                    run += 1;
                    assert!(
                        !topdown_tile(body, 0, tx, tz).solid(),
                        "corte en la calzada en ({tx},{tz})"
                    );
                } else {
                    if run > 40 {
                        runs += 1;
                    }
                    run = 0;
                }
            }
        }
        assert!(runs > 0, "no se encontró ni un tramo largo de carretera que recorrer");
        let _ = Tile::Cliff;
    }

    /// Hay carriles con sentido sobre la calzada — la base de un NPC conductor.
    #[test]
    fn roads_expose_lanes_with_a_direction() {
        let body = CelestialBody::Earth;
        let mut lanes = std::collections::HashSet::new();
        let r = crate::celestial_bodies::body_walk_radius(body) as i64;
        let mut found = 0;
        for tz in (-r / 3..r / 3).step_by(11) {
            for tx in (-r / 3..r / 3).step_by(11) {
                if let Some(dir) = lane_at(body, tx, tz) {
                    lanes.insert(dir);
                    found += 1;
                }
            }
        }
        assert!(found > 50, "apenas {found} celdas con carril");
        assert!(lanes.len() >= 2, "todos los carriles van en el mismo sentido: {lanes:?}");
    }
}
