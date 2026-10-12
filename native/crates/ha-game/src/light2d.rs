/// Cómo abrir el juego en PC (Linux), desde la raíz del repo:
///
/// ```bash
/// cd native && HA_DAYPHASE=0 cargo run -p ha-game --features desktop
/// ```
///
/// Noche forzada: `HA_DAYPHASE=0`. Apagar luces: `HA_NO_LIGHTS=1`.
/// Alternativa con reload: `node scripts/dev-pc.mjs` (si tenés cargo-watch).
///
/// ## Cómo se ilumina el mundo (2 pasadas, `night_veil.wgsl`)
///
/// 1. **Sombra**, blending MULTIPLICATIVO: el mundo se oscurece conservando su
///    albedo y su saturación (`mundo × sombra`). Por eso la noche se ve nítida
///    y no como una sábana de color encima de todo.
/// 2. **Luz**, blending ADITIVO: cada `Light2d` SUMA fotones con su color. Dos
///    luces que se solapan dan más luz — el blend entre luces sale gratis y
///    bien— y el gradiente (cerca brilla, lejos no; la cara que da a la luz más
///    que la de atrás) se evalúa POR PÍXEL sobre la escena entera.
///
/// Las dos pasadas son quads a pantalla completa colgados de la cámara. **No
/// hay quads de halo por luz**: eran `Sprite` alpha, o sea que mezclaban hacia
/// su propio color y le comían la saturación a todo lo que tapaban — esa era la
/// "niebla blanca que deja las cosas opacas". El bulbo del emisor lo dibuja
/// ahora la pasada aditiva, que suma y por definición no puede desaturar nada.
///
/// ## Color del aura = color de la luz
///
/// El bulbo, el tinte del pozo y el tinte de sprites usan **siempre**
/// `Light2d.color` (el del manifest / Ki / luciérnaga), nunca un amarillo fijo.

use bevy::asset::load_internal_asset;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::camera::ScalingMode;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::mesh::MeshVertexBufferLayoutRef;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d,
    RenderPipelineDescriptor, ShaderRef, ShaderType, SpecializedMeshPipelineError,
    TextureDimension, TextureFormat,
};
use bevy::sprite::{AlphaMode2d, Anchor, Material2d, Material2dKey, Material2dPlugin};

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
/// **Bajado de 2.6 a 1.5 (jul 2026).** Con 2.6, la linterna (`radius 5.4`)
/// abría un pozo de **14 m de radio**: a zoom de caminata eso es la pantalla
/// entera, y una luz que ilumina toda la pantalla no es una luz, es subir el
/// brillo. Ése era el "todo lavado y descolorido" — no la desaturación del
/// shader, sino que no quedaba noche contra la cual leer el charco.
///
/// A 1.5, la misma linterna hace ~6 m de pozo: unos seis pasos alrededor tuyo.
/// Se ve dónde termina, se ve el mundo oscuro más allá, y llevar luz vuelve a
/// ser una decisión.
const LIGHT_REACH: f32 = 1.5;

/// Caída radial compartida por velo, textura de halo y tinte de sprites —
/// **gemela de `falloff()` en `night_veil.wgsl`**: si se separan, el sprite se
/// aclara donde el suelo sigue negro y eso se lee como un borde marcado.
///
/// Es el kernel poly6 `(1 - x²)³`, y lo importante es que su derivada vale **0
/// en los DOS extremos**. La gaussiana renormalizada que había antes llegaba a 0
/// en `x = 1` con pendiente −0.27: la luz se terminaba de golpe y ese escalón,
/// sobre una noche casi negra, es exactamente el "corte al ras" — un disco
/// recortado en vez de un charco que se funde con la oscuridad. Con pendiente 0
/// no hay dónde ver el final.
fn falloff(x: f32) -> f32 {
    if x >= 1.0 {
        return 0.0;
    }
    let t = 1.0 - x * x;
    t * t * t
}

/// Resolución de la textura de gradiente. Se estira a varios metros en pantalla,
/// así que a 96 el alpha de 8 bits dejaba escalones visibles en el móvil.
const GLOW_TEX: u32 = 160;

/// Ancho de la textura de sombra direccional (el alto es la mitad). Se estira a
/// varios metros, así que por debajo de esto la punta que se desintegra deja
/// escalones en vez de disolverse.
const SHADOW_TEX: u32 = 192;

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

/// Un emisor de luz. Se pone en la MISMA entidad que el sprite que ilumina; el
/// pozo, el bulbo y el tinte los dibujan las dos pasadas a pantalla completa
/// (`night_veil.wgsl`), no hay geometría por luz.
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

/// Capa `Light` de una pose (ventanas encendidas): se dibuja sobre el sprite del
/// edificio y se enciende con la noche, igual que el halo.
#[derive(Component)]
struct LitOverlay;

/// La textura de gradiente radial. La usan el sol/la luna (`sky2d`) y cualquier
/// cosa que necesite una mancha blanda; las luces ya no la usan (su bulbo lo
/// dibuja la pasada aditiva del shader).
#[derive(Resource)]
pub struct GlowTexture(pub Handle<Image>);

/// La textura de la sombra proyectada: **direccional**, no un disco. El eje X va
/// del PIE del objeto (u=0, donde la sombra toca y es más densa) a la PUNTA
/// (u=1, donde se desintegra). Ver [`generate_shadow_texture`].
#[derive(Resource)]
pub struct ShadowTexture(pub Handle<Image>);

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
    /// `rgb` = color de la luz i (LINEAL) · `w` = radio declarado del emisor, que
    /// es lo que le da tamaño al bulbo en la pasada aditiva.
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

/// Pasada 1 — **SOMBRA**, con blending MULTIPLICATIVO (`dst * src`).
///
/// El shader devuelve un MULTIPLICADOR del mundo, no un color que se pinte
/// encima. Ésa es toda la diferencia entre "de noche" y "hay una sábana gris
/// tapando el juego": multiplicar conserva el albedo y la saturación de cada
/// cosa, así que el pasto sigue verde (oscuro) y la escena se lee NÍTIDA.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct NightVeilMaterial {
    #[uniform(0)]
    pub uniform: NightVeilUniform,
}

/// Reemplaza el blend del target por uno cualquiera, después de que
/// `AlphaMode2d` haya puesto el suyo. `AlphaMode2d::Blend` se sigue declarando
/// porque es lo que mete el quad en la fase TRANSPARENTE (ordenada por z): sin
/// eso las dos pasadas podrían dibujarse en cualquier orden.
fn set_blend(descriptor: &mut RenderPipelineDescriptor, entry: &'static str, blend: BlendState) {
    let Some(fragment) = descriptor.fragment.as_mut() else { return };
    fragment.entry_point = entry.into();
    if let Some(Some(target)) = fragment.targets.first_mut() {
        target.blend = Some(blend);
    }
}

/// `final = dst * src` — oscurecer sin pintar.
const MULTIPLY_BLEND: BlendState = BlendState {
    color: BlendComponent {
        src_factor: BlendFactor::Dst,
        dst_factor: BlendFactor::Zero,
        operation: BlendOperation::Add,
    },
    alpha: BlendComponent {
        src_factor: BlendFactor::Zero,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    },
};

/// `final = dst + src` — sumar fotones. Es el blend "correcto" entre luces: dos
/// focos que se solapan dan más luz y ninguno puede tapar al otro.
const ADDITIVE_BLEND: BlendState = BlendState {
    color: BlendComponent {
        src_factor: BlendFactor::One,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    },
    alpha: BlendComponent {
        src_factor: BlendFactor::Zero,
        dst_factor: BlendFactor::One,
        operation: BlendOperation::Add,
    },
};

impl Material2d for NightVeilMaterial {
    fn fragment_shader() -> ShaderRef {
        NIGHT_VEIL_SHADER_HANDLE.into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        set_blend(descriptor, "fragment", MULTIPLY_BLEND);
        Ok(())
    }
}

/// Pasada 2 — **LUZ**, con blending ADITIVO. Mismo uniform que la sombra (lo
/// escribe el mismo [`tick_lights`]), otro entry point y otro blend.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub struct LightAddMaterial {
    #[uniform(0)]
    pub uniform: NightVeilUniform,
}

impl Material2d for LightAddMaterial {
    fn fragment_shader() -> ShaderRef {
        NIGHT_VEIL_SHADER_HANDLE.into()
    }
    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        set_blend(descriptor, "fragment_add", ADDITIVE_BLEND);
        Ok(())
    }
}

/// Handle del material de la pasada aditiva (lo spawnea `sky2d`).
#[derive(Resource, Clone)]
pub struct LightAddMat(pub Handle<LightAddMaterial>);

/// Marca los dos quads a pantalla completa para poder APAGARLOS cuando no hacen
/// nada. A pleno día la sombra multiplica por 1 y la luz suma 0: son dos lecturas
/// y dos escrituras de la pantalla entera para dejarla igual. En el Redmi, a
/// 1080p, eso son ~4 M de píxeles por frame tirados — y el día es justo cuando
/// más cosas hay en cuadro.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum LightPass {
    /// Multiplicativa (oscurece).
    Shade,
    /// Aditiva (ilumina).
    Add,
}


/// `true` con `HA_SHADOW_DEBUG=1`: pinta cada sombra de magenta opaco para poder
/// medir su forma y su rumbo en una captura.
fn shadow_debug() -> bool {
    std::env::var("HA_SHADOW_DEBUG").map(|v| v != "0").unwrap_or(false)
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
            .add_plugins(Material2dPlugin::<LightAddMaterial>::default())
            .add_systems(PreStartup, (generate_glow_texture, generate_shadow_texture))
            // Sólo en partida: no hay pozos de luz, ni sombras, ni velo de noche
            // que calcular detrás del splash o del menú.
            .add_systems(
                Update,
                (attach_ground_shadows, ki_aura, tick_lights, cast_shadows, toggle_light_passes)
                    .chain()
                    .run_if(crate::game_phase::world_active),
            )
            // En PostUpdate para pisar a los otros sistemas que escriben
            // `sprite.color` cada frame (fauna, NPCs, máquinas) — si no, parpadea.
            // La pasada cara va a 20 Hz; la re-aplicación del tinte, a 60 — si no,
            // los sprites que se repintan solos (fauna, NPCs, máquinas) asomarían
            // sin teñir en los frames intermedios. Ver `reapply_lit_sprites`.
            .add_systems(
                PostUpdate,
                (light_sprites, reapply_lit_sprites)
                    .chain()
                    .run_if(crate::game_phase::world_active),
            );
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
            // Misma curva que el velo, así el bloom y el pozo de luz caen juntos y
            // no se ve un anillo donde uno termina antes.
            let s = falloff(d);
            // **Sin elevar al cuadrado.** Con `s * s` la densidad se apelotonaba
            // en el foco y el resto del quad quedaba casi vacío: perfecto para un
            // núcleo, inútil para la neblina, que es justo la capa que tiene que
            // llenar el charco entero de color. Lineal + un realce hacia el centro
            // da un derrame ancho que sigue sin manchar el sprite de abajo.
            let a = (s * (0.55 + 0.45 * (1.0 - d))).clamp(0.0, 1.0);
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

/// La textura de la SOMBRA PROYECTADA. No es un disco: es direccional, y por eso
/// hace falta aparte del degradé radial.
///
///   * **X = a lo largo de la sombra.** `u = 0` es el PIE del objeto —donde la
///     sombra nace, densa y con el borde más definido— y `u = 1` la PUNTA, que
///     se **desintegra**: la densidad se apaga con `(1-u)²` y encima se le suma
///     un ruido que va comiendo el borde de a poco. En la vida real la punta de
///     una sombra no termina, se disuelve en la penumbra, y ese difuminado entre
///     luz y sombra es la mitad de que la escena se lea con aire.
///   * **Y = a lo ancho.** Un perfil `(1-v²)²` con derivada 0 en los bordes: los
///     costados también se funden, así que la sombra no tiene contorno.
///
/// El quad se ancla en `CenterLeft` (ver [`attach_ground_shadows`]) para poder
/// girarlo alrededor del pie sin recalcular nada.
fn generate_shadow_texture(mut images: ResMut<Assets<Image>>, mut commands: Commands) {
    let (w, h) = (SHADOW_TEX, SHADOW_TEX / 2);
    let mut data = vec![0u8; (w * h * 4) as usize];
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let u = x as f32 / (w - 1) as f32; // 0 = pie, 1 = punta
            let v = (y as f32 / (h - 1) as f32) * 2.0 - 1.0; // -1..1 a lo ancho
            // A lo ancho la sombra NO se afina con la distancia: se ABLANDA. Una
            // sombra que se estrecha hasta un punto se lee como una raya (era el
            // aspecto viejo); la de verdad conserva el ancho del objeto y lo que
            // crece es la penumbra, el difuminado del borde. El exponente sube con
            // `u`, así que el perfil pasa de casi lleno en el contacto a una
            // campana blanda en la punta.
            //
            // 30 jul 2026 (sol invisible): el borde arranca MÁS DURO (0.8 + 2.0·u
            // en vez de 1.0 + 2.4·u). Con el disco del sol fuera de pantalla, la
            // sombra es lo único que dice de dónde viene la luz, así que tiene que
            // leerse nítida en el contacto; el ablandado sigue creciendo hacia la
            // punta, que es donde la penumbra corresponde de verdad.
            let vv = v.abs().min(1.0);
            let soft = 0.8 + 2.0 * u;
            let cross = (1.0 - vv * vv).max(0.0).powf(soft);
            // Largo: densa en el contacto, muerta en la punta. 1.6 → 1.3: aguanta
            // el cuerpo de la sombra medio metro más antes de disolverse, que es la
            // diferencia entre "hay una sombra" y "hay una mancha".
            let along = (1.0 - u).powf(1.3);
            // La punta se DESINTEGRA: un ruido barato le come mordiscos justo
            // donde ya está tenue, así el final no es un degradé perfecto (que el
            // ojo sigue leyendo como un borde) sino algo que se deshace. Hash y no
            // `(x*7+y*13)%17`: ese patrón es periódico y al estirarlo deja rayas
            // diagonales en vez de grano.
            let mut hsh = (x as u32).wrapping_mul(0x9e37_79b9) ^ (y as u32).wrapping_mul(0x85eb_ca6b);
            hsh ^= hsh >> 13;
            hsh = hsh.wrapping_mul(0xc2b2_ae35);
            let n = (hsh >> 8 & 0xffff) as f32 / 65535.0;
            let erode = (1.0 - u * u * 0.8 * n).clamp(0.0, 1.0);
            let a = (cross * along * erode).clamp(0.0, 1.0);
            let bayer = [[-0.375f32, 0.125], [0.375, -0.125]][(y & 1) as usize][(x & 1) as usize];
            let i = ((y * w as i32 + x) * 4) as usize;
            data[i] = 255;
            data[i + 1] = 255;
            data[i + 2] = 255;
            data[i + 3] = (a * 255.0 + bayer).clamp(0.0, 255.0) as u8;
        }
    }
    let mut image = Image::new(
        Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    commands.insert_resource(ShadowTexture(images.add(image)));
}

/// Cuánta oscuridad hay AHORA (0 = día pleno, 1 = negro): decide si las luces
/// nocturnas se encienden y con qué fuerza. Se calcula una vez por tick.
fn darkness(
    clock: &DayClock,
    weather: &crate::weather::Weather,
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
    // El clima entra por el MISMO número que el ciclo del día: un cielo tapado es
    // menos día, así que las antorchas se encienden a media tarde de tormenta sin
    // que haga falta ninguna regla aparte (ver `weather::Weather::daylight_mul`).
    (1.0 - clock.daylight() * weather.daylight_mul()).clamp(0.0, 1.0)
}

/// El corazón: enciende/apaga y modula cada luz visible, con culling por
/// viewport y tope de presupuesto.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn tick_lights(
    time: Res<Time>,
    clock: Res<DayClock>,
    weather: Res<crate::weather::Weather>,
    region: Res<RegionRuntime>,
    tiles: Res<TileWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    camera: Query<(&Transform, &OrthographicProjection), With<GameCamera>>,
    lights: Query<(Entity, &Light2d, &GlobalTransform)>,
    children: Query<&Children>,
    mut overlays: Query<(&mut Sprite, &mut Visibility), With<LitOverlay>>,
    veil_mat: Option<Res<NightVeilMat>>,
    mut veil_assets: ResMut<Assets<NightVeilMaterial>>,
    add_mat: Option<Res<LightAddMat>>,
    mut add_assets: ResMut<Assets<LightAddMaterial>>,
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
        if let Some(ah) = add_mat.as_ref() {
            if let Some(mat) = add_assets.get_mut(&ah.0) {
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
    let dark = darkness(&clock, &weather, &region, &tiles, &ctx, &planet);
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
            if let Ok((_, mut vis)) = overlays.get_mut(k) {
                if *vis != Visibility::Hidden {
                    *vis = Visibility::Hidden;
                }
            }
        }
    }
    // 4) El uniform de las DOS pasadas (sombra y luz): la misma lista de luces,
    // que es lo que garantiza que el pozo y el bulbo caigan exactamente juntos.
    //
    // El PARPADEO entra acá y no sólo en el emisor: antes la llama latía en el
    // quad del halo pero el pozo del suelo era fijo, así que el fuego titilaba
    // "en el aire" mientras la luz que tiraba era un foco de estudio. Ahora
    // respira el charco entero.
    let mut uniform_lights = [Vec4::ZERO; VEIL_LIGHTS];
    let mut uniform_tints = [Vec4::ZERO; VEIL_LIGHTS];
    let mut count = 0u32;
    for &(_, e) in near.iter().take(VEIL_LIGHTS) {
        let Ok((_, light, gt)) = lights.get(e) else { continue };
        let env = if light.day { 1.0 } else { dark };
        let strength = (light.intensity * env * flicker_of(e, light, t)).clamp(0.0, 1.6);
        if strength <= 0.02 {
            continue;
        }
        let p = gt.translation().truncate() + light.offset;
        // **LINEAL, no sRGB.** El shader escribe a un target sRGB, o sea que
        // trabaja en lineal, y acá se mandaban las componentes sRGB tal cual. Una
        // llama (1.0, 0.72, 0.34) leída como lineal es un amarillo pálido y
        // desaturado: el charco salía color crema. La misma llama en lineal es
        // (1.0, 0.48, 0.09) — el naranja hondo que uno espera de un fuego.
        let c = light.color.to_linear();
        uniform_lights[count as usize] =
            Vec4::new(p.x, p.y, light.radius * LIGHT_REACH, strength);
        // `w` = radio DECLARADO del emisor: es el tamaño del bulbo (el punto
        // caliente) en la pasada aditiva. Una vela tiene uno chiquito y una
        // hoguera uno gordo, sin ninguna constante mágica en el shader. En 0 no
        // hay bulbo: es el viejo `draw_glow == false` de las luciérnagas, que ya
        // traen su propio sprite de aura y con otro encima se queman a blanco.
        let bulb_r = if light.draw_glow { light.radius } else { 0.0 };
        uniform_tints[count as usize] = Vec4::new(c.red, c.green, c.blue, bulb_r);
        count += 1;
    }
    if let Some(vh) = veil_mat.as_ref() {
        if let Some(mat) = veil_assets.get_mut(&vh.0) {
            mat.uniform.lights = uniform_lights;
            mat.uniform.tints = uniform_tints;
            mat.uniform.count = count;
        }
    }

    if let Some(ah) = add_mat.as_ref() {
        if let Some(mat) = add_assets.get_mut(&ah.0) {
            // El color/α de la noche lo escribe `sky2d::sync_sky` en el material
            // del velo; la pasada aditiva lo necesita igual (para su término
            // ambiente), así que se copia de ahí en vez de duplicar la lógica.
            if let Some(vh) = veil_mat.as_ref() {
                if let Some(v) = veil_assets.get(&vh.0) {
                    mat.uniform.color = v.uniform.color;
                }
            }
            mat.uniform.lights = uniform_lights;
            mat.uniform.tints = uniform_tints;
            mat.uniform.count = count;
        }
    }
    // 5) Las ventanas encendidas de las poses `Light`.
    for &(_, e) in &near {
        let Ok((_, light, _)) = lights.get(e) else { continue };
        let env = if light.day { 1.0 } else { dark };
        let strength = (light.intensity * env * flicker_of(e, light, t)).clamp(0.0, 3.0);
        let Ok(kids) = children.get(e) else { continue };
        for &k in kids {
            let Ok((mut sprite, mut vis)) = overlays.get_mut(k) else { continue };
            // La ventana encendida: se funde con la noche (de día el arte apagado
            // del propio edificio manda).
            let a = (strength * 0.9).clamp(0.0, 1.0);
            sprite.color = Color::srgba(1.0, 1.0, 1.0, a);
            if *vis != Visibility::Inherited {
                *vis = Visibility::Inherited;
            }
        }
    }
}

/// Apaga las dos pasadas cuando no aportan nada (ver [`LightPass`]): a pleno día
/// y sin ninguna luz encendida, la sombra multiplica por 1 y la luz suma 0.
///
/// Va aparte de [`tick_lights`] y no dentro por un motivo tonto pero real: ese
/// sistema ya tiene 16 parámetros, que es el máximo que Bevy sabe convertir en
/// sistema, y el 17.º hace fallar el `chain()` con un error de trait ilegible.
fn toggle_light_passes(
    veil_mat: Option<Res<NightVeilMat>>,
    veil_assets: Res<Assets<NightVeilMaterial>>,
    mut passes: Query<(&LightPass, &mut Visibility)>,
) {
    let Some(uniform) = veil_mat.as_ref().and_then(|vh| veil_assets.get(&vh.0)) else { return };
    let useful = uniform.uniform.color.w > 0.002 || uniform.uniform.count > 0;
    let want = if useful { Visibility::Inherited } else { Visibility::Hidden };
    for (_, mut vis) in &mut passes {
        if *vis != want {
            *vis = want;
        }
    }
}

/// Parpadeo de llama: dos senos incoherentes → fuego, no metrónomo. La fase sale
/// del id de la entidad para que dos antorchas nunca latan iguales.
fn flicker_of(e: Entity, light: &Light2d, t: f32) -> f32 {
    if light.flicker <= 0.0 {
        return 1.0;
    }
    let phase = (e.index() % 997) as f32 * 0.618;
    let w = (t * 11.3 + phase).sin() * 0.62 + (t * 23.7 + phase * 1.7).sin() * 0.38;
    (1.0 + light.flicker * 0.3 * w).clamp(0.25, 1.4)
}

// ── Sombras de contacto ──────────────────────────────────────────────────────

/// Ya se le miró si merece sombra (tenga o no) — evita re-escanear cada frame.
#[derive(Component)]
struct ShadowChecked;

/// La sombra de un objeto del mundo. Guarda su tamaño de reposo y la altura del
/// pie porque [`cast_shadows`] la ESTIRA y la GIRA según de dónde venga la luz:
/// sin el original, cada tick partiría del valor deformado del tick anterior y la
/// sombra se iría escapando sola.
#[derive(Component)]
pub struct GroundShadow {
    /// `x` = largo en reposo (sin luz direccional), `y` = ANCHO — y el ancho es
    /// el del objeto, no un valor fijo: la sombra de un roble es gorda y la de un
    /// poste es finita.
    base: Vec2,
    foot: f32,
}

/// Debajo de este alto (m) un sprite no proyecta sombra: florcitas, piedritas y
/// briznas de pasto la tendrían más grande que ellas mismas.
const SHADOW_MIN_H: f32 = 0.7;

/// Un foco a menos de esto (m) del objeto se considera SUYO: no lo proyecta.
const SELF_LIGHT_M: f32 = 0.6;

/// Le cuelga una sombra a cada objeto del mundo.
///
/// No hay sombras proyectadas de verdad (haría falta geometría de oclusores en el
/// shader, y esto tiene que correr en el Redmi): es un quad direccional anclado
/// al pie del objeto que [`cast_shadows`] gira y estira. Pero la señal que el ojo
/// lee —de dónde viene la luz, cuánto se alarga, cómo se deshace la punta— es la
/// correcta, y sin ella un árbol o un NPC FLOTAN sobre el pasto.
///
/// Corre sobre todo sprite del mundo sin padre: los overlays, las partículas
/// (viven en z ≥ 880) y lo que ya ES luz quedan fuera solos.
fn attach_ground_shadows(
    mut commands: Commands,
    shadow_tex: Option<Res<ShadowTexture>>,
    candidates: Query<
        (Entity, &Sprite, &GlobalTransform),
        (
            Without<ShadowChecked>,
            Without<LitOverlay>,
            Without<GroundShadow>,
            Without<crate::sim::UnlitSprite>,
            Without<Parent>,
        ),
    >,
) {
    let Some(tex) = shadow_tex else { return };
    for (entity, sprite, gt) in &candidates {
        commands.entity(entity).insert(ShadowChecked);
        let Some(size) = sprite.custom_size else { continue };
        if size.y < SHADOW_MIN_H || gt.translation().z > WORLD_Z_MAX {
            continue;
        }
        // **El ancho es el del objeto.** Antes la mancha medía `ancho × 0.44` a lo
        // ancho: o sea que la sombra de cualquier cosa era una tirita fina, igual
        // para un roble que para un cactus, y se leía como un valor por defecto y
        // no como la sombra DE ESO. Ahora el eje transversal es el ancho real del
        // sprite (un pelo menos, que la penumbra de la textura ya se derrama).
        let width = size.x * 0.92;
        // El largo en reposo es cortito: sin luz direccional lo que corresponde es
        // oclusión de contacto, la sombrita pegada bajo los pies. La estira la luz.
        let length = size.x * 0.62;
        // `UnlitSprite` para que `light_sprites` no la aclare: una sombra que se
        // ilumina deja de ser sombra. El tono no es negro puro sino el verde
        // azulado de la sombra ambiental (`sky2d`), para que de día y de noche
        // pertenezca a la misma paleta.
        let foot = -size.y * 0.5 + width * 0.10;
        commands.entity(entity).with_children(|p| {
            p.spawn((
                GroundShadow { base: Vec2::new(length, width), foot },
                crate::sim::UnlitSprite,
                Sprite {
                    image: tex.0.clone(),
                    custom_size: Some(Vec2::new(length, width)),
                    color: Color::srgba(0.02, 0.06, 0.055, 0.7),
                    // **Anclada al PIE.** La textura va del pie (u=0) a la punta
                    // (u=1), así que anclando en el borde izquierdo el quad gira
                    // alrededor del objeto: la sombra nace SIEMPRE donde el objeto
                    // toca el suelo y sólo se alarga hacia el lado contrario a la
                    // luz. Con el ancla al centro (lo de antes) había que correr la
                    // traslación a mano y la mancha se despegaba de los pies.
                    anchor: Anchor::CenterLeft,
                    ..default()
                },
                // Un pelo por detrás, para que el propio sprite la pise en vez de
                // quedar la mancha encima de los zapatos.
                Transform::from_xyz(0.0, foot, -0.02),
            ));
        });
    }
}

/// **La sombra apunta al revés que la luz.** Cada sombra se gira y se estira
/// según de dónde venga la luz: el sol/la luna (`SkyLight`, que recorre su arco a
/// lo largo del día) más los focos cercanos, sumados como vectores. Con dos
/// antorchas a los lados la sombra se parte en la resultante, que es lo que el
/// ojo espera; con el sol rasante del amanecer se alarga y al mediodía se recoge
/// bajo los pies.
///
/// **Una luz no proyecta su propia sombra.** El emisor está EN el objeto, así que
/// la sombra caería justo debajo de la lámpara, donde no puede haber ninguna: una
/// antorcha que se dibuja su propio manchón es el error que más delata el truco.
/// Se saltean la luz que vive en la misma entidad y las que están prácticamente
/// encima (a menos de [`SELF_LIGHT_M`]), donde además la dirección es puro ruido.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn cast_shadows(
    time: Res<Time>,
    sky: Res<crate::sky2d::SkyLight>,
    weather: Res<crate::weather::Weather>,
    clock: Res<DayClock>,
    region: Res<RegionRuntime>,
    tiles: Res<TileWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    // `Without<GroundShadow>`: la query de sombras pide `&mut Transform`, así que
    // Bevy exige que ésta sea DEMOSTRABLEMENTE disjunta o aborta con B0001 al
    // primer frame de partida (se probó: panic al entrar a `Playing`).
    camera: Query<(&Transform, &OrthographicProjection), (With<GameCamera>, Without<GroundShadow>)>,
    lights: Query<(Entity, &Light2d, &GlobalTransform)>,
    parents: Query<&GlobalTransform, Without<GroundShadow>>,
    mut shadows: Query<(&GroundShadow, &Parent, &mut Transform, &mut Sprite)>,
    mut acc: Local<f32>,
    span: Res<crate::debug::CpuSpanStats>,
) {
    let _guard = crate::debug::span_guard(&span, crate::debug::CpuSpanSlot::Lights);
    *acc += time.delta_secs();
    if *acc < 1.0 / LIGHT_HZ {
        return;
    }
    *acc = 0.0;
    if lights_disabled() {
        return;
    }
    let Ok((cam_tf, proj)) = camera.get_single() else { return };
    let view_h = match proj.scaling_mode {
        ScalingMode::FixedVertical { viewport_height } => viewport_height,
        _ => 30.0,
    };
    let cam = cam_tf.translation.truncate();
    let cull_r = cull_radius(view_h);
    let dark = darkness(&clock, &weather, &region, &tiles, &ctx, &planet);

    // Focos activos — mismo conjunto y mismo criterio que las dos pasadas.
    let mut active: Vec<(f32, Entity, Vec2, f32, f32)> = Vec::new();
    for (e, light, gt) in &lights {
        let k = light.intensity * if light.day { 1.0 } else { dark };
        if k <= 0.05 {
            continue;
        }
        let reach = light.radius * LIGHT_REACH;
        let p = gt.translation().truncate() + light.offset;
        if p.distance(cam) > cull_r + reach {
            continue;
        }
        active.push((light_priority(p.distance(cam), light), e, p, reach, k));
    }
    active.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    active.truncate(VEIL_LIGHTS);

    // Término del cielo: es el MISMO para todo el mundo (el sol está infinitamente
    // lejos), así que se calcula una vez. Rasante = sombra larga.
    //
    // **El LARGO lo manda la elevación, no la fuerza.** Antes los dos salían de
    // `strength`, y como al amanecer el sol tira poco (0.32) las sombras del
    // amanecer —las que tendrían que ser larguísimas— medían menos que las del
    // mediodía. Ahora la fuerza sólo decide si HAY sombra (`sky_on` satura
    // enseguida) y el largo sale del ángulo: rasante = larga, cenital = un
    // asiento bajo los pies.
    let sky_w = sky.strength;
    let sky_on = (sky_w * 2.5).min(1.0);
    let sky_v = -sky.dir * sky_on * (0.30 + 2.40 * (1.0 - sky.elevation));

    // **La sombra es del color complementario a la luz que la hace**, y ahora eso
    // importa el doble: el disco del sol no se dibuja (ver `sky2d::SkyDisc`), así
    // que su COLOR sólo puede decirse por acá. Con el sol rasante y naranja del
    // amanecer la sombra se va al azul frío —contraste complementario, el mismo
    // principio que el velo de `sky2d`, y lo que hace que una hora dorada se lea
    // dorada—; al mediodía, con luz blanca, se queda en el verde azulado neutro de
    // la sombra ambiental. La luna (azul) no entra: `x - z` le da negativo.
    let warm = (sky.color.x - sky.color.z).clamp(0.0, 1.0) * sky_on;
    let tint = Vec3::new(0.02, 0.06, 0.055).lerp(Vec3::new(0.030, 0.045, 0.105), warm);

    for (sh, parent, mut tf, mut sprite) in &mut shadows {
        let owner = parent.get();
        let Ok(gt) = parents.get(owner) else { continue };
        let p = gt.translation().truncate();
        if p.distance(cam) > cull_r {
            continue;
        }
        let mut v = sky_v;
        let mut w = sky_w;
        for (_, le, lp, reach, k) in &active {
            // La luz que ESTE objeto lleva puesta no lo proyecta (ni la de un foco
            // prácticamente encima): estaríamos dibujando la sombra de la lámpara
            // debajo de la lámpara.
            let d = p.distance(*lp);
            if *le == owner || d < SELF_LIGHT_M {
                continue;
            }
            let fall = falloff(d / *reach);
            if fall <= 0.0 {
                continue;
            }
            let s = fall * *k;
            let dir = (*lp - p).normalize_or_zero();
            // Cuanto más cerca del foco, más larga y más marcada la sombra: es lo
            // que hace que pasar al lado de una fogata se sienta.
            v -= dir * s * (0.5 + 0.9 * fall);
            w += s;
        }
        let len = v.length().min(2.4);
        let dir = v.normalize_or_zero();
        // El quad está anclado al pie, así que la traslación no se mueve: la sombra
        // nace siempre donde el objeto toca el suelo. Lo único que cambia es hacia
        // dónde apunta y cuánto mide.
        tf.translation.x = 0.0;
        tf.translation.y = sh.foot;
        tf.rotation = if len > 0.06 {
            Quat::from_rotation_z(dir.y.atan2(dir.x))
        } else {
            // Sin ninguna luz direccional la sombra se queda bajo los pies,
            // apuntando "hacia abajo" en pantalla: eso es oclusión de contacto y
            // siempre corresponde.
            Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)
        };
        // Se alarga a lo largo del rumbo y se ADELGAZA muy poco: el ancho tiene que
        // seguir siendo el del objeto (una sombra que se afina hasta la nada se lee
        // como una raya, no como la sombra de algo).
        sprite.custom_size = Some(Vec2::new(
            sh.base.x * (1.0 + len * 1.35),
            sh.base.y * (1.0 - 0.10 * len).max(0.80),
        ));
        // Con el cielo tapado la sombra se DILUYE: un día nublado es luz difusa y
        // no proyecta nada, sólo deja el asiento del objeto en el suelo. Y cuanto
        // más se estira, más se abre la penumbra y menos densa queda — así el
        // difuminado entre luz y sombra crece con el largo, como en la vida real.
        //
        // Subido junto con el perfil de la textura al sacar el disco del sol de
        // pantalla: la sombra pasó a ser LA señal de la luz direccional (no un
        // adorno al lado del astro), así que se pinta más marcada y se diluye
        // menos al estirarse (0.16 → 0.11 por metro).
        let spread = 1.0 - 0.11 * len;
        let a = (0.46 + 0.50 * w.min(1.2)) * (1.0 - weather.cover * 0.45) * spread;
        sprite.color = if shadow_debug() {
            // `HA_SHADOW_DEBUG=1`: magenta opaco. Es la única forma de VER dónde
            // cae una sombra, hacia dónde apunta y cuánto mide — a su alpha real
            // se confunde con el arte oscuro de la base de cada sprite, y uno
            // termina tuneando por corazonada algo que quizá ni se está pintando.
            Color::srgba(1.0, 0.0, 0.8, 1.0)
        } else {
            Color::srgba(tint.x, tint.y, tint.z, a.clamp(0.0, 0.92))
        };
    }
}

/// Color propio del sprite antes de que la luz lo multiplique. Se captura al
/// entrar en luz y se restaura al salir (así, si otro sistema le cambia el tinte
/// mientras está a oscuras, la próxima vez se toma el valor nuevo).
///
/// **`lit` en vez de quitar el componente.** Antes esto se insertaba al entrar en
/// un pozo de luz y se QUITABA al salir. Insertar o quitar un componente es un
/// cambio ESTRUCTURAL: mueve la entidad de arquetipo e invalida las cachés de
/// query. Volando, cientos de sprites cruzan ese umbral por frame, así que el
/// coste no era parejo — era un tirón cada vez que el paisaje entraba o salía de
/// la luz. Ahora el componente se pone una vez y no se saca nunca; apagarse es
/// escribir un `bool`.
#[derive(Component)]
struct LitBase {
    color: Color,
    /// Multiplicador de luz vigente. Se RECALCULA a 20 Hz (caro: un barrido por
    /// foco activo) pero se APLICA todos los frames (baratísimo: una
    /// multiplicación). Ver `reapply_lit_sprites`.
    mul: Vec3,
    lit: bool,
}

/// Color final de un sprite teñido. Acotado a 1.35 porque la pasada aditiva del
/// shader ya suma su parte encima: si los dos empujan fuerte, al lado de una
/// antorcha los NPC se van a blanco amarillento y pierden la cara.
fn lit_color(base: Color, mul: Vec3) -> Color {
    let s = base.to_srgba();
    Color::srgba(
        s.red * (1.0 + mul.x).min(1.35),
        s.green * (1.0 + mul.y).min(1.35),
        s.blue * (1.0 + mul.z).min(1.35),
        s.alpha,
    )
}

/// **La mitad barata de la iluminación de sprites, y corre a 60 Hz.**
///
/// `light_sprites` recalcula el aporte de cada foco sólo 20 veces por segundo,
/// pero el tinte tiene que volver a escribirse en CADA frame: fauna, NPCs y
/// máquinas repintan su `sprite.color` en `Update` todos los frames, y por eso
/// la pasada de luz vive en `PostUpdate` — para pisarlos. Sin esta re-aplicación,
/// bajar la frecuencia de la pasada cara haría que en dos de cada tres frames
/// esos sprites se vieran SIN teñir: exactamente el parpadeo que el comentario
/// del plugin advierte.
///
/// Es barato de verdad porque la query pide `&LitBase`, y ese componente sólo lo
/// tienen los sprites que alguna vez estuvieron en un pozo de luz — no los miles
/// de decoración del mundo.
fn reapply_lit_sprites(mut sprites: Query<(&LitBase, &mut Sprite)>) {
    if lights_disabled() {
        return;
    }
    for (base, mut sprite) in &mut sprites {
        if base.lit {
            sprite.color = lit_color(base.color, base.mul);
        }
    }
}

/// Z por encima del cual un sprite NO es "mundo" (velo de noche ~140, halos 700,
/// luciérnagas 900): esos no se tiñen.
const WORLD_Z_MAX: f32 = 100.0;

/// **El objeto también se ilumina, no sólo el aire.** Multiplica el color de cada
/// sprite cercano (decoración, árboles, piedras, props, fauna, NPCs, jugador) por
/// el color del foco. Es un término chico —el grueso de la luz lo pinta la pasada
/// aditiva del shader, por píxel— pero es lo que hace que el objeto se sienta
/// hecho de materia que reacciona a la luz y no una calcomanía con un foco
/// encima.
///
/// **Se mide en el borde MÁS CERCANO del sprite, no en su centro.** Ésa es la
/// diferencia entre "todo lo que cae en el charco se ve igual" y un gradiente: un
/// árbol de cuatro metros con la fogata a su izquierda tiene su costado a un
/// metro de la llama y el otro a cinco, y con la distancia al centro los dos
/// medían lo mismo. Ahora el que le da la cara a la luz recibe de verdad más.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn light_sprites(
    mut commands: Commands,
    clock: Res<DayClock>,
    weather: Res<crate::weather::Weather>,
    region: Res<RegionRuntime>,
    tiles: Res<TileWorld>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    camera: Query<(&Transform, &OrthographicProjection), With<GameCamera>>,
    lights: Query<(&Light2d, &GlobalTransform)>,
    // `UnlitSprite` = lo que ya ES luz (aura de Ki, estela) más la sombra de
    // vuelo. Sin excluirlos, este sistema les captura el color en `LitBase` y a
    // partir de ahí multiplica el color CAPTURADO en vez del actual: como
    // `creative_fly` los repinta cada frame en `Update`, el aura terminaba
    // latiendo con un frame viejo — o sea titilando.
    mut sprites: Query<
        (Entity, &GlobalTransform, &mut Sprite, Option<&mut LitBase>),
        (Without<LitOverlay>, Without<crate::sim::UnlitSprite>),
    >,
    mut acc: Local<f32>,
    time: Res<Time>,
    span: Res<crate::debug::CpuSpanStats>,
) {
    let _guard = crate::debug::span_guard(&span, crate::debug::CpuSpanSlot::Lights);
    if lights_disabled() {
        return;
    }
    // **A 20 Hz, como las sombras.** Este sistema recorre TODOS los sprites del
    // mundo y por cada uno mide contra cada foco activo; corría a 60 Hz sin
    // motivo. El tinte de un sprite por una fogata no cambia lo bastante rápido
    // como para que se note el paso — es el mismo criterio (y el mismo `LIGHT_HZ`)
    // que ya usaban `tick_lights` y `cast_shadows`.
    *acc += time.delta_secs();
    if *acc < 1.0 / LIGHT_HZ {
        return;
    }
    *acc = 0.0;
    let Ok((cam_tf, proj)) = camera.get_single() else {
        return;
    };
    let dark = darkness(&clock, &weather, &region, &tiles, &ctx, &planet);
    let view_h = match proj.scaling_mode {
        ScalingMode::FixedVertical { viewport_height } => viewport_height,
        _ => 30.0,
    };
    let cam = cam_tf.translation.truncate();
    let cull_r = cull_radius(view_h);

    // Focos activos a la vista, con la MISMA prioridad y el MISMO corte que las
    // dos pasadas (`tick_lights`): antes se quedaba con las 16 primeras de la
    // query, o sea un subconjunto arbitrario → sprites aclarados fuera de todo
    // pozo de luz.
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
    // alejar el zoom, con el charco ya dibujado debajo.
    let sprite_reach = active.iter().map(|(_, _, r, _, _)| *r).fold(0.0f32, f32::max);

    for (e, gt, mut sprite, base) in &mut sprites {
        let p = gt.translation();
        if p.z > WORLD_Z_MAX || p.truncate().distance(cam) > cull_r + sprite_reach {
            continue;
        }
        let sp = p.truncate();
        let half = sprite.custom_size.unwrap_or(Vec2::splat(1.0)) * 0.5;
        let mut acc = Vec3::ZERO;
        for (_, lp, r, rgb, k) in &active {
            // Punto del sprite MÁS CERCANO al foco (clamp del foco a la caja) y su
            // opuesto: entre los dos sale el gradiente cerca/lejos del objeto.
            let near = lp.clamp(sp - half, sp + half);
            let far = sp * 2.0 - near;
            let f_near = falloff(near.distance(*lp) / *r);
            let f_far = falloff(far.distance(*lp) / *r);
            if f_near <= 0.0 {
                continue;
            }
            // Pesa la cara iluminada: 70% el lado que da a la luz, 30% el de
            // atrás. Un sprite plano no puede tener medio tinte, pero sí puede
            // reaccionar a CUÁL de sus lados mira a la lámpara.
            let fall = f_far * 0.3 + f_near * 0.7;
            // Un sprite cenital no tiene normal, pero sí tiene ARRIBA: una luz por
            // encima le da en la cara, una por debajo lo deja a contraluz.
            let dir = (*lp - sp).normalize_or_zero();
            let side = 0.62 + 0.38 * (0.5 + 0.5 * dir.y);
            acc += *rgb * fall * *k * 0.30 * side;
        }
        // Estando apagado, el color VIVO del sprite es el suyo (otro sistema pudo
        // haberlo repintado mientras estaba a oscuras); estando encendido, el
        // color vivo ya lleva la luz multiplicada, así que el bueno es el
        // guardado. Ése era el sentido original de poner y quitar el componente,
        // y se conserva entero con el `bool`.
        // Estando apagado, el color VIVO del sprite es el suyo (otro sistema pudo
        // haberlo repintado mientras estaba a oscuras); estando encendido, el
        // color vivo ya lleva la luz multiplicada, así que el bueno es el
        // guardado. Ése era el sentido original de poner y quitar el componente,
        // y se conserva entero con el `bool`.
        let src = match base.as_deref() {
            Some(b) if b.lit => b.color,
            _ => sprite.color,
        };
        if acc.max_element() <= 0.01 {
            if let Some(mut b) = base {
                if b.lit {
                    sprite.color = src;
                    b.lit = false;
                    b.mul = Vec3::ZERO;
                }
            }
            continue;
        }
        match base {
            Some(mut b) => {
                b.color = src;
                b.mul = acc;
                b.lit = true;
            }
            None => {
                commands.entity(e).insert(LitBase { color: src, mul: acc, lit: true });
            }
        }
        sprite.color = lit_color(src, acc);
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
        let target = if fly.flying() { 1.0 } else { 0.18 };
        light.intensity += (target - light.intensity) * 0.15;
        return;
    }
    let Ok(p) = player.get_single() else { return };
    commands.entity(p).insert((
        KiAura,
        Light2d {
            // **Cian de verdad, no celeste pálido.** El aura del jugador es la
            // única luz que está SIEMPRE en cuadro y siempre en el centro: con un
            // color casi blanco (0.62, 0.86, 1.0) su neblina —que mide 5.6 radios,
            // o sea media pantalla— bañaba la escena entera de blanco azulado.
            // Ése era el grueso de la "niebla blanca que tapa los colores".
            color: Color::srgb(0.30, 0.72, 1.0),
            radius: 2.2,
            intensity: 0.18,
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
