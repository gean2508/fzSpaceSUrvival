//! **Incendios y bomberos** (pedido del usuario, 4 oct 2026: una ciudad simulada a nivel AAA).
//!
//! De vez en cuando un edificio cercano se prende fuego (más seguido de noche). El cuartel de bomberos más
//! cercano (`building/bomberos`, ver `ha_shared::city::edificio_cercano`) despacha su autobomba, que va a la puerta
//! del edificio con la sirena prendida, se planta y lo apaga con la manguera. **Si nadie responde** (no hay cuartel
//! a menos de 220 m, o llega tarde) a los 75 s el fuego revienta la puerta y a los 120 s salta al edificio de al
//! lado; quien esté cerca de las llamas se quema.
//!
//! Cupo (checklist §2): como mucho dos fuegos a la vez, una autobomba por fuego y llamas hechas con 4 sprites de color.

use bevy::prelude::*;
use ha_shared::interiores::Lote;
use ha_shared::vehiculos::{self as veh, Clase, Estado, Persecucion};

use crate::building::PuertaDeFachada;
use crate::game_phase::GamePhase;
use crate::light2d::Light2d;
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, SimState};
use crate::sky2d::DayClock;
use crate::vehiculos::{spawn_vehiculo, Quien, Vehiculo};
use crate::world2d::{active_surface_body, render_xy, ActivePlanet, TileWorld};

const MAX_FUEGOS: usize = 2;
const SEG_PARA_ROMPER_PUERTA: f32 = 75.0;
const SEG_PARA_CONTAGIAR: f32 = 120.0;
const SEG_PARA_APAGARSE: f32 = 240.0;
const RADIO_CUARTEL: i64 = 220;
const VIDA_DEL_FUEGO: f32 = 100.0;
/// Cuánto baja la manguera de la autobomba por segundo.
const CAUDAL: f32 = 9.0;

#[derive(Component)]
pub struct Incendio {
    pub lote: Lote,
    pub puerta: (f32, f32),
    pub vida: f32,
    pub edad: f32,
    pub camion: Option<Entity>,
    contagiado: bool,
    puerta_rota: bool,
}

#[derive(Component)]
struct Llama(f32);

#[derive(Component)]
struct SirenaBombero;

/// La autobomba que ya terminó y se va (se borra al rato).
#[derive(Component)]
struct Retirada(f32);

#[derive(Resource, Default)]
pub struct Incendios {
    cd: f32,
}

pub struct IncendiosPlugin;

impl Plugin for IncendiosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Incendios>()
            .add_systems(
                Update,
                (encender, despachar, arder, apagar, retirar, sirenas).chain().run_if(in_state(GamePhase::Playing)),
            )
            .add_systems(OnExit(GamePhase::Playing), limpiar);
    }
}

fn limpiar(mut commands: Commands, q: Query<Entity, With<Incendio>>, mut inc: ResMut<Incendios>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
    *inc = Incendios::default();
}

fn hash(a: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^ (x >> 12)
}

/// Probabilidad de que en una revisión (cada 20 s) se prenda algo: de noche más.
pub fn probabilidad_de_incendio(luz: f32) -> f32 {
    0.07 + 0.12 * (1.0 - luz.clamp(0.0, 1.0))
}

#[allow(clippy::too_many_arguments)]
fn encender(
    mut commands: Commands,
    time: Res<Time>,
    day: Res<DayClock>,
    tiles: Res<TileWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut inc: ResMut<Incendios>,
    int: Res<crate::interiores::Interiores>,
    player: Query<&SimState, With<Player>>,
    puertas: Query<&PuertaDeFachada>,
    fuegos: Query<&Incendio>,
) {
    inc.cd -= time.delta_secs().min(0.2);
    if inc.cd > 0.0 {
        return;
    }
    inc.cd = 20.0;
    if tiles.z != 0 || int.dentro.is_some() || !matches!(ctx.active, ha_shared::WorldContext::Overworld) {
        return;
    }
    let n = fuegos.iter().count();
    if n >= MAX_FUEGOS {
        return;
    }
    let Ok(sim) = player.get_single() else { return };
    let (px, pz) = (sim.body.x, sim.body.z);
    let body = active_surface_body(&ctx, &planet);
    let h = hash((time.elapsed_secs() * 1000.0) as u32);
    if (h % 1000) as f32 / 1000.0 >= probabilidad_de_incendio(day.daylight()) {
        return;
    }
    // Una puerta de edificio entre 14 y 90 m: se ve venir pero no está encima.
    let candidatas: Vec<&PuertaDeFachada> = puertas
        .iter()
        .filter(|p| {
            let d = ((p.tx as f32 + 0.5) - px).hypot((p.tz as f32 + 0.5) - pz);
            (14.0..90.0).contains(&d) && !fuegos.iter().any(|f| (f.puerta.0 - (p.tx as f32 + 0.5)).abs() < 6.0 && (f.puerta.1 - (p.tz as f32 + 0.5)).abs() < 6.0)
        })
        .collect();
    if candidatas.is_empty() {
        return;
    }
    let p = candidatas[(h >> 8) as usize % candidatas.len()];
    let lote = match ha_shared::amueblado::casa_en(body, p.tx, p.tz) {
        Some((x, z, w, hh, clave)) => Lote { body: body as u8, x, z, w, h: hh, clave },
        None => Lote { body: body as u8, x: p.tx - 6, z: p.tz - 7, w: 13, h: 8, clave: ha_shared::interiores::clave_estatica(&p.art) },
    };
    encender_en(&mut commands, lote, (p.tx as f32 + 0.5, p.tz as f32 + 0.5));
}

/// Prende el edificio `lote` (llamas sobre su frente). Público para los drivers de QA.
pub fn encender_en(commands: &mut Commands, lote: Lote, puerta: (f32, f32)) {
    let r = render_xy(puerta.0, puerta.1);
    let ancho = (lote.w as f32 * 0.6).clamp(3.0, 9.0);
    commands
        .spawn((
            Incendio { lote, puerta, vida: VIDA_DEL_FUEGO, edad: 0.0, camion: None, contagiado: false, puerta_rota: false },
            Transform::from_xyz(r.x, r.y + 1.0, crate::layers::OBRA),
            Visibility::Inherited,
            Light2d { color: Color::srgb(1.0, 0.55, 0.18), radius: 9.0, intensity: 1.6, flicker: 1.0, day: true, ..default() },
            Name::new("Incendio"),
        ))
        .with_children(|c| {
            for k in 0..4 {
                let x = (k as f32 - 1.5) * ancho / 4.0;
                c.spawn((
                    Sprite::from_color(Color::srgba(1.0, 0.55, 0.12, 0.9), Vec2::new(0.9, 1.4)),
                    Transform::from_xyz(x, 0.4, 0.001 * k as f32),
                    Llama(k as f32 * 1.7),
                ));
            }
        });
}

/// Manda la autobomba del cuartel más cercano.
#[allow(clippy::too_many_arguments)]
fn despachar(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    obs: crate::obstaculos::Obstaculos,
    mut fuegos: Query<(Entity, &mut Incendio)>,
    camiones: Query<(), With<Vehiculo>>,
) {
    let body = active_surface_body(&ctx, &planet);
    for (_, mut f) in &mut fuegos {
        if f.edad < 6.0 {
            continue;
        }
        if f.camion.is_some_and(|c| camiones.get(c).is_ok()) {
            continue;
        }
        if f.camion.is_some() && f.edad < 40.0 {
            continue; // el camión que mandó se perdió: se reintenta pasado un rato
        }
        f.camion = None;
        let Some((cx, cz, cw, ch)) =
            ha_shared::city::edificio_cercano(body, f.puerta.0 as i64, f.puerta.1 as i64, RADIO_CUARTEL, "building/bomberos")
        else {
            continue;
        };
        // La cochera está al frente (sur) del cuartel.
        let (sx, sz) = (cx as f32 + 0.5, (cz + ch / 2) as f32 + 2.0);
        let _ = cw;
        if !obs.libre(body, sx, sz, 1.6) {
            continue;
        }
        let Some((clave, _)) = veh::MODELOS.iter().find(|(_, c)| *c == Clase::Bomberos) else { continue };
        let Some(clase) = veh::clase_de(clave) else { continue };
        let Some(([_, _, w, h], _)) = art.pose(clave, "abajo") else { continue };
        let ficha = veh::ficha(clase);
        let estado = Estado::nuevo(sx, sz, std::f32::consts::FRAC_PI_2, &ficha, h as f32 / 32.0, w as f32 / 32.0);
        let objetivo = (f.puerta.0, f.puerta.1 + 5.0);
        if let Some(e) = spawn_vehiculo(&mut commands, &art, clave, clase, estado, Quien::Bombero(Persecucion::default(), objetivo), false, hash(f.lote.x as u32)) {
            commands.entity(e).with_children(|c| {
                c.spawn((
                    Sprite::from_color(Color::srgb(0.9, 0.1, 0.1), Vec2::new(1.1, 0.4)),
                    Transform::from_xyz(0.0, 0.0, 0.5),
                    SirenaBombero,
                ));
            });
            f.camion = Some(e);
        }
    }
}

/// Las llamas crecen y titilan; sin respuesta, rompen la puerta y se contagian.
#[allow(clippy::too_many_arguments)]
fn arder(
    mut commands: Commands,
    time: Res<Time>,
    mut int: ResMut<crate::interiores::Interiores>,
    mut fuegos: Query<(Entity, &mut Incendio, &Children)>,
    mut llamas: Query<(&Llama, &mut Sprite, &mut Transform)>,
    mut player: Query<&mut SimState, With<Player>>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
) {
    let dt = time.delta_secs().min(0.2);
    let t = time.elapsed_secs();
    let body = active_surface_body(&ctx, &planet);
    let mut nuevos: Vec<(Lote, (f32, f32))> = Vec::new();
    for (e, mut f, hijos) in &mut fuegos {
        f.edad += dt;
        let intensidad = (f.vida / VIDA_DEL_FUEGO).clamp(0.0, 1.0) * (0.5 + (f.edad / 40.0).min(1.0) * 0.5);
        for &h in hijos.iter() {
            if let Ok((l, mut s, mut tf)) = llamas.get_mut(h) {
                let v = (t * 9.0 + l.0).sin() * 0.5 + 0.5;
                s.color = Color::srgba(1.0, 0.35 + 0.45 * v, 0.08, 0.55 + 0.4 * intensidad);
                s.custom_size = Some(Vec2::new(0.8 + 0.3 * v, (0.8 + 1.4 * v) * (0.4 + intensidad)));
                tf.translation.y = 0.2 + s.custom_size.map_or(0.0, |z| z.y * 0.5);
            }
        }
        // Sin respuesta, el fuego manda.
        if f.camion.is_none() || f.edad > SEG_PARA_ROMPER_PUERTA + 40.0 {
            if f.edad > SEG_PARA_ROMPER_PUERTA && !f.puerta_rota {
                f.puerta_rota = true;
                int.puerta_de(&f.lote).golpear(1.0e6);
            }
            if f.edad > SEG_PARA_CONTAGIAR && !f.contagiado {
                f.contagiado = true;
                nuevos.push((Lote { x: f.lote.x + f.lote.w + 4, ..f.lote }, (f.puerta.0 + f.lote.w as f32 + 4.0, f.puerta.1)));
            }
        }
        if f.edad > SEG_PARA_APAGARSE {
            commands.entity(e).despawn_recursive();
            continue;
        }
        // Quien esté al lado se quema.
        if let Ok(mut sim) = player.get_single_mut() {
            if (f.puerta.0 - sim.body.x).hypot(f.puerta.1 - sim.body.z) < 2.6 {
                sim.hp = (sim.hp - 4.0 * dt * (1.0 - sim.armadura)).max(1.0);
            }
        }
    }
    let _ = body;
    for (lote, puerta) in nuevos {
        if fuegos.iter().count() < MAX_FUEGOS + 1 {
            encender_en(&mut commands, lote, puerta);
        }
    }
}

/// La autobomba llega, se planta y apaga.
#[allow(clippy::too_many_arguments)]
fn apagar(
    mut commands: Commands,
    time: Res<Time>,
    mut fuegos: Query<(Entity, &mut Incendio)>,
    mut camiones: Query<(Entity, &mut Vehiculo)>,
) {
    let dt = time.delta_secs().min(0.2);
    for (fe, mut f) in &mut fuegos {
        let Some(ce) = f.camion else { continue };
        let Ok((_, mut v)) = camiones.get_mut(ce) else {
            f.camion = None;
            continue;
        };
        let d = (v.estado.x - f.puerta.0).hypot(v.estado.z - f.puerta.1);
        if d < 9.0 && v.estado.vel.abs() < 3.5 {
            f.vida -= CAUDAL * dt;
            if f.vida <= 0.0 {
                commands.entity(fe).despawn_recursive();
                v.quien = Quien::Nadie;
                commands.entity(ce).insert(Retirada(12.0));
            }
        }
    }
}

fn retirar(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Retirada)>) {
    for (e, mut r) in &mut q {
        r.0 -= time.delta_secs();
        if r.0 <= 0.0 {
            commands.entity(e).despawn_recursive();
        }
    }
}

fn sirenas(time: Res<Time>, mut q: Query<&mut Sprite, With<SirenaBombero>>) {
    let rojo = ((time.elapsed_secs() * 4.0) as i32) % 2 == 0;
    for mut s in &mut q {
        s.color = if rojo { Color::srgb(0.95, 0.1, 0.1) } else { Color::srgb(1.0, 0.85, 0.2) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn de_noche_se_prende_mas_seguido_que_de_dia() {
        assert!(probabilidad_de_incendio(0.0) > probabilidad_de_incendio(1.0));
        assert!(probabilidad_de_incendio(1.0) > 0.0 && probabilidad_de_incendio(0.0) < 0.5);
    }

    #[test]
    fn la_manguera_apaga_antes_de_que_el_fuego_rompa_la_puerta() {
        // Una autobomba que sale al instante y tarda 40 s en llegar apaga en VIDA/CAUDAL segundos.
        assert!(40.0 + VIDA_DEL_FUEGO / CAUDAL < SEG_PARA_ROMPER_PUERTA);
    }
}
