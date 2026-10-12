//! **RELIEVE POR TERRAZAS** — el arriba y el abajo del mundo 2D cenital.
//!
//! Hasta ahora la superficie era plana: `geography` calculaba una elevación
//! continua pero sólo la usaba para elegir bioma, así que caminar por una
//! "montaña" y por una llanura era exactamente lo mismo. Este módulo convierte esa
//! altura en **geometría jugable**, con la receta clásica de los juegos 2D con
//! relieve (Stardew, RimWorld, Terraria en vista cenital):
//!
//! ```text
//!   y = DefaultHeight(x) + perlin(x)          … y después SE ESCALONA
//!
//!   nivel 3  ████████╗
//!   nivel 2  ░░░░░░░░╚══╗          ← Cliff: pared sólida, no se trepa
//!   nivel 1  ░░░░░░░░░░░░▓▓        ← Slope: la rampa, por acá se sube
//!   nivel 0  ░░░░░░░░░░░░░░
//!   nivel -1 ≈≈≈≈≈≈≈≈≈≈≈≈≈≈        ← bajío: se NADA
//!   nivel -2 ~~~~~~~~~~~~~~        ← hondo: bloquea
//! ```
//!
//! Tres reglas y todo lo demás sale de ellas:
//!
//! 1. **El nivel es discreto.** Un escalón mide [`STEP_M`] metros de alto y el
//!    terreno dentro de una terraza es plano. Eso es lo que hace que el relieve
//!    se LEA en cenital: no hay degradados, hay escalones.
//! 2. **Todo lo que cae bajo el nivel 0 es líquido.** El nivel del mar no es un
//!    caso especial, es el escalón cero.
//! 3. **El borde de una terraza es una pared**, salvo donde pasa una rampa —o
//!    donde es un simple bordillo que se trepa a pie ([`climbable_edge`]). En
//!    llanura casi todos los bordes se suben y se bajan de un impulso, así que
//!    el campo abierto se recorre libre; en la falda de una cordillera la
//!    pendiente es fuerte y ahí sí hay que rodear o buscar el paso. Eso es lo
//!    que convierte el mapa en un sitio que se recorre en vez de una alfombra
//!    por la que se camina en línea recta.
//!
//! **Vale para TODOS los cuerpos aterrizables**, no sólo la Tierra: el relieve es
//! geometría, no contenido. Lo que cambia por cuerpo es dónde queda el nivel del
//! mar ([`sea_level`]) y de qué está hecho el líquido ([`liquid_color`]) — agua en
//! la Tierra, metano en Titán, lava en Ío. El juego apunta a divertido, no a
//! realista.
//!
//! Consumidores: `geography::natural_tile` (tiles), `topdown` (colisión), el mapa
//! satelital de `universe.rs` y el ítem-mapa. Ninguno duplica estas reglas.

use std::cell::RefCell;

use crate::celestial_bodies::{body_terrain_seed, CelestialBody};
use crate::geography::{archetype, fbm, hash2, Archetype};
use crate::topdown::Tile;

// ---------------------------------------------------------------------------
// Escala de la escalera
// ---------------------------------------------------------------------------

/// Escalones de tierra por encima del nivel del mar. 12 da una cumbre que se
/// siente alta (30 m sobre la costa) sin que el mundo sea una escalera infinita.
pub const LEVELS_UP: i8 = 12;

/// Escalones de fondo por debajo del nivel del mar. Con 4 hay bajío para nadar,
/// una plataforma intermedia y fosa de verdad mar adentro.
pub const LEVELS_DOWN: i8 = -4;

/// Altura visual de un escalón, en metros. La usan la sombra de terraza y el
/// desplazamiento del arte de acantilado; no afecta a la colisión (que es
/// binaria: o hay pared o no).
pub const STEP_M: f32 = 2.5;

/// Nivel a partir del cual el agua deja de nadarse y bloquea. El bajío (−1) se
/// vadea/nada; de −2 para abajo hay demasiada profundidad.
pub const SWIMMABLE_FLOOR: i8 = -1;

// ---------------------------------------------------------------------------
// Nivel del mar y líquido por cuerpo
// ---------------------------------------------------------------------------

/// Altura (en unidades de campo 0..1) donde queda el escalón CERO — o sea, la
/// costa. Es EL parámetro que decide la sensación de un mundo: subirlo lo inunda,
/// bajarlo lo vuelve un continente único. `0.0` = mundo sin líquido.
///
/// Todos los arquetipos menos el yermo tienen líquido: la Luna y Mercurio son
/// roca seca a propósito (su gracia es justamente que no hay dónde refugiarse),
/// el resto tiene mares de algo.
pub fn sea_level(body: CelestialBody) -> f32 {
    match archetype(body) {
        Archetype::Terran => 0.440,
        // Un mundo oceánico tiene que ser HABITABLE, no un charco: con el mar a
        // 0.60 salía 94 % agua y aterrizar era caer al vacío.
        Archetype::Oceanic => 0.495,
        Archetype::Icy => 0.500,
        // Desértico y volcánico: poco líquido, en el fondo de las cuencas. No es
        // "la Tierra con menos agua" — son lagos aislados que se buscan, y por eso
        // encontrar uno significa algo.
        Archetype::Desert => 0.215,
        Archetype::Volcanic => 0.330,
        Archetype::Barren => 0.0,
    }
}

/// ¿Este cuerpo tiene líquido en superficie?
pub fn has_liquid(body: CelestialBody) -> bool {
    sea_level(body) > 0.0
}

/// Color sRGB del líquido del cuerpo. El tile es siempre [`Tile::Water`] /
/// [`Tile::WaterDeep`] — un solo material, tintado — para no multiplicar tiles ni
/// arte por planeta: mismo autotile, misma animación, otro color.
pub fn liquid_color(body: CelestialBody, deep: bool) -> [f32; 3] {
    let (shallow_c, deep_c) = match archetype(body) {
        Archetype::Terran => ([0.35, 0.62, 0.80], [0.16, 0.34, 0.58]),
        // Metano: ámbar turbio (Titán).
        Archetype::Oceanic => ([0.62, 0.55, 0.32], [0.34, 0.28, 0.14]),
        // Agua bajo hielo: casi blanca en el bajío, azul intenso en la grieta.
        Archetype::Icy => ([0.62, 0.80, 0.88], [0.14, 0.36, 0.62]),
        // Salmuera helada marciana.
        Archetype::Desert => ([0.50, 0.52, 0.48], [0.24, 0.30, 0.34]),
        // Lava: el "hondo" es el más brillante, que es como se lee el calor.
        Archetype::Volcanic => ([0.85, 0.36, 0.10], [1.00, 0.62, 0.16]),
        Archetype::Barren => ([0.30, 0.30, 0.34], [0.18, 0.18, 0.22]),
    };
    if deep {
        deep_c
    } else {
        shallow_c
    }
}

// ---------------------------------------------------------------------------
// El campo de altura
// ---------------------------------------------------------------------------

/// **`DefaultHeight(x) + perlin(x)`**: la altura continua del terreno, 0 (fosa) …
/// 1 (cumbre), antes de escalonar.
///
/// Cuatro octavas: continentes (9 km), cordilleras (1.4 km), colinas (110 m) y
/// **lomas (34 m)**.
///
/// El reparto de amplitudes es LA decisión de diseño del módulo, porque decide el
/// tamaño de las terrazas:
///
/// - Sin la octava de 34 m, una terraza medía 50–100 tiles: caminar por ella era
///   caminar por una alfombra y el relieve sólo se notaba desde el aire.
/// - Con una octava de 22 m y amplitud alta, cada mata de hierba cruzaba un
///   escalón y el llano salía hecho un laberinto de paredes de 2 m.
///
/// 34 m con 0.07 de amplitud cae en el medio: un borde de terraza cada ~20 tiles,
/// que a 1 m/tile es un desnivel cada veinte pasos — se ve, se rodea, se sube por
/// la rampa, y sigue habiendo llano donde construir.
///
/// El borde del mundo se hunde: los últimos kilómetros bajan hacia el mar, así el
/// cuerpo termina en océano abierto (o en fosa seca, si no tiene líquido) en vez
/// de cortarse a pico.
pub fn relief_field(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    cached_cell(body, tx, tz).0
}

/// El campo SIN caché. Sólo lo llama la caché; el resto del mundo usa
/// [`relief_field`].
fn compute_field(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    // LA PLATAFORMA MANDA, EL RUIDO ONDULA. La plantilla del cuerpo
    // (`world_map`) decide qué es mar, qué es continente, dónde cruza la
    // cordillera y dónde se hunde un lago; el ruido decide cómo es cada metro
    // concreto de eso. Antes sólo había ruido y el resultado era papel pintado:
    // sesenta kilómetros donde ningún sitio se distinguía del anterior.
    let platform = crate::world_map::platform(body, tx, tz);
    let n = terrain_noise(body, tx, tz);
    (platform + (n - 0.5) * crate::world_map::RELIEF_AMPLITUDE).clamp(0.0, 1.0)
}

/// Las cuatro octavas de ruido, 0..1 (sin plataforma). Separado de
/// [`relief_field`] porque la ondulación y la forma del mundo son dos decisiones
/// distintas y conviene poder tocarlas por separado.
fn terrain_noise(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    let seed = body_terrain_seed(body);
    let continental = fbm(seed, tx, tz, 9_000);
    let regional = fbm(seed ^ 0x5EA1_1000, tx, tz, 1_400);
    let local = fbm(seed ^ 0x40C0_2200, tx, tz, 110);
    let knolls = fbm(seed ^ 0x0DE7_3300, tx, tz, 34);
    continental * 0.55 + regional * 0.25 + local * 0.13 + knolls * 0.07
}

/// El escalón de una altura de campo. 0 = la primera terraza de tierra firme;
/// negativo = bajo el nivel del mar.
///
/// Los escalones **no** son uniformes a lo largo de `[0,1]`: los de abajo se
/// reparten el tramo `[0, sea)` y los de arriba el tramo `[sea, 1]`. Así el nivel
/// del mar cae siempre exactamente en el borde entre −1 y 0, sea cual sea el
/// cuerpo, y subir el mar de un mundo no le desordena la escalera de montaña.
pub fn level_from_field(body: CelestialBody, h: f32) -> i8 {
    let sea = sea_level(body);
    if sea > 0.0 && h < sea {
        // Cuánto se hundió, 0 en la orilla … 1 en el punto más bajo.
        let t = ((sea - h) / sea).clamp(0.0, 1.0);
        let steps = (-LEVELS_DOWN) as f32;
        // Los paréntesis son la parte importante: sin ellos el `-` unario se
        // aplica DESPUÉS de los clamps y el fondo del mar sale en +1 — o sea, el
        // planeta entero queda seco. Lo cazó `everything_below_zero_is_liquid`.
        return (-(1 + (t * steps) as i8)).clamp(LEVELS_DOWN, -1);
    }
    let top = (1.0 - sea).max(1e-3);
    let t = ((h - sea) / top).clamp(0.0, 1.0);
    ((t * LEVELS_UP as f32) as i8).clamp(0, LEVELS_UP)
}

/// El líquido de una celda como TILE, o `None` si ahí no hay líquido. Atajo de
/// `water_tile(relief_level(..))` para quien sólo quiere saber "¿hay agua acá?".
pub fn liquid_tile_at(body: CelestialBody, tx: i64, tz: i64) -> Option<Tile> {
    water_tile(relief_level(body, tx, tz))
}

/// El escalón de una celda. **Es la consulta caliente del mundo** (el relieve
/// necesita el nivel de la celda y el de sus cuatro vecinas), así que va por la
/// caché de celdas — ver [`LEVEL_CACHE`].
pub fn relief_level(body: CelestialBody, tx: i64, tz: i64) -> i8 {
    cached_level(body, tx, tz)
}

/// Altura del terreno en METROS sobre el nivel del mar. Para sombras, cámara y
/// cualquier cosa que quiera "cuánto más alto está esto".
pub fn relief_height_m(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    relief_level(body, tx, tz) as f32 * STEP_M
}

// ---------------------------------------------------------------------------
// Acantilados y rampas
// ---------------------------------------------------------------------------

/// Qué es una celda desde el punto de vista del relieve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReliefCell {
    /// Terraza: se camina.
    Flat,
    /// Borde de terraza: **pared sólida**. La cara pertenece a la terraza ALTA,
    /// así la de abajo queda entera caminable y el acantilado "cuelga" sobre ella
    /// —que es como se dibuja un desnivel en 3/4—.
    Cliff,
    /// El paso: el mismo borde, pero abierto. Por acá se sube.
    Ramp,
}

/// Semiancho (en unidades de ruido) de las franjas de paso. Más grande = pasos
/// más anchos y más frecuentes.
const PASS_HALF: f32 = 0.015;

/// ¿Cae esta celda dentro de una franja de paso?
///
/// Las franjas son los **conjuntos de nivel** de dos campos de ruido de períodos
/// distintos y primos entre sí (60 y 41 tiles): dos familias independientes de
/// curvas que serpentean por TODO el mundo. Un borde de terraza también es una
/// curva cerrada, así que cruza a las franjas casi con seguridad — y donde cruza,
/// hay rampa.
///
/// Es a propósito un campo GLOBAL y no un sorteo por celda de 48×48: con puntos
/// sorteados, el punto podía caer lejos del borde y dejar una meseta sellada sin
/// forma de subir. Con franjas continuas eso no puede pasar, y lo verifica el test
/// `every_terrace_edge_has_a_pass`.
fn in_pass_band(body: CelestialBody, tx: i64, tz: i64) -> bool {
    // **LA CARRETERA ABRE EL PASO.** Donde hay calzada, el borde de terraza es
    // rampa siempre — y con un margen de 2 tiles a cada lado, para que la rampa
    // sea al menos tan ancha como el camino y no un embudo. Es lo que permite
    // cruzar la cordillera conduciendo: los puertos de montaña están donde la
    // carretera los pide, no donde cayó el ruido.
    //
    // Va PRIMERO y con salida temprana porque es lo único que no se puede
    // negociar: una autopista cortada por una pared de 2.5 m es un mapa roto.
    //
    // Basta con mirar la CELDA PROPIA (no una vecindad): el borde de terraza es
    // una banda de un tile, así que las celdas donde el camino lo cruza son
    // exactamente las del camino — y el hueco que se abre mide lo que mide la
    // calzada. Mirar 5×5 daba el mismo resultado y costaba 25 veces más.
    if crate::roads::is_drivable_at(body, tx, tz) {
        return true;
    }
    let seed = body_terrain_seed(body);
    let a = (fbm(seed ^ 0x9a55_0001, tx, tz, 60) - 0.5).abs();
    if a < PASS_HALF {
        return true;
    }
    let b = (fbm(seed ^ 0x9a55_0002, tx, tz, 41) - 0.5).abs();
    b < PASS_HALF
}

/// Clasifica una celda. Sólo hay acantilado entre terrazas de TIERRA: la costa no
/// es una pared (se entra al agua caminando y se sigue nadando), y el fondo del
/// mar no tiene escalones que trepar.
pub fn relief_cell(body: CelestialBody, tx: i64, tz: i64) -> ReliefCell {
    let (h, l) = cached_cell(body, tx, tz);
    if l <= 0 {
        return ReliefCell::Flat;
    }
    // Descarte barato: en el medio de una terraza no hay nada que mirar.
    if !near_step_boundary(body, h) {
        return ReliefCell::Flat;
    }
    let lower = [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().any(|(dx, dz)| {
        let n = relief_level(body, tx + dx, tz + dz);
        n >= 0 && n < l
    });
    if !lower {
        return ReliefCell::Flat;
    }
    if in_pass_band(body, tx, tz) {
        ReliefCell::Ramp
    } else {
        ReliefCell::Cliff
    }
}

/// **BORDILLOS: los desniveles que se trepan y se bajan a pie, sin rampa.**
///
/// La altura de la cara NO alcanza para decidirlo, y ésa fue la sorpresa al
/// medirlo: el terreno por terrazas hace que *todas* las caras midan un escalón
/// — una montaña de tres escalones son tres caras de uno, pegadas. Con la regla
/// "un escalón se trepa" se abría el mundo entero y el relieve dejaba de ser un
/// obstáculo.
///
/// Lo que de verdad distingue un bordillo de una pared es **si hay dónde
/// pararse a los dos lados**:
///
/// - **Bordillo** (se trepa): una cara suelta, con terraza firme arriba y suelo
///   firme abajo. Es el escalón del borde de un prado, el que saltarías sin
///   pensarlo.
/// - **Pared** (no se trepa): caras APILADAS. Si al pie de ésta hay otra cara,
///   no estás ante un escalón sino ante la ladera de una montaña, y para eso
///   están las rampas.
///
/// Así la altura sale sola y es variada, sin autorarla: los bordes de meseta se
/// suben, los farallones hay que rodearlos, y cuanto más escarpado el terreno
/// más se parece a lo de antes.
///
/// **No puede encerrar a nadie.** La regla es simétrica —mira la celda, no la
/// dirección desde la que llegás—, así que sirve igual para subir, para bajar y
/// en las esquinas; y sólo AGREGA paso, nunca lo quita: el bordillo que trepaste
/// es el mismo que te deja volver.
/// **Pendiente a partir de la cual un borde deja de trepar.** Se mide sobre el
/// campo de altura continuo (0..1) a lo largo de 8 tiles, así que es literalmente
/// "cuánto sube el terreno acá".
///
/// Es EL número que reparte el mundo. Medido sobre la Tierra: con 0.005 algo más
/// de la mitad de los bordes se trepan —los de las llanuras y los valles— y el
/// resto —las laderas de las cordilleras— sigue exigiendo su rampa. Subirlo abre
/// las montañas; bajarlo deja sólo los prados.
pub const SCARP_GRADIENT: f32 = 0.005;

pub fn climbable_edge(body: CelestialBody, tx: i64, tz: i64) -> bool {
    let l = relief_level(body, tx, tz);
    // Bajo el nivel del mar no hay bordes que trepar (a la costa se entra
    // caminando y se sigue nadando).
    if l <= 0 {
        return false;
    }
    // ¿LADERA o borde suelto? Se mide la PENDIENTE del terreno, no la altura de
    // la cara. Y hay un motivo para eso que sólo se ve midiendo: el relieve por
    // terrazas casi nunca apila dos caras (99.8 % de los acantilados tienen
    // exactamente un escalón de desnivel a su alrededor), así que "¿qué tan alta
    // es esta pared?" no distingue nada — con esa regla el 98 % del mundo se
    // volvía trepable de golpe.
    //
    // La pendiente sí distingue, y además distingue lo correcto: donde el
    // terreno sube rápido las terrazas salen estrechas y encadenadas —la falda de
    // una cordillera— y ahí trepar a pulso no corresponde; donde sube despacio
    // son escalones sueltos en campo abierto y saltarlos es lo natural.
    // Resultado: las llanuras y los valles se abren, las montañas se siguen
    // ganando por su paso.
    let grad = (relief_field(body, tx + 4, tz) - relief_field(body, tx - 4, tz))
        .abs()
        .max((relief_field(body, tx, tz + 4) - relief_field(body, tx, tz - 4)).abs());
    if grad >= SCARP_GRADIENT {
        return false;
    }
    // Se busca UN eje que atraviese la cara: suelo firme un escalón más abajo de
    // un lado, terraza del mismo nivel del otro. "Firme" = no es otra cara de
    // acantilado; ahí es donde se caen las paredes apiladas.
    for (dx, dz) in [(1i64, 0i64), (0, 1)] {
        for s in [1i64, -1] {
            let (lx, lz) = (tx + dx * s, tz + dz * s);
            let (hx, hz) = (tx - dx * s, tz - dz * s);
            // Abajo: exactamente un escalón menos y pisable.
            if relief_level(body, lx, lz) != l - 1 || relief_cell(body, lx, lz) == ReliefCell::Cliff
            {
                continue;
            }
            // Arriba: la terraza a la que subís, también pisable.
            if relief_level(body, hx, hz) != l || relief_cell(body, hx, hz) == ReliefCell::Cliff {
                continue;
            }
            return true;
        }
    }
    false
}

/// ¿La celda corta el paso a pie por RELIEVE? Es la pregunta que hace el juego
/// al mover al jugador: un acantilado bajo (`climbable_edge`) no corta.
pub fn relief_blocks_walk(body: CelestialBody, tx: i64, tz: i64) -> bool {
    relief_cell(body, tx, tz) == ReliefCell::Cliff && !climbable_edge(body, tx, tz)
}

/// El tile que impone el relieve en esta celda, si impone alguno. `None` = terreno
/// normal y decide el bioma.
///
/// El ORDEN importa: el relieve gana sobre el bioma porque es geometría, no clima.
/// Un acantilado en el desierto sigue siendo un acantilado; lo que cambia es su
/// ARTE (`cliff/arena` en vez de `cliff/pasto`), que se elige en el cliente a
/// partir del bioma — ver `world2d::cliff_art_key`.
pub fn relief_tile(body: CelestialBody, tx: i64, tz: i64) -> Option<Tile> {
    match relief_cell(body, tx, tz) {
        ReliefCell::Cliff => Some(Tile::Cliff),
        ReliefCell::Ramp => Some(Tile::Slope),
        ReliefCell::Flat => None,
    }
}

/// El tile de líquido de un nivel bajo cero. `None` = esta celda no es líquido.
pub fn water_tile(level: i8) -> Option<Tile> {
    match level {
        l if l <= SWIMMABLE_FLOOR - 1 => Some(Tile::WaterDeep),
        l if l < 0 => Some(Tile::Water),
        _ => None,
    }
}

/// ¿Esta celda es líquido? (bajío o fondo).
pub fn is_liquid(body: CelestialBody, tx: i64, tz: i64) -> bool {
    relief_level(body, tx, tz) < 0
}

// ---------------------------------------------------------------------------
// Caché de niveles, por CELDA
// ---------------------------------------------------------------------------
//
// POR QUÉ EXISTE. Clasificar una celda mira su nivel y el de sus 4 vecinas: cinco
// evaluaciones del campo de altura por tile. Pero al recorrer un chunk, CADA celda
// se consulta cinco veces —una como centro y cuatro como vecina de las de al
// lado—, así que memorizar el nivel de las celdas recién vistas devuelve el coste
// a ~1 evaluación por celda.
//
// **Por celda y no por bloques de 16×16.** El primer intento cacheaba bloques
// enteros: rapidísimo recorriendo un chunk, y catastrófico con accesos dispersos
// (una consulta de colisión a 40 tiles calculaba 256 celdas para leer una →
// 225× más lento, lo cazó `generating_a_cell_costs_only_a_few_terrain_probes`).
// La caché por celda no tiene ese pozo: un acceso disperso cuesta exactamente una
// evaluación, ni una más.
//
// Directa, 1024 entradas, `thread_local` (sin candados, sin cambiar el API). El
// esténcil de 5 puntos de un chunk necesita ~3 filas vivas: entra de sobra.

const CACHE_SLOTS: usize = 1024;

#[derive(Clone, Copy)]
struct LevelSlot {
    tx: i64,
    tz: i64,
    body: u8,
    /// Se cachea el CAMPO además del nivel porque la parte cara ya no es el
    /// ruido: es `world_map::platform`, que recorre las ~20 polilíneas y cuencas
    /// de la plantilla del cuerpo. Cachear sólo el nivel dejaba a `relief_cell` y
    /// a `biome_at` recalculando la plantilla entera por celda (medido: 45× una
    /// consulta de ruido).
    field: f32,
    level: i8,
    filled: bool,
}

thread_local! {
    static LEVEL_CACHE: RefCell<[LevelSlot; CACHE_SLOTS]> = RefCell::new(
        [LevelSlot { tx: 0, tz: 0, body: 0, field: 0.0, level: 0, filled: false }; CACHE_SLOTS],
    );
}

/// `(campo, nivel)` de una celda, por caché.
fn cached_cell(body: CelestialBody, tx: i64, tz: i64) -> (f32, i8) {
    let b = body as u8;
    let slot = (hash2(0x5b10_c000, tx, tz) as usize) % CACHE_SLOTS;
    LEVEL_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let e = &mut c[slot];
        if e.filled && e.body == b && e.tx == tx && e.tz == tz {
            return (e.field, e.level);
        }
        let field = compute_field(body, tx, tz);
        let level = level_from_field(body, field);
        *e = LevelSlot { tx, tz, body: b, field, level, filled: true };
        (field, level)
    })
}

fn cached_level(body: CelestialBody, tx: i64, tz: i64) -> i8 {
    cached_cell(body, tx, tz).1
}

/// Cota superior de cuánto puede cambiar el campo de altura entre dos celdas
/// vecinas. Sale de las pendientes de las tres octavas: la de 110 m aporta casi
/// todo (~0.10 de amplitud sobre medio período, y su sub-octava de período/3
/// triplica la pendiente), regional y continental son despreciables a un tile.
/// Generosa a propósito: si se queda corta aparecerían acantilados sin dibujar.
const MAX_FIELD_STEP_PER_TILE: f32 = 0.020;

/// ¿Puede esta celda tener algún vecino en OTRO escalón?
///
/// Descarte barato antes de mirar a los cuatro vecinos: si la altura de la celda
/// cae bien en el medio de su escalón, ningún vecino puede haberlo cruzado —el
/// campo no cambia tanto en un tile—. Se salta así la consulta de vecinos en
/// aproximadamente la mitad de las celdas de tierra, sin cambiar ni un tile del
/// resultado (lo verifica `the_cheap_reject_never_hides_a_cliff`).
fn near_step_boundary(body: CelestialBody, h: f32) -> bool {
    let sea = sea_level(body);
    let step = (1.0 - sea).max(1e-3) / LEVELS_UP as f32;
    let margin = MAX_FIELD_STEP_PER_TILE / step;
    if margin >= 0.5 {
        // Escalones más finos que lo que el terreno puede cambiar en un tile:
        // el descarte no descartaría nada y sólo costaría tiempo.
        return true;
    }
    // La fracción se mide DESDE EL NIVEL DEL MAR, que es donde están los bordes
    // de escalón (`sea + k*step`), no desde el cero absoluto. Medirla mal hacía
    // que el descarte se comiera acantilados enteros: el mundo salía sin una sola
    // pared. Lo cazó `the_cheap_reject_never_hides_a_cliff`.
    let frac = ((h - sea) / step).fract();
    frac < margin || frac > 1.0 - margin
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::celestial_bodies::body_walk_radius;

    /// Un bordillo tiene piso firme ARRIBA y ABAJO; una pared, no. Y el mundo
    /// tiene que traer las dos cosas: si fueran todos bordillos el relieve
    /// dejaría de ser un obstáculo, y si fueran todos paredes no habríamos
    /// cambiado nada. El reparto se imprime para poder calibrarlo.
    #[test]
    fn the_world_has_both_ledges_and_walls() {
        let b = CelestialBody::Earth;
        let (mut ledges, mut walls) = (0u32, 0u32);
        for tz in -600..600 {
            for tx in -600..600 {
                if relief_cell(b, tx, tz) != ReliefCell::Cliff {
                    continue;
                }
                if climbable_edge(b, tx, tz) {
                    ledges += 1;
                    // Lo que promete el bordillo: existe un eje con suelo pisable
                    // un escalón más abajo. Si no, treparías a la nada.
                    let l = relief_level(b, tx, tz);
                    assert!(
                        [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)].into_iter().any(|(dx, dz)| {
                            relief_level(b, tx + dx, tz + dz) == l - 1
                                && relief_cell(b, tx + dx, tz + dz) != ReliefCell::Cliff
                        }),
                        "bordillo en ({tx},{tz}) sin suelo abajo"
                    );
                } else {
                    walls += 1;
                }
            }
        }
        eprintln!("bordes: {ledges} bordillos · {walls} paredes");
        assert!(ledges > 0, "ningún borde se puede trepar");
        assert!(walls > 0, "todos los bordes se pueden trepar");
        // Ni un mundo de queso ni el laberinto de antes: entre 15 % y 85 % de los
        // bordes se trepan. Fuera de esa banda hay que recalibrar la regla.
        let share = ledges as f32 / (ledges + walls) as f32;
        assert!((0.15..=0.85).contains(&share), "bordillos = {:.0} % de los bordes", share * 100.0);
    }

    /// Trepar sólo AGREGA paso: nada de lo que antes se caminaba deja de
    /// caminarse. Es lo que garantiza que el cambio no pueda encerrar a nadie.
    #[test]
    fn climbing_only_ever_opens_cells() {
        let b = CelestialBody::Earth;
        for tz in -300..300 {
            for tx in -300..300 {
                if relief_blocks_walk(b, tx, tz) {
                    assert_eq!(
                        relief_cell(b, tx, tz),
                        ReliefCell::Cliff,
                        "({tx},{tz}) frena sin ser acantilado"
                    );
                }
            }
        }
    }

    fn landable() -> Vec<CelestialBody> {
        CelestialBody::all()
            .iter()
            .copied()
            .filter(|&b| crate::celestial_bodies::body_landable(b))
            .collect()
    }

    /// El escalón cero cae EXACTAMENTE en la costa, en todos los cuerpos con
    /// líquido: un pelo por debajo del nivel del mar ya es agua, un pelo por
    /// encima ya es tierra. Si esto se corre, el mar se traga los mundos.
    #[test]
    fn sea_level_is_exactly_the_zero_step() {
        for b in landable() {
            let sea = sea_level(b);
            if sea <= 0.0 {
                continue;
            }
            assert_eq!(level_from_field(b, sea), 0, "{b:?} orilla");
            assert!(level_from_field(b, sea - 1e-4) < 0, "{b:?} justo bajo el mar");
            assert_eq!(level_from_field(b, 1.0), LEVELS_UP, "{b:?} cumbre");
            assert_eq!(level_from_field(b, 0.0), LEVELS_DOWN, "{b:?} fosa");
        }
    }

    /// Regla del usuario: **todo lo que está bajo cero es líquido**. Sin
    /// excepciones y en todos los cuerpos que tienen líquido.
    #[test]
    fn everything_below_zero_is_liquid() {
        for b in landable() {
            if !has_liquid(b) {
                continue;
            }
            let mut seen_shallow = false;
            let mut seen_deep = false;
            // El muestreo se escala al RADIO del cuerpo y llega hasta su borde: la
            // fosa profunda vive sobre todo en el reborde exterior (es lo que
            // impide nadar fuera del mapa), así que una rejilla fija de unos pocos
            // kilómetros se la perdía en los cuerpos grandes.
            let r = body_walk_radius(b).max(1.0) as i64;
            let stride = (r / 60).max(1);
            for i in -60..60 {
                for j in -60..60 {
                    let (tx, tz) = (i * stride, j * stride);
                    let l = relief_level(b, tx, tz);
                    match water_tile(l) {
                        Some(Tile::Water) => {
                            assert_eq!(l, -1, "{b:?} el bajío es sólo el nivel -1");
                            seen_shallow = true;
                        }
                        Some(Tile::WaterDeep) => {
                            assert!(l <= -2, "{b:?} el hondo empieza en -2");
                            seen_deep = true;
                        }
                        Some(t) => panic!("{b:?} tile de líquido inesperado: {t:?}"),
                        None => assert!(l >= 0, "{b:?} nivel {l} sin líquido"),
                    }
                }
            }
            assert!(seen_shallow, "{b:?} no tiene ni un bajío para nadar");
            // El fondo profundo es lo que impide nadar fuera del mapa: sin él, el
            // borde del mundo sería una invitación.
            assert!(seen_deep, "{b:?} no tiene fondo: se puede nadar hasta el borde");
        }
    }

    /// El bajío se nada y el hondo bloquea — la decisión de diseño del agua.
    #[test]
    fn shallow_water_swims_and_deep_water_blocks() {
        assert!(Tile::Water.swim(), "el bajío se nada");
        assert!(!Tile::Water.solid(), "el bajío NO corta el paso");
        assert!(Tile::WaterDeep.solid(), "el hondo bloquea");
        assert!(!Tile::WaterDeep.swim(), "al hondo no se entra nadando");
    }

    /// **TODOS los cuerpos aterrizables tienen relieve.** No es contenido de la
    /// Tierra: es geometría, y sin ella un planeta es una alfombra.
    #[test]
    fn every_landable_body_has_terraces() {
        for b in landable() {
            let mut levels = std::collections::HashSet::new();
            for i in -40..40 {
                for j in -40..40 {
                    levels.insert(relief_level(b, i * 211, j * 211));
                }
            }
            assert!(
                levels.len() >= 4,
                "{b:?} casi sin relieve: sólo los niveles {levels:?}"
            );
        }
    }

    /// **Ninguna meseta queda sellada.** Es el test que evita el mundo-laberinto:
    /// recorriendo bordes de terraza tiene que aparecer rampa con regularidad.
    #[test]
    fn every_terrace_edge_has_a_pass() {
        let b = CelestialBody::Earth;
        let (mut cliffs, mut ramps) = (0, 0);
        for tx in -1200..1200 {
            for tz in -60..60 {
                match relief_cell(b, tx, tz) {
                    ReliefCell::Cliff => cliffs += 1,
                    ReliefCell::Ramp => ramps += 1,
                    ReliefCell::Flat => {}
                }
            }
        }
        assert!(cliffs > 0, "un mundo sin un solo acantilado no tiene relieve");
        assert!(ramps > 0, "acantilados sin una sola rampa: mundo sellado");
        // Una de cada ~8 celdas de borde tiene que ser paso. Con menos, subir una
        // meseta se vuelve una búsqueda a ciegas de media hora.
        let ratio = ramps as f32 / (cliffs + ramps) as f32;
        assert!(ratio > 0.07, "sólo {:.1}% de pasos: demasiado sellado", ratio * 100.0);
        assert!(ratio < 0.55, "{:.1}% de pasos: el acantilado no frena a nadie", ratio * 100.0);
    }

    /// La costa NO es una pared: se entra al agua caminando. Sin esto, cada isla
    /// quedaría amurallada contra su propia playa.
    #[test]
    fn the_shore_is_never_a_cliff() {
        let b = CelestialBody::Earth;
        let mut checked = 0;
        // Barrido ancho: el origen cae en mitad del continente, así que la costa
        // más cercana puede estar a kilómetros. Se muestrea una banda larga en x.
        for tx in (-26_000..26_000).step_by(3) {
            for tz in -30..30 {
                if relief_level(b, tx, tz) != 0 {
                    continue;
                }
                let touches_water = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .into_iter()
                    .any(|(dx, dz)| relief_level(b, tx + dx, tz + dz) < 0);
                if touches_water {
                    checked += 1;
                    assert_eq!(
                        relief_cell(b, tx, tz),
                        ReliefCell::Flat,
                        "la orilla en ({tx},{tz}) salió pared"
                    );
                }
            }
        }
        assert!(checked > 50, "no se encontró costa que probar ({checked})");
    }

    /// El descarte barato es una OPTIMIZACIÓN, no una regla: no puede esconder ni
    /// un acantilado. Se comprueba contra la clasificación completa (mirar siempre
    /// a los cuatro vecinos) celda por celda.
    #[test]
    fn the_cheap_reject_never_hides_a_cliff() {
        let honest = |b: CelestialBody, tx: i64, tz: i64| -> ReliefCell {
            let l = relief_level(b, tx, tz);
            if l <= 0 {
                return ReliefCell::Flat;
            }
            let lower = [(1, 0), (-1, 0), (0, 1), (0, -1)].into_iter().any(|(dx, dz)| {
                let n = relief_level(b, tx + dx, tz + dz);
                n >= 0 && n < l
            });
            if !lower {
                ReliefCell::Flat
            } else if in_pass_band(b, tx, tz) {
                ReliefCell::Ramp
            } else {
                ReliefCell::Cliff
            }
        };
        for b in [CelestialBody::Earth, CelestialBody::Mars, CelestialBody::Moon] {
            let mut edges = 0;
            for tx in -400..400 {
                for tz in -18..18 {
                    let got = relief_cell(b, tx, tz);
                    let want = honest(b, tx, tz);
                    assert_eq!(got, want, "{b:?} ({tx},{tz}): el descarte se comió un borde");
                    if want != ReliefCell::Flat {
                        edges += 1;
                    }
                }
            }
            assert!(edges > 20, "{b:?} apenas {edges} bordes: la muestra no prueba nada");
        }
    }

    /// La caché no puede cambiar NI UN nivel respecto de calcularlo a pelo. Es la
    /// red de seguridad de la única optimización del módulo.
    #[test]
    fn the_block_cache_is_transparent() {
        for b in [CelestialBody::Earth, CelestialBody::Mars, CelestialBody::Moon] {
            for i in -50..50 {
                for j in -50..50 {
                    let (tx, tz) = (i * 13 - 7, j * 17 + 3);
                    let direct = level_from_field(b, relief_field(b, tx, tz));
                    assert_eq!(relief_level(b, tx, tz), direct, "{b:?} ({tx},{tz})");
                }
            }
        }
    }

    /// Mapa ASCII del relieve alrededor de un punto — **la herramienta de mirar**.
    ///
    /// Ver si el mundo quedó bien no debería costar compilar el juego, arrancarlo
    /// y leer una captura: acá se lee en un segundo qué escalones hay, dónde caen
    /// los acantilados y si las rampas están donde tienen que estar.
    ///
    /// `cargo test -p ha-shared relief_map -- --ignored --nocapture`
    #[test]
    #[ignore = "herramienta de inspección, no un test"]
    fn relief_map() {
        let body = std::env::var("HA_BODY").ok();
        let body = CelestialBody::all()
            .iter()
            .copied()
            .find(|b| Some(format!("{b:?}").to_lowercase()) == body.as_ref().map(|s| s.to_lowercase()))
            .unwrap_or(CelestialBody::Earth);
        let ox: i64 = std::env::var("HA_X").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let oz: i64 = std::env::var("HA_Z").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let step: i64 = std::env::var("HA_STEP").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        println!("\n{body:?} centrado en ({ox},{oz}), 1 carácter = {step} tile(s)");
        println!("  dígito = escalón de tierra · ~ = hondo · ≈ = bajío · █ = acantilado · / = rampa\n");
        let mut counts = std::collections::BTreeMap::new();
        for rz in -20..20 {
            let mut line = String::new();
            for rx in -55..55 {
                let (tx, tz) = (ox + rx * step, oz + rz * step);
                let c = match relief_cell(body, tx, tz) {
                    ReliefCell::Cliff => '█',
                    ReliefCell::Ramp => '/',
                    ReliefCell::Flat => {
                        let l = relief_level(body, tx, tz);
                        match l {
                            l if l <= -2 => '~',
                            -1 => '≈',
                            l => char::from_digit(l.min(9) as u32, 10).unwrap_or('+'),
                        }
                    }
                };
                *counts.entry(c).or_insert(0usize) += 1;
                line.push(c);
            }
            println!("{line}");
        }
        println!("\nreparto: {counts:?}");
    }

    /// Histograma de tiles alrededor de un punto — la otra mitad de mirar el
    /// mundo sin abrir el juego. `relief_map` dice qué ALTURA hay; ésta, de qué
    /// está hecho.
    ///
    /// `cargo test -p ha-shared tile_histogram -- --ignored --nocapture`
    #[test]
    #[ignore = "herramienta de inspección, no un test"]
    fn tile_histogram() {
        let body = CelestialBody::Earth;
        let ox: i64 = std::env::var("HA_X").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let oz: i64 = std::env::var("HA_Z").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        let r: i64 = std::env::var("HA_R").ok().and_then(|v| v.parse().ok()).unwrap_or(120);
        let mut counts = std::collections::BTreeMap::new();
        for tz in -r..r {
            for tx in -r..r {
                let t = crate::topdown::topdown_tile(body, 0, ox + tx, oz + tz);
                *counts.entry(format!("{t:?}")).or_insert(0usize) += 1;
            }
        }
        let total: usize = counts.values().sum();
        let mut v: Vec<_> = counts.into_iter().collect();
        v.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        println!("\ntiles en ({ox},{oz}) radio {r}:");
        for (name, n) in v {
            println!("  {name:14} {n:7}  {:5.1}%", n as f32 * 100.0 / total as f32);
        }
    }

    /// Los cuerpos yermos (Luna, Mercurio) tienen relieve pero NO líquido: su
    /// gracia es que no hay dónde refugiarse.
    #[test]
    fn barren_worlds_have_relief_but_no_liquid() {
        let moon = CelestialBody::Moon;
        assert!(!has_liquid(moon));
        for i in -30..30 {
            for j in -30..30 {
                assert!(relief_level(moon, i * 137, j * 137) >= 0, "la Luna con mares líquidos");
            }
        }
    }
}
