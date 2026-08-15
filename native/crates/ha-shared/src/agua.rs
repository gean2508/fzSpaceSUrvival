//! **LOS CUERPOS DE AGUA** — de qué orilla es cada uno, dónde cae una cascada y
//! qué flota en cada sitio.
//!
//! El agua estaba resuelta como un material: un tile azul, un autotile, una
//! tabla de sorteo de cosas flotantes y listo. Eso da lagos que son agujeros
//! rectangulares con un nenúfar y una boya de salvavidas en medio de la sierra.
//! Un cuerpo de agua no es un material, es un SITIO, y tres cosas lo hacen:
//!
//! 1. **La orilla es del terreno que la rodea** ([`orilla_de`]). El pack trae
//!    tres riberas dibujadas —pasto, playa de arena, orilla pelada— y usar la de
//!    arena en la sierra es exactamente el corte brusco que se veía. La familia
//!    la elige el bioma, igual que el material de un acantilado
//!    (`world2d::cliff_material`).
//! 2. **Donde el agua cruza un escalón, cae** ([`cascada_at`]). No hace falta
//!    sembrar cascadas: el relieve ya dice dónde hay un desnivel y el río ya dice
//!    por dónde corre el agua; donde se cruzan, hay salto. Una cascada puesta a
//!    dedo estaría en cualquier parte — ésta sólo puede estar donde el agua de
//!    verdad baja un escalón.
//! 3. **Lo que flota depende del sitio, no de un dado** ([`sitio_de_agua`]). El
//!    junco crece donde el agua toca la tierra; el nenúfar necesita agua QUIETA;
//!    el tronco a la deriva pide corriente; el cardumen, agua abierta lejos de la
//!    orilla. Con una sola tabla de pesos, el pack entero salía repartido al azar
//!    por todo el líquido del planeta y nada estaba donde tenía sentido.
//!
//! Sim compartida: sin Bevy y determinista. El cliente sólo dibuja lo que este
//! módulo decide.

use crate::celestial_bodies::CelestialBody;
use crate::geography::{biome_at, Biome};
use crate::relief;
use crate::topdown::Tile;
use crate::worldgen::{hash_cell, ExtraDecor};

// ---------------------------------------------------------------------------
// La orilla
// ---------------------------------------------------------------------------

/// De qué está hecha la ribera de esta celda de agua.
///
/// **Son DOS, y la de piedra ya no está.** El pack trae una tercera familia
/// —`Sea_Water_Tileset_No_Sand`, el agua que toca la roca pelada— y usarla era
/// exactamente el corte tosco que se pidió sacar: el líquido pegado al pedregal,
/// sin una brizna en el medio. Regla del usuario: **no hay agua sin césped
/// alrededor**, y la superficie del mundo es pasto. Así que la sierra, la
/// montaña y la cumbre beben con ribera de pasto como todo el resto.
///
/// La playa de arena se queda porque es el sitio donde el corte contra el pasto
/// sería el error inverso — pero sólo ahí (ver [`familia_de_bioma`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orilla {
    /// Ribera de tierra con matas. Es la de casi todo el planeta: pradera,
    /// bosque, pantano, sierra, montaña y cualquier río de llanura.
    Pasto,
    /// Playa de arena. **Sólo la costa y los secarrales**: es el agua de panal
    /// del pack y fuera de la playa se lee como un error.
    Arena,
}

impl Orilla {
    /// El slot de arte de esta familia. Si no está curado, el juego cae al
    /// `tiles/water` de siempre y no queda ninguna celda sin dibujar.
    pub fn art_key(self) -> &'static str {
        match self {
            Orilla::Pasto => "tiles/water",
            Orilla::Arena => "tiles/water_arena",
        }
    }
}

/// La familia que le corresponde a un bioma de TIERRA. `None` para los tres
/// biomas mojados (`Ocean`, `River`, `Swamp`): ésos no dicen de qué es la orilla,
/// dicen que ahí hay agua.
///
/// **La arena es de la arena y de nada más.** El agua de playa del pack lleva un
/// panal dibujado dentro del líquido, y ése es el "agua con cuadraditos" que
/// aparecía en mitad de la sierra: la familia de roca compartía ese centro. Acá
/// la pide sólo quien tiene arena de verdad bajo los pies.
fn familia_de_bioma(b: Biome) -> Option<Orilla> {
    Some(match b {
        Biome::Beach | Biome::Desert => Orilla::Arena,
        // **LA SABANA BEBE CON PASTO, PORQUE SU SUELO ES PASTO.**
        //
        // Estaba en la lista de la arena y ahí se colaba el panal de playa tierra
        // adentro: medido en `agua-3-playa.png`, un río de llanura con las dos
        // riberas de arena cruzando un campo VERDE — porque `biome_tile(Savanna)`
        // devuelve `Grass`. Es exactamente el bug que ya se arregló del lado del
        // acantilado (dos tablas paralelas bioma→material y bioma→tile que
        // discrepaban en cuanto un bioma no era del color de su nombre), sólo que
        // del lado del agua.
        //
        // Y es lo que pidió el usuario: el agua de playa —la que trae panal— es la
        // del MAR, distinta del agua dulce de los ríos y lagunas de tierra
        // adentro. El médano de verdad sigue teniendo su playa: `Desert` tiene
        // suelo de arena y se quedó donde estaba.
        Biome::Savanna => Orilla::Pasto,
        // Todo lo demás bebe con pasto, incluida la piedra: el borde entre el
        // agua y la roca desnuda es el corte que se pidió sacar.
        Biome::Highland | Biome::Mountain | Biome::Snow | Biome::Ice => Orilla::Pasto,
        Biome::Regolith | Biome::Basalt => Orilla::Pasto,
        Biome::Grassland | Biome::Forest => Orilla::Pasto,
        Biome::Ocean | Biome::River | Biome::Swamp => return None,
    })
}

/// Hasta dónde se busca tierra alrededor de una celda de agua. Tres tiles
/// alcanzan para cualquier cauce del pack; más allá ya es agua abierta y no hay
/// orilla que dibujar.
const BUSCA_ORILLA: i64 = 3;

/// **La orilla la manda el terreno que rodea al agua, no el agua.**
pub fn orilla_de(body: CelestialBody, tx: i64, tz: i64) -> Orilla {
    // **EL MAR PRIMERO, Y BARATO.** El nivel viene cacheado; bajo cero es mar
    // abierto y su orilla es playa — el único sitio donde el panal de la arena
    // corresponde. Sin esta salida temprana, cada celda de océano pagaría el
    // barrido de vecinos de acá abajo, y el océano son millones de celdas.
    //
    // **PERO BAJO CERO NO ES SÓLO EL MAR.** Un lago es una cuenca que hunde el
    // terreno (`world_map::basins`, `world_map::pond_in`), así que estaba del
    // lado equivocado de este `if` y **toda laguna del planeta salía ribeteada
    // de arena**, incluidas las del bosque y las de la sierra. Eso es justo el
    // panal de playa fuera de la playa que se pidió sacar. El agua de tierra
    // adentro sigue de largo y se le pregunta a la tierra de al lado, igual que
    // a un río: la pregunta es O(1) y sólo la paga el líquido, nunca el pasto.
    if relief::relief_level(body, tx, tz) < 0
        && !crate::world_map::agua_interior(body, tx, tz)
    {
        return Orilla::Arena;
    }
    // **EL AGUA NO TIENE BIOMA PROPIO: tiene el del sitio por donde pasa.**
    //
    // Un cauce es `Biome::River` lo mismo bajando una cordillera que cruzando
    // una pradera, y un charco de pantano es `Swamp`: ninguno de los tres
    // biomas mojados dice de qué está hecha su ribera.
    //
    // La respuesta está en la TIERRA de al lado, y se busca en anillos hacia
    // afuera para que mande la más cercana: el arroyo que cruza el médano sale
    // con playa, el mismo arroyo tres tiles más allá ya toca pasto.
    for r in 1..=BUSCA_ORILLA {
        for (dx, dz) in [(r, 0), (-r, 0), (0, r), (0, -r), (r, r), (-r, -r), (r, -r), (-r, r)] {
            if let Some(o) = familia_de_bioma(biome_at(body, tx + dx, tz + dz)) {
                return o;
            }
        }
    }
    // Agua ancha sin tierra a la vista: no hay orilla que dibujar y el centro se
    // ve igual en las dos familias.
    Orilla::Pasto
}

// ---------------------------------------------------------------------------
// Cascadas
// ---------------------------------------------------------------------------

// La cascada del pack es MODULAR y por eso se ARMA en vez de estamparse: un
// desnivel de un escalón y uno de diez son la misma boca y la misma poza con
// distinta cantidad de caída en el medio. Las tres piezas se llaman `boca`,
// `tramo` y `poza`, y son las poses del slot `agua/cascada` que corta
// `scripts/import-agua.py` — el que las pide por su nombre es
// `world2d::spawn_chunk_cascadas`. Acá hubo un `enum ParteCascada` que nombraba
// las tres y que no llamaba nadie: el nombre de la pose es el del manifest, y
// duplicarlo en un enum sólo daba dos verdades para lo mismo.

/// **Ancho de una cascada, en tiles.** El dibujo del pack mide tres y no se
/// puede estirar sin que el chorro se vea deformado, así que un salto se arma
/// por grupos de tres celdas y un cauce de siete tiene dos cascadas y un tramo
/// de rápidos, que es como se ve un río de verdad.
pub const ANCHO_CASCADA: i64 = 3;

/// Hasta dónde se rastrea el cauce a lo ancho para agrupar de a tres. Un cauce
/// más ancho que esto se parte en varios saltos, que es lo correcto.
const CAUCE_MAX: i64 = 24;

/// **Alto máximo de un salto, en PISOS de meseta.**
///
/// Un piso es un escalón de terraza (`relief::STEP_M`) y **el salto mide lo que
/// mide la pared por la que cae**, ni más ni menos: [`relief::alto_de_pared`] es
/// el único que decide, y de ahí sale tanto el chorro como la roca de los lados.
/// Por eso el tope no es un número propio del agua sino el del relieve.
///
/// # Por qué antes valía cuatro (y por qué eso era mentira)
///
/// El tope lo puso el usuario cuando toda pared del planeta medía un escalón:
/// daba igual poner cuatro o cuarenta, porque `arriba − abajo` siempre daba uno
/// y **todos los saltos del mundo eran de un piso**. La queja es literal: *"si
/// hay una cascada de tres pisos, obligas a la cascada a que se vea de acuerdo a
/// un piso de acantilado"*. No era el agua: era que el terreno no sabía apilar.
///
/// Con la escalera de `relief::SALTO_DE_TERRAZA`, un río que cruza el borde de
/// una terraza alta cae los ocho o diez pisos que baja el terreno, con la boca al
/// ras de la meseta y la poza al ras del suelo de abajo. Y sigue habiendo saltos
/// chicos encadenados donde el desnivel es chico, que es como baja un río de
/// verdad (ver `hay_cascadas_encadenadas_bajando_la_sierra`).
pub const PISOS_MAX: i64 = relief::MAX_PARED;

/// Cuántas filas de dibujo ocupa la POZA del pack (el reventón sobre el agua de
/// abajo). El cliente lee la medida real del recorte; esto es lo que usa la sim
/// para saber hasta dónde llega el dibujo sin abrir el manifiesto.
pub const POZA_FILAS: i64 = 2;

/// **Hasta qué fila, contando desde el canto, llega el DIBUJO de un salto de
/// `pisos` escalones.**
///
/// El armado es boca (en el canto, fila 0) + un tramo por piso + la poza, y por
/// eso la última fila cae `POZA_FILAS` por debajo del pie de la pared: el
/// reventón va sobre el agua, un cuadro después de donde la roca toca el suelo.
/// Lo usa `world2d::spawn_chunk_cascadas` para colocar las piezas y
/// [`bajo_el_chorro`] para no sortear un nenúfar en medio del chorro.
pub fn filas_de_dibujo(pisos: i64) -> i64 {
    pisos + POZA_FILAS
}

/// Pared de terraza: el material del que está hecho un salto.
fn pared(body: CelestialBody, tx: i64, tz: i64) -> bool {
    matches!(relief::relief_cell(body, tx, tz), relief::ReliefCell::Cliff)
}

/// **El salto que arranca en esta celda**, o `None` si acá no cae nada. Devuelve
/// cuántas filas mide el chorro.
///
/// Un salto es una **pared de terraza que mira al sur** con agua encima (el
/// labio) y agua debajo (por donde sigue bajando el torrente hasta la poza).
///
/// **El sur y no cualquier lado.** La vista es cenital 3/4 y el chorro del pack
/// cae hacia la cámara: una cascada dibujada de frente colocada en una pared que
/// mira al este se vería atravesando la roca. Un salto que cae al norte existe
/// en la geometría, simplemente no se dibuja — se lee como el rápido que es.
///
/// # De dónde sale el ALTO: de lo que BAJA el terreno, y de nada más
///
/// Devuelve **pisos de meseta**, no tiles de dibujo: el desnivel entre la
/// terraza del labio y la del pie. Un piso es un escalón, y un escalón es
/// exactamente lo que mide la pared que el kit del pack cuelga al lado del
/// salto (labio + cuerpo + pie). Por eso la boca queda al ras de la meseta y la
/// poza al ras del suelo de abajo — que es lo que se pidió.
///
/// **El terreno SÍ apila, desde la escalera del mundo.** Este párrafo decía
/// "hoy todos los saltos del planeta bajan UNO, el cálculo sigue siendo la resta
/// por si algún día el relieve apila": ese día llegó y el comentario se quedó
/// mintiendo. El campo de altura sigue sin poder cambiar más de
/// `MAX_FIELD_STEP_PER_TILE` por tile, pero la terraza ya no vale un escalón fijo
/// — `relief::SALTO_DE_TERRAZA` la hace crecer de 1 a 10 según la altura, así que
/// **cruzar el borde de una terraza alta es un farallón de diez pisos y la
/// cascada que cae por ahí baja diez**. El tope es [`PISOS_MAX`], que es el del
/// relieve y no uno propio del agua.
///
/// ## Lo que se probó y no era
///
/// El alto salió durante un tiempo de la ALTURA SOBRE EL MAR del labio
/// (`terraza × relief::STEP_M`, en metros → tiles), con la idea de "montaña más
/// grande, cascada más grande". Medido con
/// `medir_acantilados::corte_de_una_cascada`, eso daba chorros de **6 y de 10
/// filas colgando de desniveles de un solo escalón**: el dibujo no tenía nada
/// que ver con el terreno, empezaba un tile por encima del canto y terminaba en
/// mitad del río. La variedad de tamaño no se perdió al sacarlo — la da el río,
/// que baja la sierra encadenando saltos de un piso separados por rápidos.
///
/// La poda de la poza se queda, y no cuesta casi nada: medida, el agua al pie va
/// de 1 a 20 tiles con **8 de mediana**. Es lo que impide que el chorro se
/// derrame sobre la tierra seca.
fn salto_desde(body: CelestialBody, tx: i64, tz: i64) -> Option<i64> {
    // El labio: el canto de una pared, con agua justo encima.
    if !pared(body, tx, tz) || !cauce(body, tx, tz - 1) {
        return None;
    }
    // **Y tiene que BAJAR hacia el sur.** Sin esto, una pared de las que corren
    // de norte a sur —las que miran al este o al oeste— con el río pasando por
    // sus dos puntas se leía como una cascada: el chorro caería paralelo al
    // muro, atravesando la roca.
    //
    // `alto_de_pared` ya contesta las dos cosas de una: cuánto baja el terreno
    // hacia el sur, y `0` si no baja para ese lado.
    let pisos = relief::alto_de_pared(body, tx, tz);
    if pisos == 0 {
        return None;
    }
    // **La poza va AL PIE DE LA PARED, no debajo del canto.** El muro tapa
    // `pisos + 1` filas, así que el agua de abajo empieza recién después; con
    // paredes de un escalón las dos cuentas daban casi lo mismo y por eso nunca
    // se notó, con una de diez el chorro reventaba diez tiles dentro de la roca.
    let pie = tz + pisos + 1;
    let mut agua = 0i64;
    while agua < filas_de_dibujo(pisos) && cauce(body, tx, pie + agua) {
        agua += 1;
    }
    if agua == 0 {
        // Sin poza el río se corta contra la pared: lo de arriba era un charco
        // colgado, no un salto.
        return None;
    }
    Some(pisos)
}

/// Hay líquido dibujado en la celda (bajío, hondo o cauce de río).
fn mojada(body: CelestialBody, tx: i64, tz: i64) -> bool {
    matches!(
        crate::geography::natural_tile(body, tx, tz),
        Tile::Water | Tile::WaterDeep
    )
}

/// **Por acá pasa agua** — el CAUCE, no el tile pintado.
///
/// Es la pregunta que hay que hacerle a un salto, y la diferencia con [`mojada`]
/// no es cosmética: `geography::natural_tile` **seca** el pie de las paredes
/// (`relief::bajo_el_dibujo_de_una_pared`) y para saber si esa celda está exenta
/// —porque es la garganta de una cascada— nos pregunta a nosotros. Consultarle
/// el tile pintado desde acá sería morderse la cola.
///
/// El cauce es lo que el mundo decide antes de pintar nada: nivel bajo cero (mar
/// o lago) o río (`geography::hay_rio`).
///
/// Es público porque los drivers de medición (`tests/medir_cascadas.rs`,
/// `tests/medir_rios.rs`) tienen que hacer exactamente esta pregunta, y mientras
/// fue privado se la hacían con una copia local — dos definiciones de "por acá
/// pasa agua" que sólo hacía falta que alguien tocara una para que divergieran.
pub fn cauce(body: CelestialBody, tx: i64, tz: i64) -> bool {
    relief::relief_level(body, tx, tz) < 0 || crate::geography::hay_rio(body, tx, tz)
}

/// **El salto que empieza en esta celda**, si esta celda es su esquina noroeste.
///
/// Devuelve **cuántos pisos de meseta baja** (1..=[`PISOS_MAX`]), que es lo que
/// mide el dibujo: un tramo de chorro por piso, más la boca arriba y la poza
/// abajo. Sólo la celda ANCLA responde: las otras dos
/// del ancho y las de más abajo son parte del mismo dibujo, no cascadas nuevas.
/// Así una cascada se spawnea una sola vez y no tres veces solapadas.
pub fn cascada_at(body: CelestialBody, tx: i64, tz: i64) -> Option<i64> {
    let tramo = tramo_de_caida(body, tx, tz)?;
    // La ventana AL RAS que le toca a este tramo: sus tres columnas caen en la
    // misma fila. Sólo su esquina noroeste responde.
    let (vx, vz) = tramo.ventana?;
    if (vx, vz) != (tx, tz) {
        return None;
    }
    // **Manda el tramo que MENOS baja de los tres.** El chorro se dibuja entero
    // de una pieza y las tres columnas comparten una misma boca, así que si una
    // baja dos pisos y su vecina uno, el salto es de uno: el resto del desnivel
    // lo sigue enseñando la pared de terraza.
    (0..ANCHO_CASCADA)
        .map(|d| salto_desde(body, vx + d, vz))
        .try_fold(PISOS_MAX, |acc, a| Some(acc.min(a?)))
}

/// Un tramo de caída: las columnas seguidas de un mismo cauce que cruzan el
/// escalón, con la fila en la que cae cada una y dónde se le dibuja la garganta.
struct TramoDeCaida {
    /// Esquina noroeste de la ventana de tres columnas AL RAS que se dibuja, o
    /// `None` si en todo el tramo no hay tres columnas que caigan en la misma
    /// fila: entonces el desnivel se lee como rápido y no se dibuja garganta.
    ventana: Option<(i64, i64)>,
}

/// **EL TRAMO DE CAÍDA AL QUE PERTENECE ESTA COLUMNA, Y DÓNDE VA SU GARGANTA.**
///
/// # El bug: la deriva se acumulaba y el dibujo no se enteraba
///
/// [`salto_vecino`] tolera un tile de desnivel entre columnas vecinas y **sigue
/// desde la fila nueva**, así que a lo ancho de las tres celdas del recorte la
/// deriva llega a ±2. El dibujo, en cambio, estampaba un rectángulo rígido en la
/// fila del ancla. Medido con `medir_cascadas::encaje_de_la_cascada` sobre
/// 1.800×1.800 tiles: de 81 columnas de chorro sólo **50 caían en la fila donde
/// se dibujaba la boca**, y de las 54 paredes que flanquean una garganta sólo
/// **17** tenían su canto ahí — 10 no tenían pared ninguna en ±2 filas. Eso es
/// el chorro colgado al lado del acantilado en vez de dentro de él.
///
/// # La regla: la garganta se ancla AL RAS o no se ancla
///
/// La tolerancia sigue sirviendo para SEGUIR el cauce —las paredes de terraza
/// corren en diagonal y sin ella el tramo se cortaría cada dos celdas—, pero ya
/// no decide dónde se dibuja. Se reconstruye la fila de cada columna del tramo y
/// se busca una ventana de [`ANCHO_CASCADA`] columnas **que caigan las tres en
/// la misma fila**. Si la hay, ahí va la garganta, con la boca al ras de la
/// meseta de las tres. Si no la hay, el desnivel se queda como pared con agua
/// arriba y abajo, o sea un rápido — que es una lectura correcta del terreno.
///
/// De las ventanas al ras posibles se elige la **más centrada** en el tramo (y a
/// igualdad, la de más al oeste): un dibujo por tramo, como antes, para que un
/// río ancho no salga con columnas de roca alineadas en mitad del agua.
fn tramo_de_caida(body: CelestialBody, tx: i64, tz: i64) -> Option<TramoDeCaida> {
    salto_desde(body, tx, tz)?;
    // **Sin reservar memoria**: esto es camino caliente —lo llaman el spawn de
    // cada chunk, `en_la_garganta`, `bajo_el_chorro` y el autotile del agua— y un
    // `Vec` por consulta se nota. El tramo no puede pasar de [`CAUCE_MAX`]
    // columnas por construcción, así que entra en la pila.
    // La columna que pregunta va en el MEDIO del buffer y el tramo crece a los
    // dos lados desde ahí; por eso mide `2·CAUCE_MAX − 1` aunque el tramo nunca
    // pase de `CAUCE_MAX` columnas. Poner el ancla al final y crecer sólo hacia
    // la derecha fue el primer intento y se salía del buffer en cuanto el cauce
    // seguía al este.
    const ANCLA: usize = (CAUCE_MAX - 1) as usize;
    let mut filas = [0i64; (2 * CAUCE_MAX - 1) as usize];
    filas[ANCLA] = tz;
    let mut izq = 0i64;
    let mut z = tz;
    while izq + 1 < CAUCE_MAX {
        let Some((nz, _)) = salto_vecino(body, tx - izq - 1, z) else { break };
        izq += 1;
        filas[ANCLA - izq as usize] = nz;
        z = nz;
    }
    let mut der = 0i64;
    let mut z = tz;
    while izq + der + 1 < CAUCE_MAX {
        let Some((nz, _)) = salto_vecino(body, tx + der + 1, z) else { break };
        der += 1;
        filas[ANCLA + der as usize] = nz;
        z = nz;
    }
    let filas = &filas[ANCLA - izq as usize..=ANCLA + der as usize];
    // `filas[i]` es la fila de caída de la columna `x0 + i`.
    let x0 = tx - izq;
    let largo = filas.len() as i64;
    // Tres celdas de ancho para el dibujo: un cauce de una o dos celdas es un
    // chorrito, no una cascada, y se deja como está.
    if largo < ANCHO_CASCADA {
        return Some(TramoDeCaida { ventana: None });
    }
    // La ventana al ras más centrada. El centro del tramo en "índice de columna".
    let centro = (largo - ANCHO_CASCADA) as f32 / 2.0;
    let mut mejor: Option<(f32, i64)> = None;
    for i in 0..=(largo - ANCHO_CASCADA) {
        let f = filas[i as usize];
        if (1..ANCHO_CASCADA).any(|d| filas[(i + d) as usize] != f) {
            continue;
        }
        let d = (i as f32 - centro).abs();
        if mejor.is_none_or(|(md, _)| d < md) {
            mejor = Some((d, i));
        }
    }
    Some(TramoDeCaida {
        ventana: mejor.map(|(_, i)| (x0 + i, filas[i as usize])),
    })
}

/// El salto de la columna de al lado, **tolerando un tile de desnivel**.
///
/// Sin esta tolerancia el dibujo casi nunca encontraba dónde anclarse, y no por
/// falta de agua: **las paredes de terraza corren en diagonal**, así que la
/// caída de una columna está una fila más arriba o más abajo que la de su
/// vecina, y pedir tres columnas que empiecen en la MISMA fila es pedir un tramo
/// de muro perfectamente horizontal. Medido: de las columnas de salto que hay en
/// la sierra, con la regla estricta se anclaba una de cada cinco.
///
/// Un tile de diferencia sobre un chorro que ya mide varias filas no se lee como
/// un error; lo que se lee es la cascada que antes no estaba.
fn salto_vecino(body: CelestialBody, tx: i64, tz: i64) -> Option<(i64, i64)> {
    [0i64, 1, -1]
        .into_iter()
        .find_map(|dz| salto_desde(body, tx, tz + dz).map(|a| (tz + dz, a)))
}

/// Cuál de las tres variantes de cascada le toca a este salto. Es del SALTO, no
/// de la celda: las tres piezas apiladas tienen que ser del mismo dibujo.
pub fn variante_cascada(tx: i64, tz: i64) -> u32 {
    hash_cell(0x6a17_0003, tx, tz) % 3
}

// ---------------------------------------------------------------------------
// Qué flota, y por qué está ahí
// ---------------------------------------------------------------------------

/// El sitio que ocupa una celda de líquido. Es lo que decide qué puede haber
/// flotando en ella.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SitioDeAgua {
    /// El agua toca la tierra. Acá crece el junco y se apoya la mata flotante.
    Ribera,
    /// Agua quieta y encerrada: una laguna, un remanso, el charco de un pantano.
    /// Es lo único donde el nenúfar puede prender.
    Remanso,
    /// Cauce de río: hay corriente. Lo que hay acá es lo que la corriente
    /// arrastra o lo que la resiste — troncos, rocas, espuma.
    Corriente,
    /// Agua abierta lejos de la orilla. El pez y la burbuja viven acá; nada
    /// puesto por nadie llega tan lejos.
    Abierto,
}

/// **En qué sitio de agua estás.** `None` si la celda no es líquido.
pub fn sitio_de_agua(body: CelestialBody, tx: i64, tz: i64) -> Option<SitioDeAgua> {
    if !mojada(body, tx, tz) {
        return None;
    }
    // La hondura no cuenta como sitio: lo que se hunde no flota, y un tronco a
    // ocho metros de profundidad es un tronco hundido.
    if relief::relief_level(body, tx, tz) < relief::SWIMMABLE_FLOOR {
        return None;
    }
    let seco = |dx: i64, dz: i64| !mojada(body, tx + dx, tz + dz);
    let toca_tierra = seco(1, 0) || seco(-1, 0) || seco(0, 1) || seco(0, -1);
    if toca_tierra {
        return Some(SitioDeAgua::Ribera);
    }
    // **El ANCHO manda sobre el bioma, y ése fue el ajuste que salió de la
    // captura.** El bioma `River` cubre desde el arroyo que se vadea hasta el
    // estuario de doscientos metros, y preguntándole primero a él, un cuerpo de
    // agua entero salía clasificado "corriente": el mar se llenó de troncos a la
    // deriva de horizonte a horizonte. Si a dos celdas no hay orilla por ningún
    // lado, esto es agua abierta — lo diga el bioma o no.
    let orilla_cerca = (-2..=2).any(|d| seco(d, -2) || seco(d, 2) || seco(-2, d) || seco(2, d));
    if !orilla_cerca {
        return Some(SitioDeAgua::Abierto);
    }
    if biome_at(body, tx, tz) == Biome::River {
        return Some(SitioDeAgua::Corriente);
    }
    // Agua quieta con tierra cerca pero sin tocarla: el interior de una laguna
    // chica, el charco del pantano. Es lo único donde prende el nenúfar.
    Some(SitioDeAgua::Remanso)
}

/// Lo que crece o se apoya en la RIBERA, con el agua tocando la tierra.
///
/// **`agua/junco` NO está**, y no es un olvido: Modern Exteriors no dibujó
/// juncos, y una clave sin pixel dentro de una tabla de pesos no es inofensiva
/// — se lleva su parte de los sorteos y devuelve nada, así que la orilla sale
/// con un tercio menos de cosas de las que dice esta tabla. El slot sigue
/// declarado en `ha_assets::slots` para el día que tenga arte; hasta entonces la
/// vegetación de la orilla es la mata flotante del propio pack, que es
/// exactamente lo que crece en la ribera de la lámina de referencia.
pub const RIBERA: &[ExtraDecor] = &[
    ExtraDecor { key: "agua/mata_flotante", size: [1.0, 1.0], weight: 40 },
    ExtraDecor { key: "tiles/floating_rock_moss", size: [1.0, 1.0], weight: 18 },
    ExtraDecor { key: "agua/nenufar", size: [1.0, 1.0], weight: 16 },
    ExtraDecor { key: "tiles/floating_trunk", size: [3.0, 1.0], weight: 10 },
    ExtraDecor { key: "agua/onda", size: [1.0, 1.0], weight: 10 },
];

/// **El remanso: agua quieta.** Es el único sitio del nenúfar — una planta de
/// hoja flotante no prende en una corriente ni en mar abierto.
pub const REMANSO: &[ExtraDecor] = &[
    ExtraDecor { key: "agua/nenufar", size: [1.0, 1.0], weight: 34 },
    ExtraDecor { key: "agua/hondura", size: [1.0, 1.0], weight: 30 },
    ExtraDecor { key: "agua/onda", size: [1.0, 1.0], weight: 22 },
    ExtraDecor { key: "agua/mata_flotante", size: [1.0, 1.0], weight: 14 },
    ExtraDecor { key: "tiles/water_bubbles", size: [1.0, 1.0], weight: 10 },
    ExtraDecor { key: "tiles/floating_rock", size: [2.0, 2.0], weight: 6 },
];

/// **La corriente arrastra.** Lo que hay en un río es lo que baja con él o lo
/// que se le planta en el medio: troncos, rocas, la espuma que rompe contra
/// ellas. Ni un nenúfar ni una boya.
pub const CORRIENTE: &[ExtraDecor] = &[
    ExtraDecor { key: "agua/onda", size: [1.0, 1.0], weight: 40 },
    ExtraDecor { key: "agua/hondura", size: [1.0, 1.0], weight: 22 },
    ExtraDecor { key: "tiles/floating_rock", size: [2.0, 2.0], weight: 10 },
    ExtraDecor { key: "tiles/floating_rock_moss", size: [1.0, 1.0], weight: 14 },
    ExtraDecor { key: "tiles/floating_trunk", size: [3.0, 1.0], weight: 7 },
];

/// **Agua abierta.** Lejos de la orilla no llega nada que haya puesto nadie: lo
/// que se ve es vida y agua moviéndose.
pub const ABIERTO: &[ExtraDecor] = &[
    ExtraDecor { key: "agua/hondura", size: [1.0, 1.0], weight: 38 },
    ExtraDecor { key: "agua/onda", size: [1.0, 1.0], weight: 26 },
    ExtraDecor { key: "tiles/fishes", size: [1.0, 1.0], weight: 18 },
    ExtraDecor { key: "tiles/water_bubbles", size: [1.0, 1.0], weight: 12 },
    ExtraDecor { key: "tiles/floating_rock_big", size: [4.0, 4.0], weight: 2 },
    ExtraDecor { key: "tiles/floating_rock_big_moss", size: [4.0, 4.0], weight: 2 },
];

/// **Lo que en el agua es TEXTURA y no un objeto.**
///
/// Regla del usuario: *el agua no lleva sombras salvo las de las rocas y las
/// cosas que de verdad están SOBRE ella*. La onda, la hondura del fondo, la
/// burbuja y el cardumen no están encima del agua — **son** el agua, y el
/// nenúfar y la mata flotan planos como una moneda. Todos ellos salían
/// Y-sorteados como si se levantaran del suelo, así que `light2d` les colgaba su
/// mancha de sombra: mar abierto lleno de manchones oscuros sin nada que los
/// proyecte, que es justo lo que se ve en la captura.
///
/// Lo que NO está en esta lista —la roca flotante, el tronco a la deriva— tiene
/// volumen, se levanta sobre el agua y conserva su sombra.
///
/// El cliente lo usa para dibujarlos en la banda de piso
/// (`world2d::spawn_chunk_decor` → `layers::FLOOR`), que además arregla que una
/// onda te tapara los pies al nadar.
pub const TEXTURA_DE_AGUA: &[&str] = &[
    "agua/onda",
    "agua/hondura",
    "agua/nenufar",
    "agua/mata_flotante",
    "tiles/fishes",
    "tiles/water_bubbles",
];

/// ¿Esta clave es textura del agua y no un objeto flotando? Ver [`TEXTURA_DE_AGUA`].
pub fn es_textura_de_agua(key: &str) -> bool {
    TEXTURA_DE_AGUA.contains(&key)
}

/// La tabla que le toca a un sitio.
pub fn tabla_de(sitio: SitioDeAgua) -> &'static [ExtraDecor] {
    match sitio {
        SitioDeAgua::Ribera => RIBERA,
        SitioDeAgua::Remanso => REMANSO,
        SitioDeAgua::Corriente => CORRIENTE,
        SitioDeAgua::Abierto => ABIERTO,
    }
}

/// Cuántas de cada mil celdas de ese sitio llevan algo.
///
/// **Un 10 % no es "un lago lleno de cosas"**, y es el ajuste que salió de mirar
/// la captura del driver: casi todo lo que sortean estas tablas es TEXTURA del
/// agua —la hondura del fondo y la onda—, no objetos. El centro que el pack
/// dibujó para la ribera de pasto es un color plano de un solo tono, así que sin
/// esto el lago tiene orilla preciosa y por dentro es una plancha lisa. Lo que
/// de verdad es un objeto (el tronco, la roca grande) pesa poco dentro de su
/// tabla, así que sigue apareciendo cada muchos metros.
fn densidad(sitio: SitioDeAgua) -> u32 {
    match sitio {
        SitioDeAgua::Ribera => 110,
        SitioDeAgua::Remanso => 120,
        SitioDeAgua::Corriente => 100,
        SitioDeAgua::Abierto => 90,
    }
}

/// **Decoración de una celda de líquido, elegida por el SITIO.**
///
/// Reemplazó al sorteo único que vivía en `worldgen` (una sola bolsa de pesos
/// para todo el líquido del planeta, ya borrada): la clave no sale de una tabla
/// global sino de la que corresponde a dónde está esa celda, así que nada
/// aparece donde no encaja.
pub fn decor_at(body: CelestialBody, tx: i64, tz: i64) -> Option<&'static ExtraDecor> {
    let sitio = sitio_de_agua(body, tx, tz)?;
    // **Ni el labio ni la poza llevan nada encima**: el dibujo del salto ocupa
    // esas dos celdas de agua, y un nenúfar flotando justo donde revienta el
    // chorro es exactamente la clase de cosa que delata que el mundo se sortea.
    // (La celda de la caída en sí es pared, así que ya salió por `mojada`.)
    if pared(body, tx, tz + 1) || pared(body, tx, tz - 1) {
        return None;
    }
    let h = hash_cell(0x1a6e_1001, tx, tz);
    if h % 1000 >= densidad(sitio) {
        return None;
    }
    // **Y tampoco debajo del chorro**, que baja [`filas_de_dibujo`] filas por
    // debajo del canto. Un nenúfar asomando por el medio del agua blanca delata el
    // sorteo igual que uno en la poza. Va DESPUÉS del dado a propósito: sólo lo
    // paga la celda que de verdad iba a llevar algo (una de cada diez).
    if bajo_el_chorro(body, tx, tz) {
        return None;
    }
    crate::worldgen::pick_weighted(tabla_de(sitio), h >> 10)
}

/// **¿Esta celda de pared es la GARGANTA de una cascada?**
///
/// El recorte del pack no es un chorro suelto: es una garganta de tres celdas
/// —roca a los dos costados y el agua en el medio—, así que donde cae una
/// cascada la pared de terraza **ya está dibujada** por ella. Dibujar además la
/// del relieve es pintar dos rocas en la misma celda, y las dos no coinciden:
///
/// - el muro del relieve mide siempre tres celdas de alto y el chorro mide lo
///   que mida la montaña;
/// - y sobre todo, [`salto_vecino`] tolera **un tile de desnivel** entre las
///   tres columnas (sin esa tolerancia se anclaba una cascada de cada cinco,
///   porque las paredes de terraza corren en diagonal). El dibujo, en cambio, es
///   un rectángulo rígido: la columna que arranca una fila más arriba deja su
///   labio de roca asomando por encima del agua.
///
/// Eso es el "la parte de arriba de la cascada se solapa con el borde del
/// acantilado". Se arregla callando al relieve: **manda la garganta**.
///
/// La ventana de `±1` en z es exactamente la tolerancia de `salto_vecino`, y las
/// tres columnas hacia atrás son porque el ancla es la esquina NOROESTE del
/// salto: esta celda puede ser cualquiera de las tres del ancho.
pub fn garganta_de_cascada(body: CelestialBody, tx: i64, tz: i64) -> bool {
    pared(body, tx, tz) && en_la_garganta(body, tx, tz)
}

/// [`garganta_de_cascada`] **sin** exigir que la celda sea pared de terraza.
///
/// Existe por el cierre de esquinas cóncavas (`world2d::spawn_chunk_canto_concavo`):
/// esas celdas son terraza llana por definición —si fueran pared, su pieza la
/// dibujaría el bucle de las caras—, así que la pregunta con `pared` les decía
/// que no y el relieve colaba su canto dentro del recorte del pack. En pantalla
/// eran dos cuadrados de roca sueltos flotando a los costados de la boca del
/// salto.
pub fn en_la_garganta(body: CelestialBody, tx: i64, tz: i64) -> bool {
    for d in 0..ANCHO_CASCADA {
        for dz in -1..=1 {
            if cascada_at(body, tx - d, tz + dz).is_some() {
                return true;
            }
        }
    }
    false
}

/// **¿Esta celda es la BOCA de un salto** — el labio por donde el agua se
/// desborda?
///
/// A diferencia de [`en_la_garganta`], que barre un anillo, ésta es exacta: la
/// celda tiene que estar en la fila del ancla y dentro de sus
/// [`ANCHO_CASCADA`] columnas. Es la que hace falta para responder *"¿el agua de
/// arriba sigue hacia acá?"* sin abrir el chorro entero.
///
/// # Para qué: ahí el agua y la tierra NO ponen su borde
///
/// Regla del usuario, textual: *"es el único lugar por ahora donde la
/// tierra/pasto y el agua no ponen su borde, sino el asset que te mostré, ya que
/// ese es el de la transición que hace que la cascada empiece"*.
///
/// La celda de la boca es acantilado, o sea TIERRA, así que el autotile del río
/// de arriba la contaba como orilla y dibujaba su banda de barro y pasto
/// **cruzada por encima del salto**: el lago terminaba en una ribera y la
/// cascada empezaba debajo, sin que una cosa llevara a la otra. Y la transición
/// ya existe — es la propia boca del recorte del pack, que trae el labio
/// dibujado. Con esto `world2d` cuenta la boca como agua para el autotile y el
/// río desemboca en su salto en vez de terminar contra un banco de pasto.
pub fn boca_de_cascada(body: CelestialBody, tx: i64, tz: i64) -> bool {
    (0..ANCHO_CASCADA).any(|d| cascada_at(body, tx - d, tz).is_some())
}

/// **¿Esta celda es la POZA de un salto** — el reventón del chorro sobre el agua
/// de abajo?
///
/// La simétrica de [`boca_de_cascada`], y **faltaba**. Ésa es la costura que se
/// veía al pie de toda cascada del mundo.
///
/// # El mismo problema que arriba, del otro lado
///
/// La boca tiene su exención desde el 9 ago 2026 y por eso el río de arriba
/// desemboca limpio en el salto. Abajo no había ninguna: las celdas de agua bajo
/// el reventón seguían calculando su autotile normal, y como a los costados de la
/// poza hay tierra —los hombros de la garganta y el pie de la pared—, el agua les
/// pedía `edge_e` / `edge_w` / `corner_*` y **dibujaba su banda de barro y pasto
/// por debajo del sprite de la poza**. En pantalla, el chorro reventaba contra un
/// ribete de orilla en vez de contra su propia agua: el lago terminaba en un
/// borde y la cascada quedaba encima, superpuesta.
///
/// La transición ya existe, igual que arriba: **es la propia poza del recorte del
/// pack**, que trae la espuma y el encuentro con el agua dibujados. Declarándola
/// agua para el autotile, el borde de la orilla se aparta y la poza se funde con
/// el río de abajo.
///
/// # Qué celdas son
///
/// Las [`ANCHO_CASCADA`] columnas del salto, en las [`POZA_FILAS`] filas que
/// ocupa el reventón — que empiezan **una fila por debajo del pie de la pared**:
/// el ancla está en la fila `tz₀`, el chorro baja `pisos` filas, y de ahí cuelga
/// la poza. Es exactamente el rectángulo que estampa
/// `world2d::spawn_chunk_cascadas`, ni una celda más.
///
/// # Barato primero, exacto después
///
/// La pide el autotile del agua, o sea **ocho veces por cada celda de líquido
/// del planeta**. `cascada_at` es de lo más caro que hay en este módulo (rastrea
/// el cauce a lo ancho y le pregunta a cada columna por su salto), así que acá se
/// hace lo mismo que en [`bajo_el_chorro`]: se busca pared de terraza hacia el
/// norte —`relief_cell` va por caché— y sólo cuando aparece una se le pregunta si
/// de ahí cuelga un chorro cuya poza llegue hasta acá. En mar abierto la
/// respuesta sale de doce consultas cacheadas y ni una a la geografía.
pub fn poza_de_cascada(body: CelestialBody, tx: i64, tz: i64) -> bool {
    // La primera fila de poza va justo después de la última de chorro, o sea a
    // `pisos + 1` del ancla; la última, `POZA_FILAS - 1` más abajo.
    for k in 1..=(PISOS_MAX + POZA_FILAS) {
        if !pared(body, tx, tz - k) {
            continue;
        }
        // El ancla es la esquina noroeste del salto: esta columna puede ser
        // cualquiera de las tres del ancho.
        for d in 0..ANCHO_CASCADA {
            let Some(pisos) = cascada_at(body, tx - d, tz - k) else { continue };
            if k > pisos && k <= pisos + POZA_FILAS {
                return true;
            }
        }
    }
    false
}

/// **EL HOMBRO DE UNA GARGANTA: la pared que flanquea el chorro, a los dos
/// lados y de la misma altura.**
///
/// Devuelve `(fila del canto, pisos)` si esta celda es uno de los dos hombros de
/// un salto, o `None`. Los hombros son las columnas `ancla - 1` y
/// `ancla + ANCHO_CASCADA`, y la fila es la del ancla: **la misma** en la que se
/// dibuja la boca.
///
/// # Por qué hace falta preguntarlo, si el relieve ya pone paredes
///
/// Porque no las pone donde el chorro las necesita. El borde de terraza serpea y
/// el chorro se ancla AL RAS de sus tres columnas (`tramo_de_caida`), así que la
/// cuarta columna —la de al lado— cae donde cae. Medido sobre 1.800×1.800 tiles
/// con `medir_cascadas::pared_junto_a_la_garganta`, de los **22 hombros** de los
/// 11 saltos del recorte:
///
/// - **9 no son pared** (`relief_cell` los da por llanos): al lado del chorro no
///   hay roca ninguna y el agua cae pegada al pasto.
/// - de los 13 que sí, **7 tienen su canto una fila más arriba o más abajo**
///   (`encaje_de_la_cascada`: −1 → 4, +1 → 3), o sea la roca empieza y termina
///   desfasada del chorro.
/// - y **3 se dibujan como bordillo** (dos celdas) contra las cuatro filas del
///   salto, así que la pared se corta a media caída.
///
/// Eso es exactamente lo que se ve en las capturas: una banda de acantilado
/// suelta a un costado del salto, más corta que el chorro y a otra altura. La
/// regla que pidió el usuario es que la pared **crezca hacia arriba y hacia
/// abajo hasta cubrir el salto**, en los dos costados y siempre igual.
///
/// # Es DIBUJO, no geometría
///
/// Igual que el cierre de esquinas cóncavas: `relief_cell`, la colisión y la
/// navegación no se enteran. Un hombro no le cierra el paso a nadie que no lo
/// tuviera cerrado ya — sólo hace que la garganta se vea como una garganta.
pub fn hombro_de_garganta(body: CelestialBody, tx: i64, tz: i64) -> Option<(i64, i64)> {
    // A la izquierda del ancla, o a la derecha del ancho. Sólo la fila del
    // ancla: el dibujo del hombro cuelga desde ahí, no se ancla dos veces.
    let pisos = cascada_at(body, tx + 1, tz).or_else(|| cascada_at(body, tx - ANCHO_CASCADA, tz))?;
    Some((tz, pisos))
}

/// **CUÁNTAS filas dibuja un hombro**: el labio en la fila del ancla, un cuerpo
/// por piso y el pie al ras del suelo de abajo.
///
/// Ojo con la unidad, que no es la de [`filas_de_dibujo`]: ésta es un CONTEO de
/// filas y aquélla es *hasta qué fila* llega el chorro contando desde el canto.
/// La última fila del hombro es la `pisos + 1`, y el chorro llega una más abajo
/// —el reventón, que ya va sobre el agua—. Por eso la roca del costado termina
/// justo donde revienta la poza, ni un cuadro antes ni uno después.
pub fn filas_de_hombro(pisos: i64) -> i64 {
    pisos + 2
}

/// ¿Le cae encima un salto a esta celda de agua?
///
/// Barato primero, exacto después: se buscan paredes de terraza hacia el norte
/// —`relief_cell` va por caché— y sólo cuando aparece una se le pregunta a
/// [`cascada_at`] si de ahí cuelga un chorro que llegue hasta acá. En campo
/// abierto la respuesta sale del primer barrido sin tocar la geografía.
fn bajo_el_chorro(body: CelestialBody, tx: i64, tz: i64) -> bool {
    for k in 1..=filas_de_dibujo(PISOS_MAX) {
        if !pared(body, tx, tz - k) {
            continue;
        }
        // El ancla es la esquina noroeste del salto: esta columna puede ser
        // cualquiera de las tres del ancho.
        for d in 0..ANCHO_CASCADA {
            if cascada_at(body, tx - d, tz - k).is_some_and(|pisos| k <= filas_de_dibujo(pisos)) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Ventanas repartidas por el planeta.** Un solo cuadrado alrededor del
    /// origen no sirve para preguntarle nada al mundo: los continentes, las
    /// cuencas y los cauces se miden en miles de tiles, así que 1.200×1.200 en
    /// (0,0) puede caer entero en un secarral y "no hay agua" sería mentira.
    const VENTANAS: &[(i64, i64)] = &[
        (0, 0),
        (2_400, -1_800),
        (-3_100, 900),
        (1_500, 3_300),
        (-2_000, -2_600),
        (4_200, 1_100),
    ];

    /// Un barrido de la Tierra tiene que encontrar los cuatro sitios: si uno no
    /// aparece nunca, su tabla es arte que nadie ve.
    #[test]
    fn los_cuatro_sitios_existen() {
        let body = CelestialBody::Earth;
        let mut vistos = [0u32; 4];
        for &(cx, cz) in VENTANAS {
            for tz in cz - 150..cz + 150 {
            for tx in cx - 150..cx + 150 {
                match sitio_de_agua(body, tx, tz) {
                    Some(SitioDeAgua::Ribera) => vistos[0] += 1,
                    Some(SitioDeAgua::Remanso) => vistos[1] += 1,
                    Some(SitioDeAgua::Corriente) => vistos[2] += 1,
                    Some(SitioDeAgua::Abierto) => vistos[3] += 1,
                    None => {}
                }
            }
            }
        }
        for (i, n) in vistos.iter().enumerate() {
            assert!(*n > 0, "el sitio {i} no aparece en ninguna ventana: {vistos:?}");
        }
    }

    /// El nenúfar no puede caer en una corriente ni en mar abierto, y la boya no
    /// existe fuera de la obra del hombre. Es la regla que se rompía con una
    /// tabla única.
    #[test]
    fn nada_flota_donde_no_corresponde() {
        assert!(!CORRIENTE.iter().any(|d| d.key == "agua/nenufar"));
        assert!(!ABIERTO.iter().any(|d| d.key == "agua/nenufar"));
        for tabla in [RIBERA, REMANSO, CORRIENTE, ABIERTO] {
            assert!(
                !tabla.iter().any(|d| d.key.contains("boya") || d.key.contains("flotador")),
                "el salvavidas es del hombre, no del lago"
            );
        }
    }

    /// Cada familia pide su hoja, y **la orilla pelada ya no es una de ellas**:
    /// `tiles/water_roca` es el agua que toca la roca desnuda y esa es
    /// exactamente la que se sacó (regla del usuario: no hay agua sin césped).
    /// Si alguien la vuelve a pedir, la pide por su nombre y este test lo cuenta.
    #[test]
    fn cada_bioma_su_orilla() {
        assert_eq!(Orilla::Arena.art_key(), "tiles/water_arena");
        assert_eq!(Orilla::Pasto.art_key(), "tiles/water");
        for b in [Biome::Highland, Biome::Mountain, Biome::Snow, Biome::Ice] {
            assert_eq!(
                familia_de_bioma(b),
                Some(Orilla::Pasto),
                "{b:?} volvió a beber contra la piedra pelada"
            );
        }
    }

    /// **Las cascadas existen y son de tres tiles.** Donde un río cruza un
    /// escalón tiene que haber salto; si no aparece ninguno en un barrido serio,
    /// la regla no se está cumpliendo y el arte de cascada no lo ve nadie.
    #[test]
    fn hay_cascadas_donde_el_rio_cruza_un_escalon() {
        let body = CelestialBody::Earth;
        let mut saltos = 0;
        for tz in -900..900 {
            for tx in -900..900 {
                if let Some(pisos) = cascada_at(body, tx, tz) {
                    assert!((1..=PISOS_MAX).contains(&pisos), "salto de {pisos} pisos");
                    saltos += 1;
                }
            }
        }
        assert!(saltos > 0, "ni una cascada en 1.800x1.800 tiles");
    }

    /// **La playa se queda en la playa, y el resto del líquido bebe con pasto.**
    ///
    /// Las dos mitades importan y son la queja que lo motivó:
    ///
    /// - la ribera de pasto tiene que ser lo NORMAL, no la excepción, porque no
    ///   hay agua sin césped alrededor;
    /// - el agua de panal —la de arena— no puede aparecer en la sierra ni en una
    ///   laguna de tierra adentro.
    ///
    /// # El criterio que no medía nada
    ///
    /// "Sierra" se preguntaba con `biome_at` **de la celda mojada**, y una celda
    /// mojada nunca es `Highland` ni `Mountain`: es `River` si es cauce y `Ocean`
    /// si está bajo el nivel del mar. O sea que el contador salía 0 pasara lo que
    /// pasara, y el `assert_eq!(0)` no defendía nada — de hecho no se enteró de
    /// que **todas las lagunas del planeta tenían ribera de arena**, porque un
    /// lago hunde el terreno y quedaba del lado del mar en `orilla_de`.
    ///
    /// Ahora se pregunta por la TIERRA de al lado, que es a quien `orilla_de` le
    /// pregunta de verdad, y se cuenta aparte el agua de tierra adentro.
    #[test]
    fn las_dos_orillas_aparecen_donde_corresponde() {
        let body = CelestialBody::Earth;
        let mut vistas = [0u32; 2];
        let mut arena_en_la_sierra = 0u32;
        let mut arena_en_lago = 0u32;

        /// El bioma de la primera tierra que se encuentra alrededor, buscando en
        /// anillos igual que `orilla_de`.
        fn tierra_de_al_lado(body: CelestialBody, tx: i64, tz: i64) -> Option<Biome> {
            for r in 1..=BUSCA_ORILLA {
                for (dx, dz) in
                    [(r, 0), (-r, 0), (0, r), (0, -r), (r, r), (-r, -r), (r, -r), (-r, r)]
                {
                    let b = biome_at(body, tx + dx, tz + dz);
                    if familia_de_bioma(b).is_some() {
                        return Some(b);
                    }
                }
            }
            None
        }
        // **Ventanas densas repartidas, no un barrido ralo.** La orilla es una
        // banda de UN tile: muestrear cada 37 tiles no la encuentra nunca, y el
        // primer intento de este test "demostró" que no había ni una ribera de
        // pasto en todo el planeta. Se recorre completo, pero sólo dentro de
        // cuadros chicos esparcidos por el mundo.
        //
        // **Y repartidos por TODO el planeta, no por el centro.** Con ±9.000 el
        // barrido no tocaba una sola celda de sierra: las cordilleras de este
        // mapa están más afuera.
        let mut cz = -15_000i64;
        while cz < 15_000 {
            let mut cx = -15_000i64;
            while cx < 15_000 {
                for tz in cz..cz + 70 {
                    for tx in cx..cx + 70 {
                        if !matches!(sitio_de_agua(body, tx, tz), Some(SitioDeAgua::Ribera)) {
                            continue;
                        }
                        let o = orilla_de(body, tx, tz);
                        match o {
                            Orilla::Pasto => vistas[0] += 1,
                            Orilla::Arena => vistas[1] += 1,
                        }
                        let sierra = matches!(
                            tierra_de_al_lado(body, tx, tz),
                            Some(
                                Biome::Highland | Biome::Mountain | Biome::Snow | Biome::Ice
                            )
                        );
                        if sierra && o == Orilla::Arena {
                            arena_en_la_sierra += 1;
                        }
                        // **Un lago no es el mar.** La laguna del bosque y la
                        // cuenca con nombre beben con la ribera del terreno que
                        // las rodea; sólo se les permite arena si de verdad
                        // están en un secarral.
                        if o == Orilla::Arena
                            && crate::world_map::agua_interior(body, tx, tz)
                            && !matches!(
                                tierra_de_al_lado(body, tx, tz),
                                Some(Biome::Desert | Biome::Savanna | Biome::Beach)
                            )
                        {
                            arena_en_lago += 1;
                        }
                    }
                }
                cx += 3_000;
            }
            cz += 3_000;
        }
        assert!(vistas[0] > 0, "no hay ninguna ribera de pasto: {vistas:?}");
        assert!(vistas[1] > 0, "no hay ninguna playa de arena: {vistas:?}");
        assert!(
            vistas[0] > vistas[1],
            "la playa dejó de ser la excepción: {vistas:?} (pasto, arena)"
        );
        assert_eq!(
            arena_en_la_sierra, 0,
            "el agua de panal volvió a la montaña: {arena_en_la_sierra} celdas"
        );
        assert_eq!(
            arena_en_lago, 0,
            "una laguna de tierra adentro salió ribeteada de playa: {arena_en_lago} celdas"
        );
    }

    /// **UNA CASCADA MIDE LO QUE BAJA EL TERRENO.**
    ///
    /// Es la regla que pidió el usuario —*"que el principio de la cascada desde
    /// arriba esté al ras de donde está la meseta"*— convertida en número: el
    /// alto que devuelve [`cascada_at`] tiene que ser exactamente el desnivel
    /// entre la terraza del labio y la del pie, y nunca pasar de [`PISOS_MAX`].
    ///
    /// Reemplaza a `hay_cascadas_de_varios_escalones`, que exigía lo contrario
    /// (chorros de cinco tiles o más colgando de desniveles de un escalón).
    /// Aquella regla venía de "montaña más grande, cascada más grande" y el
    /// tamaño lo sacaba de la altura sobre el mar del labio: en pantalla eso era
    /// una cinta de agua de diez filas al lado de una pared de tres celdas.
    ///
    /// # El desnivel se le pregunta a `alto_de_pared`, no a la resta de niveles
    ///
    /// Esto comparaba `pisos` contra `relief_level(tz−1) − relief_level(tz+1)`, o
    /// sea contra el salto de TERRAZA, que nunca pasa de uno: dos celdas pegadas
    /// no se llevan dos terrazas porque el campo no puede cambiar tanto por tile.
    /// Con eso la regla decía en realidad *"toda cascada mide un piso"*, y pasaba
    /// sólo porque las que caían en la ventana estaban todas en la terraza 1
    /// —donde `SALTO_DE_TERRAZA` vale 1 y los dos números coinciden por
    /// casualidad—. En cuanto los cauces se movieron (14 ago 2026, el tirón del
    /// sumidero) apareció un salto en terraza alta y saltó: *"dibuja 4 pisos y el
    /// terreno baja 1"*. Las dos cosas eran ciertas y la que estaba mal era la
    /// comparación.
    ///
    /// El alto de una pared lo dice **`relief::alto_de_pared`** y nadie más, que
    /// es de donde `salto_desde` lo saca. Es la misma confusión que dejó escrita
    /// la doctrina de "en este mundo no existe la pared de dos escalones".
    #[test]
    fn la_cascada_mide_lo_que_baja_el_terreno() {
        let body = CelestialBody::Earth;
        let mut por_pisos = [0u32; (PISOS_MAX + 1) as usize];
        // Parches repartidos por el planeta entero: las cordilleras no están en
        // el centro del mapa, y muestrear sólo el medio es cómo este archivo se
        // convenció una vez de que no había orillas de piedra.
        let mut cz = -15_000i64;
        while cz < 15_000 {
            let mut cx = -15_000i64;
            while cx < 15_000 {
                for tz in cz..cz + 100 {
                    for tx in cx..cx + 100 {
                        let Some(pisos) = cascada_at(body, tx, tz) else { continue };
                        assert!(
                            (1..=PISOS_MAX).contains(&pisos),
                            "salto de {pisos} pisos en ({tx},{tz})"
                        );
                        por_pisos[pisos as usize] += 1;
                        // **El desnivel real de la columna ancla, en ESCALONES.**
                        // El salto es el mínimo de las tres columnas del ancho
                        // (`cascada_at`), así que la del ancla es una cota
                        // superior: nunca puede dibujar más de lo que ella baja.
                        let baja = relief::alto_de_pared(body, tx, tz);
                        assert!(
                            pisos <= baja,
                            "el salto de ({tx},{tz}) dibuja {pisos} pisos y su pared mide {baja}"
                        );
                    }
                }
                cx += 2_000;
            }
            cz += 2_000;
        }
        eprintln!("cascadas por pisos: {por_pisos:?}");
        assert!(
            por_pisos.iter().sum::<u32>() > 0,
            "ni una cascada en todo el planeta: {por_pisos:?}"
        );
    }

    /// **La variedad de tamaño la da el RÍO, no el dibujo.**
    ///
    /// Al atar el alto al desnivel, todos los saltos del mundo miden un piso
    /// —el terreno no apila escalones— y la pregunta legítima es si el mundo se
    /// quedó sin cascadas grandes. No: un río de montaña baja **encadenando**
    /// saltos separados por tramos de rápidos, que es como baja un río de
    /// verdad. Este test comprueba que la cadena existe; si dejara de existir,
    /// la sierra sería un plano inclinado y el tamaño único sí sería un
    /// problema.
    ///
    /// La ventana no está elegida a ojo: medida con
    /// `medir_acantilados::saltos_encadenados`, de **32 anclas del planeta, 9
    /// tienen otra río abajo** a entre 8 y 40 tiles, con hasta 12 de
    /// desplazamiento lateral (el cauce serpentea mientras baja). La primera
    /// versión de este test pedía ±2 tiles y 24 de caída y por eso contó cero.
    #[test]
    fn hay_cascadas_encadenadas_bajando_la_sierra() {
        let body = CelestialBody::Earth;
        let mut anclas = Vec::new();
        let mut cz = -16_000i64;
        while cz < 16_000 {
            let mut cx = -16_000i64;
            while cx < 16_000 {
                // Sólo alrededor de terreno alto: barrer el planeta entero tile
                // a tile para esto cuesta minutos y las cascadas encadenadas son
                // de sierra por definición.
                if relief::relief_level(body, cx, cz) >= 4 {
                    for tz in cz - 120..cz + 120 {
                        for tx in cx - 120..cx + 120 {
                            if cascada_at(body, tx, tz).is_some() {
                                anclas.push((tx, tz));
                            }
                        }
                    }
                }
                cx += 1_500;
            }
            cz += 1_500;
        }
        let cadenas = anclas
            .iter()
            .filter(|&&(tx, tz)| {
                anclas
                    .iter()
                    .any(|&(ox, oz)| oz > tz && oz - tz <= 90 && (ox - tx).abs() <= 12)
            })
            .count();
        eprintln!("anclas: {} · con otro salto río abajo: {cadenas}", anclas.len());
        assert!(cadenas > 0, "ningún río encadena dos saltos: la sierra se aplanó");
    }

    /// **Ninguna montaña con los pies en el agua.** El borde de terraza que da
    /// al líquido es costa, no farallón: donde el terreno baja hasta el mar,
    /// baja caminando. Lo garantiza `relief::relief_cell`, y se comprueba acá
    /// porque es una regla del AGUA — un peñón en medio del mar se lee como que
    /// se cayó una montaña adentro.
    #[test]
    fn ningun_acantilado_toca_el_agua() {
        let body = CelestialBody::Earth;
        for &(cx, cz) in VENTANAS {
            for tz in cz - 120..cz + 120 {
                for tx in cx - 120..cx + 120 {
                    if !matches!(relief::relief_cell(body, tx, tz), relief::ReliefCell::Cliff) {
                        continue;
                    }
                    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        assert!(
                            relief::relief_level(body, tx + dx, tz + dz) >= 0,
                            "acantilado en ({tx},{tz}) con el agua al lado"
                        );
                    }
                }
            }
        }
    }

    /// **NINGUNA PARED CON LOS PIES EN EL AGUA — ni la primera fila ni las que
    /// cuelgan.**
    ///
    /// `ningun_acantilado_toca_el_agua` sólo miraba los cuatro vecinos, y eso
    /// alcanzaba mientras la pared se dibujara dentro de su celda. No se dibuja
    /// ahí: la cara que mira al sur cuelga tantas filas sobre la terraza de
    /// abajo como escalones mida (`relief::alto_de_pared`, y hasta dónde llega
    /// lo contesta `relief::bajo_el_dibujo_de_una_pared`). Con el río a dos
    /// tiles, la roca salía pintada encima del agua — el *"acantilado flotando
    /// sobre el agua e incoherente"* de la queja.
    ///
    /// La garganta de una cascada está exenta a propósito: ahí el cauce CRUZA el
    /// escalón y el recorte del pack ya trae su propia roca a los dos lados.
    #[test]
    fn ninguna_pared_se_dibuja_sobre_el_agua() {
        let body = CelestialBody::Earth;
        let mut mojadas = Vec::new();
        for &(cx, cz) in VENTANAS {
            for tz in cz - 150..cz + 150 {
                for tx in cx - 150..cx + 150 {
                    if !matches!(relief::relief_cell(body, tx, tz), relief::ReliefCell::Cliff) {
                        continue;
                    }
                    // Sólo la cara que mira al sur cuelga sobre la terraza baja.
                    if relief::relief_level(body, tx, tz + 1) >= relief::relief_level(body, tx, tz) {
                        continue;
                    }
                    if garganta_de_cascada(body, tx, tz) {
                        continue;
                    }
                    // Las filas que tapa el dibujo de ESTA pared: el muro se
                    // retira del agua tanto como mida (`relief::es_canto`), así
                    // que un farallón de diez pisos exige once filas secas.
                    for k in 1..=relief::alto_de_pared(body, tx, tz) + 1 {
                        if mojada(body, tx, tz + k) {
                            mojadas.push((tx, tz, k));
                        }
                    }
                }
            }
        }
        assert!(
            mojadas.is_empty(),
            "{} paredes dibujadas sobre el agua, p.ej. {:?}",
            mojadas.len(),
            &mojadas[..mojadas.len().min(8)]
        );
    }

    /// **Y DEBAJO DEL PIE QUEDA UNA FILA DE LLANO.**
    ///
    /// La otra mitad de la regla, y la que se había perdido. Textual del usuario:
    /// *"siempre tiene que haber un borde donde termina la pared, un borde más de
    /// bloques de llano, que es el borde que da de llano con agua"*. O sea que no
    /// alcanza con que la roca no se dibuje sobre el líquido
    /// ([`ninguna_pared_se_dibuja_sobre_el_agua`], que es lo único que se estaba
    /// comprobando): hace falta **una fila de césped puro** entre el pie de la
    /// roca y la celda mojada, porque la transición agua/tierra se dibuja del
    /// lado del agua y sin esa fila no tiene dónde ir.
    ///
    /// Lo hacía una constante (`SUELO_AL_PIE = 3`) y al reescribirlo en función
    /// del alto real de cada pared se perdió. Medido con
    /// `medir_cascadas::pie_de_pared_contra_agua`: **110 caras de 3.666** tenían
    /// el agua pegada al pie donde la biblia anotaba 0.
    ///
    /// # Las dos que quedan
    ///
    /// Después del arreglo quedan **2 de 3.666** (0,05 %), las dos en paredes
    /// altas —no bordillos— de la ventana (−600..600): `(−367, 494)` con alto 3 y
    /// `(−167, 505)` con alto 2. No se persiguieron: son celdas donde el dibujo
    /// del muro se poda contra un canto de más abajo y la última fila deja de ser
    /// `ReliefCell::Pared`. El tope está escrito con el número medido y no con un
    /// porcentaje, así que si alguien rompe la regla de verdad, esto salta.
    #[test]
    fn queda_una_fila_de_llano_al_pie_de_toda_pared() {
        let body = CelestialBody::Earth;
        let mut pegadas = Vec::new();
        let mut caras = 0usize;
        for &(cx, cz) in VENTANAS {
            for tz in cz - 150..cz + 150 {
                for tx in cx - 150..cx + 150 {
                    if !matches!(relief::relief_cell(body, tx, tz), relief::ReliefCell::Cliff) {
                        continue;
                    }
                    if relief::relief_level(body, tx, tz + 1) >= relief::relief_level(body, tx, tz)
                    {
                        continue;
                    }
                    // La garganta está exenta: ahí el cauce cruza el escalón y el
                    // agua tiene que llegar hasta la roca.
                    if en_la_garganta(body, tx, tz) {
                        continue;
                    }
                    caras += 1;
                    // La primera fila que el dibujo NO tapa. Ésa tiene que ser
                    // tierra, y recién la siguiente puede ser agua.
                    let pie = relief::alto_de_pared(body, tx, tz) + 1;
                    if mojada(body, tx, tz + pie + 1) {
                        pegadas.push((tx, tz));
                    }
                }
            }
        }
        assert!(caras > 0, "el barrido no encontró una sola cara: la regla no probó nada");
        assert!(
            pegadas.len() <= 2,
            "{} caras con el agua pegada al pie (el tope medido es 2), p.ej. {:?}",
            pegadas.len(),
            &pegadas[..pegadas.len().min(8)]
        );
    }

    /// Secar el pie de las paredes no puede dejar el mundo sin ríos: si el
    /// cauce se corta cada vez que pasa cerca de un escalón, lo que se arregló
    /// en la pared se rompió en el río.
    #[test]
    fn el_rio_sigue_corriendo_despues_de_secar_los_pies() {
        let body = CelestialBody::Earth;
        let mut cauce = 0u32;
        for &(cx, cz) in VENTANAS {
            for tz in cz - 120..cz + 120 {
                for tx in cx - 120..cx + 120 {
                    if crate::geography::biome_at(body, tx, tz) == Biome::River
                        && mojada(body, tx, tz)
                    {
                        cauce += 1;
                    }
                }
            }
        }
        eprintln!("celdas de cauce con agua: {cauce}");
        assert!(cauce > 1_000, "el río se secó: sólo {cauce} celdas de cauce con agua");
    }

    /// **LA GARGANTA SE ANCLA AL RAS DE SUS TRES COLUMNAS.**
    ///
    /// El dibujo es un rectángulo rígido de [`ANCHO_CASCADA`] celdas que se
    /// estampa en la fila del ancla, así que las tres columnas del salto tienen
    /// que caer EN esa fila. [`salto_vecino`] tolera un tile por columna y va
    /// acumulando, así que sin este guardián la deriva llega a ±2 y el chorro
    /// cuelga al lado del acantilado en vez de dentro de él: medido antes del
    /// arreglo, de 81 columnas sólo 50 caían donde se dibujaba la boca.
    ///
    /// **El barrido son las [`VENTANAS`], no un cuadrado en el origen.** Miraba
    /// 800×800 alrededor de (0,0) y ahí había cascadas por casualidad: en cuanto
    /// los cauces se movieron (14 ago 2026, el tirón del sumidero) ese cuadro se
    /// quedó sin ninguna y el test se cayó por "no se está midiendo nada", que es
    /// justo lo que el `assert` final existe para avisar. Es la lección que este
    /// archivo ya tiene escrita tres veces: un cuadrado en el origen no le
    /// pregunta nada al mundo.
    /// **Y NI SIQUIERA LAS VENTANAS ALCANZAN: SE BUSCA LA SIERRA.** (14 ago 2026,
    /// segunda vez.) Desde que los cauces se trazan de verdad
    /// (`crate::rios`), el río nace en la ladera alta y baja por la pendiente de
    /// la plataforma, así que las cascadas se concentran donde tienen que estar
    /// —en la montaña— y las seis ventanas de [`VENTANAS`], que son casi todas
    /// llanura, se quedaron otra vez sin ninguna. Medido con
    /// `medir_cascadas::material_de_las_cascadas`, en la sierra hay 22 anclas en
    /// 1.800×1.800.
    ///
    /// La salida no es mover las ventanas a mano —esa lección ya está escrita
    /// arriba— sino **buscar** la primera sierra de verdad, igual que hace
    /// `medir_cascadas`. Así el test sigue midiendo aunque el relieve se mueva.
    #[test]
    fn la_garganta_cae_al_ras_de_sus_tres_columnas() {
        let body = CelestialBody::Earth;
        let mut anclas = 0;
        // Se BARRE hasta encontrar cascadas de verdad, igual que
        // `medir_cascadas::material_de_las_cascadas`: primero las ventanas de
        // siempre, y si no aparece ninguna, sierra adentro hasta juntar diez.
        let mut sitios: Vec<(i64, i64)> = VENTANAS.to_vec();
        'busca: for cz in (-16_000..16_000).step_by(900) {
            for cx in (-16_000..16_000).step_by(900) {
                if crate::relief::relief_level(body, cx, cz) >= 5 {
                    sitios.push((cx, cz));
                    if sitios.len() >= VENTANAS.len() + 40 {
                        break 'busca;
                    }
                }
            }
        }
        for (cx, cz) in sitios {
            for tz in cz - 250..cz + 250 {
                for tx in cx - 250..cx + 250 {
                    let Some(pisos) = cascada_at(body, tx, tz) else { continue };
                    anclas += 1;
                    for d in 0..ANCHO_CASCADA {
                        let propio = salto_desde(body, tx + d, tz);
                        assert!(
                            propio.is_some(),
                            "la columna {d} del salto de ({tx},{tz}) no cae en la fila {tz}: \
                             el dibujo quedaría colgado fuera del escalón"
                        );
                        assert!(
                            propio.is_some_and(|p| p >= pisos),
                            "la columna {d} de ({tx},{tz}) baja menos que el salto ({pisos} pisos)"
                        );
                    }
                }
            }
            if anclas >= 10 {
                break;
            }
        }
        assert!(anclas > 0, "ni una cascada en el barrido: no se está midiendo nada");
    }

    /// Una cascada se ancla una sola vez: si la celda de al lado también fuera
    /// ancla, se dibujarían tres chorros solapados.
    #[test]
    fn una_cascada_no_se_ancla_dos_veces() {
        let body = CelestialBody::Earth;
        for tz in -400..400 {
            for tx in -400..400 {
                if cascada_at(body, tx, tz).is_some() {
                    for d in 1..ANCHO_CASCADA {
                        assert!(
                            cascada_at(body, tx + d, tz).is_none(),
                            "dos anclas pegadas en ({tx},{tz})"
                        );
                    }
                }
            }
        }
    }
}
