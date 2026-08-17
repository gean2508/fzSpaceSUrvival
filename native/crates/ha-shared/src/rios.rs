//! **EL RÍO SE TRAZA, NO SE PINTA.**
//!
//! ## Lo que había, y por qué no era un río
//!
//! El cauce salía de un entramado de ruido (`geography::river_here`): una cresta
//! de fBm doblada hacia los lagos. Se veía bien en una captura y **no era un
//! sistema hidrográfico**, y estaba medido (`medir_rios`, Tierra, tres ventanas
//! de 1.200×1.200):
//!
//! | | resultado |
//! |---|---|
//! | cabeceras seguidas aguas abajo | 34 |
//! | de ésas, que llegan al mar o a un lago | **0** |
//! | bajada media siguiendo el cauce | 0,01–0,19 escalones |
//! | lagunas con agua y sin ningún afluente | 16 de 36 |
//!
//! O sea: **todas morían en seco** y ninguna bajaba. Un campo de ruido no tiene
//! aguas abajo; cruza laderas en diagonal porque no sabe qué es una ladera. El
//! comentario que quedó en `geography` tras probar el sesgo por pendiente ya lo
//! decía: *"un cauce que baje de verdad de la cumbre al mar no se saca deformando
//! un campo de ruido, hay que trazarlo"*.
//!
//! Pedido del usuario: *"que surja de por ahí de algún lado alto y de ahí vaya
//! bajando, se bifurque, llegue al lago, llegue al mar"*. Las cuatro cosas son
//! propiedades de un TRAZADO, no de un ruido.
//!
//! ## Cómo se traza sin generar el planeta
//!
//! El mundo se genera por celda y bajo demanda, así que no hay ningún sitio donde
//! "correr la hidrología" de una vez. La salida es que el trazado sea
//! **determinista y local**:
//!
//! 1. El planeta se divide en cuencas de [`CUENCA`] tiles. Cada una sortea una
//!    NACIENTE con un hash — igual que las bocas de cueva o las lagunas.
//! 2. Una naciente sólo vale si está **alto pero no en la cumbre**
//!    ([`NACE_DESDE`]…[`NACE_HASTA`] de la escalera de tierra). Es literal del
//!    pedido: el río no sale del pico, sale de la ladera alta.
//! 3. Desde ahí se baja por **máxima pendiente de [`crate::world_map::platform`]**
//!    —la plantilla autorada del cuerpo: continentes, cordilleras y cuencas— a
//!    zancadas de [`PASO`] tiles, hasta el mar, hasta una laguna o hasta agotar
//!    [`PASOS_MAX`].
//! 4. En un punto sorteado del recorrido el cauce **se bifurca**: sale un brazo
//!    con sesgo lateral que baja por su cuenta.
//!
//! Se baja por la PLATAFORMA y no por el campo de altura completo a propósito: el
//! campo lleva el ruido encima, y el ruido tiene mínimos locales cada pocos tiles
//! —un descenso por ahí se mete en el primer hoyo y se queda—. La plataforma es
//! suave y es la que dice de verdad hacia dónde cae el continente. El ruido lo
//! pone el mundo después, y para que no le levante una loma al cauce por el medio,
//! **el río excava** ([`perfil_del_cauce`]).
//!
//! ## Por qué esto se puede consultar por celda
//!
//! `hay_rio` lo pregunta cada celda de tierra del mundo. Tres memos encadenados
//! lo hacen barato:
//!
//! - el trazado de una cuenca se calcula una vez y se guarda ([`TRAZOS`]);
//! - los segmentos que pasan por un bloque de [`BLOQUE`] tiles se recogen una vez
//!   ([`BLOQUES`]) descartando por caja los trazos que ni se acercan;
//! - y la consulta de una celda mira sólo esa lista corta.
//!
//! Un chunk son 256 celdas dentro del mismo bloque, así que el trazado se paga
//! una vez cada 128×128 tiles y no una vez por celda.

use std::cell::RefCell;
use std::rc::Rc;

use crate::celestial_bodies::{body_terrain_seed, CelestialBody};
use crate::geography::hash2;

/// Lado de una cuenca, en tiles. Una naciente por cuenca como mucho.
///
/// 900 con [`PASOS_MAX`]×[`PASO`] = 1.536 tiles de recorrido máximo deja el radio
/// de búsqueda en dos cuencas (una malla de 5×5): un río puede cruzar la cuenca
/// vecina entera y todavía se lo encuentra desde donde termina.
pub const CUENCA: i64 = 900;

/// Zancada del trazado, en tiles. Es también el radio con el que se mide la
/// pendiente: pasos más cortos siguen el ruido de la plataforma en vez de su
/// forma, y el cauce se vuelve una greca.
const PASO: i64 = 48;

/// Cuántas zancadas como mucho.
///
/// **56×48 = 2.688 tiles, y el número sale de una medición.** Con 48×32 = 1.536
/// los cauces se agotaban a medio camino: trazados desde la sierra, ninguno
/// llegaba al mar ni a una laguna y todos terminaban por límite de pasos, o sea
/// el mismo "EnSeco" que se venía a arreglar, con otro mecanismo. Desde la
/// cordillera del norte hasta la costa de este planeta hay unos dos mil tiles.
const PASOS_MAX: usize = 80;

/// Cuántas cuencas alrededor hay que mirar para no perderse un río que viene de
/// lejos. `PASOS_MAX × PASO` = 2.688 tiles, o sea tres cuencas de [`CUENCA`]:
/// con menos, un río largo desaparece a mitad de camino porque su naciente cae
/// fuera de la vecindad consultada.
const RADIO_CUENCAS: i64 = 5;

/// **CUÁNTO SERPENTEA.** Desplazamiento lateral máximo por zancada, en tiles.
///
/// Sin esto el cauce sale RECTO: la plataforma es suave, así que la máxima
/// pendiente apunta casi siempre al mismo rumbo y el río baja como una regla.
/// Medido en el primer trazado: 48 zancadas y **45 tiles de desvío total en x**
/// contra 1.536 de avance — un rastrillo, no un río. El serpenteo es ruido de
/// baja frecuencia sobre la perpendicular al rumbo, así que el cauce sigue
/// bajando pero dibuja meandros.
const MEANDRO: f32 = 0.55;

/// **CUÁNTO TIRA EL AGUA DE UN RÍO QUE PASA CERCA.**
///
/// Un río no busca sólo la bajada: busca el mar. En un llano la pendiente de la
/// plataforma es casi nula y la elección de rumbo se decide por milésimas, así
/// que sin este término el cauce da tumbos y muere en el prado — medido, sólo
/// **3 de 36 lagunas** tenían un afluente. `world_map::sumidero_cercano` ya sabe
/// dónde está el agua más próxima y con cuánto peso; acá ese rumbo se suma al
/// puntaje de cada zancada.
///
/// 0,004 es del orden de la bajada de plataforma en una zancada de [`PASO`]
/// tiles: manda donde el terreno es plano y no donde hay ladera, que es
/// exactamente el reparto que se quiere.
const TIRON_DEL_AGUA: f32 = 0.010;

/// **DÓNDE NACE UN RÍO**, en fracción de la escalera de tierra (0 = orilla,
/// 1 = techo del mundo).
///
/// El pedido fue explícito en el "no": *"no desde el top de la montaña, sino que
/// surja de por ahí de algún lado alto"*. Una naciente en el pico sale de la
/// nieve y baja por la cara del acantilado; una en la ladera alta sale de donde
/// salen los ríos.
/// **0,30 y no 0,42, y está medido.** Con 0,42 el único terreno que pasaba el
/// filtro era la cordillera: la sierra quedaba con 0,74 % de cauce y el llano y
/// la cuenca oeste con **0,00 % y 0,06 %**, o sea un planeta con ríos sólo arriba
/// del todo. La llanura de este mundo está entre 0,17 y 0,25 de la escalera
/// (medido sobre `world_map::platform`), así que 0,30 deja nacer un río en
/// cualquier loma que sobresalga del llano sin llegar a poner nacientes en la
/// pradera misma. El techo sigue en 0,86: la cumbre es nieve y roca, no un
/// manantial.
const NACE_DESDE: f32 = 0.30;
const NACE_HASTA: f32 = 0.86;

/// Lado del bloque de consulta, en tiles. Ver [`BLOQUES`].
const BLOQUE: i64 = 128;

/// Ancho del cauce en la naciente y en la desembocadura, en tiles. Un río crece:
/// arriba se cruza de un salto, abajo hay que vadearlo.
const ANCHO_NACIENTE: f32 = 1.2;
const ANCHO_BOCA: f32 = 4.0;

/// **CUÁNTO SE HUNDE EL FONDO DEL CAUCE bajo la plataforma, EN ESCALONES.**
///
/// # Por qué en escalones y no en unidades de campo
///
/// Era `0.030` de campo, a ojo *"un tercio de escalón"*. Medido, un escalón vale
/// `(1 − sea_level) / LEVELS_UP` = **0,0467** en la Tierra: 0,030 es menos que
/// uno, así que la talla **no bajaba el cauce ni un nivel** — sólo corría la
/// frontera de cuantización. El nivel de la orilla parpadeaba a lo largo del
/// curso y por eso los farallones cruzaban el río en zig-zag en vez de dejarlo
/// pasar (la captura de la sierra del 16 ago 2026).
///
/// # Por qué UN escalón exacto y no más
///
/// Se probó con 1,15 —la idea era que el redondeo no dejara tramos empatados con
/// su orilla— y **el número lo desmintió la ciudad**: con esa hondura, la ribera
/// de Ciudad Central mete un borde de terraza más en el casco, y `city::lot_at`
/// no planta parcelas a caballo de un escalón. Medido con
/// `city::la_ciudad_no_esta_llena_de_baldios`: **135 de 218 manzanas** (62 %),
/// por debajo del dos de cada tres que separa una ciudad de un descampado.
///
/// Con 1,0 pasan las dos cosas, y de hecho el río sale MEJOR
/// (`medir_rios`, ventana de sierra de 1.200 tiles):
///
/// | | antes (0,030 de campo) | 1,15 | **1,0** |
/// |---|---|---|---|
/// | cauce cortado por roca (sierra) | 21,75 % | 4,39 % | **4,12 %** |
/// | cauce cortado por roca (llano) | 0,52 % | 1,11 % | **0,01 %** |
/// | salto de campo p99 en el valle | 0,0317 | 0,0214 | **0,0199** |
/// | cauce por debajo de su ribera (sierra) | 83,6 % | — | **96,1 %** |
const HONDURA_EN_ESCALONES: f32 = 1.0;

/// La hondura del cauce en unidades de campo, para este cuerpo. Un escalón de
/// terraza mide distinto según dónde tenga el mar cada planeta.
fn hondura(body: CelestialBody) -> f32 {
    let paso = (1.0 - crate::relief::sea_level(body)).max(1e-3) / crate::relief::LEVELS_UP as f32;
    paso * HONDURA_EN_ESCALONES
}

/// **Cuántas veces más ancho que el agua es el VALLE**, el trozo de terreno que
/// el río aplana a su alrededor.
///
/// Sin valle, el cauce sería una zanja de paredes rectas. Con 3, un río de cinco
/// tiles baja por una vaguada de quince y la orilla se camina.
const VALLE: f32 = 3.0;

/// **Y NUNCA MENOS DE ESTO, EN TILES.** Lo que gobierna el dibujo del valle no es
/// la proporción, es la PENDIENTE.
///
/// Un río de naciente mide 1,2 tiles y su valle proporcional 3,6: en esos 3,6
/// tiles el campo pasaba de la cota lisa del cauce al terreno con el ruido
/// entero, o sea hasta `RELIEF_AMPLITUDE/2` = 0,21 — **0,06 por tile, el triple
/// de `relief::MAX_FIELD_STEP_PER_TILE`**. Y esa cota no es decorativa: el
/// descarte barato de `relief::near_step_boundary` se apoya en ella. El propio
/// empalme del valle fabricaba los escalones que después cortan el cauce.
///
/// Con 8 tiles y el ruido atenuado ([`RUIDO_EN_EL_CAUCE`]) el peor salto queda en
/// `0,75 × 0,21 / 8 ≈ 0,020`, o sea dentro de la cota. Se calibra con
/// `medir_rios::el_campo_no_salta_en_el_valle`.
const VALLE_MINIMO: f32 = 8.0;

/// **CUÁNTO RUIDO DE TERRENO SOBREVIVE EN EL EJE DEL CAUCE.**
///
/// El empalme anterior anulaba el ruido por completo en el eje (`peso = 1`) y lo
/// devolvía entero en el borde del valle: pegaba un campo liso contra uno
/// rugoso, y esa costura es un escalón. Dejando pasar un cuarto, el fondo del río
/// sigue ondulando con el terreno —que es lo que hace un río de verdad— y lo que
/// el río impone es su HONDURA relativa, no una cota plana de hormigón.
pub(crate) const RUIDO_EN_EL_CAUCE: f32 = 0.25;

/// El valle de un cauce de ancho `a`, en tiles. Ver [`VALLE_MINIMO`].
fn valle_de(a: f32) -> f32 {
    (a * VALLE).max(VALLE_MINIMO)
}

/// **Dónde termina un cauce.** Lo devuelve [`destinos`], que es la medición que
/// importa de este módulo: un río que no termina en agua no es un río.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destino {
    /// Llegó al mar (la plataforma bajó del nivel del líquido).
    Mar,
    /// Entró en una laguna.
    Lago,
    /// Se quedó sin zancadas antes de llegar a ninguna de las dos.
    SinLlegar,
    /// El terreno dejó de bajar y no había collado que cruzar.
    Estancado,
}

/// Un tramo de cauce: dos puntos y el ancho en cada uno.
#[derive(Clone, Copy)]
struct Tramo {
    x0: f32,
    z0: f32,
    x1: f32,
    z1: f32,
    a0: f32,
    a1: f32,
    /// La cota de la PLATAFORMA en cada extremo. Es lo que convierte al cauce en
    /// un perfil y no en una resta: el fondo del río toma esta altura, que baja
    /// zancada a zancada por construcción, así que el agua corre cuesta abajo
    /// aunque el ruido del terreno tenga otra opinión (ver [`perfil_del_cauce`]).
    h0: f32,
    h1: f32,
}

/// El trazado de una cuenca: sus tramos y la caja que los contiene.
#[derive(Clone)]
struct Trazo {
    tramos: Vec<Tramo>,
    min_x: f32,
    min_z: f32,
    max_x: f32,
    max_z: f32,
}

impl Trazo {
    fn vacio() -> Self {
        Trazo { tramos: Vec::new(), min_x: 0.0, min_z: 0.0, max_x: -1.0, max_z: -1.0 }
    }
}

/// **LADO DE LA REJILLA DE UN BLOQUE**, en sub-celdas. Con [`BLOQUE`] = 128 y 8
/// de lado, cada sub-celda mide 16 tiles.
const REJILLA: i64 = 8;
const SUBCELDA: f32 = (BLOQUE / REJILLA) as f32;

/// **HASTA DÓNDE PUEDE LLEGAR UN TRAMO.** El valle más ancho posible es el de la
/// desembocadura ([`ANCHO_BOCA`] × [`VALLE`]); un tile más de margen por el
/// redondeo de la celda. Ningún tramo más lejos que esto puede imponerle cota a
/// una celda, y por eso la rejilla puede descartarlo sin mirar.
const ALCANCE_MAX: f32 = {
    let proporcional = ANCHO_BOCA * VALLE;
    // `f32::max` no es const: el valle más ancho es el mayor de los dos.
    if proporcional > VALLE_MINIMO { proporcional + 1.0 } else { VALLE_MINIMO + 1.0 }
};

/// **LOS TRAMOS DE UN BLOQUE, INDEXADOS POR SUB-CELDA.**
///
/// El filtro por caja del bloque deja una lista corta comparada con los 25
/// trazados de la vecindad, pero sigue siendo TODA la cuenca que cruza esos 128
/// tiles — decenas de tramos, y `perfil_del_cauce` los recorría enteros por
/// celda. Medido (`coste_del_relieve::reparto_del_campo`, release), el río era
/// **1,3 µs de los 2,0 que cuesta el campo de altura en el llano**: más que la
/// plantilla del mundo y el ruido juntos.
///
/// Un tramo sólo puede afectar a lo que tenga a menos de `ancho × VALLE` (como
/// mucho [`ANCHO_BOCA`] × [`VALLE`] = 12 tiles), así que se reparte en una
/// rejilla y cada celda mira sólo su casilla. La rejilla se arma UNA vez por
/// bloque, junto con la lista; la consulta pasa a ser un índice y dos o tres
/// distancias.
struct BloqueRios {
    tramos: Vec<Tramo>,
    /// CSR: los candidatos de la sub-celda `i` son `orden[inicio[i]..inicio[i+1]]`.
    inicio: Vec<u32>,
    orden: Vec<u16>,
    /// Esquina del bloque, en tiles.
    x0: f32,
    z0: f32,
}

impl BloqueRios {
    fn vacio() -> Self {
        BloqueRios {
            tramos: Vec::new(),
            inicio: vec![0; (REJILLA * REJILLA) as usize + 1],
            orden: Vec::new(),
            x0: 0.0,
            z0: 0.0,
        }
    }

    fn armar(tramos: Vec<Tramo>, x0: f32, z0: f32) -> Self {
        // Los índices de la rejilla son `u16`. Un bloque de 128 tiles guarda
        // decenas de tramos, no decenas de miles, pero un truncamiento silencioso
        // acá saldría como un río que desaparece en un trozo del mapa.
        debug_assert!(
            tramos.len() <= u16::MAX as usize,
            "el bloque tiene {} tramos y la rejilla indexa con u16",
            tramos.len()
        );
        let n = (REJILLA * REJILLA) as usize;
        let mut cuenta = vec![0u32; n + 1];
        // Dos pasadas (contar, colocar) para no reservar un `Vec` por casilla.
        //
        // **El alcance es el MÁXIMO del mundo y no el de cada tramo**, y eso es lo
        // que hace la rejilla exactamente equivalente a recorrer la lista entera.
        // Con un alcance por tramo, un afluente estrecho que pasara más cerca
        // podía tapar al río ancho de al lado y el valle desaparecía en un trozo.
        // Con el máximo, todo tramo a menos de [`ALCANCE_MAX`] está en la casilla,
        // y ningún tramo que quede fuera puede ganar: su distancia ya es mayor que
        // cualquier valle posible. La poda sigue siendo enorme (13 tiles contra los
        // 128 del bloque).
        let caja = |t: &Tramo| {
            let alcance = ALCANCE_MAX;
            let (minx, maxx) = (t.x0.min(t.x1) - alcance, t.x0.max(t.x1) + alcance);
            let (minz, maxz) = (t.z0.min(t.z1) - alcance, t.z0.max(t.z1) + alcance);
            let ix0 = (((minx - x0) / SUBCELDA).floor() as i64).clamp(0, REJILLA - 1);
            let ix1 = (((maxx - x0) / SUBCELDA).floor() as i64).clamp(0, REJILLA - 1);
            let iz0 = (((minz - z0) / SUBCELDA).floor() as i64).clamp(0, REJILLA - 1);
            let iz1 = (((maxz - z0) / SUBCELDA).floor() as i64).clamp(0, REJILLA - 1);
            (ix0, ix1, iz0, iz1)
        };
        for t in &tramos {
            let (ix0, ix1, iz0, iz1) = caja(t);
            for iz in iz0..=iz1 {
                for ix in ix0..=ix1 {
                    cuenta[(iz * REJILLA + ix) as usize + 1] += 1;
                }
            }
        }
        for i in 0..n {
            cuenta[i + 1] += cuenta[i];
        }
        let total = cuenta[n] as usize;
        let mut orden = vec![0u16; total];
        let mut cursor = cuenta.clone();
        for (i, t) in tramos.iter().enumerate() {
            let (ix0, ix1, iz0, iz1) = caja(t);
            for iz in iz0..=iz1 {
                for ix in ix0..=ix1 {
                    let c = &mut cursor[(iz * REJILLA + ix) as usize];
                    orden[*c as usize] = i as u16;
                    *c += 1;
                }
            }
        }
        BloqueRios { tramos, inicio: cuenta, orden, x0, z0 }
    }

    fn is_empty(&self) -> bool {
        self.tramos.is_empty()
    }

    /// Los tramos que pueden alcanzar el punto `(x, z)`.
    fn candidatos(&self, x: f32, z: f32) -> impl Iterator<Item = &Tramo> {
        let ix = (((x - self.x0) / SUBCELDA).floor() as i64).clamp(0, REJILLA - 1);
        let iz = (((z - self.z0) / SUBCELDA).floor() as i64).clamp(0, REJILLA - 1);
        let i = (iz * REJILLA + ix) as usize;
        let (a, b) = (self.inicio[i] as usize, self.inicio[i + 1] as usize);
        self.orden[a..b].iter().map(move |&k| &self.tramos[k as usize])
    }
}

/// La clave de un memo de mundo: `(cuerpo, época de la semilla, celda)`.
type Clave = (u8, u32, i64, i64);

thread_local! {
    /// Trazados ya calculados, por `(cuerpo, época, cx, cz)`. 128 entradas: la
    /// malla de 7×7 que necesita una consulta ([`RADIO_CUENCAS`]) entra dos veces,
    /// que es lo que hace que moverse al bloque de al lado no vuelva a trazar
    /// nada — comparten 42 de las 49 cuencas.
    static TRAZOS: RefCell<Vec<(Clave, Rc<Trazo>)>> = const { RefCell::new(Vec::new()) };
    /// Los tramos que tocan un bloque de [`BLOQUE`] tiles, ya filtrados por caja.
    /// Es lo que hace que la consulta de una celda mire una lista corta en vez de
    /// los 25 trazados de la vecindad.
    static BLOQUES: RefCell<Vec<(Clave, Rc<BloqueRios>)>> = const { RefCell::new(Vec::new()) };
    /// **El último bloque consultado.** `compute_field` pregunta por CADA celda del
    /// mundo y las celdas llegan en orden: un chunk son 256 consultas seguidas al
    /// mismo bloque. Sin este atajo, cada una recorría la lista de [`BLOQUES`]
    /// comparando claves. Medido por `cuevas::preguntar_por_la_cueva_es_barato`,
    /// que es el guardián de coste del generador.
    static ULTIMO: RefCell<Option<(Clave, Rc<BloqueRios>)>> = const { RefCell::new(None) };
}

const MEMO_TRAZOS: usize = 256;
const MEMO_BLOQUES: usize = 16;

/// **CUÁNTAS NACIENTES SORTEA UNA CUENCA.**
///
/// Era una. Medido sobre la Tierra, con una sola naciente por cuenca y el filtro
/// de altura, **sólo el 11 % de las cuencas tenía río** y las tres ventanas de
/// `medir_rios` (1.200×1.200 tiles cada una) no tocaban ni un cauce: un planeta
/// con ríos que no se cruzan no tiene ríos. Con tres candidatas por cuenca —cada
/// una válida sólo si cae en la ladera alta— la densidad vuelve al orden del
/// entramado de ruido que se reemplazó (1,3–5,9 % de la tierra), que es el que el
/// mundo tenía calibrado.
const NACIENTES_POR_CUENCA: u32 = 4;

/// Las nacientes de una cuenca: hasta [`NACIENTES_POR_CUENCA`], y sólo las que
/// caen en la ladera alta ([`NACE_DESDE`]…[`NACE_HASTA`]).
///
/// No hay sorteo de "esta cuenca tiene río": lo decide el TERRENO. Un sorteo
/// encima del filtro de altura dejaba comarcas enteras de montaña sin un hilo de
/// agua por azar, y el agua de montaña no es cuestión de suerte.
fn nacientes(body: CelestialBody, cx: i64, cz: i64) -> Vec<(f32, f32)> {
    let sea = crate::relief::sea_level(body);
    if sea <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..NACIENTES_POR_CUENCA {
        let h = hash2(body_terrain_seed(body) ^ 0x21ce_5000 ^ i.wrapping_mul(0x9e37_79b9), cx, cz);
        let margen = 90;
        let span = (CUENCA - margen * 2).max(1);
        let x = cx * CUENCA + margen + ((h >> 7) as i64).rem_euclid(span);
        let z = cz * CUENCA + margen + ((h >> 17) as i64).rem_euclid(span);
        let p = crate::world_map::platform(body, x, z);
        let alt = ((p - sea) / (1.0 - sea)).clamp(0.0, 1.0);
        if (NACE_DESDE..=NACE_HASTA).contains(&alt) {
            out.push((x as f32, z as f32));
        }
    }
    out
}

/// El mejor rumbo desde `(x, z)`, medido a [`PASO`] tiles sobre la plataforma, y
/// **si ese rumbo baja de verdad**. Quien llama decide cuántas zancadas seguidas
/// sin bajar tolera ([`ESTANCADO_MAX`]).
fn cuesta_abajo(body: CelestialBody, x: f32, z: f32, sesgo: (f32, f32)) -> Option<((f32, f32), bool)> {
    const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const RUMBOS: [(f32, f32); 8] = [
        (1.0, 0.0), (D, D), (0.0, 1.0), (-D, D),
        (-1.0, 0.0), (-D, -D), (0.0, -1.0), (D, -D),
    ];
    let aqui = crate::world_map::platform(body, x as i64, z as i64);
    // El rumbo al agua más cercana, con su peso. Ver [`TIRON_DEL_AGUA`].
    let hacia_el_agua = crate::world_map::sumidero_cercano(body, x as i64, z as i64)
        .and_then(|((cx, cz), peso)| {
            let (dx, dz) = ((cx as f32) - x, (cz as f32) - z);
            let l = (dx * dx + dz * dz).sqrt();
            (l > 1.0).then(|| (dx / l, dz / l, peso))
        });
    let mut mejor: Option<((f32, f32), f32)> = None;
    for (dx, dz) in RUMBOS {
        let nx = x + dx * PASO as f32;
        let nz = z + dz * PASO as f32;
        let p = crate::world_map::platform(body, nx as i64, nz as i64);
        // El sesgo es lo que separa un brazo bifurcado del tronco: dos rumbos con
        // la misma pendiente dejan de empatar y cada uno se va por su lado.
        let mut puntaje = (aqui - p) + (dx * sesgo.0 + dz * sesgo.1);
        if let Some((ux, uz, peso)) = hacia_el_agua {
            puntaje += (dx * ux + dz * uz) * TIRON_DEL_AGUA * peso;
        }
        if mejor.is_none_or(|(_, m)| puntaje > m) {
            mejor = Some(((nx, nz), puntaje));
        }
    }
    let ((nx, nz), puntaje) = mejor?;
    Some(((nx, nz), puntaje > 0.0))
}

/// **CUÁNTAS ZANCADAS SEGUIDAS SIN BAJAR SE TOLERAN.**
///
/// La primera versión cortaba el cauce en cuanto ninguna vecina estaba más baja,
/// y con eso los ríos morían a las pocas zancadas: medido, la sierra tenía 0,72 %
/// de cauce y el llano **cero**, o sea que ningún río bajaba de la montaña. Un
/// río de verdad cruza collados y rellena hondonadas —eso es un lago— y sigue.
/// Cuatro zancadas de tolerancia (unos 190 tiles) le alcanzan para pasar un
/// umbral sin convertirse en un cauce que sube por la ladera de enfrente.
const ESTANCADO_MAX: u32 = 4;

/// Traza un brazo desde `(x, z)` hasta el mar, una laguna o el agotamiento.
#[allow(clippy::too_many_arguments)]
fn trazar_brazo(
    body: CelestialBody,
    mut x: f32,
    mut z: f32,
    sesgo: (f32, f32),
    pasos: usize,
    desde: f32,
    out: &mut Vec<Tramo>,
    fin: &mut Destino,
) -> Vec<(f32, f32, f32)> {
    let sea = crate::relief::sea_level(body);
    let mut camino = Vec::with_capacity(pasos);
    let mut estancado = 0u32;
    // La cota del fondo, monótona decreciente desde la naciente.
    let mut cota = crate::world_map::platform(body, x as i64, z as i64);
    for i in 0..pasos {
        // El ancho crece con lo andado: arroyo arriba, río abajo.
        let t = desde + (1.0 - desde) * (i as f32 / pasos.max(1) as f32);
        let a0 = ANCHO_NACIENTE + (ANCHO_BOCA - ANCHO_NACIENTE) * t;
        camino.push((x, z, a0));
        // **El mar y el lago son el final, y son el punto del ejercicio.** Un
        // cauce que se corta antes es exactamente lo que medía `medir_rios` como
        // "EnSeco".
        if crate::world_map::platform(body, x as i64, z as i64) <= sea {
            *fin = Destino::Mar;
            break;
        }
        // **NO SE PARA EN LA ORILLA: SE ENTRA.** El corte estaba en
        // `pond_covering`, que da positivo a 1,25 radios del centro — o sea
        // FUERA del agua visible. El cauce se detenía a las puertas del lago y
        // medido daba lo que tenía que dar: de 36 lagunas con agua, sólo 1 tenía
        // un afluente que le tocara la orilla. Se sigue hasta media laguna
        // adentro; el tile de agua del lago se come el resto y lo que se ve es un
        // río desembocando.
        if let Some(((cx, cz), r)) = crate::world_map::pond_covering(body, x as i64, z as i64) {
            let d = ((x - cx as f32).powi(2) + (z - cz as f32).powi(2)).sqrt();
            if d < r * 0.5 {
                *fin = Destino::Lago;
                break;
            }
        }
        // El sesgo se va apagando: un brazo sale de costado y después se
        // comporta como cualquier río, siguiendo la pendiente.
        let apagado = 1.0 - (i as f32 / pasos.max(1) as f32);
        let s = (sesgo.0 * apagado, sesgo.1 * apagado);
        let Some(((nx, nz), bajo)) = cuesta_abajo(body, x, z, s) else {
            *fin = Destino::Estancado;
            break;
        };
        // Un collado no termina un río: se cruza. Ver [`ESTANCADO_MAX`].
        estancado = if bajo { 0 } else { estancado + 1 };
        if estancado > ESTANCADO_MAX {
            *fin = Destino::Estancado;
            break;
        }
        // **EL MEANDRO.** La perpendicular al rumbo, desplazada por un ruido
        // lento: el cauce sigue bajando (la pendiente ya eligió el rumbo) pero
        // deja de ser una recta. Ver [`MEANDRO`].
        let (rx, rz) = (nx - x, nz - z);
        let l = (rx * rx + rz * rz).sqrt().max(1e-3);
        let onda = crate::geography::fbm(
            body_terrain_seed(body) ^ 0x21ce_7000,
            x as i64,
            z as i64,
            PASO * 6,
        ) - 0.5;
        let lat = onda * 2.0 * MEANDRO * PASO as f32;
        let (nx, nz) = (nx - rz / l * lat, nz + rx / l * lat);
        // **LA COTA DEL FONDO NO PUEDE SUBIR.** El rumbo puede estancarse un par
        // de zancadas para cruzar un collado ([`ESTANCADO_MAX`]) y el meandro
        // puede dejar el punto siguiente en terreno más alto; el agua, no. La
        // cota se queda en la que traía, y por eso seguir el cauce aguas abajo es
        // siempre bajar — que es la propiedad entera de este módulo.
        let alla = crate::world_map::platform(body, nx as i64, nz as i64).min(cota);
        let a1 = ANCHO_NACIENTE
            + (ANCHO_BOCA - ANCHO_NACIENTE)
                * (desde + (1.0 - desde) * ((i + 1) as f32 / pasos.max(1) as f32));
        out.push(Tramo { x0: x, z0: z, x1: nx, z1: nz, a0, a1, h0: cota, h1: alla });
        cota = alla;
        x = nx;
        z = nz;
    }
    camino
}

/// El trazado completo de una cuenca: un tronco por naciente, cada uno con su
/// brazo bifurcado.
fn trazar(body: CelestialBody, cx: i64, cz: i64) -> Trazo {
    let fuentes = nacientes(body, cx, cz);
    if fuentes.is_empty() {
        return Trazo::vacio();
    }
    let mut tramos = Vec::new();
    for (n, (x, z)) in fuentes.into_iter().enumerate() {
        let mut fin = Destino::SinLlegar;
        let camino = trazar_brazo(body, x, z, (0.0, 0.0), PASOS_MAX, 0.0, &mut tramos, &mut fin);
        // **LA BIFURCACIÓN.** Sale de un punto sorteado del tercio medio del
        // tronco: más arriba el brazo nace donde el río todavía es un hilo y no
        // se lee como una horquilla; más abajo, se junta con la desembocadura.
        if camino.len() < 6 {
            continue;
        }
        let h = hash2(
            body_terrain_seed(body) ^ 0x21ce_6000 ^ (n as u32).wrapping_mul(0x85eb_ca6b),
            cx,
            cz,
        );
        let i = camino.len() / 3 + (h as usize % (camino.len() / 3).max(1));
        let i = i.min(camino.len() - 1);
        let (bx, bz, _) = camino[i];
        // El sesgo lateral es perpendicular al rumbo que llevaba el tronco, y el
        // lado lo decide el mismo hash: si fuera siempre el mismo, todos los ríos
        // del planeta se abrirían hacia la misma mano.
        let (px, pz) = rumbo_entre(camino[i.saturating_sub(1)], camino[i]);
        let lado = if (h >> 9).is_multiple_of(2) { 1.0 } else { -1.0 };
        let sesgo = (-pz * 0.004 * lado, px * 0.004 * lado);
        let t = i as f32 / camino.len() as f32;
        let mut fin_brazo = Destino::SinLlegar;
        trazar_brazo(body, bx, bz, sesgo, PASOS_MAX - i, t, &mut tramos, &mut fin_brazo);
    }
    let mut tr = Trazo {
        min_x: f32::MAX,
        min_z: f32::MAX,
        max_x: f32::MIN,
        max_z: f32::MIN,
        tramos,
    };
    for t in &tr.tramos {
        tr.min_x = tr.min_x.min(t.x0.min(t.x1));
        tr.max_x = tr.max_x.max(t.x0.max(t.x1));
        tr.min_z = tr.min_z.min(t.z0.min(t.z1));
        tr.max_z = tr.max_z.max(t.z0.max(t.z1));
    }
    tr
}

/// El rumbo unitario que llevaba el cauce entre dos puntos de su camino. Es lo
/// que da la perpendicular por la que sale el brazo bifurcado.
fn rumbo_entre(a: (f32, f32, f32), b: (f32, f32, f32)) -> (f32, f32) {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let l = (dx * dx + dz * dz).sqrt();
    if l <= 1e-3 {
        (1.0, 0.0)
    } else {
        (dx / l, dz / l)
    }
}

/// **`Rc` y no `Trazo`: esto se devolvía CLONADO en cada acierto.** Armar un
/// bloque consulta las 121 cuencas de la vecindad ([`RADIO_CUENCAS`]) y cada
/// trazo son ~50 tramos, así que cada bloque pagaba 121 reservas y copias de
/// vector antes siquiera de empezar a filtrar. El `Rc` deja el acierto en un
/// contador.
fn trazo_de(body: CelestialBody, cx: i64, cz: i64) -> Rc<Trazo> {
    let clave = (body as u8, crate::partida::epoca(), cx, cz);
    if let Some(t) = TRAZOS.with(|m| {
        m.borrow().iter().find(|(k, _)| *k == clave).map(|(_, v)| v.clone())
    }) {
        return t;
    }
    let t = Rc::new(trazar(body, cx, cz));
    TRAZOS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= MEMO_TRAZOS {
            m.remove(0);
        }
        m.push((clave, t.clone()));
    });
    t
}

/// Los tramos que pueden tocar el bloque de `(bx, bz)`.
fn tramos_del_bloque(body: CelestialBody, bx: i64, bz: i64) -> Rc<BloqueRios> {
    let clave = (body as u8, crate::partida::epoca(), bx, bz);
    // **El bloque de la consulta anterior, sin recorrer nada.** Ver [`ULTIMO`].
    if let Some(v) = ULTIMO.with(|u| {
        u.borrow().as_ref().and_then(|(k, v)| (*k == clave).then(|| v.clone()))
    }) {
        return v;
    }
    if let Some(v) = BLOQUES.with(|m| {
        m.borrow().iter().find(|(k, _)| *k == clave).map(|(_, v)| v.clone())
    }) {
        ULTIMO.with(|u| *u.borrow_mut() = Some((clave, v.clone())));
        return v;
    }
    let x0 = (bx * BLOQUE) as f32;
    let z0 = (bz * BLOQUE) as f32;
    let x1 = x0 + BLOQUE as f32;
    let z1 = z0 + BLOQUE as f32;
    let margen = ANCHO_BOCA + 2.0;
    let (ccx, ccz) = (
        (bx * BLOQUE).div_euclid(CUENCA),
        (bz * BLOQUE).div_euclid(CUENCA),
    );
    let mut out = Vec::new();
    for ox in -RADIO_CUENCAS..=RADIO_CUENCAS {
        for oz in -RADIO_CUENCAS..=RADIO_CUENCAS {
            let tr = trazo_de(body, ccx + ox, ccz + oz);
            // Descarte por caja: la inmensa mayoría de los trazos de la vecindad
            // ni se acercan al bloque, y cada uno son ~50 tramos.
            if tr.max_x < x0 - margen || tr.min_x > x1 + margen {
                continue;
            }
            if tr.max_z < z0 - margen || tr.min_z > z1 + margen {
                continue;
            }
            for t in &tr.tramos {
                let (tminx, tmaxx) = (t.x0.min(t.x1), t.x0.max(t.x1));
                let (tminz, tmaxz) = (t.z0.min(t.z1), t.z0.max(t.z1));
                if tmaxx < x0 - margen || tminx > x1 + margen {
                    continue;
                }
                if tmaxz < z0 - margen || tminz > z1 + margen {
                    continue;
                }
                out.push(*t);
            }
        }
    }
    // **`Rc` y no `Vec`: esto se devolvía CLONADO en cada celda.** Un bloque
    // guarda decenas de tramos, así que cada consulta de terreno pagaba una
    // reserva de memoria y una copia — por celda, en el camino caliente de
    // `relief::compute_field`. Es lo que puso el generador en 2,27 µs/celda
    // contra un presupuesto de 2,0.
    let out = Rc::new(BloqueRios::armar(out, x0, z0));
    BLOQUES.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() >= MEMO_BLOQUES {
            m.remove(0);
        }
        m.push((clave, out.clone()));
    });
    ULTIMO.with(|u| *u.borrow_mut() = Some((clave, out.clone())));
    out
}

/// Distancia de un punto a un tramo, y el ancho y la cota interpolados en el pie
/// de esa perpendicular.
fn distancia(t: &Tramo, x: f32, z: f32) -> (f32, f32, f32) {
    let (dx, dz) = (t.x1 - t.x0, t.z1 - t.z0);
    let len2 = dx * dx + dz * dz;
    let u = if len2 <= 1e-6 {
        0.0
    } else {
        (((x - t.x0) * dx + (z - t.z0) * dz) / len2).clamp(0.0, 1.0)
    };
    let px = t.x0 + dx * u;
    let pz = t.z0 + dz * u;
    (
        ((x - px).powi(2) + (z - pz).powi(2)).sqrt(),
        t.a0 + (t.a1 - t.a0) * u,
        t.h0 + (t.h1 - t.h0) * u,
    )
}

/// **¿Corre agua por esta celda?** Es la consulta que reemplaza al entramado de
/// ruido: `true` si la celda cae dentro del ancho de algún tramo trazado.
pub fn cauce_en(body: CelestialBody, tx: i64, tz: i64) -> bool {
    cerca_del_cauce(body, tx, tz).is_some_and(|(d, a)| d <= a)
}

/// La distancia al cauce más cercano y su ancho ahí, o `None` si no hay ninguno
/// en el bloque. Lo usa además [`perfil_del_cauce`] para cavar el valle.
fn cerca_del_cauce(body: CelestialBody, tx: i64, tz: i64) -> Option<(f32, f32)> {
    let bloque = tramos_del_bloque(body, tx.div_euclid(BLOQUE), tz.div_euclid(BLOQUE));
    if bloque.is_empty() {
        return None;
    }
    let (x, z) = (tx as f32 + 0.5, tz as f32 + 0.5);
    let mut mejor: Option<(f32, f32)> = None;
    for t in bloque.candidatos(x, z) {
        let (d, a, _) = distancia(t, x, z);
        if mejor.is_none_or(|(md, _)| d < md) {
            mejor = Some((d, a));
        }
    }
    mejor
}

/// **EL PERFIL DEL CAUCE: qué cota impone el río, y con cuánta fuerza.**
///
/// Devuelve `(cota, peso)` o `None` si por acá no pasa ningún río. Lo llama
/// `relief::compute_field`, que mezcla el terreno con esa cota.
///
/// # Por qué IMPONE una cota en vez de restar una hondura
///
/// El primer intento restaba una constante al campo donde había cauce. No
/// alcanzó, y está medido: la amplitud del ruido de terreno es **0,42** y la
/// resta era 0,03, o sea que el ruido seguía mandando y el fondo del río subía y
/// bajaba a su antojo. Siguiendo el cauce celda a celda
/// (`medir_rios::hasta_donde_llega_el_cauce`), el recorrido moría a las dos
/// celdas porque ninguna vecina estaba más abajo: exactamente el mismo "muere en
/// seco" que tenía el cauce de ruido, con otra causa.
///
/// Un río de verdad no ondula con el terreno: **excava hasta su perfil**. Cada
/// tramo trae la cota de la plataforma en sus extremos ([`Tramo::h0`]), que baja
/// zancada a zancada por construcción, y el campo se lleva hacia ahí con un peso
/// que vale 1 en el eje del agua y se apaga en el borde del valle ([`VALLE`]).
/// Así el fondo es monótono —seguir el agua es siempre bajar— y la orilla empalma
/// suave con el terreno de al lado en vez de ser el filo de una zanja.
pub fn perfil_del_cauce(body: CelestialBody, tx: i64, tz: i64) -> Option<(f32, f32)> {
    let bloque = tramos_del_bloque(body, tx.div_euclid(BLOQUE), tz.div_euclid(BLOQUE));
    if bloque.is_empty() {
        return None;
    }
    let (x, z) = (tx as f32 + 0.5, tz as f32 + 0.5);
    let mut mejor: Option<(f32, f32, f32)> = None;
    for t in bloque.candidatos(x, z) {
        let (d, a, h) = distancia(t, x, z);
        if mejor.is_none_or(|(md, _, _)| d < md) {
            mejor = Some((d, a, h));
        }
    }
    let (d, a, h) = mejor?;
    let valle = valle_de(a);
    if d >= valle {
        return None;
    }
    // El peso cae como un coseno alzado: 1 en el eje, 0 y con derivada 0 en el
    // borde del valle. Un apagado lineal deja una arista visible en la orilla.
    let t = 1.0 - d / valle;
    let peso = t * t * (3.0 - 2.0 * t);
    Some((h - hondura(body), peso))
}

/// **DÓNDE TERMINAN LOS RÍOS DE UNA CUENCA.** Es la medición del módulo: la
/// diferencia entre un sistema hidrográfico y una decoración es qué fracción de
/// los cauces llega al agua. La usa `medir_rios::donde_termina_cada_rio`.
pub fn destinos(body: CelestialBody, cx: i64, cz: i64) -> Vec<Destino> {
    let mut out = Vec::new();
    for (x, z) in nacientes(body, cx, cz) {
        let mut tramos = Vec::new();
        let mut fin = Destino::SinLlegar;
        trazar_brazo(body, x, z, (0.0, 0.0), PASOS_MAX, 0.0, &mut tramos, &mut fin);
        out.push(fin);
    }
    out
}
