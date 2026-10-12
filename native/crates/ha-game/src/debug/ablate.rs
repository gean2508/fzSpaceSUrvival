//! **Ablación de capas a pantalla completa — herramienta de diagnóstico.**
//!
//! El Redmi mide 73 ms de frame con 1,27 ms de CPU medida, jitter de 0,2 ms y
//! 123 sprites visibles en 33 lotes. Bajar el display a 1/4 de píxeles lo llevó
//! a 20,5 ms: **el costo es por PÍXEL**, no CPU ni draw calls. Lo que falta es
//! saber QUÉ capa se los come, y eso no se adivina — se mide.
//!
//! Este módulo apaga y prende capas por turnos y anota el fps de cada
//! configuración. Una sola corrida da la atribución completa:
//!
//! **Ronda 1 (hecha, 2026-08-01).** Velo = 5 ms, overlays = 0 ms, quedaban 74 ms
//! sin explicar. O sea que el gasto está en la ESCENA misma: ~260 sprites de
//! decoración (árboles grandes) + su sombra + el suelo, todos mezclados con alpha
//! a 2400x1080. Eso es sobredibujado, y contra el sobredibujado hay dos palancas.
//!
//! **Ronda 2 (hecha).** Escala 0.7 → 80 ms baja a 47. Quitar las sombras (la
//! MITAD de los sprites visibles: 423 → 202) → **0 ms**. O sea que tampoco es
//! sobredibujado de sprites: el costo va con el PÍXEL DE MUNDO, y lo único que
//! cubre cada píxel de mundo es el SUELO.
//!
//! **Ronda 3 (esta).** El suelo se dibuja con `AlphaMode2d::Blend` (81 mallas,
//! 25 344 quads que tapan la pantalla) → cae en la fase transparente: sin
//! descarte por profundidad y mezclando cada fragmento, que en una GPU de tiles
//! es el peor caso. Sólo necesita mezcla MIENTRAS se funde hacia el satelital
//! (`fade_surface_lod`); a pie es opaco. Se mide si eso es el gasto:
//!
//! | cfg | qué cambia |
//! |-----|------------|
//! | 0   | nada (línea base, escala 1.0, suelo Blend) |
//! | 1   | **suelo opaco** (`AlphaMode2d::Opaque`) |
//! | 2   | escala de render 0.7 (el mundo dibuja el 49 % de los píxeles; la UI no) |
//! | 3   | las dos |
//!
//! Lo de la escala tiene un motivo extra para medirse ACÁ: en Android el camino
//! de blit está descartado desde el 31 jul por un reporte de "mundo negro", y el
//! propio comentario de `render/scale.rs` pide medirlo con el cable antes de
//! reactivarlo. Con el celu conectado se puede mirar si el mundo se ve o no.
//!
//! **Es temporal.** Se enciende con `HA_ABLATE=1` en escritorio y —porque a un
//! APK no se le pasan variables de entorno— con el archivo marcador
//! `ablate.on` en el directorio de persistencia del juego, que se deja con:
//!
//! ```sh
//! adb shell "run-as com.hungerarena.app touch files/ablate.on"   # build debuggable
//! ```
//!
//! Si el marcador no está, este módulo no toca absolutamente nada.

use bevy::prelude::*;


/// Segundos que dura cada configuración. Tiene que dar para varias muestras de
/// `ha-perf` (que sale cada 5 s) sin que la corrida entera sea eterna.
const PHASE_SECS: f32 = 12.0;
const PHASES: u32 = 4;

pub struct AblatePlugin;

impl Plugin for AblatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Ablate>()
            // En `PostUpdate` y ANTES de que Bevy propague visibilidad:
            // `light2d::toggle_light_passes` reescribe la visibilidad de las
            // pasadas cada frame en `Update`, así que desde acá se le pisa el
            // valor y la ablación se aplica en el mismo frame.
            .add_systems(
                PostUpdate,
                tick_ablate.before(bevy::render::view::VisibilitySystems::VisibilityPropagate),
            );
    }
}

#[derive(Resource, Default)]
struct Ablate {
    on: Option<bool>,
    t: f32,
    phase: u32,
    logged: bool,
}

/// **APAGADO — la atribución de la ronda 3 ya cerró** (1 ago 2026): el suelo en
/// `Blend` era 53 de los 81 ms, y el arreglo vive en `world2d::fade_surface_lod`.
///
/// Estuvo encendido siempre en Android porque a un APK no se le pasan variables
/// de entorno. Eso ya no puede quedar: la ablación PISA el `alpha_mode` del suelo
/// en `PostUpdate` cada 12 s, así que dejarla prendida devuelve el juego a los
/// 81 ms una fase de cada dos y tapa cualquier medición posterior — pasó exacto
/// al verificar el arreglo.
///
/// Para volver a correrla hay que reactivar el `cfg!(target_os = "android")` a
/// mano en un build de diagnóstico; en escritorio alcanza con `HA_ABLATE=1`.
fn enabled() -> bool {
    std::env::var("HA_ABLATE").is_ok()
}

fn tick_ablate(
    time: Res<Time<bevy::time::Real>>,
    stats: Res<super::perf::PerfStats>,
    mut ab: ResMut<Ablate>,
    mut scale: ResMut<crate::render::scale::RenderScaleOverride>,
    ground: Res<crate::world2d::ChunkMaterial>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let on = *ab.on.get_or_insert_with(enabled);
    if !on {
        return;
    }
    ab.t += time.delta_secs();
    if ab.t >= PHASE_SECS {
        ab.t = 0.0;
        ab.phase = (ab.phase + 1) % PHASES;
        ab.logged = false;
    }
    let phase = ab.phase;
    // Al final de cada fase (cuando el fps ya se estabilizó) se anota.
    if !ab.logged && ab.t > PHASE_SECS * 0.6 {
        ab.logged = true;
        eprintln!(
            "ha-ablate: cfg={phase} ({}) fps={:.0} frame_ms={:.1} spr_vis={} lotes={}",
            match phase {
                0 => "base",
                1 => "suelo opaco",
                2 => "escala 0.7",
                _ => "suelo opaco + escala 0.7",
            },
            stats.fps,
            stats.frame_ms,
            stats.breakdown.sprite_visible,
            stats.breakdown.sprite_batches,
        );
    }

    let want_scale = if phase >= 2 { 0.7 } else { 1.0 };
    if scale.0 != Some(want_scale) {
        scale.0 = Some(want_scale);
    }
    // `fade_surface_lod` reescribe este material cada frame, así que el modo hay
    // que volver a imponerlo cada frame (no alcanza con hacerlo al cambiar fase).
    let want_mode = if phase == 1 || phase == 3 {
        bevy::sprite::AlphaMode2d::Opaque
    } else {
        bevy::sprite::AlphaMode2d::Blend
    };
    if let Some(m) = materials.get_mut(&ground.0) {
        if m.alpha_mode != want_mode {
            m.alpha_mode = want_mode;
        }
    }
}
