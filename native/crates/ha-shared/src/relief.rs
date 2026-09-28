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
/// siente alta sin que el mundo sea una escalera infinita.
///
/// **Cuánto mide esa cumbre no sale de acá**, sale de [`SALTO_DE_TERRAZA`]: la
/// escalera no es uniforme, así que la terraza 12 está a 84 escalones ≈ **210 m**
/// sobre la costa, no a `12 × STEP_M`. El "30 m" que decía esta línea era de
/// cuando todas las terrazas medían lo mismo.
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

/// Peso conjunto de las dos octavas GRANDES en [`terrain_noise`] (continental y
/// regional). Sirve para renormalizar cuando se descartan las dos finas: sin
/// esto el campo suave quedaría centrado en 0,40 en vez de en 0,50 y toda la
/// cordillera bajaría dos escalones.
const PESO_DE_LAS_GRANDES: f32 = 0.55 + 0.25;

/// **LA ALTURA A ESCALA DE CORDILLERA** — el campo sin el temblor de las lomas.
///
/// # Para qué hace falta un segundo campo
///
/// El campo de altura lleva encima la octava de lomas de 34 m
/// ([`terrain_noise`]), que es la que hace que caminar por una terraza no sea
/// caminar por una alfombra. Perfecta para la geometría, **veneno para una línea
/// de nieve**: la frontera de cualquier umbral sobre un campo así no es una curva
/// de nivel, es una greca fractal, y donde el campo pasa cerca del umbral en una
/// zona ancha —la cresta de la cordillera— la greca **llena la zona**.
///
/// Medido en el sitio que retrata `relieve-0-pared.png` (1763,-5087): la pantalla
/// salía **73 % nieve y 27 % montaña entreveradas**, con el **34 % de las celdas
/// dentro del damero de transición**. Y no era el ancho de la franja —medida al
/// cruzarla da 1 tile de mediana y el 95 % por debajo de 8, que es justo lo que
/// pide `AJEDREZ_EN_PASOS`—: era que la frontera pasa por todas partes. En
/// pantalla eso es confeti blanco y verde de borde a borde.
///
/// La nieve es un fenómeno de **escala de cordillera**, no de loma: a qué altura
/// empieza no lo deciden los montículos de treinta metros. Así que para ESA
/// decisión —y sólo para ésa— el campo se promedia en una cruz de
/// [`RADIO_DE_CORDILLERA`] tiles. Nueve consultas, todas contra la caché de
/// celdas, que desde el arreglo de la caché espacial cuestan ~0,006 µs cada una.
///
/// **No toca la geometría**: los escalones, los acantilados y las rampas siguen
/// saliendo del campo completo. Lo único que se decide acá es de qué está hecho
/// el suelo, que es exactamente lo que ya hacía el difuminado.
pub fn campo_de_cordillera(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    let seed = body_terrain_seed(body);
    let continental = fbm(seed, tx, tz, 9_000);
    let regional = fbm(seed ^ 0x5EA1_1000, tx, tz, 1_400);
    let n = (continental * 0.55 + regional * 0.25) / PESO_DE_LAS_GRANDES;
    let h = crate::world_map::platform(body, tx, tz) + (n - 0.5) * crate::world_map::RELIEF_AMPLITUDE;
    h.clamp(0.0, 1.0)
}

/// El escalón que le tocaría a una celda **según la altura de cordillera**, y en
/// qué parte de ese escalón está. Es el par que usa la línea de nieve
/// (`geography::biome_calc`), con la misma escalera que el resto del mundo.
pub fn escalon_de_cordillera(body: CelestialBody, tx: i64, tz: i64) -> (i8, f32) {
    let h = campo_de_cordillera(body, tx, tz);
    let sea = sea_level(body);
    if sea > 0.0 && h < sea {
        return (level_from_field(body, h), 0.0);
    }
    let top = (1.0 - sea).max(1e-3);
    let t = ((h - sea) / top).clamp(0.0, 1.0) * LEVELS_UP as f32;
    (level_from_field(body, h), (t - t.floor()).clamp(0.0, 1.0))
}

/// El campo SIN caché. Sólo lo llama la caché; el resto del mundo usa
/// [`relief_field`].

fn compute_field(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    campo_crudo(body, tx, tz).clamp(0.0, 1.0)
}

/// El cuerpo del campo: la plataforma, el ruido y el valle del río.
fn campo_crudo(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    // LA PLATAFORMA MANDA, EL RUIDO ONDULA. La plantilla del cuerpo
    // (`world_map`) decide qué es mar, qué es continente, dónde cruza la
    // cordillera y dónde se hunde un lago; el ruido decide cómo es cada metro
    // concreto de eso. Antes sólo había ruido y el resultado era papel pintado:
    // sesenta kilómetros donde ningún sitio se distinguía del anterior.
    let platform = crate::world_map::platform(body, tx, tz);
    let n = terrain_noise(body, tx, tz);
    let base = platform + (n - 0.5) * crate::world_map::RELIEF_AMPLITUDE;
    // **Y EL RÍO EXCAVA SU VALLE.** Sin esto el cauce es una cinta de agua
    // colgada sobre el relieve: la amplitud del ruido es 0,42 y se come cualquier
    // hondura que se le reste, así que el fondo del río sube y baja con el
    // terreno y seguirlo aguas abajo muere a las dos celdas. El río impone su
    // COTA (`rios::perfil_del_cauce`), que baja zancada a zancada por
    // construcción; el peso la apaga hacia el borde del valle.
    //
    // No hay recursión: `perfil_del_cauce` sólo consulta la plataforma y las
    // lagunas, nunca el campo ni el escalón — que además se calculan dentro del
    // préstamo de `LEVEL_CACHE`.
    //
    // **EL RUIDO SE ATENÚA, NO SE ANULA.** El empalme era
    // `base × (1−peso) + cota × peso`, y como `base` lleva el ruido dentro, en el
    // eje del cauce el terreno quedaba perfectamente liso y a tres tiles volvía
    // entero: pegaba un campo suave contra uno rugoso en el ancho del valle, y
    // esa costura ES un escalón. Medido, hasta 0,06 de campo por tile, el triple
    // de [`MAX_FIELD_STEP_PER_TILE`] — el propio río fabricaba los farallones que
    // después lo cortaban en zig-zag. Ahora la plataforma se lleva a la cota y el
    // ruido se atenúa aparte con el mismo peso suave: el fondo sigue ondulando
    // con el terreno y lo que el río impone es su hondura RELATIVA.
    let h = match crate::rios::perfil_del_cauce(body, tx, tz) {
        Some((cota, peso)) => {
            let ondula = (n - 0.5)
                * crate::world_map::RELIEF_AMPLITUDE
                * (1.0 - peso * (1.0 - crate::rios::RUIDO_EN_EL_CAUCE));
            platform * (1.0 - peso) + cota * peso + ondula
        }
        None => base,
    };
    rellenar_pueblo(body, tx, tz, h).clamp(0.0, 1.0)
}

/// **NINGÚN HUECO DE MAR ADENTRO DE UN PUEBLO** (27 sep 2026).
///
/// El pueblo se funda mirando la forma GRUESA del continente
/// (`geography::continental_at`, una octava), pero su casco mide cientos de
/// metros y el ruido fino hunde huecos bajo el nivel del mar dentro de él:
/// lagos irregulares en medio de las manzanas, con la calle cruzándolos
/// (medido: 2.616 de 2.623 celdas de agua de un casco eran nivel < 0, sin río ni
/// laguna). Adentro del casco, donde la forma gruesa dice TIERRA, el terreno se
/// levanta a media grada sobre el mar. Donde la forma gruesa dice mar (la costa
/// de una ciudad costera) no se toca: la ciudad sigue terminando en su orilla.
///
/// No hay recursión: `settlement_covering` y `continental_at` usan la plataforma
/// sin lagunas y nunca el campo del relieve.
fn rellenar_pueblo(body: CelestialBody, tx: i64, tz: i64, h: f32) -> f32 {
    let sea = sea_level(body);
    if sea <= 0.0 || h >= sea {
        return h;
    }
    if crate::geography::settlement_covering(body, tx, tz).is_none() {
        return h;
    }
    if crate::geography::continental_at(body, tx, tz) < sea + 0.01 {
        return h;
    }
    let paso = (1.0 - sea).max(1e-3) / LEVELS_UP as f32;
    h.max(sea + paso * 0.5)
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

/// **EN QUÉ PARTE DE SU ESCALÓN ESTÁ UNA CELDA**: 0 = recién lo pisó, 1 = a un
/// pelo de subir al siguiente.
///
/// [`relief_level`] cuantiza y tira esta información, y sin ella el mundo sólo
/// sabe decir "esta celda está en el escalón 8 y la de al lado en el 9" — que es
/// un corte a escuadra. La transición entre dos biomas de altura la necesita
/// para saber **cuánto le falta** a una celda para ser cumbre, y difuminar el
/// borde en vez de trazarlo (`geography::pasa_el_escalon`).
///
/// Bajo el mar devuelve 0: ahí la escalera es la de las cuencas y nadie difumina
/// un fondo marino.
pub fn nivel_fraccion(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    let h = relief_field(body, tx, tz);
    let sea = sea_level(body);
    if sea > 0.0 && h < sea {
        return 0.0;
    }
    let top = (1.0 - sea).max(1e-3);
    let t = ((h - sea) / top).clamp(0.0, 1.0) * LEVELS_UP as f32;
    (t - t.floor()).clamp(0.0, 1.0)
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
///
/// **Esto era `nivel × STEP_M`**, o sea la fórmula de cuando todas las terrazas
/// medían lo mismo: daba 30 m en la cumbre contra los 210 m que suma
/// [`SALTO_DE_TERRAZA`]. No lo llamaba nadie, que es lo único que evitó que
/// alguien dibujara una cordillera de treinta metros. La altura sale de la
/// escalera y de ningún otro sitio.
pub fn relief_height_m(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    altura_de_terraza(relief_level(body, tx, tz)) as f32 * STEP_M
}

// ---------------------------------------------------------------------------
// Acantilados y rampas
// ---------------------------------------------------------------------------

/// Qué es una celda desde el punto de vista del relieve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReliefCell {
    /// Terraza: se camina.
    Flat,
    /// **El canto**: la fila de arriba de una pared, el borde de la meseta. Es la
    /// celda que dibuja el muro entero (labio + cuerpo + pie) colgándolo hacia el
    /// sur, sobre las celdas de [`ReliefCell::Pared`].
    Cliff,
    /// **El cuerpo de la pared**: las celdas que el muro TAPA con su dibujo. No se
    /// caminan —estás dentro de la roca— y no dibujan nada por su cuenta: son la
    /// huella del muro que cuelga del canto que tienen al norte.
    ///
    /// Es la pieza que faltaba. Mientras toda pared medía un escalón, la roca
    /// cubría dos celdas caminables y el jugador se metía dentro del dibujo sin
    /// que se notara demasiado; con paredes de diez pisos, once celdas de roca
    /// atravesable son un agujero en el mundo.
    Pared,
    /// El paso: el mismo desnivel, pero abierto. Por acá se sube, y se sube
    /// RECORRIENDO — un paso de una pared de diez pisos mide once celdas de
    /// largo, no una (biblia §5: nada de teletransporte por tile).
    Ramp,
}

// ---------------------------------------------------------------------------
// LA ESCALERA DEL MUNDO: cuánto mide cada terraza
// ---------------------------------------------------------------------------

/// **CUÁNTOS ESCALONES SE LEVANTA CADA TERRAZA SOBRE LA ANTERIOR.**
///
/// Ésta es LA constante del relieve, y hasta ahora no existía: la altura de una
/// terraza era `nivel × STEP_M` y punto, o sea que todas las terrazas medían lo
/// mismo. Sumado a que el campo de altura no puede cambiar más de
/// [`MAX_FIELD_STEP_PER_TILE`] por tile —menos de medio escalón—, eso tenía una
/// consecuencia que se comió el sistema entero: **ninguna pared del planeta
/// podía medir más de un escalón**. Medido: las 7.832 caras de la Tierra bajaban
/// exactamente uno.
///
/// De ahí salieron todos los parches que este módulo ya no necesita: la cascada
/// con tope de cuatro pisos que igual siempre salía de uno, el kit de tres
/// celdas fijas, y el bordillo decidido por una "cota trepable" regional —porque
/// si todas las caras miden lo mismo, la altura no puede distinguirlas—.
///
/// Con la escalera no uniforme el relieve dice lo que tiene que decir:
///
/// ```text
///   terraza  1  2  3  4  5  6  7  8  9 10 11 12
///   escalón  1  2  3  4  5  6  7  8  9 10 10 10   ← lo que mide SU pared
///   altura   1  3  6 10 15 21 28 36 45 55 65 75
/// ```
///
/// # POR QUÉ LA ESCALERA SUBE DE A UNO Y NO A SALTOS
///
/// La tabla decía `1 2 3 4 6 8 10 10 10 10 10 10` y eso dejaba el mundo con
/// **dos tamaños de pared, no con variedad**. Medido
/// (`medir_acantilados::cuanto_recortan_los_topes_de_la_pared`, Tierra 800×800):
///
/// ```text
///   alto  1    2    3   4   5   6   7   8   9   10
///        698 1473  346  38  42  40  17  26  30  515
///        └── 96 % del mundo ──┘  └ casi nada ┘   └ pico
/// ```
///
/// Bimodal: o bordillo, o farallón máximo, y nada en el medio. La causa es
/// aritmética — de la terraza 7 para arriba **todos los saltos valían 10**, así
/// que las 515 caras de la cordillera medían exactamente lo mismo. Y los valores
/// 5, 7 y 9 no existían en la tabla, así que ninguna pared del planeta podía
/// medirlos.
///
/// Con la escalera de a uno cada altura de 1 a 10 es alcanzable y el macizo
/// enseña caras de todos los tamaños según por dónde se lo cruce, que es lo que
/// se pidió: paredes y cascadas chicas, medianas y grandes, con diez de techo.
/// Los tres dieces del final se quedan porque son las terrazas 10-12, que casi no
/// existen (la Tierra pone el 82 % de su superficie en los niveles 0-3).
///
/// El total baja de 84 a **75 escalones ≈ 188 m**, un 11 % más bajo: la cumbre
/// sigue siendo cumbre y a cambio la ladera tiene relieve legible en vez de dos
/// alturas repetidas.
///
/// **Los números salen de medir el planeta, no de elegirlos lindos**
/// (`histograma_de_terrazas`): la Tierra reparte su superficie en 0 (10 %),
/// 1 (43 %), 2 (24 %), 3 (5 %) y una cola de 4 a 8 que es la cordillera. Poner
/// los farallones en las terrazas 10-12 —que fue el primer intento— dejaba el
/// mundo entero de bordillos, porque esas terrazas no existen.
///
/// - **La llanura sigue abierta pero deja de ser una alfombra.** El primer
///   escalón es un bordillo que se trepa de un impulso (60 % de las caras del
///   mundo); el segundo ya es una pared de dos pisos que hay que rodear o cruzar
///   por el paso.
/// - **La montaña se vuelve montaña.** Arriba, cruzar de terraza es un farallón
///   de ocho o diez pisos que hay que rodear o subir por el paso — y una cascada
///   que cae por ahí cae diez pisos, porque mide lo que mide el terreno.
/// - **Y se van achicando al bajar**, que es exactamente lo que se pidió: el
///   mismo macizo enseña caras cada vez más chicas a medida que desciende.
///
/// El total —la suma de la tabla, **75 escalones ≈ 188 m**— es la altura de la
/// terraza 12 sobre la costa.
pub const SALTO_DE_TERRAZA: [i8; (LEVELS_UP + 1) as usize] =
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10, 10];

/// **Tope de lo que una pared puede medir**, en escalones. Diez pisos, que es lo
/// que pidió el usuario y lo que el kit del pack puede apilar sin que el muro se
/// coma la terraza de abajo entera.
pub const MAX_PARED: i64 = 10;

/// Hasta dónde se busca hacia el norte al preguntar si una celda está debajo de
/// un muro ([`pie_de_pared`]). Una pared de [`MAX_PARED`] escalones tapa
/// `MAX_PARED + 1` filas —los cuerpos más el pie—, así que buscar sólo diez
/// dejaba la última fila de roca caminable.
const ALCANCE_PARED: i64 = MAX_PARED + 1;

/// La altura de una terraza en ESCALONES sobre el nivel del mar, acumulando
/// [`SALTO_DE_TERRAZA`]. Bajo el mar es 0: el fondo no tiene paredes.
pub fn altura_de_terraza(nivel: i8) -> i32 {
    if nivel <= 0 {
        return 0;
    }
    let n = nivel.min(LEVELS_UP) as usize;
    SALTO_DE_TERRAZA[1..=n].iter().map(|&s| s as i32).sum()
}

/// **CUÁNTO MIDE LA PARED QUE CUELGA DE ESTE CANTO**, en escalones. `0` = acá no
/// cuelga nada (no baja hacia el sur, o lo que hay al sur es líquido).
///
/// Es el número que gobierna TODO el sistema: cuántas filas de `cuerpo` apila el
/// muro, cuántos tramos mide la cascada que cae por él, cuántas celdas ocupa su
/// pie y cuántas hay que caminar para subir el paso. Un solo sitio, para que no
/// puedan volver a discrepar.
///
/// **La pared mira siempre al sur** porque la vista es 3/4: es la única cara que
/// se enseña entera. Las otras tres se ven de canto y se dibujan con una celda.
///
/// # Y NUNCA MIDE MÁS DE LO QUE LA TERRAZA DE ABAJO PUEDE PRESTARLE
///
/// Ver [`TERRAZA_VISIBLE`]. El alto que pide la escalera del mundo es el techo,
/// no la respuesta: si abajo no hay sitio, la cara se dibuja más baja y la
/// diferencia la sigue contando la terraza siguiente, que está a un paso.
pub fn alto_de_pared(body: CelestialBody, tx: i64, tz: i64) -> i64 {
    // Memorizada: hasta once celdas seguidas hacia el sur preguntan por ESTE
    // canto (ver [`PARED_CACHE`]), y el cliente recorre cada chunk varias veces.
    let b = body as u8;
    let epoca = crate::partida::epoca();
    let slot = slot_espacial(tx, tz);
    if let Some(v) = PARED_CACHE.with(|c| {
        let c = c.borrow();
        let e = &c[slot];
        (e.filled && e.body == b && e.tx == tx && e.tz == tz && e.epoca == epoca)
            .then_some(e.alto as i64)
    }) {
        return v;
    }
    let alto = alto_de_pared_sin_memo(body, tx, tz);
    if crate::grueso::activo() {
        return alto;
    }
    PARED_CACHE.with(|c| {
        c.borrow_mut()[slot] =
            ParedSlot { tx, tz, body: b, alto: alto as i16, filled: true, epoca };
    });
    alto
}

fn alto_de_pared_sin_memo(body: CelestialBody, tx: i64, tz: i64) -> i64 {
    let l = relief_level(body, tx, tz);
    if l <= 0 {
        return 0;
    }
    let s = relief_level(body, tx, tz + 1);
    if s < 0 || s >= l {
        return 0;
    }
    alto_entre(l, s)
        .min(meseta_detras(body, tx, tz, l))
        .min(hueco_al_sur(body, tx, tz, s))
}

/// **UNA PARED NO PUEDE SER MÁS ALTA QUE ANCHA ES LA MESETA QUE LA SOSTIENE.**
///
/// Éste es el que rompía la lectura del mundo, y no se veía en ninguna medida
/// porque las medias estaban bien. Se ve en el volcado de una ladera de la sierra
/// (`medir_acantilados::mapa_de_zona` sobre 1763,-5086):
///
/// ```text
///   ...MMMMM#.....#MMMMMMMM   8888889999988888899999999
///   ...#######################   88888888888888888888888
///   ...###############MMMMM###   88888888888888899999888
/// ```
///
/// Esos `MMMMM` son **manchas de nivel 9 de dos filas de fondo** —el campo de
/// altura rozando el borde del escalón, un montículo— y `SALTO_DE_TERRAZA` dice
/// que entre la terraza 8 y la 9 hay **diez escalones**. Así que cada pimpollo de
/// 5×2 celdas colgaba un farallón de once filas de roca, y como la ladera está
/// llena de ellos, los muros se solapaban hasta tapar la llanura entera: eso es la
/// pared gris que se come media pantalla en `relieve-0-pared.png`, y el marrón que
/// llena la isla de punta a punta en la captura del usuario.
///
/// El alto de `SALTO_DE_TERRAZA` no está mal — dice cuánto sube la MONTAÑA de la
/// terraza 8 a la 9, y eso es verdad de la montaña—. Lo que faltaba es que una
/// mancha de dos filas no es la montaña: es un montículo que casualmente cruzó el
/// umbral. Un macizo enseña su farallón cuando tiene macizo detrás.
///
/// Se mide hacia el NORTE porque la cara que se dibuja mira al sur: lo que hay
/// detrás del canto es el cuerpo de la meseta. Cuenta también lo que sigue
/// subiendo (`>= l`): una terraza más alta pegada atrás es más macizo, no menos.
fn meseta_detras(body: CelestialBody, tx: i64, tz: i64, l: i8) -> i64 {
    let mut filas = 0i64;
    while filas < MAX_PARED && relief_level(body, tx, tz - 1 - filas) >= l {
        filas += 1;
    }
    // La propia fila del canto ya es meseta, así que un montículo de una sola fila
    // enseña un bordillo y no una grieta.
    1 + filas
}

/// **CUÁNTO SUELO LE QUEDA A UNA TERRAZA DESPUÉS DE QUE LE CUELGUEN LA PARED.**
///
/// Es el número que faltaba y por el que el mundo se veía como se veía. Medido
/// sobre la Tierra con `medir_acantilados::mapa_de_zona`, una loma de 15×12 tiles salía **entera de
/// roca**: ni una celda de meseta, ni una de terraza intermedia — un pegote
/// marrón con forma de colina. En la captura de la isla del usuario es lo mismo a
/// escala de isla: pasto sólo en el borde que toca el mar y adentro piedra hasta
/// el otro extremo.
///
/// La cuenta explica por qué. Una ladera reparte sus escalones a la distancia que
/// le permite [`MAX_FIELD_STEP_PER_TILE`]: en la pendiente máxima una terraza mide
/// **dos tiles y medio**. Y la pared que le cuelga encima tapa `alto + 1` filas —
/// tres para un desnivel de dos escalones—. O sea que la roca de arriba se comía
/// la terraza de abajo ENTERA, y la de más abajo también, hasta que la ladera se
/// acababa. Cada anillo del cono existía en la geometría y no se veía nunca.
///
/// Así que la terraza se queda con lo suyo: **dos filas de suelo propio**, que es
/// lo mínimo para que se lea como una superficie y no como una junta entre dos
/// rocas. Con eso una ladera empinada sale como la dibujó el usuario —tortita
/// sobre tortita, cada una con su tapa de pasto y su bordillo— y los farallones de
/// diez pisos se quedan donde de verdad hay sitio para ellos: al pie de la
/// cordillera, contra el llano.
///
/// # Y DOS FILAS FIJAS NO ALCANZAN CONTRA UNA PARED DE DIEZ
///
/// Éste era el mínimo, no el reparto. Medido con
/// `medir_acantilados::cuanto_recortan_los_topes_de_la_pared` (Tierra, 800×800):
/// **3.225 cantos colgaban 14.626 filas de roca**, y los topes de acá recortaban
/// un mísero 8 %. De esos cantos, 568 seguían bajando los diez escalones enteros
/// —once filas de roca cada uno— así que sólo los farallones máximos ponían el
/// 43 % de toda la piedra del planeta.
///
/// En pantalla eso es una ladera que alterna **once filas de muro contra dos de
/// suelo**: 85 % roca. No se lee como una escalera de terrazas, se lee como una
/// losa gris con juntas — que es la queja del usuario ("el interior de la meseta
/// sale relleno de bordes"). Un piso no se lee por su línea de borde: se lee por
/// su RELLENO y la sombra al pie, y con dos filas no hay relleno que leer.
///
/// Por eso el reparto ya no es una resta fija sino [`REPARTO_DE_TERRAZA`]: la
/// pared nunca se queda con más de la mitad del hueco que tiene debajo. Donde hay
/// sitio de sobra —la cordillera contra el llano— el farallón sigue saliendo
/// entero; donde la terraza es angosta, la pared baja lo que la terraza le presta
/// y el resto lo cuenta el escalón siguiente, que está a un paso.
const TERRAZA_VISIBLE: i64 = 2;

/// **DE CADA CUÁNTO SE QUEDA LA PARED CON EL HUECO QUE TIENE DEBAJO.**
///
/// `2` = mitad y mitad: por cada fila de roca que cuelga, una fila de terraza que
/// se camina. Es la proporción mínima para que el escalón de abajo se lea como
/// una superficie propia y no como la junta entre dos muros.
const REPARTO_DE_TERRAZA: i64 = 2;

/// El alto máximo, en escalones, que admite el hueco al sur de este canto.
///
/// Se cuentan las filas seguidas que siguen siendo la terraza `abajo`; el dibujo
/// del muro ocupa `alto + 1` de ellas y el resto queda de suelo. Nunca devuelve
/// menos de uno: un bordillo no tapa nada (`pie_de_pared` sólo marca roca de dos
/// escalones para arriba), así que siempre cabe.
///
/// El reparto son las **dos** reglas juntas, y manda la más dura:
/// - [`TERRAZA_VISIBLE`] filas de suelo como mínimo absoluto, para que ni la
///   terraza más angosta quede sin una sola celda que pisar;
/// - y nunca más de `1/`[`REPARTO_DE_TERRAZA`] del hueco, para que una pared alta
///   sobre una terraza ancha tampoco se la coma entera.
///
/// El segundo es el que faltaba: con sólo el mínimo fijo, un farallón de diez
/// pisos se llevaba once filas de las trece que hubiera y dejaba dos — la losa
/// gris de las capturas.
///
/// **Sin `relief_cell` ni `pie_de_pared`**, que son quienes nos llaman: sólo
/// niveles, todos servidos por la caché de celdas.
fn hueco_al_sur(body: CelestialBody, tx: i64, tz: i64, abajo: i8) -> i64 {
    // El techo de la cuenta sube con el reparto: para saber si un muro de diez
    // cabe en su mitad hay que poder contar las veinte filas de abajo.
    let tope = MAX_PARED * REPARTO_DE_TERRAZA + TERRAZA_VISIBLE + 1;
    let mut filas = 0i64;
    while filas < tope && relief_level(body, tx, tz + 1 + filas) == abajo {
        filas += 1;
    }
    let por_minimo = filas - TERRAZA_VISIBLE - 1;
    let por_reparto = filas / REPARTO_DE_TERRAZA - 1;
    por_minimo.min(por_reparto).max(1)
}

/// El alto de la pared que separa dos terrazas, en escalones. Nunca menos de uno
/// —dos terrazas distintas siempre tienen un desnivel que enseñar— ni más de
/// [`MAX_PARED`].
fn alto_entre(arriba: i8, abajo: i8) -> i64 {
    (altura_de_terraza(arriba) - altura_de_terraza(abajo)).clamp(1, MAX_PARED as i32) as i64
}

/// **¿ESTÁ ESTA CELDA DEBAJO DEL DIBUJO DE UNA PARED?** Devuelve la fila del
/// canto del que cuelga y cuánto mide, o `None` si acá no hay roca encima.
///
/// El muro se dibuja hacia el sur: un canto de `alto` escalones ocupa su propia
/// celda (el labio) más `alto + 1` filas por debajo (los cuerpos y el pie). Esas
/// filas son ROCA, y por eso no se caminan.
///
/// Sólo cuentan las paredes de dos escalones para arriba. Un bordillo sigue
/// funcionando como siempre —su dibujo asoma sobre terreno caminable y se trepa
/// de un impulso—, que es lo que mantiene la llanura abierta.
///
/// # El coste
///
/// Se mira hacia el norte hasta encontrar un canto, y se corta en cuanto el
/// terreno BAJA en esa dirección (ahí no puede colgar nada). En la llanura el
/// bucle da dos o tres vueltas contra la caché de niveles; sólo en la falda de
/// una cordillera llega a [`MAX_PARED`].
pub fn pie_de_pared(body: CelestialBody, tx: i64, tz: i64) -> Option<(i64, i64)> {
    let l = relief_level(body, tx, tz);
    if l < 1 {
        return None;
    }
    // El nivel de la terraza de la que colgaría el muro: la nuestra, porque entre
    // nosotros y su canto no puede haber ningún cambio de escalón (si lo hubiera,
    // ese sería el canto y cortaríamos ahí).
    let anterior = l;
    for k in 1..=ALCANCE_PARED {
        let n = relief_level(body, tx, tz - k);
        if n < anterior {
            // Hacia el norte se baja: esta celda está del lado alto, no debajo de
            // ninguna cara.
            return None;
        }
        if n > anterior {
            // **EL PRIMER CANTO ES EL ÚNICO QUE PUEDE TAPARNOS.** Si su muro no
            // llega hasta acá, el de la terraza de más arriba tampoco: se corta
            // contra éste (es la misma poda que aplica el dibujo, ver
            // `world2d::pared_apilada`). Sin cortar acá, una celda podía quedar
            // marcada como roca por un farallón lejano cuyo dibujo nunca llega —
            // o sea una pared invisible.
            // **EL MISMO ALTO QUE DIBUJA EL CANTO**, no el que pide la escalera.
            // Acá decía `alto_entre(n, anterior)` —el alto bruto— y por eso la
            // roca invisible: `alto_de_pared` recorta la cara a lo que la terraza
            // de abajo puede prestarle ([`hueco_al_sur`]), así que un muro
            // recortado dejaba marcadas como pared filas que su dibujo ya no
            // tapaba. Una sola fuente para las dos preguntas.
            let alto = alto_de_pared(body, tx, tz - k);
            // `alto + 1` filas: los cuerpos y el pie. La celda del canto es suya
            // y la resuelve la rama de arriba de `relief_cell`.
            return (alto >= 2 && k <= alto + 1 && es_canto(body, tx, tz - k, n))
                .then_some((tz - k, alto));
        }
    }
    None
}

/// **DÓNDE TERMINA EL DIBUJO DEL MURO QUE CUELGA DE ESTE CANTO.**
///
/// Devuelve la fila del PIE —la última que la roca tapa— o `None` si de este canto
/// no cuelga nada. La primera fila de terraza que se VE es la siguiente.
///
/// Es la cuenta que hacen [`pie_de_pared`] y `world2d::pared_apilada`, con nombre
/// propio para que el cliente no la vuelva a escribir: la usa la sombra de terraza
/// (`world2d::sombra_de_muro_en_la_columna`), que antes preguntaba por el vecino
/// del norte y por eso pintaba su franja **dentro de la roca**, donde no la ve
/// nadie. En pantalla eso eran dos praderas del mismo verde separadas por una
/// banda marrón, sin ninguna pista de cuál está más alta.
pub fn pie_del_muro(body: CelestialBody, tx: i64, tz: i64) -> Option<i64> {
    let alto = alto_de_pared(body, tx, tz);
    (alto >= 1).then(|| tz + alto + 1)
}

/// Semiancho (en unidades de ruido) de las franjas de paso. Más grande = pasos
/// más anchos y más frecuentes.
const PASS_HALF: f32 = 0.015;

/// **Períodos de las familias de franjas de paso, en tiles.**
///
/// Dos familias largas y **una corta**. Las largas (60 y 41) son las de siempre:
/// serpentean por todo el mundo y cruzan cualquier borde de terraza de tamaño
/// normal. La corta es el arreglo de la meseta sellada.
///
/// # Por qué hizo falta la tercera
///
/// `every_terrace_edge_has_a_pass` sólo miraba el RATIO global de rampas contra
/// caras, y ese ratio estaba bien mientras había mesetas sin una sola forma de
/// subir. Medido de verdad con `medir_cascadas::mesetas_selladas` (Tierra,
/// 800×800 tiles, componentes conexas de terreno alto y un BFS de caminabilidad
/// encima): **12 de 48 mesetas eran islas** — ni rampa ni bordillo en todo su
/// perímetro, y tampoco se llegaba a ellas desde la terraza de arriba. De 3 a
/// 238 celdas, todas de nivel 3.
///
/// La razón es de geometría y no de suerte: una franja de período 60 cruza sí o
/// sí una meseta de 200 tiles de lado, y se le escapa una de 15. El perímetro
/// chico es el que se queda sin paso, así que lo que faltaba era una familia con
/// el período del tamaño de esas mesetas.
///
/// Ensanchar `PASS_HALF` habría hecho las dos cosas a la vez —más pasos Y más
/// anchos, o sea menos pared en todo el mundo— y es justamente lo que se
/// descartó al medir `PASS_DILATE`.
const PASS_PERIODOS: [i64; 3] = [60, 41, 23];

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
    // **Y EL SENDERO SUBE AL MONTE**, por el mismo motivo y con el mismo
    // mecanismo. La garantía que ya había —toda meseta tiene una rampa— es por
    // meseta, no por macizo: un cono de doce terrazas son doce anillos, y que
    // cada uno tenga su paso en algún punto de su perímetro no dice que los doce
    // estén enhebrados. `world_map::sendero_de_cumbre` los enhebra.
    //
    // Va después de la calzada porque una ciudad no se funda en la cumbre y las
    // dos preguntas casi nunca coinciden; y antes del ruido porque es más barata
    // (una vuelta por cordillera con un descarte por distancia).
    if crate::world_map::sendero_de_cumbre(body, tx, tz) {
        return true;
    }
    pass_noise(body, tx, tz)
}

/// La parte de RUIDO de [`in_pass_band`], sin la carretera. Se separa para poder
/// ENSANCHAR el paso sin volver a preguntar por la calzada en cada vecino: la
/// calzada ya se abre su propio hueco del ancho que mide, y `is_drivable_at` es
/// lo caro de las dos preguntas.
fn pass_noise(body: CelestialBody, tx: i64, tz: i64) -> bool {
    let seed = body_terrain_seed(body);
    // Salida temprana por familia: en el llano casi nunca se llega a la tercera.
    PASS_PERIODOS.iter().enumerate().any(|(i, &periodo)| {
        let s = seed ^ (0x9a55_0001 + i as u32);
        (fbm(s, tx, tz, periodo) - 0.5).abs() < PASS_HALF
    })
}

/// **UN PASO SE CRUZA, NO SE ENHEBRA.** Cuántas celdas se ensancha la franja de
/// paso a cada lado.
///
/// Medido antes de tocarlo (test `medir_acantilados`, Tierra, 800×800 tiles):
/// de 520 pasos, **465 medían UNA celda** de ancho y sólo 55 medían dos. Un
/// hueco de un tile en una pared es una aguja: se ve como un error de dibujo
/// antes que como un camino, y el kit de escalinata del pack —dos columnas de
/// escalones entre dos espolones de roca— directamente no entra.
///
/// Ensanchar la franja de ruido (`PASS_HALF`) habría hecho las dos cosas a la
/// vez: pasos más anchos Y más pasos, o sea menos pared. Dilatar sólo el ancho
/// deja la MISMA cantidad de pasos, en los mismos sitios, con tres celdas de
/// luz. Es la diferencia entre "hay más caminos" y "los caminos que hay se
/// cruzan".
///
/// # No alcanzó, y ahora está medido de nuevo (21 ago 2026)
///
/// La dilatación en cruz abre la celda de al lado, pero **no ensancha la corrida
/// A LO LARGO DEL CANTO**, que es lo que se ve: medido con
/// `medir_acantilados::ancho_de_los_pasos` sobre cuatro ventanas, **el 88,9 % de
/// los pasos seguía midiendo UNA celda** y sólo el 3,4 % llegaba a cuatro. Y
/// cuatro es lo que mide el kit del pack —espolón, dos columnas de peldaños,
/// espolón—, así que a un paso de una celda le tocaba **una columna suelta de un
/// dibujo de cuatro**: es el *"no sé por qué recortás sólo un pedazo y usás eso
/// de rampa"* del usuario.
///
/// Se probó ensanchar a lo largo del canto y **no sirvió** — ver [`PASO_ANCHO`],
/// donde está la medición que explica por qué: el borde de terraza es una
/// escalera, no una línea.
const PASS_DILATE: i64 = 1;

/// **CUÁNTO MIDE UN PASO A LO LARGO DEL CANTO**, en celdas.
///
/// Cuatro, y no es un número elegido: es **lo que mide el kit del paso** del pack
/// (`Four_Sand_Mountains_*`, ver `scripts/import-acantilado.py`) — espolón de
/// roca, dos columnas de peldaños y el otro espolón. El usuario lo decidió
/// mirando la lámina de ejemplo: *"el paso mide lo que el kit"*.
///
/// # Y NO ALCANZA CON ENSANCHAR EL PASO
///
/// Se probó ensanchar la franja a lo largo del canto y **no movió la aguja**: de
/// 88,9 % de pasos de una celda a 89,4 %, con un 22 % más de celdas de paso a
/// cambio de nada. La medición que lo explica es
/// `medir_acantilados::corridas_rectas_de_canto`: **el 86,2 % de TODOS los
/// cantos del mundo —sean paso o no— mide una sola celda en línea recta**. Los
/// bordes de terraza no son líneas, son escaleras diagonales, y un kit de cuatro
/// celdas no se apoya en el 95,6 % de ellos.
///
/// O sea que para colocar el kit entero primero hay que **enderezar el canto**:
/// que el paso talle una muesca recta de cuatro celdas al mismo nivel, que es
/// exactamente lo que el kit dibuja (`Four_Sand_Mountains_1_Top` ES una muesca).
/// Eso es trabajo de generación, no de dibujo, y está anotado en §13.
pub const PASO_ANCHO: i64 = 4;

/// **¿ES ESTA CELDA EL CANTO DE UNA TERRAZA?** — o sea, la fila de arriba de una
/// pared. `l` es su nivel, que quien pregunta ya tiene.
///
/// Sólo hay pared entre terrazas de TIERRA: la costa no es un muro (se entra al
/// agua caminando y se sigue nadando) y el fondo del mar no tiene escalones.
fn es_canto(body: CelestialBody, tx: i64, tz: i64, l: i8) -> bool {
    if l <= 0 {
        return false;
    }
    // **NINGUNA MONTAÑA DENTRO DEL AGUA.**
    //
    // El borde de terraza que da al mar no es una pared: es la costa. Sin esto,
    // cualquier loma que llegara a la orilla plantaba un farallón de roca con el
    // agua lamiéndole el pie, y un islote de tres tiles salía como un peñón —
    // que en cenital se lee como que a alguien se le cayó una montaña al mar.
    //
    // Se mira el NIVEL de los vecinos y no el tile, que es lo que evita la
    // recursión: `geography` nos llama a nosotros para decidir el tile, así que
    // preguntarle de vuelta sería morderse la cola. Un nivel negativo es líquido
    // por definición (`water_tile`), que es todo lo que hace falta saber.
    let mut lower = false;
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let n = relief_level(body, tx + dx, tz + dz);
        if n < 0 {
            return false;
        }
        if n < l {
            lower = true;
        }
    }
    if !lower {
        return false;
    }
    // **Y LA PARED SE RETIRA DEL MAR TANTO COMO MIDA SU DIBUJO.**
    //
    // La cara que mira al sur cuelga `alto + 1` filas sobre la terraza de abajo,
    // así que con el agua más cerca que eso el muro se dibujaba encima del
    // líquido — el "acantilado flotando sobre el agua" de las capturas. Antes el
    // retiro era una constante de tres filas, que era la medida del único alto
    // que existía; ahora un farallón de diez pisos se retira doce y un bordillo
    // dos, porque cada uno se retira lo que tapa.
    let s = relief_level(body, tx, tz + 1);
    if s < l {
        for k in 1..=alto_entre(l, s) + 2 {
            if relief_level(body, tx, tz + k) < 0 {
                return false;
            }
        }
    }
    // **Y NINGUNA PARED SOBRE UN RÍO.** Es la hermana de "ninguna montaña dentro
    // del agua", y faltaba: los dos retiros de arriba preguntan por `nivel < 0`,
    // o sea mar y lagos, y un río corre en nivel ≥ 0. Medido antes de tocarlo
    // (`medir_rios::el_cauce_no_lo_corta_un_acantilado`, ventana de sierra):
    // **el 21,75 % de las celdas de cauce eran roca** — el río cortado en seco
    // por farallones en zig-zag de la captura del 16 ago 2026. Y no queda ahí:
    // `relief_tile` gana sobre el bioma (`geography::natural_tile`), así que esa
    // roca además SECA hasta tres filas más de cauce
    // (`bajo_el_dibujo_de_una_pared`) — las orlas marrones alrededor del agua.
    //
    // **La excepción es la GARGANTA**: donde el cauce CRUZA el escalón —río
    // arriba y río abajo de esta celda— el agua tiene que pasar, el salto es
    // correcto y el recorte del pack ya trae su propia roca a los dos lados.
    // Sacar también ésas dejaría el mundo sin cascadas.
    //
    // Se pregunta con `hay_rio`, que mira el CAUCE y no el tile pintado: sólo
    // consulta el nivel y el trazado (`rios::cauce_en`), nunca `relief_cell`, así
    // que no hay recursión. Es la misma llamada que ya hace `garganta_exenta`.
    //
    // Y "cruzar" son TRES cosas, no dos: agua arriba, agua abajo **y que el
    // terreno baje ahí**. Con sólo las dos primeras, un río que corre de norte a
    // sur por un llano cumple siempre —cada celda del cauce tiene agua a los dos
    // lados— y la excepción se tragaba el retiro entero: medido, el cauce seguía
    // cortado en el 7,4 % de la sierra. Un salto sin desnivel no es un salto.
    let cruza = s < l
        && crate::geography::hay_rio(body, tx, tz - 1)
        && crate::geography::hay_rio(body, tx, tz + 1);
    if !cruza {
        let filas = if s < l { alto_entre(l, s) + 1 } else { 0 };
        for k in 0..=filas {
            if crate::geography::hay_rio(body, tx, tz + k) {
                return false;
            }
        }
    }
    true
}

/// **¿ES ESTA CELDA EL CANTO DE UNA TERRAZA?** La versión pública de [`es_canto`],
/// para quien tiene la celda y no su nivel.
///
/// Hace falta porque [`ReliefCell::Ramp`] cubre dos cosas distintas: el canto por
/// el que se entra al paso y las celdas del corredor que lo baja. Contar las dos
/// como "borde de terraza" hincha cualquier medida por el alto del muro.
pub fn canto_de_terraza(body: CelestialBody, tx: i64, tz: i64) -> bool {
    es_canto(body, tx, tz, relief_level(body, tx, tz))
}

/// **NUNCA HAY PASO EN UNA CARA QUE MIRA AL SUR.** Regla del usuario, textual:
/// *"en el sur no se ocupa rampa, prohibido para los acantilados usar rampas en
/// caras del sur — o sea el down en rampas; sólo usar izquierda, derecha, top"*.
///
/// El motivo es de dibujo y se ve en cuanto se mira una captura: la cara sur es
/// la única que se enseña ENTERA (cuelga sobre la terraza de abajo), así que un
/// paso ahí es un tajo recto que parte el muro de arriba abajo — y cuanto más
/// alta la pared, más largo el tajo. Las otras tres caras se ven de canto: ahí el
/// paso es una muesca en el filo y se lee como lo que es.
///
/// Que la subida esté en otro lado del macizo no cierra nada: el borde de una
/// terraza es una curva cerrada, así que siempre tiene caras al este, al oeste o
/// al norte. Lo que cambia es que hay que ir a buscarla — que es justamente lo
/// que hace del mapa un sitio que se recorre.
fn cara_al_sur(body: CelestialBody, tx: i64, tz: i64, l: i8) -> bool {
    relief_level(body, tx, tz + 1) < l
}

/// **¿POR ACÁ SE SUBE?** El paso: la franja de ruido, su ensanche y el paso de
/// último recurso de la meseta chica. `l` es el nivel de la celda.
fn es_paso(body: CelestialBody, tx: i64, tz: i64, l: i8) -> bool {
    // Prohibido al sur (ver [`cara_al_sur`]). Va primero y es barato: una consulta
    // de nivel contra la caché, y se ahorra el ruido de las tres familias.
    if cara_al_sur(body, tx, tz, l) {
        return false;
    }
    if in_pass_band(body, tx, tz) {
        return true;
    }
    // El ensanche ([`PASS_DILATE`]): si la franja pasa a un tile de acá, esta
    // celda es la orilla del paso y también se abre. Sólo se pregunta acá
    // —después de saber que la celda sería PARED— así que el mundo plano no
    // paga nada.
    for d in 1..=PASS_DILATE {
        for (dx, dz) in [(d, 0), (-d, 0), (0, d), (0, -d)] {
            if pass_noise(body, tx + dx, tz + dz) {
                return true;
            }
        }
    }
    // **Y TODA MESETA SE CIERRA CON UNA RAMPA**, incluida la que se le escapó a
    // las tres familias de franjas. Va la última porque es la más cara de todas
    // las preguntas y sólo la pagan las celdas que ya iban a ser pared.
    paso_de_meseta_chica(body, tx, tz, l)
}

/// **CUÁNTO SE RETIRA DEL BORDE LO QUE SE SIEMBRA.** Dos celdas, y es lo que
/// pidió el usuario textual: *"lo coherente es que se detecte el borde dos
/// cuadros antes, o sea hacia el centro de la meseta"*.
///
/// El número no es de gusto, es el tamaño de lo que se planta. Un árbol del pack
/// mide más de una celda y se dibuja hacia ARRIBA desde su pie, así que un roble
/// sembrado en la celda pegada al canto queda con medio tronco colgando sobre la
/// pared que cae — flotando, que es como se veía. Con dos celdas de retiro el
/// prop más grande de las tablas entra entero dentro de la meseta.
pub const RETIRO_DEL_CANTO: i64 = 2;

/// **¿ESTA CELDA ESTÁ DEMASIADO AL BORDE PARA PLANTARLE ALGO?**
///
/// Verdadero si el terreno CAE dentro de [`RETIRO_DEL_CANTO`] celdas: o sea, si
/// hay una vecina de nivel menor a distancia de Chebyshev ≤ 2. Es la franja
/// interior de la meseta, la que se lee como "el filo" aunque la celda en sí sea
/// terreno llano y edificable.
///
/// No reemplaza a [`canto_de_terraza`]: el canto en sí ya es `Tile::Cliff` y
/// ninguna tabla siembra sobre él. Lo que faltaba era el retiro HACIA ADENTRO.
///
/// **El llano no paga nada.** La pregunta sale por nivel antes de mirar una sola
/// vecina, así que las celdas de nivel 0 —la enorme mayoría del mundo— cuestan
/// una consulta a la caché y se van. Sólo las 24 vecinas de una celda ya
/// elevada se recorren, y con salida temprana en cuanto una cae.
pub fn retiro_del_canto(body: CelestialBody, tx: i64, tz: i64) -> bool {
    let l = relief_level(body, tx, tz);
    // En el llano y bajo el agua no hay meseta de la que caerse.
    if l <= 0 {
        return false;
    }
    let r = RETIRO_DEL_CANTO;
    for dz in -r..=r {
        for dx in -r..=r {
            if dx == 0 && dz == 0 {
                continue;
            }
            if relief_level(body, tx + dx, tz + dz) < l {
                return true;
            }
        }
    }
    false
}

/// Clasifica una celda: terraza, canto, cuerpo de pared o paso.
///
/// El orden importa y es el de la geometría: **primero mando yo, después el que
/// tengo encima**. Una celda que es canto de su propia terraza dibuja su muro
/// aunque le caiga roca de otro más alto; sólo si no es canto se pregunta si
/// está debajo del dibujo de una pared ajena.
pub fn relief_cell(body: CelestialBody, tx: i64, tz: i64) -> ReliefCell {
    // Descarte antes de la memoria: en el llano y bajo el agua la respuesta sale
    // de la caché de niveles y no vale la pena ocupar una ranura con ella.
    let (h, l) = cached_cell(body, tx, tz);
    if l <= 0 {
        return ReliefCell::Flat;
    }
    let b = body as u8;
    let epoca = crate::partida::epoca();
    let slot = slot_espacial(tx, tz);
    if let Some(v) = CELL_CACHE.with(|c| {
        let c = c.borrow();
        let e = &c[slot];
        (e.filled && e.body == b && e.tx == tx && e.tz == tz && e.epoca == epoca).then_some(e.cell)
    }) {
        return v;
    }
    let cell = relief_cell_sin_memo(body, tx, tz, h, l);
    if crate::grueso::activo() {
        return cell;
    }
    CELL_CACHE.with(|c| {
        c.borrow_mut()[slot] = CellSlot { tx, tz, body: b, cell, filled: true, epoca };
    });
    cell
}

fn relief_cell_sin_memo(body: CelestialBody, tx: i64, tz: i64, h: f32, l: i8) -> ReliefCell {
    // Descarte barato: en el medio de una terraza no puede haber ningún canto.
    // Ojo — descarta el CANTO, no el pie: una celda bien plantada en su terraza
    // puede estar igual debajo del farallón de la terraza de arriba.
    if near_step_boundary(body, h) && es_canto(body, tx, tz, l) {
        return if es_paso(body, tx, tz, l) {
            ReliefCell::Ramp
        } else {
            ReliefCell::Cliff
        };
    }
    // **Y SI NO SOS EL CANTO, ¿TENÉS ROCA ENCIMA?** Las filas que el muro tapa
    // son el muro: no se caminan. Y donde el muro es un paso, sus filas son la
    // subida — once celdas de rampa para un farallón de diez pisos, que es lo
    // que convierte "subir" en un recorrido y no en un tile mágico.
    if let Some((cz, _)) = pie_de_pared(body, tx, tz) {
        let nivel_canto = relief_level(body, tx, cz);
        return if es_paso(body, tx, cz, nivel_canto) {
            ReliefCell::Ramp
        } else {
            ReliefCell::Pared
        };
    }
    ReliefCell::Flat
}

/// **Cuántas celdas puede tener una meseta para que se le abra un paso a mano.**
///
/// Medido con `medir_cascadas::mesetas_selladas` (Tierra, 800×800): con las tres
/// familias de franjas quedan 5 mesetas aisladas y la mayor tiene **47 celdas**.
/// 96 las cubre con margen y deja fuera a las mesetas de verdad — la mediana del
/// mundo es de 199 celdas y la mayor de 8.073.
///
/// El número también es el techo del coste: el relleno corta apenas se pasa, así
/// que una cara de montaña paga 97 consultas de nivel (todas de la caché de
/// celdas) y ni una más.
const MESETA_CHICA: usize = 96;

/// Hasta dónde se aleja el relleno de la celda que pregunta. Con [`MESETA_CHICA`]
/// celdas no se puede llegar más lejos ni en una meseta con forma de serpiente.
const MESETA_RADIO: i64 = 48;

/// **EL PASO DE ÚLTIMO RECURSO: una meseta chica se abre igual.**
///
/// Las franjas de paso ([`in_pass_band`]) son campos globales, y por eso a una
/// meseta grande la cruzan seguro y a una chica se le escapan: es cuestión de
/// perímetro, no de suerte. Medido, eso dejaba **12 de 48 mesetas convertidas en
/// islas** — sin rampa propia y sin llegada desde la terraza de arriba.
///
/// La salida fácil era meter más familias de franjas, y se midió: con una cuarta
/// de período 13 no queda ninguna meseta sellada, pero el borde de terraza pasa
/// de 28 % a **54 % de rampa** — más paso que pared en todo el planeta, o sea
/// pagar en la cordillera un problema que sólo tienen las lomas.
///
/// Así que la loma se arregla en la loma. Se rellena la meseta a la que pertenece
/// esta celda (cortando en cuanto pasa de [`MESETA_CHICA`], que es lo que la hace
/// barata) y, si de verdad es chica, se le abre **UN** paso: el trozo de su borde
/// alrededor de la celda que le toca por sorteo. Un paso, no cuatro — el mínimo
/// que pidió la regla, y en un sitio que depende sólo de la meseta, así que todas
/// sus celdas contestan lo mismo.
///
/// Es determinista y sin estado: el ancla es la celda menor de la meseta, que no
/// depende de por dónde se empezó a rellenar.
///
/// # El "visto" es un mapa de bits, no un `Vec::contains`
///
/// Era `celdas.contains(&(nx, nz))` dentro del bucle de vecinos: con 96 celdas y
/// cuatro vecinos cada una, **hasta 18.000 comparaciones por celda del mundo** —
/// y esto se pregunta por cada celda que iba a ser pared, o sea toda la ladera de
/// una montaña. Medido, era la mitad de los 8-10 µs que costaba `relief_cell`.
/// La ventana está acotada por [`MESETA_RADIO`], así que "ya lo vi" cabe en un
/// mapa de bits de 97×97 en la pila y la pregunta pasa a ser un desplazamiento.
fn paso_de_meseta_chica(body: CelestialBody, tx: i64, tz: i64, l: i8) -> bool {
    const LADO: i64 = MESETA_RADIO * 2 + 1;
    const PALABRAS: usize = ((LADO * LADO) as usize).div_ceil(64);
    let mut visto = [0u64; PALABRAS];
    let mut marcar = |visto: &mut [u64; PALABRAS], x: i64, z: i64| -> bool {
        let i = ((z - tz + MESETA_RADIO) * LADO + (x - tx + MESETA_RADIO)) as usize;
        let (w, b) = (i / 64, 1u64 << (i % 64));
        let ya = visto[w] & b != 0;
        visto[w] |= b;
        ya
    };
    // Relleno acotado de la meseta: celdas del MISMO nivel, 4-conexas.
    let mut celdas: Vec<(i64, i64)> = Vec::with_capacity(MESETA_CHICA + 1);
    let mut pila = vec![(tx, tz)];
    celdas.push((tx, tz));
    marcar(&mut visto, tx, tz);
    while let Some((x, z)) = pila.pop() {
        for (dx, dz) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let (nx, nz) = (x + dx, z + dz);
            if (nx - tx).abs() > MESETA_RADIO || (nz - tz).abs() > MESETA_RADIO {
                // Tan lejos que ya no es una loma: no hace falta terminar.
                return false;
            }
            if relief_level(body, nx, nz) != l || marcar(&mut visto, nx, nz) {
                continue;
            }
            if celdas.len() >= MESETA_CHICA {
                return false;
            }
            celdas.push((nx, nz));
            pila.push((nx, nz));
        }
    }
    // El BORDE de la meseta: por donde se podría bajar. Ordenado para que el
    // sorteo no dependa del orden en que salió del relleno.
    // **Y el borde donde se puede abrir es el que NO mira al sur** (ver
    // [`cara_al_sur`]): de nada sirve elegir por sorteo una celda a la que la
    // regla le va a negar el paso después.
    let mut borde: Vec<(i64, i64)> = celdas
        .iter()
        .copied()
        .filter(|&(x, z)| {
            !cara_al_sur(body, x, z, l)
                && [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)]
                    .into_iter()
                    .any(|(dx, dz)| relief_level(body, x + dx, z + dz) < l)
        })
        .collect();
    if borde.is_empty() {
        return false;
    }
    borde.sort_unstable_by_key(|&(x, z)| (z, x));
    // El ancla: la celda menor de la meseta. No depende de dónde empezamos.
    let ancla = *borde.first().expect("el borde no está vacío");
    let elegida = borde[hash2(0x9a55_00FF, ancla.0, ancla.1) as usize % borde.len()];
    // El paso mide lo mismo que los demás ([`PASS_DILATE`] a cada lado), o sería
    // la aguja de un tile que se descartó al medir.
    (elegida.0 - tx).abs() + (elegida.1 - tz).abs() <= PASS_DILATE
}

/// **BORDILLOS: los desniveles que se trepan y se bajan a pie, sin rampa.**
///
/// Ahora es lo que siempre tendría que haber sido: **un bordillo es una pared de
/// UN escalón**. Se trepa de un impulso, en cualquier dirección y también por las
/// esquinas.
///
/// La regla vieja no podía ser ésta y por eso era otra. Mientras todas las caras
/// del planeta medían un escalón —porque la altura de una terraza era uniforme—,
/// "un escalón se trepa" abría el mundo entero y el relieve dejaba de frenar a
/// nadie; hubo que inventarle al muro una *cota trepable regional* (un campo de
/// ruido de período 700) para repartir bordillos y paredes sin mirar la altura,
/// que es justamente lo que no se puede explicar mirando el mapa. Con la escalera
/// de [`SALTO_DE_TERRAZA`], la altura vuelve a ser el dato: la llanura sube de a
/// un escalón y se recorre, la montaña sube de a ocho y hay que buscar el paso.
///
/// **No puede encerrar a nadie**: la regla es simétrica —mira la celda, no la
/// dirección desde la que llegás— y sólo AGREGA paso, nunca lo quita. El bordillo
/// que trepaste es el mismo que te deja volver.
pub fn climbable_edge(body: CelestialBody, tx: i64, tz: i64) -> bool {
    relief_cell(body, tx, tz) == ReliefCell::Cliff && alto_de_pared(body, tx, tz) <= 1
}

/// ¿La celda corta el paso a pie por RELIEVE? Es la pregunta que hace el juego al
/// mover al jugador: un bordillo ([`climbable_edge`]) no corta; el cuerpo de una
/// pared sí, siempre — estás dentro de la roca.
pub fn relief_blocks_walk(body: CelestialBody, tx: i64, tz: i64) -> bool {
    match relief_cell(body, tx, tz) {
        ReliefCell::Pared => true,
        ReliefCell::Cliff => alto_de_pared(body, tx, tz) > 1,
        _ => false,
    }
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
        // El canto y el cuerpo de la pared son la MISMA roca y por eso el mismo
        // tile: se pican igual, bloquean igual y valen lo mismo. Lo que los
        // distingue es quién dibuja (el canto cuelga el muro entero) y eso lo
        // resuelve el cliente preguntando por [`pie_de_pared`].
        ReliefCell::Cliff | ReliefCell::Pared => Some(Tile::Cliff),
        ReliefCell::Ramp => Some(Tile::Slope),
        ReliefCell::Flat => None,
    }
}

/// **¿Cae esta celda debajo del DIBUJO de una pared, o en la fila de llano que
/// va justo después?** Si sí, acá no puede haber líquido.
///
/// Son dos cosas y las dos las pidió el usuario, textualmente: *"siempre tiene
/// que haber un borde donde termina la pared, un borde más de bloques de llano,
/// que es el borde que da de llano con agua"*.
///
/// 1. **Lo que el dibujo TAPA.** Las filas de una pared de verdad ya son
///    [`ReliefCell::Pared`] —tile de roca— y por ahí no entra ni el río ni el
///    charco. El bordillo es a propósito la excepción: su dibujo asoma sobre
///    terreno que se sigue caminando (es lo que mantiene la llanura abierta), y
///    ese terreno puede ser el cauce de un río.
/// 2. **La fila de llano que se ve.** La de justo debajo del pie. Sin ella el
///    agua queda pegada a la roca y no hay dónde dibujar la orilla: el muro
///    parece nacer dentro del río.
///
/// # La fila que se había perdido
///
/// Esto lo hacía antes una constante, `SUELO_AL_PIE = 3`, que decía "dos filas
/// que tapa la roca **más una de llano**". Al reescribirlo en función del alto
/// real de cada pared se conservaron las dos primeras y **se perdió la tercera**.
/// No lo cazó ningún test porque el guardián que hay
/// (`agua::ninguna_pared_se_dibuja_sobre_el_agua`) pregunta por lo que el dibujo
/// tapa, no por lo que se ve debajo. Lo cazó la medición:
/// `medir_cascadas::pie_de_pared_contra_agua` daba **110 caras de 3.666 con el
/// agua pegada al pie**, contra las 0 que anotaba la biblia.
///
/// **No se pregunta por el mar**: un nivel bajo cero no se seca —sería tierra
/// flotando— y para eso está el retiro de [`es_canto`], que directamente no
/// planta pared con el agua cerca.
pub fn bajo_el_dibujo_de_una_pared(body: CelestialBody, tx: i64, tz: i64) -> bool {
    if relief_level(body, tx, tz) < 0 {
        return false;
    }
    // **La fila de llano del muro alto**, barata y primero: si justo al norte hay
    // cuerpo de pared, ésta es la fila que queda a la vista bajo el pie. Una sola
    // consulta cacheada, y resuelve de un saque los farallones de hasta diez
    // pisos — sus filas de roca ya las tapa `relief_tile`.
    if relief_cell(body, tx, tz - 1) == ReliefCell::Pared && !garganta_exenta(body, tx, tz - 1) {
        return true;
    }
    // Tres filas para el bordillo: las dos que tapa su dibujo (cuerpo y pie)
    // colgando de su canto, más la de llano que se ve debajo.
    for k in 1..=3 {
        let cz = tz - k;
        if relief_cell(body, tx, cz) != ReliefCell::Cliff || alto_de_pared(body, tx, cz) != 1 {
            continue;
        }
        if garganta_exenta(body, tx, cz) {
            continue;
        }
        return true;
    }
    false
}

/// **La garganta de una cascada está exenta de secarse.** Ahí el cauce CRUZA el
/// escalón y el recorte del pack ya trae su propia roca a los dos lados, así que
/// el agua tiene que pasar; secarla dejaría el río cortado en seco contra la
/// pared.
///
/// Dos preguntas y en este orden, que es lo que la hace barata: primero el cauce
/// cruzando (dos evaluaciones de ruido), y sólo si cruza se le pregunta a `agua`
/// si de verdad hay un salto anclado —que barre el cauce a lo ancho y es cara—.
/// Un muro que corre PARALELO al río tiene río abajo pero no arriba y sale por
/// la primera.
fn garganta_exenta(body: CelestialBody, tx: i64, cz: i64) -> bool {
    crate::geography::hay_rio(body, tx, cz - 1)
        && crate::geography::hay_rio(body, tx, cz + 1)
        && crate::agua::en_la_garganta(body, tx, cz)
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
// # Y EL ÍNDICE ES LA POSICIÓN, NO UN HASH — 1.024 RANURAS NO ERAN NINGUNA
//
// Ésta era la línea: `hash2(...) % 1024`. El comentario de arriba dice "el
// esténcil de 5 puntos necesita ~3 filas vivas", y para `relief_level` suelto es
// verdad. Pero **`relief_cell` no es un esténcil de 5 puntos**: por dentro llama
// a `pie_de_pared` (11 filas al norte), y cada fila de ésas a `alto_de_pared`
// (hasta 10 al norte por `meseta_detras` + 23 al sur por `hueco_al_sur`). Son
// del orden de **300 consultas de nivel por celda**, repartidas por una ventana
// de 32×80 tiles — 2.560 celdas distintas contra 1.024 ranuras, y encima
// mapeadas por hash, o sea chocándose entre ellas.
//
// Medido (`coste_del_relieve::reparto_del_coste`, release): `relief_level`
// cacheado costaba 0,6 µs y `relief_cell` **8 a 10 µs**, o sea quince campos de
// altura recalculados por celda. En un chunk de 32×32 eso son **11 a 26 ms**,
// más que un fotograma entero de los 16 ms del Redmi — por eso el mundo llegaba
// a medio armar y la captura de la cumbre salía vacía: el chunk nunca terminaba.
//
// El arreglo no es "más ranuras": es **dejar de hashear**. El índice son los bits
// bajos de la posición, así que cualquier ventana de [`CACHE_LADO`]×[`CACHE_LADO`]
// entra ENTERA sin un solo choque, se recorra en el orden que se recorra. 128 de
// lado cubre de sobra la ventana que toca `relief_cell` (32×80) y un chunk con su
// margen; y sigue siendo una tabla directa, así que un acceso disperso sigue
// costando exactamente una evaluación.
//
// El coste en memoria es de ~400 KB por tabla y por hilo — el precio de que la
// montaña se dibuje.

/// Lado, en tiles, de la ventana que la caché mapea sin choques.
const CACHE_LADO: i64 = 128;
const CACHE_SLOTS: usize = (CACHE_LADO * CACHE_LADO) as usize;

/// La ranura de una celda: su posición módulo [`CACHE_LADO`]. Sin hash — es lo
/// que hace que un recorrido por filas no se pise a sí mismo.
#[inline]
fn slot_espacial(tx: i64, tz: i64) -> usize {
    (tz.rem_euclid(CACHE_LADO) * CACHE_LADO + tx.rem_euclid(CACHE_LADO)) as usize
}

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
    /// La época de la semilla con la que se calculó. Cambiar de partida cambia el
    /// mundo entero (`crate::partida`), así que lo memorizado con la semilla
    /// anterior deja de valer — sin esto, la primera partida nueva salía con
    /// parches del mundo anterior donde la caché seguía caliente.
    epoca: u32,
}

thread_local! {
    static LEVEL_CACHE: RefCell<Vec<LevelSlot>> = RefCell::new(
        vec![LevelSlot { tx: i64::MIN, tz: i64::MIN, body: 0, field: 0.0, level: 0, filled: false, epoca: 0 }; CACHE_SLOTS],
    );
}

/// `(campo, nivel)` de una celda, por caché.
fn cached_cell(body: CelestialBody, tx: i64, tz: i64) -> (f32, i8) {
    let b = body as u8;
    let epoca = crate::partida::epoca();
    let slot = slot_espacial(tx, tz);
    LEVEL_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let e = &mut c[slot];
        if e.filled && e.body == b && e.tx == tx && e.tz == tz && e.epoca == epoca {
            return (e.field, e.level);
        }
        let field = compute_field(body, tx, tz);
        let level = level_from_field(body, field);
        // en modo grueso (sin ríos) se calcula pero NO se guarda — ver `crate::grueso`
        if !crate::grueso::activo() {
            *e = LevelSlot { tx, tz, body: b, field, level, filled: true, epoca };
        }
        (field, level)
    })
}

fn cached_level(body: CelestialBody, tx: i64, tz: i64) -> i8 {
    cached_cell(body, tx, tz).1
}

// ---------------------------------------------------------------------------
// Caché del ALTO DE PARED y de la CLASIFICACIÓN
// ---------------------------------------------------------------------------
//
// La caché de niveles arregla el fondo del pozo, pero no la redundancia de
// arriba: `relief_cell(x, z)` pregunta por el canto que tiene al norte, y
// `relief_cell(x, z+1)` pregunta por EL MISMO canto — hasta once celdas seguidas
// repiten `alto_de_pared` y `es_paso` sobre la misma fila. Y el cliente recorre
// cada chunk varias veces (malla, decoración, orilla, sombra, cascadas, cordón):
// otra vez lo mismo, entero.
//
// Las dos memorias son puras: dependen sólo de `(cuerpo, celda, época)`.

#[derive(Clone, Copy)]
struct ParedSlot {
    tx: i64,
    tz: i64,
    body: u8,
    alto: i16,
    filled: bool,
    epoca: u32,
}

#[derive(Clone, Copy)]
struct CellSlot {
    tx: i64,
    tz: i64,
    body: u8,
    cell: ReliefCell,
    filled: bool,
    epoca: u32,
}

thread_local! {
    static PARED_CACHE: RefCell<Vec<ParedSlot>> = RefCell::new(
        vec![ParedSlot { tx: i64::MIN, tz: i64::MIN, body: 0, alto: 0, filled: false, epoca: 0 }; CACHE_SLOTS],
    );
    static CELL_CACHE: RefCell<Vec<CellSlot>> = RefCell::new(
        vec![CellSlot { tx: i64::MIN, tz: i64::MIN, body: 0, cell: ReliefCell::Flat, filled: false, epoca: 0 }; CACHE_SLOTS],
    );
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

    /// **Ventanas repartidas por el planeta.** Un bordillo es una pared de UN
    /// escalón ([`alto_de_pared`] `<= 1`), así que una sola ventana de
    /// 1.200×1.200 puede caer entera en una comarca alta —toda paredes— y
    /// contestar "acá no se trepa nada", que es justamente lo que la regla
    /// promete. Lo que hay que medir es el planeta.
    const VENTANAS: &[(i64, i64)] = &[
        // Dos de cordillera (`donde_estan_las_montanas`): sin ellas el muestreo
        // era todo llanura y ninguna pared pasaba de tres escalones — que fue
        // justo el error de calibración de la primera escalera.
        (1_900, -4_900),
        (4_092, -4_960),
        (0, 0),
        (2_400, -1_800),
        (-3_100, 900),
        (1_500, 3_300),
        (-2_000, -2_600),
        (4_200, 1_100),
        (-3_450, 3_150),
        (3_000, 3_000),
    ];

    /// Un bordillo es una pared de un escalón; una pared de verdad mide más. Y el
    /// mundo tiene que traer las dos cosas: si fueran todos bordillos el relieve
    /// dejaría de ser un obstáculo, y si fueran todos paredes la llanura sería un
    /// laberinto. El reparto se imprime para poder calibrarlo.
    #[test]
    fn the_world_has_both_ledges_and_walls() {
        let b = CelestialBody::Earth;
        let (mut ledges, mut walls) = (0u32, 0u32);
        for (ox, oz) in VENTANAS {
            for tz in oz - 220..oz + 220 {
                for tx in ox - 220..ox + 220 {
                    if relief_cell(b, tx, tz) != ReliefCell::Cliff {
                        continue;
                    }
                    if climbable_edge(b, tx, tz) {
                        ledges += 1;
                    } else {
                        walls += 1;
                    }
                }
            }
        }
        eprintln!("bordes: {ledges} bordillos · {walls} paredes");
        assert!(ledges > 0, "ningún borde se puede trepar");
        assert!(walls > 0, "todos los bordes se pueden trepar");
        // El bordillo MANDA en número y eso es correcto: la mitad del planeta
        // vive en las dos primeras terrazas y ahí el relieve tiene que dejar
        // caminar. Lo que la banda vigila es que no se vaya a ninguno de los dos
        // extremos — ni mundo de queso ni alfombra.
        let share = ledges as f32 / (ledges + walls) as f32;
        assert!((0.20..=0.90).contains(&share), "bordillos = {:.0} % de los bordes", share * 100.0);
    }

    /// **LA VERTICALIDAD EXISTE**: el mundo tiene que traer paredes de varios
    /// pisos, no sólo escalones sueltos.
    ///
    /// Es el test que le pone guardián al motivo de todo el rediseño. Antes de la
    /// escalera de [`SALTO_DE_TERRAZA`], **las 7.832 caras de la Tierra bajaban
    /// exactamente un escalón**: no había forma de que una cascada de tres pisos
    /// cayera por un desnivel de tres pisos, porque el desnivel no existía.
    #[test]
    fn hay_paredes_de_varios_pisos() {
        let b = CelestialBody::Earth;
        let mut por_alto = std::collections::BTreeMap::new();
        for (ox, oz) in VENTANAS {
            for tz in oz - 220..oz + 220 {
                for tx in ox - 220..ox + 220 {
                    if relief_cell(b, tx, tz) != ReliefCell::Cliff {
                        continue;
                    }
                    let alto = alto_de_pared(b, tx, tz);
                    if alto > 0 {
                        *por_alto.entry(alto).or_insert(0u32) += 1;
                    }
                }
            }
        }
        eprintln!("caras que cuelgan, por alto en escalones: {por_alto:?}");
        let total: u32 = por_alto.values().sum();
        assert!(total > 500, "apenas {total} caras: la muestra no prueba nada");
        let altas: u32 = por_alto.iter().filter(|(&a, _)| a >= 3).map(|(_, &n)| n).sum();
        assert!(altas > 0, "ni una sola pared de tres pisos en todo el muestreo");
        let mas_alta = *por_alto.keys().max().expect("hay caras");
        assert!(mas_alta >= 4, "la pared más alta del mundo mide {mas_alta} escalones");
        assert!(mas_alta <= MAX_PARED, "una pared de {mas_alta} escalones se sale del tope");
    }

    /// **EL DIBUJO Y LA COLISIÓN MIDEN LO MISMO.** Cada fila que un muro tapa
    /// tiene que ser roca sólida, y ninguna otra.
    ///
    /// Es el invariante que impide las dos formas de romper el mundo: pared
    /// invisible (celda que frena sin dibujo encima) y roca fantasma (celda que
    /// se camina con un farallón pintado encima).
    #[test]
    fn cada_fila_que_tapa_un_muro_es_roca() {
        let b = CelestialBody::Earth;
        let mut tapadas = 0u32;
        for (ox, oz) in VENTANAS {
            for tz in oz - 120..oz + 120 {
                for tx in ox - 120..ox + 120 {
                    if relief_cell(b, tx, tz) != ReliefCell::Cliff {
                        continue;
                    }
                    let alto = alto_de_pared(b, tx, tz);
                    if alto < 2 {
                        continue;
                    }
                    // Las filas que el muro DIBUJA: hasta `alto + 1`, o hasta el
                    // canto de la terraza siguiente si aparece antes. Es la
                    // misma poda que aplican `pie_de_pared` y
                    // `world2d::pared_apilada`, y comprobar que las tres cuentan
                    // igual es justamente el invariante.
                    let base = relief_level(b, tx, tz + 1);
                    for k in 1..=alto + 1 {
                        if k >= 2 && relief_level(b, tx, tz + k) != base {
                            break;
                        }
                        // O es cuerpo de esta pared, o es el canto de otra que
                        // arranca acá mismo (terreno escarpado), o es el corredor
                        // de un paso. Lo que no puede ser es terraza llana.
                        let c = relief_cell(b, tx, tz + k);
                        assert!(
                            matches!(c, ReliefCell::Pared | ReliefCell::Cliff | ReliefCell::Ramp),
                            "({tx},{tz}) pared de {alto}: la fila {k} salió {c:?} y se camina"
                        );
                        tapadas += 1;
                    }
                }
            }
        }
        assert!(tapadas > 200, "apenas {tapadas} filas tapadas: la muestra no prueba nada");
    }

    /// **NINGUNA RAMPA EN UNA CARA QUE MIRA AL SUR.** Regla del usuario, y el
    /// guardián que impide que vuelva por la ventana: la cara sur es la única que
    /// se enseña entera, así que un paso ahí es un tajo recto que parte el muro
    /// de arriba abajo — y cuanto más alta la pared, más largo el tajo.
    #[test]
    fn ninguna_rampa_mira_al_sur() {
        let b = CelestialBody::Earth;
        let (mut rampas, mut al_sur) = (0u32, Vec::new());
        for (ox, oz) in VENTANAS {
            for tz in oz - 200..oz + 200 {
                for tx in ox - 200..ox + 200 {
                    if relief_cell(b, tx, tz) != ReliefCell::Ramp || !canto_de_terraza(b, tx, tz) {
                        continue;
                    }
                    rampas += 1;
                    // `alto_de_pared` sólo devuelve algo cuando la cara CAE al
                    // sur: es exactamente la pregunta que hay que hacer.
                    if alto_de_pared(b, tx, tz) > 0 {
                        al_sur.push((tx, tz));
                    }
                }
            }
        }
        assert!(rampas > 100, "apenas {rampas} rampas: la muestra no prueba nada");
        assert!(
            al_sur.is_empty(),
            "{} rampas en cara sur, p.ej. {:?}",
            al_sur.len(),
            &al_sur[..al_sur.len().min(6)]
        );
    }

    /// Nada frena que no sea roca: si una celda corta el paso, es el canto de una
    /// pared alta o el cuerpo de una. La rampa y la terraza nunca frenan.
    #[test]
    fn climbing_only_ever_opens_cells() {
        let b = CelestialBody::Earth;
        for tz in -300..300 {
            for tx in -300..300 {
                if relief_blocks_walk(b, tx, tz) {
                    let c = relief_cell(b, tx, tz);
                    assert!(
                        matches!(c, ReliefCell::Cliff | ReliefCell::Pared),
                        "({tx},{tz}) frena siendo {c:?}"
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
                // **Se cuentan CANTOS, no celdas de roca.** Una pared de ocho
                // pisos ocupa nueve celdas y un paso de ocho pisos también: si
                // se contaran celdas, el ratio mediría el alto de los muros en
                // vez de cuántos bordes tienen paso.
                if !canto_de_terraza(b, tx, tz) {
                    continue;
                }
                match relief_cell(b, tx, tz) {
                    ReliefCell::Cliff => cliffs += 1,
                    ReliefCell::Ramp => ramps += 1,
                    ReliefCell::Pared | ReliefCell::Flat => {}
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

    /// **TODA MESETA SE CIERRA CON UNA RAMPA** — una por una, no en promedio.
    ///
    /// Regla del usuario: *"toda meseta como mínimo debe tener una rampa para
    /// poder subir en una dirección coherente. De ahí se puede tener cuatro
    /// rampas […] pero como mínima, una. Todas las mesetas se cierran."*
    ///
    /// [`every_terrace_edge_has_a_pass`] mira el RATIO global y por eso daba
    /// verde mientras el mundo tenía mesetas isla: medido con
    /// `medir_cascadas::mesetas_selladas`, **12 de 48 no tenían por dónde
    /// subir** (ni rampa, ni bordillo, ni llegada desde arriba) con un ratio del
    /// 28 %. Un promedio sano no dice nada de la meseta concreta a la que te
    /// estás acercando.
    ///
    /// Acá se marcan las componentes conexas de terreno alto y se le pregunta a
    /// cada una. Las que tocan el borde del recorte están cortadas por la mitad
    /// y no se juzgan.
    #[test]
    fn toda_meseta_tiene_por_donde_subir() {
        let b = CelestialBody::Earth;
        const R: i64 = 300;
        let ancho = (R * 2) as usize;
        let idx = |x: i64, z: i64| ((z + R) as usize) * ancho + ((x + R) as usize);
        let mut nivel = vec![0i8; ancho * ancho];
        for z in -R..R {
            for x in -R..R {
                nivel[idx(x, z)] = relief_level(b, x, z);
            }
        }
        let mut visto = vec![false; ancho * ancho];
        let (mut mesetas, mut selladas) = (0usize, Vec::new());
        for z0 in -R..R {
            for x0 in -R..R {
                let l = nivel[idx(x0, z0)];
                if l < 1 || visto[idx(x0, z0)] {
                    continue;
                }
                let mut pila = vec![(x0, z0)];
                visto[idx(x0, z0)] = true;
                let (mut celdas, mut abierta, mut cortada, mut borde) = (0usize, false, false, 0);
                while let Some((x, z)) = pila.pop() {
                    celdas += 1;
                    let mut es_borde = false;
                    for (dx, dz) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                        let (nx, nz) = (x + dx, z + dz);
                        if !(-R..R).contains(&nx) || !(-R..R).contains(&nz) {
                            cortada = true;
                            continue;
                        }
                        let nl = nivel[idx(nx, nz)];
                        if nl == l {
                            if !visto[idx(nx, nz)] {
                                visto[idx(nx, nz)] = true;
                                pila.push((nx, nz));
                            }
                        } else if nl < l {
                            es_borde = true;
                        }
                    }
                    if es_borde {
                        borde += 1;
                        if relief_cell(b, x, z) == ReliefCell::Ramp || climbable_edge(b, x, z) {
                            abierta = true;
                        }
                    }
                }
                if cortada || borde == 0 {
                    continue;
                }
                mesetas += 1;
                if !abierta {
                    selladas.push((celdas, l, x0, z0));
                }
            }
        }
        assert!(mesetas > 8, "sólo {mesetas} mesetas completas: el recorte no mide nada");
        assert!(
            selladas.is_empty(),
            "{} de {mesetas} mesetas sin una sola rampa ni bordillo en todo su borde: {:?}",
            selladas.len(),
            &selladas[..selladas.len().min(6)]
        );
    }

    /// **SE CAMINA HASTA LA CUMBRE.**
    ///
    /// Regla del usuario: *"la idea de todo esto es poder caminar hasta la cima
    /// de las montañas […] que haya caminos acantilados que nos lleven a la cima,
    /// otros que no nos lleven a nada"*.
    ///
    /// [`toda_meseta_tiene_por_donde_subir`] no basta y la diferencia importa: esa
    /// regla es **por meseta**, y un monte de doce terrazas son doce anillos
    /// encadenados. Que cada anillo tenga su rampa en algún punto de su perímetro
    /// no dice nada de si los doce están enhebrados — se podía subir tres
    /// escalones y quedarse dando vueltas. Acá se pregunta lo único que se puede
    /// jugar: **saliendo del pie, ¿se llega arriba a pie?**
    ///
    /// Lo hace un relleno de caminabilidad (`relief_blocks_walk`) desde el borde
    /// del recorte hacia adentro, y se mira si toca la cumbre. Lo que abre el
    /// camino es `world_map::sendero_de_cumbre`, enganchado en [`in_pass_band`]
    /// igual que la carretera.
    ///
    /// Los ramales muertos NO son un fallo y por eso no se cuentan: el sendero
    /// tiene tres brazos y `es_paso` corta el que cae en cara sur. Ése es el
    /// camino que no lleva a ninguna parte, y estaba pedido.
    #[test]
    fn se_camina_hasta_la_cumbre() {
        let b = CelestialBody::Earth;
        let cumbres = crate::world_map::cumbres(b);
        assert!(!cumbres.is_empty(), "la plantilla no declara un solo monte con cumbre");
        let mut sin_camino = Vec::new();
        for (nombre, (cx, cz)) in &cumbres {
            // Un radio que cubre el cono entero (`width` 0,085 de 30 km ≈ 2.550
            // tiles) con margen para arrancar en el llano de alrededor.
            const R: i64 = 2_800;
            // El relleno se hace sobre una rejilla gruesa: a un tile de paso, un
            // cuadro de 5.600 de lado son 31 millones de celdas y el test tarda
            // minutos. Con paso 4 el sendero (7 celdas de luz) sigue entrando y
            // el cono se recorre en segundos.
            const PASO: i64 = 4;
            let lado = (R * 2 / PASO) as usize;
            let idx = |x: i64, z: i64| {
                (((z + R) / PASO) as usize) * lado + (((x + R) / PASO) as usize)
            };
            let mut visto = vec![false; lado * lado];
            // Arranque: el llano del suroeste del cuadro, que está fuera del cono.
            let inicio = (-R + PASO, -R + PASO);
            let mut pila = vec![inicio];
            visto[idx(inicio.0, inicio.1)] = true;
            let mut techo = -1i8;
            while let Some((x, z)) = pila.pop() {
                let (tx, tz) = (cx + x, cz + z);
                techo = techo.max(relief_level(b, tx, tz));
                for (dx, dz) in [(PASO, 0i64), (-PASO, 0), (0, PASO), (0, -PASO)] {
                    let (nx, nz) = (x + dx, z + dz);
                    if !(-R..R).contains(&nx) || !(-R..R).contains(&nz) {
                        continue;
                    }
                    if visto[idx(nx, nz)] {
                        continue;
                    }
                    // El agua no cuenta como camino: se nada, y a la cumbre se
                    // sube caminando.
                    let nl = relief_level(b, cx + nx, cz + nz);
                    if nl < 0 || relief_blocks_walk(b, cx + nx, cz + nz) {
                        continue;
                    }
                    visto[idx(nx, nz)] = true;
                    pila.push((nx, nz));
                }
            }
            let cima = relief_level(b, *cx, *cz);
            // No se exige pisar la celda exacta de la cumbre —el centroide de la
            // espina puede caer en una celda de pared— sino LLEGAR A SU ESCALÓN.
            if techo < cima {
                sin_camino.push((*nombre, cima, techo));
            }
        }
        assert!(
            sin_camino.is_empty(),
            "hay montes a los que no se sube caminando (nombre, escalón de la cumbre, \
             hasta dónde se llega): {sin_camino:?}"
        );
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
        // La misma clasificación PERO sin el descarte por `near_step_boundary`:
        // si las dos difieren en una sola celda, el descarte se comió un muro.
        let honest = |b: CelestialBody, tx: i64, tz: i64| -> ReliefCell {
            let l = relief_level(b, tx, tz);
            if l <= 0 {
                return ReliefCell::Flat;
            }
            if es_canto(b, tx, tz, l) {
                return if es_paso(b, tx, tz, l) { ReliefCell::Ramp } else { ReliefCell::Cliff };
            }
            match pie_de_pared(b, tx, tz) {
                Some((cz, _)) => {
                    let n = relief_level(b, tx, cz);
                    if es_paso(b, tx, cz, n) {
                        ReliefCell::Ramp
                    } else {
                        ReliefCell::Pared
                    }
                }
                None => ReliefCell::Flat,
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
                    ReliefCell::Pared => '▓',
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

    /// **QUÉ TERRAZAS EXISTEN DE VERDAD** — el histograma que hay que mirar antes
    /// de tocar [`SALTO_DE_TERRAZA`].
    ///
    /// La escalera reparte alturas por número de terraza, así que sólo sirve si
    /// las terrazas que reparte son las que el mundo usa: doce escalones repartidos
    /// linealmente en `[sea, 1]` no significan doce escalones equiprobables. Sin
    /// mirar esto, la primera versión puso los farallones en las terrazas 10-12 —
    /// que en la Tierra no existen— y el planeta entero salió de bordillos.
    ///
    /// `cargo test -p ha-shared histograma_de_terrazas -- --ignored --nocapture`
    #[test]
    #[ignore = "herramienta de medición, no un guardián"]
    fn donde_estan_las_montanas() {
        let b = CelestialBody::Earth;
        let mut mejor: Vec<(i8, i64, i64)> = Vec::new();
        for i in -160..160 {
            for j in -160..160 {
                let (tx, tz) = (i * 31, j * 31);
                let l = relief_level(b, tx, tz);
                if l >= 5 {
                    mejor.push((l, tx, tz));
                }
            }
        }
        mejor.sort_unstable_by_key(|&(l, _, _)| std::cmp::Reverse(l));
        println!("\ncumbres: {:?}", &mejor[..mejor.len().min(20)]);
        // **Y los montes CON NOMBRE, que son a los que se sube.** El barrido de
        // arriba encuentra el punto más alto que pilla la rejilla; esto dice a
        // qué escalón llega de verdad cada cumbre declarada en `world_map`, que
        // es el número que decide dónde empieza la nieve.
        for (nombre, (cx, cz)) in crate::world_map::cumbres(b) {
            let mut techo = (0i8, cx, cz);
            for dz in -300..=300 {
                for dx in -300..=300 {
                    let l = relief_level(b, cx + dx, cz + dz);
                    if l > techo.0 {
                        techo = (l, cx + dx, cz + dz);
                    }
                }
            }
            println!(
                "  {nombre}: centro ({cx},{cz}) escalón {} · lo más alto a 300 tiles: {} en ({},{})",
                relief_level(b, cx, cz),
                techo.0,
                techo.1,
                techo.2
            );
        }
    }

    /// # DOS POBLACIONES NO SE DIVIDEN ENTRE SÍ
    ///
    /// Este test tuvo un error que costó una sesión entera de trabajo sobre un
    /// problema que no existía: contaba la SUPERFICIE en una rejilla ancha
    /// (±4.420 tiles, paso 17) y las CARAS celda a celda en [`VENTANAS`], que son
    /// diez parches de 400×400 — dos de ellos en la meseta de nivel 8 de la
    /// cordillera, **fuera** del cuadrado de la rejilla. Leídos como si fueran la
    /// misma muestra decían que el 13 % del planeta (niveles 4-7) no colgaba una
    /// sola cara, y no era cierto: en la rejilla no hay nivel 8 (0,02 %) y en las
    /// ventanas no hay niveles 4-6 (cero celdas). Medido donde esos niveles de
    /// verdad están (`medir_acantilados::los_niveles_4_a_7_donde_estan`) cuelgan
    /// una cara cada 115-367 celdas, que es el mismo orden que las terrazas 1-2.
    ///
    /// Así que ahora **el reparto y las caras se cuentan sobre las mismas
    /// celdas** —las de las ventanas, celda a celda— y la rejilla ancha se
    /// imprime aparte y rotulada, para mirar la forma del planeta y nada más.
    #[test]
    #[ignore = "herramienta de medición, no un guardián"]
    fn histograma_de_terrazas() {
        let b = CelestialBody::Earth;
        let mut niveles = std::collections::BTreeMap::new();
        let mut caras = std::collections::BTreeMap::new();
        let mut planeta = std::collections::BTreeMap::new();
        // Rejilla ancha y salteada: la forma del planeta. NO se cruza con las
        // caras — ver la nota de arriba.
        for i in -260..260 {
            for j in -260..260 {
                let (tx, tz) = (i * 17, j * 17);
                *planeta.entry(relief_level(b, tx, tz)).or_insert(0u32) += 1;
            }
        }
        // Y la muestra que sí se cruza: superficie y caras, las dos celda a celda
        // y sobre las mismas ventanas.
        for (ox, oz) in VENTANAS {
            for tz in oz - 200..oz + 200 {
                for tx in ox - 200..ox + 200 {
                    let l = relief_level(b, tx, tz);
                    *niveles.entry(l).or_insert(0u32) += 1;
                    if relief_cell(b, tx, tz) == ReliefCell::Cliff {
                        let s = relief_level(b, tx, tz + 1);
                        if s < l && s >= 0 {
                            *caras.entry(l).or_insert(0u32) += 1;
                        }
                    }
                }
            }
        }
        let mundo: u32 = planeta.values().sum();
        println!("\nforma del planeta — rejilla ancha ({mundo} muestras, paso 17):");
        for (n, c) in &planeta {
            println!("  nivel {n:3}  {c:7}  {:5.2}%", *c as f32 * 100.0 / mundo as f32);
        }
        let total: u32 = niveles.values().sum();
        println!("\nsuperficie por terraza EN LAS VENTANAS (de {total} celdas):");
        for (n, c) in &niveles {
            println!("  nivel {n:3}  {c:7}  {:5.2}%", *c as f32 * 100.0 / total as f32);
        }
        // **CUÁNTA TERRAZA HAY ENTRE DOS PAREDES.** Es el número que decide si una
        // pared de `n` escalones cabe: su dibujo tapa `n + 1` filas, así que si
        // entre un canto y el siguiente hay 15 tiles, un muro de 10 se come dos
        // tercios de la terraza y el mundo queda sin suelo donde caminar.
        let mut anchos: Vec<i64> = Vec::new();
        for (ox, oz) in VENTANAS {
            for tz in oz - 200..oz + 200 {
                for tx in ox - 200..ox + 200 {
                    if !canto_de_terraza(b, tx, tz) || alto_de_pared(b, tx, tz) == 0 {
                        continue;
                    }
                    let base = relief_level(b, tx, tz + 1);
                    let mut d = 1;
                    while d < 60 && relief_level(b, tx, tz + d) == base {
                        d += 1;
                    }
                    anchos.push(d);
                }
            }
        }
        anchos.sort_unstable();
        if !anchos.is_empty() {
            let p = |q: f32| anchos[((anchos.len() - 1) as f32 * q) as usize];
            println!(
                "\nterraza entre dos paredes ({} medidas): p10={} mediana={} p90={}",
                anchos.len(),
                p(0.10),
                p(0.50),
                p(0.90)
            );
        }
        let tc: u32 = caras.values().sum();
        println!("\ncaras que cuelgan, por terraza de la que caen ({tc} en total):");
        // **La densidad es lo que se lee**, no el reparto: una terraza que ocupa
        // poco planeta cuelga pocas caras aunque esté llena de cantos. Se divide
        // por las celdas DE ESTA MISMA MUESTRA (ver la nota del test).
        for (n, c) in &caras {
            let celdas = niveles.get(n).copied().unwrap_or(0);
            println!(
                "  desde nivel {n:3}  {c:6}  {:5.1}%   pared de {} escalones · una cara cada {:.0} celdas de esa terraza",
                *c as f32 * 100.0 / tc.max(1) as f32,
                SALTO_DE_TERRAZA[(*n).min(LEVELS_UP) as usize],
                celdas as f32 / (*c).max(1) as f32
            );
        }
        // Y las terrazas que existen en la muestra y NO cuelgan nada: es la
        // pregunta que el test viejo contestaba mal.
        let mudas: Vec<i8> =
            niveles.keys().copied().filter(|n| *n > 0 && !caras.contains_key(n)).collect();
        println!("terrazas presentes en la muestra sin una sola cara: {mudas:?}");
    }

    /// **LO QUE CUESTA GENERAR EL MUNDO** — la regla 2 del proyecto (60 fps del
    /// Redmi manda) medida donde se paga: `natural_tile`, que es lo que llama el
    /// streaming de chunks por cada celda.
    ///
    /// El rediseño le sumó a `relief_cell` una segunda pregunta ([`pie_de_pared`],
    /// que mira hasta once celdas al norte), así que hay que saber cuánto cuesta
    /// y no suponerlo. Se mide en llanura Y en cordillera, que son los dos
    /// extremos: en la llanura el bucle corta enseguida, en la montaña lo paga
    /// entero.
    ///
    /// `cargo test -p ha-shared coste_de_generar -- --ignored --nocapture`
    #[test]
    #[ignore = "herramienta de medición, no un guardián"]
    fn coste_de_generar_el_mundo() {
        let b = CelestialBody::Earth;
        for (nombre, ox, oz) in [("llanura", 0i64, 0i64), ("cordillera", 1_900, -4_900)] {
            // Un chunk mide 32×32; se generan 64 chunks, que es más de lo que el
            // streaming pide de golpe.
            let t0 = std::time::Instant::now();
            let mut acc = 0u64;
            for tz in oz..oz + 256 {
                for tx in ox..ox + 256 {
                    acc += crate::geography::natural_tile(b, tx, tz) as u64;
                }
            }
            let d = t0.elapsed();
            println!(
                "{nombre}: 65.536 celdas en {:?} → {:.2} µs/celda · {:.1} ms por chunk de 32×32 (checksum {acc})",
                d,
                d.as_secs_f64() * 1e6 / 65_536.0,
                d.as_secs_f64() * 1e3 * 1_024.0 / 65_536.0,
            );
        }
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

    /// **NINGÚN MONTÍCULO CUELGA UN FARALLÓN.** El bug que dejaba la isla del
    /// usuario marrón de punta a punta: una mancha de nivel N de dos filas de
    /// fondo pedía el desnivel ENTERO de su terraza —diez escalones entre la 8 y la
    /// 9— y tapaba once filas de la llanura de abajo con roca por la que no se
    /// camina. Ver [`meseta_detras`].
    #[test]
    fn una_pared_no_es_mas_alta_que_su_meseta() {
        let body = CelestialBody::Earth;
        let mut revisados = 0;
        // La sierra: es donde viven los saltos de diez escalones.
        for tz in -5_150..-4_850 {
            for tx in 1_600..1_900 {
                let alto = alto_de_pared(body, tx, tz);
                if alto == 0 {
                    continue;
                }
                revisados += 1;
                // Fondo de meseta detrás del canto, contando su propia fila.
                let l = relief_level(body, tx, tz);
                let fondo = 1 + (0..MAX_PARED)
                    .take_while(|k| relief_level(body, tx, tz - 1 - k) >= l)
                    .count() as i64;
                assert!(
                    alto <= fondo,
                    "pared de {alto} escalones sobre una meseta de {fondo} filas en ({tx},{tz})"
                );
            }
        }
        assert!(revisados > 200, "la ventana no tenía paredes que revisar: {revisados}");
    }

    /// **Y NINGUNA PARED SE COME LA TERRAZA QUE TIENE DEBAJO.** Le tiene que
    /// quedar [`TERRAZA_VISIBLE`] filas de suelo propio, o el mundo se lee como
    /// una sola roca en vez de como tortitas apiladas. Ver [`hueco_al_sur`].
    #[test]
    fn la_terraza_de_abajo_siempre_enseña_su_suelo() {
        let body = CelestialBody::Earth;
        for tz in -5_150..-4_850 {
            for tx in 1_600..1_900 {
                let alto = alto_de_pared(body, tx, tz);
                // Un bordillo no tapa nada (`pie_de_pared` sólo marca roca de dos
                // escalones para arriba), así que siempre cabe.
                if alto <= 1 {
                    continue;
                }
                let abajo = relief_level(body, tx, tz + 1);
                let filas = (0..MAX_PARED + TERRAZA_VISIBLE + 1)
                    .take_while(|k| relief_level(body, tx, tz + 1 + k) == abajo)
                    .count() as i64;
                assert!(
                    alto + 1 + TERRAZA_VISIBLE <= filas,
                    "pared de {alto} sobre una terraza de {filas} filas en ({tx},{tz})"
                );
            }
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
