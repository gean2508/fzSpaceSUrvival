//! **Infectados: la noche tiene dientes** (Project Zomboid).
//!
//! Cuando cae el sol, en la superficie de la Tierra empiezan a salir infectados
//! del borde de lo que ves. Son lentos, pero no paran: te huelen desde lejos, te
//! siguen, y en grupo hacen daño de verdad. De día se queman.
//!
//! Lo que hace que sea un juego y no sólo un susto:
//! * **Las paredes los frenan.** Se mueven con el mismo `Obstaculos` que todo lo
//!   demás: una casa con la puerta cerrada, una cerca o un muro son defensa. La
//!   base importa.
//! * **Escalan con la noche y con la sala**: más cada día de juego (tope 8) y +2
//!   por cada otro jugador conectado — el coop los reparte, no los regala.
//! * **Se pelean con lo que lleves en la mano** (`fauna::dano_de`): el hacha pega
//!   más que el puño. Un toque sobre uno, a menos de `ALCANCE`, es un golpe.
//! * **Rinden**: sueltan lo que sueltan (comida, cuero, huesos) y cuentan para el
//!   marcador de bajas, que se muestra en la sala.
//! * La armadura (`equipo.rs`) reduce su daño como el de cualquier otro.
//!
//! Costo: ≤ 8 entidades con un sprite, movimiento por cuadro con 2 consultas de
//! colisión cada una, nada de pathfinding. Fuera de la Tierra y de noche cerrada
//! no corre.

use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::camera::GameCamera;
use crate::game_phase::GamePhase;
use crate::input::EquippedItem;
use crate::input::{touch_blocked, touch_in_move_zone, TouchBlockRects, TouchLayout};
use crate::obstaculos::Obstaculos;
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, SimState};
use crate::sky2d::DayClock;
use crate::world2d::{active_surface_body, sort_z, ActivePlanet};

/// Daylight por debajo del cual salen.
const NOCHE_MAX_LUZ: f32 = 0.30;
/// Daylight por encima del cual se queman.
const AMANECER_LUZ: f32 = 0.55;
const TOPE_BASE: usize = 2;
const TOPE_MAX: usize = 8;
const ANILLO_MIN: f32 = 20.0;
const ANILLO_MAX: f32 = 28.0;
/// Más lejos que esto de todos los jugadores, se van.
const OLVIDO: f32 = 70.0;
const OLFATO: f32 = 45.0;
/// A los vecinos los huelen de más cerca que a vos (no se arrastra a toda la horda sobre el pueblo).
const OLFATO_VECINO: f32 = 16.0;
const VEL: f32 = 2.1;
const VEL_CERCA: f32 = 2.6;
pub(crate) const ATAQUE_RADIO: f32 = 0.95;
pub(crate) const ATAQUE_CD: f32 = 1.2;
const DANO: f32 = 7.0;
const VIDA: f32 = 12.0;
/// Alcance del jugador para golpear (m) y radio del toque sobre el infectado.
const ALCANCE: f32 = 2.8;
const PICK_R: f32 = 1.3;
const GOLPE_CD: f32 = 0.45;
const SPAWN_CADA: f32 = 6.0;
const RADIO_CUERPO: f32 = 0.35;
/// Segundos empujando una puerta antes de abrirla a la fuerza.
const DERRIBO_S: f32 = 6.0;
/// Segundos hasta que un cadáver se descompone.
const VIDA_CADAVER: f32 = 420.0;
const TAMANO: Vec2 = Vec2::new(0.95, 1.9);

#[derive(Component)]
pub struct Infectado {
    pub(crate) x: f32,
    pub(crate) z: f32,
    pub(crate) vida: f32,
    pub(crate) ataque_cd: f32,
    /// Segundos de destello rojo tras un golpe.
    pub(crate) destello: f32,
    /// Para que no vayan todos en fila: un desvío fijo por individuo.
    desvio: f32,
    /// Segundos seguidos empujando contra algo que no cede.
    bloqueado: f32,
    /// Lo que llevaba encima (su "vida anterior"): se vuelve el inventario del cadáver.
    botin: Vec<(&'static str, u32)>,
    /// Custodia un alijo: no se quema de día.
    pub guardia: bool,
}

/// El cuerpo que queda: se abre como un cofre (`chests.rs`) y se descompone solo.
#[derive(Component)]
pub struct Cadaver {
    uid: u32,
    edad: f32,
}

/// Puntos (m) donde un infectado lleva `DERRIBO_S` empujando sin avanzar: `romper_paredes` decide
/// si ahí hay una pared de madera que ceda. La piedra y las fachadas NO ceden: defender la base
/// con piedra tiene que valer la pena.
#[derive(Resource, Default)]
pub struct GolpesAParedes(Vec<(f32, f32)>);

/// Golpes que aguanta una pared/ventana/baranda de madera.
const VIDA_PARED_MADERA: f32 = 3.0;

#[derive(Resource, Default)]
pub struct Bajas {
    pub total: u32,
}

#[derive(Resource, Default)]
struct Acumulado {
    spawn: f32,
    golpe: f32,
    quema: f32,
}

/// Qué llevaba encima, sorteado por `n`: algo de comer casi siempre, algo útil a veces, una
/// prenda del creador de personajes de vez en cuando y, muy raro, algo de valor. Nunca vacío.
pub fn botin_de(n: u32) -> Vec<(&'static str, u32)> {
    let mut s = n | 1;
    let mut azar = move |m: u32| {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s % m.max(1)
    };
    let mut v: Vec<(&'static str, u32)> = Vec::new();
    if azar(10) < 7 {
        v.push((["apple", "bread", "orange", "carrot", "egg", "pie"][azar(6) as usize], 1 + azar(2)));
    }
    if azar(10) < 5 {
        v.push((["stone", "sticks", "torch", "bone", "leather", "wood"][azar(6) as usize], 1 + azar(3)));
    }
    if azar(100) < 35 {
        v.push((ha_shared::armadura::prenda_al_azar(azar(1000)), 1));
    }
    if azar(100) < 8 {
        v.push((["gem_blue", "gold_ingot", "iron_ingot"][azar(3) as usize], 1));
    }
    if v.is_empty() {
        v.push(("apple", 1));
    }
    v
}

/// **Noche de horda**: cada tercera noche (día 2, 5, 8…) salen el doble y más seguido.
pub fn es_horda(dia: f32) -> bool {
    dia.max(0.0) as u32 % 3 == 2
}

/// Un infectado que custodia un alijo (`alijos.rs`): igual que los demás pero el sol no lo quema.
pub fn spawn_guardia(commands: &mut Commands, art: &crate::art2d::ArtLibrary, x: f32, z: f32, n: u32) {
    let skin = format!("npc/char_{}", 1 + n % 20);
    let mut sprite = art.sprite_or_color(&skin, Color::srgb(0.5, 0.7, 0.5), TAMANO);
    sprite.color = Color::srgb(0.55, 0.78, 0.55);
    commands.spawn((
        sprite,
        Transform::from_xyz(x, -z, sort_z(z)),
        Infectado {
            x,
            z,
            vida: VIDA * 1.3,
            ataque_cd: 0.0,
            destello: 0.0,
            desvio: ((n % 100) as f32 / 100.0 - 0.5) * 0.9,
            bloqueado: 0.0,
            botin: botin_de(n.wrapping_mul(2654435761)),
            guardia: true,
        },
        Name::new("Guardia"),
    ));
}

/// Cuántos pueden andar sueltos: crece con los días y con la gente en la sala.
pub fn tope(dia: f32, otros_jugadores: usize) -> usize {
    (TOPE_BASE + dia.max(0.0) as usize + otros_jugadores * 2).min(TOPE_MAX + otros_jugadores * 2)
}

pub struct InfectadosPlugin;

impl Plugin for InfectadosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Bajas>()
            .init_resource::<GolpesAParedes>()
            .init_resource::<Acumulado>()
            .add_systems(
                Update,
                (aparecer, mover_y_morder, romper_paredes, golpear, cadaveres, amanecer).chain().run_if(in_state(GamePhase::Playing)),
            )
            .add_systems(OnExit(GamePhase::Playing), limpiar);
    }
}

fn limpiar(mut commands: Commands, q: Query<Entity, Or<(With<Infectado>, With<Cadaver>)>>, mut bajas: ResMut<Bajas>, mut a: ResMut<Acumulado>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
    *bajas = Bajas::default();
    // (los golpes pendientes mueren con la partida)
    *a = Acumulado::default();
}

fn en_superficie(ctx: &WorldContextRuntime) -> bool {
    matches!(ctx.active, ha_shared::WorldContext::Overworld)
}

#[allow(clippy::too_many_arguments)]
fn aparecer(
    mut commands: Commands,
    time: Res<Time>,
    mut acc: ResMut<Acumulado>,
    day: Res<DayClock>,
    art: Res<crate::art2d::ArtLibrary>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    multi: Res<crate::multijugador::Multi>,
    obst: Obstaculos,
    player: Query<&Transform, With<Player>>,
    existentes: Query<(), With<Infectado>>,
) {
    let horda_ahora = es_horda(day.day_index()) && day.daylight() <= NOCHE_MAX_LUZ;
    acc.spawn += time.delta_secs();
    if acc.spawn < if horda_ahora { SPAWN_CADA * 0.4 } else { SPAWN_CADA } {
        return;
    }
    acc.spawn = 0.0;
    if !en_superficie(&ctx) || day.daylight() > NOCHE_MAX_LUZ {
        return;
    }
    let Ok(pt) = player.get_single() else { return };
    let horda = es_horda(day.day_index());
    let cupo = tope(day.day_index(), multi.remotos.len()) * if horda { 2 } else { 1 };
    if existentes.iter().count() >= cupo {
        return;
    }
    let body = active_surface_body(&ctx, &planet);
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    // Un ángulo y una distancia al azar; si cae en pared o agua, se pierde el intento.
    let s = time.elapsed().as_micros() as u64;
    let h = |k: u64| {
        let mut x = s ^ k.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        x ^= x >> 31;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        ((x >> 40) as f32) / (1u64 << 24) as f32
    };
    let ang = h(1) * std::f32::consts::TAU;
    let dist = ANILLO_MIN + h(2) * (ANILLO_MAX - ANILLO_MIN);
    let (x, z) = (px + ang.cos() * dist, pz + ang.sin() * dist);
    if obst.caja_frena(body, x, z, RADIO_CUERPO) || obst.solido(body, x.floor() as i64, z.floor() as i64) {
        return;
    }
    let skin = format!("npc/char_{}", 1 + (h(3) * 20.0) as u32 % 20);
    let mut sprite = art.sprite_or_color(&skin, Color::srgb(0.5, 0.7, 0.5), TAMANO);
    sprite.color = Color::srgb(0.55, 0.78, 0.55);
    commands.spawn((
        sprite,
        Transform::from_xyz(x, -z, sort_z(z)),
        Infectado { x, z, vida: VIDA, ataque_cd: 0.0, destello: 0.0, desvio: (h(4) - 0.5) * 0.9, bloqueado: 0.0, botin: botin_de((h(5) * 1.0e6) as u32), guardia: false },
        Name::new("Infectado"),
    ));
}

#[allow(clippy::too_many_arguments)]
fn mover_y_morder(
    time: Res<Time>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    obst: Obstaculos,
    mut pedidos: ResMut<crate::puertas::PedidosDePuerta>,
    mut golpes: ResMut<GolpesAParedes>,
    player: Query<&Transform, (With<Player>, Without<Infectado>)>,
    vecinos: Query<&crate::society::Npc>,
    mut sim: Query<&mut SimState, With<Player>>,
    mut q: Query<(&mut Infectado, &mut Transform, &mut Sprite)>,
    mut ultimo: Local<f32>,
    mut ultimo_z: Local<f32>,
) {
    let Ok(pt) = player.get_single() else { return };
    let dt = time.delta_secs().min(0.1);
    let body = active_surface_body(&ctx, &planet);
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    // RUIDO: quien corre o vuela se oye de más lejos; quien camina despacio, de menos.
    let vel_j = ((px - *ultimo).abs() + (pz - *ultimo_z).abs()) / dt.max(1e-3);
    *ultimo = px;
    *ultimo_z = pz;
    let olfato = if vel_j > 6.0 { OLFATO * 1.6 } else if vel_j < 0.3 { OLFATO * 0.6 } else { OLFATO };
    let mut mordidas = 0u32;
    for (mut i, mut tf, mut sprite) in &mut q {
        i.ataque_cd = (i.ataque_cd - dt).max(0.0);
        i.destello = (i.destello - dt).max(0.0);
        // Persigue a quien tenga más cerca: vos o un vecino (que se defiende, ver `barrio.rs`).
        let (mut tx, mut tz) = (px, pz);
        let mut d_jug = ((px - i.x).powi(2) + (pz - i.z).powi(2)).sqrt().max(1e-3);
        let mut a_un_vecino = false;
        for v in vecinos.iter() {
            let dv = ((v.x - i.x).powi(2) + (v.z - i.z).powi(2)).sqrt().max(1e-3);
            if dv < d_jug && dv < OLFATO_VECINO {
                (tx, tz, d_jug, a_un_vecino) = (v.x, v.z, dv, true);
            }
        }
        let (dx, dz) = (tx - i.x, tz - i.z);
        let d = d_jug;
        if d < olfato.max(if a_un_vecino { OLFATO_VECINO } else { 0.0 }) {
            let v = if d < 8.0 { VEL_CERCA } else { VEL };
            // Rumbo al jugador con un desvío fijo que se apaga al acercarse.
            let a = dz.atan2(dx) + i.desvio * (d / olfato).min(1.0);
            let (sx, sz) = (a.cos() * v * dt, a.sin() * v * dt);
            // Deslizamiento por ejes: contra una pared se arrastra, no se clava.
            let mut avanzo = false;
            if !obst.caja_frena(body, i.x + sx, i.z, RADIO_CUERPO) {
                i.x += sx;
                avanzo = true;
            }
            if !obst.caja_frena(body, i.x, i.z + sz, RADIO_CUERPO) {
                i.z += sz;
                avanzo = true;
            }
            if avanzo {
                i.bloqueado = 0.0;
            } else {
                i.bloqueado += dt;
                // Tras empujar un buen rato, derriban lo que sea una puerta: el pedido
                // sólo abre puertas, una pared sigue siendo una pared.
                if i.bloqueado >= DERRIBO_S {
                    pedidos.0.push((i.x + a.cos() * 0.5, i.z + a.sin() * 0.5));
                    golpes.0.push((i.x + a.cos() * 0.6, i.z + a.sin() * 0.6));
                    i.bloqueado = 0.0;
                }
            }
        }
        if d < ATAQUE_RADIO && i.ataque_cd <= 0.0 && !a_un_vecino {
            i.ataque_cd = ATAQUE_CD;
            mordidas += 1;
        }
        tf.translation = Vec3::new(i.x, -i.z, sort_z(i.z));
        sprite.color = if i.destello > 0.0 { Color::srgb(1.0, 0.35, 0.35) } else { Color::srgb(0.55, 0.78, 0.55) };
        // Mira hacia donde va.
        tf.scale.x = if dx < 0.0 { -1.0 } else { 1.0 };
    }
    if mordidas > 0 {
        if let Ok(mut s) = sim.get_single_mut() {
            s.hp = (s.hp - DANO * mordidas as f32 * (1.0 - s.armadura)).max(0.0);
        }
    }
}

/// ¿Esta pieza de borde cede a los infectados? Madera sí (pared, ventana, baranda); el resto no.
pub fn cede(id: &str) -> bool {
    matches!(id, "edge_wall_wood" | "edge_window" | "edge_railing_wood")
}

/// Cada golpe acumulado le saca 1 a la pared de madera más cercana; a 0 cae.
fn romper_paredes(
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut golpes: ResMut<GolpesAParedes>,
    mut walls: ResMut<crate::walls::BuiltWalls>,
    mut vida: Local<std::collections::HashMap<(i64, i64, u8), f32>>,
) {
    if golpes.0.is_empty() {
        return;
    }
    use ha_shared::build_grid::WallAxis;
    let body = active_surface_body(&ctx, &planet);
    for (x, z) in golpes.0.drain(..) {
        let (cx, cz) = (x.floor() as i64, z.floor() as i64);
        let mut mejor: Option<(f32, i64, i64, WallAxis)> = None;
        for tx in cx - 1..=cx + 2 {
            for tz in cz - 1..=cz + 2 {
                for axis in [WallAxis::V, WallAxis::H] {
                    let Some(w) = walls.at(body, 0, tx, tz, axis) else { continue };
                    if !cede(w.def().id) {
                        continue;
                    }
                    // Distancia del punto al centro del borde (un tile de largo).
                    let (ex, ez) = match axis {
                        WallAxis::V => (tx as f32, tz as f32 + 0.5),
                        WallAxis::H => (tx as f32 + 0.5, tz as f32),
                    };
                    let d = ((x - ex).powi(2) + (z - ez).powi(2)).sqrt();
                    if d < 1.2 && mejor.map(|m| d < m.0).unwrap_or(true) {
                        mejor = Some((d, tx, tz, axis));
                    }
                }
            }
        }
        let Some((_, tx, tz, axis)) = mejor else { continue };
        let k = (tx, tz, axis.as_u8());
        let v = vida.entry(k).or_insert(VIDA_PARED_MADERA);
        *v -= 1.0;
        if *v <= 0.0 {
            vida.remove(&k);
            walls.remove(body, 0, tx, tz, axis);
            crate::causal::registrar("pared", format!("cae una pared de madera en ({tx},{tz})"));
        }
    }
}

/// Un toque sobre un infectado a mano es un golpe.
#[allow(clippy::too_many_arguments)]
fn golpear(
    mut commands: Commands,
    time: Res<Time>,
    mut acc: ResMut<Acumulado>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut touches: EventReader<TouchInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    touch_layout: Res<TouchLayout>,
    blocks: Res<TouchBlockRects>,
    equipped: Res<EquippedItem>,
    mut bajas: ResMut<Bajas>,
    mut chests: ResMut<crate::chests::ChestWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    player: Query<&Transform, (With<Player>, Without<Infectado>)>,
    mut q: Query<(Entity, &mut Infectado, &Sprite, &Transform)>,
) {
    acc.golpe = (acc.golpe - time.delta_secs()).max(0.0);
    let screen = if mouse.just_pressed(MouseButton::Left) {
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
    let (Some(screen), Ok((camera, cam_tf)), Ok(win), Ok(pt)) =
        (screen, cam.get_single(), windows.get_single(), player.get_single())
    else {
        return;
    };
    if acc.golpe > 0.0 {
        return;
    }
    let Some(mundo) = crate::render::scale::window_to_world_2d(camera, cam_tf, win, screen) else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let mut mejor: Option<(Entity, f32)> = None;
    for (e, i, _, _) in &q {
        let dm = Vec2::new(i.x, -i.z).distance(mundo);
        let dj = ((i.x - px).powi(2) + (i.z - pz).powi(2)).sqrt();
        if dm <= PICK_R && dj <= ALCANCE && mejor.map(|(_, b)| dm < b).unwrap_or(true) {
            mejor = Some((e, dm));
        }
    }
    let Some((e, _)) = mejor else { return };
    acc.golpe = GOLPE_CD;
    let Ok((_, mut i, sprite, _)) = q.get_mut(e) else { return };
    i.vida -= crate::fauna::dano_de(&equipped.item_id);
    i.destello = 0.15;
    // Retroceso: lo empuja lejos de vos.
    let (dx, dz) = (i.x - px, i.z - pz);
    let d = (dx * dx + dz * dz).sqrt().max(1e-3);
    i.x += dx / d * 0.5;
    i.z += dz / d * 0.5;
    if i.vida <= 0.0 {
        let (x, z) = (i.x, i.z);
        bajas.total += 1;
        crate::causal::registrar("baja", format!("infectado abatido en ({x:.0},{z:.0}), botín {}", i.botin.len()));
        // El cuerpo queda en el suelo con TODO lo que llevaba: se abre como un cofre.
        let body = active_surface_body(&ctx, &planet);
        let uid = chests.crear_cadaver(body, x, z, &i.botin);
        let mut cuerpo = sprite.clone();
        cuerpo.color = Color::srgb(0.38, 0.5, 0.4);
        commands.spawn((
            cuerpo,
            Transform {
                translation: Vec3::new(x, -z, sort_z(z) - 0.01),
                rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                ..default()
            },
            Cadaver { uid, edad: 0.0 },
            Name::new("Cadaver"),
        ));
        commands.entity(e).despawn_recursive();
    }
}

/// Un toque sobre un cadáver a mano abre su inventario (el panel del cofre). Los cuerpos se
/// descomponen a los `VIDA_CADAVER` segundos; si lo estás revisando, esperan.
#[allow(clippy::too_many_arguments)]
fn cadaveres(
    mut commands: Commands,
    time: Res<Time>,
    acc: Res<Acumulado>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut touches: EventReader<TouchInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    touch_layout: Res<TouchLayout>,
    blocks: Res<TouchBlockRects>,
    mut chests: ResMut<crate::chests::ChestWorld>,
    mut panel: ResMut<crate::chests::ChestPanelState>,
    player: Query<&Transform, (With<Player>, Without<Cadaver>)>,
    mut q: Query<(Entity, &mut Cadaver, &Transform)>,
) {
    let dt = time.delta_secs();
    for (e, mut c, _) in &mut q {
        c.edad += dt;
        if c.edad > VIDA_CADAVER && panel.open_uid != Some(c.uid) {
            chests.quitar(c.uid);
            commands.entity(e).despawn_recursive();
        }
    }
    let screen = if mouse.just_pressed(MouseButton::Left) {
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
    let (Some(screen), Ok((camera, cam_tf)), Ok(win), Ok(pt)) =
        (screen, cam.get_single(), windows.get_single(), player.get_single())
    else {
        return;
    };
    // El golpe que lo mató no lo abre.
    if acc.golpe > 0.0 || panel.is_open() {
        return;
    }
    let Some(mundo) = crate::render::scale::window_to_world_2d(camera, cam_tf, win, screen) else { return };
    let pj = pt.translation.truncate();
    for (_, c, tf) in &q {
        let p = tf.translation.truncate();
        if p.distance(mundo) <= PICK_R && p.distance(pj) <= ALCANCE {
            panel.open_uid = Some(c.uid);
            return;
        }
    }
}

/// De día se queman (de a uno cada pocos segundos) y los que quedan lejos se van.
#[allow(clippy::too_many_arguments)]
fn amanecer(
    mut commands: Commands,
    time: Res<Time>,
    mut acc: ResMut<Acumulado>,
    day: Res<DayClock>,
    ctx: Res<WorldContextRuntime>,
    player: Query<&Transform, (With<Player>, Without<Infectado>)>,
    q: Query<(Entity, &Infectado)>,
) {
    acc.quema += time.delta_secs();
    if acc.quema < 2.0 {
        return;
    }
    acc.quema = 0.0;
    let quemar = day.daylight() > AMANECER_LUZ || !en_superficie(&ctx);
    let Ok(pt) = player.get_single() else { return };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let mut ya = false;
    for (e, i) in &q {
        let lejos = ((i.x - px).powi(2) + (i.z - pz).powi(2)).sqrt() > OLVIDO;
        if lejos || (quemar && !ya && !i.guardia) {
            commands.entity(e).despawn_recursive();
            ya = ya || (quemar && !i.guardia);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_madera_cede_y_la_piedra_no() {
        assert!(cede("edge_wall_wood") && cede("edge_window"));
        assert!(!cede("edge_wall_stone") && !cede("edge_wall_fachada_local") && !cede("edge_door_wood"));
        for id in ["edge_wall_wood", "edge_window", "edge_railing_wood"] {
            assert!(ha_shared::build_grid::EDGE_PIECES.iter().any(|p| p.id == id), "{id} no existe");
        }
    }

    #[test]
    fn cada_tercera_noche_es_horda() {
        assert!(!es_horda(0.0) && !es_horda(1.0) && es_horda(2.0) && es_horda(5.0) && !es_horda(6.0));
    }

    #[test]
    fn el_tope_crece_con_los_dias_y_con_la_sala() {
        assert_eq!(tope(0.0, 0), 2);
        assert!(tope(3.0, 0) > tope(0.0, 0));
        assert!(tope(100.0, 0) <= TOPE_MAX);
        assert_eq!(tope(0.0, 2), 6);
    }

    #[test]
    fn el_botin_nunca_esta_vacio_y_son_items_con_nombre() {
        let mut con_ropa = 0;
        for n in 0..500u32 {
            let b = botin_de(n.wrapping_mul(2654435761));
            assert!(!b.is_empty());
            for (it, q) in &b {
                assert!(*q >= 1);
                assert!(ha_assets::slots::slot_label(&format!("items/{it}")).is_some(), "sin slot: {it}");
            }
            con_ropa += b.iter().any(|(i, _)| ha_shared::armadura::es_hallable(i)) as u32;
        }
        assert!((100..300).contains(&con_ropa), "ropa en {con_ropa}/500: fuera de lo esperado (~35 %)");
    }

    #[test]
    fn son_mas_lentos_que_el_jugador_y_la_mordida_no_mata_de_un_tiro() {
        assert!(VEL_CERCA < 4.0, "tienen que poder escaparse");
        assert!(DANO * 3.0 < 100.0);
        // Un golpe de puño no los mata; un hacha sí necesita pocos.
        assert!(crate::fauna::dano_de("hand") < VIDA);
        assert!((VIDA / crate::fauna::dano_de("axe")).ceil() <= 4.0);
    }
}
