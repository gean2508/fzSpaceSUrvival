//! **Escala de render: el mundo dibuja menos píxeles, el HUD no.**
//!
//! `InputCalibration::render_scale` existía desde hacía tiempo —declarado,
//! acotado a 0.55–1.0 y con botones en el editor de UI (`ui_editor.rs`)— pero
//! nadie lo leía: mover ese control no hacía absolutamente nada. Acá es donde
//! finalmente hace algo.
//!
//! ## Cómo
//!
//! La cámara de juego deja de dibujar contra la ventana y pasa a dibujar contra
//! una `Image` más chica (`físico × escala`). Una segunda cámara —[`BlitCamera`],
//! orden 1— estampa esa imagen a pantalla completa y se queda con la UI.
//!
//! El reparto importa: **el mundo pierde resolución, el HUD y el texto no.** Bajar
//! la resolución de todo se ve enseguida en la tipografía y en los bordes de los
//! botones, que es lo que el ojo mira de cerca; bajarla sólo en el mundo, con el
//! muestreo `nearest` que ya usa el juego, es casi imperceptible en un teléfono y
//! ahorra proporcionalmente al cuadrado de la escala. A 0.8 son un 36 % menos de
//! píxeles de mundo por frame.
//!
//! ## La trampa que ya se pisó una vez
//!
//! El comentario de `camera::spawn_camera` cuenta que hubo una segunda cámara de
//! HUD que terminó **re-dibujando el mundo entero** como un mini-overlay, y que
//! por eso se volvió a una sola. Lo que faltaba era aislarlas: acá la cámara de
//! blit vive en [`BLIT_LAYER`] y lo único que hay en esa capa es el quad de la
//! imagen. Todo el mundo sigue en la capa 0, que la cámara de blit no ve. Sin
//! ese aislamiento el bug vuelve.

use bevy::prelude::*;
use bevy::render::camera::RenderTarget;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureUsages,
};
use bevy::render::view::RenderLayers;
use bevy::window::PrimaryWindow;

use crate::camera::GameCamera;
use crate::ui_editor::UiLayoutConfig;

/// Capa exclusiva del quad de blit. El mundo vive en la 0 y la cámara de blit no
/// la mira — es lo que impide que ésta re-dibuje la escena entera (ver el módulo).
const BLIT_LAYER: usize = 1;

/// Por debajo de esto no vale la pena el rodeo del render target: a escala 1.0 la
/// cámara dibuja directo contra la ventana, como siempre.
const DIRECT_ABOVE: f32 = 0.995;

pub struct RenderScalePlugin;

impl Plugin for RenderScalePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RenderScaleState>()
            .init_resource::<RenderScaleOverride>()
            .add_systems(Startup, spawn_blit_camera)
            .add_systems(Update, sync_render_target);
    }
}

/// La cámara que estampa el render del mundo y encima dibuja la UI.
#[derive(Component)]
pub struct BlitCamera;

#[derive(Component)]
struct BlitQuad;

/// Fuerza una escala por encima de todo (calibración guardada, default por
/// plataforma y `HA_RENDER_SCALE`). Existe para que el banco de pruebas
/// (`debug::ablate`) pueda recorrer escalas EN EL TELÉFONO y medir la curva real,
/// que es la única forma de decidir este número — a ojo ya se decidió mal una vez.
/// `None` = nadie la fuerza, manda la lógica normal.
#[derive(Resource, Default)]
pub struct RenderScaleOverride(pub Option<f32>);

#[derive(Resource, Default)]
pub struct RenderScaleState {
    /// Escala vigente (1.0 = directo a ventana, sin target intermedio).
    pub scale: f32,
    /// Tamaño en píxeles del target, para no recrear la imagen sin necesidad.
    /// Público porque el overlay de PERF lo muestra: es la única forma de ver EN
    /// EL TELÉFONO si el camino de blit está activo y a qué resolución, sin adb.
    pub size: UVec2,
    image: Option<Handle<Image>>,
}

fn spawn_blit_camera(mut commands: Commands) {
    if crate::camera::una_sola_camara() {
        return;
    }
    commands.spawn((
        BlitCamera,
        Camera2d,
        Camera {
            // Después de la cámara de mundo (orden 0, por defecto).
            order: 1,
            // Lo pone `sync_render_target` según el camino activo, y la diferencia
            // NO es cosmética:
            //
            // - Con blit (escala < 1) tiene que LIMPIAR. La cámara de mundo dibuja
            //   fuera de pantalla, así que si esta no limpia, nadie limpia la
            //   ventana: cuando el blit falló en el teléfono el resultado fue el
            //   framebuffer sin inicializar —NEGRO TOTAL— con la UI flotando
            //   encima. Un fallo silencioso indistinguible de "el juego se rompió".
            //   Limpiando con el color de cielo, ese mismo fallo se ve como un
            //   cielo vacío y se diagnostica de un vistazo.
            // - Sin blit (escala 1.0) NO debe limpiar nunca: la cámara de mundo ya
            //   dibujó la escena EN LA VENTANA, y limpiar después la borraría
            //   entera dejando sólo la UI.
            clear_color: ClearColorConfig::None,
            ..default()
        },
        // La UI se muda acá: se maqueta y se dibuja a resolución NATIVA, encima
        // del mundo ya escalado. Ésta es la mitad que hace que valga la pena.
        // (`bevy_ui` mide el layout con `physical_viewport_size` de su cámara, que
        // acá es la ventana entera — la proyección 1×1 de abajo no lo afecta.)
        bevy::ui::IsDefaultUiCamera,
        // También acá: `Camera` exige `Msaa` y su default es 4×. Sin esto, el pase
        // que dibuja el HUD y el quad de blit correría multisampleado para nada.
        Msaa::Off,
        RenderLayers::layer(BLIT_LAYER),
        // Proyección 1×1: el quad de blit mide (1,1) y llena el viewport sea cual
        // sea la resolución o el aspecto, sin tener que reajustarlo al redimensionar.
        OrthographicProjection {
            scaling_mode: bevy::render::camera::ScalingMode::Fixed {
                width: 1.0,
                height: 1.0,
            },
            ..OrthographicProjection::default_2d()
        },
    ));
}

fn sync_render_target(
    mut commands: Commands,
    config: Res<UiLayoutConfig>,
    forced: Res<RenderScaleOverride>,
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<RenderScaleState>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut game_cam: Query<&mut Camera, (With<GameCamera>, Without<BlitCamera>)>,
    mut blit_cam: Query<&mut Camera, (With<BlitCamera>, Without<GameCamera>)>,
    quad: Query<Entity, With<BlitQuad>>,
) {
    let Ok(win) = window.get_single() else {
        return;
    };
    let Ok(mut cam) = game_cam.get_single_mut() else {
        return;
    };
    // `HA_RENDER_SCALE=0.7` fuerza la escala en escritorio. Sin esto el camino del
    // render target sólo se ejercitaría en el teléfono (donde el default es 0.8) y
    // en la PC nunca se probaría — que es justo donde se prueba todo lo demás.
    // EN EL TELÉFONO EL BLIT ESTÁ DESCARTADO (2026-07-31). Reporte con foto desde un
    // Xiaomi: mundo NEGRO, HUD perfecta, overlay `blit 0.80 1920x864`. El dato que
    // señala al render target es que, si fallara sólo el QUAD, la ventana quedaría del
    // color del cielo — la cámara de blit limpia con `ClearColor`, y el overlay mostraba
    // `clear=(0.6,0.?2,0.5)`, que no es negro. Negro total = la ventana no recibe ni la
    // limpieza, y aun así la UI (que cuelga de esa misma cámara) se dibuja.
    //
    // Se ignora a propósito el valor GUARDADO en la calibración: los teléfonos que ya
    // jugaron tienen 0.8 en disco y el default nuevo no les llegaría nunca.
    //
    // Para volver a habilitarlo hay que MEDIRLO con el cable primero
    // (`docs/RENDIMIENTO.md`), no cambiarlo de nuevo a ojo. `HA_RENDER_SCALE` sigue
    // mandando, así que el camino del render target se puede seguir ejercitando.
    #[cfg(target_os = "android")]
    let saved = {
        let _ = &config; // sólo se lee en escritorio (abajo)
        1.0_f32
    };
    #[cfg(not(target_os = "android"))]
    let saved = config.calibration.clamp().render_scale;
    let scale = forced
        .0
        .or_else(|| crate::flags::var("HA_RENDER_SCALE").ok().and_then(|v| v.parse::<f32>().ok()))
        .unwrap_or(saved)
        .clamp(0.25, 1.0);
    let win_px = UVec2::new(win.physical_width().max(1), win.physical_height().max(1));
    let want = if scale >= DIRECT_ABOVE {
        win_px
    } else {
        (win_px.as_vec2() * scale).round().as_uvec2().max(UVec2::ONE)
    };

    if state.scale == scale && state.size == want {
        return;
    }
    state.scale = scale;
    state.size = want;

    // Escala 1.0 → se vuelve al camino directo: la cámara de mundo dibuja contra
    // la ventana y el quad se va. Así el rodeo no se paga cuando no se usa.
    if scale >= DIRECT_ABOVE {
        cam.target = RenderTarget::Window(bevy::window::WindowRef::Primary);
        state.image = None;
        for e in &quad {
            commands.entity(e).despawn_recursive();
        }
        // Sin blit, la escena YA está dibujada en la ventana por la cámara de
        // mundo: si ésta limpiara, la borraría entera y quedaría sólo la UI.
        if let Ok(mut blit) = blit_cam.get_single_mut() {
            blit.clear_color = ClearColorConfig::None;
        }
        return;
    }

    // Con blit sí limpia: nadie más toca la ventana, y un fallo del quad tiene que
    // verse como cielo vacío y no como pantalla negra (ver `spawn_blit_camera`).
    if let Ok(mut blit) = blit_cam.get_single_mut() {
        blit.clear_color = ClearColorConfig::Default;
    }

    let mut image = Image::new_fill(
        Extent3d {
            width: want.x,
            height: want.y,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        // **`Rgba8UnormSrgb`, no BGRA — es la causa más probable del mundo negro
        // en el teléfono.** En escritorio (Vulkan/NVK) `Bgra8UnormSrgb` funcionaba,
        // pero en GPUs móviles (Adreno/Mali) los formatos BGRA suelen existir sólo
        // para la swapchain y NO como adjunto de color de un render target normal.
        // RGBA8 sRGB es el formato universalmente soportado, y es el que usan los
        // ejemplos de render-a-textura de Bevy.
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    // `RENDER_ATTACHMENT` es lo que la habilita como destino de la cámara;
    // `TEXTURE_BINDING` es lo que después deja leerla desde el sprite del blit.
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
        | TextureUsages::COPY_DST
        | TextureUsages::RENDER_ATTACHMENT;
    let handle = images.add(image);
    cam.target = RenderTarget::Image(handle.clone());

    for e in &quad {
        commands.entity(e).despawn_recursive();
    }
    commands.spawn((
        BlitQuad,
        Sprite {
            image: handle.clone(),
            // (1,1) en el espacio de la proyección fija de arriba = pantalla completa.
            custom_size: Some(Vec2::ONE),
            ..default()
        },
        Transform::default(),
        RenderLayers::layer(BLIT_LAYER),
        // El mundo ya viene iluminado dentro de la imagen: teñir el quad sería
        // aplicarle la luz una segunda vez, a la pantalla entera.
        crate::sim::UnlitSprite,
        Name::new("RenderScaleBlit"),
    ));
    state.image = Some(handle);
}

/// **Píxel de ventana → punto del mundo, con o sin escala de render.**
///
/// `Camera::viewport_to_world_2d` interpreta su argumento en el espacio del
/// VIEWPORT de la cámara. Mientras la cámara de juego dibujaba contra la ventana
/// los dos espacios coincidían y se le podía pasar la posición del cursor tal
/// cual; apuntando a una imagen más chica dejan de coincidir, y pasarle píxeles
/// de ventana desalinea el apuntado y la selección de NPCs justo en proporción a
/// la escala.
///
/// El factor sale de comparar los dos tamaños, así que esto da lo mismo que antes
/// cuando la escala es 1.0 — no hay un camino "con escala" y otro "sin".
pub fn window_to_world_2d(
    camera: &Camera,
    cam_tf: &GlobalTransform,
    window: &Window,
    window_pos: Vec2,
) -> Option<Vec2> {
    let vp = camera.logical_viewport_size()?;
    let win = window.size();
    if win.x <= 0.0 || win.y <= 0.0 {
        return None;
    }
    let viewport_pos = window_pos * (vp / win);
    camera.viewport_to_world_2d(cam_tf, viewport_pos).ok()
}
