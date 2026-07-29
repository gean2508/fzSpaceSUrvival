/// Cómo abrir el juego en PC (Linux), desde la raíz del repo:
///
/// ```bash
/// cd native && HA_DAYPHASE=0 cargo run -p ha-game --features desktop
/// ```
///
/// Noche forzada: `HA_DAYPHASE=0`. Apagar luces: `HA_NO_LIGHTS=1`.
/// Alternativa con reload: `node scripts/dev-pc.mjs` (si tenés cargo-watch).
///
/// ## Color del aura = color de la luz
///
/// Core/Halo/Mist y el tinte de sprites usan **siempre** `Light2d.color`
/// (el del manifest / Ki / luciérnaga). El núcleo solo se mezcla hacia
/// **blanco neutro** (mismo matiz), nunca hacia amarillo fijo.

use bevy::asset::load_internal_asset;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::camera::ScalingMode;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    AsBindGroup, Extent3d, ShaderRef, ShaderType, TextureDimension, TextureFormat,
};
use bevy::sprite::{AlphaMode2d, Material2d, Material2dPlugin};

use crate::art2d::ArtLibrary;
use crate::camera::GameCamera;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::Player;
use crate::sky2d::DayClock;
use crate::world2d::{active_surface_body, surface_region_active, ActivePlanet, TileWorld};

const NIGHT_VEIL_SHADER_HANDLE: Handle<Shader> =
    Handle::weak_from_u128(0x4e49_4754_4856_4549_4c5f_5348_4144_4552);

/// Máximo de luces que el velo puede agujerear (uniform array fijo en WGSL).
///
/// **Es el presupuesto real de luces del juego**, no sólo del velo: el halo, el
/// tinte de sprites y el agujero del velo se calculan siempre sobre ESTE mismo
/// conjunto (las N más cercanas a la cámara). Si el halo se dibujara para más
/// luces que el velo, la luz nº 17 sería un aura flotando sobre suelo negro —
/// un hueco marcado que aparece y desaparece al hacer zoom.
pub const VEIL_LIGHTS: usize = 24;

/// Alcance del pozo de luz respecto al radio declarado del emisor. **Lo usan el
/// velo y el tinte de sprites por igual**: si no coinciden, un sprite se aclara
/// donde el suelo sigue negro y eso se lee como "hueco marcado".
const LIGHT_REACH: f32 = 2.6;

/// Dureza de la gaussiana — gemela de `FALLOFF_K` en `night_veil.wgsl`.
const FALLOFF_K: f32 = 3.2;
/// `exp(-FALLOFF_K)`, para renormalizar y que la caída llegue a 0 exacto.
const FALLOFF_EDGE: f32 = 0.040_762;

/// Caída radial compartida por velo y sprites: 1 en el foco, 0 exacto en `x = 1`,
/// sin derivada brusca en ningún punto (por eso no se ve el borde del disco).
fn falloff(x: f32) -> f32 {
    if x >= 1.0 {
        return 0.0;
    }
    ((-FALLOFF_K * x * x).exp() - FALLOFF_EDGE) / (1.0 - FALLOFF_EDGE)
}

/// Resolución de la textura de gradiente. Se estira a varios metros en pantalla,
/// así que a 96 el alpha de 8 bits dejaba escalones visibles en el móvil.
const GLOW_TEX: u32 = 160;

/// Z LOCAL de los quads de luz respecto a su emisor. El velo de noche vive a z
/// global ~140 (hijo de la cámara, ver `sky2d`) y los sprites del mundo a ~2
/// (`world2d::sort_z`) → 700 los deja siempre por encima del velo y por debajo
/// de las luciérnagas (900) y del HUD.
const GLOW_Z: f32 = 700.0;

/// Cuántas veces por segundo se recalculan alpha/parpadeo/culling. El movimiento
/// de los quads NO depende de esto (lo hace la propagación de transforms).
const LIGHT_HZ: f32 = 20.0;

/// Radio de corte de luces para una altura de viewport dada. **Depende del zoom**
/// y es el mismo para el velo, el halo y el tinte de sprites: si cada sistema
/// usara el suyo, al alejar la cámara unos empezarían a apagar luces antes que
/// otros y aparecerían charcos sin halo (o al revés). El tope evita encender
/// media Tierra en vista orbital, donde un pozo de luz mide medio píxel igual.
fn cull_radius(view_h: f32) -> f32 {
    (view_h * 1.4).min(400.0)
}

/// Prioridad para repartir el presupuesto de [`VEIL_LIGHTS`]: **menor = más
/// importante**. No alcanza con la distancia cruda — un bosque tiene decenas de
/// luciérnagas y se quedaban con todos los slots, dejando al farol de al lado
/// sin charco (un emisor brillante sobre suelo negro, el "hueco marcado"). Se
/// pondera por cuánto ilumina cada una: un farol gana a un bicho aunque esté
/// cuatro veces más lejos.
fn light_priority(dist: f32, light: &Light2d) -> f32 {
    dist / (light.radius * light.intensity).max(0.05)
}

/// Oscuridad mínima del entorno para que una luz nocturna se encienda.
const DARK_ON: f32 = 0.04;

/// Un emisor de luz. Se pone en la MISMA entidad que el sprite que ilumina; los
/// quads del halo se crean solos como hijos ([`build_glow_quads`]) si
/// [`Self::draw_glow`] es true.
#[derive(Component, Clone, Copy, Debug)]
pub struct Light2d {
    /// Tinte del halo (sRGB).
    pub color: Color,
    /// Radio del aura en metros de mundo.
    pub radius: f32,
    /// Fuerza (0..3). 1 = normal.
    pub intensity: f32,
    /// Desplazamiento del foco respecto al centro del sprite (metros, y+ arriba).
    pub offset: Vec2,
    /// 0 = luz fija; 1 = llama viva.
    pub flicker: f32,
    /// `true` = también se ve de día.
    pub day: bool,
    /// `false` = solo agujerea velo / tiñe sprites (luciérnagas ya traen su aura).
    pub draw_glow: bool,
}

impl Default for Light2d {
    fn default() -> Self {
        Self {
            color: Color::srgb(1.0, 0.82, 0.45),
            radius: 4.0,
            intensity: 1.0,
            offset: Vec2::ZERO,
            flicker: 0.0,
            day: false,
            draw_glow: true,
        }
    }
}

impl Light2d {
    /// Convierte la emisión declarada en el manifest (editor de assets) a la
    /// componente del juego.
    pub fn from_emission(em: &ha_assets::Emission) -> Self {
        let [r, g, b] = em.color;
        Self {
            color: Color::srgb(r, g, b),
            radius: em.radius.max(0.1),
            intensity: em.intensity.clamp(0.0, 3.0),
            offset: Vec2::new(em.offset[0], em.offset[1]),
            flicker: em.flicker.clamp(0.0, 1.0),
            day: em.day,
            draw_glow: true,
        }
    }
}

/// Aura de Ki del jugador: una [`Light2d`] cuya intensidad la maneja el juego
/// (sube al volar / cargar Ki) en vez del manifest.
#[derive(Component)]
pub struct KiAura;

/// Las tres capas de una luz. Apiladas dan la caída que pediste: un foco chico,
/// un halo cálido alrededor, y una **neblina** muy ancha y muy tenue que es la
/// que hace que el ENTORNO se vea iluminado hasta donde llega la luz (y no un
/// disco recortado). Ninguna estorba la vista: la más grande casi no tiene alpha.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GlowLayer {
    /// Foco: el punto caliente de la fuente.
    Core,
    /// Halo: el aura cálida inmediata.
    Halo,
    /// Neblina: derrame amplio, sutilísimo — "hasta dónde alcanza la luz".
    Mist,
}

impl GlowLayer {
    /// `(escala del radio, alpha relativo)`.
    ///
    /// **La luz ilumina, no tapa.** Los quads se dibujan ENCIMA del velo de noche
    /// (por eso aclaran el entorno), así que también pasan por encima del sprite
    /// que ilumina: con alpha alto lo lavaban de blanco y la neblina de 4.2×
    /// radio manchaba media pantalla. Ahora: esferas chicas y muy translúcidas
    /// —se nota el aire iluminado, se sigue viendo el personaje debajo—. El
    /// contraste lo pone la noche, que es casi negra (`sky2d::NIGHT_VEIL`).
    fn shape(self) -> (f32, f32) {
        // El velo agujerea el mundo; estos quads solo aportan bloom/aura del
        // emisor. Los alphas son BAJOS a propósito: son `Sprite` normales, o sea
        // blending por alpha, o sea **pintan encima**. Sobre una noche de verdad
        // (velo casi negro) un alpha alto se lee como una mancha lechosa que tapa
        // el suelo — justo lo contrario a "se ve el piso iluminado".
        match self {
            GlowLayer::Core => (0.42, 0.15),
            GlowLayer::Halo => (1.55, 0.05),
            GlowLayer::Mist => (3.10, 0.018),
        }
    }
}

#[derive(Component)]
struct GlowQuad(GlowLayer);

/// Capa `Light` de una pose (ventanas encendidas): se dibuja sobre el sprite del
/// edificio y se enciende con la noche, igual que el halo.
#[derive(Component)]
struct LitOverlay;

/// La textura de gradiente compartida por TODAS las luces (un solo draw call).
#[derive(Resource)]
pub struct GlowTexture(pub Handle<Image>);

/// Handle del material del velo (lo spawnea `sky2d`, lo actualiza `tick_lights`).
#[derive(Resource, Clone)]
pub struct NightVeilMat(pub Handle<NightVeilMaterial>);

/// Uniform del velo nocturno — layout = `night_veil.wgsl`.
#[derive(Clone, Copy, ShaderType)]
pub struct NightVeilUniform {
    pub color: Vec4,
    pub count: u32,
    pub _p0: u32,
    pub _p1: u32,
    pub _p2: u32,
    pub lights: [Vec4; VEIL_LIGHTS],
    /// `rgb` = color de la luz i — tiñe el velo que queda a su alrededor.
    pub tints: [Vec4; VEIL_LIGHTS],
}

impl Default for NightVeilUniform {
    fn default() -> Self {
        Self {
            // Lineal (target sRGB) — lo pisa `sky2d::sync_sky` cada frame.
            color: Vec4::new(0.0012, 0.0016, 0.0045, 0.0),
            count: 0,
            _p0: 0,
            _p1: 0,
            _p2: 0,
            lights: [Vec4::ZERO; VEIL_LIGHTS],
            tints: [Vec4::ZERO; VEIL_LIGHTS],
        }
    }
}

/// Material 2D del velo: α baja cerca de cada luz → se ve el piso.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct NightVeilMaterial {
    #[uniform(0)]
    pub uniform: NightVeilUniform,
}

impl Material2d for NightVeilMaterial {
    fn fragment_shader() -> ShaderRef {
        NIGHT_VEIL_SHADER_HANDLE.into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

/// `true` si el usuario apagó el sistema con `HA_NO_LIGHTS=1` (para medir costo).
fn lights_disabled() -> bool {
    std::env::var("HA_NO_LIGHTS").map(|v| v != "0").unwrap_or(false)
}

pub struct Light2dPlugin;

impl Plugin for Light2dPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            NIGHT_VEIL_SHADER_HANDLE,
            "night_veil.wgsl",
            Shader::from_wgsl
        );
        app.add_plugins(Material2dPlugin::<NightVeilMaterial>::default())
            .add_systems(PreStartup, generate_glow_texture)
            .add_systems(Update, (build_glow_quads, ki_aura, tick_lights).chain())
            // En PostUpdate para pisar a los otros sistemas que escriben
            // `sprite.color` cada frame (fauna, NPCs, máquinas) — si no, parpadea.
            .add_systems(PostUpdate, light_sprites);
    }
}

/// Gradiente radial procedural: alpha con *smootherstep* (derivada cero en los
/// dos extremos) → el borde del aura se desvanece sin banda ni corte. RGB blanco:
/// el color lo pone el tinte del sprite, así una sola textura sirve para todas.
fn generate_glow_texture(mut images: ResMut<Assets<Image>>, mut commands: Commands) {
    let n = GLOW_TEX as i32;
    let half = (n - 1) as f32 * 0.5;
    let mut data = vec![0u8; (GLOW_TEX * GLOW_TEX * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            let dx = (x as f32 - half) / half;
            let dy = (y as f32 - half) / half;
            let d = (dx * dx + dy * dy).sqrt().min(1.0);
            // Gaussiana renormalizada: misma curva que el velo, así el bloom y el
            // pozo de luz caen juntos y no se ve un anillo donde uno termina antes.
            let s = falloff(d);
            // La densidad se concentra cerca del foco y el resto es casi
            // transparente → se ve el aire iluminado sin manchar el sprite.
            let a = (s * s * (0.45 + 0.55 * (1.0 - d))).clamp(0.0, 1.0);
            // Dither ordenado (Bayer 2×2): sin él, un degradé tan largo sobre 8
            // bits de alpha deja anillos concéntricos al estirarlo en pantalla.
            let bayer = [[-0.375f32, 0.125], [0.375, -0.125]][(y & 1) as usize][(x & 1) as usize];
            let i = ((y * n + x) * 4) as usize;
            data[i] = 255;
            data[i + 1] = 255;
            data[i + 2] = 255;
            data[i + 3] = (a * 255.0 + bayer).clamp(0.0, 255.0) as u8;
        }
    }
    let mut image = Image::new(
        Extent3d { width: GLOW_TEX, height: GLOW_TEX, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // El juego usa `nearest` global (pixel art); el degradé necesita `linear` o
    // se ve escalonado al estirarlo a varios metros.
    image.sampler = ImageSampler::linear();
    commands.insert_resource(GlowTexture(images.add(image)));
}

/// Le cuelga los quads a cada emisor nuevo. Corre sólo sobre `Added<Light2d>`
/// → cero costo en régimen. Respeta `draw_glow == false` (luciérnagas).
fn build_glow_quads(
    mut commands: Commands,
    glow: Option<Res<GlowTexture>>,
    added: Query<(Entity, &Light2d), Added<Light2d>>,
) {
    let Some(glow) = glow else { return };
    if lights_disabled() {
        return;
    }
    for (entity, light) in &added {
        if !light.draw_glow {
            continue;
        }
        let tex = glow.0.clone();
        commands.entity(entity).with_children(|p| {
            // De la más ancha a la más chica: mist → halo → core.
            for (i, layer) in [GlowLayer::Mist, GlowLayer::Halo, GlowLayer::Core]
                .into_iter()
                .enumerate()
            {
                let (scale, _) = layer.shape();
                if scale <= 0.0 {
                    continue;
                }
                p.spawn((
                    GlowQuad(layer),
                    Sprite {
                        image: tex.clone(),
                        custom_size: Some(Vec2::splat(light.radius * scale)),
                        color: Color::NONE,
                        ..default()
                    },
                    Transform::from_xyz(light.offset.x, light.offset.y, GLOW_Z + i as f32 * 0.4),
                    Visibility::Hidden,
                ));
            }
        });
    }
}

/// Cuánta oscuridad hay AHORA (0 = día pleno, 1 = negro): decide si las luces
/// nocturnas se encienden y con qué fuerza. Se calcula una vez por tick.
fn darkness(
    clock: &DayClock,
    region: &RegionRuntime,
    tiles: &TileWorld,
    ctx: &WorldContextRuntime,
    planet: &ActivePlanet,
) -> f32 {
    if tiles.z < 0 {
        return 1.0; // subsuelo: siempre de noche
    }
    if surface_region_active(region).is_none() {
        return 1.0; // espacio / vacío
    }
    let body = active_surface_body(ctx, planet);
    if ha_shared::transition_profile(body).sky_fade_end <= 0.0 {
        return 1.0; // cuerpo sin atmósfera: cielo negro incluso "de día"
    }
    (1.0 - clock.daylight()).clamp(0.0, 1.0)
}

/// El corazón: enciende/apaga y modula cada luz visible, con culling por
/// viewport y tope de presupuesto.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn tick_lights(
    time: Res<Time>,
    clock: Res<DayClock>,
    region: Res<RegionRuntime>,
    tiles: Res<TileWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    camera: Query<(&Transform, &OrthographicProjection), With<GameCamera>>,
    lights: Query<(Entity, &Light2d, &GlobalTransform)>,
    children: Query<&Children>,
    mut quads: Query<(&GlowQuad, &mut Sprite, &mut Visibility), Without<LitOverlay>>,
    mut overlays: Query<(&mut Sprite, &mut Visibility), (With<LitOverlay>, Without<GlowQuad>)>,
    veil_mat: Option<Res<NightVeilMat>>,
    mut veil_assets: ResMut<Assets<NightVeilMaterial>>,
    mut acc: Local<f32>,
) {
    *acc += time.delta_secs();
    if *acc < 1.0 / LIGHT_HZ {
        return;
    }
    *acc = 0.0;
    if lights_disabled() {
        if let Some(vh) = veil_mat.as_ref() {
            if let Some(mat) = veil_assets.get_mut(&vh.0) {
                mat.uniform.count = 0;
            }
        }
        return;
    }
    let Ok((cam_tf, proj)) = camera.get_single() else {
        return;
    };
    let view_h = match proj.scaling_mode {
        ScalingMode::FixedVertical { viewport_height } => viewport_height,
        _ => 30.0,
    };
    let cam = cam_tf.translation.truncate();
    let dark = darkness(&clock, &region, &tiles, &ctx, &planet);
    let t = time.elapsed_secs();

    // 1) Candidatas dentro de cuadro (con su prioridad).
    let mut near: Vec<(f32, Entity)> = Vec::new();
    let mut off: Vec<Entity> = Vec::new();
    for (e, light, gt) in &lights {
        let on = light.day || dark > DARK_ON;
        let p = gt.translation().truncate();
        let d = p.distance(cam);
        // El corte cuenta el ALCANCE del pozo, no el radio del sprite: un farol
        // apenas fuera de cuadro sigue iluminando lo que sí se ve, y si lo
        // apagáramos su charco de luz desaparecería de golpe al paneo/zoom.
        if on && light.intensity > 0.0 && d <= cull_radius(view_h) + light.radius * LIGHT_REACH {
            near.push((light_priority(d, light), e));
        } else {
            off.push(e);
        }
    }
    // 2) Presupuesto: quedan las VEIL_LIGHTS más importantes — el MISMO conjunto
    // que agujerea el velo y que tiñe sprites. Ordenar del todo (no `select_nth`)
    // hace falta: si el subconjunto cambiara con el orden de la query, los
    // agujeros parpadearían de un tick al otro.
    near.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    if near.len() > VEIL_LIGHTS {
        for (_, e) in near.drain(VEIL_LIGHTS..) {
            off.push(e);
        }
    }
    // 3) Apagar lo que quedó fuera (barato: sólo toca las que cambian).
    for e in off {
        let Ok(kids) = children.get(e) else { continue };
        for &k in kids {
            if let Ok((_, _, mut vis)) = quads.get_mut(k) {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            } else if let Ok((_, mut vis)) = overlays.get_mut(k) {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
    // 3b) Agujeros en el velo (hasta VEIL_LIGHTS, las más cercanas).
    if let Some(vh) = veil_mat.as_ref() {
        if let Some(mat) = veil_assets.get_mut(&vh.0) {
            let mut count = 0u32;
            for &(_, e) in near.iter().take(VEIL_LIGHTS) {
                let Ok((_, light, gt)) = lights.get(e) else { continue };
                let env = if light.day { 1.0 } else { dark };
                let str = (light.intensity * env * 0.95).clamp(0.0, 1.0);
                if str <= 0.02 {
                    continue;
                }
                let p = gt.translation().truncate() + light.offset;
                let c = light.color.to_srgba();
                // Mismo alcance que el tinte de sprites (`LIGHT_REACH`): pozo ancho
                // con caída gaussiana → el suelo se aclara mucho más allá del halo
                // y la vuelta a la oscuridad no tiene borde.
                mat.uniform.lights[count as usize] =
                    Vec4::new(p.x, p.y, light.radius * LIGHT_REACH, str);
                mat.uniform.tints[count as usize] =
                    Vec4::new(c.red, c.green, c.blue, 0.0);
                count += 1;
            }
            mat.uniform.count = count;
        }
    }
    // 4) Encender y modular las que sí.
    for &(_, e) in &near {
        let Ok((_, light, _)) = lights.get(e) else { continue };
        // Parpadeo: dos senos incoherentes → llama, no metrónomo. La fase sale
        // del id de la entidad para que dos antorchas nunca latan iguales.
        let phase = (e.index() % 997) as f32 * 0.618;
        let flick = if light.flicker > 0.0 {
            let w = (t * 11.3 + phase).sin() * 0.62 + (t * 23.7 + phase * 1.7).sin() * 0.38;
            (1.0 + light.flicker * 0.3 * w).clamp(0.25, 1.4)
        } else {
            1.0
        };
        let env = if light.day { 1.0 } else { dark };
        let strength = (light.intensity * env * flick).clamp(0.0, 3.0);
        let base = light.color.to_srgba();
        let Ok(kids) = children.get(e) else { continue };
        for &k in kids {
            if let Ok((quad, mut sprite, mut vis)) = quads.get_mut(k) {
                let (_, rel) = quad.0.shape();
                // El foco tira al blanco (una llama vista de cerca quema el color);
                // halo y neblina conservan el tinte cálido del asset.
                let (r, g, b) = if quad.0 == GlowLayer::Core {
                    // Sólo un toque de blanco: si lo blanqueás de más, el foco
                    // pierde el color de la llama y decolora lo que hay debajo.
                    (
                        base.red * 0.85 + 0.15,
                        base.green * 0.85 + 0.13,
                        base.blue * 0.85 + 0.10,
                    )
                } else {
                    (base.red, base.green, base.blue)
                };
                sprite.color = Color::srgba(r, g, b, (strength * rel).clamp(0.0, 1.0));
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            } else if let Ok((mut sprite, mut vis)) = overlays.get_mut(k) {
                // La ventana encendida: se funde con la noche (de día el arte
                // apagado del propio edificio manda).
                let a = (strength * 0.9).clamp(0.0, 1.0);
                sprite.color = Color::srgba(1.0, 1.0, 1.0, a);
                if *vis != Visibility::Inherited {
                    *vis = Visibility::Inherited;
                }
            }
        }
    }
}

/// Color propio del sprite antes de que la luz lo multiplique. Se captura al
/// entrar en luz y se restaura al salir (así, si otro sistema le cambia el tinte
/// mientras está a oscuras, la próxima vez se toma el valor nuevo).
#[derive(Component)]
struct LitBase(Color);

/// Z por encima del cual un sprite NO es "mundo" (velo de noche ~140, halos 700,
/// luciérnagas 900): esos no se tiñen.
const WORLD_Z_MAX: f32 = 100.0;

/// **La luz ilumina de verdad:** multiplica el color de cada sprite cercano
/// (suelo decorado, árboles, piedras, props, fauna, NPCs, jugador) por el color
/// del foco, con caída radial suave. Con el velo agujereado, este tinte se VE
/// (antes el overlay α≈0.96 lo aplastaba). Sesgo direccional barato: un poco más
/// de aporte si el foco está "de frente" al sprite (aproximación sin normal map).
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn light_sprites(
    mut commands: Commands,
    clock: Res<DayClock>,
    region: Res<RegionRuntime>,
    tiles: Res<TileWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    camera: Query<(&Transform, &OrthographicProjection), With<GameCamera>>,
    lights: Query<(&Light2d, &GlobalTransform)>,
    mut sprites: Query<
        (Entity, &GlobalTransform, &mut Sprite, Option<&LitBase>),
        (Without<GlowQuad>, Without<LitOverlay>),
    >,
) {
    if lights_disabled() {
        return;
    }
    let Ok((cam_tf, proj)) = camera.get_single() else {
        return;
    };
    let dark = darkness(&clock, &region, &tiles, &ctx, &planet);
    let view_h = match proj.scaling_mode {
        ScalingMode::FixedVertical { viewport_height } => viewport_height,
        _ => 30.0,
    };
    let cam = cam_tf.translation.truncate();
    let cull_r = cull_radius(view_h);

    // Focos activos a la vista, con la MISMA prioridad y el MISMO corte que el
    // velo (`tick_lights`): antes se quedaba con las 16 primeras de la query, o
    // sea un subconjunto arbitrario → sprites aclarados fuera de todo pozo de luz.
    let mut active: Vec<(f32, Vec2, f32, Vec3, f32)> = Vec::new();
    for (light, gt) in &lights {
        let k = light.intensity * if light.day { 1.0 } else { dark };
        if k <= 0.02 {
            continue;
        }
        let reach = light.radius * LIGHT_REACH;
        let p = gt.translation().truncate() + light.offset;
        let d = p.distance(cam);
        if d > cull_r + reach {
            continue;
        }
        let c = light.color.to_srgba();
        active.push((light_priority(d, light), p, reach, Vec3::new(c.red, c.green, c.blue), k));
    }
    active.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    active.truncate(VEIL_LIGHTS);

    // El sprite se tiñe si está en el pozo, aunque la cámara esté lejos: cortar
    // por `cull_r` a secas dejaba props sin teñir en el borde de la pantalla al
    // alejar el zoom, con el charco del velo ya dibujado debajo.
    let sprite_reach = active.iter().map(|(_, _, r, _, _)| *r).fold(0.0f32, f32::max);

    for (e, gt, mut sprite, base) in &mut sprites {
        let p = gt.translation();
        if p.z > WORLD_Z_MAX || p.truncate().distance(cam) > cull_r + sprite_reach {
            continue;
        }
        // Aporte acumulado de todos los focos: caída smoothstep + sesgo lateral.
        let mut acc = Vec3::ZERO;
        let sp = p.truncate();
        for (_, lp, r, rgb, k) in &active {
            // Misma gaussiana que el velo y que la textura de glow: el sprite se
            // aclara exactamente donde el suelo se aclara.
            let fall = falloff(sp.distance(*lp) / *r);
            if fall <= 0.0 {
                continue;
            }
            // Sesgo: luz a la derecha aclara más el "lado" (aprox. sin normal map).
            let dir = (*lp - sp).normalize_or_zero();
            let side = 0.72 + 0.28 * (dir.x.abs() * 0.5 + dir.y.abs() * 0.5 + 0.5);
            acc += *rgb * fall * *k * 0.95 * side;
        }
        let src = base.map(|b| b.0).unwrap_or(sprite.color);
        if acc.max_element() <= 0.01 {
            if base.is_some() {
                sprite.color = src;
                commands.entity(e).remove::<LitBase>();
            }
            continue;
        }
        if base.is_none() {
            commands.entity(e).insert(LitBase(src));
        }
        // Multiplicativo y acotado: la luz suma brillo con SU color, sin quemar.
        let s = src.to_srgba();
        sprite.color = Color::srgba(
            s.red * (1.0 + acc.x).min(2.1),
            s.green * (1.0 + acc.y).min(2.1),
            s.blue * (1.0 + acc.z).min(2.1),
            s.alpha,
        );
    }
}

/// Aura de Ki del jugador: se le pone sola y respira con el vuelo. Es la prueba
/// viva de que el sistema también sirve para cosas que se mueven (y el gancho
/// para el traje EVA/RCS del futuro).
fn ki_aura(
    mut commands: Commands,
    fly: Res<crate::creative_fly::CreativeFlight>,
    player: Query<Entity, With<Player>>,
    mut aura: Query<&mut Light2d, With<KiAura>>,
) {
    if lights_disabled() {
        return;
    }
    if let Ok(mut light) = aura.get_single_mut() {
        // Volando el Ki se enciende; caminando queda un rescoldo tenue.
        let target = if fly.flying() { 1.0 } else { 0.28 };
        light.intensity += (target - light.intensity) * 0.15;
        return;
    }
    let Ok(p) = player.get_single() else { return };
    commands.entity(p).insert((
        KiAura,
        Light2d {
            color: Color::srgb(0.62, 0.86, 1.0),
            radius: 2.6,
            intensity: 0.28,
            offset: Vec2::new(0.0, 0.2),
            flicker: 0.45,
            day: false,
            draw_glow: true,
        },
    ));
}

// ── Enganche desde los spawners ──────────────────────────────────────────────

/// Le da luz a una entidad recién spawneada **si su slot la declara** en el
/// manifest. `keys` es la misma cadena de fallback que usaste para el sprite
/// (específica → genérica). Si ningún slot declara `emission`, esto es una
/// búsqueda en un `HashMap` y nada más.
///
/// ```ignore
/// let mut e = commands.spawn((sprite, Transform::from_xyz(x, y, z)));
/// light2d::attach(&mut e, &art, &["decor/torch_moon", "decor/torch"], size);
/// ```
pub fn attach(
    entity: &mut bevy::ecs::system::EntityCommands,
    art: &ArtLibrary,
    keys: &[&str],
    size: Vec2,
) {
    if let Some(em) = art.emission_first(keys) {
        entity.insert(Light2d::from_emission(em));
    }
    // Pose etiquetada `Light` (ventanas encendidas): recorte extra encima del
    // sprite, con el mismo encendido nocturno que el halo.
    let Some((key, pose, rect_px)) = keys.iter().find_map(|k| {
        art.pose_with_layer_named(k, ha_assets::ArtLayer::Light)
            .map(|(name, r)| (*k, name.to_string(), r))
    }) else {
        return;
    };
    // La pose de luz puede vivir en su propia hoja (asset de varios PNG).
    let Some(image) = art.pose_image(key, &pose) else { return };
    let [x, y, w, h] = rect_px;
    entity.with_children(|p| {
        p.spawn((
            LitOverlay,
            Sprite {
                image,
                custom_size: Some(size),
                color: Color::NONE,
                rect: Some(Rect::from_corners(
                    Vec2::new(x as f32, y as f32),
                    Vec2::new((x + w) as f32, (y + h) as f32),
                )),
                ..default()
            },
            // Por encima del velo de noche (si no, la ventana "encendida" se
            // oscurecería con todo lo demás) pero por debajo del halo.
            Transform::from_xyz(0.0, 0.0, GLOW_Z - 1.0),
            Visibility::Hidden,
        ));
    });
}
