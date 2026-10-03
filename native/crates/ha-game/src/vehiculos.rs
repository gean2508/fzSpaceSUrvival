//! **Vehículos en el mundo: tránsito, estacionados, manejar y robar.**
//!
//! La física, el catálogo y el conductor son de `ha_shared::vehiculos` (puros y con
//! tests). Acá vive lo que es del cliente:
//!
//! - **Tránsito con cupo** ([`manejar_transito`]): hasta [`CUPO_TRANSITO`] vehículos
//!   alrededor del jugador, sembrados en una calzada con mano FUERA de cámara —
//!   uno por cuadro como mucho (checklist §2: materialización con cupo)—, y
//!   borrados al quedar lejos. Uno de cada cinco nace **estacionado** contra el
//!   cordón: es el auto que se toma sin robárselo a nadie.
//! - **Subir, bajar, robar** ([`subir_bajar`]): E, o tocar el vehículo estando al
//!   lado. Si lo maneja un vecino, se baja corriendo y el auto es tuyo — en el
//!   momento, como en GTA.
//! - **Manejar** ([`fisica_vehiculos`]): el stick apunta el rumbo
//!   (`mando_desde_stick`). Mientras manejás, el jugador ES el auto para el resto
//!   del juego: su `SimState` sigue al vehículo, así que la cámara, la carga de
//!   chunks y todo lo que pregunta "¿dónde está el jugador?" lo siguen sin saber
//!   que existe un auto.
//! - **Atropellar**: el paragolpes deja tres [`crate::armas::Bala`] invisibles por
//!   paso; los que ya saben recibir un balazo (vecinos, fauna, infectados) reciben
//!   el golpe con sus testigos y su muerte, sin una línea nueva de su lado.
//!
//! El suelo se consulta por un caché por celda ([`CacheSuelo`]): `topdown_tile`
//! recalcula ruido en cada pregunta y un conductor pregunta unas cien veces por
//! paso.

use std::cell::RefCell;
use std::collections::HashMap;

use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use ha_shared::vehiculos::{self as veh, Clase, Conductor, Estado, Ficha, Mando, Suelo};

use crate::audio::{PlaySfxAt, Sfx};
use crate::camera::{CameraShakeEvent, GameCamera};
use crate::game_phase::GamePhase;
use crate::input::{touch_blocked, touch_in_move_zone, MovementIntent, TouchBlockRects, TouchLayout};
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, PlayerVisual, SimState};
use crate::world2d::{active_surface_body, render_xy, sort_z, ActivePlanet, TileWorld};

/// Cuántos vehículos de tránsito hay a la vez alrededor del jugador. Cada uno es
/// un sprite y un conductor a 20 Hz: diez se pagan holgado en el Redmi.
pub const CUPO_TRANSITO: usize = 10;
/// Más allá de esto el tránsito se borra (y el que usaste, a [`LEJOS_USADO`]).
const LEJOS_TRANSITO: f32 = 120.0;
const LEJOS_USADO: f32 = 400.0;
/// Desde cuán cerca se sube (del centro del vehículo, más su medio largo).
const ALCANCE_SUBIR: f32 = 2.2;
/// Píxeles por metro del pack (1 tile = 32 px = 1 m).
const PX_M: f32 = 32.0;
const TAM_VECINO: Vec2 = Vec2::new(0.95, 1.9);

pub struct VehiculosPlugin;

impl Plugin for VehiculosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Manejando>()
            .init_resource::<CacheSuelo>()
            .init_resource::<Transito>()
            .add_systems(
                FixedUpdate,
                fisica_vehiculos.after(crate::sim::tick_sim).run_if(in_state(GamePhase::Playing)),
            )
            .add_systems(
                Update,
                (manejar_transito, subir_bajar, dibujar_vehiculos, huir, hud_vehiculo)
                    .chain()
                    .run_if(in_state(GamePhase::Playing)),
            )
            .add_systems(
                PostUpdate,
                ocultar_al_conductor
                    .before(bevy::render::view::VisibilitySystems::VisibilityPropagate)
                    .run_if(in_state(GamePhase::Playing)),
            );
    }
}

/// El vehículo que maneja el jugador, si alguno.
#[derive(Resource, Default)]
pub struct Manejando {
    pub auto: Option<Entity>,
}

impl Manejando {
    pub fn activo(&self) -> bool {
        self.auto.is_some()
    }
}

#[derive(Resource, Default)]
pub struct CacheSuelo {
    cuerpo: u8,
    mapa: HashMap<(i64, i64), Suelo>,
}

#[derive(Resource, Default)]
struct Transito {
    semilla: u32,
}

#[derive(Clone, Copy, Debug)]
pub enum Quien {
    Nadie,
    Jugador,
    Vecino(Conductor),
}

#[derive(Component)]
pub struct Vehiculo {
    pub clave: &'static str,
    pub clase: Clase,
    pub ficha: Ficha,
    pub estado: Estado,
    pub quien: Quien,
    /// Lo manejó el jugador: no se borra con el tránsito (lo dejaste ahí, ahí está).
    pub usado: bool,
    /// Para interpolar el dibujo entre pasos de 20 Hz.
    prev: (f32, f32),
    pose: &'static str,
    uid: u32,
    sirena: f32,
}

/// El vecino al que le robaste el auto, corriendo.
#[derive(Component)]
struct Huyendo {
    dir: Vec2,
    vida: f32,
    skin: String,
    pose: String,
}

#[derive(Component)]
struct HudVehiculo;

/// ¿Un círculo a pie choca con algún vehículo? Lo usa `sim::tick_sim` para que el
/// jugador no atraviese los autos.
pub fn frena_a_pie<'a>(autos: impl Iterator<Item = &'a Vehiculo>, x: f32, z: f32, r: f32) -> bool {
    autos.into_iter().any(|v| {
        let (ps, n, rv) = v.estado.probes(v.estado.x, v.estado.z, v.estado.rumbo);
        ps.iter().take(n).any(|&(px, pz)| (px - x).powi(2) + (pz - z).powi(2) < (rv + r).powi(2))
    })
}

fn pose_de(rumbo: f32) -> &'static str {
    let (dx, dz) = (rumbo.cos(), rumbo.sin());
    if dz.abs() >= dx.abs() {
        if dz > 0.0 { "abajo" } else { "arriba" }
    } else if dx < 0.0 {
        "izquierda"
    } else {
        "derecha"
    }
}

/// Largo y ancho en metros, MEDIDOS del sprite de frente (`abajo`): el pack
/// dibujó el auto de 2,3 × 4,1 m y así choca.
fn medidas(art: &crate::art2d::ArtLibrary, clave: &str) -> Option<(f32, f32)> {
    let ([_, _, w, h], _) = art.pose(clave, "abajo")?;
    Some((h as f32 / PX_M, w as f32 / PX_M))
}

fn suelo_cacheado(cache: &RefCell<&mut CacheSuelo>, tiles: &TileWorld, body: ha_shared::CelestialBody, tx: i64, tz: i64) -> Suelo {
    if let Some(s) = cache.borrow().mapa.get(&(tx, tz)) {
        return *s;
    }
    let s = veh::suelo_de(tiles.tile_at(body, tx, tz));
    cache.borrow_mut().mapa.insert((tx, tz), s);
    s
}

fn hash(a: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^ (x >> 12)
}

// ---------------------------------------------------------------------------
// Física (20 Hz)
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn fisica_vehiculos(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    intent: Res<MovementIntent>,
    mut manejando: ResMut<Manejando>,
    world_ctx: Res<WorldContextRuntime>,
    planet_ctx: Res<ActivePlanet>,
    obs: crate::obstaculos::Obstaculos,
    mut cache: ResMut<CacheSuelo>,
    npcs: Query<&crate::society::Npc>,
    mut autos: Query<(Entity, &mut Vehiculo)>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
    (mut sfx, mut shake): (EventWriter<PlaySfxAt>, EventWriter<CameraShakeEvent>),
) {
    let _p = crate::debug::probe("fx_vehiculos");
    let dt = crate::game_time::fixed_delta_secs(&time);
    let body = active_surface_body(&world_ctx, &planet_ctx);
    if cache.cuerpo != body as u8 || cache.mapa.len() > 60_000 {
        cache.mapa.clear();
        cache.cuerpo = body as u8;
    }
    if manejando.auto.is_some_and(|e| autos.get(e).is_err()) {
        manejando.auto = None;
    }
    let pj = player.get_single().map(|(s, _)| (s.body.x, s.body.z)).ok();
    // Quién está dónde, para que nadie se coma al de adelante.
    let fotos: Vec<(Entity, f32, f32, f32)> =
        autos.iter().map(|(e, v)| (e, v.estado.x, v.estado.z, v.estado.ancho * 0.5)).collect();
    let peatones: Vec<(f32, f32)> = npcs.iter().map(|n| (n.x, n.z)).collect();
    let cache = RefCell::new(&mut *cache);
    let tiles = obs.tiles();
    let suelo = |tx: i64, tz: i64| suelo_cacheado(&cache, tiles, body, tx, tz);
    let calzada = |tx: i64, tz: i64| suelo(tx, tz) == Suelo::Asfalto;

    for (e, mut v) in &mut autos {
        v.prev = (v.estado.x, v.estado.z);
        let mut quien = v.quien;
        let mando = match &mut quien {
            Quien::Jugador => veh::mando_desde_stick(&v.estado, intent.move_x, intent.move_z),
            Quien::Nadie => Mando::default(),
            Quien::Vecino(c) => {
                let mut c2 = *c;
                let (fx, fz) = (v.estado.rumbo.cos(), v.estado.rumbo.sin());
                let mut libre = f32::INFINITY;
                let mut mira = |x: f32, z: f32, r: f32| {
                    let (dx, dz) = (x - v.estado.x, z - v.estado.z);
                    let ad = dx * fx + dz * fz;
                    let lat = (dz * fx - dx * fz).abs();
                    if ad > 0.0 && lat < v.estado.ancho * 0.5 + r + 0.3 {
                        libre = libre.min(ad - r);
                    }
                };
                for &(o, x, z, r) in &fotos {
                    if o != e {
                        mira(x, z, r);
                    }
                }
                for &(x, z) in &peatones {
                    mira(x, z, 0.35);
                }
                if let Some((x, z)) = pj {
                    if !manejando.activo() {
                        mira(x, z, 0.35);
                    }
                }
                let m = veh::conducir(&v.estado, &v.ficha, &mut c2, &calzada, libre);
                *c = c2;
                m
            }
        };
        v.quien = quien;
        let ficha = v.ficha;
        let yo = e;
        let choca = |x: f32, z: f32, r: f32| {
            obs.solido(body, x.floor() as i64, z.floor() as i64)
                || obs.caja_frena(body, x, z, r)
                || fotos.iter().any(|&(o, ox, oz, or)| o != yo && (ox - x).powi(2) + (oz - z).powi(2) < (or + r).powi(2))
        };
        let paso = veh::paso(&mut v.estado, mando, &ficha, dt, &suelo, &choca);
        let r = render_xy(v.estado.x, v.estado.z);
        if paso.choque > veh::CHOQUE_QUE_DANIA {
            sfx.send(PlaySfxAt(Sfx::Stamp, r));
            if matches!(v.quien, Quien::Jugador) {
                shake.send(CameraShakeEvent { trauma: (paso.choque / 25.0).min(0.6) });
            }
        }
        // El paragolpes: tres golpes invisibles a lo ancho, que recorren lo que el
        // auto recorre este paso.
        let vel = v.estado.vel;
        if vel.abs() > 4.0 {
            let (fx, fz) = (v.estado.rumbo.cos(), v.estado.rumbo.sin());
            let sentido = vel.signum();
            let frente = Vec2::new(v.estado.x, v.estado.z) + Vec2::new(fx, fz) * sentido * v.estado.largo * 0.5;
            let lado = Vec2::new(-fz, fx) * v.estado.ancho * 0.33;
            let v2 = Vec2::new(fx, fz) * vel;
            for k in [-1.0f32, 0.0, 1.0] {
                let p = frente + lado * k;
                commands.spawn((
                    crate::armas::Bala {
                        pos: p,
                        prev: p - v2 * dt,
                        vel: v2,
                        dano: (vel.abs() - 3.0) * 0.9 * (ficha.salud / 100.0).clamp(0.5, 2.0),
                        origen: p,
                        alcance: (vel.abs() * dt).max(0.05),
                        atraviesa: 3,
                        golpeados: Vec::new(),
                        viva: true,
                        impacto: Sfx::Hurt,
                        rayo: false,
                    },
                    Transform::from_xyz(p.x, -p.y, 18.0),
                ));
            }
        }
        if matches!(v.quien, Quien::Jugador) {
            if let Ok((mut sim, mut visual)) = player.get_single_mut() {
                visual.from_x = v.prev.0;
                visual.from_z = v.prev.1;
                visual.to_x = v.estado.x;
                visual.to_z = v.estado.z;
                visual.to_rot = v.estado.rumbo;
                sim.body.x = v.estado.x;
                sim.body.z = v.estado.z;
                sim.body.rot = v.estado.rumbo;
                sim.body.vx = 0.0;
                sim.body.vz = 0.0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tránsito: sembrar y borrar con cupo
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn manejar_transito(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    world_ctx: Res<WorldContextRuntime>,
    planet_ctx: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    zoom: Res<crate::camera::CameraZoom>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    mut cache: ResMut<CacheSuelo>,
    mut transito: ResMut<Transito>,
    manejando: Res<Manejando>,
    autos: Query<(Entity, &Vehiculo)>,
    player: Query<&SimState, With<Player>>,
) {
    let Ok(pj) = player.get_single() else { return };
    let (px, pz) = (pj.body.x, pj.body.z);
    let body = active_surface_body(&world_ctx, &planet_ctx);
    let en_superficie = tiles.z == 0 && !matches!(world_ctx.active, ha_shared::WorldContext::Space);
    // Borrar lo lejano (o todo el tránsito si ya no estás en la superficie).
    let mut vivos = 0;
    for (e, v) in &autos {
        let d = ((v.estado.x - px).powi(2) + (v.estado.z - pz).powi(2)).sqrt();
        let tope = if v.usado { LEJOS_USADO } else { LEJOS_TRANSITO };
        if Some(e) != manejando.auto && (d > tope || (!en_superficie && !v.usado)) {
            commands.entity(e).despawn_recursive();
        } else if !v.usado {
            vivos += 1;
        }
    }
    if !en_superficie || fly.flying() || vivos >= CUPO_TRANSITO || !ha_shared::geography::has_civilisation(body) {
        return;
    }
    // Un intento de siembra por cuadro, FUERA de cámara.
    let afuera = (zoom.live_view_h * 1.05).max(36.0);
    transito.semilla = hash(transito.semilla.wrapping_add(0x51ed));
    let h = transito.semilla;
    let ang = (h % 3600) as f32 / 3600.0 * std::f32::consts::TAU;
    let dist = afuera + ((h >> 12) % 40) as f32;
    let (tx, tz) = ((px + ang.cos() * dist).floor() as i64, (pz + ang.sin() * dist).floor() as i64);
    let cache = RefCell::new(&mut *cache);
    let calzada = |x: i64, z: i64| suelo_cacheado(&cache, &tiles, body, x, z) == Suelo::Asfalto;
    if !calzada(tx, tz) || ha_shared::roads::cruce_con(&calzada, tx, tz) {
        return;
    }
    let Some(o) = ha_shared::roads::carril_con(&calzada, tx, tz) else { return };
    let (fx, fz) = veh::dir_octante(o);
    let (rx, rz) = (-fz, fx);
    // La calle tiene que dar para dos manos (5 tiles): un callejón no lleva tránsito.
    let ancho = 1
        + (1..=6).take_while(|&k| calzada(tx + rx * k, tz + rz * k)).count()
        + (1..=6).take_while(|&k| calzada(tx - rx * k, tz - rz * k)).count();
    if ancho < 5 {
        return;
    }
    if autos.iter().any(|(_, v)| (v.estado.x - tx as f32).powi(2) + (v.estado.z - tz as f32).powi(2) < 100.0) {
        return;
    }
    let clave = veh::modelo_de_transito(hash(h ^ 0xabcd));
    let Some(clase) = veh::clase_de(clave) else { return };
    let Some((largo, ancho_m)) = medidas(&art, clave) else { return };
    let ficha = veh::ficha(clase);
    // Uno de cada cinco, estacionado contra el cordón de su mano.
    let estacionado = (h >> 20) % 5 == 0;
    let (mut x, mut z) = (tx as f32 + 0.5, tz as f32 + 0.5);
    if estacionado {
        let al_cordon = (1..=6).take_while(|&k| calzada(tx + rx * k, tz + rz * k)).count() as f32;
        x += rx as f32 * (al_cordon + 0.5 - ancho_m * 0.5);
        z += rz as f32 * (al_cordon + 0.5 - ancho_m * 0.5);
    }
    let rumbo = -(o as f32) * std::f32::consts::FRAC_PI_4;
    let estado = Estado::nuevo(x, z, rumbo, &ficha, largo, ancho_m);
    let quien = if estacionado { Quien::Nadie } else { Quien::Vecino(Conductor::nuevo(o, h)) };
    spawn_vehiculo(&mut commands, &art, clave, clase, estado, quien, false, h);
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_vehiculo(
    commands: &mut Commands,
    art: &crate::art2d::ArtLibrary,
    clave: &'static str,
    clase: Clase,
    estado: Estado,
    quien: Quien,
    usado: bool,
    uid: u32,
) -> Option<Entity> {
    let pose = pose_de(estado.rumbo);
    let ([_, _, w, h], _) = art.pose(clave, pose)?;
    let sprite = art.pose_sprite(clave, pose, Vec2::new(w as f32 / PX_M, h as f32 / PX_M))?;
    let r = render_xy(estado.x, estado.z);
    Some(
        commands
            .spawn((
                Vehiculo {
                    clave,
                    clase,
                    ficha: veh::ficha(clase),
                    estado,
                    quien,
                    usado,
                    prev: (estado.x, estado.z),
                    pose,
                    uid,
                    sirena: 0.0,
                },
                sprite,
                Transform::from_xyz(r.x, r.y, sort_z(estado.z)),
                Name::new("Vehiculo"),
            ))
            .id(),
    )
}

// ---------------------------------------------------------------------------
// Subir, bajar, robar
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn subir_bajar(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut touches: EventReader<TouchInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    (touch_layout, blocks): (Res<TouchLayout>, Res<TouchBlockRects>),
    (gameplay_block, fly): (Res<crate::game_ui::UiGameplayBlock>, Res<crate::creative_fly::CreativeFlight>),
    art: Res<crate::art2d::ArtLibrary>,
    (world_ctx, planet_ctx): (Res<WorldContextRuntime>, Res<ActivePlanet>),
    obs: crate::obstaculos::Obstaculos,
    mut manejando: ResMut<Manejando>,
    mut autos: Query<(Entity, &mut Vehiculo)>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
    mut sfx: EventWriter<PlaySfxAt>,
) {
    let tecla = keys.just_pressed(KeyCode::KeyG) || keys.just_pressed(KeyCode::Enter);
    let pantalla = if mouse.just_pressed(MouseButton::Left) {
        windows.get_single().ok().and_then(|w| w.cursor_position())
    } else {
        touches
            .read()
            .find(|t| {
                t.phase == TouchPhase::Started
                    && !touch_in_move_zone(t.position, &touch_layout)
                    && !touch_blocked(t.position, &blocks.0)
            })
            .map(|t| t.position)
    };
    touches.clear();
    let Ok((mut sim, mut visual)) = player.get_single_mut() else { return };
    let body = active_surface_body(&world_ctx, &planet_ctx);

    // Despegar volando te baja del auto: el personaje no se lleva el auto al cielo.
    let forzar = manejando.activo() && fly.flying();
    if gameplay_block.blocked && !forzar {
        return;
    }
    let punto = pantalla.and_then(|s| {
        let (camera, cam_tf) = cam.get_single().ok()?;
        let win = windows.get_single().ok()?;
        crate::render::scale::window_to_world_2d(camera, cam_tf, win, s).map(|r| Vec2::new(r.x, -r.y))
    });

    if let Some(e) = manejando.auto {
        let Ok((_, mut v)) = autos.get_mut(e) else {
            manejando.auto = None;
            return;
        };
        let toco_mi_auto = punto.is_some_and(|p| (p - Vec2::new(v.estado.x, v.estado.z)).length() < v.estado.largo * 0.5);
        if !(tecla || toco_mi_auto || forzar) {
            return;
        }
        if v.estado.vel.abs() > 6.0 && !forzar {
            return; // nadie se tira de un auto a 25 km/h
        }
        // Se baja por la puerta del conductor (izquierda), o por donde haya lugar.
        let (fx, fz) = (v.estado.rumbo.cos(), v.estado.rumbo.sin());
        let (lx, lz) = (fz, -fx);
        let a = v.estado.ancho * 0.5 + 0.6;
        let b = v.estado.largo * 0.5 + 0.6;
        let r = ha_shared::topdown::PLAYER_RADIUS;
        let salida = [(lx * a, lz * a), (-lx * a, -lz * a), (fx * b, fz * b), (-fx * b, -fz * b)]
            .into_iter()
            .map(|(dx, dz)| (v.estado.x + dx, v.estado.z + dz))
            .find(|&(x, z)| obs.libre(body, x, z, r) && !obs.caja_frena(body, x, z, r));
        let Some((x, z)) = salida.or(forzar.then_some((v.estado.x, v.estado.z))) else { return };
        v.quien = Quien::Nadie;
        v.usado = true;
        sim.body.x = x;
        sim.body.z = z;
        visual.from_x = x;
        visual.to_x = x;
        visual.from_z = z;
        visual.to_z = z;
        manejando.auto = None;
        sfx.send(PlaySfxAt(Sfx::DoorOpen, render_xy(x, z)));
        return;
    }

    if !(tecla || punto.is_some()) || fly.flying() {
        return;
    }
    let yo = Vec2::new(sim.body.x, sim.body.z);
    let mut mejor: Option<(Entity, f32)> = None;
    for (e, v) in &autos {
        let c = Vec2::new(v.estado.x, v.estado.z);
        let d = c.distance(yo) - v.estado.largo * 0.5;
        if d > ALCANCE_SUBIR || v.estado.chatarra() {
            continue;
        }
        if let Some(p) = punto {
            if p.distance(c) > v.estado.largo * 0.5 + 0.3 {
                continue;
            }
        }
        if mejor.is_none_or(|(_, m)| d < m) {
            mejor = Some((e, d));
        }
    }
    let Some((e, _)) = mejor else { return };
    let Ok((_, mut v)) = autos.get_mut(e) else { return };
    if let Quien::Vecino(_) = v.quien {
        // ROBO: el vecino se baja corriendo para el lado contrario.
        let skin = format!("npc/char_{}", v.uid % 20 + 1);
        let huye = (Vec2::new(v.estado.x, v.estado.z) - yo).normalize_or(Vec2::X);
        let sale = Vec2::new(v.estado.x, v.estado.z) + huye * (v.estado.ancho * 0.5 + 0.7);
        if let Some(sprite) = art.pose_sprite(&skin, "walk_down", TAM_VECINO).or_else(|| Some(art.sprite_or_color(&skin, Color::srgb(0.85, 0.82, 0.75), TAM_VECINO))) {
            let r = render_xy(sale.x, sale.y);
            commands.spawn((
                Huyendo { dir: huye, vida: 6.0, skin, pose: String::new() },
                sprite,
                Transform::from_xyz(r.x, r.y, sort_z(sale.y)),
            ));
        }
    }
    v.estado.vel = 0.0;
    v.quien = Quien::Jugador;
    v.usado = true;
    manejando.auto = Some(e);
    sfx.send(PlaySfxAt(Sfx::DoorClose, render_xy(v.estado.x, v.estado.z)));
}

fn huir(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<crate::art2d::ArtLibrary>,
    mut q: Query<(Entity, &mut Huyendo, &mut Transform, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    for (e, mut h, mut tf, mut sprite) in &mut q {
        h.vida -= dt;
        if h.vida <= 0.0 {
            commands.entity(e).despawn_recursive();
            continue;
        }
        let paso = h.dir * 4.5 * dt;
        tf.translation.x += paso.x;
        tf.translation.y -= paso.y;
        tf.translation.z = sort_z(-tf.translation.y);
        let dir = if h.dir.y.abs() >= h.dir.x.abs() {
            if h.dir.y > 0.0 { "down" } else { "up" }
        } else if h.dir.x < 0.0 {
            "left"
        } else {
            "right"
        };
        let pose = format!("walk_{dir}");
        if h.pose != pose {
            if let Some(s) = art.pose_sprite(&h.skin, &pose, TAM_VECINO) {
                *sprite = s;
            }
            h.pose = pose;
        }
    }
}

// ---------------------------------------------------------------------------
// Dibujo
// ---------------------------------------------------------------------------

fn dibujar_vehiculos(
    time: Res<Time<Fixed>>,
    real: Res<Time>,
    art: Res<crate::art2d::ArtLibrary>,
    tiles: Res<TileWorld>,
    mut q: Query<(&mut Vehiculo, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    let t = time.overstep_fraction() as f32;
    for (mut v, mut tf, mut sprite, mut vis) in &mut q {
        *vis = if tiles.z == 0 { Visibility::Inherited } else { Visibility::Hidden };
        let x = v.prev.0 + (v.estado.x - v.prev.0) * t;
        let z = v.prev.1 + (v.estado.z - v.prev.1) * t;
        let mut pose = pose_de(v.estado.rumbo);
        // La ambulancia con alguien al volante lleva la sirena prendida.
        if v.clase == Clase::Ambulancia && !matches!(v.quien, Quien::Nadie) {
            v.sirena += real.delta_secs();
            if (v.sirena * 2.5) as u32 % 2 == 1 {
                pose = match pose {
                    "abajo" => "abajo_luz",
                    "arriba" => "arriba_luz",
                    "izquierda" => "izquierda_luz",
                    _ => "derecha_luz",
                };
            }
        }
        if v.pose != pose {
            if let Some(([_, _, w, h], _)) = art.pose(v.clave, pose) {
                if let Some(s) = art.pose_sprite(v.clave, pose, Vec2::new(w as f32 / PX_M, h as f32 / PX_M)) {
                    *sprite = s;
                }
            }
            v.pose = pose;
        }
        if v.estado.chatarra() {
            sprite.color = Color::srgb(0.35, 0.33, 0.32);
        }
        let medio_alto = sprite.custom_size.map_or(1.0, |s| s.y * 0.5);
        let r = render_xy(x, z);
        // Se ordena por el PIE del sprite (su borde sur), como todo lo que se para en el suelo.
        tf.translation = Vec3::new(r.x, r.y, sort_z(z + medio_alto - 0.4));
    }
}

/// El que maneja no se dibuja: va adentro.
fn ocultar_al_conductor(manejando: Res<Manejando>, mut q: Query<&mut Visibility, With<Player>>) {
    if manejando.activo() {
        for mut v in &mut q {
            *v = Visibility::Hidden;
        }
    }
}

fn hud_vehiculo(
    mut commands: Commands,
    manejando: Res<Manejando>,
    layout: Res<TouchLayout>,
    autos: Query<&Vehiculo>,
    player: Query<&SimState, With<Player>>,
    mut q: Query<(&mut Text, &mut Visibility), With<HudVehiculo>>,
) {
    if q.is_empty() {
        let k = crate::ui_pack::k(&layout);
        commands.spawn((
            Text::new(""),
            TextFont { font_size: (20.0 * k).clamp(14.0, 24.0), ..default() },
            TextColor(Color::srgb(0.95, 0.95, 0.85)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px((64.0 * k).max(52.0)),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-200.0)),
                width: Val::Px(400.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            TextLayout::new_with_justify(JustifyText::Center),
            Visibility::Hidden,
            GlobalZIndex(41),
            HudVehiculo,
            Name::new("HudVehiculo"),
        ));
        return;
    }
    let tactil = crate::platform::uses_touch_gameplay_ui();
    let texto = if let Some(v) = manejando.auto.and_then(|e| autos.get(e).ok()) {
        let kmh = (v.estado.vel.abs() * 3.6).round() as i32;
        let salud = (v.estado.salud / v.ficha.salud * 100.0).max(0.0).round() as i32;
        let bajar = if tactil { "tocá el vehículo para bajar" } else { "G para bajar" };
        Some(format!("{} · {kmh} km/h · {salud}% · {bajar}", v.ficha.nombre))
    } else {
        player.get_single().ok().and_then(|pj| {
            autos
                .iter()
                .filter(|v| !v.estado.chatarra())
                .find(|v| {
                    ((v.estado.x - pj.body.x).powi(2) + (v.estado.z - pj.body.z).powi(2)).sqrt() - v.estado.largo * 0.5
                        < ALCANCE_SUBIR
                })
                .map(|v| {
                    let verbo = if matches!(v.quien, Quien::Vecino(_)) { "robar" } else { "subir" };
                    let como = if tactil { "Tocalo para" } else { "G para" };
                    format!("{} · {como} {verbo}", v.ficha.nombre)
                })
        })
    };
    for (mut t, mut vis) in &mut q {
        match &texto {
            Some(s) => {
                if t.0 != *s {
                    t.0 = s.clone();
                }
                *vis = Visibility::Visible;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Todo modelo del catálogo tiene arte con sus cuatro poses de rumbo — si el
    /// importador cambia una clave, esto falla antes que el tránsito quede invisible.
    #[test]
    fn todo_modelo_tiene_sus_cuatro_poses() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/art/manifest.json");
        let man: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for (clave, _) in veh::MODELOS {
            let e = &man["sprites"][clave];
            assert!(!e.is_null(), "{clave} no está en el manifest");
            for p in ["abajo", "arriba", "izquierda", "derecha"] {
                assert!(!e["poses"][p].is_null(), "{clave} no tiene la pose {p}");
            }
        }
    }

    #[test]
    fn la_pose_sigue_al_rumbo() {
        assert_eq!(pose_de(0.0), "derecha");
        assert_eq!(pose_de(std::f32::consts::PI), "izquierda");
        assert_eq!(pose_de(std::f32::consts::FRAC_PI_2), "abajo");
        assert_eq!(pose_de(-std::f32::consts::FRAC_PI_2), "arriba");
    }
}
