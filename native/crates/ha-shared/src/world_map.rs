//! **LA PLANTILLA DEL MUNDO** — la forma que tiene cada cuerpo, autorada a mano.
//!
//! El ruido solo hace paisaje, no hace *un sitio*. Un mundo de puro fbm es papel
//! pintado: cada kilómetro se parece al anterior, no hay nada que reconocer y
//! volar diez minutos no significa nada. Este módulo le da a cada cuerpo una
//! **estructura declarada**: dónde está el continente, por dónde cruza la
//! cordillera, dónde hay lagos, islas y ciudades con nombre.
//!
//! ```text
//!   ~~~~~~~~~~~~~~~~~~~~~~~~~~~  ← océano (plataforma bajo el nivel del mar)
//!   ~~~~▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄~~~~~
//!   ~~▄▄▄▄▄▄███████▄▄▄▄▄▄▄▄▄~~~  ← cordillera central (suma altura)
//!   ~~▄▄◯▄▄▄███████▄▄▄▄◍▄▄▄▄~~~  ← ◯ lago (resta) · ◍ ciudad con nombre
//!   ~~~~▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄~~~~~
//! ```
//!
//! ## Cómo se combina con el ruido
//!
//! La plantilla no dibuja el terreno: pone la **plataforma** sobre la que el ruido
//! ondula. `relief_field` hace `plataforma + (ruido − 0.5) × amplitud`, así que:
//!
//! - la plantilla decide **qué es mar y qué es tierra**, dónde está la sierra y
//!   dónde el valle — o sea, la geografía que se recuerda;
//! - el ruido decide **cómo es** cada metro concreto — o sea, la que se camina.
//!
//! Ninguno de los dos podría hacer el trabajo del otro, y por eso están separados.
//!
//! ## Todas las coordenadas son NORMALIZADAS
//!
//! `[-1, 1]` sobre el radio caminable del cuerpo. Así la misma tabla vale para la
//! Tierra (60 km de lado) y para una luna chica, y cambiar el tamaño de un cuerpo
//! no obliga a reescribir su mapa.
//!
//! ## Todos los cuerpos tienen la suya
//!
//! El relieve es geometría, no contenido: un planeta sin forma es una alfombra y
//! no vale la pena visitarlo. Lo que SÍ es exclusivo de la Tierra es la
//! civilización (`geography::has_civilisation`).

use crate::celestial_bodies::{body_walk_radius, CelestialBody};
use crate::topdown::TILE_M;

// ---------------------------------------------------------------------------
// Vocabulario de la plantilla
// ---------------------------------------------------------------------------

/// Una masa de tierra: una polilínea "espina" engordada `width`. Con polilíneas
/// en vez de círculos salen penínsulas, bahías y lóbulos — costas que se
/// reconocen — en vez de islas redondas.
pub struct Land {
    pub spine: &'static [(f32, f32)],
    pub width: f32,
}

/// Una cordillera: misma idea, pero SUMA altura en vez de definir tierra.
pub struct Range {
    pub name: &'static str,
    pub spine: &'static [(f32, f32)],
    pub width: f32,
    /// Cuánta altura suma en la cresta (en unidades de campo 0..1).
    pub height: f32,
}

/// Una cuenca que RESTA altura. Si baja del nivel del mar, se llena de líquido
/// sola — no hay que declarar "acá hay agua", basta con hundirlo.
pub struct Basin {
    pub name: &'static str,
    pub c: (f32, f32),
    pub r: f32,
    pub depth: f32,
}

/// Una isla suelta, mar adentro.
pub struct Isle {
    pub name: &'static str,
    pub c: (f32, f32),
    pub r: f32,
    /// Positivo = asoma; negativo = ruina sumergida (la Ciudad Hundida).
    pub height: f32,
}

/// Un parche de clima: sesga la humedad para que el bosque quede DONDE dice el
/// mapa y no donde cayó el ruido.
pub struct Patch {
    pub name: &'static str,
    pub c: (f32, f32),
    pub r: f32,
    /// + = más húmedo (bosque), − = más seco (llanura, estepa).
    pub moisture: f32,
}

/// La trama de una ciudad. **Es lo que rompe el damero**: hasta ahora las cuatro
/// ciudades del mundo eran la misma ciudad con otro hash, y desde el aire el
/// planeta entero se veía como un tablero de ajedrez.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CityStyle {
    /// Retícula ortogonal cerrada, con zona industrial. La metrópolis del norte.
    Grid,
    /// Anillos concéntricos y avenidas radiales. El centro histórico.
    Radial,
    /// Trama que sigue la costa, con muelles perpendiculares. El puerto.
    Port,
    /// Manzanas grandes de torres. La ciudad nueva.
    Modern,
    /// Cuatro calles y casas sueltas: pueblos y aldeas del procedural.
    Rural,
}

/// Una ciudad AUTORADA: existe siempre, en ese sitio y con ese nombre.
pub struct Town {
    pub name: &'static str,
    pub c: (f32, f32),
    /// Radio del casco, en TILES (no normalizado: una metrópolis mide lo que mide).
    pub radius: i64,
    pub style: CityStyle,
}

/// El mapa completo de un cuerpo.
pub struct WorldTemplate {
    /// Cuánto por debajo del nivel del mar queda el fondo oceánico base.
    pub ocean_drop: f32,
    /// Cuánto por encima del nivel del mar queda la tierra firme base.
    pub land_lift: f32,
    pub lands: &'static [Land],
    pub ranges: &'static [Range],
    pub basins: &'static [Basin],
    pub isles: &'static [Isle],
    pub patches: &'static [Patch],
    pub towns: &'static [Town],
}

/// Amplitud del ruido sobre la plataforma. Es el "cuánto ondula el terreno
/// alrededor de lo que dice el mapa": subirlo borra la plantilla bajo el ruido,
/// bajarlo deja un mundo de mesetas geométricas.
pub const RELIEF_AMPLITUDE: f32 = 0.42;

// ---------------------------------------------------------------------------
// LA TIERRA — la plantilla de la imagen de referencia
// ---------------------------------------------------------------------------

static EARTH: WorldTemplate = WorldTemplate {
    ocean_drop: 0.20,
    land_lift: 0.10,
    // Un continente central O→E con dos lóbulos (NE y SE) y una península al SO.
    // Los lóbulos son lo que da bahías y costas con forma, en vez de un óvalo.
    lands: &[
        Land { spine: &[(-0.72, 0.02), (-0.30, -0.08), (0.15, -0.04), (0.60, 0.12)], width: 0.30 },
        // Lóbulo NO — Metrópolis Norte y la Laguna de la Bahía. Sin él, la
        // metrópolis quedaba mar adentro: la declaraba el mapa pero el continente
        // no llegaba hasta ahí (lo cazó `named_cities_stand_on_habitable_ground`).
        Land { spine: &[(-0.62, -0.32), (-0.42, -0.24), (-0.24, -0.14)], width: 0.17 },
        // Lóbulo NE — los Campos de Chronos y los cañones del viento.
        Land { spine: &[(0.30, -0.40), (0.62, -0.32), (0.80, -0.16)], width: 0.20 },
        // Lóbulo SE — Villa Sur y el delta.
        Land { spine: &[(0.48, 0.28), (0.72, 0.36), (0.84, 0.50)], width: 0.17 },
        // Península SO — Puerto Oeste y la Vanguardia Primigenia.
        Land { spine: &[(-0.86, 0.30), (-0.66, 0.20), (-0.48, 0.14)], width: 0.13 },
    ],
    ranges: &[
        // MONTE ALTÍSIMO: la cordillera central. Cruza el continente de OSO a ENE
        // y parte el mundo en norte y sur — es LA barrera del mapa, la que hay que
        // rodear o cruzar por un paso.
        Range {
            name: "Cordillera Monte Altísimo",
            // Corrida al NORTE del origen a propósito: el jugador aparece en
            // (0,0) y despertar dentro de la cordillera es despertar rodeado de
            // paredes. Con la sierra al norte, el spawn cae en el valle central y
            // la montaña es lo que se ve en el horizonte y se decide subir.
            spine: &[(-0.46, -0.10), (-0.14, -0.22), (0.18, -0.16), (0.42, -0.06)],
            width: 0.13,
            height: 0.26,
        },
        // Sierra secundaria del sur: más baja, se cruza, pero encajona el valle.
        Range {
            name: "Sierra del Sur",
            spine: &[(-0.46, 0.34), (-0.10, 0.42), (0.22, 0.36)],
            width: 0.075,
            height: 0.18,
        },
    ],
    basins: &[
        Basin { name: "Lago del Oeste", c: (-0.56, 0.00), r: 0.10, depth: 0.24 },
        Basin { name: "Lagos Espejo", c: (0.00, 0.22), r: 0.11, depth: 0.22 },
        Basin { name: "Laguna de la Bahía", c: (-0.32, -0.26), r: 0.08, depth: 0.20 },
        // El Cañón del Viento: una cuenca ESTRECHA en el lóbulo NE. Al hundirse
        // entre terrazas altas se lee como un cañón, no como un lago.
        Basin { name: "Cañón del Viento", c: (0.72, -0.24), r: 0.055, depth: 0.26 },
    ],
    isles: &[
        Isle { name: "Archipiélago Volcánico", c: (0.02, -0.70), r: 0.075, height: 0.26 },
        Isle { name: "Islas Susurrantes", c: (0.52, -0.62), r: 0.09, height: 0.20 },
        // Ruina SUMERGIDA: altura negativa, así que nunca asoma — es un bajío que
        // se nada, con el fondo a la vista.
        Isle { name: "Ciudad Hundida de Aethel", c: (-0.84, -0.14), r: 0.07, height: -0.06 },
        Isle { name: "Descanso del Kraken", c: (-0.06, 0.80), r: 0.10, height: -0.10 },
    ],
    patches: &[
        Patch { name: "Bosque Susurrante", c: (-0.16, -0.40), r: 0.20, moisture: 0.30 },
        Patch { name: "Bosque Denso", c: (0.70, -0.16), r: 0.17, moisture: 0.28 },
        // Los Campos de Chronos: meseta SECA — pradera abierta, no bosque.
        Patch { name: "Campos de Chronos", c: (0.34, -0.34), r: 0.22, moisture: -0.24 },
        Patch { name: "Delta del Sur", c: (0.66, 0.46), r: 0.14, moisture: 0.32 },
    ],
    towns: &[
        // Radios en tiles = metros. Una metrópolis de 1800 mide 3.6 km de punta a
        // punta: caminarla lleva un rato, que es lo que la hace una ciudad.
        Town { name: "Metrópolis Norte", c: (-0.50, -0.26), radius: 1800, style: CityStyle::Grid },
        // Al PIE de la cordillera (que corre por z≈−0.16 a esta longitud), no
        // encima: una ciudad de 4.4 km de lado en la alta montaña no tiene dónde
        // asentarse. Lo cazó `named_cities_stand_on_habitable_ground`.
        Town { name: "Ciudad Central", c: (0.32, 0.08), radius: 2200, style: CityStyle::Radial },
        Town { name: "Puerto Oeste", c: (-0.76, 0.22), radius: 1400, style: CityStyle::Port },
        Town { name: "Villa Sur", c: (0.70, 0.34), radius: 1200, style: CityStyle::Modern },
    ],
};

// ---------------------------------------------------------------------------
// Los demás cuerpos — cada uno con su forma, no la Tierra repintada
// ---------------------------------------------------------------------------

static MOON: WorldTemplate = WorldTemplate {
    // Sin líquido: `ocean_drop` sólo hunde las cuencas (los mares de basalto).
    ocean_drop: 0.16,
    land_lift: 0.12,
    lands: &[Land { spine: &[(-0.70, 0.0), (0.0, 0.0), (0.70, 0.0)], width: 0.85 }],
    ranges: &[Range {
        name: "Montes Apeninos",
        spine: &[(-0.30, -0.34), (0.02, -0.18), (0.26, 0.04)],
        width: 0.07,
        height: 0.26,
    }],
    basins: &[
        Basin { name: "Mar de la Tranquilidad", c: (0.22, -0.20), r: 0.30, depth: 0.20 },
        Basin { name: "Mar de las Lluvias", c: (-0.30, -0.26), r: 0.26, depth: 0.19 },
        Basin { name: "Océano de las Tormentas", c: (-0.52, 0.16), r: 0.28, depth: 0.17 },
    ],
    isles: &[],
    patches: &[],
    towns: &[],
};

static MARS: WorldTemplate = WorldTemplate {
    ocean_drop: 0.22,
    land_lift: 0.11,
    lands: &[Land { spine: &[(-0.75, 0.0), (0.0, -0.05), (0.75, 0.05)], width: 0.80 }],
    ranges: &[
        // Monte Olimpo: un volcán, o sea una cordillera de espina cortísima.
        Range { name: "Monte Olimpo", spine: &[(-0.44, -0.20), (-0.38, -0.14)], width: 0.13, height: 0.40 },
        Range { name: "Dorsa Tharsis", spine: &[(-0.20, -0.06), (-0.06, 0.10), (0.04, 0.24)], width: 0.07, height: 0.24 },
    ],
    basins: &[
        // Valles Marineris: una cuenca LARGA. Se declara con tres cuencas
        // solapadas porque un cañón es una zanja, no un agujero.
        Basin { name: "Valles Marineris", c: (0.10, 0.06), r: 0.09, depth: 0.30 },
        Basin { name: "Valles Marineris", c: (0.28, 0.09), r: 0.09, depth: 0.30 },
        Basin { name: "Valles Marineris", c: (0.46, 0.12), r: 0.08, depth: 0.28 },
        Basin { name: "Cuenca Hellas", c: (0.36, 0.48), r: 0.20, depth: 0.26 },
    ],
    isles: &[],
    patches: &[Patch { name: "Planicie Boreal", c: (0.0, -0.62), r: 0.30, moisture: 0.22 }],
    towns: &[],
};

static EUROPA: WorldTemplate = WorldTemplate {
    // Casi todo mar bajo hielo: la tierra son las placas que asoman.
    ocean_drop: 0.16,
    land_lift: 0.07,
    lands: &[
        Land { spine: &[(-0.60, -0.30), (-0.10, -0.10), (0.40, -0.25)], width: 0.14 },
        Land { spine: &[(-0.35, 0.35), (0.15, 0.45), (0.60, 0.30)], width: 0.12 },
    ],
    ranges: &[Range {
        name: "Dorsal de Hielo",
        spine: &[(-0.55, 0.10), (0.0, 0.02), (0.55, 0.12)],
        width: 0.05,
        height: 0.18,
    }],
    basins: &[Basin { name: "Grieta de Conamara", c: (0.05, -0.05), r: 0.16, depth: 0.22 }],
    isles: &[Isle { name: "Placa Errante", c: (0.62, -0.55), r: 0.09, height: 0.16 }],
    patches: &[],
    towns: &[],
};

static IO: WorldTemplate = WorldTemplate {
    ocean_drop: 0.18,
    land_lift: 0.13,
    lands: &[Land { spine: &[(-0.70, 0.0), (0.70, 0.0)], width: 0.80 }],
    ranges: &[
        Range { name: "Monte Bœsaule", spine: &[(-0.30, 0.28), (-0.18, 0.36)], width: 0.11, height: 0.36 },
        Range { name: "Dorsal de Azufre", spine: &[(0.10, -0.40), (0.35, -0.15), (0.50, 0.15)], width: 0.06, height: 0.22 },
    ],
    // Lagos de LAVA: cuencas hundidas bajo el "nivel del mar" volcánico.
    basins: &[
        Basin { name: "Patera de Loki", c: (-0.12, -0.14), r: 0.15, depth: 0.28 },
        Basin { name: "Patera de Pele", c: (0.28, 0.30), r: 0.12, depth: 0.26 },
        Basin { name: "Lago Tvashtar", c: (-0.48, -0.44), r: 0.10, depth: 0.24 },
    ],
    isles: &[],
    patches: &[],
    towns: &[],
};

static TITAN: WorldTemplate = WorldTemplate {
    // Oceánico: mares de metano y un puñado de islas donde vivir.
    ocean_drop: 0.17,
    land_lift: 0.09,
    // Un mundo oceánico tiene que ser HABITABLE, no un charco: islas grandes
    // donde plantar una base, con mucho más mar que la Tierra alrededor.
    lands: &[
        Land { spine: &[(-0.52, 0.16), (-0.05, 0.04), (0.42, 0.20)], width: 0.26 },
        Land { spine: &[(0.28, -0.46), (0.62, -0.30)], width: 0.19 },
        Land { spine: &[(-0.30, 0.52), (0.10, 0.58)], width: 0.15 },
    ],
    ranges: &[Range {
        name: "Montes Mithrim",
        spine: &[(-0.30, 0.10), (0.05, 0.02), (0.30, 0.14)],
        width: 0.06,
        height: 0.20,
    }],
    basins: &[
        Basin { name: "Kraken Mare", c: (-0.10, -0.48), r: 0.28, depth: 0.24 },
        Basin { name: "Ligeia Mare", c: (0.38, -0.62), r: 0.20, depth: 0.22 },
    ],
    isles: &[Isle { name: "Isla Mágica", c: (-0.55, -0.40), r: 0.07, height: 0.14 }],
    patches: &[Patch { name: "Dunas de Shangri-La", c: (-0.45, 0.45), r: 0.24, moisture: -0.26 }],
    towns: &[],
};

/// Plantilla genérica para los cuerpos sin mapa propio (asteroides, lunitas):
/// una masa central con una sierra y un par de cuencas. No es un mapa autorado
/// pero SÍ es una forma — mejor que ruido plano, y basta hasta que alguien le
/// escriba la suya.
static GENERIC: WorldTemplate = WorldTemplate {
    ocean_drop: 0.16,
    land_lift: 0.11,
    lands: &[Land { spine: &[(-0.62, -0.10), (0.0, 0.05), (0.62, -0.05)], width: 0.72 }],
    ranges: &[Range {
        name: "Dorsal Central",
        spine: &[(-0.35, 0.15), (0.10, -0.05), (0.45, 0.10)],
        width: 0.07,
        height: 0.24,
    }],
    basins: &[
        Basin { name: "Cuenca Norte", c: (-0.20, -0.38), r: 0.18, depth: 0.20 },
        Basin { name: "Cuenca Sur", c: (0.25, 0.40), r: 0.16, depth: 0.19 },
    ],
    isles: &[],
    patches: &[],
    towns: &[],
};

/// La plantilla de un cuerpo. **Todos tienen una** — un planeta sin forma es una
/// alfombra; el genérico es el peor caso, no la ausencia de mapa.
pub fn template(body: CelestialBody) -> &'static WorldTemplate {
    use CelestialBody as B;
    match body {
        B::Earth => &EARTH,
        B::Moon => &MOON,
        B::Mars => &MARS,
        B::Europa => &EUROPA,
        B::Io => &IO,
        B::Titan => &TITAN,
        _ => &GENERIC,
    }
}

// ---------------------------------------------------------------------------
// Campos: de la tabla al terreno
// ---------------------------------------------------------------------------

/// Celda del mundo → coordenadas normalizadas `[-1, 1]`.
pub fn normalized(body: CelestialBody, tx: i64, tz: i64) -> (f32, f32) {
    let r = body_walk_radius(body).max(1.0);
    (tx as f32 * TILE_M / r, tz as f32 * TILE_M / r)
}

/// Normalizadas → celda del mundo.
pub fn denormalized(body: CelestialBody, p: (f32, f32)) -> (i64, i64) {
    let r = body_walk_radius(body).max(1.0);
    ((p.0 * r / TILE_M) as i64, (p.1 * r / TILE_M) as i64)
}

fn dist_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vz) = (b.0 - a.0, b.1 - a.1);
    let (wx, wz) = (p.0 - a.0, p.1 - a.1);
    let len2 = vx * vx + vz * vz;
    let t = if len2 <= 1e-9 { 0.0 } else { ((wx * vx + wz * vz) / len2).clamp(0.0, 1.0) };
    let (dx, dz) = (wx - vx * t, wz - vz * t);
    (dx * dx + dz * dz).sqrt()
}

fn dist_to_spine(p: (f32, f32), spine: &[(f32, f32)]) -> f32 {
    if spine.is_empty() {
        return f32::MAX;
    }
    if spine.len() == 1 {
        let (dx, dz) = (p.0 - spine[0].0, p.1 - spine[0].1);
        return (dx * dx + dz * dz).sqrt();
    }
    spine
        .windows(2)
        .map(|s| dist_to_segment(p, s[0], s[1]))
        .fold(f32::MAX, f32::min)
}

/// Fracción de un rasgo que es NÚCLEO PLANO (el resto es la orilla).
const CORE: f32 = 0.55;

/// `1` en el núcleo, `0` fuera, con una orilla suave.
///
/// **Es una meseta, no una campana**, y la diferencia no es cosmética: con una
/// campana pura (`(1-x)²(3-2x)`) sólo el punto EXACTO del centro llegaba a la
/// altura declarada, así que el interior de un continente se quedaba a media
/// altura y sus ciudades costeras amanecían bajo el agua — que es literalmente lo
/// que pasó con Metrópolis Norte. Con núcleo plano hasta el 55 % del radio, un
/// continente es tierra firme entera y sólo la orilla se degrada.
///
/// La orilla SUAVE sigue importando: es lo que deja que el ruido la muerda y salga
/// una costa irregular en vez de un recorte perfecto.
fn falloff(d: f32, radius: f32) -> f32 {
    plateau(d, radius, CORE)
}

fn plateau(d: f32, radius: f32, core: f32) -> f32 {
    if radius <= 1e-6 {
        return 0.0;
    }
    let x = (d / radius).clamp(0.0, 1.0);
    let t = ((1.0 - x) / (1.0 - core)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Perfil de CRESTA para las cordilleras: núcleo casi nulo, así la altura cae
/// desde la línea de cumbres hacia los dos lados.
///
/// Una sierra con el mismo perfil de meseta que un continente sale como un
/// altiplano gigantesco: medido, la cordillera central ocupaba el tercio medio
/// del planeta a nivel 7-9 y no dejaba valle. Una cordillera tiene que ser una
/// pared con laderas, no una mesa.
fn ridge(d: f32, radius: f32) -> f32 {
    plateau(d, radius, 0.12)
}

/// **La plataforma**: la altura base que la plantilla impone en esta celda, antes
/// de que el ruido ondule sobre ella.
///
/// Se construye por capas, de abajo a arriba: océano → continentes → islas →
/// cordilleras → cuencas → borde del mundo. El orden importa poco (todo suma)
/// salvo el borde, que se aplica al final para que NADA pueda salvar el reborde:
/// el mundo tiene que terminar en mar abierto siempre.
pub fn platform(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    let t = template(body);
    let sea = crate::relief::sea_level(body);
    // Sin líquido (Luna), el "nivel del mar" es 0 y no sirve de referencia: se usa
    // un nivel medio ficticio para que las cuencas se hundan igual.
    let datum = if sea > 0.0 { sea } else { 0.45 };
    let p = normalized(body, tx, tz);

    let mut h = datum - t.ocean_drop;

    // Continentes: el más "adentro" manda (máximo, no suma — dos masas que se
    // tocan forman un istmo, no una montaña).
    let land = t
        .lands
        .iter()
        .map(|l| falloff(dist_to_spine(p, l.spine), l.width))
        .fold(0.0f32, f32::max);
    h += land * (t.land_lift + t.ocean_drop);

    // Islas: suman sobre el océano (o restan, si son ruinas sumergidas).
    for i in t.isles {
        let d = ((p.0 - i.c.0).powi(2) + (p.1 - i.c.1).powi(2)).sqrt();
        h += falloff(d, i.r) * i.height;
    }

    // Cordilleras: perfil de cresta, no de meseta.
    for r in t.ranges {
        h += ridge(dist_to_spine(p, r.spine), r.width) * r.height;
    }

    // Cuencas: restan. Si bajan del nivel del mar se llenan solas.
    for b in t.basins {
        let d = ((p.0 - b.c.0).powi(2) + (p.1 - b.c.1).powi(2)).sqrt();
        h -= falloff(d, b.r) * b.depth;
    }

    // LAGUNAS REPARTIDAS: las cuencas de la tabla son los lagos GRANDES, los que
    // tienen nombre. Éstas son las charcas de las afueras — una cada ~1.4 km,
    // donde ir a pescar, montar un campamento o simplemente beber. Un mundo con
    // cuatro lagos declarados y nada más se siente vacío entre ciudad y ciudad.
    h -= pond_depth(body, tx, tz);

    // EL BORDE DEL MUNDO, al final y sin apelación: los últimos kilómetros bajan
    // a mar abierto. Sin esto una cordillera declarada cerca del borde crearía un
    // muro en el confín del mapa en vez de una costa.
    let d = (p.0 * p.0 + p.1 * p.1).sqrt();
    let rim = ((d - 0.80) / 0.20).clamp(0.0, 1.0);
    h -= rim * (h - (datum - t.ocean_drop * 1.4)).max(0.0);

    h
}

// ---------------------------------------------------------------------------
// Lagunas repartidas
// ---------------------------------------------------------------------------

/// Lado de la celda de laguna, en tiles. Una candidata cada 1.4 km: caminando te
/// cruzás con agua cada tanto sin que el mundo sea un pantano.
pub const POND_PERIOD: i64 = 1_400;

/// Radio máximo de una laguna, en tiles.
const POND_MAX_R: f32 = 190.0;

/// El centro y el radio de la laguna de una celda `(rx, rz)`, si esa celda tiene
/// una. Determinista y O(1), igual que las bocas de cueva.
///
/// Devuelve la laguna aunque caiga en el mar o en la montaña: quien decide si
/// **existe** es el terreno (una laguna sobre el océano no se nota, y una en la
/// cumbre queda por encima del nivel del mar y no se llena). Filtrarlo acá
/// costaría una consulta de relieve dentro del cálculo del relieve — recursión.
pub fn pond_in(body: CelestialBody, rx: i64, rz: i64) -> Option<((i64, i64), f32)> {
    let h = crate::geography::hash2(crate::celestial_bodies::body_terrain_seed(body) ^ 0x1a6e_0000, rx, rz);
    // Poco más de la mitad de las celdas tienen laguna; el resto es tierra seca.
    if h % 100 >= 55 {
        return None;
    }
    let margin = POND_MAX_R as i64 + 40;
    let span = (POND_PERIOD - margin * 2).max(1);
    let cx = rx * POND_PERIOD + margin + ((h >> 7) as i64).rem_euclid(span);
    let cz = rz * POND_PERIOD + margin + ((h >> 17) as i64).rem_euclid(span);
    // Tamaños variados: charcas de 60 m y lagos de 190 m.
    let r = 60.0 + ((h >> 3) % 130) as f32;
    Some(((cx, cz), r))
}

/// Cuánto hunde el terreno la laguna de esta celda (0 = ninguna).
fn pond_depth(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    let (rx, rz) = (tx.div_euclid(POND_PERIOD), tz.div_euclid(POND_PERIOD));
    let mut best = 0.0f32;
    // Las cuatro celdas vecinas: una laguna cerca del borde de su celda tiene que
    // seguir hundiendo el terreno del otro lado.
    for (ox, oz) in [(0, 0), (-1, 0), (0, -1), (-1, -1)] {
        let Some(((cx, cz), r)) = pond_in(body, rx + ox, rz + oz) else { continue };
        let d = (((tx - cx).pow(2) + (tz - cz).pow(2)) as f32).sqrt();
        // Profundidad proporcional al tamaño: las charcas se vadean, los lagos no.
        let depth = 0.055 + 0.075 * (r / POND_MAX_R);
        best = best.max(plateau(d, r, 0.35) * depth);
    }
    best
}

/// El centro de la laguna que contiene esta celda, si la hay. Lo usan los
/// campamentos y los puestos de pesca para colocarse **en la orilla**.
pub fn pond_covering(body: CelestialBody, tx: i64, tz: i64) -> Option<((i64, i64), f32)> {
    let (rx, rz) = (tx.div_euclid(POND_PERIOD), tz.div_euclid(POND_PERIOD));
    for (ox, oz) in [(0, 0), (-1, 0), (0, -1), (-1, -1)] {
        let Some((c, r)) = pond_in(body, rx + ox, rz + oz) else { continue };
        let d = (((tx - c.0).pow(2) + (tz - c.1).pow(2)) as f32).sqrt();
        if d <= r * 1.25 {
            return Some((c, r));
        }
    }
    None
}

/// Sesgo de humedad de la plantilla: dónde el mapa dice que hay bosque o estepa.
pub fn moisture_bias(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    let t = template(body);
    if t.patches.is_empty() {
        return 0.0;
    }
    let p = normalized(body, tx, tz);
    let mut m = 0.0;
    for patch in t.patches {
        let d = ((p.0 - patch.c.0).powi(2) + (p.1 - patch.c.1).powi(2)).sqrt();
        m += falloff(d, patch.r) * patch.moisture;
    }
    m
}

// ---------------------------------------------------------------------------
// Ciudades autoradas
// ---------------------------------------------------------------------------

/// La ciudad autorada cuyo centro cae en la celda de asentamiento `(sx, sz)`, si
/// hay alguna. Es la puerta por la que `geography::settlement_in` prefiere lo
/// declarado sobre el sorteo por hash.
pub fn town_in_site(body: CelestialBody, sx: i64, sz: i64, period: i64) -> Option<&'static Town> {
    let t = template(body);
    t.towns.iter().find(|town| {
        let (cx, cz) = denormalized(body, town.c);
        cx.div_euclid(period) == sx && cz.div_euclid(period) == sz
    })
}

/// El centro de una ciudad autorada, en celdas del mundo.
pub fn town_center(body: CelestialBody, town: &Town) -> (i64, i64) {
    denormalized(body, town.c)
}

/// ¿Cae esta celda dentro del casco de una ciudad autorada?
pub fn town_covering(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static Town> {
    template(body).towns.iter().find(|town| {
        let (cx, cz) = denormalized(body, town.c);
        (tx - cx).abs() <= town.radius && (tz - cz).abs() <= town.radius
    })
}

// ---------------------------------------------------------------------------
// Nombres de región — para el ítem-mapa y los carteles
// ---------------------------------------------------------------------------

/// Cómo se llama el sitio donde estás. Lo lee el ítem-mapa y el HUD: un mundo
/// donde los sitios TIENEN NOMBRE se recuerda; uno de coordenadas, no.
///
/// Prioridad de lo más específico a lo más general: ciudad → isla → cuenca →
/// cordillera → parche de clima.
pub fn region_name(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static str> {
    let t = template(body);
    if let Some(town) = town_covering(body, tx, tz) {
        return Some(town.name);
    }
    let p = normalized(body, tx, tz);
    let near = |c: (f32, f32), r: f32| ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt() < r;
    if let Some(i) = t.isles.iter().find(|i| near(i.c, i.r)) {
        return Some(i.name);
    }
    if let Some(b) = t.basins.iter().find(|b| near(b.c, b.r * 0.9)) {
        return Some(b.name);
    }
    if let Some(r) = t.ranges.iter().find(|r| dist_to_spine(p, r.spine) < r.width) {
        return Some(r.name);
    }
    t.patches.iter().find(|c| near(c.c, c.r * 0.8)).map(|c| c.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relief::{relief_level, sea_level};

    fn landable() -> Vec<CelestialBody> {
        CelestialBody::all()
            .iter()
            .copied()
            .filter(|&b| crate::celestial_bodies::body_landable(b))
            .collect()
    }

    /// **La cordillera central existe y es una BARRERA.** Es el rasgo que pidió
    /// el mapa de referencia: cruzando el continente de norte a sur por el medio
    /// hay que toparse con montaña alta.
    #[test]
    fn earth_has_a_central_mountain_range() {
        let body = CelestialBody::Earth;
        // Se recorre el MERIDIANO ENTERO, no una ventana de ±3 km alrededor del
        // origen: la cordillera se corrió al norte para que el jugador no aparezca
        // dentro de ella, y un barrido corto centrado en el spawn ya no la cruza.
        let (cx, _) = denormalized(body, (0.0, 0.0));
        let r = body_walk_radius(body) as i64;
        let mut peak = 0i8;
        for tz in -r..r {
            peak = peak.max(relief_level(body, cx, tz));
        }
        assert!(peak >= 8, "la cordillera central no llega a montaña: pico {peak}");
        // Y tiene nombre.
        let mut named = false;
        for tz in -r..r {
            if region_name(body, cx, tz) == Some("Cordillera Monte Altísimo") {
                named = true;
                break;
            }
        }
        assert!(named, "la cordillera existe pero no se llama de ninguna manera");
    }

    /// Las cuatro ciudades de la plantilla existen, están en su cuadrante y NO
    /// comparten trama — que es lo que rompe el damero de la captura vieja.
    #[test]
    fn the_four_named_cities_are_distinct_and_placed() {
        let t = template(CelestialBody::Earth);
        assert_eq!(t.towns.len(), 4);
        let mut styles = std::collections::HashSet::new();
        for town in t.towns {
            assert!(styles.insert(town.style), "{} repite trama", town.name);
            assert!(town.radius >= 1_000, "{} es demasiado chica para una ciudad", town.name);
        }
        // REPARTIDAS, no agrupadas. Se comprueba que haya ciudades en los dos
        // lados de cada eje y que estén lejos entre sí — no un cupo exacto por
        // cuadrante: dónde cae cada una la manda el terreno (Ciudad Central tuvo
        // que bajar al pie de la cordillera), y un `assert_eq!(2)` convertía
        // cualquier ajuste de mapa en un fallo de test sin que nada estuviera mal.
        assert!(t.towns.iter().any(|c| c.c.1 < 0.0), "ninguna ciudad al norte");
        assert!(t.towns.iter().any(|c| c.c.1 > 0.0), "ninguna ciudad al sur");
        assert!(t.towns.iter().any(|c| c.c.0 < 0.0), "ninguna ciudad al oeste");
        assert!(t.towns.iter().any(|c| c.c.0 > 0.0), "ninguna ciudad al este");
        for (i, a) in t.towns.iter().enumerate() {
            for b in &t.towns[i + 1..] {
                let d = ((a.c.0 - b.c.0).powi(2) + (a.c.1 - b.c.1).powi(2)).sqrt();
                assert!(d > 0.35, "{} y {} están pegadas ({d:.2})", a.name, b.name);
            }
        }
    }

    /// Una ciudad no puede fundarse en el mar ni en la cumbre. Es la prueba de que
    /// la plantilla de relieve y la de ciudades hablan entre sí.
    #[test]
    fn named_cities_stand_on_habitable_ground() {
        let body = CelestialBody::Earth;
        for town in template(body).towns {
            let (cx, cz) = town_center(body, town);
            let l = relief_level(body, cx, cz);
            assert!(l >= 0, "{} está bajo el agua (nivel {l})", town.name);
            assert!(l <= 6, "{} está en la alta montaña (nivel {l})", town.name);
        }
    }

    /// **Todos los cuerpos tienen forma**: mar/tierra o cuenca/altiplano, nunca
    /// una plataforma plana. Si esto falla, ese cuerpo volvió a ser una alfombra.
    #[test]
    fn every_body_has_a_shaped_platform() {
        for b in landable() {
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            let r = body_walk_radius(b).max(1.0) as i64;
            for i in -30..30 {
                for j in -30..30 {
                    let h = platform(b, i * r / 30, j * r / 30);
                    lo = lo.min(h);
                    hi = hi.max(h);
                }
            }
            assert!(hi - lo > 0.25, "{b:?} tiene una plataforma plana ({lo:.2}..{hi:.2})");
        }
    }

    /// El borde del mundo baja SIEMPRE, aunque la plantilla declare una montaña
    /// cerca del confín: si no, el mapa terminaría en un muro.
    #[test]
    fn the_world_rim_always_falls_away() {
        for b in landable() {
            let r = body_walk_radius(b).max(1.0) as i64;
            let centre = platform(b, 0, 0);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1)] {
                let rim = platform(b, dx * r * 99 / 100, dz * r * 99 / 100);
                assert!(rim < centre, "{b:?} no baja hacia el borde ({rim:.2} vs {centre:.2})");
            }
        }
    }

    /// Los nombres de región cubren de verdad el mapa: recorrerlo tiene que dar
    /// varios sitios con nombre, no uno solo.
    #[test]
    fn the_earth_map_is_full_of_named_places() {
        let body = CelestialBody::Earth;
        let mut names = std::collections::HashSet::new();
        let r = body_walk_radius(body) as i64;
        for i in -40..40 {
            for j in -40..40 {
                if let Some(n) = region_name(body, i * r / 40, j * r / 40) {
                    names.insert(n);
                }
            }
        }
        assert!(names.len() >= 10, "sólo {} sitios con nombre: {names:?}", names.len());
    }

    /// **Hay lagunas repartidas por el mapa, fuera de las ciudades.** Es lo que
    /// convierte el campo entre núcleos en algo que se recorre: agua para beber,
    /// para pescar y para montar un campamento.
    #[test]
    fn ponds_are_scattered_across_the_countryside() {
        let body = CelestialBody::Earth;
        let mut ponds = 0;
        let mut in_town = 0;
        for rx in -12..12 {
            for rz in -12..12 {
                let Some(((cx, cz), r)) = pond_in(body, rx, rz) else { continue };
                assert!((60.0..=190.0).contains(&r), "laguna de tamaño raro: {r}");
                ponds += 1;
                if town_covering(body, cx, cz).is_some() {
                    in_town += 1;
                }
            }
        }
        assert!(ponds > 200, "sólo {ponds} lagunas candidatas en 24x24 celdas");
        // Las ciudades son cuatro puntos en 60 km: casi ninguna laguna cae dentro.
        assert!(in_town * 20 < ponds, "las lagunas caen dentro de las ciudades");
    }

    /// Una laguna se llena de verdad: el terreno baja bajo el nivel del mar y
    /// aparece agua nadable, con orilla firme alrededor.
    #[test]
    fn a_pond_actually_holds_water_with_a_walkable_shore() {
        let body = CelestialBody::Earth;
        let mut checked = 0;
        'outer: for rx in -12..12 {
            for rz in -12..12 {
                let Some(((cx, cz), _)) = pond_in(body, rx, rz) else { continue };
                // Sólo las que caen en tierra firme: una laguna sorteada sobre el
                // océano no se distingue del océano, y eso es correcto.
                if crate::relief::relief_level(body, cx + 400, cz) < 1 {
                    continue;
                }
                if crate::relief::relief_level(body, cx, cz) >= 0 {
                    continue;
                }
                assert!(
                    crate::relief::is_liquid(body, cx, cz),
                    "el centro de la laguna en ({cx},{cz}) no tiene agua"
                );
                checked += 1;
                if checked >= 5 {
                    break 'outer;
                }
            }
        }
        assert!(checked > 0, "ninguna laguna llegó a llenarse de agua");
    }

    /// Los cuerpos con líquido tienen mar de verdad alrededor de su tierra: la
    /// plantilla no puede dejar un planeta seco ni uno inundado.
    #[test]
    fn templates_leave_both_land_and_sea() {
        for b in landable() {
            if !crate::relief::has_liquid(b) {
                continue;
            }
            let (mut land, mut sea) = (0, 0);
            let r = body_walk_radius(b).max(1.0) as i64;
            for i in -30..30 {
                for j in -30..30 {
                    if relief_level(b, i * r / 30, j * r / 30) >= 0 {
                        land += 1;
                    } else {
                        sea += 1;
                    }
                }
            }
            let total = (land + sea) as f32;
            assert!(land as f32 / total > 0.12, "{b:?} casi sin tierra: {land}/{total}");
            assert!(sea as f32 / total > 0.08, "{b:?} casi sin mar: {sea}/{total}");
            let _ = sea_level(b);
        }
    }
}
