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

/// **Los lotes que se COMPONEN del kit modular, y cuántas plantas tienen.**
///
/// `clave del lote -> (mínimo de pisos, máximo)`, sin contar la planta baja ni la
/// azotea. Cuál le toca a cada parcela sale de su esquina, así que una manzana del
/// centro tiene torres de alturas distintas sin guardar un byte por edificio.
///
/// ## Por qué existe
///
/// `5_Floor_Modular_Building` son 343 dibujos y hasta hoy no llegaba **ninguno**
/// al mundo generado: son columnas y bandas de 9 celdas —planta baja, piso,
/// azotea— hechas para apilarse, así que `import-edificios.py`, que le busca a
/// cada lote UNA fachada entera, no encontraba ni un candidato en esa carpeta y
/// caía a los condominios de `4_Generic_Building`. Resultado medido: la oficina y
/// la torre del centro recibían **el mismo dibujo** (`edificios_condominio_38`),
/// y la tienda, el mercado y el shopping **otro mismo dibujo** entre los tres. Un
/// centro donde todas las torres son la misma torre no se lee como un centro.
///
/// El kit ya estaba importado y medido (`scripts/import-modulares.py` →
/// `edificio/pb_*`, `edificio/piso_*`, `edificio/azotea*`) y el juego ya sabía
/// apilarlo: es lo que dibuja `ha-game::edificios` para lo que construye el
/// jugador. Lo único que faltaba era que la ciudad lo usara.
///
/// ## Por qué es una tabla aparte y no un campo de [`LotProgram`]
///
/// Porque son ocho lotes de cincuenta. Un campo obliga a escribir `pisos: (0, 0)`
/// en las otras cuarenta y dos líneas, que es ruido en el único lugar del archivo
/// que se lee de un vistazo.
///
/// **El ancho no está acá**: sale de la huella del lote, redondeada a módulos de
/// [`crate::edificio::MODULO`] tiles. Ésa es la gracia de componer — el edificio
/// mide lo que mide la parcela, no lo que midió un dibujo suelto.
pub const LOTES_MODULARES: &[(&str, u8, u8)] = &[
    // Centro: lo más alto de la ciudad y lo que más se ve de lejos.
    ("building/torre", 4, 8),
    ("building/oficina", 2, 5),
    ("building/hotel", 3, 6),
    // Comercio: local abajo y una o dos plantas de vivienda encima, que es
    // exactamente como el pack dibujó la hoja.
    ("building/tienda", 1, 3),
    ("building/shopping", 0, 2),
    // Residencial de altura. El chalet NO va acá: el pack lo dibujó entero.
    ("building/duplex", 1, 2),
];

/// Los pisos que le tocan a ESTA parcela, si es de las que se componen.
///
/// Determinista por esquina: la misma manzana se dibuja igual siempre, y el
/// streaming no hace titilar la altura de una torre al entrar y salir.
/// **La comisaría más cercana** (centro del lote, en tiles) dentro de `radio` tiles de `(tx, tz)`.
///
/// Busca entre los lotes que el generador ya coloca (`building/comisaria*`) barriendo una grilla de
/// 6 tiles alrededor del punto, así que sirve para que la policía a pie salga de un edificio que
/// existe en el mundo y no de la nada. Es barata para llamarla cada pocos segundos.
pub fn comisaria_cercana(body: CelestialBody, tx: i64, tz: i64, radio: i64) -> Option<(i64, i64)> {
    edificio_cercano(body, tx, tz, radio, "building/comisaria").map(|(x, z, _, _)| (x, z))
}

/// **El edificio más cercano de un programa** (`building/bomberos`, `building/hospital`…): `(centro x, centro z, ancho, alto)`
/// del lote, dentro de `radio` tiles de `(tx, tz)`. Barre una grilla de 6 tiles: barato para llamarlo cada pocos segundos.
pub fn edificio_cercano(body: CelestialBody, tx: i64, tz: i64, radio: i64, prefijo: &str) -> Option<(i64, i64, i64, i64)> {
    let mut mejor: Option<(i64, (i64, i64, i64, i64))> = None;
    let mut z = tz - radio;
    while z <= tz + radio {
        let mut x = tx - radio;
        while x <= tx + radio {
            if let Some(s) = crate::geography::settlement_covering(body, x, z) {
                if let Some(lot) = lot_at(body, &s, x, z) {
                    if lot.program().key.starts_with(prefijo) {
                        let c = (lot.min.0 + lot.w / 2, lot.min.1 + lot.h / 2, lot.w, lot.h);
                        let d = (c.0 - tx).pow(2) + (c.1 - tz).pow(2);
                        if mejor.map_or(true, |(m, _)| d < m) {
                            mejor = Some((d, c));
                        }
                    }
                }
            }
            x += 6;
        }
        z += 6;
    }
    mejor.map(|(_, c)| c)
}

#[allow(dead_code)]
fn comisaria_cercana_vieja(body: CelestialBody, tx: i64, tz: i64, radio: i64) -> Option<(i64, i64)> {
    let mut mejor: Option<(i64, (i64, i64))> = None;
    let mut z = tz - radio;
    while z <= tz + radio {
        let mut x = tx - radio;
        while x <= tx + radio {
            if let Some(s) = crate::geography::settlement_covering(body, x, z) {
                if let Some(lot) = lot_at(body, &s, x, z) {
                    if lot.program().key.starts_with("building/comisaria") {
                        let c = (lot.min.0 + lot.w / 2, lot.min.1 + lot.h / 2);
                        let d = (c.0 - tx).pow(2) + (c.1 - tz).pow(2);
                        if mejor.map_or(true, |(m, _)| d < m) {
                            mejor = Some((d, c));
                        }
                    }
                }
            }
            x += 6;
        }
        z += 6;
    }
    mejor.map(|(_, c)| c)
}

pub fn pisos_modulares(key: &str, x: i64, z: i64) -> Option<u8> {
    let &(_, min, max) = LOTES_MODULARES.iter().find(|(k, _, _)| *k == key)?;
    if max <= min {
        return Some(min);
    }
    let h = (x.wrapping_mul(0x27d4_eb2d) ^ z.wrapping_mul(0x1656_67b1)).unsigned_abs();
    Some(min + (h % (max - min + 1) as u64) as u8)
}

/// [`pisos_modulares`] con la ciudad en cuenta: **una metrópolis tiene torres
/// más altas en su centro**, y el suburbio, más bajas. Es lo que se ve de lejos
/// —el perfil de la ciudad— y lo primero que dice si es densa o no.
pub fn pisos_modulares_en(body: CelestialBody, key: &str, x: i64, z: i64) -> Option<u8> {
    let base = pisos_modulares(key, x, z)?;
    let Some(s) = crate::geography::settlement_covering(body, x, z) else { return Some(base) };
    let block = block_size(s.style, s.tier);
    let ring = ((x - s.center.0).div_euclid(block).abs()).max((z - s.center.1).div_euclid(block).abs());
    let extra: i32 = match densidad(body, &s) {
        // El corazón del centro, más alto que el resto: sale el perfil en punta.
        Densidad::Metropolis if ring <= anillos_de_zona(body, &s).0 / 2 => 3,
        Densidad::Metropolis => 2,
        Densidad::Suburbio => -1,
        _ => 0,
    };
    Some((base as i32 + extra).max(0) as u8)
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
    LotProgram { key: "building/casa_chica", cells: (26, 17), district: District::Residential, weight: 22 },
    LotProgram { key: "building/casa_mediana", cells: (28, 18), district: District::Residential, weight: 26 },
    LotProgram { key: "building/casa_grande", cells: (31, 22), district: District::Residential, weight: 16 },
    LotProgram { key: "building/casa_country", cells: (42, 24), district: District::Residential, weight: 10 },
    // **La casa EN HILERA**: la que el pack dibujó modular (un tramo de 10 celdas
    // que se repite y un remate de 12 que cierra la fila). La parcela es ancha y
    // poco profunda porque eso ES una hilera, y el cliente compone la fachada del
    // ancho exacto de la huella módulo por módulo (`building::spawn_fachada_modular`),
    // así que acá el tamaño no está limitado por lo que mida un dibujo suelto.
    LotProgram { key: "building/casa_hilera", cells: (46, 22), district: District::Residential, weight: 18 },
    // **`building/villa`, no `building/villa_6`.** `villa_6` es el nombre que el
    // importador masivo le puso a un recorte de UNA CELDA de la hoja de villas
    // (un felpudo, una maceta): la parcela más grande del barrio apuntaba a un
    // sprite de 32×32 px estirado a 30×22 tiles. `building/villa` es el slot
    // curado que recibe las villas de verdad.
    LotProgram { key: "building/villa", cells: (35, 34), district: District::Residential, weight: 6 },
    LotProgram { key: "building/duplex", cells: (39, 24), district: District::Residential, weight: 12 },
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
    // **Acá vivía `building/heladeria_1`, y era un mueble de INTERIORES.** El
    // pack de Modern Interiors trae un tema `Ice_Cream_Shop` con el mostrador, la
    // heladera y el cartel del local *visto por dentro*; el importador masivo los
    // metía en `building/` porque el nombre de archivo dice "shop", y la ciudad
    // terminaba plantando un mostrador de 2×3 celdas en una parcela de la avenida
    // — con el agravante de que la heladería EXTERIOR ya estaba dos líneas más
    // arriba (`edificio/tienda_heladeria`, el local de 9 celdas del kit modular),
    // así que era además un lote duplicado.
    //
    // Ahora los interiores no pueden caer en `building/`
    // (`import-pack-masivo.py::CARPETAS_DE_AFUERA`) y ningún lote puede apuntar a
    // un dibujo de adentro (`art2d::la_ciudad_no_se_construye_con_interiores`).
    LotProgram { key: "building/tienda", cells: (9, 9), district: District::Commercial, weight: 5 },
    LotProgram { key: "building/mercado_chico_1", cells: (14, 12), district: District::Commercial, weight: 8 },
    LotProgram { key: "building/mercado_mediano_1", cells: (18, 14), district: District::Commercial, weight: 8 },
    LotProgram { key: "building/gasolinera", cells: (15, 14), district: District::Commercial, weight: 7 },
    LotProgram { key: "building/shopping", cells: (30, 13), district: District::Commercial, weight: 5 },
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
    LotProgram { key: "building/oficina", cells: (23, 19), district: District::Downtown, weight: 22 },
    LotProgram { key: "building/torre", cells: (16, 23), district: District::Downtown, weight: 18 },
    LotProgram { key: "building/hotel", cells: (30, 22), district: District::Downtown, weight: 8 },
    // La torre del reloj de la estación: el pack la dibujó entera y es el hito
    // que se ve de lejos en las referencias.
    LotProgram { key: "building/reloj_torre", cells: (14, 20), district: District::Downtown, weight: 5 },
    // ── Industria. La central eléctrica con sus molinos es de las referencias:
    // va en el borde, que es donde el pack la dibujó (contra la arboleda).
    LotProgram { key: "building/nave_industrial", cells: (15, 16), district: District::Industrial, weight: 20 },
    LotProgram { key: "building/deposito", cells: (13, 11), district: District::Industrial, weight: 14 },
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
    // El cementerio de la lámina: pasto y filas de lápidas con flores (`worldgen::prefab_de_lote_at`).
    LotProgram { key: "lot/cementerio", cells: (22, 18), district: District::Park, weight: 6 },
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
    LotProgram { key: "building/ayuntamiento", cells: (20, 24), district: District::Civic, weight: 6 },
    // El helipuerto con su helicóptero (lámina de LimeZu: pad pintado + aparato). Es un lote de losa como la
    // plaza: lo compone `worldgen::prefab_de_lote_at`.
    LotProgram { key: "lot/helipuerto", cells: (18, 15), district: District::Park, weight: 5 },
    LotProgram { key: "building/universidad", cells: (32, 32), district: District::Civic, weight: 28 },
    LotProgram { key: "building/escuela", cells: (28, 16), district: District::Civic, weight: 22 },
    LotProgram { key: "building/comisaria", cells: (24, 20), district: District::Civic, weight: 14 },
    LotProgram { key: "building/bomberos", cells: (12, 10), district: District::Civic, weight: 12 },
    LotProgram { key: "building/correo_edificio", cells: (11, 9), district: District::Civic, weight: 10 },
    LotProgram { key: "building/estacion_tren", cells: (11, 7), district: District::Civic, weight: 6 },
];

/// El distrito de una manzana.
///
/// Se decide por ANILLO (distancia al centro en manzanas) más un sorteo, para que
/// haya parques y equipamiento salpicados en vez de tres coronas perfectas. La
/// trama de la ciudad inclina el reparto: una ciudad moderna es más centro, un
/// puerto es más industria.
pub use crate::world_map::Densidad;

/// **Cuán densa es esta ciudad** (ver [`Densidad`]).
///
/// La autorada lo declara en el mapa; la ciudad procedural lo sortea por su
/// centro —mitad ciudades, mitad suburbios—, y pueblos y aldeas son `Pueblo`.
pub fn densidad(body: CelestialBody, s: &Settlement) -> Densidad {
    if s.tier != SettlementTier::City {
        return Densidad::Pueblo;
    }
    if let Some(nombre) = s.name {
        if let Some(t) = crate::world_map::template(body).towns.iter().find(|t| t.name == nombre) {
            return t.densidad;
        }
    }
    if hash2(body_terrain_seed(body) ^ 0xde75_0000, s.center.0, s.center.1) % 2 == 0 {
        Densidad::Ciudad
    } else {
        Densidad::Suburbio
    }
}

/// **Hasta qué anillo de manzanas llega cada zona**: `(centro, comercio,
/// barrio_denso)`. Pasado el último es barrio de casas.
///
/// Antes eran números fijos (el centro = 2 a 4 anillos según la trama) sin
/// mirar el radio, así que las cuatro ciudades de la Tierra —de 1.200 a 2.200
/// tiles de radio, o sea 30 a 50 anillos— eran un centro chiquito rodeado de
/// kilómetros de chalets. Una metrópolis que es 95 % suburbio no se lee como una
/// metrópolis. Ahora las zonas son FRACCIONES del casco, y la densidad de la
/// ciudad decide cuánto.
fn anillos_de_zona(body: CelestialBody, s: &Settlement) -> (i64, i64, i64) {
    let core = match s.style {
        CityStyle::Modern => 4,
        CityStyle::Radial => 3,
        CityStyle::Grid => 3,
        CityStyle::Port => 2,
        CityStyle::Rural => 0,
    };
    let rb = (s.radius / block_size(s.style, s.tier)).max(1);
    let (c, k, d) = match densidad(body, s) {
        Densidad::Metropolis => (30, 50, 72),
        Densidad::Ciudad => (12, 24, 40),
        Densidad::Suburbio => (0, 10, 0),
        Densidad::Pueblo => (0, 0, 0),
    };
    let centro = (rb * c / 100).max(core);
    let comercio = (rb * k / 100).max(centro + 3);
    let denso = (rb * d / 100).max(comercio);
    (centro, comercio, denso)
}

/// ¿Es esta manzana de barrio DENSO? Residencial igual, pero de hileras y
/// dúplex en vez de quintas: es lo que hace que el borde del centro de una
/// metrópolis no salte de golpe de las torres al chalet con pileta.
fn barrio_denso(body: CelestialBody, s: &Settlement, bx: i64, bz: i64) -> bool {
    s.tier == SettlementTier::City && bx.abs().max(bz.abs()) <= anillos_de_zona(body, s).2
}

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
    // Parques y equipamiento: repartidos por toda la ciudad, no en un anillo. La
    // metrópolis gasta menos suelo en plazas; el suburbio, más.
    let (parque, civico) = match densidad(body, s) {
        Densidad::Metropolis => (4, 9),
        Densidad::Suburbio => (9, 13),
        _ => (6, 11),
    };
    match h % 100 {
        x if x <= parque => return District::Park,
        x if x <= civico => return District::Civic,
        _ => {}
    }
    if s.tier != SettlementTier::City {
        // Un pueblo no tiene centro financiero: tiene una plaza y casas.
        return if ring == 0 { District::Commercial } else { District::Residential };
    }
    let (centro, comercio, _) = anillos_de_zona(body, s);
    let rb = s.radius / block_size(s.style, s.tier);
    if ring <= centro {
        District::Downtown
    } else if ring <= comercio {
        District::Commercial
    } else if ring >= (comercio + 6).max(rb * 80 / 100) && h % 7 == 0 {
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
        // 1, 2 o 4: la supermanzana parte de a mitades (`supermanzana`), así
        // que un programa de tres manzanas recibe cuatro.
        let mut n = 1;
        while n * block - margin * 2 < want && n < 4 {
            n *= 2;
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
    let filled = match (s.tier, densidad(body, s)) {
        (SettlementTier::City, Densidad::Metropolis) => 97,
        (SettlementTier::City, Densidad::Suburbio) => 80,
        (SettlementTier::City, _) => 88,
        (SettlementTier::Town, _) => 62,
        (SettlementTier::Hamlet, _) => 34,
    };
    if h % 100 >= filled {
        if (nx, nz) == (1, 1) { diag("1a-densidad"); }
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
    // **El barrio cambia con la densidad.** Mismo distrito residencial, otra
    // mezcla: cerca del centro de una ciudad densa, hileras y dúplex; en el
    // suburbio, quintas y villas. El peso es el de la tabla por un factor, así
    // que la tabla sigue siendo el único lugar donde se leen las proporciones.
    let denso = district == District::Residential && barrio_denso(body, s, ax, az);
    let suburbio = district == District::Residential && !denso && densidad(body, s) == Densidad::Suburbio;
    let peso = |p: &LotProgram| -> u32 {
        let w = p.weight as u32;
        match p.key {
            "building/casa_hilera" | "building/duplex" if denso => w * 3,
            "building/casa_country" | "building/villa" if denso => 0,
            "building/casa_country" | "building/villa" if suburbio => w * 3,
            "building/casa_hilera" if suburbio => w / 3,
            _ => w,
        }
    };
    let total: u32 = LOT_PROGRAMS.iter().filter(|p| cabe(p)).map(peso).sum();
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
        diag("1b-nada-del-distrito-cabe-en-1x1");
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
        let w = peso(p);
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
    if !crate::grueso::activo() {
        MEMO.with(|m| m.borrow_mut()[ranura] = Ranura { clave, valor: v, lleno: true });
    }
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
    if !crate::grueso::activo() {
        MEMO.with(|m| m.borrow_mut()[ranura] = Ranura { clave, valor: v, lleno: true });
    }
    v
}

/// **La franja verde que toda carretera deja a los lados**, en tiles.
///
/// No es estética: la calzada se dibuja DESPUÉS que lo edificado, así que una
/// pared a un tile del asfalto sale con el asfalto encima.
const RETIRO_DE_CARRETERA: f32 = 2.0;

/// **¿Pisa este rectángulo un enlace interurbano?**
///
/// Se barre la parcela en rejilla de tres tiles. Tres y no más: la más angosta
/// de las carreteras entre núcleos es la ruta, que mide 7 tiles de calzada
/// (`RoadClass::Route::half_width` = 3,5), así que ninguna se puede colar entre
/// dos muestras. Y se barre el BORDE además del interior, que es por donde una
/// carretera roza una parcela sin pasarle por el medio.
///
/// Sale caro (unas cien consultas) y por eso no se llama nunca a la ligera:
/// quien pregunta comprueba antes, con UNA consulta al centro, que haya alguna
/// carretera lo bastante cerca como para que la respuesta pueda ser `true`.
fn enlace_cruza(body: CelestialBody, ox: i64, oz: i64, w: i64, hgt: i64) -> bool {
    const PASO: i64 = 3;
    let toca = |x: f32, z: f32| {
        crate::geography::link_clearance(body, x, z)
            .is_some_and(|m| m + RETIRO_DE_CARRETERA >= 0.0)
    };
    let mut dz = 0;
    loop {
        let mut dx = 0;
        loop {
            if toca((ox + dx) as f32, (oz + dz) as f32) {
                return true;
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
    false
}

fn calcular_retiro(body: CelestialBody, ox: i64, oz: i64, w: i64, hgt: i64) -> Retiro {
    const PASO: i64 = 4;
    // Rejilla de muestras del rectángulo: qué escalón hay en cada una, o `None`
    // si es líquido. Las coordenadas de la muestra `i` se recuperan con
    // `muestra()`, que es la misma cuenta que llena la rejilla.
    let muestra = |i: i64, largo: i64| (i * PASO).min(largo - 1);
    let nx = (w + PASO - 1) / PASO;
    let nz = (hgt + PASO - 1) / PASO;
    let mut nivel = Vec::with_capacity((nx * nz) as usize);
    for j in 0..nz {
        for i in 0..nx {
            let n = crate::relief::relief_level(body, ox + muestra(i, w), oz + muestra(j, hgt));
            nivel.push(if n < 0 { None } else { Some(n) });
        }
    }
    let en = |i: i64, j: i64| nivel[(j * nx + i) as usize];
    // Todo el rectángulo en el mismo escalón seco: el caso normal, y se contesta
    // sin buscar nada.
    if let Some(n0) = en(0, 0) {
        if nivel.iter().all(|&n| n == Some(n0)) {
            return Retiro::Sitio((ox, oz, w, hgt));
        }
    }

    // **EL RECTÁNGULO LLANO MÁS GRANDE QUE HAY DENTRO**, no el mejor de cuatro
    // cortes.
    //
    // Lo que había probaba cuatro recortes del rectángulo entero —quedarse con
    // lo de antes o lo de después del desnivel, en cada eje— y si ninguno de los
    // cuatro llegaba al lado mínimo, la parcela se descartaba. Con un borde de
    // terraza entrando EN DIAGONAL (que es como entran casi todos) los cuatro
    // cortes salen chicos a la vez aunque quede media manzana llana en una
    // esquina, y la manzana se quedaba sin nada. Medido sobre las cuatro
    // ciudades de la Tierra, era **la primera causa de baldío**: 31 de las 72
    // manzanas vacías de Ciudad Central y 30 de las 57 de Villa Sur — más que la
    // densidad, más que el agua y más que la carretera juntas. Es exactamente lo
    // que el jugador ve como "parcelas que rechazan casas por falta de espacio".
    //
    // Ahora se busca el rectángulo máximo de un solo escalón dentro de la
    // rejilla de muestras (histograma por filas, el algoritmo de siempre): sobre
    // 14×14 muestras cuesta microsegundos y lo paga una sola vez por parcela
    // gracias a la memo de `retiro_del_desnivel`.
    let mut mejor: Option<(i64, i64, i64, i64)> = None; // (i0, j0, i1, j1) inclusive
    let mut mejor_area = 0i64;
    let mut niveles: Vec<i8> = nivel.iter().flatten().copied().collect();
    niveles.sort_unstable();
    niveles.dedup();
    for objetivo in niveles {
        // Altura de la columna de celdas buenas que termina en esta fila.
        let mut alto = vec![0i64; nx as usize];
        for j in 0..nz {
            for i in 0..nx {
                alto[i as usize] =
                    if en(i, j) == Some(objetivo) { alto[i as usize] + 1 } else { 0 };
            }
            // Rectángulo máximo en el histograma de esta fila, a lo bruto: `nx`
            // es 14 como mucho, así que el cuadrático es más barato que la pila.
            for i in 0..nx {
                let mut h = alto[i as usize];
                for k in i..nx {
                    h = h.min(alto[k as usize]);
                    if h == 0 {
                        break;
                    }
                    let area = (k - i + 1) * h;
                    if area > mejor_area {
                        mejor_area = area;
                        mejor = Some((i, j - h + 1, k, j));
                    }
                }
            }
        }
    }
    let Some((i0, j0, i1, j1)) = mejor else {
        // Ni una muestra seca: esto es agua, no una parcela con un charco.
        return Retiro::Mojado;
    };

    // De muestras a tiles. **Los bordes que se CORTARON se retiran un tile más**:
    // entre dos muestras hay cuatro celdas sin mirar, y el borde de la terraza
    // está justo ahí. Los bordes que coinciden con el filo del rectángulo no se
    // tocan — ahí no hay nada que esquivar.
    let borde = |i: i64, largo: i64, n: i64, hacia_arriba: bool| {
        let t = muestra(i, largo);
        if hacia_arriba {
            if i == n - 1 { largo - 1 } else { t - 1 }
        } else if i == 0 {
            0
        } else {
            t + 1
        }
    };
    let x0 = borde(i0, w, nx, false);
    let x1 = borde(i1, w, nx, true);
    let z0 = borde(j0, hgt, nz, false);
    let z1 = borde(j1, hgt, nz, true);
    let (nw, nh) = (x1 - x0 + 1, z1 - z0 + 1);
    if nw < LADO_MINIMO || nh < LADO_MINIMO {
        return Retiro::SinSitio;
    }
    Retiro::Sitio((ox + x0, oz + z0, nw, nh))
}

thread_local! {
    pub static DIAG: std::cell::RefCell<std::collections::BTreeMap<&'static str, u32>> =
        std::cell::RefCell::new(std::collections::BTreeMap::new());
}
fn diag(k: &'static str) {
    if std::env::var("HA_DIAG").is_ok() {
        DIAG.with(|d| *d.borrow_mut().entry(k).or_insert(0) += 1);
    }
}
pub fn diag_dump() -> String {
    DIAG.with(|d| format!("{:?}", d.borrow()))
}
pub fn diag_clear() { DIAG.with(|d| d.borrow_mut().clear()); }

/// **La SUPERMANZANA de la manzana `(bx, bz)`: `(ax, az, nx, nz)`.**
///
/// Un programa más ancho que el hueco de una manzana (casa grande, dúplex, villa,
/// hilera, country, hospital…) reserva varias, y la calle que las separaba tiene
/// que DESAPARECER: si no, la casa queda a caballo del asfalto —la calzada se
/// dibuja encima de lo edificado— y se ve partida al medio. Era "las casas están
/// sobre las calles": medido el 25 ago 2026, el 14 % de las celdas edificadas de
/// Metrópolis Norte tenía calzada encima (`ninguna_pared_de_ciudad_se_planta_en_la_calzada`).
///
/// La calle la pinta `geography::urban_civil`, que **no puede mirar el terreno**
/// (el relieve la consulta a ella) y por lo tanto no puede preguntarle a
/// [`lot_at`]. Por eso esto es una PARTICIÓN de la retícula que sale sólo de
/// hashes: cada bloque de 4×4 manzanas alineado se parte en rectángulos de 1, 2
/// o 4 de lado, y un rectángulo se queda entero si el sorteo de su programa
/// (`program_at_anchor`, que tampoco mira el terreno) le da un edificio de
/// exactamente ese tamaño. La calle vive donde dos manzanas vecinas caen en
/// supermanzanas distintas, y como la partición es de rectángulos, nunca queda
/// un tramo de calle colgando sin salida.
///
/// Si después el terreno no deja plantar el programa grande (agua, desnivel), la
/// supermanzana se queda igual sin calle interior y se llena de parcelas sueltas
/// ([`lot_at`]): una calle de menos no rompe nada; una casa partida, sí.
pub fn supermanzana(body: CelestialBody, s: &Settlement, bx: i64, bz: i64) -> (i64, i64, i64, i64) {
    const RANURAS: usize = 256;
    type Clave = (u8, i64, i64, i64, i64, u32);
    thread_local! {
        static MEMO: std::cell::RefCell<[Option<(Clave, (i64, i64, i64, i64))>; RANURAS]> =
            const { std::cell::RefCell::new([None; RANURAS]) };
    }
    let clave: Clave = (body as u8, s.center.0, s.center.1, bx, bz, crate::partida::epoca());
    let ranura = (hash2(0x5ae7_0000, s.center.0 + bx, s.center.1 + bz) as usize) % RANURAS;
    if let Some(v) = MEMO.with(|m| m.borrow()[ranura].and_then(|(c, v)| (c == clave).then_some(v))) {
        return v;
    }
    let v = supermanzana_sin_memo(body, s, bx, bz);
    if !crate::grueso::activo() {
        MEMO.with(|m| m.borrow_mut()[ranura] = Some((clave, v)));
    }
    v
}

fn supermanzana_sin_memo(body: CelestialBody, s: &Settlement, bx: i64, bz: i64) -> (i64, i64, i64, i64) {
    let block = block_size(s.style, s.tier);
    let (cx, cz) = s.center;
    let libre = s.radius - crate::roads::ring_class(s.tier).half_shoulder().ceil() as i64;
    let seed = body_terrain_seed(body) ^ 0x5ae7_1000;
    let (mut ax, mut az, mut nx, mut nz) = (bx - bx.rem_euclid(4), bz - bz.rem_euclid(4), 4i64, 4i64);
    loop {
        if nx * nz == 1 {
            return (ax, az, 1, 1);
        }
        // Entera dentro del casco: una supermanzana que cruza la circunvalación
        // borraría calles que el lote después no puede ocupar.
        let dentro = [ax, ax + nx].iter().all(|&i| (i * block).abs() <= libre)
            && [az, az + nz].iter().all(|&j| (j * block).abs() <= libre);
        if dentro {
            let margin = bounding_margin(s.style, ax, az, nx, nz);
            if program_at_anchor(body, s, ax, az, nx, nz, block, margin).is_some() {
                return (ax, az, nx, nz);
            }
        }
        // Se parte por la mitad; el eje lo sortea el rectángulo, así salen tiras
        // (4×1, 2×1) además de cuadrados.
        let h = hash2(seed, cx + ax * 5 + nx, cz + az * 5 + nz);
        let por_x = if nx > 1 && nz > 1 { h & 1 == 0 } else { nx > 1 };
        if por_x {
            nx /= 2;
            if bx >= ax + nx {
                ax += nx;
            }
        } else {
            nz /= 2;
            if bz >= az + nz {
                az += nz;
            }
        }
    }
}

/// El lote que ocupa esta celda, si hay alguno.
///
/// Primero el programa de su SUPERMANZANA ([`supermanzana`]) si abarca varias
/// manzanas; si no se pudo plantar o no llega hasta esta celda, el de la
/// manzana suelta, mientras no pise al grande.
///
/// **Reservar no es ocupar.** Un hospital reserva 2×2 manzanas y llena una
/// parte: el resto recibe su propio lote chico, mientras no se pisen.
///
/// Da la misma respuesta para todas las celdas de una parcela porque todas caen
/// en la misma supermanzana y en la misma manzana: nada de lo que se decide
/// depende de qué celda pregunta.
pub fn lot_at(body: CelestialBody, s: &Settlement, tx: i64, tz: i64) -> Option<Lot> {
    let block = block_size(s.style, s.tier);
    let (cx, cz) = s.center;
    let bx = (tx - cx).div_euclid(block);
    let bz = (tz - cz).div_euclid(block);
    let (ax, az, nx, nz) = supermanzana(body, s, bx, bz);
    let mut mayor = None;
    if nx * nz > 1 {
        if let Some(l) = candidato(body, s, ax, az, nx, nz, block) {
            if l.contains(tx, tz) {
                return Some(l);
            }
            diag("6-no-contiene-la-celda");
            mayor = Some(l);
        }
    }
    let l = candidato(body, s, bx, bz, 1, 1, block)?;
    if mayor.is_some_and(|m| l.overlaps(&m)) {
        diag("3-pisa-mayor");
        return None;
    }
    l.contains(tx, tz).then_some(l)
}

/// El lote que el programa del ancla `(ax, az)` planta en sus `nx × nz`
/// manzanas, con todas las pruebas de terreno hechas. No mira qué celda
/// pregunta: depende sólo del rectángulo.
fn candidato(
    body: CelestialBody,
    s: &Settlement,
    ax: i64,
    az: i64,
    nx: i64,
    nz: i64,
    block: i64,
) -> Option<Lot> {
    let (cx, cz) = s.center;
    // **El margen sale de la calle MÁS ANCHA que rodea al lote**, no sólo de la
    // del bloque ancla: un lote limita con cuatro calles y cualquiera puede ser
    // una avenida.
    let margin = bounding_margin(s.style, ax, az, nx, nz);
    let Some(prog) = program_at_anchor(body, s, ax, az, nx, nz, block, margin) else {
        if (nx, nz) == (1, 1) {
            diag("1-sorteo-vacio-1x1");
        }
        return None;
    };
    let p = &LOT_PROGRAMS[prog];
    // **EL SITIO LLANO ES EL DE LA MANZANA, NO EL DEL PROGRAMA.** Se le pregunta
    // al hueco edificable ENTERO dónde está su parte llana
    // (`retiro_del_desnivel`, el rectángulo llano más grande) y el programa se
    // planta DENTRO de ella: si el borde de terraza pasaba por la esquina
    // noroeste, la parcela moría aunque el resto fuera una explanada.
    let ox0 = cx + ax * block + margin;
    let oz0 = cz + az * block + margin;
    let uw = (nx * block - margin * 2).max(4);
    let uh = (nz * block - margin * 2).max(4);
    let (ox, oz, sw, sh) = match retiro_del_desnivel(body, ox0, oz0, uw, uh) {
        Retiro::Mojado => {
            diag("4-mojado");
            return None;
        }
        Retiro::SinSitio => {
            diag("5-sin-sitio-desnivel");
            return None;
        }
        Retiro::Sitio(r) => r,
    };
    let w = p.cells.0.min(sw).max(4);
    let hgt = p.cells.1.min(sh).max(4);
    // **Y la parcela se apoya en SU CALLE**: contra el borde sur, que es la cara
    // que `residential_tile` trata como frente, y centrada a lo ancho. Clavada en
    // la esquina noroeste quedaba una franja de pasto de hasta veinte tiles entre
    // la casa y la vereda.
    let ox = ox + (sw - w) / 2;
    let oz = oz + (sh - hgt);
    // Fuera del casco no hay ciudad — y el lote ENTERO tiene que caber, con su
    // retiro de la circunvalación.
    let libre = s.radius - crate::roads::ring_class(s.tier).half_shoulder() as i64;
    if (ox - cx).abs() > libre
        || (oz - cz).abs() > libre
        || (ox + w - cx).abs() > libre
        || (oz + hgt - cz).abs() > libre
    {
        diag("2-fuera-del-casco");
        return None;
    }
    let lot = Lot { min: (ox, oz), w, h: hgt, program: prog };
    // **La carretera pasa antes que la parcela.** El enlace interurbano entra al
    // casco y se dibuja DESPUÉS de lo edificado, así que partiría la casa. El
    // círculo que envuelve al lote es sólo el primer filtro (una consulta);
    // cuando avisa, se mira el RECTÁNGULO de verdad ([`enlace_cruza`]).
    let radio = (((w * w + hgt * hgt) as f32).sqrt()) * 0.5;
    let (mx, mz) = (ox as f32 + w as f32 * 0.5, oz as f32 + hgt as f32 * 0.5);
    if let Some(margen) = crate::geography::link_clearance(body, mx, mz) {
        if margen + radio >= 0.0 && enlace_cruza(body, ox, oz, w, hgt) {
            diag("7-carretera-interurbana");
            return None;
        }
    }
    // **Y la CASA no se planta dentro del río.** El retiro mira el mar y los
    // lagos del relieve; el río lo traza `crate::rios` y sólo lo ve el tile
    // natural. Se mira la huella de la CASA y no el lote: un jardín puede dar al
    // río; una casa, no.
    if p.district == District::Residential && casa_mojada(body, &lot) {
        diag("8-casa-mojada");
        return None;
    }
    Some(lot)
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

/// Parque: losa, cancha, arboleda o natatorio. Nunca paredes.
fn park_tile(p: &LotProgram, lot: Lot, lx: i64, lz: i64, _h: u32) -> Option<Tile> {
    match p.key {
        // La arboleda es terreno natural: los árboles los siembra la decoración.
        "lot/arboleda" | "lot/cementerio" => None,
        // **EL NATATORIO: agua de verdad, no una losa celeste.** El vaso es
        // `Tile::Water` —el líquido de poco fondo que se NADA, el mismo de la
        // pileta del patio de una casa—, así que el natatorio del barrio se USA:
        // te metés y nadás. Alrededor
        // queda un andén de baldosa de dos tiles —donde la decoración pone las
        // reposeras y el trampolín— y el borde exterior marca el recinto igual
        // que en una cancha.
        "lot/natatorio" => {
            // **Piletas, no un lago**: el vaso es una grilla de piletas de 6×4 (la misma del patio de las casas,
            // `agua/pileta_completa`: borde de piedra con esquinas coherentes) separadas por baldosa. Un solo
            // rectángulo de 20×14 de agua se dibujaba con la orilla de un río, y eso no es una pileta.
            let (w, d) = (lot.w, lot.h);
            let (nx, nz) = ((w - 6 + 2) / 8, (d - 6 + 2) / 6);
            let (ox, oz) = (3 + ((w - 6) - (8 * nx - 2)) / 2, 3 + ((d - 6) - (6 * nz - 2)) / 2);
            let (u, v) = (lx - ox, lz - oz);
            if u < 0 || v < 0 || u >= 8 * nx - 2 || v >= 6 * nz - 2 || u % 8 >= 6 || v % 6 >= 4 {
                return Some(Tile::Pavement);
            }
            let (ru, rv) = (u % 8, v % 6);
            let rim = ru == 0 || rv == 0 || ru == 5 || rv == 3;
            Some(if rim { Tile::Pavement } else { Tile::Water })
        }
        // La plaza de la lámina es de baldosa clara de punta a punta (el basalto oscuro es de las canchas).
        "lot/plaza" | "lot/helipuerto" => Some(Tile::Pavement),
        // Cancha: una losa con borde. El borde es lo que la separa del
        // césped y hace que se lea como una cancha y no como un claro.
        _ => {
            let edge = lx == 0 || lz == 0 || lx == lot.w - 1 || lz == lot.h - 1;
            Some(if edge { Tile::Pavement } else { Tile::Basalt })
        }
    }
}

/// **Cuánto mide UNA vivienda de la hilera, en tiles.**
///
/// Una hilera no es una casa ancha: son varias casas pegadas, cada una con su
/// puerta y su interior. Pintar tres módulos encima de una sola vivienda —que es
/// lo que se hacía— deja el exterior contando tres casas y el interior una: el
/// usuario lo vio de una mirada.
///
/// El número es el módulo del pack (10 celdas × 0,95 m/celda ≈ 9,5 m) redondeado
/// a la grilla del mundo. Que exista acá y no en el cliente es lo que hace que
/// las dos mitades cuenten las mismas casas: el sim divide la hilera en tramos de
/// este ancho y el cliente dibuja UN módulo por tramo.
pub const ANCHO_DE_HILERA: i64 = 10;

/// **Por dónde se entra a este edificio desde el frente**, en x absoluto.
///
/// Existía como "el medio de la fachada" escrito a mano en `geography::building_at`
/// —`x + w / 2`— y con la hilera dejó de ser cierto: ahí cae la MEDIANERA entre la
/// primera y la segunda vivienda, o sea una pared. Lo cazó
/// `settlements_are_deterministic_and_earth_only`. La respuesta la tiene que dar
/// quien coloca las puertas, no quien las busca.
pub fn puerta_del_frente(clave: &str, x: i64, w: i64) -> i64 {
    if es_hilera(clave) {
        // La primera vivienda de la fila: su puerta está en el medio de SU tramo.
        x + ANCHO_DE_HILERA / 2 - 1
    } else if es_modular(clave) && w % crate::edificio::MODULO == 0 {
        // Edificio modular: una puerta por módulo (ver `plain_building_tile`); la de referencia es la del módulo del medio.
        let m = crate::edificio::MODULO;
        x + (w / m / 2) * m + m / 2
    } else {
        x + w / 2
    }
}

/// ¿Este programa de lote se compone con el kit modular (una puerta por módulo)?
pub fn es_modular(key: &str) -> bool {
    LOTES_MODULARES.iter().any(|(k, _, _)| *k == key)
}

/// ¿Este programa de lote es una hilera de viviendas pegadas?
pub fn es_hilera(key: &str) -> bool {
    key == "building/casa_hilera"
}

/// Tiles de ancho que hay que dejar al costado de la casa para la pileta (6 de pileta + 2 de aire).
const LUGAR_DE_PILETA: i64 = 8;

/// ¿Esta parcela lleva pileta? (si tiene lugar: ver `sitio_de_pileta`). Un 55 %: es un barrio, no un balneario.
fn quiere_pileta(h: u32) -> bool {
    (h >> 5) % 100 < 55
}

/// **El JARDÍN de una parcela: el lote menos su retiro**, en coordenadas del lote.
///
/// El lote reservado llega hasta la vereda, y hasta hoy la cerca se plantaba
/// justo en ese filo: `lx == 0 || lz == 0 || …`. Dos problemas, y el usuario vio
/// los dos.
///
/// 1. **La cerca terminaba sobre la calle.** El margen del lote sale de la calle
///    más ancha que lo rodea (`bounding_margin`), así que del lado de la calle
///    angosta el filo del lote queda pegado al asfalto — y ahí aparecía el
///    cerramiento, cruzado en la calzada.
/// 2. **Todas las parcelas medían lo mismo.** El rectángulo es `p.cells` acotado
///    a la manzana, o sea el mismo número para las mil casas del barrio: una
///    grilla de cuadrados idénticos.
///
/// El retiro resuelve los dos de una: es **distinto en cada lado y en cada lote**
/// (0 a 3 tiles, sorteados de bits distintos del hash), así que la cerca nunca
/// nace en el filo y dos parcelas vecinas no miden lo mismo aunque el programa
/// sea el mismo. Lo que queda afuera del jardín es terreno natural, que es lo que
/// hace que el barrio se lea irregular sin tocar el trazado de calles.
///
/// Se achica hasta donde la casa siga entrando: una parcela sin casa no es una
/// parcela.
fn jardin_de_la_parcela(lot: Lot, h: u32) -> Parte {
    let (w, d) = (lot.w, lot.h);
    // Mínimo en el que todavía entra la casa más chica con su patio: la del DIBUJO más chico del programa
    // (`huellas_de_programa`) más lugar para el camino al frente y un borde al costado. Sin dibujos medidos
    // (la hilera), el mínimo de siempre.
    let (min_w, min_d) = huellas_de_programa(lot.program().key)
        .first()
        .map(|&(hw, hd)| ((hw + 4 + if quiere_pileta(h) { LUGAR_DE_PILETA } else { 0 }).max(12), (hd + 5).max(10)))
        .unwrap_or((12, 10));
    let mut oeste = ((h >> 19) % 4) as i64;
    let mut este = ((h >> 21) % 4) as i64;
    let mut norte = ((h >> 23) % 4) as i64;
    // El frente se retira menos: es la cara que mira a la calle y la que da la
    // línea del barrio. Retirarlo mucho deja la casa flotando en el medio.
    let mut sur = ((h >> 25) % 2) as i64;
    while w - oeste - este < min_w && (oeste > 0 || este > 0) {
        if oeste >= este {
            oeste -= 1;
        } else {
            este -= 1;
        }
    }
    while d - norte - sur < min_d && (norte > 0 || sur > 0) {
        if norte >= sur {
            norte -= 1;
        } else {
            sur -= 1;
        }
    }
    (oeste, norte, (w - oeste - este).max(4), (d - norte - sur).max(4))
}

/// **Dónde cae esta celda en el borde de un rectángulo**, contando en sentido
/// horario desde la esquina noroeste. `None` = no está en el borde.
///
/// Es lo que permite que una cerca cubra **un tramo** del perímetro y no siempre
/// el perímetro entero: con el borde convertido en una recta de largo
/// `2·(w-1) + 2·(d-1)`, "media cerca" o "un cuarto" es un intervalo, y las
/// esquinas salen solas.
fn indice_de_borde(w: i64, d: i64, lx: i64, lz: i64) -> Option<i64> {
    let (a, b) = (w - 1, d - 1);
    if lz == 0 {
        Some(lx)
    } else if lx == w - 1 {
        Some(a + lz)
    } else if lz == d - 1 {
        Some(a + b + (a - lx))
    } else if lx == 0 {
        Some(a + b + a + (b - lz))
    } else {
        None
    }
}

/// **Un rectángulo edificado dentro de una parcela**, en coordenadas del lote:
/// `(x, z, ancho, fondo)`.
pub type Parte = (i64, i64, i64, i64);

/// Escala con la que el juego dibuja el pack (`ha-game::art2d::ESCALA_ARTE`): un test del cliente exige que
/// sean el mismo número. Vive acá porque la sim tiene que saber cuánto mide un dibujo sin cargar arte.
pub const ESCALA_ARTE_DEL_PACK: f32 = 0.95;

/// **Qué rectángulo del suelo ocupa una casa cuyo dibujo mide `w × h` celdas del pack.**
///
/// Pedido del usuario (3 oct 2026): *la casa ocupa SU parte de la parcela, el interior ocupa esa misma parte y
/// el resto es jardín.* Antes la huella era un 55 % × 50 % del jardín sin mirar el dibujo, y la fachada se
/// estiraba para taparla. Ahora el ancho ES el del dibujo (con la escala del pack) y el fondo es la porción de
/// suelo de la casa: el dibujo 3/4 es techo + pared de frente, así que sólo ~60 % de su alto es suelo.
/// Lo usan la sim (`partes_de_la_parcela`) y el cliente (`building::elegir_fachada`): **tienen que dar lo mismo**.
pub fn huella_de_dibujo(w_celdas: i64, h_celdas: i64) -> (i64, i64) {
    let w = (w_celdas as f32 * ESCALA_ARTE_DEL_PACK).round() as i64;
    let d = (h_celdas as f32 * ESCALA_ARTE_DEL_PACK * 0.6).round() as i64;
    (w.max(6), d.max(5))
}

/// Las huellas posibles de un programa residencial (una por tamaño de dibujo distinto), de la más chica a la
/// más grande. Vacío si el programa no tiene casas medidas (la hilera, por ejemplo, se arma de a módulos).
pub fn huellas_de_programa(key: &str) -> Vec<(i64, i64)> {
    let mut v: Vec<(i64, i64)> = crate::casas_medidas::CASAS_DEL_PACK
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, ms)| ms.iter().map(|&(w, h)| huella_de_dibujo(w, h)).collect())
        .unwrap_or_default();
    v.sort();
    v.dedup();
    v
}

/// **Qué casa del pack se levanta en esta parcela**: `(clave del programa cuyo arte se dibuja, huella)`.
///
/// Se sortea entre los dibujos del propio programa que entran en el jardín. Si el terreno recortó el lote y
/// ninguno entra (12×14 cuando el programa pide 17), se usa la casa más grande de CUALQUIER programa
/// residencial que sí entre: mejor otra casa entera que la misma estirada o achicada. `None` = ni la más
/// chica entra (se cae a la huella vieja). El cliente dibuja la variante de `clave` cuya huella coincide.
pub fn casa_elegida(lot: Lot, h: u32, w: i64, d: i64) -> Option<(&'static str, (i64, i64))> {
    let entra = |&(hw, hd): &(i64, i64)| hw <= w - 3 && hd <= d - 5;
    let propia = lot.program().key;
    let propias: Vec<(i64, i64)> = huellas_de_programa(propia).into_iter().filter(entra).collect();
    if !propias.is_empty() {
        return Some((propia, propias[((h >> 15) as usize) % propias.len()]));
    }
    // Otro programa: la huella más grande que entre (la más parecida a lo que el programa quería).
    LOT_PROGRAMS
        .iter()
        .filter(|p| p.district == District::Residential && !es_hilera(p.key))
        .flat_map(|p| huellas_de_programa(p.key).into_iter().filter(entra).map(move |hu| (p.key, hu)))
        .max_by_key(|&(k, (hw, hd))| (hw * hd, std::cmp::Reverse(k)))
}

/// **Dónde cae la casa de una parcela residencial, y por dónde entra el auto.**
///
/// Existe porque la cuenta estaba ESCRITA DOS VECES —en `residential_tile`, que
/// pinta los tiles, y en `roofed_rect`, que le pone la tapa— con un comentario al
/// lado avisando que tenían que dar lo mismo. Un comentario no es un mecanismo:
/// ahora dan lo mismo porque son la misma cuenta.
///
/// **El garaje ya no es una caja aparte.** Era un rectángulo de 6×5 con paredes
/// de ladrillo, piso de madera y un portón, plantado en el jardín — y como
/// `roofed_rect` sólo tapa el edificio principal, se dibujaba SIN TAPA: desde la
/// vereda se veía el piso de adentro. Ése es el "sobresalía un borde" del
/// reporte, y era literal.
///
/// Se podría haberle puesto una tapa, pero no hay con qué: Modern Exteriors no
/// dibuja un galpón de 6×5 en ningún lado — dibuja **casas con el portón en la
/// fachada** (`Terraced_House_3`, `casa_chica#3`, media docena más). Así que el
/// juego se adapta al arte, que es la regla: el garaje pasa a ser el portón que
/// la casa ya tiene dibujado, y de él sale la ENTRADA DE AUTO hasta la calle.
/// Lo que se pierde es un cuarto de 4×3 al que casi nadie entraba; lo que se gana
/// es que el barrio deja de tener una caja destapada por parcela.
///
/// El segundo valor es el tramo `[x0, x1)` del frente de la casa por donde entra
/// el auto, en coordenadas del lote.
pub fn partes_de_la_parcela(lot: Lot, h: u32) -> (Parte, Option<(i64, i64)>) {
    // Todo se mide contra el JARDÍN (el lote menos su retiro), no contra el lote:
    // si la casa se colocara respecto del lote, con un retiro grande quedaría
    // pisando su propia cerca. Ver `jardin_de_la_parcela`.
    let (jx, jz, w, d) = jardin_de_la_parcela(lot, h);
    // **La casa mide lo que mide su dibujo** (`casa_elegida`); si ni la más chica entra, la huella vieja.
    let del_dibujo = casa_elegida(lot, h, w, d).map(|(_, huella)| huella);
    let (mut house_w, house_d) = del_dibujo.unwrap_or_else(|| lot.program().huella_de(w, d));
    // Una hilera mide un número ENTERO de viviendas: si sobra media, el cliente
    // dibujaría un módulo cortado sobre un interior que no existe.
    if es_hilera(lot.program().key) {
        // Se REDONDEA HACIA ABAJO, nunca hacia arriba: forzar un mínimo de dos
        // viviendas hacía crecer la casa más allá del jardín, así que su fila del
        // frente —con las puertas— caía fuera del lote y la casa quedaba sellada
        // (lo cazó `toda_casa_generada_tiene_por_donde_entrar`). Si no entra ni un
        // módulo entero, se deja como está: una hilera de una vivienda es una casa
        // adosada, que es exactamente lo que dibuja el remate.
        let n = house_w / ANCHO_DE_HILERA;
        if n >= 1 {
            house_w = n * ANCHO_DE_HILERA;
        }
    }
    let (house_w, house_d) = (house_w, house_d);
    // El `max` del techo no es paranoia: `clamp(a, b)` con `b < a` es un panic,
    // y desde que la parcela se RETIRA de los desniveles (`lot_at`) el fondo
    // puede bajar hasta `LADO_MINIMO`.
    // **Con pileta, la casa se arrima a un costado** para que el jardín que sobra quede JUNTO (una pileta no
    // entra en dos mitades de 4 tiles). Sin pileta o sin lugar, la casa cae donde diga el hash, como siempre.
    let house_x = if quiere_pileta(h) && w - house_w >= 4 + LUGAR_DE_PILETA {
        if (h >> 6) & 1 == 0 { jx + 2 } else { jx + w - house_w - 2 }
    } else {
        jx + 2 + ((h >> 11) as i64).rem_euclid((w - house_w - 3).max(1))
    };
    let casa = (house_x, jz + 2, house_w, house_d);
    let _ = d;
    // Cada extra se sortea aparte: así hay casas con entrada de auto y sin
    // pileta, con pileta y sin entrada, y ninguna manzana repite la de al lado.
    let hay_garaje = w >= 16 && (h >> 2) % 100 < 62;
    // El portón va contra una punta del frente —nunca en el medio, que es donde
    // está la puerta de la gente— y mide dos tiles, como el portal.
    let cochera = hay_garaje.then(|| {
        if (h >> 3) & 1 == 0 {
            (house_x + 1, house_x + 3)
        } else {
            (house_x + house_w - 3, house_x + house_w - 1)
        }
    });
    (casa, cochera)
}

/// **La parcela residencial**: casa + garaje + entrada + patio + pileta + cerca.
/// El corazón de que un barrio se vea habitado.
/// Lado mínimo de un cuarto, en tiles. Menos que esto no es una habitación: no
/// entra una cama y una persona al lado, que es la prueba de fuego del pack
/// (`interior_siembra` coloca muebles de hasta 3 tiles).
const LADO_MINIMO_DE_CUARTO: i64 = 4;

/// Cuántas veces como mucho se parte una planta. Un chalet no tiene catorce
/// cuartos, y el corte cuesta una vuelta de bucle por celda consultada.
const CORTES_MAXIMOS: u32 = 4;

/// **Los TABIQUES de una planta: dónde la casa se parte en cuartos.**
///
/// Devuelve qué hay en la celda `(rx, rz)` del interior de una planta de
/// `w × d` tiles — `None` si ahí no hay tabique y la celda es piso.
///
/// ## Por qué existe
///
/// Hasta ahora el interior de toda casa generada era **un galpón**: el
/// perímetro de pared y, adentro, `WoodFloor` de punta a punta
/// (`if !perimeter { return Some(Tile::WoodFloor) }`). Una casa de campo de
/// 23×12 era un salón de 21×10 sin una sola división. Eso es lo que dejaba a las
/// 54 paredes de interior del pack sin ningún lugar donde dibujarse: no es que
/// faltara el pixel, es que **no había una sola pared de interior en el mundo**.
///
/// ## Cómo parte
///
/// Partición binaria: se corta el rect por su lado largo, se deja un vano en el
/// tabique, y se sigue dentro de la mitad donde cae la celda que preguntó. Es
/// iterativo y sin memoria — la misma celda contesta lo mismo siempre, que es lo
/// que se necesita cuando quien pregunta es un `topdown_tile` por celda.
///
/// Se corta por el lado LARGO para que no salgan pasillos: partir siempre por el
/// mismo eje deja tiras de 2 tiles de ancho, que no son cuartos.
///
/// El vano es un `Tile::Door` y no un hueco pelado a propósito: el pack dibuja la
/// pared CON el hueco (`doorway/int_*`, la hoja de arcos), y una abertura sin
/// marco se lee como una pared rota.
/// El primer corte libre a partir de `base`, girando dentro de `[lo, hi]`.
/// `None` = todo el rango está vedado y este rect no se parte.
fn corte_libre(lo: i64, hi: i64, base: u32, veda: &[i64]) -> Option<i64> {
    if hi < lo {
        return None;
    }
    let n = (hi - lo + 1) as u32;
    (0..n)
        .map(|k| lo + ((base % n + k) % n) as i64)
        .find(|c| !veda.contains(c))
}

/// La fila (o columna) donde se abre el vano de un tabique: sorteada entre las
/// que no están vedadas, y nunca pegada a la esquina.
fn vano_libre(lo: i64, hi: i64, base: u32, veda: &[i64]) -> i64 {
    let libres: Vec<i64> = (lo..hi).filter(|c| !veda.contains(c)).collect();
    if libres.is_empty() {
        return lo;
    }
    libres[base as usize % libres.len()]
}

/// **Los TABIQUES de una planta: dónde la casa se parte en cuartos.**
///
/// Devuelve qué hay en la celda `(rx, rz)` del interior de una planta de
/// `w × d` tiles — `None` si ahí no hay tabique y la celda es piso.
///
/// ## Por qué existe
///
/// Hasta ahora el interior de toda casa generada era **un galpón**: el
/// perímetro de pared y, adentro, `WoodFloor` de punta a punta. Una casa de
/// campo de 23×12 era un salón de 21×10 sin una sola división. Eso es lo que
/// dejaba a las 54 paredes de interior del pack sin ningún lugar donde
/// dibujarse: no faltaba el pixel, es que **no había una sola pared de interior
/// en el mundo**.
///
/// ## Cómo parte
///
/// Partición binaria: se corta el rect por su lado largo, se deja un vano en el
/// tabique, y se sigue dentro de la mitad donde cae la celda que preguntó. Es
/// iterativo y sin memoria — la misma celda contesta lo mismo siempre, que es lo
/// que hace falta cuando quien pregunta es un `topdown_tile` por celda.
///
/// Se corta por el lado LARGO para que no salgan pasillos: partir siempre por el
/// mismo eje deja tiras de 2 tiles de ancho, que no son cuartos.
///
/// ## La VEDA, que es lo único delicado
///
/// Un vano abre contra la fila que tiene enfrente. Si un corte posterior planta
/// su tabique **justo en esa fila**, el vano queda abriendo a una pared y el
/// cuarto se sella — medido: 2.613 de 16.000 plantas quedaban con habitaciones
/// inalcanzables antes de esto. Así que cada vano veda su coordenada para todos
/// los cortes que vengan después, y un corte que sólo pueda caer en una
/// coordenada vedada **no se hace**. Con la veda: 0 de 16.000.
///
/// El vano es un `Tile::Door` y no un hueco pelado a propósito: el pack dibuja
/// la pared CON el hueco (`doorway/int_*`, la hoja de arcos), y una abertura sin
/// marco se lee como una pared rota.
fn tabique_de_planta(rx: i64, rz: i64, w: i64, d: i64, semilla: u32) -> Option<Tile> {
    // Rect interior, en coordenadas de la planta (sin contar el perímetro).
    let (mut x0, mut z0, mut x1, mut z1) = (1i64, 1i64, w - 1, d - 1);
    let mut veda_x: Vec<i64> = Vec::new();
    let mut veda_z: Vec<i64> = Vec::new();
    let mut h = semilla;
    for _ in 0..CORTES_MAXIMOS {
        let (aw, ad) = (x1 - x0, z1 - z0);
        // Dos cuartos de lado mínimo MÁS la celda del tabique.
        let corta_x = aw >= 2 * LADO_MINIMO_DE_CUARTO + 1;
        let corta_z = ad >= 2 * LADO_MINIMO_DE_CUARTO + 1;
        if !corta_x && !corta_z {
            return None;
        }
        // El lado largo manda; con empate, el que se pueda.
        let por_x = if corta_x && corta_z { aw >= ad } else { corta_x };
        h = h.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        if por_x {
            let Some(corte) =
                // El corte se lleva su propia celda, así que el cuarto de la
                // derecha va de `corte + 1` a `x1`: el tope es `x1 - LADO - 1`,
                // no `x1 - LADO`. Con el tope de más quedaban cuartos de tres
                // tiles de fondo, que es un pasillo.
                corte_libre(
                    x0 + LADO_MINIMO_DE_CUARTO,
                    x1 - LADO_MINIMO_DE_CUARTO - 1,
                    h >> 8,
                    &veda_x,
                )
            else {
                return None;
            };
            let vano = vano_libre(z0 + 1, z1 - 1, h >> 20, &veda_z);
            if rx == corte {
                return Some(if rz == vano { Tile::Door } else { Tile::WoodWall });
            }
            veda_z.push(vano);
            if rx < corte {
                x1 = corte;
            } else {
                x0 = corte + 1;
            }
        } else {
            let Some(corte) =
                corte_libre(
                    z0 + LADO_MINIMO_DE_CUARTO,
                    z1 - LADO_MINIMO_DE_CUARTO - 1,
                    h >> 8,
                    &veda_z,
                )
            else {
                return None;
            };
            let vano = vano_libre(x0 + 1, x1 - 1, h >> 20, &veda_x);
            if rz == corte {
                return Some(if rx == vano { Tile::Door } else { Tile::WoodWall });
            }
            veda_x.push(vano);
            if rz < corte {
                z1 = corte;
            } else {
                z0 = corte + 1;
            }
        }
    }
    None
}

/// **Las columnas que ocupa el PORTAL DEL FRENTE de una casa**, en coordenadas
/// del lote: `[desde, hasta)`.
///
/// Existe para que la puerta, el camino que llega a ella y el portón de la cerca
/// salgan de la MISMA cuenta. Estaban en tres sitios distintos y no coincidían:
/// la puerta se centraba en la casa, el portón en el jardín y el camino no
/// existía. Como la casa se sortea corrida dentro de su terreno
/// (`partes_de_la_parcela`), el portón caía cinco tiles al costado de la puerta y
/// entre los dos había pasto — es "los caminos de la casa están mal puestos".
pub fn portal_del_frente(house_x: i64, house_w: i64) -> (i64, i64) {
    let ancho = (house_w / 4).clamp(2, 6);
    let desde = house_x + (house_w - ancho) / 2;
    (desde, desde + ancho)
}

/// **Dónde va la pileta de una parcela**, en coordenadas del lote: `(x, z)` de su esquina, de 6×4 con el borde.
///
/// A la derecha de la casa si ahí sobran 9 tiles de jardín, si no a la izquierda, si no no hay pileta. Pegada al
/// fondo (la pileta es del patio de atrás) y a un tile de la casa. La mitad de las parcelas que tienen lugar la
/// tienen: el barrio se lee como un barrio, no como casas sueltas.
fn sitio_de_pileta(casa: Parte, jardin: Parte, h: u32) -> Option<(i64, i64)> {
    if !quiere_pileta(h) {
        return None;
    }
    let (cx, cz, cw, _) = casa;
    let (jx, _, jw, jd) = jardin;
    if jd < 11 {
        return None;
    }
    let derecha = (jx + jw - 1) - (cx + cw);
    let izquierda = cx - (jx + 1);
    if derecha >= 9 {
        Some((cx + cw + 2, cz))
    } else if izquierda >= 9 {
        Some((cx - 8, cz))
    } else {
        None
    }
}

/// **¿Esta celda es JARDÍN de una parcela residencial?** (terreno libre dentro de la cerca: ni casa, ni camino,
/// ni entrada de auto, ni pileta, ni patio). Es donde se siembra la decoración de jardín
/// (`worldgen::jardin_decor_at`): el pedido del usuario es que la parcela no sea sólo casa.
pub fn celda_de_jardin(body: CelestialBody, tx: i64, tz: i64) -> bool {
    let Some(s) = crate::geography::settlement_covering(body, tx, tz) else { return false };
    let Some(lot) = lot_at(body, &s, tx, tz) else { return false };
    if lot.program().district != District::Residential {
        return false;
    }
    let (lx, lz) = (tx - lot.min.0, tz - lot.min.1);
    let h = hash2(body_terrain_seed(body) ^ 0xfa11_0000, lot.min.0, lot.min.1);
    let (jx, jz, jw, jd) = jardin_de_la_parcela(lot, h);
    // Adentro de la cerca, no sobre ella.
    if lx <= jx || lz <= jz || lx >= jx + jw - 1 || lz >= jz + jd - 1 {
        return false;
    }
    residential_tile(lot, lx, lz, h).is_none()
}

fn residential_tile(lot: Lot, lx: i64, lz: i64, h: u32) -> Option<Tile> {
    // **El jardín, no el lote.** Lo que queda afuera es terreno natural: ahí nace
    // la variedad de tamaño de parcela y ahí está el colchón que impide que la
    // cerca termine en la calzada. Ver `jardin_de_la_parcela`.
    let (jx, jz, jw, jd) = jardin_de_la_parcela(lot, h);
    if lx < jx || lz < jz || lx >= jx + jw || lz >= jz + jd {
        return None;
    }
    // Todo lo de adentro se sigue midiendo en coordenadas del LOTE —es lo que
    // devuelve `partes_de_la_parcela` y lo que espera `roofed_rect`—; el jardín
    // sólo aporta su rectángulo.
    let (w, d) = (jw, jd);
    let (casa, cochera) = partes_de_la_parcela(lot, h);
    let has_patio = (h >> 8) % 100 < 70;

    // 1) CERCA, con portón al frente (borde sur, mirando a la calle).
    //
    // **No siempre da la vuelta entera.** Una cuadra donde las mil parcelas están
    // cercadas igual se lee como una grilla, no como un barrio: hay quien cerca
    // todo, quien cerca el frente y medio costado, y quien puso cuatro tramos y
    // se quedó sin ganas. El borde se recorre como una recta (`indice_de_borde`)
    // y la cerca es un INTERVALO de esa recta: el largo sale de una fracción
    // sorteada y el arranque, de otro bit del hash.
    // El portón de la gente va donde va el camino, y el del auto donde va la
    // entrada: una cerca con la puerta en otro sitio que el sendero es
    // exactamente lo que se veía en el barrio.
    let (casa_x, _, casa_w, _) = casa;
    let (cam0, cam1) = portal_del_frente(casa_x, casa_w);
    let (cam0, cam1) = (cam0.clamp(jx + 1, jx + jw - 2), cam1.clamp(jx + 2, jx + jw - 1));
    if let Some(idx) = indice_de_borde(jw, jd, lx - jx, lz - jz) {
        let perimetro = 2 * (w - 1) + 2 * (d - 1);
        // **LA CERCA DA LA VUELTA ENTERA** (29 sep 2026). Antes se cercaba un
        // tramo sorteado del perímetro —la mitad, un tercio, un cuarto— para que
        // el barrio no se leyera como una grilla; el usuario lo vio como lo que
        // parecía: cercas rotas, a medio hacer. Un cerco se cierra, con sus
        // portones al frente. La variedad del barrio la dan el tamaño de la
        // parcela, la casa y el jardín, no una cerca incompleta.
        let largo = perimetro;
        // El arranque se cuenta desde el frente: lo primero que cerca cualquiera
        // es la cara que da a la calle, no el fondo del terreno.
        let frente = 2 * (w - 1) + (d - 1) - w / 2;
        let inicio = frente + ((h >> 27) % 5) as i64 - 2;
        let dentro = (idx - inicio).rem_euclid(perimetro) < largo;
        // Dos aberturas en el frente: la del sendero y la del auto. La segunda
        // faltaba, así que la entrada de asfalto moría contra su propia cerca.
        let en_frente = lz == jz + jd - 1;
        let porton_de_pie = en_frente && lx >= cam0 && lx < cam1;
        let porton_de_auto =
            en_frente && cochera.is_some_and(|(c0, c1)| lx >= c0 && lx < c1);
        if porton_de_auto {
            return Some(Tile::Basalt);
        }
        if porton_de_pie {
            return Some(Tile::Pavement);
        }
        if dentro {
            return Some(Tile::Fence);
        }
        return None;
    }

    // 2) CASA: pegada al fondo (norte), centrada-ish, tamaño variable. La
    // geometría la reparte `partes_de_la_parcela`, que es también la que mira el
    // cliente para saber dónde va la fachada.
    let (house_x, house_z, house_w, house_d) = casa;
    let in_house = lx >= house_x && lx < house_x + house_w && lz >= house_z && lz < house_z + house_d;
    if in_house {
        let (rx, rz) = (lx - house_x, lz - house_z);
        // **La hilera se parte en viviendas.** Cada tramo es una casa entera —su
        // medianera, su puerta al frente y otra al fondo— y el cliente le dibuja
        // encima UN módulo del pack (ver `ANCHO_DE_HILERA`). Sin esto, tres
        // fachadas se apoyaban sobre un único caserón de veinticinco tiles.
        if es_hilera(lot.program().key) {
            let ux = rx.rem_euclid(ANCHO_DE_HILERA);
            let medianera = ux == 0 || ux == ANCHO_DE_HILERA - 1;
            let borde = rz == 0 || rz == house_d - 1;
            // Portal de dos celdas al frente y al fondo de CADA vivienda: una
            // casa sin puerta no existe, y la calzada puede comerse una cara.
            let centro = ANCHO_DE_HILERA / 2;
            // Sólo al frente: la de atrás y las de las puntas quedaban bajo la
            // fachada, que no las dibuja (ver la casa suelta, más abajo).
            if rz == house_d - 1 && (ux == centro - 1 || ux == centro) {
                return Some(Tile::Door);
            }
            if medianera || borde {
                let esquina = medianera && borde;
                // Ventanas sólo al frente/fondo y en las cabeceras de la hilera: una MEDIANERA es pared contra la vecina.
                let cabecera = rx == 0 || rx == house_w - 1;
                if !esquina && (borde || cabecera) && (rx + rz) % 3 == 1 {
                    return Some(Tile::Window);
                }
                return Some(Tile::WoodWall);
            }
            return Some(Tile::WoodFloor);
        }
        let perimeter = rx == 0 || rz == 0 || rx == house_w - 1 || rz == house_d - 1;
        if !perimeter {
            // **Adentro hay CUARTOS, no un galpón.** Ver `tabique_de_planta`.
            // La semilla es del LOTE, así que la planta se reparte igual en cada
            // consulta y entre partidas, sin guardar nada.
            let semilla = h ^ 0x7ab1_0000;
            // **La fila de adentro pegada a la fachada es un RECIBIDOR: sin tabiques.** Con la casa del tamaño
            // de su dibujo (3 oct 2026) los cuartos son chicos, y un tabique entre la puerta y el resto dejaba
            // la cama del vecino del otro lado de una pared (`hogares::de_la_calle_a_la_cama…`, 14 de 28).
            // Esa fila es por donde se entra y donde `Edificio::adentro` pone a la gente: libre, se llega a
            // todos los cuartos por ella.
            if rz != house_d - 2 {
                if let Some(t) = tabique_de_planta(rx, rz, house_w, house_d, semilla) {
                    return Some(t);
                }
            }
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
        //
        // ⚠ **29 sep 2026: SÓLO AL FRENTE.** Las puertas de los costados y del
        // fondo quedaban DEBAJO de la fachada, que no las dibuja: desde afuera se
        // entraba a la casa atravesando una pared pintada (reporte del usuario).
        // Desde que ninguna pared se planta en la calzada
        // (`ninguna_pared_en_la_calzada`), la calle ya no se come el frente, así
        // que la razón de las cuatro caras desapareció: se entra por donde la
        // fachada tiene su puerta, y por el portón.
        // A lo ancho, la MISMA cuenta que usan el camino y el portón
        // ([`portal_del_frente`]): que las tres coincidan es el arreglo.
        let (p0, p1) = portal_del_frente(0, house_w);
        let en_x = rx >= p0 && rx < p1;
        if en_x && rz == house_d - 1 {
            return Some(Tile::Door);
        }
        // **El PORTÓN**: la otra abertura del frente, la del auto. Es una puerta
        // como cualquier otra —se cruza— y la fachada del pack ya la trae
        // dibujada donde va.
        if rz == house_d - 1 {
            if let Some((c0, c1)) = cochera {
                if lx >= c0 && lx < c1 {
                    return Some(Tile::Door);
                }
            }
        }
        // Ventanas: alternadas en las cuatro caras, sin tocar las esquinas.
        let corner = (rx == 0 || rx == house_w - 1) && (rz == 0 || rz == house_d - 1);
        if !corner && (rx + rz) % 3 == 1 {
            return Some(Tile::Window);
        }
        return Some(Tile::WoodWall);
    }

    // 3) EL CAMINO: de la puerta de la casa al portón de la cerca, en línea
    //    recta. Es lo que faltaba — la parcela tenía puerta y tenía portón, y
    //    entre los dos, veinte tiles de pasto sin una losa.
    if lx >= cam0 && lx < cam1 && lz >= house_z + house_d {
        return Some(Tile::Pavement);
    }

    // 4) ENTRADA DE AUTO: del portón de la cochera hasta la calle, sin cortarse
    //    en la cerca (ahí tiene su propia abertura, arriba).
    if let Some((c0, c1)) = cochera {
        if lx >= c0 && lx < c1 && lz >= house_z + house_d {
            return Some(Tile::Basalt);
        }
    }

    // 5) PILETA con su borde de piedra, al costado de la casa (jardín de atrás). Va donde SOBRA lugar —a la
    //    derecha o a la izquierda de la casa— y nunca pisando la casa: con la casa del tamaño de su dibujo
    //    (3 oct 2026) sobra jardín de un lado u otro, y la pileta es parte de ese jardín.
    if let Some((px, pz)) = sitio_de_pileta(casa, (jx, jz, jw, jd), h) {
        if lx >= px && lx < px + 6 && lz >= pz && lz < pz + 4 {
            let (rx, rz) = (lx - px, lz - pz);
            let rim = rx == 0 || rz == 0 || rx == 5 || rz == 3;
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

/// **El rectángulo que EDIFICA un lote macizo**, en coordenadas del lote.
///
/// No es "el lote menos la vereda", que es lo que era. Un lote CRECE para llenar
/// manzanas enteras (`blocks_needed`: una universidad ocupa cuatro), así que el
/// lote colocado suele ser bastante más grande que el `cells` que declara su
/// programa — el dúplex declara 26×18 y en una manzana ancha se planta en 26×24.
/// Construir todo el lote hacía dos cosas mal a la vez:
///
/// 1. **La fachada no llegaba.** `import-edificios.py` le busca a cada programa
///    el arte que mejor calza con su `cells` declarado, y el cliente no estira un
///    dibujo más de 1,4× (`building::AGRANDE_MAX`, y con razón: pasado eso lo que
///    se ve es el pixel ampliado). Con el edificio dos veces más grande que su
///    dibujo, la fachada tapaba la mitad sur y **por el resto se veía el piso de
///    madera y los muebles de adentro desde la vereda**.
/// 2. Un dúplex de 24×22 no es un dúplex.
///
/// Ahora el edificio mide lo que declara su programa (acotado a lo que entra), se
/// apoya contra el FRENTE —que es la cara que se dibuja— y lo que sobra del lote
/// queda de patio. Es la misma regla que ya seguía la parcela residencial, donde
/// la casa nunca fue el lote entero.
fn edificado_de_lote(p: &LotProgram, lot: Lot) -> (i64, i64, i64, i64) {
    // Vereda de un tile alrededor: separa la fachada de la calzada.
    let (max_w, max_d) = ((lot.w - 2).max(1), (lot.h - 2).max(1));
    let bw = p.cells.0.clamp(1, max_w);
    let bd = p.cells.1.clamp(1, max_d);
    // Centrado a lo ancho y apoyado contra el frente (sur), que es la cara que
    // mira a la calle y la que el pack dibuja.
    let bx = 1 + (max_w - bw) / 2;
    let bz = 1 + (max_d - bd);
    (bx, bz, bw, bd)
}

/// Edificio "macizo" (comercio, centro, industria, cívico): un rectángulo con
/// paredes, ventanas y su vereda. Los grandes llevan patio interior para que no
/// sean un bloque sólido de 56×40.
/// Los edificios cívicos grandes se levantan en un **campus**: el predio es césped, no explanada de baldosa.
pub fn es_campus(clave: &str) -> bool {
    matches!(clave, "building/universidad" | "building/escuela" | "building/hospital")
}

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
    let (bx, bz, bw, bd) = edificado_de_lote(p, lot);
    // Lo que sobra del lote es explanada: vereda alrededor del edificio. **Menos en los campus** (universidad,
    // escuela, hospital): ahí lo que sobra es césped —el patio grande que los rodea— con un sendero de baldosa
    // hasta la puerta y una franja de vereda pegada al frente.
    if lx < bx || lz < bz || lx >= bx + bw || lz >= bz + bd {
        if es_campus(p.key) {
            let puerta = bx + bw / 2;
            let sendero = lz >= bz + bd && (lx == puerta || lx == puerta - 1);
            let franja = lz == bz + bd;
            return Some(if sendero || franja { Tile::Pavement } else { Tile::Grass });
        }
        return Some(Tile::Pavement);
    }
    let (rx, rz) = (lx - bx, lz - bz);
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
            return Some(if es_campus(p.key) { Tile::Grass } else { Tile::Pavement });
        }
    }

    if !perimeter {
        return Some(Tile::WoodFloor);
    }
    // Puerta al frente, de dos tiles: misma razón que en la casa — la calzada gana
    // sobre lo edificado y un portal de una sola celda se puede quedar sin ella.
    // **Edificio modular: una puerta por MÓDULO** (la banda del pack dibuja una en cada uno, y una puerta dibujada por la
    // que no se pasa es el bug). Cae en el centro del módulo, que es donde la tiene el arte; con un solo módulo coincide
    // con la de siempre. Los demás edificios llevan una al medio.
    let modular = es_modular(p.key);
    if rz == bd - 1 && bw >= 4 {
        let m = crate::edificio::MODULO;
        let puerta = if modular && bw % m == 0 { matches!(rx % m, 2 | 3) } else { rx == bw / 2 || rx == bw / 2 - 1 };
        if puerta {
            return Some(Tile::Door);
        }
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
    // **Contra el JARDÍN, no contra el lote.** La cerca dejó de plantarse en el
    // filo del lote —ahí terminaba sobre la calzada— y ahora corre por el borde
    // del jardín, que se retira una cantidad distinta en cada lado
    // (`jardin_de_la_parcela`). Preguntando por el lote, ninguna celda de cerca
    // caía en su borde y la orientación volvía `None`: los postes salían del lado
    // del jardín. Lo cazó `una_cerca_vertical_no_usa_pieza_horizontal`.
    let h = hash2(body_terrain_seed(body) ^ 0xfa11_0000, lot.min.0, lot.min.1);
    let (jx, jz, jw, jd) = jardin_de_la_parcela(lot, h);
    let (lx, lz) = (tx - lot.min.0 - jx, tz - lot.min.1 - jz);
    // El orden importa sólo en las esquinas, y en una esquina el sufijo no se usa
    // (`cercas::cadena` sólo lo pide para los tramos rectos).
    if lx == 0 {
        Some(Afuera::W)
    } else if lx == jw - 1 {
        Some(Afuera::E)
    } else if lz == 0 {
        Some(Afuera::N)
    } else if lz == jd - 1 {
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
        let h = hash2(body_terrain_seed(body) ^ 0xfa11_0000, lot.min.0, lot.min.1);
        let ((hx, hz, hw, hd), _) = partes_de_la_parcela(lot, h);
        // La clave del arte es la de la casa que de verdad se levantó (puede ser de otro programa si el terreno
        // recortó el lote): ver `casa_elegida`.
        let (jx, jz, jw, jd) = jardin_de_la_parcela(lot, h);
        let _ = (jx, jz);
        let clave = casa_elegida(lot, h, jw, jd).filter(|&(_, hu)| hu == (hw, hd)).map(|(k, _)| k).unwrap_or(p.key);
        return Some((lot.min.0 + hx, lot.min.1 + hz, hw, hd, clave));
    }
    // **La clave del PROGRAMA viaja con el rect.** Antes se tiraba y quien
    // dibujaba el techo adivinaba el arte por el tamaño (5×4 → casa chica), lo
    // que además de frágil apuntaba a tres slots sin pixel. El lote ya sabe qué
    // edificio es; lo único que faltaba era no perder el dato.
    let (bx, bz, bw, bd) = edificado_de_lote(p, lot);
    Some((lot.min.0 + bx, lot.min.1 + bz, bw, bd, p.key))
}

#[cfg(test)]
mod tests_tabiques {
    use super::*;
    use std::collections::{HashSet, VecDeque};

    /// La planta partida en cuartos, como la ve el jugador: `true` = se pisa.
    fn planta(w: i64, d: i64, semilla: u32) -> Vec<(i64, i64)> {
        let mut libres = Vec::new();
        for rz in 1..d - 1 {
            for rx in 1..w - 1 {
                if tabique_de_planta(rx, rz, w, d, semilla) != Some(Tile::WoodWall) {
                    libres.push((rx, rz));
                }
            }
        }
        libres
    }

    /// **Ningún cuarto queda sellado.**
    ///
    /// Es LA invariante de `tabique_de_planta`, y no es teórica: la primera
    /// versión —sin la veda de los vanos— dejaba habitaciones inalcanzables en
    /// 2.613 de estas 16.000 plantas, porque un vano abría contra el tabique que
    /// plantaba el corte siguiente. Un cuarto al que no se entra es peor que no
    /// tener cuartos: el mobiliario se siembra ahí adentro y no lo ve nadie.
    #[test]
    fn ningun_cuarto_queda_sellado() {
        let mut partidas = 0;
        for w in 6..26 {
            for d in 5..25 {
                for sem in 0..40u32 {
                    let semilla = sem.wrapping_mul(2_654_435_761);
                    let libres: HashSet<(i64, i64)> = planta(w, d, semilla).into_iter().collect();
                    if libres.is_empty() {
                        continue;
                    }
                    if libres.len() < ((w - 2) * (d - 2)) as usize {
                        partidas += 1;
                    }
                    let inicio = *libres.iter().next().unwrap();
                    let mut vistas = HashSet::from([inicio]);
                    let mut cola = VecDeque::from([inicio]);
                    while let Some((x, z)) = cola.pop_front() {
                        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                            let n = (x + dx, z + dz);
                            if libres.contains(&n) && vistas.insert(n) {
                                cola.push_back(n);
                            }
                        }
                    }
                    assert_eq!(
                        vistas.len(),
                        libres.len(),
                        "planta {w}x{d} semilla {sem}: {} celdas sin salida",
                        libres.len() - vistas.len()
                    );
                }
            }
        }
        // Y que de verdad esté partiendo: una casa de un solo ambiente no
        // ejercita nada, así que si esto baja es que dejó de haber cuartos.
        assert!(partidas > 12_000, "sólo {partidas} plantas con tabiques");
    }

    /// **Todo cuarto da para amueblarse.**
    ///
    /// Se miden los cuartos DE VERDAD: se tratan los vanos como pared, así cada
    /// componente conexa es una habitación y no la planta entera. Cada lado
    /// tiene que llegar a `LADO_MINIMO_DE_CUARTO` — o ser el interior completo
    /// en ese eje, que es el caso de la casa que no se partió por ahí. Un cuarto
    /// de dos tiles de ancho es un pasillo, y ahí no entra un mueble del pack.
    #[test]
    fn todo_cuarto_da_para_amueblarse() {
        for w in 6..26 {
            for d in 5..25 {
                for sem in 0..20u32 {
                    let semilla = sem.wrapping_mul(2_654_435_761);
                    let piso = |rx, rz| tabique_de_planta(rx, rz, w, d, semilla).is_none();
                    let mut vistas: HashSet<(i64, i64)> = HashSet::new();
                    for rz0 in 1..d - 1 {
                        for rx0 in 1..w - 1 {
                            if !piso(rx0, rz0) || vistas.contains(&(rx0, rz0)) {
                                continue;
                            }
                            // Relleno de UN cuarto (los vanos no se cruzan).
                            let mut cola = VecDeque::from([(rx0, rz0)]);
                            let mut cuarto = vec![(rx0, rz0)];
                            vistas.insert((rx0, rz0));
                            while let Some((x, z)) = cola.pop_front() {
                                for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                                    let (nx, nz) = (x + dx, z + dz);
                                    if nx < 1 || nz < 1 || nx >= w - 1 || nz >= d - 1 {
                                        continue;
                                    }
                                    if piso(nx, nz) && vistas.insert((nx, nz)) {
                                        cola.push_back((nx, nz));
                                        cuarto.push((nx, nz));
                                    }
                                }
                            }
                            let ancho = cuarto.iter().map(|c| c.0).max().unwrap()
                                - cuarto.iter().map(|c| c.0).min().unwrap()
                                + 1;
                            let fondo = cuarto.iter().map(|c| c.1).max().unwrap()
                                - cuarto.iter().map(|c| c.1).min().unwrap()
                                + 1;
                            assert!(
                                ancho >= LADO_MINIMO_DE_CUARTO || ancho == w - 2,
                                "planta {w}x{d} sem {sem}: cuarto de {ancho} de ancho"
                            );
                            assert!(
                                fondo >= LADO_MINIMO_DE_CUARTO || fondo == d - 2,
                                "planta {w}x{d} sem {sem}: cuarto de {fondo} de fondo"
                            );
                        }
                    }
                }
            }
        }
    }
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
        // (3 oct 2026: ×2 y no ×4. La casa chica pasó a 26×17 para dar jardín y pileta junto a una casa que
        // mide lo que su dibujo; la universidad sigue siendo más del doble.)
        let uni = LOT_PROGRAMS.iter().find(|p| p.key == "building/universidad").unwrap();
        let casa = LOT_PROGRAMS.iter().find(|p| p.key == "building/casa_chica").unwrap();
        let (a_uni, a_casa) = (uni.cells.0 * uni.cells.1, casa.cells.0 * casa.cells.1);
        assert!(
            a_uni > a_casa * 2,
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
        // barrido arranca lejos y es ancho. Cuán lejos lo dice la densidad de la
        // ciudad (`anillos_de_zona`): en una metrópolis el comercio llega a la
        // mitad del casco.
        let lejos = block_size(s.style, s.tier) * (anillos_de_zona(body, &s).2 + 2);
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

    /// **La casa mide lo que mide su dibujo, y la parcela NO es sólo casa** (pedido del usuario, 3 oct 2026).
    ///
    /// Para cada vivienda generada: su huella tiene que ser EXACTAMENTE la de uno de los dibujos del pack de su
    /// programa (`huella_de_dibujo`), y tiene que sobrar jardín: la casa nunca ocupa más del 60 % del lote
    /// (en el barrido real el máximo es 51 %: un lote que el terreno recortó).
    #[test]
    fn la_casa_mide_su_dibujo_y_la_parcela_tiene_jardin() {
        let body = CelestialBody::Earth;
        let s = a_city();
        let mut vistos = std::collections::HashSet::new();
        let (mut casas, mut a_medida) = (0, 0);
        let mut peor_ocupacion = 0.0f32;
        let mut sin_jardin: Vec<(i64, i64)> = Vec::new();
        let r = s.radius.min(1_500);
        for dz in (-r..r).step_by(5) {
            for dx in (-r..r).step_by(5) {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                let Some(lot) = lot_at(body, &s, tx, tz) else { continue };
                let p = lot.program();
                if p.district != District::Residential || es_hilera(p.key) || !vistos.insert(lot.min) {
                    continue;
                }
                let Some((_, _, hw, hd, clave)) = roofed_rect(body, &s, tx, tz) else { continue };
                casas += 1;
                if huellas_de_programa(clave).contains(&(hw, hd)) {
                    a_medida += 1;
                }
                let ocupacion = (hw * hd) as f32 / (lot.w * lot.h) as f32;
                peor_ocupacion = peor_ocupacion.max(ocupacion);
                if ocupacion > 0.6 {
                    sin_jardin.push(lot.min);
                }
            }
        }
        assert!(casas > 20, "el barrido no encontró casas: {casas}");
        println!("{a_medida} de {casas} casas miden exactamente su dibujo; ocupación máxima del lote {:.0} %", peor_ocupacion * 100.0);
        assert!(
            a_medida * 100 >= casas * 95,
            "sólo {a_medida} de {casas} casas miden su dibujo: el jardín es chico para el pack"
        );
        assert!(sin_jardin.is_empty(), "{} casas ocupan más del 60 % de su lote: {:?}", sin_jardin.len(), &sin_jardin[..sin_jardin.len().min(5)]);
    }

    /// **La parcela no es sólo casa**: tiene jardín decorado y, en una buena parte, pileta — y la pileta nunca
    /// pisa la casa (pedido del usuario, 3 oct 2026).
    #[test]
    fn la_parcela_tiene_jardin_decorado_y_pileta_sin_pisar_la_casa() {
        let body = CelestialBody::Earth;
        let s = a_city();
        let mut vistos = std::collections::HashSet::new();
        let (mut casas, mut con_pileta, mut jardin_total, mut decoradas) = (0usize, 0usize, 0usize, 0usize);
        let r = s.radius.min(1_200);
        for dz in (-r..r).step_by(5) {
            for dx in (-r..r).step_by(5) {
                let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                let Some(lot) = lot_at(body, &s, tx, tz) else { continue };
                if lot.program().district != District::Residential || es_hilera(lot.program().key) || !vistos.insert(lot.min) {
                    continue;
                }
                casas += 1;
                let h = hash2(body_terrain_seed(body) ^ 0xfa11_0000, lot.min.0, lot.min.1);
                let (casa, _) = partes_de_la_parcela(lot, h);
                let jardin = jardin_de_la_parcela(lot, h);
                if let Some((px, pz)) = sitio_de_pileta(casa, jardin, h) {
                    con_pileta += 1;
                    let (cx, cz, cw, cd) = casa;
                    let pisa = px < cx + cw && px + 6 > cx && pz < cz + cd && pz + 4 > cz;
                    assert!(!pisa, "la pileta {:?} pisa la casa {:?} en el lote {:?}", (px, pz), casa, lot.min);
                    assert!(px > jardin.0 && px + 6 < jardin.0 + jardin.2, "la pileta se sale de la cerca");
                }
                for lz in 0..lot.h {
                    for lx in 0..lot.w {
                        if celda_de_jardin(body, lot.min.0 + lx, lot.min.1 + lz) {
                            jardin_total += 1;
                            if crate::worldgen::jardin_decor_at(body, lot.min.0 + lx, lot.min.1 + lz).is_some() {
                                decoradas += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(casas > 20, "el barrido no encontró casas: {casas}");
        println!("{con_pileta} de {casas} parcelas con pileta; {decoradas} de {jardin_total} celdas de jardín decoradas");
        assert!(con_pileta * 100 >= casas * 20, "muy pocas parcelas con pileta: {con_pileta} de {casas}");
        assert!(jardin_total > casas * 20, "casi no queda jardín: {jardin_total} celdas para {casas} casas");
        assert!(decoradas * 100 >= jardin_total * 3, "el jardín no se decora: {decoradas} de {jardin_total}");
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

    /// **NINGUNA PARED SE PLANTA EN LA CALZADA.**
    ///
    /// La calzada gana sobre lo edificado (`topdown::topdown_tile`), así que una
    /// casa a caballo de una calle no se ve como una casa partida: se ve como
    /// una casa a la que le falta un trozo, con el asfalto pasándole por dentro
    /// y el sprite de la fachada dibujado igual encima. Es lo que el usuario
    /// reportó como "casas sobre carreteras".
    ///
    /// Se mide sobre lo EDIFICADO (pared, ventana, puerta, piso), no sobre el
    /// jardín: que a un terreno le pase una calle por la punta del césped no
    /// molesta a nadie, y el retiro de la parcela ya deja esa franja verde a
    /// propósito.
    ///
    /// Medido el 25 ago 2026: 7.611 celdas de casa con calzada encima de 53.333
    /// edificadas en Metrópolis Norte (el 14 %). Un programa más ancho que el
    /// hueco de una manzana reservaba dos y se plantaba a caballo de la calle
    /// interior, que nunca desaparecía. Desde el 28 sep 2026 la calle interior de
    /// una supermanzana no se pinta (`supermanzana` + `geography::urban_civil`) y
    /// esto da cero.
    #[test]
    fn ninguna_pared_de_ciudad_se_planta_en_la_calzada() {
        let body = CelestialBody::Earth;
        for t in crate::world_map::template(body).towns {
            let s = Settlement {
                tier: SettlementTier::City,
                center: crate::world_map::town_center(body, t),
                style: t.style,
                name: Some(t.name),
                radius: t.radius,
            };
            let (mut sobre_asfalto, mut edificado) = (0u32, 0u32);
            let mut ejemplo = None;
            let r = 420;
            for dz in -r..r {
                for dx in -r..r {
                    let (tx, tz) = (s.center.0 + dx, s.center.1 + dz);
                    let Some(tile) = lot_tile(body, &s, tx, tz) else { continue };
                    if !matches!(
                        tile,
                        Tile::WoodWall
                            | Tile::StoneBrick
                            | Tile::Window
                            | Tile::Door
                            | Tile::WoodFloor
                    ) {
                        continue;
                    }
                    edificado += 1;
                    if crate::geography::civil_at(body, tx, tz)
                        == Some(crate::geography::Civil::Road)
                    {
                        sobre_asfalto += 1;
                        ejemplo.get_or_insert((tx, tz, tile));
                    }
                }
            }
            let nombre = s.name.unwrap_or("(sin nombre)");
            assert!(edificado > 5_000, "{nombre}: apenas {edificado} celdas edificadas para medir");
            assert_eq!(
                sobre_asfalto, 0,
                "{nombre}: {sobre_asfalto} celdas de casa con calzada encima \
                 (de {edificado} edificadas), la primera en {ejemplo:?}"
            );
        }
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

    /// **Lo que se compone tiene que existir y tiene que dar exacto.**
    ///
    /// `LOTES_MODULARES` nombra parcelas por clave, y una clave escrita a mano se
    /// desincroniza sin hacer ruido: si el lote no existe, el kit no se dibuja
    /// nunca y nadie se entera (el edificio cae al sprite de respaldo). Se
    /// comprueban las dos mitades:
    ///
    /// 1. **Que el lote exista** en `LOT_PROGRAMS`.
    /// 2. **Que su huella sea un múltiplo exacto de un módulo.** El composer
    ///    redondea para abajo, así que una huella de 15 tiles con módulos de 9
    ///    dibuja UN módulo de 9 y deja 6 tiles de solar pelado — que es
    ///    exactamente el "queda una mota en un solar vacío" que ya cazamos del
    ///    otro lado. Las parcelas de esta tabla se declaran para dar justo.
    #[test]
    fn las_parcelas_modulares_dan_modulos_enteros() {
        let mut mal = Vec::new();
        for &(clave, min, max) in LOTES_MODULARES {
            let Some(p) = LOT_PROGRAMS.iter().find(|p| p.key == clave) else {
                mal.push(format!("{clave}: no es un lote de LOT_PROGRAMS"));
                continue;
            };
            assert!(min <= max, "{clave}: pisos ({min}, {max}) al reves");
            let (w, _) = p.huella();
            if w % crate::edificio::MODULO != 0 {
                mal.push(format!(
                    "{clave}: huella de {w} tiles de frente, que no es multiplo de {}: \
                     se dibujarian {} modulos y quedarian {} tiles de solar pelado",
                    crate::edificio::MODULO,
                    w / crate::edificio::MODULO,
                    w % crate::edificio::MODULO,
                ));
            }
        }
        assert!(mal.is_empty(), "{}", mal.join("\n  "));
    }

    /// Dos parcelas distintas del centro no salen siempre de la misma altura, y
    /// la misma parcela sale siempre igual. Lo primero es la variedad que se
    /// buscaba; lo segundo es que el streaming no la haga titilar.
    #[test]
    fn la_altura_de_una_torre_es_estable_y_variada() {
        let alturas: std::collections::BTreeSet<u8> = (0..40)
            .flat_map(|x| (0..40).map(move |z| (x * 13, z * 17)))
            .filter_map(|(x, z)| pisos_modulares("building/torre", x, z))
            .collect();
        assert!(
            alturas.len() > 3,
            "todas las torres salen de la misma altura: {alturas:?}"
        );
        assert_eq!(
            pisos_modulares("building/torre", 1234, -567),
            pisos_modulares("building/torre", 1234, -567),
        );
        assert_eq!(pisos_modulares("building/casa_chica", 0, 0), None);
    }
}

#[cfg(test)]
mod tests_fachadas_solapadas {
    use super::*;
    use std::collections::HashSet;

    /// **Dos casas no se montan una sobre otra.** Cada lote dibuja su fachada sobre el
    /// rectángulo `roofed_rect`; si dos de esos rectángulos se pisan, una fachada tapa a la otra.
    #[test]
    fn ninguna_fachada_pisa_a_otra() {
        let body = CelestialBody::Earth;
        let mut pueblos = 0;
        let mut malos = Vec::new();
        for sz in -12..12 {
            for sx in -12..12 {
                let Some(s) = crate::geography::settlement_in(body, sx, sz) else { continue };
                pueblos += 1;
                let radio = s.radius.clamp(24, 120);
                let mut rects: HashSet<(i64, i64, i64, i64)> = HashSet::new();
                for tz in (s.center.1 - radio..=s.center.1 + radio).step_by(2) {
                    for tx in (s.center.0 - radio..=s.center.0 + radio).step_by(2) {
                        if let Some((x, z, w, d, _)) = roofed_rect(body, &s, tx, tz) {
                            rects.insert((x, z, w, d));
                        }
                    }
                }
                let v: Vec<_> = rects.into_iter().collect();
                for i in 0..v.len() {
                    for j in i + 1..v.len() {
                        let (a, b) = (v[i], v[j]);
                        if a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3 {
                            malos.push((s.center, a, b));
                        }
                    }
                }
            }
        }
        assert!(pueblos > 0);
        assert!(malos.is_empty(), "{} pares de fachadas se pisan; primeros: {:?}", malos.len(), &malos[..malos.len().min(5)]);
    }
}
