//! GEOGRAFÍA DEL MUNDO — biomas, ríos, carreteras y asentamientos.
//!
//! Con 60 km de lado (`cosmic_scale`), un mundo de puro ruido de período 22 es
//! papel pintado: todo se ve igual en todas partes y volar diez minutos no
//! significa nada. Este módulo le da al planeta la estructura que hace que
//! recorrerlo valga la pena, al estilo de un mapa de mundo abierto grande:
//!
//! - **Relieve en cuatro escalas** — continentes (9 km), cordilleras (1.4 km),
//!   colinas (110 m) y textura (22 m). Se cruzan mares, se rodean sierras.
//! - **Agua que se comporta** — océano bajo el nivel del mar, playas en la
//!   orilla, **ríos** que bajan de las cumbres al mar y **pantanos** donde el
//!   terreno es bajo y húmedo a la vez.
//! - **Civilización** — asentamientos por rangos (aldea, pueblo, ciudad) y una
//!   **red de carreteras** que los une, con **puentes** donde cruza agua. Es lo
//!   que convierte un paisaje en un país.
//! - **Arquetipos por cuerpo** — la Tierra es el mundo templado donde se vive;
//!   los demás son variaciones honestas (oceánico, desértico, helado, volcánico,
//!   yermo) con su propio nivel del mar y su propia paleta, no la Tierra pintada
//!   de otro color.
//!
//! Todo es **determinista y O(1) por celda**: no hay mapa en memoria, hay una
//! función. Por eso el mismo mundo lo pueden evaluar el streaming de tiles, el
//! mapa satelital de `universe.rs` y la textura del disco del planeta sin que
//! ninguno tenga que ponerse de acuerdo con los otros.
//!
//! `worldgen.rs` (decoración) y `topdown.rs` (tiles, colisión, capas) consumen
//! esto; no dupliques ninguna de estas reglas allí.

use crate::celestial_bodies::{body_terrain_seed, body_walk_radius, CelestialBody};
use crate::topdown::{Tile, TILE_M};

// ---------------------------------------------------------------------------
// Ruido base
// ---------------------------------------------------------------------------

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

pub(crate) fn hash2(seed: u32, a: i64, b: i64) -> u32 {
    let a = (a as i32 as u32).wrapping_mul(0x9e37_79b9);
    let b = (b as i32 as u32).wrapping_mul(0x85eb_ca6b);
    hash_u32(seed ^ a ^ b.rotate_left(13))
}

fn unit(seed: u32, a: i64, b: i64) -> f32 {
    (hash2(seed, a, b) & 0x00ff_ffff) as f32 / 16_777_215.0
}

/// Ruido de valor bilineal suavizado; `period` en tiles.
fn value_noise(seed: u32, tx: i64, tz: i64, period: i64) -> f32 {
    let p = period.max(1);
    let (cx, cz) = (tx.div_euclid(p), tz.div_euclid(p));
    let fx = tx.rem_euclid(p) as f32 / p as f32;
    let fz = tz.rem_euclid(p) as f32 / p as f32;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sz = fz * fz * (3.0 - 2.0 * fz);
    let n00 = unit(seed, cx, cz);
    let n10 = unit(seed, cx + 1, cz);
    let n01 = unit(seed, cx, cz + 1);
    let n11 = unit(seed, cx + 1, cz + 1);
    let a = n00 + (n10 - n00) * sx;
    let b = n01 + (n11 - n01) * sx;
    a + (b - a) * sz
}

pub(crate) fn fbm(seed: u32, tx: i64, tz: i64, period: i64) -> f32 {
    let a = value_noise(seed, tx, tz, period);
    let b = value_noise(seed ^ 0x51ed_2b37, tx, tz, (period / 3).max(2));
    (a * 0.68 + b * 0.32).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Arquetipos: qué CLASE de mundo es cada cuerpo
// ---------------------------------------------------------------------------

/// La familia a la que pertenece un mundo. Decide nivel del mar, paleta y si
/// hay civilización. La Tierra es el hogar; los demás son sitios donde SE PUEDE
/// vivir, cada uno con su propio problema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Archetype {
    /// Templado: océanos, bosques, desiertos, sierras, gente. La Tierra.
    Terran,
    /// Casi todo mar, unas pocas islas. Vivir aquí es vivir en la costa.
    Oceanic,
    /// Roca y arena, sin mar; hielo en los extremos.
    Desert,
    /// Corteza helada con islas de roca asomando.
    Icy,
    /// Basalto, lava y ceniza. Hostil y espectacular.
    Volcanic,
    /// Regolito sin aire: la Luna, Mercurio, las lunitas.
    Barren,
}

pub fn archetype(body: CelestialBody) -> Archetype {
    use CelestialBody as B;
    match body {
        B::Earth => Archetype::Terran,
        B::Titan | B::Neptune => Archetype::Oceanic,
        B::Mars | B::Uranus => Archetype::Desert,
        B::Europa => Archetype::Icy,
        B::Venus | B::Io => Archetype::Volcanic,
        _ => Archetype::Barren,
    }
}

// El nivel del mar de cada cuerpo se mudó a `relief::sea_level`: ahí es donde
// vive la escalera de terrazas, y el nivel del mar no es más que su escalón cero.
// Se reexporta para no tocar los call sites históricos.
pub use crate::relief::sea_level;

/// ¿Este mundo genera civilización (casas, pueblos, carreteras)?
pub fn has_civilisation(body: CelestialBody) -> bool {
    matches!(archetype(body), Archetype::Terran)
}

// ---------------------------------------------------------------------------
// Relieve y clima
// ---------------------------------------------------------------------------

/// Sólo la escala CONTINENTAL del relieve (1 fbm en vez de 4). Es la prueba
/// barata de "¿esto es tierra firme?" que usan los asentamientos y las bocas de
/// cueva, que se consultan para las celdas vecinas de CADA tile: con la
/// elevación completa, generar un chunk costaba decenas de veces más.
pub fn continental_at(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    // **Tiene que coincidir con `relief_field` en el SIGNO** (mar o tierra),
    // aunque no en el detalle: si no, los pueblos se fundan donde esta función
    // cree que hay costa y el terreno de verdad pone océano. Por eso comparte la
    // plataforma de la plantilla y sólo se ahorra tres de las cuatro octavas.
    let h = fbm(body_terrain_seed(body), tx, tz, 9_000);
    let amp = crate::world_map::RELIEF_AMPLITUDE * 0.7;
    // Sin lagunas: ver `world_map::platform_sin_lagunas` (rompe el círculo
    // laguna → pueblo → continental → laguna).
    (crate::world_map::platform_sin_lagunas(body, tx, tz) + (h - 0.5) * amp).clamp(0.0, 1.0)
}

/// Relieve compuesto, 0 (fosa) … 1 (cumbre).
///
/// Es exactamente [`crate::relief::relief_field`]: la altura del terreno y el
/// campo que se escalona en terrazas son **la misma cosa**, y tenerlos separados
/// era pedir que se fueran desincronizando (el mapa satelital pintando una loma
/// donde el suelo tenía un acantilado). La octava fina de 22 m que esta función
/// tenía se quitó a propósito — ver la nota en `relief_field`.
pub fn elevation_at(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    crate::relief::relief_field(body, tx, tz)
}

/// Humedad, 0 (desierto) … 1 (selva): franjas climáticas grandes + variación
/// regional, para que los biomas sean REGIONES que se atraviesan.
pub fn moisture_at(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    let seed = body_terrain_seed(body);
    let belt = fbm(seed ^ 0x1234_abcd, tx + 900, tz - 400, 5_200);
    let regional = fbm(seed ^ 0x77A5_4400, tx - 300, tz + 700, 700);
    // Los parches de la plantilla mandan sobre el ruido climático: el Bosque
    // Denso está donde dice el mapa, no donde cayó el fbm. Sin esto, poner un
    // bosque con nombre en el mapa y que salga una estepa era cuestión de suerte.
    let authored = crate::world_map::moisture_bias(body, tx, tz);
    (belt * 0.62 + regional * 0.38 + authored).clamp(0.0, 1.0)
}

/// Frío del lugar, 0 (ecuador) … 1 (polo). En un mundo-disco la "latitud" es la
/// distancia al centro, así que los casquetes quedan en el borde.
///
/// # La cumbre ya no se enfría por acá
///
/// Esto tenía un segundo término, `altitude = ((h - 0.74) / 0.20)`, que nevaba
/// las montañas por la altura CRUDA del campo. Era el último sitio del módulo
/// donde el bioma y la geometría podían discrepar, y discrepaban: el escalón de
/// relieve no sale del campo directo sino de [`crate::relief::level_from_field`],
/// así que la nieve caía donde el ruido pasaba de 0,74 y no donde el terreno
/// levantaba una cumbre. Con los tres montes de la plantilla en el escalón 10-11
/// y la cordillera en el 9, eso dejaba la nieve repartida por casualidad.
///
/// Ahora la cumbre se decide por ESCALÓN, en [`biome_at`], igual que ya hacían
/// `Highland` y `Mountain` — que es la regla del módulo: *el bioma sigue al
/// escalón, no a la altura cruda*.
fn coldness_at(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    let r = body_walk_radius(body).max(1.0);
    let d = ((tx * tx + tz * tz) as f64).sqrt() as f32 * TILE_M / r;
    ((d - 0.66) / 0.26).clamp(0.0, 1.0)
}

/// **¿Corre agua por esta celda?** El trazado lo hace `crate::rios`.
///
/// # El cementerio del cauce de ruido
///
/// Durante mucho tiempo esto fue un **entramado de fBm**: una cresta de ruido
/// sobre un dominio deformado, con el ancho interpolado por la altura y un
/// "tirón del sumidero" que doblaba el cauce hacia las lagunas. Se probaron
/// además dos variantes de fase radial y un sesgo por la pendiente de la
/// plataforma. Todo eso se fue el 14 ago 2026 y conviene saber por qué, porque
/// cada intento parecía razonable:
///
/// | | resultado medido (`medir_rios`, Tierra, 3 ventanas) |
/// |---|---|
/// | cabeceras seguidas aguas abajo | 34 |
/// | de ésas, que llegaban al mar o a un lago | **0** |
/// | bajada media siguiendo el cauce | 0,01–0,19 escalones |
/// | lagunas con agua y sin afluente | 16 de 36 |
///
/// El tirón del sumidero llegó a subir las lagunas con afluente de 11 a 20 de 36,
/// y el sesgo por pendiente subió el cauce de sierra del 1,3 % al 3,9 % — pero
/// **ninguno movió el descenso ni una décima**, que era para lo que estaban. La
/// conclusión es estructural y no de calibración: *un campo de ruido no tiene
/// aguas abajo*. Cruza laderas en diagonal porque no sabe qué es una ladera, y no
/// hay parámetro que lo arregle.
///
/// Lo que sí lo arregla es trazar el cauce, que es lo que hace `crate::rios`:
/// naciente sorteada en la ladera alta, descenso por máxima pendiente de la
/// plataforma, bifurcación y final en el agua.
fn river_here(body: CelestialBody, tx: i64, tz: i64, _h: f32, sea: f32) -> bool {
    if sea <= 0.0 {
        return false;
    }
    crate::rios::cauce_en(body, tx, tz)
}

// ---------------------------------------------------------------------------
// Biomas
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Biome {
    Ocean,
    Beach,
    River,
    Swamp,
    Grassland,
    Forest,
    Savanna,
    Desert,
    Highland,
    Mountain,
    Snow,
    Basalt,
    Ice,
    Regolith,
}

/// **A PARTIR DE QUÉ ESCALÓN HAY NIEVE.**
///
/// Sale de medir el planeta (`relief::donde_estan_las_montanas`), no de elegirlo:
///
/// | | escalón de la cumbre |
/// |---|---|
/// | Cordillera Monte Altísimo | 9 |
/// | Monte Solitario | 10 |
/// | Pico del Cuervo | 11 |
/// | Monte Ceniza | 10 |
///
/// y la superficie de la Tierra se reparte casi entera entre los escalones 0 y 3
/// (`relief::histograma_de_terrazas`). Con **9** la nieve es el techo del mundo y
/// nada más: no aparece en ninguna loma, y el que sube un monte la ve llegar
/// antes de la cima.
pub const NIVEL_NIEVE: i8 = 9;

/// **LA MATRIZ DE DIFUMINADO (Bayer 4×4).**
///
/// El orden clásico de dithering ordenado: los 16 valores repartidos de forma que
/// al ir subiendo el umbral las celdas se van encendiendo lo más separadas
/// posible entre sí. Con el umbral a la mitad da un **damero**, y por encima y
/// por debajo se abre y se cierra en pixeles sueltos — que es exactamente lo que
/// se pidió: *"que vaya compartiendo borde con el suelo base normal, intercalando
/// tipo ajedrez, difuminándose en pixeles"*.
///
/// Ordenada y no al azar a propósito: el ruido da manchas y grumos, la matriz da
/// una trama pareja que se lee como una transición y no como suciedad. Y es
/// determinista por celda, así que no hay que guardar un byte por tile.
const BAYER: [[u8; 4]; 4] = [
    [0, 8, 2, 10],
    [12, 4, 14, 6],
    [3, 11, 1, 9],
    [15, 7, 13, 5],
];

/// **QUÉ PARTE DEL ESCALÓN DE ABAJO SE DIFUMINA**, de 0 a 1.
///
/// **0,12 está medido en pantalla, no elegido.** Con 0,45 —el primer intento— la
/// pradera entera de `relieve-0-pared.png` salió hecha un damero de nieve y
/// pasto de borde a borde: una terraza mide cientos de tiles y su fracción de
/// altura sube despacio, así que el 45 % de arriba **es media montaña**, no una
/// franja. La transición tiene que medirse en lo que se ve, y lo que se ve es
/// una orilla de unos pocos metros. Con 0,12 —el segundo intento— la trama todavía
/// se comía media pantalla en `relieve-0-pared.png`: se lee como un damero, no
/// como una nevada que empieza. 0,07 deja una franja que se cruza caminando.
///
/// # Y ACHICARLO NUNCA IBA A ARREGLARLO
///
/// Tres intentos bajando el número (0,45 → 0,12 → 0,07) y en la captura fresca
/// el confeti seguía ahí, porque **el ancho no era el problema: era el damero**.
/// Un [`BAYER`] de 4×4 decide celda por celda, así que DENTRO de la franja —mida
/// diez tiles o mida dos— siempre se ve un tablero de ajedrez de nieve y pasto.
/// Un tile es un metro: el jugador ve el tablero, no el degradado. Y encima cada
/// celda del damero es una celda de borde, así que `world2d::spawn_chunk_orilla`
/// le paga un sprite de orilla a cada cuadradito.
///
/// El ajedrez se queda —se pidió y está bien— pero deja de ser lo único: la
/// frontera la ondula [`ondula_el_borde`] y el ancho de la franja lo fija
/// [`AJEDREZ_EN_PASOS`] **en tiles medidos sobre la ladera**, no en unidades de
/// altura. Éste pasa a ser el TECHO de esa cuenta: en una pared vertical la franja
/// nunca se pasa de acá, y en una llanura sale angosta sola.
const ANCHO_DE_TRANSICION: f32 = 0.18;

/// **CUÁNTOS PASOS DE ANCHO TIENE LA FRANJA DE AJEDREZ**, contados caminando.
///
/// Es la corrección de fondo. Los tres intentos anteriores (0,45 → 0,12 → 0,07)
/// median la franja en **fracción de altura**, y por eso no había número que
/// funcionara en los dos sitios a la vez: en una ladera empinada la fracción
/// cambia mucho por tile y el 7 % son dos pasos, pero en una llanura casi no
/// cambia y **el mismo 7 % son doscientos tiles de tablero**. De ahí el confeti
/// de borde a borde en `relieve-0-pared.png`: la captura es una pradera casi
/// plana.
///
/// Midiendo la pendiente local, la franja mide lo mismo se camine donde se camine:
/// ocho pasos de ajedrez, que se cruzan en unos segundos y se leen como una
/// transición.
const AJEDREZ_EN_PASOS: f32 = 8.0;

/// **DE QUÉ TAMAÑO SON LAS LENGÜETAS DEL BORDE**, en tiles.
///
/// Es el período del ruido que ondula la frontera entre un bioma de altura y el
/// de abajo. `12` da manchas que se cruzan en unos pasos: bastante grandes para
/// leerse como una lengua de nieve, bastante chicas para que el borde no parezca
/// recto. Por debajo de ~6 el ruido vuelve a degenerar en confeti, que es el bug
/// que [`ANCHO_DE_TRANSICION`] documenta.
const LENGUA_DE_BIOMA: i64 = 12;

/// **CUÁNTO SE CORRE EL BORDE, ARRIBA Y ABAJO, RESPECTO DE SU LÍNEA RECTA.**
///
/// El ruido devuelve 0..1 y acá se recentra a −0,5..+0,5: la mitad de las celdas
/// adelanta la nieve y la otra mitad la retrasa, así que la frontera ondula
/// alrededor de donde caería sin ruido en vez de desplazarse en bloque.
fn ondula_el_borde(body: CelestialBody, tx: i64, tz: i64) -> f32 {
    fbm(body_terrain_seed(body) ^ 0x5ce7_1a93, tx, tz, LENGUA_DE_BIOMA) - 0.5
}

/// **¿ESTA CELDA CUENTA COMO `umbral` O MÁS, CON EL BORDE DIFUMINADO?**
///
/// El bioma de altura se decidía con un `>=` sobre el escalón, y un escalón es
/// una meseta entera: el resultado era un **corte a escuadra** entre el prado
/// verde y la cumbre blanca, que es la queja de fondo — *"está el suelo base
/// verde y arriba la meseta de nieve; se ve horrible"*.
///
/// Acá el escalón de abajo se reparte: en su parte superior
/// ([`ANCHO_DE_TRANSICION`]) la frontera **ondula** en vez de cortar recto, según
/// cuánto le falta a la celda para subir ([`crate::relief::nivel_fraccion`]).
/// Cerca del borde de abajo no pasa nadie, pegado al de arriba pasan todos, y en
/// el medio el borde va y viene en lengüetas de [`LENGUA_DE_BIOMA`] tiles. En
/// pantalla es la nieve bajando por la ladera en lenguas, no posada encima.
///
/// # EL AJEDREZ SE QUEDA; LO QUE SE ARREGLA ES DÓNDE Y CUÁNTO
///
/// La trama de [`BAYER`] se pidió tal cual —*"intercalando tipo ajedrez,
/// difuminándose en pixeles"*— y no es lo que estaba mal. Lo que estaba mal es
/// que la franja se medía en **fracción de altura**, y eso no significa lo mismo
/// en una ladera que en una llanura: en la pradera casi plana de
/// `relieve-0-pared.png` el 7 % de altura son cientos de tiles, así que el
/// tablero cubría la pantalla de borde a borde. Por eso bajar el número tres
/// veces nunca lo arregló (ver [`ANCHO_DE_TRANSICION`]).
///
/// Ahora la franja se mide **caminando** ([`AJEDREZ_EN_PASOS`]): se calcula la
/// pendiente local de la fracción y de ahí sale cuánta altura son ocho pasos
/// AQUÍ. En la llanura eso es una franja finita en vez de media pantalla; en la
/// ladera, la de siempre.
///
/// Y la frontera alrededor de la que se teje el ajedrez ya no es recta: la ondula
/// [`ondula_el_borde`], así que el límite de la nieve entra y sale en lengüetas
/// en vez de cortar por una curva de nivel perfecta.
///
/// **No difumina la geometría**: el escalón sigue siendo el que era y el
/// acantilado se dibuja igual. Lo único que se mezcla es de qué está hecho el
/// SUELO, que es lo que se pidió.
pub fn pasa_el_escalon(body: CelestialBody, tx: i64, tz: i64, level: i8, umbral: i8) -> bool {
    pasa_el_escalon_con(body, tx, tz, level, umbral, true, |x, z| {
        crate::relief::nivel_fraccion(body, x, z)
    })
}

/// **LA LÍNEA DE NIEVE SE DECIDE A ESCALA DE CORDILLERA.**
///
/// Misma trama de ajedrez, mismo ancho medido caminando; lo que cambia es sobre
/// qué campo se traza la frontera: [`crate::relief::escalon_de_cordillera`] en vez
/// del escalón de la celda.
///
/// # Por qué la nieve y no las demás gradas
///
/// **Ojo, esto estaba desactualizado y costó un bug.** Decía que sierra (5) y
/// alta montaña (8) pintan el mismo material que el prado *"así que su frontera
/// no se ve y da igual por dónde vaya"*. Dejó de ser cierto el día que
/// `pasto_sierra` se volvió una losa propia: `tiles/grass` es (71,151,87) y
/// `tiles/pasto_sierra` (106,148,88), y esa frontera **se ve**. Lo que sigue
/// siendo cierto es que la nieve es la que más se ve —blanco contra verde— y por
/// eso es la única que se traza sobre el campo de CORDILLERA, que no tiembla.
///
/// Las otras dos ya no van con ajedrez: van con la frontera trazada sobre el
/// campo de la celda ([`pasa_la_grada`]), que es lo que le da a la orla del pack
/// un borde que coser.
///
/// Medido en `relieve-0-pared.png` (1763,-5087), pantalla de 80×40 tiles: con el
/// escalón de la celda salía **73 % nieve entreverada con 27 % montaña y el 34 %
/// de las celdas dentro del damero** — confeti blanco y verde de borde a borde. El
/// ancho de la franja no era el problema (1 tile de mediana al cruzarla): el
/// problema era que la frontera pasaba por todas partes, porque el campo lleva la
/// octava de lomas de 34 m. Ver [`crate::relief::campo_de_cordillera`].
fn pasa_la_linea_de_nieve(body: CelestialBody, tx: i64, tz: i64, umbral: i8) -> bool {
    let alto = |x: i64, z: i64| {
        let (n, f) = crate::relief::escalon_de_cordillera(body, x, z);
        (n - umbral) as f32 + f
    };
    let a = alto(tx, tz);
    // Lejos de la línea no hay nada que sortear, y así el 97 % del mundo paga
    // cuatro consultas en vez de doce.
    if a >= 1.0 {
        return true;
    }
    if a <= -1.0 {
        return false;
    }
    // El ancho de la franja, medido CAMINANDO igual que en `pasa_el_escalon_con`:
    // cuánta altura cambia en un paso, por [`AJEDREZ_EN_PASOS`].
    let gx = (alto(tx + 1, tz) - alto(tx - 1, tz)) * 0.5;
    let gz = (alto(tx, tz + 1) - alto(tx, tz - 1)) * 0.5;
    let pendiente = (gx * gx + gz * gz).sqrt();
    let ancho = (pendiente * AJEDREZ_EN_PASOS).clamp(1e-4, ANCHO_DE_TRANSICION);
    // **Y ACÁ EL AJEDREZ ES SIMÉTRICO.** `pasa_el_escalon` sólo puede AÑADIR: si
    // el escalón ya llegó devuelve `true` sin sortear, y sortea nada más en el
    // escalón de abajo. Sobre el campo de la celda eso casi no se nota (la franja
    // es angosta porque la pendiente es fuerte), pero sobre el campo de cordillera
    // —que varía despacio— la franja mide sus ocho pasos de verdad y el sesgo se
    // vuelve mensurable: medido sobre la Tierra entera, la nieve pasaba del
    // **2,52 % al 4,78 %** de la tierra firme, o sea el doble, y la línea se
    // corría ladera abajo en vez de ondular alrededor de su cota.
    //
    // Con la frontera centrada en cero, la mitad de las celdas de la franja
    // adelanta la nieve y la otra mitad la retrasa: el borde ONDULA alrededor de
    // su curva de nivel, que es lo que se pidió, y el área se conserva.
    let borde = ondula_el_borde(body, tx, tz) * ancho;
    let t = ((a - borde) / ancho + 0.5).clamp(0.0, 1.0);
    let umbral_celda =
        (BAYER[tz.rem_euclid(4) as usize][tx.rem_euclid(4) as usize] as f32 + 0.5) / 16.0;
    t > umbral_celda
}

/// **La grada de sierra (5) y la de montaña (8), con la frontera TRAZADA.**
///
/// Es [`pasa_el_escalon`] sin el ajedrez: la misma frontera ondulada sobre el
/// mismo campo, pero decidida por una línea en vez de por la trama de [`BAYER`].
///
/// # Por qué éstas no llevan damero y la nieve sí
///
/// El ajedrez existe para coser dos materiales cuando no hay pieza de
/// transición. La nieve es blanco contra verde: ahí la trama se lee como
/// difuminado y el usuario la pidió así. Sierra y montaña son **dos pastos** —
/// `tiles/grass` (71,151,87) contra `tiles/pasto_sierra` (106,148,88)—, y el
/// damero entre dos verdes parecidos no se lee como degradado: se lee como
/// SUCIEDAD, un tablero de 4×4 celdas moteando la ladera. Es el moteado de la
/// captura del 16 ago 2026.
///
/// Y hay una razón de fondo, no sólo de gusto: la costura entre dos pastos ahora
/// la dibuja el pack (`world2d::spawn_chunk_orilla`), y una orla necesita una
/// FRONTERA —una línea con un adentro y un afuera—, no una nube de celdas
/// sorteadas. Con damero, cada celda del tablero pediría su propio ribete.
fn pasa_la_grada(body: CelestialBody, tx: i64, tz: i64, level: i8, umbral: i8) -> bool {
    pasa_el_escalon_con(body, tx, tz, level, umbral, false, |x, z| {
        crate::relief::nivel_fraccion(body, x, z)
    })
}

/// El cuerpo común: la frontera ondulada con el ancho medido en pasos sobre el
/// campo que le pasen, resuelta con el ajedrez de [`BAYER`] (`ajedrez = true`) o
/// con una línea (`false`, ver [`pasa_la_grada`]).
fn pasa_el_escalon_con(
    body: CelestialBody,
    tx: i64,
    tz: i64,
    level: i8,
    umbral: i8,
    ajedrez: bool,
    frac: impl Fn(i64, i64) -> f32,
) -> bool {
    if level >= umbral {
        return true;
    }
    if level != umbral - 1 {
        return false;
    }
    let f = frac(tx, tz);
    // **Cuánta altura cambia en UN PASO**, acá mismo. Es el dato que faltaba: sin
    // él, "el 7 % de arriba del escalón" es una orilla en la sierra y una provincia
    // en la pampa.
    let gx = (frac(tx + 1, tz) - frac(tx - 1, tz)) * 0.5;
    let gz = (frac(tx, tz + 1) - frac(tx, tz - 1)) * 0.5;
    let pendiente = (gx * gx + gz * gz).sqrt();
    // Ocho pasos de ancho, con el techo de siempre por si la pared es vertical.
    // El piso evita que en un terreno perfectamente llano la franja sea 0 y el
    // ajedrez desaparezca del todo.
    let ancho = (pendiente * AJEDREZ_EN_PASOS).clamp(1e-4, ANCHO_DE_TRANSICION);
    // La frontera, ondulada, y la franja de ajedrez centrada en ella.
    let borde = (1.0 - ancho) + ondula_el_borde(body, tx, tz) * ancho;
    let t = ((f - borde) / ancho + 0.5).clamp(0.0, 1.0);
    // Sin ajedrez, la frontera es la línea: pasa la mitad de arriba de la franja.
    // El área se conserva —el umbral 0,5 es el promedio de la trama de Bayer—,
    // así que cambiar de una a otra no mueve el reparto de biomas del mundo.
    let umbral_celda = if ajedrez {
        (BAYER[tz.rem_euclid(4) as usize][tx.rem_euclid(4) as usize] as f32 + 0.5) / 16.0
    } else {
        0.5
    };
    t > umbral_celda
}

/// El bioma de una celda. Es la decisión de diseño del mundo; el tile concreto
/// sale después (`biome_tile`), así el mismo bioma puede pintarse distinto en
/// cuerpos distintos.
///
/// **Los biomas de altura llegan difuminados** ([`pasa_el_escalon`]): la nieve, la
/// montaña y la sierra se disuelven en el bioma de abajo en vez de empezar de
/// golpe en el borde de una terraza. Quien necesite el bioma LISO de la terraza
/// —el acantilado, que es una pieza sola y no puede cambiar de material celda a
/// celda— pregunta por [`biome_de_terraza`].
pub fn biome_at(body: CelestialBody, tx: i64, tz: i64) -> Biome {
    biome_calc(body, tx, tz, true)
}

/// **El bioma de la TERRAZA, sin difuminar.**
///
/// Lo pide el relieve: un acantilado es una pieza de kit de tres celdas de alto y
/// su material sale del suelo que corona. Si le pasáramos el bioma difuminado,
/// una misma pared alternaría celda a celda entre el acantilado de nieve y el de
/// pasto — un muro a rayas, que es peor que el corte que el difuminado viene a
/// arreglar. La trama es del SUELO; la pared es de la terraza.
///
/// **El juego no la llama**: quien quiera saber de qué es una celda pregunta por
/// [`superficie_de`], que es la que garantiza que el suelo y la pared que lo
/// corona contesten lo mismo. Tener dos entradas fue justamente el bug del anillo
/// de la meseta. Queda pública para las MEDICIONES, que necesitan comparar el
/// bioma liso con el difuminado (`mapa_de_la_nieve`, `coste_del_relieve`).
pub fn biome_de_terraza(body: CelestialBody, tx: i64, tz: i64) -> Biome {
    biome_calc(body, tx, tz, false)
}

/// **De qué pasto es una celda.** El pasto es el único suelo con variantes de
/// clima, y la losa concreta la elige el cliente (`world2d::losa_de_pasto`): acá
/// vive la DECISIÓN, que es de la sim, para que el suelo y la pared que lo corona
/// no puedan sacarla de dos tablas distintas.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Pasto {
    /// El de referencia: pradera, y todo lo que no es pasto.
    Referencia,
    Bosque,
    Seco,
    Pantano,
    Sierra,
}

/// **LA SUPERFICIE DE UNA CELDA: de qué es, y si es pasto, de cuál.**
///
/// Una sola respuesta por celda, que consumen los tres que antes la sacaban por
/// su cuenta: la malla del suelo (qué losa pinta), la orilla (¿cambia contra el
/// vecino?) y el relieve (de qué material es la pared que corona esta celda).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Superficie {
    pub tile: Tile,
    pub pasto: Pasto,
}

/// **LA FUENTE ÚNICA DE QUÉ SE PISA EN UNA CELDA.**
///
/// # El bug que vino a cerrar
///
/// Había dos respuestas para la misma pregunta y nadie obligaba a que
/// coincidieran: la pared sacaba su material del bioma LISO ([`biome_de_terraza`])
/// y la losa del suelo sacaba su clima del bioma DIFUMINADO ([`biome_at`]). En la
/// franja del ajedrez eso es una pared de sierra sobre una losa de pradera —el
/// borde de la meseta de un color y el relleno de otro, que es como lo describió
/// el usuario el 16 ago 2026: *"es como si las mesetas no tuvieran su terreno"*.
///
/// # La regla
///
/// **El difuminado se suspende sobre el relieve.** Un canto, el cuerpo de su
/// pared y su rampa son UNA pieza de kit: si la trama del ajedrez entra ahí, la
/// pieza sale a rayas. Y como la malla pinta esa misma celda con esta misma
/// función, borde y relleno no pueden discrepar — no por disciplina, por
/// construcción.
///
/// Devuelve el suelo que hay DEBAJO del relieve: `Tile::Cliff` y `Tile::Slope`
/// son geometría (dicen "acá se termina la terraza"), no material. La roca la
/// dibuja el sprite que cuelga.
pub fn superficie_de(body: CelestialBody, tx: i64, tz: i64) -> Superficie {
    let b = superficie_epoca();
    let slot = superficie_slot(tx, tz);
    let cuerpo = body as u8;
    SUPERFICIE_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let e = &mut c[slot];
        if e.filled && e.body == cuerpo && e.tx == tx && e.tz == tz && e.epoca == b {
            return e.s;
        }
        let s = superficie_sin_memo(body, tx, tz);
        if !crate::grueso::activo() {
            *e = SuperficieSlot { tx, tz, body: cuerpo, epoca: b, s, filled: true };
        }
        s
    })
}

fn superficie_sin_memo(body: CelestialBody, tx: i64, tz: i64) -> Superficie {
    // Sobre relieve manda la terraza; en llano, el bioma difuminado de siempre.
    let manda_la_terraza = matches!(
        crate::relief::relief_cell(body, tx, tz),
        crate::relief::ReliefCell::Cliff
            | crate::relief::ReliefCell::Pared
            | crate::relief::ReliefCell::Ramp
    );
    let b = biome_calc(body, tx, tz, !manda_la_terraza);
    let t = biome_tile(body, b, tx, tz);
    // Un canto nunca tiene el pie mojado (`relief_cell` devuelve `Flat` contra el
    // agua), pero el BIOMA de al lado sí puede serlo: el pantano sortea charcos
    // celda a celda y el río pinta agua. Un acantilado con el pie mojado es
    // exactamente lo que no puede pasar, así que ahí manda el pasto.
    let t = match t {
        Tile::Water | Tile::WaterDeep if manda_la_terraza => Tile::Grass,
        otro => otro,
    };
    // **El pasto es del CLIMA, no del tile.** Una celda de charco de pantano o de
    // arena de playa no pinta pasto, pero la orilla que la separa del prado sí
    // tiene que llevar el verde del pantano — y lo mismo la pared que corona un
    // canto. Atarlo al tile dejaba la orla de todo el pantano con el verde de
    // referencia.
    Superficie { tile: t, pasto: pasto_de(b) }
}

/// La misma respuesta partiendo de un tile YA resuelto por el mundo (una edición
/// del jugador, obra civil, el piso de una casa): sólo refina de qué pasto es.
///
/// Es lo que pide la malla, que ya preguntó `render_tile` y no puede tirar esa
/// respuesta a la basura: lo que el jugador cavó gana sobre lo que dice el bioma.
pub fn superficie_de_tile(body: CelestialBody, tile: Tile, tx: i64, tz: i64) -> Superficie {
    Superficie { tile, pasto: superficie_de(body, tx, tz).pasto }
}

/// Los cuatro climas que crían su propio pasto. El resto va al de referencia: la
/// playa y el desierto no son pasto (son arena) y la cumbre es nieve.
///
/// **La montaña comparte el de la sierra**: son la misma pastura de altura, y
/// darle losa propia a cada grada era una losa por escalón — arte y atlas que el
/// Redmi no necesita. Lo que separa una terraza de la de abajo es su TONO, que ya
/// da el aclarado por escalón (`world2d::gris_de_celda`).
fn pasto_de(b: Biome) -> Pasto {
    match b {
        Biome::Forest => Pasto::Bosque,
        Biome::Savanna => Pasto::Seco,
        Biome::Swamp => Pasto::Pantano,
        Biome::Highland | Biome::Mountain => Pasto::Sierra,
        _ => Pasto::Referencia,
    }
}

// La caché de superficie es la misma tabla directa que usa `relief`: índice
// posicional (sin hash, para que un recorrido por filas no se pise a sí mismo) y
// la época de la partida en la clave, o la primera partida nueva sale con
// parches del mundo anterior. Ver `relief::slot_espacial`.
const SUPERFICIE_LADO: i64 = 128;
const SUPERFICIE_SLOTS: usize = (SUPERFICIE_LADO * SUPERFICIE_LADO) as usize;

#[derive(Clone, Copy)]
struct SuperficieSlot {
    tx: i64,
    tz: i64,
    body: u8,
    epoca: u32,
    s: Superficie,
    filled: bool,
}

thread_local! {
    static SUPERFICIE_CACHE: std::cell::RefCell<Vec<SuperficieSlot>> = std::cell::RefCell::new(
        vec![
            SuperficieSlot {
                tx: i64::MIN,
                tz: i64::MIN,
                body: 0,
                epoca: 0,
                s: Superficie { tile: Tile::Grass, pasto: Pasto::Referencia },
                filled: false,
            };
            SUPERFICIE_SLOTS
        ],
    );
}

#[inline]
fn superficie_slot(tx: i64, tz: i64) -> usize {
    (tz.rem_euclid(SUPERFICIE_LADO) * SUPERFICIE_LADO + tx.rem_euclid(SUPERFICIE_LADO)) as usize
}

#[inline]
fn superficie_epoca() -> u32 {
    crate::partida::epoca()
}

fn biome_calc(body: CelestialBody, tx: i64, tz: i64, difuminar: bool) -> Biome {
    let arch = archetype(body);
    let sea = sea_level(body);
    let h = elevation_at(body, tx, tz);
    let m = moisture_at(body, tx, tz);
    let cold = coldness_at(body, tx, tz);

    // EL BIOMA SIGUE AL ESCALÓN, no a la altura cruda: así el bioma y la geometría
    // no pueden discrepar (nunca más una playa colgada en mitad de un acantilado).
    let level = crate::relief::relief_level(body, tx, tz);
    if level < 0 {
        return if cold > 0.75 { Biome::Ice } else { Biome::Ocean };
    }
    // Playa: el pie de la primera terraza de tierra. La franja mide un cuarto de
    // escalón, así que en una costa tendida sale una playa ancha y en una brava
    // apenas una orilla — que es como se comportan las playas.
    if sea > 0.0 && level == 0 {
        let step = (1.0 - sea).max(1e-3) / crate::relief::LEVELS_UP as f32;
        if h < sea + step * 0.25 {
            return Biome::Beach;
        }
    }
    // **EL RÍO ES UN TRAZADO SOBRE EL TERRENO, NO UNA GRADA DEL RELIEVE.**
    //
    // Pintar agua es cosa del SUELO; de qué está hecha la ladera que el río cruza
    // lo dice la ladera. Sin el `difuminar &&`, una celda de acantilado con río
    // daba River → `Tile::Water` → `superficie_de` la forzaba a pasto → material
    // `"pasto"`, o sea **acantilados con el kit de PASTO en plena zona de nieve**,
    // que es lo que se ve alrededor del cauce en la captura de la sierra.
    //
    // El camino sin difuminar es exactamente el del relieve (ver `superficie_de`
    // y [`biome_de_terraza`]), así que esto no le cambia un tile al suelo.
    if difuminar && river_here(body, tx, tz, h, sea) {
        return Biome::River;
    }
    if cold > 0.70 {
        return Biome::Snow;
    }
    // **LA CUMBRE ES NIEVE, Y SE DECIDE POR ESCALÓN.**
    //
    // Es la tercera grada de la misma escalera: sierra (5), alta montaña (8) y
    // nieve (NIVEL_NIEVE). Antes la nieve salía del término de altitud de
    // `coldness_at`, o sea del campo crudo, y por lo tanto caía donde el ruido
    // pasaba un umbral y no donde el terreno levantaba una cumbre.
    //
    // El número está MEDIDO, no elegido (`relief::donde_estan_las_montanas`):
    // la cordillera Monte Altísimo corona en el escalón 9 y los tres montes
    // sueltos de la plantilla —Solitario, Cuervo, Ceniza— en 10 y 11. Con 9, la
    // nieve es exactamente el techo del mundo: la cresta de la cordillera y el
    // tercio de arriba de cada monte, que es hasta donde se sube.
    // **LAS TRES GRADAS SE PISAN DIFUMINADAS.** Con `>=` a secas, cada una
    // empezaba de golpe en el borde de una terraza y el mundo salía a franjas de
    // color. `pasa_el_escalon` reparte el 45 % de arriba del escalón anterior en
    // un damero que se abre a medida que el terreno sube (ver [`BAYER`]), así que
    // la cumbre se disuelve en la montaña, la montaña en la sierra y la sierra en
    // el prado. Con `difuminar = false` es el `>=` de siempre, que es lo que
    // necesita el acantilado ([`biome_de_terraza`]).
    let sube = |umbral: i8| {
        if difuminar {
            pasa_la_grada(body, tx, tz, level, umbral)
        } else {
            level >= umbral
        }
    };
    // **LA NIEVE VA POR SU PROPIO CAMPO** (ver [`pasa_la_linea_de_nieve`]): es la
    // única grada cuya frontera se VE —blanco contra verde— y con el escalón de
    // la celda salía entreverada en vez de trazada. Sin difuminar es el mismo
    // umbral sobre el mismo campo suave, para que el acantilado siga sacando un
    // material por terraza y no uno por celda.
    //
    // **Y sólo se pregunta cerca de la cota.** El campo suave cuesta una
    // plataforma y dos octavas, y `biome_at` se llama varias veces por celda: si
    // se consultara en toda la Tierra, el 97,8 % del planeta (que está por debajo
    // del escalón 9, medido en `auditoria_del_mapa::de_que_esta_hecha_la_tierra`)
    // lo pagaría para que le dijeran que no nieva. Quitar las dos octavas finas
    // mueve el campo como mucho su amplitud —(0,13+0,07) × 0,42 ≈ 0,084, o sea
    // menos de dos escalones— así que por debajo de `NIVEL_NIEVE − 3` la
    // respuesta es no y no hace falta calcularla.
    let nieva = level >= NIVEL_NIEVE - 3
        && if difuminar {
            pasa_la_linea_de_nieve(body, tx, tz, NIVEL_NIEVE)
        } else {
            crate::relief::escalon_de_cordillera(body, tx, tz).0 >= NIVEL_NIEVE
        };
    if nieva {
        return Biome::Snow;
    }
    if sube(8) {
        return Biome::Mountain;
    }
    if sube(5) {
        return Biome::Highland;
    }

    match arch {
        Archetype::Terran | Archetype::Oceanic => {
            // Pantano: tierra baja Y húmeda. Es el bioma que más se echa de menos
            // en un mapa grande — da variedad sin inventar tiles.
            // **Nunca adentro de un pueblo** (28 sep 2026): una ciudad fundada en
            // el llano húmedo salía sembrada de charcos entre las manzanas. El
            // casco drena; el pantano queda en el campo.
            if sea > 0.0 && level <= 1 && m > 0.60 && settlement_covering(body, tx, tz).is_none() {
                Biome::Swamp
            } else if m > 0.64 {
                Biome::Forest
            } else if m > 0.44 {
                Biome::Grassland
            } else if m > 0.28 {
                Biome::Savanna
            } else {
                Biome::Desert
            }
        }
        Archetype::Desert => {
            if m > 0.72 {
                Biome::Savanna
            } else {
                Biome::Desert
            }
        }
        Archetype::Icy => Biome::Ice,
        Archetype::Volcanic => {
            // Las planicies bajas son basalto; a partir del tercer escalón, ceniza.
            if level <= 2 {
                Biome::Basalt
            } else {
                Biome::Desert
            }
        }
        Archetype::Barren => {
            // Los MARES lunares son las cuencas: los dos escalones más bajos.
            if level <= 1 {
                Biome::Basalt
            } else {
                Biome::Regolith
            }
        }
    }
}

/// El tile que pinta un bioma. Los sólidos (`WaterDeep`, `Stone`, `RockRed`)
/// quedan para lo que de verdad corta el paso: mar abierto, cumbres y macizos.
///
/// **La tierra pelada no es un bioma: es una herida.** Este `match` devolvía
/// `Tile::Dirt` en la sabana, en el pantano y en toda tierra alta de un mundo
/// templado — o sea en la mayor parte de la Tierra. Y `tiles/dirt` no tiene losa
/// en el pack (`atlas-suelo.py` lo informa como "SIN LOSA parecida"), así que
/// esas regiones salían pintadas de marrón plano, sin textura, de horizonte a
/// horizonte: el mundo entero se veía como un baldío.
///
/// La regla ahora es la del pedido: **la superficie viva es PASTO**, con el pixel
/// del pack, y la tierra sólo se ve donde alguien la destapó — al cavar
/// (`world2d::set_tile` deja un edit `Dirt` que además rebrota solo) o donde el
/// suelo de verdad es otra cosa (arena, hielo, roca, regolito, basalto). Los
/// biomas siguen existiendo: lo que los distingue es la DENSIDAD de lo que crece
/// (`vegetation_density`) y su especie de árbol, no el color del piso — una
/// sabana con cuatro árboles contados se lee sabana igual, y mejor.
pub fn biome_tile(body: CelestialBody, biome: Biome, tx: i64, tz: i64) -> Tile {
    let seed = body_terrain_seed(body);
    match biome {
        // El océano se divide en bajío (se nada) y fondo (bloquea) según el
        // escalón: por eso se puede entrar al mar caminando desde la playa y a la
        // vez el océano abierto sigue siendo un límite.
        Biome::Ocean => crate::relief::water_tile(crate::relief::relief_level(body, tx, tz))
            .unwrap_or(Tile::Water),
        // Los ríos son SIEMPRE vadeables: un río que bloquea es un muro que parte
        // el mapa en dos sin avisar.
        Biome::River => Tile::Water,
        Biome::Beach => Tile::Sand,
        // El pantano es una mezcla: charcos entre matas. Se cruza bordeando —
        // y las matas son matas, no barro.
        //
        // **UN CHARCO DE UN TILE NO SE PUEDE DIBUJAR.** El período era 9, o sea
        // ruido casi celda a celda: medido (`medir_acantilados::agua_que_no_se_puede_dibujar`), el
        // 1,7 % de las celdas de agua del mundo quedaba con tres o cuatro lados
        // de tierra, y para ésas el autotile no tiene pieza — `autotile_shape`
        // cae a `center`, que es el interior del agua, sin orilla. En pantalla:
        // **un cuadrado celeste liso plantado en el pasto**, que es exactamente lo
        // que el usuario marcó en círculo rojo por todo el borde de la laguna.
        //
        // Se arregla en el origen y en dos pasos, porque hacen falta los dos:
        // el período sube a [`CHARCO_PERIODO`] —así un charco es una mancha y no
        // una mota— y después se le exige a la celda que TENGA CON QUÉ CASAR
        // ([`orilla_dibujable`]). Lo que no se puede dibujar no se genera.
        Biome::Swamp => {
            if charco_dibujable(seed, tx, tz, CHARCO_PROFUNDIDAD) {
                Tile::Water
            } else {
                Tile::Grass
            }
        }
        // Los ÁRBOLES los pone la decoración; la sabana se distingue del bosque
        // por cuántos hay, no por el color del suelo.
        Biome::Grassland | Biome::Forest | Biome::Savanna => Tile::Grass,
        Biome::Desert => Tile::Sand,
        // La sierra y la montaña son ALTURA, no material: de qué están hechas lo
        // decide el cuerpo. Sin esto, las tierras altas de la Luna salían de
        // TIERRA VEGETAL — un altiplano de humus en un mundo sin aire.
        Biome::Highland => match archetype(body) {
            Archetype::Barren => Tile::Regolith,
            Archetype::Volcanic => Tile::Basalt,
            Archetype::Icy => Tile::Ice,
            Archetype::Desert => Tile::RockRed,
            // La sierra de un mundo templado es pastizal de altura, no un
            // terraplén: es donde más se notaba el marrón plano desde el aire.
            _ => Tile::Grass,
        },
        // La cumbre SE PISA: la roca maciza que bloquea es la del subsuelo, y en
        // superficie lo que corta el paso es el acantilado.
        //
        // **Y se pisa PASTO, no piedra.** `Tile::Rock` dejaba la cordillera de
        // un mundo templado empedrada de gris hasta el horizonte, que es
        // exactamente el "cuadrados grises" de la queja. La regla es la misma
        // que ya cumple la sierra dos brazos más arriba: la superficie viva del
        // mundo es pasto, y la altura se lee por el relieve. La piedra desnuda
        // sigue estando donde corresponde — bajo tierra, y en los cuerpos que no
        // tienen con qué cubrirse.
        Biome::Mountain => match archetype(body) {
            Archetype::Barren => Tile::Regolith,
            Archetype::Volcanic => Tile::Basalt,
            Archetype::Icy => Tile::Ice,
            Archetype::Desert => Tile::RockRed,
            _ => Tile::Grass,
        },
        Biome::Snow | Biome::Ice => Tile::Ice,
        Biome::Basalt => Tile::Basalt,
        Biome::Regolith => Tile::Regolith,
    }
}

/// **Período del ruido de charcos del pantano, en tiles.**
///
/// Era **9**, y a esa escala el ruido cambia casi celda a celda: el pantano salía
/// moteado de agua suelta en vez de encharcado, y el moteado es lo que el autotile
/// no sabe dibujar.
///
/// **14 está medido, no elegido.** El agua que no se puede dibujar
/// (`medir_acantilados::agua_que_no_se_puede_dibujar`, ventana de 600×600 sobre la laguna de 2200,-2000)
/// pasa de **1,70 % a 0,36 %**, y el resto es costa y río, no pantano. Con 22 baja
/// un poco más (0,27 %) pero el charco se agranda tanto que el pantano pierde
/// perímetro y `las_dos_orillas_aparecen_donde_corresponde` se cae: la ribera de
/// pasto deja de ser mayoría frente a la playa de arena. 14 arregla el dibujo sin
/// vaciar el bioma.
const CHARCO_PERIODO: i64 = 14;

/// Cuántas veces se re-pregunta "¿tenés con qué casar?" contra vecinas que a su
/// vez pueden haberse quitado. Con UNA sola pasada (lo de antes) la celda se
/// evaluaba contra el ruido crudo, pero la vecina con la que contaba podía
/// haberse ido en TODA su pasada: quedaban canales de una celda de ancho hechos de
/// cuadrados celestes lisos, sin orilla — el arroyo recto con cantos a escuadra
/// junto al acantilado. Tres alcanza: cada pasada es una capa más de vecinas.
const CHARCO_PROFUNDIDAD: u8 = 3;

/// ¿Hay agua de charco en esta celda del pantano DESPUÉS de sacar lo que no se
/// puede dibujar? Recursivo por `prof` para que el quitar una celda no deje a su
/// vecina sin pareja.
fn charco_dibujable(seed: u32, tx: i64, tz: i64, prof: u8) -> bool {
    let crudo = |x: i64, z: i64| fbm(seed ^ 0x5A11_0001, x, z, CHARCO_PERIODO) < 0.42;
    if !crudo(tx, tz) {
        return false;
    }
    if prof == 0 {
        return true;
    }
    let v = |x: i64, z: i64| {
        if prof == 1 { crudo(x, z) } else { charco_dibujable(seed, x, z, prof - 1) }
    };
    orilla_dibujable(v(tx, tz - 1), v(tx + 1, tz), v(tx, tz + 1), v(tx - 1, tz))
}

/// **¿PUEDE ESTA CELDA DE AGUA DIBUJAR SU ORILLA?**
///
/// `n/e/s/w` = "por ese lado también hay agua". El autotile del cliente
/// (`world2d::autotile_shape`) sabe dibujar el interior, un borde y una esquina;
/// no tiene pieza para una celda con **tres o cuatro lados en tierra** (una gota
/// suelta o una punta de un tile) ni para **dos lados opuestos** (un canal de un
/// tile de ancho), y en esos casos cae al interior — agua sin orilla, o sea un
/// cuadrado liso.
///
/// Así que esas celdas no se generan: se dejan en tierra. Es la regla de la casa
/// aplicada al revés —*el asset se usa como está dibujado*—, y de paso redondea la
/// costa, que es lo que se pidió: sin puntas de un tile, el borde queda con la
/// forma del terreno pero sin púas.
pub fn orilla_dibujable(n: bool, e: bool, s: bool, w: bool) -> bool {
    let lados = [n, e, s, w].iter().filter(|v| **v).count();
    if lados >= 3 {
        return true;
    }
    // Dos lados sirven sólo si son ADYACENTES (una esquina); opuestos es un canal
    // de un tile y no tiene pieza.
    lados == 2 && !((n && s) || (e && w))
}

/// Cuánta vegetación admite un bioma, 0..1. Lo lee la decoración del cliente:
/// así el bosque es bosque y la sabana tiene cuatro árboles contados.
pub fn vegetation_density(biome: Biome) -> f32 {
    match biome {
        Biome::Forest => 1.0,
        Biome::Swamp => 0.55,
        Biome::Grassland => 0.28,
        Biome::Savanna => 0.10,
        Biome::Highland => 0.08,
        Biome::Beach => 0.03,
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Civilización: asentamientos por rango y carreteras
// ---------------------------------------------------------------------------

/// Lado de la celda de asentamiento, en tiles. Uno cada 2.6 km reparte ~530
/// núcleos por un planeta de 60 km: te cruzás con uno cada tanto volando, y deja
/// sitio para que una ciudad sea de verdad grande.
pub const SITE_PERIOD: i64 = 2_600;

/// Rango del núcleo: su tamaño, su trama y el ancho de la carretera que llega.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettlementTier {
    /// Un puñado de casas junto al camino.
    Hamlet,
    /// Calle principal y varias manzanas.
    Town,
    /// Trama urbana con avenidas — adonde llevan las carreteras.
    City,
}

impl SettlementTier {
    /// Radio del núcleo, en tiles.
    pub fn radius(self) -> i64 {
        match self {
            SettlementTier::Hamlet => 40,
            SettlementTier::Town => 130,
            SettlementTier::City => 620, // 1.24 km de punta a punta: un centro urbano
        }
    }
    /// Separación entre calles dentro del núcleo.
    fn block(self) -> i64 {
        match self {
            SettlementTier::Hamlet => 22,
            SettlementTier::Town => 26,
            SettlementTier::City => 34,
        }
    }
    /// Semiancho de la calzada que sale de él.
    fn road_half(self) -> f32 {
        match self {
            SettlementTier::Hamlet => 2.0,
            SettlementTier::Town => 3.0,
            SettlementTier::City => 4.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settlement {
    pub tier: SettlementTier,
    /// Centro del núcleo, en tiles absolutos.
    pub center: (i64, i64),
    /// Trama urbana. **Es lo que rompe el damero**: hasta que existió, las cuatro
    /// ciudades del mundo eran la misma ciudad con otro hash y el planeta entero
    /// se veía desde el aire como un tablero de ajedrez.
    pub style: CityStyle,
    /// Nombre, si es una ciudad AUTORADA en `world_map`. Las procedurales no
    /// tienen: son "un pueblo", y está bien que lo sean.
    pub name: Option<&'static str>,
    /// Radio del casco en tiles. Las autoradas lo declaran (una metrópolis mide
    /// lo que mide); las procedurales lo heredan de su rango.
    pub radius: i64,
}

pub use crate::world_map::CityStyle;

/// Memoria de un solo salto para [`settlement_in`], por hilo.
///
/// **Por qué existe.** Cada tile de ciudad pregunta por las NUEVE celdas de
/// asentamiento vecinas (`settlement_covering`), y cada pregunta cuesta una
/// octava de ruido continental más el barrido de ciudades autoradas. Pintar un
/// chunk de 32×32 son ~30.000 de esas consultas… sobre las mismas nueve
/// respuestas, una y otra vez. La tabla es un memo puro —misma entrada, misma
/// salida— así que no toca el determinismo: sólo deja de recalcular lo mismo.
/// Midiendo en la ciudad, un chunk pasó de ~9 ms a menos de 1 (§10).
const MEMO_SITIOS: usize = 64;

thread_local! {
    static SITIOS: std::cell::RefCell<[Option<(u32, i64, i64, Option<Settlement>)>; MEMO_SITIOS]> =
        const { std::cell::RefCell::new([None; MEMO_SITIOS]) };
}

/// El núcleo de una celda de asentamiento, si el sorteo y el terreno lo
/// permiten. Es barata a propósito: la consultan las carreteras para las nueve
/// celdas vecinas de CADA tile.
pub fn settlement_in(body: CelestialBody, sx: i64, sz: i64) -> Option<Settlement> {
    if !has_civilisation(body) {
        return None;
    }
    // La ÉPOCA entra en la clave: al empezar otra partida el sorteo de pueblos
    // cambia con la semilla (`crate::partida`) y lo memorizado deja de valer.
    let clave = (body as u32) ^ crate::partida::epoca().wrapping_mul(0x85eb_ca6b);
    let ranura = (hash2(0x5171_0000, sx, sz) ^ clave.wrapping_mul(0x9e37_79b9)) as usize
        % MEMO_SITIOS;
    if let Some(hit) = SITIOS.with(|t| {
        t.borrow()[ranura].and_then(|(b, x, z, v)| (b == clave && x == sx && z == sz).then_some(v))
    }) {
        return hit;
    }
    let calculado = settlement_in_sin_memo(body, sx, sz);
    // Se memoriza también en grueso: el sitio depende de la plataforma
    // continental, nunca de los ríos, así que un valor calculado desde el aire es
    // el mismo que el de a pie. Sin esto cada muestra del disco repetía el sorteo
    // de las nueve celdas vecinas.
    SITIOS.with(|t| t.borrow_mut()[ranura] = Some((clave, sx, sz, calculado)));
    calculado
}

fn settlement_in_sin_memo(body: CelestialBody, sx: i64, sz: i64) -> Option<Settlement> {
    // LAS CIUDADES AUTORADAS MANDAN. Si el mapa del cuerpo declara una ciudad con
    // nombre en esta celda, ésa es la ciudad: sitio, tamaño y trama fijos. El
    // sorteo por hash sólo puebla lo que queda entre ellas — pueblos y aldeas,
    // que es exactamente el papel que les toca.
    if let Some(town) = crate::world_map::town_in_site(body, sx, sz, SITE_PERIOD) {
        return Some(Settlement {
            tier: SettlementTier::City,
            center: crate::world_map::town_center(body, town),
            style: town.style,
            name: Some(town.name),
            radius: town.radius,
        });
    }
    let seed = body_terrain_seed(body) ^ 0xb01d_0000;
    let h = hash2(seed, sx, sz);
    // Reparto de rangos: la mayoría son aldeas; una ciudad es un hito.
    let tier = match h % 100 {
        0..=3 => SettlementTier::City,
        4..=20 => SettlementTier::Town,
        21..=52 => SettlementTier::Hamlet,
        _ => return None,
    };
    let margin = tier.radius() + 20;
    let span = (SITE_PERIOD - margin * 2).max(1);
    let cx = sx * SITE_PERIOD + margin + ((h >> 9) as i64).rem_euclid(span);
    let cz = sz * SITE_PERIOD + margin + ((h >> 19) as i64).rem_euclid(span);
    // Sólo se funda en tierra firme y no en la alta montaña. Prueba CONTINENTAL
    // (1 octava): esto se evalúa para 4 celdas vecinas de cada tile del mundo.
    let he = continental_at(body, cx, cz);
    let sea = sea_level(body);
    if he < sea + 0.02 || he > 0.72 {
        return None;
    }
    // No se funda un pueblo pegado a una ciudad autorada: quedaría un suburbio
    // fantasma pisándole las calles.
    if crate::world_map::town_covering(body, cx, cz).is_some() {
        return None;
    }
    Some(Settlement {
        tier,
        center: (cx, cz),
        // Los núcleos procedurales son rurales salvo los que llegan a ciudad, que
        // se ganan una retícula.
        style: match tier {
            SettlementTier::City => CityStyle::Grid,
            _ => CityStyle::Rural,
        },
        name: None,
        radius: tier.radius(),
    })
}

/// Qué hay de obra civil en una celda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Civil {
    /// Calzada sobre tierra.
    Road,
    /// Calzada sobre agua — un puente. Es lo que une dos continentes.
    Bridge,
    /// Suelo urbanizado (acera/solar) dentro de un núcleo.
    Pavement,
}

/// **¿Hay agua dibujada en esta celda?** — la única definición de "mojado" que
/// vale para decidir dónde se construye y dónde no.
///
/// Existe porque había DOS y no coincidían: `relief::is_liquid` mira el nivel del
/// relieve, y el tile que se dibuja sale además del BIOMA. Con dos predicados,
/// `civil_at` recortaba el asfalto sobre una celda que `building_tile` seguía
/// considerando seca: donde desaparecía la calzada quedaba en su lugar el ladrillo
/// del edificio, o sea **una pared sólida en mitad de la calle**. Manda el tile,
/// que es lo que el jugador ve y pisa.
///
/// El bajío cuenta igual que el fondo: un puente que sólo cubre la parte honda te
/// deja vadeando el último tramo, y eso se lee como un puente roto.
pub fn hay_agua(body: CelestialBody, tx: i64, tz: i64) -> bool {
    matches!(natural_tile(body, tx, tz), Tile::Water | Tile::WaterDeep)
}

fn dist_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vz) = (b.0 - a.0, b.1 - a.1);
    let (wx, wz) = (p.0 - a.0, p.1 - a.1);
    let len2 = vx * vx + vz * vz;
    let t = if len2 <= 1e-6 { 0.0 } else { ((wx * vx + wz * vz) / len2).clamp(0.0, 1.0) };
    let (dx, dz) = (wx - vx * t, wz - vz * t);
    (dx * dx + dz * dz).sqrt()
}

/// Obra civil en una celda: carretera, puente o suelo urbano.
///
/// La red une cada núcleo con su vecino al este y al sur. Cada tramo se comba
/// por un punto medio desplazado con hash, así las carreteras serpentean como
/// carreteras y no como una cuadrícula. Donde el tramo cruza agua sale un
/// PUENTE — que es exactamente cómo terminan conectados dos continentes sin que
/// nadie los haya colocado a mano.
pub fn civil_at(body: CelestialBody, tx: i64, tz: i64) -> Option<Civil> {
    let kind = civil_kind_at(body, tx, tz)?;
    let nat = natural_tile(body, tx, tz);
    // **LA ACERA NO SE COME LA PARED.**
    //
    // El suelo urbano se dibuja por encima del terreno natural
    // (`topdown::topdown_tile`), así que donde el borde de una terraza cruzaba un
    // casco la baldosa borraba la cara del acantilado: el muro se cortaba en seco
    // al llegar al pueblo, y encima quedaba un hueco por el que se pasaba
    // caminando —la celda dejaba de ser sólida—. Medido sobre la Tierra
    // (`medir_acantilados`), eran 21.450 celdas de acera/asfalto plantadas sobre
    // una cara.
    //
    // La CALZADA sí manda, y por eso se pregunta sólo por `Cliff`: donde pasa un
    // camino el relieve ya abre rampa (`relief::in_pass_band`), así que una
    // calzada nunca cae sobre una cara — y si cayera, una autopista cortada por
    // una pared de 2,5 m es peor que una baldosa de menos. La rampa (`Slope`)
    // tampoco cuenta: por ahí se sube, y el asfalto sube con ella.
    if nat == Tile::Cliff && kind == Civil::Pavement {
        return None;
    }
    if !matches!(nat, Tile::Water | Tile::WaterDeep) {
        return Some(kind);
    }
    // **LA CIUDAD SE ACABA EN LA ORILLA.**
    //
    // El casco es un cuadrado alrededor del centro y a una ciudad costera medio
    // cuadrado le cae en el mar. La retícula pintaba calles allá afuera y esta
    // misma función las ascendía a PUENTE: cada ciudad de la costa tenía
    // kilómetros cuadrados de viaducto sobre el océano abierto, con las veredas
    // flotando entre ellos y ni un edificio encima. Se veía desde la órbita.
    //
    // Se corta acá y no dentro de la trama porque **la trama no puede mirar el
    // terreno**: el relieve la consulta a ella para abrir rampas donde pasa un
    // camino, y preguntarse mutuamente desbordaba la pila. Acá el terreno ya está
    // en la mano. (La prueba continental —una octava— sí se puede usar allá
    // dentro, pero no distingue una costa: discrepa con el relieve de verdad en
    // toda una franja, y la franja de una metrópolis son kilómetros de asfalto
    // sobre las olas. Lo caza `ninguna_calle_ni_vereda_sobre_el_agua`.)
    if settlement_covering(body, tx, tz).is_some() {
        return None;
    }
    // Fuera de un casco, la calzada sobre líquido es un PUENTE — que es
    // exactamente cómo terminan unidos dos continentes sin que nadie los haya
    // colocado a mano.
    Some(Civil::Bridge)
}

/// Obra civil **sin mirar el terreno**: la geometría pura de la red (dónde hay
/// calzada y dónde acera). Nunca devuelve `Bridge`, porque eso depende de si
/// debajo hay agua.
///
/// Existe separada de [`civil_at`] por una razón concreta: el RELIEVE la consulta
/// para abrir rampas donde pasa un camino, y `civil_at` consulta el relieve para
/// decidir si es puente. Llamarse entre sí era una recursión infinita — desbordó
/// la pila en el primer test que la tocó. Partir la función en "geometría" y
/// "geometría + terreno" rompe el ciclo sin duplicar ninguna regla.
pub fn civil_kind_at(body: CelestialBody, tx: i64, tz: i64) -> Option<Civil> {
    if !has_civilisation(body) {
        return None;
    }
    // 1) TRAMA URBANA. Se pregunta por el núcleo que CUBRE esta celda, con la
    //    misma búsqueda que usan los edificios (`settlement_covering`, las nueve
    //    celdas vecinas). Antes esto miraba sólo cuatro —las mismas que necesita
    //    la red de enlaces, que va hacia el este y el sur—, y como una metrópolis
    //    de 3.6 km es más ancha que la celda de 2.6 km, media ciudad caía en
    //    celdas que su propio núcleo no alcanzaba a mirar: quedaba **sin una sola
    //    calle**, pero CON los edificios (ésos ya buscaban en las nueve). Se veía
    //    como un barrio entero de casas plantadas en el campo y una línea recta
    //    donde el asfalto empezaba de golpe. Lo caza `la_ciudad_no_se_corta_en_la
    //    _frontera_de_su_celda`.
    if let Some(s) = settlement_covering(body, tx, tz) {
        // Dentro del casco manda la trama, incluso cuando dice "acá no hay obra"
        // (el interior de la manzana): un enlace no la atraviesa. Desemboca en la
        // circunvalación, que es justo el borde por el que se entra.
        return urban_civil(body, &s, tx, tz);
    }
    // 2) ENLACES entre núcleos.
    match link_clearance(body, tx as f32, tz as f32) {
        Some(margen) if margen >= 0.0 => Some(Civil::Road),
        _ => None,
    }
}

/// Cuánto SOBRA (o falta) para estar sobre el enlace interurbano más cercano:
/// `semiancho − distancia`, en tiles. Positivo = la celda es calzada del enlace.
/// `None` = por acá no pasa ningún enlace.
///
/// Se expone para que las parcelas puedan retirarse de la carretera antes de
/// plantarse: el asfalto se dibuja después que ellas y las cortaría al medio.
pub fn link_clearance(body: CelestialBody, px: f32, pz: f32) -> Option<f32> {
    let p = (px, pz);
    let cx = (px.floor() as i64).div_euclid(SITE_PERIOD);
    let cz = (pz.floor() as i64).div_euclid(SITE_PERIOD);
    let seed = body_terrain_seed(body) ^ 0x20ad_5000;

    let mut best: Option<f32> = None;
    for dx in -1..=0 {
        for dz in -1..=0 {
            let (ax, az) = (cx + dx, cz + dz);
            let Some(from) = settlement_in(body, ax, az) else { continue };
            let (fx, fz) = from.center;
            for (nx, nz) in [(ax + 1, az), (ax, az + 1)] {
                let Some(to) = settlement_in(body, nx, nz) else { continue };
                let a = (fx as f32, fz as f32);
                let b = (to.center.0 as f32, to.center.1 as f32);
                let hh = hash2(seed, ax + nx * 7, az + nz * 13);
                // **EL ENLACE VA EN TRAMOS RECTOS, NO EN DIAGONAL** (28 sep 2026).
                // Era una recta (con un quiebre) a cualquier ángulo, y el kit del
                // pack sólo dibuja calles ortogonales: a 25° el asfalto salía en
                // escalones de un metro, la línea central saltaba de columna en
                // columna y el borde contra el pasto quedaba serruchado ("las
                // calles en diagonal están rotas", reporte del usuario). Ahora sale
                // del centro del pueblo por su eje —que es una calle de la
                // retícula—, dobla 90° en un punto sorteado entre el 35 y el 65 %
                // del camino, y entra al otro pueblo por su eje. Con eso la línea,
                // los cruces y las sendas salen solos: se miden sobre el asfalto
                // (`roads::pintura_con`), igual que en la ciudad.
                let t = 0.35 + (hh & 0xffff) as f32 / 65535.0 * 0.30;
                // vecino al este → tramos E-O con el quiebre N-S; al sur, al revés
                let este = nx != ax;
                let tramos: [((f32, f32), (f32, f32)); 3] = if este {
                    let xm = (a.0 + (b.0 - a.0) * t).round();
                    [(a, (xm, a.1)), ((xm, a.1), (xm, b.1)), ((xm, b.1), b)]
                } else {
                    let zm = (a.1 + (b.1 - a.1) * t).round();
                    [(a, (a.0, zm)), ((a.0, zm), (b.0, zm)), ((b.0, zm), b)]
                };
                // El ancho del enlace sale del RANGO de los dos extremos: dos
                // ciudades se unen por autopista, a una aldea le llega una ruta.
                let half = crate::roads::link_class(from.tier, to.tier).half_width();
                let d = tramos
                    .iter()
                    .map(|&(p0, p1)| dist_caja_a_tramo(p, p0, p1))
                    .fold(f32::INFINITY, f32::min);
                let margen = half - d;
                best = Some(best.map_or(margen, |m: f32| m.max(margen)));
            }
        }
    }
    best
}

/// Distancia de un punto a un tramo ORTOGONAL, en norma de caja (L∞): así la
/// calzada de alrededor es un rectángulo y dos tramos que se tocan forman una
/// esquina en ángulo recto, no un cuarto de círculo que la grilla raspa en
/// escalones. Se mide al centro de la celda (el `+0,5`).
fn dist_caja_a_tramo(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (px, pz) = (p.0.floor() + 0.5, p.1.floor() + 0.5);
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (z0, z1) = (a.1.min(b.1), a.1.max(b.1));
    let dx = (x0 - px).max(0.0).max(px - x1);
    let dz = (z0 - pz).max(0.0).max(pz - z1);
    dx.max(dz)
}

/// **La trama urbana de un núcleo en una celda**: calzada, vereda, o nada (el
/// interior de la manzana, que queda en terreno natural — patios y jardines
/// entre las casas, que es como se lee una ciudad y no una explanada de hormigón).
///
/// **La calle va CENTRADA en la línea de la retícula.** Antes ocupaba los primeros
/// `ancho` tiles de la manzana, o sea que estaba entera de un lado de la línea:
/// cada calzada tenía vereda al norte y al oeste, y al sur y al este el asfalto
/// lindaba con el pasto. Toda la ciudad tenía media vereda, y eso es exactamente
/// lo que se reportó como "veredas cortadas". Al centrarla, la vereda sale a los
/// dos lados sola, sin un caso especial.
///
/// Lo que pinta acá se recorta después contra el agua en [`civil_at`] — ver ahí
/// por qué no puede ser en esta función.
fn urban_civil(body: CelestialBody, s: &Settlement, tx: i64, tz: i64) -> Option<Civil> {
    // LA CIRCUNVALACIÓN, antes que la retícula: es el borde del casco, y es donde
    // desemboca toda carretera que llega de fuera (ver `roads::ring_class`).
    let borde = s.radius as f32
        - (tx - s.center.0).abs().max((tz - s.center.1).abs()) as f32;
    let anillo = crate::roads::ring_class(s.tier);
    let b = crate::roads::block_size(s.style, s.tier);
    let (dist_x, line_x) = nearest_grid_line(tx - s.center.0, b);
    let (dist_z, line_z) = nearest_grid_line(tz - s.center.1, b);
    let cv = crate::roads::axis_class(s.style, crate::roads::StreetAxis::Vertical, line_x);
    let ch = crate::roads::axis_class(s.style, crate::roads::StreetAxis::Horizontal, line_z);
    // Ojo: acá NO se mira si hay agua debajo. No se puede (el relieve consulta a
    // esta función) y tampoco hace falta: lo recorta `civil_at`, que sí tiene el
    // terreno en la mano.
    // **LA CALLE NO CRUZA UNA SUPERMANZANA.** Un programa de varias manzanas
    // ocupa el sitio donde iba la calle que las separaba; si se pintara igual,
    // la casa quedaría partida por el asfalto (`city::supermanzana`). El tramo
    // existe sólo si las dos manzanas que separa son de supermanzanas distintas.
    // Se pregunta sólo cerca de una línea: el interior de la manzana no paga.
    let sm = |bx: i64, bz: i64| crate::city::supermanzana(body, s, bx, bz);
    let fila = (tz - s.center.1).div_euclid(b);
    let columna = (tx - s.center.0).div_euclid(b);
    let hay_v = dist_x < cv.half_shoulder() && sm(line_x - 1, fila) != sm(line_x, fila);
    let hay_h = dist_z < ch.half_shoulder() && sm(columna, line_z - 1) != sm(columna, line_z);
    if borde < anillo.half_width()
        || (hay_v && dist_x < cv.half_width())
        || (hay_h && dist_z < ch.half_width())
    {
        Some(Civil::Road)
    } else if hay_v || hay_h || borde < anillo.half_shoulder() {
        Some(Civil::Pavement)
    } else {
        // El interior de la manzana: ni calzada ni vereda. Es la mayoría de las
        // celdas del casco.
        None
    }
}

/// (El "fleco" de la vereda —0 o 1 tile de más sorteado a lo largo de la calle—
/// se quitó el 27 sep 2026: el borde salía deshilachado, salpicado de baldosas
/// sueltas, y el usuario lo leyó como calles rotas. Una vereda del pack termina
/// en una línea recta con su cordón; el pasto de la parcela arranca ahí.)

/// La línea de retícula más cercana a un desplazamiento del centro: a qué
/// distancia queda (tiles) y qué índice tiene.
fn nearest_grid_line(d: i64, block: i64) -> (f32, i64) {
    let i = d.div_euclid(block);
    let rem = d.rem_euclid(block);
    if rem * 2 <= block {
        (rem as f32, i)
    } else {
        ((block - rem) as f32, i + 1)
    }
}

/// El tile de una pieza de obra civil. Reutiliza tiles caminables — el basalto
/// oscuro hace de asfalto y la piedra de acera — en vez de inventar más.
pub fn civil_tile(c: Civil) -> Tile {
    match c {
        // Asfalto, no basalto. La calzada usaba el tile de la roca volcánica
        // lunar por no tener uno propio: caminable pero gris plano, así que
        // ninguna calle del mundo se veía como una calle. Ver `Tile::Asphalt`.
        Civil::Road | Civil::Bridge => Tile::Asphalt,
        // `Pavement` y no `Stone`: la roca del subsuelo es SÓLIDA, así que las
        // aceras de todas las ciudades eran muros. Ver `Tile::Pavement`.
        Civil::Pavement => Tile::Pavement,
    }
}

/// El tile natural (sin obra civil ni edificios) de una celda de superficie.
///
/// **El relieve manda sobre el bioma**: un acantilado es un acantilado en el
/// desierto, en el bosque y en la Luna — es geometría, no clima. Lo que el bioma
/// decide es su ARTE (`cliff/arena` vs `cliff/pasto`), que elige el cliente.
pub fn natural_tile(body: CelestialBody, tx: i64, tz: i64) -> Tile {
    if let Some(t) = crate::relief::relief_tile(body, tx, tz) {
        return t;
    }
    let t = biome_tile(body, biome_at(body, tx, tz), tx, tz);
    // **AL PIE DE UNA PARED VA LLANO, NO AGUA.** El muro de terraza cuelga
    // tantas filas sobre la terraza de abajo como escalones mida
    // (`relief::alto_de_pared`, de 1 a 10); si el río o el charco del pantano
    // llegan hasta ahí, la roca queda pintada encima del líquido. Quién dice
    // hasta dónde llega ese dibujo: `relief::bajo_el_dibujo_de_una_pared`.
    //
    // **Y lo que sobra tras el corte tiene que poder dibujarse** (29 sep 2026):
    // secar el pie de la pared parte el agua en trozos — un canal de una celda que
    // sube junto al acantilado, un cuadrado suelto — y a un trozo así el autotile
    // no le sabe poner orilla (`orilla_dibujable`). Salía como cuadros celestes
    // lisos de cantos rectos, "el río roto" de las capturas. Ver [`mojada`].
    if matches!(t, Tile::Water | Tile::WaterDeep) && !mojada(body, tx, tz, RESTO_DEL_CORTE) {
        return tile_seco(body, tx, tz);
    }
    t
}

/// Cuántas capas de vecinas se re-miran al decidir si un resto de agua sobrevive
/// al corte de la pared (ver [`mojada`]). Igual que `CHARCO_PROFUNDIDAD`.
const RESTO_DEL_CORTE: u8 = 3;

/// ¿Queda agua en esta celda una vez cortado el pie de las paredes y quitados los
/// restos que no se pueden dibujar? Sólo juzga el agua QUIETA: un río conserva su
/// cauce tal cual (`hay_rio`), que es angosto a propósito.
fn mojada(body: CelestialBody, tx: i64, tz: i64, prof: u8) -> bool {
    if let Some(t) = crate::relief::relief_tile(body, tx, tz) {
        return matches!(t, Tile::Water | Tile::WaterDeep);
    }
    if !matches!(biome_tile(body, biome_at(body, tx, tz), tx, tz), Tile::Water | Tile::WaterDeep)
        || crate::relief::bajo_el_dibujo_de_una_pared(body, tx, tz)
    {
        return false;
    }
    if prof == 0 || hay_rio(body, tx, tz) {
        return true;
    }
    orilla_dibujable(
        mojada(body, tx, tz - 1, prof - 1),
        mojada(body, tx + 1, tz, prof - 1),
        mojada(body, tx, tz + 1, prof - 1),
        mojada(body, tx - 1, tz, prof - 1),
    )
}

/// **¿Corre un río por esta celda?** Es [`river_here`] con la altura y el nivel
/// del mar ya resueltos, para que `relief` pueda preguntarlo sin recalcular el
/// bioma entero.
///
/// Mira el CAUCE, no el tile pintado: `natural_tile` seca el río al pie de una
/// pared y preguntarle a él sería morderse la cola (es quien nos llama).
pub fn hay_rio(body: CelestialBody, tx: i64, tz: i64) -> bool {
    if crate::relief::relief_level(body, tx, tz) < 0 {
        return false;
    }
    let sea = sea_level(body);
    river_here(body, tx, tz, elevation_at(body, tx, tz), sea)
}

/// **El suelo seco de una celda mojada**: lo que habría ahí si no pasara el río
/// ni hubiera charco. Es el bioma recalculado sin agua dulce; y si aun así sale
/// líquido —una celda de mar, que no se puede secar— manda el pasto, la misma
/// salida que usa `world2d::suelo_del_relieve`.
fn tile_seco(body: CelestialBody, tx: i64, tz: i64) -> Tile {
    let b = match biome_at(body, tx, tz) {
        // El río y el pantano son AGUA DULCE sobre tierra: lo que hay debajo es
        // el bioma de al lado. Se toma el de la celda de tierra más cercana en
        // vez de reimplementar la cascada de umbrales de `biome_at`.
        Biome::River | Biome::Swamp => bioma_de_tierra_cerca(body, tx, tz),
        otro => otro,
    };
    match biome_tile(body, b, tx, tz) {
        Tile::Water | Tile::WaterDeep => Tile::Grass,
        t => t,
    }
}

/// El bioma de la celda de TIERRA más cercana, en anillos. Sin ninguna a la
/// vista (un río ancho al pie de una pared larga) se cae a pradera, que es el
/// suelo de referencia del mundo.
fn bioma_de_tierra_cerca(body: CelestialBody, tx: i64, tz: i64) -> Biome {
    for r in 1..=4i64 {
        for (dx, dz) in [(r, 0), (-r, 0), (0, r), (0, -r), (r, r), (-r, r), (r, -r), (-r, -r)] {
            let b = biome_at(body, tx + dx, tz + dz);
            if !matches!(b, Biome::River | Biome::Swamp | Biome::Ocean) {
                return b;
            }
        }
    }
    Biome::Grassland
}


// ---------------------------------------------------------------------------
// Edificios: manzanas dentro de los núcleos
// ---------------------------------------------------------------------------

/// La huella de un edificio generado: rectángulo + celda de puerta. Las paredes
/// y el suelo salen como TILES normales (`building_tile`), así colisionan, se
/// excavan y se persisten igual que todo lo demás; el cliente sólo añade el
/// techo a partir de estos mismos datos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuildingSite {
    pub min: (i64, i64),
    pub w: i64,
    pub h: i64,
    /// Celda absoluta de la puerta.
    pub door: (i64, i64),
}

/// El edificio de una MANZANA del núcleo `s`, si esa manzana está edificada.
///
/// Las manzanas son la retícula que dejan las calles (`civil_at`), y la casa se
/// planta dentro con su puerta hacia la calle. Por eso una ciudad se ve como
/// una ciudad —fachadas alineadas a la avenida— en vez de casas sueltas tiradas
/// por el campo, que es lo que salía cuando cada celda de 1.8 km ponía una casa
/// y ya.
pub fn building_in_block(
    body: CelestialBody,
    s: Settlement,
    bx: i64,
    bz: i64,
) -> Option<BuildingSite> {
    // El tamaño de manzana y el ancho de la calle salen de la TRAMA de la ciudad,
    // los mismos que dibuja `civil_at`. Antes eran `s.tier.block()` y un `+3`
    // hardcodeado que suponía calles de 3 tiles: en cuanto las avenidas pasaron a
    // medir 8, las casas empezaron a plantarse ENCIMA DE LA CALZADA — un muro de
    // madera cruzado en mitad de la autopista. Lo cazó `no_road_is_ever_blocked_by_a_cliff`.
    let block = crate::roads::block_size(s.style, s.tier);
    let street = crate::roads::street_class(s.style, bx, bz);
    let margin = street.half_shoulder().ceil() as i64;
    let (cx, cz) = s.center;
    // Origen de la manzana: justo después de la acera de su calle.
    let ox = cx + bx * block + margin;
    let oz = cz + bz * block + margin;
    // ¿Cae la manzana dentro del núcleo?
    let r = s.radius;
    if (ox - cx).abs() > r || (oz - cz).abs() > r {
        return None;
    }
    let seed = body_terrain_seed(body) ^ 0xb1_0c_0000;
    let h = hash2(seed, ox, oz);
    // Densidad por rango: la ciudad tiene casi todas las manzanas edificadas;
    // la aldea, cuatro casas contadas.
    let filled = match s.tier {
        SettlementTier::City => 82,
        SettlementTier::Town => 58,
        SettlementTier::Hamlet => 30,
    };
    if h % 100 >= filled {
        return None;
    }
    // La casa deja un patio dentro de la manzana. El interior ÚTIL es la manzana
    // menos las dos aceras (la de esta calle y la de la siguiente), no un `-4`
    // fijo: con avenidas de 8 tiles ese margen se quedaba corto por el lado sur y
    // la casa invadía la calle de la manzana de abajo.
    let inner = (block - margin * 2).max(4);
    let w = (5 + (h >> 7) as i64 % (inner - 5).max(1)).min(inner);
    let hh = (4 + (h >> 13) as i64 % (inner - 4).max(1)).min(inner);
    let min = (ox + ((h >> 19) as i64).rem_euclid((inner - w).max(1)), oz);
    // Puerta al norte, mirando a la calle de la que cuelga la manzana.
    let door = (min.0 + w / 2, min.1);
    // No se construye sobre agua, sobre roca ni colgando de un acantilado.
    // `buildable_ground` y no `!solid()`: desde que el bajío se nada, `!solid()`
    // decía que sí a fundar una casa dentro del lago.
    if !natural_tile(body, min.0, min.1).buildable_ground() {
        return None;
    }
    // **Y LA HUELLA ENTERA EN UN SOLO ESCALÓN.** La comprobación de arriba mira
    // UNA celda —la esquina noroeste— y una casa mide entre 5×4 y media manzana:
    // en cuanto el borde de una terraza le entraba por cualquier otro lado, la
    // pared de ladrillo tapaba la cara del acantilado (el tile edificado gana
    // sobre el natural, ver `topdown::topdown_tile`) y el desnivel del mundo
    // aparecía cortado a la altura de la casa. Es la misma regla que aplica
    // `city::lot_at` a las parcelas, con el mismo barrido barato.
    let (mut bajo, mut alto) = (i8::MAX, i8::MIN);
    let mut dz = 0;
    loop {
        let mut dx = 0;
        loop {
            let n = crate::relief::relief_level(body, min.0 + dx, min.1 + dz);
            bajo = bajo.min(n);
            alto = alto.max(n);
            if dx == w - 1 {
                break;
            }
            dx = (dx + 2).min(w - 1);
        }
        if dz == hh - 1 {
            break;
        }
        dz = (dz + 2).min(hh - 1);
    }
    if alto != bajo {
        return None;
    }
    Some(BuildingSite { min, w, h: hh, door })
}

/// El núcleo cuyo casco contiene este tile, si alguno.
pub fn settlement_covering(body: CelestialBody, tx: i64, tz: i64) -> Option<Settlement> {
    let cx = tx.div_euclid(SITE_PERIOD);
    let cz = tz.div_euclid(SITE_PERIOD);
    // **Gana el núcleo MÁS GRANDE**, no el primero que aparezca en el barrido.
    // Los cascos se solapan —una aldea puede fundarse pegada al borde de una
    // metrópolis y meterle 130 tiles dentro—, y devolviendo el primero, trozos
    // del borde de la ciudad quedaban gobernados por la aldea: su retícula es
    // otra, su circunvalación está en otro sitio, y donde tenía que haber anillo
    // había pasto. Lo caza `el_casco_tiene_circunvalacion`. Empate de tamaño →
    // manda el de centro más cercano, que deja la decisión sin ambigüedad.
    let mut mejor: Option<Settlement> = None;
    for dx in -1..=1 {
        for dz in -1..=1 {
            // `continue`, no `?`: una celda vecina vacía no puede cancelar la
            // búsqueda entera — ese `?` dejaba TODAS las ciudades sin edificios.
            let Some(s) = settlement_in(body, cx + dx, cz + dz) else { continue };
            let r = s.radius;
            if (tx - s.center.0).abs() > r || (tz - s.center.1).abs() > r {
                continue;
            }
            let dist = |c: &Settlement| {
                (tx - c.center.0).abs().max((tz - c.center.1).abs())
            };
            let gana = match &mejor {
                None => true,
                Some(m) => (r, -dist(&s), s.center) > (m.radius, -dist(m), m.center),
            };
            if gana {
                mejor = Some(s);
            }
        }
    }
    mejor
}

/// El edificio que ocupa este tile, si lo hay.
///
/// Devuelve la huella **techada** (la casa, no el jardín ni la pileta): es lo que
/// el cliente necesita para poner el techo y hacerlo transparente al entrar.
pub fn building_at(body: CelestialBody, tx: i64, tz: i64) -> Option<BuildingSite> {
    let s = settlement_covering(body, tx, tz)?;
    let (x, z, w, h, clave) = crate::city::roofed_rect(body, &s, tx, tz)?;
    if tx < x || tz < z || tx >= x + w || tz >= z + h {
        return None;
    }
    Some(BuildingSite {
        min: (x, z),
        w,
        h,
        door: (crate::city::puerta_del_frente(clave, x, w), z + h - 1),
    })
}

/// El tile de un edificio generado en esta celda.
///
/// **Delega en `city::lot_tile`**, que es quien sabe de parcelas: casa + garaje +
/// entrada + pileta + cerca, con el programa y el tamaño que le toque al lote. La
/// versión vieja pintaba un rectángulo de madera por manzana y ya — de ahí salía
/// el damero de cajas idénticas.
pub fn building_tile(body: CelestialBody, tx: i64, tz: i64) -> Option<Tile> {
    let s = settlement_covering(body, tx, tz)?;
    // **Donde la trama quería calzada, el edificio no la reemplaza.**
    //
    // El lote ya se rechaza entero si su barrido encuentra agua, pero el barrido
    // va cada 4 tiles y un charco más chico se le cuela. Bajo una casa eso no se
    // ve; sobre la calle sí, y de la peor manera: `civil_at` recorta el asfalto
    // encima del agua, así que donde la calzada desaparecía quedaba en su lugar el
    // ladrillo del edificio — **una pared sólida cruzada en mitad de la calle**.
    // Lo cazaron `no_road_is_ever_blocked_by_a_cliff` y
    // `a_road_crossing_a_terrace_stays_continuous`.
    if hay_agua(body, tx, tz) && civil_kind_at(body, tx, tz).is_some() {
        return None;
    }
    crate::city::lot_tile(body, &s, tx, tz)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **SOBRE RELIEVE MANDA LA TERRAZA, NO LA TRAMA DEL SUELO.**
    ///
    /// Es el guardián del anillo de la meseta por el lado del material. Había DOS
    /// respuestas para "de qué es esta celda": la pared la sacaba del bioma LISO
    /// ([`biome_de_terraza`]) y la losa del suelo del DIFUMINADO ([`biome_at`]),
    /// y nadie obligaba a que coincidieran — en la franja de transición eso es
    /// una pared de sierra plantada sobre una losa de pradera, o sea el borde de
    /// la meseta de un color y el relleno de otro.
    ///
    /// [`superficie_de`] contesta una sola vez, y sobre un canto, un cuerpo de
    /// pared o una rampa contesta con la TERRAZA. El test lo comprueba celda a
    /// celda **y exige que la muestra tenga casos donde las dos respuestas
    /// difieren**: sin eso, la afirmación se cumpliría sola y no guardaría nada.
    #[test]
    fn sobre_relieve_manda_la_terraza() {
        use crate::relief::{relief_cell, ReliefCell};
        let b = CelestialBody::Earth;
        let (mut celdas, mut discrepan) = (0u32, 0u32);
        for (cx, cz) in [(0i64, 0i64), (1_400, -5_200), (2_400, -1_800), (-3_100, 900)] {
            for tz in cz - 120..cz + 120 {
                for tx in cx - 120..cx + 120 {
                    if matches!(relief_cell(b, tx, tz), ReliefCell::Flat) {
                        continue;
                    }
                    celdas += 1;
                    let s = superficie_de(b, tx, tz);
                    assert_eq!(
                        s.pasto,
                        pasto_de(biome_de_terraza(b, tx, tz)),
                        "el relieve de ({tx},{tz}) saca su pasto de la trama del suelo"
                    );
                    if pasto_de(biome_at(b, tx, tz)) != s.pasto {
                        discrepan += 1;
                    }
                }
            }
        }
        assert!(celdas > 500, "muestra corta: {celdas} celdas de relieve");
        // **Y la afirmación de arriba tiene que poder fallar.** Las dos respuestas
        // difieren sólo en la franja de transición de una grada, que es angosta:
        // si en la muestra no cae ninguna, se busca en grande antes de dar el test
        // por bueno.
        if discrepan == 0 {
            'busca: for i in -60..60i64 {
                for j in -60..60i64 {
                    let (tx, tz) = (i * 137, j * 137);
                    if pasto_de(biome_at(b, tx, tz)) != pasto_de(biome_de_terraza(b, tx, tz)) {
                        discrepan += 1;
                        break 'busca;
                    }
                }
            }
        }
        assert!(
            discrepan > 0,
            "en {celdas} celdas de relieve el bioma difuminado y el de terraza \
             nunca difieren: el test no está guardando nada"
        );
    }

    /// **La tierra pelada se CAVA, no se genera.** Ver el comentario de
    /// [`biome_tile`]: `Tile::Dirt` no tiene losa en el pack, así que cada celda
    /// que lo devolvía era un metro cuadrado de marrón plano. Ningún bioma de
    /// ningún cuerpo puede pintarlo; lo que hay debajo del pasto se ve al cavar.
    #[test]
    fn ningun_bioma_pinta_tierra_pelada() {
        for body in CelestialBody::all() {
            for i in -30..30 {
                for j in -30..30 {
                    let (tx, tz) = (i * 617, j * 617);
                    let t = biome_tile(*body, biome_at(*body, tx, tz), tx, tz);
                    assert_ne!(t, Tile::Dirt, "{body:?} pinta tierra pelada en ({tx},{tz})");
                }
            }
        }
    }

    #[test]
    fn earth_has_a_real_geography() {
        let mut seen = std::collections::HashSet::new();
        for i in -20..20 {
            for j in -20..20 {
                seen.insert(biome_at(CelestialBody::Earth, i * 700, j * 700));
            }
        }
        for want in [Biome::Ocean, Biome::Beach, Biome::Grassland, Biome::Forest] {
            assert!(seen.contains(&want), "falta {want:?} en la Tierra: {seen:?}");
        }
        assert!(seen.len() >= 6, "geografia pobre: {seen:?}");
    }

    #[test]
    fn rivers_exist() {
        let mut rivers = 0;
        for i in -70..70 {
            for j in -70..70 {
                if biome_at(CelestialBody::Earth, i * 80, j * 80) == Biome::River {
                    rivers += 1;
                }
            }
        }
        assert!(rivers > 0, "un planeta sin un solo rio");
    }

    /// **LOS RÍOS TERMINAN EN AGUA, Y NO TODOS EN LA MISMA.**
    ///
    /// # Por qué la pregunta cambió de forma
    ///
    /// Este guardián barría las lagunas del mapa y contaba cuántas tenían un
    /// cauce **tocando su orilla**, buscando la primera celda seca de cada rumbo.
    /// Servía cuando el río era una cresta de ruido pintada sobre el terreno: el
    /// agua del lago y el agua del río eran dos manchas independientes y lo único
    /// que se podía comprobar era si se rozaban.
    ///
    /// Desde que el cauce se traza (`crate::rios`) esa medición miente, y miente
    /// hacia abajo: **el río excava hasta su perfil**, así que su desembocadura
    /// está por debajo del nivel del líquido y la sonda de orilla la descarta por
    /// mojada antes de reconocerla. Medido, daba 1 de 36 lagunas "con afluente"
    /// mientras el trazador informaba que el **49 % de los ríos del planeta
    /// terminan dentro de una laguna**. Lo que fallaba era la sonda.
    ///
    /// Así que se le pregunta al trazador, que es quien sabe dónde para cada río
    /// y por qué. Se comprueban las dos mitades del pedido del usuario —*"llegue
    /// al lago, llegue al mar"*—: que una parte sana termine en agua, y que no
    /// terminen todos en lo mismo. Un charco de lluvia en una hondonada es un
    /// charco, no la desembocadura de nada.
    #[test]
    fn hay_lagos_con_afluente_y_lagos_sin_el() {
        use crate::rios::{destinos, Destino};
        let b = CelestialBody::Earth;
        let mut mar = 0;
        let mut lago = 0;
        let mut total = 0;
        for cz in -10..=10 {
            for cx in -10..=10 {
                for d in destinos(b, cx, cz) {
                    total += 1;
                    match d {
                        Destino::Mar => mar += 1,
                        Destino::Lago => lago += 1,
                        // Entubado en el borde de un pueblo: llega a un desagüe.
                        Destino::Pueblo => mar += 1,
                        _ => {}
                    }
                }
            }
        }
        assert!(total >= 100, "sólo {total} ríos trazados: el barrido no mide nada");
        assert!(
            (mar + lago) * 2 >= total,
            "sólo {} de {total} ríos llegan al agua: los cauces vuelven a morir en seco",
            mar + lago
        );
        assert!(lago > 0, "ningún río termina en una laguna: el agua quieta no recibe nada");
        assert!(mar > 0, "ningún río llega al mar: el continente no drena");
        assert!(
            lago * 10 < total * 9,
            "{lago} de {total} ríos terminan en laguna: se pidió que ALGUNOS lo hicieran, no todos"
        );
    }

    #[test]
    fn every_world_is_walkable_somewhere() {
        // Ningún arquetipo puede salir casi todo sólido: aterrizar ahí sería
        // quedar enterrado.
        for &b in CelestialBody::all() {
            if !crate::celestial_bodies::body_landable(b) {
                continue;
            }
            let mut walkable = 0;
            let r = body_walk_radius(b).max(1.0) as i64;
            for i in -12..12 {
                for j in -12..12 {
                    // `buildable_ground`: tierra donde PLANTARSE. Con `!solid()`
                    // el bajío contaría como suelo y un mundo hecho de charcos
                    // pasaría el test sin tener dónde poner una base.
                    if natural_tile(b, i * r / 12, j * r / 12).buildable_ground() {
                        walkable += 1;
                    }
                }
            }
            // 15 % es el mínimo: un mundo oceánico ES mayormente agua a
            // propósito, pero tiene que haber isla donde posarse y vivir.
            assert!(walkable > 90, "{b:?} casi sin suelo pisable: {walkable}/576");
        }
    }

    #[test]
    fn roads_connect_settlements_and_bridge_water() {
        // La red vial existe, une núcleos y **cruza el agua con puente** en vez de
        // cortarse en la orilla.
        //
        // Se barre una franja del mundo en vez de caminar el segmento recto entre
        // dos núcleos, que es lo que hacía antes: los tramos se COMBAN por un punto
        // medio desplazado hasta un 22 % de su longitud (`civil_at`), o sea hasta
        // ~570 tiles en un tramo de 2.6 km. Muestrear la recta era muestrear campo
        // vacío — encontraba calzada sólo cerca de los extremos y no veía un puente
        // en su vida. Este barrido mira dónde ESTÁ la carretera, no dónde debería.
        let body = CelestialBody::Earth;
        let (mut roads, mut bridges, mut pavement) = (0, 0, 0);
        let r = body_walk_radius(body) as i64;
        for tz in (-r / 2..r / 2).step_by(7) {
            for tx in (-r / 2..r / 2).step_by(7) {
                match civil_at(body, tx, tz) {
                    Some(Civil::Road) => roads += 1,
                    Some(Civil::Bridge) => bridges += 1,
                    Some(Civil::Pavement) => pavement += 1,
                    None => {}
                }
            }
        }
        assert!(roads > 200, "apenas {roads} celdas de calzada en medio planeta");
        assert!(pavement > 50, "ciudades sin acera: {pavement}");
        assert!(bridges > 0, "ninguna carretera cruza agua con puente");
    }

    #[test]
    fn cities_are_rarer_than_hamlets() {
        let (mut cities, mut hamlets) = (0, 0);
        for sx in -30..30 {
            for sz in -30..30 {
                match settlement_in(CelestialBody::Earth, sx, sz).map(|s| s.tier) {
                    Some(SettlementTier::City) => cities += 1,
                    Some(SettlementTier::Hamlet) => hamlets += 1,
                    _ => {}
                }
            }
        }
        assert!(hamlets > cities * 3, "ciudades {cities} vs aldeas {hamlets}");
        assert!(cities > 0, "un mundo sin una sola ciudad");
    }

    #[test]
    fn archetypes_are_actually_different_worlds() {
        // Cada arquetipo tiene su propia cantidad de líquido. TODOS los cuerpos
        // con atmósfera tienen algo (el juego apunta a divertido, no a realista:
        // un lago de metano en Titán y una salmuera marciana son sitios adonde ir),
        // pero un mundo oceánico tiene que ser MUCHO más líquido que la Tierra y
        // un desierto, mucho menos. Si no, son la Tierra repintada.
        // El muestreo se escala al RADIO del cuerpo y cubre el planeta entero. La
        // rejilla fija de ±3.6 km que había sólo veía el continente central, así
        // que medía "cuánta agua hay tierra adentro" — que en la Tierra es casi
        // nada — y comparaba eso con el océano de Marte. Comparación sin sentido.
        let water_frac = |b: CelestialBody| {
            let r = body_walk_radius(b).max(1.0) as i64;
            let mut w = 0;
            for i in -14..14 {
                for j in -14..14 {
                    if matches!(
                        natural_tile(b, i * r / 14, j * r / 14),
                        Tile::Water | Tile::WaterDeep
                    ) {
                        w += 1;
                    }
                }
            }
            w as f32 / (28.0 * 28.0)
        };
        let earth = water_frac(CelestialBody::Earth);
        let titan = water_frac(CelestialBody::Titan);
        let mars = water_frac(CelestialBody::Mars);
        let moon = water_frac(CelestialBody::Moon);
        assert!(titan > earth, "oceanico {titan} <= terrestre {earth}");
        // Medido: tierra 0.32 · titán 0.41 · marte 0.17 · luna 0.00. Lo que el
        // test defiende es la RELACIÓN entre arquetipos, no un número exacto: un
        // desierto tiene claramente menos mar que un mundo templado, pero tiene.
        assert!(mars < earth * 0.7, "el desierto tiene tanto mar como la Tierra: {mars}");
        assert!(mars > 0.0, "un desierto sin un solo lago adonde ir: {mars}");
        assert_eq!(moon, 0.0, "la Luna con mares líquidos: {moon}");
    }

    #[test]
    fn cities_have_streets_and_buildings_along_them() {
        // Una ciudad tiene que tener las dos cosas: calzada y fachadas. Si sólo
        // hay una, es un descampado con rayas o un bloque macizo.
        let body = CelestialBody::Earth;
        let mut city = None;
        'find: for sx in -30..30 {
            for sz in -30..30 {
                if let Some(s) = settlement_in(body, sx, sz) {
                    if s.tier == SettlementTier::City {
                        city = Some(s);
                        break 'find;
                    }
                }
            }
        }
        let s = city.expect("ninguna ciudad en 60x60 celdas");
        let (mut roads, mut walls, mut doors) = (0, 0, 0);
        let r = s.radius;
        for dx in -r..r {
            for dz in -r..r {
                let (x, z) = (s.center.0 + dx, s.center.1 + dz);
                if civil_at(body, x, z) == Some(Civil::Road) {
                    roads += 1;
                }
                match building_tile(body, x, z) {
                    Some(Tile::WoodWall) => walls += 1,
                    Some(Tile::Door) => doors += 1,
                    _ => {}
                }
            }
        }
        assert!(roads > 500, "ciudad sin calles: {roads}");
        assert!(walls > 500, "ciudad sin edificios: {walls}");
        assert!(doors > 20, "edificios sin puertas: {doors}");
    }

    /// Una de las ciudades autoradas grandes, para mirarla entera.
    fn una_metropoli() -> Settlement {
        let body = CelestialBody::Earth;
        let t = &crate::world_map::template(body).towns[0];
        Settlement {
            tier: SettlementTier::City,
            center: crate::world_map::town_center(body, t),
            style: t.style,
            name: Some(t.name),
            radius: t.radius,
        }
    }

    /// **LA CIUDAD NO SE CORTA EN LA FRONTERA DE SU CELDA.**
    ///
    /// La celda de asentamiento mide 2.6 km y una metrópolis 3.6: la ciudad es
    /// más ancha que la casilla donde está anotada. `civil_kind_at` sólo miraba
    /// cuatro celdas vecinas (las que necesita la red de enlaces, que va al este
    /// y al sur), así que desde la frontera hacia el norte y el oeste no
    /// encontraba su propia ciudad y no pintaba ni una calle — pero los
    /// edificios sí salían, porque ésos ya buscaban en las nueve. El resultado
    /// era un barrio entero de casas en medio del campo y una línea recta donde
    /// el asfalto empezaba de golpe.
    ///
    /// Se mide cruzando esa frontera: el asfalto tiene que ser igual de denso a
    /// los dos lados.
    #[test]
    fn la_ciudad_no_se_corta_en_la_frontera_de_su_celda() {
        let body = CelestialBody::Earth;
        let s = una_metropoli();
        let (cx, cz) = s.center;
        // La frontera de celda más cercana al centro, en cada eje.
        for eje in 0..2 {
            let borde = if eje == 0 {
                cx.div_euclid(SITE_PERIOD) * SITE_PERIOD
            } else {
                cz.div_euclid(SITE_PERIOD) * SITE_PERIOD
            };
            let (mut antes, mut despues) = (0, 0);
            // 200 tiles a cada lado de la línea, dentro del casco.
            for k in 1..=200i64 {
                for t in (-400..400).step_by(7) {
                    let (a, d) = if eje == 0 {
                        ((borde - k, cz + t), (borde + k - 1, cz + t))
                    } else {
                        ((cx + t, borde - k), (cx + t, borde + k - 1))
                    };
                    if (a.0 - cx).abs() > s.radius || (a.1 - cz).abs() > s.radius {
                        continue;
                    }
                    antes += civil_at(body, a.0, a.1).is_some() as i64;
                    despues += civil_at(body, d.0, d.1).is_some() as i64;
                }
            }
            assert!(antes > 500 && despues > 500, "eje {eje}: {antes} / {despues} celdas de obra");
            // Menos de un 25 % de diferencia: la misma ciudad a los dos lados.
            let (lo, hi) = (antes.min(despues) as f32, antes.max(despues) as f32);
            assert!(
                lo / hi > 0.75,
                "eje {eje}: la trama se corta en la frontera de celda ({antes} de un lado, {despues} del otro)"
            );
        }
    }

    /// **NI UNA CALLE NI UNA VEREDA SOBRE EL AGUA, DENTRO DE UNA CIUDAD.**
    ///
    /// El casco es un cuadrado alrededor del centro, y a una ciudad costera medio
    /// cuadrado le cae en el agua. La retícula pintaba calles allá afuera y
    /// `civil_at` las ascendía a PUENTE: cada ciudad de la costa tenía kilómetros
    /// cuadrados de viaducto sobre el océano, con las veredas flotando entre
    /// ellos y ni un edificio encima. Se veía desde la órbita.
    ///
    /// Se mide contra el TERRENO QUE SE VE (`natural_tile`) y no contra la
    /// aproximación continental de una octava: esa aproximación fue el primer
    /// intento de arreglo y no sirvió, porque discrepa con el relieve de verdad en
    /// toda una franja y la franja de una metrópolis son kilómetros de asfalto
    /// sobre las olas. Un test escrito contra la aproximación lo habría dado por
    /// bueno — habría estado comprobando la regla en vez del mapa.
    ///
    /// Los puentes de los ENLACES entre ciudades no cuentan y tienen que seguir
    /// existiendo: son los que unen los continentes.
    #[test]
    fn ninguna_calle_ni_vereda_sobre_el_agua() {
        let body = CelestialBody::Earth;
        let mut sobre_agua = 0;
        let mut revisadas = 0;
        for t in crate::world_map::template(body).towns {
            let s = Settlement {
                tier: SettlementTier::City,
                center: crate::world_map::town_center(body, t),
                style: t.style,
                name: Some(t.name),
                radius: t.radius,
            };
            let r = s.radius;
            for dz in (-r..r).step_by(11) {
                for dx in (-r..r).step_by(11) {
                    let (x, z) = (s.center.0 + dx, s.center.1 + dz);
                    if !matches!(natural_tile(body, x, z), Tile::Water | Tile::WaterDeep) {
                        continue;
                    }
                    revisadas += 1;
                    if civil_at(body, x, z).is_some() {
                        sobre_agua += 1;
                    }
                }
            }
        }
        assert!(revisadas > 500, "ninguna ciudad da al agua: sólo {revisadas} celdas");
        assert_eq!(
            sobre_agua, 0,
            "{sobre_agua} de {revisadas} celdas de agua dentro de un casco urbano con obra civil encima"
        );
    }

    /// **TODA CARRETERA DESEMBOCA EN ALGO.** El enlace interurbano llega al borde
    /// del casco y se acaba; si ahí no hay nada, la carretera muere contra el
    /// jardín de alguien —que es lo que pasaba salvo que su punto de entrada
    /// cayera justo sobre una calle de la retícula, o sea casi nunca—. La
    /// circunvalación es lo que recoge a todas.
    #[test]
    fn el_casco_tiene_circunvalacion() {
        let body = CelestialBody::Earth;
        let s = una_metropoli();
        let (cx, cz) = s.center;
        let r = s.radius;
        let mut vuelta = 0;
        let mut huecos = 0;
        // Se recorre el perímetro del casco: en tierra firme tiene que haber
        // calzada en TODA la vuelta.
        for k in (-r..r).step_by(3) {
            for (x, z) in [(cx + k, cz - r), (cx + k, cz + r), (cx - r, cz + k), (cx + r, cz + k)] {
                // En el agua el casco se acaba a propósito: no hay anillo que
                // buscar. Se mide sobre el terreno que se ve.
                if matches!(natural_tile(body, x, z), Tile::Water | Tile::WaterDeep) {
                    continue;
                }
                vuelta += 1;
                if civil_at(body, x, z) != Some(Civil::Road) {
                    huecos += 1;
                }
            }
        }
        assert!(vuelta > 200, "el perímetro del casco casi no toca tierra: {vuelta}");
        assert_eq!(huecos, 0, "{huecos} de {vuelta} celdas del anillo sin calzada");
    }

    #[test]
    fn only_earth_is_civilised_for_now() {
        assert!(has_civilisation(CelestialBody::Earth));
        for &b in CelestialBody::all() {
            if b != CelestialBody::Earth {
                assert!(civil_at(b, 100, 100).is_none(), "{b:?} con carreteras");
            }
        }
    }
}
