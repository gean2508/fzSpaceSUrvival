//! **Cartel de "en pruebas".** Aparece SOLO mientras alguien está probando el
//! juego en el teléfono, y se va solo cuando termina.
//!
//! ## Por qué un archivo y no una variable de entorno
//!
//! A un APK lanzado por el sistema no se le pasan variables de entorno (el mismo
//! motivo por el que `MENU -> SONDA` es un interruptor en pantalla y no
//! `HA_STAGES`). Lo que sí se puede escribir desde afuera sin permisos ni root es
//! el directorio de datos externos de la propia app, así que el canal es un
//! archivo ahí:
//!
//! ```sh
//! adb shell 'echo "instalando APK" > /sdcard/Android/data/com.hungerarena.app/files/ha-prueba'
//! adb shell 'rm /sdcard/Android/data/com.hungerarena.app/files/ha-prueba'   # se apaga
//! ```
//!
//! Existe el archivo → hay cartel, y su contenido es el renglón de abajo (lo que
//! se está haciendo AHORA). No existe → no hay nada, ni entidad ni sistema
//! haciendo trabajo visible. Por eso no queda encendido "para siempre": no hay
//! nada que apagar, se borra el archivo y listo.
//!
//! El sondeo es cada medio segundo y es un `metadata()` — no abre el archivo si
//! no cambió la fecha de modificación. En el presupuesto del Redmi (regla 2) no
//! se nota, y sólo corre cuando el archivo está.

use bevy::prelude::*;

/// Cada cuánto se mira si el archivo cambió. Medio segundo alcanza de sobra para
/// una guía visual y no cuesta nada.
const SONDEO_SEGS: f32 = 0.5;

#[derive(Resource)]
struct Sondeo(Timer);

impl Default for Sondeo {
    fn default() -> Self {
        Self(Timer::from_seconds(SONDEO_SEGS, TimerMode::Repeating))
    }
}

/// Lo último que se leyó, para no reconstruir el cartel en cada sondeo.
#[derive(Resource, Default)]
struct UltimoTexto(Option<String>);

#[derive(Component)]
struct PruebaRoot;

#[derive(Component)]
struct PruebaDetalle;

pub struct PruebaOverlayPlugin;

impl Plugin for PruebaOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sondeo>()
            .init_resource::<UltimoTexto>()
            .add_systems(Update, sondear);
    }
}

/// Dónde vive el archivo. En el teléfono, el directorio externo de la app —
/// escribible por `adb` sin permisos. En escritorio, el temporal de siempre.
fn ruta() -> &'static str {
    #[cfg(target_os = "android")]
    {
        "/sdcard/Android/data/com.hungerarena.app/files/ha-prueba"
    }
    #[cfg(not(target_os = "android"))]
    {
        "/tmp/ha-prueba"
    }
}

fn sondear(
    mut commands: Commands,
    time: Res<Time<bevy::time::Real>>,
    mut sondeo: ResMut<Sondeo>,
    mut ultimo: ResMut<UltimoTexto>,
    root: Query<Entity, With<PruebaRoot>>,
    mut detalle: Query<&mut Text, With<PruebaDetalle>>,
) {
    if !sondeo.0.tick(time.delta()).just_finished() {
        return;
    }
    // CADUCA SOLO. Si el archivo lleva mas de `VIDA_SEGS` sin tocarse, el cartel
    // se va aunque nadie lo haya borrado: si la prueba se corta a la mitad —se
    // cae la conexion, se cierra la terminal— no queda un cartel colgado
    // estorbando la pantalla. Mientras Leo esta trabajando lo reescribe en cada
    // paso, asi que se mantiene vivo solo.
    const VIDA_SEGS: u64 = 90;
    let fresco = std::fs::metadata(ruta())
        .and_then(|m| m.modified())
        .map(|t| t.elapsed().map(|e| e.as_secs() < VIDA_SEGS).unwrap_or(true))
        .unwrap_or(false);
    let ahora = fresco.then(|| std::fs::read_to_string(ruta()).ok()).flatten().map(|s| {
        // Una línea, corta: es una guía visual, no un log.
        s.lines().next().unwrap_or("").chars().take(60).collect::<String>()
    });
    if ahora == ultimo.0 {
        return;
    }
    ultimo.0 = ahora.clone();

    match ahora {
        // Se borró el archivo: se va el cartel entero.
        None => {
            for e in &root {
                commands.entity(e).despawn_recursive();
            }
        }
        Some(linea) => {
            // Ya está puesto: sólo cambia el renglón de abajo.
            if let Ok(mut t) = detalle.get_single_mut() {
                t.0 = linea;
                return;
            }
            spawn_cartel(&mut commands, &linea);
        }
    }
}

fn spawn_cartel(commands: &mut Commands, linea: &str) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(6.0),
                left: Val::Percent(50.0),
                // Centrado sin transform: el margen negativo lo corre media
                // caja a la izquierda.
                margin: UiRect::left(Val::Px(-110.0)),
                width: Val::Px(220.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(5.0)),
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
            BorderRadius::all(Val::Px(6.0)),
            // Por ENCIMA de todo, incluido el HUD y los paneles: si el cartel
            // queda tapado no sirve de guia.
            GlobalZIndex(3000),
            // No se toca: es un cartel, no un boton. Sin esto se comeria los
            // toques de lo que tiene debajo justo mientras estamos probando.
            bevy::ui::FocusPolicy::Pass,
            PruebaRoot,
            Name::new("PruebaOverlay"),
        ))
        .with_children(|c| {
            c.spawn((
                Text::new("EN PRUEBAS CON LEO"),
                TextFont { font_size: 11.0, ..default() },
                TextColor(Color::srgb(0.91, 0.82, 0.38)),
            ));
            c.spawn((
                Text::new(linea.to_string()),
                TextFont { font_size: 9.0, ..default() },
                TextColor(Color::srgb(0.88, 0.88, 0.94)),
                PruebaDetalle,
            ));
        });
}
