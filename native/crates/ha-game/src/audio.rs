//! Sonido completo 8/16-bit — TODO sintetizado en memoria al arrancar (cero
//! assets en disco, cero peso en el APK). Filosofía Minecraft: la música es
//! escasa (pistas calmas con minutos de silencio entre medio), el mundo suena
//! por sus acciones. Diseño: `PLAN_SONIDO.md` + `PLAN_EDITOR_SONIDO.md`.
//!
//! El DISEÑO de cada sonido vive en `native/assets/audio.json` (editable con
//! `./build-y-publicar.sh sonido`), embebido al compilar; el synth está en el
//! crate `ha-audio`. Si el JSON está roto, caemos al default (nunca mudo).
//!
//! - SFX: evento `PlaySfx(Sfx::…)` desde cualquier sistema (pitch ±5% aleatorio).
//! - Música: director por contexto (superficie/espacio/subsuelo) con silencios.
//! - Observadores autónomos: pasos, daño/comer/beber, despegue/aterrizaje,
//!   clicks de UI — sin tocar los módulos observados.

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;
use std::collections::HashMap;

use crate::scene::{RegionId, RegionRuntime};
use crate::sim::Player;
use crate::world2d::TileWorld;
use ha_audio::{render_sfx, render_track, wav_bytes, AudioSpec, Lcg};
use ha_shared::topdown::{Tile, TILE_M};

/// El spec editable, embebido en el binario (editás → recompilás → suena).
const AUDIO_JSON: &str = include_str!("../../../assets/audio.json");

/// Volúmenes maestros (0..1). Listo para conectar a un menú de ajustes.
#[derive(Resource)]
pub struct AudioSettings {
    pub sfx: f32,
    pub music: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self { sfx: 0.8, music: 0.5 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sfx {
    Click,
    Dig,
    Chop,
    Place,
    Stamp,
    StepSoft1,
    StepSoft2,
    StepHard1,
    StepHard2,
    BagOpen,
    BagClose,
    Craft,
    Eat,
    Drink,
    Hurt,
    Jump,
    Touchdown,
    DoorOpen,
    DoorClose,
    Stairs,
    Launch,
    Pickup,
}

impl Sfx {
    /// Nombre lógico en `audio.json` (lo que edita el editor de sonido).
    fn key(self) -> &'static str {
        match self {
            Sfx::Click => "click",
            Sfx::Dig => "dig",
            Sfx::Chop => "chop",
            Sfx::Place => "place",
            Sfx::Stamp => "stamp",
            Sfx::StepSoft1 => "step_soft1",
            Sfx::StepSoft2 => "step_soft2",
            Sfx::StepHard1 => "step_hard1",
            Sfx::StepHard2 => "step_hard2",
            Sfx::BagOpen => "bag_open",
            Sfx::BagClose => "bag_close",
            Sfx::Craft => "craft",
            Sfx::Eat => "eat",
            Sfx::Drink => "drink",
            Sfx::Hurt => "hurt",
            Sfx::Jump => "jump",
            Sfx::Touchdown => "touchdown",
            Sfx::DoorOpen => "door_open",
            Sfx::DoorClose => "door_close",
            Sfx::Stairs => "stairs",
            Sfx::Launch => "launch",
            Sfx::Pickup => "pickup",
        }
    }

    const ALL: [Sfx; 22] = [
        Sfx::Click, Sfx::Dig, Sfx::Chop, Sfx::Place, Sfx::Stamp,
        Sfx::StepSoft1, Sfx::StepSoft2, Sfx::StepHard1, Sfx::StepHard2,
        Sfx::BagOpen, Sfx::BagClose, Sfx::Craft, Sfx::Eat, Sfx::Drink,
        Sfx::Hurt, Sfx::Jump, Sfx::Touchdown, Sfx::DoorOpen, Sfx::DoorClose,
        Sfx::Stairs, Sfx::Launch, Sfx::Pickup,
    ];
}

/// Pedido de efecto — desde cualquier sistema: `sfx.send(PlaySfx(Sfx::Dig))`.
#[derive(Event)]
pub struct PlaySfx(pub Sfx);

#[derive(Resource, Default)]
pub struct SfxBank {
    sounds: HashMap<Sfx, Handle<AudioSource>>,
    /// Loop de empuje del cohete/vuelo (PlaybackSettings::LOOP).
    thrust: Option<Handle<AudioSource>>,
}

/// Contexto musical — decide qué pista puede sonar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MusicMood {
    Surface,
    Space,
    Underground,
}

#[derive(Resource, Default)]
pub struct MusicBank {
    surface: Vec<Handle<AudioSource>>,
    space: Vec<Handle<AudioSource>>,
    underground: Vec<Handle<AudioSource>>,
}

/// Director: cuenta el silencio y elige la próxima pista del contexto activo.
#[derive(Resource)]
struct MusicDirector {
    /// Segundos de silencio restantes antes de la próxima pista.
    silence: f32,
    /// Rango de silencio entre pistas (del spec editable).
    silence_min: f32,
    silence_max: f32,
    rng: Lcg,
}

impl Default for MusicDirector {
    fn default() -> Self {
        // Primer tema a los ~25 s de juego (entrada amable, no inmediata).
        Self { silence: 25.0, silence_min: 90.0, silence_max: 240.0, rng: Lcg(0xC418_2026) }
    }
}

#[derive(Component)]
struct MusicTrack;

#[derive(Component)]
struct ThrustLoop;

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioSettings>()
            .init_resource::<SfxBank>()
            .init_resource::<MusicBank>()
            .init_resource::<MusicDirector>()
            .add_event::<PlaySfx>()
            .add_systems(Startup, build_audio_banks)
            .add_systems(
                Update,
                (
                    play_sfx_events,
                    ui_click_sounds,
                    footstep_sounds,
                    survival_sounds,
                    flight_sounds,
                    music_director,
                ),
            );
    }
}

// ---------------------------------------------------------------------------
// Bancos — sintetiza TODO desde el spec embebido (audio.json)
// ---------------------------------------------------------------------------

fn build_audio_banks(
    mut audio_assets: ResMut<Assets<AudioSource>>,
    mut bank: ResMut<SfxBank>,
    mut music: ResMut<MusicBank>,
    mut settings: ResMut<AudioSettings>,
    mut director: ResMut<MusicDirector>,
) {
    let spec = match AudioSpec::from_json(AUDIO_JSON) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ha-audio: audio.json invalido ({e}) — usando diseño default");
            AudioSpec::default_spec()
        }
    };
    let fallback = AudioSpec::default_spec();

    settings.sfx = spec.sfx_volume;
    settings.music = spec.music_volume;
    director.silence = spec.director.first_delay;
    director.silence_min = spec.director.silence_min;
    director.silence_max = spec.director.silence_max.max(spec.director.silence_min);

    for kind in Sfx::ALL {
        // Si el spec editado perdió un SFX, cae al diseño default (nunca mudo).
        let Some(s) = spec.sfx.get(kind.key()).or_else(|| fallback.sfx.get(kind.key())) else {
            continue;
        };
        let wav = wav_bytes(s.rate, &render_sfx(s));
        bank.sounds.insert(kind, audio_assets.add(AudioSource { bytes: wav.into() }));
    }
    let thrust_wav = wav_bytes(spec.thrust.rate, &render_sfx(&spec.thrust));
    bank.thrust = Some(audio_assets.add(AudioSource { bytes: thrust_wav.into() }));

    for track in &spec.tracks {
        let wav = wav_bytes(track.rate, &render_track(track));
        let handle = audio_assets.add(AudioSource { bytes: wav.into() });
        match track.mood.as_str() {
            "space" => music.space.push(handle),
            "underground" => music.underground.push(handle),
            _ => music.surface.push(handle),
        }
    }
    eprintln!(
        "ha-audio: {} SFX + {} pistas sintetizadas en memoria (audio.json)",
        bank.sounds.len(),
        music.surface.len() + music.space.len() + music.underground.len()
    );
}

// ---------------------------------------------------------------------------
// Reproducción
// ---------------------------------------------------------------------------

fn play_sfx_events(
    mut commands: Commands,
    mut events: EventReader<PlaySfx>,
    bank: Res<SfxBank>,
    settings: Res<AudioSettings>,
    mut rng: Local<Option<Lcg>>,
) {
    let rng = rng.get_or_insert_with(|| Lcg(0x5f30_9e37));
    for PlaySfx(kind) in events.read() {
        let Some(handle) = bank.sounds.get(kind) else { continue };
        // Pitch ±5% para que lo repetido no suene a metralleta.
        let speed = rng.range(0.95, 1.05);
        commands.spawn((
            AudioPlayer::new(handle.clone()),
            PlaybackSettings::DESPAWN
                .with_volume(Volume::new(settings.sfx))
                .with_speed(speed),
        ));
    }
}

/// Click en CUALQUIER botón de UI (observador global — sin tocar el HUD).
fn ui_click_sounds(
    mut sfx: EventWriter<PlaySfx>,
    buttons: Query<&Interaction, (Changed<Interaction>, With<Button>)>,
) {
    for i in &buttons {
        if *i == Interaction::Pressed {
            sfx.send(PlaySfx(Sfx::Click));
        }
    }
}

/// Pasos: cadencia por velocidad real del jugador, muestra según el material
/// del tile pisado (pasto/arena = suave; piedra/madera/regolito = duro).
#[allow(clippy::too_many_arguments)]
fn footstep_sounds(
    time: Res<Time>,
    region: Res<RegionRuntime>,
    ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<crate::world2d::ActivePlanet>,
    tiles: Res<TileWorld>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    mut sfx: EventWriter<PlaySfx>,
    mut prev_pos: Local<Option<Vec2>>,
    mut acc: Local<f32>,
    mut alternate: Local<bool>,
    player: Query<&Transform, With<Player>>,
) {
    if crate::world2d::surface_region_active(&region).is_none() || fly.flying() {
        *prev_pos = None;
        return;
    }
    let Ok(tf) = player.get_single() else { return };
    let pos = tf.translation.truncate();
    let Some(prev) = *prev_pos else {
        *prev_pos = Some(pos);
        return;
    };
    *prev_pos = Some(pos);
    let dt = time.delta_secs().max(1e-4);
    let speed = pos.distance(prev) / dt;
    if speed < 0.8 {
        *acc = 0.0;
        return;
    }
    *acc += time.delta_secs();
    // Cadencia: caminar ~0.42 s, correr más rápido (cap 0.26 s).
    let interval = (2.2 / speed).clamp(0.26, 0.5);
    if *acc < interval {
        return;
    }
    *acc = 0.0;
    *alternate = !*alternate;
    let body = crate::world2d::active_surface_body(&ctx, &planet);
    let tx = (tf.translation.x / TILE_M).floor() as i64;
    let tz = (-tf.translation.y / TILE_M).floor() as i64;
    let soft = matches!(
        tiles.tile_at(body, tx, tz),
        Tile::Grass | Tile::Dirt | Tile::Sand
    );
    let kind = match (soft, *alternate) {
        (true, false) => Sfx::StepSoft1,
        (true, true) => Sfx::StepSoft2,
        (false, false) => Sfx::StepHard1,
        (false, true) => Sfx::StepHard2,
    };
    sfx.send(PlaySfx(kind));
}

/// Daño / comer / beber — observando los DELTAS de survival (cero acople).
fn survival_sounds(
    mut sfx: EventWriter<PlaySfx>,
    mut prev: Local<Option<(f32, f32, f32)>>,
    player: Query<&crate::sim::SimState, With<Player>>,
) {
    let Ok(sim) = player.get_single() else { return };
    let now = (sim.hp, sim.survival.hunger, sim.survival.thirst);
    let Some(p) = *prev else {
        *prev = Some(now);
        return;
    };
    *prev = Some(now);
    if now.0 < p.0 - 0.5 {
        sfx.send(PlaySfx(Sfx::Hurt));
    }
    if now.1 > p.1 + 2.0 {
        sfx.send(PlaySfx(Sfx::Eat));
    }
    if now.2 > p.2 + 2.0 {
        sfx.send(PlaySfx(Sfx::Drink));
    }
}

/// Despegue/aterrizaje + loop de empuje, observando los flancos del vuelo
/// creativo y del cohete.
fn flight_sounds(
    mut commands: Commands,
    bank: Res<SfxBank>,
    settings: Res<AudioSettings>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    flight: Res<crate::flight::Flight>,
    actions: Res<crate::input::TouchActions>,
    mut sfx: EventWriter<PlaySfx>,
    mut was_flying: Local<bool>,
    thrust_q: Query<Entity, With<ThrustLoop>>,
) {
    let flying = fly.flying() || flight.active;
    if flying && !*was_flying {
        sfx.send(PlaySfx(Sfx::Launch));
    }
    if !flying && *was_flying {
        sfx.send(PlaySfx(Sfx::Touchdown));
    }
    *was_flying = flying;

    // Loop de aura/empuje: mientras CARGA el Ki en tierra (power-up) o propulsa
    // en el aire (JUMP/USE). `flight.active` == vuelo activo (espejo del Ki-fly).
    let thrusting = fly.charging || (fly.flying() && (actions.jump || actions.action)) || flight.active;
    let has_loop = !thrust_q.is_empty();
    if thrusting && !has_loop {
        if let Some(h) = bank.thrust.clone() {
            commands.spawn((
                ThrustLoop,
                AudioPlayer::new(h),
                PlaybackSettings::LOOP.with_volume(Volume::new(settings.sfx * 0.5)),
            ));
        }
    } else if !thrusting && has_loop {
        for e in &thrust_q {
            commands.entity(e).despawn_recursive();
        }
    }
}

/// Director de música: pista por contexto, con MINUTOS de silencio entre
/// pistas (filosofía Minecraft — la música como acontecimiento, no wallpaper).
fn music_director(
    mut commands: Commands,
    time: Res<Time>,
    region: Res<RegionRuntime>,
    tiles: Res<TileWorld>,
    music: Res<MusicBank>,
    settings: Res<AudioSettings>,
    mut director: ResMut<MusicDirector>,
    playing: Query<Entity, With<MusicTrack>>,
) {
    // Mientras suena una pista, no contar silencio (la entidad se auto-despawnea
    // al terminar por PlaybackSettings::DESPAWN).
    if !playing.is_empty() {
        return;
    }
    director.silence -= time.delta_secs();
    if director.silence > 0.0 {
        return;
    }
    let mood = match region.active {
        RegionId::Space => MusicMood::Space,
        _ if tiles.z < 0 => MusicMood::Underground,
        _ => MusicMood::Surface,
    };
    let pool = match mood {
        MusicMood::Surface => &music.surface,
        MusicMood::Space => &music.space,
        MusicMood::Underground => &music.underground,
    };
    if pool.is_empty() {
        director.silence = 30.0;
        return;
    }
    let pick = (director.rng.next() as usize) % pool.len();
    commands.spawn((
        MusicTrack,
        AudioPlayer::new(pool[pick].clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::new(settings.music)),
    ));
    // Próximo tema: silencio (rango del spec) después de que éste termine.
    let (lo, hi) = (director.silence_min, director.silence_max);
    director.silence = director.rng.range(lo, hi);
}
