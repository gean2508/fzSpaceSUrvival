//! 2D follow camera — surface view (Terraria zoom) and SFS-style flight zoom.

use bevy::input::mouse::MouseWheel;
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::render::camera::ScalingMode;

use crate::scene::{RegionId, RegionRuntime};
use crate::sim::Player;

/// Marker for the single in-game camera.
#[derive(Component)]
pub struct GameCamera;

/// Legacy first/third-person pitch clamps — kept for input compat (unused in 2D).
pub const FP_MIN_PITCH: f32 = -1.35;
pub const FP_MAX_PITCH: f32 = 1.35;

/// Viewport height (world metres) while walking on a surface.
const SURFACE_VIEW_H: f32 = 30.0;
/// User zoom clamps (viewport height, world m): from tight to whole-SYSTEM (the
/// max frames the Sun + planets + Moon; Neptune's orbit is ~630 km from the Sun).
/// Encuadre mínimo. 10 m (antes 18) para que volando se pueda hacer un plano
/// PERSONAL del personaje: pedido explícito — "quiero acercarme más al jugador sin
/// interferir con su altura". El zoom nunca toca la altura (ver `camera_zoom_input`).
const ZOOM_MIN_H: f32 = 10.0;
const ZOOM_MAX_H: f32 = 40_000_000.0;
// (Acá vivían `SURFACE_ZOOM_MIN_H`/`MAX_H`, el rango angosto de a pie. Se fueron con el
// resto de los reencuadres automáticos: ver `follow_player_camera`.)
/// On-foot pinch gesture: both fingers in the TOP band of the screen (above the
/// stick zones, `layout.zone_top`). While active the joysticks force-release
/// these pointer ids so the pinch never fights the twin-stick (see
/// `input/joystick.rs`). `ids` = the two participating touch ids.
#[derive(Resource, Default)]
pub struct PinchActive {
    pub ids: Option<(u64, u64)>,
}

/// Encuadre de la cámara. La cámara SIEMPRE está centrada en el jugador; lo único
/// que se mueve es cuánto mundo entra en pantalla, y eso sale de DOS cosas que se
/// multiplican:
///
/// - **la altura** (`auto_view_h`): subir aleja el suelo. Es la sensación de subir,
///   y es la razón de ser del vuelo en una vista cenital — el personaje no se mueve
///   en pantalla, el mundo se achica debajo suyo.
/// - **el pellizco del jugador** (`user_mult`): un multiplicador PERSISTENTE encima.
///   Nunca se resetea ni se pisa, así que despegar, rebasar de marco y aterrizar no
///   te cambian el encuadre que elegiste.
///
/// Que sea un producto (y no un `max`, ni dos caminos según se vuele o no) es lo
/// que hace todo lo demás: es continuo en el despegue y el aterrizaje —
/// `view_h_for_alt(b, 0) == SURFACE_VIEW_H`, o sea `auto = 1×` a ras de suelo—, y
/// bajar deshace subir EXACTAMENTE porque `auto` es una función pura de la altura.
#[derive(Resource)]
pub struct CameraZoom {
    /// Multiplicador del jugador sobre el encuadre automático. 1.0 = el que pide la
    /// altura. Lo escribe SÓLO el pellizco/scroll.
    pub(crate) user_mult: f32,
    /// Encuadre que pide la ALTURA, antes del pellizco (30 m en el suelo, el disco
    /// del planeta en órbita). Ver `universe::view_h_for_alt`.
    pub(crate) auto_view_h: f32,
    /// Objetivo de este frame = `auto_view_h * user_mult`, acotado. La proyección
    /// lo persigue con un lerp.
    pub(crate) view_h: f32,
    /// Encuadre REALMENTE en pantalla este frame. Es el que deben mirar los
    /// sistemas que deciden qué nivel de detalle dibujar: si usaran el objetivo, el
    /// terreno se desvanecería antes de que la cámara llegue a alejarse.
    pub(crate) live_view_h: f32,
    /// Previous two-finger distance (px) for pinch deltas.
    pinch_prev: Option<f32>,
}

impl Default for CameraZoom {
    fn default() -> Self {
        Self {
            user_mult: 1.0,
            auto_view_h: SURFACE_VIEW_H,
            view_h: SURFACE_VIEW_H,
            live_view_h: SURFACE_VIEW_H,
            pinch_prev: None,
        }
    }
}

impl CameraZoom {
    /// Pedir un encuadre EN PANTALLA (metros de mundo de alto), sea cual sea la
    /// altura actual. Lo usan los drivers de QA, que razonan en lo que se ve.
    /// Un jugador no tiene esto: él pellizca, y el pellizco mueve el multiplicador.
    pub fn pedir_encuadre(&mut self, h: f32) {
        self.user_mult = (h / self.auto_view_h.max(1e-3)).max(1e-6);
    }

    /// Multiplicar el encuadre del jugador (pellizco, scroll). Acotado a un rango
    /// amplio: el producto se acota aparte, contra lo que el motor puede dibujar.
    fn escalar(&mut self, ratio: f32) {
        self.user_mult = (self.user_mult * ratio).clamp(MULT_MIN, MULT_MAX);
    }
}

/// Cuánto puede apartarse el jugador del encuadre automático. El rango es enorme a
/// propósito por los dos lados: mirar el sistema entero desde el suelo (×1.3M) y
/// hacer un plano personal del personaje en órbita (×1/300) son los dos cosas que
/// se pidieron. El producto se acota después contra `ZOOM_MIN_H`/`ZOOM_MAX_H`.
const MULT_MIN: f32 = 0.003;
const MULT_MAX: f32 = 2_000_000.0;

/// Facing direction on the 2D strip: rot 0 → +s, rot PI → -s.
pub fn facing_sign(rot: f32) -> f32 {
    if rot.abs() > std::f32::consts::FRAC_PI_2 {
        -1.0
    } else {
        1.0
    }
}

/// Legacy shim: flat aim from the look intent — fixed +s in 2D (aim is
/// pointer-driven now, see `targeting.rs`).
pub fn look_forward_flat(_look: &crate::input::CameraLookIntent) -> Vec2 {
    Vec2::new(1.0, 0.0)
}

/// Top-down: the stick maps straight to the ground plane (x = east, z = south).
pub fn camera_relative_move(mx: f32, mz: f32, _yaw: f32) -> (f32, f32) {
    (mx, mz)
}

#[derive(Event)]
pub struct CameraShakeEvent {
    pub trauma: f32,
}

#[derive(Resource, Default)]
pub struct CameraTrauma(pub f32);

/// Cuánto se adelanta la cámara hacia el cuerpo al que te estás aproximando, en
/// metros de mundo. Lo escribe `creative_fly` (que es quien sabe hacia dónde
/// vas); la cámara sólo lo aplica, acotado a un cuarto de pantalla.
#[derive(Resource, Default)]
pub struct ApproachFraming {
    pub lead: Vec2,
}

pub struct FollowCameraPlugin;

impl Plugin for FollowCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraTrauma>()
            .init_resource::<ApproachFraming>()
            .init_resource::<CameraZoom>()
            .init_resource::<PinchActive>()
            .add_event::<CameraShakeEvent>()
            .add_systems(Startup, spawn_camera)
            .add_systems(
                Update,
                (absorb_shake_events, camera_zoom_input, follow_player_camera).chain(),
            );
    }
}

/// **Una sola cámara cuando no hay blit** (todo Android: ahí la escala de render está descartada).
///
/// Con la cámara de mundo + la de blit/UI sobre la MISMA ventana, el Redmi mostraba cada ~6 s un
/// cuadro de pantalla SIN ningún sprite (ni jugador, ni árboles, ni velo nocturno: sólo suelo y
/// UI) — el "el mundo parpadea y a veces la UI también" del 1 oct 2026. Aislado grabando la
/// pantalla (ffmpeg, cuadro a cuadro) y descartando, una por una: luces, velo, visibilidad,
/// fases de render, pipelines, modo de presentación, render en paralelo y tope de FPS. Con una
/// sola cámara: 0 destellos en 15 s y sin tirones de >50 ms. La UI cuelga de la cámara del mundo.
/// `HA_DOS_CAMARAS=1` devuelve las dos (para investigar).
pub fn una_sola_camara() -> bool {
    cfg!(target_os = "android") && crate::flags::var("HA_DOS_CAMARAS").is_err()
}

fn spawn_camera(mut commands: Commands) {
    let sola = una_sola_camara();
    let mut e = commands.spawn((
        GameCamera,
        Camera2d,
        // **La UI ya no cuelga de acá: vive en la cámara de blit**
        // (`render::scale::BlitCamera`), que dibuja a resolución nativa encima
        // del mundo escalado. La segunda cámara que hubo antes —y que terminó
        // re-dibujando el mundo entero como un mini-overlay— fallaba por no
        // aislar capas; ésta vive en `BLIT_LAYER` y no ve la capa del mundo.
        // Ver el módulo `render::scale` para el porqué completo.
        // **MSAA APAGADO.** `Camera` exige un `Msaa` y su default es `Sample4`,
        // así que sin decir nada corríamos con multisampling 4×. En un juego de
        // sprites muestreados con `nearest` (ver `ImagePlugin` en `lib.rs`) no
        // suaviza nada que se vea —los bordes duros del pixel-art son el estilo,
        // no un artefacto— pero cuadruplica el ancho de banda del pase principal,
        // que en una GPU móvil de tiles es EL recurso escaso.
        //
        // (Tonemapping y deband NO hacía falta tocarlos, aunque a primera vista lo
        // pareciera: `Camera2d` ya exige `Tonemapping::None`, y el default de
        // `DebandDither` es `Disabled`. En 2D venían apagados de fábrica.)
        Msaa::Off,
        OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: SURFACE_VIEW_H,
            },
            near: -1000.0,
            far: 1000.0,
            ..OrthographicProjection::default_2d()
        },
        Transform::from_xyz(0.0, 0.0, 100.0),
    ));
    if sola {
        e.insert(bevy::ui::IsDefaultUiCamera);
    }
}

fn absorb_shake_events(
    time: Res<Time>,
    mut events: EventReader<CameraShakeEvent>,
    mut trauma: ResMut<CameraTrauma>,
) {
    for e in events.read() {
        trauma.0 = (trauma.0 + e.trauma).min(1.0);
    }
    trauma.0 = (trauma.0 - time.delta_secs() * 1.8).max(0.0);
}

/// Qué tan rápido persigue el encuadre a su objetivo, por segundo. Alto a propósito: el
/// zoom del jugador tiene que sentirse inmediato. Sólo amortigua los saltos.
const ZOOM_FOLLOW_RATE: f32 = 9.0;

/// Techo duro de cuánto puede cambiar el encuadre, como proporción **por segundo**. Es el
/// invariante "una transición no se nota" (regla 8) hecho código: por más que el objetivo
/// pegue un salto —el rebase de marco entre cuerpos lo hacía saltar x1.79—, en pantalla
/// nunca hay un corte. `MAX_ZOOM_JUMP` del driver mide exactamente esto.
///
/// **Va por segundo, no por frame.** Era `1.02` por frame, y eso ataba la velocidad del
/// reencuadre a los fps: x1.02^60 = x3.3 por segundo en la compu, pero x1.02^13 = x1.29 en
/// el Redmi. O sea que la misma maniobra tardaba ~2.5 veces más justo en el aparato que
/// manda (regla 2), y se veía como una cámara que se queda trabada afuera: al tocar suelo
/// tras un descenso desde 600 m, volver al encuadre de superficie (30 m) le llevaba ~7.8 s
/// en el celu contra 1.7 s en la compu. El driver `flight` lo cazó como
/// `camera_zoomed_out_after_touchdown` (view_h=227) el 3 ago 2026.
///
/// El valor conserva el tacto que ya estaba tuneado a 60 fps: 1.02^60 ≈ 3.3.
const ZOOM_MAX_STEP_PER_S: f32 = 3.3;

/// …y ADEMÁS un techo por frame, porque el invariante de "no se nota" es sobre lo que ve
/// el ojo en UN frame, no por segundo. Con `dt` grande (el celu tironeando) el tope por
/// segundo solo dejaría pasar un salto visible: a 2 fps daría x1.81. Éste lo corta. Queda
/// por debajo del x1.5 que el driver considera corte.
const ZOOM_MAX_STEP_FRAME: f32 = 1.35;

/// **El encuadre de este frame**: lo que pide la altura, escalado por el jugador.
///
/// - `auto_h`: `universe::view_h_for_alt(cuerpo, altura)` — vale `SURFACE_VIEW_H`
///   exacto a ras de suelo, así que despegar y aterrizar son continuos.
/// - `mult`: el multiplicador del JUGADOR (lo único que escribe el pellizco).
///
/// Es una función pura a propósito: la continuidad y la simetría subir/bajar son
/// propiedades de ESTA cuenta (nada se guarda entre frames), y así se pueden probar
/// sin motor — ver los tests del final del módulo.
fn desired_frame_h(auto_h: f32, mult: f32, min_h: f32, max_h: f32) -> f32 {
    (auto_h * mult).clamp(min_h, max_h)
}

/// Pinch (mobile) / scroll (desktop) adjust the zoom; the camera stays centred on
/// the target and NEVER moves the player. Escribe SIEMPRE el `view_h` absoluto: a pie,
/// volando y en el espacio. No hay multiplicador ni piso por altura — el encuadre es del
/// jugador y cruzar un límite no lo toca. Zoom only ever changes the viewport, never the
/// character.
fn camera_zoom_input(
    region: Res<RegionRuntime>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    trans: Res<crate::transition::FlightTransition>,
    layout: Res<crate::input::TouchLayout>,
    editor: Res<crate::ui_editor::UiEditorState>,
    mut pinch: ResMut<PinchActive>,
    mut zoom: ResMut<CameraZoom>,
    mut wheel: EventReader<MouseWheel>,
    touches: Res<Touches>,
    button_rects: Res<crate::input::actions::ActionButtonRects>,
) {
    let mut scroll = 0.0;
    for e in wheel.read() {
        scroll += e.y;
    }
    if editor.menu_open {
        pinch.ids = None;
        zoom.pinch_prev = None;
        return;
    }
    // Flying = a transition owns the base zoom (auto-reveal) OR we're in the
    // orbital frame. Then scroll/pinch tune the MULTIPLIER; on foot they set the
    // absolute `view_h`.
    let flying_view = fly.flying() || trans.active() || matches!(region.active, RegionId::Space);

    if flying_view {
        // Dedos CANDIDATOS al pellizco: los que no están sobre un botón de acción
        // ni en la zona del stick.
        //
        // Antes valía "cualquier par de dedos, en cualquier parte", y eso era un bug
        // de verdad: el botón BAJAR vive en medio de la pantalla (`ui_editor.rs`,
        // 0.72 × 0.60), o sea JUSTO donde aterriza un dedo al pellizcar. Resultado:
        // hacer zoom volando mantenía BAJAR pulsado y te hacía perder altura — que
        // es exactamente el "el zoom me baja la altura de vuelo" reportado. Y al
        // revés, el dedo del stick contaba como punto del pellizco, así que mover el
        // stick cambiaba el zoom solo.
        //
        // Filtrando los candidatos, cada dedo hace UNA cosa: el del botón pulsa, el
        // del stick pilota, y sólo los libres hacen zoom.
        let free: Vec<(u64, Vec2)> = touches
            .iter()
            .map(|t| (t.id(), t.position()))
            .filter(|(_, p)| {
                !crate::input::actions::touch_hits_action_button(*p, &button_rects)
                    && !crate::input::layout::touch_in_move_zone(*p, &layout)
            })
            .collect();
        // Registrar los ids del pellizco (antes se ponía `None` a propósito): así el
        // stick y los botones pueden ignorarlos con la guarda `in_pinch` que YA usan.
        pinch.ids = if free.len() >= 2 { Some((free[0].0, free[1].0)) } else { None };
        // UN SOLO CAMINO, SIEMPRE EL MISMO VALOR. Antes esto se bifurcaba según hubiera o no
        // una transición conduciendo el encuadre: con transición el gesto movía un
        // multiplicador, sin transición movía un valor absoluto. Esa bifurcación era el
        // salto: cruzar el límite (despegar, entrar al espacio, aterrizar) cambiaba qué
        // cosa estabas manejando, y el valor de la otra se descartaba. Ahora el pellizco
        // escribe SIEMPRE `user_mult`, vueles o no, así que cruzar un límite no mueve nada.
        if scroll.abs() > 0.0 {
            zoom.escalar(1.0 - scroll * 0.12);
        }
        let pts: Vec<Vec2> = free.iter().map(|(_, p)| *p).collect();
        if pts.len() >= 2 {
            let dist = pts[0].distance(pts[1]).max(1.0);
            if let Some(prev) = zoom.pinch_prev {
                if (dist - prev).abs() > 1.5 {
                    let ratio = (prev / dist).clamp(0.5, 2.0);
                    zoom.escalar(ratio);
                }
            }
            zoom.pinch_prev = Some(dist);
        } else {
            zoom.pinch_prev = None;
        }
    } else {
        if scroll.abs() > 0.0 {
            // Wheel up = zoom in (smaller viewport).
            zoom.escalar(1.0 - scroll * 0.12);
        }
        // On foot: pellizco SÓLO cuando AMBOS dedos están en la banda superior
        // (arriba de las zonas de stick) — así el twin-stick de abajo nunca se
        // malinterpreta como pinch. `input/joystick.rs` consulta `PinchActive`.
        let top_band = layout.zone_top.max(1.0);
        let pts: Vec<(u64, Vec2)> = touches
            .iter()
            .map(|t| (t.id(), t.position()))
            .filter(|(_, p)| p.y < top_band)
            .collect();
        if pts.len() == 2 {
            let dist = pts[0].1.distance(pts[1].1).max(1.0);
            if pinch.ids == Some((pts[0].0, pts[1].0)) {
                if let Some(prev) = zoom.pinch_prev {
                    if (dist - prev).abs() > 1.5 {
                        let ratio = (prev / dist).clamp(0.5, 2.0);
                        zoom.escalar(ratio);
                    }
                }
            }
            pinch.ids = Some((pts[0].0, pts[1].0));
            zoom.pinch_prev = Some(dist);
        } else {
            pinch.ids = None;
            zoom.pinch_prev = None;
        }
    }
}

#[allow(clippy::type_complexity)]
fn follow_player_camera(
    time: Res<Time>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    trans: Res<crate::transition::FlightTransition>,
    region: Res<RegionRuntime>,
    trauma: Res<CameraTrauma>,
    approach: Res<ApproachFraming>,
    mut zoom: ResMut<CameraZoom>,
    mut was_flying: Local<bool>,
    player: Query<&Transform, (With<Player>, Without<GameCamera>)>,
    mut camera: Query<(&mut Transform, &mut OrthographicProjection), With<GameCamera>>,
) {
    let Ok((mut cam_tf, mut proj)) = camera.get_single_mut() else {
        return;
    };

    let flying = fly.flying() || trans.active() || matches!(region.active, RegionId::Space);

    // On entering/leaving a flight, reset the user multiplier so each new
    // flight starts at the automatic framing; on touchdown also pull the
    // absolute zoom back into the surface range for the next time we're on
    // foot (a pinched-far-out space view must not leak into walking).
    if flying != *was_flying {
        // NO se toca `view_h`: despegar y aterrizar ya no reencuadran la cámara.
        *was_flying = flying;
    }

    // ── ZOOM ───────────────────────────────────────────────────────────────
    // **SUBIR TIENE QUE VERSE COMO SUBIR.** En una vista cenital el personaje no se
    // mueve en pantalla al despegar: lo único que puede contar que estás subiendo es
    // que el suelo se ALEJE. Por eso la altura vuelve a mandar en el encuadre, con
    // `universe::view_h_for_alt` — una sola función continua para la subida, la
    // órbita y el descenso, en todos los cuerpos. Que el jugador se dibuje con
    // tamaño de pantalla constante (`sim::interpolate_player_visual`) completa el
    // efecto: vos te quedás, el mundo se achica.
    //
    // (Acá NO había altura desde el 3 ago 2026, y ese fue el error que esto arregla:
    // se la había sacado entera para matar el salto del despegue. Funcionó — no
    // saltaba— pero de paso mató el vuelo: subías al espacio y en pantalla no pasaba
    // absolutamente nada, ni el suelo se iba ni había forma de saber que estabas
    // arriba. El salto no era culpa de que la altura encuadrara, era culpa de CÓMO:
    // ver abajo.)
    //
    // Lo que sí se conserva de aquella pasada, porque era lo correcto:
    //
    // 1. **El pellizco es un MULTIPLICADOR persistente, no un valor absoluto que se
    //    pisa.** El bug viejo no era que la altura encuadrara: era que `trans.view_h`
    //    MACHACABA `zoom.view_h` cada frame mientras el gesto del usuario movía otra
    //    variable distinta, que además se reseteaba a 1.0 al despegar y al aterrizar.
    //    O sea que cruzar un límite cambiaba qué cosa estabas manejando y tiraba la
    //    otra. Ahora hay UN producto, `auto × mult`, y el gesto escribe siempre el
    //    mismo factor: despegar, rebasar de marco y aterrizar no lo tocan.
    // 2. **Nada se guarda de un frame para el otro.** `auto` es una función pura de la
    //    altura, así que bajar deshace subir EXACTAMENTE, sin trinquete posible — que
    //    era el otro bug ("subo y el mundo se aleja; bajo y no pasa nada").
    // 3. **Es continuo en los dos extremos del vuelo**: `view_h_for_alt(b, 0)` es
    //    `SURFACE_VIEW_H` por construcción (hay un test en `universe.rs`), o sea que
    //    a ras de suelo `auto = 1×` y el despegue arranca exactamente donde estaba el
    //    encuadre de a pie. Sumado a que la subida ahora ARRANCA suave
    //    (`creative_fly::THRUST_RAMP_S`) y a que `NEAR_ZOOM_RATE` es 1 m de encuadre
    //    por metro de altura, el "se aleja de golpe" ya no tiene de dónde salir: el
    //    ritmo de reencuadre queda por debajo de ×1/s (lo clava
    //    `el_despegue_no_se_aleja_de_golpe`).
    let alt = if fly.flying() { fly.alt } else { 0.0 };
    zoom.auto_view_h = crate::universe::view_h_for_alt(fly.body, alt);
    // UN SOLO RANGO, VUELE O NO. El rango angosto de superficie (12–45 m) era una cosa
    // que movía la cámara sola: aterrizar con el encuadre abierto lo recortaba a 45 m,
    // o sea un reencuadre de ~8 s que el jugador no pidió. Se puede sacar sin riesgo
    // porque abrir el encuadre a ras NO desborda el streaming: `world2d::MAX_CHUNK_RANGE`
    // es un techo duro del anillo de chunks y el detalle fino ya se funde contra el
    // satelital a partir de `universe::FINE_FADE_VIEW_H` (95 m). O sea que mirar el
    // planeta desde el suelo cuesta lo mismo que mirarlo desde órbita.
    let (min_h, max_h) = (ZOOM_MIN_H, ZOOM_MAX_H);
    let desired_h = desired_frame_h(zoom.auto_view_h, zoom.user_mult, min_h, max_h);
    zoom.view_h = desired_h;

    // ── FOLLOW ─────────────────────────────────────────────────────────────
    // SIEMPRE anclada al jugador. En el marco único (`universe.rs`) el personaje
    // nunca cambia de coordenadas al despegar, así que no hay ningún caso en el
    // que la cámara deba mirar a otro sitio — y no puede volver a pasar que se
    // vaya a un punto donde el jugador no está.
    let Ok(p) = player.get_single() else {
        return;
    };
    let mut target = p.translation.truncate();

    // ENCUADRE DE APROXIMACIÓN: volando, la cámara se adelanta hacia el cuerpo al
    // que vas, hasta un cuarto de pantalla. Sin esto el planeta te queda SIEMPRE
    // a un costado y la aproximación se lee como dar vueltas alrededor de su
    // centro hasta que de golpe crece; con esto lo tenés delante, creciendo, y
    // entrás en él — que es lo que uno espera al volar hacia un mundo.
    if flying {
        let lead = approach.lead;
        if lead.length_squared() > 0.0 {
            let max_lead = desired_h * 0.25;
            target += lead.clamp_length_max(max_lead);
        }
    }

    let blend = (time.delta_secs() * 6.0).clamp(0.0, 1.0);
    let current = cam_tf.translation.truncate();
    let mut next = current.lerp(target, blend);
    // SNAP when essentially arrived — the lerp is asymptotic and would otherwise
    // micro-jitter the camera every frame (reads as the whole world trembling at
    // the huge orbital zoom). Threshold scales with zoom → sub-pixel at any scale.
    let snap_eps = (desired_h * 0.0004).max(0.001);
    if (target - next).length() <= snap_eps {
        next = target;
    }

    if trauma.0 > 0.0 {
        let t = time.elapsed_secs() * 40.0;
        let amp = trauma.0 * trauma.0 * 0.6;
        next += Vec2::new((t * 1.3).sin(), (t * 1.7).cos()) * amp;
    }
    cam_tf.translation = next.extend(100.0);

    let current_h = match proj.scaling_mode {
        ScalingMode::FixedVertical { viewport_height } => viewport_height,
        _ => SURFACE_VIEW_H,
    };
    // SEGUIMIENTO DEL ENCUADRE — en espacio logarítmico y con tope por frame.
    //
    // Antes era `current + (desired - current) * (dt * 4.0).clamp(0,1)`, y tenía dos
    // problemas que sólo se notaban con pocos fps, o sea justo en el celu:
    //
    // 1. **Se apagaba solo.** Con `dt >= 0.25 s` el factor satura en 1.0 y el suavizado
    //    deja de existir: el encuadre salta entero en un frame. En el Redmi a 10 fps el
    //    paso ya era del 40 %.
    // 2. **Era lineal sobre una magnitud multiplicativa.** El zoom se percibe en
    //    proporción (de 100 a 200 se ve igual de brusco que de 1000 a 2000), así que
    //    interpolar linealmente da una curva que se siente despareja.
    //
    // Con eso, el rebase de marco Tierra↔Luna —que cambia `auto_h` de golpe porque el
    // perfil de transición pasa de un cuerpo al otro— se veía como un tirón. Medido con
    // el driver navegando como un jugador (zoom abierto): **x1.79 en un frame**.
    //
    // Ahora: exponencial en log (independiente del frame rate, nunca sobrepasa) y encima
    // un techo duro de cuánto puede cambiar la proporción en UN frame. El techo es lo que
    // garantiza el invariante, incluso si `desired_h` pega un salto enorme; lo peor que
    // pasa es que el encuadre tarde un segundo largo en acomodarse, y un segundo de
    // cámara acomodándose NO es una cutscene: no bloquea nada y seguís volando.
    let dt = time.delta_secs();
    let mut next_h = if current_h > 0.0 && desired_h > 0.0 {
        let k = 1.0 - (-ZOOM_FOLLOW_RATE * dt).exp();
        let paso = (desired_h / current_h).powf(k);
        // El tope se compone por tiempo (no por frame), así que el reencuadre tarda lo
        // mismo en la compu que en el celu; el techo por frame es el que garantiza que
        // igual no se vea un corte cuando `dt` se va al demonio.
        let tope = ZOOM_MAX_STEP_PER_S.powf(dt).min(ZOOM_MAX_STEP_FRAME);
        current_h * paso.clamp(1.0 / tope, tope)
    } else {
        desired_h
    };
    // Same snap for the zoom: stop the perpetual micro-rescale that shimmers text.
    if (desired_h - next_h).abs() <= (desired_h * 0.0004).max(0.001) {
        next_h = desired_h;
    }
    proj.scaling_mode = ScalingMode::FixedVertical {
        viewport_height: next_h,
    };
    zoom.live_view_h = next_h;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universe::{climb_rate, view_h_for_alt};
    use ha_shared::CelestialBody;

    const TIERRA: CelestialBody = CelestialBody::Earth;

    /// El encuadre a una altura, con el multiplicador del jugador puesto.
    fn encuadre(alt: f32, mult: f32) -> f32 {
        desired_frame_h(view_h_for_alt(TIERRA, alt), mult, ZOOM_MIN_H, ZOOM_MAX_H)
    }

    #[test]
    fn subir_aleja_el_suelo() {
        // EL pedido: subir tiene que VERSE. En vista cenital el personaje no se
        // mueve en pantalla, así que si el encuadre no se abre con la altura no hay
        // absolutamente ninguna señal de que estés subiendo — que es exactamente
        // como quedó el juego al sacarle la altura a la cámara.
        let alturas = [0.0f32, 50.0, 400.0, 3_000.0, 20_000.0, 200_000.0];
        let mut prev = 0.0;
        for alt in alturas {
            let f = encuadre(alt, 1.0);
            assert!(f > prev, "a {alt} m el encuadre ({f}) no creció respecto del anterior ({prev})");
            prev = f;
        }
    }

    #[test]
    fn despegar_y_aterrizar_no_dan_un_salto() {
        // A ras de suelo el encuadre automático vale EXACTO el de a pie, así que
        // cruzar el límite vuelo↔pie no puede mover nada. Vale para cualquier
        // multiplicador: volver del espacio y posarse tiene que dejarte con el
        // encuadre que traías, no reencuadrarte de golpe.
        for mult in [0.4f32, 1.0, 3.0, 150.0] {
            let pie = desired_frame_h(SURFACE_VIEW_H, mult, ZOOM_MIN_H, ZOOM_MAX_H);
            let ras = encuadre(0.0, mult);
            assert!((ras - pie).abs() < 1e-3, "salto al despegar/aterrizar: {pie} -> {ras}");
        }
    }

    #[test]
    fn bajar_deshace_subir() {
        // Subir y bajar por la misma escalera tiene que devolver los MISMOS
        // encuadres. Sale de que `auto` es una función PURA de la altura: no hay
        // estado que trinquetee, que era el bug de "subo y el mundo se aleja; bajo y
        // no pasa nada hasta que aparezco de golpe en el suelo".
        let subida = [0.0f32, 50.0, 400.0, 3_000.0, 20_000.0];
        for &mult in &[1.0f32, 6.0] {
            let up: Vec<f32> = subida.iter().map(|&a| encuadre(a, mult)).collect();
            let mut bajada = subida;
            bajada.reverse();
            for (i, &alt) in bajada.iter().enumerate() {
                let f = encuadre(alt, mult);
                let expect = up[up.len() - 1 - i];
                assert!(
                    (f - expect).abs() < 1e-3,
                    "bajando a {alt} m el encuadre es {f}, subiendo era {expect}"
                );
            }
        }
    }

    #[test]
    fn el_pellizco_manda_a_cualquier_altura() {
        // El gesto siempre hace algo, vueles o no: el producto nunca deja de ser
        // proporcional al multiplicador. (Antes el gesto peleaba contra un piso por
        // altura que se lo comía en el espacio.)
        for alt in [0.0f32, 400.0, 50_000.0] {
            let cerca = encuadre(alt, 0.5);
            let normal = encuadre(alt, 1.0);
            let lejos = encuadre(alt, 4.0);
            assert!(cerca < normal && normal < lejos, "a {alt} m el pellizco no encuadra");
        }
    }

    #[test]
    fn el_despegue_no_se_aleja_de_golpe() {
        // EL bug reportado, hecho test: "al darle a volar la cámara se alejaba de
        // golpe". Se simula la subida REAL —la altura la integra `climb_rate`, la
        // misma curva que usa `creative_fly`— y se mide a qué ritmo se reencuadra.
        //
        // Se mide la proporción por SEGUNDO, que es la magnitud que se percibe: que
        // el encuadre se multiplique por 1.6 en un segundo es una subida enérgica;
        // por 11 (lo que hacía antes el primer segundo de vuelo) es un tirón.
        //
        // Y se comprueba por los DOS lados, con tres cotas:
        //
        // - **DESPEGUE** (los primeros 6 s, que es lo que se reportó roto). Acá el
        //   ritmo tiene que ser el de crucero y nada más: `climb_rate` y
        //   `view_h_for_alt` están atadas para que valga ×1.65/s EXACTO a cualquier
        //   altura baja (ver `universe::CLIMB_BASE`). Por eso el techo va tan pegado
        //   al piso: si alguien desata una de las dos, esto se entera.
        // - **VUELO ENTERO**: el pico está en la revelación del planeta (~×2.6/s a
        //   los 12 s, cuando el mundo pasa de "tu mapa" a "un disco"). Que se note
        //   está bien —es el momento— pero tiene que quedar por debajo del techo del
        //   rate limiter (`ZOOM_MAX_STEP_PER_S`) para que sea la CURVA la que manda y
        //   no el recorte.
        // - **PISO**: caza la regresión contraria, la que dejó el juego así. Si nada
        //   se aleja no hay subida: cruzabas la atmósfera y en pantalla no pasaba
        //   absolutamente nada.
        const TECHO_DESPEGUE: f32 = 1.70;
        const PISO_DESPEGUE: f32 = 1.60;
        const TECHO_VUELO: f32 = 3.0;
        // Hasta 15 s: de pisar el pasto a ver la Tierra entera. Más arriba la subida
        // topa contra el tope de `climb_rate` (40 km/s) y el ritmo cae solo — eso es
        // el crucero interplanetario, donde lo que se mueve es el lateral, no la
        // altura.
        const PISO_VUELO: f32 = 1.10;
        let dt = 1.0 / 60.0;
        let mut alt = 0.0f32;
        let mut prev = encuadre(alt, 1.0);
        let (mut peor, mut menor): (f32, f32) = (1.0, f32::MAX);
        let (mut peor_desp, mut menor_desp): (f32, f32) = (1.0, f32::MAX);
        let mut t = 0.0f32;
        while t < 15.0 {
            alt += climb_rate(alt) * dt;
            t += dt;
            let f = encuadre(alt, 1.0);
            let por_s = (f / prev).powf(1.0 / dt);
            prev = f;
            peor = peor.max(por_s);
            menor = menor.min(por_s);
            if t <= 6.0 {
                peor_desp = peor_desp.max(por_s);
                menor_desp = menor_desp.min(por_s);
            }
        }
        assert!(
            peor_desp <= TECHO_DESPEGUE,
            "el despegue reencuadra a ×{peor_desp:.2}/s (techo ×{TECHO_DESPEGUE}/s): la cámara se aleja de golpe"
        );
        assert!(
            menor_desp >= PISO_DESPEGUE,
            "el despegue reencuadra a ×{menor_desp:.2}/s (piso ×{PISO_DESPEGUE}/s): el suelo no se aleja parejo"
        );
        assert!(
            peor <= TECHO_VUELO,
            "hay un tramo del vuelo a ×{peor:.2}/s (techo ×{TECHO_VUELO}/s): eso ya es un corte"
        );
        assert!(
            menor >= PISO_VUELO,
            "hay un tramo del vuelo a ×{menor:.2}/s (piso ×{PISO_VUELO}/s): ahí el suelo no se aleja y no se siente que subís"
        );
    }
}
