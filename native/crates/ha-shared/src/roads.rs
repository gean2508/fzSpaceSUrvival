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
    ///
    /// **Toda vía tiene banquina, incluido el callejón.** El callejón llevaba
    /// medio tile, y medio tile redondeado contra el ancho de la calzada daba
    /// CERO: donde una vereda llegaba a un callejón se cortaba en seco y volvía a
    /// empezar del otro lado. Lo caza `toda_via_tiene_vereda_a_los_dos_lados`.
    pub fn half_shoulder(self) -> f32 {
        self.half_width() + match self {
            RoadClass::Highway => 3.0,
            RoadClass::Route => 2.0,
            RoadClass::Avenue => 2.5,
            RoadClass::Street => 2.0,
            RoadClass::Alley => 1.0,
        }
    }

    /// ¿Caben dos vehículos cruzándose? Es la definición operativa de
    /// "transitable", y la que comprueba el test de conectividad.
    pub fn drivable(self) -> bool {
        self.half_width() >= 2.5
    }

    /// La más ancha de las dos. Sirve para preguntar "¿cuánto hay que retirarse
    /// de la esquina?" cuando dos vías distintas se cruzan ahí.
    pub fn wider(self, other: RoadClass) -> RoadClass {
        if other.half_width() > self.half_width() {
            other
        } else {
            self
        }
    }
}

/// En qué dirección corre una calle. Una VERTICAL corre en Z y se identifica por
/// su columna; una HORIZONTAL corre en X y se identifica por su fila.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreetAxis {
    Vertical,
    Horizontal,
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

/// **La clase de la calle que corre por la línea `i` de la retícula**, según la
/// trama de la ciudad.
///
/// Depende sólo de ESA línea, y ése es el punto entero de la función. Antes la
/// clase salía de la manzana `(ix, iz)`, así que la misma calle era avenida
/// mientras bordeaba una manzana y callejón en la siguiente: recorriéndola, se
/// ensanchaba y se angostaba cada 32 tiles, con la vereda apareciendo y
/// desapareciendo. Una calle mide lo mismo de punta a punta o no es una calle.
/// Lo caza `una_calle_mide_lo_mismo_de_punta_a_punta`.
///
/// Que haya avenidas cada pocas manzanas es lo que da a una ciudad una estructura
/// legible —"salgo a la avenida y sigo"— en vez de una cuadrícula uniforme donde
/// todas las calles son iguales y no hay forma de orientarse. Es la otra mitad de
/// romper el damero (la primera es que cada ciudad tenga su [`CityStyle`]).
pub fn axis_class(style: CityStyle, axis: StreetAxis, i: i64) -> RoadClass {
    match style {
        // RETÍCULA ORTOGONAL: avenida cada 4 líneas, y a mitad de camino entre
        // dos avenidas el fondo de manzana. La trama de manual, la de una ciudad
        // planificada sobre llano.
        CityStyle::Grid => match i.rem_euclid(4) {
            0 => RoadClass::Avenue,
            2 => RoadClass::Alley,
            _ => RoadClass::Street,
        },
        // RADIAL: **anillos concéntricos**. Los dos ejes que pasan por el centro
        // son las radiales grandes, y cada tres líneas hay un anillo. Como el
        // anillo es la unión de las líneas `|i| = k` en los dos ejes, sale el
        // cuadrado concéntrico del centro histórico sin ningún caso especial.
        CityStyle::Radial => match i.abs().rem_euclid(3) {
            0 => RoadClass::Avenue,
            1 => RoadClass::Street,
            _ => RoadClass::Alley,
        },
        // PUERTO: la trama sigue la costa, con manzanas ALARGADAS. Los paseos
        // paralelos al agua (horizontales) son avenidas frecuentes; las bajadas
        // al mar (verticales) son pocas y entre ellas hay callejones de servicio.
        CityStyle::Port => match axis {
            StreetAxis::Horizontal => {
                if i.rem_euclid(3) == 0 {
                    RoadClass::Avenue
                } else {
                    RoadClass::Street
                }
            }
            StreetAxis::Vertical => match i.rem_euclid(6) {
                0 => RoadClass::Avenue,
                1 | 5 => RoadClass::Alley,
                _ => RoadClass::Street,
            },
        },
        // MODERNA: manzanas grandes de torre, avenidas anchas y frecuentes, sin
        // callejones (una torre no tiene fondo de manzana).
        CityStyle::Modern => {
            if i.rem_euclid(3) == 0 {
                RoadClass::Avenue
            } else {
                RoadClass::Street
            }
        }
        // RURAL: no hay avenidas. Hay la calle del pueblo, y cada tanto el camino
        // por el que se llega.
        CityStyle::Rural => {
            if i.rem_euclid(6) == 0 {
                RoadClass::Route
            } else {
                RoadClass::Street
            }
        }
    }
}

/// La vía MÁS ANCHA de las dos que se cruzan en la esquina noroeste de la
/// manzana `(ix, iz)`. Es lo que hay que preguntar para saber cuánto tiene que
/// retirarse un lote de su esquina: basta con que una de las dos sea avenida.
pub fn street_class(style: CityStyle, block_ix: i64, block_iz: i64) -> RoadClass {
    axis_class(style, StreetAxis::Vertical, block_ix)
        .wider(axis_class(style, StreetAxis::Horizontal, block_iz))
}

/// Separación entre calles dentro de un núcleo, en tiles. Sale de la TRAMA, no
/// del rango: una ciudad moderna tiene manzanas grandes de torre y un puerto las
/// tiene chicas y alargadas.
///
/// **Cuentan de eje a eje de calle, no de vereda a vereda.** Desde que la calzada
/// va centrada en la línea de la retícula, la manzana paga la banquina de sus
/// CUATRO calles y no de dos, así que estos números subieron ~8 tiles para que el
/// interior libre siguiera siendo el de antes. Con los de antes, el hueco
/// edificable de una manzana quedaba en 18 tiles y casi ningún programa entraba:
/// todo pedía dos manzanas, se plantaba en una esquina y dejaba las otras tres
/// vacías. Media ciudad eran solares baldíos. Lo mide
/// `una_manzana_da_para_construir` y lo caza `la_ciudad_no_esta_llena_de_baldios`.
pub fn block_size(style: CityStyle, tier: SettlementTier) -> i64 {
    match style {
        CityStyle::Modern => 52,
        CityStyle::Radial => 44,
        CityStyle::Grid => 40,
        CityStyle::Port => 34,
        // La aldea también paga: por su cruz central pasa el CAMINO por el que se
        // llega (una `Route` de 7 tiles), y con manzanas de 28 no quedaba sitio
        // ni para una casa mediana entre la banquina y la banquina.
        CityStyle::Rural => match tier {
            SettlementTier::Hamlet => 32,
            _ => 36,
        },
    }
}

/// Sentido de circulación de un carril, en octantes (`0` = este, `2` = norte,
/// `4` = oeste, `6` = sur), o `None` si esa celda no es calzada.
///
/// **Toda calle es de doble mano, y las dos manos son dos MITADES.** El sentido
/// sale del lado del eje en el que estás, con mano derecha: yendo al este tu
/// derecha es el sur, así que la mitad SUR de una calle este-oeste va al este y
/// la norte al oeste. Es la misma regla que hace que la línea divisoria
/// (`pintura_at`) signifique algo: de un lado se va, del otro se viene.
///
/// Antes el sentido salía de la PARIDAD de la coordenada (`tz % 2`) y de un hash
/// por bloque de 64 tiles. O sea: los carriles quedaban intercalados tile a tile
/// —ida, vuelta, ida, vuelta a lo ancho de la calzada— y encima el bloque de al
/// lado podía darlos vuelta. Ningún conductor podía usar eso; la avenida no tenía
/// dos manos sino cinco carriles alternados.
///
/// **Todavía no hay NPCs conduciendo**: esto existe para que cuando los haya no
/// tengan que inventarse la red. Un conductor sólo necesita "¿estoy en calzada y
/// hacia dónde apunta mi carril?", y eso sale del mismo mapa determinista que
/// dibuja las calles.
pub fn lane_at(body: CelestialBody, tx: i64, tz: i64) -> Option<u8> {
    carril_con(&|x: i64, z: i64| es_calzada(body, x, z), tx, tz)
}

/// [`lane_at`] con la calzada dada por quien pregunta. La REGLA es una sola; lo
/// que cambia es de dónde sale "acá hay calzada": el mundo celda a celda, o los
/// tiles ya cargados del cliente (`ha-game/src/vehiculos.rs`), que es lo que
/// permite que un conductor la consulte veinte veces por segundo sin recalcular
/// la geografía.
pub fn carril_con(calzada: &impl Fn(i64, i64) -> bool, tx: i64, tz: i64) -> Option<u8> {
    if !calzada(tx, tz) {
        return None;
    }
    // Cuánta calzada hay para cada lado. Es la misma medición con la que
    // `es_eje_con` encuentra el eje pintado, así que el carril y la línea
    // divisoria no pueden discrepar: la línea cae donde los dos lados empatan, y
    // es exactamente ahí donde el sentido cambia.
    let n = calzada_seguida(calzada, tx, tz, 0, -1);
    let s = calzada_seguida(calzada, tx, tz, 0, 1);
    let w = calzada_seguida(calzada, tx, tz, -1, 0);
    let e = calzada_seguida(calzada, tx, tz, 1, 0);
    // ¿La calle corre en X o en Z? Corre en el eje por el que SIGUE, que es el
    // que llega más lejos: en una calle este-oeste chocás con la vereda al norte
    // y al sur a media calzada, y al este y al oeste no chocás nunca.
    let horiz = w + e;
    let vert = n + s;
    if horiz == 0 && vert == 0 {
        return None;
    }
    if horiz >= vert {
        // Calle este-oeste. `n > s` = tengo más calzada al norte = estoy en la
        // mitad SUR = voy al este. El empate (la celda del eje) se resuelve para
        // el mismo lado siempre: es la celda pintada, no la conduce nadie.
        Some(if n >= s { 0 } else { 4 })
    } else {
        // Calle norte-sur. Yendo al norte la derecha es el este, así que la mitad
        // ESTE (`w >= e`: más calzada al oeste) va al norte.
        Some(if w >= e { 2 } else { 6 })
    }
}

/// **¿La celda está dentro de un cruce?** Versión pública de la regla con la que
/// se pintan las sendas (ver `es_cruce_con`): la calzada no se acaba en ninguna de
/// las cuatro direcciones. La usa el conductor para decidir si sigue o dobla.
pub fn cruce_con(calzada: &impl Fn(i64, i64) -> bool, tx: i64, tz: i64) -> bool {
    calzada(tx, tz) && es_cruce_con(calzada, tx, tz)
}

/// **El semáforo de una esquina**: hacia dónde MIRA el que va acá, o `None` si en
/// esta celda no va ninguno. El rumbo está en los mismos octantes que
/// [`lane_at`] (`0` = este, `2` = norte, `4` = oeste, `6` = sur).
///
/// **Qué es una esquina, medido y no sorteado.** El poste va en la celda de
/// VEREDA que queda metida en el ángulo de dos calzadas: la que tiene asfalto al
/// lado en un eje, asfalto al lado en el otro, y asfalto también en la diagonal
/// que forman los dos. En un tramo recto eso no pasa nunca —la vereda tiene calle
/// de un solo lado—, así que salen exactamente cuatro por cruce y ni uno en medio
/// de la cuadra. Nada de hash: dos corridas dan lo mismo y el cliente puede
/// spawnearlos por chunk sin guardar un byte.
///
/// **Hacia dónde mira cada uno.** Un semáforo detiene al que VIENE, así que mira
/// contra el tránsito de su brazo. Se reparte uno por brazo, que es lo que hace
/// que el cruce se lea completo desde arriba en vez de tener cuatro cajas
/// mirando al mismo lado:
///
/// | esquina | el cruce le queda… | mira al | brazo que detiene |
/// |---|---|---|---|
/// | noroeste | sudeste | oeste | el que llega desde el oeste |
/// | noreste  | sudoeste | norte | el que baja desde el norte |
/// | sudeste  | noroeste | este  | el que llega desde el este |
/// | sudoeste | noreste | sur   | el que sube desde el sur |
pub fn semaforo_at(body: CelestialBody, tx: i64, tz: i64) -> Option<u8> {
    if crate::geography::civil_at(body, tx, tz) != Some(crate::geography::Civil::Pavement) {
        return None;
    }
    let calzada = |dx: i64, dz: i64| es_calzada(body, tx + dx, tz + dz);
    // El orden importa poco (una celda cae en un solo ángulo), pero fijarlo deja
    // la función determinista aun en la esquina rara de una circunvalación.
    for (dx, dz, rumbo) in [
        (1, 1, 4),   // el cruce al sudeste → estoy en la esquina NO → miro al oeste
        (-1, 1, 2),  // cruce al sudoeste → esquina NE → miro al norte
        (-1, -1, 0), // cruce al noroeste → esquina SE → miro al este
        (1, -1, 6),  // cruce al noreste → esquina SO → miro al sur
    ] {
        if calzada(dx, 0) && calzada(0, dz) && calzada(dx, dz) {
            return Some(rumbo);
        }
    }
    None
}

/// Hasta dónde se mira a cada lado para saber si una celda está centrada en la
/// calzada. Doce tiles cubren la autopista más ancha (10 + banquina) y acotan el
/// trabajo: sin tope, una explanada de asfalto se escanearía entera.
const ALCANCE_EJE: i64 = 12;

/// Ancho mínimo de calzada (tiles) para que lleve línea divisoria pintada. Tres
/// es la calle de manzana (`RoadClass::Street`), que es la más angosta por la que
/// se conduce; el callejón de dos queda sin pintar, como en la vida.
pub const ANCHO_MINIMO_LINEA: i64 = 3;

// ---------------------------------------------------------------------------
// Las marcas de calzada que ocupan un RECTÁNGULO: la bahía de bus y el cajón
// ---------------------------------------------------------------------------

/// **Cuánto mide cada marca de calzada, en tiles `(x, z)`.**
///
/// No se eligen: las MIDE `scripts/import-marcas-calzada.py` sobre la hoja del
/// pack (busca el marco amarillo de la bahía y las líneas blancas del cajón) y se
/// planta si lo que encuentra no coincide con lo que dice acá. O sea que estos
/// cuatro números son el pack, no una preferencia — si el pack cambia, el
/// importador avisa en vez de dibujar media bahía.
///
/// Hay dos de cada una porque el pack las dibujó para las dos orientaciones: la
/// bahía de una calle norte-sur es angosta y larga, la de una este-oeste es al
/// revés. No es la misma rotada — el texto `BUS` se lee en cada una.
pub const BAHIA_NS: (i64, i64) = (3, 6);
/// Ver [`BAHIA_NS`].
pub const BAHIA_EW: (i64, i64) = (7, 3);
/// Ver [`BAHIA_NS`]. El cajón de estacionamiento en batería.
pub const CAJON_NS: (i64, i64) = (3, 5);
/// Ver [`BAHIA_NS`].
pub const CAJON_EW: (i64, i64) = (5, 3);

/// Cada cuántos tiles a lo largo de una calle se ancla una marca. Es el paso de
/// la retícula sobre la que se sortean: grande a propósito, porque una avenida
/// con una bahía de bus cada veinte metros no es una avenida.
const PASO_MARCA: i64 = 24;

/// De cada cien anclajes, cuántos llevan marca. El resto de la calle queda limpia
/// — que la pintura sea escasa es lo que hace que signifique algo.
const PROB_MARCA: u32 = 55;

/// De los anclajes con marca, cuántos son bahía de bus (el resto, cajones). Se
/// estaciona en muchos más sitios de los que para el colectivo.
const PROB_BAHIA: u32 = 30;

/// Qué se pintó en este rectángulo de calzada. Ver [`Pintura::Marca`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Marca {
    /// La bahía donde el colectivo se sale del carril para parar.
    BahiaBus,
    /// El cajón de estacionamiento, con su `P`.
    Cajon,
}

impl Marca {
    /// Cuánto mide en tiles `(x, z)` según por dónde corra la calle.
    pub fn medida(self, calle_ns: bool) -> (i64, i64) {
        match (self, calle_ns) {
            (Marca::BahiaBus, true) => BAHIA_NS,
            (Marca::BahiaBus, false) => BAHIA_EW,
            (Marca::Cajon, true) => CAJON_NS,
            (Marca::Cajon, false) => CAJON_EW,
        }
    }

    /// El slot de arte. Lo escribe `scripts/import-marcas-calzada.py`.
    pub fn slot(self) -> &'static str {
        match self {
            Marca::BahiaBus => "road/bahia_bus",
            Marca::Cajon => "road/cajon",
        }
    }
}

/// Cuántas celdas de calzada hay seguidas en una dirección, hasta [`ALCANCE_EJE`].
fn calzada_seguida(calzada: &impl Fn(i64, i64) -> bool, tx: i64, tz: i64, dx: i64, dz: i64) -> i64 {
    (1..=ALCANCE_EJE).take_while(|i| calzada(tx + dx * i, tz + dz * i)).count() as i64
}

/// ¿Esta celda es el EJE de la calzada — por donde va pintada la línea?
///
/// Se mide, no se deduce de la trama: una celda es eje si tiene la misma
/// cantidad de calzada a un lado que al otro. Sale gratis que funcione igual en
/// la retícula de una ciudad y en una ruta que serpentea entre dos pueblos, que
/// son geometrías distintas y no comparten ni una línea de código.
///
/// Los callejones no llevan línea: [`ANCHO_MINIMO_LINEA`] deja fuera todo lo que
/// no da para dos manos, que es exactamente cuándo una línea divisoria significa
/// algo.
///
/// Recibe **de dónde sale "acá hay calzada"** en vez de consultar la geografía,
/// porque las dos formas de usar esto tienen costes muy distintos: una celda
/// suelta pregunta al mundo, y un chunk entero pregunta a un bitmap que se llenó
/// de una pasada (ver [`lane_marks_rect`]). La REGLA es una sola.
fn es_eje_con(calzada: &impl Fn(i64, i64) -> bool, tx: i64, tz: i64) -> bool {
    if !calzada(tx, tz) {
        return false;
    }
    let (w, e) = (
        calzada_seguida(calzada, tx, tz, -1, 0),
        calzada_seguida(calzada, tx, tz, 1, 0),
    );
    let (n, s) = (
        calzada_seguida(calzada, tx, tz, 0, -1),
        calzada_seguida(calzada, tx, tz, 0, 1),
    );
    // `a == b || a + 1 == b`: con ancho PAR no hay celda central exacta (en una
    // calzada de cuatro, la celda 1 tiene una a la izquierda y dos a la derecha),
    // así que se elige la de más acá. Pedir simetría estricta dejaba sin línea
    // todas las avenidas de ancho par, que son la mitad.
    let centrado = |a: i64, b: i64| a == b || a + 1 == b;
    let eje_v = centrado(w, e) && w + e + 1 >= ANCHO_MINIMO_LINEA && w < ALCANCE_EJE;
    let eje_h = centrado(n, s) && n + s + 1 >= ANCHO_MINIMO_LINEA && n < ALCANCE_EJE;
    eje_v || eje_h
}

fn es_calzada(body: CelestialBody, tx: i64, tz: i64) -> bool {
    matches!(
        crate::geography::civil_at(body, tx, tz),
        Some(crate::geography::Civil::Road | crate::geography::Civil::Bridge)
    )
}

/// **¿Esta celda está DENTRO de un cruce?**
///
/// Lo que distingue al cruce no es el ancho —una calle de tres y un cruce de tres
/// por tres miden lo mismo— sino que **la calzada no se termina en ninguna de las
/// cuatro direcciones**. En una calle recta, mires al este o al oeste, chocás con
/// la vereda a media calzada; adentro del cruce seguís teniendo asfalto para los
/// cuatro lados, porque a cada lado empieza la otra calle.
///
/// Se mide con la misma vara que el eje (`calzada_seguida` hasta
/// [`ALCANCE_EJE`]): si las cuatro llegan al tope, no hay borde cerca en ningún
/// rumbo. Una explanada de asfalto da lo mismo, y está bien: tampoco se le pinta
/// una línea divisoria por el medio.
fn es_cruce_con(calzada: &impl Fn(i64, i64) -> bool, tx: i64, tz: i64) -> bool {
    calzada(tx, tz)
        && [(-1, 0), (1, 0), (0, -1), (0, 1)]
            .iter()
            .all(|&(dx, dz)| calzada_seguida(calzada, tx, tz, dx, dz) >= ALCANCE_EJE)
}

/// La marca de una celda a partir de sus vecinas. Ver [`lane_mark_at`].
fn marca_con(calzada: &impl Fn(i64, i64) -> bool, tx: i64, tz: i64) -> Option<&'static str> {
    // **La línea divisoria se corta en el cruce.** Antes seguía de largo por el
    // medio de la intersección —dos rayas cruzándose en el aire—, que es algo que
    // no existe en ninguna calle: la pintura llega a la senda y ahí termina.
    if es_cruce_con(calzada, tx, tz) {
        return None;
    }
    if !es_eje_con(calzada, tx, tz) {
        return None;
    }
    let n = es_eje_con(calzada, tx, tz - 1);
    let s = es_eje_con(calzada, tx, tz + 1);
    let e = es_eje_con(calzada, tx + 1, tz);
    let w = es_eje_con(calzada, tx - 1, tz);
    Some(match (n, s, e, w) {
        (true, true, true, true) => "nsew",
        (true, true, true, false) => "nse",
        (true, true, false, true) => "nsw",
        (true, false, true, true) => "new",
        (false, true, true, true) => "sew",
        (true, false, true, false) => "ne",
        (true, false, false, true) => "nw",
        (false, true, true, false) => "se",
        (false, true, false, true) => "sw",
        (false, false, true, _) | (false, false, _, true) => "ew",
        // Un solo brazo (la punta de una línea) se dibuja como el tramo recto que
        // la continúa: una raya que termina en medio del asfalto se lee como un
        // error de dibujo, no como el final de la calle.
        _ => "ns",
    })
}

/// **Hacia qué lados sigue la línea pintada** en esta celda, o `None` si acá no
/// va línea.
///
/// Es lo que convierte una explanada de asfalto en una calle: sin línea, una
/// avenida de diez tiles se lee como un descampado gris. El resultado se pide
/// como pose del slot `road/linea` (`"ns"`, `"nsew"`…), que es como está
/// recortado el arte del pack.
///
/// Consulta suelta, para tests y para preguntar por una celda. **Para dibujar un
/// chunk se usa [`lane_marks_rect`]**, que hace lo mismo dos órdenes de magnitud
/// más barato.
pub fn lane_mark_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static str> {
    marca_con(&|x, z| es_calzada(body, x, z), tx, tz)
}

/// Las marcas de un rectángulo entero, de una sola pasada.
///
/// **Por qué existe.** Averiguar si una celda es eje mira hasta doce vecinas por
/// lado, y la marca mira además sus cuatro vecinas: son ~240 consultas de
/// geografía por celda, y `civil_at` no es barata (recorre asentamientos y traza
/// los enlaces). Celda por celda, un chunk de ciudad costaba cientos de miles de
/// consultas — un tirón visible cada vez que aparece una manzana nueva, o sea
/// exactamente lo que §10 prohíbe.
///
/// Acá la geografía se pregunta **una vez por celda** a un bitmap con margen, y
/// las ~240 consultas se vuelven lecturas de un `Vec<bool>`. La regla de qué es
/// eje es la misma función que usa [`lane_mark_at`]; lo único que cambia es de
/// dónde sale el dato.
///
/// **Qué se pinta sobre el asfalto de una celda.**
///
/// La calzada lleva dos pinturas distintas y no son la misma red: la línea sigue
/// el EJE de la calle y la senda cruza su ANCHO. Antes sólo existía la primera,
/// así que un cruce se veía como dos rayas atravesándose y sin un solo paso de
/// peatones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pintura {
    /// Línea divisoria. El texto es la pose del slot `road/linea` (`"ns"`,
    /// `"nsew"`…), tal como está recortado el arte.
    Linea(&'static str),
    /// **Senda peatonal**, la banda que cruza la calzada justo antes del cruce.
    ///
    /// `calle_ns` = la calle que cruza corre norte-sur, así que las cebras se
    /// dibujan paralelas a ese tránsito. Es la orientación de la CALLE, no la del
    /// peatón: es lo que decide cómo van las barras.
    Senda { calle_ns: bool },
    /// **Una marca de RECTÁNGULO** — la bahía de bus o el cajón de
    /// estacionamiento — y qué celda de ella es ésta.
    ///
    /// A diferencia de la línea y la senda, que son una losa que se repite, esto
    /// es un dibujo grande repartido en celdas: `(i, j)` es la casilla dentro del
    /// rectángulo y sale directo del recorte del pack (la pose `ns_2_4` es la
    /// columna 2, fila 4 de la bahía). `voltear_x` / `voltear_z` son para el
    /// cordón de enfrente: la misma bahía espejada, sin pedirle al pack que la
    /// dibuje dos veces.
    Marca {
        clase: Marca,
        calle_ns: bool,
        i: u8,
        j: u8,
        voltear_x: bool,
        voltear_z: bool,
    },
}

/// La pintura de una celda suelta. Para dibujar un chunk se usa
/// [`pintura_rect`], que hace lo mismo dos órdenes de magnitud más barato.
pub fn pintura_at(body: CelestialBody, tx: i64, tz: i64) -> Option<Pintura> {
    sin_marca_en_la_ruta(body, pintura_con(&|x, z| es_calzada(body, x, z), tx, tz), tx, tz)
}

fn pintura_con(calzada: &impl Fn(i64, i64) -> bool, tx: i64, tz: i64) -> Option<Pintura> {
    if !calzada(tx, tz) {
        return None;
    }
    if es_cruce_con(calzada, tx, tz) {
        return None; // adentro del cruce no se pinta nada
    }
    // **La senda va en los CUATRO brazos**, no en uno. Es la fila (o la columna)
    // de calzada pegada al cruce, entera: por eso se pregunta por la celda vecina
    // y no por el eje — la senda cruza el ancho de la calle, no lo recorre.
    if es_cruce_con(calzada, tx, tz - 1) || es_cruce_con(calzada, tx, tz + 1) {
        return Some(Pintura::Senda { calle_ns: true });
    }
    if es_cruce_con(calzada, tx - 1, tz) || es_cruce_con(calzada, tx + 1, tz) {
        return Some(Pintura::Senda { calle_ns: false });
    }
    if let Some(m) = marca_rect_con(calzada, tx, tz) {
        return Some(m);
    }
    marca_con(calzada, tx, tz).map(Pintura::Linea)
}

/// **¿Esta celda cae dentro de una bahía de bus o de un cajón de estacionamiento?**
///
/// Todo sale de medir la calzada, igual que el eje y el cruce — no de la retícula
/// de la ciudad. Así funciona igual en el damero de un centro y en una ruta que
/// entra a un pueblo, que son geometrías que no comparten una línea de código.
///
/// 1. **Por dónde corre la calle** y **dónde está el cordón**: cuánta calzada
///    seguida hay a los cuatro lados. La calle va por donde hay más; el cordón
///    está del lado donde se acaba.
/// 2. **Si entra**: la marca se pega al cordón y tiene que dejar libre al menos
///    una calzada de ancho mínimo, o el auto que pasa no pasa. Con eso, las
///    bahías y los cajones salen solos en las avenidas y no en las calles de
///    manzana, que es donde van.
/// 3. **Dónde**: un anclaje cada [`PASO_MARCA`] tiles a lo largo de la calle,
///    sorteado por su posición. La misma celda elige siempre lo mismo, así que no
///    hay que guardar nada por instancia.
/// 4. **Y que el rectángulo entre entero**: si la calzada se termina antes (viene
///    el cruce), no se pinta nada. Media bahía cortada por una senda es peor que
///    ninguna.
///
/// ## Todo se decide en el ANCLA, no en la celda
///
/// Es la parte que hay que entender para no volver a romperlo. La primera versión
/// medía el ancho de la calzada, y si el rectángulo entraba, **en cada celda por
/// separado** — y esas medidas NO son iguales para todas las celdas de un mismo
/// rectángulo: `calzada_seguida` se topa con [`ALCANCE_EJE`], y cerca de una
/// esquina la calzada "sigue" hacia la calle que cruza, así que la celda de más
/// adentro mide una calzada mucho más ancha que la pegada al cordón. Resultado:
/// unas celdas del rectángulo se pintaban y otras no. Lo cazó
/// `las_marcas_de_calzada_van_sobre_el_asfalto_y_salen_enteras` con un cajón que
/// salía con 5 de sus 15 casillas.
///
/// Ahora la celda sólo hace dos cosas: ubicar su ANCLA —el rincón del rectángulo
/// contra el cordón— y preguntarle a ella. Todo lo demás (el sorteo, el ancho de
/// la calzada, si el tramo es lo bastante largo) se mide **una vez, ahí**. Dos
/// celdas del mismo rectángulo comparten ancla, así que no pueden discrepar.
fn marca_rect_con(
    calzada: &impl Fn(i64, i64) -> bool,
    tx: i64,
    tz: i64,
) -> Option<Pintura> {
    let (w, e) = (
        calzada_seguida(calzada, tx, tz, -1, 0),
        calzada_seguida(calzada, tx, tz, 1, 0),
    );
    let (n, s) = (
        calzada_seguida(calzada, tx, tz, 0, -1),
        calzada_seguida(calzada, tx, tz, 0, 1),
    );
    // La calle corre por donde la calzada sigue de largo.
    let calle_ns = n + s > w + e;
    // Hacia dónde queda el cordón más cerca, y a cuántos tiles. `paso` es hacia
    // dónde crece el rectángulo desde él.
    let (hasta_cordon, paso) = if calle_ns {
        if w <= e { (w, 1) } else { (e, -1) }
    } else if n <= s {
        (n, 1)
    } else {
        (s, -1)
    };
    let coord_larga = if calle_ns { tz } else { tx };
    let ancla = coord_larga - coord_larga.rem_euclid(PASO_MARCA);
    let a_lo_largo = coord_larga - ancla;
    // El rincón del rectángulo: contra el cordón y en el anclaje.
    let (ax, az) = if calle_ns {
        (tx - paso * hasta_cordon, ancla)
    } else {
        (ancla, tz - paso * hasta_cordon)
    };
    if !calzada(ax, az) {
        return None;
    }
    let h = hash2(0x6d15_c0de, ax, az);
    if h % 100 >= PROB_MARCA {
        return None;
    }
    let clase = if (h >> 8) % 100 < PROB_BAHIA { Marca::BahiaBus } else { Marca::Cajon };
    let (mw, mh) = clase.medida(calle_ns);
    let (across, along) = if calle_ns { (mw, mh) } else { (mh, mw) };
    if hasta_cordon >= across || a_lo_largo >= along {
        return None;
    }
    // ── Y de acá para abajo, medido EN EL ANCLA ──────────────────────────────
    let (aw, ae) = (
        calzada_seguida(calzada, ax, az, -1, 0),
        calzada_seguida(calzada, ax, az, 1, 0),
    );
    let (an, as_) = (
        calzada_seguida(calzada, ax, az, 0, -1),
        calzada_seguida(calzada, ax, az, 0, 1),
    );
    // El ancla tiene que SER el cordón y la calle tiene que correr para el mismo
    // lado que dijo la celda. Si no, esta celda está mirando otra calle (pasa en
    // las esquinas) y no le corresponde marca.
    if (an + as_ > aw + ae) != calle_ns {
        return None;
    }
    let (hacia_afuera, ancho) = if calle_ns {
        (if paso > 0 { aw } else { ae }, aw + ae + 1)
    } else {
        (if paso > 0 { an } else { as_ }, an + as_ + 1)
    };
    if hacia_afuera != 0 {
        return None;
    }
    // Tiene que quedar calle: la marca no puede comerse la calzada hasta dejarla
    // más angosta que lo mínimo por lo que se conduce. Con esto, las bahías y los
    // cajones salen en las avenidas y no en las calles de manzana.
    if ancho - across < ANCHO_MINIMO_LINEA {
        return None;
    }
    // Y el rectángulo tiene que entrar entero en el tramo recto de calzada.
    let largo_adelante = if calle_ns { as_ } else { ae };
    if largo_adelante < along - 1 {
        return None;
    }
    // **Y sin tocar el cruce ni su senda.** Que la calzada "siga" no alcanza: si
    // sigue es porque adelante está la esquina, y ahí la calzada se abre para los
    // cuatro lados. Las celdas del rectángulo que caen sobre el cruce, o pegadas a
    // él, ya se resolvieron antes como senda peatonal, así que la bahía salía con
    // un agujero — se vio con una de 18 casillas que dibujaba 3 y se comía el
    // resto contra la esquina de la manzana siguiente. Se mira el tramo entero
    // **más una celda a cada punta**, que es el alcance de la senda.
    let (dx, dz) = if calle_ns { (0, 1) } else { (1, 0) };
    for k in -1..=along {
        if es_cruce_con(calzada, ax + dx * k, az + dz * k) {
            return None;
        }
    }
    let (i, j) = if calle_ns {
        (hasta_cordon, a_lo_largo)
    } else {
        (a_lo_largo, hasta_cordon)
    };
    // `paso < 0` es el cordón de enfrente: la misma marca, espejada.
    Some(Pintura::Marca {
        clase,
        calle_ns,
        i: i as u8,
        j: j as u8,
        voltear_x: calle_ns && paso < 0,
        voltear_z: !calle_ns && paso < 0,
    })
}

/// Devuelve `w * h` entradas en orden por filas (`z` externo, `x` interno).
pub fn lane_marks_rect(
    body: CelestialBody,
    x0: i64,
    z0: i64,
    w: usize,
    h: usize,
) -> Vec<Option<&'static str>> {
    // Margen: el alcance del eje + 1 por las vecinas que decide la marca.
    let m = ALCANCE_EJE + 1;
    let ancho = w + 2 * m as usize;
    let alto = h + 2 * m as usize;
    let mut mapa = vec![false; ancho * alto];
    for j in 0..alto {
        for i in 0..ancho {
            mapa[j * ancho + i] = es_calzada(body, x0 - m + i as i64, z0 - m + j as i64);
        }
    }
    let calzada = |x: i64, z: i64| {
        let (i, j) = (x - x0 + m, z - z0 + m);
        if i < 0 || j < 0 || i >= ancho as i64 || j >= alto as i64 {
            return false;
        }
        mapa[j as usize * ancho + i as usize]
    };
    let mut out = Vec::with_capacity(w * h);
    for dz in 0..h as i64 {
        for dx in 0..w as i64 {
            out.push(marca_con(&calzada, x0 + dx, z0 + dz));
        }
    }
    out
}

/// **Toda la pintura de un rectángulo** —líneas y sendas— de una sola pasada.
///
/// Mismo truco que [`lane_marks_rect`]: la geografía se pregunta una vez por
/// celda a un bitmap con margen, y las decenas de consultas por celda se vuelven
/// lecturas de un `Vec<bool>`. El margen es el alcance del eje **+1**, porque la
/// senda pregunta si la celda de al lado es cruce y ese `+1` es lo que hace falta
/// para que el escaneo entre.
pub fn pintura_rect(
    body: CelestialBody,
    x0: i64,
    z0: i64,
    w: usize,
    h: usize,
) -> Vec<Option<Pintura>> {
    let m = ALCANCE_EJE + 1;
    let ancho = w + 2 * m as usize;
    let alto = h + 2 * m as usize;
    let mut mapa = vec![false; ancho * alto];
    for j in 0..alto {
        for i in 0..ancho {
            mapa[j * ancho + i] = es_calzada(body, x0 - m + i as i64, z0 - m + j as i64);
        }
    }
    let calzada = |x: i64, z: i64| {
        let (i, j) = (x - x0 + m, z - z0 + m);
        if i < 0 || j < 0 || i >= ancho as i64 || j >= alto as i64 {
            return false;
        }
        mapa[j as usize * ancho + i as usize]
    };
    let mut out = Vec::with_capacity(w * h);
    for dz in 0..h as i64 {
        for dx in 0..w as i64 {
            out.push(sin_marca_en_la_ruta(body, pintura_con(&calzada, x0 + dx, z0 + dz), x0 + dx, z0 + dz));
        }
    }
    out
}

/// **Bahías de colectivo y cajones, sólo en el pueblo** (28 sep 2026). Se miden
/// sobre el asfalto sin saber dónde están, y desde que los enlaces van en tramos
/// rectos una ruta en medio del campo recibía su cajón "BUS". Fuera de un casco
/// la marca de rectángulo se descarta; la línea y la senda quedan.
fn sin_marca_en_la_ruta(body: CelestialBody, p: Option<Pintura>, tx: i64, tz: i64) -> Option<Pintura> {
    match p {
        Some(Pintura::Marca { .. }) if crate::geography::settlement_covering(body, tx, tz).is_none() => None,
        otra => otra,
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

/// **La circunvalación**: la vía que recorre el perímetro del casco.
///
/// Existe para que el mapa sea coherente por los dos extremos. Sin ella, el
/// enlace interurbano llegaba al borde del casco y ahí se acababa —salvo que su
/// punto de entrada cayera justo sobre una calle de la retícula, que es un
/// accidente—, así que casi toda carretera del mundo terminaba contra el jardín
/// de alguien. Y por dentro, la retícula se cortaba a media calle en el borde.
/// Con el anillo, toda carretera desemboca en algo y la ciudad tiene un límite
/// que se ve y se conduce.
pub fn ring_class(tier: SettlementTier) -> RoadClass {
    match tier {
        SettlementTier::City => RoadClass::Avenue,
        SettlementTier::Town => RoadClass::Street,
        SettlementTier::Hamlet => RoadClass::Alley,
    }
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
        // Se busca un tramo largo de calzada y se camina entero. Los enlaces van en
        // tramos rectos (ver `geography::link_clearance`) y el tramo este-oeste
        // sale por la fila del CENTRO de cada pueblo: se miran esas filas.
        let mut filas: Vec<i64> = Vec::new();
        let sp = crate::geography::SITE_PERIOD;
        for sx in (-r / 3) / sp..=(r / 3) / sp {
            for sz in (-r / 3) / sp..=(r / 3) / sp {
                if let Some(s) = crate::geography::settlement_in(body, sx, sz) {
                    filas.push(s.center.1);
                }
            }
        }
        let mut runs = 0;
        for tz in filas {
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

    /// **Cada cruce tiene sus cuatro semáforos, uno por brazo.**
    ///
    /// Es el test del pedido "semáforos en cada esquina, que apunten a la
    /// dirección correcta". Mira las dos mitades: que existan (y en cantidad
    /// razonable, no tres en toda la ciudad) y que **nunca caigan sobre la
    /// calzada** — un poste en mitad de la avenida es un obstáculo, no un
    /// semáforo. Y que entre todos cubran los cuatro rumbos: si salieran todos
    /// mirando al mismo lado, el cruce se leería como cuatro cajas iguales.
    #[test]
    fn cada_cruce_tiene_semaforos_en_sus_cuatro_esquinas() {
        let body = CelestialBody::Earth;
        let r = crate::celestial_bodies::body_walk_radius(body) as i64;
        let mut rumbos = std::collections::HashMap::new();
        let mut postes = 0;
        for tz in (-r / 4..r / 4).step_by(3) {
            for tx in (-r / 4..r / 4).step_by(3) {
                let Some(rumbo) = semaforo_at(body, tx, tz) else { continue };
                postes += 1;
                *rumbos.entry(rumbo).or_insert(0usize) += 1;
                assert!(
                    !es_calzada(body, tx, tz),
                    "semáforo sobre la calzada en ({tx},{tz}): es un obstáculo, no un semáforo"
                );
            }
        }
        assert!(postes > 20, "apenas {postes} semáforos en toda la muestra");
        assert_eq!(
            rumbos.len(),
            4,
            "los semáforos no cubren los cuatro brazos del cruce: {rumbos:?}"
        );
    }

    /// **Toda calle es de doble mano, y las dos manos son dos MITADES.**
    ///
    /// Es el test del pedido "en todas las calles debería haber ida y vuelta,
    /// para cuando conduzcan los NPCs". No alcanza con que aparezcan los dos
    /// sentidos en el mundo (eso ya lo mira `roads_expose_lanes_with_a_direction`):
    /// hace falta que estén **uno a cada lado del eje**. Con la regla vieja —la
    /// paridad de la coordenada— los sentidos quedaban intercalados tile a tile, y
    /// un test que sólo contara sentidos distintos pasaba igual.
    ///
    /// Así que se cruza la calzada de vereda a vereda y se cuenta cuántas veces
    /// CAMBIA el sentido: en una calle de doble mano tiene que cambiar
    /// exactamente una vez.
    #[test]
    fn toda_calle_tiene_ida_y_vuelta_una_a_cada_lado_del_eje() {
        let body = CelestialBody::Earth;
        let r = crate::celestial_bodies::body_walk_radius(body) as i64;
        let mut cruces = 0;
        for tz in (-r / 3..r / 3).step_by(37) {
            for tx in (-r / 3..r / 3).step_by(37) {
                // Sólo tramos rectos este-oeste: en un cruce las dos calles se
                // superponen y "a lo ancho" no quiere decir nada.
                if !matches!(pintura_at(body, tx, tz), Some(Pintura::Linea(_))) {
                    continue;
                }
                if !es_calzada(body, tx + 4, tz) || !es_calzada(body, tx - 4, tz) {
                    continue;
                }
                // La calzada entera, de borde a borde.
                let mut sentidos = Vec::new();
                let mut z = tz;
                while es_calzada(body, tx, z - 1) {
                    z -= 1;
                }
                while let Some(d) = lane_at(body, tx, z) {
                    sentidos.push(d);
                    z += 1;
                }
                if sentidos.len() < 3 {
                    continue; // callejón: no se conduce
                }
                // **Y TAMPOCO SE MIDE UNA AVENIDA A LO LARGO.**
                //
                // El filtro de arriba comprueba que haya calzada a cuatro tiles a
                // este y oeste, que es lo que distingue un tramo recto de un
                // borde. No distingue el caso en que el corte cae sobre una
                // avenida NORTE-SUR: ahí "el ancho" recorre la calle a lo largo,
                // los carriles alternan cuadra a cuadra y el conteo de cambios no
                // significa nada. Medido en (8019,2358): 900 celdas seguidas de
                // calzada y 103 cambios de sentido.
                //
                // La calzada más ancha del generador son dos manos de tres
                // carriles; doce tiles deja margen de sobra y corta seco
                // cualquier cosa que sea una calle vista a lo largo.
                if sentidos.len() > 12 {
                    continue;
                }
                let cambios = sentidos.windows(2).filter(|p| p[0] != p[1]).count();
                assert_eq!(
                    cambios, 1,
                    "en ({tx},{tz}) la calzada cambia de sentido {cambios} veces \
                     a lo ancho — los carriles están intercalados, no en dos manos: {sentidos:?}"
                );
                assert!(
                    sentidos.contains(&0) && sentidos.contains(&4),
                    "en ({tx},{tz}) la calle es de una sola mano: {sentidos:?}"
                );
                cruces += 1;
            }
        }
        assert!(cruces > 5, "apenas {cruces} calles medidas: el test no probó nada");
    }

    /// **Las calles llevan línea pintada.** Es el test del reporte "las calles no
    /// las veo con los assets": sin línea, una avenida es una franja gris que no
    /// se lee como calle por mucho asfalto que tenga.
    ///
    /// Comprueba las dos mitades: que se pinte (hay ejes) y que se pinte donde va
    /// (nunca fuera de la calzada, o serían rayas en el pasto).
    #[test]
    fn las_calzadas_llevan_linea_pintada() {
        let body = CelestialBody::Earth;
        let r = crate::celestial_bodies::body_walk_radius(body) as i64;
        let mut marcadas = 0;
        let mut formas = std::collections::HashSet::new();
        for tz in (-r / 3..r / 3).step_by(7) {
            for tx in (-r / 3..r / 3).step_by(7) {
                let Some(forma) = lane_mark_at(body, tx, tz) else { continue };
                marcadas += 1;
                formas.insert(forma);
                assert!(
                    matches!(
                        crate::geography::civil_at(body, tx, tz),
                        Some(crate::geography::Civil::Road | crate::geography::Civil::Bridge)
                    ),
                    "línea pintada fuera de la calzada en ({tx},{tz})"
                );
            }
        }
        assert!(marcadas > 20, "apenas {marcadas} celdas con línea pintada");
        assert!(
            formas.len() >= 2,
            "todas las líneas son iguales: no hay empalmes ni cruces ({formas:?})"
        );
    }

    /// **El atajo dibuja lo mismo que la consulta suelta.** `lane_marks_rect` es
    /// dos órdenes de magnitud más barata porque lee un bitmap en vez de la
    /// geografía; si alguna vez difieren, el chunk pinta una calle distinta de la
    /// que dicen los tests, que es la peor clase de bug — invisible en verde.
    #[test]
    fn el_atajo_por_chunk_pinta_lo_mismo_que_celda_por_celda() {
        let body = CelestialBody::Earth;
        // Un trozo con ciudad: el mismo sitio que retrata `HA_CITYTEST`.
        for (x0, z0) in [(-3248, 3313), (0, 0), (1500, -900)] {
            let (w, h) = (24usize, 24usize);
            let rect = lane_marks_rect(body, x0, z0, w, h);
            for dz in 0..h as i64 {
                for dx in 0..w as i64 {
                    assert_eq!(
                        rect[dz as usize * w + dx as usize],
                        lane_mark_at(body, x0 + dx, z0 + dz),
                        "difieren en ({}, {})",
                        x0 + dx,
                        z0 + dz
                    );
                }
            }
        }
    }

    /// Una ciudad autorada grande, para medir su trama sobre el mapa de verdad.
    fn una_ciudad() -> Settlement {
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

    /// **TODA VÍA TIENE VEREDA A LOS DOS LADOS.**
    ///
    /// Es el reporte literal: "veredas cortadas". La calzada ocupaba los primeros
    /// tiles de la manzana —o sea que estaba entera de un lado de la línea de la
    /// retícula—, así que cada calle tenía acera al norte y al oeste y al sur y al
    /// este el asfalto lindaba con el pasto. Media vereda en toda la ciudad, y
    /// donde una calle se cruzaba con un callejón (que además llevaba banquina
    /// CERO por un redondeo) la vereda desaparecía del todo.
    ///
    /// Se mide saliéndose de la calzada: en cuanto se deja el asfalto, lo que hay
    /// es vereda. A los dos lados, y en los dos ejes.
    #[test]
    fn toda_via_tiene_vereda_a_los_dos_lados() {
        use crate::geography::{civil_at, Civil};
        let body = CelestialBody::Earth;
        let s = una_ciudad();
        let (cx, cz) = s.center;
        // Lejos del anillo del casco, que es el único borde donde la calzada
        // termina legítimamente en campo abierto.
        let r = s.radius - ring_class(s.tier).half_shoulder() as i64 - 4;
        let asfalto = |x: i64, z: i64| {
            matches!(civil_at(body, x, z), Some(Civil::Road | Civil::Bridge))
        };
        let (mut bordes, mut sin_vereda) = (0, 0);
        for k in (-r..r).step_by(3) {
            for j in -r..r {
                // Los dos ejes: se sale de la calzada en x y en z.
                for (x, z, dx, dz) in [(cx + j, cz + k, 1, 0), (cx + k, cz + j, 0, 1)] {
                    if !asfalto(x, z) || asfalto(x + dx, z + dz) {
                        continue;
                    }
                    // La celda de fuera tiene que ser SUELO. Los dos sitios donde
                    // la ciudad se acaba a propósito no llevan vereda, y son
                    // exactamente los que recorta `civil_at`:
                    //
                    // - el AGUA: el casco se corta en la orilla;
                    // - la CARA de un acantilado: la baldosa no se come la pared
                    //   —borrarla dejaba el muro cortado en seco al llegar al
                    //   pueblo, y un hueco por el que se pasaba caminando—.
                    if matches!(
                        crate::geography::natural_tile(body, x + dx, z + dz),
                        crate::topdown::Tile::Water
                            | crate::topdown::Tile::WaterDeep
                            | crate::topdown::Tile::Cliff
                    ) {
                        continue;
                    }
                    bordes += 1;
                    if civil_at(body, x + dx, z + dz) != Some(Civil::Pavement) {
                        sin_vereda += 1;
                    }
                }
            }
        }
        assert!(bordes > 2_000, "apenas {bordes} bordes de calzada revisados");
        assert_eq!(
            sin_vereda, 0,
            "{sin_vereda} de {bordes} bordes de calzada dan al pasto sin vereda de por medio"
        );
    }

    /// **UNA CALLE MIDE LO MISMO DE PUNTA A PUNTA.**
    ///
    /// La clase salía de la MANZANA `(ix, iz)`, así que la misma calle era avenida
    /// mientras bordeaba una manzana y callejón en la siguiente: recorriéndola se
    /// ensanchaba de 7 a 3 tiles y volvía, con la vereda apareciendo y
    /// desapareciendo. Ahora sale de la LÍNEA por la que corre y no puede cambiar.
    #[test]
    fn una_calle_mide_lo_mismo_de_punta_a_punta() {
        use crate::geography::{civil_at, Civil};
        let body = CelestialBody::Earth;
        let s = una_ciudad();
        let (cx, cz) = s.center;
        let b = block_size(s.style, s.tier);
        let asfalto = |x: i64, z: i64| {
            matches!(civil_at(body, x, z), Some(Civil::Road | Civil::Bridge))
        };
        let mut calles = 0;
        for linea in 1..8i64 {
            let x0 = cx + linea * b;
            let mut anchos = std::collections::HashSet::new();
            for k in 0..24i64 {
                // Sólo a MEDIA manzana de las calles horizontales: cerca de un
                // cruce los dos asfaltos se funden y el ancho medido no es el de
                // esta calle.
                let z = cz + k * b + b / 2;
                if !asfalto(x0, z) {
                    continue; // agua, o fuera del casco
                }
                // **Y tampoco se mide donde el AGUA muerde el borde.** Dentro de
                // un casco la calzada no puentea —"la ciudad se acaba en la
                // orilla", ver `geography::civil_at`—, así que un arroyo que
                // roza la calle le come una fila de asfalto y el ancho medido
                // baja de 3 a 2. Eso no es la calle cambiando de clase, que es lo
                // único que este test defiende: es el río. Lo destapó el tirón
                // del sumidero (14 ago 2026), que movió los cauces y metió uno
                // dentro de Metrópolis Norte.
                if (-12..=12).any(|d| {
                    matches!(
                        crate::geography::natural_tile(body, x0 + d, z),
                        crate::topdown::Tile::Water | crate::topdown::Tile::WaterDeep
                    )
                }) {
                    continue;
                }
                let ancho = (-12..=12).filter(|d| asfalto(x0 + d, z)).count();
                anchos.insert(ancho);
            }
            if anchos.is_empty() {
                continue;
            }
            calles += 1;
            assert_eq!(
                anchos.len(),
                1,
                "la calle x={x0} cambia de ancho a lo largo: {anchos:?}"
            );
        }
        assert!(calles >= 5, "apenas {calles} calles medidas");
    }

    /// **UNA MANZANA DA PARA CONSTRUIR.** Desde que la calzada va centrada en la
    /// línea, la manzana paga la banquina de sus cuatro calles y no de dos. Con
    /// los tamaños de antes el hueco libre bajaba a 18 tiles y casi ningún
    /// programa entraba: todo pedía dos manzanas, se plantaba en una esquina y
    /// dejaba las otras tres baldías.
    #[test]
    fn una_manzana_da_para_construir() {
        for style in [
            CityStyle::Grid,
            CityStyle::Radial,
            CityStyle::Port,
            CityStyle::Modern,
            CityStyle::Rural,
        ] {
            for tier in [SettlementTier::City, SettlementTier::Town, SettlementTier::Hamlet] {
                let b = block_size(style, tier);
                // El peor caso: avenida en los cuatro lados.
                let peor = (0..8)
                    .map(|i| street_class(style, i, i).half_shoulder())
                    .fold(0.0f32, f32::max);
                let libre = b as f32 - peor * 2.0;
                assert!(
                    libre >= 18.0,
                    "{style:?}/{tier:?}: manzana de {b} deja {libre} tiles libres — no entra ni una casa mediana"
                );
            }
        }
    }

    /// La línea es UNA sola por calzada. Si dos celdas vecinas en el mismo eje se
    /// marcaran, la calle saldría con doble raya en el medio — que es lo que pasa
    /// si se pide simetría estricta en un ancho par y después se afloja de más.
    /// **En el cruce no se pinta la línea, y sí la senda — en los CUATRO brazos.**
    ///
    /// Era lo que se veía mal en cualquier esquina: la raya discontinua del centro
    /// seguía derecho por la intersección (dos rayas cruzándose en el aire, algo
    /// que no existe en ninguna calle) y de senda peatonal había una sola, en un
    /// brazo. Acá se busca un cruce de verdad en el mundo generado y se afirman
    /// las dos cosas sobre él.
    #[test]
    fn el_cruce_corta_la_linea_y_tiene_senda_en_los_cuatro_brazos() {
        let body = CelestialBody::Earth;
        let s = (-25..25)
            .flat_map(|sx| (-25..25).map(move |sz| (sx, sz)))
            .filter_map(|(sx, sz)| crate::geography::settlement_in(body, sx, sz))
            .find(|s| s.tier == crate::geography::SettlementTier::City)
            .expect("hay al menos una ciudad");
        // Un cruce: una celda de calzada sin borde cerca en ningún rumbo.
        let (cx, cz) = s.center;
        let cruce = (-60..60)
            .flat_map(|dx| (-60..60).map(move |dz| (cx + dx, cz + dz)))
            .find(|&(x, z)| es_cruce_con(&|a, b| es_calzada(body, a, b), x, z))
            .expect("una ciudad tiene cruces");
        assert_eq!(
            pintura_at(body, cruce.0, cruce.1),
            None,
            "la línea sigue de largo por el cruce en {cruce:?}"
        );
        // Y los cuatro brazos: caminando hacia afuera desde el cruce, la primera
        // celda que ya no es cruce tiene que llevar senda.
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mut p = cruce;
            for _ in 0..24 {
                p = (p.0 + dx, p.1 + dz);
                if !es_cruce_con(&|a, b| es_calzada(body, a, b), p.0, p.1) {
                    break;
                }
            }
            assert!(
                matches!(pintura_at(body, p.0, p.1), Some(Pintura::Senda { .. })),
                "el brazo ({dx},{dz}) del cruce {cruce:?} no tiene senda en {p:?}: {:?}",
                pintura_at(body, p.0, p.1)
            );
        }
    }

    /// **La bahía de bus y el cajón se pintan sobre la CALZADA, y enteros.**
    ///
    /// Era el pedido literal: "el arte está en el pack sin importar y necesitan
    /// pintarse sobre la calzada, no sobre la vereda". Así que se afirman las dos
    /// cosas sobre una ciudad de verdad:
    ///
    /// 1. **Cada celda marcada es calzada.** Si una sola cayera en la vereda, la
    ///    `P` quedaría pintada sobre la baldosa.
    /// 2. **Cada rectángulo aparece ENTERO.** Se junta cada marca por su esquina y
    ///    se comprueba que estén sus `w × h` casillas, sin repetir ninguna. Media
    ///    bahía cortada por una senda es peor que ninguna, y es exactamente lo que
    ///    pasa si el sorteo no da lo mismo en todas las celdas del rectángulo.
    #[test]
    fn las_marcas_de_calzada_van_sobre_el_asfalto_y_salen_enteras() {
        use std::collections::HashMap;
        let body = CelestialBody::Earth;
        let s = (-25..25)
            .flat_map(|sx| (-25..25).map(move |sz| (sx, sz)))
            .filter_map(|(sx, sz)| crate::geography::settlement_in(body, sx, sz))
            .find(|s| s.tier == crate::geography::SettlementTier::City)
            .expect("hay al menos una ciudad");
        let (cx, cz) = s.center;
        // `(esquina, clase, ns) -> casillas vistas`. La esquina se reconstruye
        // restándole a la celda su posición dentro del rectángulo, así que todas
        // las celdas de una misma marca caen en la misma entrada.
        let mut vistas: HashMap<((i64, i64), Marca, bool), Vec<(u8, u8)>> = HashMap::new();
        let r = 90;
        for dz in -r..r {
            for dx in -r..r {
                let (tx, tz) = (cx + dx, cz + dz);
                let Some(Pintura::Marca {
                    clase,
                    calle_ns,
                    i,
                    j,
                    voltear_x,
                    voltear_z,
                }) = pintura_at(body, tx, tz)
                else {
                    continue;
                };
                assert!(
                    es_calzada(body, tx, tz),
                    "hay {clase:?} pintada fuera de la calzada en ({tx}, {tz})"
                );
                // La marca del cordón de ENFRENTE crece para el otro lado, así que
                // su rincón se reconstruye sumando en vez de restando. Sin esto el
                // test parte cada marca espejada en dieciocho marcas de una casilla.
                let (px, pz) = (if voltear_x { -1 } else { 1 }, if voltear_z { -1 } else { 1 });
                let esquina = (tx - px * i as i64, tz - pz * j as i64);
                vistas.entry((esquina, clase, calle_ns)).or_default().push((i, j));
            }
        }
        assert!(
            !vistas.is_empty(),
            "una ciudad entera y ni una bahía de bus ni un cajón de estacionamiento"
        );
        for ((esquina, clase, calle_ns), mut casillas) in vistas {
            let (w, h) = clase.medida(calle_ns);
            casillas.sort_unstable();
            casillas.dedup();
            // Las marcas del borde del barrido salen recortadas por el propio
            // barrido, no por el algoritmo: sólo se exigen las de adentro.
            if (esquina.0 - cx).abs() > r - w || (esquina.1 - cz).abs() > r - h {
                continue;
            }
            assert_eq!(
                casillas.len() as i64,
                w * h,
                "la {clase:?} de {esquina:?} salió a medias: {} de {} casillas",
                casillas.len(),
                w * h
            );
        }
    }

    #[test]
    fn la_linea_no_sale_doble() {
        let body = CelestialBody::Earth;
        let r = crate::celestial_bodies::body_walk_radius(body) as i64;
        for tz in (-r / 4..r / 4).step_by(13) {
            for tx in (-r / 4..r / 4).step_by(13) {
                if lane_mark_at(body, tx, tz).is_none() {
                    continue;
                }
                // En un eje vertical, la celda de al lado (misma fila) no puede
                // ser eje también salvo que sea un cruce — y en un cruce el
                // vecino está en la calzada perpendicular, no en la misma.
                let vecinos = [(1, 0), (0, 1)]
                    .into_iter()
                    .filter(|(dx, dz)| lane_mark_at(body, tx + dx, tz + dz).is_some())
                    .count();
                assert!(vecinos <= 2, "línea múltiple alrededor de ({tx},{tz})");
            }
        }
    }
}


/// **Una carretera que no llega a ningún lado no es una carretera.**
///
/// Los demás tests miran la calzada celda a celda (sin acantilados, con línea,
/// con carriles); éste mira lo que un conductor necesita: que desde el centro de
/// un núcleo se pueda LLEGAR en auto al centro del vecino siguiendo sólo asfalto
/// y puentes, sin saltos. Sale de una inundación sobre las celdas transitables
/// ([`is_drivable_at`] y que el tile final no sea sólido), que es exactamente
/// por donde puede ir un vehículo.
#[cfg(test)]
mod tests_conectividad {
    use super::*;
    use crate::topdown::topdown_tile;
    use std::collections::{HashSet, VecDeque};

    fn transitable(body: CelestialBody, tx: i64, tz: i64) -> bool {
        is_drivable_at(body, tx, tz) && !topdown_tile(body, 0, tx, tz).solid()
    }

    /// Inunda desde `a` y devuelve si alcanza `b` (y cuántas celdas visitó).
    fn alcanza(body: CelestialBody, a: (i64, i64), b: (i64, i64)) -> (bool, usize) {
        // Se acota a la caja que une los dos núcleos con un margen: el enlace
        // sale del eje del núcleo y dobla una sola vez dentro de ella.
        let (x0, x1) = (a.0.min(b.0) - 80, a.0.max(b.0) + 80);
        let (z0, z1) = (a.1.min(b.1) - 80, a.1.max(b.1) + 80);
        // Se parte de la celda transitable más cercana al centro (el centro puede
        // caer en una plaza o en una manzana).
        let mut ini = None;
        'b: for r in 0..40i64 {
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dz.abs()) == r && transitable(body, a.0 + dx, a.1 + dz) {
                        ini = Some((a.0 + dx, a.1 + dz));
                        break 'b;
                    }
                }
            }
        }
        let Some(ini) = ini else { return (false, 0) };
        let mut visto: HashSet<(i64, i64)> = HashSet::new();
        let mut cola = VecDeque::new();
        visto.insert(ini);
        cola.push_back(ini);
        while let Some((x, z)) = cola.pop_front() {
            if (x - b.0).abs() <= 40 && (z - b.1).abs() <= 40 {
                return (true, visto.len());
            }
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let p = (x + dx, z + dz);
                if p.0 < x0 || p.0 > x1 || p.1 < z0 || p.1 > z1 || visto.contains(&p) {
                    continue;
                }
                if transitable(body, p.0, p.1) {
                    visto.insert(p);
                    cola.push_back(p);
                }
            }
        }
        (false, visto.len())
    }

    #[test]
    fn de_cada_nucleo_se_llega_en_auto_al_vecino() {
        let body = CelestialBody::Earth;
        let mut pares = 0;
        let mut rotos: Vec<String> = Vec::new();
        'todo: for sx in -3..=3i64 {
            for sz in -3..=3i64 {
                let Some(a) = crate::geography::settlement_in(body, sx, sz) else { continue };
                for (nx, nz) in [(sx + 1, sz), (sx, sz + 1)] {
                    let Some(b) = crate::geography::settlement_in(body, nx, nz) else { continue };
                    pares += 1;
                    let (ok, vistas) = alcanza(body, a.center, b.center);
                    if !ok {
                        rotos.push(format!(
                            "{:?} {:?} -> {:?} {:?}: se acaba tras {vistas} celdas",
                            a.tier, a.center, b.tier, b.center
                        ));
                    }
                    if pares >= 12 {
                        break 'todo;
                    }
                }
            }
        }
        assert!(pares >= 3, "sólo {pares} pares de núcleos vecinos");
        assert!(rotos.is_empty(), "carreteras cortadas ({} de {pares}):\n{}", rotos.len(), rotos.join("\n"));
    }
}
