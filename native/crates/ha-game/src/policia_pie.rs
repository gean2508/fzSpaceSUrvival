//! **Policía a pie y comisarías.** Con búsqueda activa (`policia::Busqueda`), además de las patrullas
//! en coche salen agentes a pie. Nacen en la **comisaría más cercana** (el lote `building/comisaria*`
//! que el generador de ciudades ya coloca, `ha_shared::city::comisaria_cercana`) si hay una a menos de
//! 140 m; si no, entran por el borde de lo que ves. Te persiguen andando con las mismas paredes que
//! a vos (`Obstaculos`), te disparan a menos de 12 m y, si uno te alcanza y estás casi quieto 2,5 s,
//! te atrapa: multa en vida y se acaba la búsqueda. Sin búsqueda, se retiran y desaparecen.
//!
//! Cupo (checklist §2): como mucho `nivel + 1` agentes, un spawn cada 5 s, y todo fuera de la
//! superficie o sin búsqueda se limpia.

use bevy::prelude::*;

use crate::audio::{PlaySfxAt, Sfx};
use crate::camera::CameraShakeEvent;
use crate::game_phase::GamePhase;
use crate::policia::Busqueda;
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, SimState};
use crate::world2d::{active_surface_body, render_xy, sort_z, ActivePlanet};

const VEL: f32 = 3.2;
const RADIO: f32 = 0.35;
const ALCANCE_TIRO: f32 = 12.0;
const DANIO: f32 = 3.0;
const MULTA_HP: f32 = 12.0;
const SEGUNDOS_PARA_ATRAPARTE: f32 = 2.5;
const RADIO_COMISARIA: i64 = 140;
const TAMANO: Vec2 = Vec2::new(0.95, 1.9);

#[derive(Component)]
pub struct PoliciaPie {
    x: f32,
    z: f32,
    tiro_cd: f32,
}

#[derive(Resource, Default)]
struct Estado {
    spawn_cd: f32,
    atrapado: f32,
    /// Comisaría ya resuelta para esta búsqueda (cache; se recalcula al cambiar de sitio).
    comisaria: Option<(i64, i64)>,
    sondeo: f32,
}

pub struct PoliciaPiePlugin;

impl Plugin for PoliciaPiePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Estado>()
            .add_systems(Update, agentes.run_if(in_state(GamePhase::Playing)))
            .add_systems(OnExit(GamePhase::Playing), limpiar);
    }
}

fn limpiar(mut commands: Commands, q: Query<Entity, With<PoliciaPie>>, mut e: ResMut<Estado>) {
    for x in &q {
        commands.entity(x).despawn_recursive();
    }
    *e = Estado::default();
}

fn hash(a: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^ (x >> 12)
}

/// Cuántos agentes a pie debe haber con este nivel de búsqueda.
pub fn cupo(nivel: u8) -> usize {
    if nivel == 0 { 0 } else { nivel as usize + 1 }
}

#[allow(clippy::too_many_arguments)]
fn agentes(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<crate::art2d::ArtLibrary>,
    (ctx, planet): (Res<WorldContextRuntime>, Res<ActivePlanet>),
    obs: crate::obstaculos::Obstaculos,
    mut busq: ResMut<Busqueda>,
    mut est: ResMut<Estado>,
    mut policias: Query<(Entity, &mut PoliciaPie, &mut Transform), Without<Player>>,
    mut player: Query<&mut SimState, With<Player>>,
    (mut sfx, mut shake): (EventWriter<PlaySfxAt>, EventWriter<CameraShakeEvent>),
) {
    let dt = time.delta_secs().min(0.1);
    let Ok(mut sim) = player.get_single_mut() else { return };
    let body = active_surface_body(&ctx, &planet);
    let yo = Vec2::new(sim.body.x, sim.body.z);
    let en_superficie = matches!(ctx.active, ha_shared::WorldContext::Overworld) && obs.capa() == 0;

    // Sin búsqueda (o fuera de la superficie) se retiran.
    if !busq.buscado() || !en_superficie {
        for (e, _, _) in &policias {
            commands.entity(e).despawn_recursive();
        }
        est.atrapado = 0.0;
        est.comisaria = None;
        return;
    }

    est.spawn_cd = (est.spawn_cd - dt).max(0.0);
    est.sondeo -= dt;
    if est.sondeo <= 0.0 {
        est.sondeo = 5.0;
        est.comisaria = ha_shared::city::comisaria_cercana(body, yo.x.floor() as i64, yo.y.floor() as i64, RADIO_COMISARIA);
    }

    // --- Nacer: en la comisaría si hay una cerca; si no, por el borde ---
    let vivos = policias.iter().count();
    if vivos < cupo(busq.nivel) && est.spawn_cd <= 0.0 {
        est.spawn_cd = 5.0;
        let semilla = (time.elapsed_secs() * 1000.0) as u32;
        for k in 0..8u32 {
            let h = hash(semilla.wrapping_add(k * 6271));
            let (x, z) = match est.comisaria {
                // Sale por delante de la comisaría, desparramado un poco.
                Some((cx, cz)) => (cx as f32 + ((h % 9) as f32 - 4.0), cz as f32 + 6.0 + ((h >> 8) % 4) as f32),
                None => {
                    let ang = (h % 3600) as f32 / 3600.0 * std::f32::consts::TAU;
                    let dist = 40.0 + ((h >> 12) % 20) as f32;
                    (yo.x + ang.cos() * dist, yo.y + ang.sin() * dist)
                }
            };
            if !obs.libre(body, x, z, RADIO) {
                continue;
            }
            let skin = format!("npc/char_{}", 1 + h % 20);
            let mut sprite = art.sprite_or_color(&skin, Color::srgb(0.2, 0.3, 0.7), TAMANO);
            sprite.color = Color::srgb(0.45, 0.6, 1.0); // uniforme azul
            commands.spawn((
                sprite,
                Transform::from_xyz(x, -z, sort_z(z)),
                PoliciaPie { x, z, tiro_cd: 0.5 + (h % 10) as f32 / 10.0 },
                Name::new("PoliciaPie"),
            ));
            break;
        }
    }

    // --- Perseguir, disparar, atrapar ---
    let mut visto = false;
    let mut pegado = false;
    for (_, mut p, mut tf) in &mut policias {
        let pos = Vec2::new(p.x, p.z);
        let d = pos.distance(yo);
        visto |= d < 60.0;
        pegado |= d < 1.8;
        p.tiro_cd = (p.tiro_cd - dt).max(0.0);
        if d > 1.4 {
            let dir = (yo - pos) / d.max(1e-3);
            let paso = dir * VEL * dt;
            // Deslizamiento por ejes: contra una pared se arrastra, no se clava.
            if obs.libre(body, p.x + paso.x, p.z, RADIO) {
                p.x += paso.x;
            }
            if obs.libre(body, p.x, p.z + paso.y, RADIO) {
                p.z += paso.y;
            }
        }
        if d < ALCANCE_TIRO && p.tiro_cd <= 0.0 {
            p.tiro_cd = 1.4;
            sfx.send(PlaySfxAt(Sfx::ShotPistol, render_xy(p.x, p.z)));
            let h = hash((time.elapsed_secs() * 733.0) as u32 ^ (p.x as u32));
            if h % 100 < 50 {
                sim.hp = (sim.hp - DANIO * (1.0 - sim.armadura)).max(0.0);
                sfx.send(PlaySfxAt(Sfx::Hurt, render_xy(yo.x, yo.y)));
                shake.send(CameraShakeEvent { trauma: 0.12 });
            }
        }
        tf.translation = Vec3::new(p.x, -p.z, sort_z(p.z));
    }
    if visto {
        busq.visto();
    }
    if pegado && (sim.body.vx * sim.body.vx + sim.body.vz * sim.body.vz).sqrt() < 2.0 {
        est.atrapado += dt;
        if est.atrapado >= SEGUNDOS_PARA_ATRAPARTE {
            est.atrapado = 0.0;
            busq.atrapar("Te atraparon: multa");
            sim.hp = (sim.hp - MULTA_HP).max(1.0);
        }
    } else {
        est.atrapado = (est.atrapado - dt).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_cupo_crece_con_la_busqueda_y_sin_busqueda_no_hay_nadie() {
        assert_eq!(cupo(0), 0);
        assert_eq!(cupo(1), 2);
        assert_eq!(cupo(3), 4);
    }
}
