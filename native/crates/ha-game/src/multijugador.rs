//! **Multijugador (LAN y online)** — la parte de Bevy de [`crate::red`].
//!
//! Cada cliente corre su mundo con la MISMA semilla; la red sólo lleva a los
//! demás jugadores: dónde están, a dónde miran y lo que dicen. Por eso un
//! jugador se ve como un aldeano más (misma hoja `npc/char_N`, misma medida) y
//! no hay sim compartida que bifurcar (regla 3 del CLAUDE.md). Que el servidor
//! sea autoritativo sobre el MUNDO queda para cuando haya edición compartida
//! (biblia §15, "lo que falta").
//!
//! Dos puertas de entrada:
//! * **Menú → MULTIJUGADOR**: lista los anfitriones de la LAN (por aviso UDP) y
//!   el servidor online; tocar uno conecta, toma la semilla de la sala, crea (o
//!   reabre) el mundo con esa semilla y entra.
//! * **En el juego → MULTIJUGADOR**: `ANFITRIÓN LAN` levanta un hub dentro de tu
//!   propio juego y avisa por la red; `ONLINE` te conecta al servidor con la
//!   semilla de TU mundo.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::game_phase::{GamePhase, SelectedCharacter};
use crate::i18n::{idioma_actual, Idioma};
use crate::red::{self, Anfitrion, Aviso, Buscador, Cliente, Evento, Hub, Msg};
use crate::scene::WorldContextRuntime;
use crate::sim::Player;
use crate::world2d::{active_surface_body, sort_z, ActivePlanet};

const HZ_ESTADO: f32 = 10.0;
/// Un anfitrión que no avisa en este tiempo desaparece de la lista.
const CADUCA_AVISO: Duration = Duration::from_secs(7);
const AVATAR: Vec2 = Vec2::new(0.95, 1.9);
/// Qué tan rápido el avatar alcanza la posición recibida (1/s).
const SUAVIZADO: f32 = 12.0;
/// Si el remoto está más lejos que esto de donde lo dibujamos, se teletransporta.
const SALTO: f32 = 20.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EstadoRed {
    #[default]
    Fuera,
    Conectando,
    Dentro,
}

pub struct Remoto {
    pub nombre: String,
    pub piel: String,
    pub x: f32,
    pub z: f32,
    pub cuerpo: u8,
    pub rumbo: f32,
    pub vuelo: bool,
    /// Clave del vehículo que maneja ahora (vacío = a pie).
    pub veh: String,
    pub pts: u32,
    pub baj: u32,
    /// Lo que se ve ahora (suavizado hacia x/z).
    pub vx: f32,
    pub vz: f32,
    pub entidad: Option<Entity>,
}

/// Cómo se conectó este cliente: decide qué pasa cuando llega el `Id`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origen {
    /// Desde el menú principal: falta crear/abrir el mundo de la sala y entrar.
    Menu,
    /// Desde dentro del juego: ya estamos en un mundo.
    Juego,
}

#[derive(Resource, Default)]
pub struct Multi {
    hub: Option<Hub>,
    aviso: Option<Aviso>,
    cliente: Option<Mutex<Cliente>>,
    buscador: Option<Mutex<Buscador>>,
    pub estado: EstadoRed,
    pub mensaje: String,
    pub mi_id: Option<u32>,
    pub semilla_sala: u32,
    pub remotos: HashMap<u32, Remoto>,
    pub anfitriones: Vec<Anfitrion>,
    origen: Option<Origen>,
    /// Cambia con todo lo que la UI muestra (se redibuja sólo cuando cambia).
    pub version: u32,
    acc: f32,
    /// Charla que llegó y todavía no se dijo en voz alta.
    charlas: Vec<(u32, String)>,
    nombre_local: String,
    piel_local: String,
}

impl Multi {
    fn tocar(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    pub fn es_anfitrion(&self) -> bool {
        self.hub.is_some()
    }

    pub fn conectado(&self) -> bool {
        self.estado == EstadoRed::Dentro
    }

    pub fn buscando(&self) -> bool {
        self.buscador.is_some()
    }

    /// Empieza o deja de escuchar los avisos de la LAN.
    pub fn buscar(&mut self, si: bool) {
        if si && self.buscador.is_none() {
            self.buscador = Buscador::escuchar().map(Mutex::new);
        } else if !si {
            self.buscador = None;
            self.anfitriones.clear();
        }
        self.tocar();
    }

    pub fn conectar(&mut self, destino: String, nombre: &str, piel: &str, semilla: u32, origen: Origen) {
        self.desconectar();
        self.nombre_local = nombre.to_string();
        self.piel_local = piel.to_string();
        self.origen = Some(origen);
        self.estado = EstadoRed::Conectando;
        self.mensaje = format!("conectando a {destino}...");
        self.cliente = Some(Mutex::new(Cliente::conectar(
            destino,
            red::hola(nombre, piel, semilla, ""),
        )));
        self.tocar();
    }

    pub fn desconectar(&mut self) {
        self.cliente = None;
        self.estado = EstadoRed::Fuera;
        self.mi_id = None;
        self.origen = None;
        self.remotos.clear();
        self.tocar();
    }

    /// Levanta un hub propio y se conecta a él (el anfitrión también es jugador).
    pub fn alojar(&mut self, mundo: &str, semilla: u32, nombre: &str, piel: &str) -> Result<String, String> {
        self.cerrar_todo();
        let hub = Hub::levantar(red::PUERTO, semilla)
            .or_else(|_| Hub::levantar(0, semilla))
            .map_err(|e| format!("no se pudo abrir el puerto: {e}"))?;
        let puerto = hub.puerto;
        self.aviso = Aviso::emitir(puerto, mundo.to_string(), semilla, || 0);
        self.hub = Some(hub);
        self.conectar(format!("127.0.0.1:{puerto}"), nombre, piel, semilla, Origen::Juego);
        let ip = red::ip_local().unwrap_or_else(|| "?".into());
        Ok(format!("{ip}:{puerto}"))
    }

    pub fn cerrar_todo(&mut self) {
        self.desconectar();
        self.aviso = None;
        self.hub = None;
        self.mensaje.clear();
        self.tocar();
    }

    pub fn decir(&mut self, texto: &str) {
        if let Some(c) = &self.cliente {
            if let Ok(c) = c.lock() {
                c.enviar(Msg::Chat { id: 0, msg: texto.to_string() });
            }
        }
    }

    fn enviar(&self, m: Msg) {
        if let Some(c) = &self.cliente {
            if let Ok(c) = c.lock() {
                c.enviar(m);
            }
        }
    }
}

/// El servidor online configurado: variable `HA_SERVIDOR` (o prop de Android),
/// el host horneado al compilar (`HA_SERVER_HOST`) o `servidor.cfg`.
pub fn servidor_online() -> Option<String> {
    let crudo = crate::flags::var("HA_SERVIDOR")
        .ok()
        .or_else(|| option_env!("HA_SERVER_HOST").map(str::to_string))
        .or_else(|| std::fs::read_to_string("servidor.cfg").ok())?;
    let t = crudo.trim();
    if t.is_empty() {
        return None;
    }
    Some(if t.contains(':') { t.to_string() } else { format!("{t}:{}", red::PUERTO) })
}

/// La hoja con que te ven los demás: una de las 20 del Character Generator,
/// elegida por el nombre para que siempre seas la misma.
pub fn piel_para(nombre: &str) -> String {
    let h = nombre.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    format!("npc/char_{}", 1 + h % 20)
}

#[derive(Component)]
pub struct JugadorRemoto(pub u32);

/// Qué sprite lleva puesto el avatar ahora (`piel` o `vehículo#pose`), para no rehacerlo cada cuadro.
#[derive(Component)]
pub struct Dibujado(String);

/// El sprite que le toca a un remoto: su piel a pie, o su vehículo con la pose del rumbo.
/// Devuelve la clave de lo dibujado y el sprite (`None` = no hay arte, se queda con lo que tiene).
fn sprite_remoto(art: &crate::art2d::ArtLibrary, r: &Remoto) -> (String, Option<Sprite>) {
    if r.veh.is_empty() {
        return (r.piel.clone(), Some(art.sprite_or_color(&r.piel, Color::srgb(0.9, 0.6, 0.2), AVATAR)));
    }
    let pose = crate::vehiculos::pose_de_rumbo(r.rumbo);
    let clave = format!("{}#{pose}", r.veh);
    let sprite = art.pose(&r.veh, pose).and_then(|([_, _, w, h], _)| {
        art.pose_sprite(&r.veh, pose, Vec2::new(w as f32 / 32.0, h as f32 / 32.0))
    });
    (clave, sprite)
}

#[derive(Component)]
struct EtiquetaRemoto;

pub struct MultijugadorPlugin;

impl Plugin for MultijugadorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Multi>()
            .init_resource::<PanelMulti>()
            .add_systems(Update, (sondear_red, buscar_lan, entrar_al_mundo, redibujar_menu).chain())
            .add_systems(
                Update,
                (enviar_estado, sincronizar_avatares, charlas_remotas, panel_multi, botones_multi)
                    .chain()
                    .run_if(in_state(GamePhase::Playing)),
            )
            .add_systems(OnExit(GamePhase::Playing), salir_de_la_partida);
    }
}

fn salir_de_la_partida(mut multi: ResMut<Multi>, mut commands: Commands, avatares: Query<Entity, With<JugadorRemoto>>) {
    multi.cerrar_todo();
    for e in &avatares {
        commands.entity(e).despawn_recursive();
    }
}

/// Aplica lo que llegó por la red.
fn sondear_red(mut multi: ResMut<Multi>) {
    let mut eventos = Vec::new();
    if let Some(c) = &multi.cliente {
        if let Ok(c) = c.lock() {
            while let Ok(e) = c.rx.try_recv() {
                eventos.push(e);
            }
        }
    }
    if eventos.is_empty() {
        return;
    }
    for e in eventos {
        match e {
            Evento::Caido(txt) => {
                multi.cliente = None;
                multi.estado = EstadoRed::Fuera;
                multi.mi_id = None;
                multi.remotos.clear();
                multi.mensaje = txt;
                if multi.hub.is_some() {
                    multi.hub = None;
                    multi.aviso = None;
                }
            }
            Evento::Mensaje(m) => match m {
                Msg::Id { id, semilla, .. } => {
                    multi.mi_id = Some(id);
                    multi.semilla_sala = semilla;
                    multi.estado = EstadoRed::Dentro;
                    multi.mensaje = format!("conectado · mundo {semilla}");
                }
                Msg::Ent { id, nombre, piel } => {
                    if Some(id) != multi.mi_id {
                        multi.remotos.entry(id).or_insert(Remoto {
                            nombre: nombre.clone(),
                            piel,
                            x: 0.0,
                            z: 0.0,
                            cuerpo: 0,
                            rumbo: 0.0,
                            vuelo: false,
                            veh: String::new(),
                            pts: 0,
                            baj: 0,
                            vx: f32::NAN,
                            vz: f32::NAN,
                            entidad: None,
                        });
                        multi.mensaje = format!("{nombre} entró");
                    }
                }
                Msg::Est { id, x, z, cuerpo, rumbo, vuelo, pts, baj, veh } => {
                    let m = &mut *multi;
                    if let Some(r) = m.remotos.get_mut(&id) {
                        if r.pts != pts || r.baj != baj {
                            m.version = m.version.wrapping_add(1);
                        }
                        r.pts = pts;
                        r.baj = baj;
                        r.x = x;
                        r.z = z;
                        r.cuerpo = cuerpo;
                        r.rumbo = rumbo;
                        r.vuelo = vuelo;
                        r.veh = veh;
                    }
                }
                Msg::Chat { id, msg } => multi.charlas.push((id, msg)),
                Msg::Chau { id } => {
                    if let Some(r) = multi.remotos.remove(&id) {
                        multi.mensaje = format!("{} salió", r.nombre);
                    }
                }
                Msg::Error { msg } => multi.mensaje = msg,
                // Mundo compartido, árbitro y salas: los consume `mundo_compartido` (en curso).
                _ => {}
            },
        }
    }
    multi.tocar();
}

fn buscar_lan(mut multi: ResMut<Multi>) {
    let mut nuevos = Vec::new();
    if let Some(b) = &multi.buscador {
        if let Ok(b) = b.lock() {
            while let Ok(a) = b.rx.try_recv() {
                nuevos.push(a);
            }
        }
    }
    let mut cambio = !nuevos.is_empty();
    for a in nuevos {
        match multi.anfitriones.iter_mut().find(|x| x.direccion == a.direccion) {
            Some(x) => *x = a,
            None => multi.anfitriones.push(a),
        }
    }
    let antes = multi.anfitriones.len();
    multi.anfitriones.retain(|a| a.visto.elapsed() < CADUCA_AVISO);
    cambio |= antes != multi.anfitriones.len();
    if cambio {
        multi.tocar();
    }
}

/// Conectado desde el menú: llegó la semilla de la sala → se abre ESE mundo y se
/// entra. Reusa el mundo si ya tenés uno con esa semilla.
fn entrar_al_mundo(
    mut multi: ResMut<Multi>,
    mut next: ResMut<NextState<GamePhase>>,
    mut seleccionado: ResMut<SelectedCharacter>,
    mut editor: ResMut<crate::ui_editor::UiEditorState>,
    mut pantalla: ResMut<crate::menu_listas::Pantalla>,
    fase: Res<State<GamePhase>>,
) {
    if multi.origen != Some(Origen::Menu) || multi.estado != EstadoRed::Dentro || *fase.get() != GamePhase::MainMenu {
        return;
    }
    let semilla = multi.semilla_sala;
    match crate::partida::listar().into_iter().find(|f| f.semilla == semilla && !f.creativo) {
        Some(f) => crate::partida::abrir(&f.id),
        None => {
            crate::partida::crear(&format!("En línea {semilla}"), Some(semilla), false);
        }
    }
    if let Some(guardado) = crate::game_phase::load_savegame() {
        if seleccionado.look.is_none() {
            *seleccionado = guardado;
        }
    }
    editor.creative = false;
    *pantalla = crate::menu_listas::Pantalla::Ninguna;
    multi.buscador = None;
    multi.origen = Some(Origen::Juego);
    next.set(GamePhase::Loading);
}

/// La pantalla MULTIJUGADOR del menú se redibuja cuando cambia la lista.
fn redibujar_menu(multi: Res<Multi>, mut pantalla: ResMut<crate::menu_listas::Pantalla>, mut visto: Local<u32>) {
    if *pantalla == crate::menu_listas::Pantalla::Multijugador && multi.version != *visto {
        *visto = multi.version;
        pantalla.set_changed();
    }
}

fn enviar_estado(
    time: Res<Time>,
    mut multi: ResMut<Multi>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    player: Query<&Transform, With<Player>>,
    metas: Res<crate::metas::Metas>,
    bajas: Res<crate::infectados::Bajas>,
    manejando: Res<crate::vehiculos::Manejando>,
    autos: Query<&crate::vehiculos::Vehiculo>,
) {
    if multi.estado != EstadoRed::Dentro {
        return;
    }
    multi.acc += time.delta_secs();
    if multi.acc < 1.0 / HZ_ESTADO {
        return;
    }
    multi.acc = 0.0;
    let Ok(tf) = player.get_single() else { return };
    let cuerpo = active_surface_body(&ctx, &planet) as u8;
    let (_, _, mut rumbo) = tf.rotation.to_euler(EulerRot::XYZ);
    let vuelo = !matches!(ctx.active, ha_shared::WorldContext::Overworld | ha_shared::WorldContext::Luna);
    // Al volante: los demás te ven en tu vehículo y con su rumbo (la pose sale de ahí).
    let mut veh = String::new();
    if let Some(v) = manejando.auto.and_then(|e| autos.get(e).ok()) {
        veh = v.clave.to_string();
        rumbo = v.estado.rumbo;
    }
    multi.enviar(Msg::Est { id: 0, x: tf.translation.x, z: -tf.translation.y, cuerpo, rumbo, vuelo, pts: metas.puntos(), baj: bajas.total, veh });
}

#[allow(clippy::too_many_arguments)]
fn sincronizar_avatares(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<crate::art2d::ArtLibrary>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut multi: ResMut<Multi>,
    mut avatares: Query<(Entity, &JugadorRemoto, &mut Transform, &mut Visibility, &mut Sprite, &mut Dibujado)>,
) {
    let mi_cuerpo = active_surface_body(&ctx, &planet) as u8;
    let dt = time.delta_secs();
    // Los que ya no están en la tabla se van.
    for (e, j, _, _, _, _) in &avatares {
        if !multi.remotos.contains_key(&j.0) {
            commands.entity(e).despawn_recursive();
        }
    }
    let ids: Vec<u32> = multi.remotos.keys().copied().collect();
    for id in ids {
        let Some(r) = multi.remotos.get_mut(&id) else { continue };
        if r.vx.is_nan() {
            // Sin estado todavía: no se dibuja a un fantasma en (0,0).
            if r.x == 0.0 && r.z == 0.0 {
                continue;
            }
            r.vx = r.x;
            r.vz = r.z;
        }
        let (dx, dz) = (r.x - r.vx, r.z - r.vz);
        if dx * dx + dz * dz > SALTO * SALTO {
            r.vx = r.x;
            r.vz = r.z;
        } else {
            let k = (SUAVIZADO * dt).min(1.0);
            r.vx += dx * k;
            r.vz += dz * k;
        }
        let visible = r.cuerpo == mi_cuerpo;
        match r.entidad.filter(|e| avatares.get(*e).is_ok()) {
            Some(e) => {
                if let Ok((_, _, mut tf, mut vis, mut sprite, mut dib)) = avatares.get_mut(e) {
                    tf.translation = Vec3::new(r.vx, -r.vz, sort_z(r.vz));
                    *vis = if visible { Visibility::Inherited } else { Visibility::Hidden };
                    // A pie o en vehículo: cambia el sprite sólo cuando cambia la clave/pose.
                    let (clave, nuevo) = sprite_remoto(&art, r);
                    if dib.0 != clave {
                        if let Some(s) = nuevo {
                            *sprite = s;
                            dib.0 = clave;
                        }
                    }
                }
            }
            None => {
                let (clave, nuevo) = sprite_remoto(&art, r);
                let sprite = nuevo.unwrap_or_else(|| art.sprite_or_color(&r.piel, Color::srgb(0.9, 0.6, 0.2), AVATAR));
                let mut ent = commands.spawn((
                    sprite,
                    Dibujado(clave),
                    Transform::from_xyz(r.vx, -r.vz, sort_z(r.vz)),
                    JugadorRemoto(id),
                    Name::new("JugadorRemoto"),
                ));
                ent.with_children(|c| {
                    c.spawn((
                        Text2d::new(r.nombre.clone()),
                        TextFont { font_size: 22.0, ..default() },
                        TextColor(Color::srgb(0.65, 1.0, 0.85)),
                        Transform::from_xyz(0.0, AVATAR.y * 0.75, 1.0),
                        crate::textos_mundo::TextoEnMundo { px: 14.0, font_size: 22.0 },
                        EtiquetaRemoto,
                    ));
                });
                r.entidad = Some(ent.id());
            }
        }
    }
}

fn charlas_remotas(
    mut multi: ResMut<Multi>,
    mut speech: ResMut<crate::dialog::SpeechQueue>,
) {
    if multi.charlas.is_empty() {
        return;
    }
    let charlas = std::mem::take(&mut multi.charlas);
    for (id, msg) in charlas {
        if Some(id) == multi.mi_id {
            continue; // lo que dije yo ya lo vi
        }
        if let Some(r) = multi.remotos.get(&id) {
            if let Some(e) = r.entidad {
                crate::dialog::say_from(&mut speech, e, r.vx, r.vz, msg, Color::srgb(0.65, 1.0, 0.85), true);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Panel dentro del juego
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
pub struct PanelMulti {
    pub abierto: bool,
    dibujado: Option<(bool, u32, u32, u32)>,
}

/// El botón del menú del juego que abre/cierra el panel.
#[derive(Component)]
pub struct BotonMultijugador;

#[derive(Component)]
struct PanelMultiRaiz;

#[derive(Component, Clone, Debug)]
enum AccionMulti {
    Anfitrion,
    Online,
    Frase(&'static str),
    Cerrar,
}

const FRASES: [&str; 4] = ["¡Hola!", "¡Vení!", "¡Ayuda!", "Gracias."];

fn color_texto() -> Color {
    Color::srgb(0.65, 1.0, 0.85)
}

#[allow(clippy::too_many_arguments)]
fn panel_multi(
    mut commands: Commands,
    mut panel: ResMut<PanelMulti>,
    multi: Res<Multi>,
    abrir: Query<&Interaction, (Changed<Interaction>, With<BotonMultijugador>)>,
    raiz: Query<Entity, With<PanelMultiRaiz>>,
    metas: Res<crate::metas::Metas>,
    bajas: Res<crate::infectados::Bajas>,
) {
    let (mis_pts, mis_baj) = (metas.puntos(), bajas.total);
    if abrir.iter().any(|i| *i == Interaction::Pressed) {
        panel.abierto = !panel.abierto;
    }
    let quiero = (panel.abierto, multi.version, mis_pts, mis_baj);
    if panel.dibujado == Some(quiero) {
        return;
    }
    panel.dibujado = Some(quiero);
    for e in &raiz {
        commands.entity(e).despawn_recursive();
    }
    if !panel.abierto {
        return;
    }
    let online = servidor_online();
    let estado = match multi.estado {
        EstadoRed::Fuera => "sin conexión".to_string(),
        EstadoRed::Conectando => "conectando...".to_string(),
        EstadoRed::Dentro => format!("{} jugadores en la sala", multi.remotos.len() + 1),
    };
    let ip = if multi.es_anfitrion() {
        format!("Te encuentran en {}:{}", red::ip_local().unwrap_or_else(|| "?".into()), red::PUERTO)
    } else {
        String::new()
    };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(120.0),
                width: Val::Px(420.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                padding: UiRect::all(Val::Px(8.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.07, 0.12)),
            BorderColor(color_texto()),
            crate::ui_pack::MarcoDeHud(crate::ui_pack::marco::PANEL),
            GlobalZIndex(850),
            FocusPolicy::Block,
            Interaction::default(),
            PanelMultiRaiz,
            Name::new("PanelMulti"),
        ))
        .with_children(|p| {
            let linea = |p: &mut ChildBuilder, t: String, px: f32, c: Color| {
                p.spawn((Text::new(t), TextFont { font_size: px, ..default() }, TextColor(c)));
            };
            linea(p, "MULTIJUGADOR".into(), 20.0, crate::ui_pack::Pack::TINTA);
            linea(p, estado, 13.0, Color::srgb(0.55, 0.33, 0.05));
            if !multi.mensaje.is_empty() {
                linea(p, multi.mensaje.clone(), 12.0, crate::ui_pack::Pack::SUAVE);
            }
            if !ip.is_empty() {
                linea(p, ip, 12.0, crate::ui_pack::Pack::SUAVE);
            }
            // Ranking de la sala (OASIS): vos y los demás, por puntos.
            let en = idioma_actual() == Idioma::En;
            let mut fila_rank: Vec<(String, u32, u32)> =
                vec![(if en { "You".into() } else { "Vos".into() }, mis_pts, mis_baj)];
            for r in multi.remotos.values().take(7) {
                fila_rank.push((r.nombre.clone(), r.pts, r.baj));
            }
            fila_rank.sort_by_key(|f| std::cmp::Reverse(f.1 + f.2 * 2));
            for (n, pt, bj) in &fila_rank {
                linea(p, format!("· {n}  {pt} pts · {bj} {}", if en { "kills" } else { "bajas" }), 12.0, crate::ui_pack::Pack::TINTA);
            }
            let total_baj: u32 = fila_rank.iter().map(|f| f.2).sum();
            if multi.conectado() {
                linea(
                    p,
                    if en { format!("Room total: {total_baj} infected down") } else { format!("Total de la sala: {total_baj} infectados abatidos") },
                    12.0,
                    Color::srgb(0.55, 0.33, 0.05),
                );
            }
            let fila = Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(6.0),
                row_gap: Val::Px(6.0),
                margin: UiRect::top(Val::Px(6.0)),
                ..default()
            };
            p.spawn(fila.clone()).with_children(|f| {
                boton(
                    f,
                    if multi.es_anfitrion() { "CERRAR LAN".into() } else { "ANFITRIÓN LAN".into() },
                    AccionMulti::Anfitrion,
                    multi.es_anfitrion(),
                );
                if online.is_some() {
                    boton(
                        f,
                        if multi.conectado() && !multi.es_anfitrion() { "DESCONECTAR".into() } else { "ONLINE".into() },
                        AccionMulti::Online,
                        multi.conectado() && !multi.es_anfitrion(),
                    );
                }
                boton(f, "CERRAR".into(), AccionMulti::Cerrar, false);
            });
            if multi.conectado() {
                p.spawn(fila).with_children(|f| {
                    for fr in FRASES {
                        boton(f, fr.to_string(), AccionMulti::Frase(fr), false);
                    }
                });
            }
        });
}

fn boton(p: &mut ChildBuilder, etiqueta: String, a: AccionMulti, activo: bool) {
    let c = if activo { Color::srgb(1.0, 0.4, 0.75) } else { color_texto() };
    p.spawn((
        Button,
        Node {
            height: Val::Px(34.0),
            border: UiRect::all(Val::Px(2.0)),
            padding: UiRect::horizontal(Val::Px(8.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor(c),
        BackgroundColor(Color::srgb(0.10, 0.14, 0.22)),
        a,
    ))
    .with_children(|b| {
        b.spawn((Text::new(etiqueta), TextFont { font_size: 14.0, ..default() }, TextColor(c)));
    });
}

#[allow(clippy::too_many_arguments)]
fn botones_multi(
    mut multi: ResMut<Multi>,
    mut panel: ResMut<PanelMulti>,
    sel: Res<SelectedCharacter>,
    botones: Query<(&Interaction, &AccionMulti), Changed<Interaction>>,
    player: Query<&Transform, With<Player>>,
    mut speech: ResMut<crate::dialog::SpeechQueue>,
    mut commands: Commands,
) {
    for (i, a) in &botones {
        if *i != Interaction::Pressed {
            continue;
        }
        let nombre = if sel.name.is_empty() { "Viajero".to_string() } else { sel.name.clone() };
        let piel = piel_para(&nombre);
        let semilla = ha_shared::partida::semilla();
        match a {
            AccionMulti::Anfitrion => {
                if multi.es_anfitrion() {
                    multi.cerrar_todo();
                } else {
                    let mundo = crate::partida::activa().unwrap_or_else(|| "Mundo".into());
                    match multi.alojar(&format!("Mundo de {nombre}"), semilla, &nombre, &piel) {
                        Ok(dir) => {
                            eprintln!("ha-multi: anfitrión en {dir} (mundo {mundo}, semilla {semilla})");
                            multi.mensaje = format!("anfitrión · {dir}");
                        }
                        Err(e) => multi.mensaje = e,
                    }
                }
            }
            AccionMulti::Online => {
                if multi.conectado() && !multi.es_anfitrion() {
                    multi.cerrar_todo();
                } else if let Some(dir) = servidor_online() {
                    multi.conectar(dir, &nombre, &piel, semilla, Origen::Juego);
                }
            }
            AccionMulti::Frase(f) => {
                multi.decir(f);
                // Uno se ve diciéndolo también.
                if let Ok(tf) = player.get_single() {
                    crate::dialog::say(&mut speech, tf.translation.x, -tf.translation.y, *f, color_texto());
                }
            }
            AccionMulti::Cerrar => panel.abierto = false,
        }
        multi.tocar();
    }
    // Si el servidor te dio otra semilla que la de tu mundo, avisar (no se
    // puede cambiar de mundo a mitad de partida).
    if multi.conectado() && multi.semilla_sala != 0 && multi.semilla_sala != ha_shared::partida::semilla() && multi.origen == Some(Origen::Juego) {
        let m = format!("la sala usa el mundo {}: entrá desde el menú", multi.semilla_sala);
        if multi.mensaje != m {
            multi.mensaje = m;
            multi.tocar();
        }
    }
    let _ = &mut commands;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_piel_es_estable_y_esta_en_las_veinte() {
        assert_eq!(piel_para("Ana"), piel_para("Ana"));
        for n in ["Ana", "Beto", "", "Zoë", "X"] {
            let p = piel_para(n);
            let k: u32 = p.trim_start_matches("npc/char_").parse().unwrap();
            assert!((1..=20).contains(&k), "{p}");
        }
    }

    #[test]
    fn el_invitado_toma_la_semilla_de_la_sala() {
        let hub = Hub::levantar(0, 777).unwrap();
        let mut invitado = Multi::default();
        invitado.conectar(format!("127.0.0.1:{}", hub.puerto), "Beto", "npc/char_2", 0, Origen::Menu);
        let mut app = App::new();
        app.insert_resource(invitado).add_systems(Update, sondear_red);
        let fin = Instant::now() + Duration::from_secs(5);
        while Instant::now() < fin && app.world().resource::<Multi>().estado != EstadoRed::Dentro {
            app.update();
            std::thread::sleep(Duration::from_millis(50));
        }
        let m = app.world().resource::<Multi>();
        assert_eq!(m.estado, EstadoRed::Dentro);
        assert_eq!(m.semilla_sala, 777);
        assert_eq!(hub.jugadores(), 1);
    }
}
