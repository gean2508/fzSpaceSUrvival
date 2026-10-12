//! **Trepar bordillos y descolgarse de ellos** — el peso del desnivel.
//!
//! La regla ya existía y es de `ha_shared::relief`: un acantilado de UN escalón
//! es un bordillo y no frena a quien va a pie (`TileWorld::walk_blocked`), los de
//! dos o más siguen siendo pared. Lo que faltaba era que se NOTARA. Sin nada que
//! lo acompañe, cruzar un bordillo es indistinguible de atravesar una pared: el
//! personaje entra en una celda que se ve más alta, sale por el otro lado a la
//! misma velocidad y nunca pasó nada. Fue el reporte textual — "te da la sensación
//! de que atravesaste una pared, no de que trepaste".
//!
//! Acá no se toca la simulación (es determinista y vive en `/shared`): se le
//! agregan las tres cosas que el cuerpo lee como esfuerzo, todas del lado del
//! cliente.
//!
//! | qué | trepar (subís un escalón) | descolgarte (bajás uno) |
//! |---|---|---|
//! | sprite | se IZA sobre el borde y se asienta | cae rápido y aterriza duro |
//! | polvo | poco, en el pie que empuja | ráfaga en los dos pies al impactar |
//! | sonido | roce de tela y piedra | golpe seco |
//! | cámara | nada | un tirón mínimo |
//!
//! El polvo se pinta con el color del SUELO que pisaste (`tile_render_color`, la
//! misma función que dibuja los tiles): en la playa salta arena, en el regolito
//! salta gris. Es una línea de código y hace que el mismo efecto sirva en todos
//! los cuerpos sin arte nuevo.
//!
//! Nada de esto bloquea el control: es feedback, no una animación con dueño (ver
//! biblia §2, regla 8 — una transición nunca te quita el mando).

use bevy::prelude::*;
use ha_shared::relief;
use ha_shared::topdown::{Tile, TILE_M};

use crate::audio::{PlaySfxAt, Sfx};
use crate::camera::CameraShakeEvent;
use crate::creative_fly::CreativeFlight;
use crate::render::particles::ProceduralParticle;
use crate::scene::WorldContextRuntime;
use crate::sim::{Player, SimState};
use crate::world2d::{active_surface_body, render_xy, ActivePlanet, TileShade, TileWorld};

/// Cuánto se levanta el sprite al trepar (m). Es un cuarto de la altura visual de
/// un escalón (`relief::STEP_M` = 2.5): el personaje se IZA, no salta a la Luna.
const CLIMB_RISE_M: f32 = 0.6;
/// Cuánto cae al descolgarse (m). Un poco más que al trepar — bajar es dejarse ir.
const DROP_FALL_M: f32 = 0.85;
/// Duración del izado (s). Corta: es un impulso, no una animación.
const CLIMB_SECS: f32 = 0.26;
/// Duración de la caída (s). Más corta todavía — la gravedad no negocia.
const DROP_SECS: f32 = 0.16;

/// Desplazamiento vertical del sprite del jugador mientras trepa o cae, en metros
/// de mundo. Lo escribe este módulo y lo SUMA `sim::interpolate_player_visual`:
/// los pies del personaje siguen en su celda (la simulación no se entera), sólo
/// el dibujo se despega del suelo.
#[derive(Resource, Default)]
pub struct VaultBob {
    /// Offset actual (m sobre los pies). 0 = apoyado.
    pub y: f32,
    /// Segundos que lleva el impulso en curso.
    t: f32,
    /// Duración total (0 = no hay impulso).
    dur: f32,
    /// Altura del impulso (m).
    height: f32,
    /// `true` = trepando (arco: sube y se asienta), `false` = cayendo.
    up: bool,
}

impl VaultBob {
    /// Caer de un piso a otro usa el MISMO impulso que descolgarse de un bordillo
    /// (`world2d::fall_through_void`): un desnivel es un desnivel, y que se sienta
    /// igual es lo que evita tener dos vocabularios de caída en el mismo juego.
    pub fn drop_impact(&mut self) {
        self.start(false);
    }

    fn start(&mut self, up: bool) {
        self.t = 0.0;
        self.up = up;
        self.dur = if up { CLIMB_SECS } else { DROP_SECS };
        self.height = if up { CLIMB_RISE_M } else { DROP_FALL_M };
    }
}

/// Última celda pisada, para detectar el cruce.
#[derive(Resource, Default)]
struct LastCell {
    tile: Option<(i64, i64)>,
    level: i8,
}

pub struct VaultPlugin;

impl Plugin for VaultPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VaultBob>()
            .init_resource::<LastCell>()
            .add_systems(Update, (detect_ledge_crossing, tick_vault_bob).chain());
    }
}

/// ¿Cruzó el jugador el borde de una terraza este frame? Se mira el NIVEL de
/// relieve de la celda, no la geometría: es la misma fuente de verdad que decide
/// si el bordillo se puede trepar, así que el efecto sale exactamente donde el
/// juego te deja pasar y en ningún otro sitio.
#[allow(clippy::too_many_arguments)]
fn detect_ledge_crossing(
    mut commands: Commands,
    fly: Res<CreativeFlight>,
    tiles: Res<TileWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut last: ResMut<LastCell>,
    mut bob: ResMut<VaultBob>,
    mut sfx: EventWriter<PlaySfxAt>,
    mut shake: EventWriter<CameraShakeEvent>,
    player: Query<&SimState, With<Player>>,
) {
    // Volando no hay bordillos que trepar, y el subsuelo no tiene terrazas.
    if fly.flying() || tiles.z != 0 {
        last.tile = None;
        return;
    }
    let Ok(sim) = player.get_single() else {
        last.tile = None;
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let tx = (sim.body.x / TILE_M).floor() as i64;
    let tz = (sim.body.z / TILE_M).floor() as i64;
    let level = relief::relief_level(body, tx, tz);

    let Some((px, pz)) = last.tile else {
        last.tile = Some((tx, tz));
        last.level = level;
        return;
    };
    if (px, pz) == (tx, tz) {
        return;
    }
    let prev_level = last.level;
    last.tile = Some((tx, tz));
    last.level = level;

    // Un salto de más de una celda no es caminar: es un aterrizaje, un
    // teletransporte o un cambio de cuerpo. Se resincroniza y ya.
    if (tx - px).abs() > 1 || (tz - pz).abs() > 1 {
        return;
    }
    let step = level - prev_level;
    if step == 0 {
        return;
    }
    // Sólo los BORDILLOS. Una rampa (`Tile::Slope`) también cambia de nivel y
    // subirla caminando es lo normal: darle un golpe de polvo a cada paso de una
    // carretera de montaña convertiría el efecto en ruido.
    let crossed_cliff = tiles.tile_at(body, tx, tz) == Tile::Cliff
        || tiles.tile_at(body, px, pz) == Tile::Cliff;
    if !crossed_cliff {
        return;
    }

    let feet = render_xy(sim.body.x, sim.body.z);
    // El polvo es del suelo que pisás: arena en la playa, regolito en la Luna.
    let rgb = crate::world2d::tile_render_color(&tiles, body, tx, tz, TileShade::Coarse);
    let dust = Color::srgb(rgb[0] * 1.15, rgb[1] * 1.15, rgb[2] * 1.15);
    // Hacia dónde ibas: el polvo sale por detrás del pie que empuja.
    let away = Vec2::new(-(tx - px) as f32, (tz - pz) as f32).normalize_or_zero();
    let seed = (tx.unsigned_abs() as u32) ^ ((tz.unsigned_abs() as u32) << 11);

    if step > 0 {
        // TREPAR: te izás. Poco polvo (lo que raspa la punta del pie) y el roce.
        bob.start(true);
        spawn_dust(&mut commands, feet, sim.body.z, dust, 3, -away, 1.1, seed);
        sfx.send(PlaySfxAt(Sfx::ClimbLedge, feet));
    } else {
        // DESCOLGARSE: caés. Ráfaga de polvo al impactar, golpe seco y un tirón
        // de cámara mínimo — es lo que vuelve el desnivel algo que se SIENTE.
        bob.start(false);
        spawn_dust(&mut commands, feet, sim.body.z, dust, 7, away, 1.7, seed ^ 0x9E37);
        sfx.send(PlaySfxAt(Sfx::DropLedge, feet));
        shake.send(CameraShakeEvent { trauma: 0.10 });
    }
}

/// Motas de polvo a los pies. Reusa la partícula genérica (`ProceduralParticle`,
/// la misma de las cosechas): nace con un envión, se encoge, se apaga y se borra
/// sola — nada que mantener por frame.
#[allow(clippy::too_many_arguments)]
fn spawn_dust(
    commands: &mut Commands,
    at: Vec2,
    world_z: f32,
    color: Color,
    n: usize,
    bias: Vec2,
    kick: f32,
    seed: u32,
) {
    let mut rng = ha_shared::rng::Rng::new(seed | 1);
    // Justo detrás del jugador en la banda Y-sort: el polvo se ve a sus pies sin
    // taparle la cara.
    let z = crate::layers::ysort(world_z) - crate::layers::HAIR;
    for _ in 0..n {
        let spread = Vec2::new(rng.range(-1.0, 1.0), rng.range(-0.5, 1.0)).normalize_or_zero();
        let dir = (bias * 0.6 + spread * 0.7).normalize_or_zero();
        let speed = kick * rng.range(0.55, 1.25);
        commands.spawn((
            // `UnlitSprite`: la mota se repinta cada frame (el fundido de
            // `tick_particles`), y `light2d` cachea el color de lo que ilumina —
            // le congelaría el alfa entre sus pasadas de 20 Hz y el polvo
            // parpadearía al lado de una antorcha. La noche la sigue oscureciendo
            // igual: el velo va por encima de esta banda.
            crate::sim::UnlitSprite,
            Sprite::from_color(color, Vec2::splat(rng.range(0.10, 0.20))),
            Transform::from_translation(
                (at + Vec2::new(rng.range(-0.22, 0.22), rng.range(-0.18, 0.05))).extend(z),
            ),
            ProceduralParticle {
                velocity: (dir * speed).extend(0.0),
                lifetime: Timer::from_seconds(rng.range(0.28, 0.5), TimerMode::Once),
                scale_decay: 2.2,
            },
            Name::new("VaultDust"),
        ));
    }
}

/// Avanza el impulso vertical del sprite. Dos curvas distintas porque son dos
/// gestos distintos: trepar es un ARCO (te izás por encima del borde y te
/// asentás), caer es una parábola que termina de golpe contra el suelo.
fn tick_vault_bob(time: Res<Time>, mut bob: ResMut<VaultBob>) {
    if bob.dur <= 0.0 {
        bob.y = 0.0;
        return;
    }
    bob.t += time.delta_secs();
    let u = (bob.t / bob.dur).clamp(0.0, 1.0);
    bob.y = if bob.up {
        // Arco: sale de 0, pasa por el máximo antes de la mitad y vuelve a 0.
        bob.height * (u * std::f32::consts::PI).sin() * (1.0 - u * 0.35)
    } else {
        // Caída: arranca arriba y acelera hacia el suelo.
        bob.height * (1.0 - u) * (1.0 - u)
    };
    if u >= 1.0 {
        bob.dur = 0.0;
        bob.y = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ha_shared::CelestialBody;

    #[test]
    fn el_impulso_empieza_y_termina_apoyado() {
        // Un impulso que no vuelve a 0 dejaría al personaje flotando para siempre.
        for up in [true, false] {
            let mut bob = VaultBob::default();
            bob.start(up);
            let dur = bob.dur;
            let mut t = 0.0;
            let mut max: f32 = 0.0;
            while t <= dur + 0.05 {
                let u = (t / dur).clamp(0.0, 1.0);
                let y = if up {
                    bob.height * (u * std::f32::consts::PI).sin() * (1.0 - u * 0.35)
                } else {
                    bob.height * (1.0 - u) * (1.0 - u)
                };
                assert!(y >= -1e-6, "el sprite se hundio bajo sus pies: {y}");
                max = max.max(y);
                t += 1.0 / 60.0;
            }
            assert!(max > 0.2, "impulso invisible ({max} m)");
        }
    }

    /// La condicion que dispara el efecto tiene que poder darse en el mundo REAL,
    /// no solo en teoria: si ningun bordillo trepable cumpliera "cambia el nivel Y
    /// una de las dos celdas es Cliff", esto seria codigo muerto que nadie ve.
    #[test]
    fn los_bordillos_del_mundo_disparan_el_efecto() {
        let tiles = TileWorld::default();
        let body = CelestialBody::Earth;
        let mut found = 0;
        // Un barrido de 200x200 tiles alrededor del origen: el terreno por terrazas
        // deja bordillos por todas partes en llanura (ver `relief::climbable_edge`).
        for tz in -100..100i64 {
            for tx in -100..100i64 {
                if !relief::climbable_edge(body, tx, tz) {
                    continue;
                }
                if tiles.tile_at(body, tx, tz) != Tile::Cliff {
                    continue;
                }
                let l = relief::relief_level(body, tx, tz);
                // El vecino de abajo: cruzarlo es el paso que dispara el polvo.
                if [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)]
                    .iter()
                    .any(|(dx, dz)| relief::relief_level(body, tx + dx, tz + dz) == l - 1)
                {
                    found += 1;
                }
            }
        }
        assert!(
            found > 0,
            "ningun bordillo del mundo cumple la condicion: el efecto nunca se veria"
        );
    }
}
