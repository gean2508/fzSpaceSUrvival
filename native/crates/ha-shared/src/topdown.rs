//! Top-Down 3/4 world model (LimeZu Modern) — successor to `flat2d.rs`.
//!
//! Migration to a Stardew/top-down view. The walkable ground is the XZ plane
//! (the same plane the solar system lives on): `body.x` = East(+)/West(−),
//! `body.z` = North(−)/South(+), `body.y` = height (0 on foot). Screen render
//! is `(x, -z)` — identical to `orbital2d.rs`, so surface and space share a
//! projection and launching is a zoom-out, not a coordinate change.
//!
//! Depth is a discrete **Z-Level** (`z_level: i32`): 0 = surface, negative =
//! underground mine floors, each its own deterministic top-down grid. Space is
//! NOT a z-level — it is the orbital mode. Ver `biblia.md` §4.

use crate::celestial_bodies::{body_atmo_top, body_terrain_seed, CelestialBody};
use crate::constants::{FRICTION, SPRINT_MULT, STAMINA_SPRINT_DRAIN};
use crate::input::InputState;
use crate::movement::MovementBody;
use crate::types::{MOV_IDLE, MOV_RUNNING, MOV_SPRINTING, MOV_SWIMMING};

pub const TILE_M: f32 = 1.0;
pub const PLAYER_RADIUS: f32 = 0.34;
/// **El fondo del mundo**: por debajo de este nivel todo es [`Tile::Bedrock`], y
/// el lecho de roca no se pica ni se cava. Es lo único que frena a quien baja.
///
/// **Hay UNA capa de subsuelo** (2 oct 2026, decisión del usuario: «hay que priorizar el
/// rendimiento»): la de las cuevas, con biomas por región (`subsuelo.rs`). Antes eran 16
/// niveles de roca maciza que nadie visitaba y cada uno costaba chunks, luz y memoria en el
/// Redmi.
///
/// **Es el índice más bajo que existe, no la profundidad del lecho.** Desde que
/// una meseta es tierra APILADA sobre el piso base ([`tierra_de_meseta`]), el que
/// cava en la cordillera arranca más arriba y necesita más índices para llegar al
/// mismo fondo: por eso hay [`TIERRA_DE_MESETA_MAX`] niveles de margen. La
/// profundidad del lecho, medida desde el piso base, es [`LECHO_DE_ROCA`].
pub const MIN_Z_LEVEL: i32 = LECHO_DE_ROCA - TIERRA_DE_MESETA_MAX;

/// **Profundidad del lecho de roca, MEDIDA DESDE EL PISO BASE.** Por debajo de
/// esto todo es [`Tile::Bedrock`] y no se pica: es el límite del mundo.
pub const LECHO_DE_ROCA: i32 = -2;

/// **Cuánta tierra de meseta se puede llegar a apilar sobre el piso base.**
///
/// La escalera del relieve llega a 12 terrazas, pero el 82 % de la tierra vive
/// entre la 0 y la 3 (`relief::histograma_de_terrazas`): pedirle al que cava en la
/// cumbre doce niveles de tierra antes de la primera piedra sería un peaje de
/// varios minutos para llegar a lo mismo. Con 6 la cordillera se siente honda y el
/// mundo sigue cabiendo en índices razonables.
pub const TIERRA_DE_MESETA_MAX: i32 = 6;

/// **LA MESETA ES TIERRA SOBRE EL PISO BASE.** Cuántos niveles de tierra hay que
/// atravesar en esta columna antes de llegar al piso base — 0 en la llanura.
///
/// Regla del usuario, textual: *"solo al picar el piso base llegamos a las cuevas,
/// no el piso de las mesetas, ya que en teoría es como que hay tierra sobre el
/// piso base; para llegar a la cueva cavás toda esa tierra"*. Es la misma lectura
/// que su dibujo de las tortitas apiladas: la terraza 2 no es otro suelo, es el
/// suelo de siempre con dos losas de tierra encima.
///
/// Antes esto no existía y el subsuelo era plano: cavabas un tile en la cumbre de
/// una montaña y estabas en la cueva, al mismo nivel que si hubieras cavado en la
/// playa. El relieve terminaba en la superficie y por debajo el mundo se olvidaba
/// de que había una montaña.
pub fn tierra_de_meseta(body: CelestialBody, tx: i64, tz: i64) -> i32 {
    (crate::relief::relief_level(body, tx, tz) as i32).clamp(0, TIERRA_DE_MESETA_MAX)
}

// ---------------------------------------------------------------------------
// Tiles
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Tile {
    // Surface (z=0)
    Grass,
    Dirt,
    Sand,
    /// Líquido de POCO FONDO: no corta el paso, **se nada** (ver [`Tile::swim`]).
    /// Es el nivel −1 del relieve: la orilla, los ríos, los lagos de pescar y la
    /// pileta del patio. El color sale del cuerpo (`relief::liquid_color`), así que
    /// el mismo tile es agua en la Tierra, metano en Titán y lava en Ío.
    Water,
    /// Líquido HONDO (nivel −2 y más abajo): bloquea. Es lo que hace que el océano
    /// siga siendo un límite y que cruzar de continente pida puente, vuelo o barco.
    WaterDeep,
    Regolith,
    /// Hielo: superficie helada CAMINABLE — casquetes polares, cumbres nevadas
    /// y los mundos de hielo (Europa). Resbala a la vista, no en el paso.
    Ice,
    /// Basalto: llanura oscura de roca volcánica. CAMINABLE — es suelo, no
    /// muro. Es lo que dibuja los mares de la Luna y las planicies oscuras de
    /// Marte, que son justo los rasgos que hacen reconocible a cada cuerpo
    /// desde el espacio.
    Basalt,
    RockRed,
    /// **Cara de acantilado**: el borde de una terraza contra la de abajo. Sólida
    /// y alta — no se trepa, se rodea o se sube por una [`Tile::Slope`]. Es lo que
    /// convierte una cordillera en una barrera de verdad. Ver `crate::relief`.
    Cliff,
    /// **Rampa**: el mismo borde de terraza, pero abierto. Se sube caminando (y
    /// conduciendo: las carreteras siempre cruzan por acá).
    Slope,
    /// Cerca de parcela: corta el paso y se levanta del suelo, pero es delgada —
    /// el arte la dibuja como listones, no como muro.
    Fence,
    /// **Roca desnuda de superficie**: la cumbre y la ladera alta. CAMINABLE.
    ///
    /// Es distinto de [`Tile::Stone`], que es la roca MACIZA del subsuelo y
    /// bloquea. Confundirlas convertía cada montaña en una mole impasable de
    /// kilómetros —medido: 29 % del terreno alrededor del origen— y eso anula el
    /// sistema de terrazas entero: si la montaña ya es un muro, sobra el
    /// acantilado. Lo que corta el paso en altura es [`Tile::Cliff`]; la cumbre se
    /// pisa, se acampa en ella y se mira el mundo desde arriba.
    Rock,
    /// **Pavimento**: acera, vereda, losa de plaza, borde de pileta, patio.
    /// CAMINABLE.
    ///
    /// Existe porque hasta ahora la obra civil pintaba las aceras con
    /// [`Tile::Stone`], que es la ROCA del subsuelo y por tanto sólida: las
    /// veredas de todas las ciudades del juego eran muros infranqueables. Nadie lo
    /// notó porque a pie se camina por la calzada, pero cualquier cosa que
    /// intentara cruzar de la calle al portal chocaba con una pared invisible.
    Pavement,
    /// **Calzada**: el asfalto por el que se conduce. CAMINABLE.
    ///
    /// Las carreteras se pintaban con [`Tile::Basalt`] —la roca volcánica de los
    /// mares de la Luna— por no tener un tile propio. Funcionaba (es caminable)
    /// pero significaba que el juego no tenía forma de dibujar una calle: el
    /// basalto es gris plano, así que las ciudades eran una explanada del mismo
    /// color que el suelo lunar, sin textura ni línea de carril, y el asfalto que
    /// venía en el pack desde el primer día no se usaba en ninguna parte.
    ///
    /// Su arte (`floor/asfalto`) trae varias losas y el mundo elige una por celda,
    /// así que la calzada tiene grietas y tapas de alcantarilla en vez de repetir
    /// el mismo cuadrado. La línea pintada va aparte (`road/linea`), porque no es
    /// del suelo: es una red que sigue el eje de la calle.
    Asphalt,
    // Underground (z<0)
    Stone,
    /// **Tierra maciza del subsuelo**: lo que hay entre tu galería y el resto del
    /// mundo, y por tanto lo que llena la pantalla cuando estás abajo.
    ///
    /// Existe porque el subsuelo se rellenaba con [`Tile::Stone`] —gris de roca— y
    /// además ni se dibujaba: bajo tierra se pintaba la SUPERFICIE a plena luz
    /// (pasto, árboles, mariposas) con un agujero negro en el medio. Se leía como
    /// un mapa roto, no como estar debajo. Ahora el que baja ve tierra hasta donde
    /// le llega la vista, y lo único que no es tierra es lo que se excavó.
    ///
    /// Es sólida (no se camina: se pica) y PLANA (`upright() == false`): un campo
    /// de tierra de horizonte a horizonte dibujado como muros altos sería una
    /// selva de sprites y no se leería como el macizo que es.
    Subsoil,
    RockDark,
    Bedrock,
    CaveFloor,
    // Player-placed / shared
    WoodFloor,
    /// **Piso de piedra del jugador** — la losa que se CAMINA.
    ///
    /// Existe porque la pieza "Piso piedra" del menú colocaba [`Tile::StoneBrick`],
    /// que es sólido y `upright`: te dejaba encerrado sobre tu propio piso, te
    /// tapaba lo de atrás, se dibujaba con `wall/stone` y no se podía reemplazar
    /// (`place_tile` rechaza sobre sólido). El ladrillo se queda para los
    /// asentamientos generados y las partidas viejas; el piso es esto.
    StoneFloor,
    Wall,
    // Build pieces (construction system) — placed only, never generated
    // (except Door/WoodWall/WoodFloor, reused by generated settlements).
    WoodWall,
    StoneBrick,
    Door,
    /// Window: blocks walking like a wall, but art shows through (see `window/basic`).
    Window,
    /// **Un ESCALÓN de un tramo**, visto desde la planta baja. Se camina como
    /// cualquier piso: por sí solo no hace nada: no cambia de nivel, no lleva a
    /// ningún lado. Lo que sube es el TRAMO al que pertenece
    /// ([`crate::escaleras::Tramo`]) — cuatro de éstos en fila, y andarlos.
    ///
    /// Que el tile sea inerte es a propósito y es la lección del sistema viejo: si
    /// el poder de cambiar de planta vive en una celda, pisarla teletransporta.
    Step,
    /// **El mismo tramo visto desde la planta de arriba**: el hueco de la escalera.
    /// También se camina —es por donde se baja— y también es inerte.
    Stairwell,
    /// **Hueco: acá no hay materia, en NINGÚN nivel.**
    ///
    /// Es el tile del vacío del piso alto *y* el del pozo que cavaste en el
    /// suelo: la misma celda, la misma lectura y el mismo código. Se ve lo de
    /// abajo atenuado (`world2d::tile_render_color`) y el que se para encima se
    /// CAE un nivel (`world2d::fall_through_void`) — así el mundo es voxel: al
    /// sacar un bloque queda la cavidad, y la cavidad se atraviesa.
    Air,
}

impl Tile {
    /// Blocks walking (walls, deep liquid, solid rock — cave floors are walkable).
    /// Build walls block; wood floors and doors are walkable.
    ///
    /// **`Water` ya NO es sólido**: el bajío se nada (ver [`Tile::swim`]). Lo que
    /// bloquea es [`Tile::WaterDeep`]. Si estás leyendo esto porque algo dejó de
    /// frenar al jugador en el agua: el sitio a mirar es quién quería decir "no se
    /// camina" (sigue igual) y quién quería decir "no es suelo edificable" (ésos
    /// tuvieron que pasar a comprobar `!= Water` explícitamente).
    pub fn solid(self) -> bool {
        matches!(
            self,
            Tile::WaterDeep
                | Tile::Wall
                | Tile::Bedrock
                | Tile::Stone
                | Tile::Subsoil
                | Tile::RockDark
                | Tile::RockRed
                | Tile::Cliff
                | Tile::Fence
                | Tile::WoodWall
                | Tile::StoneBrick
                | Tile::Window
        )
    }

    /// Líquido de poco fondo: **se nada**. No corta el paso, pero el que lo cruza
    /// va más lento y con la pose de natación (`swim_*`) en vez de la de caminar.
    pub fn swim(self) -> bool {
        matches!(self, Tile::Water)
    }

    /// ¿Es suelo firme sobre el que se puede FUNDAR algo (una casa, una máquina,
    /// una parcela)? Distinto de `!solid()`: el bajío se camina pero no se
    /// construye a secas, y una rampa es un paso, no una parcela.
    ///
    /// Existe porque cuando `Water` dejó de ser sólido, todos los sitios que
    /// usaban `!solid()` para decir "acá se puede edificar" empezaron a plantar
    /// casas dentro del lago.
    pub fn buildable_ground(self) -> bool {
        !self.solid() && !self.swim() && !matches!(self, Tile::Slope | Tile::Air)
    }

    /// **¿Se puede CAVAR esta celda hacia abajo?** (voxel 2D — ver biblia §5.)
    ///
    /// Distinto de picar: picar saca un bloque que tenés al lado (una pared, una
    /// veta, un acantilado) y deja la celda abierta en TU nivel; cavar saca el
    /// suelo que pisás y deja el HUECO ([`Tile::Air`]), con la cavidad abierta en
    /// el nivel de abajo. Por eso lo que se cava es el piso —tierra, roca de
    /// superficie, suelo de cueva, tu propia losa— y no el líquido (se rellena,
    /// no se pica), ni el vacío (ya es un hueco), ni lo sólido (eso es picar).
    pub fn diggable_down(self) -> bool {
        !self.solid() && !self.swim() && !matches!(self, Tile::Air | Tile::Bedrock)
    }

    /// ¿Este material se levanta del suelo (ocluye lo que tiene detrás)?
    ///
    /// **No es lo mismo que [`Tile::solid`]**, y confundirlos es lo que hacía
    /// que el AGUA tapara al jugador: el agua bloquea el paso (es `solid`) pero
    /// es PLANA, así que debe dibujarse en la banda de suelo, nunca Y-sorted.
    /// `solid` decide si podés caminar; `upright` decide si te tapa.
    pub fn upright(self) -> bool {
        matches!(
            self,
            Tile::Wall
                | Tile::Bedrock
                | Tile::Stone
                | Tile::RockDark
                | Tile::RockRed
                // El acantilado y la cerca se LEVANTAN del suelo: te tapan lo que
                // tienen detrás, que es justamente cómo se lee un desnivel en 3/4.
                // El agua (bajío u hondo) NO: es plana, y confundir "bloquea" con
                // "es alto" es de donde salía el bug del agua tapando al jugador.
                | Tile::Cliff
                | Tile::Fence
                | Tile::WoodWall
                | Tile::StoneBrick
                | Tile::Window
                | Tile::Door
        )
    }

    /// Diggable (removable) → yields this item id, if any. Build pieces refund
    /// their material so deconstruction is just "dig the piece".
    pub fn dug_item(self) -> Option<&'static str> {
        match self {
            Tile::Grass | Tile::Dirt => Some("dirt"),
            Tile::Sand => Some("sand"),
            Tile::Regolith => Some("regolith"),
            Tile::Basalt => Some("stone"),
            Tile::Ice => Some("stone"),
            Tile::Stone | Tile::RockRed | Tile::Pavement | Tile::Rock => Some("stone"),
            // Picar el macizo del subsuelo devuelve TIERRA: es tierra. Es lo que
            // hace que excavar tu sótano te deje con qué rellenar el pozo de al
            // lado, sin pasar por la piedra.
            Tile::Subsoil => Some("dirt"),
            // Picar la calzada devuelve piedra: es hormigón, no madera.
            Tile::Asphalt => Some("stone"),
            Tile::RockDark => Some("stone"),
            // Picar un acantilado abre el paso: es la alternativa "a lo Minecraft"
            // a buscar la rampa, y cuesta el tiempo de picar toda la pared.
            Tile::Cliff => Some("stone"),
            Tile::Slope => Some("dirt"),
            Tile::Fence => Some("wood"),
            Tile::StoneFloor => Some("stone"),
            Tile::WoodFloor | Tile::Wall | Tile::WoodWall | Tile::Door => Some("wood"),
            Tile::Window | Tile::Step | Tile::Stairwell => Some("wood"),
            Tile::StoneBrick => Some("stone"),
            _ => None,
        }
    }

    /// **Slot de arte** de la celda: la clave lógica que el juego le pide a
    /// `ArtLibrary` (y que vos reskineás en el editor de assets).
    ///
    /// Vive acá, al lado del tile, y NO en el cliente: es la única forma de que
    /// el editor pueda auditar que toda pieza de construcción tiene su slot con
    /// nombre humano (`ha_assets::game_slots`). Si un tile estrena arte, la
    /// clave se agrega acá y el test de auditoría exige el slot correspondiente.
    ///
    /// `None` = el tile se dibuja SIEMPRE por color ([`Tile::color`]): el
    /// subsuelo, el aire, la roca del fondo de cueva. Y aunque devuelva `Some`,
    /// si el slot no tiene PNG asignado el juego cae al color — una puerta sin
    /// arte es un cuadrado café, nunca un hueco.
    pub fn art_key(self) -> Option<&'static str> {
        match self {
            Tile::Water => Some("tiles/water"),
            Tile::WaterDeep => Some("tiles/water_deep"),
            Tile::Grass => Some("tiles/grass"),
            Tile::Sand => Some("tiles/sand"),
            Tile::Regolith => Some("tiles/regolith"),
            Tile::Basalt => Some("tiles/basalt"),
            Tile::Ice => Some("tiles/ice"),
            Tile::Stone => Some("tiles/stone"),
            Tile::Dirt => Some("tiles/dirt"),
            // El macizo del subsuelo y el suelo de la galería tienen slot PROPIO
            // (no reusan el pasto ni la roca) porque son lo único que se ve
            // cuando estás abajo: si el subsuelo va a leerse como subsuelo, su
            // pixel tiene que poder elegirse aparte en HA Studio.
            Tile::Subsoil => Some("tiles/subsuelo"),
            Tile::CaveFloor => Some("floor/cueva"),
            // Relieve. La clave genérica es el fallback; `cliff_art_key` elige antes
            // la variante por bioma (`cliff/arena` en un desierto, `cliff/nieve` en la
            // cumbre) para que la misma pared se lea distinta según dónde esté.
            Tile::Cliff => Some("cliff/roca"),
            Tile::Slope => Some("slope/roca"),
            Tile::Fence => Some("fence/madera"),
            Tile::Pavement => Some("floor/vereda"),
            Tile::Asphalt => Some("floor/asfalto"),
            Tile::Rock => Some("tiles/roca"),
            // Piezas de construcción (autotile de Room_Builder). El muro legacy
            // (`Wall`) comparte el arte de madera.
            Tile::WoodFloor => Some("floor/wood"),
            Tile::StoneFloor => Some("floor/stone"),
            Tile::WoodWall | Tile::Wall => Some("wall/wood"),
            Tile::StoneBrick => Some("wall/stone"),
            Tile::Door => Some("door/wood"),
            Tile::Window => Some("window/basic"),
            // El tramo reusa el arte que ya tenías asignado en HA Studio: el
            // escalón visto desde abajo sube, y visto desde la planta de arriba
            // baja. Un solo tramo, dos lecturas — no dos escaleras.
            Tile::Step => Some("stairs/up"),
            Tile::Stairwell => Some("stairs/down"),
            _ => None,
        }
    }

    /// Flat sRGB colour (procedural fallback until LimeZu tilesets land).
    pub fn color(self) -> [f32; 3] {
        match self {
            Tile::Grass => [0.36, 0.62, 0.29],
            // **LA TIERRA Y LA ARENA SON LAS DEL PACK, no dos colores inventados.**
            //
            // Estos dos números decidían el aspecto del suelo y no salían de
            // ningún lado: `atlas-suelo.py` buscaba la losa del pack más parecida
            // y no encontraba NINGUNA a menos de 28 de distancia, así que la
            // playa y el pozo cavado se quedaban en color plano — el único suelo
            // del mundo que no era pixel comprado. Y se notaba justo donde más
            // duele, porque la orilla del agua (`tiles/water_arena`) y el
            // acantilado de arena (`cliff/arena`) SÍ traen la arena del pack: la
            // playa se veía de un amarillo y su propia orilla de otro.
            //
            // Ahora los dos son los tonos que LimeZu declara: la tierra khaki de
            // `Grass_3` (170,143,90) y la arena del mundo (230,174,85), medida en
            // `Sea_Water_Tileset` y en la meseta del kit de acantilado
            // (`import-acantilado.py::ARENA_ESPERADA`), que son el mismo amarillo.
            Tile::Dirt => [0.667, 0.561, 0.353],
            Tile::Sand => [0.902, 0.682, 0.333],
            // Colores GENÉRICOS del líquido. El color real lo pone el cuerpo
            // (`relief::liquid_color`): el mismo tile es agua, metano o lava según
            // dónde estés. Estos son el fallback de quien no sabe en qué cuerpo
            // está (íconos de inventario, tarjetas de la mochila).
            Tile::Water => [0.35, 0.62, 0.80],
            Tile::WaterDeep => [0.16, 0.34, 0.58],
            Tile::Cliff => [0.38, 0.33, 0.29],
            Tile::Slope => [0.52, 0.42, 0.30],
            Tile::Fence => [0.52, 0.38, 0.22],
            // CREMA, no gris. Es el color de la vereda que el pack dibuja para una
            // ciudad (`Sidewalk_1`), y es contra este color que `atlas-suelo.py`
            // elige la baldosa: con el gris de antes ganaban el adoquinado del
            // parque y las losas del jardín, así que las aceras de todas las
            // ciudades tenían piso de sendero. Ver el comentario de `floor/vereda`
            // allá — si cambia uno, cambia el otro.
            Tile::Pavement => [0.83, 0.80, 0.77],
            Tile::Asphalt => [0.28, 0.27, 0.29],
            Tile::Rock => [0.52, 0.50, 0.47],
            Tile::Regolith => [0.62, 0.62, 0.64],
            Tile::Basalt => [0.29, 0.28, 0.31],
            Tile::Ice => [0.82, 0.88, 0.93],
            Tile::RockRed => [0.60, 0.34, 0.24],
            Tile::Stone => [0.46, 0.47, 0.50],
            // Tierra apisonada, oscura: el fallback tiene que leerse como TIERRA
            // sin arte asignado (§fallbacks por código), no como un gris de roca.
            Tile::Subsoil => [0.29, 0.21, 0.14],
            Tile::RockDark => [0.30, 0.30, 0.34],
            Tile::Bedrock => [0.16, 0.16, 0.18],
            // El piso de la galería: tierra pisada, gris parda y bastante más
            // clara que el macizo, para que el hueco se distinga aun sin arte ni
            // luz. El gris (y no el marrón de la tierra suelta) es a propósito:
            // es lo que hace que la losa que le elige `atlas-suelo.py` sea el
            // suelo apisonado del pack y no un parqué — una cueva con piso de
            // madera se lee como un sótano de utilería.
            Tile::CaveFloor => [0.42, 0.38, 0.33],
            Tile::WoodFloor => [0.58, 0.44, 0.26],
            // Gris de hormigón, no lila claro: el color es lo que se ve donde
            // todavía no llega la textura, y tiene que ser EL MISMO material que
            // la losa que le eligió `atlas-suelo.py`. Con el claro de antes, un
            // piso de piedra sobre tierra se leía como un cuadrado blanco.
            Tile::StoneFloor => [0.54, 0.53, 0.50],
            Tile::Wall => [0.38, 0.34, 0.30],
            Tile::WoodWall => [0.45, 0.31, 0.17],
            Tile::StoneBrick => [0.55, 0.55, 0.58],
            Tile::Door => [0.30, 0.20, 0.10],
            Tile::Window => [0.55, 0.65, 0.75],
            Tile::Step => [0.62, 0.50, 0.32],
            Tile::Stairwell => [0.32, 0.24, 0.16],
            Tile::Air => [0.0, 0.0, 0.0],
        }
    }
}

// ---------------------------------------------------------------------------
// Construction system — build pieces catalogue (deterministic, shared)
// ---------------------------------------------------------------------------

/// A placeable structural piece. Placed on an empty surface cell via the build
/// mode (hammer + `BuildModeActive`), snapping to the aimed cell. Removing a
/// piece is just digging it — `Tile::dug_item` refunds the material.
#[derive(Clone, Copy, Debug)]
pub struct BuildPiece {
    pub id: &'static str,
    pub label: &'static str,
    /// The tile this piece places.
    pub tile: Tile,
    /// Resource consumed to place one (item id + quantity).
    pub cost_item: &'static str,
    pub cost_qty: u32,
}

/// Ordered palette of build pieces (index = palette slot).
///
/// **Sólo piezas de CELDA**: pisos y escaleras. Paredes, puertas y ventanas
/// dejaron de ser celdas — viven en el borde entre celdas (`build_grid::
/// EDGE_PIECES`, estilo Last Day on Earth). Los tiles `WoodWall`/`Door`/… siguen
/// existiendo para los asentamientos generados y las partidas viejas.
pub static BUILD_PIECES: &[BuildPiece] = &[
    BuildPiece {
        id: "build_wood_floor",
        label: "Piso madera",
        tile: Tile::WoodFloor,
        cost_item: "wood",
        cost_qty: 1,
    },
    BuildPiece {
        id: "build_stone_floor",
        label: "Piso piedra",
        tile: Tile::StoneFloor,
        cost_item: "stone",
        cost_qty: 1,
    },
    BuildPiece {
        id: "build_pavement",
        label: "Vereda",
        tile: Tile::Pavement,
        cost_item: "stone",
        cost_qty: 1,
    },
    // **Un gesto, una escalera entera.** Coloca el TRAMO completo hacia donde
    // mirás (`escaleras::PASOS` escalones + el rellano de llegada), no un escalón
    // suelto: una escalera de un escalón no existe, y dejar que se construya de a
    // celdas obligaba a adivinar cuál de las puntas sube. El coste es por escalón
    // (`cost_qty` × PASOS, lo cobra `build::apply_build_hold`).
    BuildPiece {
        id: "build_stairs",
        label: "Escalera",
        tile: Tile::Step,
        cost_item: "wood",
        cost_qty: 2,
    },
];

/// Number of pieces in the build palette.
pub fn build_piece_count() -> usize {
    BUILD_PIECES.len()
}

/// Palette piece by index (clamped-safe: returns None if out of range).
pub fn build_piece(idx: usize) -> Option<&'static BuildPiece> {
    BUILD_PIECES.get(idx)
}

// ---------------------------------------------------------------------------
// Prefabs — multi-tile stamps (rooms/houses placed in ONE hold). Body-agnostic:
// they place wherever building is allowed (Earth, Moon, Mars, stations). Art is
// optional (`room/*` slots + autotile `floor/*`/`wall/*`); logic is tiles only.
// ---------------------------------------------------------------------------

/// A rectangular room prefab: perimeter = wall, interior = floor, one door on
/// the south wall. `interior_walls` adds inner partitions (offsets rel. to the
/// prefab min corner); an inner `Door` splits rooms without leaving the house.
#[derive(Clone, Copy, Debug)]
pub struct Prefab {
    pub id: &'static str,
    pub label: &'static str,
    /// Footprint in tiles (w along +x, h along +z).
    pub w: i64,
    pub h: i64,
    /// Door offset on the perimeter (rel. to min corner).
    pub door: (i64, i64),
    /// Inner partition cells (rel. offsets) stamped as `WoodWall`.
    pub interior_walls: &'static [(i64, i64)],
    /// Inner door cells (rel. offsets) stamped as `Door`.
    pub interior_doors: &'static [(i64, i64)],
    /// Material charged per wall cell (floors cost 1 of the same item).
    pub cost_item: &'static str,
}

/// Curated presets — small enough to read on a phone screen. `casa_1` derives
/// from the LimeZu `Home_Designs` layout: two rooms split by a partition.
pub static PREFABS: &[Prefab] = &[
    Prefab {
        id: "room_small",
        label: "Cuarto 5x4",
        w: 5,
        h: 4,
        door: (2, 3),
        interior_walls: &[],
        interior_doors: &[],
        cost_item: "wood",
    },
    Prefab {
        id: "room_medium",
        label: "Cuarto 7x6",
        w: 7,
        h: 6,
        door: (3, 5),
        interior_walls: &[],
        interior_doors: &[],
        cost_item: "wood",
    },
    Prefab {
        id: "room_large",
        label: "Salon 9x8",
        w: 9,
        h: 8,
        door: (4, 7),
        interior_walls: &[],
        interior_doors: &[],
        cost_item: "wood",
    },
    Prefab {
        id: "casa_1",
        label: "Casa 9x7",
        w: 9,
        h: 7,
        door: (4, 6),
        // Partition wall at x=5 splitting living room / bedroom…
        interior_walls: &[(5, 1), (5, 2), (5, 4), (5, 5)],
        // …with an inner doorway at (5,3).
        interior_doors: &[(5, 3)],
        cost_item: "wood",
    },
];

pub fn prefab_count() -> usize {
    PREFABS.len()
}

pub fn prefab(idx: usize) -> Option<&'static Prefab> {
    PREFABS.get(idx)
}

/// Tile stamped at offset `(dx, dz)` of the prefab, or `None` outside the
/// footprint. Perimeter → wall, door offsets → `Door`, interior → floor.
pub fn prefab_cell(p: &Prefab, dx: i64, dz: i64) -> Option<Tile> {
    if dx < 0 || dz < 0 || dx >= p.w || dz >= p.h {
        return None;
    }
    if p.door == (dx, dz) || p.interior_doors.contains(&(dx, dz)) {
        return Some(Tile::Door);
    }
    if p.interior_walls.contains(&(dx, dz)) {
        return Some(Tile::WoodWall);
    }
    let perimeter = dx == 0 || dz == 0 || dx == p.w - 1 || dz == p.h - 1;
    Some(if perimeter { Tile::WoodWall } else { Tile::WoodFloor })
}

/// Total material cost of stamping the prefab: 2 per wall cell, 3 per door,
/// 1 per floor cell (mirrors the single-piece prices).
pub fn prefab_cost(p: &Prefab) -> u32 {
    let mut qty = 0;
    for dz in 0..p.h {
        for dx in 0..p.w {
            qty += match prefab_cell(p, dx, dz) {
                Some(Tile::WoodWall) => 2,
                Some(Tile::Door) => 3,
                Some(Tile::WoodFloor) => 1,
                _ => 0,
            };
        }
    }
    qty
}

/// Un "asset grande" colocable: un sprite ÚNICO (p.ej. 4×4) con footprint de
/// colisión. A diferencia de un prefab (que estampa tiles), esto coloca UNA
/// estructura-sprite que se registra en `BuiltStructures` (juego); su colisión
/// sale de la huella del art (manifest A1) o, si no hay huella pintada, de la
/// base completa `w×h`. El jugador camina ALREDEDOR (Y-sort), no adentro.
/// Agnóstico al cuerpo. (`biblia.md` §5.)
#[derive(Clone, Copy, Debug)]
pub struct LargeAsset {
    pub id: &'static str,
    pub label: &'static str,
    /// Slot de arte lógico (el usuario le asigna la textura en el editor). Sin
    /// arte, cae a un quad de color (como el resto del decor).
    pub art: &'static str,
    /// Sección del menú CONSTRUIR donde aparece. Con 120+ colocables la lista
    /// plana dejó de ser navegable en un celular — ver `placeables::PlaceCat`.
    pub cat: crate::placeables::PlaceCat,
    /// Footprint en tiles (`w` en +x, `h` en +z) — tamaño visual y base de
    /// colisión por defecto.
    pub w: i64,
    pub h: i64,
    pub cost_item: &'static str,
    pub cost_qty: u32,
}

/// Catálogo de assets grandes colocables. **Vive en `placeables.rs`**: acá quedan
/// sólo los accesos que usa el motor.
///
/// Eran tres entradas escritas a mano (peñasco, estatua, monumento) mientras el
/// catálogo de arte tenía 150 objetos con nombre. Mover la tabla no cambió una
/// línea de la colocación — es la MISMA `LargeAsset` por el MISMO camino.
pub use crate::placeables::PLACEABLES as LARGE_ASSETS;

pub fn large_asset_count() -> usize {
    LARGE_ASSETS.len()
}

pub fn large_asset(idx: usize) -> Option<&'static LargeAsset> {
    LARGE_ASSETS.get(idx)
}

// ---------------------------------------------------------------------------
// Deterministic settlements — auto-generated houses (per-body gated table).
// Building MECHANICS are body-agnostic; only this auto-generation is gated.
// ---------------------------------------------------------------------------

/// Una posible aldea por celda de SITE_PERIOD×SITE_PERIOD tiles.
///
/// ESCALA GTA-V (29 jul 2026): con un mundo de 60 km de lado, una aldea cada
/// 48 m era una calle continua de casas. A 1.8 km salen ~1 100 asentamientos
/// repartidos por el planeta — se cruzan volando cada tanto, que es lo que
/// hace que el mapa se sienta habitado en vez de saturado.
pub use crate::geography::SITE_PERIOD;

/// Which bodies auto-generate settlements (easy to extend: add a match arm).
pub fn body_generates_buildings(body: CelestialBody) -> bool {
    matches!(body, CelestialBody::Earth)
}

pub use crate::geography::{building_at, building_tile, BuildingSite};

/// Compat: el "sitio" de una celda de asentamiento es ahora el primer edificio
/// del núcleo que la ocupa. La generación de verdad vive en `crate::geography`
/// (manzanas dentro de la trama urbana) — esto sólo mantiene en pie a los
/// llamadores antiguos.
pub fn building_site_in(body: CelestialBody, sx: i64, sz: i64) -> Option<BuildingSite> {
    // **Memoizada por celda de asentamiento.** `prop_decor_at` la pregunta POR TILE
    // y el barrido de abajo puede recorrer hasta 24² anillos de `building_at`
    // (un núcleo sin edificio cerca del centro los agota): medido en el vuelo de
    // regreso a una ciudad, ~3 ms por tile — un chunk de 256 tiles tardaba 0,8 s
    // sólo en esto. La respuesta depende sólo del núcleo, no de quién pregunta.
    const RANURAS: usize = 32;
    type Clave = (u8, i64, i64, u32);
    thread_local! {
        static MEMO: std::cell::RefCell<[Option<(Clave, Option<BuildingSite>)>; RANURAS]> =
            const { std::cell::RefCell::new([const { None }; RANURAS]) };
    }
    let clave: Clave = (body as u8, sx, sz, crate::partida::epoca());
    let ranura = (hash_u32(sx as u32 ^ (sz as u32).wrapping_mul(0x9e37_79b1)) as usize) % RANURAS;
    if let Some(v) = MEMO.with(|m| m.borrow()[ranura].as_ref().and_then(|(c, v)| (*c == clave).then_some(*v))) {
        return v;
    }
    let v = building_site_in_sin_memo(body, sx, sz);
    // En modo grueso (disco planetario) los edificios se deciden sin ríos: esa
    // respuesta no vale para el chunk de a pie.
    if !crate::grueso::activo() {
        MEMO.with(|m| m.borrow_mut()[ranura] = Some((clave, v)));
    }
    v
}

fn building_site_in_sin_memo(body: CelestialBody, sx: i64, sz: i64) -> Option<BuildingSite> {
    let s = crate::geography::settlement_in(body, sx, sz)?;
    // El primer edificio que aparece barriendo en anillos desde el centro del
    // núcleo. Antes se pedía "el edificio de la manzana (0,0)", pero con parcelas
    // de tamaño variable la manzana central bien puede ser una plaza o el patio de
    // un hospital: buscar el primero que haya es lo que esta función siempre quiso
    // decir ("dame UN edificio de este núcleo, para plantarle props alrededor").
    for r in 0..24i64 {
        for dz in -r..=r {
            for dx in -r..=r {
                if r > 0 && dx.abs() != r && dz.abs() != r {
                    continue;
                }
                let (tx, tz) = (s.center.0 + dx * 9, s.center.1 + dz * 9);
                if let Some(b) = crate::geography::building_at(body, tx, tz) {
                    return Some(b);
                }
            }
        }
    }
    None
}

/// El tile de edificio generado en esta celda, si lo hay.
pub fn building_cell(body: CelestialBody, tx: i64, tz: i64) -> Option<Tile> {
    crate::geography::building_tile(body, tx, tz)
}

// ---------------------------------------------------------------------------
// Deterministic hash noise (2D)
// ---------------------------------------------------------------------------

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

fn hash2(seed: u32, a: i64, b: i64) -> u32 {
    let a = (a as i32 as u32).wrapping_mul(0x9e37_79b9);
    let b = (b as i32 as u32).wrapping_mul(0x85eb_ca6b);
    hash_u32(seed ^ a ^ hash_u32(b))
}

fn hash01(seed: u32, a: i64, b: i64) -> f32 {
    (hash2(seed, a, b) & 0x00ff_ffff) as f32 / 16_777_215.0
}

/// 2D value noise with smooth interpolation, lattice every `period` tiles.
fn value_noise_2d(seed: u32, tx: i64, tz: i64, period: i64) -> f32 {
    let period = period.max(2);
    let cx = tx.div_euclid(period);
    let cz = tz.div_euclid(period);
    let fx = (tx.rem_euclid(period)) as f32 / period as f32;
    let fz = (tz.rem_euclid(period)) as f32 / period as f32;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sz = fz * fz * (3.0 - 2.0 * fz);
    let n00 = hash01(seed, cx, cz);
    let n10 = hash01(seed, cx + 1, cz);
    let n01 = hash01(seed, cx, cz + 1);
    let n11 = hash01(seed, cx + 1, cz + 1);
    let a = n00 + (n10 - n00) * sx;
    let b = n01 + (n11 - n01) * sx;
    a + (b - a) * sz
}

/// Fractal (2-octave) 2D noise in 0..1.
fn fbm2(seed: u32, tx: i64, tz: i64, period: i64) -> f32 {
    let a = value_noise_2d(seed, tx, tz, period);
    let b = value_noise_2d(seed ^ 0x51ed_2b37, tx, tz, (period / 3).max(2));
    (a * 0.68 + b * 0.32).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Terrain generation per (body, z_level)
// ---------------------------------------------------------------------------

/// El tile natural de una celda de superficie. La geografía (relieve, clima,
/// biomas, ríos) vive en `crate::geography` — ÚNICA fuente de verdad, compartida
/// con la decoración y con el mapa satelital del cliente.
fn surface_tile(body: CelestialBody, tx: i64, tz: i64) -> Tile {
    crate::geography::natural_tile(body, tx, tz)
}

/// Fuera del cuerpo: la celda cae más allá del radio caminable. La orilla se
/// ondula un poco con ruido para que la costa no sea un círculo perfecto.
pub fn beyond_world_edge(body: CelestialBody, tx: i64, tz: i64) -> bool {
    let r = crate::celestial_bodies::body_walk_radius(body);
    if r <= 0.0 {
        return false;
    }
    let d = ((tx * tx + tz * tz) as f64).sqrt() as f32 * TILE_M;
    let wobble = (fbm2(body_terrain_seed(body) ^ 0xC0A5_7000, tx / 24, tz / 24, 3) - 0.5) * r * 0.03;
    d > r * 0.985 + wobble
}

/// The natural (un-edited) tile at a grid cell of a given z_level.
pub fn topdown_tile(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Tile {
    // **Memoria por celda.** El que camina pregunta por las MISMAS ~12 celdas varias
    // veces por tick (colisión, nado, cercas) y cada respuesta recalcula ruido,
    // lotes y obra civil: ~55 µs en la compu, x5-8 en el Redmi (medido: `tick_sim`
    // se llevaba 12-15 ms por cuadro). La respuesta es pura dada la semilla, la
    // época y las zonas autoradas, y las tres entran en la clave. En grueso (malla
    // lejana, disco del planeta) no se memoriza: devuelve otra cosa a propósito.
    if z != 0 || crate::grueso::activo() {
        return topdown_tile_calc(body, z, tx, tz);
    }
    const RANURAS: usize = 2048;
    type Clave = (u8, i64, i64, u32, u32);
    thread_local! {
        static MEMO: std::cell::RefCell<Vec<Option<(Clave, Tile)>>> =
            std::cell::RefCell::new(vec![None; RANURAS]);
    }
    let clave: Clave = (body as u8, tx, tz, crate::partida::epoca(), crate::zonas::generacion());
    let h = (tx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (tz as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    let ranura = (h >> 40) as usize % RANURAS;
    if let Some(t) = MEMO.with(|m| m.borrow()[ranura].as_ref().and_then(|(c, t)| (*c == clave).then_some(*t))) {
        return t;
    }
    let t = topdown_tile_calc(body, z, tx, tz);
    MEMO.with(|m| m.borrow_mut()[ranura] = Some((clave, t)));
    t
}

fn topdown_tile_calc(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Tile {
    // Los interiores de los edificios viven en su propia banda de pisos (ver `interiores`).
    if crate::interiores::es_interior(z) {
        return crate::interiores::tile(body, z, tx, tz);
    }
    if z == 0 {
        // MUNDO FINITO (29 jul 2026): más allá del radio caminable del cuerpo no
        // hay más mundo. El mapa es procedural pero ACOTADO — 60 km de punta a
        // punta en la Tierra —, así que el disco que ves desde el espacio es
        // literalmente todo el terreno que existe, y el borde es agua (o roca en
        // los cuerpos sin aire) en vez de una llanura infinita.
        if beyond_world_edge(body, tx, tz) {
            return if body_atmo_top(body) > 0.0 { Tile::Water } else { Tile::RockDark };
        }
        // **Desde el aire una ciudad es una mancha, no mil parcelas.** En grueso
        // (disco del planeta, malla lejana) cada muestra dentro de un casco
        // recalculaba lote, casa y calle sin caché (~2 ms por tile; una tanda de
        // píxeles del disco tardaba 100-370 ms y era el tirón al reentrar volando
        // hacia una ciudad). Se devuelve un mosaico barato de vereda y pasto.
        if crate::grueso::activo()
            && crate::geography::settlement_covering(body, tx, tz).is_some()
        {
            let nat = surface_tile(body, tx, tz);
            if matches!(nat, Tile::Water | Tile::WaterDeep | Tile::Cliff | Tile::Slope) {
                return nat;
            }
            return if hash01(0x0C1D_AD05, tx.div_euclid(3), tz.div_euclid(3)) < 0.42 {
                Tile::Pavement
            } else {
                Tile::Grass
            };
        }
        // OBRA CIVIL primero, y dentro de ella **la CALZADA es sagrada**: gana
        // sobre cualquier cosa edificada.
        //
        // Antes los edificios iban antes que la obra civil, y bastaba con que la
        // geometría de una parcela se pasara un tile para dejar un muro cruzado en
        // mitad de la avenida. Discutir cada caso de redondeo es una batalla
        // interminable; declarar que la calzada no se pisa la termina de una vez:
        // por muy mal que salga una parcela, la carretera sigue siendo carretera y
        // el mapa sigue siendo transitable. Lo cazó `no_road_is_ever_blocked_by_a_cliff`.
        //
        // La ACERA (`Pavement`) no gana: va por debajo, porque una acera es el
        // suelo que rodea al edificio y no tiene sentido que le coma el portal.
        let civil = crate::geography::civil_at(body, tx, tz);
        if let Some(c @ (crate::geography::Civil::Road | crate::geography::Civil::Bridge)) = civil {
            return crate::geography::civil_tile(c);
        }
        // Generated settlements override the natural ground (Earth-only table;
        // the tiles behave like any other — collide, dig, persist edits).
        if let Some(t) = building_cell(body, tx, tz) {
            return t;
        }
        if let Some(c) = civil {
            return crate::geography::civil_tile(c);
        }
        // La BOCA de una cueva asoma en superficie como el hueco de escalera que
        // es. Va DESPUÉS de la obra civil y de los asentamientos: una escalera
        // abierta en mitad de la avenida —o dentro del salón de una casa— es
        // justo la clase de accidente que hace impasable un mapa generado. Si una
        // parcela le cae encima, la cueva se queda sin esa entrada y hay que
        // llegar a ella cavando, que es una forma perfectamente buena de entrar.
        if let Some(t) = crate::cuevas::tile_natural(body, 0, tx, tz) {
            return t;
        }
        let nat = surface_tile(body, tx, tz);
        // **BLINDAJE DE LA CALZADA CONTRA EL ACANTILADO.** La cara de un acantilado dibuja su borde
        // superior una celda hacia el norte: si la celda de arriba es calzada o acera, el pedazo de
        // roca le cae encima y la carretera queda cortada por un muro (pasó en varias carreteras al
        // pie de una terraza, 4 oct 2026). Donde una obra civil queda al norte, el canto se vuelve
        // rampa: la carretera sube o baja por ahí, y el asfalto queda limpio.
        if nat == Tile::Cliff
            && [(-1, -1), (0, -1), (1, -1)]
                .iter()
                .any(|&(dx, dz)| matches!(crate::geography::civil_at(body, tx + dx, tz + dz), Some(_)))
        {
            return Tile::Slope;
        }
        // **EL INTERIOR DE LA MANZANA ES PARCELA, no el bioma** (27 sep 2026). Sin
        // lote ni obra civil, la celda caía al terreno natural: manzanas enteras de
        // arena naranja entre veredas. Adentro de un casco el suelo suelto es
        // pasto de parcela; el agua (ríos y canales) y la roca se quedan.
        if matches!(nat, Tile::Sand | Tile::Dirt)
            && crate::geography::settlement_covering(body, tx, tz).is_some()
        {
            return Tile::Grass;
        }
        return nat;
    }
    if z > 0 {
        // **Los pisos altos son VACÍO, no roca.**
        //
        // Antes esto devolvía `Bedrock`, que es sólido: `place_tile` rechaza toda
        // celda sólida y `break_tile` no pica bedrock, así que arriba no se podía
        // ni construir ni cavar. El único z>0 que existía era la sala 7×7 que
        // tallaba `carve_stair_room` al llegar por la grada — o sea que "poner una
        // escalera y armar el segundo piso" (Project Zomboid) era imposible: subías
        // a un cuarto fijo del que no se podía salir ni agrandar.
        //
        // `Air` no es sólido, así que la celda admite piso, y a la vez no es suelo
        // (`buildable_ground` la excluye): el que camina sobre vacío se CAE al piso
        // de abajo (`world2d::fall_through_void`). Ésa es la regla entera de la
        // verticalidad, y es la misma que hace que un balcón se sienta un balcón.
        return Tile::Air;
    }
    // **El subsuelo es MACIZO salvo donde hay una cueva.** (4 ago 2026)
    //
    // El 3 ago se borraron las galerías procedurales enteras porque entrar en
    // ellas se leía como cambiar de escena: *"odio las cuevas y cómo entramos a
    // la cueva"*. Lo que estaba roto era la ENTRADA, no la existencia de la
    // cueva. Ahora vuelven con boca de escalera caminada y en un solo nivel
    // (`cuevas.rs`), así que entrar es andar y adentrarse no vuelve a cambiar de
    // capa ni una vez más.
    //
    // Todo lo demás sigue siendo piedra hasta el lecho de roca, y ahí el único
    // hueco es el que abriste con el pico (`TileWorld::dig_down`). Tu túnel se
    // dibuja EN EL MISMO MUNDO, al lado de tu casa, no en otra pantalla.
    // **LA CUEVA VA PRIMERO Y SIGUE PEGADA A SU BOCA.** Una cueva natural es un
    // agujero en la ladera con su escalera: si se la moviera de nivel con el
    // terreno, el tramo que baja desde la boca se quedaría colgado. Y son la mitad
    // de la razón para bajar, así que enterrarlas bajo la meseta las volvería
    // inencontrables en el 90 % del mundo (el 43 % de la tierra es terraza 1).
    if let Some(t) = crate::cuevas::tile_natural(body, z, tx, tz) {
        return t;
    }
    // **Y LA PROFUNDIDAD SE MIDE DESDE EL PISO BASE, NO DESDE DONDE PISÁS.**
    //
    // Una meseta es tierra apilada ([`tierra_de_meseta`]): el que cava a ciegas en
    // la terraza 2 tiene dos niveles de tierra por delante antes de estar donde
    // estaría si hubiera cavado en la llanura. Sin esto el subsuelo era plano y se
    // olvidaba de la montaña que tenía encima — un tile de pico en la cumbre te
    // dejaba a la misma altura que uno en la playa.
    let zb = z + tierra_de_meseta(body, tx, tz);
    if zb >= 0 {
        // Todavía estamos dentro de la tierra de la meseta.
        return Tile::Subsoil;
    }
    if zb <= LECHO_DE_ROCA || z <= MIN_Z_LEVEL {
        return Tile::Bedrock;
    }
    // **Los primeros metros son TIERRA; más abajo, roca.**
    //
    // No es geología: es lectura. El nivel de las cuevas y los dos siguientes son
    // donde se juega —el sótano que cavás, la galería que encontrás—, y ahí lo que
    // tiene que llenar la pantalla es tierra: es lo que dice "estás debajo". La
    // roca empieza cuando ya bajaste de verdad, y entonces sí el cambio de
    // material CUENTA algo (y sigue dando piedra al picarla, que es de donde sale
    // media construcción).
    // También por profundidad desde el piso base: la franja de tierra tiene que
    // estar donde está la cueva, no donde casualmente empezaste a cavar.
    match body {
        CelestialBody::Moon => Tile::RockDark,
        CelestialBody::Earth if zb >= SUELO_DE_TIERRA => Tile::Subsoil,
        _ => Tile::Stone,
    }
}

/// Hasta este nivel (inclusive, contando hacia abajo) el subsuelo de la Tierra es
/// tierra; por debajo es roca. Tres plantas: la de las cuevas y dos más.
pub const SUELO_DE_TIERRA: i32 = -3;

// ---------------------------------------------------------------------------
// (Los pozos y portales de cueva vivían acá. Se borraron el 3 ago 2026: ver
// `topdown_tile` — el subsuelo es macizo y el único hueco es el que cavás.)
// ---------------------------------------------------------------------------

pub fn solid_at(body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
    topdown_tile(body, z, tx, tz).solid()
}

// ---------------------------------------------------------------------------
// Deposits (ore/gem veins) — deterministic, denser underground
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deposit {
    OreIron,
    OreGold,
    GemBlue,
    GemRed,
}

/// Deterministic deposit at a walkable cell, if any.
///
/// **En la superficie una veta sólo asoma en terreno virgen**: no sobre calzada, vereda,
/// lote ni casa (`civil_at`/`building_cell`), ni en el agua o la rampa. Sin esto, cada
/// celda de la calle tenía un 0,3 % de "veta" y salían pepitas marrones sobre el asfalto y
/// la vereda. Y no hay vetas en los pisos altos: ahí hay losas, no roca.
pub fn deposit_at(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<Deposit> {
    if z > 0 {
        return None;
    }
    if z == 0
        && (!topdown_tile(body, 0, tx, tz).buildable_ground()
            || building_cell(body, tx, tz).is_some()
            || crate::geography::civil_at(body, tx, tz).is_some())
    {
        return None;
    }
    deposit_roll(body, z, tx, tz)
}

fn deposit_roll(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<Deposit> {
    // Surface: sparse rocks/ore. Underground: richer, deeper = rarer gems.
    let seed = body_terrain_seed(body) ^ 0xde90_0000u32 ^ (z as u32).wrapping_mul(0x2545);
    let h = hash2(seed, tx, tz);
    let roll = h % 1000;
    let deep = z < 0;
    match body {
        CelestialBody::Moon => match roll {
            0..=3 if deep => Some(Deposit::GemRed),
            4..=14 if deep => Some(Deposit::OreIron),
            0..=1 => Some(Deposit::OreIron),
            _ => None,
        },
        CelestialBody::Earth if deep => {
            // **Lo que se ve en la pared depende del bioma** (`subsuelo.rs`): la veta de hierro es de
            // hierro, las cavernas de cristal brillan con gemas, y el resto tiene lo de siempre.
            use crate::subsuelo::Bioma;
            match (crate::subsuelo::bioma_en(body, tx, tz), roll) {
                (Bioma::Hierro, 0..=26) => Some(Deposit::OreIron),
                (Bioma::Hierro, 27..=29) => Some(Deposit::OreGold),
                (Bioma::Cristal, 0..=12) => Some(Deposit::GemBlue),
                (Bioma::Cristal, 13..=21) => Some(Deposit::GemRed),
                (Bioma::Cristal, 22..=24) => Some(Deposit::OreGold),
                (Bioma::Arcilla, 0..=2) => Some(Deposit::OreIron),
                (Bioma::Arcilla, 3..=6) => Some(Deposit::OreGold),
                (Bioma::Musgo, 0..=3) => Some(Deposit::OreIron),
                (Bioma::Musgo, 4..=5) => Some(Deposit::GemBlue),
                (Bioma::Piedra, 0..=6) => Some(Deposit::OreIron),
                (Bioma::Piedra, 7..=10) => Some(Deposit::OreGold),
                (Bioma::Piedra, 11..=13) => Some(Deposit::GemBlue),
                _ => None,
            }
        }
        CelestialBody::Earth => match roll {
            0..=2 => Some(Deposit::OreIron),
            _ => None,
        },
        _ => match roll {
            0..=8 if deep => Some(Deposit::OreIron),
            9..=12 if deep => Some(Deposit::GemBlue),
            0..=1 => Some(Deposit::OreIron),
            _ => None,
        },
    }
}

// ---------------------------------------------------------------------------
// 8-direction locomotion (no gravity, capsule vs tile grid, axis-separated)
// ---------------------------------------------------------------------------

/// Nearest of 8 facing directions (0..8, 0 = East, CCW) for animation.
pub fn dir8(rot: f32) -> u8 {
    let tau = std::f32::consts::TAU;
    (((rot.rem_euclid(tau) / tau) * 8.0).round() as u8) % 8
}

/// Step the player on a top-down floor. `body.x`/`body.z` are the ground
/// plane; `move_x`→x, `move_z`→z. `solid(tx,tz)` answers for the ACTIVE floor
/// including player edits.
pub fn step_topdown_movement(
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    solid: &dyn Fn(i64, i64) -> bool,
) {
    step_topdown_movement_ex(body, input, dt, solid, &|_x, _z| false)
}

/// Cuánto del paso normal se avanza NADANDO. Bastante lento como para que cruzar
/// un lago sea una decisión (rodearlo suele salir más barato) y no tanto como para
/// que sea un castigo.
pub const SWIM_SPEED_MULT: f32 = 0.45;

/// Igual que `step_topdown_movement`, pero además contra las **paredes de
/// borde** (`build_grid`): `walls(x, z)` responde si la cápsula centrada ahí
/// pisa una pared que corta el paso. Separado del grid de celdas porque una
/// pared vive en la ARISTA entre dos celdas, no en una celda.
pub fn step_topdown_movement_ex(
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    solid: &dyn Fn(i64, i64) -> bool,
    walls: &dyn Fn(f32, f32) -> bool,
) {
    step_topdown_movement_full(body, input, dt, solid, walls, &|_x, _z| false)
}

/// La versión completa: además del grid y las paredes de borde, `swim(tx,tz)`
/// responde si esa celda es líquido de poco fondo.
///
/// Nadar no es un modo aparte —no hay estado que entrar ni salir—: es el MISMO
/// paso, más lento, sin esprintar y con otra pose. Por eso se puede entrar al agua
/// caminando desde la playa sin ningún corte, que es lo que hace que la costa se
/// sienta continua en vez de un borde con un cartel.
pub fn step_topdown_movement_full(
    body: &mut MovementBody,
    input: &InputState,
    dt: f32,
    solid: &dyn Fn(i64, i64) -> bool,
    walls: &dyn Fn(f32, f32) -> bool,
    swim: &dyn Fn(i64, i64) -> bool,
) {
    let swimming = swim(
        (body.x / TILE_M).floor() as i64,
        (body.z / TILE_M).floor() as i64,
    );
    let mut mx = input.move_x;
    let mut mz = input.move_z;
    let len = (mx * mx + mz * mz).sqrt();
    if len > 1.0 {
        mx /= len;
        mz /= len;
    }
    let moving = len > 0.1;
    // Nadando no se esprinta: la carrera es de pies, no de brazos.
    let wants_sprint = input.sprint == 1 && moving && body.stamina > 0.0 && !swimming;
    let speed_mult = if wants_sprint {
        SPRINT_MULT
    } else if swimming {
        SWIM_SPEED_MULT
    } else {
        1.0
    };
    let target_vx = mx * body.speed * speed_mult;
    let target_vz = mz * body.speed * speed_mult;
    let blend = (FRICTION * dt).clamp(0.0, 1.0);
    body.vx += (target_vx - body.vx) * blend;
    body.vz += (target_vz - body.vz) * blend;

    if wants_sprint {
        body.stamina = (body.stamina - STAMINA_SPRINT_DRAIN * dt).max(0.0);
    } else {
        body.stamina = (body.stamina + STAMINA_SPRINT_DRAIN * 0.6 * dt).min(body.max_stamina);
    }

    // X primero y Z después, resolviendo contra los sólidos (deslizar por la
    // pared) — y con CORRECCIÓN DE ESQUINA en los dos ejes (ver `corner_slip`).
    let free = |x: f32, z: f32| !capsule_hits(x, z, solid) && !walls(x, z);
    // Cuánto se permite correr de lado en este frame: nunca más de lo que el
    // propio jugador avanza, así el asistente jamás se siente un empujón.
    let slack = (body.speed * speed_mult * dt).min(CORNER_SLIP);

    // **Quieto no se consulta el mundo.** Sin intención de moverse y sin velocidad
    // residual la posición no cambia, así que probar la cápsula contra el terreno
    // (decenas de `topdown_tile` por tick en una celda sin caché) era puro costo:
    // en el Redmi, parado en el pasto, eran 12-15 ms por cuadro (medido).
    let quieto = !moving && body.vx.abs() < 1e-3 && body.vz.abs() < 1e-3;
    if quieto {
        body.vx = 0.0;
        body.vz = 0.0;
    }
    let new_x = body.x + body.vx * dt;
    if quieto {
        // sin cambios de posición
    } else if free(new_x, body.z) {
        body.x = new_x;
    } else if let Some(z) = corner_slip(body.z, slack, &|z| free(new_x, z)) {
        body.x = new_x;
        body.z = z;
    } else {
        body.vx = 0.0;
    }
    let new_z = body.z + body.vz * dt;
    if quieto {
        // sin cambios de posición
    } else if free(body.x, new_z) {
        body.z = new_z;
    } else if let Some(x) = corner_slip(body.x, slack, &|x| free(x, new_z)) {
        body.z = new_z;
        body.x = x;
    } else {
        body.vz = 0.0;
    }

    // El estado de natación se mantiene también quieto en el agua: flotar es una
    // pose, no una pausa — si no, pararse en mitad del lago te devolvía a la pose
    // de estar de pie sobre el agua.
    body.state = if swimming {
        MOV_SWIMMING
    } else if !moving {
        MOV_IDLE
    } else if wants_sprint {
        MOV_SPRINTING
    } else {
        MOV_RUNNING
    };
    if moving {
        body.rot = mz.atan2(mx);
    }
    body.y = 0.0;
}

/// Cuánto se puede correr al jugador de lado, como MUCHO, para librar el pico de
/// una esquina (metros por frame).
///
/// 0.16 sobre un radio de 0.34: alcanza para que un roce de un par de
/// centímetros no te frene, y es demasiado poco como para atravesar nada — un
/// hueco de verdad mide un tile entero.
const CORNER_SLIP: f32 = 0.16;

/// **Que no te comas la esquina.**
///
/// El problema, que en un juego cenital se siente todo el tiempo: vas caminando
/// pegado a una pared y el pico de un bloque te roza dos centímetros. La cápsula
/// choca, el eje se cancela y quedás CLAVADO contra una arista, empujando sin
/// avanzar. No es que no quepas: te sobra casi todo el ancho, te falta un pelo.
///
/// La corrección es la de los plataformeros: si el movimiento está bloqueado
/// pero corriéndose un poco de lado deja de estarlo, se corre y se pasa. Sólo
/// se prueban desplazamientos **pequeños** (`CORNER_SLIP`, y nunca más de lo
/// que el jugador avanza en ese frame), así que jamás puede meterte por un
/// hueco que no existe ni teletransportarte: si hace falta más que un roce,
/// devuelve `None` y chocás como siempre.
///
/// Se prueba de menos a más y en los dos sentidos, quedándose con el
/// desplazamiento MÁS CHICO que funciona — el que menos se nota. Y se aplica en
/// los dos ejes, así que sirve igual yendo en horizontal que en vertical, y en
/// las esquinas de 90° que es donde más engancha.
fn corner_slip(cur: f32, slack: f32, free_at: &dyn Fn(f32) -> bool) -> Option<f32> {
    if slack <= 0.0 {
        return None;
    }
    const STEPS: i32 = 4;
    for i in 1..=STEPS {
        let d = slack * i as f32 / STEPS as f32;
        for cand in [cur + d, cur - d] {
            if free_at(cand) {
                return Some(cand);
            }
        }
    }
    None
}

fn capsule_hits(x: f32, z: f32, solid: &dyn Fn(i64, i64) -> bool) -> bool {
    let x0 = ((x - PLAYER_RADIUS) / TILE_M).floor() as i64;
    let x1 = ((x + PLAYER_RADIUS - 1e-4) / TILE_M).floor() as i64;
    let z0 = ((z - PLAYER_RADIUS) / TILE_M).floor() as i64;
    let z1 = ((z + PLAYER_RADIUS - 1e-4) / TILE_M).floor() as i64;
    for tx in x0..=x1 {
        for tz in z0..=z1 {
            if solid(tx, tz) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::empty_input;

    fn natural_solid(body: CelestialBody, z: i32) -> impl Fn(i64, i64) -> bool {
        move |tx, tz| solid_at(body, z, tx, tz)
    }

    /// **No quedarse comido por una esquina.** Un bloque suelto con el jugador
    /// rozándolo por un pelo: antes el eje se cancelaba y te quedabas empujando
    /// contra la arista sin avanzar nunca. Ahora se corre lo justo y pasa.
    #[test]
    fn a_corner_does_not_swallow_you() {
        // Un único tile sólido en (2, 2).
        let solid = |tx: i64, tz: i64| (tx, tz) == (2, 2);
        let nowalls = |_x: f32, _z: f32| false;
        let mut b = MovementBody::new(4.0, PLAYER_RADIUS);
        // Caminando hacia +x justo al sur del bloque (que ocupa z ∈ [2,3]), tan
        // pegado que la cápsula lo roza por 5 cm. Eso es un ROCE, no un choque:
        // sobra el 93 % del ancho del jugador.
        b.x = 0.5;
        b.z = 3.0 + PLAYER_RADIUS - 0.05;
        let mut input = empty_input(0);
        input.move_x = 1.0;
        let start = b.x;
        for _ in 0..120 {
            step_topdown_movement_ex(&mut b, &input, 1.0 / 60.0, &solid, &nowalls);
        }
        assert!(
            b.x > start + 2.0,
            "se quedó trabado en la esquina: avanzó sólo {:.2} m",
            b.x - start
        );
        // Y NO lo atravesó: la cápsula sigue enteramente al sur del bloque.
        assert!(b.z >= 3.0 - 1e-3, "se metió dentro del sólido (z={})", b.z);
    }

    /// El asistente de esquina no puede abrir puertas que no existen: contra una
    /// pared LLENA se choca igual que siempre.
    #[test]
    fn corner_slip_never_opens_a_solid_wall() {
        // Pared vertical completa en x = 2.
        let solid = |tx: i64, _tz: i64| tx == 2;
        let nowalls = |_x: f32, _z: f32| false;
        let mut b = MovementBody::new(4.0, PLAYER_RADIUS);
        b.x = 0.5;
        b.z = 5.5;
        let mut input = empty_input(0);
        input.move_x = 1.0;
        for _ in 0..240 {
            step_topdown_movement_ex(&mut b, &input, 1.0 / 60.0, &solid, &nowalls);
        }
        assert!(b.x < 2.0, "atravesó la pared (x={})", b.x);
    }

    #[test]
    fn terrain_is_deterministic() {
        let a = topdown_tile(CelestialBody::Earth, 0, 12, -7);
        let b = topdown_tile(CelestialBody::Earth, 0, 12, -7);
        assert_eq!(a, b);
        // La Luna ya no es un gris uniforme: regolito, mares de basalto y algún
        // pico. Lo que importa es que sea determinista y lunar.
        // `Cliff`/`Slope` entraron con el relieve por terrazas: una celda lunar
        // puede caer en el borde de una terraza, y eso sigue siendo terreno lunar.
        assert!(matches!(
            topdown_tile(CelestialBody::Moon, 0, 3, 3),
            Tile::Regolith | Tile::Basalt | Tile::Stone | Tile::Cliff | Tile::Slope
        ));
        assert_eq!(topdown_tile(CelestialBody::Earth, -20, 0, 0), Tile::Bedrock);
    }

    #[test]
    fn surface_has_variety() {
        // Los biomas ahora son REGIONES de kilómetros (continentes, cordilleras,
        // desiertos), no manchas de un tile: hay que barrer el planeta, no un
        // cuadrado de 40 m, para verlos. Que un patch chico sea homogéneo es la
        // intención, no un fallo.
        let mut kinds = std::collections::HashSet::new();
        for i in -14..14 {
            for j in -14..14 {
                kinds.insert(topdown_tile(CelestialBody::Earth, 0, i * 1_000, j * 1_000));
            }
        }
        assert!(kinds.len() >= 4, "expected biome variety, got {kinds:?}");
        assert!(kinds.contains(&Tile::Water), "un planeta sin oceanos");
        assert!(kinds.contains(&Tile::Grass), "un planeta sin praderas");
    }

    /// GUARDA DE COSTE, medida en RELATIVO.
    ///
    /// Un chunk son 256 celdas y el streaming construye decenas por segundo,
    /// además del mapa satelital (miles de muestras por reconstrucción). Cuando
    /// la geografía nueva entró sin optimizar, cada celda repetía diez veces el
    /// cálculo de los pozos de cueva y el juego se quedaba colgado arrancando.
    ///
    /// Se compara contra el coste de UNA evaluación de relieve medida en la
    /// misma pasada, no contra un número de microsegundos: así el test dice
    /// "generar una celda no puede costar más que unas pocas consultas de
    /// terreno" y no depende de lo rápida ni de lo cargada que esté la máquina
    /// (un umbral absoluto se volvía intermitente en el pipeline de publicación).
    #[test]
    fn generating_a_cell_costs_only_a_few_terrain_probes() {
        use std::time::Instant;
        let body = CelestialBody::Earth;
        const N: i64 = 4_000;
        // Precalentar ambos caminos.
        for i in 0..256 {
            let _ = topdown_tile(body, 0, i, i);
            let _ = crate::geography::elevation_at(body, i, i);
        }

        // BASE: el campo de relieve crudo, SIN caché (`relief_field`, no
        // `relief_level`). Es la unidad honesta —"una evaluación de ruido"— y
        // además no calienta la caché de niveles, que era lo que hacía que el caso
        // disperso midiera más barato que el secuencial.
        let t0 = Instant::now();
        let mut sink = 0.0f32;
        for i in 0..N {
            sink += crate::relief::relief_field(body, 900_000 + i * 7, 900_000 + i * 13);
        }
        let base = t0.elapsed().as_nanos().max(1) as f64;

        // (a) BARRIDO SECUENCIAL — el camino caliente de verdad: es como
        // `build_chunk_mesh` recorre un chunk, 60 veces por segundo en el Redmi.
        // La caché de niveles de `relief` cobra aquí: cada celda se consulta cinco
        // veces (como centro y como vecina de sus cuatro contiguas) y se calcula
        // una sola.
        let t1 = Instant::now();
        for cz in 0..(N / 64) {
            for cx in 0..64 {
                sink += topdown_tile(body, 0, cx, cz) as u8 as f32;
            }
        }
        let sequential = t1.elapsed().as_nanos() as f64;

        // (b) CONSULTAS DISPERSAS — colisión, IA, el ítem-mapa. **Las mismas
        // celdas** que (a), sólo que recorridas en orden salteado: así la única
        // variable entre las dos medidas es la localidad, y no el terreno que a
        // cada una le tocó (un barrido que se sale del mundo choca con el
        // `beyond_world_edge` y sale gratis — medir eso no dice nada).
        let cells = 64 * (N / 64);
        let t2 = Instant::now();
        for i in 0..cells {
            let k = (i * 2_657) % cells; // 2657 es primo con 64·62: recorre todo
            sink += topdown_tile(body, 0, k % 64, k / 64) as u8 as f32;
        }
        let scattered = t2.elapsed().as_nanos() as f64;
        assert!(sink.is_finite());

        let seq_ratio = sequential / base;
        let sca_ratio = scattered / base;
        // Generar una celda mira relieve (con sus cuatro vecinas), humedad, río,
        // obra civil y pozos de cueva: medido en una máquina en reposo da ~8x
        // secuencial y ~11x disperso.
        //
        // **El umbral es deliberadamente holgado (40x).** Esto corre en paralelo
        // con el resto de la suite, así que la relación oscila entre 8x y 16x según
        // la contención — apretarlo a 13x lo volvía intermitente, que es peor que
        // no tenerlo. Lo que este guardián tiene que cazar es la regresión de ORDEN
        // DE MAGNITUD: la primera versión de la caché de relieve, que agrupaba los
        // niveles en bloques de 16×16 y recalculaba las 256 celdas para leer una,
        // marcaba 225x. Eso lo detecta con 40x y sin falsos positivos.
        assert!(
            seq_ratio < 40.0,
            "generar un chunk cuesta {seq_ratio:.1}x por celda (disperso: {sca_ratio:.1}x)"
        );
        assert!(
            sca_ratio < 40.0,
            "una consulta suelta cuesta {sca_ratio:.1}x: alguien perdió la localidad"
        );
    }

    
    
    
    /// **Bajo el nivel de las cuevas, el subsuelo es macizo.**
    ///
    /// Las galerías procedurales de antes se repartían por TODOS los niveles: se
    /// cavaba dos metros y ya estabas en un laberinto que no habías buscado. Las
    /// cuevas de ahora viven en un solo nivel (`cuevas::NIVEL`), así que de `−2`
    /// para abajo la mina sigue siendo lo que abrís vos con el pico. Si esto
    /// falla, alguien volvió a sembrar hueco por todo el espesor del mundo.
    #[test]
    fn bajo_las_cuevas_el_subsuelo_es_macizo() {
        let body = CelestialBody::Earth;
        for z in (MIN_Z_LEVEL + 1)..crate::cuevas::NIVEL {
            for tx in 0..40 {
                for tz in 0..40 {
                    let t = topdown_tile(body, z, tx, tz);
                    assert!(t.solid(), "z={z} ({tx},{tz}) nació hueco: {t:?}");
                }
            }
        }
        // Y el fondo no se toca: el lecho de roca frena al que baja.
        for z in [MIN_Z_LEVEL, MIN_Z_LEVEL - 3] {
            assert_eq!(topdown_tile(body, z, 7, 7), Tile::Bedrock, "z={z}");
        }
    }

    /// **LA MESETA ES TIERRA SOBRE EL PISO BASE, Y CAVARLA CUESTA.**
    ///
    /// Regla del usuario: en la terraza N hay N niveles de tierra antes de llegar
    /// adonde llegarías cavando en la llanura. Antes el subsuelo era plano —un tile
    /// de pico en la cumbre te dejaba a la misma profundidad que uno en la playa—
    /// y el mundo se olvidaba, bajo tierra, de la montaña que tenía encima.
    ///
    /// Se comprueban las tres cosas que la regla promete: la tierra de arriba está,
    /// la piedra empieza donde tiene que empezar, y el lecho de roca sigue estando
    /// a la MISMA profundidad absoluta se cave donde se cave.
    #[test]
    fn cavar_en_la_meseta_cuesta_su_tierra() {
        let body = CelestialBody::Earth;
        let mut vistos = 0;
        for tx in (0..4_000i64).step_by(53) {
            for tz in (-4_000..0i64).step_by(53) {
                let apilada = tierra_de_meseta(body, tx, tz);
                if apilada == 0 {
                    continue;
                }
                vistos += 1;
                // Toda la tierra de la meseta es tierra, no roca ni hueco.
                for z in (-apilada)..0 {
                    let t = topdown_tile(body, z, tx, tz);
                    assert!(
                        t == Tile::Subsoil || matches!(t, Tile::CaveFloor | Tile::Step),
                        "en la terraza {apilada}, z={z} ({tx},{tz}) salió {t:?} en vez de tierra"
                    );
                }
                // Y el lecho está a la misma profundidad ABSOLUTA que en el llano:
                // más abajo en índice, exactamente lo que se apiló arriba.
                assert_eq!(
                    topdown_tile(body, LECHO_DE_ROCA - apilada, tx, tz),
                    Tile::Bedrock,
                    "el lecho se movió en ({tx},{tz}), terraza {apilada}"
                );
                assert_ne!(
                    topdown_tile(body, LECHO_DE_ROCA - apilada + 1, tx, tz),
                    Tile::Bedrock,
                    "el lecho subió de más en ({tx},{tz}), terraza {apilada}"
                );
            }
        }
        assert!(vistos > 100, "la ventana no tenía mesetas que revisar: {vistos}");
    }

    /// **La cueva se ve desde la superficie y se camina hasta el fondo.** Es el
    /// recorrido entero, en el mundo de verdad: boca → cuatro escalones → pasillo.
    /// Si la boca no asomara arriba, la cueva sería inencontrable; si el pasillo
    /// no se caminara, sería una boca ciega.
    #[test]
    fn a_la_cueva_se_entra_desde_la_superficie_caminando() {
        let body = CelestialBody::Earth;
        let c = (0..60i64)
            .find_map(|gx| {
                let c = crate::cuevas::cueva_de(body, gx, 0)?;
                // Sólo sirve una cuya boca no haya tapado una parcela o la calzada
                // (la obra civil manda, ver `topdown_tile`).
                let (bx, bz) = c.boca;
                let t = topdown_tile(body, 0, bx, bz);
                (!t.solid()).then_some(c)
            })
            .expect("alguna cueva con la boca despejada");
        let tramo = c.tramo(body);
        for (x, z, i) in tramo.celdas() {
            assert_eq!(
                topdown_tile(body, 0, x, z),
                Tile::Stairwell,
                "el escalón {i} no asoma en superficie"
            );
            assert!(
                !topdown_tile(body, crate::cuevas::NIVEL, x, z).solid(),
                "el escalón {i} está macizo por dentro"
            );
        }
        // Y desde el pie, todo el eje del pasillo se camina.
        let (ax, az) = crate::escaleras::eje(c.adentro);
        for u in (crate::escaleras::PASOS as i64)..=(crate::escaleras::PASOS as i64 + c.largo) {
            let (x, z) = (c.boca.0 + ax * u, c.boca.1 + az * u);
            assert!(
                !topdown_tile(body, crate::cuevas::NIVEL, x, z).solid(),
                "el pasillo se corta en u={u}"
            );
        }
    }

    #[test]
    fn dir8_maps_cardinals() {
        use std::f32::consts::PI;
        assert_eq!(dir8(0.0), 0); // East
        assert_eq!(dir8(PI / 2.0), 2); // South (+z)
        assert_eq!(dir8(PI), 4); // West
        assert_eq!(dir8(-PI / 2.0), 6); // North (-z)
    }

    #[test]
    fn walks_in_8_directions_no_gravity() {
        let solid = |_tx: i64, _tz: i64| false; // terreno despejado: se prueba el MOVIMIENTO
        let mut b = MovementBody::new(5.0, PLAYER_RADIUS);
        b.x = 100.0;
        b.z = 100.0;
        let mut input = empty_input(1);
        // Move north-east (x+, z-).
        input.move_x = 1.0;
        input.move_z = -1.0;
        let (x0, z0, y0) = (b.x, b.z, b.y);
        for _ in 0..60 {
            step_topdown_movement(&mut b, &input, 0.05, &solid);
        }
        assert!(b.x > x0 + 1.0, "should move east");
        assert!(b.z < z0 - 1.0, "should move north");
        assert_eq!(b.y, y0, "no vertical drift in top-down");
    }

    #[test]
    fn collision_blocks_into_wall() {
        // A solid column at tx=105: walking east must stop before it.
        let solid = |tx: i64, _tz: i64| tx >= 105;
        let mut b = MovementBody::new(5.0, PLAYER_RADIUS);
        b.x = 100.0;
        b.z = 0.0;
        let mut input = empty_input(1);
        input.move_x = 1.0;
        for _ in 0..120 {
            step_topdown_movement(&mut b, &input, 0.05, &solid);
        }
        assert!(b.x < 105.0, "player must not pass into solid tiles, x={}", b.x);
        assert!(b.x > 103.0, "player should reach near the wall, x={}", b.x);
    }

    #[test]
    fn prefab_pattern_and_cost() {
        for p in PREFABS {
            // Door sits on the perimeter (or inner partition) and is walkable.
            assert_eq!(prefab_cell(p, p.door.0, p.door.1), Some(Tile::Door), "{}", p.id);
            // Corners are walls; strictly interior (non-partition) cells are floor.
            assert_eq!(prefab_cell(p, 0, 0), Some(Tile::WoodWall));
            assert_eq!(prefab_cell(p, p.w - 1, p.h - 1), Some(Tile::WoodWall));
            assert_eq!(prefab_cell(p, 1, 1), Some(Tile::WoodFloor), "{}", p.id);
            // Outside the footprint: nothing.
            assert_eq!(prefab_cell(p, -1, 0), None);
            assert_eq!(prefab_cell(p, p.w, 0), None);
            // Cost = walls×2 + doors×3 + floors×1 and is stable/deterministic.
            assert!(prefab_cost(p) > 0);
        }
        // room_small 5×4: perimeter 14 cells − 1 door = 13 walls (26) + door (3)
        // + 6 floors = 35.
        let small = &PREFABS[0];
        assert_eq!((small.w, small.h), (5, 4));
        assert_eq!(prefab_cost(small), 35);
        // casa_1 has an inner doorway connecting both rooms.
        let casa = PREFABS.iter().find(|p| p.id == "casa_1").unwrap();
        assert_eq!(prefab_cell(casa, 5, 3), Some(Tile::Door));
        assert_eq!(prefab_cell(casa, 5, 2), Some(Tile::WoodWall));
    }

    #[test]
    fn tile_serde_roundtrip() {
        // world_edits.json (editor de mundo H0) serializa tiles → deben ir y volver.
        for t in [Tile::Water, Tile::WoodFloor, Tile::Grass, Tile::StoneBrick, Tile::Door] {
            let j = serde_json::to_string(&t).unwrap();
            let back: Tile = serde_json::from_str(&j).unwrap();
            assert_eq!(t, back);
        }
    }

    #[test]
    fn large_assets_catalog_sane() {
        assert!(large_asset_count() >= 1);
        for i in 0..large_asset_count() {
            let a = large_asset(i).unwrap();
            assert!(a.w > 0 && a.h > 0, "{}", a.id);
            assert!(!a.art.is_empty() && a.cost_qty > 0, "{}", a.id);
        }
        assert!(large_asset(large_asset_count()).is_none());
    }

    #[test]
    fn settlements_are_deterministic_and_earth_only() {
        // Same site cell → same site, twice (pure function of the seed).
        for sx in -8..8 {
            for sz in -8..8 {
                assert_eq!(
                    building_site_in(CelestialBody::Earth, sx, sz),
                    building_site_in(CelestialBody::Earth, sx, sz)
                );
                assert!(building_site_in(CelestialBody::Moon, sx, sz).is_none());
                assert!(building_site_in(CelestialBody::Mars, sx, sz).is_none());
            }
        }
        // Find a generated house and sanity-check its tiles + door.
        let mut found = false;
        for sx in -20..20 {
            for sz in -20..20 {
                let Some(site) = building_site_in(CelestialBody::Earth, sx, sz) else {
                    continue;
                };
                found = true;
                let (ox, oz) = site.min;
                // El perímetro es MURO — de madera en una casa, de ladrillo en un
                // hospital: la parcela decide el material según su programa, así
                // que el test comprueba la clase de tile, no una variante concreta.
                let corner = building_cell(CelestialBody::Earth, ox, oz);
                assert!(
                    matches!(
                        corner,
                        Some(Tile::WoodWall | Tile::StoneBrick | Tile::Window | Tile::Door)
                    ),
                    "la esquina del edificio no es pared: {corner:?}"
                );
                // El interior es piso pisable.
                let inside = building_cell(CelestialBody::Earth, ox + 2, oz + 2);
                assert!(
                    inside.is_some_and(|t| !t.solid()),
                    "el interior del edificio no se puede pisar: {inside:?}"
                );
                // Y se entra por la puerta.
                assert!(!solid_at(CelestialBody::Earth, 0, site.door.0, site.door.1));
            }
        }
        assert!(found, "some settlement should generate in a 40×40-site sweep");
    }

    #[test]
    fn new_build_tiles_behave() {
        assert!(Tile::Window.solid());
        assert_eq!(Tile::Window.dug_item(), Some("wood"), "build pieces refund material");
        // **El piso alto es VACÍO, no roca.** Si volviera a ser sólido (bedrock),
        // el segundo piso dejaría de poderse construir: `place_tile` rechaza toda
        // celda sólida. Es la invariante entera de la verticalidad.
        for z in 1..=3 {
            let t = topdown_tile(CelestialBody::Earth, z, 3, 3);
            assert_eq!(t, Tile::Air, "piso {z}");
            assert!(!t.solid(), "el vacio no puede frenar ni bloquear la obra");
            assert!(!t.buildable_ground(), "y tampoco es suelo: sobre el vacio se CAE");
        }
    }

    
    
    #[test]
    fn deposits_are_deterministic_and_deeper_is_richer() {
        let a = deposit_at(CelestialBody::Earth, -5, 42, 17);
        assert_eq!(a, deposit_at(CelestialBody::Earth, -5, 42, 17));
        let deep = (0..80)
            .flat_map(|tx| (0..80).map(move |tz| (tx, tz)))
            .filter(|(tx, tz)| deposit_at(CelestialBody::Earth, -5, *tx, *tz).is_some())
            .count();
        let surf = (0..80)
            .flat_map(|tx| (0..80).map(move |tz| (tx, tz)))
            .filter(|(tx, tz)| deposit_at(CelestialBody::Earth, 0, *tx, *tz).is_some())
            .count();
        assert!(deep > surf, "underground should have more deposits ({deep} vs {surf})");
    }

    #[test]
    fn ninguna_veta_asoma_en_la_ciudad() {
        for body in [CelestialBody::Earth, CelestialBody::Moon, CelestialBody::Mars] {
            for tx in (-3000..3000).step_by(3) {
                for tz in (-3000..3000).step_by(3) {
                    if crate::geography::civil_at(body, tx, tz).is_some() {
                        assert!(deposit_at(body, 0, tx, tz).is_none(), "veta en la ciudad {body:?} ({tx},{tz})");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod memo_tests {
    use super::*;

    #[test]
    fn la_memoria_no_cambia_ninguna_respuesta() {
        for (cx, cz) in [(0i64, 0i64), (-754, -1818), (3000, -900)] {
            for dx in -12..12 {
                for dz in -12..12 {
                    let a = topdown_tile_calc(CelestialBody::Earth, 0, cx + dx, cz + dz);
                    let b = topdown_tile(CelestialBody::Earth, 0, cx + dx, cz + dz);
                    let c = topdown_tile(CelestialBody::Earth, 0, cx + dx, cz + dz);
                    assert_eq!((a, a), (b, c), "({},{})", cx + dx, cz + dz);
                }
            }
        }
    }
}
