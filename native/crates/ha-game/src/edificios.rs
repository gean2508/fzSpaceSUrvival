//! **La cáscara de un edificio, dibujada con el kit modular del pack.**
//!
//! La geometría —planta, contorno, tramos, aberturas— vive en
//! `ha_shared::edificio` y no sabe de sprites. Acá está lo otro: el estado vivo,
//! su persistencia, la paleta con la que se levanta y el dibujo.
//!
//! ## Cómo se lee un edificio en cenital 3/4
//!
//! El pack de Modern Exteriors dibuja **elevaciones frontales**: bandas de
//! [`MODULO`] celdas de ancho que se apilan. Así que la cáscara se planta sobre
//! el **frente** (la cara sur del contorno) y crece hacia arriba en pantalla:
//!
//! ```text
//!   ┌──────────┐  ← azotea (7 celdas, con su parapeto y sus máquinas)
//!   │ ▯ ▯ ▯ ▯ │  ← piso  (4 celdas) × cuantas plantas tenga
//!   │ ▯ ▯ ▯ ▯ │
//!   │ ▯ ▯▐▌▯ ▯ │  ← planta baja (3 celdas, con LA ENTRADA)
//!   ────────────  ← la línea del frente: acá está parado el que mira
//! ```
//!
//! No hay dibujo de costado ni de espalda en el pack, y no se inventa: esas caras
//! las sigue poniendo el sistema de paredes de siempre (`crate::walls`, con los
//! materiales `fachada_*`). Un edificio se mira de frente, igual que en la hoja.
//!
//! ## Por qué la cáscara no son paredes sueltas
//!
//! Porque una banda de 9 celdas es UN dibujo. Colocarla borde por borde obliga a
//! recortarla en columnas de 1 celda, que es exactamente el destrozo que dejó la
//! primera importación del pack (media ventana por pared). Acá el edificio se
//! coloca entero y el dibujo se planta por módulos enteros.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use ha_shared::build_grid::{WallAxis, WallKind};
use ha_shared::edificio::{
    Borde, Cuarto, Edificio, ALTO_AZOTEA, ALTO_PISO, ALTO_PLANTA_BAJA, MODULO,
};
use ha_shared::topdown::TILE_M;

use crate::art2d::ArtLibrary;
use crate::inventory_persist::persist_dir;
use crate::platform::is_app_paused;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::Player;
use crate::world2d::{active_surface_body, render_xy, surface_region_active, ActivePlanet, TileWorld};

/// Offset de índice para los botones de EDIFICIO en la paleta de construir.
///
/// Mismo mecanismo que `fauna::SPAWN_PALETTE_BASE`: `build.rs` lo intercepta
/// antes de `build_selection`, así el índice de paleta de las tablas no se mueve.
pub const PALETTE_BASE: usize = 20_000;

/// Fondo del cuarto que funda "Ambiente", en celdas. El frente son [`MODULO`]
/// porque es lo único que el pack sabe dibujar.
const FONDO_AMBIENTE: i64 = 5;

/// Las acciones de la paleta, en el orden en que se muestran.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Accion {
    /// Funda un edificio nuevo: un ambiente de `MODULO × FONDO_AMBIENTE`.
    Ambiente,
    /// Le pega otro ambiente al edificio apuntado. El tabique sale solo.
    Ampliar,
    /// Una planta más / una menos.
    Apilar,
    Desapilar,
    /// Abre el borde de cáscara más cercano.
    Abrir(WallKind),
    /// Cambia el tema (ladrillo → azul → hormigón).
    Tema,
    /// **Un PLANO entero** (`ha_shared::planos`): cuartos, pisos, puertas, ventanas, piso y muebles
    /// de una vez, pagando los materiales. El índice es el del catálogo.
    Plano(usize),
}

/// La paleta, tal cual se dibuja en la ficha EDIFICIO.
pub const ACCIONES: &[(Accion, &str, &str)] = &[
    (Accion::Ambiente, "Ambiente", "Funda un edificio de 9 de frente donde apuntás"),
    (Accion::Ampliar, "Ampliar", "Le pega otro ambiente; el tabique aparece solo"),
    (Accion::Apilar, "Una planta más", "Sube un piso: la azotea se muda arriba"),
    (Accion::Desapilar, "Una planta menos", "Baja un piso (nunca por debajo de la baja)"),
    (Accion::Abrir(WallKind::Door), "Puerta", "Abre una puerta en la pared apuntada"),
    (Accion::Abrir(WallKind::Doorway), "Vano", "Deja el paso franco, sin hoja"),
    (Accion::Abrir(WallKind::Window), "Ventana", "Abre una ventana"),
    (Accion::Abrir(WallKind::Wall), "Tapiar", "Vuelve a cerrar la abertura apuntada"),
    (Accion::Tema, "Cambiar tema", "Va rotando: ladrillo, azul, hormigón"),
    (Accion::Plano(0), "Plano: Cabaña", "Un cuarto con puerta y ventanas · 40 madera, 10 piedra"),
    (Accion::Plano(1), "Plano: Casa", "Living y dormitorio, dos plantas · 60 madera, 30 piedra"),
    (Accion::Plano(2), "Plano: Casa ancha", "Living y cocina lado a lado · 90 madera, 40 piedra"),
    (Accion::Plano(3), "Plano: Residencia", "Cuatro ambientes, tres plantas · 160 madera, 90 piedra"),
    (Accion::Plano(4), "Plano: Tienda", "Local con depósito y vidriera · 70 madera, 40 piedra"),
    (Accion::Plano(5), "Plano: Torre", "Una torre de cuatro plantas · 80 madera, 60 piedra"),
    (Accion::Plano(6), "Plano: Galpón", "Taller con depósito, dos portones · 90 madera, 50 piedra"),
    (Accion::Plano(7), "Plano: Departamentos", "Tres unidades, cuatro plantas · 200 madera, 160 piedra"),
];

pub fn accion(sub: usize) -> Option<Accion> {
    ACCIONES.get(sub).map(|(a, _, _)| *a)
}

pub fn accion_label(sub: usize) -> &'static str {
    ACCIONES.get(sub).map(|(_, l, _)| *l).unwrap_or("")
}

/// El slot de arte que ilustra cada acción en la paleta. **La tarjeta muestra lo
/// que vas a obtener**: la acción de fundar se ve como una planta baja, la de
/// apilar como un piso, la de abrir como la puerta que abre.
pub fn icono(sub: usize) -> &'static str {
    match accion(sub) {
        Some(Accion::Ambiente) => "edificio/pb_vidrio",
        Some(Accion::Ampliar) => "edificio/local",
        Some(Accion::Apilar) | Some(Accion::Desapilar) => "edificio/piso_ventanal",
        Some(Accion::Abrir(WallKind::Door)) => "edificio/pb_madera",
        Some(Accion::Abrir(WallKind::Doorway)) => "edificio/pb_arco",
        Some(Accion::Abrir(WallKind::Window)) => "edificio/piso_doble",
        Some(Accion::Abrir(WallKind::Wall)) => "edificio/piso_rejilla",
        Some(Accion::Tema) => "edificio/azotea",
        Some(Accion::Plano(i)) => match ha_shared::planos::PLANOS.get(i).map(|p| p.uso) {
            Some(ha_shared::planos::Uso::Comercio) => "edificio/pb_vidrio",
            Some(ha_shared::planos::Uso::Taller) => "edificio/pb_arco",
            Some(ha_shared::planos::Uso::Torre) => "edificio/piso_ventanal",
            _ => "edificio/local",
        },
        None => "edificio/azotea",
    }
}

/// Los tres temas de la hoja, en el orden en que los importó el script.
pub const TEMAS: &[(&str, &str)] = &[
    ("ladrillo", "Ladrillo"),
    ("azul", "Azul"),
    ("hormigon", "Hormigón"),
];

/// Un edificio vivo: dónde está, de qué color y su geometría.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EdificioVivo {
    pub body: u8,
    pub z: i32,
    /// Índice en [`TEMAS`].
    pub tema: usize,
    pub ed: Edificio,
}

impl EdificioVivo {
    fn tema_slug(&self) -> &'static str {
        TEMAS[self.tema % TEMAS.len()].0
    }
}

#[derive(Resource, Default)]
pub struct Edificios {
    pub list: Vec<EdificioVivo>,
    /// Sube con cada cambio; el dibujo se refresca al instante cuando cambia.
    pub rev: u64,
    dirty: bool,
    debounce: f32,
    was_paused: bool,
}

impl Edificios {
    /// El edificio que contiene esa celda, si hay alguno en esta capa y cuerpo.
    pub fn en(&self, body: u8, z: i32, cx: i64, cz: i64) -> Option<usize> {
        self.list
            .iter()
            .position(|e| e.body == body && e.z == z && e.ed.contiene(cx, cz))
    }

    /// El edificio más cercano a esa celda dentro de un radio corto. Es lo que
    /// usan "Ampliar" y las aberturas: apuntás al lado de la casa, no adentro.
    pub fn cerca_de(&self, body: u8, z: i32, cx: i64, cz: i64) -> Option<usize> {
        self.en(body, z, cx, cz).or_else(|| {
            self.list.iter().position(|e| {
                e.body == body
                    && e.z == z
                    && [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .iter()
                        .any(|(dx, dz)| e.ed.contiene(cx + dx, cz + dz))
            })
        })
    }

    pub fn toco(&mut self) {
        self.rev += 1;
        self.dirty = true;
    }

    /// ¿Alguna cáscara frena acá? La cáscara **no** frena por sí misma: las
    /// paredes que colisionan las sigue poniendo `walls`. Esto es sólo para que
    /// el resto del juego pueda preguntar "¿esto es un edificio?".
    pub fn hay_edificio(&self, body: u8, z: i32, cx: i64, cz: i64) -> bool {
        self.en(body, z, cx, cz).is_some()
    }
}

/// Sprite vivo de una banda de la cáscara.
///
/// Los campos son `pub(crate)` para que el driver `HA_CASATEST` pueda medir sobre
/// los sprites VIVOS —que es lo único que prueba que el streaming los puso donde
/// van— y no sobre la función que los calcula.
#[derive(Component)]
pub(crate) struct BandaSprite {
    pub(crate) idx: usize,
    /// Qué módulo del frente es, y qué banda de la pila (0 = planta baja).
    pub(crate) modulo: usize,
    pub(crate) piso: usize,
}

pub struct EdificiosPlugin;

impl Plugin for EdificiosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Edificios>()
            .add_systems(OnEnter(crate::game_phase::GamePhase::Loading), cargar_al_arrancar)
            .add_systems(Update, (dibujar_cascara, sincronizar_tabiques, guardado_debounced));
    }
}

// --- Acciones de la paleta ---------------------------------------------------

/// Aplica la acción sobre la celda apuntada. Devuelve el mensaje para el chip, o
/// `None` si no pasó nada (la acción no aplicaba ahí).
pub fn aplicar(
    eds: &mut Edificios,
    body: u8,
    z: i32,
    cx: i64,
    cz: i64,
    // Punto exacto del mundo, para elegir el borde más cercano al abrir.
    wx: f32,
    wz: f32,
    accion: Accion,
) -> Option<&'static str> {
    match accion {
        Accion::Ambiente => {
            if eds.cerca_de(body, z, cx, cz).is_some() {
                return None; // ahí ya hay casa: eso es Ampliar
            }
            // El cuarto se funda con la celda apuntada en el MEDIO del frente:
            // apuntás a la puerta, no a una esquina que no se ve.
            let cuarto = Cuarto::new(cx - MODULO / 2, cz - FONDO_AMBIENTE + 1, MODULO, FONDO_AMBIENTE);
            eds.list.push(EdificioVivo {
                body,
                z,
                tema: (cx.unsigned_abs() as usize).wrapping_add(cz.unsigned_abs() as usize) % TEMAS.len(),
                ed: Edificio::nuevo(cuarto),
            });
            eds.toco();
            Some("Ambiente fundado")
        }
        Accion::Ampliar => {
            let i = eds.cerca_de(body, z, cx, cz)?;
            let nuevo = cuarto_pegado(&eds.list[i].ed, cx, cz)?;
            if !eds.list[i].ed.ampliar(nuevo) {
                return None;
            }
            eds.toco();
            Some("Ambiente agregado")
        }
        Accion::Apilar => {
            let i = eds.cerca_de(body, z, cx, cz)?;
            eds.list[i].ed.apilar();
            eds.toco();
            Some("Una planta más")
        }
        Accion::Desapilar => {
            let i = eds.cerca_de(body, z, cx, cz)?;
            eds.list[i].ed.desapilar();
            eds.toco();
            Some("Una planta menos")
        }
        Accion::Abrir(kind) => {
            let i = eds.cerca_de(body, z, cx, cz)?;
            let borde = borde_mas_cercano(&eds.list[i].ed, wx, wz)?;
            if !eds.list[i].ed.abrir(borde, kind) {
                return None;
            }
            eds.toco();
            Some(match kind {
                WallKind::Door => "Puerta abierta",
                WallKind::Doorway => "Vano abierto",
                WallKind::Window => "Ventana abierta",
                WallKind::Wall => "Tapiado",
            })
        }
        // Los planos se atienden en `build.rs` (cobran materiales y necesitan el mundo entero).
        Accion::Plano(_) => None,
        Accion::Tema => {
            let i = eds.cerca_de(body, z, cx, cz)?;
            eds.list[i].tema = (eds.list[i].tema + 1) % TEMAS.len();
            eds.toco();
            Some("Tema cambiado")
        }
    }
}

/// Cuántas unidades de la clase de material `clase` (`ceramica`, `metal`…) hay en la mochila.
pub fn unidades_de_clase(inv: &crate::inventory_persist::PlayerInventory, clase: &str) -> u32 {
    inv.items
        .iter()
        .filter(|(i, _)| crate::materias::derivado_de_item(i).map(|(c, _)| c == clase).unwrap_or(false))
        .map(|(_, q)| *q)
        .sum()
}

/// Gasta `n` unidades de esa clase (de las pilas que haya).
pub fn gastar_clase(inv: &mut crate::inventory_persist::PlayerInventory, clase: &str, mut n: u32) {
    let pilas: Vec<(String, u32)> = inv
        .items
        .iter()
        .filter(|(i, _)| crate::materias::derivado_de_item(i).map(|(c, _)| c == clase).unwrap_or(false))
        .map(|(i, q)| (i.clone(), *q))
        .collect();
    for (item, q) in pilas {
        if n == 0 {
            break;
        }
        let t = q.min(n);
        inv.remove(&item, t);
        n -= t;
    }
}

/// **Qué acabado de fachada se puede pagar** con lo que hay en la mochila: ladrillo si hay cerámica
/// suficiente (la cerámica se DESCUBRE, `experimentos.rs`), azul si hay metal, y si no, hormigón (que
/// sale de la piedra). Devuelve el índice en [`TEMAS`] y, si hace falta, `(clase, unidades)`.
pub fn acabado_para(inv: &crate::inventory_persist::PlayerInventory, area: i64, pisos: i64) -> (usize, Option<(&'static str, u32)>) {
    for (slug, clase, por) in ha_shared::planos::ACABADOS {
        let Some(clase) = clase else { continue };
        let n = ha_shared::planos::costo_acabado(area, pisos, por);
        if unidades_de_clase(inv, clase) >= n {
            let idx = TEMAS.iter().position(|(s, _)| *s == slug).unwrap_or(0);
            return (idx, Some((clase, n)));
        }
    }
    (TEMAS.iter().position(|(s, _)| *s == "hormigon").unwrap_or(0), None)
}

/// **Levanta un plano entero** con la celda `(ax, az)` en el medio de su frente. `Err` si no se puede
/// (terreno ocupado, agua, otro edificio encima). No cobra: de eso se encarga quien lo llama.
#[allow(clippy::too_many_arguments)]
pub fn fundar_plano(
    tiles: &mut crate::world2d::TileWorld,
    structures: &mut crate::structures::BuiltStructures,
    eds: &mut Edificios,
    body: u8,
    z: i32,
    ax: i64,
    az: i64,
    plano: &ha_shared::planos::Plano,
    tema: usize,
    semilla: u32,
    // Los constructores de los pueblos sólo levantan sobre terreno natural (no sobre calles ni veredas).
    solo_natural: bool,
) -> Result<usize, &'static str> {
    let Some(ed) = plano.edificio(ax, az) else { return Err("El plano no entra") };
    let Some(cuerpo) = crate::world2d::body_from_u8(body) else { return Err("Cuerpo desconocido") };
    for (x, zz) in ed.celdas() {
        let t = tiles.tile_at(cuerpo, x, zz);
        if t.solid() || t == ha_shared::topdown::Tile::Water || t == ha_shared::topdown::Tile::WaterDeep {
            return Err("El terreno no sirve: hay obstáculos o agua");
        }
        if solo_natural && !matches!(t, ha_shared::topdown::Tile::Grass | ha_shared::topdown::Tile::Dirt | ha_shared::topdown::Tile::Sand) {
            return Err("No se construye sobre la calle");
        }
        if eds.en(body, z, x, zz).is_some() || structures.blocks(cuerpo, z, x, zz) {
            return Err("Ya hay algo construido ahí");
        }
    }
    eds.list.push(EdificioVivo { body, z, tema, ed });
    let i = eds.list.len() - 1;
    eds.toco();
    estampar_piso(tiles, eds, body, z, ax, az);
    let mut puestos = 0;
    for m in plano.muebles(ax, az, semilla) {
        structures.add(crate::structures::PlacedStructure {
            body,
            z,
            cx: m.x0,
            cz: m.z0,
            w: m.cw,
            h: m.prof,
            art: m.key.to_string(),
            // El pie del mueble frena; el respaldo, que se dibuja sobre la pared, no.
            solid: (0..m.cw as u32).map(|c| (c, (m.prof - 1) as u32)).collect(),
            rot: 0,
        });
        puestos += 1;
    }
    Ok(puestos)
}

/// **El piso de adentro** (C8.1): todas las celdas del edificio que está en `(cx, cz)` pasan a
/// `WoodFloor`. Es idempotente y sólo toca celdas que no sean sólidas (`place_tile` lo exige), así
/// que un ambiente nuevo no pisa una pared ni el agua honda. Las paredes de adentro (tabiques) ya las
/// dibuja `walls` desde `Edificio::tabiques`; lo que faltaba era el suelo.
pub fn estampar_piso(
    tiles: &mut crate::world2d::TileWorld,
    eds: &Edificios,
    body: u8,
    z: i32,
    cx: i64,
    cz: i64,
) -> usize {
    let Some(i) = eds.cerca_de(body, z, cx, cz) else { return 0 };
    let Some(cuerpo) = crate::world2d::body_from_u8(body) else { return 0 };
    let mut puestas = 0;
    for (x, zz) in eds.list[i].ed.celdas() {
        if tiles.tile_at(cuerpo, x, zz) == ha_shared::topdown::Tile::WoodFloor {
            continue;
        }
        if tiles.place_tile(cuerpo, x, zz, ha_shared::topdown::Tile::WoodFloor) {
            puestas += 1;
        }
    }
    eprintln!("ha-edificios: piso estampado en {puestas} celdas del edificio {i}");
    puestas
}

/// Dónde pegar el ambiente nuevo: del lado del edificio hacia el que apuntaste.
///
/// Se mira la celda apuntada contra la caja del edificio y se elige el lado. Así
/// "Ampliar" es apuntar y tocar: no hay que elegir el rumbo en un menú.
fn cuarto_pegado(ed: &Edificio, cx: i64, cz: i64) -> Option<Cuarto> {
    let celdas = ed.celdas();
    let (x0, x1) = (celdas.iter().map(|c| c.0).min()?, celdas.iter().map(|c| c.0).max()? + 1);
    let (z0, z1) = (celdas.iter().map(|c| c.1).min()?, celdas.iter().map(|c| c.1).max()? + 1);
    let ancho = x1 - x0;
    // Distancia a cada lado; gana el más cerca de la celda apuntada.
    let lados = [
        (cz - z0, Cuarto::new(x0, z0 - FONDO_AMBIENTE, ancho, FONDO_AMBIENTE)), // norte
        (z1 - 1 - cz, Cuarto::new(x0, z1, ancho, FONDO_AMBIENTE)),              // sur
        (cx - x0, Cuarto::new(x0 - MODULO, z0, MODULO, z1 - z0)),               // oeste
        (x1 - 1 - cx, Cuarto::new(x1, z0, MODULO, z1 - z0)),                    // este
    ];
    lados.into_iter().min_by_key(|(d, _)| *d).map(|(_, c)| c)
}

/// El borde de la cáscara más cercano al punto apuntado.
fn borde_mas_cercano(ed: &Edificio, wx: f32, wz: f32) -> Option<Borde> {
    let mut mejor: Option<(f32, Borde)> = None;
    let bordes = ed
        .contorno()
        .into_iter()
        .map(|(b, _)| b)
        .chain(ed.tabiques());
    for b in bordes {
        let (cx, cz) = match b.axis() {
            WallAxis::H => (b.tx as f32 + 0.5, b.tz as f32),
            WallAxis::V => (b.tx as f32, b.tz as f32 + 0.5),
        };
        let d = (cx - wx).powi(2) + (cz - wz).powi(2);
        if mejor.map(|(m, _)| d < m).unwrap_or(true) {
            mejor = Some((d, b));
        }
    }
    mejor.map(|(_, b)| b)
}

// --- Dibujo ------------------------------------------------------------------

/// Qué variante de planta baja / piso le toca a un módulo. Determinista: la misma
/// esquina da siempre la misma fachada, así que la ciudad no titila al streamear.
pub(crate) fn variante(base: &[&'static str], x: i64, z: i64, modulo: usize) -> &'static str {
    let h = (x.wrapping_mul(73_856_093) ^ z.wrapping_mul(19_349_663) ^ (modulo as i64).wrapping_mul(83_492_791))
        .unsigned_abs() as usize;
    base[h % base.len()]
}

pub(crate) const PLANTAS_BAJAS: &[&str] = &[
    "edificio/pb_vidrio",
    "edificio/pb_cajero",
    "edificio/pb_arco",
    "edificio/pb_madera",
];
pub(crate) const PISOS: &[&str] = &[
    "edificio/piso_ventanal",
    "edificio/piso_rejilla",
    "edificio/piso_balcon",
    "edificio/piso_doble",
];
pub(crate) const AZOTEAS: &[&str] = &["edificio/azotea", "edificio/azotea_clara"];

/// El colisionador del contorno de UN edificio (ver [`dibujar_cascara`]).
#[derive(Component)]
pub struct CascaraSolida;

/// **Las paredes de adentro** (los tabiques) se vuelven paredes de verdad: piezas de borde del
/// kit de madera (pared, vano, puerta, ventana) que se dibujan con su cara de interior, frenan y
/// se pueden abrir. Es idempotente (`BuiltWalls::add` reemplaza el borde) y corre sólo cuando
/// cambia algún edificio (`rev`).
fn sincronizar_tabiques(eds: Res<Edificios>, mut walls: ResMut<crate::walls::BuiltWalls>, mut ultimo: Local<u64>) {
    if *ultimo == eds.rev && eds.rev != 0 {
        return;
    }
    *ultimo = eds.rev;
    let idx = |id: &str| ha_shared::build_grid::EDGE_PIECES.iter().position(|p| p.id == id);
    let (Some(pared), Some(vano), Some(puerta), Some(ventana)) =
        (idx("edge_wall_wood"), idx("edge_doorway_wood"), idx("edge_door_wood"), idx("edge_window"))
    else {
        return;
    };
    for ev in &eds.list {
        for b in ev.ed.tabiques() {
            let kind = ev.ed.aberturas().get(&b).copied().unwrap_or(WallKind::Wall);
            let piece = match kind {
                WallKind::Wall => pared,
                WallKind::Doorway => vano,
                WallKind::Door => puerta,
                WallKind::Window => ventana,
            };
            let w = crate::walls::PlacedWall { body: ev.body, z: ev.z, tx: b.tx, tz: b.tz, axis: b.axis(), piece };
            if walls.at(crate::world2d::body_from_u8(ev.body).unwrap_or(ha_shared::CelestialBody::Earth), ev.z, b.tx, b.tz, b.axis()) != Some(&w) {
                walls.add(w);
            }
        }
    }
}

/// Rectángulos `[x0, z0, x1, z1]` (metros) del contorno que FRENA: pared y ventana sí; puerta y vano,
/// por donde se entra, no.
pub fn rects_del_contorno(ed: &Edificio) -> Vec<[f32; 4]> {
    const GROSOR: f32 = 0.18;
    let mut v = Vec::new();
    for (b, _) in ed.contorno() {
        let kind = ed.aberturas().get(&b).copied().unwrap_or(WallKind::Wall);
        if kind.is_entrance() {
            continue;
        }
        let (x, z) = (b.tx as f32 * TILE_M, b.tz as f32 * TILE_M);
        v.push(match b.axis() {
            WallAxis::H => [x, z - GROSOR, x + TILE_M, z + GROSOR],
            WallAxis::V => [x - GROSOR, z, x + GROSOR, z + TILE_M],
        });
    }
    v
}

/// Cuánto lejos del jugador se dibuja la cáscara, en tiles.
const RADIO: i64 = 48;

#[allow(clippy::too_many_arguments)]
fn dibujar_cascara(
    mut commands: Commands,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    art: Res<ArtLibrary>,
    eds: Res<Edificios>,
    player: Query<&Transform, With<Player>>,
    vivos: Query<Entity, With<BandaSprite>>,
    solidos: Query<Entity, With<CascaraSolida>>,
    mut last_rev: Local<u64>,
    mut last_z: Local<i32>,
) {
    if surface_region_active(&region).is_none() {
        for e in &vivos {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    // La cáscara cambia poco: se redibuja entera sólo cuando alguien la tocó o
    // cuando cambiás de planta. No hay nada que animar frame a frame.
    if *last_rev == eds.rev && *last_z == tiles.z {
        return;
    }
    *last_rev = eds.rev;
    *last_z = tiles.z;
    for e in &vivos {
        commands.entity(e).despawn_recursive();
    }
    for e in &solidos {
        commands.entity(e).despawn_recursive();
    }
    // **El contorno frena** (antes la cáscara era sólo dibujo y se atravesaba): un colisionador por
    // edificio, con los huecos de puertas y vanos abiertos.
    {
        let body = active_surface_body(&ctx, &planet) as u8;
        for ev in eds.list.iter().filter(|e| e.body == body && e.z == tiles.z) {
            commands.spawn((CascaraSolida, crate::building::SolidoDeFachada(rects_del_contorno(&ev.ed))));
        }
    }
    let Ok(player_tf) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet) as u8;
    let ptx = (player_tf.translation.x / TILE_M).floor() as i64;
    let ptz = (-player_tf.translation.y / TILE_M).floor() as i64;

    for (idx, ev) in eds.list.iter().enumerate() {
        if ev.body != body || ev.z != tiles.z {
            continue;
        }
        let tema = ev.tema_slug();
        for tramo in ev.ed.tramos().into_iter().filter(|t| t.cara.es_frente()) {
            if (tramo.tx - ptx).abs() > RADIO || (tramo.tz - ptz).abs() > RADIO {
                continue;
            }
            let (enteros, _resto) = tramo.modulos();
            for m in 0..enteros.max(0) {
                let x0 = tramo.tx + m * MODULO;
                let mut alto = 0i64; // celdas ya apiladas por encima de la línea
                let pila = std::iter::once((PLANTAS_BAJAS, ALTO_PLANTA_BAJA))
                    .chain((0..ev.ed.pisos()).map(|_| (PISOS, ALTO_PISO)))
                    .chain(std::iter::once((AZOTEAS, ALTO_AZOTEA)));
                for (piso, (tabla, celdas)) in pila.enumerate() {
                    // Pisos y azotea iguales en todo el frente (ver `building::spawn_fachada_apilada`).
                    let pieza = if piso == 0 {
                        variante(tabla, x0, tramo.tz, m as usize)
                    } else {
                        variante(tabla, tramo.tx, tramo.tz, 0)
                    };
                    let clave = art.resolved_key(pieza, Some(tema));
                    // La banda mide UNA CELDA DEL PACK POR TILE: es una pieza que
                    // embaldosa a lo largo del frente, igual que una pared
                    // (`ArtLibrary::medida_de_obra`). Con la escala de objeto
                    // (0,95 m/celda) un módulo de 9 daría 8,55 m sobre 9 tiles y
                    // cada edificio se montaría medio metro sobre el de al lado.
                    let size = art.medida_de_obra(
                        &clave,
                        None,
                        Vec2::new(MODULO as f32 * TILE_M, celdas as f32 * TILE_M),
                    );
                    let pie = tramo.tz as f32 * TILE_M;
                    let base = render_xy((x0 as f32 + MODULO as f32 * 0.5) * TILE_M, pie);
                    commands.spawn((
                        BandaSprite { idx, modulo: m as usize, piso },
                        art.sprite_or_color(&clave, color_de_respaldo(piso), size),
                        Transform::from_xyz(
                            base.x,
                            base.y + alto as f32 * TILE_M + size.y * 0.5,
                            crate::layers::fachada(pie),
                        ),
                        // La cáscara tapa lo que tiene delante en pantalla, igual
                        // que una pared alta: si estás debajo, se abre.
                        crate::layers::Overhead::pared(
                            (x0, tramo.tz - alto - celdas),
                            (MODULO, celdas),
                        )
                        .exterior()
                        .con_pie(tramo.tz),
                    ));
                    alto += celdas;
                }
            }
        }
    }
}

/// Color con el que se dibuja una banda sin arte asignado. No es un cuadrado
/// plano por accidente: es el paño del edificio, que es lo que el fallback
/// procedural ya hace en el resto del juego.
pub(crate) fn color_de_respaldo(piso: usize) -> Color {
    if piso == 0 {
        Color::srgb(0.42, 0.28, 0.24) // planta baja: ladrillo oscuro
    } else {
        Color::srgb(0.55, 0.38, 0.32)
    }
}

// --- Persistencia ------------------------------------------------------------

#[derive(Serialize, Deserialize, Default)]
struct EdificiosSave {
    edificios: Vec<EdificioVivo>,
}

fn edificios_path() -> std::path::PathBuf {
    persist_dir().join("edificios.json")
}

fn cargar_al_arrancar(mut eds: ResMut<Edificios>) {
    // **Cada partida arranca con lo SUYO.** Esto corría en `Startup`, antes de
    // elegir partida: leía la carpeta base y lo de las pruebas viejas aparecía en
    // cualquier partida nueva. Ahora corre al entrar a la partida (`Loading`) y
    // parte de cero; ver `partida.rs`.
    let rev = eds.rev;
    *eds = Edificios::default();
    eds.rev = rev.wrapping_add(1);
    let Ok(raw) = std::fs::read_to_string(edificios_path()) else {
        return;
    };
    let Ok(save) = serde_json::from_str::<EdificiosSave>(&raw) else {
        return;
    };
    eds.list = save.edificios;
    eds.rev += 1;
    // Cargar no es construir: sin esto la partida recién abierta se reescribiría
    // sola a los 20 s (mismo criterio que `walls`).
    eds.dirty = false;
}

fn guardado_debounced(time: Res<Time>, mut eds: ResMut<Edificios>) {
    let paused = is_app_paused();
    let flush_now = paused && !eds.was_paused && eds.dirty;
    eds.was_paused = paused;
    if !eds.dirty {
        return;
    }
    eds.debounce += time.delta_secs();
    if !flush_now && eds.debounce < 20.0 {
        return;
    }
    eds.debounce = 0.0;
    eds.dirty = false;
    let save = EdificiosSave { edificios: eds.list.clone() };
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(edificios_path(), json);
    }
}

/// Para el chip de modo construir y los tests: cuántas acciones tiene la ficha.
pub fn accion_count() -> usize {
    ACCIONES.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eds_con_uno() -> Edificios {
        let mut eds = Edificios::default();
        aplicar(&mut eds, 0, 0, 10, 10, 10.5, 10.5, Accion::Ambiente);
        eds
    }

    #[test]
    fn el_contorno_frena_salvo_donde_hay_puerta() {
        let p = ha_shared::planos::plano("casa_doble").unwrap();
        let e = p.edificio(100, 100).unwrap();
        let n_contorno = e.contorno().len();
        let rects = rects_del_contorno(&e);
        // Dos puertas de calle: dos huecos. El vano interior no es contorno.
        assert_eq!(rects.len(), n_contorno - 2, "{} bordes de contorno, {} rects", n_contorno, rects.len());
        // Un rect del frente cae sobre la línea z = 101 (el borde sur de la fila de adelante).
        assert!(rects.iter().any(|r| r[1] < 101.0 && r[3] > 101.0));
    }

    #[test]
    fn los_planos_de_la_paleta_son_los_del_catalogo() {
        for (i, p) in ha_shared::planos::PLANOS.iter().enumerate() {
            let fila = ACCIONES.iter().find(|(a, _, _)| *a == Accion::Plano(i)).expect("falta la tarjeta");
            assert_eq!(fila.1, format!("Plano: {}", p.nombre), "{}", p.id);
            assert!(fila.2.contains(&p.madera.to_string()) && fila.2.contains(&p.piedra.to_string()), "el costo de {} no coincide", p.id);
        }
    }

    #[test]
    fn ambiente_funda_un_modulo_de_frente() {
        let eds = eds_con_uno();
        assert_eq!(eds.list.len(), 1);
        let frentes: Vec<_> = eds.list[0]
            .ed
            .tramos()
            .into_iter()
            .filter(|t| t.cara.es_frente())
            .collect();
        assert_eq!(frentes.len(), 1);
        assert_eq!(frentes[0].modulos(), (1, 0), "un módulo entero, no nueve columnas");
        // Y la celda apuntada queda EN el frente, no en una esquina.
        assert!(eds.list[0].ed.contiene(10, 10));
    }

    #[test]
    fn ambiente_sobre_una_casa_no_funda_otra() {
        let mut eds = eds_con_uno();
        assert_eq!(aplicar(&mut eds, 0, 0, 10, 10, 10.5, 10.5, Accion::Ambiente), None);
        assert_eq!(eds.list.len(), 1);
    }

    #[test]
    fn ampliar_pega_del_lado_al_que_apuntas() {
        let mut eds = eds_con_uno();
        let antes = eds.list[0].ed.cuartos().len();
        // Apuntando al borde norte del edificio.
        let (x0, z0) = (10 - MODULO / 2, 10 - FONDO_AMBIENTE + 1);
        assert!(aplicar(&mut eds, 0, 0, x0 + 1, z0, x0 as f32, z0 as f32, Accion::Ampliar).is_some());
        assert_eq!(eds.list[0].ed.cuartos().len(), antes + 1);
        assert!(!eds.list[0].ed.tabiques().is_empty(), "el tabique sale solo");
    }

    #[test]
    fn abrir_una_puerta_toca_la_cascara_y_no_el_aire() {
        let mut eds = eds_con_uno();
        let frente = eds.list[0]
            .ed
            .tramos()
            .into_iter()
            .find(|t| t.cara.es_frente())
            .expect("hay frente");
        let (bx, bz) = (frente.tx + 4, frente.tz);
        assert!(aplicar(
            &mut eds, 0, 0, bx, bz - 1, bx as f32 + 0.5, bz as f32, Accion::Abrir(WallKind::Door)
        )
        .is_some());
        assert!(eds.list[0].ed.tiene_entrada());
        // Lejos de toda casa no pasa nada.
        assert_eq!(
            aplicar(&mut eds, 0, 0, 900, 900, 900.0, 900.0, Accion::Abrir(WallKind::Door)),
            None
        );
    }

    #[test]
    fn apilar_y_desapilar_no_bajan_de_la_planta_baja() {
        let mut eds = eds_con_uno();
        aplicar(&mut eds, 0, 0, 10, 10, 10.5, 10.5, Accion::Apilar);
        aplicar(&mut eds, 0, 0, 10, 10, 10.5, 10.5, Accion::Apilar);
        assert_eq!(eds.list[0].ed.pisos(), 2);
        for _ in 0..5 {
            aplicar(&mut eds, 0, 0, 10, 10, 10.5, 10.5, Accion::Desapilar);
        }
        assert_eq!(eds.list[0].ed.pisos(), 0);
    }

    /// La variante de cada módulo es estable: dos consultas dan lo mismo, así la
    /// ciudad no titila cuando el streaming la vuelve a dibujar.
    #[test]
    fn la_variante_de_un_modulo_es_estable() {
        for (x, z, m) in [(0i64, 0i64, 0usize), (17, -4, 2), (-31, 900, 1)] {
            assert_eq!(variante(PISOS, x, z, m), variante(PISOS, x, z, m));
        }
        // Y no es siempre la misma: el pack trae cuatro y se usan.
        let usadas: std::collections::BTreeSet<_> =
            (0..64).map(|i| variante(PISOS, i, i * 3, i as usize)).collect();
        assert!(usadas.len() > 1, "todas las fachadas salieron iguales");
    }

    #[test]
    fn cada_accion_de_la_paleta_tiene_nombre_humano() {
        for (i, (_, label, hint)) in ACCIONES.iter().enumerate() {
            assert!(!label.is_empty() && !hint.is_empty(), "acción {i} sin nombre");
            assert!(
                label.chars().next().is_some_and(|c| c.is_uppercase()),
                "«{label}» no es un nombre"
            );
            assert_eq!(accion_label(i), *label);
        }
        assert_eq!(accion_count(), ACCIONES.len());
    }
}
