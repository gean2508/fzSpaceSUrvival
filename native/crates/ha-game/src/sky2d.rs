//! 2D sky — day/night clear color, night overlay, starfield in vacuum.
//!
//! Replaces the Ghibli dome + fog + starfield shells: on Earth the clear
//! color runs a 720 s day cycle; airless bodies and space render near-black
//! with camera-following star sprites (never a crossable shell — biblia §1).

use bevy::prelude::*;

use crate::camera::GameCamera;
use crate::scene::{RegionId, RegionRuntime};
use crate::sim::{Player, SimState};
use crate::transition::FlightTransition;

/// Full day length in seconds (matches the old sky_clock cycle).
pub const DAY_SECS: f32 = 720.0;

/// Opacidad máxima del velo nocturno. Alta a propósito: de noche apenas se ve
/// por dónde vas, y por eso una farola o una fogata (ver `light2d`) valen algo.
///
/// La "niebla negra densa" NO se arregla bajando esto —bajarlo da atardecer
/// permanente, no noche—. Se arregla con el COLOR del velo: ver `sync_sky`.
const NIGHT_VEIL: f32 = 0.965;

/// Cuánto crepúsculo hay (0..1): pico en el amanecer (~0.20) y en el atardecer
/// (~0.80) de la fase del día. Es la curva que tiñe cielo y velo de naranja/violeta
/// — la "vibra lofi" del atardecer sale de acá.
pub fn dusk_amount(phase: f32) -> f32 {
    let bump = |c: f32, w: f32| {
        let t = ((phase - c).abs() / w).min(1.0);
        1.0 - t * t * (3.0 - 2.0 * t) // smoothstep invertido: 1 en el centro
    };
    bump(0.21, 0.09).max(bump(0.80, 0.10))
}

/// Creative ascent fade band (biblia §11): sky darkens into space.
const SKY_FADE_START_M: f32 = 150.0;
const SKY_FADE_END_M: f32 = 1200.0;

#[derive(Resource)]
pub struct DayClock {
    pub t: f32,
}

impl Default for DayClock {
    fn default() -> Self {
        // Start mid-morning. `HA_DAYPHASE=<0..1>` arranca en otra hora (0 =
        // medianoche, 0.5 = mediodía) — así se pueden probar/capturar la noche y
        // el sistema de luces (`light2d`) sin esperar 12 minutos de ciclo.
        let phase = std::env::var("HA_DAYPHASE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .map(|p| p.rem_euclid(1.0))
            .unwrap_or(0.28);
        Self { t: DAY_SECS * phase }
    }
}

impl DayClock {
    /// 0..1 phase of the day (0 = midnight).
    pub fn phase(&self) -> f32 {
        (self.t / DAY_SECS).fract()
    }

    /// Hora del reloj de juego `(hora, minuto)` — fase 0 = 00:00, 0.5 = 12:00.
    /// Lo usa el reloj del HUD; el ciclo entero dura `DAY_SECS`.
    pub fn clock_hm(&self) -> (u32, u32) {
        let mins = (self.phase() * 24.0 * 60.0) as u32 % 1440;
        (mins / 60, mins % 60)
    }

    /// Nombre ASCII del tramo del día (el HUD no dibuja acentos —
    /// memoria `game-font-ascii-only`).
    pub fn phase_name(&self) -> &'static str {
        let p = self.phase();
        match p {
            _ if p < 0.17 => "NOCHE",
            _ if p < 0.27 => "AMANECER",
            _ if p < 0.73 => "DIA",
            _ if p < 0.86 => "ATARDECER",
            _ => "NOCHE",
        }
    }

    /// 0 = full night, 1 = full day.
    pub fn daylight(&self) -> f32 {
        // Day between 0.2 and 0.8 phase with smooth dawn/dusk ramps.
        let p = self.phase();
        let up = ((p - 0.17) / 0.1).clamp(0.0, 1.0);
        let down = ((0.83 - p) / 0.1).clamp(0.0, 1.0);
        up.min(down)
    }
}

/// Kept for HUD compat: lantern state (crafted light) — dims the night overlay.
#[derive(Resource, Default)]
pub struct PlayerLanternControl {
    pub equipped: bool,
    pub lit: bool,
}

#[derive(Component)]
struct NightOverlay;

#[derive(Component)]
struct StarSprite;

/// Sitio y tamaño de una estrella en unidades de PANTALLA (1.0 = alto del
/// viewport). El cielo se re-escala al encuadre vivo, así que llena la pantalla
/// tanto a 30 m de zoom como a 100 km.
#[derive(Component)]
struct StarField {
    unit: Vec2,
    size: f32,
}

/// Camera-following cover layer for every ascent/descent (rocket AND creative)
/// — hides the ground (and the surface↔orbit swap) mid-atmosphere, clears into
/// space. Alpha + tint are driven by `FlightTransition` (clouds on bodies with
/// an atmosphere, a brief dark fade on airless ones). Drop a `decor/clouds`
/// texture later for real clouds.
#[derive(Component)]
struct CloudOverlay;

pub struct Sky2dPlugin;

impl Plugin for Sky2dPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DayClock>()
            .init_resource::<PlayerLanternControl>()
            .init_resource::<SkyAttached>()
            .add_systems(Startup, spawn_cloud_overlay)
            .add_systems(
                Update,
                (spawn_sky_attachments, advance_day_clock, sync_sky, sync_cloud_overlay),
            );
    }
}

fn advance_day_clock(time: Res<Time>, mut clock: ResMut<DayClock>) {
    clock.t += time.delta_secs();
}

/// El velo de noche y las estrellas son HIJOS de la cámara. Antes se creaban en
/// `Startup`, donde el orden contra `camera::spawn_camera` (otro `Startup`, de
/// otro plugin) NO está definido: si el cielo corría primero la query salía
/// vacía y **el velo de noche no se creaba nunca** — la noche no oscurecía y las
/// estrellas no aparecían. Ahora se intenta cada frame hasta que la cámara
/// existe, y recién ahí se marca hecho.
#[derive(Resource, Default)]
struct SkyAttached(bool);

fn spawn_sky_attachments(
    mut commands: Commands,
    mut done: ResMut<SkyAttached>,
    camera: Query<Entity, With<GameCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<crate::light2d::NightVeilMaterial>>,
) {
    if done.0 {
        return;
    }
    let Ok(cam) = camera.get_single() else {
        return;
    };
    done.0 = true;
    let veil = materials.add(crate::light2d::NightVeilMaterial {
        uniform: crate::light2d::NightVeilUniform::default(),
    });
    commands.insert_resource(crate::light2d::NightVeilMat(veil.clone()));
    commands.entity(cam).with_children(|p| {
        // Night dimmer in front of the world, behind UI. Material agujerea α
        // cerca de cada Light2d (ver light2d::tick_lights + night_veil.wgsl).
        p.spawn((
            NightOverlay,
            Mesh2d(meshes.add(Rectangle::new(4.0, 2.4))),
            MeshMaterial2d(veil),
            Transform::from_xyz(0.0, 0.0, 40.0),
        ));
        // Static star pattern that follows the camera (skybox, never crossed).
        // Denser field with varied size/brightness/tint for parallax-less depth;
        // all flat quads → cost is trivial even at a few hundred.
        let mut seed = 0x1357_9bdfu32;
        for i in 0..320 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let x = ((seed & 0xffff) as f32 / 65535.0 - 0.5) * 2.4;
            let y = (((seed >> 16) & 0xffff) as f32 / 65535.0 - 0.5) * 1.5;
            // A few bright "near" stars, most faint "far" ones.
            let bright = i % 11 == 0;
            let s = if bright { 0.007 + (seed % 4) as f32 * 0.002 } else { 0.003 + (seed % 5) as f32 * 0.0011 };
            let a = if bright { 0.95 } else { 0.45 + (seed % 6) as f32 * 0.07 };
            // Subtle warm/cool tint variation.
            let (r, g, b) = match seed % 5 {
                0 => (1.0, 0.92, 0.85),
                1 => (0.85, 0.9, 1.0),
                _ => (0.96, 0.96, 1.0),
            };
            // Posición y tamaño se guardan NORMALIZADOS (fracción de la altura
            // del viewport) y `sync_sky` los multiplica por el encuadre vivo.
            // Antes sólo se escalaba el TAMAÑO: las 320 estrellas se quedaban
            // dentro de ±1.2 m de la cámara, así que en el espacio (encuadre de
            // decenas de km) el cielo entero era UN PUNTO de 2 px en el centro
            // de la pantalla. De ahí el "no se ve nada" ahí arriba.
            p.spawn((
                StarSprite,
                StarField { unit: Vec2::new(x, y), size: s },
                Sprite::from_color(Color::srgba(r, g, b, a), Vec2::ONE),
                // DETRÁS de todo el mundo (los discos planetarios están en
                // z ≈ −20). Antes iban en +30, o sea delante del suelo: pisando
                // la Luna (cielo siempre "vacío") las estrellas se dibujaban
                // ENCIMA del regolito, y en órbita salpicaban el planeta.
                Transform::from_xyz(0.0, 0.0, -400.0),
                Visibility::Hidden,
            ));
        }
    });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sync_sky(
    clock: Res<DayClock>,
    region: Res<RegionRuntime>,
    trans: Res<FlightTransition>,
    lantern: Res<PlayerLanternControl>,
    tiles: Res<crate::world2d::TileWorld>,
    mut clear: ResMut<ClearColor>,
    player: Query<&SimState, With<Player>>,
    camera: Query<&OrthographicProjection, With<GameCamera>>,
    mut overlay: Query<&mut Transform, (With<NightOverlay>, Without<StarSprite>)>,
    veil_mat: Option<Res<crate::light2d::NightVeilMat>>,
    mut veil_assets: ResMut<Assets<crate::light2d::NightVeilMaterial>>,
    mut stars: Query<
        (&StarField, &mut Visibility, &mut Transform, &mut Sprite),
        (With<StarSprite>, Without<NightOverlay>),
    >,
) {
    let view_h = camera
        .get_single()
        .ok()
        .map(|p| match p.scaling_mode {
            bevy::render::camera::ScalingMode::FixedVertical { viewport_height } => viewport_height,
            _ => 30.0,
        })
        .unwrap_or(30.0);

    // How "spacey" is the view? 0 = surface sky by day, 1 = void. Bodies with
    // an atmosphere get their own tinted sky (from the transition profile);
    // airless surfaces are void + stars, as always.
    let mut space_f: f32;
    let mut body_sky: Option<Vec3> = None;
    let day = clock.daylight();
    match region.active {
        RegionId::Overworld { .. } | RegionId::Cave(_) => {
            space_f = 0.0;
            // Walking/flying high on Earth fades the atmosphere out (§11).
            if let Ok(sim) = player.get_single() {
                let h = sim.body.y;
                space_f = ((h - SKY_FADE_START_M) / (SKY_FADE_END_M - SKY_FADE_START_M))
                    .clamp(0.0, 1.0);
            }
            if trans.in_surface_frame() {
                space_f = space_f.max(trans.sky_blend.clamp(0.0, 1.0));
            }
        }
        RegionId::Luna | RegionId::Planet(_) => {
            let body = match region.active {
                RegionId::Planet(b) => b,
                _ => ha_shared::CelestialBody::Moon,
            };
            let p = ha_shared::transition_profile(body);
            if p.sky_fade_end > 0.0 {
                // Same on-foot fade band as Earth, scaled to this atmosphere,
                // so the sky you land under matches the descent you flew.
                let start = p.sky_fade_end * (SKY_FADE_START_M / 900.0);
                let end = p.sky_fade_end * (SKY_FADE_END_M / 900.0);
                space_f = player
                    .get_single()
                    .map(|sim| ((sim.body.y - start) / (end - start)).clamp(0.0, 1.0))
                    .unwrap_or(0.0);
                if trans.in_surface_frame() {
                    space_f = space_f.max(trans.sky_blend.clamp(0.0, 1.0));
                }
                body_sky = Some(Vec3::from_array(p.cover_color) * 0.85);
            } else {
                space_f = 1.0;
            }
        }
        _ => space_f = 1.0,
    }

    // Paleta del cielo. La Tierra corre su ciclo con TRES tramos, no dos: el
    // ATARDECER (y el amanecer) tienen su propio color cálido, que es de donde
    // sale la vibra "lofi/mágica" — pasar de día a noche interpolando un solo
    // par daba un gris sucio. `dusk_amt` es 1 en el pico del crepúsculo.
    let day_sky = Vec3::new(0.45, 0.68, 0.85);
    let dusk_sky = Vec3::new(0.86, 0.44, 0.30); // naranja/rosa de atardecer
    let night_sky = Vec3::new(0.05, 0.06, 0.15); // azul profundo, no negro
    let dusk_amt = dusk_amount(clock.phase());
    let sky = body_sky.unwrap_or_else(|| {
        let base = night_sky.lerp(day_sky, day);
        base.lerp(dusk_sky, dusk_amt * 0.85)
    });
    let void = Vec3::new(0.01, 0.01, 0.03);
    let rgb = sky.lerp(void, space_f);
    *clear = ClearColor(Color::srgb(rgb.x, rgb.y, rgb.z));

    // Underground (Z-Level < 0): near-black cave, lantern cuts the dark.
    let underground = tiles.z < 0;
    if underground {
        let cave = Vec3::new(0.03, 0.03, 0.05);
        *clear = ClearColor(Color::srgb(cave.x, cave.y, cave.z));
    }

    // Velo de noche. La noche es MUY oscura a propósito (apenas ves por dónde
    // vas → la luz vale algo y el mundo se siente mágico); el crepúsculo no
    // oscurece con gris sino con un violeta cálido que casa con el cielo
    // naranja. La linterna sigue abriendo el velo.
    let mut night_alpha = (1.0 - day) * NIGHT_VEIL * (1.0 - space_f);
    // **Casi negro, y ese es el punto.** Es color LINEAL (el shader escribe a un
    // target sRGB): 0.010 lineal ya es ~9% en pantalla, y con α 0.965 ese término
    // constante aportaba MÁS que el albedo del mundo → todo quedaba del mismo
    // gris azulado, sin tono ni contraste: la "niebla negra densa". Con el velo
    // casi negro la mezcla es ~albedo × 0.035, o sea un OSCURECIMIENTO: igual de
    // noche, pero el pasto sigue siendo verde y la piedra gris, y por eso se
    // entiende lo que mirás. Subir estos números vuelve a lavar todo.
    let mut veil = Vec3::new(0.0012, 0.0016, 0.0045);
    if dusk_amt > 0.0 {
        veil = veil.lerp(Vec3::new(0.22, 0.08, 0.18), dusk_amt); // violeta de atardecer
    }
    if underground {
        // La cueva es más cerrada que la noche, pero por el mismo mecanismo:
        // más α, no más color (si no, vuelve la niebla plana).
        night_alpha = 0.975;
        veil = Vec3::new(0.0006, 0.0006, 0.0016);
    }
    if lantern.lit {
        night_alpha *= 0.45;
    }
    if let Some(vh) = veil_mat.as_ref() {
        if let Some(mat) = veil_assets.get_mut(&vh.0) {
            mat.uniform.color = Vec4::new(veil.x, veil.y, veil.z, night_alpha);
        }
    }
    for mut tf in &mut overlay {
        // Cover the viewport at any zoom.
        tf.scale = Vec3::splat(view_h * 1.6);
    }

    // Estrellas en el vacío / de noche a gran altura. Posición Y tamaño se
    // escalan al encuadre: el campo cubre siempre la pantalla entera.
    let stars_on = space_f > 0.55 || (day < 0.25 && space_f > 0.2);
    for (field, mut vis, mut tf, mut sprite) in &mut stars {
        *vis = if stars_on {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if !stars_on {
            continue;
        }
        tf.translation.x = field.unit.x * view_h;
        tf.translation.y = field.unit.y * view_h;
        sprite.custom_size = Some(Vec2::splat((field.size * view_h).max(0.001)));
    }
}

/// Full-screen UI overlay for the ascent/descent cover. A UI Node (renders
/// reliably on top, unlike the old camera-child sprite which never drew).
fn spawn_cloud_overlay(mut commands: Commands) {
    commands.spawn((
        CloudOverlay,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.86, 0.89, 0.95, 0.0)),
        // Negative z: above the 2D world (UI pass always is) but BELOW the HUD so
        // the clouds cover the ground, never the hearts/hotbar/buttons.
        GlobalZIndex(-50),
        bevy::ui::FocusPolicy::Pass,
        Name::new("CloudOverlay"),
    ));
}

fn sync_cloud_overlay(
    trans: Res<FlightTransition>,
    mut q: Query<&mut BackgroundColor, With<CloudOverlay>>,
) {
    let [r, g, b] = trans.cover_color;
    for mut bg in &mut q {
        *bg = BackgroundColor(Color::srgba(r, g, b, trans.cover.clamp(0.0, 1.0)));
    }
}
