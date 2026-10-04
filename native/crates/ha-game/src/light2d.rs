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
///
/// **Subido de 40 a 64 (3 oct 2026)** con la grilla de baldosas ([`TILES_X`]): el costo ya
/// no es píxeles × luces sino píxeles × luces que se solapan, así que el tope de uniform
/// dejó de ser el límite de rendimiento.
///
/// **Subido de 24 a 40 (ago 2026), y sin pagarlo.** El disparador fue medir el
/// manifest: de 14.440 sprites sólo CUATRO declaraban emisión, así que el pueblo
/// de noche estaba negro por falta de emisores, no por falta de presupuesto. Con
/// los 76 emisores que mide `scripts/import-emisiones.py` —faroles, velas,
/// fogatas, linternas de camping, letreros, reflectores— una plaza satura 24
/// enseguida, y lo que se veía era el charco del farol 25 apareciendo y
/// desapareciendo al caminar.
///
/// Lo que hace que 40 cuesten lo que costaban 24 es el bucle del shader: ahora
/// rechaza por distancia AL CUADRADO contra `1/alcance²` precalculado acá abajo
/// (ver `uniform_lights`), o sea sin raíz ni división. El rechazo pasó de ~11
/// operaciones a ~6, y el rechazo es lo que se paga 40 veces por píxel.
pub const VEIL_LIGHTS: usize = 64;

/// **Cuántas luces se evalúan de verdad.** El velo y la pasada aditiva recorren TODAS las
/// luces por cada píxel de la pantalla, así que el costo es píxeles × luces: medido en el
/// Redmi (30 sep 2026), una ciudad de noche con las 40 luces llenas = ~60 ms de GPU por
/// cuadro (`resto` en `ha-stages`), contra 2 ms sin luces. En el celu se usan menos (las
/// más importantes, ver `light_priority`); `HA_LUCES=n` lo fuerza para medir.
///
/// **16 en el celu desde el 3 oct 2026**, por la grilla de baldosas ([`TILES_X`]):
/// el shader ya no paga cada luz en cada píxel sino sólo donde su pozo llega, así
/// que duplicar el tope cuesta lo que cuestan los charcos que se SOLAPAN, no 8
/// pasadas más por la pantalla entera.
pub fn presupuesto_de_luces() -> usize {
    static N: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *N.get_or_init(|| {
        let por_defecto = if cfg!(target_os = "android") { 16 } else { VEIL_LIGHTS };
        crate::flags::var("HA_LUCES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(por_defecto)
            .clamp(1, VEIL_LIGHTS)
    })
}

/// **Grilla de baldosas de luz** (3 oct 2026). Las dos pasadas a pantalla completa
/// recorrían TODAS las luces en CADA píxel: el costo era píxeles × luces aunque el
/// 90 % de las luces no llegara a ese píxel (medido en el Redmi, ~4 ms de GPU por
/// luz — por eso el tope de 8 en Android). Rechazar una luz es barato en ALU, pero
/// en una Mali cada vuelta del bucle es además una lectura del uniform con índice
/// dinámico, y eso es lo que se pagaba.
///
/// Ahora la CPU parte el encuadre en `TILES_X × TILES_Y` baldosas y anota, por
/// baldosa, una máscara de bits con las luces que la TOCAN (`tiles_lo` = luces
/// 0..31, `tiles_hi` = 32..63). El shader lee UNA máscara y recorre sólo esos bits:
/// el costo pasa a ser píxeles × (luces que se solapan ahí), que en una calle con
/// faroles es 0, 1 o 2 — no 8 ni 40.
///
/// **Se ve EXACTAMENTE igual.** Una luz fuera de la máscara es una luz cuyo pozo no
/// llega a ningún punto de la baldosa, o sea una que el bucle viejo descartaba con
/// `continue` sin tocar nada; los bits se recorren de menor a mayor, el mismo orden
/// del bucle viejo, así que hasta el redondeo del producto `cover` es el mismo. Un
/// píxel que cae fuera de la grilla (la cámara se movió entre dos ticks de 20 Hz)
/// usa la máscara completa: más lento, nunca distinto.
pub const TILES_X: usize = 20;
pub const TILES_Y: usize = 10;
/// Baldosas empaquetadas de a cuatro por `UVec4` (el uniform alinea a 16 bytes).
pub const TILE_VECS: usize = TILES_X * TILES_Y / 4;
/// Margen de la grilla alrededor del encuadre (fracción por lado): cubre lo que la
/// cámara recorre entre dos ticks sin caer en la máscara completa.
const GRID_MARGIN: f32 = 0.12;

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

/// Tamaño del bulbo como fracción del radio declarado — **gemela de `BULB_R` en
/// `night_veil.wgsl`**. Vive de los dos lados porque el shader recibe
/// `1/(radio·BULB_R)` ya calculado (para no dividir por píxel): si los dos
/// números se separan, el punto caliente deja de caer donde cae el pozo.
const BULB_R: f32 = 0.16;

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
pub(crate) const GLOW_Z: f32 = 700.0;

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
    /// `xy` = viewport en PÍXELES del target de la cámara de mundo. Lo necesita
    /// la viñeta, que trabaja en coordenadas de pantalla y no de mundo (una
    /// viñeta en metros se estiraría con el zoom). `zw` libres.
    pub view: Vec4,
    pub count: u32,
    pub _p0: u32,
    pub _p1: u32,
    pub _p2: u32,
    pub lights: [Vec4; VEIL_LIGHTS],
    /// `rgb` = color de la luz i (LINEAL) · `w` = radio declarado del emisor, que
    /// es lo que le da tamaño al bulbo en la pasada aditiva.
    pub tints: [Vec4; VEIL_LIGHTS],
    /// `rgb` = el color de la luz i ya SATURADO (`tint^1,6`, ver `fragment_add`).
    /// Es una constante por luz: calcularla acá ahorra tres `pow` por luz y por
    /// píxel en la pasada aditiva. `w` libre.
    pub cols: [Vec4; VEIL_LIGHTS],
    /// Grilla de baldosas (ver [`TILES_X`]): `xy` = esquina inferior izquierda en
    /// metros, `zw` = 1/lado de baldosa. `zw = 0` = sin grilla (máscara completa).
    pub grid: Vec4,
    /// Máscara de luces 0..31 por baldosa (baldosa `k` = `tiles_lo[k/4][k%4]`).
    pub tiles_lo: [UVec4; TILE_VECS],
    /// Máscara de luces 32..63 por baldosa.
    pub tiles_hi: [UVec4; TILE_VECS],
}

impl Default for NightVeilUniform {
    fn default() -> Self {
        Self {
            // Lineal (target sRGB) — lo pisa `sky2d::sync_sky` cada frame.
            color: Vec4::new(0.0012, 0.0016, 0.0045, 0.0),
            view: Vec4::ZERO,
            count: 0,
            _p0: 0,
            _p1: 0,
            _p2: 0,
            lights: [Vec4::ZERO; VEIL_LIGHTS],
            tints: [Vec4::ZERO; VEIL_LIGHTS],
            cols: [Vec4::ZERO; VEIL_LIGHTS],
            grid: Vec4::ZERO,
            tiles_lo: [UVec4::ZERO; TILE_VECS],
            tiles_hi: [UVec4::ZERO; TILE_VECS],
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
    crate::flags::var("HA_SHADOW_DEBUG").map(|v| v != "0").unwrap_or(false)
}

/// `true` si el usuario apagó el sistema con `HA_NO_LIGHTS=1` (para medir costo).
fn lights_disabled() -> bool {
    crate::flags::var("HA_NO_LIGHTS").map(|v| v != "0").unwrap_or(false)
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

/// **Todo lo que decide cuánta luz hay**, en un solo parámetro.
///
/// Eran seis `Res` sueltos repetidos en los tres sistemas de luz, y con el
/// encierro fueron siete: Bevy topa en dieciséis parámetros por sistema y
/// `tick_lights` ya estaba al borde. Juntarlos también deja la respuesta en un
/// sitio — `ambiente.darkness()` — en vez de tres llamadas idénticas de siete
/// argumentos que había que mantener sincronizadas a mano.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Ambiente<'w> {
    clock: Res<'w, DayClock>,
    weather: Res<'w, crate::weather::Weather>,
    region: Res<'w, RegionRuntime>,
    tiles: Res<'w, TileWorld>,
    ctx: Res<'w, WorldContextRuntime>,
    planet: Res<'w, ActivePlanet>,
    interior: Res<'w, crate::interior::Interior>,
}

impl Ambiente<'_> {
    /// Cuánta oscuridad hay AHORA (0 = día pleno, 1 = negro): decide si las luces
    /// nocturnas se encienden y con qué fuerza. Se calcula una vez por tick.
    ///
    /// Público porque los fuegos artificiales (`render::fireworks`) preguntan lo
    /// mismo —¿ya es de noche?— y duplicar los siete `Res` de [`Ambiente`] para
    /// responderlo era exactamente el problema que este parámetro vino a
    /// resolver.
    pub fn darkness(&self) -> f32 {
        darkness(
            &self.clock,
            &self.weather,
            &self.region,
            &self.tiles,
            &self.ctx,
            &self.planet,
            &self.interior,
        )
    }
}

fn darkness(
    clock: &DayClock,
    weather: &crate::weather::Weather,
    region: &RegionRuntime,
    tiles: &TileWorld,
    ctx: &WorldContextRuntime,
    planet: &ActivePlanet,
    interior: &crate::interior::Interior,
) -> f32 {
    // **Bajo tierra ya NO se apaga el mundo entero.** Antes bajar ponía la
    // pantalla en noche cerrada, porque bajar significaba estar en otra escena.
    // Ahora tu túnel es un agujero en el mismo mapa: el sol le sigue pegando al
    // pasto de al lado, y lo que está oscuro es el pozo, que se dibuja oscuro
    // (`world2d::pit_quads`). Apagar el día acá volvía a inventar la cueva.
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
    let dia = (1.0 - clock.daylight() * weather.daylight_mul()).clamp(0.0, 1.0);
    // **Y el ENCIERRO, que es la otra forma de quedarse sin cielo.**
    //
    // No es "estás bajo tierra" —eso era el interruptor viejo, que apagaba el
    // mundo entero al bajar un escalón—, es cuánto te alejaste de la abertura por
    // la que entra la luz (`interior.rs`). En la boca de la cueva vale 0 y no
    // oscurece nada; veinte pasos adentro vale 1 y la antorcha pasa a ser lo que
    // decide si ves. Entre medio hay un gradiente, y ese gradiente ES la
    // transición: no hay otra.
    dia.max(interior.encierro)
}

/// El corazón: enciende/apaga y modula cada luz visible, con culling por
/// viewport y tope de presupuesto.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn tick_lights(
    time: Res<Time>,
    ambiente: Ambiente,
    camera: Query<(&Transform, &OrthographicProjection, &Camera), With<GameCamera>>,
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
    let Ok((cam_tf, proj, cam)) = camera.get_single() else {
        return;
    };
    // El viewport del TARGET, no el de la ventana: con escala de render la
    // cámara de mundo dibuja contra una imagen más chica y la viñeta tiene que
    // cerrarse sobre ESA (ver `render::scale`). `physical_target_size` ya
    // devuelve la del target activo, sea imagen o ventana.
    let view_px = cam
        .physical_target_size()
        .map(|s| Vec2::new(s.x as f32, s.y as f32))
        .unwrap_or(Vec2::ZERO);
    let view_h = match proj.scaling_mode {
        ScalingMode::FixedVertical { viewport_height } => viewport_height,
        _ => 30.0,
    };
    let cam = cam_tf.translation.truncate();
    let dark = ambiente.darkness();
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
    if near.len() > presupuesto_de_luces() {
        for (_, e) in near.drain(presupuesto_de_luces()..) {
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
    let mut uniform_cols = [Vec4::ZERO; VEIL_LIGHTS];
    let mut count = 0u32;
    for &(_, e) in near.iter().take(presupuesto_de_luces()) {
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
        // **`z` = 1/alcance², no el alcance.** El shader rechaza por distancia al
        // cuadrado y así se ahorra una raíz Y una división por luz y por píxel —
        // que es exactamente lo que financia el presupuesto de [`VEIL_LIGHTS`].
        // Precalcularlo acá cuesta una división por luz y por tick (20 Hz), o sea
        // nada, contra millones por frame allá.
        let reach = (light.radius * LIGHT_REACH).max(0.05);
        uniform_lights[count as usize] =
            Vec4::new(p.x, p.y, 1.0 / (reach * reach), strength);
        // `w` = radio DECLARADO del emisor: es el tamaño del bulbo (el punto
        // caliente) en la pasada aditiva. Una vela tiene uno chiquito y una
        // hoguera uno gordo, sin ninguna constante mágica en el shader. En 0 no
        // hay bulbo: es el viejo `draw_glow == false` de las luciérnagas, que ya
        // traen su propio sprite de aura y con otro encima se queman a blanco.
        // …y `w` = **1/(radio·BULB_R)**, por el mismo motivo: el bulbo y el halo
        // se evalúan con una multiplicación en vez de una división. En 0 no hay
        // bulbo NI halo: es el viejo `draw_glow == false`.
        let bulb_w = if light.draw_glow && light.radius > 0.01 {
            1.0 / (light.radius * BULB_R)
        } else {
            0.0
        };
        uniform_tints[count as usize] = Vec4::new(c.red, c.green, c.blue, bulb_w);
        uniform_cols[count as usize] = Vec4::new(
            c.red.max(0.0).powf(1.6),
            c.green.max(0.0).powf(1.6),
            c.blue.max(0.0).powf(1.6),
            0.0,
        );
        count += 1;
    }
    let view_u = view_px.extend(0.0).extend(0.0);
    // La grilla cubre el encuadre (más un margen) y se ancla a múltiplos de la
    // baldosa: caminando despacio el uniform no cambia de un tick al otro y Bevy
    // no tiene nada que volver a subir.
    let (grid, tiles_lo, tiles_hi) = if view_px.x > 0.0 && view_px.y > 0.0 {
        let alto = view_h * proj.scale.max(1e-3);
        let ext = Vec2::new(alto * view_px.x / view_px.y, alto) * (1.0 + 2.0 * GRID_MARGIN);
        // Una baldosa de holgura por eje: el `floor` del ancla corre la grilla
        // hasta una baldosa, y sin ella el borde de la pantalla quedaría afuera.
        let tile = ext / Vec2::new(TILES_X as f32 - 1.0, TILES_Y as f32 - 1.0);
        let origin = ((cam - ext * 0.5) / tile).floor() * tile;
        let (lo, hi) = light_tiles(&uniform_lights[..count as usize], origin, tile);
        (Vec4::new(origin.x, origin.y, 1.0 / tile.x, 1.0 / tile.y), lo, hi)
    } else {
        (Vec4::ZERO, [UVec4::ZERO; TILE_VECS], [UVec4::ZERO; TILE_VECS])
    };
    if let Some(vh) = veil_mat.as_ref() {
        let igual = veil_assets.get(&vh.0).is_some_and(|m| {
            m.uniform.count == count
                && m.uniform.view == view_u
                && m.uniform.lights == uniform_lights
                && m.uniform.tints == uniform_tints
                && m.uniform.grid == grid
                && m.uniform.tiles_lo == tiles_lo
                && m.uniform.tiles_hi == tiles_hi
        });
        if !igual {
            if let Some(mat) = veil_assets.get_mut(&vh.0) {
                mat.uniform.lights = uniform_lights;
                mat.uniform.tints = uniform_tints;
                mat.uniform.cols = uniform_cols;
                mat.uniform.grid = grid;
                mat.uniform.tiles_lo = tiles_lo;
                mat.uniform.tiles_hi = tiles_hi;
                mat.uniform.count = count;
                mat.uniform.view = view_u;
            }
        }
    }

    if let Some(ah) = add_mat.as_ref() {
        // El color/α de la noche lo escribe `sky2d::sync_sky` en el material del velo; la
        // pasada aditiva lo necesita igual (para su término ambiente), así que se copia de
        // ahí en vez de duplicar la lógica. Y sólo se toca si algo cambió (ver `sync_sky`).
        let color_velo = veil_mat
            .as_ref()
            .and_then(|vh| veil_assets.get(&vh.0))
            .map(|v| v.uniform.color);
        let igual = add_assets.get(&ah.0).is_some_and(|m| {
            m.uniform.count == count
                && m.uniform.view == view_u
                && color_velo.map_or(true, |c| m.uniform.color == c)
                && m.uniform.lights == uniform_lights
                && m.uniform.tints == uniform_tints
                && m.uniform.grid == grid
                && m.uniform.tiles_lo == tiles_lo
                && m.uniform.tiles_hi == tiles_hi
        });
        if !igual {
            if let Some(mat) = add_assets.get_mut(&ah.0) {
                if let Some(c) = color_velo {
                    mat.uniform.color = c;
                }
                mat.uniform.lights = uniform_lights;
                mat.uniform.tints = uniform_tints;
                mat.uniform.cols = uniform_cols;
                mat.uniform.grid = grid;
                mat.uniform.tiles_lo = tiles_lo;
                mat.uniform.tiles_hi = tiles_hi;
                mat.uniform.count = count;
                mat.uniform.view = view_u;
            }
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

/// Máscaras de luces por baldosa (ver [`TILES_X`]). `lights` es lo mismo que va al
/// uniform: `xy` = centro, `z` = 1/alcance². El bit `i` de una baldosa se prende si
/// el pozo de la luz `i` llega a ALGÚN punto de la baldosa (distancia del centro al
/// rectángulo, no al centro de la baldosa).
///
/// Es conservador a propósito: el rectángulo se agranda un 2 % de baldosa y el
/// alcance un 1 %, así un píxel justo en el borde entre dos baldosas —donde el
/// `floor` del shader puede caer de cualquier lado por redondeo— nunca pierde una
/// luz que el bucle completo le habría sumado.
fn light_tiles(lights: &[Vec4], origin: Vec2, tile: Vec2) -> ([UVec4; TILE_VECS], [UVec4; TILE_VECS]) {
    let mut lo = [UVec4::ZERO; TILE_VECS];
    let mut hi = [UVec4::ZERO; TILE_VECS];
    let pad = tile * 0.02;
    for (i, l) in lights.iter().enumerate().take(VEIL_LIGHTS) {
        if l.z <= 0.0 {
            continue;
        }
        let reach = (1.0 / l.z).sqrt() * 1.01;
        let c = l.truncate().truncate();
        // Sólo las baldosas bajo la caja del pozo.
        let a = ((c - reach - origin) / tile).floor();
        let b = ((c + reach - origin) / tile).floor();
        if b.x < 0.0 || b.y < 0.0 || a.x >= TILES_X as f32 || a.y >= TILES_Y as f32 {
            continue;
        }
        let (x0, x1) = (a.x.max(0.0) as usize, (b.x as usize).min(TILES_X - 1));
        let (y0, y1) = (a.y.max(0.0) as usize, (b.y as usize).min(TILES_Y - 1));
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let min = origin + tile * Vec2::new(tx as f32, ty as f32) - pad;
                let max = min + tile + pad * 2.0;
                if c.clamp(min, max).distance_squared(c) > reach * reach {
                    continue;
                }
                let k = ty * TILES_X + tx;
                if i < 32 {
                    lo[k / 4][k % 4] |= 1 << i;
                } else {
                    hi[k / 4][k % 4] |= 1 << (i - 32);
                }
            }
        }
    }
    (lo, hi)
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
            if crate::flags::var("HA_PERFLOG").is_ok() {
                eprintln!(
                    "ha-veil: pasadas de luz {:?} -> {:?} (alfa={:.3} luces={})",
                    *vis, want, uniform.uniform.color.w, uniform.uniform.count
                );
            }
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

/// **Este sprite no proyecta sombra, y no hace falta ni mirarlo.**
///
/// [`attach_ground_shadows`] decide por CAPA: lo que vive por encima de
/// `layers::YSORT_MIN` se levanta del suelo y proyecta. La cascada rompe esa
/// regla y por eso necesita decirlo a mano — Y-sortea (el que camina por debajo
/// de la poza tiene que pasar por delante del chorro) pero **es agua**, y el agua
/// no lleva sombra: no está SOBRE el líquido, es el líquido cayendo.
///
/// Sin esto, en la captura del driver (`HA_AGUATEST`) se veía la mancha
/// rectangular de las tres piezas del salto tumbada sobre la poza. Es la misma
/// regla que ya cumplen la onda, la hondura y el nenúfar por la vía de la capa
/// (`agua::TEXTURA_DE_AGUA`); ésta es la puerta para quien no puede bajar de capa.
pub(crate) fn sin_sombra(ent: &mut bevy::ecs::system::EntityCommands) {
    ent.insert(ShadowChecked);
}

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
    /// **La sombra tiene la FORMA del objeto** (29 sep 2026). Es el mismo dibujo
    /// del objeto, espejado, en negro translúcido y aplastado contra el suelo: la
    /// sombra de un árbol es copa y tronco, no una franja con el borde recto
    /// ("la sombra es una puta línea recta"). Sólo para lo que no se anima (el
    /// recorte de un sprite animado cambia cada cuadro y la copia quedaría
    /// congelada); lo animado sigue con la mancha de contacto.
    silueta: bool,
    /// El objeto que proyecta. La silueta cuelga de un soporte intermedio (ver
    /// [`ESCORZO_DEL_SUELO`]), así que el dueño ya no es su padre directo.
    owner: Entity,
}

/// **El suelo se ve escorzado.** La cámara es 3/4: un metro sobre el suelo hacia
/// el fondo ocupa menos pantalla que un metro hacia los lados. Una sombra es una
/// figura tendida SOBRE EL SUELO, así que hay que girarla en el plano del suelo y
/// recién después aplastar ese plano en `y`. Un sprite no puede hacerlo solo (la
/// escala se aplica antes de la rotación), por eso la silueta cuelga de un soporte
/// con esta escala y es la silueta la que gira adentro. Girada a pelo era un
/// árbol acostado con el grosor de un árbol de pie: "un árbol oscuro pegado al
/// lado del árbol", no la sombra de nada.
const ESCORZO_DEL_SUELO: f32 = 0.55;

/// Cuánto del alto del objeto (0..1) se mete la sombra bajo su propio pie, para
/// que arranque DEBAJO del tronco y no al lado, que es lo que la despega.
const SOLAPE_DEL_PIE: f32 = 0.07;

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
        (Entity, &Sprite, &GlobalTransform, Has<crate::art2d::ArtAnimated>),
        (
            Without<ShadowChecked>,
            Without<LitOverlay>,
            Without<GroundShadow>,
            Without<crate::sim::UnlitSprite>,
            Without<Parent>,
            // **El arte de tile no proyecta** (29 sep 2026): el acantilado, la
            // orilla y la cascada son TERRENO, y el terreno ya tiene su sombra de
            // terraza horneada en el suelo. Con la de sprite encima, cada columna
            // de acantilado tiraba una franja/cuadro oscuro sobre el pasto y el
            // agua — las bandas diagonales y los cuadrados de las capturas.
            Without<crate::world2d::TileArt>,
        ),
    >,
) {
    let Some(tex) = shadow_tex else { return };
    for (entity, sprite, gt, animado) in &candidates {
        crate::render::try_insert(&mut commands, entity, "sombra/marca", ShadowChecked);
        let Some(size) = sprite.custom_size else { continue };
        let z = gt.translation().z;
        if size.y < SHADOW_MIN_H || z > WORLD_Z_MAX {
            continue;
        }
        // **Un piso no proyecta sombra.** El alto del sprite no alcanza para
        // saber si algo se levanta del suelo: la línea divisoria de la calzada
        // mide un tile de alto igual que un cajón, y le salía su sombrita sobre
        // el asfalto — con el asfalto de al lado, que es el mismo suelo, sin
        // ninguna. Quien lo sabe es la CAPA: debajo de `layers::YSORT_MIN` vive
        // el arte plano (agua, pisos, caminos, pintura de calzada, decals) y
        // encima lo que se para. Ver `layers`.
        if z < crate::layers::YSORT_MIN {
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
        //
        // **Y SALE DEL LADO MÁS GRANDE, no siempre del ancho.** Para un decorado
        // más o menos cuadrado (un roble, una roca) `size.x` y `size.y` son
        // parecidos y da lo mismo. Para un acantilado no: es una columna de UNA
        // celda de ancho que puede medir DIEZ de alto (`pared_apilada`), y con
        // `size.x` la sombra salía del porte de una piedra suelta sin importar si
        // el muro medía uno o nueve pisos — que es por lo que un farallón entero
        // no se leía distinto de un guijarro. `size.x.max(size.y)` no le cambia
        // nada a la mayoría del decorado (el máximo sigue siendo el ancho) y le
        // da su tamaño real a lo que es angosto y alto.
        let length = size.x.max(size.y) * 0.62;
        // `UnlitSprite` para que `light_sprites` no la aclare: una sombra que se
        // ilumina deja de ser sombra. El tono no es negro puro sino el verde
        // azulado de la sombra ambiental (`sky2d`), para que de día y de noche
        // pertenezca a la misma paleta.
        let foot = -size.y * 0.5 + width * 0.10;
        // La silueta: lo estático con su propio dibujo (ver `GroundShadow::silueta`).
        if sprite.texture_atlas.is_none() && !animado {
            let rig = commands
                .spawn((
                    Transform::from_xyz(0.0, -size.y * 0.5 + size.y * SOLAPE_DEL_PIE, -crate::layers::HAIR)
                        .with_scale(Vec3::new(1.0, ESCORZO_DEL_SUELO, 1.0)),
                    Visibility::default(),
                ))
                .id();
            let shadow = commands
                .spawn((
                    GroundShadow { base: size, foot: 0.0, silueta: true, owner: entity },
                    crate::sim::UnlitSprite,
                    Sprite {
                        image: sprite.image.clone(),
                        rect: sprite.rect,
                        custom_size: Some(size),
                        flip_x: sprite.flip_x,
                        // Espejada hacia abajo y colgada del pie: el pie del dibujo
                        // queda pegado al pie del objeto y la copa cae lejos.
                        flip_y: true,
                        color: Color::srgba(0.02, 0.06, 0.055, 0.45),
                        anchor: Anchor::TopCenter,
                        ..default()
                    },
                    Transform::default(),
                ))
                .id();
            crate::render::try_add_child(&mut commands, rig, shadow);
            crate::render::try_add_child(&mut commands, entity, rig);
            continue;
        }
        // `try_add_child` y no `with_children`: el dueño de esta sombra puede
        // haberse ido en este mismo frame (el streamer tiró su chunk mientras
        // caminabas), y colgarle un hijo a una entidad muerta es un pánico —
        // el cierre al azar tras un rato de juego. Ver `render::try_add_child`.
        let shadow = commands
            .spawn((
                GroundShadow { base: Vec2::new(length, width), foot, silueta: false, owner: entity },
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
                // quedar la mancha encima de los zapatos. **Un pelo de la banda
                // Y-sort (`HAIR`, ¼ de metro), no `0.02`**: en esa banda 0.02 son
                // 400 m de mundo, y la sombra quedaba detrás de TODO lo parado en
                // 400 m a la redonda — el canto de una terraza la cortaba en seco
                // (captura del usuario, 3 oct 2026).
                Transform::from_xyz(0.0, foot, -crate::layers::HAIR),
            ))
            .id();
        crate::render::try_add_child(&mut commands, entity, shadow);
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
    ambiente: Ambiente,
    // `Without<GroundShadow>`: la query de sombras pide `&mut Transform`, así que
    // Bevy exige que ésta sea DEMOSTRABLEMENTE disjunta o aborta con B0001 al
    // primer frame de partida (se probó: panic al entrar a `Playing`).
    camera: Query<(&Transform, &OrthographicProjection), (With<GameCamera>, Without<GroundShadow>)>,
    lights: Query<(Entity, &Light2d, &GlobalTransform)>,
    parents: Query<&GlobalTransform, Without<GroundShadow>>,
    mut shadows: Query<(&GroundShadow, &mut Transform, &mut Sprite)>,
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
    let dark = ambiente.darkness();

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
    active.truncate(presupuesto_de_luces());

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

    for (sh, mut tf, mut sprite) in &mut shadows {
        let owner = sh.owner;
        let Ok(gt) = parents.get(owner) else { continue };
        let p = gt.translation().truncate();
        if p.distance(cam) > cull_r {
            continue;
        }
        let mut v = sky_v;
        let mut w = sky_w;
        for (_, le, lp, reach, k) in &active {
            // Mismo rechazo barato que en `light_sprites`: este también es un
            // bucle objeto × luz, y con el presupuesto de [`VEIL_LIGHTS`] en 40
            // la raíz cuadrada se pagaría 40 veces por cada sombra en cuadro.
            // Al cuadrado y sin raíz descarta lo que no llega; la distancia real
            // se calcula sólo para las luces que de verdad proyectan.
            let d2 = p.distance_squared(*lp);
            if d2 > *reach * *reach {
                continue;
            }
            // La luz que ESTE objeto lleva puesta no lo proyecta (ni la de un foco
            // prácticamente encima): estaríamos dibujando la sombra de la lámpara
            // debajo de la lámpara.
            let d = d2.sqrt();
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
        if sh.silueta {
            // La silueta cuelga del pie hacia su `-y` local: se gira para que ese
            // lado apunte en contra de la luz, y se alarga con el sol rasante. En
            // reposo (sin luz direccional) queda aplastada bajo el objeto.
            tf.translation.x = 0.0;
            tf.translation.y = sh.foot;
            // El rumbo se mide en el plano del suelo (sin escorzo): el soporte lo aplasta después.
            let ang = if len > 0.06 {
                (dir.y / ESCORZO_DEL_SUELO).atan2(dir.x) + std::f32::consts::FRAC_PI_2
            } else {
                0.0
            };
            tf.rotation = Quat::from_rotation_z(ang);
            sprite.custom_size = Some(Vec2::new(sh.base.x, sh.base.y * (0.30 + 0.32 * len)));
            let a = (0.30 + 0.30 * w.min(1.2)) * (1.0 - ambiente.weather.cover * 0.45) * (1.0 - 0.10 * len);
            sprite.color = if shadow_debug() {
                Color::srgba(1.0, 0.0, 0.8, 1.0)
            } else {
                Color::srgba(tint.x, tint.y, tint.z, a.clamp(0.0, 0.7))
            };
            continue;
        }
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
        let a = (0.46 + 0.50 * w.min(1.2)) * (1.0 - ambiente.weather.cover * 0.45) * spread;
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
    ambiente: Ambiente,
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
    // ¿Quedó algún sprite teñido del tick anterior? Ver la salida temprana abajo.
    mut habia_luz: Local<bool>,
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
    let dark = ambiente.darkness();
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
    active.truncate(presupuesto_de_luces());

    // **De día no hay nada que hacer** (3 oct 2026). Sin ninguna luz activa el
    // bucle de abajo recorría igual TODOS los sprites del mundo 20 veces por
    // segundo para concluir, sprite por sprite, que ninguno estaba en un pozo —
    // o sea el día entero, que es cuando más cosas hay en cuadro. Sólo hace falta
    // una vuelta más, la del atardecer al día, para devolverles su color a los que
    // quedaron teñidos; después el sistema duerme hasta que se prenda una luz.
    if active.is_empty() && !*habia_luz {
        return;
    }
    *habia_luz = !active.is_empty();

    // El sprite se tiñe si está en el pozo, aunque la cámara esté lejos: cortar
    // por `cull_r` a secas dejaba props sin teñir en el borde de la pantalla al
    // alejar el zoom, con el charco ya dibujado debajo.
    let sprite_reach = active.iter().map(|(_, _, r, _, _)| *r).fold(0.0f32, f32::max);

    for (e, gt, mut sprite, base) in &mut sprites {
        let p = gt.translation();
        // En la vuelta de apagado (sin luces activas) no se corta por distancia: un
        // sprite que quedó teñido lejos de la cámara tiene que volver a su color
        // ahora, porque después el sistema se duerme y ya no lo vería.
        if p.z > WORLD_Z_MAX || (!active.is_empty() && p.truncate().distance(cam) > cull_r + sprite_reach) {
            continue;
        }
        // **El arte PLANO no se tiñe acá.** Todo lo que está por debajo de
        // `layers::YSORT_MIN` —la baldosa de vereda, el asfalto, la pintura de la
        // calzada, un charco, una alfombra— ya lo ilumina el shader POR PÍXEL, y
        // con un degradado continuo. Teñirlo además por sprite le suma un valor
        // ÚNICO a toda la baldosa, así que dentro de un pozo de luz cada tile de
        // 32 px salta de golpe al nivel de su centro: en la captura de
        // `HA_NIGHTTEST` la vereda alrededor de cada farol se veía como un
        // damero de cuadrados más claros y más oscuros pisando el charco redondo
        // que el shader acababa de dibujar bien.
        //
        // El tinte por sprite existe para lo que SE LEVANTA del suelo, que es
        // donde aporta algo que el shader no puede: la cara que mira a la lámpara
        // recibe más que la de atrás. Una baldosa no tiene caras.
        //
        // Y de paso es la mitad del costo de este bucle: en la captura del barrio
        // hay 3.915 sprites de tile contra ~560 objetos.
        if p.z < crate::layers::YSORT_MIN {
            continue;
        }
        let sp = p.truncate();
        let half = sprite.custom_size.unwrap_or(Vec2::splat(1.0)) * 0.5;
        let mut acc = Vec3::ZERO;
        for (_, lp, r, rgb, k) in &active {
            // **Rechazo barato ANTES de la cuenta cara.** Esto es un bucle
            // sprite × luz: con cientos de sprites en cuadro y el presupuesto de
            // [`VEIL_LIGHTS`] es lo más caro que hace el sistema de luz en la CPU.
            // El 90 % de los pares están fuera de alcance, y descartarlos con una
            // resta y un producto escalar —sin `clamp`, sin dos raíces, sin dos
            // `falloff`— es lo que financia haber subido el presupuesto de 24 a
            // 40 sin que esto cueste más que antes.
            //
            // El margen es conservador (media diagonal de la caja del sprite):
            // rechaza sólo lo que con seguridad no toca, así el resultado es
            // idéntico al de calcularlos todos.
            let margen = *r + half.length();
            if sp.distance_squared(*lp) > margen * margen {
                continue;
            }
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
                crate::render::try_insert(&mut commands, e, "luz/tinte", LitBase { color: src, mul: acc, lit: true });
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
        // Volando el Ki se enciende; CAMINANDO NO EMITE NADA. El rescoldo de
        // 0.18 hacia que el jugador fuese una farola azul andante: no es un
        // emisor de luz, y siendo la unica luz siempre en el centro del cuadro
        // le tenia todo el suelo tenido de cian.
        // Cargando en tierra el aura se prende de a poco con la carga (biblia §20: que
        // la carga se SIENTA); al despegar llega a pleno.
        let target = if fly.flying() {
            1.0
        } else if fly.charging {
            0.8 * (fly.charge_t / crate::creative_fly::TAKEOFF_HOLD).clamp(0.0, 1.0)
        } else {
            0.0
        };
        light.intensity += (target - light.intensity) * 0.15;
        return;
    }
    let Ok(p) = player.get_single() else { return };
    commands.entity(p).try_insert((
        KiAura,
        Light2d {
            // **Cian de verdad, no celeste pálido.** El aura del jugador es la
            // única luz que está SIEMPRE en cuadro y siempre en el centro: con un
            // color casi blanco (0.62, 0.86, 1.0) su neblina —que mide 5.6 radios,
            // o sea media pantalla— bañaba la escena entera de blanco azulado.
            // Ése era el grueso de la "niebla blanca que tapa los colores".
            color: Color::srgb(0.30, 0.72, 1.0),
            radius: 2.2,
            // Arranca APAGADA: solo el vuelo Ki la enciende.
            intensity: 0.0,
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
    // Se llama una vez por pieza colocada, o sea cientos de veces por chunk.
    // §10 la señala como sospechosa del costo de `spawn_chunk_decor`; con sonda
    // propia deja de ser sospecha.
    let _probe = crate::debug::probe("light2d::attach");
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

#[cfg(test)]
mod tests_baldosas {
    use super::*;

    /// Generador chico y determinista (no hace falta `rand` para esto).
    fn lcg(s: &mut u64) -> f32 {
        *s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (*s >> 40) as f32 / (1u64 << 24) as f32
    }

    /// **Ningún píxel pierde una luz.** Para puntos al azar de la grilla, toda luz
    /// que el bucle viejo del shader le habría sumado (`dist² · z < 1`) tiene que
    /// estar en la máscara de su baldosa — si no, la grilla cambiaría la imagen.
    #[test]
    fn la_mascara_nunca_pierde_una_luz_que_llega() {
        for semilla in 1..=6u64 {
            let mut s = semilla;
            let origin = Vec2::new(lcg(&mut s) * 200.0 - 100.0, lcg(&mut s) * 200.0 - 100.0);
            let tile = Vec2::new(1.0 + lcg(&mut s) * 4.0, 1.0 + lcg(&mut s) * 4.0);
            let ext = tile * Vec2::new(TILES_X as f32, TILES_Y as f32);
            let mut lights = [Vec4::ZERO; VEIL_LIGHTS];
            for l in lights.iter_mut() {
                let c = origin + Vec2::new(lcg(&mut s) * 1.4 - 0.2, lcg(&mut s) * 1.4 - 0.2) * ext;
                let reach = 0.3 + lcg(&mut s) * 9.0;
                *l = Vec4::new(c.x, c.y, 1.0 / (reach * reach), 1.0);
            }
            let (lo, hi) = light_tiles(&lights, origin, tile);
            for _ in 0..4000 {
                let p = origin + Vec2::new(lcg(&mut s), lcg(&mut s)) * ext;
                let g = (p - origin) / tile;
                let k = (g.y as usize).min(TILES_Y - 1) * TILES_X + (g.x as usize).min(TILES_X - 1);
                for (i, l) in lights.iter().enumerate() {
                    let dp = p - l.truncate().truncate();
                    if dp.dot(dp) * l.z >= 1.0 {
                        continue;
                    }
                    let bit = if i < 32 { lo[k / 4][k % 4] >> i } else { hi[k / 4][k % 4] >> (i - 32) };
                    assert_eq!(bit & 1, 1, "semilla {semilla}: la luz {i} llega a {p:?} y su baldosa {k} no la tiene");
                }
            }
        }
    }

    /// Y la grilla de verdad PODA: una luz chica no se anota en baldosas lejanas.
    #[test]
    fn una_luz_chica_solo_marca_sus_baldosas() {
        let origin = Vec2::ZERO;
        let tile = Vec2::splat(2.0);
        let lights = [Vec4::new(5.0, 5.0, 1.0 / (1.5 * 1.5), 1.0)];
        let (lo, hi) = light_tiles(&lights, origin, tile);
        let marcadas: u32 = lo.iter().map(|v| v.to_array().iter().map(|b| b.count_ones()).sum::<u32>()).sum();
        assert!((1..=9).contains(&marcadas), "marcó {marcadas} baldosas");
        assert!(hi.iter().all(|v| *v == UVec4::ZERO));
        let lejos = 9 * TILES_X + 19;
        assert_eq!(lo[lejos / 4][lejos % 4], 0);
    }
}
