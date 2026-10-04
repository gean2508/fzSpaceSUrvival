//! Clima de superficie: **despejado → nublado → lluvia → tormenta**, con viento,
//! relámpagos y truenos. Todo procedural (cero assets) y todo derivado de UN
//! estado (`Weather`), que es lo que consultan el cielo, las luces, las sombras y
//! los bichos.
//!
//! ## Por qué el clima es un recurso y no un efecto
//!
//! Lo que hace que un clima se sienta no es la lluvia: es que **todo lo demás
//! reaccione**. Nublado apaga las sombras (no hay sol que las proyecte) y adelanta
//! el encendido de las luces; la lluvia guarda las luciérnagas y las mariposas;
//! la tormenta agrega viento que ladea la lluvia y arrastra las partículas. Por
//! eso acá sólo vive el ESTADO y el dibujo de la lluvia — cada sistema lee
//! `Weather` y decide lo suyo:
//!
//! - `sky2d`   → color del cielo, opacidad del velo, sol/luna tapados.
//! - `light2d` → oscuridad efectiva (las luces se encienden antes) y sombras.
//! - `ambient` → los bichos se guardan; el viento los empuja.
//!
//! Sólo corre en cuerpos CON atmósfera y sobre la superficie: en la Luna no hay
//! clima que valga, y bajo tierra no llueve.

use bevy::audio::{AudioPlayer, AudioSink, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;

use crate::scene::WorldContextRuntime;
use crate::sim::Player;
use crate::world2d::{surface_region_active, ActivePlanet, TileWorld};
use ha_audio::{wav_bytes, Lcg};
use ha_shared::topdown::TILE_M;

/// Los cuatro climas. Es una escalera: cada uno trae lo del anterior y algo más
/// (nubes → agua → viento y rayos).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WeatherKind {
    #[default]
    Clear,
    Cloudy,
    Rain,
    Storm,
}

impl WeatherKind {
    /// Nombre ASCII para el HUD (la fuente del juego no dibuja acentos).
    pub fn label(self) -> &'static str {
        match self {
            WeatherKind::Clear => "DESPEJADO",
            WeatherKind::Cloudy => "NUBLADO",
            WeatherKind::Rain => "LLUVIA",
            WeatherKind::Storm => "TORMENTA",
        }
    }

    /// `(nubosidad, lluvia, viento)` a los que tiende este clima.
    fn targets(self) -> (f32, f32, f32) {
        match self {
            WeatherKind::Clear => (0.0, 0.0, 0.15),
            WeatherKind::Cloudy => (0.62, 0.0, 0.35),
            WeatherKind::Rain => (0.85, 0.55, 0.5),
            WeatherKind::Storm => (1.0, 1.0, 1.0),
        }
    }

    /// Cuánto dura (segundos) antes de volver a tirar los dados. La tormenta es
    /// corta a propósito: es un evento, no un estado.
    fn span(self) -> (f32, f32) {
        match self {
            WeatherKind::Clear => (180.0, 420.0),
            WeatherKind::Cloudy => (120.0, 300.0),
            WeatherKind::Rain => (90.0, 210.0),
            WeatherKind::Storm => (45.0, 110.0),
        }
    }

    /// A dónde puede ir después. **Cadena, no ruleta**: de despejado no se pasa a
    /// tormenta de un frame al otro — primero se nubla. Eso es lo que hace que el
    /// cielo se sienta observado en vez de sorteado.
    fn next(self, r: f32) -> Self {
        match self {
            WeatherKind::Clear => {
                if r < 0.55 {
                    WeatherKind::Clear
                } else {
                    WeatherKind::Cloudy
                }
            }
            WeatherKind::Cloudy => {
                if r < 0.38 {
                    WeatherKind::Clear
                } else if r < 0.82 {
                    WeatherKind::Rain
                } else {
                    WeatherKind::Storm
                }
            }
            WeatherKind::Rain => {
                if r < 0.45 {
                    WeatherKind::Cloudy
                } else if r < 0.80 {
                    WeatherKind::Rain
                } else {
                    WeatherKind::Storm
                }
            }
            WeatherKind::Storm => {
                if r < 0.70 {
                    WeatherKind::Rain
                } else {
                    WeatherKind::Cloudy
                }
            }
        }
    }
}

/// El estado del cielo AHORA. Los tres valores continuos se persiguen a su
/// objetivo con una rampa lenta: nunca hay un frame en el que empiece a llover.
#[derive(Resource)]
pub struct Weather {
    pub kind: WeatherKind,
    /// Nubosidad 0..1 — apaga el sol, aplana las sombras, oscurece el día.
    pub cover: f32,
    /// Agua cayendo 0..1.
    pub rain: f32,
    /// Viento con signo, en "fuerza" 0..1 (el signo es la dirección en x).
    pub wind: f32,
    /// Hacia dónde sopla (±1). Se sortea al cambiar el clima y NO cada frame: un
    /// viento que cambia de lado sesenta veces por segundo es viento cero.
    wind_dir: f32,
    /// Fogonazo de relámpago 0..1 (decae solo).
    pub flash: f32,
    /// Segundos hasta el trueno del último relámpago (el sonido llega después de
    /// la luz — es lo que hace que un rayo se sienta lejos o encima tuyo).
    thunder_in: Option<(f32, f32)>,
    /// Segundos hasta el próximo relámpago.
    bolt_in: f32,
    /// Segundos hasta volver a tirar los dados del clima.
    next_change: f32,
    rng: Lcg,
    /// `true` cuando lo que cae es NIEVE y no agua. No es un clima aparte: es la
    /// misma precipitación leída en un sitio FRÍO (bioma nevado/hielo/montaña), que
    /// es como funciona de verdad — la misma borrasca es lluvia abajo y nieve arriba.
    pub snowing: bool,
    /// Nieve ACUMULADA en el suelo, 0..1. Sube mientras nieva y se derrite despacio
    /// cuando para: el paisaje se queda blanco un rato después de la nevada, que es
    /// justo lo que hace que la nevada se sienta un evento y no un filtro.
    pub snow_depth: f32,
}

impl Default for Weather {
    fn default() -> Self {
        Self {
            kind: WeatherKind::Clear,
            cover: 0.0,
            rain: 0.0,
            wind: 0.15,
            wind_dir: 1.0,
            flash: 0.0,
            thunder_in: None,
            bolt_in: 6.0,
            // Empieza despejado un rato largo: la primera impresión del juego no
            // puede ser una tormenta.
            next_change: 240.0,
            rng: Lcg(0x51E5_7A11),
            snowing: false,
            snow_depth: 0.0,
        }
    }
}

impl Weather {
    /// Cuánto atenúa la luz del día (1 = pleno sol, 0.35 = tormenta cerrada). Lo
    /// usan el cielo y `light2d` para que las luces se enciendan con el nublado.
    pub fn daylight_mul(&self) -> f32 {
        1.0 - 0.62 * self.cover
    }

    /// Cuánto de la luz DIRECCIONAL (sol/luna) sobrevive: con el cielo tapado no
    /// hay sombras proyectadas, hay penumbra pareja.
    pub fn direct_mul(&self) -> f32 {
        (1.0 - self.cover * 0.92).max(0.0)
    }

    /// Viento en m/s sobre las partículas (positivo = hacia el este/derecha).
    pub fn wind_ms(&self) -> f32 {
        self.wind * 6.5
    }

    /// `true` mientras el agua desaconseje sacar los bichos a volar.
    pub fn wet(&self) -> bool {
        self.rain > 0.22
    }
}

/// Loop de lluvia + trueno, sintetizados al arrancar (cero peso en el APK).
#[derive(Resource, Default)]
struct WeatherAudio {
    rain: Option<Handle<AudioSource>>,
    thunder: Option<Handle<AudioSource>>,
}

#[derive(Component)]
struct RainLoop;

#[derive(Component)]
pub struct LightningFlash;

/// Una gota. Vive en coordenadas de RENDER (la lluvia cae por la pantalla, no por
/// la grilla del mundo) y se recicla arriba cuando sale por abajo.
#[derive(Component)]
struct RainDrop {
    vy: f32,
    seed: u32,
}

/// Cuántas gotas hay como mucho. Son quads planos de un solo color: batchean
/// todas juntas, así que el costo real es el `Transform` de cada una.
const DROPS: usize = 160;
/// Semi-alto (m) de la caja donde viven, en metros de mundo.
const RAIN_H: f32 = 22.0;
/// Semi-ancho.
const RAIN_W: f32 = 30.0;
/// Z de dibujo: por encima del velo de noche y de las luciérnagas — la lluvia
/// pasa por delante de todo, incluido el jugador.
const RAIN_Z: f32 = 930.0;

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Weather>()
            .init_resource::<WeatherAudio>()
            .init_resource::<SnowTracks>()
            .add_systems(Startup, (build_weather_audio, spawn_flash_overlay, spawn_snow_blanket))
            // Sólo en partida: ni lluvia, ni truenos, ni fogonazos de rayo en el
            // splash y el menú (el fogonazo es un overlay a pantalla completa —
            // exactamente el tipo de parpadeo que no debe existir en el arranque).
            .add_systems(
                Update,
                (
                    tick_weather,
                    spawn_rain,
                    update_rain,
                    track_footsteps,
                    sync_snow_blanket,
                    sync_weather_audio,
                    sync_flash,
                )
                    .chain()
                    .run_if(crate::game_phase::world_active),
            );
    }
}

// ── Estado ───────────────────────────────────────────────────────────────────

/// Clima forzado por entorno (QA). Se lee cada vez: es una comparación de string
/// por frame, y a cambio se puede cambiar el clima sin recompilar.
fn forced_kind() -> Option<WeatherKind> {
    match crate::flags::var("HA_WEATHER").ok()?.to_ascii_lowercase().as_str() {
        "clear" | "despejado" => Some(WeatherKind::Clear),
        "cloudy" | "nublado" => Some(WeatherKind::Cloudy),
        "rain" | "lluvia" => Some(WeatherKind::Rain),
        "storm" | "tormenta" => Some(WeatherKind::Storm),
        _ => None,
    }
}

fn hash01(mut s: u32) -> f32 {
    s ^= s << 13;
    s ^= s >> 17;
    s ^= s << 5;
    (s % 100_000) as f32 / 100_000.0
}

/// ¿Este mundo tiene clima? Hace falta aire (la Luna no llueve) y estar en
/// superficie: bajo tierra el cielo no existe.
fn weather_allowed(
    region: &crate::scene::RegionRuntime,
    tiles: &TileWorld,
    ctx: &WorldContextRuntime,
    planet: &ActivePlanet,
) -> bool {
    // `z != 0` y no `z > 0`: bajo un techo de tierra tampoco llueve. Que la
    // lluvia siguiera cayendo dentro de la galería era la mitad de lo que hacía
    // que el subsuelo se leyera como "el overworld pintado de marrón".
    if tiles.z != 0 || surface_region_active(region).is_none() {
        return false;
    }
    let body = crate::world2d::active_surface_body(ctx, planet);
    ha_shared::transition_profile(body).sky_fade_end > 0.0
}

#[allow(clippy::too_many_arguments)]
fn tick_weather(
    time: Res<Time>,
    region: Res<crate::scene::RegionRuntime>,
    tiles: Res<TileWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mut w: ResMut<Weather>,
    snow_player: Query<&Transform, With<Player>>,
    mut sfx: Commands,
    audio: Res<WeatherAudio>,
    settings: Res<crate::audio::AudioSettings>,
    mut shake: EventWriter<crate::camera::CameraShakeEvent>,
) {
    let _probe = crate::debug::probe("tick_weather");
    let dt = time.delta_secs().min(0.25);
    let allowed = weather_allowed(&region, &tiles, &ctx, &planet);

    // `HA_WEATHER=clear|cloudy|rain|storm` clava el clima: sin esto, probar una
    // tormenta es esperar sentado a que el dado la saque.
    if let Some(forced) = forced_kind() {
        w.kind = forced;
        w.next_change = 1.0e9;
    } else if allowed {
        w.next_change -= dt;
        if w.next_change <= 0.0 {
            let r = w.rng.f32();
            w.kind = w.kind.next(r);
            let (lo, hi) = w.kind.span();
            let f = w.rng.f32();
            w.next_change = lo + (hi - lo) * f;
            w.wind_dir = if w.rng.f32() < 0.5 { -1.0 } else { 1.0 };
        }
    }
    // Fuera de un mundo con clima el estado no se pierde: se DESVANECE (así
    // volver a la Tierra no te tira una tormenta encima de golpe).
    let (mut c_t, mut r_t, mut w_t) = w.kind.targets();
    if !allowed {
        c_t = 0.0;
        r_t = 0.0;
        w_t = 0.0;
    }

    // ── NIEVE ────────────────────────────────────────────────────────────────
    // No es un clima aparte: es la MISMA precipitación cayendo en un sitio frío.
    // La misma borrasca es lluvia en el valle y nieve en la cumbre, y eso se lee
    // andando — subir una montaña con el cielo cerrado y ver el agua volverse
    // nieve vale más que un dado que saque "NIEVE" cada tantos minutos.
    // `HA_SNOW=1` fuerza el frío donde estés: probar una nevada es, si no, buscar una
    // cumbre y esperar a que el dado saque lluvia. Mismo espíritu que `HA_WEATHER`.
    let forced_cold = crate::flags::var("HA_SNOW").is_ok();
    let cold_here = allowed
        && (forced_cold
        || snow_player
            .get_single()
            .map(|t| {
                use ha_shared::geography::Biome;
                let body = crate::world2d::active_surface_body(&ctx, &planet);
                let (tx, tz) = (t.translation.x.floor() as i64, (-t.translation.y).floor() as i64);
                matches!(
                    ha_shared::geography::biome_at(body, tx, tz),
                    Biome::Snow | Biome::Ice | Biome::Mountain
                )
            })
            .unwrap_or(false));
    w.snowing = cold_here && w.rain > 0.02;
    // Se acumula mientras nieva (~50 s hasta el manto pleno) y se derrite MUCHO más
    // despacio: el paisaje se queda blanco un rato después de que pare.
    let melt = if cold_here { 0.0006 } else { 0.0035 };
    w.snow_depth = if w.snowing {
        (w.snow_depth + dt * 0.02 * (0.4 + w.rain)).min(1.0)
    } else {
        (w.snow_depth - dt * melt).max(0.0)
    };
    // Rampas lentas y separadas: la nubosidad tarda un minuto largo en cerrarse,
    // el agua arranca y para más rápido (un chaparrón), el viento va en el medio.
    let approach = |v: f32, t: f32, k: f32| v + (t - v) * (k * dt).min(1.0);
    w.cover = approach(w.cover, c_t, 0.10);
    w.rain = approach(w.rain, r_t, 0.22);
    // Ráfagas: el viento respira en vez de soplar parejo (dos senos incoherentes,
    // el mismo truco que la llama de una antorcha).
    let gust = 1.0 + 0.25 * (time.elapsed_secs() * 0.7).sin() * (time.elapsed_secs() * 0.23).cos();
    let dir = w.wind_dir;
    w.wind = approach(w.wind, w_t * gust * dir, 0.35);

    // ── Relámpagos ──────────────────────────────────────────────────────────
    w.flash = (w.flash - dt * 4.2).max(0.0);
    if allowed && w.kind == WeatherKind::Storm && w.cover > 0.6 {
        w.bolt_in -= dt;
        if w.bolt_in <= 0.0 {
            let f = w.rng.f32();
            w.bolt_in = 4.0 + f * 11.0;
            w.flash = 1.0;
            // Distancia del rayo: de 0.4 s (encima) a 4 s (en el horizonte). El
            // retardo del trueno ES la distancia — no hace falta nada más.
            let d = w.rng.f32();
            w.thunder_in = Some((0.4 + d * 3.6, 1.0 - d * 0.55));
        }
    } else {
        w.bolt_in = w.bolt_in.max(3.0);
    }
    if let Some((t, vol)) = w.thunder_in {
        let t = t - dt;
        if t <= 0.0 {
            w.thunder_in = None;
            if let Some(h) = audio.thunder.as_ref() {
                sfx.spawn((
                    AudioPlayer::new(h.clone()),
                    PlaybackSettings::DESPAWN
                        .with_volume(Volume::new(settings.sfx * vol.clamp(0.2, 1.0))),
                ));
            }
            // Un trueno cerca SE SIENTE. La sacudida va con el sonido, no con el
            // fogonazo: es el golpe de aire, no la luz.
            if vol > 0.55 {
                shake.send(crate::camera::CameraShakeEvent { trauma: 0.10 + 0.22 * vol });
            }
        } else {
            w.thunder_in = Some((t, vol));
        }
    }
}

// ── Lluvia ───────────────────────────────────────────────────────────────────

fn spawn_rain(mut commands: Commands, mut done: Local<bool>) {
    if *done {
        return;
    }
    *done = true;
    for i in 0..DROPS {
        commands.spawn((
            RainDrop { vy: 0.0, seed: 0x9E37_79B9 ^ i as u32 },
            // `UnlitSprite`: la gota es agua, no un objeto del mundo — que no la
            // tiña la luz de una antorcha ni le cuelguen una sombra de contacto.
            crate::sim::UnlitSprite,
            Sprite::from_color(Color::srgba(0.70, 0.80, 0.96, 0.0), Vec2::new(0.05, 0.6)),
            Transform::from_xyz(0.0, 0.0, RAIN_Z),
            Visibility::Hidden,
        ));
    }
}

#[allow(clippy::type_complexity)]
fn update_rain(
    time: Res<Time>,
    w: Res<Weather>,
    tiles: Res<TileWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    zoom: Res<crate::camera::CameraZoom>,
    player: Query<&Transform, With<Player>>,
    mut drops: Query<(&mut RainDrop, &mut Transform, &mut Sprite, &mut Visibility), Without<Player>>,
    mut tick: Local<u32>,
) {
    let _probe = crate::debug::probe("update_rain");
    // A gran encuadre (volando alto, órbita) la gota mide menos de un píxel y son
    // 160 transformes por frame para nada.
    // Adentro de un edificio no llueve (el techo está; el sonido llega apagado, ver `sync_weather_audio`).
    let on = w.rain > 0.02 && zoom.live_view_h < 90.0 && !ha_shared::interiores::es_interior(tiles.z);
    if !on {
        for (_, _, _, mut vis) in &mut drops {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        }
        return;
    }
    let Ok(pt) = player.get_single() else { return };
    let (px, py) = (pt.translation.x, pt.translation.y);
    let dt = time.delta_secs().min(0.1);
    *tick = tick.wrapping_add(1);
    let body = crate::world2d::active_surface_body(&ctx, &planet);
    let wind = w.wind_ms();
    // Cuántas gotas se usan de verdad: una llovizna no tiene la densidad de una
    // tormenta, y las que sobran se apagan (no se despawnean: se reciclan).
    let live = (DROPS as f32 * w.rain).round() as usize;
    let slant = (wind / 15.0).clamp(-0.7, 0.7);
    for (i, (mut d, mut tf, mut sprite, mut vis)) in drops.iter_mut().enumerate() {
        if i >= live {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        }
        if d.vy == 0.0 {
            d.vy = -(13.0 + hash01(d.seed) * 7.0);
        }
        // El copo cae a un cuarto de la velocidad de la gota: es lo que separa
        // visualmente una nevada de un chaparrón, más que el color.
        let fall = if w.snowing { 0.22 } else { 0.85 + 0.5 * w.rain };
        tf.translation.x += wind * dt * if w.snowing { 0.5 } else { 1.0 };
        tf.translation.y += d.vy * fall * dt;
        // Salió de la caja (cayó, o caminaste): renace arriba, en una columna
        // nueva alrededor tuyo.
        let out = tf.translation.y < py - RAIN_H
            || (tf.translation.x - px).abs() > RAIN_W
            || (tf.translation.y - py).abs() > RAIN_H * 1.6;
        if out {
            d.seed = d.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223 ^ *tick);
            tf.translation.x = px + (hash01(d.seed) - 0.5) * 2.0 * RAIN_W;
            tf.translation.y = py + RAIN_H * (0.75 + 0.35 * hash01(d.seed ^ 0x9E37_79B9));
            d.vy = -(13.0 + hash01(d.seed ^ 0x51ED_2701) * 7.0);
        }
        // **La lluvia no entra bajo techo.** Una celda de muro/roca la para
        // (sólido), y una de PISO CONSTRUIDO también: si hay tablas o baldosas
        // bajo tus pies es porque hay una casa encima. Sin esa segunda condición
        // sólo dejaba de llover justo sobre las paredes, que es el único sitio
        // donde no importa.
        let tx = (tf.translation.x / TILE_M).floor() as i64;
        let tz = (-tf.translation.y / TILE_M).floor() as i64;
        let tile = tiles.tile_at(body, tx, tz);
        // `is_build_floor` y no una lista a mano: es la MISMA pregunta que se
        // hace la construcción ("¿es piso construido?") y tenerla escrita dos
        // veces fue exactamente lo que dejó afuera al piso de piedra — llovía
        // adentro de una casa según con qué material la hubieras hecho.
        let indoor = ha_shared::build_grid::is_floor_piece(tile);
        let covered = tiles.z != 0 || tile.solid() || indoor;
        if w.snowing {
            // COPO: cuadradito blanco que baja despacio y se mece. Reusa la misma
            // gota (pool, reciclado, "no entra bajo techo") — un copo no es otro
            // sistema de partículas, es agua a otra temperatura.
            let a = if covered { 0.0 } else { (0.35 + 0.45 * w.rain).min(0.85) };
            sprite.color = Color::srgba(0.97, 0.98, 1.0, a);
            let sway = ((time.elapsed_secs() * 1.7) + hash01(d.seed) * 6.28).sin() * 0.35;
            tf.translation.x += sway * dt;
            tf.rotation = Quat::IDENTITY;
            let sz = 0.10 + hash01(d.seed ^ 0x5EED) * 0.10;
            sprite.custom_size = Some(Vec2::splat(sz));
        } else {
        let a = if covered { 0.0 } else { (0.10 + 0.30 * w.rain).min(0.42) };
        sprite.color = Color::srgba(0.70, 0.80, 0.96, a);
        // Ladeada por el viento: una tormenta se lee en la INCLINACIÓN de la
        // lluvia mucho antes que en su densidad.
        tf.rotation = Quat::from_rotation_z(slant);
        sprite.custom_size = Some(Vec2::new(0.05, 0.5 + 0.5 * w.rain));
        }
        let want = if covered { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
    }
}

// ── Manto de nieve acumulada ─────────────────────────────────────────────────

/// Lado (m) de una celda de nieve. Es el grano del manto: con esto se ven ondas y
/// pisadas, y no hace falta un sprite por tile (que serían miles).
const SNOW_CELL: f32 = 2.0;
/// Rejilla de celdas dibujadas alrededor del jugador — 26×16 = 416 quads planos del
/// mismo color, que batchean juntos. Cubre de sobra el encuadre a pie.
const SNOW_COLS: i32 = 26;
const SNOW_ROWS: i32 = 16;
/// Cuánto tarda una pisada en volver a taparse **nevando**. Si no nieva no se tapa:
/// el rastro se queda ahí, que es medio sentido de caminar sobre nieve.
const TRACK_REFILL: f32 = 0.055;
/// Cuántas pisadas se recuerdan. Sobra para lo que se ve en pantalla; las viejas se
/// podan por distancia, así que andar diez kilómetros no acumula memoria.
const MAX_TRACKS: usize = 900;

/// Una celda del manto. Se spawnean todas al arrancar y se RECICLAN: cada frame se
/// re-anclan a la rejilla de celdas alrededor tuyo, así el manto no "nada" con la
/// cámara — está clavado al mundo, y por eso se le nota el relieve al caminar.
#[derive(Component)]
struct SnowCell;

/// Las pisadas: celda → cuánta nieve le queda pisoteada (1 = recién pisada).
#[derive(Resource, Default)]
pub struct SnowTracks(std::collections::HashMap<(i64, i64), f32>);

fn spawn_snow_blanket(mut commands: Commands) {
    for _ in 0..(SNOW_COLS * SNOW_ROWS) {
        commands.spawn((
            SnowCell,
            // `UnlitSprite`: el color se repinta cada frame, y `light2d` captura el
            // color de referencia UNA vez — sin esto el manto latiría (es el mismo
            // motivo por el que la gota de lluvia está excluida). La luz del día se
            // le aplica a mano, abajo.
            crate::sim::UnlitSprite,
            Sprite::from_color(Color::srgba(0.94, 0.96, 1.0, 0.0), Vec2::splat(SNOW_CELL)),
            // Justo encima del arte de piso y muy por debajo del Y-sort: la nieve tapa
            // el terreno, nunca a lo que está de pie sobre ella.
            Transform::from_xyz(0.0, 0.0, crate::layers::FLOOR + 0.02),
            Visibility::Hidden,
        ));
    }
}

/// Ruido barato y estable por celda (0..1). Es lo que le da GRANO al manto: sin él
/// la nieve es una lámina de pintura blanca y se nota plana.
fn cell_noise(cx: i64, cz: i64, salt: u32) -> f32 {
    let mut h = (cx as u32)
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add((cz as u32).wrapping_mul(0x85EB_CA6B))
        .wrapping_add(salt);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    (h & 0xFFFF) as f32 / 65_535.0
}

/// Marca por dónde vas pisando. Sólo cuando hay manto: sin nieve no hay rastro que
/// dejar. Se guarda la celda pisada y las cuatro vecinas más cercanas al paso, así
/// el surco tiene ancho de persona y no de píxel.
fn track_footsteps(
    time: Res<Time>,
    w: Res<Weather>,
    mut tracks: ResMut<SnowTracks>,
    player: Query<&Transform, With<Player>>,
    mut last: Local<Option<Vec2>>,
) {
    if w.snow_depth <= 0.01 {
        if !tracks.0.is_empty() {
            tracks.0.clear();
        }
        return;
    }
    let dt = time.delta_secs().min(0.25);
    // Nevando, el rastro se vuelve a tapar; con el cielo despejado se queda.
    if w.snowing {
        tracks.0.retain(|_, v| {
            *v -= TRACK_REFILL * dt;
            *v > 0.02
        });
    }
    let Ok(pt) = player.get_single() else { return };
    let here = Vec2::new(pt.translation.x, -pt.translation.y);
    // Sólo deja huella si se MOVIÓ: parado quieto no se cava un pozo.
    let moved = last.map_or(true, |l| l.distance_squared(here) > 0.04);
    *last = Some(here);
    if !moved || tracks.0.len() >= MAX_TRACKS {
        return;
    }
    let (cx, cz) = ((here.x / SNOW_CELL).floor() as i64, (here.y / SNOW_CELL).floor() as i64);
    for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        let d = ((cx + dx) as f32 * SNOW_CELL - here.x).hypot((cz + dz) as f32 * SNOW_CELL - here.y);
        if d > SNOW_CELL * 1.1 {
            continue;
        }
        let e = tracks.0.entry((cx + dx, cz + dz)).or_insert(0.0);
        *e = e.max(1.0 - d / (SNOW_CELL * 1.1));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sync_snow_blanket(
    w: Res<Weather>,
    clock: Res<crate::sky2d::DayClock>,
    tiles: Res<TileWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tracks: Res<SnowTracks>,
    zoom: Res<crate::camera::CameraZoom>,
    player: Query<&Transform, (With<Player>, Without<SnowCell>)>,
    mut cells: Query<(&mut Transform, &mut Sprite, &mut Visibility), With<SnowCell>>,
) {
    let _probe = crate::debug::probe("sync_snow_blanket");
    // Bajo tierra no nieva, y con el manto derretido no hay nada que dibujar (con
    // buen tiempo esto es un `if` y 416 sprites ocultos: cero coste).
    //
    // Y sólo A PIE (mismo corte que la lluvia): la rejilla cubre 52×32 m, así que
    // desde 900 m de altura se veía como un RECUADRO gris pegado en mitad de la
    // pantalla. El detalle del suelo es cosa del suelo.
    let depth = if tiles.z != 0 || zoom.live_view_h >= 90.0 { 0.0 } else { w.snow_depth };
    if depth <= 0.01 {
        for (_, _, mut vis) in &mut cells {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        }
        return;
    }
    let Ok(pt) = player.get_single() else { return };
    let body = crate::world2d::active_surface_body(&ctx, &planet);
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let (pcx, pcz) = ((px / SNOW_CELL).floor() as i64, (pz / SNOW_CELL).floor() as i64);
    // La nieve es blanca, pero de noche una llanura nevada no brilla igual que a
    // mediodía. Se tiñe a mano porque el manto está fuera del paso de luz.
    let lit = (0.30 + 0.70 * clock.daylight() * w.daylight_mul()).clamp(0.25, 1.0);

    for (i, (mut tf, mut sprite, mut vis)) in cells.iter_mut().enumerate() {
        let (col, row) = (i as i32 % SNOW_COLS, i as i32 / SNOW_COLS);
        let cx = pcx + (col - SNOW_COLS / 2) as i64;
        let cz = pcz + (row - SNOW_ROWS / 2) as i64;
        let (wx, wz) = (cx as f32 * SNOW_CELL, cz as f32 * SNOW_CELL);
        let (tx, tz) = (wx.floor() as i64, wz.floor() as i64);
        let tile = tiles.tile_at(body, tx, tz);
        // No cuaja sobre agua abierta ni dentro de una casa: si hay techo, no cae.
        // `is_build_floor` y no una lista a mano: es la MISMA pregunta que se
        // hace la construcción ("¿es piso construido?") y tenerla escrita dos
        // veces fue exactamente lo que dejó afuera al piso de piedra — llovía
        // adentro de una casa según con qué material la hubieras hecho.
        let indoor = ha_shared::build_grid::is_floor_piece(tile);
        let wet = matches!(
            tile,
            ha_shared::topdown::Tile::Water | ha_shared::topdown::Tile::WaterDeep
        );
        if indoor || wet {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
            continue;
        }
        // RELIEVE, que es lo que le faltaba: tres cosas a la vez.
        //  1. grano por celda (ruido) → el manto deja de ser una lámina;
        //  2. la ALTURA del terreno (`relief_level`) → cuaja más arriba, menos en la
        //     hondonada, así la nieve DIBUJA el paisaje en vez de taparlo;
        //  3. el tamaño del quad varía un pelo → el borde queda mordido, no recto.
        let grain = 0.62 + 0.38 * cell_noise(cx, cz, 0x5_0057);
        let level = ha_shared::relief::relief_level(body, tx, tz).max(0) as f32;
        let high = (level / ha_shared::relief::LEVELS_UP as f32).clamp(0.0, 1.0);
        let trampled = tracks.0.get(&(cx, cz)).copied().unwrap_or(0.0);
        let a = depth * grain * (0.75 + 0.35 * high) * (1.0 - 0.85 * trampled);
        let want = if a > 0.012 { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if a <= 0.012 {
            continue;
        }
        let r = crate::world2d::render_xy(wx, wz);
        tf.translation.x = r.x;
        tf.translation.y = r.y;
        let sz = SNOW_CELL * (0.92 + 0.16 * cell_noise(cx, cz, 0xD1F7));
        sprite.custom_size = Some(Vec2::splat(sz));
        // Nunca opaco del todo: se tiene que seguir leyendo QUÉ hay debajo (el camino,
        // el borde del acantilado). Nieve, no pintura blanca.
        sprite.color = Color::srgba(0.94 * lit, 0.96 * lit, 1.0 * lit, (a * 0.80).min(0.88));
    }
}

// ── Relámpago (pantalla) ─────────────────────────────────────────────────────

fn spawn_flash_overlay(mut commands: Commands) {
    commands.spawn((
        LightningFlash,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.85, 0.90, 1.0, 0.0)),
        // Igual que las nubes de transición: encima del mundo, DEBAJO del HUD.
        GlobalZIndex(-40),
        bevy::ui::FocusPolicy::Pass,
        crate::game_ui::GameplayUi,
        Name::new("LightningFlash"),
    ));
}

fn sync_flash(w: Res<Weather>, tiles: Res<TileWorld>, mut q: Query<&mut BackgroundColor, With<LightningFlash>>) {
    // Doble parpadeo: el fogonazo tiene un rebote a mitad de camino, que es lo
    // que separa un rayo de "alguien encendió la luz".
    // Un relámpago adentro entra apenas por las ventanas.
    let f = w.flash * if ha_shared::interiores::es_interior(tiles.z) { 0.25 } else { 1.0 };
    let a = (f * f * 0.5 + if f > 0.55 && f < 0.75 { 0.10 } else { 0.0 }).min(0.6);
    for mut bg in &mut q {
        *bg = BackgroundColor(Color::srgba(0.85, 0.90, 1.0, a));
    }
}

// ── Sonido ───────────────────────────────────────────────────────────────────

/// Loop de lluvia: ruido blanco pasado por un pasa-bajos suave + un pasa-altos
/// muy leve. Dos segundos alcanzan: el ruido no tiene tono, así que el empalme
/// no se oye.
fn rain_wav() -> Vec<u8> {
    let rate = 22_050u32;
    let n = (rate as f32 * 2.0) as usize;
    let mut rng = Lcg(0x9A3B_11C7);
    let mut buf = Vec::with_capacity(n);
    let (mut lp, mut hp) = (0.0f32, 0.0f32);
    for i in 0..n {
        let white = rng.f32() * 2.0 - 1.0;
        lp += (white - lp) * 0.42;
        hp += (lp - hp) * 0.02;
        let mut v = (lp - hp) * 0.55;
        // Gotitas sueltas encima del siseo: sin ellas suena a ventilador.
        if rng.f32() < 0.0015 {
            v += (rng.f32() * 2.0 - 1.0) * 0.5;
        }
        // Entrada/salida del loop suavizadas (medio milisegundo) por si acaso.
        let e = (i as f32 / 256.0).min(1.0).min((n - i) as f32 / 256.0);
        buf.push(v * 0.5 * e);
    }
    wav_bytes(rate, &buf)
}

/// Trueno: un crack corto y brillante encima de un retumbo largo y muy grave.
fn thunder_wav() -> Vec<u8> {
    let rate = 22_050u32;
    let n = (rate as f32 * 3.2) as usize;
    let mut rng = Lcg(0x2CE6_7B15);
    let mut buf = Vec::with_capacity(n);
    let (mut lp1, mut lp2) = (0.0f32, 0.0f32);
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let white = rng.f32() * 2.0 - 1.0;
        // Retumbo: dos pasa-bajos en serie → casi sólo graves.
        lp1 += (white - lp1) * 0.025;
        lp2 += (lp1 - lp2) * 0.05;
        // Envolvente: golpe seco y cola larga que va y viene (el eco del valle).
        let crack = (-t * 26.0).exp();
        let roll = (-t * 0.85).exp() * (0.75 + 0.25 * (t * 5.3).sin() * (t * 1.7).cos());
        let v = lp2 * 9.0 * roll + white * 0.35 * crack;
        buf.push(v.clamp(-1.0, 1.0) * 0.85);
    }
    wav_bytes(rate, &buf)
}

fn build_weather_audio(mut assets: ResMut<Assets<AudioSource>>, mut bank: ResMut<WeatherAudio>) {
    bank.rain = Some(assets.add(AudioSource { bytes: rain_wav().into() }));
    bank.thunder = Some(assets.add(AudioSource { bytes: thunder_wav().into() }));
}

/// Enciende/apaga el loop de lluvia y le sigue el volumen a la intensidad.
fn sync_weather_audio(
    mut commands: Commands,
    w: Res<Weather>,
    bank: Res<WeatherAudio>,
    settings: Res<crate::audio::AudioSettings>,
    tiles: Res<TileWorld>,
    loops: Query<(Entity, &AudioSink), With<RainLoop>>,
) {
    // Adentro, la lluvia se oye apagada: el edificio filtra el ruido de afuera.
    let adentro = if ha_shared::interiores::es_interior(tiles.z) { 0.18 } else { 1.0 };
    if crate::platform::is_app_paused() {
        return;
    }
    let want = w.rain > 0.05;
    match loops.get_single() {
        Ok((e, sink)) => {
            if !want {
                commands.entity(e).despawn();
            } else {
                sink.set_volume(settings.sfx * 0.55 * w.rain * adentro);
            }
        }
        Err(_) => {
            if want {
                if let Some(h) = bank.rain.as_ref() {
                    commands.spawn((
                        RainLoop,
                        AudioPlayer::new(h.clone()),
                        PlaybackSettings::LOOP
                            .with_volume(Volume::new(settings.sfx * 0.55 * w.rain * adentro)),
                    ));
                }
            }
        }
    }
}
