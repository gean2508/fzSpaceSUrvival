//! Paredes de BORDE (construcción estilo *Last Day on Earth*).
//!
//! El piso vive en la celda (`TileWorld`); la pared vive en la **arista** entre
//! dos celdas. Las reglas puras (snapping, apoyo, colisión) están en
//! `ha_shared::build_grid`; acá vive el estado del juego:
//! - `BuiltWalls`: lo construido, con persistencia en `walls.json`
//!   (mismo debounce/flush-on-pause que `roofs.json` y `structures.json`),
//! - streaming de sprites alrededor del jugador (Y-sorted, anclados al pie del
//!   borde y con la pose que corresponde: `front`/`back`/`left`/`right`),
//! - puertas: se abren/cierran por proximidad (poses `open`/`closed`).
//!
//! Las poses las recorta el usuario en el editor de assets sobre el atlas del
//! slot (`wall/wood`, `wall/stone`, `door/wood`, `window/basic`). Sin arte
//! asignado el sistema igual funciona: quad de color.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use ha_shared::build_grid::{
    edge_center, edge_cells, edge_piece, edge_piece_index, is_build_floor, EdgePiece, WallAxis,
    WallKind, EDGE_PIECES,
};
use ha_shared::topdown::TILE_M;
use ha_shared::CelestialBody;

use crate::art2d::ArtLibrary;
use crate::inventory_persist::persist_dir;
use crate::platform::is_app_paused;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::Player;
use crate::world2d::{
    active_surface_body, render_xy, sort_z, surface_region_active, ActivePlanet, TileWorld,
};

/// Radio (tiles) alrededor del jugador donde las paredes existen como entidad.
const WALL_STREAM_RADIUS: i64 = 48;
/// Refresco del streaming (s). Colocar/quitar refresca al instante igual.
const WALL_STREAM_PERIOD: f32 = 0.2;
/// Distancia (tiles) a la que una puerta se abre.
const DOOR_OPEN_DIST: f32 = 1.7;

/// Una pared colocada. `(tx, tz, axis)` es el BORDE (ver `build_grid`), no la
/// celda; `piece` es el índice en `EDGE_PIECES` (se persiste por id, no por
/// índice, para que reordenar el catálogo no rompa partidas guardadas).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PlacedWall {
    pub body: u8,
    pub z: i32,
    pub tx: i64,
    pub tz: i64,
    pub axis: WallAxis,
    pub piece: usize,
}

impl PlacedWall {
    pub fn def(&self) -> &'static EdgePiece {
        edge_piece(self.piece).unwrap_or(&EDGE_PIECES[0])
    }
}

/// Clave de un borde: cuerpo, piso, celda y eje. Es lo que indexa `BuiltWalls`.
type EdgeKey = (u8, i32, i64, i64, u8);

fn edge_key(w: &PlacedWall) -> EdgeKey {
    (w.body, w.z, w.tx, w.tz, w.axis.as_u8())
}

/// Registro persistente de paredes construidas por el jugador.
///
/// **Indexado por borde, no barrido.** `blocks()` es la consulta más caliente del
/// juego: la llama `wall_capsule_hits` para cada arista que toca la cápsula (≈9),
/// dos veces por eje y por frame, para el jugador **y para cada animal y NPC**.
/// Con la búsqueda lineal de antes eso era `paredes × 9 × 2 × entidades` por
/// frame: gratis con la choza de cuatro paredes de la prueba, y decenas de
/// millones de comparaciones en cuanto alguien levanta el refugio que este
/// sistema existe para levantar. Justo el costo que aparece cuando el jugador
/// hace lo que le pedimos.
///
/// El `Vec` se queda porque el streaming y el guardado necesitan recorrer en
/// orden estable; el mapa sólo traduce borde → posición.
#[derive(Resource, Default)]
pub struct BuiltWalls {
    pub list: Vec<PlacedWall>,
    /// Borde → índice en `list`. Se mantiene al día en `add`/`remove`.
    index: std::collections::HashMap<EdgeKey, usize>,
    /// Sube en cada alta/baja: el streaming lo mira para refrescar al instante
    /// (`is_changed()` no sirve — el sistema de guardado toca el recurso cada
    /// frame y lo marcaría siempre).
    rev: u64,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

impl BuiltWalls {
    /// Contador de altas/bajas. Quien cachea algo derivado de lo construido
    /// (el streaming de sprites, el análisis de cuartos) lo mira para saber si
    /// tiene que rehacerlo.
    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// La pared que ocupa ese borde, si hay alguna.
    pub fn at(&self, body: CelestialBody, z: i32, tx: i64, tz: i64, axis: WallAxis) -> Option<&PlacedWall> {
        let idx = *self.index.get(&(body as u8, z, tx, tz, axis.as_u8()))?;
        self.list.get(idx)
    }

    /// ¿Hay alguna pieza de borde en las CUATRO aristas de esta celda? Es lo que
    /// hace que el anillo de paredes de una planta aguante la losa de la de
    /// arriba (`build_grid::floor_supported`): techás tu casa apoyándote en sus
    /// propias paredes, aunque el interior no tenga piso construido.
    pub fn any_edge_of_cell(&self, body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
        let h = WallAxis::H.as_u8();
        let v = WallAxis::V.as_u8();
        let b = body as u8;
        self.index.contains_key(&(b, z, tx, tz, h))
            || self.index.contains_key(&(b, z, tx, tz + 1, h))
            || self.index.contains_key(&(b, z, tx, tz, v))
            || self.index.contains_key(&(b, z, tx + 1, tz, v))
    }

    /// ¿Ese borde corta el paso? (pared/ventana sí, puerta no).
    pub fn blocks(&self, body: CelestialBody, z: i32, tx: i64, tz: i64, axis: WallAxis) -> bool {
        self.at(body, z, tx, tz, axis)
            .map(|w| w.def().kind.blocks())
            .unwrap_or(false)
    }

    /// Coloca (reemplazando lo que hubiera en ese borde).
    pub fn add(&mut self, w: PlacedWall) {
        let key = edge_key(&w);
        match self.index.get(&key) {
            // Ya había algo en ese borde: se reemplaza en su sitio (el índice de
            // los demás no se mueve).
            Some(&idx) => self.list[idx] = w,
            None => {
                self.index.insert(key, self.list.len());
                self.list.push(w);
            }
        }
        self.dirty = true;
        self.rev += 1;
    }

    /// Quita la pared de un borde; devuelve la que estaba.
    pub fn remove(
        &mut self,
        body: CelestialBody,
        z: i32,
        tx: i64,
        tz: i64,
        axis: WallAxis,
    ) -> Option<PlacedWall> {
        let idx = self.index.remove(&(body as u8, z, tx, tz, axis.as_u8()))?;
        // `swap_remove` es O(1) pero mueve la última al hueco: hay que reapuntar
        // su entrada del índice o la pared movida deja de existir para la
        // colisión (y sigue dibujándose). Es el bug clásico de este patrón.
        let gone = self.list.swap_remove(idx);
        if idx < self.list.len() {
            let moved = edge_key(&self.list[idx]);
            self.index.insert(moved, idx);
        }
        self.dirty = true;
        self.rev += 1;
        Some(gone)
    }
}

/// Sprite vivo de una pared (clave de streaming + pose actual).
#[derive(Component)]
pub(crate) struct WallSprite {
    pub(crate) tx: i64,
    pub(crate) tz: i64,
    axis: WallAxis,
    z: i32,
    body: CelestialBody,
    cara: Cara,
    piece: usize,
}

/// Puerta de borde: alterna `open`/`closed` por proximidad del jugador.
#[derive(Component)]
struct WallDoor {
    /// Segundos que le quedan a la transición `opening`/`closing` en curso.
    t: f32,
    /// Centro del borde en mundo (x, z) — para la distancia al jugador.
    cx: f32,
    cz: f32,
    art: &'static str,
    open: bool,
    /// Medida con la que se spawneó. Cambiar de pose tiene que redibujar del
    /// MISMO tamaño: con una constante acá, la puerta cambiaba de talle al
    /// abrirse.
    size: Vec2,
    /// La cara en la que quedó puesta. Decide si se anima con la hoja de fachada
    /// o con la de interior (ver [`pose_de_puerta`]).
    cara: Cara,
}

pub struct WallsPlugin;

impl Plugin for WallsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuiltWalls>()
            .add_systems(Startup, load_walls_on_startup)
            .add_systems(Update, (stream_walls, animate_wall_doors, debounced_walls_save));
    }
}

/// **Qué ES esta pieza para la habitación que encierra**, que es lo único que
/// decide cómo se dibuja.
///
/// El bug que esto viene a matar: antes la cara se resolvía a un NOMBRE DE POSE
/// (`front`/`back`/`left`/`right`) y la geometría era la misma para las cuatro —
/// un cartel de un tile de ancho y 2,4 de alto, plantado y mirando a cámara. O
/// sea que un muro lateral se dibujaba EXACTAMENTE igual que uno de frente, y
/// levantar cuatro paredes daba cuatro carteles de frente en vez de una caja. Y
/// encima `right` heredaba el alto del antepecho, así que el lado este de tu
/// choza quedaba a la rodilla y el oeste entero.
///
/// En cenital 3/4 hay TRES dibujos, no cuatro, porque los dos laterales son el
/// mismo dibujo espejado:
///
/// ```text
///        ╔═══════════════╗   ← Fondo: cara entera, un tile de ancho
///        ║               ║
///        ┃               ┃   ← Lateral: losa FINA de canto, corre con la pared
///        ┃               ┃
///        ▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁   ← Cercano: antepecho bajo, se ve por encima
/// ```
/// **Y hay una quinta pregunta, que es la que traen las fachadas** (8 ago 2026):
/// ¿esta cara da a la calle o separa dos ambientes de la misma casa?
///
/// Hasta hoy `Fondo` metía en la misma bolsa el TABIQUE interior (piso de los dos
/// lados) y la ESPALDA del edificio (piso sólo al sur), y `Lateral { este: false }`
/// metía además el tabique norte-sur. Daba igual mientras las cuatro caras salieran
/// del mismo pack de interiores; deja de dar igual en cuanto la cara de afuera se
/// dibuja con la elevación de Modern Exteriors, que es tres veces más alta y va en
/// otra banda de z ([`crate::layers::FACHADA_BASE`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Cara {
    /// TABIQUE interior que corre E-O: hay piso construido de los dos lados. No
    /// da a la calle, así que se dibuja con arte de interior y Y-sortea normal.
    Tabique,
    /// Cara NORTE del edificio (piso sólo al sur): su sprite crece sobre la
    /// calle, o sea que lo que se ve ahí es la ESPALDA de la casa.
    Fondo,
    /// Cara SUR del edificio (piso sólo al norte): el FRENTE, lo primero que ve
    /// alguien parado en la vereda.
    Cercano,
    /// Muro LATERAL (corre N-S). Se ve DE CANTO. `este` = es el muro este de la
    /// habitación (o sea, el cuarto queda al oeste). `interior` = es un tabique
    /// (piso de los dos lados), no el costado del edificio.
    Lateral { este: bool, interior: bool },
}

impl Cara {
    /// La cara que toca según de qué lado hay piso construido.
    fn de(axis: WallAxis, floor_a: bool, floor_b: bool) -> Cara {
        match axis {
            // a = celda norte, b = celda sur.
            WallAxis::H => match (floor_a, floor_b) {
                (true, false) => Cara::Cercano, // habitación al norte → su FRENTE
                (false, true) => Cara::Fondo,   // habitación al sur → su ESPALDA
                // Piso de los dos lados = tabique. Piso de ninguno = una pared que
                // se quedó sin apoyo: `integrity.rs` puede sacar las dos losas en
                // el mismo frame en que esto se evalúa, y tratarla como tabique es
                // exactamente lo que hacía antes — o sea, lo seguro.
                _ => Cara::Tabique,
            },
            // a = celda oeste, b = celda este.
            WallAxis::V => Cara::Lateral {
                este: floor_a && !floor_b,
                interior: floor_a && floor_b,
            },
        }
    }

    /// Nombre de pose que le pide al editor. Si el usuario recortó esa cara, se
    /// usa su pixel; si no, se cae a `front`/`center` y la GEOMETRÍA sigue siendo
    /// la correcta — que es la mitad que de verdad hace leer la caja.
    ///
    /// `back` **ya significaba "la cara de afuera"** en este repo:
    /// `scripts/import-construccion.py` la recorta con `afuera()` ("pared plana,
    /// sin friso ni zócalo"). O sea que el frente de una casa siempre pidió la
    /// pose correcta; lo único que faltaba era el pixel de exteriores.
    fn pose(self) -> &'static str {
        match self {
            Cara::Tabique => "front",
            // Pose NUEVA y opcional: sin recorte propio cae a `front` por la
            // cadena y la espalda se ve como se veía hasta hoy.
            Cara::Fondo => "espalda",
            Cara::Cercano => "back",
            Cara::Lateral { este: false, .. } => "left",
            Cara::Lateral { este: true, .. } => "right",
        }
    }

    fn es_lateral(self) -> bool {
        matches!(self, Cara::Lateral { .. })
    }

    /// **¿Esta cara da a la calle?** Piso de un solo lado = es el borde del
    /// edificio, no un tabique.
    ///
    /// Es lo que decide la banda de dibujo: nada de adentro de una casa —su
    /// tumbado, su techo, sus muebles— puede quedar delante de su propia cara
    /// exterior (ver [`crate::layers::FACHADA_BASE`]).
    pub(crate) fn es_exterior(self) -> bool {
        !matches!(self, Cara::Tabique | Cara::Lateral { interior: true, .. })
    }
}

/// Cadena de respaldo de poses: la que toca, la genérica del slot, `center`.
fn pose_chain(cara: Cara) -> [&'static str; 3] {
    [cara.pose(), "front", "center"]
}

/// **La cadena de poses de una PUERTA, según su cara y su estado.**
///
/// Existe porque hasta hoy la puerta era la única pieza que ignoraba la cara:
/// `animate_wall_doors` pedía `"open"`/`"closed"` a secas, así que una puerta
/// puesta en el frente de la casa —la que se ve desde la vereda— se dibujaba con
/// la hoja de interior en cuanto alguien se le acercaba.
///
/// Las dos reglas que la forman:
///
/// - **Sólo el FRENTE tiene puerta de fachada.** La espalda y los tabiques siguen
///   con la de interior: el pack no trae una hoja de atrás, y una puerta de
///   interior en un tabique es exactamente lo que corresponde.
/// - **Una cara nunca cae en otra cara.** Si el tema de fachada no trae el cuadro
///   abierto —y hay varios que no lo traen— la hoja se queda dibujada cerrada,
///   que es raro pero es de la misma casa. Caerse a `"front"` pondría el paño de
///   pared interior en medio del frente, que es el defecto que este trabajo entero
///   viene a sacar. (Se cruza igual: `WallKind::Door` no frena, mire como mire.)
fn pose_de_puerta(cara: Cara, estado: &str) -> [&'static str; 4] {
    if !matches!(cara, Cara::Cercano) {
        return match estado {
            "open" => ["open", "closed", "front", "center"],
            "opening" => ["opening", "closed", "front", "center"],
            "closing" => ["closing", "closed", "front", "center"],
            _ => ["closed", "front", "center", "center"],
        };
    }
    match estado {
        "open" => ["back_open", "back_closed", "open", "closed"],
        "opening" => ["back_opening", "back_open", "back_closed", "closed"],
        "closing" => ["back_closing", "back_closed", "closed", "closed"],
        _ => ["back_closed", "closed", "front", "center"],
    }
}

/// **La cara que le toca a un borde según dónde está el piso construido.**
///
/// Vive acá —y no dentro del streaming— porque la usan los DOS lados de la
/// misma moneda: el que dibuja la pared puesta y el que dibuja el fantasma antes
/// de ponerla. Si cada uno la dedujera por su cuenta, el fantasma volvería a
/// prometer una cosa y el juego a colocar otra.
pub(crate) fn cara_de_borde(
    has_floor: &dyn Fn(i64, i64) -> bool,
    tx: i64,
    tz: i64,
    axis: WallAxis,
) -> Cara {
    let [a, b] = edge_cells(tx, tz, axis);
    Cara::de(axis, has_floor(a.0, a.1), has_floor(b.0, b.1))
}

/// **Lo que se ve de una pieza de borde: su sprite y su medida.**
///
/// **Una sola función para el fantasma y para la pared.** Era el bug de la
/// puerta: el fantasma mostraba el marco entero, con su pixel y su tamaño, y al
/// aceptar salía *"un palo con una puntita blanca"* — porque el que colocaba
/// dibujaba otra cosa (una franja recortada del medio del dibujo, estirada sobre
/// una losa fina inventada). Mientras existan dos caminos, uno de los dos miente.
///
/// Y devuelve el **sprite entero**, nunca un recorte: si el usuario le asignó una
/// pose a esa cara se dibuja esa pose completa, y si no, la de frente completa.
/// Sacar "material" de la columna central del dibujo funcionaba de casualidad en
/// un paño de tablones y era un disparate en cualquier pieza con hueco — la
/// columna central de un VANO es el vano, o sea nada.
pub(crate) fn vista_de_pieza(art: &ArtLibrary, p: &EdgePiece, cara: Cara) -> (Sprite, Vec2) {
    let size = medida_de_pieza(art, p, cara);
    (wall_sprite(art, p, &pose_chain(cara), size), size)
}

/// Color de respaldo cuando el slot no tiene arte todavía (el sistema se ve y se
/// prueba igual).
fn fallback_color(p: &EdgePiece) -> Color {
    // Una FACHADA sin pixel se ve de LADRILLO, no de tabla ni de losa gris: es el
    // paño que trae el pack (`import-fachadas.py` mide rgb≈162,91,76), y con el
    // color de la madera la pieza nueva se veía igual que la vieja.
    if ha_shared::build_grid::es_fachada(p) {
        return match p.kind {
            WallKind::Door => Color::srgb(0.42, 0.46, 0.52),
            WallKind::Window => Color::srgb(0.62, 0.70, 0.78),
            WallKind::Wall | WallKind::Doorway => Color::srgb(0.64, 0.36, 0.30),
        };
    }
    // El material sale del slot (`wall/stone`, `door/stone`, `doorway/stone`…),
    // así que la pieza de piedra se ve de piedra aunque nadie le haya asignado
    // pixel todavía.
    let piedra = p.art.contains("stone") || p.art.contains("piedra");
    match p.kind {
        WallKind::Door if piedra => Color::srgb(0.38, 0.36, 0.34),
        WallKind::Door => Color::srgb(0.30, 0.20, 0.10),
        WallKind::Window if piedra => Color::srgb(0.58, 0.66, 0.70),
        WallKind::Window => Color::srgb(0.55, 0.65, 0.75),
        // El VANO es el marco: se dibuja con el color de SU pared, porque marco y
        // muro son el mismo material — lo que cambia es que en el medio no hay nada.
        WallKind::Wall | WallKind::Doorway if piedra => Color::srgb(0.55, 0.55, 0.58),
        WallKind::Wall | WallKind::Doorway => Color::srgb(0.45, 0.31, 0.17),
    }
}

/// Ancho de cada jamba del vano, en fracción del ancho de la pieza.
const JAMBA: f32 = 0.19;
/// Alto del dintel del vano, en fracción del alto de la pieza.
const DINTEL: f32 = 0.22;

/// **El vano dibujado por código**: dos jambas y un dintel, con el hueco vacío en
/// el medio.
///
/// Es el fallback de la pieza mientras su slot no tenga pixel, y no puede ser un
/// cuadrado de color como el de las demás: un vano pintado entero se ve
/// exactamente igual que una pared, o sea que la pieza nueva sería invisible
/// hasta que alguien le recorte el arte. Tres tiras alcanzan para que se lea "acá
/// se pasa" de un vistazo, que es todo lo que la pieza tiene que decir.
fn marco_de_vano(padre: &mut bevy::ecs::system::EntityCommands, size: Vec2, color: Color) {
    let (w, h) = (size.x, size.y);
    let jamba = Vec2::new(w * JAMBA, h);
    let dintel = Vec2::new(w, h * DINTEL);
    let tiras = [
        (Vec2::new(-(w - jamba.x) * 0.5, 0.0), jamba),
        (Vec2::new((w - jamba.x) * 0.5, 0.0), jamba),
        (Vec2::new(0.0, (h - dintel.y) * 0.5), dintel),
    ];
    padre.with_children(|p| {
        for (offset, tam) in tiras {
            p.spawn((
                Sprite::from_color(color, tam),
                // Un pelo adelante del quad transparente del padre, para que no
                // dependa del orden de dibujo entre hermanos.
                Transform::from_xyz(offset.x, offset.y, 0.001),
            ));
        }
    });
}

/// Sprite de la pared: primera pose disponible de la cadena, si no el arte plano
/// del slot, si no un quad de color.
fn wall_sprite(art: &ArtLibrary, p: &EdgePiece, poses: &[&str], size: Vec2) -> Sprite {
    for name in poses {
        if let Some(s) = art.pose_sprite(p.art, name, size) {
            return s;
        }
    }
    art.sprite_or_color(p.art, fallback_color(p), size)
}

/// **La medida de una pieza de borde: una celda del pack, un tile de la grilla.**
///
/// Ver [`ArtLibrary::medida_de_obra`] para el porqué. En corto: una pared
/// embaldosa a lo largo de un muro, así que tiene que medir lo que mide su ranura
/// o se monta encima de la vecina — que es exactamente lo que pasaba, con la
/// segunda pared tapando media primera y la tercera media segunda.
///
/// Los números, porque son la queja entera: `wall/wood` es un recorte de 1×2
/// celdas. Con la regla de los objetos (× [`ESCALA_ARTE`] = 1,5) daba 1,5 × 3,0 m
/// sobre un borde de 1 m → medio metro de solape con cada vecina. Con la regla de
/// obra da **1,0 × 2,0 m**: encaja borde con borde, conserva la proporción exacta
/// del dibujo y sigue siendo más alta que el jugador (1,9 m).
///
/// Una pieza que en el pack mide 2 celdas de ancho —el vano, que trae sus dos
/// jambas y el arco— ocupa 2 tiles, y eso está bien: lo que no puede es
/// solaparse con la que sigue.
///
/// La medida sale de LA POSE que se va a dibujar, no de `front` a secas: si el
/// usuario recortó una cara con otra forma, el sprite mide lo suyo.
/// La medida sale de **la misma pose que va a dibujar el sprite**, recorriendo
/// la misma cadena de respaldo ([`pose_chain`]). Si se preguntaran por poses
/// distintas, una cara sin recorte propio saldría con el pixel de `front` estirado
/// al tamaño de otra cosa — que es la forma sutil de volver a deformar el dibujo.
pub(crate) fn medida_de_pieza(art: &ArtLibrary, p: &EdgePiece, cara: Cara) -> Vec2 {
    let respaldo = Vec2::new(TILE_M, TILE_M * p.render_h);
    for name in pose_chain(cara) {
        // ⚠ El `has_pose` no sobra. `celdas_de(key, Some(pose))` **no devuelve
        // `None` cuando la pose falta**: cae al `rect` base del slot. Sin este
        // guard, el bucle cortaba SIEMPRE en la primera vuelta con la medida del
        // slot, mientras `wall_sprite` seguía buscando y terminaba dibujando otra
        // pose — o sea el pixel de una cara estirado al talle de otra. No se veía
        // porque en `wall/wood` el rect del slot ES el de `front`; con una fachada
        // (`back` de 1×3, `front` de 1×2) se deforma a la vista.
        if art.has_pose(p.art, name) {
            if let Some(celdas) = art.celdas_de(p.art, Some(name)) {
                return celdas * TILE_M;
            }
        }
    }
    art.medida_de_obra(p.art, None, respaldo)
}

/// **Qué celdas TAPA en pantalla el sprite de una pared.**
///
/// El sprite se apoya en su borde y crece hacia arriba, o sea hacia el norte del
/// mundo (render y = −z). Así que oculta las filas que tiene delante. Ésas son
/// las celdas que, si las pisás, tienen que hacerlo desvanecerse.
///
/// La huella sale del TAMAÑO con el que se dibuja, no de una constante: si una
/// pieza es más ancha que su borde (el vano y sus jambas) tapa las columnas de al
/// lado, y tiene que abrirse cuando estás bajo cualquiera de ellas.
fn huella_que_tapa(tx: i64, tz: i64, cara: Cara, size: Vec2) -> crate::layers::Overhead {
    let filas = (size.y / TILE_M).ceil().max(1.0) as i64;
    let cols = (size.x / TILE_M).round().max(1.0) as i64;
    let huella = match cara {
        // Horizontal: el sprite se centra en la mitad del borde y crece al norte.
        Cara::Tabique | Cara::Fondo | Cara::Cercano => {
            let x0 = tx - (cols - 1) / 2;
            crate::layers::Overhead::pared((x0, tz - filas), (cols, filas))
        }
        // Lateral: sólo la columna de ADENTRO. El sprite se centra en la línea del
        // borde, así que roza las dos celdas, pero marcar las dos hacía que
        // caminar por la vereda de una casa la volviera translúcida desde afuera —
        // un muro se rodea, no se cruza (ver `Overhead::pared`).
        Cara::Lateral { este, .. } => {
            let col = if este { tx - 1 } else { tx };
            crate::layers::Overhead::pared((col, tz + 1 - filas), (1, filas))
        }
    };
    // Una cara exterior se apila sobre el tumbado, así que adentro tiene que
    // borrarse más que él o la franja de la entrada queda turbia.
    // Parte de una casa: al entrar se abre con el resto de su recinto.
    let huella = huella.con_pie(tz);
    if cara.es_exterior() {
        huella.exterior()
    } else {
        huella
    }
}

/// Posición y orden de dibujo del sprite de una pared. El pie del sprite se
/// apoya en el borde y crece hacia arriba en pantalla; el Y-sort usa la fila del
/// pie, así el jugador pasa por delante o por detrás como corresponde.
///
/// **La cara decide la BANDA** (8 ago 2026). Un tabique Y-sortea con el resto del
/// mundo; una cara exterior va a [`crate::layers::fachada`], por encima de los
/// techos. Sin eso, el tumbado de la propia casa —que se planta en
/// [`crate::layers::OVERHEAD`], plano, y cubre justo las celdas por las que el
/// sprite de la pared sur crece hacia el norte— la borraba entera: desde la
/// vereda no había casa, había una losa. Y no se pierde nada por subirla: el
/// sprite **nunca** crece hacia el sur, así que no puede taparle la cara a nadie
/// que esté parado en la calle, delante de él.
pub(crate) fn wall_transform(tx: i64, tz: i64, axis: WallAxis, size: Vec2, cara: Cara) -> Transform {
    let (cx, foot_z) = match axis {
        // Horizontal: pie sobre la propia línea z = tz.
        WallAxis::H => (tx as f32 + 0.5, tz as f32),
        // Vertical: ocupa la celda z ∈ [tz, tz+1]; su pie es el borde sur.
        WallAxis::V => (tx as f32, (tz + 1) as f32),
    };
    let r = render_xy(cx * TILE_M, foot_z * TILE_M);
    let pie = foot_z * TILE_M;
    // Las dos bandas Y-sortean con el mismo span: dos fachadas que se solapan se
    // ordenan entre sí igual que dos paredes.
    let z = if cara.es_exterior() { crate::layers::fachada(pie) } else { sort_z(pie) };
    Transform::from_xyz(r.x, r.y + size.y * 0.5, z)
}

#[allow(clippy::too_many_arguments)]
fn stream_walls(
    mut commands: Commands,
    time: Res<Time>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    art: Res<ArtLibrary>,
    built: Res<BuiltWalls>,
    player: Query<&Transform, With<Player>>,
    live: Query<(Entity, &WallSprite)>,
    mut acc: Local<f32>,
    mut last_rev: Local<u64>,
) {
    if surface_region_active(&region).is_none() {
        for (e, _) in &live {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    // Refresco periódico; colocar o quitar una pared se ve al instante.
    *acc += time.delta_secs();
    if *acc < WALL_STREAM_PERIOD && *last_rev == built.rev {
        return;
    }
    *acc = 0.0;
    *last_rev = built.rev;
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let z = tiles.z;
    let ptx = (player_tf.translation.x / TILE_M).floor() as i64;
    let ptz = (-player_tf.translation.y / TILE_M).floor() as i64;

    // Paredes deseadas (mismo cuerpo/capa, cerca) con la cara que les toca.
    let has_floor = |cx: i64, cz: i64| is_build_floor(tiles.tile_at(body, cx, cz));
    let want: Vec<(&PlacedWall, Cara)> = built
        .list
        .iter()
        .filter(|w| {
            w.body == body as u8
                && w.z == z
                && (w.tx - ptx).abs() <= WALL_STREAM_RADIUS
                && (w.tz - ptz).abs() <= WALL_STREAM_RADIUS
        })
        .map(|w| (w, cara_de_borde(&has_floor, w.tx, w.tz, w.axis)))
        .collect();

    // Despawn de las que ya no aplican (lejos, otra capa, o cambió su cara/pieza).
    let mut alive: Vec<(i64, i64, WallAxis)> = Vec::new();
    for (e, s) in &live {
        let keep = s.body == body
            && s.z == z
            && want.iter().any(|(w, cara)| {
                w.tx == s.tx && w.tz == s.tz && w.axis == s.axis && *cara == s.cara && w.piece == s.piece
            });
        if keep {
            alive.push((s.tx, s.tz, s.axis));
        } else {
            commands.entity(e).despawn_recursive();
        }
    }
    // Spawn de las que faltan.
    for (w, cara) in want {
        if alive.contains(&(w.tx, w.tz, w.axis)) {
            continue;
        }
        let def = w.def();
        // La medida y el sprite salen de LA MISMA función que usa el fantasma
        // (`vista_de_pieza`): lo que viste antes de aceptar es lo que queda.
        let (sprite_cara, size) = vista_de_pieza(&art, def, cara);
        // La puerta de FRENTE arranca cerrada y la anima `animate_wall_doors`; la
        // lateral no se anima porque no hay pixel girado que animar — es el vano.
        let puerta_animada = def.kind == WallKind::Door && !cara.es_lateral();
        // Un vano SIN arte asignado no puede caer al cuadrado de color: se
        // dibujaría igual que una pared. Se deja el quad del padre transparente y
        // el marco lo ponen tres tiras hijas (`marco_de_vano`).
        let vano_sin_arte = def.kind == WallKind::Doorway && !art.tile_has_art(def.art);
        let sprite = if vano_sin_arte {
            Sprite::from_color(Color::NONE, size)
        } else if puerta_animada {
            wall_sprite(&art, def, &pose_de_puerta(cara, "closed"), size)
        } else {
            sprite_cara
        };
        let mut ent = commands.spawn((
            WallSprite {
                tx: w.tx,
                tz: w.tz,
                axis: w.axis,
                z,
                body,
                cara,
                piece: w.piece,
            },
            sprite,
            wall_transform(w.tx, w.tz, w.axis, size, cara),
            // **La pared que te tapa se transparenta, como un techo.**
            //
            // En cenital 3/4 el muro CERCANO de una habitación —el del sur, el del
            // este— se dibuja hacia arriba en pantalla, o sea ENCIMA del cuarto que
            // rodea. Con paredes de 2.4 tiles eso enterraba el interior entero: se
            // levantaba un cuarto de 3×3 y lo que se veía era un bloque de tablas.
            // Era el "el exterior aparece adelante y el interior atrás" que se
            // reporta, y no es un problema de recorte del arte: es que nada le
            // decía a la pared que se quitara.
            //
            // El mecanismo ya existía para los techos (`layers::Overhead`), y es el
            // mismo: cubrís esta huella, me desvanezco. Que sea el mismo para las
            // dos cosas importa — un cuarto con techo y paredes tiene que abrirse
            // de una sola manera, no de dos.
            huella_que_tapa(w.tx, w.tz, cara, size),
        ));
        if vano_sin_arte {
            marco_de_vano(&mut ent, size, fallback_color(def));
        }
        if puerta_animada {
            let (cx, cz) = edge_center(w.tx, w.tz, w.axis);
            ent.insert(WallDoor { cx, cz, art: def.art, open: false, t: 0.0, size, cara });
        }
        // Un muro con farol/ventana encendida ilumina si el slot lo declara.
        crate::light2d::attach(&mut ent, &art, &[def.art], size);
    }
}

/// Puertas de borde: cerca del jugador → pose `open`, lejos → `closed`. Sólo
/// toca el sprite cuando el estado cambia (cero costo en reposo).
fn animate_wall_doors(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<ArtLibrary>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
    player: Query<&Transform, With<Player>>,
    mut doors: Query<(Entity, &mut WallDoor, &mut Sprite)>,
) {
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let dt = time.delta_secs();
    let ptx = player_tf.translation.x / TILE_M;
    let ptz = -player_tf.translation.y / TILE_M;
    for (ent, mut door, mut sprite) in &mut doors {
        // Misma cadena de estados que las puertas de tile (`building`): reposo →
        // transición → reposo, y la transición corre entera.
        if door.t > 0.0 {
            door.t -= dt;
            if door.t <= 0.0 {
                door.t = 0.0;
                let rest = if door.open { "open" } else { "closed" };
                let size = door.size;
                crate::building::set_door_pose(
                    &mut commands,
                    ent,
                    &art,
                    door.art,
                    &pose_de_puerta(door.cara, rest),
                    &mut sprite,
                    size,
                );
            }
            continue;
        }
        let (dx, dz) = (ptx - door.cx, ptz - door.cz);
        let near = (dx * dx + dz * dz).sqrt() < DOOR_OPEN_DIST;
        if near == door.open {
            continue;
        }
        door.open = near;
        sfx.send(crate::audio::PlaySfxAt(
            if near { crate::audio::Sfx::DoorOpen } else { crate::audio::Sfx::DoorClose },
            render_xy(door.cx * TILE_M, door.cz * TILE_M),
        ));
        let transicion = if near { "opening" } else { "closing" };
        let (poses, secs) = match crate::building::door_transition(
            &art,
            door.art,
            &pose_de_puerta(door.cara, transicion),
        ) {
            // Hay transición recortada: se reproduce ésa y nada más.
            Some((p, s)) => ([p; 4], s),
            // Sin ella, salto directo al reposo por su propia cadena de cara.
            None => (pose_de_puerta(door.cara, if near { "open" } else { "closed" }), 0.0),
        };
        door.t = secs;
        let size = door.size;
        crate::building::set_door_pose(&mut commands, ent, &art, door.art, &poses, &mut sprite, size);
    }
}

// --- Persistencia (mirror de structures.json) --------------------------------

/// Formato en disco: la pieza va por **id**, no por índice de catálogo.
#[derive(Serialize, Deserialize, Default)]
struct WallsSave {
    walls: Vec<(u8, i32, i64, i64, u8, String)>,
}

fn walls_path() -> std::path::PathBuf {
    persist_dir().join("walls.json")
}

fn load_walls_on_startup(mut built: ResMut<BuiltWalls>) {
    let Ok(raw) = std::fs::read_to_string(walls_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<WallsSave>(&raw) else {
        return;
    };
    for (body, z, tx, tz, axis, piece) in save.walls {
        // Pieza desconocida (catálogo cambiado) → se descarta esa pared, no se
        // rompe la partida.
        let Some(piece) = edge_piece_index(&piece) else {
            continue;
        };
        // Por `add`, no por `list.push`: es lo único que deja el índice de bordes
        // al día. Una partida cargada por el otro camino se dibujaría entera y no
        // frenaría a nadie.
        built.add(PlacedWall {
            body,
            z,
            tx,
            tz,
            axis: WallAxis::from_u8(axis),
            piece,
        });
    }
    // Cargar no es construir: sin esto la partida recién abierta se reescribiría
    // sola a los 20 s por el `dirty` que deja `add`.
    built.dirty = false;
}

fn debounced_walls_save(time: Res<Time>, mut built: ResMut<BuiltWalls>) {
    let paused = is_app_paused();
    let flush_now = paused && !built.was_paused && built.dirty;
    built.was_paused = paused;
    if !built.dirty {
        return;
    }
    built.debounce += time.delta_secs();
    if !flush_now && built.debounce < 20.0 {
        return;
    }
    built.debounce = 0.0;
    built.dirty = false;
    let save = WallsSave {
        walls: built
            .list
            .iter()
            .map(|w| (w.body, w.z, w.tx, w.tz, w.axis.as_u8(), w.def().id.to_string()))
            .collect(),
    };
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(walls_path(), json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall_at(tx: i64, tz: i64, axis: WallAxis) -> PlacedWall {
        PlacedWall { body: 0, z: 0, tx, tz, axis, piece: 0 }
    }

    /// Las cinco caras que puede tomar un borde. Todo test que barra caras usa
    /// ésta: cuando se agregue la sexta, se agrega en un solo lugar.
    const TODAS_LAS_CARAS: [Cara; 6] = [
        Cara::Tabique,
        Cara::Fondo,
        Cara::Cercano,
        Cara::Lateral { este: false, interior: false },
        Cara::Lateral { este: true, interior: false },
        Cara::Lateral { este: false, interior: true },
    ];

    /// **El índice y la lista no pueden discrepar nunca.**
    ///
    /// `remove` usa `swap_remove`: mueve la ÚLTIMA pared al hueco que dejó la que
    /// se fue. Si no se reapunta su entrada del índice, esa pared queda dibujada
    /// pero invisible para la colisión — o sea, una pared que se atraviesa
    /// caminando, y encima sólo la que estaba al final de la lista. Es la clase de
    /// bug que aparece recién cuando el jugador ya tiene un refugio grande y
    /// demuele una pieza cualquiera.
    #[test]
    fn demoler_una_pared_no_desindexa_a_las_demas() {
        let body = CelestialBody::Earth;
        let mut w = BuiltWalls::default();
        for i in 0..5 {
            w.add(wall_at(i, 0, WallAxis::H));
        }
        // Se quita la PRIMERA: la última pasa a ocupar su lugar en el `Vec`.
        assert!(w.remove(body, 0, 0, 0, WallAxis::H).is_some());
        assert_eq!(w.list.len(), 4);
        for i in 1..5 {
            assert!(w.at(body, 0, i, 0, WallAxis::H).is_some(), "pared {i} perdida");
            assert!(w.blocks(body, 0, i, 0, WallAxis::H), "pared {i} dejo de frenar");
        }
        assert!(w.at(body, 0, 0, 0, WallAxis::H).is_none(), "la demolida sigue ahi");
        // Y todo lo que dice el índice existe de verdad en la lista.
        assert_eq!(w.index.len(), w.list.len());
    }

    /// Colocar sobre un borde ocupado REEMPLAZA (no apila ni duplica).
    #[test]
    fn colocar_sobre_un_borde_ocupado_reemplaza() {
        let body = CelestialBody::Earth;
        let mut w = BuiltWalls::default();
        w.add(wall_at(4, 4, WallAxis::V));
        let mut otra = wall_at(4, 4, WallAxis::V);
        otra.piece = 1;
        w.add(otra);
        assert_eq!(w.list.len(), 1);
        assert_eq!(w.at(body, 0, 4, 4, WallAxis::V).unwrap().piece, 1);
    }

    // ── Las cuatro caras de la caja ──────────────────────────────────────────

    /// **Un muro lateral no se dibuja como uno de frente.**
    ///
    /// Éste es EL bug: las cuatro caras salían del mismo cartel de un tile de
    /// ancho, así que levantar cuatro paredes daba cuatro fachadas mirando a
    /// cámara en vez de una caja. De canto un muro ocupa su ESPESOR a lo ancho y
    /// recorre el tile a lo alto — si esto vuelve a fallar, la choza vuelve a ser
    /// cuatro carteles.
    /// **Una pieza de obra ocupa su ranura y no la del vecino.**
    ///
    /// Éste es EL bug de la segunda captura: la pared se dibujaba con la regla de
    /// los OBJETOS (× `ESCALA_ARTE` = 1,5), así que un recorte de una celda salía
    /// de 1,5 m sobre un borde de 1 m. Ponías tres en fila y la segunda se montaba
    /// medio metro sobre la primera y la tercera sobre la segunda.
    ///
    /// Una pieza que embaldosa mide lo que mide su ranura. Si esto vuelve a
    /// fallar, las paredes vuelven a apilarse unas encima de otras.
    #[test]
    fn una_pieza_de_obra_no_se_monta_sobre_su_vecina() {
        // Recorte de `wall/wood`: 1×2 celdas del pack.
        let celdas = Vec2::new(1.0, 2.0);
        let dibujo = celdas * TILE_M;
        assert!(
            dibujo.x <= TILE_M + 1e-4,
            "una pared de una celda se dibuja de {} m sobre un borde de {TILE_M} m: \
             se solapa {} m con cada vecina",
            dibujo.x,
            dibujo.x - TILE_M
        );
        // La regla de los OBJETOS tampoco puede solaparse: se le corrigió la
        // escala (`ESCALA_ARTE`, que valía 1,5 sin que nadie lo midiera) y ahora
        // sale de dividir el alto del personaje por las celdas de su marco. Una
        // celda ya no puede pisar la ranura de al lado por ningún camino.
        assert!(celdas.x * crate::art2d::ESCALA_ARTE <= TILE_M + 1e-4);
    }

    /// **Una pared sigue siendo más alta que el jugador.**
    ///
    /// Encoger el ancho a la ranura no puede volver la casa de juguete: el alto
    /// sale de la proporción del dibujo (1×2 celdas → 1×2 tiles), no de recortarlo.
    /// El jugador mide 1,9 m (`scene::spawn_player`).
    #[test]
    fn una_pared_es_mas_alta_que_el_jugador() {
        const ALTO_JUGADOR: f32 = 1.9;
        let dibujo = Vec2::new(1.0, 2.0) * TILE_M;
        assert!(
            dibujo.y > ALTO_JUGADOR,
            "la pared ({}) es más baja que el jugador ({ALTO_JUGADOR}): se lee de juguete",
            dibujo.y
        );
        // Proporción exacta del pixel: nada de estirar para llenar.
        assert!((dibujo.y / dibujo.x - 2.0).abs() < 1e-5, "el dibujo sale deformado");
    }

    /// **El fantasma y la pared miden lo mismo, cara por cara.**
    ///
    /// El fantasma dibujaba siempre `front` a su tamaño y `walls` dibujaba la cara
    /// que le tocara con otra geometría: veías el marco entero de la puerta,
    /// aceptabas, y salía otra cosa. Ahora los dos pasan por `vista_de_pieza`, y
    /// esto lo comprueba pieza por pieza — incluida la cara que el pack NO trae
    /// recortada, que es donde se colaba el estirón.
    #[test]
    fn el_fantasma_mide_lo_mismo_que_la_pared() {
        let pared = edge_piece(edge_piece_index("edge_wall_wood").unwrap()).unwrap();
        let vano = edge_piece(edge_piece_index("edge_doorway_wood").unwrap()).unwrap();
        // El pack: la pared es de 1×2 celdas, el vano de 2×2 (trae sus dos jambas).
        let art_pared = crate::art2d::ArtLibrary::test_pose(pared.art, "front", (1, 2));
        let art_vano = crate::art2d::ArtLibrary::test_pose(vano.art, "front", (2, 2));
        for cara in TODAS_LAS_CARAS {
            // Sólo hay pose `front` recortada: TODAS las caras tienen que caer en
            // ella por la cadena, y medir lo suyo — no el respaldo de la constante.
            let m = medida_de_pieza(&art_pared, pared, cara);
            assert_eq!(m, Vec2::new(TILE_M, 2.0 * TILE_M), "{cara:?}: la pared cambió de talle");
            assert_eq!(vista_de_pieza(&art_pared, pared, cara).1, m, "{cara:?}: fantasma ≠ pared");

            let v = medida_de_pieza(&art_vano, vano, cara);
            assert_eq!(v, Vec2::new(2.0 * TILE_M, 2.0 * TILE_M), "{cara:?}: el vano cambió de talle");
            assert_eq!(vista_de_pieza(&art_vano, vano, cara).1, v, "{cara:?}: fantasma ≠ vano");
        }
    }

    /// **Ninguna cara recorta el sprite.**
    ///
    /// Las cuatro caras piden una POSE ENTERA (`front`/`back`/`left`/`right`), que
    /// es lo que el usuario recorta en el editor. Sacar "material" de la columna
    /// central del dibujo funcionaba de casualidad en un paño de tablones y era un
    /// disparate en cualquier pieza con hueco: la columna central de un VANO es el
    /// hueco, o sea nada — de ahí salía *"un palo con una puntita blanca"*.
    #[test]
    fn cada_cara_pide_su_pose_entera() {
        for cara in TODAS_LAS_CARAS {
            let cadena = pose_chain(cara);
            assert_eq!(cadena[0], cara.pose());
            assert_eq!(&cadena[1..], &["front", "center"], "la cadena de respaldo cambió");
        }
        // Los dos laterales son poses distintas (el pack las trae recortadas), no
        // la misma imagen con un tinte encima.
        assert_ne!(
            Cara::Lateral { este: false, interior: false }.pose(),
            Cara::Lateral { este: true, interior: false }.pose()
        );
        // Y la espalda cae a `front` mientras no tenga recorte propio: agregar la
        // pose no puede haber cambiado cómo se ve una casa sin arte de fachada.
        assert_eq!(&pose_chain(Cara::Fondo)[1..], &["front", "center"]);
    }

    /// **Un tabique no es una fachada.** Piso de los dos lados = pared de adentro:
    /// ni se dibuja con la elevación de exteriores ni sube a la banda de fachadas.
    /// Es la distinción que `Fondo` se comía hasta el 8 ago 2026.
    #[test]
    fn un_tabique_no_es_una_fachada() {
        // E-O con piso de los dos lados: tabique.
        assert_eq!(Cara::de(WallAxis::H, true, true), Cara::Tabique);
        assert!(!Cara::Tabique.es_exterior());
        // N-S con piso de los dos lados: lateral, pero interior.
        let v = Cara::de(WallAxis::V, true, true);
        assert_eq!(v, Cara::Lateral { este: false, interior: true });
        assert!(!v.es_exterior(), "un tabique norte-sur no da a la calle");
        // Y las tres caras del borde del edificio SÍ son exteriores.
        assert!(Cara::de(WallAxis::H, true, false).es_exterior(), "el frente");
        assert!(Cara::de(WallAxis::H, false, true).es_exterior(), "la espalda");
        assert!(Cara::de(WallAxis::V, true, false).es_exterior(), "el costado");
        // Una pared que se quedó sin piso de ningún lado (`integrity` puede sacar
        // las dos losas en el mismo frame) se trata como tabique: lo de antes.
        assert_eq!(Cara::de(WallAxis::H, false, false), Cara::Tabique);
    }

    /// **La cara exterior va por delante de su propio tumbado.**
    ///
    /// EL bug que descubrió este trabajo: el tumbado se planta plano en
    /// `layers::OVERHEAD` y cubre justo las celdas por las que el sprite de la
    /// pared sur crece hacia el norte, así que **una casa techada, mirada desde la
    /// vereda, era una losa de tumbado y nada más**. Pasaba con `wall/wood` de 2 m
    /// igual que con una fachada de 3.
    #[test]
    fn la_cara_exterior_va_por_delante_del_tumbado() {
        let size = Vec2::new(TILE_M, 3.0 * TILE_M);
        for cara in TODAS_LAS_CARAS {
            let z = wall_transform(10, 10, WallAxis::H, size, cara).translation.z;
            if cara.es_exterior() {
                assert!(z > crate::layers::OVERHEAD, "{cara:?}: el techo le tapa la cara");
            } else {
                assert!(z < crate::layers::OVERHEAD, "{cara:?}: un tabique se subió a la banda de fachadas");
            }
        }
        // Y dos fachadas vecinas se siguen ordenando entre sí por su fila.
        let sur = wall_transform(10, 20, WallAxis::H, size, Cara::Cercano).translation.z;
        let norte = wall_transform(10, 10, WallAxis::H, size, Cara::Cercano).translation.z;
        assert!(sur > norte, "la casa de adelante quedó detrás de la de atrás");
    }

    /// **La medida sale de la MISMA pose que dibuja el sprite.**
    ///
    /// `celdas_de(key, Some(pose))` no devuelve `None` cuando la pose falta: cae
    /// al rect del slot. Sin el `has_pose` que lo guarda, la cadena de la medida
    /// resolvía siempre en la primera vuelta y la del sprite seguía buscando —
    /// o sea el pixel de una cara estirado al talle de otra. Con `wall/wood` no se
    /// veía (el rect del slot ES el de `front`); con una fachada, sí.
    #[test]
    fn la_fachada_mide_lo_que_mide_su_pose() {
        let pared = edge_piece(edge_piece_index("edge_wall_wood").unwrap()).unwrap();
        // Frente de 1×3 celdas (elevación), interior de 1×2 (Room Builder).
        let art = crate::art2d::ArtLibrary::test_poses(pared.art, &[("front", (1, 2)), ("back", (1, 3))]);
        assert_eq!(medida_de_pieza(&art, pared, Cara::Cercano), Vec2::new(TILE_M, 3.0 * TILE_M));
        assert_eq!(medida_de_pieza(&art, pared, Cara::Tabique), Vec2::new(TILE_M, 2.0 * TILE_M));
        // Y el fantasma promete lo mismo que la pared, cara por cara.
        for cara in TODAS_LAS_CARAS {
            assert_eq!(
                vista_de_pieza(&art, pared, cara).1,
                medida_de_pieza(&art, pared, cara),
                "{cara:?}: fantasma ≠ pared"
            );
        }
        // Una cara sin recorte propio mide lo de `front`, no lo del rect del slot.
        assert_eq!(medida_de_pieza(&art, pared, Cara::Fondo), Vec2::new(TILE_M, 2.0 * TILE_M));
    }

    /// **Una puerta de fachada no se cae a la hoja de interior.**
    ///
    /// Hay temas del pack que traen la hoja cerrada y no la abierta. Que se quede
    /// dibujada cerrada es raro; que aparezca el paño de pared interior en medio
    /// del frente de la casa es el defecto que este trabajo vino a sacar. (Se
    /// cruza igual: `WallKind::Door` no frena, mire como mire.)
    #[test]
    fn una_puerta_de_fachada_no_se_cae_a_la_de_interior() {
        for estado in ["closed", "open", "opening", "closing"] {
            let frente = pose_de_puerta(Cara::Cercano, estado);
            assert!(frente[0].starts_with("back_"), "{estado}: el frente no pide su hoja");
            if estado != "closed" {
                assert!(
                    !frente.contains(&"front") && !frente.contains(&"center"),
                    "{estado}: la puerta del frente se cae al paño de interior"
                );
            }
            // El tabique sigue con la puerta de siempre.
            let dentro = pose_de_puerta(Cara::Tabique, estado);
            assert!(!dentro[0].starts_with("back_"), "{estado}: un tabique pidió hoja de fachada");
        }
        // La espalda usa la de interior: el pack no trae hoja de atrás.
        assert_eq!(pose_de_puerta(Cara::Fondo, "open")[0], "open");
    }

    /// **La caja de cuatro paredes tiene cuatro caras distintas.** Se arma el
    /// anillo de una habitación de 3×3 y se pregunta, borde por borde, qué cara le
    /// toca: dos laterales enfrentados, un fondo y un cercano.
    #[test]
    fn una_habitacion_cerrada_da_las_cuatro_caras() {
        // Habitación en las celdas x ∈ [10,12], z ∈ [10,12].
        let dentro = |cx: i64, cz: i64| (10..=12).contains(&cx) && (10..=12).contains(&cz);
        let cara_de = |tx: i64, tz: i64, axis: WallAxis| {
            let [a, b] = edge_cells(tx, tz, axis);
            Cara::de(axis, dentro(a.0, a.1), dentro(b.0, b.1))
        };
        // Borde norte (z = 10): la habitación queda al SUR → la ESPALDA.
        assert_eq!(cara_de(11, 10, WallAxis::H), Cara::Fondo);
        // Borde sur (z = 13): la habitación queda al NORTE → el FRENTE.
        assert_eq!(cara_de(11, 13, WallAxis::H), Cara::Cercano);
        // Borde oeste (x = 10): la habitación queda al ESTE → su muro oeste.
        assert_eq!(cara_de(10, 11, WallAxis::V), Cara::Lateral { este: false, interior: false });
        // Borde este (x = 13): la habitación queda al OESTE → su muro este.
        assert_eq!(cara_de(13, 11, WallAxis::V), Cara::Lateral { este: true, interior: false });
        // Y las cuatro dan a la calle: una habitación suelta es todo cáscara.
        for (tx, tz, axis) in [
            (11, 10, WallAxis::H),
            (11, 13, WallAxis::H),
            (10, 11, WallAxis::V),
            (13, 11, WallAxis::V),
        ] {
            assert!(cara_de(tx, tz, axis).es_exterior(), "{tx},{tz},{axis:?}");
        }
        // Pero el tabique que la parte al medio, no.
        assert!(!cara_de(11, 11, WallAxis::H).es_exterior());
    }

    /// La huella de un lateral marca la columna de ADENTRO, no las dos que separa:
    /// con las dos, caminar por la vereda de una casa la volvía translúcida desde
    /// afuera. Un muro se rodea; un techo se cruza.
    #[test]
    fn el_lateral_solo_tapa_hacia_adentro() {
        let pared = Vec2::new(TILE_M, 2.0 * TILE_M);
        // Muro OESTE de la habitación (el cuarto está al este, en la columna tx).
        let o = huella_que_tapa(10, 11, Cara::Lateral { este: false, interior: false }, pared);
        assert_eq!(o.min.0, 10, "tapa la columna de adentro");
        assert_eq!(o.size.0, 1, "y sólo ésa");
        // Muro ESTE: el cuarto está al oeste, o sea en `tx - 1`.
        let e = huella_que_tapa(13, 11, Cara::Lateral { este: true, interior: false }, pared);
        assert_eq!(e.min.0, 12);
        assert_eq!(e.margen, 0, "una pared no se abre antes de cruzarla");
    }

    /// **Lo que tapa una pieza sale de su TAMAÑO, no de una constante.**
    ///
    /// Un vano trae sus dos jambas y mide dos tiles de ancho: si estás bajo la
    /// jamba de la izquierda y la huella sólo declara la columna del borde, la
    /// pieza no se desvanece y te quedás tapado detrás de ella.
    #[test]
    fn una_pieza_ancha_tapa_las_columnas_que_ocupa() {
        let vano = Vec2::new(2.0 * TILE_M, 2.0 * TILE_M);
        let h = huella_que_tapa(10, 7, Cara::Tabique, vano);
        assert_eq!(h.size.0, 2, "el vano tapa las dos columnas que ocupa");
        assert!(h.min.0 <= 10 && h.min.0 + h.size.0 > 10, "y una de ellas es su borde");
        // La pared de una celda sigue tapando una sola columna.
        let pared = huella_que_tapa(10, 7, Cara::Tabique, Vec2::new(TILE_M, 2.0 * TILE_M));
        assert_eq!((pared.min.0, pared.size.0), (10, 1));
    }

    /// **Una fachada alta tapa las filas que de verdad tapa.** El sprite de una
    /// elevación mide 3 m y crece hacia el norte: si la huella declarara las 2
    /// filas de una pared de interior, quedarías tapado en la tercera —
    /// justamente la fila del interior donde uno entra.
    #[test]
    fn la_huella_de_una_fachada_crece_con_su_alto() {
        let fachada = Vec2::new(TILE_M, 3.0 * TILE_M);
        let f = huella_que_tapa(10, 20, Cara::Cercano, fachada);
        assert_eq!(f.size.1, 3, "la fachada declara sus tres filas");
        assert_eq!(f.min.1, 17, "y son las tres al NORTE del borde, o sea adentro");
        assert_eq!(f.margen, 0, "una pared se rodea; no se abre antes de cruzarla");
        // Y se borra más que un tumbado, porque va apilada encima de él.
        assert_eq!(f.alpha_dentro, crate::layers::FACHADA_ALPHA);
        assert_eq!(
            huella_que_tapa(10, 20, Cara::Tabique, fachada).alpha_dentro,
            crate::layers::INSIDE_ALPHA
        );
    }

    /// El anillo de paredes de una planta es lo que aguanta la losa de arriba:
    /// `any_edge_of_cell` tiene que ver las CUATRO aristas de la celda.
    #[test]
    fn una_celda_ve_sus_cuatro_aristas() {
        let body = CelestialBody::Earth;
        for (tx, tz, axis) in [
            (7, 7, WallAxis::H), // norte
            (7, 8, WallAxis::H), // sur
            (7, 7, WallAxis::V), // oeste
            (8, 7, WallAxis::V), // este
        ] {
            let mut w = BuiltWalls::default();
            w.add(wall_at(tx, tz, axis));
            assert!(w.any_edge_of_cell(body, 0, 7, 7), "{tx},{tz},{axis:?}");
            // Y no ve la de la celda de al lado.
            assert!(!w.any_edge_of_cell(body, 0, 20, 20));
        }
    }
}
