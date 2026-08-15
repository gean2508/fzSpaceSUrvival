//! Construcción por **cuadrícula + bordes** (estilo *Last Day on Earth*).
//!
//! Dos capas distintas, no una:
//! - **Pisos**: ocupan la CELDA (`Tile::WoodFloor` en el grid de `topdown.rs`).
//!   Se colocan libres, en cualquier parte del mapa; la grilla es global (1 tile
//!   = 1 m), así que no hay "zona de base" ni offsets relativos que mantener.
//! - **Paredes / puertas / ventanas**: ocupan el **BORDE** entre dos celdas, no
//!   la celda. Un borde se identifica por `(tx, tz, axis)`:
//!   - `WallAxis::H` = línea `z = tz`, tramo `x ∈ [tx, tx+1]` (separa la celda de
//!     arriba `(tx, tz-1)` de la de abajo `(tx, tz)`).
//!   - `WallAxis::V` = línea `x = tx`, tramo `z ∈ [tz, tz+1]` (separa `(tx-1, tz)`
//!     de `(tx, tz)`).
//!
//! Por qué bordes y no celdas: es lo que hace que una habitación de 3×3 tenga
//! 9 tiles de piso caminables (y no 1 útil rodeado de 8 de muro), que la pared
//! sea fina al caminar y que puerta/ventana caigan justo en la línea del muro.
//!
//! Este módulo es SÓLO reglas puras (sin Bevy, sin IO): catálogo de piezas,
//! snapping del borde más cercano, validación de apoyo y colisión cápsula-vs-
//! pared. El estado vivo (lo construido, su persistencia y sus sprites) es
//! `ha-game::walls`.

use serde::{Deserialize, Serialize};

use crate::topdown::{Prefab, Tile, PLAYER_RADIUS, TILE_M};

/// Grosor de COLISIÓN de una pared, en metros. Fino a propósito: la pared se
/// dibuja alta (`WALL_RENDER_H`) pero sólo roba esto de espacio caminable.
pub const WALL_THICK: f32 = 0.26;

/// Alto VISUAL de una pared en tiles. El sprite se ancla al pie del borde y
/// crece hacia arriba en pantalla (perspectiva 3/4).
///
/// **La referencia es el jugador, no el tile.** El personaje se dibuja de
/// `0.95 × 1.9` (ver `scene.rs`), o sea casi dos tiles de alto. Con el 1.5 de
/// antes una pared —y sobre todo una PUERTA— quedaba más BAJA que quien la
/// cruza: por eso la puerta se veía diminuta en el mundo. Una puerta tiene que
/// leerse como algo por lo que pasás por debajo.
///
/// 2.4 tiles = ~1.26 veces la altura del jugador. Es la proporción de un umbral
/// real y sigue dejando ver lo que hay detrás del muro en cenital (a partir de
/// ~3 el muro tapa media pantalla y el interior deja de leerse).
///
/// No toca la colisión: el grosor caminable sigue siendo `WALL_THICK`.
pub const WALL_RENDER_H: f32 = 2.4;

/// Alto VISUAL de una BARANDA en tiles.
///
/// **Sale de la ESCALA DEL PACK, no de un número lindo**: el arte de LimeZu está
/// dibujado a 32 px por celda y una celda son 1,5 tiles del juego, así que la
/// baranda —que ocupa una celda— mide 1,5. (La pared no es la excepción: su
/// recorte son 52 px = 1,6 celdas = los 2,4 de [`WALL_RENDER_H`].) Queda a la
/// altura del pecho: se lee "acá se termina el piso" y no tapa lo de atrás.
pub const RAILING_RENDER_H: f32 = 1.5;

/// Alto VISUAL de una FACHADA en tiles, **sólo como respaldo**: el alto real sale
/// del recorte de la pose (`walls::medida_de_pieza`), y el de exteriores mide 4
/// celdas del pack.
///
/// Es más del doble que el jugador a propósito. Una fachada de 2,4 —lo que mide
/// una pared de interior— no se lee como edificio sino como tapia: en Modern
/// Exteriors la planta baja de un local YA ocupa cuatro celdas, con su vidriera,
/// su dintel y su cornisa. Y no tapa el interior a pesar de la altura porque la
/// cara exterior se desvanece cuando estás adentro (`layers::Overhead::exterior`)
/// y la colisión sigue siendo el rectángulo fino de [`WALL_THICK`].
pub const FACHADA_RENDER_H: f32 = 4.0;

/// Orientación de un borde de la cuadrícula.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WallAxis {
    /// Horizontal: corre este-oeste sobre la línea `z = tz`.
    H,
    /// Vertical: corre norte-sur sobre la línea `x = tx`.
    V,
}

impl WallAxis {
    pub fn as_u8(self) -> u8 {
        match self {
            WallAxis::H => 0,
            WallAxis::V => 1,
        }
    }
    pub fn from_u8(v: u8) -> Self {
        if v == 1 {
            WallAxis::V
        } else {
            WallAxis::H
        }
    }
}

/// Qué es la pieza que ocupa el borde.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WallKind {
    Wall,
    Door,
    /// **Vano**: la pared con el hueco de la puerta y sin hoja — el marco.
    ///
    /// Es la pieza que faltaba para poder cerrar una casa: hasta ahora, o ponías
    /// pared (y quedaba sellada) o ponías puerta. El vano es el paso franco: cierra
    /// el recinto igual que una puerta —el cuarto se reconoce, la luz entra por
    /// ahí— pero no tiene hoja que se abra. Sirve además de umbral entre dos
    /// ambientes de la misma casa, que con una puerta cada vez es un fastidio.
    Doorway,
    Window,
}

impl WallKind {
    /// ¿Corta el paso? La puerta NO (se cruza; su sprite se abre por
    /// proximidad) y el vano tampoco (es un hueco); pared y ventana sí.
    pub fn blocks(self) -> bool {
        !matches!(self, WallKind::Door | WallKind::Doorway)
    }

    /// **¿Se entra por acá?** Puerta y vano sí; ventana y pared no. Es lo que
    /// decide si un recinto cerrado llega a ser un cuarto (`ha-game::cuartos`):
    /// una caja sin ninguna de las dos es un cajón, no una habitación.
    pub fn is_entrance(self) -> bool {
        matches!(self, WallKind::Door | WallKind::Doorway)
    }
}

/// Una pieza de borde del catálogo (paleta de construcción).
#[derive(Clone, Copy, Debug)]
pub struct EdgePiece {
    pub id: &'static str,
    pub label: &'static str,
    /// Slot de arte (`wall/wood`): sus poses `front`/`back`/`left`/`right` las
    /// recorta el usuario en el editor de assets.
    pub art: &'static str,
    pub kind: WallKind,
    pub cost_item: &'static str,
    pub cost_qty: u32,
    /// Alto VISUAL en tiles. Casi todas las piezas son [`WALL_RENDER_H`] —una
    /// pared por la que pasás por debajo—, pero una BARANDA es a la cintura: si
    /// se dibujara alta taparía el piso que rodea y dejaría de leerse como el
    /// borde de un balcón. No toca la colisión, que sigue siendo el rectángulo
    /// fino de [`WALL_THICK`].
    pub render_h: f32,
}

/// Catálogo ordenado de piezas de borde (el índice es el slot de paleta).
///
/// **Cada material trae el JUEGO COMPLETO**: pared, vano, puerta y ventana en
/// madera, y los mismos cuatro en piedra. Antes había dos paredes pero una sola
/// puerta y una sola ventana, las dos de madera: construir de piedra significaba
/// terminar la casa con carpintería prestada, y el vano —la pared con el hueco de
/// la puerta— no existía en ninguna de las dos. Que el juego sea el mismo para
/// cada textura es lo que hace que elegir material sea elegir un aspecto y no
/// renunciar a piezas. (`el_juego_de_piezas_es_el_mismo_en_cada_material` lo
/// cuida.)
pub static EDGE_PIECES: &[EdgePiece] = &[
    EdgePiece {
        id: "edge_wall_wood",
        label: "Pared madera",
        art: "wall/wood",
        kind: WallKind::Wall,
        cost_item: "wood",
        cost_qty: 2,
        render_h: WALL_RENDER_H,
    },
    EdgePiece {
        id: "edge_doorway_wood",
        label: "Vano madera",
        art: "doorway/wood",
        kind: WallKind::Doorway,
        cost_item: "wood",
        cost_qty: 2,
        render_h: WALL_RENDER_H,
    },
    EdgePiece {
        id: "edge_door_wood",
        label: "Puerta madera",
        art: "door/wood",
        kind: WallKind::Door,
        cost_item: "wood",
        cost_qty: 3,
        render_h: WALL_RENDER_H,
    },
    EdgePiece {
        id: "edge_window",
        label: "Ventana madera",
        art: "window/basic",
        kind: WallKind::Window,
        cost_item: "wood",
        cost_qty: 2,
        render_h: WALL_RENDER_H,
    },
    EdgePiece {
        id: "edge_wall_stone",
        label: "Pared piedra",
        art: "wall/stone",
        kind: WallKind::Wall,
        cost_item: "stone",
        cost_qty: 2,
        render_h: WALL_RENDER_H,
    },
    EdgePiece {
        id: "edge_doorway_stone",
        label: "Vano piedra",
        art: "doorway/stone",
        kind: WallKind::Doorway,
        cost_item: "stone",
        cost_qty: 2,
        render_h: WALL_RENDER_H,
    },
    EdgePiece {
        id: "edge_door_stone",
        label: "Puerta piedra",
        art: "door/stone",
        kind: WallKind::Door,
        cost_item: "stone",
        cost_qty: 3,
        render_h: WALL_RENDER_H,
    },
    EdgePiece {
        id: "edge_window_stone",
        label: "Ventana piedra",
        art: "window/stone",
        kind: WallKind::Window,
        cost_item: "stone",
        cost_qty: 2,
        render_h: WALL_RENDER_H,
    },
    // **Baranda** — el borde de un balcón, de un rellano o de un hueco de
    // escalera. Existe porque los pisos altos ya se construyen losa a losa y el
    // borde quedaba a pico: nada te avisaba dónde se termina el suelo, y en
    // cenital eso es una caída sin aviso. Frena como una ventana (se ve a
    // través) y se dibuja a la cintura, no a la altura de una pared.
    // Arte: el sistema de pisos altos de Modern Interiors
    // (`17_Visibile_Upstairs_System`), que trae las barandas dibujadas.
    EdgePiece {
        id: "edge_railing_wood",
        label: "Baranda",
        art: "railing/madera",
        kind: WallKind::Window,
        cost_item: "wood",
        cost_qty: 1,
        render_h: RAILING_RENDER_H,
    },
    // ── FACHADAS ────────────────────────────────────────────────────────────
    //
    // **La pared que da a la calle** (8 ago 2026). Los dos packs dibujan un
    // edificio en proyecciones distintas: Modern Interiors lo corta en cenital
    // (paredes finas vistas desde adentro) y Modern Exteriors lo dibuja de
    // ELEVACIÓN (el frente, con su paño, su vidriera y su cornisa). Hasta hoy la
    // construcción por bordes estaba pintada entera con las primeras, así que una
    // casa del jugador mirada desde la vereda era un interior dado vuelta.
    //
    // No hace falta un sistema nuevo: `walls::Cara` ya elige una POSE por cara, así
    // que un material puede tener su `back` (la cara de afuera) cortado de
    // exteriores y su `front` (el tabique) de interiores. Lo trae
    // `scripts/import-fachadas.py`.
    //
    // ⚠ **Van al FINAL del catálogo, y tiene que quedarse así.**
    // `build::edge_piece_of_kind` devuelve el PRIMER `(kind, cost_item)` que calce,
    // y de él salen los anillos de los prefabs: con la fachada de piedra antes que
    // `edge_wall_stone`, `room_small` y `casa_1` se estamparían de fachada — una
    // habitación interior con el frente de un local para afuera. Lo clava
    // `los_prefabs_no_estampan_fachadas`.
    //
    // El costo es `stone` a propósito: una fachada de ladrillo es una TERMINACIÓN,
    // no un recurso nuevo. Un `cost_item` propio obligaría además a un tumbado de
    // ese material (`el_juego_de_piezas_es_el_mismo_en_cada_material`), y el techo
    // de una casa con frente de ladrillo es el mismo techo.
    EdgePiece {
        id: "edge_wall_fachada_local",
        label: "Pared de local",
        art: "wall/fachada_local",
        kind: WallKind::Wall,
        cost_item: "stone",
        cost_qty: 2,
        render_h: FACHADA_RENDER_H,
    },
    EdgePiece {
        id: "edge_doorway_fachada_local",
        label: "Vano de local",
        art: "doorway/fachada_local",
        kind: WallKind::Doorway,
        cost_item: "stone",
        cost_qty: 2,
        render_h: FACHADA_RENDER_H,
    },
    EdgePiece {
        id: "edge_door_fachada_local",
        label: "Puerta de local",
        art: "door/fachada_local",
        kind: WallKind::Door,
        cost_item: "stone",
        cost_qty: 3,
        render_h: FACHADA_RENDER_H,
    },
    EdgePiece {
        id: "edge_window_fachada_local",
        label: "Vidriera de local",
        art: "window/fachada_local",
        kind: WallKind::Window,
        cost_item: "stone",
        cost_qty: 2,
        render_h: FACHADA_RENDER_H,
    },
    EdgePiece {
        id: "edge_wall_fachada_portal",
        label: "Pared de portal",
        art: "wall/fachada_portal",
        kind: WallKind::Wall,
        cost_item: "stone",
        cost_qty: 2,
        render_h: FACHADA_RENDER_H,
    },
    EdgePiece {
        id: "edge_doorway_fachada_portal",
        label: "Vano de portal",
        art: "doorway/fachada_portal",
        kind: WallKind::Doorway,
        cost_item: "stone",
        cost_qty: 2,
        render_h: FACHADA_RENDER_H,
    },
    EdgePiece {
        id: "edge_door_fachada_portal",
        label: "Puerta de portal",
        art: "door/fachada_portal",
        kind: WallKind::Door,
        cost_item: "stone",
        cost_qty: 3,
        render_h: FACHADA_RENDER_H,
    },
    EdgePiece {
        id: "edge_window_fachada_portal",
        label: "Ventana de portal",
        art: "window/fachada_portal",
        kind: WallKind::Window,
        cost_item: "stone",
        cost_qty: 2,
        render_h: FACHADA_RENDER_H,
    },
];

/// **El prefijo que hace de una pieza de borde una FACHADA.** Se pregunta por el
/// slot de arte y no por un `bool` en la tabla porque el dato ya está ahí: los
/// cuatro slots de un material de fachada lo llevan en el nombre, y un `bool`
/// paralelo es una segunda fuente de verdad que se puede desincronizar.
pub const FACHADA: &str = "fachada_";

/// ¿Esta pieza es de un material de fachada?
pub fn es_fachada(p: &EdgePiece) -> bool {
    p.art.contains(FACHADA)
}

// ---------------------------------------------------------------------------
// Techos — la TERCERA capa: piso = celda, pared = borde, techo = celda de arriba
// ---------------------------------------------------------------------------

/// Qué clase de tapa es. Sólo cambia cómo se DIBUJA: el tumbado es plano y se
/// mira desde abajo (interior), el techo es de aguas y se mira desde arriba
/// (exterior). Ninguno de los dos toca la colisión — un techo no es un piso, y
/// caminar por encima exige construir la losa del piso de arriba.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CeilingKind {
    /// **Tumbado**: la tapa lisa que ves desde adentro del cuarto.
    Ceiling,
    /// **Techo modular**: la cubierta de afuera. Autotile por vecinos, así que
    /// una sola clave de arte da faldones, cumbrera y esquinas.
    Roof,
}

/// Una pieza de techo del catálogo.
#[derive(Clone, Copy, Debug)]
pub struct CeilingPiece {
    pub id: &'static str,
    pub label: &'static str,
    /// Slot de arte. Los `Roof` usan poses de autotile
    /// (`center`/`edge_*`/`corner_*`/`inner_*`), igual que el agua y los pisos;
    /// los `Ceiling` con una sola pose (`center`) ya se ven bien.
    pub art: &'static str,
    pub kind: CeilingKind,
    pub cost_item: &'static str,
    pub cost_qty: u32,
}

/// Catálogo ordenado de tapas (el índice es el slot de paleta).
///
/// Es deliberadamente corto: el techo modular sale de UNA clave con autotile, no
/// de nueve piezas (faldón norte, esquina, cumbrera…) que el jugador tendría que
/// ir encajando a mano en un celular. Vos pintás la superficie y el juego elige
/// la forma de cada celda por sus vecinas.
pub static CEILING_PIECES: &[CeilingPiece] = &[
    CeilingPiece {
        id: "ceil_wood",
        label: "Tumbado madera",
        art: "ceiling/madera",
        kind: CeilingKind::Ceiling,
        cost_item: "wood",
        cost_qty: 1,
    },
    // La casa de piedra también se tapa con piedra: es la misma regla de "cada
    // material trae el juego completo" que las piezas de borde.
    CeilingPiece {
        id: "ceil_stone",
        label: "Tumbado piedra",
        art: "ceiling/piedra",
        kind: CeilingKind::Ceiling,
        cost_item: "stone",
        cost_qty: 1,
    },
    // **Y los techos de AGUAS vuelven** (8 ago 2026). Salieron de esta paleta el 4
    // ago con un motivo escrito: "lo que uno construye a mano son CAJAS, y el
    // tejado de aguas es lo que distingue a las casas ya hechas del mundo".
    //
    // Ese motivo caducó con las fachadas. Mientras la cara de afuera de tu casa era
    // un paño de interior, taparla con un tumbado liso no desentonaba: todo el
    // conjunto se veía igual de provisorio. Ahora el frente es una elevación de
    // Modern Exteriors, y una casa con frente de edificio y tapa lisa **no lee como
    // casa desde el norte** — se ve el frente de un local y, arriba, una losa de
    // madera. El tejado es la otra mitad de la cáscara.
    //
    // Van DESPUÉS de los tumbados: `ceiling_piece_for` devuelve el primer match por
    // `cost_item`, así que el prefab de habitación sigue entregando la casa con
    // tumbado (que es lo que corresponde adentro) y el tejado se pone a mano.
    CeilingPiece {
        id: "roof_teja",
        label: "Techo de teja",
        art: "roof/teja",
        kind: CeilingKind::Roof,
        cost_item: "stone",
        cost_qty: 1,
    },
    CeilingPiece {
        id: "roof_chapa",
        label: "Techo de chapa",
        art: "roof/chapa",
        kind: CeilingKind::Roof,
        cost_item: "wood",
        cost_qty: 1,
    },
];

/// **El tumbado que le corresponde a un material** (`"wood"` / `"stone"`). Lo usa
/// el prefab de habitación para tapar lo que estampa: una casa se entrega
/// terminada —piso, paredes, puerta Y techo—, no a la intemperie.
pub fn ceiling_piece_for(cost_item: &str) -> usize {
    CEILING_PIECES
        .iter()
        .position(|c| c.cost_item == cost_item)
        .unwrap_or(0)
}

// **Los techos de aguas SÍ son del jugador, desde el 8 ago 2026.**
//
// Del 4 al 8 de agosto no lo fueron, y el motivo estaba escrito acá: "lo que uno
// construye a mano son CAJAS (pared + piso + tumbado plano) […] el tejado de aguas
// es lo que distingue a las casas ya hechas del mundo". Se dijo entonces que si
// algún día el jugador construía casas con tejado esto volvía — y volvió, porque
// pasó justo eso.
//
// Lo que cambió es la CARA. Mientras el frente de tu casa era un paño de Room
// Builder, taparla con un tumbado liso no desentonaba: el conjunto entero se leía
// provisorio, y ahí la caja era honesta. Con la fachada de Modern Exteriors puesta,
// una casa con frente de edificio y tapa lisa se ve mitad y mitad — el frente de un
// local y, encima, una losa de madera. El tejado no es un adorno: es la otra mitad
// de la cáscara que la fachada empezó.
//
// (La distinción con las casas del mapa no se pierde: ésas siguen siendo un dibujo
// entero del pack por lote, `building::roof_sprite`, y no una superficie autotileada
// losa a losa.)

pub fn ceiling_piece_count() -> usize {
    CEILING_PIECES.len()
}

pub fn ceiling_piece(idx: usize) -> Option<&'static CeilingPiece> {
    CEILING_PIECES.get(idx)
}

/// Índice de paleta por id (para resolver lo persistido sin atarse al orden).
pub fn ceiling_piece_index(id: &str) -> Option<usize> {
    CEILING_PIECES.iter().position(|p| p.id == id)
}

/// **Apoyo de una tapa**: se techa lo que construiste. La celda tiene que tener
/// piso construido, o una pared en alguno de sus bordes — así el alero de un
/// muro entra pero no se puede techar el medio de un prado.
pub fn ceiling_supported(
    has_floor: &dyn Fn(i64, i64) -> bool,
    has_wall_around: &dyn Fn(i64, i64) -> bool,
    tx: i64,
    tz: i64,
) -> bool {
    has_floor(tx, tz) || has_wall_around(tx, tz)
}

pub fn edge_piece_count() -> usize {
    EDGE_PIECES.len()
}

pub fn edge_piece(idx: usize) -> Option<&'static EdgePiece> {
    EDGE_PIECES.get(idx)
}

/// Índice de paleta de una pieza por id (para resolver lo persistido).
pub fn edge_piece_index(id: &str) -> Option<usize> {
    EDGE_PIECES.iter().position(|p| p.id == id)
}

/// Pieza de madera por defecto para lo que no resuelva (nunca falla).
pub fn default_edge_piece() -> &'static EdgePiece {
    &EDGE_PIECES[0]
}

/// Las dos celdas que el borde separa: `[antes, después]` en el eje del borde
/// (H → norte/sur; V → oeste/este).
pub fn edge_cells(tx: i64, tz: i64, axis: WallAxis) -> [(i64, i64); 2] {
    match axis {
        WallAxis::H => [(tx, tz - 1), (tx, tz)],
        WallAxis::V => [(tx - 1, tz), (tx, tz)],
    }
}

/// Centro del borde en coordenadas de mundo (x, z).
pub fn edge_center(tx: i64, tz: i64, axis: WallAxis) -> (f32, f32) {
    match axis {
        WallAxis::H => ((tx as f32 + 0.5) * TILE_M, tz as f32 * TILE_M),
        WallAxis::V => (tx as f32 * TILE_M, (tz as f32 + 0.5) * TILE_M),
    }
}

/// **Snapping**: borde de la cuadrícula más cercano al punto de mundo `(wx, wz)`.
/// Es lo único que traduce "dónde apunta el dedo" → "qué borde". Mira las 4
/// aristas de la celda apuntada y devuelve la más cercana.
pub fn nearest_edge(wx: f32, wz: f32) -> (i64, i64, WallAxis) {
    let tx = (wx / TILE_M).floor() as i64;
    let tz = (wz / TILE_M).floor() as i64;
    let fx = wx / TILE_M - tx as f32; // 0..1 dentro de la celda
    let fz = wz / TILE_M - tz as f32;
    // (distancia, borde)
    let candidates = [
        (fz, (tx, tz, WallAxis::H)),           // norte
        (1.0 - fz, (tx, tz + 1, WallAxis::H)), // sur
        (fx, (tx, tz, WallAxis::V)),           // oeste
        (1.0 - fx, (tx + 1, tz, WallAxis::V)), // este
    ];
    let mut best = candidates[0];
    for c in &candidates[1..] {
        if c.0 < best.0 {
            best = *c;
        }
    }
    best.1
}

/// ¿El tile cuenta como PISO de base para apoyar una pared? Sólo el piso
/// construido — así la primera pared exige haber puesto piso (regla LDoE).
///
/// **Los DOS pisos de la paleta, no sólo el de madera.** Mientras "Piso piedra"
/// colocaba un ladrillo sólido esto no se notaba; ahora que es una losa que se
/// camina ([`Tile::StoneFloor`]), dejarla afuera significaría que sobre tu piso
/// de piedra no entra ni una pared, ni un techo, ni la losa del piso de arriba —
/// una regla que depende de con qué material empezaste, que es la peor clase.
/// La vereda NO entra: es calle, no cimiento.
///
/// **Los escalones tampoco.** Un tramo de escalera es piso construido y se
/// camina, pero apoyar una pared en él dejaría cerrar la escalera con un muro a
/// media subida, y apoyar la losa del piso de arriba la taparía. Que sea el único
/// piso sobre el que no se construye es lo que garantiza que un tramo empezado
/// llegue siempre a algún sitio.
pub fn is_build_floor(t: Tile) -> bool {
    matches!(t, Tile::WoodFloor | Tile::StoneFloor)
}

/// **¿Esta celda es PIEZA DE PISO construida?** Es [`is_build_floor`] más el tramo
/// de escalera.
///
/// La diferencia con `is_build_floor` es de qué se está preguntando:
///
/// - `is_build_floor` = "¿sirve de CIMIENTO para una pared?" — el escalón no,
///   porque si no se podría tapiar la escalera a media subida.
/// - `is_floor_piece` = "¿se camina, aguanta lo de encima y cuenta como interior?"
///   — el escalón sí, obviamente: es piso, y **una escalera sostiene su propio
///   rellano**. Sin esto la losa de llegada nacía sin apoyo y el sistema de
///   integridad la derrumbaba, o sea que toda escalera terminaba en el vacío.
pub fn is_floor_piece(t: Tile) -> bool {
    is_build_floor(t) || matches!(t, Tile::Step | Tile::Stairwell)
}

/// **Validación de apoyo**: una pared/puerta/ventana necesita piso construido en
/// al menos UNA de las dos celdas que separa (así se pega al borde del suelo).
pub fn wall_supported(has_floor: &dyn Fn(i64, i64) -> bool, tx: i64, tz: i64, axis: WallAxis) -> bool {
    edge_cells(tx, tz, axis).iter().any(|&(cx, cz)| has_floor(cx, cz))
}

/// **Apoyo de un PISO en altura** (`z > 0`), la regla de Project Zomboid.
///
/// En el suelo (`z <= 0`) un piso se coloca donde quieras: hay terreno debajo. En
/// altura no hay nada debajo salvo lo que construiste, así que una losa entra si:
///
/// 1. **Hay algo debajo que la aguante** (`supported_below`): el piso o la pared
///    del nivel de abajo, o sea que estás techando tu propia casa; o
/// 2. **se engancha a una losa vecina del MISMO nivel** (`floor_here`): así se
///    extiende una planta tile a tile y salen balcones y voladizos.
///
/// La regla 2 es encadenable a propósito —igual que en Zomboid— porque la que
/// impide el edificio flotante es la 1: la primera losa de una planta nueva SÓLO
/// puede nacer encima de algo. Sin la regla 1 podrías empezar un piso en el aire
/// en medio del océano; sin la regla 2 no podrías volar un alero ni un tile.
pub fn floor_supported(
    z_level: i32,
    supported_below: &dyn Fn(i64, i64) -> bool,
    floor_here: &dyn Fn(i64, i64) -> bool,
    tx: i64,
    tz: i64,
) -> bool {
    if z_level <= 0 {
        return true; // hay terreno: el suelo se apoya solo
    }
    if supported_below(tx, tz) {
        return true;
    }
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .any(|&(dx, dz)| floor_here(tx + dx, tz + dz))
}

/// **Colisión**: ¿la cápsula del jugador centrada en `(x, z)` pisa alguna pared
/// que corte el paso? `blocked(tx, tz, axis)` responde por borde.
///
/// Cada pared es un rectángulo fino: V → `x ∈ [tx±WALL_THICK/2]`, `z ∈ [tz, tz+1]`;
/// H → `z ∈ [tz±WALL_THICK/2]`, `x ∈ [tx, tx+1]`. Se prueban sólo los bordes que
/// tocan el AABB de la cápsula (a lo sumo un puñado).
pub fn wall_capsule_hits(x: f32, z: f32, blocked: &dyn Fn(i64, i64, WallAxis) -> bool) -> bool {
    let h = WALL_THICK * 0.5;
    let (x0, x1) = (x - PLAYER_RADIUS, x + PLAYER_RADIUS);
    let (z0, z1) = (z - PLAYER_RADIUS, z + PLAYER_RADIUS);
    let (cx0, cx1) = (((x0 - h) / TILE_M).floor() as i64, ((x1 + h) / TILE_M).ceil() as i64);
    let (cz0, cz1) = (((z0 - h) / TILE_M).floor() as i64, ((z1 + h) / TILE_M).ceil() as i64);

    // Verticales: línea x = tx, tramo de una celda en z.
    for tx in cx0..=cx1 {
        let wx = tx as f32 * TILE_M;
        if x1 <= wx - h || x0 >= wx + h {
            continue;
        }
        for tz in cz0..=cz1 {
            let (za, zb) = (tz as f32 * TILE_M, (tz + 1) as f32 * TILE_M);
            if z1 > za && z0 < zb && blocked(tx, tz, WallAxis::V) {
                return true;
            }
        }
    }
    // Horizontales: línea z = tz, tramo de una celda en x.
    for tz in cz0..=cz1 {
        let wz = tz as f32 * TILE_M;
        if z1 <= wz - h || z0 >= wz + h {
            continue;
        }
        for tx in cx0..=cx1 {
            let (xa, xb) = (tx as f32 * TILE_M, (tx + 1) as f32 * TILE_M);
            if x1 > xa && x0 < xb && blocked(tx, tz, WallAxis::H) {
                return true;
            }
        }
    }
    false
}

/// Bordes que estampa un prefab colocado con su esquina mínima en
/// `(min_x, min_z)`: perímetro completo + tabiques interiores, con las puertas
/// en su lugar. El piso lo estampa aparte (toda la huella `w×h`).
///
/// Traduce el catálogo viejo (que describía la habitación por CELDAS de muro) al
/// modelo de bordes: una celda de muro del perímetro es la arista externa de la
/// habitación, y un tabique interior en la columna `dx` es la arista vertical
/// `x = min_x + dx`.
pub fn prefab_edges(p: &Prefab, min_x: i64, min_z: i64) -> Vec<(i64, i64, WallAxis, WallKind)> {
    let mut out = Vec::new();
    // Perímetro: dos líneas H (norte y sur) y dos V (oeste y este).
    for dx in 0..p.w {
        out.push((min_x + dx, min_z, WallAxis::H, WallKind::Wall));
        out.push((min_x + dx, min_z + p.h, WallAxis::H, WallKind::Wall));
    }
    for dz in 0..p.h {
        out.push((min_x, min_z + dz, WallAxis::V, WallKind::Wall));
        out.push((min_x + p.w, min_z + dz, WallAxis::V, WallKind::Wall));
    }
    // Tabiques interiores (columnas de muro) → aristas verticales.
    for &(dx, dz) in p.interior_walls {
        out.push((min_x + dx, min_z + dz, WallAxis::V, WallKind::Wall));
    }
    // Puertas: la del perímetro se ubica según en qué lado cae su celda; las
    // interiores parten el tabique vertical.
    let mut doors: Vec<(i64, i64, WallAxis)> = Vec::new();
    let (dx, dz) = p.door;
    doors.push(perimeter_door_edge(p, min_x, min_z, dx, dz));
    for &(dx, dz) in p.interior_doors {
        doors.push((min_x + dx, min_z + dz, WallAxis::V));
    }
    for (tx, tz, axis) in doors {
        // La puerta REEMPLAZA la pared de ese borde (no se apilan).
        out.retain(|&(ex, ez, ea, _)| !(ex == tx && ez == tz && ea == axis));
        out.push((tx, tz, axis, WallKind::Door));
    }
    out
}

/// Borde del perímetro donde cae una celda de puerta `(dx, dz)` del prefab.
fn perimeter_door_edge(p: &Prefab, min_x: i64, min_z: i64, dx: i64, dz: i64) -> (i64, i64, WallAxis) {
    if dz == p.h - 1 {
        (min_x + dx, min_z + p.h, WallAxis::H) // sur
    } else if dz == 0 {
        (min_x + dx, min_z, WallAxis::H) // norte
    } else if dx == 0 {
        (min_x, min_z + dz, WallAxis::V) // oeste
    } else {
        (min_x + p.w, min_z + dz, WallAxis::V) // este
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topdown::PREFABS;

    #[test]
    fn edge_snapping_picks_the_nearest_side() {
        // Cerca del borde norte de la celda (3,7).
        assert_eq!(nearest_edge(3.5, 7.05), (3, 7, WallAxis::H));
        // Cerca del borde sur → el H de la fila siguiente.
        assert_eq!(nearest_edge(3.5, 7.95), (3, 8, WallAxis::H));
        // Cerca del oeste / este.
        assert_eq!(nearest_edge(3.02, 7.5), (3, 7, WallAxis::V));
        assert_eq!(nearest_edge(3.97, 7.5), (4, 7, WallAxis::V));
        // Funciona en coordenadas negativas (mapa infinito).
        assert_eq!(nearest_edge(-0.1, -0.5), (0, -1, WallAxis::V));
    }

    #[test]
    fn edge_cells_and_center_are_consistent() {
        assert_eq!(edge_cells(4, 9, WallAxis::H), [(4, 8), (4, 9)]);
        assert_eq!(edge_cells(4, 9, WallAxis::V), [(3, 9), (4, 9)]);
        let (cx, cz) = edge_center(4, 9, WallAxis::H);
        assert!((cx - 4.5).abs() < 1e-5 && (cz - 9.0).abs() < 1e-5);
        let (cx, cz) = edge_center(4, 9, WallAxis::V);
        assert!((cx - 4.0).abs() < 1e-5 && (cz - 9.5).abs() < 1e-5);
    }

    #[test]
    fn walls_need_a_floor_on_one_side() {
        let floor_at_5_5 = |cx: i64, cz: i64| (cx, cz) == (5, 5);
        // Borde norte de (5,5): apoya (la celda sur tiene piso).
        assert!(wall_supported(&floor_at_5_5, 5, 5, WallAxis::H));
        // Borde de una celda sin piso a ninguno de los dos lados: no apoya.
        assert!(!wall_supported(&floor_at_5_5, 20, 20, WallAxis::H));
        assert!(is_build_floor(Tile::WoodFloor) && !is_build_floor(Tile::Grass));
    }

    /// **El piso de piedra es un PISO.** La pieza "Piso piedra" colocaba
    /// `StoneBrick`, que bloquea el paso y tapa lo de atrás: ponías tu piso y
    /// quedabas parado sobre una pared. Y como `is_build_floor` sólo miraba la
    /// madera, encima no aguantaba nada de lo que se construye sobre un piso.
    #[test]
    fn el_piso_de_piedra_se_camina_y_aguanta_paredes() {
        assert!(!Tile::StoneFloor.solid(), "el piso de piedra frena al jugador");
        assert!(!Tile::StoneFloor.upright(), "el piso de piedra tapa lo de atrás");
        assert!(Tile::StoneFloor.buildable_ground(), "no se puede fundar sobre él");
        assert!(is_build_floor(Tile::StoneFloor), "no aguanta una pared");
        // Y sigue siendo distinto del ladrillo de los asentamientos generados.
        assert!(Tile::StoneBrick.solid() && !is_build_floor(Tile::StoneBrick));
        // La vereda se camina pero NO es cimiento: es calle.
        assert!(!is_build_floor(Tile::Pavement));
        // La pieza de la paleta coloca el piso, no el ladrillo.
        let piso = crate::topdown::BUILD_PIECES
            .iter()
            .find(|p| p.id == "build_stone_floor")
            .expect("falta la pieza de piso de piedra");
        assert_eq!(piso.tile, Tile::StoneFloor);
        assert_eq!(piso.tile.art_key(), Some("floor/stone"));
    }

    #[test]
    fn capsule_collides_with_thin_walls_only_where_they_are() {
        // Pared vertical en x = 10, celda z = 3.
        let wall = |tx: i64, tz: i64, a: WallAxis| (tx, tz, a) == (10, 3, WallAxis::V);
        assert!(wall_capsule_hits(10.0, 3.5, &wall), "encima de la pared");
        assert!(wall_capsule_hits(9.8, 3.5, &wall), "el radio la toca");
        assert!(!wall_capsule_hits(9.0, 3.5, &wall), "lejos: libre");
        assert!(!wall_capsule_hits(10.0, 5.5, &wall), "otra fila: libre");
        // Pared horizontal en z = 3, celda x = 10.
        let wall = |tx: i64, tz: i64, a: WallAxis| (tx, tz, a) == (10, 3, WallAxis::H);
        assert!(wall_capsule_hits(10.5, 3.0, &wall));
        assert!(!wall_capsule_hits(10.5, 4.5, &wall));
    }

    #[test]
    fn doors_do_not_block_but_walls_and_windows_do() {
        assert!(WallKind::Wall.blocks());
        assert!(WallKind::Window.blocks());
        assert!(!WallKind::Door.blocks());
        // El vano es un hueco: se cruza igual que la puerta, y como ella habilita
        // el cuarto. La ventana cierra y NO es entrada (por una ventana no se entra).
        assert!(!WallKind::Doorway.blocks());
        assert!(WallKind::Doorway.is_entrance() && WallKind::Door.is_entrance());
        assert!(!WallKind::Window.is_entrance() && !WallKind::Wall.is_entrance());
    }

    /// **El juego de piezas es el mismo en cada material.** Si mañana entra un
    /// material nuevo (ladrillo, chapa) y le falta la puerta o el vano, esto lo
    /// caza: elegir textura no puede costar piezas.
    #[test]
    fn el_juego_de_piezas_es_el_mismo_en_cada_material() {
        use std::collections::{BTreeMap, BTreeSet};
        // La baranda es la excepción declarada: no es una cara de la casa, es el
        // borde de un balcón, y no tiene sentido pedirle una versión por material.
        let mut por_material: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for p in EDGE_PIECES.iter().filter(|p| p.id != "edge_railing_wood") {
            por_material
                .entry(p.cost_item)
                .or_default()
                .insert(match p.kind {
                    WallKind::Wall => "pared",
                    WallKind::Doorway => "vano",
                    WallKind::Door => "puerta",
                    WallKind::Window => "ventana",
                });
        }
        let completo: BTreeSet<&str> = ["pared", "vano", "puerta", "ventana"].into();
        for (material, piezas) in &por_material {
            assert_eq!(piezas, &completo, "al material '{material}' le faltan piezas");
        }
        // Y cada material tiene su tapa: una casa se termina con el techo puesto.
        for material in por_material.keys() {
            let tapa = ceiling_piece(ceiling_piece_for(material)).expect("tapa");
            assert_eq!(
                tapa.cost_item, *material,
                "'{material}' no tiene tumbado propio"
            );
        }
        assert!(por_material.len() >= 2, "un solo material no prueba nada");
    }

    /// **Y la fachada también trae el juego completo.**
    ///
    /// El test de arriba agrupa por `cost_item`, y los materiales de fachada cuestan
    /// `stone` a propósito (una fachada es una terminación, no un recurso nuevo), así
    /// que se le cuelan de la mano del material que sí está completo. Éste agrupa por
    /// el SLOT, que es lo que de verdad distingue un material de fachada de otro:
    /// sin él, se podría agregar media familia —pared y puerta y nada más— y ningún
    /// test se enteraría. Es la misma regla de biblia §5, aplicada donde ahora vive.
    #[test]
    fn la_fachada_trae_el_juego_completo() {
        use std::collections::{BTreeMap, BTreeSet};
        let mut por_slot: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for p in EDGE_PIECES.iter().filter(|p| es_fachada(p)) {
            // `wall/fachada_local` → `fachada_local`
            let material = p.art.split('/').nth(1).expect("el slot lleva namespace");
            por_slot.entry(material).or_default().insert(match p.kind {
                WallKind::Wall => "pared",
                WallKind::Doorway => "vano",
                WallKind::Door => "puerta",
                WallKind::Window => "ventana",
            });
        }
        assert!(!por_slot.is_empty(), "no hay ninguna fachada en el catálogo");
        let completo: BTreeSet<&str> = ["pared", "vano", "puerta", "ventana"].into();
        for (material, piezas) in &por_slot {
            assert_eq!(piezas, &completo, "a la fachada '{material}' le faltan piezas");
        }
    }

    /// **Las fachadas van al final del catálogo, y no es cosmético.**
    ///
    /// `build::edge_piece_of_kind` devuelve el PRIMER `(kind, cost_item)` que calce, y
    /// de él salen los anillos de los prefabs. Con una fachada de piedra antes que
    /// `edge_wall_stone`, `room_small` y `casa_1` se estamparían de fachada: una
    /// habitación interior con el frente de un local mirando para afuera, y encima
    /// más alta que la casa. Reordenar la tabla es exactamente lo que rompe esto, así
    /// que queda clavado acá.
    #[test]
    fn los_prefabs_no_estampan_fachadas() {
        for material in ["wood", "stone"] {
            for kind in [WallKind::Wall, WallKind::Doorway, WallKind::Door, WallKind::Window] {
                let primera = EDGE_PIECES
                    .iter()
                    .find(|p| p.kind == kind && p.cost_item == material)
                    .expect("el material trae el juego completo");
                assert!(
                    !es_fachada(primera),
                    "la primera pieza {kind:?} de '{material}' es la fachada `{}`: \
                     los prefabs la van a estampar",
                    primera.art
                );
            }
        }
    }

    #[test]
    fn prefabs_stamp_a_closed_ring_with_one_door() {
        for p in PREFABS {
            let edges = prefab_edges(p, 0, 0);
            let doors = edges.iter().filter(|e| e.3 == WallKind::Door).count();
            assert_eq!(doors, 1 + p.interior_doors.len(), "{}", p.id);
            // Perímetro completo (2·w horizontales + 2·h verticales) + tabiques.
            // La puerta del perímetro reemplaza a su pared, así que no suma.
            let perimeter = 2 * (p.w + p.h) as usize;
            assert_eq!(
                edges.len(),
                perimeter + p.interior_walls.len() + p.interior_doors.len(),
                "{} no cierra el anillo",
                p.id
            );
            // Ningún borde repetido (la puerta reemplaza a su pared).
            let mut seen = std::collections::BTreeSet::new();
            for &(tx, tz, a, _) in &edges {
                assert!(seen.insert((tx, tz, a.as_u8())), "{} borde duplicado", p.id);
            }
        }
    }

    /// El segundo piso: la primera losa nace sobre la casa de abajo, la segunda
    /// se engancha a la primera, y en el aire virgen no entra nada.
    #[test]
    fn un_piso_alto_necesita_algo_debajo_o_un_vecino() {
        // Abajo hay casa en la franja x ∈ [0,4], z = 0.
        let below = |cx: i64, cz: i64| (0..5).contains(&cx) && cz == 0;
        let no_floor = |_: i64, _: i64| false;
        // Sobre la casa: entra.
        assert!(floor_supported(1, &below, &no_floor, 2, 0));
        // Al lado de la casa, sin vecino en el mismo nivel: no entra.
        assert!(!floor_supported(1, &below, &no_floor, 2, 3));
        // Con una losa vecina ya puesta en (2,2): el voladizo a (2,3) entra.
        let floor_at_2_2 = |cx: i64, cz: i64| (cx, cz) == (2, 2);
        assert!(floor_supported(1, &below, &floor_at_2_2, 2, 3));
        // Y en el suelo (o bajo tierra) la regla no aplica: siempre hay terreno.
        assert!(floor_supported(0, &no_floor, &no_floor, 900, 900));
        assert!(floor_supported(-2, &no_floor, &no_floor, 900, 900));
    }

    #[test]
    fn edge_catalogue_is_sane() {
        assert!(edge_piece_count() >= 3);
        for i in 0..edge_piece_count() {
            let p = edge_piece(i).unwrap();
            assert!(!p.art.is_empty() && p.cost_qty > 0, "{}", p.id);
            assert_eq!(edge_piece_index(p.id), Some(i));
        }
        assert!(edge_piece(edge_piece_count()).is_none());
        assert!(edge_piece_index("no_existe").is_none());
    }
}
