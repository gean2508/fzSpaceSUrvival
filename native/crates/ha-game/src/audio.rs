//! Sonido completo 8/16-bit — TODO sintetizado en memoria al arrancar (cero
//! assets en disco, cero peso en el APK). Filosofía Minecraft: la música es
//! escasa (pistas calmas con minutos de silencio entre medio), el mundo suena
//! por sus acciones. Diseño: `biblia.md` §7.
//!
//! El DISEÑO de cada sonido vive en `native/assets/audio.json` (editable con
//! `./build-y-publicar.sh sonido`), embebido al compilar; el synth está en el
//! crate `ha-audio`. Si el JSON está roto, caemos al default (nunca mudo).
//!
//! - SFX: evento `PlaySfx(Sfx::…)` desde cualquier sistema (pitch ±5% aleatorio).
//! - SFX de MUNDO: `PlaySfxAt(Sfx::…, pos)` — suena desde un punto del mapa, con
//!   distancia, paneo izquierda/derecha y **sordina por obstáculos** (ver abajo).
//! - Música: director por contexto (superficie/espacio/subsuelo) con silencios.
//! - Observadores autónomos: pasos, daño/comer/beber, hachazos al talar,
//!   despegue/aterrizaje, clicks de UI — sin tocar los módulos observados.
//!
//! ## Propagación (por qué un golpe detrás de una pared suena distinto)
//!
//! Un sonido de mundo no es "el sample a volumen fijo": se resuelve contra la
//! GEOMETRÍA que hay entre la fuente y el oyente. Marchamos el segmento
//! fuente→jugador contando obstáculos (paredes construidas y tiles sólidos:
//! roca, muro, acantilado) y de ahí sale una **oclusión** 0..1 que hace dos
//! cosas: baja el volumen y elige una copia con MÁS GRAVES del mismo sample
//! (pre-renderizadas al arrancar, `MUFFLE_LEVELS`). Eso es lo que el oído lee
//! como "está del otro lado de la pared": lo que una pared se come primero son
//! los agudos, no el volumen. Sin paredes en medio el sonido es seco y sólo
//! cae por distancia.

use bevy::audio::{AudioPlayer, AudioSink, AudioSource, PlaybackSettings, SpatialListener, Volume};
use bevy::prelude::*;
use std::collections::HashMap;

use crate::scene::{RegionId, RegionRuntime};
use crate::sim::Player;
use crate::walls::BuiltWalls;
use crate::world2d::TileWorld;
use ha_audio::{render_sfx, render_track, wav_bytes, AudioSpec, Lcg};
use ha_shared::build_grid::WallAxis;
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
    /// Trepar un bordillo: roce de tela contra piedra (ver `vault.rs`).
    ClimbLedge,
    /// Descolgarse de un bordillo: golpe seco de los dos pies.
    DropLedge,
    // — 2 oct 2026 (motor v2): superficies, subsuelo, descubrimiento y peligro —
    StepSand1,
    StepSand2,
    StepWood1,
    StepWood2,
    StepSnow,
    /// Gota en la cueva (reverb largo).
    Drip,
    /// Campanitas de cristal (bioma de cristal).
    CrystalChime,
    /// Derrumbe suave de roca.
    Crumble,
    /// Pico contra mena o gema.
    OrePing,
    /// ¡Un experimento salió! (cuaderno del jugador)
    Discover,
    /// El experimento no pasó nada / se echó a perder.
    Fizzle,
    /// Meta cumplida.
    Goal,
    /// «No se puede»: aviso suave de rechazo.
    Deny,
    /// Entrar al agua.
    Splash,
    /// Gemido de infectado.
    Groan,
    /// Mordida.
    Bite,
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
            Sfx::ClimbLedge => "climb_ledge",
            Sfx::DropLedge => "drop_ledge",
            Sfx::StepSand1 => "step_sand1",
            Sfx::StepSand2 => "step_sand2",
            Sfx::StepWood1 => "step_wood1",
            Sfx::StepWood2 => "step_wood2",
            Sfx::StepSnow => "step_snow",
            Sfx::Drip => "drip",
            Sfx::CrystalChime => "crystal_chime",
            Sfx::Crumble => "crumble",
            Sfx::OrePing => "ore_ping",
            Sfx::Discover => "discover",
            Sfx::Fizzle => "fizzle",
            Sfx::Goal => "goal",
            Sfx::Deny => "deny",
            Sfx::Splash => "splash",
            Sfx::Groan => "groan",
            Sfx::Bite => "bite",
        }
    }

    const ALL: [Sfx; 40] = [
        Sfx::Click, Sfx::Dig, Sfx::Chop, Sfx::Place, Sfx::Stamp,
        Sfx::StepSoft1, Sfx::StepSoft2, Sfx::StepHard1, Sfx::StepHard2,
        Sfx::BagOpen, Sfx::BagClose, Sfx::Craft, Sfx::Eat, Sfx::Drink,
        Sfx::Hurt, Sfx::Jump, Sfx::Touchdown, Sfx::DoorOpen, Sfx::DoorClose,
        Sfx::Stairs, Sfx::Launch, Sfx::Pickup,
        Sfx::ClimbLedge, Sfx::DropLedge,
        Sfx::StepSand1, Sfx::StepSand2, Sfx::StepWood1, Sfx::StepWood2, Sfx::StepSnow,
        Sfx::Drip, Sfx::CrystalChime, Sfx::Crumble, Sfx::OrePing, Sfx::Discover, Sfx::Fizzle,
        Sfx::Goal, Sfx::Deny, Sfx::Splash, Sfx::Groan, Sfx::Bite,
    ];
}

/// Pedido de efecto **sin posición** (UI, avisos, cosas "en la cabeza" del
/// jugador) — desde cualquier sistema: `sfx.send(PlaySfx(Sfx::Click))`.
#[derive(Event)]
pub struct PlaySfx(pub Sfx);

/// Pedido de efecto **con posición de mundo** (coordenadas de render, las
/// mismas del `Transform` del jugador). Es el que hay que usar para TODO lo que
/// pasa en el mapa: hachazos, picar, colocar, puertas, NPCs trabajando. Se
/// atenúa por distancia, panea a izquierda/derecha y se amortigua si hay
/// paredes o roca en medio (ver el encabezado del módulo).
#[derive(Event)]
pub struct PlaySfxAt(pub Sfx, pub Vec2);

/// Alcance (m): más lejos que esto un sonido de mundo ni se encola.
const SFX_RANGE_M: f32 = 46.0;
/// Distancia a la que se ubica el emisor respecto del oyente. Sólo sirve para
/// el PANEO: rodio no atenúa por debajo de 1 m, así que el volumen real queda
/// 100 % en nuestras manos (distancia + oclusión) y no se aplica dos veces.
const EMITTER_RADIUS: f32 = 1.0;
/// Separación de "orejas": cuanto más grande, más marcado el paneo L/R.
const EAR_GAP: f32 = 0.55;
/// El paneo de rodio reparte entre 0.5 y 1.0 por oreja: un sonido centrado
/// quedaría a 0.75 y todo lo posicional sonaría más flojo que la UI. Esto lo
/// devuelve al nivel del resto.
const PAN_COMPENSATION: f32 = 1.3;
/// Cuánta oclusión (0..1) aporta CADA obstáculo entre la fuente y el oyente.
/// ~3 paredes ya tapan del todo — pasar de un cuarto al de al lado se nota,
/// estar al otro lado del edificio deja apenas un retumbe.
const OCCLUSION_PER_BLOCKER: f32 = 0.34;
/// Copias del mismo sample con distinto grado de sordina (0 = seco).
const MUFFLE_LEVELS: usize = 3;
/// Corte del filtro paso-bajos de cada nivel (Hz) y cuántas veces se aplica:
/// un polo suena a manta, tres a "pared de ladrillo".
const MUFFLE_CUTOFF_HZ: [f32; MUFFLE_LEVELS] = [0.0, 1600.0, 520.0];
const MUFFLE_PASSES: [u32; MUFFLE_LEVELS] = [0, 2, 3];

#[derive(Resource, Default)]
pub struct SfxBank {
    /// Por SFX, una copia por nivel de sordina (índice 0 = seco).
    sounds: HashMap<Sfx, [Handle<AudioSource>; MUFFLE_LEVELS]>,
    /// Loop de empuje del cohete/vuelo (PlaybackSettings::LOOP).
    thrust: Option<Handle<AudioSource>>,
}

/// Las "orejas" del jugador: entidad con `SpatialListener` que seguimos a su
/// posición cada frame. Sin ella rodio panea respecto del origen del mundo.
#[derive(Component)]
struct AudioEars;

/// Versión del motor de música: sube cuando cambia CÓMO suena una pista, para invalidar el caché de disco.
use ha_audio::{hash_pista, MUSICA_VERSION};

/// Qué pistas trae YA compuestas el APK (`assets/audio/musica/<nombre>.wav`) y con qué huella. Componer en el celu
/// costaba decenas de segundos de CPU por pieza —un hilo entero compitiendo con el render en un Helio G88— y ése era
/// uno de los motivos del lag; ahora sólo se compone lo que el índice no cubre (una partitura editada en el estudio).
const MUSICA_PRERENDERIZADA: &str = include_str!("../assets/audio/musica.json");

fn prerenderizada(t: &ha_audio::TrackSpec) -> bool {
    serde_json::from_str::<HashMap<String, String>>(MUSICA_PRERENDERIZADA)
        .ok()
        .and_then(|m| m.get(&t.name).cloned())
        .map_or(false, |h| h == format!("{:08x}", hash_pista(t)))
}

/// **Las pistas se componen cuando hacen falta, no al arrancar.** Una pieza de dos minutos tarda ~2 s en la laptop y
/// bastante más en el Redmi: sintetizar once en el boot lo dejaría minutos con la pantalla trabada. Ahora el banco
/// guarda las PARTITURAS (`TrackSpec`), compone una a la vez en un hilo aparte y deja el WAV en disco
/// (`<datos>/musica/`), así la segunda vez que suena es instantánea.
#[derive(Resource, Default)]
pub struct MusicBank {
    /// Partituras por contexto (`TrackSpec::mood`): surface, night, underground, cristal, space, tension, menu.
    specs: HashMap<String, Vec<ha_audio::TrackSpec>>,
    /// Pistas ya compuestas (nombre → audio).
    listas: HashMap<String, Handle<AudioSource>>,
    /// La que se está componiendo ahora (una sola a la vez).
    en_curso: Option<(String, bevy::tasks::Task<Vec<u8>>)>,
    /// Para cargar las pistas pre-renderizadas del APK.
    server: Option<AssetServer>,
}

impl MusicBank {
    /// Pide componer `t` en segundo plano si no está lista ni hay otra en curso.
    fn pedir(&mut self, t: &ha_audio::TrackSpec) {
        if self.listas.contains_key(&t.name) {
            return;
        }
        // La pista ya viene compuesta en el APK: se carga como un asset más, sin gastar CPU.
        if let Some(server) = &self.server {
            if prerenderizada(t) {
                let h = server.load(format!("audio/musica/{}.wav", t.name));
                self.listas.insert(t.name.clone(), h);
                return;
            }
        }
        if self.en_curso.is_some() {
            return;
        }
        let spec = t.clone();
        let pool = bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        self.en_curso = Some((t.name.clone(), pool.spawn(async move { wav_de_pista(&spec) })));
    }

    fn primera(&self, mood: &str) -> Option<&ha_audio::TrackSpec> {
        self.specs.get(mood).and_then(|v| v.first())
    }
}

/// El WAV de una pista: del caché de disco si ya se compuso con esta versión y esta partitura; si no, se compone.
fn wav_de_pista(t: &ha_audio::TrackSpec) -> Vec<u8> {
    let h = hash_pista(t);
    let dir = crate::inventory_persist::base_dir().join("musica");
    let ruta = dir.join(format!("{}-{:08x}.wav", t.name, h));
    if let Ok(b) = std::fs::read(&ruta) {
        return b;
    }
    let wav = wav_bytes(t.rate, &render_track(t));
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&ruta, &wav);
    wav
}

/// Recoge la pista que terminó de componerse y la deja lista para sonar.
fn terminar_composicion(mut bank: ResMut<MusicBank>, mut assets: ResMut<Assets<AudioSource>>) {
    let Some((nombre, tarea)) = bank.en_curso.as_mut() else { return };
    let Some(bytes) = bevy::tasks::block_on(bevy::tasks::futures_lite::future::poll_once(tarea)) else { return };
    let nombre = nombre.clone();
    bank.en_curso = None;
    let handle = assets.add(AudioSource { bytes: bytes.into() });
    bank.listas.insert(nombre, handle);
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
    /// Segundos hasta que otra pista de TENSIÓN (infectados cerca) pueda interrumpir.
    tension_cd: f32,
    /// Lo que se pidió y todavía se está componiendo (no se cuenta el silencio mientras se espera).
    esperando: Option<String>,
}

impl Default for MusicDirector {
    fn default() -> Self {
        // Primer tema a los ~25 s de juego (entrada amable, no inmediata).
        Self { silence: 25.0, silence_min: 90.0, silence_max: 240.0, rng: Lcg(0xC418_2026), tension_cd: 0.0, esperando: None }
    }
}

#[derive(Component)]
struct MusicTrack;

/// La pista se está apagando (se baja el volumen y se borra).
#[derive(Component)]
struct Fundir(f32);

/// La música del menú (se repite mientras estés en el menú).
#[derive(Component)]
struct MusicaMenu;

/// La pista de tensión (infectados cerca).
#[derive(Component)]
struct MusicaTension;

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
            .add_event::<PlaySfxAt>()
            .add_systems(Startup, build_audio_banks)
            .add_systems(Update, (terminar_composicion, musica_menu, fundir_musica))
            .add_systems(
                Update,
                (
                    sync_audio_ears,
                    play_sfx_events,
                    play_positional_sfx,
                    ui_click_sounds,
                    footstep_sounds,
                    ambiente_subsuelo,
                    harvest_hit_sounds,
                    survival_sounds,
                    flight_sounds,
                    music_director,
                )
                    // En background no se sintetiza ni se encola NADA: el mixer
                    // de rodio vive en su propio hilo y seguiría sonando con la
                    // pantalla bloqueada (ver `platform::sync_lifecycle_pause`).
                    .run_if(|| !crate::platform::is_app_paused()),
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
    server: Res<AssetServer>,
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
        let dry = render_sfx(s);
        // Una copia por nivel de sordina: filtrar en vivo por sample costaría
        // CPU en el hilo de audio; acá se paga una vez al arrancar y son
        // milisegundos de PCM (nada de peso en el APK, todo sintetizado).
        let variants = std::array::from_fn(|lvl| {
            let buf = if lvl == 0 {
                dry.clone()
            } else {
                lowpass(&dry, s.rate, MUFFLE_CUTOFF_HZ[lvl], MUFFLE_PASSES[lvl])
            };
            let wav = wav_bytes(s.rate, &buf);
            audio_assets.add(AudioSource { bytes: wav.into() })
        });
        bank.sounds.insert(kind, variants);
    }
    let thrust_wav = wav_bytes(spec.thrust.rate, &render_sfx(&spec.thrust));
    bank.thrust = Some(audio_assets.add(AudioSource { bytes: thrust_wav.into() }));

    for track in &spec.tracks {
        music.specs.entry(track.mood.clone()).or_default().push(track.clone());
    }
    music.server = Some(server.clone());
    // La música del menú se compone YA (en un hilo aparte): es lo primero que se oye.
    if let Some(t) = music.primera("menu").cloned() {
        music.pedir(&t);
    }
    eprintln!(
        "ha-audio: {} SFX sintetizados + {} partituras de música (se componen al hacer falta, con caché en disco)",
        bank.sounds.len(),
        spec.tracks.len()
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
        let Some(handles) = bank.sounds.get(kind) else { continue };
        // Pitch ±5% para que lo repetido no suene a metralleta.
        let speed = rng.range(0.95, 1.05);
        commands.spawn((
            AudioPlayer::new(handles[0].clone()),
            PlaybackSettings::DESPAWN
                .with_volume(Volume::new(settings.sfx))
                .with_speed(speed),
        ));
    }
}

// ---------------------------------------------------------------------------
// Sonido posicional: distancia + paneo + oclusión por obstáculos
// ---------------------------------------------------------------------------

/// Paso-bajos de un polo aplicado `passes` veces sobre el PCM ya renderizado.
/// Es la "pared" en forma de filtro: se lleva los agudos y deja el cuerpo.
fn lowpass(buf: &[f32], rate: u32, cutoff_hz: f32, passes: u32) -> Vec<f32> {
    let mut out = buf.to_vec();
    if cutoff_hz <= 0.0 || rate == 0 {
        return out;
    }
    let dt = 1.0 / rate as f32;
    let rc = 1.0 / (std::f32::consts::TAU * cutoff_hz);
    let alpha = dt / (rc + dt);
    for _ in 0..passes {
        let mut y = 0.0f32;
        for s in out.iter_mut() {
            y += alpha * (*s - y);
            *s = y;
        }
    }
    out
}

/// Las orejas viven en su propia entidad (rotación identidad) siguiendo al
/// jugador: si colgaran del jugador o de la cámara, cualquier giro de esos
/// transforms invertiría el paneo sin que nadie lo pidiera.
fn sync_audio_ears(
    mut commands: Commands,
    player: Query<&Transform, (With<Player>, Without<AudioEars>)>,
    mut ears: Query<&mut Transform, With<AudioEars>>,
) {
    let Ok(tf) = player.get_single() else { return };
    let pos = tf.translation.truncate().extend(0.0);
    if let Ok(mut ear_tf) = ears.get_single_mut() {
        ear_tf.translation = pos;
    } else {
        commands.spawn((
            AudioEars,
            SpatialListener::new(EAR_GAP),
            Transform::from_translation(pos),
        ));
    }
}

/// Punto de mundo → celda del grid (el eje Z del mundo es `-y` en pantalla).
fn tile_of(p: Vec2) -> (i64, i64) {
    ((p.x / TILE_M).floor() as i64, (-p.y / TILE_M).floor() as i64)
}

/// Cuánto tapan la geometría entre `from` y `to` (0 = línea limpia, 1 = sordo).
///
/// Marcha el segmento a media celda contando: tiles sólidos (roca, muro,
/// acantilado, bloques colocados) y paredes construidas en las aristas que
/// cruza. Es la misma geometría que decide el paso del jugador, así que el
/// oído y las piernas nunca se contradicen.
fn occlusion_between(
    from: Vec2,
    to: Vec2,
    body: ha_shared::CelestialBody,
    z: i32,
    tiles: &TileWorld,
    walls: &BuiltWalls,
) -> f32 {
    let delta = to - from;
    let dist = delta.length();
    if dist < 0.5 {
        return 0.0;
    }
    // Media celda por paso, con techo: un sonido a 46 m no puede costar más de
    // ~130 sondeos por evento (y los eventos son puntuales, no por frame).
    let steps = ((dist / (TILE_M * 0.5)).ceil() as i32).clamp(1, 140);
    let mut blockers = 0.0f32;
    let mut prev = tile_of(from);
    for i in 1..=steps {
        let p = from + delta * (i as f32 / steps as f32);
        let cell = tile_of(p);
        if cell == prev {
            continue;
        }
        // Pared construida en la arista que acabamos de cruzar. En diagonal se
        // cruzan dos aristas: se cobran las dos (esquina = más tapado).
        if cell.0 != prev.0 {
            let tx = cell.0.max(prev.0);
            if walls.blocks(body, z, tx, prev.1, WallAxis::V) {
                blockers += 1.0;
            }
        }
        if cell.1 != prev.1 {
            let tz = cell.1.max(prev.1);
            if walls.blocks(body, z, cell.0, tz, WallAxis::H) {
                blockers += 1.0;
            }
        }
        // Tile macizo (roca, muro de cueva, bloque puesto por el jugador). El
        // agua honda no tapa un sonido aéreo, así que no cuenta.
        let t = tiles.tile_at(body, cell.0, cell.1);
        if t.solid() && t != Tile::WaterDeep {
            blockers += 1.0;
        }
        prev = cell;
    }
    (blockers * OCCLUSION_PER_BLOCKER).clamp(0.0, 1.0)
}

/// Reproduce los SFX de mundo: volumen por distancia, sordina por obstáculos y
/// paneo L/R hacia donde ocurrió.
#[allow(clippy::too_many_arguments)]
fn play_positional_sfx(
    mut commands: Commands,
    mut events: EventReader<PlaySfxAt>,
    bank: Res<SfxBank>,
    settings: Res<AudioSettings>,
    tiles: Res<TileWorld>,
    walls: Res<BuiltWalls>,
    ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<crate::world2d::ActivePlanet>,
    region: Res<RegionRuntime>,
    player: Query<&Transform, With<Player>>,
    mut rng: Local<Option<Lcg>>,
) {
    if events.is_empty() {
        return;
    }
    let Ok(tf) = player.get_single() else {
        events.clear();
        return;
    };
    let listener = tf.translation.truncate();
    // En vuelo/espacio no hay grid de superficie contra el cual ocluir: el
    // sonido queda limpio, sólo con distancia y paneo.
    let occluding = crate::world2d::surface_region_active(&region).is_some();
    let body = crate::world2d::active_surface_body(&ctx, &planet);
    let rng = rng.get_or_insert_with(|| Lcg(0x71c3_5a19));

    for PlaySfxAt(kind, pos) in events.read() {
        let Some(handles) = bank.sounds.get(kind) else { continue };
        let dist = listener.distance(*pos);
        if dist > SFX_RANGE_M {
            continue;
        }
        // Caída suave hasta el borde del alcance (cuadrática: cerca manda la
        // acción, lejos queda de fondo sin cortarse de golpe).
        let falloff = (1.0 - dist / SFX_RANGE_M).clamp(0.0, 1.0).powi(2);
        let occ = if occluding {
            occlusion_between(*pos, listener, body, tiles.z, &tiles, &walls)
        } else {
            0.0
        };
        // La pared se come volumen ADEMÁS de agudos, pero nunca hasta el
        // silencio: un golpe al otro lado se sigue intuyendo (eso es lo que
        // avisa "hay alguien ahí").
        let gain = settings.sfx * PAN_COMPENSATION * falloff * (1.0 - 0.6 * occ);
        if gain <= 0.004 {
            continue;
        }
        let level = ((occ * MUFFLE_LEVELS as f32) as usize).min(MUFFLE_LEVELS - 1);
        // El emisor se coloca a 1 m en la DIRECCIÓN de la fuente: rodio panea
        // por posición pero también atenúa por distancia, y esa atenuación ya
        // la calculamos nosotros (con las paredes incluidas).
        let dir = (*pos - listener).normalize_or_zero();
        let emitter = (listener + dir * EMITTER_RADIUS).extend(0.0);
        // Lo lejano y tapado además suena un pelo más grave.
        let speed = rng.range(0.95, 1.05) - 0.04 * occ;
        commands.spawn((
            AudioPlayer::new(handles[level].clone()),
            PlaybackSettings::DESPAWN
                .with_volume(Volume::new(gain))
                .with_speed(speed)
                .with_spatial(true),
            Transform::from_translation(emitter),
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
    mut en_agua: Local<bool>,
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
    // Cada superficie suena a su material: pasto/tierra blando, arena que cruje, nieve/hielo, madera hueca y
    // piedra dura (la galería, la vereda, el piso de piedra). Al meterse en el agua, un chapuzón.
    let tile = tiles.tile_at(body, tx, tz);
    if tile.swim() {
        if !*en_agua {
            *en_agua = true;
            sfx.send(PlaySfx(Sfx::Splash));
        }
        return;
    }
    *en_agua = false;
    let alt = *alternate;
    let kind = match tile {
        Tile::Sand => if alt { Sfx::StepSand2 } else { Sfx::StepSand1 },
        Tile::Ice => Sfx::StepSnow,
        Tile::WoodFloor | Tile::Door | Tile::Step | Tile::Stairwell | Tile::Window => if alt { Sfx::StepWood2 } else { Sfx::StepWood1 },
        Tile::Grass | Tile::Dirt => if alt { Sfx::StepSoft2 } else { Sfx::StepSoft1 },
        _ => if alt { Sfx::StepHard2 } else { Sfx::StepHard1 },
    };
    sfx.send(PlaySfx(kind));
}

/// **El aire del subsuelo.** Bajo tierra el silencio también es un sonido: cada 4-12 s, algo lejano —una gota con su
/// eco, un derrumbe suave, un golpe de mena, campanitas de cristal— según el BIOMA donde estás
/// (`ha_shared::subsuelo`). Va sin posición (nadie sabe de dónde viene) y bajito: es ambiente, no aviso.
fn ambiente_subsuelo(
    time: Res<Time>,
    tiles: Res<TileWorld>,
    ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<crate::world2d::ActivePlanet>,
    player: Query<&Transform, With<Player>>,
    mut sfx: EventWriter<PlaySfx>,
    mut resto: Local<f32>,
    mut rng: Local<Option<Lcg>>,
) {
    use ha_shared::subsuelo::Bioma;
    if tiles.z >= 0 {
        *resto = 5.0;
        return;
    }
    *resto -= time.delta_secs();
    if *resto > 0.0 {
        return;
    }
    let Ok(tf) = player.get_single() else { return };
    let rng = rng.get_or_insert_with(|| Lcg(0xD21F_5EED));
    let body = crate::world2d::active_surface_body(&ctx, &planet);
    let bioma = ha_shared::subsuelo::bioma_en(body, (tf.translation.x / TILE_M).floor() as i64, (-tf.translation.y / TILE_M).floor() as i64);
    let r = rng.f32();
    let kind = match bioma {
        Bioma::Piedra => if r < 0.7 { Sfx::Drip } else { Sfx::Crumble },
        Bioma::Arcilla => if r < 0.55 { Sfx::Drip } else { Sfx::Crumble },
        Bioma::Musgo => if r < 0.92 { Sfx::Drip } else { Sfx::Crumble },
        Bioma::Cristal => if r < 0.55 { Sfx::CrystalChime } else { Sfx::Drip },
        Bioma::Hierro => if r < 0.4 { Sfx::OrePing } else if r < 0.8 { Sfx::Drip } else { Sfx::Crumble },
    };
    sfx.send(PlaySfx(kind));
    *resto = rng.range(4.0, 12.0) * if bioma == Bioma::Musgo { 0.6 } else { 1.0 };
}

/// Cada cuánto cae un hachazo/picazo mientras se mantiene el hold (s).
const HIT_PERIOD: f32 = 0.34;

/// **Golpes de recolección**: talar, picar roca/mineral, romper un bloque.
///
/// El hold de recolección nunca había sonado — `Sfx::Chop` existía en el banco
/// pero NINGÚN sistema lo emitía, así que talar un árbol era mudo. Se resuelve
/// acá, observando `HoldInteractState` como los demás observadores del módulo
/// (sin tocar targeting ni el HUD): el primer golpe sale al instante de apretar
/// y después uno por `HIT_PERIOD`, en la POSICIÓN del objetivo — de modo que el
/// hachazo del vecino detrás de una pared llega tapado, como cualquier otro
/// sonido de mundo.
fn harvest_hit_sounds(
    time: Res<Time>,
    hold: Res<crate::input::HoldInteractState>,
    target: Res<crate::targeting::LookTarget>,
    player: Query<&Transform, With<Player>>,
    mut sfx: EventWriter<PlaySfxAt>,
    mut acc: Local<f32>,
    mut swinging: Local<bool>,
) {
    use ha_shared::InteractTargetKind as K;
    let kind = hold.locked_kind.or(target.kind);
    let hit = kind.and_then(|k| match k {
        K::Tree => Some(Sfx::Chop),
        K::OreIron | K::OreGold | K::GemBlue | K::GemRed => Some(Sfx::OrePing),
        K::Rock | K::Block => Some(Sfx::Dig),
        _ => None,
    });
    let Some(hit) = hit.filter(|_| hold.active) else {
        *swinging = false;
        *acc = 0.0;
        return;
    };
    // El punto del objetivo; si todavía no hay uno, el propio jugador.
    let pos = if target.kind.is_some() {
        target.hit_point.truncate()
    } else if let Ok(tf) = player.get_single() {
        tf.translation.truncate()
    } else {
        return;
    };
    if !*swinging {
        *swinging = true;
        *acc = 0.0;
        sfx.send(PlaySfxAt(hit, pos));
        return;
    }
    *acc += time.delta_secs();
    if *acc >= HIT_PERIOD {
        *acc -= HIT_PERIOD;
        sfx.send(PlaySfxAt(hit, pos));
    }
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

/// Director de música: pista por contexto, con MINUTOS de silencio entre pistas (filosofía Minecraft — la música como
/// acontecimiento, no wallpaper) y, como Project Zomboid, una pieza de TENSIÓN que corta cuando se acercan los infectados.
///
/// Contextos: espacio · subsuelo (y, en el bioma de cristal, su pieza propia) · noche · día. De noche suena otra música
/// que de día; los infectados a menos de 28 celdas apagan lo que suene y ponen `tension` (con 150 s de enfriamiento).
#[allow(clippy::too_many_arguments)]
fn music_director(
    mut commands: Commands,
    time: Res<Time>,
    phase: Res<State<crate::game_phase::GamePhase>>,
    region: Res<RegionRuntime>,
    tiles: Res<TileWorld>,
    day: Res<crate::sky2d::DayClock>,
    ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<crate::world2d::ActivePlanet>,
    mut bank: ResMut<MusicBank>,
    settings: Res<AudioSettings>,
    mut director: ResMut<MusicDirector>,
    playing: Query<(Entity, Option<&MusicaTension>), (With<MusicTrack>, Without<Fundir>, Without<MusicaMenu>)>,
    infectados: Query<&crate::infectados::Infectado>,
    player: Query<&Transform, With<Player>>,
) {
    if !crate::game_phase::world_active(phase) {
        return;
    }
    director.tension_cd = (director.tension_cd - time.delta_secs()).max(0.0);
    // ¿Hay infectados cerca? Entonces la tensión corta lo que suene.
    let cerca = player.get_single().ok().map(|t| (t.translation.x, -t.translation.y)).map_or(false, |(px, pz)| {
        infectados.iter().any(|i| ((i.x - px).powi(2) + (i.z - pz).powi(2)) < 28.0 * 28.0)
    });
    let sonando_tension = playing.iter().any(|(_, t)| t.is_some());
    let quiere_tension = cerca && director.tension_cd <= 0.0 && !sonando_tension && region.active != RegionId::Space && tiles.z >= 0;
    if quiere_tension {
        for (e, _) in &playing {
            commands.entity(e).insert(Fundir(0.0));
        }
        director.silence = 0.0;
        director.esperando = Some("tension".to_string());
        director.tension_cd = 150.0;
    }
    // Mientras suena una pista (no apagándose), no contar silencio.
    if !playing.is_empty() && !quiere_tension {
        return;
    }
    if director.esperando.is_none() {
        director.silence -= time.delta_secs();
        if director.silence > 0.0 {
            return;
        }
    }
    let mood = if let Some(m) = director.esperando.clone() {
        m
    } else if region.active == RegionId::Space {
        "space".to_string()
    } else if tiles.z < 0 {
        let body = crate::world2d::active_surface_body(&ctx, &planet);
        let bioma = player
            .get_single()
            .ok()
            .map(|t| ha_shared::subsuelo::bioma_en(body, (t.translation.x / TILE_M).floor() as i64, (-t.translation.y / TILE_M).floor() as i64));
        if bioma == Some(ha_shared::subsuelo::Bioma::Cristal) && director.rng.f32() < 0.55 {
            "cristal".to_string()
        } else {
            "underground".to_string()
        }
    } else if day.daylight() < 0.25 {
        "night".to_string()
    } else {
        "surface".to_string()
    };
    let n = bank.specs.get(&mood).map_or(0, |v| v.len());
    if n == 0 {
        director.esperando = None;
        director.silence = 30.0;
        return;
    }
    let pick = (director.rng.next() as usize) % n;
    let spec = bank.specs[&mood][pick].clone();
    let Some(handle) = bank.listas.get(&spec.name).cloned() else {
        // Todavía no está compuesta: se pide y se espera (sin gastar el silencio).
        bank.pedir(&spec);
        director.esperando = Some(mood);
        return;
    };
    director.esperando = None;
    let mut ent = commands.spawn((
        MusicTrack,
        AudioPlayer::new(handle),
        PlaybackSettings::DESPAWN.with_volume(Volume::new(settings.music)),
    ));
    if mood == "tension" {
        ent.insert(MusicaTension);
    }
    // Próximo tema: silencio (rango del spec) después de que éste termine.
    let (lo, hi) = (director.silence_min, director.silence_max);
    director.silence = director.rng.range(lo, hi);
}

/// La música del MENÚ: suena (en bucle) mientras estés en el menú o creando el personaje, y se apaga al jugar.
fn musica_menu(
    mut commands: Commands,
    phase: Res<State<crate::game_phase::GamePhase>>,
    mut bank: ResMut<MusicBank>,
    settings: Res<AudioSettings>,
    sonando: Query<Entity, (With<MusicaMenu>, Without<Fundir>)>,
) {
    use crate::game_phase::GamePhase;
    let en_menu = matches!(*phase.get(), GamePhase::MainMenu | GamePhase::CharacterCreate);
    if !en_menu {
        for e in &sonando {
            commands.entity(e).insert(Fundir(0.0));
        }
        return;
    }
    if !sonando.is_empty() {
        return;
    }
    let Some(spec) = bank.primera("menu").cloned() else { return };
    match bank.listas.get(&spec.name).cloned() {
        Some(h) => {
            commands.spawn((
                MusicTrack,
                MusicaMenu,
                AudioPlayer::new(h),
                PlaybackSettings::LOOP.with_volume(Volume::new(settings.music * 0.8)),
            ));
        }
        None => bank.pedir(&spec),
    }
}

/// Baja el volumen de lo que se está apagando (1,8 s) y lo borra.
fn fundir_musica(mut commands: Commands, time: Res<Time>, mut q: Query<(Entity, &mut Fundir, Option<&AudioSink>)>, settings: Res<AudioSettings>) {
    for (e, mut f, sink) in &mut q {
        f.0 += time.delta_secs();
        let k = (1.0 - f.0 / 1.8).clamp(0.0, 1.0);
        if let Some(s) = sink {
            s.set_volume(settings.music * k);
        }
        if k <= 0.0 {
            commands.entity(e).despawn_recursive();
        }
    }
}
