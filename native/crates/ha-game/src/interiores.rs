//! **Entrar a un edificio** (cliente). Pedido del usuario, 4 oct 2026, estilo GTA San Andreas: el
//! interior es un espacio aparte (`ha_shared::interiores`), no se ve nada del exterior mientras estás
//! adentro, y las casas pueden ser más grandes por dentro que la huella exterior (hasta 1,5×).
//!
//! * **La puerta se marca exacto**: sobre cada puerta de fachada cercana flota una flecha amarilla.
//! * **Botón propio** ENTRAR / SALIR (no compite con USAR: con un jamón en la mano igual aparece).
//! * **Puerta con vida**: cerrada con llave, los infectados y la policía la golpean hasta romperla. Se
//!   refuerza por niveles (madera → reforzada → hierro → blindada, que ya no se rompe) a un costo real.
//!   Rota, la fachada queda sin puerta hasta que la reponés.
//! * **Te siguen**: mientras estás adentro, los hostiles se juntan en la puerta de afuera; si la puerta está
//!   abierta o rota, entran tras de vos.
//! * **Gente adentro**: los residentes duermen de noche en sus camas y circulan de día.
//!
//! Nada de esto se guarda: el interior sale del lote (pura), y el estado de las puertas es de la sesión.

use std::collections::HashMap;

use bevy::prelude::*;
use ha_shared::interiores::{self, Lote};
use ha_shared::topdown::Tile;
use ha_shared::CelestialBody;

use crate::building::PuertaDeFachada;
use crate::game_phase::GamePhase;
use crate::infectados::Infectado;
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::policia_pie::PoliciaPie;
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, PlayerVisual, SimState};
use crate::sky2d::DayClock;
use crate::world2d::{active_surface_body, render_xy, sort_z, ActivePlanet, TileWorld};

/// A cuánto de la puerta se puede pulsar ENTRAR.
const ALCANCE_PUERTA: f32 = 3.0;
/// Hasta dónde se dibuja la flecha de una puerta.
const ALCANCE_FLECHA: f32 = 16.0;
/// Radio alrededor de la puerta donde los hostiles asedian.
const RADIO_ASEDIO: f32 = 2.6;

/// Vida de la puerta según su nivel (`f32::INFINITY` = blindada).
pub fn vida_de_puerta(nivel: u8) -> f32 {
    match nivel {
        0 => 40.0,
        1 => 120.0,
        2 => 400.0,
        _ => f32::INFINITY,
    }
}

/// Nombre del nivel de la puerta.
pub fn nombre_de_nivel(nivel: u8) -> &'static str {
    match nivel {
        0 => "madera",
        1 => "reforzada",
        2 => "de hierro",
        _ => "blindada",
    }
}

/// Lo que cuesta pasar de `nivel` al siguiente: `(ítem, cantidad)` en una lista. `None` si ya es blindada.
pub fn costo_de_mejora(nivel: u8) -> Option<&'static [(&'static str, u32)]> {
    match nivel {
        0 => Some(&[("wood", 10)]),
        1 => Some(&[("iron_ingot", 6)]),
        2 => Some(&[("iron_ingot", 10), ("gold_ingot", 5)]),
        _ => None,
    }
}

/// Lo que cuesta reponer una puerta rota (siempre vuelve de madera).
pub const COSTO_REPOSICION: (&str, u32) = ("wood", 6);

/// Una puerta de edificio: su estado de sesión.
#[derive(Clone, Copy, Debug)]
pub struct Puerta {
    pub cerrada: bool,
    pub nivel: u8,
    pub vida: f32,
    pub rota: bool,
}

impl Default for Puerta {
    fn default() -> Self {
        Self { cerrada: false, nivel: 0, vida: vida_de_puerta(0), rota: false }
    }
}

impl Puerta {
    /// Daño por segundo recibido de hostiles. Una blindada no recibe nada.
    pub fn golpear(&mut self, dano: f32) -> bool {
        if self.rota || self.nivel >= 3 {
            return false;
        }
        self.vida -= dano;
        if self.vida <= 0.0 {
            self.rota = true;
            self.cerrada = false;
            self.vida = 0.0;
            return true;
        }
        false
    }

    pub fn mejorar(&mut self) -> bool {
        if self.rota || self.nivel >= 3 {
            return false;
        }
        self.nivel += 1;
        self.vida = vida_de_puerta(self.nivel);
        true
    }

    pub fn reponer(&mut self) {
        *self = Puerta::default();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Dentro {
    pub slot: u32,
    pub lote: Lote,
    /// Dónde salís: afuera de la puerta, en el mundo exterior.
    pub salida: (f32, f32),
    /// El centro de la puerta de afuera (donde se juntan los hostiles).
    pub puerta_ext: (f32, f32),
}

#[derive(Clone, Copy, Debug)]
pub enum Cerca {
    Entrada { lote: Lote, puerta: (f32, f32) },
    Salida,
}

#[derive(Resource, Default)]
pub struct Interiores {
    pub dentro: Option<Dentro>,
    pub puertas: HashMap<(u8, i64, i64), Puerta>,
    pub cerca: Option<Cerca>,
    pub aviso: Option<(String, f32)>,
}

impl Interiores {
    pub fn puerta_de(&mut self, l: &Lote) -> &mut Puerta {
        self.puertas.entry((l.body, l.x, l.z)).or_default()
    }

    fn avisar(&mut self, t: impl Into<String>) {
        self.aviso = Some((t.into(), 3.0));
    }
}

/// Hacia dónde van los hostiles: el jugador, o la puerta de afuera si estás encerrado.
#[derive(Resource, Default)]
pub struct ObjetivoHostil {
    pub activo: bool,
    pub puerta: (f32, f32),
    /// Origen del espacio interior (para saber quién ya está adentro).
    pub origen: (f32, f32),
}

impl ObjetivoHostil {
    /// ¿Esa posición es de adentro?
    pub fn esta_adentro(&self, x: f32, z: f32) -> bool {
        (x - self.origen.0).abs() < 400.0 && (z - self.origen.1).abs() < 400.0
    }

    /// Dónde apunta un hostil en `(x, z)` si el jugador está en `jug`.
    pub fn objetivo(&self, x: f32, z: f32, jug: (f32, f32)) -> (f32, f32) {
        if self.activo && !self.esta_adentro(x, z) {
            self.puerta
        } else {
            jug
        }
    }
}

#[derive(Component)]
struct Flecha {
    tx: i64,
    tz: i64,
    salida: bool,
}

#[derive(Component)]
struct PanelPuerta;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum AccionPuerta {
    Pasar,
    Llave,
    Reforzar,
    Reponer,
}

#[derive(Component)]
struct TextoPanel(AccionPuerta);

#[derive(Component)]
struct TextoAviso;

#[derive(Component)]
pub struct Ocupante {
    slot: u32,
    piso: i32,
    dormido: bool,
    base: Vec2,
    fase: f32,
}

pub struct InterioresPlugin;

impl Plugin for InterioresPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Interiores>()
            .init_resource::<ObjetivoHostil>()
            .add_systems(Startup, crear_panel)
            .add_systems(OnEnter(GamePhase::Loading), cargar_puertas)
            .add_systems(Update, guardar_puertas.run_if(in_state(GamePhase::Playing)))
            .add_systems(
                Update,
                (
                    detectar_puerta,
                    flechas,
                    panel_de_puerta,
                    atender_botones,
                    publicar_objetivo,
                    asedio_de_puertas,
                    ocupantes,
                    ocultar_puertas_rotas,
                )
                    .chain()
                    .run_if(in_state(GamePhase::Playing)),
            )
            .add_systems(
                Update,
                append_interior_touch_blocks
                    .after(crate::ui_editor::append_editor_touch_blocks)
                    .before(crate::hud::append_hud_touch_blocks),
            )
            .add_systems(OnExit(GamePhase::Playing), salir_de_partida);
    }
}

// ---------------------------------------------------------------------------
// Guardado: el estado de las puertas (cerrada, nivel, vida, rota) sobrevive a la sesión
// ---------------------------------------------------------------------------

fn ruta_puertas() -> std::path::PathBuf {
    crate::inventory_persist::persist_dir().join("puertas_edificios.json")
}

type FilaPuerta = (u8, i64, i64, bool, u8, f32, bool);

fn cargar_puertas(mut int: ResMut<Interiores>) {
    int.puertas.clear();
    let Ok(raw) = std::fs::read_to_string(ruta_puertas()) else { return };
    if let Ok(filas) = serde_json::from_str::<Vec<FilaPuerta>>(&raw) {
        for (b, x, z, cerrada, nivel, vida, rota) in filas {
            int.puertas.insert((b, x, z), Puerta { cerrada, nivel, vida: if vida.is_finite() { vida } else { 1.0e9 }, rota });
        }
    }
}

fn guardar_puertas(time: Res<Time>, int: Res<Interiores>, mut acc: Local<f32>, mut ultimo: Local<String>) {
    *acc += time.delta_secs();
    if *acc < 4.0 {
        return;
    }
    *acc = 0.0;
    let mut filas: Vec<FilaPuerta> = int
        .puertas
        .iter()
        .filter(|(_, p)| p.cerrada || p.nivel > 0 || p.rota)
        .map(|(&(b, x, z), p)| (b, x, z, p.cerrada, p.nivel, if p.vida.is_finite() { p.vida } else { 1.0e9 }, p.rota))
        .collect();
    filas.sort_by_key(|f| (f.0, f.1, f.2));
    let Ok(json) = serde_json::to_string(&filas) else { return };
    if *ultimo == json {
        return;
    }
    if let Some(dir) = ruta_puertas().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::write(ruta_puertas(), &json).is_ok() {
        *ultimo = json;
    }
}

fn salir_de_partida(mut commands: Commands, mut int: ResMut<Interiores>, q: Query<Entity, Or<(With<Flecha>, With<Ocupante>)>>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
    *int = Interiores::default();
    interiores::reiniciar();
}

// ---------------------------------------------------------------------------
// Qué puerta tengo cerca
// ---------------------------------------------------------------------------

fn lote_de_puerta(body: CelestialBody, p: &PuertaDeFachada) -> Lote {
    // La puerta está en la fila del frente del edificio: el rect sale de la ciudad.
    match ha_shared::amueblado::casa_en(body, p.tx, p.tz) {
        Some((x, z, w, h, clave)) => Lote { body: body as u8, x, z, w, h, clave },
        None => Lote {
            body: body as u8,
            x: p.tx - 6,
            z: p.tz - 7,
            w: 13,
            h: 8,
            clave: interiores::clave_estatica(&p.art),
        },
    }
}

/// Una puerta de edificio a la vista: la celda y el edificio al que pertenece.
#[derive(Clone, Copy)]
struct PuertaVista {
    tx: i64,
    tz: i64,
    lote: Lote,
}

/// Todas las puertas a menos de `radio` de `(px, pz)`: las de las fachadas de la ciudad (`PuertaDeFachada`) y las
/// de las casas que colocó alguien (`BuiltStructures`: el jugador o un vecino constructor). La puerta de una casa
/// colocada está en el medio de su frente, que es donde el dibujo del pack la pinta.
fn puertas_vistas(
    body: CelestialBody,
    px: f32,
    pz: f32,
    radio: f32,
    fachadas: &Query<&PuertaDeFachada>,
    estructuras: &crate::structures::BuiltStructures,
) -> Vec<PuertaVista> {
    let mut v = Vec::new();
    for p in fachadas.iter() {
        let (cx, cz) = (p.tx as f32 + 0.5, p.tz as f32 + 0.5);
        if (cx - px).hypot(cz - pz) < radio {
            v.push(PuertaVista { tx: p.tx, tz: p.tz, lote: lote_de_puerta(body, p) });
        }
    }
    for s in &estructuras.list {
        if s.body != body as u8 || s.z != 0 || !es_edificio_enterable(&s.art, s.w, s.h) {
            continue;
        }
        let (tx, tz) = (s.cx + s.w / 2, s.cz + s.h - 1);
        if ((tx as f32 + 0.5) - px).hypot((tz as f32 + 0.5) - pz) < radio {
            v.push(PuertaVista {
                tx,
                tz,
                lote: Lote { body: body as u8, x: s.cx, z: s.cz, w: s.w, h: s.h, clave: interiores::clave_estatica(&s.art) },
            });
        }
    }
    v
}

/// ¿Esta pieza colocada es un edificio con interior? (casas, tiendas, hospitales… no bancos ni faroles).
pub fn es_edificio_enterable(art: &str, w: i64, h: i64) -> bool {
    (art.starts_with("building/") || art.starts_with("edificio/") || art.starts_with("completo/")) && w >= 5 && h >= 4
}

#[allow(clippy::too_many_arguments)]
fn detectar_puerta(
    time: Res<Time>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    mut int: ResMut<Interiores>,
    player: Query<&SimState, With<Player>>,
    puertas: Query<&PuertaDeFachada>,
    estructuras: Res<crate::structures::BuiltStructures>,
    mut acc: Local<f32>,
) {
    let dt = time.delta_secs().min(0.2);
    if let Some((_, t)) = int.aviso.as_mut() {
        *t -= dt;
    }
    if int.aviso.as_ref().is_some_and(|(_, t)| *t <= 0.0) {
        int.aviso = None;
    }
    *acc += dt;
    if *acc < 0.1 {
        return;
    }
    *acc = 0.0;
    let Ok(sim) = player.get_single() else { return };
    let (px, pz) = (sim.body.x, sim.body.z);
    let body = active_surface_body(&ctx, &planet);

    if let Some(d) = int.dentro {
        let Some(plano) = interiores::plano_de(d.slot) else { return };
        let en_planta_baja = tiles.z == interiores::z_de(d.slot, 0);
        let (cx, cz) = (plano.puerta.0 as f32 + 0.5, plano.puerta.1 as f32 + 0.5);
        let cerca = en_planta_baja && (cx - px).hypot(cz - pz) < ALCANCE_PUERTA;
        int.cerca = cerca.then_some(Cerca::Salida);
        return;
    }
    if !matches!(ctx.active, ha_shared::WorldContext::Overworld) || tiles.z != 0 {
        int.cerca = None;
        return;
    }
    let mut mejor: Option<(f32, Lote, (f32, f32))> = None;
    for p in puertas_vistas(body, px, pz, ALCANCE_PUERTA, &puertas, &estructuras) {
        let (cx, cz) = (p.tx as f32 + 0.5, p.tz as f32 + 0.5);
        let d = (cx - px).hypot(cz - pz);
        if mejor.map_or(true, |(m, _, _)| d < m) {
            mejor = Some((d, p.lote, (cx, cz)));
        }
    }
    int.cerca = mejor.map(|(_, lote, puerta)| Cerca::Entrada { lote, puerta });
}

// ---------------------------------------------------------------------------
// La flecha amarilla
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn flechas(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<crate::art2d::ArtLibrary>,
    tiles: Res<TileWorld>,
    int: Res<Interiores>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    estructuras: Res<crate::structures::BuiltStructures>,
    player: Query<&SimState, With<Player>>,
    puertas: Query<&PuertaDeFachada>,
    mut flechas: Query<(Entity, &Flecha, &mut Transform)>,
) {
    let Ok(sim) = player.get_single() else { return };
    let (px, pz) = (sim.body.x, sim.body.z);
    // Qué puertas llevan flecha ahora: (celda, es la de salida).
    let mut deseadas: Vec<(i64, i64, bool)> = Vec::new();
    if let Some(d) = int.dentro {
        if let Some(plano) = interiores::plano_de(d.slot) {
            if tiles.z == interiores::z_de(d.slot, 0) {
                deseadas.push((plano.puerta.0, plano.puerta.1, true));
            }
        }
    } else if tiles.z == 0 {
        let body = active_surface_body(&ctx, &planet);
        for p in puertas_vistas(body, px, pz, ALCANCE_FLECHA, &puertas, &estructuras) {
            if deseadas.len() < 6 {
                deseadas.push((p.tx, p.tz, false));
            }
        }
    }
    let t = time.elapsed_secs();
    for (e, f, mut tf) in &mut flechas {
        if !deseadas.contains(&(f.tx, f.tz, f.salida)) {
            commands.entity(e).despawn_recursive();
            continue;
        }
        let r = render_xy(f.tx as f32 + 0.5, f.tz as f32 + 0.5);
        // Flota sobre el dintel y sube y baja.
        tf.translation = Vec3::new(r.x, r.y + 2.6 + (t * 3.0).sin() * 0.18, crate::layers::OBRA);
    }
    for (tx, tz, salida) in deseadas {
        if flechas.iter().any(|(_, f, _)| (f.tx, f.tz, f.salida) == (tx, tz, salida)) {
            continue;
        }
        let mut sprite = art.sprite_or_color("ui/ic_flecha_arriba", Color::srgb(1.0, 0.82, 0.12), Vec2::splat(1.0));
        sprite.flip_y = true; // apunta hacia abajo, a la puerta
        sprite.color = Color::srgb(1.0, 0.82, 0.12);
        sprite.custom_size = Some(Vec2::splat(1.0));
        let r = render_xy(tx as f32 + 0.5, tz as f32 + 0.5);
        commands.spawn((
            sprite,
            Transform::from_xyz(r.x, r.y + 2.6, crate::layers::OBRA),
            Flecha { tx, tz, salida },
            Name::new("FlechaDePuerta"),
        ));
    }
}

// ---------------------------------------------------------------------------
// El panel de botones
// ---------------------------------------------------------------------------

fn boton(parent: &mut ChildBuilder, accion: AccionPuerta, ancho: f32) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(ancho),
                min_height: Val::Px(30.0),
                padding: UiRect::axes(Val::Px(6.0), Val::Px(4.0)),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(crate::ui_pack::Pack::TINTA),
            BackgroundColor(Color::srgb(0.80, 0.66, 0.44)),
            accion,
            Visibility::Hidden,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(""),
                TextFont { font_size: 13.0, ..default() },
                TextColor(crate::ui_pack::Pack::TINTA),
                TextLayout::new_with_justify(JustifyText::Center),
                TextoPanel(accion),
            ));
        });
}

fn crear_panel(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(120.0),
                width: Val::Px(150.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            },
            GlobalZIndex(55),
            PanelPuerta,
            Name::new("PanelPuerta"),
        ))
        .with_children(|p| {
            for a in [AccionPuerta::Pasar, AccionPuerta::Llave, AccionPuerta::Reforzar, AccionPuerta::Reponer] {
                boton(p, a, 150.0);
            }
            p.spawn((
                Text::new(""),
                TextFont { font_size: 12.0, ..default() },
                TextColor(Color::srgb(1.0, 0.95, 0.8)),
                TextoAviso,
            ));
        });
}

fn costo_texto(c: &[(&str, u32)]) -> String {
    c.iter().map(|(i, n)| format!("{n} {}", nombre_item(i))).collect::<Vec<_>>().join(" + ")
}

fn nombre_item(i: &str) -> &str {
    match i {
        "wood" => "madera",
        "iron_ingot" => "hierro",
        "gold_ingot" => "oro",
        o => o,
    }
}

fn tiene(inv: &PlayerInventory, c: &[(&str, u32)]) -> bool {
    c.iter().all(|(i, n)| inv.qty(i) >= *n)
}

/// Qué botones se ven y qué dicen.
#[allow(clippy::too_many_arguments)]
fn panel_de_puerta(
    int: Res<Interiores>,
    inv: Res<PlayerInventory>,
    mut paneles: Query<(&mut Node, &mut Visibility), With<PanelPuerta>>,
    mut botones: Query<(&AccionPuerta, &mut Visibility), (With<Button>, Without<PanelPuerta>)>,
    mut textos: Query<(&TextoPanel, &mut Text), Without<TextoAviso>>,
    mut aviso: Query<&mut Text, With<TextoAviso>>,
    layout: Res<crate::input::TouchLayout>,
) {
    if let Ok((mut n, mut v)) = paneles.get_single_mut() {
        n.left = Val::Px(layout.insets.left as f32 + 12.0);
        n.top = Val::Px((layout.height * 0.30).max(110.0));
        *v = Visibility::Inherited;
    }
    // Estado de la puerta a la que apunta el panel.
    let (lote, adentro) = match (int.cerca, int.dentro) {
        (Some(Cerca::Entrada { lote, .. }), _) => (Some(lote), false),
        (Some(Cerca::Salida), Some(d)) => (Some(d.lote), true),
        _ => (None, false),
    };
    let estado = lote.and_then(|l| int.puertas.get(&(l.body, l.x, l.z)).copied().or(Some(Puerta::default())));
    let mut visible = |a: AccionPuerta| -> bool {
        match (a, estado) {
            (_, None) => false,
            (AccionPuerta::Pasar, Some(_)) => true,
            (AccionPuerta::Llave, Some(p)) => adentro && !p.rota,
            (AccionPuerta::Reforzar, Some(p)) => adentro && !p.rota && p.nivel < 3,
            (AccionPuerta::Reponer, Some(p)) => p.rota,
        }
    };
    for (a, mut v) in &mut botones {
        *v = if visible(*a) { Visibility::Inherited } else { Visibility::Hidden };
    }
    for (t, mut txt) in &mut textos {
        let Some(p) = estado else { continue };
        txt.0 = match t.0 {
            AccionPuerta::Pasar => {
                if adentro {
                    "SALIR".to_string()
                } else if p.cerrada {
                    "ENTRAR (cerrada)".to_string()
                } else {
                    "ENTRAR".to_string()
                }
            }
            AccionPuerta::Llave => if p.cerrada { "ABRIR LLAVE".to_string() } else { "CERRAR CON LLAVE".to_string() },
            AccionPuerta::Reforzar => match costo_de_mejora(p.nivel) {
                Some(c) => {
                    let falta = if tiene(&inv, c) { "" } else { " (te falta)" };
                    format!("REFORZAR: {}{falta}", costo_texto(c))
                }
                None => String::new(),
            },
            AccionPuerta::Reponer => {
                let c = [COSTO_REPOSICION];
                let falta = if tiene(&inv, &c) { "" } else { " (te falta)" };
                format!("REPONER PUERTA: {}{falta}", costo_texto(&c))
            }
        };
    }
    if let Ok(mut t) = aviso.get_single_mut() {
        let msg = match (&int.aviso, estado) {
            (Some((m, _)), _) => m.clone(),
            (None, Some(p)) if adentro => format!("Puerta {} · {}", nombre_de_nivel(p.nivel), if p.rota { "ROTA".into() } else if p.nivel >= 3 { "irrompible".to_string() } else { format!("{:.0} de vida", p.vida) }),
            _ => String::new(),
        };
        if t.0 != msg {
            t.0 = msg;
        }
    }
}

/// Los rectángulos del panel, para que el stick y el mundo no se roben el toque.
pub fn append_interior_touch_blocks(
    mut blocks: ResMut<crate::input::TouchBlockRects>,
    int: Res<Interiores>,
    layout: Res<crate::input::TouchLayout>,
) {
    if int.cerca.is_some() {
        let left = layout.insets.left as f32 + 12.0;
        let top = (layout.height * 0.30).max(110.0);
        blocks.0.push([left - 4.0, top - 4.0, left + 158.0, top + 4.0 * 40.0]);
    }
}

// ---------------------------------------------------------------------------
// Pulsar un botón
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn atender_botones(
    mut commands: Commands,
    time: Res<Time>,
    day: Res<DayClock>,
    art: Res<crate::art2d::ArtLibrary>,
    mut int: ResMut<Interiores>,
    mut tiles: ResMut<TileWorld>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    botones: Query<(&Interaction, &AccionPuerta), Changed<Interaction>>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
    ocupantes_vivos: Query<Entity, With<Ocupante>>,
) {
    let _ = &time;
    for (i, a) in &botones {
        if *i != Interaction::Pressed {
            continue;
        }
        let Some(cerca) = int.cerca else { continue };
        match (*a, cerca) {
            (AccionPuerta::Pasar, Cerca::Entrada { lote, puerta }) => {
                let p = *int.puerta_de(&lote);
                if p.cerrada && !p.rota {
                    // Tenés la llave si la cerraste vos: abrir con la llave.
                    int.puerta_de(&lote).cerrada = false;
                    int.avisar("Abriste con tu llave");
                }
                let slot = interiores::registrar(lote);
                let Some(plano) = interiores::plano_de(slot) else { continue };
                // La salida: justo afuera de la puerta (al sur).
                let salida = (puerta.0, puerta.1 + 1.3);
                int.dentro = Some(Dentro { slot, lote, salida, puerta_ext: puerta });
                tiles.z = interiores::z_de(slot, 0);
                tiles.version = tiles.version.wrapping_add(1);
                if let Ok((mut sim, mut vis)) = player.get_single_mut() {
                    teletransportar(&mut sim, &mut vis, plano.entrada.0, plano.entrada.1);
                }
                // Los residentes de este edificio aparecen.
                for e in &ocupantes_vivos {
                    commands.entity(e).despawn_recursive();
                }
                crear_ocupantes(&mut commands, &art, slot, &plano, &lote, day.phase() * 24.0);
            }
            (AccionPuerta::Pasar, Cerca::Salida) => {
                if let Some(d) = int.dentro {
                    tiles.z = 0;
                    tiles.version = tiles.version.wrapping_add(1);
                    if let Ok((mut sim, mut vis)) = player.get_single_mut() {
                        teletransportar(&mut sim, &mut vis, d.salida.0, d.salida.1);
                    }
                    int.dentro = None;
                    for e in &ocupantes_vivos {
                        commands.entity(e).despawn_recursive();
                    }
                }
            }
            (AccionPuerta::Llave, _) => {
                if let Some(d) = int.dentro {
                    let p = int.puerta_de(&d.lote);
                    p.cerrada = !p.cerrada;
                    let msg = if p.cerrada { "Cerraste con llave" } else { "Abriste la puerta" };
                    int.avisar(msg);
                }
            }
            (AccionPuerta::Reforzar, _) => {
                if let Some(d) = int.dentro {
                    let nivel = int.puerta_de(&d.lote).nivel;
                    if let Some(c) = costo_de_mejora(nivel) {
                        if tiene(&inv, c) {
                            for (item, n) in c {
                                inv.remove(item, *n);
                            }
                            persist.mark_dirty();
                            int.puerta_de(&d.lote).mejorar();
                            let nv = int.puerta_de(&d.lote).nivel;
                            int.avisar(format!("Puerta {}", nombre_de_nivel(nv)));
                        } else {
                            int.avisar("Te faltan materiales");
                        }
                    }
                }
            }
            (AccionPuerta::Reponer, Cerca::Entrada { lote, .. }) => reponer(&mut int, &mut inv, &mut persist, lote),
            (AccionPuerta::Reponer, Cerca::Salida) => {
                if let Some(d) = int.dentro {
                    reponer(&mut int, &mut inv, &mut persist, d.lote);
                }
            }
        }
    }
}

fn reponer(int: &mut Interiores, inv: &mut PlayerInventory, persist: &mut InventoryPersistState, lote: Lote) {
    let c = [COSTO_REPOSICION];
    if !tiene(inv, &c) {
        int.avisar("Te faltan materiales");
        return;
    }
    inv.remove(c[0].0, c[0].1);
    persist.mark_dirty();
    int.puerta_de(&lote).reponer();
    int.avisar("Repusiste la puerta");
}

fn teletransportar(sim: &mut SimState, vis: &mut PlayerVisual, x: f32, z: f32) {
    sim.body.x = x;
    sim.body.z = z;
    sim.body.vx = 0.0;
    sim.body.vz = 0.0;
    vis.from_x = x;
    vis.to_x = x;
    vis.from_z = z;
    vis.to_z = z;
}

// ---------------------------------------------------------------------------
// Hostiles: te siguen, asedian la puerta, entran si está abierta
// ---------------------------------------------------------------------------

fn publicar_objetivo(int: Res<Interiores>, ctx: Res<WorldContextRuntime>, planet: Res<ActivePlanet>, mut obj: ResMut<ObjetivoHostil>) {
    let body = active_surface_body(&ctx, &planet);
    let (ox, oz) = interiores::origen(body);
    obj.origen = (ox as f32, oz as f32);
    match int.dentro {
        Some(d) => {
            obj.activo = true;
            obj.puerta = d.puerta_ext;
        }
        None => obj.activo = false,
    }
}

#[allow(clippy::too_many_arguments)]
fn asedio_de_puertas(
    time: Res<Time>,
    mut int: ResMut<Interiores>,
    obj: Res<ObjetivoHostil>,
    mut infectados: Query<&mut Infectado>,
    mut policias: Query<&mut PoliciaPie>,
) {
    let Some(d) = int.dentro else { return };
    let Some(plano) = interiores::plano_de(d.slot) else { return };
    let dt = time.delta_secs().min(0.2);
    let (px, pz) = d.puerta_ext;
    let mut dano = 0.0f32;
    let mut a_entrar: Vec<(bool, usize)> = Vec::new();
    let abierta = {
        let p = int.puerta_de(&d.lote);
        p.rota || !p.cerrada
    };
    for (n, mut i) in infectados.iter_mut().enumerate() {
        if i.guardia || obj.esta_adentro(i.x, i.z) {
            continue;
        }
        // Se acercan a la puerta en línea recta (la colisión de este nivel es la del interior).
        let (dx, dz) = (px - i.x, pz - i.z);
        let dist = dx.hypot(dz);
        if dist > 1.4 && dist < 55.0 {
            let v = 2.4 * i.clase.velocidad() * dt;
            i.x += dx / dist * v.min(dist);
            i.z += dz / dist * v.min(dist);
        }
        if (i.x - px).hypot(i.z - pz) < RADIO_ASEDIO {
            if abierta {
                a_entrar.push((true, n));
            } else {
                let mult = match i.rol {
                    3 => 6.0,
                    _ => match i.clase {
                        crate::oleadas::Clase::Tanque => 2.5,
                        _ => 1.0,
                    },
                };
                dano += 3.0 * mult * dt;
            }
        }
        let _ = &mut i;
    }
    for (n, mut p) in policias.iter_mut().enumerate() {
        if obj.esta_adentro(p.x(), p.z()) {
            continue;
        }
        let (dx, dz) = (px - p.x(), pz - p.z());
        let dist = dx.hypot(dz);
        if dist > 1.4 && dist < 70.0 {
            let v = 3.0 * dt;
            let (nx, nz) = (p.x() + dx / dist * v.min(dist), p.z() + dz / dist * v.min(dist));
            p.poner(nx, nz);
        }
        if (p.x() - px).hypot(p.z() - pz) < RADIO_ASEDIO {
            if abierta {
                a_entrar.push((false, n));
            } else {
                dano += 6.0 * dt;
            }
        }
    }
    if dano > 0.0 {
        let rompio = int.puerta_de(&d.lote).golpear(dano);
        if rompio {
            int.avisar("¡Rompieron la puerta!");
        }
    }
    // Los que llegan a una puerta abierta o rota entran, uno tras otro.
    let (ex, ez) = (plano.entrada.0, plano.entrada.1);
    let mut k = 0.0f32;
    for (es_infectado, n) in a_entrar.into_iter().take(3) {
        k += 1.0;
        if es_infectado {
            if let Some(mut i) = infectados.iter_mut().nth(n) {
                i.x = ex + (k - 2.0) * 0.8;
                i.z = ez - 0.5;
            }
        } else if let Some(mut p) = policias.iter_mut().nth(n) {
            p.poner(ex + (k - 2.0) * 0.8, ez - 0.5);
        }
    }
}

/// Una puerta rota deja la fachada sin hoja: se esconde el batiente animado que está en esa celda.
fn ocultar_puertas_rotas(int: Res<Interiores>, mut puertas: Query<(&PuertaDeFachada, &mut Visibility)>, ctx: Res<WorldContextRuntime>, planet: Res<ActivePlanet>) {
    if int.puertas.values().all(|p| !p.rota) {
        return;
    }
    let body = active_surface_body(&ctx, &planet);
    for (p, mut v) in &mut puertas {
        let lote = lote_de_puerta(body, p);
        let rota = int.puertas.get(&(lote.body, lote.x, lote.z)).is_some_and(|d| d.rota);
        *v = if rota { Visibility::Hidden } else { Visibility::Inherited };
    }
}

// ---------------------------------------------------------------------------
// La gente de adentro
// ---------------------------------------------------------------------------

fn crear_ocupantes(commands: &mut Commands, art: &crate::art2d::ArtLibrary, slot: u32, plano: &interiores::Plano, lote: &Lote, hora: f32) {
    let n = interiores::residentes(lote);
    let camas = camas_del_edificio(plano, slot);
    for k in 0..n {
        let h = ha_shared::worldgen::hash_cell(0x0c0a_7e, lote.x + k as i64 * 13, lote.z) as usize;
        let skin = format!("npc/char_{}", 1 + h % 20);
        let despierto = interiores::despierto(plano.arq, lote, k, hora);
        let mut sprite = art.sprite_or_color(&skin, Color::srgb(0.7, 0.6, 0.5), Vec2::new(0.95, 1.9));
        let (pos, piso, rot, dormido) = if !despierto && !camas.is_empty() {
            let (piso, bx, bz) = camas[k as usize % camas.len()];
            sprite.color = Color::srgb(0.85, 0.85, 0.95);
            (Vec2::new(bx, bz), piso, std::f32::consts::FRAC_PI_2, true)
        } else {
            // De día: parado en algún ambiente de la planta baja.
            let amb: Vec<_> = plano.ambientes.iter().filter(|a| a.0 == 0).collect();
            let a = amb.get(h % amb.len().max(1)).copied().copied().unwrap_or((0, plano.x0 + 2, plano.z0 + 2, 3, 3));
            let ax = a.1 as f32 + 1.0 + (h % (a.3.max(3) as usize - 1)) as f32;
            let az = a.2 as f32 + 1.0 + ((h >> 8) % (a.4.max(3) as usize - 1)) as f32;
            (Vec2::new(ax, az), 0, 0.0, false)
        };
        let r = render_xy(pos.x, pos.y);
        commands.spawn((
            sprite,
            Transform { translation: Vec3::new(r.x, r.y, sort_z(pos.y)), rotation: Quat::from_rotation_z(rot), ..default() },
            Ocupante { slot, piso, dormido, base: pos, fase: h as f32 },
            Name::new("Residente"),
        ));
    }
}

/// Las camas del edificio: `(piso, x, z)` donde se acuesta alguien.
fn camas_del_edificio(plano: &interiores::Plano, slot: u32) -> Vec<(i32, f32, f32)> {
    let mut v = Vec::new();
    for piso in 0..plano.pisos {
        let z = interiores::z_de(slot, piso);
        let casa = (plano.x0, plano.z0, plano.w, plano.h, plano.clave);
        for m in ha_shared::amueblado::muebles_de_z(CelestialBody::Earth, z, casa).iter() {
            if crate::descanso::uso_de(m.decor.key) == Some(crate::descanso::Uso::Cama) {
                let (cx, pie) = m.centro_pie();
                v.push((piso, cx, pie - m.prof as f32 * 0.5));
            }
        }
    }
    v
}

fn ocupantes(
    time: Res<Time>,
    tiles: Res<TileWorld>,
    mut q: Query<(&Ocupante, &mut Transform, &mut Visibility)>,
) {
    let t = time.elapsed_secs();
    for (o, mut tf, mut v) in &mut q {
        let aqui = tiles.z == interiores::z_de(o.slot, o.piso);
        *v = if aqui { Visibility::Inherited } else { Visibility::Hidden };
        if !o.dormido && aqui {
            // Pasea un poquito en su lugar.
            let dx = (t * 0.4 + o.fase).sin() * 0.6;
            let dz = (t * 0.3 + o.fase * 1.7).cos() * 0.4;
            let r = render_xy(o.base.x + dx, o.base.y + dz);
            tf.translation = Vec3::new(r.x, r.y, sort_z(o.base.y + dz));
        }
    }
}

/// QA: fija la puerta «cercana» y pulsa el botón indicado, igual que un dedo. `accion`: pasar | llave | reforzar | reponer.
pub fn debug_pulsar(world: &mut World, cerca: Option<Cerca>, accion: &str) {
    if let Some(c) = cerca {
        world.resource_mut::<Interiores>().cerca = Some(c);
    }
    let quiere = match accion {
        "pasar" => AccionPuerta::Pasar,
        "llave" => AccionPuerta::Llave,
        "reforzar" => AccionPuerta::Reforzar,
        _ => AccionPuerta::Reponer,
    };
    let mut q = world.query::<(&AccionPuerta, &mut Interaction)>();
    for (a, mut i) in q.iter_mut(world) {
        if *a == quiere {
            *i = Interaction::Pressed;
        }
    }
}

/// ¿Estoy adentro de un edificio? Lo consultan el clima, el sonido y las apariciones ambientales.
pub fn adentro(int: &Interiores) -> bool {
    int.dentro.is_some()
}

#[allow(dead_code)]
fn _para_el_compilador(_: Tile) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_puerta_se_rompe_pero_la_blindada_no() {
        let mut p = Puerta::default();
        assert!(!p.golpear(10.0));
        assert!(p.golpear(100.0), "una puerta de madera cede");
        assert!(p.rota && !p.cerrada);
        let mut b = Puerta { nivel: 3, vida: vida_de_puerta(3), ..Puerta::default() };
        assert!(!b.golpear(1.0e9), "la blindada es irrompible");
        assert!(!b.rota);
    }

    #[test]
    fn mejorar_sube_el_nivel_y_la_vida_hasta_el_tope() {
        let mut p = Puerta::default();
        let v0 = p.vida;
        assert!(p.mejorar());
        assert!(p.vida > v0);
        assert!(p.mejorar() && p.mejorar());
        assert_eq!(p.nivel, 3);
        assert!(!p.mejorar(), "ya no se mejora más");
        assert!(costo_de_mejora(3).is_none());
    }

    #[test]
    fn cada_mejora_cuesta_mas_que_la_anterior() {
        let valor = |c: &[(&str, u32)]| c.iter().map(|(i, n)| *n * match *i { "wood" => 1, "iron_ingot" => 4, "gold_ingot" => 10, _ => 1 }).sum::<u32>();
        let costos: Vec<u32> = (0..3).map(|n| valor(costo_de_mejora(n).unwrap())).collect();
        assert!(costos.windows(2).all(|w| w[0] < w[1]), "{costos:?}");
    }

    #[test]
    fn los_hostiles_apuntan_a_la_puerta_si_estas_adentro_y_a_vos_si_no() {
        let obj = ObjetivoHostil { activo: true, puerta: (10.0, 20.0), origen: (30000.0, 0.0) };
        assert_eq!(obj.objetivo(5.0, 5.0, (30010.0, 3.0)), (10.0, 20.0));
        // Ya adentro: te persigue a vos.
        assert_eq!(obj.objetivo(30005.0, 2.0, (30010.0, 3.0)), (30010.0, 3.0));
        let libre = ObjetivoHostil { activo: false, ..obj };
        assert_eq!(libre.objetivo(5.0, 5.0, (1.0, 1.0)), (1.0, 1.0));
    }
}
