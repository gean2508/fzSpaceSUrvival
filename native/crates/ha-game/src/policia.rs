//! **La policía: robar un auto o atropellar a alguien te pone en búsqueda.**
//!
//! Estilo GTA, sin cutscenes (regla 9): ningún cartel tapa la pantalla ni se
//! corta el control. Un delito sube el nivel de búsqueda ([`Busqueda`], 1 a 3);
//! con nivel > 0 el juego mantiene tantas patrullas como niveles, nacidas a
//! 70-110 m del jugador y fuera de cámara, que lo persiguen (`ha_shared::vehiculos::perseguir`,
//! corre en `vehiculos::fisica_vehiculos`). A menos de 14 m le disparan; si una te
//! alcanza y estás casi parado 3,5 s, te atrapa: multa en vida y, si ibas en un
//! vehículo robado, lo confiscan. Si dejás de verlas 25 s, bajás un nivel.
//!
//! El cupo manda (checklist §2): como mucho `NIVEL_MAX` patrullas, un spawn cada
//! 6 s y nada de patrullas con la búsqueda en cero.

use bevy::prelude::*;
use ha_shared::vehiculos::{self as veh, Clase, Estado, Persecucion};

use crate::audio::{PlaySfxAt, Sfx};
use crate::camera::CameraShakeEvent;
use crate::game_phase::GamePhase;
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, SimState};
use crate::vehiculos::{spawn_vehiculo, Manejando, Quien, Vehiculo};
use crate::world2d::{active_surface_body, render_xy, ActivePlanet};

/// El coche de la patrulla: un sedán blanco del pack con la barra de luces encima.
pub const CLAVE_PATRULLA: &str = "vehicle/coche_13";
pub const NIVEL_MAX: u8 = 3;
/// A cuánto dispara la patrulla.
const ALCANCE_TIRO: f32 = 14.0;
/// A cuánto te "ve" (si no hay ninguna más cerca que esto, corre el reloj de perdida).
const ALCANCE_VISTA: f32 = 60.0;
const SEGUNDOS_PARA_PERDERLAS: f32 = 25.0;
const SEGUNDOS_PARA_ATRAPARTE: f32 = 3.5;
/// Vida que cuesta cada tiro que acierta (antes de la armadura).
const DANIO_TIRO: f32 = 4.0;
const MULTA_HP: f32 = 15.0;

pub struct PoliciaPlugin;

impl Plugin for PoliciaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Busqueda>().add_systems(
            Update,
            (patrullas, animar_sirenas).chain().run_if(in_state(GamePhase::Playing)),
        );
    }
}

/// El nivel de búsqueda del jugador.
#[derive(Resource, Default, Debug)]
pub struct Busqueda {
    /// 0 = nada; 3 = el máximo.
    pub nivel: u8,
    /// Segundos sin que ninguna patrulla lo vea.
    sin_ver: f32,
    /// Para no sumar tres niveles por el mismo atropello.
    enfriar_delito: f32,
    spawn_cd: f32,
    disparo_cd: f32,
    atrapado: f32,
    /// Un aviso corto para el HUD («Te perdieron de vista»).
    pub aviso: Option<(String, f32)>,
}

impl Busqueda {
    /// Un delito: sube el nivel (con un enfriamiento de 4 s entre delitos).
    pub fn delito(&mut self, gravedad: u8, que: &str) {
        if self.enfriar_delito > 0.0 {
            return;
        }
        self.enfriar_delito = 4.0;
        self.sin_ver = 0.0;
        self.nivel = (self.nivel + gravedad).min(NIVEL_MAX);
        self.aviso = Some((que.to_string(), 3.0));
    }

    pub fn buscado(&self) -> bool {
        self.nivel > 0
    }
}

/// La barra de luces de la patrulla (hija del vehículo).
#[derive(Component)]
struct Sirena;

fn hash(a: u32) -> u32 {
    let mut x = a.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^ (x >> 12)
}

#[allow(clippy::too_many_arguments)]
fn patrullas(
    mut commands: Commands,
    time: Res<Time>,
    art: Res<crate::art2d::ArtLibrary>,
    (world_ctx, planet_ctx): (Res<WorldContextRuntime>, Res<ActivePlanet>),
    obs: crate::obstaculos::Obstaculos,
    mut busq: ResMut<Busqueda>,
    mut manejando: ResMut<Manejando>,
    mut autos: Query<(Entity, &mut Vehiculo)>,
    mut player: Query<&mut SimState, With<Player>>,
    (mut sfx, mut shake): (EventWriter<PlaySfxAt>, EventWriter<CameraShakeEvent>),
) {
    let dt = time.delta_secs().min(0.1);
    busq.enfriar_delito = (busq.enfriar_delito - dt).max(0.0);
    busq.spawn_cd = (busq.spawn_cd - dt).max(0.0);
    busq.disparo_cd = (busq.disparo_cd - dt).max(0.0);
    if let Some((_, t)) = busq.aviso.as_mut() {
        *t -= dt;
    }
    if busq.aviso.as_ref().is_some_and(|(_, t)| *t <= 0.0) {
        busq.aviso = None;
    }
    let Ok(mut sim) = player.get_single_mut() else { return };
    let body = active_surface_body(&world_ctx, &planet_ctx);
    let yo = Vec2::new(sim.body.x, sim.body.z);

    let patrullas: Vec<Entity> = autos.iter().filter(|(_, v)| matches!(v.quien, Quien::Policia(_))).map(|(e, _)| e).collect();

    // Sin búsqueda las patrullas se aquietan (y el tránsito las borra al alejarse).
    if !busq.buscado() {
        for e in patrullas {
            if let Ok((_, mut v)) = autos.get_mut(e) {
                v.quien = Quien::Nadie;
            }
        }
        busq.atrapado = 0.0;
        busq.sin_ver = 0.0;
        return;
    }

    // --- Nacer ---
    if (patrullas.len() as u8) < busq.nivel && busq.spawn_cd <= 0.0 && obs.capa() == 0 {
        busq.spawn_cd = 6.0;
        let semilla = (time.elapsed_secs() * 1000.0) as u32;
        for k in 0..10u32 {
            let h = hash(semilla.wrapping_add(k * 7919));
            let ang = (h % 3600) as f32 / 3600.0 * std::f32::consts::TAU;
            let dist = 70.0 + ((h >> 12) % 40) as f32;
            let (x, z) = (yo.x + ang.cos() * dist, yo.y + ang.sin() * dist);
            if !obs.libre(body, x, z, 2.2) || obs.tiles().tile_at(body, x.floor() as i64, z.floor() as i64).swim() {
                continue;
            }
            let ficha = veh::ficha(Clase::Patrulla);
            let Some(([_, _, w, h2], _)) = art.pose(CLAVE_PATRULLA, "abajo") else { break };
            let (largo, ancho) = (h2 as f32 / 32.0, w as f32 / 32.0);
            let rumbo = (yo.y - z).atan2(yo.x - x);
            let estado = Estado::nuevo(x, z, rumbo, &ficha, largo, ancho);
            if let Some(e) = spawn_vehiculo(
                &mut commands,
                &art,
                CLAVE_PATRULLA,
                Clase::Patrulla,
                estado,
                Quien::Policia(Persecucion::default()),
                false,
                h,
            ) {
                commands.entity(e).with_children(|c| {
                    c.spawn((
                        Sprite::from_color(Color::srgb(0.9, 0.1, 0.1), Vec2::new(1.1, 0.4)),
                        Transform::from_xyz(0.0, 0.0, 0.5),
                        Sirena,
                    ));
                });
            }
            break;
        }
    }

    // --- Ver, disparar, atrapar ---
    let velocidad_mia = manejando
        .auto
        .and_then(|e| autos.get(e).ok().map(|(_, v)| v.estado.vel.abs()))
        .unwrap_or(0.0);
    let mut cerca_vista = false;
    let mut pegada = false;
    let mut tiradores: Vec<Vec2> = Vec::new();
    for (_, v) in autos.iter() {
        if !matches!(v.quien, Quien::Policia(_)) || v.estado.chatarra() {
            continue;
        }
        let p = Vec2::new(v.estado.x, v.estado.z);
        let d = p.distance(yo);
        cerca_vista |= d < ALCANCE_VISTA;
        pegada |= d < 4.5;
        if d < ALCANCE_TIRO {
            tiradores.push(p);
        }
    }
    if cerca_vista {
        busq.sin_ver = 0.0;
    } else {
        busq.sin_ver += dt;
        if busq.sin_ver > SEGUNDOS_PARA_PERDERLAS {
            busq.sin_ver = 0.0;
            busq.nivel -= 1;
            busq.aviso = Some((if busq.nivel == 0 { "Te perdieron de vista".into() } else { "Bajó la búsqueda".into() }, 3.0));
        }
    }
    if !tiradores.is_empty() && busq.disparo_cd <= 0.0 {
        busq.disparo_cd = 0.9;
        let h = hash((time.elapsed_secs() * 977.0) as u32);
        for p in &tiradores {
            sfx.send(PlaySfxAt(Sfx::ShotPistol, render_xy(p.x, p.y)));
        }
        // 60 % de acierto por tanda: una patrulla sola no te mata en un semáforo.
        if h % 100 < 60 {
            sim.hp = (sim.hp - DANIO_TIRO * tiradores.len() as f32 * (1.0 - sim.armadura)).max(0.0);
            sfx.send(PlaySfxAt(Sfx::Hurt, render_xy(yo.x, yo.y)));
            shake.send(CameraShakeEvent { trauma: 0.18 });
        }
    }
    if pegada && velocidad_mia < 2.5 {
        busq.atrapado += dt;
        if busq.atrapado >= SEGUNDOS_PARA_ATRAPARTE {
            busq.atrapado = 0.0;
            busq.nivel = 0;
            sim.hp = (sim.hp - MULTA_HP).max(1.0);
            let mut aviso = "Te atraparon: multa".to_string();
            if let Some(e) = manejando.auto {
                let robado = autos.get(e).is_ok_and(|(_, v)| v.robado);
                if robado {
                    manejando.auto = None;
                    commands.entity(e).despawn_recursive();
                    aviso = "Te atraparon: multa y confiscaron el vehículo".into();
                }
            }
            busq.aviso = Some((aviso, 4.0));
        }
    } else {
        busq.atrapado = (busq.atrapado - dt).max(0.0);
    }
}

/// La barra de luces alterna rojo y azul cuatro veces por segundo.
fn animar_sirenas(time: Res<Time>, mut q: Query<&mut Sprite, With<Sirena>>) {
    let rojo = (time.elapsed_secs() * 4.0) as u32 % 2 == 0;
    for mut s in &mut q {
        s.color = if rojo { Color::srgb(0.95, 0.1, 0.1) } else { Color::srgb(0.15, 0.3, 1.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_delito_sube_el_nivel_con_enfriamiento_y_tope() {
        let mut b = Busqueda::default();
        b.delito(1, "robo");
        assert_eq!(b.nivel, 1);
        b.delito(1, "otro al instante");
        assert_eq!(b.nivel, 1, "dos delitos en el mismo segundo cuentan como uno");
        b.enfriar_delito = 0.0;
        b.delito(5, "masacre");
        assert_eq!(b.nivel, NIVEL_MAX);
    }
}
