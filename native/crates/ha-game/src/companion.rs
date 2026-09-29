//! Compañero-IA — la "mano derecha" del jugador, controlada por la IA FRUGAL del
//! usuario (IAFZ-64, `iafz-sys`). Sigue al jugador reusando el genoma base
//! `FAUNA.adn` (el gym ya lo entrenó para buscar la "comida" por el canal de
//! dirección → le pasamos al JUGADOR como esa comida, y lo persigue con su
//! conocimiento base), APRENDE del maestro (el tester-IA o el jugador) y EVOLUCIONA
//! solo por hill-climb online + persistencia. Diseño en `PLAN_COMPANERO_IA.md`.
//!
//! AISLADO de `fauna.rs`: ids de cerebro (990/991) y archivo (`companion.adn`)
//! propios, para no tocar el ecosistema. Barato: un `think` a ~15 Hz, quad de color
//! (slot lógico `npc/companion` para el arte del usuario).

use std::path::PathBuf;

use bevy::prelude::*;
use ha_shared::fauna_brain::sense;
use ha_shared::CelestialBody;

use crate::scene::WorldContextRuntime;
use crate::sim::Player;
use crate::world2d::{active_surface_body, render_xy, sort_z, ActivePlanet, TileWorld};

/// Cerebros reservados fuera del rango de la fauna (0..72) y dentro de MAX_AGENTS
/// (1000): el genoma VIGENTE y una COPIA MUTADA en prueba (hill-climb).
const CHAMPION_ID: i32 = 990;
const TRIAL_ID: i32 = 991;
/// Conocimiento BASE: el genoma que el gym ya entrenó para forrajear/esquivar.
const GENOME: &[u8] = include_bytes!("../../iafz-sys/genomes/FAUNA.adn");

const AI_HZ: f32 = 15.0;
const TURN: f32 = 2.8;
const VEL: f32 = 5.2; // un pelín más rápido que la fauna: te alcanza
const EVAL_SECS: f32 = 6.0; // ventana de evaluación champion/trial
const FOLLOW_MIN: f32 = 2.0; // no se te encima
const LEASH: f32 = 55.0; // si te alejás más (volaste/teleport), reaparece a tu lado
const BASE_MUT: f32 = 0.05;
/// Hoja del Character Generator que viste al compañero (ver `society::spawn_npc`).
const COMPANION_SKIN: &str = "npc/char_7";
/// Medida humana: la misma que el jugador y los aldeanos (el cuadro es 1:2).
const COMPANION_SIZE: Vec2 = Vec2::new(0.95, 1.9);

fn companion_adn() -> PathBuf {
    crate::inventory_persist::persist_dir().join("vivarium").join("companion.adn")
}

#[derive(Component)]
pub struct Companion {
    x: f32,
    z: f32,
    heading: f32,
    body: CelestialBody,
    /// Fase del hill-climb: false = corriendo el CHAMPION, true = probando el TRIAL.
    using_trial: bool,
    champ_fit: f32,
    trial_fit: f32,
    phase_t: f32,
    no_improve: u32,
    rate: f32,
    prev_px: f32,
    prev_pz: f32,
    say_cd: f32,
    was_founded: bool,
}

/// Tarea asignada al compañero (mano derecha): lo maneja el jugador (o el tester
/// vía comando). `Follow` = te sigue; `Gather` = deambula amplio (reúne/explora);
/// `Pray` = orbita cerca del ídolo (venera).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CompanionTask {
    #[default]
    Follow,
    Gather,
    Pray,
}

#[derive(Resource, Default)]
pub struct CompanionOrder(pub CompanionTask);

pub struct CompanionPlugin;

impl Plugin for CompanionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CompanionOrder>()
            .add_systems(Update, (spawn_companion, tick_companion).chain());
    }
}

/// Spawnea al compañero UNA vez, en la superficie, junto al jugador. Carga el
/// genoma persistido (`companion.adn`) si existe — así el conocimiento se ACUMULA
/// entre sesiones — o el base `FAUNA.adn` la primera vez.
fn spawn_companion(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    existing: Query<(), With<Companion>>,
    player: Query<&Transform, With<Player>>,
) {
    if !existing.is_empty() {
        return;
    }
    // Sólo en superficie (la mano derecha camina con vos; volar es F2).
    if !matches!(ctx.active, ha_shared::WorldContext::Overworld | ha_shared::WorldContext::Luna) {
        return;
    }
    let Ok(pt) = player.get_single() else {
        return;
    };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let body = active_surface_body(&ctx, &planet);

    // Champion: genoma persistido si hay, si no el base entrenado por el gym.
    let adn = companion_adn();
    let loaded = adn.exists() && iafz_sys::load(CHAMPION_ID, &adn.to_string_lossy());
    if !loaded {
        iafz_sys::load_bytes(CHAMPION_ID, GENOME);
    }
    iafz_sys::reset_echo(CHAMPION_ID);
    // Primer trial = champion + mutación.
    iafz_sys::mutate(CHAMPION_ID, TRIAL_ID, BASE_MUT);
    iafz_sys::reset_echo(TRIAL_ID);

    let (cx, cz) = (px + 1.5, pz + 1.5);
    // **Cuerpo entero, como un aldeano.** El slot `npc/companion` apuntaba a un PNG
    // de 39×39 que es SÓLO UNA CABEZA (resto del arte 3D horneado, `e4d9595`), y
    // estirado a 1,7 m cuadrado el compañero era una cabeza flotante (lo reportó
    // el usuario el 28 sep 2026). Se viste con una hoja del Character Generator,
    // la misma familia y la misma medida humana que los aldeanos y el jugador.
    let sprite = art.sprite_or_color(COMPANION_SKIN, Color::srgb(0.35, 0.7, 1.0), COMPANION_SIZE);
    let mut ent = commands.spawn((
        sprite,
        Transform::from_xyz(cx, -cz, sort_z(cz)),
        Companion {
            x: cx,
            z: cz,
            heading: 0.0,
            body,
            using_trial: false,
            champ_fit: 0.0,
            trial_fit: 0.0,
            phase_t: 0.0,
            no_improve: 0,
            rate: BASE_MUT,
            prev_px: px,
            prev_pz: pz,
            say_cd: 3.0,
            was_founded: false,
        },
        Name::new("Companion"),
    ));
    crate::light2d::attach(&mut ent, &art, &[COMPANION_SKIN], COMPANION_SIZE);
    eprintln!(
        "ha-companion: spawn (genoma {})",
        if loaded { "persistido" } else { "base FAUNA.adn" }
    );
}

/// El compañero piensa con la IA frugal, sigue al jugador y evoluciona (hill-climb).
#[allow(clippy::type_complexity)]
fn tick_companion(
    time: Res<Time>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    order: Res<CompanionOrder>,
    religion: Res<crate::society::Religion>,
    mut speech: ResMut<crate::dialog::SpeechQueue>,
    player: Query<&Transform, (With<Player>, Without<Companion>)>,
    mut q: Query<(&mut Companion, &mut Transform, &mut Sprite)>,
    mut acc: Local<f32>,
) {
    // ~15 Hz como la fauna (batería). En pausa dt=0 → no corre.
    *acc += time.delta_secs();
    if *acc < 1.0 / AI_HZ {
        return;
    }
    let dt = (*acc).min(0.2);
    *acc = 0.0;

    let Ok((mut c, mut tf, mut sprite)) = q.get_single_mut() else {
        return;
    };
    let Ok(pt) = player.get_single() else {
        return;
    };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let body = active_surface_body(&ctx, &planet);

    // Si cambiaste de cuerpo o te alejaste mucho (volaste), reaparece a tu lado —
    // sigue siendo tu mano derecha.
    let far = {
        let dx = px - c.x;
        let dz = pz - c.z;
        (dx * dx + dz * dz).sqrt() > LEASH
    };
    if body != c.body || far {
        c.x = px + 1.5;
        c.z = pz + 1.5;
        c.body = body;
    }

    // ── PENSAR (IA frugal) ────────────────────────────────────────────────────
    let brain = if c.using_trial { TRIAL_ID } else { CHAMPION_ID };
    let dx = px - c.x;
    let dz = pz - c.z;
    let dist = (dx * dx + dz * dz).sqrt().max(0.001);
    // El JUGADOR es la "comida": el genoma base ya sabe ir hacia el sector de comida.
    let energy = (1.0 - dist / 30.0).clamp(0.0, 1.0);
    let s = sense(body, c.x, c.z, c.heading, energy, &[(px, pz)], &[]);
    let steer = iafz_sys::think(brain, s).tanh();
    // La IA frugal aporta esquive/textura (autoridad reducida); la TAREA da rumbo.
    c.heading += steer * TURN * dt * 0.4;

    // ── TAREA (mano derecha): distancia y estilo según la orden del jugador ─────
    use std::f32::consts::{FRAC_PI_2, PI, TAU};
    let desired = match order.0 {
        CompanionTask::Follow => FOLLOW_MIN,
        CompanionTask::Pray => 1.3, // se pega a orar
        CompanionTask::Gather => 7.0, // deambula amplio (reúne/explora)
    };
    let toward = dz.atan2(dx);
    let goal = if dist > desired + 0.5 {
        toward // acercate
    } else if matches!(order.0, CompanionTask::Pray) {
        toward + FRAC_PI_2 // orbita al ídolo
    } else {
        c.heading + (c.x * 0.3 + time.elapsed_secs() * 0.5).sin() * 0.3 // paseo suave
    };
    let diff = (goal - c.heading + PI).rem_euclid(TAU) - PI;
    c.heading += diff.clamp(-1.0, 1.0) * TURN * dt * 0.9;

    // ── MOVER ─────────────────────────────────────────────────────────────────
    let speed = VEL * if dist > desired { 1.0 } else { 0.25 };
    let nx = c.x + c.heading.cos() * speed * dt;
    let nz = c.z + c.heading.sin() * speed * dt;
    let mut hit_wall = false;
    if tiles.solid_at(body, nx.floor() as i64, nz.floor() as i64) {
        c.heading += 1.3; // rebota
        hit_wall = true;
    } else {
        c.x = nx;
        c.z = nz;
    }

    // ── FITNESS (qué es ser buena mano derecha) ───────────────────────────────
    // Cercanía sostenida al jugador + IMITAR su rumbo (aprender del maestro) −
    // chocar paredes.
    let mut fit = (12.0 - dist).clamp(-6.0, 12.0) * dt;
    let pvx = px - c.prev_px;
    let pvz = pz - c.prev_pz;
    let pspeed = (pvx * pvx + pvz * pvz).sqrt();
    if pspeed > 0.05 {
        let (cdx, cdz) = (c.heading.cos(), c.heading.sin());
        let dot = (cdx * pvx + cdz * pvz) / pspeed; // −1..1: rumbo vs el del maestro
        fit += dot * 3.0 * dt;
    }
    if hit_wall {
        fit -= 2.0 * dt;
    }
    c.prev_px = px;
    c.prev_pz = pz;
    if c.using_trial {
        c.trial_fit += fit;
    } else {
        c.champ_fit += fit;
    }

    // ── HILL-CLIMB (evoluciona solo) ──────────────────────────────────────────
    c.phase_t += dt;
    if c.phase_t >= EVAL_SECS {
        c.phase_t = 0.0;
        if !c.using_trial {
            // Terminó la ventana del champion → ahora probamos el trial.
            c.using_trial = true;
            iafz_sys::reset_echo(TRIAL_ID);
        } else {
            // Terminó la ventana del trial → comparamos.
            if c.trial_fit > c.champ_fit {
                // El trial es mejor → PROMOVER (copia sin mutar) y GUARDAR.
                iafz_sys::mutate(TRIAL_ID, CHAMPION_ID, 0.0);
                let adn = companion_adn();
                if let Some(dir) = adn.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                iafz_sys::save(CHAMPION_ID, &adn.to_string_lossy());
                c.no_improve = 0;
                c.rate = BASE_MUT;
                eprintln!(
                    "ha-companion: champion MEJORADO (trial {:.1} > champ {:.1}), guardado",
                    c.trial_fit, c.champ_fit
                );
            } else {
                c.no_improve += 1;
                // Anti-mínimo local: sin mejoras → explorar más fuerte.
                if c.no_improve >= 3 {
                    c.rate = (c.rate * 1.6).min(0.35);
                    c.no_improve = 0;
                }
            }
            // Nuevo trial = champion + mutación; volvemos a correr el champion.
            iafz_sys::mutate(CHAMPION_ID, TRIAL_ID, c.rate);
            iafz_sys::reset_echo(TRIAL_ID);
            c.using_trial = false;
            c.champ_fit = 0.0;
            c.trial_fit = 0.0;
        }
    }

    // ── RENDER ────────────────────────────────────────────────────────────────
    let r = render_xy(c.x, c.z);
    tf.translation = Vec3::new(r.x, r.y, sort_z(c.z));
    // Skin natural; leve dorado si ya fundaron el culto (es el primer discípulo).
    sprite.color = if religion.founded {
        Color::srgb(1.0, 0.96, 0.78)
    } else {
        Color::WHITE
    };

    // ── CHAT (mano derecha): reacciona al culto y a la tarea; conversa ─────────
    c.say_cd -= dt;
    let just_founded = religion.founded && !c.was_founded;
    c.was_founded = religion.founded;
    if just_founded {
        crate::dialog::say(
            &mut speech,
            c.x,
            c.z,
            format!("Contigo hasta el fin, {}!", religion.name),
            Color::srgb(1.0, 0.9, 0.35),
        );
        c.say_cd = 6.0;
    } else if c.say_cd <= 0.0 {
        c.say_cd = 9.0;
        let line = match order.0 {
            CompanionTask::Follow if religion.founded => "Los fieles crecen, jefe.",
            CompanionTask::Follow => "A tus ordenes.",
            CompanionTask::Gather => "Reuniendo al rebano.",
            CompanionTask::Pray => "Que el Ki nos guie.",
        };
        crate::dialog::say(&mut speech, c.x, c.z, line, Color::srgb(0.6, 0.9, 1.0));
    }
}
