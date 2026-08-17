//! **BARRIOS** — parcelas de tamaño real, no un rectángulo por manzana.
//!
//! Lo que había: `building_in_block` plantaba **un** rectángulo de madera por
//! manzana, del mismo tamaño aproximado en todas partes y con el 82 % de las
//! manzanas ocupadas. Desde el aire eso es un tablero de ajedrez, y de cerca son
//! cien cajas idénticas. No es una ciudad; es papel pintado con esquinas.
//!
//! Lo que hay ahora:
//!
//! ```text
//!  ┌──────────────────┐   parcela residencial (18×14)
//!  │ ▓▓▓▓▓  ╔═════╗   │   ▓ entrada de auto (asfalto)
//!  │ ▓▓▓▓▓  ║ casa ║   │   ╔═╗ paredes + piso + puerta + ventanas
//!  │ ╔════╗ ║      ║   │   garaje pegado a la entrada
//!  │ ║gara║ ╚══D══╝   │
//!  │ ╚════╝  ░░░ ~~~  │   ░ patio de piedra (parrilla)
//!  │         ░░░ ~~~  │   ~ PILETA — y como el bajío se nada, se nada de verdad
//!  └──────────────────┘   perímetro: cerca con portón al frente
//! ```
//!
//! Tres ideas y de ahí sale todo:
//!
//! 1. **Distritos.** El centro tiene torres, el anillo tiene comercio, las
//!    afueras casas, y hay parques y equipamiento repartidos. Una ciudad se
//!    reconoce por cómo cambia al recorrerla.
//! 2. **Cada programa tiene SU tamaño.** Una casa mide 14×10 y una universidad
//!    56×40. Los programas grandes **reservan varias manzanas** en vez de
//!    recortarse — un hospital a medias no es un hospital.
//! 3. **Variedad por hashes independientes.** El programa, la orientación, el
//!    tamaño de la casa dentro del lote y si tiene garaje, pileta o parrilla salen
//!    de sorteos distintos. Dos manzanas seguidas no se parecen.

use crate::celestial_bodies::{body_terrain_seed, CelestialBody};
use crate::geography::{hash2, CityStyle, Settlement, SettlementTier};
use crate::roads::{block_size, street_class};
use crate::topdown::Tile;

/// El carácter de una zona de la ciudad.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum District {
    /// El centro: torres y oficinas, manzanas macizas.
    Downtown,
    /// Comercio y servicios: tiendas, mercados, estacionamientos.
    Commercial,
    /// Casas con jardín. La mayor parte de cualquier ciudad.
    Residential,
    /// Naves, depósitos, obra. En el borde.
    Industrial,
    /// Plazas, canchas, arboledas. El respiro.
    Park,
    /// Equipamiento GRANDE: hospital, universidad, comisaría, estación.
    Civic,
}

/// Un programa de parcela: qué se construye y **cuánto mide**.
pub struct LotProgram {
    /// Clave lógica del arte (`building/hospital`, …).
    pub key: &'static str,
    /// Tamaño en TILES. Es el campo que importa: no todo mide lo mismo.
    pub cells: (i64, i64),
    pub district: District,
    pub weight: u8,
}

/// Qué fracción del lote residencial ocupa la CASA, en porcentaje de cada lado.
/// El resto es jardín, entrada de auto y pileta. Lo usan `residential_tile`,
/// [`roofed_rect`] y [`LotProgram::huella`], que **tienen que dar lo mismo** o el
/// techo se dibuja donde no está la casa.
const CASA_DEL_LOTE: (i64, i64) = (55, 50);

impl LotProgram {
    /// **La huella EDIFICADA de esta parcela, en tiles** — lo que de verdad se
    /// cubre con un edificio, no el lote entero.
    ///
    /// Es la cuenta que hay que hacer antes de elegirle una fachada: para un lote
    /// residencial de 16×12 la casa mide 8×6, así que buscarle un edificio de
    /// 16×12 trae uno del doble de grande que el hueco donde va a entrar. Era uno
    /// de los cuatro motivos por los que el barrio se veía como cajas.
    ///
    /// Vive acá y no en el importador para que haya UNA sola cuenta: la consultan
    /// `scripts/import-edificios.py` (leyéndola de este archivo), [`roofed_rect`]
    /// y el test `ningun_lote_de_la_ciudad_se_dibuja_como_una_caja`.
    pub fn huella(&self) -> (i64, i64) {
        self.huella_de(self.cells.0, self.cells.1)
    }

    /// La huella edificada de un lote REAL de `w × d` tiles.
    ///
    /// No es lo mismo que [`Self::huella`] y la diferencia importa: `lot_at`
    /// **recorta** el lote a lo que da la manzana (`p.cells.min(nx * block - …)`),
    /// así que una parcela de 42 tiles declarados puede existir de 39. La casa
    /// tiene que salir del lote que EXISTE o se dibuja saliéndose de su terreno —
    /// y peor, `residential_tile` y `roofed_rect` dejarían de coincidir, que es lo
    /// que pone el techo donde no está la casa.
    pub fn huella_de(&self, w: i64, d: i64) -> (i64, i64) {
        if self.district == District::Residential {
            (
                (w * CASA_DEL_LOTE.0 / 100).clamp(6, (w - 4).max(6)),
                (d * CASA_DEL_LOTE.1 / 100).clamp(5, (d - 5).max(5)),
            )
        } else {
            ((w - 2).max(1), (d - 2).max(1))
        }
    }
}

/// El catálogo. Los tamaños salen de lo que el edificio ES: una casa de pueblo
/// entra en media manzana, un hospital necesita cuatro.
pub const LOT_PROGRAMS: &[LotProgram] = &[
    // ── Residencial: la variedad vive acá, porque es el 60 % de la ciudad.
    //
    // **Las parcelas miden lo que miden las CASAS del pack, no al revés.** La
    // casa ocupa el 55 % del ancho del lote (`residential_tile`), y las casas
    // que LimeZu dibujó miden de 9 a 23 tiles de ancho — o sea que un lote de
    // 12×10 dejaba un hueco de 6 tiles para una casa de 15. Con `world_sprite`
    // estirando el dibujo al hueco, el barrio entero salía aplastado a la mitad
    // de su alto. Ahora el lote se agranda hasta que la casa entre entera y el
    // dibujo se apoya en el frente sin deformarse (`building::roof_layout`).
    //
    // La otra mitad del arreglo es que cada uno de estos slots tiene ahora
    // ~10 fachadas distintas del pack (`scripts/import-edificios.py`) y la
    // parcela elige la suya por su esquina: el barrio dejó de ser una casa
    // fotocopiada 200 veces.
    LotProgram { key: "building/casa_chica", cells: (17, 14), district: District::Residential, weight: 22 },
    LotProgram { key: "building/casa_mediana", cells: (26, 18), district: District::Residential, weight: 26 },
    LotProgram { key: "building/casa_grande", cells: (31, 22), district: District::Residential, weight: 16 },
    LotProgram { key: "building/casa_country", cells: (42, 24), district: District::Residential, weight: 10 },
    // **`building/villa`, no `building/villa_6`.** `villa_6` es el nombre que el
    // importador masivo le puso a un recorte de UNA CELDA de la hoja de villas
    // (un felpudo, una maceta): la parcela más grande del barrio apuntaba a un
    // sprite de 32×32 px estirado a 30×22 tiles. `building/villa` es el slot
    // curado que recibe las villas de verdad.
    LotProgram { key: "building/villa", cells: (35, 34), district: District::Residential, weight: 6 },
    LotProgram { key: "building/duplex", cells: (26, 24), district: District::Residential, weight: 12 },
    // ── Comercio. **Los rubros son de verdad**, no un `building/tienda`
    // genérico repetido: el pack dibujó dieciocho locales distintos con su
    // vidriera y su cartel (`edificio/tienda_*`, `cartel/*`) y la ciudad los
    // ignoraba, así que una cuadra comercial eran doce veces la misma tienda.
    // Los pesos son bajos y parejos a propósito — lo que hace que una avenida
    // se lea como avenida es que los locales SEAN distintos entre sí.
    // **Las claves apuntan al arte que EXISTE**, no a nombres bonitos. El pack
    // ya está importado (2.878 objetos) y el importador los nombró como los
    // nombró el pack: hay `edificio/tienda_panaderia` con su vidriera, y NO hay
    // ninguna `building/panaderia`. Inventar la clave linda deja el local
    // dibujado como un cuadrado de color plano, que es peor que la tienda
    // genérica que había. Es la regla 7 leída al derecho: ante la duda, se
    // adapta el juego a los assets.
    //
    // **Y "que exista" no alcanza: tiene que LLENAR la parcela.** Apuntar a una
    // clave del pack que existe pero mide una celda deja el mismo solar vacío que
    // no apuntar a nada — es lo que pasaba con `building/almacen_1` (1×2 celdas en
    // una parcela de 14 tiles), `building/heladeria_1` (2×3),
    // `building/escuela_patio_tubo_modular_18` (1×2) y el cartel de precios de la
    // estación de servicio, que era literalmente un CARTEL haciendo de edificio.
    // Los que tenían edificio de verdad en otra clave se repuntaron; a los demás
    // se les achicó la parcela hasta el talle del dibujo. Lo cuida
    // `art2d::ningun_lote_de_la_ciudad_se_dibuja_como_una_caja`.
    LotProgram { key: "edificio/tienda_panaderia", cells: (14, 12), district: District::Commercial, weight: 8 },
    LotProgram { key: "edificio/tienda_heladeria", cells: (14, 12), district: District::Commercial, weight: 7 },
    LotProgram { key: "edificio/tienda_carniceria", cells: (14, 12), district: District::Commercial, weight: 6 },
    LotProgram { key: "edificio/tienda_juguetes", cells: (14, 12), district: District::Commercial, weight: 5 },
    LotProgram { key: "edificio/tienda_mascotas", cells: (14, 12), district: District::Commercial, weight: 5 },
    LotProgram { key: "edificio/tienda_gimnasio", cells: (18, 14), district: District::Commercial, weight: 5 },
    LotProgram { key: "edificio/tienda_discos", cells: (14, 12), district: District::Commercial, weight: 4 },
    LotProgram { key: "edificio/tienda_pesca", cells: (14, 12), district: District::Commercial, weight: 4 },
    LotProgram { key: "edificio/tienda_galletas", cells: (14, 12), district: District::Commercial, weight: 4 },
    LotProgram { key: "edificio/tienda_vidriera", cells: (14, 12), district: District::Commercial, weight: 5 },
    LotProgram { key: "building/heladeria_1", cells: (7, 6), district: District::Commercial, weight: 5 },
    LotProgram { key: "building/tienda", cells: (19, 13), district: District::Commercial, weight: 5 },
    LotProgram { key: "building/mercado_chico_1", cells: (14, 12), district: District::Commercial, weight: 8 },
    LotProgram { key: "building/mercado_mediano_1", cells: (18, 14), district: District::Commercial, weight: 8 },
    LotProgram { key: "building/gasolinera", cells: (18, 14), district: District::Commercial, weight: 7 },
    LotProgram { key: "building/shopping", cells: (19, 13), district: District::Commercial, weight: 5 },
    LotProgram { key: "lot/estacionamiento", cells: (20, 16), district: District::Commercial, weight: 10 },
    // **LOS FOOD TRUCKS Y LOS PUESTOS DE FERIA.** Otro de los que figuraba como
    // "no está en el manifest": el pack trae el tuk-tuk de pizza entero
    // (`vehicle/pizza_tuk_tuk_*`, 6×6 celdas) y los puestos de feria de la
    // escuela (`building/puestos_*`). No hay ninguna PIZZERÍA dibujada en este
    // pack —ni ningún lavadero—, así que la pizza entra por donde el pack sí la
    // dibujó: sobre ruedas. Es la regla 7 al derecho: se adapta el juego al
    // asset, no al revés.
    LotProgram { key: "vehicle/pizza_tuk_tuk_tuk_tuk_1", cells: (9, 9), district: District::Commercial, weight: 6 },
    LotProgram { key: "building/puestos_1", cells: (7, 6), district: District::Commercial, weight: 7 },
    // ── Centro.
    LotProgram { key: "building/oficina", cells: (17, 19), district: District::Downtown, weight: 22 },
    LotProgram { key: "building/torre", cells: (30, 28), district: District::Downtown, weight: 18 },
    LotProgram { key: "building/hotel", cells: (28, 22), district: District::Downtown, weight: 8 },
    // La torre del reloj de la estación: el pack la dibujó entera y es el hito
    // que se ve de lejos en las referencias.
    LotProgram { key: "building/reloj_torre", cells: (14, 20), district: District::Downtown, weight: 5 },
    // ── Industria. La central eléctrica con sus molinos es de las referencias:
    // va en el borde, que es donde el pack la dibujó (contra la arboleda).
    LotProgram { key: "building/nave_industrial", cells: (18, 16), district: District::Industrial, weight: 20 },
    LotProgram { key: "building/deposito", cells: (13, 14), district: District::Industrial, weight: 14 },
    LotProgram { key: "building/granero", cells: (10, 10), district: District::Industrial, weight: 8 },
    LotProgram { key: "building/hangar", cells: (30, 24), district: District::Industrial, weight: 5 },
    // Las casas rodantes del camping: el pack las trae en tres tamaños y son la
    // escena del borde en las referencias (camping contra la arboleda).
    LotProgram { key: "building/casa_rodante_grande_1", cells: (16, 12), district: District::Industrial, weight: 6 },
    // **LA CENTRAL ELÉCTRICA CON SUS MOLINOS.** Estaba anotada como pendiente
    // "porque no está en el manifest con ningún nombre" — y sí estaba, con el
    // nombre que le puso el importador masivo: la subestación que `City_Props`
    // dibujó entera (7×8 celdas, con sus transformadores y su escalera) se llama
    // `building/energia_casa`, y el molino animado de cuatro frames
    // `anim/additional_houses_wind_mill`. Lo que faltaba no era el pixel: era el
    // lote que los planta. Van los dos en industria y con pesos parecidos, así
    // que la central sale rodeada de molinos como en la referencia.
    LotProgram { key: "building/energia_casa", cells: (11, 12), district: District::Industrial, weight: 8 },
    LotProgram { key: "anim/additional_houses_wind_mill", cells: (7, 11), district: District::Industrial, weight: 10 },
    // ── Parque. Los juegos y la pileta son los que hacen que un parque se
    // JUEGUE en vez de sólo verse verde — y los dos están dibujados enteros.
    LotProgram { key: "lot/plaza", cells: (26, 22), district: District::Park, weight: 18 },
    LotProgram { key: "lot/cancha_basket", cells: (22, 14), district: District::Park, weight: 14 },
    LotProgram { key: "lot/cancha_tenis", cells: (26, 14), district: District::Park, weight: 9 },
    LotProgram { key: "lot/arboleda", cells: (24, 20), district: District::Park, weight: 16 },
    // El parque de juegos, con las piezas que el pack dibujó: el patio de
    // juguetes, el tubo modular de la escuela y las hamacas de las villas.
    LotProgram { key: "building/patio_juguete_1", cells: (9, 8), district: District::Park, weight: 13 },
    LotProgram { key: "building/villa_patio_juguete_hamaca_1", cells: (9, 8), district: District::Park, weight: 9 },
    // **EL NATATORIO.** No lleva fachada y no la necesita: es agua de verdad
    // —`Tile::ShallowWater`, o sea que se NADA— con su andén de baldosa y sus
    // piezas de borde. El pack dibujó 48 recortes de pileta y 39 props
    // (`swimming_pool`) y ninguno es un edificio, así que declararlo como
    // `building/*` habría sido pedirle al pack algo que no dibujó. Ver
    // `park_tile`.
    LotProgram { key: "lot/natatorio", cells: (26, 20), district: District::Park, weight: 10 },
    // El puesto de limonada de las villas: chiquito, dibujado entero, y es lo
    // que hace que una plaza se vea usada.
    LotProgram { key: "building/limonada_puesto_1", cells: (6, 6), district: District::Park, weight: 8 },
    // ── Cívico: LOS GRANDES. Ocupan varias manzanas a propósito.
    LotProgram { key: "building/hospital", cells: (28, 22), district: District::Civic, weight: 14 },
    LotProgram { key: "building/universidad", cells: (44, 32), district: District::Civic, weight: 10 },
    LotProgram { key: "building/escuela", cells: (38, 28), district: District::Civic, weight: 16 },
    LotProgram { key: "building/comisaria", cells: (24, 20), district: District::Civic, weight: 14 },
    LotProgram { key: "building/bomberos", cells: (12, 10), district: District::Civic, weight: 12 },
    LotProgram { key: "building/correo_edificio", cells: (20, 16), district: District::Civic, weight: 10 },
    LotProgram { key: "building/estacion_tren", cells: (22, 16), district: District::Civic, weight: 6 },
];

/// El distrito de una manzana.
///
/// Se decide por ANILLO (distancia al centro en manzanas) más un sorteo, para que
/// haya parques y equipamiento salpicados en vez de tres coronas perfectas. La
/// trama de la ciudad inclina el reparto: una ciudad moderna es más centro, un
/// puerto es más industria.
pub fn district_at(body: CelestialBody, s: &Settlement, bx: i64, bz: i64) -> District {
    // **Lo que pintaste manda.** Si una zona del editor de mundo cubre el centro
    // de esta manzana, su carácter reemplaza al anillo — vos repartís los
    // barrios, el generador reparte lo que va dentro de cada uno
    // (`ha_shared::zonas`). Sin zonas autoradas esto es una carga atómica y sigue
    // de largo, así que el mundo por defecto no paga nada.
    let block = block_size(s.style, s.tier);
    let centro = (
        s.center.0 + bx * block + block / 2,
        s.center.1 + bz * block + block / 2,
    );
    if let Some(c) = crate::zonas::clase_en(body as u8, centro.0, centro.1) {
        return c.district();
    }
    let h = hash2(body_terrain_seed(body) ^ 0xd157_0000, s.center.0 + bx, s.center.1 + bz);
    let ring = bx.abs().max(bz.abs());
    // Parques y equipamiento: repartidos por toda la ciudad, no en un anillo.
    match h % 100 {
        0..=6 => return District::Park,
        7..=11 => return District::Civic,
        _ => {}
    }
    let core = match s.style {
        CityStyle::Modern => 4,
        CityStyle::Radial => 3,
        CityStyle::Grid => 3,
        CityStyle::Port => 2,
        CityStyle::Rural => 0,
    };
    if s.tier != SettlementTier::City {
        // Un pueblo no tiene centro financiero: tiene una plaza y casas.
        return if ring == 0 { District::Commercial } else { District::Residential };
    }
    if ring <= core {
        District::Downtown
    } else if ring <= core + 3 {
        District::Commercial
    } else if ring >= core + 9 && h % 7 == 0 {
        // La industria vive en el borde, y sólo en parte de él.
        District::Industrial
    } else {
        District::Residential
    }
}

/// Un lote colocado: el rectángulo que ocupa y con qué frente.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lot {
    pub min: (i64, i64),
    pub w: i64,
    pub h: i64,
    /// Índice en [`LOT_PROGRAMS`].
    pub program: usize,
}

impl Lot {
    pub fn program(&self) -> &'static LotProgram {
        &LOT_PROGRAMS[self.program]
    }

    pub fn contains(&self, tx: i64, tz: i64) -> bool {
        tx >= self.min.0 && tx < self.min.0 + self.w && tz >= self.min.1 && tz < self.min.1 + self.h
    }

    /// ¿Se pisan los dos rectángulos? Un lote chico que se solapa con uno grande
    /// nunca se dibuja entero —gana el grande, que se prueba antes— y lo que sale
    /// es media casa cortada por una pared ajena.
    fn overlaps(&self, other: &Lot) -> bool {
        self.min.0 < other.min.0 + other.w
            && other.min.0 < self.min.0 + self.w
            && self.min.1 < other.min.1 + other.h
            && other.min.1 < self.min.1 + self.h
    }
}

/// Cuántas manzanas de `block` tiles necesita un programa.
fn blocks_needed(p: &LotProgram, block: i64, margin: i64) -> (i64, i64) {
    let usable = (block - margin * 2).max(1);
    // Un lote de N manzanas dispone de `N*block - 2*margin` tiles: las calles
    // interiores desaparecen bajo un edificio grande (una universidad no tiene una
    // calle cruzándola por el medio).
    let need = |want: i64| {
        let mut n = 1;
        while n * block - margin * 2 < want && n < 4 {
            n += 1;
        }
        n
    };
    let _ = usable;
    (need(p.cells.0), need(p.cells.1))
}

/// Margen libre que hay que dejar alrededor de un lote de `nx × nz` manzanas
/// anclado en `(ax, az)`: la acera de la calle MÁS ANCHA que lo rodea.
///
/// Se mira el borde entero (las cuatro calles que lo limitan) porque basta con
/// que una sea avenida para que el lote tenga que retirarse de ella.
fn bounding_margin(style: CityStyle, ax: i64, az: i64, nx: i64, nz: i64) -> i64 {
    let mut m = 0.0f32;
    for i in 0..=nx {
        m = m.max(street_class(style, ax + i, az).half_shoulder());
        m = m.max(street_class(style, ax + i, az + nz).half_shoulder());
    }
    for j in 0..=nz {
        m = m.max(street_class(style, ax, az + j).half_shoulder());
        m = m.max(street_class(style, ax + nx, az + j).half_shoulder());
    }
    m.ceil() as i64
}

/// El programa que le toca al bloque ANCLA `(ax, az)`, si su sorteo eligió uno
/// que ocupa exactamente `(nx, nz)` manzanas.
fn program_at_anchor(
    body: CelestialBody,
    s: &Settlement,
    ax: i64,
    az: i64,
    nx: i64,
    nz: i64,
    block: i64,
    margin: i64,
) -> Option<usize> {
    let district = district_at(body, s, ax, az);
    let h = hash2(body_terrain_seed(body) ^ 0x10c5_0000, s.center.0 + ax, s.center.1 + az);
    // Densidad: la ciudad tiene casi todo edificado; la aldea, cuatro casas.
    let filled = match s.tier {
        SettlementTier::City => 88,
        SettlementTier::Town => 62,
        SettlementTier::Hamlet => 34,
    };
    if h % 100 >= filled {
        return None;
    }
    // **Sorteo ponderado entre los programas del distrito QUE OCUPAN EXACTAMENTE
    // este tamaño.** Que el tamaño case es lo que hace que una manzana pertenezca
    // a un único lote y no haya dos programas peleándose por ella.
    //
    // Antes se sorteaba entre TODOS los del distrito y se descartaba el resultado
    // si no cabía, que es rechazo puro: en el centro, donde más de la mitad de los
    // programas piden dos manzanas, más de la mitad de las manzanas sueltas se
    // quedaban sin nada. Sorteando ya dentro de los que caben, cada manzana recibe
    // lo más grande que le entra y la densidad la decide `filled`, que es donde
    // tiene que decidirse.
    //
    // Dos pasadas sobre la tabla en vez de juntar los candidatos en un `Vec`:
    // esto corre hasta doce veces POR TILE (una por tamaño de lote candidato) y
    // una reserva de heap por llamada se notaba al pintar un chunk de ciudad.
    let cabe = |p: &LotProgram| p.district == district && blocks_needed(p, block, margin) == (nx, nz);
    let total: u32 = LOT_PROGRAMS.iter().filter(|p| cabe(p)).map(|p| p.weight as u32).sum();
    if total == 0 {
        // **Una manzana suelta nunca se queda estéril.** Puede no haber NINGÚN
        // programa del distrito que quepa en una manzana: el centro de un puerto
        // tiene manzanas de 34 y su programa más chico mide 22, así que todos
        // pedían dos y las manzanas sueltas quedaban vacías — barrios enteros de
        // calles perfectas alrededor de nada. Cuando pasa, se planta el más chico
        // del distrito y el rectángulo se recorta a la manzana (`w`/`hgt` ya se
        // acotan): una oficina algo más angosta sigue siendo una oficina.
        if (nx, nz) != (1, 1) {
            return None;
        }
        return LOT_PROGRAMS
            .iter()
            .enumerate()
            .filter(|(_, p)| p.district == district)
            .min_by_key(|(_, p)| p.cells.0 * p.cells.1)
            .map(|(i, _)| i);
    }
    let mut roll = (h >> 9) % total;
    for (i, p) in LOT_PROGRAMS.iter().enumerate() {
        if !cabe(p) {
            continue;
        }
        let w = p.weight as u32;
        if roll < w {
            return Some(i);
        }
        roll -= w;
    }
    None
}

/// El lote que ocupa esta celda, si hay alguno.
///
/// Se prueba de MAYOR a menor: primero si un lote de 4×4 manzanas anclado más al
/// noroeste llega hasta acá, después 2×2, y así hasta 1×1. Ese orden es lo que
/// permite que una universidad se plante entera en vez de recortarse contra el
/// borde de su manzana.
///
/// **Reservar no es ocupar.** Un hospital reserva 2×2 manzanas y llena una: las
/// otras tres siguen disponibles y reciben su propio lote chico, mientras no se
/// pisen con él. Antes quedaban baldías, y como los programas grandes son la
/// mayoría del centro, el centro de toda ciudad era tres cuartos de descampado
/// con un edificio en una esquina.
/// **Lo más chico que puede medir un lado de parcela.**
///
/// No es un número de gusto: la parcela residencial lleva cerca perimetral y
/// dentro una casa con su fondo (`residential_tile`), y por debajo de diez tiles
/// eso no entra —la casa salía con la profundidad en negativo—. Lo tocó el
/// retiro por desnivel: hasta entonces ningún lote bajaba de la manzana entera y
/// el suelo de 4 que había escrito no lo alcanzaba nunca nadie.
const LADO_MINIMO: i64 = 10;

/// Qué le pasa a un rectángulo de parcela cuando se lo apoya en el terreno.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Retiro {
    /// Hay líquido dentro: acá no se construye y punto.
    Mojado,
    /// Lo cruza un escalón y no queda parte llana que sirva.
    SinSitio,
    /// La parte llana, ya retirada del desnivel: `(x, z, ancho, alto)`.
    Sitio((i64, i64, i64, i64)),
}

/// **NI SOBRE EL AGUA NI A CABALLO DE UN ESCALÓN.**
///
/// Barre el rectángulo en rejilla de 4 tiles —no una esquina: mirando sólo la
/// noroeste quedaban medias manzanas de casas asomando sobre las olas en cuanto
/// la costa entraba en diagonal, que es el caso normal en un puerto— y devuelve
/// qué parte de él es suelo llano de un solo escalón.
///
/// El barrido pide el NIVEL de relieve, así que las dos reglas salen de la misma
/// consulta y la segunda no cuesta nada:
///
/// - **líquido** (nivel < 0): una casa flotando no se sostiene ni con la vista.
/// - **desnivel**: si la parcela pisa dos escalones, por dentro le cruza una cara
///   de acantilado — y el tile edificado GANA sobre el natural
///   (`topdown::topdown_tile`), así que la pared del mundo desaparecía justo
///   donde entra la casa y el desnivel quedaba cortado a la altura del ladrillo.
///   Medido sobre la Tierra (`medir_acantilados`): **el 24,3 % de las parcelas
///   tenía una cara adentro**.
///
/// **El escalón no vacía la manzana: la achica.** Rechazar la parcela entera fue
/// el primer intento y dejaba la metrópolis en 124 manzanas edificadas de 207 —un
/// descampado, y lo cazó `la_ciudad_no_esta_llena_de_baldios`—. El motivo es de
/// tamaños: una manzana de ciudad mide 34 tiles y un borde de terraza cae cada
/// ~20, así que la mayoría de las parcelas rozan uno; rechazarlas por rozarlo es
/// rechazar la ciudad. Lo que se hace es RETIRARLAS: la parcela se queda con la
/// parte llana y el borde de la terraza pasa por el jardín.
///
/// Hay cuatro recortes posibles —quedarse con lo de antes o con lo de después del
/// desnivel, en cada eje— y se elige el que conserva más superficie. Los cuatro
/// son llanos por construcción: el que corta por delante deja fuera todas las
/// muestras del otro escalón (todas caen en la PRIMERA o después) y el que corta
/// por detrás, ídem con la última. Que estén los cuatro y no sólo los dos "de
/// delante" no es simetría por gusto: un desnivel que entra pegado al borde norte
/// o al oeste mataba la parcela entera, y ésos son uno de cada cuatro.
///
/// El tile de margen es literal: la casa no se apoya contra la pared, le deja el
/// retiro que tiene cualquier construcción al pie de un desnivel.
///
/// **No mira quién pregunta.** Depende sólo del rectángulo, y eso es lo que
/// permite que `lot_at` conteste lo mismo para todas las celdas de una parcela.
fn retiro_del_desnivel(body: CelestialBody, ox: i64, oz: i64, w: i64, hgt: i64) -> Retiro {
    // **MEMO POR RECTÁNGULO.** `lot_at` corre por tile y prueba hasta diez
    // candidatos, y como el terreno se mira SIEMPRE (contenga o no a la celda) el
    // mismo rectángulo se barre una vez por celda del barrio: medido, la suite
    // pasó de 214 s a 350 s. Pero los candidatos de dos celdas vecinas son los
    // mismos, así que basta con recordar el último puñado. Directa, sin candados,
    // igual que la caché de niveles de `relief`.
    const RANURAS: usize = 128;
    #[derive(Clone, Copy)]
    struct Ranura {
        clave: (u8, i64, i64, i64, i64, u32),
        valor: Retiro,
        lleno: bool,
    }
    thread_local! {
        static MEMO: std::cell::RefCell<[Ranura; RANURAS]> = std::cell::RefCell::new(
            [Ranura { clave: (0, 0, 0, 0, 0, 0), valor: Retiro::SinSitio, lleno: false }; RANURAS],
        );
    }
    // La época de la semilla cierra la clave: otra partida, otro terreno bajo el lote.
    let clave = (body as u8, ox, oz, w, hgt, crate::partida::epoca());
    let ranura = (hash2(0x1077_0000, ox.wrapping_mul(31) ^ w, oz.wrapping_mul(17) ^ hgt) as usize
        ^ body as usize)
        % RANURAS;
    if let Some(v) = MEMO.with(|m| {
        let m = m.borrow();
        (m[ranura].lleno && m[ranura].clave == clave).then_some(m[ranura].valor)
    }) {
        return v;
    }
    let v = calcular_retiro(body, ox, oz, w, hgt);
    MEMO.with(|m| m.borrow_mut()[ranura] = Ranura { clave, valor: v, lleno: true });
    v
}

/// **¿Le entra agua a la CASA de esta parcela residencial?**
///
/// Barre la huella construida (la misma que dibujan `residential_tile` y
/// `roofed_rect`) preguntando por el tile natural, que es el único que sabe del
/// río. Se recorre ENTERA y no de a saltos: un cauce de dos tiles se cuela por
/// cualquier muestreo, y es justo el que ahoga la puerta.
///
/// **Memoizada por parcela, igual que [`retiro_del_desnivel`] y por lo mismo.**
/// `lot_at` corre por TILE, así que sin memo cada celda de la ciudad volvería a
/// barrer las ~280 celdas de la casa de su parcela: 280 veces más trabajo en cada
/// tile urbano del mundo. Con la memo, la primera celda de la parcela paga el
/// barrido y las demás leen la respuesta.
fn casa_mojada(body: CelestialBody, lot: &Lot) -> bool {
    const RANURAS: usize = 64;
    #[derive(Clone, Copy)]
    struct Ranura {
        clave: (u8, i64, i64, i64, i64, u32),
        valor: bool,
        lleno: bool,
    }
    thread_local! {
        static MEMO: std::cell::RefCell<[Ranura; RANURAS]> = std::cell::RefCell::new(
            [Ranura { clave: (0, 0, 0, 0, 0, 0), valor: false, lleno: false }; RANURAS],
        );
    }
    let clave = (body as u8, lot.min.0, lot.min.1, lot.w, lot.h, crate::partida::epoca());
    let ranura = (hash2(0xca5a_0000, lot.min.0, lot.min.1) as usize ^ body as usize) % RANURAS;
    if let Some(v) = MEMO.with(|m| {
        let m = m.borrow();
        (m[ranura].lleno && m[ranura].clave == clave).then_some(m[ranura].valor)
    }) {
        return v;
    }
    let (hw, hd) = lot.program().huella_de(lot.w, lot.h);
    let h = hash2(body_terrain_seed(body) ^ 0xfa11_0000, lot.min.0, lot.min.1);
    let hx = lot.min.0 + 2 + ((h >> 11) as i64).rem_euclid((lot.w - hw - 3).max(1));
    let hz = lot.min.1 + 2;
    let v = (hz..hz + hd).any(|z| (hx..hx + hw).any(|x| crate::geography::hay_agua(body, x, z)));
    MEMO.with(|m| m.borrow_mut()[ranura] = Ranura { clave, valor: v, lleno: true });
    v
}

fn calcular_retiro(body: CelestialBody, ox: i64, oz: i64, w: i64, hgt: i64) -> Retiro {
    const PASO: i64 = 4;
    let nivel = crate::relief::relief_level(body, ox, oz);
    // La PRIMERA y la ÚLTIMA muestra de otro escalón, por eje: entre las dos
    // queda dicho por dónde entra el desnivel y cuánto llano sobra a cada lado.
    let (mut prim_x, mut prim_z) = (w, hgt);
    let (mut ult_x, mut ult_z) = (-1i64, -1i64);
    let mut dz = 0;
    loop {
        let mut dx = 0;
        loop {
            let n = crate::relief::relief_level(body, ox + dx, oz + dz);
            if n < 0 {
                return Retiro::Mojado;
            }
            if n != nivel {
                prim_x = prim_x.min(dx);
                prim_z = prim_z.min(dz);
                ult_x = ult_x.max(dx);
                ult_z = ult_z.max(dz);
            }
            if dx == w - 1 {
                break;
            }
            dx = (dx + PASO).min(w - 1);
        }
        if dz == hgt - 1 {
            break;
        }
        dz = (dz + PASO).min(hgt - 1);
    }
    if ult_x < 0 {
        return Retiro::Sitio((ox, oz, w, hgt));
    }
    // (corrimiento en x, corrimiento en z, ancho, alto)
    let opciones = [
        (0, 0, prim_x - 1, hgt),
        (0, 0, w, prim_z - 1),
        (ult_x + 2, 0, w - ult_x - 2, hgt),
        (0, ult_z + 2, w, hgt - ult_z - 2),
    ];
    match opciones
        .iter()
        .filter(|(_, _, a, b)| *a >= LADO_MINIMO && *b >= LADO_MINIMO)
        .max_by_key(|(_, _, a, b)| a * b)
    {
        Some(&(sx, sz, nw, nh)) => Retiro::Sitio((ox + sx, oz + sz, nw, nh)),
        None => Retiro::SinSitio,
    }
}

pub fn lot_at(body: CelestialBody, s: &Settlement, tx: i64, tz: i64) -> Option<Lot> {
    let block = block_size(s.style, s.tier);
    let (cx, cz) = s.center;
    let bx = (tx - cx).div_euclid(block);
    let bz = (tz - cz).div_euclid(block);
    // Los lotes MAYORES que ya se descartaron para esta celda: siguen ocupando su
    // rectángulo, así que ninguno más chico puede pisarlos. Array fijo y no `Vec`
    // porque el barrido prueba doce candidatos como mucho y esto corre por tile.
    let mut mayores = [None::<Lot>; 12];
    let mut n_mayores = 0usize;
    for n in [4i64, 3, 2, 1] {
        for nx in [n, 1] {
            for nz in [n, 1] {
                if nx == 1 && nz == 1 && n != 1 {
                    continue;
                }
                let ax = bx - bx.rem_euclid(nx);
                let az = bz - bz.rem_euclid(nz);
                // **El margen sale de la calle MÁS ANCHA que rodea al lote**, no
                // sólo de la del bloque ancla. Un lote limita con cuatro calles y
                // cualquiera de ellas puede ser una avenida: usando el margen del
                // ancla, un lote que empezaba junto a un callejón se estiraba
                // hasta meterse dentro de la avenida del otro lado, y salían
                // ventanas y paredes cruzadas en mitad de la calzada.
                let margin = bounding_margin(s.style, ax, az, nx, nz);
                let Some(prog) = program_at_anchor(body, s, ax, az, nx, nz, block, margin) else {
                    continue;
                };
                let p = &LOT_PROGRAMS[prog];
                // Rectángulo del lote: desde la acera del bloque ancla, del tamaño
                // del programa (acotado a lo que dan las manzanas reservadas).
                let ox = cx + ax * block + margin;
                let oz = cz + az * block + margin;
                let w = p.cells.0.min(nx * block - margin * 2).max(4);
                let hgt = p.cells.1.min(nz * block - margin * 2).max(4);
                // Fuera del casco no hay ciudad — y el lote ENTERO tiene que
                // caber, con su retiro de la circunvalación. Comprobando sólo la
                // esquina, la última fila de manzanas se plantaba encima del
                // anillo y quedaban medias casas cruzadas en la calzada del borde.
                let libre = s.radius - crate::roads::ring_class(s.tier).half_shoulder() as i64;
                if (ox - cx).abs() > libre
                    || (oz - cz).abs() > libre
                    || (ox + w - cx).abs() > libre
                    || (oz + hgt - cz).abs() > libre
                {
                    continue;
                }
                let bruto = Lot { min: (ox, oz), w, h: hgt, program: prog };
                if mayores.iter().flatten().any(|m| bruto.overlaps(m)) {
                    continue;
                }
                // **LO QUE RESERVA ES EL RECTÁNGULO QUE DE VERDAD EXISTE.**
                //
                // `lot_at` corre por TILE y tiene que dar la misma respuesta para
                // todas las celdas de una parcela; si no, media casa sale dibujada
                // y la otra media es pasto. Lo que rompe esa promesa es decidir la
                // reserva mirando a la celda que pregunta — y eso es exactamente lo
                // que pasaba con el atajo de "si el candidato no me contiene, lo
                // apunto y sigo": para las celdas de dentro el candidato podía no
                // existir (agua, o un desnivel que no deja sitio) y el hueco quedar
                // libre, mientras que para las de fuera figuraba ocupado. La
                // parcela chica que se metía en el hueco salía dibujada a trozos y
                // sin puerta, y lo cazó `toda_casa_generada_tiene_por_donde_entrar`.
                //
                // Por eso el terreno se mira SIEMPRE, contenga o no a la celda: es
                // lo único que hace que las doce celdas de una manzana cuenten la
                // misma historia. Sale barato porque `relief_level` va por caché y
                // los candidatos de las celdas vecinas son los mismos.
                let mut reservar = |mayores: &mut [Option<Lot>; 12], n: &mut usize, l: Lot| {
                    if *n < mayores.len() {
                        mayores[*n] = Some(l);
                        *n += 1;
                    }
                };
                let lot = match retiro_del_desnivel(body, ox, oz, w, hgt) {
                    // El candidato NO EXISTE: le entra agua, o el desnivel no le
                    // deja parte llana que sirva. No se reserva —el hueco queda
                    // libre para un programa más chico, que se barre a
                    // continuación— y, como la cuenta no mira la celda que
                    // pregunta, todas las celdas de la manzana coinciden en eso.
                    //
                    // Reservarlo "por si acaso" era lo que dejaba **casas con la
                    // fachada partida y sin puerta**: para unas celdas el
                    // rectángulo grande estaba tapado y para otras no, así que la
                    // parcela chica salía dibujada a trozos. Lo caza
                    // `toda_casa_generada_tiene_por_donde_entrar`.
                    Retiro::Mojado | Retiro::SinSitio => continue,
                    Retiro::Sitio((x, z, a, b)) => Lot { min: (x, z), w: a, h: b, program: prog },
                };
                // Esta celda puede caer fuera de la parcela: porque el candidato
                // no la contenía, o porque el retiro por desnivel la dejó en el
                // jardín. En los dos casos el rectángulo BRUTO queda reservado —el
                // retiro es franja verde de esta manzana, no sitio libre para
                // otro programa— y la celda sigue buscando uno más chico fuera.
                if !lot.contains(tx, tz) {
                    reservar(&mut mayores, &mut n_mayores, bruto);
                    continue;
                }
                // **La carretera pasa antes que la parcela.** El enlace
                // interurbano entra al casco (si no, moría contra un jardín) y se
                // dibuja DESPUÉS de lo edificado, así que partiría la casa al
                // medio. El lote se aparta entero: se compara la distancia al eje
                // con el radio del círculo que lo envuelve, que es conservador —
                // deja una franja verde a los lados de la autopista, que es
                // exactamente lo que hay al lado de una autopista.
                let (w, hgt) = (lot.w, lot.h);
                let (ox, oz) = lot.min;
                let radio = (((w * w + hgt * hgt) as f32).sqrt()) * 0.5;
                let (mx, mz) = (ox as f32 + w as f32 * 0.5, oz as f32 + hgt as f32 * 0.5);
                if let Some(margen) = crate::geography::link_clearance(body, mx, mz) {
                    if margen + radio >= 0.0 {
                        return None;
                    }
                }
                // **Y la CASA no se planta dentro del río.**
                //
                // El retiro por desnivel mira `relief_level < 0`, que es el mar y
                // los lagos del relieve — pero el río no está ahí: lo traza
                // `crate::rios` y llega al tile por `geography::natural_tile`. Una
                // parcela con el cauce cruzándola contaba como seca, y la casa
                // salía con medio frente bajo el agua y sus dos puertas ahogadas,
                // o sea **sellada** (lo cazó `toda_casa_generada_tiene_por_donde_entrar`
                // en cuanto las parcelas se agrandaron al talle del pack).
                //
                // Se mira la HUELLA DE LA CASA y no el lote entero, y ésa es toda
                // la diferencia: rechazando la parcela porque el río le roza una
                // esquina, la ciudad perdía 95 de sus 215 manzanas y se volvía un
                // descampado (`la_ciudad_no_esta_llena_de_baldios`). Un jardín
                // puede dar al río; una casa, no.
                if lot.program().district == District::Residential && casa_mojada(body, &lot) {
                    reservar(&mut mayores, &mut n_mayores, bruto);
                    continue;
                }
                return Some(lot);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Pintar la parcela
// ---------------------------------------------------------------------------

/// Qué tile pone la parcela en esta celda. `None` = terreno natural (el jardín).
///
/// Todo sale de tiles que ya existían más `Fence`: la casa es madera, el garaje
/// ladrillo, la entrada asfalto, el patio piedra y la pileta **agua de bajío**, o
/// sea nadable de verdad.
pub fn lot_tile(body: CelestialBody, s: &Settlement, tx: i64, tz: i64) -> Option<Tile> {
    let lot = lot_at(body, s, tx, tz)?;
    let p = lot.program();
    let (lx, lz) = (tx - lot.min.0, tz - lot.min.1);
    let h = hash2(body_terrain_seed(body) ^ 0xfa11_0000, lot.min.0, lot.min.1);

    match p.district {
        District::Park => park_tile(p, lot, lx, lz, h),
        District::Residential => residential_tile(lot, lx, lz, h),
        _ => plain_building_tile(p, lot, lx, lz, h),
    }
}

/// Cuántos tiles de andén hay entre el borde del natatorio y el agua. Tres es lo
/// que hace falta para que quepan las reposeras y para que se pueda caminar
/// alrededor sin caerse a la pileta en cada paso.
const ANDEN_NATATORIO: i64 = 3;

/// Parque: losa, cancha, arboleda o natatorio. Nunca paredes.
fn park_tile(p: &LotProgram, lot: Lot, lx: i64, lz: i64, _h: u32) -> Option<Tile> {
    match p.key {
        // La arboleda es terreno natural: los árboles los siembra la decoración.
        "lot/arboleda" => None,
        // **EL NATATORIO: agua de verdad, no una losa celeste.** El vaso es
        // `Tile::Water` —el líquido de poco fondo que se NADA, el mismo de la
        // pileta del patio de una casa—, así que el natatorio del barrio se USA:
        // te metés y nadás. Alrededor
        // queda un andén de baldosa de dos tiles —donde la decoración pone las
        // reposeras y el trampolín— y el borde exterior marca el recinto igual
        // que en una cancha.
        "lot/natatorio" => {
            let borde = lx == 0 || lz == 0 || lx == lot.w - 1 || lz == lot.h - 1;
            if borde {
                return Some(Tile::Pavement);
            }
            let dentro = lx >= ANDEN_NATATORIO
                && lz >= ANDEN_NATATORIO
                && lx < lot.w - ANDEN_NATATORIO
                && lz < lot.h - ANDEN_NATATORIO;
            Some(if dentro { Tile::Water } else { Tile::Basalt })
        }
        // Cancha y plaza: una losa con borde. El borde es lo que la separa del
        // césped y hace que se lea como una cancha y no como un claro.
        _ => {
            let edge = lx == 0 || lz == 0 || lx == lot.w - 1 || lz == lot.h - 1;
            Some(if edge { Tile::Pavement } else { Tile::Basalt })
        }
    }
}

/// **La parcela residencial**: casa + garaje + entrada + patio + pileta + cerca.
/// El corazón de que un barrio se vea habitado.
fn residential_tile(lot: Lot, lx: i64, lz: i64, h: u32) -> Option<Tile> {
    let (w, d) = (lot.w, lot.h);
    // Cada extra se sortea aparte: así hay casas con garaje y sin pileta, con
    // pileta y sin garaje, y ninguna manzana repite la de al lado.
    let has_garage = w >= 16 && (h >> 2) % 100 < 62;
    let has_pool = w >= 18 && d >= 12 && (h >> 5) % 100 < 34;
    let has_patio = (h >> 8) % 100 < 70;

    // 1) CERCA perimetral, con portón al frente (borde sur, mirando a la calle).
    let gate = w / 2;
    if lx == 0 || lz == 0 || lx == w - 1 || lz == d - 1 {
        let is_gate = lz == d - 1 && (lx == gate || lx == gate + 1);
        return if is_gate { None } else { Some(Tile::Fence) };
    }

    // 2) CASA: pegada al fondo (norte), centrada-ish, tamaño variable.
    // El `max` del techo no es paranoia: `clamp(a, b)` con `b < a` es un panic,
    // y desde que la parcela se RETIRA de los desniveles (`lot_at`) el fondo
    // puede bajar hasta `LADO_MINIMO`. Reventaba con `min = 5, max = 2`.
    let (house_w, house_d) = lot.program().huella_de(w, d);
    let house_x = 2 + ((h >> 11) as i64).rem_euclid((w - house_w - 3).max(1));
    let house_z = 2;
    let in_house = lx >= house_x && lx < house_x + house_w && lz >= house_z && lz < house_z + house_d;
    if in_house {
        let (rx, rz) = (lx - house_x, lz - house_z);
        let perimeter = rx == 0 || rz == 0 || rx == house_w - 1 || rz == house_d - 1;
        if !perimeter {
            return Some(Tile::WoodFloor);
        }
        // **Puerta al frente (sur) Y AL FONDO (norte), centradas y anchas.**
        //
        // Nunca una sola celda: la calzada gana sobre lo edificado
        // (`topdown_tile`), así que bastaba que una carretera rozara la fachada
        // por la celda de la puerta para dejar la casa **sellada** — cuatro
        // paredes, ventanas y ninguna forma de entrar.
        //
        // Y desde que las casas son las del pack —hasta 23 tiles de frente en vez
        // de 6— el portal de dos celdas fijas volvió a quedarse corto: una calle
        // de CINCO tiles se lo come entero, y en un puñado de casos la calle
        // pasaba justo por el medio de la casa.
        //
        // Así que el portal crece con la casa **y hay uno en las CUATRO caras**.
        // Ése es el punto: una calle es una recta, así que puede comerse los dos
        // portales que miran a un eje, pero nunca los cuatro. Para sellar una casa
        // ahora hace falta un cruce entero encima, y entonces el problema no es la
        // puerta. Medido en Ciudad Central: 4 casas selladas de 1.849 → 0. Lo
        // cuida `toda_casa_generada_tiene_por_donde_entrar`.
        let portal_x = (house_w / 4).clamp(2, 6);
        let portal_z = (house_d / 4).clamp(2, 6);
        let en_x = rx >= (house_w - portal_x) / 2 && rx < (house_w - portal_x) / 2 + portal_x;
        let en_z = rz >= (house_d - portal_z) / 2 && rz < (house_d - portal_z) / 2 + portal_z;
        if (en_x && (rz == house_d - 1 || rz == 0)) || (en_z && (rx == 0 || rx == house_w - 1)) {
            return Some(Tile::Door);
        }
        // Ventanas: alternadas en las cuatro caras, sin tocar las esquinas.
        let corner = (rx == 0 || rx == house_w - 1) && (rz == 0 || rz == house_d - 1);
        if !corner && (rx + rz) % 3 == 1 {
            return Some(Tile::Window);
        }
        return Some(Tile::WoodWall);
    }

    // 3) GARAJE: al lado de la casa, contra el frente, con su entrada de auto.
    if has_garage {
        let gw = 6;
        let gd = 5;
        let gx = if house_x >= gw + 2 { house_x - gw - 1 } else { house_x + house_w + 1 };
        let gz = d - gd - 2;
        if gx >= 1 && gx + gw < w - 1 && lx >= gx && lx < gx + gw && lz >= gz && lz < gz + gd {
            let (rx, rz) = (lx - gx, lz - gz);
            let perimeter = rx == 0 || rz == 0 || rx == gw - 1 || rz == gd - 1;
            if rz == gd - 1 && rx > 0 && rx < gw - 1 {
                return Some(Tile::Door); // portón
            }
            return Some(if perimeter { Tile::StoneBrick } else { Tile::WoodFloor });
        }
        // ENTRADA DE AUTO: del portón del garaje hasta la calle.
        if gx >= 1 && gx + gw < w - 1 && lx > gx && lx < gx + gw - 1 && lz >= gz + gd {
            return Some(Tile::Basalt);
        }
    }

    // 4) PILETA con su borde de piedra, en el patio de atrás.
    if has_pool {
        let px = w - 7;
        let pz = 2;
        if lx >= px && lx < px + 5 && lz >= pz && lz < pz + 4 {
            let (rx, rz) = (lx - px, lz - pz);
            let rim = rx == 0 || rz == 0 || rx == 4 || rz == 3;
            // `Water` = bajío ⇒ **se nada**. La pileta del patio es una pileta.
            return Some(if rim { Tile::Pavement } else { Tile::Water });
        }
    }

    // 5) PATIO de piedra junto a la puerta (ahí va la parrilla, que la siembra la
    // decoración de props).
    if has_patio {
        let cx0 = house_x + house_w / 2 - 1;
        if lx >= cx0 && lx < cx0 + 3 && lz >= house_z + house_d && lz < house_z + house_d + 3 {
            return Some(Tile::Pavement);
        }
    }

    // El resto es jardín: terreno natural, y la flora procedural lo puebla.
    None
}

/// Edificio "macizo" (comercio, centro, industria, cívico): un rectángulo con
/// paredes, ventanas y su vereda. Los grandes llevan patio interior para que no
/// sean un bloque sólido de 56×40.
fn plain_building_tile(p: &LotProgram, lot: Lot, lx: i64, lz: i64, h: u32) -> Option<Tile> {
    let (w, d) = (lot.w, lot.h);
    // Estacionamiento: sólo losa y líneas, sin edificio.
    if p.key == "lot/estacionamiento" {
        let edge = lx == 0 || lz == 0 || lx == w - 1 || lz == d - 1;
        return Some(if edge { Tile::Pavement } else { Tile::Basalt });
    }
    // Vereda de 1 tile alrededor: separa la fachada de la calzada.
    if lx == 0 || lz == 0 || lx == w - 1 || lz == d - 1 {
        return Some(Tile::Pavement);
    }
    let (rx, rz) = (lx - 1, lz - 1);
    let (bw, bd) = (w - 2, d - 2);
    let perimeter = rx == 0 || rz == 0 || rx == bw - 1 || rz == bd - 1;

    // PATIO INTERIOR en los edificios grandes: un hospital o una universidad
    // tienen patio. Sin él, 56×40 de piso continuo se lee como una losa.
    if bw >= 26 && bd >= 20 {
        let (ix, iz) = (bw / 4, bd / 4);
        let inner = rx >= ix && rx < bw - ix && rz >= iz && rz < bd - iz;
        if inner {
            let ring = rx == ix || rz == iz || rx == bw - ix - 1 || rz == bd - iz - 1;
            // El patio interior también se entra: sin este paso en su anillo, el
            // hospital tenía en el centro un rectángulo de losa al que no se podía
            // llegar por ningún lado. Un patio al que no se sale no es un patio.
            let paso = rz == bd - iz - 1 && (rx == bw / 2 || rx == bw / 2 - 1);
            if ring && !paso {
                return Some(Tile::StoneBrick);
            }
            return Some(Tile::Pavement);
        }
    }

    if !perimeter {
        return Some(Tile::WoodFloor);
    }
    // Puerta al frente, de dos tiles: misma razón que en la casa — la calzada gana
    // sobre lo edificado y un portal de una sola celda se puede quedar sin ella.
    if rz == bd - 1 && bw >= 4 && (rx == bw / 2 || rx == bw / 2 - 1) {
        return Some(Tile::Door);
    }
    let corner = (rx == 0 || rx == bw - 1) && (rz == 0 || rz == bd - 1);
    // Vidriera: el comercio tiene MUCHO más vidrio que una nave industrial, y esa
    // diferencia es la que hace que un distrito se distinga de otro a simple vista.
    let glass = match p.district {
        District::Commercial | District::Downtown => 2,
        District::Civic => 3,
        _ => 5,
    };
    if !corner && (rx + rz + (h % 3) as i64) % glass == 0 {
        return Some(Tile::Window);
    }
    Some(Tile::StoneBrick)
}

/// **Hacia dónde queda el AFUERA de la cerca de esta celda.**
///
/// El pack dibuja los dos tramos verticales de un cerco por separado —uno con los
/// postes contra el borde izquierdo de la celda y otro contra el derecho— y lo
/// mismo con los horizontales. Elegir el equivocado deja los postes del lado del
/// jardín; se ve raro en cuanto mirás una manzana entera.
///
/// La respuesta la sabe el lote y nadie más: la cerca es su perímetro, así que el
/// afuera es el borde por el que esta celda toca la calle. Se contesta acá y no
/// en el cliente porque en el cliente habría que adivinarla mirando vecinos, y
/// una parcela pegada a otra no deja adivinar nada.
///
/// Devuelve `None` si la celda no es cerca de un lote generado — una cerca puesta
/// desde el editor de mundo no tiene adentro ni afuera, y el juego cae a la pose
/// sin lado (ver `crate::cercas::cadena`).
pub fn afuera_de_cerca(
    body: CelestialBody,
    s: &Settlement,
    tx: i64,
    tz: i64,
) -> Option<crate::cercas::Afuera> {
    use crate::cercas::Afuera;
    let lot = lot_at(body, s, tx, tz)?;
    if lot.program().district != District::Residential {
        return None;
    }
    let (lx, lz) = (tx - lot.min.0, tz - lot.min.1);
    // El orden importa sólo en las esquinas, y en una esquina el sufijo no se usa
    // (`cercas::cadena` sólo lo pide para los tramos rectos).
    if lx == 0 {
        Some(Afuera::W)
    } else if lx == lot.w - 1 {
        Some(Afuera::E)
    } else if lz == 0 {
        Some(Afuera::N)
    } else if lz == lot.h - 1 {
        Some(Afuera::S)
    } else {
        None
    }
}

/// La huella de la CASA/edificio de esta celda — lo que el cliente usa para el
/// techo. Devuelve el rectángulo edificado, no el lote entero (el techo no cubre
/// el jardín ni la pileta).
pub fn roofed_rect(
    body: CelestialBody,
    s: &Settlement,
    tx: i64,
    tz: i64,
) -> Option<(i64, i64, i64, i64, &'static str)> {
    let lot = lot_at(body, s, tx, tz)?;
    let p = lot.program();
    if p.district == District::Park || p.key == "lot/estacionamiento" {
        return None;
    }
    if p.district == District::Residential {
        let (w, d) = (lot.w, lot.h);
        let h = hash2(body_terrain_seed(body) ^ 0xfa11_0000, lot.min.0, lot.min.1);
        // Mismo techo defensivo que en `residential_tile`, y por el mismo motivo:
        // las dos cuentas TIENEN que dar lo mismo o el techo del cliente se
        // dibuja donde no está la casa.
        let (house_w, house_d) = p.huella_de(w, d);
        let house_x = 2 + ((h >> 11) as i64).rem_euclid((w - house_w - 3).max(1));
        return Some((lot.min.0 + house_x, lot.min.1 + 2, house_w, house_d, p.key));
    }
    // **La clave del PROGRAMA viaja con el rect.** Antes se tiraba y quien
    // dibujaba el techo adivinaba el arte por el tamaño (5×4 → casa chica), lo
    // que además de frágil apuntaba a tres slots sin pixel. El lote ya sabe qué
    // edificio es; lo único que faltaba era no perder el dato.
    Some((lot.min.0 + 1, lot.min.1 + 1, lot.w - 2, lot.h - 2, p.key))
}

#[cfg(test)]
mod tests_zonas {
    use super::*;
    use crate::zonas::{ClaseZona, Zona, Zonas};

    /// **Una ciudad de laboratorio, y a propósito NO en la Tierra.**
    ///
    /// El registro de zonas es global al proceso (ver `zonas::instalar`), y los
    /// tests de cargo corren en paralelo: instalar zonas terrestres acá le
    /// cambiaría los distritos al resto de la suite a mitad de camino. Como
    /// `clase_en` filtra por cuerpo, usar Neptuno deja este test aislado sin
    /// candados ni orden de ejecución impuesto.
    fn ciudad() -> (CelestialBody, Settlement) {
        let body = CelestialBody::Neptune;
        (
            body,
            Settlement {
                tier: crate::geography::SettlementTier::City,
                center: (12_000, -8_000),
                style: CityStyle::Grid,
                name: None,
                radius: 1_800,
            },
        )
    }

    /// **La prueba entera del sistema de zonas**: pintar un rectángulo cambia lo
    /// que el generador construye ahí, sin que nadie coloque un edificio a mano.
    ///
    /// Es lo que se pidió — "yo reparto las zonas, vos repartís lo de adentro" —
    /// y la única forma de comprobarlo de punta a punta: el mismo bloque, con y
    /// sin zona encima, tiene que dar distrito distinto y, con él, otro catálogo
    /// de parcelas.
    #[test]
    fn pintar_una_zona_cambia_lo_que_se_construye_ahi() {
        let (body, s) = ciudad();
        let block = crate::roads::block_size(s.style, s.tier);
        // Una manzana bien lejos del centro: sin zonas es residencial o similar,
        // nunca centro financiero.
        let (bx, bz) = (9i64, 9i64);
        crate::zonas::instalar(Zonas::default());
        let sin_zona = district_at(body, &s, bx, bz);
        let vecino_sin_zona = district_at(body, &s, bx + 2, bz);

        // Ahora se pinta CENTRO justo encima de esa manzana.
        let min = (s.center.0 + bx * block, s.center.1 + bz * block);
        crate::zonas::instalar(Zonas::new(vec![Zona {
            nombre: "Centro nuevo".into(),
            body: body as u8,
            min,
            size: (block, block),
            clase: ClaseZona::Centro,
        }]));
        let con_zona = district_at(body, &s, bx, bz);
        assert_eq!(con_zona, District::Downtown, "la zona pintada no mandó");
        assert_ne!(sin_zona, con_zona, "la zona tiene que CAMBIAR algo");

        // Y lo que se construye ahí sale del catálogo del distrito nuevo.
        let del_centro: Vec<&str> = LOT_PROGRAMS
            .iter()
            .filter(|p| p.district == District::Downtown)
            .map(|p| p.key)
            .collect();
        assert!(del_centro.contains(&"building/torre"));

        // La manzana de al lado, fuera del rectángulo, sigue exactamente como
        // estaba: una zona pinta SU rectángulo y no contagia al vecino.
        assert_eq!(
            district_at(body, &s, bx + 2, bz),
            vecino_sin_zona,
            "la zona se desbordó a la manzana de al lado"
        );
        crate::zonas::instalar(Zonas::default());
    }

    /// Y una sub-zona dentro de la zona cambia sólo su pedazo: es la jerarquía
    /// pedida ("dentro de esa zona poder crear más zonas"), vista desde el
    /// generador.
    #[test]
    fn una_subzona_recorta_un_pedazo_de_su_zona() {
        let (body, s) = ciudad();
        let block = crate::roads::block_size(s.style, s.tier);
        let base = (s.center.0 + 4 * block, s.center.1 + 4 * block);
        crate::zonas::instalar(Zonas::new(vec![
            Zona {
                nombre: "Barrio".into(),
                body: body as u8,
                min: base,
                size: (block * 4, block * 4),
                clase: ClaseZona::Residencial,
            },
            Zona {
                nombre: "Fábricas".into(),
                body: body as u8,
                min: (base.0 + block, base.1 + block),
                size: (block, block),
                clase: ClaseZona::Industrial,
            },
        ]));
        assert_eq!(district_at(body, &s, 4, 4), District::Residential);
        assert_eq!(district_at(body, &s, 5, 5), District::Industrial, "la sub-zona no mandó");
        assert_eq!(district_at(body, &s, 6, 6), District::Residential);
        crate::zonas::instalar(Zonas::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::settlement_in;

    fn a_city() -> Settlement {
        let body = CelestialBody::Earth;
        crate::world_map::template(body)
            .towns
            .iter()
            .map(|t| Settlement {
                tier: SettlementTier::City,
                center: crate::world_map::town_center(body, t),
                style: t.style,
                name: Some(t.name),
                radius: t.radius,
            })
            .next()
            .unwrap()
    }

    /// **Los programas tienen tamaños DISTINTOS**, y los grandes son grandes de
    /// verdad. Si esto se aplana, volvemos a la caja única por manzana.
    #[test]
    fn lot_programs_have_genuinely_different_sizes() {
        let mut areas: Vec<i64> = LOT_PROGRAMS.iter().map(|p| p.cells.0 * p.cells.1).collect();
        areas.sort_unstable();
        let (small, big) = (areas[0], areas[areas.len() - 1]);
        assert!(big > small * 12, "el más grande sólo mide {big} y el más chico {small}");
        // Y los edificios que sabemos grandes lo son.
        //
        // **Por ÁREA, no lado a lado.** Esto pedía que la universidad midiera más
        // del triple que la casa chica en CADA lado, y eso dejó de poder cumplirse
        // el día que las parcelas se dimensionaron al arte del pack: la casa chica
        // pasó de 12×10 a 17×14 porque una casa de LimeZu mide catorce tiles de
        // frente, y el edificio de escuela más grande que el pack dibujó mide
        // diecisiete celdas. Pedir 51 de frente era pedir un dibujo que no existe.
        // Lo que el test quiere afirmar —que los grandes son grandes de verdad y
        // no se aplanó todo a la caja única por manzana— se afirma igual por área.
        let uni = LOT_PROGRAMS.iter().find(|p| p.key == "building/universidad").unwrap();
        let casa = LOT_PROGRAMS.iter().find(|p| p.key == "building/casa_chica").unwrap();
        let (a_uni, a_casa) = (uni.cells.0 * uni.cells.1, casa.cells.0 * casa.cells.1);
        assert!(
            a_uni > a_casa * 4,
            "la universidad ({}×{}) no es un edificio grande al lado de la casa chica ({}×{})",
            uni.cells.0,
            uni.cells.1,
            casa.cells.0,
            casa.cells.1
        );
    }

    /// **UNA CERCA VERTICAL NO USA UNA PIEZA HORIZONTAL.**
    ///
    /// El reporte fue literal: *"tenemos vallas muros verticales, no pongas
    /// horizontales en vertical, se ve horrible"*. La causa era que
    /// `Tile::Fence` pedía una forma de autotile de ÁREA (`edge_n`,
    /// `corner_ne`…), que ninguna cerca tiene, y el arte caía al recorte base —
    /// el mismo tramo horizontal en los cuatro lados de la parcela.
    ///
    /// Esto barre el perímetro de parcelas residenciales de verdad y comprueba
    /// las dos mitades del arreglo:
    ///
    /// 1. Los lados este y oeste piden `ns…` y **ninguna** de sus poses de
    ///    respaldo es horizontal.
    /// 2. Los lados norte y sur piden `ew…` y ninguna es vertical.
    /// 3. El tramo sabe de qué lado le queda el afuera, así que los postes no
    ///    salen dibujados del lado del jardín.
    #[test]
    fn una_cerca_vertical_no_usa_pieza_horizontal() {
        use crate::cercas::{cadena, Vecinos};
        let body = CelestialBody::Earth;
        let s = a_city();
        let mut tramos_v = 0;
        let mut tramos_h = 0;
        // **Las cercas viven en el barrio, no en el centro.** El primer intento
        // barría 90 tiles alrededor del centro y encontró CERO: ahí todo es
        // Downtown, y una torre no tiene cerca de parcela. El residencial empieza
        // pasado el anillo comercial, o sea a unas ocho manzanas — por eso el
        // barrido arranca lejos y es ancho.
        let lejos = block_size(s.style, s.tier) * 10;
        for tz in s.center.1 + lejos..s.center.1 + lejos + 140 {
            for tx in s.center.0 + lejos..s.center.0 + lejos + 140 {
                if lot_tile(body, &s, tx, tz) != Some(Tile::Fence) {
                    continue;
                }
                let cerca = |dx: i64, dz: i64| {
                    lot_tile(body, &s, tx + dx, tz + dz) == Some(Tile::Fence)
                };
                let v = Vecinos::new(cerca(0, -1), cerca(1, 0), cerca(0, 1), cerca(-1, 0));
                let afuera = afuera_de_cerca(body, &s, tx, tz);
                let poses = cadena(v, afuera);
                let m = v.mascara();
                if m == "ns" {
                    tramos_v += 1;
                    assert!(
                        afuera.is_some(),
                        "el tramo vertical en ({tx},{tz}) no sabe de qué lado queda el afuera"
                    );
                    for p in &poses {
                        assert!(
                            !p.starts_with("ew"),
                            "tramo VERTICAL en ({tx},{tz}) pidió `{p}`, que es horizontal"
                        );
                    }
                } else if m == "ew" {
                    tramos_h += 1;
                    for p in &poses {
                        assert!(
                            !p.starts_with("ns"),
                            "tramo HORIZONTAL en ({tx},{tz}) pidió `{p}`, que es vertical"
                        );
                    }
                }
            }
        }
        // Si el barrido no encontró cercas, el test no probó nada y hay que
        // enterarse: es el modo de falla silencioso de todo test que barre mundo.
        assert!(
            tramos_v > 20 && tramos_h > 20,
            "el barrido encontró {tramos_v} tramos verticales y {tramos_h} horizontales: \
             muy pocos para probar nada"
        );
    }

    /// Todos los distritos tienen al menos un programa: uno vacío dejaría un
    /// agujero de ciudad sin edificar.
    #[test]
    fn every_district_has_programs() {
        for d in [
            District::Downtown,
            District::Commercial,
            District::Residential,
            District::Industrial,
            District::Park,
            District::Civic,
        ] {
            assert!(
                LOT_PROGRAMS.iter().any(|p| p.district == d),
                "{d:?} no tiene ni un programa"
            );
        }
    }

    /// Una ciudad recorrida entera tiene VARIEDAD: varios distritos y muchos
    /// programas distintos. Es la prueba directa contra el damero.
    #[test]
    fn a_city_is_made_of_many_different_lots() {
        let body = CelestialBody::Earth;
        let s = a_city();
        let mut programs = std::collections::HashSet::new();
        let mut districts = std::collections::HashSet::new();
        let r = s.radius.min(900);
        for dz in (-r..r).step_by(9) {
            for dx in (-r..r).step_by(9) {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                if let Some(lot) = lot_at(body, &s, tx, tz) {
                    programs.insert(lot.program().key);
                    districts.insert(lot.program().district);
                }
            }
        }
        assert!(programs.len() >= 8, "sólo {} programas: {programs:?}", programs.len());
        assert!(districts.len() >= 3, "sólo {} distritos: {districts:?}", districts.len());
    }

    /// **Una parcela residencial tiene casa, cerca y jardín** — y bastantes
    /// tienen garaje y pileta. Es el pedido concreto: barrios que se vean vividos.
    #[test]
    fn residential_lots_have_houses_fences_and_extras() {
        let body = CelestialBody::Earth;
        let s = a_city();
        let (mut walls, mut doors, mut fences, mut pools, mut drives, mut gardens) =
            (0, 0, 0, 0, 0, 0);
        let r = s.radius.min(1_200);
        for dz in -r..r {
            for dx in -r..r {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                let Some(lot) = lot_at(body, &s, tx, tz) else { continue };
                if lot.program().district != District::Residential {
                    continue;
                }
                match lot_tile(body, &s, tx, tz) {
                    Some(Tile::WoodWall) => walls += 1,
                    Some(Tile::Door) => doors += 1,
                    Some(Tile::Fence) => fences += 1,
                    Some(Tile::Water) => pools += 1,
                    Some(Tile::Basalt) => drives += 1,
                    None => gardens += 1,
                    _ => {}
                }
            }
        }
        assert!(walls > 300, "el barrio no tiene casas: {walls} paredes");
        assert!(doors > 10, "casas sin puerta: {doors}");
        assert!(fences > 300, "parcelas sin cerca: {fences}");
        assert!(pools > 20, "ni una pileta en todo el barrio: {pools}");
        assert!(drives > 40, "ni una entrada de auto: {drives}");
        assert!(gardens > 500, "las parcelas no dejan jardín: {gardens}");
    }

    /// **Toda casa generada tiene por dónde entrar.**
    ///
    /// La regla es del jugador y es innegociable: una casa sin puerta es
    /// decorado, no un sitio. Y no alcanza con que `residential_tile` dibuje una,
    /// porque lo que se ve en el mundo es la COMPOSICIÓN (`topdown_tile`), donde
    /// la calzada gana sobre lo edificado: una carretera que roce la fachada podía
    /// borrar la única puerta y dejar la casa cerrada con llave para siempre. Por
    /// eso el barrido mira el tile final y no el del lote.
    #[test]
    fn toda_casa_generada_tiene_por_donde_entrar() {
        let body = CelestialBody::Earth;
        let s = a_city();
        let mut vistos = std::collections::HashSet::new();
        let mut casas = 0;
        let mut selladas: Vec<(i64, i64)> = Vec::new();
        let r = s.radius.min(1_200);
        for dz in (-r..r).step_by(5) {
            for dx in (-r..r).step_by(5) {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                let Some(lot) = lot_at(body, &s, tx, tz) else { continue };
                if lot.program().district != District::Residential || !vistos.insert(lot.min) {
                    continue;
                }
                let Some((hx, hz, hw, hd, _)) = roofed_rect(body, &s, tx, tz) else { continue };
                casas += 1;
                let puertas = (hx..hx + hw)
                    .flat_map(|x| (hz..hz + hd).map(move |z| (x, z)))
                    .filter(|&(x, z)| crate::topdown::topdown_tile(body, 0, x, z) == Tile::Door)
                    .count();
                if puertas == 0 {
                    selladas.push((hx, hz));
                }
            }
        }
        assert!(casas > 20, "el barrido no encontró casas: {casas}");
        assert!(
            selladas.is_empty(),
            "{} casas de {casas} quedaron selladas: {:?}",
            selladas.len(),
            &selladas[..selladas.len().min(5)]
        );
    }

    /// **Los edificios grandes se plantan ENTEROS**, ocupando varias manzanas, en
    /// vez de recortarse contra el borde de la suya.
    #[test]
    fn big_civic_buildings_span_several_blocks() {
        let body = CelestialBody::Earth;
        let s = a_city();
        let block = block_size(s.style, s.tier);
        let mut biggest = 0;
        let r = s.radius.min(1_500);
        for dz in (-r..r).step_by(7) {
            for dx in (-r..r).step_by(7) {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                if let Some(lot) = lot_at(body, &s, tx, tz) {
                    biggest = biggest.max(lot.w.max(lot.h));
                }
            }
        }
        assert!(
            biggest > block,
            "ningún lote pasa de una manzana ({biggest} vs {block}): los grandes se recortan"
        );
    }

    /// Las parcelas nunca se solapan: una celda pertenece a un lote y sólo a uno.
    #[test]
    fn lots_never_overlap() {
        let body = CelestialBody::Earth;
        let s = a_city();
        let mut owner: std::collections::HashMap<(i64, i64), (i64, i64)> =
            std::collections::HashMap::new();
        let r = 400;
        for dz in -r..r {
            for dx in -r..r {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                let Some(lot) = lot_at(body, &s, tx, tz) else { continue };
                // Cada celda declara UN dueño; volver a consultarla da el mismo.
                let prev = owner.insert((tx, tz), lot.min);
                assert!(prev.is_none() || prev == Some(lot.min));
            }
        }
        assert!(!owner.is_empty(), "la ciudad no tiene ni un lote");
    }

    /// **LA CIUDAD NO ESTÁ LLENA DE BALDÍOS.**
    ///
    /// Un programa que reserva 2×2 manzanas llena una y deja tres. Como los
    /// programas grandes son la mayoría del centro —y con las manzanas chicas de
    /// antes casi ninguno entraba en una sola—, el centro de toda ciudad eran tres
    /// cuartos de descampado con un edificio en una esquina. Ahora las manzanas
    /// sobrantes reciben su propio lote mientras no se pisen con el grande.
    ///
    /// Se mide por manzana, que es la unidad que el ojo lee al recorrer la ciudad:
    /// la mayoría tiene que tener algo edificado. Y se mide en **las cuatro
    /// ciudades**, porque cada una tiene su trama y su tamaño de manzana: el
    /// agujero se escondió una vez en el puerto, cuyas manzanas de 34 no admitían
    /// ni el más chico de los programas de centro.
    #[test]
    fn la_ciudad_no_esta_llena_de_baldios() {
        for t in crate::world_map::template(CelestialBody::Earth).towns {
            let s = Settlement {
                tier: SettlementTier::City,
                center: crate::world_map::town_center(CelestialBody::Earth, t),
                style: t.style,
                name: Some(t.name),
                radius: t.radius,
            };
            manzanas_edificadas(&s);
        }
    }

    fn manzanas_edificadas(s: &Settlement) {
        let body = CelestialBody::Earth;
        let block = block_size(s.style, s.tier);
        let (mut con_lote, mut manzanas) = (0, 0);
        // 15×15 manzanas alrededor del centro: centro, comercio y primeras casas.
        for bz in -7..=7i64 {
            for bx in -7..=7i64 {
                // Se muestrea la manzana en rejilla; con que una celda tenga lote,
                // la manzana está edificada. Las celdas de agua no cuentan: media
                // metrópolis costera cae en el mar y ahí no se edifica a propósito.
                let (mut hay, mut tierra) = (false, 0);
                for j in 0..6 {
                    for i in 0..6 {
                        let tx = s.center.0 + bx * block + block * i / 6 + block / 12;
                        let tz = s.center.1 + bz * block + block * j / 6 + block / 12;
                        if !crate::geography::natural_tile(body, tx, tz).buildable_ground() {
                            continue;
                        }
                        tierra += 1;
                        if lot_at(body, s, tx, tz).is_some() {
                            hay = true;
                        }
                    }
                }
                // Una manzana que es casi toda agua o acantilado no es un baldío.
                if tierra < 24 {
                    continue;
                }
                manzanas += 1;
                con_lote += hay as i64;
            }
        }
        let nombre = s.name.unwrap_or("(sin nombre)");
        eprintln!("{nombre}: {con_lote} de {manzanas} manzanas edificadas");
        assert!(manzanas > 80, "{nombre}: apenas {manzanas} manzanas en tierra firme para medir");
        // **Dos de cada tres, no tres de cada cuatro.** El umbral bajó cuando las
        // parcelas dejaron de plantarse a caballo de un escalón
        // (`lot_at`): en una ciudad de ladera el borde de terraza cruza casi todas
        // las manzanas y lo que antes se construía encima ahora se retira. Medido
        // con el cambio puesto: Villa Sur 158 de 212 (74 %), y es la peor de las
        // cuatro. Lo que este guardián caza sigue siendo el agujero de verdad —el
        // centro vacío por programas que reservan y no llenan—, que dejaba las
        // ciudades por debajo de la mitad.
        assert!(
            con_lote * 3 >= manzanas * 2,
            "{nombre}: sólo {con_lote} de {manzanas} manzanas tienen algo construido — es un descampado"
        );
    }

    /// **NINGUNA PARCELA SE PLANTA SOBRE EL MAR.** El casco es un cuadrado y a una
    /// ciudad costera medio cuadrado le cae en el agua. Se comprobaba UNA esquina
    /// del lote, así que en cuanto la costa entraba en diagonal quedaban medias
    /// manzanas de casas asomando sobre las olas.
    /// Se mide POR PARCELA y no por celda suelta: lo que hay que impedir es la
    /// media manzana asomando sobre las olas, no el charco de cuatro tiles que
    /// queda bajo el piso de una casa y que no se ve nunca.
    #[test]
    fn ninguna_parcela_se_planta_sobre_el_mar() {
        let body = CelestialBody::Earth;
        let mut peor = (0.0f32, (0i64, 0i64));
        let mut lotes = std::collections::HashSet::new();
        for t in crate::world_map::template(body).towns {
            let s = Settlement {
                tier: SettlementTier::City,
                center: crate::world_map::town_center(body, t),
                style: t.style,
                name: Some(t.name),
                radius: t.radius,
            };
            let r = s.radius;
            for dz in (-r..r).step_by(17) {
                for dx in (-r..r).step_by(17) {
                    let (x, z) = (s.center.0 + dx, s.center.1 + dz);
                    let Some(lot) = lot_at(body, &s, x, z) else { continue };
                    if !lotes.insert(lot.min) {
                        continue;
                    }
                    // El terreno de verdad, no la aproximación continental: es el
                    // que se ve, y es el que discrepa en toda la franja de costa.
                    let mojadas = (0..lot.h)
                        .flat_map(|j| (0..lot.w).map(move |i| (i, j)))
                        .filter(|(i, j)| {
                            crate::relief::is_liquid(body, lot.min.0 + i, lot.min.1 + j)
                        })
                        .count();
                    let fraccion = mojadas as f32 / (lot.w * lot.h) as f32;
                    if fraccion > peor.0 {
                        peor = (fraccion, lot.min);
                    }
                }
            }
        }
        assert!(lotes.len() > 300, "apenas {} parcelas revisadas", lotes.len());
        assert!(
            peor.0 < 0.10,
            "la parcela de {:?} tiene el {:.0} % de su superficie sobre el agua",
            peor.1,
            peor.0 * 100.0
        );
    }

    /// Compat: un pueblo procedural sigue generando parcelas (no sólo las cuatro
    /// ciudades autoradas).
    #[test]
    fn procedural_towns_also_get_lots() {
        let body = CelestialBody::Earth;
        let mut found = false;
        'outer: for sx in -14..14 {
            for sz in -14..14 {
                let Some(s) = settlement_in(body, sx, sz) else { continue };
                if s.name.is_some() {
                    continue; // ésta es una de las autoradas
                }
                for dz in (-s.radius..s.radius).step_by(5) {
                    for dx in (-s.radius..s.radius).step_by(5) {
                        if lot_at(body, &s, s.center.0 + dx, s.center.1 + dz).is_some() {
                            found = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
        assert!(found, "los pueblos procedurales se quedaron sin edificios");
    }
}


