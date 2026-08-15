//! **Un CUARTO se reconoce solo: cerrás el recinto y ya estás adentro.**
//!
//! El jugador no declara "acá empieza mi casa". Pone piso, levanta las cuatro
//! paredes, deja una puerta — y en ese momento el juego tiene que darse cuenta.
//! Este módulo es ese darse cuenta, y nada más: no dibuja, no mueve la cámara y
//! **no carga ninguna escena**. Devuelve una lista de recintos.
//!
//! ## La regla
//!
//! Un cuarto es un conjunto de celdas con piso construido que cumple dos cosas:
//!
//! 1. **Está cerrado**: desde adentro no se sale a campo abierto sin cruzar una
//!    pieza de borde. Si el relleno se escapa, no hay cuarto — hay un corral a
//!    medio hacer, y tratarlo como interior sería apagarle el sol a un patio.
//! 2. **Tiene por dónde entrar**: al menos una puerta en su contorno. Una caja
//!    sellada sin puerta no es una habitación, es un cajón; y como el jugador no
//!    puede estar adentro, tratarla como interior no le sirve a nadie.
//!
//! ## Por qué un relleno y no "contá cuatro paredes"
//!
//! Porque las casas de verdad no son cuadrados de 1×1. Contar paredes obliga a
//! decidir de antemano la forma (¿y una L?, ¿y un pasillo?, ¿y dos cuartos que
//! comparten tabique?), y cada forma nueva sería código nuevo. El relleno no
//! pregunta la forma: pregunta **si se puede salir**, que es la única propiedad
//! que de verdad define "adentro". Una L, un pasillo y dos cuartos pegados salen
//! gratis, y el tabique compartido parte el recinto en dos sin ninguna regla
//! extra.
//!
//! ## Continuidad (biblia §4)
//!
//! Que el recinto pase a ser cuarto **no es una transición**: no hay fundido, ni
//! pantalla, ni teletransporte al "interior". Lo único que cambia es que las
//! celdas quedan marcadas, y quien quiera reaccionar a eso —la luz, el ambiente,
//! el mobiliario— lo hace en gradiente por su cuenta. Entrar y salir por la
//! puerta es caminar.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;

use ha_shared::build_grid::{is_build_floor, WallAxis, WallKind};
use ha_shared::CelestialBody;

use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::walls::BuiltWalls;
use crate::world2d::{active_surface_body, surface_region_active, ActivePlanet, TileWorld};

/// Tope de celdas de un recinto. Un cuarto es una habitación, no una provincia:
/// pasado esto el relleno se corta y se decide que no hay cuarto.
///
/// Existe por coste, pero sobre todo por sentido: si alguien cerca media isla con
/// una empalizada, eso es un CAMPO CERRADO y no un interior — apagarle el cielo
/// sería exactamente el error que este módulo tiene que no cometer.
const MAX_CELDAS_CUARTO: usize = 1024;

/// Radio (tiles) alrededor del jugador donde se buscan cuartos. Lo que se puede
/// llegar a pisar en el rato entre dos análisis.
const RADIO_BUSQUEDA: i64 = 40;

/// Cada cuánto se rehace el análisis (s) si nada cambió. Colocar o quitar una
/// pieza lo dispara al instante igual (se mira `BuiltWalls::rev`).
const PERIODO: f32 = 0.5;

/// Un recinto cerrado con puerta.
#[derive(Clone, Debug, PartialEq)]
pub struct Cuarto {
    pub body: u8,
    pub z: i32,
    /// Las celdas de adentro.
    pub celdas: HashSet<(i64, i64)>,
    /// Los bordes con puerta de su contorno: por dónde se entra y se sale.
    pub puertas: Vec<(i64, i64, WallAxis)>,
}

impl Cuarto {
    pub fn contiene(&self, tx: i64, tz: i64) -> bool {
        self.celdas.contains(&(tx, tz))
    }

    pub fn area(&self) -> usize {
        self.celdas.len()
    }
}

/// Por qué una celda no llegó a ser cuarto. Sirve para poder DECIRLO (un HUD que
/// avisa "te falta la puerta" es la diferencia entre una mecánica y un misterio).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoEsCuarto {
    /// El relleno se escapó a campo abierto: falta cerrar.
    Abierto,
    /// Cerrado, pero sin ninguna puerta en el contorno.
    SinPuerta,
    /// Demasiado grande para ser una habitación.
    Enorme,
    /// Ahí no hay piso construido.
    SinPiso,
}

/// **El análisis, puro.** Toma dos preguntas y devuelve el recinto.
///
/// - `hay_piso(x, z)`: ¿esta celda tiene piso construido?
/// - `borde(x, z, eje)`: la pieza que ocupa ese borde, si hay alguna.
///
/// Se pasan como cierres a propósito: así el algoritmo se prueba con un mapa de
/// juguete, sin `TileWorld` ni `BuiltWalls` ni un mundo cargado.
pub fn analizar(
    inicio: (i64, i64),
    hay_piso: impl Fn(i64, i64) -> bool,
    borde: impl Fn(i64, i64, WallAxis) -> Option<WallKind>,
) -> Result<(HashSet<(i64, i64)>, Vec<(i64, i64, WallAxis)>), NoEsCuarto> {
    if !hay_piso(inicio.0, inicio.1) {
        return Err(NoEsCuarto::SinPiso);
    }
    let mut dentro: HashSet<(i64, i64)> = HashSet::new();
    let mut cola: VecDeque<(i64, i64)> = VecDeque::new();
    let mut puertas: Vec<(i64, i64, WallAxis)> = Vec::new();
    dentro.insert(inicio);
    cola.push_back(inicio);

    while let Some((x, z)) = cola.pop_front() {
        if dentro.len() > MAX_CELDAS_CUARTO {
            return Err(NoEsCuarto::Enorme);
        }
        // Los cuatro bordes de la celda, con el vecino que hay del otro lado.
        // (Ver `walls::BuiltWalls::any_edge_of_cell` para la misma convención.)
        let lados = [
            ((x, z, WallAxis::H), (x, z - 1)),     // norte
            ((x, z + 1, WallAxis::H), (x, z + 1)), // sur
            ((x, z, WallAxis::V), (x - 1, z)),     // oeste
            ((x + 1, z, WallAxis::V), (x + 1, z)), // este
        ];
        for ((bx, bz, eje), (nx, nz)) in lados {
            match borde(bx, bz, eje) {
                // Hay pieza: es contorno. No se cruza — ni siquiera la puerta:
                // una habitación con la puerta abierta sigue siendo un adentro.
                Some(kind) => {
                    // Puerta o VANO: las dos son por dónde se entra
                    // (`WallKind::is_entrance`). Un cuarto cuya única abertura es
                    // un marco sin hoja es un cuarto igual — se cruza andando, y
                    // por ahí entra la luz.
                    if kind.is_entrance() {
                        puertas.push((bx, bz, eje));
                    }
                }
                // Borde libre: se sigue si del otro lado hay piso construido.
                None => {
                    if !hay_piso(nx, nz) {
                        // Se sale a campo abierto sin cruzar nada: no está cerrado.
                        return Err(NoEsCuarto::Abierto);
                    }
                    if dentro.insert((nx, nz)) {
                        cola.push_back((nx, nz));
                    }
                }
            }
        }
    }
    if puertas.is_empty() {
        return Err(NoEsCuarto::SinPuerta);
    }
    // Orden estable por su clave (`WallAxis` no es `Ord`): sin esto, dos análisis
    // del mismo cuarto pueden devolver las puertas en distinto orden y el
    // `!=` que decide si hay que reindexar daría siempre "cambió".
    puertas.sort_unstable_by_key(|&(x, z, e)| (x, z, e.as_u8()));
    puertas.dedup_by_key(|&mut (x, z, e)| (x, z, e.as_u8()));
    Ok((dentro, puertas))
}

/// Los cuartos que existen ahora mismo cerca del jugador.
#[derive(Resource, Default)]
pub struct Cuartos {
    list: Vec<Cuarto>,
    /// Celda → índice del cuarto que la contiene. Es la consulta caliente (la luz
    /// y el ambiente preguntan "¿estoy adentro?" por frame).
    por_celda: HashMap<(u8, i32, i64, i64), usize>,
}

impl Cuartos {
    pub fn list(&self) -> &[Cuarto] {
        &self.list
    }

    /// El cuarto que contiene esta celda, si hay alguno.
    pub fn en(&self, body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<&Cuarto> {
        let i = *self.por_celda.get(&(body as u8, z, tx, tz))?;
        self.list.get(i)
    }

    /// ¿Esta celda es interior? La pregunta que hacen la luz y el ambiente.
    pub fn es_interior(&self, body: CelestialBody, z: i32, tx: i64, tz: i64) -> bool {
        self.por_celda.contains_key(&(body as u8, z, tx, tz))
    }

    fn reemplazar(&mut self, cuartos: Vec<Cuarto>) {
        self.por_celda.clear();
        for (i, c) in cuartos.iter().enumerate() {
            for &(x, z) in &c.celdas {
                self.por_celda.insert((c.body, c.z, x, z), i);
            }
        }
        self.list = cuartos;
    }
}

pub struct CuartosPlugin;

impl Plugin for CuartosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Cuartos>()
            .add_systems(Update, detectar.run_if(crate::game_phase::world_active));
    }
}

/// Rehace el mapa de cuartos alrededor del jugador.
///
/// **Se recorre lo construido, no el mundo.** Las celdas candidatas salen de las
/// paredes que hay (`BuiltWalls`), así que un planeta sin nada construido cuesta
/// cero: no hay rellenos que lanzar. Es la misma idea que hace barato al
/// streaming de paredes.
fn detectar(
    time: Res<Time>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    built: Res<BuiltWalls>,
    player: Query<&Transform, With<crate::sim::Player>>,
    mut cuartos: ResMut<Cuartos>,
    mut acc: Local<f32>,
    mut last_rev: Local<u64>,
    mut last_z: Local<i32>,
) {
    if surface_region_active(&region).is_none() {
        if !cuartos.list.is_empty() {
            cuartos.reemplazar(Vec::new());
        }
        return;
    }
    *acc += time.delta_secs();
    let cambio = *last_rev != built.rev() || *last_z != tiles.z;
    if *acc < PERIODO && !cambio {
        return;
    }
    *acc = 0.0;
    *last_rev = built.rev();
    *last_z = tiles.z;

    let Ok(player_tf) = player.get_single() else { return };
    let body = active_surface_body(&ctx, &planet);
    let z = tiles.z;
    let tile = ha_shared::topdown::TILE_M;
    let ptx = (player_tf.translation.x / tile).floor() as i64;
    let ptz = (-player_tf.translation.y / tile).floor() as i64;

    let hay_piso = |x: i64, zz: i64| is_build_floor(tiles.tile_at(body, x, zz));
    let borde = |x: i64, zz: i64, eje: WallAxis| {
        built.at(body, z, x, zz, eje).map(|w| w.def().kind)
    };

    // Semillas: las celdas que tocan una pared cercana. Un cuarto siempre tiene
    // al menos una, así que no se pierde ninguno, y se lanzan muchísimos menos
    // rellenos que barriendo el cuadrado entero.
    let mut vistas: HashSet<(i64, i64)> = HashSet::new();
    let mut encontrados: Vec<Cuarto> = Vec::new();
    for w in built.list.iter() {
        if w.body != body as u8
            || w.z != z
            || (w.tx - ptx).abs() > RADIO_BUSQUEDA
            || (w.tz - ptz).abs() > RADIO_BUSQUEDA
        {
            continue;
        }
        let [a, b] = ha_shared::build_grid::edge_cells(w.tx, w.tz, w.axis);
        for semilla in [a, b] {
            if vistas.contains(&semilla) || !hay_piso(semilla.0, semilla.1) {
                continue;
            }
            match analizar(semilla, hay_piso, borde) {
                Ok((celdas, puertas)) => {
                    vistas.extend(celdas.iter().copied());
                    encontrados.push(Cuarto { body: body as u8, z, celdas, puertas });
                }
                Err(_) => {
                    // Una semilla que no cierra no se vuelve a intentar desde la
                    // misma celda, pero SÍ desde otra: dos recintos distintos
                    // pueden compartir pared.
                    vistas.insert(semilla);
                }
            }
        }
    }
    // Sólo se reindexa si de verdad cambió algo: reconstruir el mapa por celda
    // cada medio segundo con la casa quieta es trabajo puro.
    if encontrados != cuartos.list {
        cuartos.reemplazar(encontrados);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mapa de juguete: piso en un rectángulo y paredes en los bordes que se le
    /// declaren. Es todo lo que el algoritmo necesita saber del mundo.
    struct Mapa {
        piso: HashSet<(i64, i64)>,
        bordes: HashMap<(i64, i64, u8), WallKind>,
    }

    impl Mapa {
        /// Un cuarto rectangular con piso y las cuatro paredes cerradas.
        fn caja(x0: i64, z0: i64, x1: i64, z1: i64) -> Self {
            let mut piso = HashSet::new();
            let mut bordes = HashMap::new();
            for x in x0..=x1 {
                for z in z0..=z1 {
                    piso.insert((x, z));
                }
            }
            for x in x0..=x1 {
                bordes.insert((x, z0, WallAxis::H.as_u8()), WallKind::Wall);
                bordes.insert((x, z1 + 1, WallAxis::H.as_u8()), WallKind::Wall);
            }
            for z in z0..=z1 {
                bordes.insert((x0, z, WallAxis::V.as_u8()), WallKind::Wall);
                bordes.insert((x1 + 1, z, WallAxis::V.as_u8()), WallKind::Wall);
            }
            Mapa { piso, bordes }
        }

        fn poner(&mut self, x: i64, z: i64, eje: WallAxis, kind: WallKind) {
            self.bordes.insert((x, z, eje.as_u8()), kind);
        }

        fn quitar(&mut self, x: i64, z: i64, eje: WallAxis) {
            self.bordes.remove(&(x, z, eje.as_u8()));
        }

        fn correr(&self, inicio: (i64, i64)) -> Result<(HashSet<(i64, i64)>, Vec<(i64, i64, WallAxis)>), NoEsCuarto> {
            analizar(
                inicio,
                |x, z| self.piso.contains(&(x, z)),
                |x, z, e| self.bordes.get(&(x, z, e.as_u8())).copied(),
            )
        }
    }

    /// **El caso que pidió el jugador**: cuatro paredes y una puerta → cuarto.
    #[test]
    fn cuatro_paredes_y_una_puerta_son_un_cuarto() {
        let mut m = Mapa::caja(10, 10, 12, 12);
        m.poner(11, 13, WallAxis::H, WallKind::Door); // puerta al sur
        let (celdas, puertas) = m.correr((11, 11)).expect("tiene que ser cuarto");
        assert_eq!(celdas.len(), 9, "un 3×3 son nueve celdas");
        assert_eq!(puertas, vec![(11, 13, WallAxis::H)]);
    }

    /// Sin puerta es un cajón, no una habitación: no se puede estar adentro.
    #[test]
    fn una_caja_sellada_no_es_un_cuarto() {
        let m = Mapa::caja(10, 10, 12, 12);
        assert_eq!(m.correr((11, 11)), Err(NoEsCuarto::SinPuerta));
    }

    /// **Un hueco en la pared lo abre.** Es la mitad que impide apagarle el sol a
    /// un patio: si falta una pieza, el relleno se escapa y no hay interior.
    #[test]
    fn un_recinto_con_un_hueco_no_cierra() {
        let mut m = Mapa::caja(10, 10, 12, 12);
        m.poner(11, 13, WallAxis::H, WallKind::Door);
        m.quitar(10, 10, WallAxis::H); // falta una pieza del muro norte
        assert_eq!(m.correr((11, 11)), Err(NoEsCuarto::Abierto));
    }

    /// La puerta CIERRA el recinto aunque se cruce caminando. Si el relleno la
    /// atravesara, ninguna casa con puerta sería nunca un interior — que es el
    /// bug más fácil de escribir en este algoritmo.
    #[test]
    fn la_puerta_no_deja_escapar_el_relleno() {
        let mut m = Mapa::caja(10, 10, 12, 12);
        m.poner(11, 13, WallAxis::H, WallKind::Door);
        // Piso también AFUERA, pegado a la puerta: si el relleno cruzara, se
        // llevaría medio mundo por delante.
        let mut m = Mapa { piso: m.piso, bordes: m.bordes };
        for x in 0..40 {
            for z in 13..20 {
                m.piso.insert((x, z));
            }
        }
        let (celdas, _) = m.correr((11, 11)).expect("sigue siendo cuarto");
        assert_eq!(celdas.len(), 9, "el relleno se escapó por la puerta");
    }

    /// Una ventana también cierra (se ve a través, no se cruza), pero no habilita
    /// el cuarto por sí sola: por una ventana no se entra.
    #[test]
    fn una_ventana_cierra_pero_no_es_una_entrada() {
        let mut m = Mapa::caja(10, 10, 12, 12);
        m.poner(11, 13, WallAxis::H, WallKind::Window);
        assert_eq!(m.correr((11, 11)), Err(NoEsCuarto::SinPuerta));
    }

    /// **Un VANO también es entrada.** Es la pieza nueva —la pared con el hueco de
    /// la puerta— y sin esto una casa terminada con vanos en vez de puertas no
    /// sería nunca un interior: ni se le apagaría la luz ni contaría como cuarto.
    #[test]
    fn un_vano_alcanza_para_que_sea_un_cuarto() {
        let mut m = Mapa::caja(10, 10, 12, 12);
        m.poner(11, 13, WallAxis::H, WallKind::Doorway);
        let (celdas, puertas) = m.correr((11, 11)).expect("el vano es una entrada");
        assert_eq!(celdas.len(), 9, "el relleno se escapó por el vano");
        assert_eq!(puertas, vec![(11, 13, WallAxis::H)]);
    }

    /// **La forma no importa.** Una L cierra igual que un cuadrado — es la razón
    /// de ser del relleno frente a contar cuatro paredes.
    #[test]
    fn un_cuarto_en_l_tambien_cierra() {
        // Piso en L: la fila z=10 de x=10..12, más la columna x=10 de z=11..12.
        let mut piso = HashSet::new();
        for x in 10..=12 {
            piso.insert((x, 10));
        }
        for z in 11..=12 {
            piso.insert((10, z));
        }
        let mut bordes = HashMap::new();
        // Contorno exacto de la L: todo borde entre una celda de piso y una que
        // no lo es lleva pieza.
        for &(x, z) in &piso {
            for ((bx, bz, e), (nx, nz)) in [
                ((x, z, WallAxis::H), (x, z - 1)),
                ((x, z + 1, WallAxis::H), (x, z + 1)),
                ((x, z, WallAxis::V), (x - 1, z)),
                ((x + 1, z, WallAxis::V), (x + 1, z)),
            ] {
                if !piso.contains(&(nx, nz)) {
                    bordes.insert((bx, bz, e.as_u8()), WallKind::Wall);
                }
            }
        }
        let mut m = Mapa { piso, bordes };
        m.poner(10, 13, WallAxis::H, WallKind::Door);
        let (celdas, _) = m.correr((10, 12)).expect("una L es un cuarto");
        assert_eq!(celdas.len(), 5);
    }

    /// Cercar media isla no es construir una casa: pasado el tope no hay cuarto,
    /// así que nadie le apaga el cielo a un campo cerrado.
    #[test]
    fn un_recinto_enorme_no_es_un_interior() {
        let n = 40; // 40×40 = 1600 > MAX_CELDAS_CUARTO
        let mut piso = HashSet::new();
        for x in 0..n {
            for z in 0..n {
                piso.insert((x, z));
            }
        }
        let m = Mapa { piso, bordes: HashMap::new() };
        // Sin bordes el relleno se escapa; con tope alcanzado, se corta antes.
        assert!(matches!(
            m.correr((20, 20)),
            Err(NoEsCuarto::Abierto) | Err(NoEsCuarto::Enorme)
        ));
    }

    /// Dos cuartos que comparten tabique son DOS cuartos, no uno: el relleno se
    /// para en la pared medianera sin que haya que decirle nada.
    #[test]
    fn un_tabique_parte_el_recinto_en_dos() {
        let mut m = Mapa::caja(10, 10, 14, 12);
        m.poner(11, 13, WallAxis::H, WallKind::Door);
        m.poner(13, 13, WallAxis::H, WallKind::Door);
        // Tabique en x = 12, de arriba abajo.
        for z in 10..=12 {
            m.poner(12, z, WallAxis::V, WallKind::Wall);
        }
        let (oeste, _) = m.correr((10, 11)).expect("el de la izquierda");
        let (este, _) = m.correr((13, 11)).expect("el de la derecha");
        assert_eq!(oeste.len(), 6, "2 columnas × 3 filas");
        assert_eq!(este.len(), 9);
        assert!(oeste.is_disjoint(&este), "los dos cuartos se solapan");
    }
}
