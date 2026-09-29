//! Top-down smoke test (HA_AUTODRIVE=1 + HA_AUTOPLAY=1): drives the player
//! through the REAL input pipeline to confirm the game boots, renders the
//! top-down world, and the player walks. Screenshots at key beats.
//!
//! NOTE: the full 11-system verification (dig/chop/craft/machine/ignition/…)
//! from the side-scroller is being re-ported to top-down in a later phase.
//! This lean version keeps the build green and gives a visual check for Fase 2.

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

use crate::input::HoldInteractState;
use crate::inventory_persist::PlayerInventory;
use crate::sim::{Player, PlayerVisual, SimState};
use crate::targeting::LookTarget;
use crate::world2d::TileWorld;

pub struct AutoDrivePlugin;

impl Plugin for AutoDrivePlugin {
    fn build(&self, app: &mut App) {
        // La actuación del driver se baja a `TouchActions` acá, DESPUÉS de que
        // `poll_action_buttons_held` ponga los bits en cero (si fuera antes, los borraría en
        // el mismo frame). Se registra para todos los drivers por igual y en las dos
        // plataformas: ése es el punto de la actuación neutra — el celu y la compu recorren
        // el mismo camino, el del botón de la pantalla.
        app.init_resource::<DriverActuation>().add_systems(
            Update,
            aplicar_actuacion_del_driver
                .after(crate::input::actions::poll_action_buttons_held),
        );
        // Creative-fly visual test: enable creative, hold JUMP, screenshot the
        // whole ascent so we can SEE it (HA_FLYTEST=1 HA_AUTOPLAY=1). Ordered
        // BEFORE the desktop key-reading system: as an exclusive `&mut World`
        // system, `fly_test_drive` is otherwise an unordered sync point
        // relative to it, so a same-frame `press()` could land AFTER that
        // system already read this frame's (stale) key state — the
        // `just_pressed` edge silently missed and never recurring since the
        // key stays "held". Explicit ordering removes that race entirely.
        if crate::flags::var("HA_FLYTEST").is_ok() {
            app.init_resource::<FlyTestState>();
            app.add_systems(
                Update,
                fly_test_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        // Moon creative-fly test: teleport to the Moon, climb, confirm you
        // reach CLEAR orbit (no instant re-teleport loop = the "white square"),
        // then measure the Earth→Moon lateral cruise time (HA_MOONTEST=1).
        if crate::flags::var("HA_MOONTEST").is_ok() {
            app.init_resource::<FlyTestState>();
            app.add_systems(
                Update,
                moon_test_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        // Rocket transition visual test: ignite from the pad, thrust to space,
        // release and fall back to touchdown — screenshots at altitude beats on
        // BOTH legs so the surface↔space choreography can be SEEN
        // (HA_ROCKETTEST=1 HA_AUTOPLAY=1).
        if crate::flags::var("HA_ROCKETTEST").is_ok() {
            app.init_resource::<RocketTestState>();
            app.add_systems(
                Update,
                rocket_test_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        // Universe round-trip test: for several bodies, force orbit→land TWICE
        // and assert the surface state (region/ctx/planet) is coherent after
        // EVERY touchdown — the "land, take off, land again → stuck in the
        // orbital dimension" soft-lock (seen on Venus). HA_CYCLETEST=1.
        // Retrato de una ciudad generada (HA_CITYTEST=1): teletransporta a la
        // ciudad más cercana y captura a tres alturas — calle, barrio y región.
        // Es la verificación VISUAL de la geografía: que haya calles, fachadas y
        // carreteras saliendo hacia los pueblos vecinos, no sólo tests en verde.
        // Retrato NOCTURNO de la iluminación (HA_NIGHTTEST=1): planta al jugador
        // en una ciudad, clava el reloj en medianoche y captura el mismo encuadre
        // a tres zooms. Es el test VISUAL de `light2d` + `night_veil.wgsl`: el
        // gradiente del pozo, el color de la sombra, la nitidez del mundo debajo
        // y la forma de las sombras proyectadas se miran, no se afirman.
        if crate::flags::var("HA_NIGHTTEST").is_ok() {
            app.init_resource::<FlyTestState>();
            app.add_systems(
                Update,
                night_test_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        // Ecología + sociedad SIN ANILLO (HA_ECOTEST=1): planta al jugador en un yermo, en
        // un bosque y en una ciudad, y afirma lo que el diseño promete — el desierto está
        // vacío de fauna Y de gente, el bosque tiene manada, y la gente está en el pueblo.
        // Es el test del cambio "la vida ya no gira alrededor del jugador".
        if crate::flags::var("HA_ECOTEST").is_ok() {
            app.init_resource::<FlyTestState>();
            app.add_systems(
                Update,
                eco_test_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        // **AGUA (HA_AGUATEST=1)**: planta al jugador en una laguna, en un río y
        // al pie de una cascada de verdad, y captura las tres. Es el ojo del
        // cambio "todo cuerpo de agua se ve como la lámina": lo que hay que
        // mirar en las capturas es que la orilla case con el terreno (nada de
        // playa de arena en la sierra), que el fondo hondo no sea un cuadrado
        // azul y que la cascada tenga sus paredes de roca y su poza.
        if crate::flags::var("HA_AGUATEST").is_ok() {
            app.init_resource::<FlyTestState>();
            app.add_systems(
                Update,
                agua_test_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        // **RELIEVE (HA_RELIEVETEST=1)**: la pared del campo, el paso y la
        // ladera urbana. Es el ojo del kit de acantilado del pack — ver
        // `relieve_test_drive` para qué hay que mirar en cada captura.
        if crate::flags::var("HA_RELIEVETEST").is_ok() {
            app.init_resource::<FlyTestState>();
            app.add_systems(
                Update,
                relieve_test_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        if crate::flags::var("HA_BARRIOTEST").is_ok() {
            app.init_resource::<FlyTestState>().init_resource::<BarrioTestState>();
            app.add_systems(
                Update,
                barrio_test_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        // **Mirar un lugar cualquiera** (HA_VISTA="x,z"): va a esa celda, espera el
        // mundo, captura a pie y desde 120 m, y sale. Para revisar un sitio
        // puntual sin escribir un driver entero.
        if crate::flags::var("HA_VISTA").is_ok() {
            app.init_resource::<FlyTestState>();
            app.add_systems(
                Update,
                vista_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        if crate::flags::var("HA_CITYTEST").is_ok() {
            app.init_resource::<FlyTestState>();
            app.add_systems(
                Update,
                city_test_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        // Viaje completo Tierra→Luna→Tierra volando de verdad (HA_TRIPTEST=1):
        // el reporte "salgo al espacio y ya no puedo regresar" como test.
        if crate::flags::var("HA_TRIPTEST").is_ok() {
            app.init_resource::<TripTestState>().init_resource::<FlyTestState>();
            app.add_systems(
                Update,
                trip_test_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        if crate::flags::var("HA_CYCLETEST").is_ok() {
            app.init_resource::<CycleTestState>().init_resource::<FlyTestState>();
            app.add_systems(
                Update,
                cycle_test_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        // Continuous performance/behaviour TESTER (HA_TESTER=1 HA_AUTOPLAY=1): an
        // AI that plays FOREVER through the real input pipeline — wander (streams
        // terrain/decor/fauna, the FPS-critical path), dig, build, then a full Ki
        // takeoff→orbit→land round trip — looping so `ha-perf fps=` samples a real
        // session. Never exits, never soft-locks (each phase self-resets on a
        // timeout). Drops a fresh `tester-live.png` every few seconds and obeys
        // on-demand commands from `tester-cmd.txt` (shot/earth/luna/fly/land) so a
        // human can steer it ("go to the Moon and test X").
        if crate::flags::var("HA_TESTER").is_ok() {
            app.init_resource::<TesterState>();
            app.init_resource::<SmokeState>(); // dig_setup/place_setup write its cell
            app.init_resource::<FlyTestState>(); // hold_with_fresh_edges uses it
            app.add_systems(
                Update,
                tester_drive
                    .before(aplicar_actuacion_del_driver)
                    .after(crate::creative_fly::sync_flight_mirror),
            );
            return;
        }
        // Construcción por BORDES (LDoE): levanta un cuarto real por el mismo
        // camino que el jugador (hold → `apply_build_hold`) y comprueba que el
        // piso queda, que las paredes se pegan a los bordes, que una pared sin
        // piso NO entra y que el muro FRENA al jugador. HA_BUILDTEST=1
        // (correr con HA_FORCE_TOUCH=1: si no, el mouse le pisa la mira).
        if crate::flags::var("HA_BUILDTEST").is_ok() {
            app.init_resource::<BuildTestState>();
            app.add_systems(Update, build_test_drive.before(aplicar_actuacion_del_driver));
            return;
        }
        // **La casa tiene FRENTE.** Levanta un cuarto, le pone la cara exterior con
        // el material de fachada y lo tapa con un tumbado — que es la combinación
        // exacta que estaba rota: el tumbado va a `layers::OVERHEAD` (plano) y el
        // sprite de la pared sur crece justo sobre las celdas que tapa, así que
        // desde la vereda no había casa, había una losa. Comprueba que la cara
        // exterior quede por delante de su propia tapa y deja las dos capturas que
        // hay que MIRAR: la calle y el interior. HA_FACHADATEST=1
        // (con HA_FORCE_TOUCH=1, como el resto de los drivers de obra).
        if crate::flags::var("HA_FACHADATEST").is_ok() {
            app.init_resource::<FachadaTestState>();
            app.add_systems(Update, fachada_test_drive.before(aplicar_actuacion_del_driver));
            return;
        }
        // **La casa MODULAR.** El hermano del anterior para la otra forma de
        // construir: el edificio se coloca entero (bandas de 9 celdas que son un
        // solo dibujo), se amplía, se le apilan plantas y se le abre la puerta.
        // Deja `casa-calle.png` y `casa-adentro.png`. HA_CASATEST=1
        // (con HA_FORCE_TOUCH=1, como el resto de los drivers de obra).
        if crate::flags::var("HA_CASATEST").is_ok() {
            app.init_resource::<CasaTestState>();
            app.add_systems(Update, casa_test_drive.before(aplicar_actuacion_del_driver));
            return;
        }
        // **Entrar en una cueva ANDANDO.** Busca una boca de verdad en el mundo
        // generado, planta al jugador afuera y camina hacia adentro. Comprueba lo
        // único que importa de este sistema: que se entra caminando (nadie
        // teletransporta a nadie) y que el afuera se apaga en GRADIENTE, no de
        // golpe. HA_CUEVATEST=1.
        // **¿Se deja tocar todo?** HA_PROPTEST=1 — ver `prop_test_drive`.
        if crate::flags::var("HA_PROPTEST").is_ok() {
            app.init_resource::<PropTestState>();
            app.add_systems(Update, prop_test_drive.before(aplicar_actuacion_del_driver));
            return;
        }
        if crate::flags::var("HA_CUEVATEST").is_ok() {
            app.init_resource::<CuevaTestState>();
            app.add_systems(Update, cueva_test_drive.before(aplicar_actuacion_del_driver));
            return;
        }
        if crate::flags::var("HA_AUTODRIVE").is_err() {
            return;
        }
        app.init_resource::<SmokeState>()
            .add_systems(Update, smoke_drive);
    }
}

#[derive(Resource, Default)]
struct FlyTestState {
    shots: u32,
    /// Previous frame's camera `view_h`, to catch any jump automatically —
    /// this is exactly the bug that caused the "clouds cut, tiny blue circle
    /// appears" report: a discontinuous zoom at the surface↔orbit swap.
    prev_view_h: f32,
    max_jump_ratio: f32,
    /// El mismo salto pero normalizado por tiempo (ver `zoom_rate_per_s`): es el que se
    /// compara contra el techo, porque el de por-frame depende de los fps.
    max_zoom_rate: f32,
    /// Momento en que se tocó suelo, para dejar que el reencuadre TERMINE antes de juzgar
    /// si la cámara quedó afuera. Cero = todavía no aterrizó.
    ground_at: f32,
    /// Simple stage machine driving JUMP/USE by OBSERVED phase, not fixed
    /// timers, so it actually exercises the climb→orbit→descend→(abort back
    /// to orbit)→descend→touchdown round trip.
    stage: u8,
    stage_entered: f32,
    cleared_at: f32,
    /// While `now < this`, skip zoom-jump comparisons — set right before OUR
    /// OWN synthetic state injections (stage 1/4 force `CreativeFlight`
    /// directly, which is NOT a real gameplay transition). A single-frame
    /// flag isn't reliable here: which frame the camera actually reacts to
    /// the forced state in depends on cross-plugin system ordering, which
    /// isn't guaranteed — a short TIME WINDOW covers it regardless.
    suppress_jump_until: f32,
    last_press_pulse: f32,
}

/// Keep `key` held (so the game reads it as a continuous hold) while
/// periodically releasing-then-repressing it to force a FRESH `just_pressed`
/// edge every `period` seconds. A single press-at-time-T can silently miss
/// its edge if that exact frame is gameplay-blocked (boot/loading screen) —
/// once "stuck held" the edge never recurs since the key never re-transitions.
/// Retrying removes the need to guess how long boot takes (it varies with
/// system load) instead of gating on a fixed timer.
/// Sostiene una acción, como un dedo que no se levanta del botón.
///
/// **Ya no fabrica flancos.** Se llamaba `hold_with_fresh_edges` porque cada `period`
/// segundos soltaba todo y volvía a pulsar: el traductor de teclado exigía `just_pressed`,
/// y escribiendo `ButtonInput` a mano no había flanco que valiera. Ese baile tenía un
/// costo real — un frame de cada N el driver NO estaba pulsando, y con el celu a 13 fps
/// eso es un hueco de 77 ms en un ascenso.
///
/// Con la actuación neutra el flanco lo calcula `aplicar_actuacion_del_driver` comparando
/// con el frame anterior, así que basta con sostener. `now`/`period` se mantienen en la
/// firma porque las etapas los pasan, pero ya no hacen falta.
fn sostener(world: &mut World, a: Accion, _now: f32, _period: f32) {
    actuar(world, a);
}

/// Segundos que se le dan a la etapa 0 para subir hasta encuadrar el planeta.
///
/// **Es una función de la escala del mundo, no un número.** Los 40 s de antes se
/// calibraron con una Tierra de 1.5 km de radio; el cuerpo mide ahora 30 km
/// (`cosmic_scale`, ×20), la meta de la etapa es "subir un radio" y el ascenso es
/// exponencial pero arranca a 30 m/s, así que el mismo vuelo tarda ~60 s y el
/// driver se declaraba STUCK sobre un vuelo perfectamente sano. Con margen: si
/// tarda más del doble, algo se rompió de verdad.
const CLIMB_BUDGET_S: f32 = 100.0;

/// Presupuesto para BAJAR de una órbita alta hasta tocar el suelo.
///
/// Era un `40.0` a mano y fallaba de a ratos, lo cual es peor que fallar siempre: pasó
/// tres corridas seguidas y rompió la cuarta. Los números medidos (`ha-trip-descenso`):
/// el descenso arranca a ~704 km y toca a los **34 s**. Seis segundos de margen.
///
/// Y no es lineal: `climb_rate = 30 + alt*0.5`, así que la bajada **decae
/// exponencialmente** — la mitad del tiempo se va en los últimos kilómetros. Subir la
/// altura de crucero un poco alarga el final bastante más de lo que uno esperaría.
///
/// El factor que de verdad manda es OTRO: `creative_fly` capa el paso a `1/20 s`, así que
/// con pocos fps el mundo avanza más lento que el reloj de pared. En el Redmi a 10 fps
/// esos 34 s de simulación son ~68 s reales, y con 40 el driver no aterrizaba **nunca** en
/// el celu. 120 deja margen para eso y para un device peor.
const LAND_BUDGET_S: f32 = 120.0;

/// Cuánto puede saltar el encuadre de un frame al siguiente sin que se lea como un corte.
///
/// Estaba en 1.5 y por eso el test daba verde mientras el usuario veía el tirón: el rebase
/// Tierra↔Luna medía **x1.09-x1.12** con el zoom cerrado y **x1.79** con el zoom abierto
/// (que es como viaja la gente). Una transición es un cambio de marco, no una cutscene
/// (regla 8): si se NOTA, está mal.
///
/// El número sale de `camera::ZOOM_MAX_STEP_PER_S`, que es quien realmente impone el techo.
/// Va un pelo por encima a propósito: la cámara puede tocar el techo EXACTO, y comparar con
/// `<` contra el mismo valor haría fallar al sistema por hacer justo lo que se le pidió.
///
/// **Se mide POR SEGUNDO, no por frame.** Antes era `1.025` por frame, que es lo mismo que
/// medir en unidades distintas según la máquina: a 60 fps eso permite x3.3 por segundo y a
/// 13 fps sólo x1.38. Con el techo de la cámara ya pasado a por-segundo (ver
/// `camera::ZOOM_MAX_STEP_PER_S`), un tope por frame acá rechazaría en el celu exactamente
/// el reencuadre que en la compu se aprueba. Sigue cazando la regresión que lo motivó: el
/// rebase Tierra↔Luna de x1.79 EN UN FRAME da una tasa por segundo enorme, y además pega
/// contra el techo por frame de abajo.
const MAX_ZOOM_RATE_PER_S: f32 = 3.4;

/// …y el techo por frame, que es el "no se ve un corte" literal: por más lento que vaya el
/// celu, ningún frame puede reencuadrar más que esto. Espeja `camera::ZOOM_MAX_STEP_FRAME`.
const MAX_ZOOM_JUMP_FRAME: f32 = 1.4;

/// Cuánto dura la ventana en la que NO se juzgan saltos de encuadre después de que el
/// driver inyecta una altura a mano.
///
/// Eran 1.6 s, y con la altura mandando otra vez en la cámara se quedaron cortos: el
/// driver teletransporta al jugador de 29 km a 600 m para probar el descenso, o sea un
/// reencuadre de ×200 que la cámara —correctamente— recorre a su ritmo máximo
/// (`camera::ZOOM_MAX_STEP_PER_S`), y eso son ~4.5 s. Al vencer la gracia a los 1.6 s el
/// driver seguía viendo la cámara a tope y lo anotaba como si el JUEGO hubiera pegado un
/// salto. Un teleport de 200× no es una maniobra que un jugador pueda hacer; el
/// invariante vale para todo lo demás de la corrida, que incluye la subida real entera y
/// el descenso real hasta el contacto.
const GRACIA_INYECCION_S: f32 = 6.0;

/// Cuánto se le deja al encuadre para volver al de superficie después de tocar suelo antes
/// de declararlo trabado. El reencuadre es continuo por diseño: pedirle que esté listo en
/// el frame del contacto es pedirle un corte, que es justo lo prohibido (regla 8). Con el
/// techo en x3.3/s, volver de 600 m a 30 m son ~2.7 s; 4 da margen sin tapar un bug real.
const GRACIA_REENCUADRE_S: f32 = 4.0;

/// Tasa de reencuadre por segundo implícita en un cambio `prev → now` que tardó `dt`.
///
/// Es la proporción del frame elevada a `1/dt`: si el encuadre cambió x1.02 en 77 ms, va a
/// ese ritmo x1.29 por segundo. Normalizar por tiempo es lo que hace que la medición valga
/// igual en el Redmi que en la compu.
fn zoom_rate_per_s(prev: f32, now: f32, dt: f32) -> f32 {
    if prev <= 0.0 || now <= 0.0 || dt <= 1e-4 {
        return 1.0;
    }
    let ratio = (now / prev).max(prev / now);
    ratio.powf(1.0 / dt)
}

/// Dónde deja el driver sus capturas y su log. Era una constante con una ruta `/tmp` del
/// escritorio, que en el celu no existe: las capturas se perdían y una corrida en Android
/// no dejaba evidencia. Ahora sale de la plataforma, y `HA_SHOTDIR` la pisa en las dos.
///
/// En Android va al directorio externo PROPIO de la app: no pide permisos (scoped storage),
/// sobrevive a la app y `adb pull` lo baja sin root — que es como se sacan las capturas del
/// Redmi (ver `celu-sin-usb`: el teléfono no está rooteado).
fn flytest_dir() -> &'static str {
    static DIR: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        if let Ok(d) = crate::flags::var("HA_SHOTDIR") {
            return d;
        }
        #[cfg(target_os = "android")]
        {
            "/sdcard/Android/data/com.hungerarena.app/files/ha-qa".to_string()
        }
        #[cfg(not(target_os = "android"))]
        {
            std::env::temp_dir().join("ha-qa").to_string_lossy().into_owned()
        }
    })
}

/// (`biblia.md` §11) Anexa un bug machine-readable (una línea JSON) al
/// archivo `HA_BUGREPORT` (default `bugreport.jsonl`). Cada driver lo llama al
/// romper un invariante; el runner `HA_SUITE` (scripts/ha-suite.mjs) agrega todo
/// para que la IA lea y resuelva. Doble uso: primer formato del log de
/// comportamiento que alimentará la IA colmena de colonos (B3/B4).
pub fn report_bug(mechanic: &str, invariant: &str, detail: &str) {
    use std::io::Write;
    // Sin `HA_BUGREPORT` cae al lado de las capturas, NO al cwd: el cwd de una app Android
    // es `/`, que no es escribible, y el reporte se perdía justo cuando más sirve.
    let path = crate::flags::var("HA_BUGREPORT")
        .unwrap_or_else(|_| format!("{}/bugreport.jsonl", flytest_dir()));
    if let Some(dir) = std::path::Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let line = serde_json::json!({
        "mechanic": mechanic,
        "invariant": invariant,
        "detail": detail,
    })
    .to_string();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
    eprintln!("ha-bug[{mechanic}]: {invariant} — {detail}");
}

fn fly_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    let dt = world.resource::<Time>().delta_secs();
    if now < 1.0 {
        return;
    }
    // Enable creative every frame (so nothing turns it back off).
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;

    // Track the camera zoom every frame and flag any discontinuous jump
    // (ratio vs previous frame) — this is the exact bug class reported.
    // El encuadre REALMENTE en pantalla (no el objetivo): es lo que el jugador
    // ve, y ahora TODO salto cuenta — ya no hay pantalla opaca detrás de la cual
    // esconder un zoom brusco, así que el invariante se aplica siempre.
    let view_h = world.resource::<crate::camera::CameraZoom>().live_view_h;
    let cover_now = world.resource::<crate::transition::FlightTransition>().cover;
    {
        let mut st = world.resource_mut::<FlyTestState>();
        if now < st.suppress_jump_until {
            // Post-injection grace window — track but don't compare.
        } else if st.prev_view_h > 0.0 {
            let ratio = (view_h / st.prev_view_h).max(st.prev_view_h / view_h);
            let rate = zoom_rate_per_s(st.prev_view_h, view_h, dt);
            if ratio > st.max_jump_ratio {
                st.max_jump_ratio = ratio;
            }
            if rate > st.max_zoom_rate {
                st.max_zoom_rate = rate;
            }
            if ratio > MAX_ZOOM_JUMP_FRAME || rate > MAX_ZOOM_RATE_PER_S {
                eprintln!(
                    "ha-flytest: VISIBLE ZOOM JUMP t={now:.2} {:.0} -> {:.0} (x{ratio:.2} en {dt:.3}s = x{rate:.2}/s, cover={cover_now:.2})",
                    st.prev_view_h, view_h
                );
            }
        }
        st.prev_view_h = view_h;
    }

    let phase = world.resource::<crate::creative_fly::CreativeFlight>().phase;
    let stage = world.resource::<FlyTestState>().stage;
    match stage {
        // 0: climb from the ground until Orbit (this leg already proves the
        // zoom-continuity fix via the jump detector above). The boot/loading
        // screen blocks gameplay input for a few seconds of VARIABLE length
        // (depends on system load) — `hold_with_fresh_edges` retries instead
        // of guessing a fixed delay.
        0 => {
            // Keep holding JUMP until we've reached orbit AND CLIMBED CLEAR of
            // the cover into open space (cover≈0). Entering orbit is NOT enough
            // — the bug was being stuck at swap_up under full cloud with no way
            // up. Requiring cover to clear is the real regression test.
            sostener(world, Accion::Saltar, now, 0.4);
            let cover = world.resource::<crate::transition::FlightTransition>().cover;
            let (armed, alt) = {
                let f = world.resource::<crate::creative_fly::CreativeFlight>();
                (f.land_armed, f.alt)
            };
            // Subir hasta un RADIO de altura: ahí el disco de la Tierra ocupa
            // ~45 % de la pantalla, que es el encuadre que hay que ver bien.
            let framed = alt >= ha_shared::body_radius(ha_shared::CelestialBody::Earth);
            if phase == crate::creative_fly::Phase::Orbit && cover < 0.05 && armed && framed {
                // Cleared! Release JUMP to HOVER (no radial movement) and linger
                // ~1.8 s so the disc screenshot lands before the descent tests.
                let cleared = world.resource::<FlyTestState>().cleared_at;
                if cleared == 0.0 {
                    let vh = world.resource::<crate::camera::CameraZoom>().live_view_h;
                    eprintln!("ha-flytest: climbed CLEAR (cover={cover:.2} view_h={vh:.0}) — should see Earth disc");
                    world.resource_mut::<FlyTestState>().cleared_at = now;
                } else {
                    release_all(world); // hover
                    if now - cleared > 0.8 {
                        shot(world, &format!("{}/earth-cleared-disc.png", flytest_dir()));
                    }
                    if now - cleared > 1.8 {
                        advance_stage(world, now);
                    }
                }
            } else if now > CLIMB_BUDGET_S {
                let ki = world.resource::<crate::flight::Flight>().ki;
                eprintln!(
                    "ha-flytest: STUCK — never cleared clouds (cover={cover:.2} phase={phase:?} alt={alt:.0} armed={armed} framed={framed} ki={ki:.1})"
                );
                std::process::exit(1);
            }
        }
        // 1: test setup — jump straight to "mid-descent" (the gameplay path to
        // get here is flying around in orbit for a while and choosing to land
        // far from any body; not worth scripting here). Mirrors exactly what
        // `creative_fly_orbit`'s landing branch does (region/ctx/phase/alt),
        // just at a deliberately mid-band altitude instead of whatever alt the
        // player happened to be at.
        1 => {
            release_all(world);
            {
                let mut fly = world.resource_mut::<crate::creative_fly::CreativeFlight>();
                fly.phase = crate::creative_fly::Phase::Descend;
                fly.vert = 1_500.0;
                fly.alt = 1_500.0;
                fly.body = ha_shared::CelestialBody::Earth;
            }
            // La región NO se toca: la deriva `sync_frame_region` de la altura.
            // This is OUR synthetic teleport (not a real gameplay transition)
            // — don't let it register as a "zoom jump" regression.
            world.resource_mut::<FlyTestState>().suppress_jump_until = now + GRACIA_INYECCION_S;
            eprintln!("ha-flytest: forced mid-descent (alt=150) for the abort test");
            advance_stage(world, now);
        }
        // 2: descend under USE for a beat first.
        //
        // Antes esto soltaba todo y esperaba la "caída por defecto". Esa caída ya
        // no existe: volar anula la gravedad y soltar los botones deja FLOTANDO
        // (pedido del 29 jul 2026). Así que ahora se mantiene USAR, que es la
        // manera de bajar — y de paso el test cubre el control real en vez de un
        // comportamiento automático que el jugador no quería.
        2 => {
            sostener(world, Accion::Usar, now, 0.4);
            if now - world.resource::<FlyTestState>().stage_entered > 1.5 {
                advance_stage(world, now);
            }
        }
        // 3: THE regression test — hold JUMP mid-descent and confirm it
        // actually climbs back to Orbit (used to just arrest the fall at
        // vy=0 — "los controles dejan de servir, no puedo subir").
        3 => {
            sostener(world, Accion::Saltar, now, 0.4);
            if phase == crate::creative_fly::Phase::Orbit {
                eprintln!("ha-flytest: mid-descent abort -> Orbit CONFIRMED");
                advance_stage(world, now);
            } else if now - world.resource::<FlyTestState>().stage_entered > 14.0 {
                // Budget: worst case climbs from ~120 m (after stage 2's 1.5 s
                // default fall from the forced 150 m) to swap_up (360 m on
                // Earth) at VERT_SPEED=32 m/s ≈ 7.5 s — 14 s leaves margin.
                eprintln!("ha-flytest: mid-descent abort FAILED (stuck in Descend)");
                std::process::exit(1);
            }
        }
        // 4: force mid-descent again (one-shot) and this time bajar hasta el suelo
        // con USAR, cubriendo el aterrizaje completo.
        4 => {
            release_all(world);
            {
                let mut fly = world.resource_mut::<crate::creative_fly::CreativeFlight>();
                fly.phase = crate::creative_fly::Phase::Descend;
                fly.vert = 600.0;
                fly.alt = 600.0;
            }
            world.resource_mut::<FlyTestState>().suppress_jump_until = now + GRACIA_INYECCION_S;
            advance_stage(world, now); // one-shot: don't re-force every frame
        }
        // 5: mantener USAR hasta tocar suelo.
        _ => {
            sostener(world, Accion::Usar, now, 0.4);
        }
    }

    // Screenshot beats by observed phase change + a few time-based ones so we
    // still SEE the mid-climb/mid-descent frames, not just the transitions.
    let want = [4.0f32, 6.0, 8.0, 10.0, 11.5, 12.8, 14.5, 17.0, 19.0, 22.0, 26.0, 30.0];
    let shots = world.resource::<FlyTestState>().shots as usize;
    if shots < want.len() && now >= want[shots] {
        let (alt, orbit, phase) = {
            let f = world.resource::<crate::creative_fly::CreativeFlight>();
            (f.alt, f.in_orbit(), f.phase)
        };
        let cloud = world.resource::<crate::transition::FlightTransition>().cover;
        let region = format!("{:?}", world.resource::<crate::scene::RegionRuntime>().active);
        let view_h = world.resource::<crate::camera::CameraZoom>().live_view_h;
        eprintln!(
            "ha-flytest[{shots}] t={now:.1} alt={alt:.0} cloud={cloud:.2} orbit={orbit} phase={phase:?} region={region} view_h={view_h:.0}"
        );
        shot(world, &format!("{}/flytest-{shots}.png", flytest_dir()));
        world.resource_mut::<FlyTestState>().shots += 1;
    }
    if stage >= 5 && world.resource::<crate::creative_fly::CreativeFlight>().phase
        == crate::creative_fly::Phase::Ground
    {
        // GRACIA POST-ATERRIZAJE. El invariante de abajo se evaluaba en el PRIMER frame de
        // `Ground`, y eso preguntaba lo imposible: que el encuadre TELETRANSPORTE al de
        // superficie en el instante del contacto. Justamente lo que no puede pasar, porque
        // el reencuadre es continuo por diseño (regla 8) y tarda su segundo largo. Así
        // reportaba `camera_zoomed_out_after_touchdown view_h=227` mientras la cámara
        // estaba, correctamente, a mitad de camino de 30.
        //
        // Ahora se le da tiempo al reencuadre y se juzga DESPUÉS. Y se loguea el objetivo
        // junto al vivo: si el vivo baja hacia el objetivo, está acomodándose; si el
        // OBJETIVO se quedó arriba, ahí sí la cámara está trabada y es un bug de verdad.
        let ground_at = {
            let mut st = world.resource_mut::<FlyTestState>();
            if st.ground_at == 0.0 {
                st.ground_at = now;
            }
            st.ground_at
        };
        let (live, objetivo) = {
            let z = world.resource::<crate::camera::CameraZoom>();
            (z.live_view_h, z.view_h)
        };
        if now - ground_at < GRACIA_REENCUADRE_S {
            eprintln!(
                "ha-flytest: reencuadrando t+{:.1}s live={live:.0} objetivo={objetivo:.0}",
                now - ground_at
            );
            return;
        }
        let max_jump = world.resource::<FlyTestState>().max_jump_ratio;
        let max_rate = world.resource::<FlyTestState>().max_zoom_rate;
        eprintln!(
            "ha-flytest: max zoom jump = x{max_jump:.2}/frame, x{max_rate:.2}/s (techo x{MAX_ZOOM_RATE_PER_S}/s)"
        );
        let zoom_corta = max_jump > MAX_ZOOM_JUMP_FRAME || max_rate > MAX_ZOOM_RATE_PER_S;
        eprintln!("ha-flytest: {}", if zoom_corta { "ZOOM CONTINUITY FAIL" } else { "ZOOM CONTINUITY PASS" });
        // POST-TOUCHDOWN INVARIANTS — the "stuck after landing" bug class: the
        // camera flew out but the player is left behind / the region or the
        // rocket state is crossed. On the ground everything must be back to the
        // walking baseline.
        let region = format!("{:?}", world.resource::<crate::scene::RegionRuntime>().active);
        let in_space = region.contains("Space");
        let rocket_active = world.resource::<crate::flight::Flight>().active;
        // Ya pasó la gracia: si el encuadre sigue afuera, es que se quedó afuera. Se mira
        // el OBJETIVO además del vivo, porque son bugs distintos — el vivo alto con el
        // objetivo bajo es un reencuadre lento (perf), el objetivo alto es la cámara
        // realmente trabada.
        let camera_stuck_out = live > 200.0 || objetivo > 200.0; // surface view is ~30 m
        let mut fail = false;
        if in_space {
            report_bug("flight", "surface_after_landing", &format!("region still {region}"));
            fail = true;
        }
        if rocket_active {
            report_bug("flight", "rocket_active_in_creative", "landed in creative but Flight.active");
            fail = true;
        }
        if camera_stuck_out {
            report_bug(
                "flight",
                "camera_zoomed_out_after_touchdown",
                &format!("live={live:.0} objetivo={objetivo:.0} tras {GRACIA_REENCUADRE_S:.0}s en el suelo"),
            );
            fail = true;
        }
        eprintln!("ha-flytest: {}", if fail { "LANDING INVARIANTS FAIL" } else { "LANDING INVARIANTS PASS" });
        eprintln!("ha-flytest: DONE");
        std::process::exit(if zoom_corta || fail { 1 } else { 0 });
    }
    // El presupuesto TOTAL cuelga del de la subida: la etapa 0 sola se lleva la
    // mayor parte (subir un radio de la Tierra), y las etapas de descenso
    // arrancan de una altura forzada, así que 60 s más alcanzan de sobra.
    if now > CLIMB_BUDGET_S + 60.0 {
        eprintln!("ha-flytest: TIMEOUT (FAIL, stage={stage})");
        std::process::exit(1);
    }
}

fn advance_stage(world: &mut World, now: f32) {
    let mut st = world.resource_mut::<FlyTestState>();
    st.stage += 1;
    st.stage_entered = now;
}

/// Coloca el vuelo a una altura fija sobre la Luna (determinista, sin deriva de
/// input) para que la captura encuadre el disco siempre igual.
///
/// En el marco único (`universe.rs`) esto es sólo: la Luna es el origen, el
/// jugador está dentro de su disco y la altura vale `alt`. La región la deriva
/// `creative_fly::sync_frame_region`, así que NO se fuerza acá — si hiciera
/// falta forzarla, sería un bug.
/// ¿La cámara ya terminó de reencuadrar? (el vivo alcanzó al objetivo, ±5 %).
///
/// Las etapas que TELETRANSPORTAN al jugador a una altura tienen que esperar esto antes
/// de sacar la foto: el encuadre persigue a su objetivo a un ritmo acotado a propósito
/// (`camera::ZOOM_MAX_STEP_PER_S`), así que un salto de ×100 tarda sus ~4 s. Esperar "1
/// segundo" sacaba la captura a mitad de camino y el log describía algo que la imagen no
/// mostraba.
fn encuadre_asentado(world: &World) -> bool {
    let z = world.resource::<crate::camera::CameraZoom>();
    (z.live_view_h - z.view_h).abs() <= z.view_h * 0.05
}

fn place_creative_orbit(world: &mut World, alt: f32) {
    world.resource_mut::<crate::universe::UniverseFrame>().ref_body =
        ha_shared::CelestialBody::Moon;
    {
        let mut fly = world.resource_mut::<crate::creative_fly::CreativeFlight>();
        fly.body = ha_shared::CelestialBody::Moon;
        fly.phase = crate::creative_fly::Phase::Orbit;
        fly.land_armed = true;
        fly.vert = alt;
        fly.alt = alt;
        fly.vy = 0.0;
        fly.pos = Vec3::ZERO;
    }
    let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
    if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
        sim.body.x = 0.0;
        sim.body.z = 0.0;
        vis.from_x = 0.0;
        vis.to_x = 0.0;
        vis.from_z = 0.0;
        vis.to_z = 0.0;
    }
    let mut intent = world.resource_mut::<crate::input::MovementIntent>();
    intent.move_x = 0.0;
    intent.move_z = 0.0;
}

/// Moon creative-fly regression: teleport to the Moon, climb, confirm you reach
/// CLEAR orbit (no instant re-teleport loop = the "cuadrado blanco"), then
/// measure the capped lateral cruise so travel isn't instant.
fn moon_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 4.0 {
        return;
    }
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
    let stage = world.resource::<FlyTestState>().stage;
    match stage {
        // 0: teleport onto the Moon surface.
        0 => {
            world.resource_mut::<crate::universe::UniverseFrame>().ref_body =
                ha_shared::CelestialBody::Moon;
            world.resource_mut::<crate::scene::RegionRuntime>().active = crate::scene::RegionId::Luna;
            world.resource_mut::<crate::scene::WorldContextRuntime>().active =
                ha_shared::WorldContext::Luna;
            let (sx, sz) =
                crate::scene::walkable_spawn(ha_shared::CelestialBody::Moon, 0.0, 6.0);
            let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
            if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
                sim.body.x = sx;
                sim.body.z = sz;
                sim.body.y = 0.0;
                vis.from_x = sx;
                vis.to_x = sx;
                vis.from_z = sz;
                vis.to_z = sz;
            }
            eprintln!("ha-moontest: teleported to Moon surface");
            advance_stage(world, now);
        }
        // 1: hold JUMP just until you're clear of the fade and armed (NOT all
        // the way up — we want to stay LOW so the Moon disc is big on screen).
        1 => {
            sostener(world, Accion::Saltar, now, 0.4);
            let (phase, body, armed, alt) = {
                let f = world.resource::<crate::creative_fly::CreativeFlight>();
                (f.phase, f.body, f.land_armed, f.alt)
            };
            let cover = world.resource::<crate::transition::FlightTransition>().cover;
            if phase == crate::creative_fly::Phase::Orbit
                && body == ha_shared::CelestialBody::Moon
                && cover < 0.05
                && armed
            {
                eprintln!("ha-moontest: reached CLEAR Moon orbit (no white-square loop, cover={cover:.2} alt={alt:.0})");
                advance_stage(world, now);
            } else if now - world.resource::<FlyTestState>().stage_entered > 30.0 {
                report_bug("moon_climb", "stuck_never_cleared", &format!("cover={cover:.2} phase={phase:?} body={body:?}"));
                std::process::exit(1);
            }
        }
        // 2: place at a FAR orbit altitude (deterministic, no input drift) and
        // screenshot — the Moon should be a clear grey disc, small.
        //
        // 24 000 m ≈ 3 radios lunares. La altura tiene que estar en la ESCALA DEL
        // CUERPO para que el disco se lea como disco: los 700 m de antes valían
        // cuando el encuadre se abría ×2.4 por metro y una altura de edificio ya te
        // sacaba a vista de planeta. Con la escala honesta (1 m de encuadre por
        // metro de altura, `universe::NEAR_ZOOM_RATE`) a 700 m sobre la Luna ves
        // 828 m de regolito — que está BIEN, pero no es lo que este tramo dice
        // probar, y un log que miente es peor que no tenerlo.
        2 => {
            place_creative_orbit(world, 24_000.0);
            if now - world.resource::<FlyTestState>().stage_entered > 1.0 && encuadre_asentado(world) {
                let vh = world.resource::<crate::camera::CameraZoom>().live_view_h;
                eprintln!("ha-moontest: FAR orbit alt=24000 view_h={vh:.0} — Moon disc small");
                shot(world, &format!("{}/moon-disc-far.png", flytest_dir()));
                advance_stage(world, now);
            }
        }
        // 3: place NEAR the Moon — the disc should be MUCH BIGGER (grew as we
        // approached), same Moon.
        3 => {
            place_creative_orbit(world, 6_000.0);
            let dt = now - world.resource::<FlyTestState>().stage_entered;
            if dt > 1.0 && encuadre_asentado(world) && world.resource::<FlyTestState>().cleared_at == 0.0 {
                let vh = world.resource::<crate::camera::CameraZoom>().live_view_h;
                eprintln!("ha-moontest: NEAR orbit alt=6000 view_h={vh:.0} — Moon disc BIG");
                shot(world, &format!("{}/moon-disc-near.png", flytest_dir()));
                world.resource_mut::<FlyTestState>().cleared_at = now;
            }
            // Let the async screenshot observer flush before exiting.
            if world.resource::<FlyTestState>().cleared_at > 0.0 && dt > 2.0 {
                eprintln!("ha-moontest: DONE");
                std::process::exit(0);
            }
        }
        _ => {}
    }
    if now > 60.0 {
        eprintln!("ha-moontest: TIMEOUT (FAIL, stage={stage})");
        std::process::exit(1);
    }
}

#[derive(Resource, Default)]
struct RocketTestState {
    ignited: bool,
    released: bool,
    was_active: bool,
    up_idx: usize,
    down_idx: usize,
    landed_at: Option<f32>,
}

/// Altitude beats to capture going up / coming down (m). The 330 beat sits in
/// the fully-covered swap band; 500/800 show the orbital reveal.
const ROCKET_UP_ALTS: [f32; 6] = [30.0, 150.0, 250.0, 330.0, 500.0, 800.0];
const ROCKET_DOWN_ALTS: [f32; 4] = [500.0, 300.0, 150.0, 50.0];

fn rocket_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    // Wait out the boot screen so the first shots show the world.
    if now < 4.0 {
        return;
    }
    if now > 120.0 {
        eprintln!("ha-rockettest: TIMEOUT (FAIL)");
        std::process::exit(1);
    }
    if !world.resource::<RocketTestState>().ignited {
        // Saiyan Ki-flight: no manual ignition — the thrust loop below holds JUMP
        // (Space), which charges the aura and lifts the character off.
        world.resource_mut::<RocketTestState>().ignited = true;
        eprintln!("ha-rockettest: charging for takeoff");
        return;
    }

    let (active, alt) = {
        let f = world.resource::<crate::flight::Flight>();
        (f.active && f.phase != ha_shared::FlightPhase::Surface, f.altitude_m())
    };
    let (cover, mode) = {
        let t = world.resource::<crate::transition::FlightTransition>();
        (t.cover, t.mode)
    };
    let (ki_alt, ki_phase) = {
        let f = world.resource::<crate::creative_fly::CreativeFlight>();
        (f.alt, f.phase)
    };
    let region = format!("{:?}", world.resource::<crate::scene::RegionRuntime>().active);
    let st = world.resource::<RocketTestState>();
    let (released, up_idx, down_idx, was_active, landed_at) =
        (st.released, st.up_idx, st.down_idx, st.was_active, st.landed_at);

    if active {
        world.resource_mut::<RocketTestState>().was_active = true;
    }

    if !released {
        actuar(world, Accion::Saltar); // thrust
        if up_idx < ROCKET_UP_ALTS.len() && alt >= ROCKET_UP_ALTS[up_idx] {
            eprintln!(
                "ha-rockettest[up-{up_idx}] t={now:.1} alt={alt:.0} ki_alt={ki_alt:.0} ki_phase={ki_phase:?} cover={cover:.2} mode={mode:?} region={region}"
            );
            shot(world, &format!("{}/rockettest-up-{up_idx}.png", flytest_dir()));
            world.resource_mut::<RocketTestState>().up_idx += 1;
        }
        if alt >= 900.0 {
            release_all(world);
            world.resource_mut::<RocketTestState>().released = true;
            eprintln!("ha-rockettest: engine off at {alt:.0} m — descending");
        }
    } else if active {
        // En Ki-flight soltar el empuje NO te hace caer: sin input flotás
        // (`creative_fly_climb` deja `vy = 0`). El descenso es activo — hay que
        // mantener USE (Shift/F) igual que el jugador en el móvil.
        actuar(world, Accion::Usar);
        if down_idx < ROCKET_DOWN_ALTS.len() && alt <= ROCKET_DOWN_ALTS[down_idx] {
            eprintln!(
                "ha-rockettest[down-{down_idx}] t={now:.1} alt={alt:.0} ki_alt={ki_alt:.0} ki_phase={ki_phase:?} cover={cover:.2} mode={mode:?} region={region}"
            );
            shot(world, &format!("{}/rockettest-down-{down_idx}.png", flytest_dir()));
            world.resource_mut::<RocketTestState>().down_idx += 1;
        }
    }

    if was_active && !active {
        match landed_at {
            None => {
                eprintln!(
                    "ha-rockettest: touchdown t={now:.1} region={region} (shots up={up_idx} down={down_idx})"
                );
                shot(world, &format!("{}/rockettest-landed.png", flytest_dir()));
                world.resource_mut::<RocketTestState>().landed_at = Some(now);
            }
            Some(t0) if now - t0 > 1.5 => {
                eprintln!("ha-rockettest: DONE");
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

#[derive(Resource, Default)]
struct SmokeState {
    phase: u32,
    entered: f32,
    start_x: f32,
    cell: (i64, i64),
    done: bool,
    /// Ya se capturó el mapa colgado (la etapa 13 dispara por ESTADO, no por
    /// reloj: en debug el juego corre a 8 FPS y una ventana de tiempo se cuela
    /// entre dos frames).
    map_shot: bool,
}


fn player_cell(world: &mut World) -> (i64, i64) {
    let mut q = world.query_filtered::<&SimState, With<Player>>();
    let s = q.single(world);
    (s.body.x.floor() as i64, s.body.z.floor() as i64)
}

/// El jugador puede no existir todavía (mundo cargando) o estar respawneando.
/// Los drivers deben esperar en vez de panicar con `single()`.
fn player_ready(world: &mut World) -> bool {
    let mut q = world.query_filtered::<&SimState, With<Player>>();
    q.get_single(world).is_ok()
}

/// **¿Está el MUNDO armado alrededor de la cámara?**
///
/// Los drivers que se TELETRANSPORTAN (barrio, ciudad) esperaban dos segundos
/// fijos y disparaban la captura pasara lo que pasara. Dos segundos alcanzan en
/// release y no alcanzan ni de lejos en depuración: el anillo de chunks se
/// reconstruye entero al saltar tres kilómetros, y el driver salía con `DONE` y
/// código 0 dejando **ocho PNG en negro**. O sea: la única herramienta que
/// existe para MIRAR lo que se genera llevaba tiempo diciendo que sí a todo.
///
/// El mundo está listo cuando la cola de chunks se vació ([`ChunkBacklog`], el
/// mismo semáforo que espera la pantalla de carga) y hay al menos un chunk de
/// suelo vivo. Es el mismo criterio que usa `loading.rs`, no uno nuevo.
/// **Planta al jugador en una celda de la SUPERFICIE, sin interpolar.**
///
/// El sim y el visual se escriben de una para que la cámara no cruce media
/// región (mismo criterio que [`teleport_surface`]). Existe para que los tres
/// drivers de ciudad —barrio, ciudad, noche— no repitan el mismo bloque.
///
/// **Y devuelve la planta a la superficie**, que es lo que hacía inútil a este
/// arnés. La partida de trabajo tiene un pozo cavado en el punto de aparición;
/// al arrancar, el jugador se cae dentro (`world2d`, "bajar hasta pisar algo") y
/// `tiles.z` queda en −1. El teletransporte no lo tocaba, así que los drivers de
/// ciudad fotografiaban el SUBSUELO: ocho PNG de tierra negra, `encierro = 1`, y
/// `DONE` con código 0. Un mes de capturas "revisadas" que no mostraban el juego.
fn teleport_a_celda(world: &mut World, tx: f32, tz: f32) {
    {
        let mut tiles = world.resource_mut::<TileWorld>();
        if tiles.z != 0 {
            tiles.z = 0;
            tiles.version = tiles.version.wrapping_add(1);
        }
    }
    let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
    if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
        sim.body.x = tx;
        sim.body.z = tz;
        vis.from_x = tx;
        vis.to_x = tx;
        vis.from_z = tz;
        vis.to_z = tz;
    }
}

fn mundo_listo(world: &mut World) -> bool {
    if world.resource::<crate::world2d::ChunkBacklog>().0 > 0 {
        return false;
    }
    // Y el chunk que PISA el jugador tiene que existir con su decoración
    // completa: la cola vacía sola no basta —el anillo se recorta con el zoom, y
    // un frame después de teletransportarse todavía no hay nada anotado—.
    let Some((px, pz)) = posicion_del_jugador(world) else { return false };
    let lado = crate::world2d::CHUNK as f32 * ha_shared::topdown::TILE_M;
    let (pcx, pcz) = ((px / lado).floor() as i64, (pz / lado).floor() as i64);
    let mut q = world.query::<&crate::world2d::TileChunk>();
    q.iter(world).any(|c| {
        c.cx == pcx && c.cz == pcz && c.decor == crate::world2d::DecorDetail::Full
    })
}

/// `(x, z)` del jugador en metros de mundo, si existe.
fn posicion_del_jugador(world: &mut World) -> Option<(f32, f32)> {
    let mut q = world.query_filtered::<&SimState, With<Player>>();
    q.get_single(world).ok().map(|s| (s.body.x, s.body.z))
}

fn dig_loot(world: &mut World) -> u32 {
    let inv = world.resource::<PlayerInventory>();
    inv.qty("dirt") + inv.qty("stone") + inv.qty("sand")
}

fn equip(world: &mut World, id: &str) {
    // Clear any hotbar selection so `equip_from_hotbar_selection` doesn't
    // override the forced tool next frame.
    world
        .resource_mut::<crate::hud::inventory_ui::InventoryUiState>()
        .selected = None;
    world.resource_mut::<crate::input::EquippedItem>().item_id = id.to_string();
}

fn dig_setup(world: &mut World, cell: (i64, i64)) {
    equip(world, "hand");
    world.resource_mut::<SmokeState>().cell = cell;
}

fn place_setup(world: &mut World, cell: (i64, i64)) {
    world.resource_mut::<PlayerInventory>().add("stone", 5);
    equip(world, "stone");
    world.resource_mut::<SmokeState>().cell = cell;
}

fn lock_block_target(world: &mut World, tx: i64, tz: i64) {
    *world.resource_mut::<LookTarget>() = LookTarget {
        kind: Some(ha_shared::InteractTargetKind::Block),
        block_cell: Some((tx as i32, tz as i32, 0)),
        locked: true,
        ..default()
    };
}

/// Igual que `lock_block_target`, pero apuntando a CAVAR: el mismo gesto sobre
/// el suelo saca el piso y abre el pozo (`TileWorld::dig_down`).
fn lock_dig_target(world: &mut World, tx: i64, tz: i64) {
    *world.resource_mut::<LookTarget>() = LookTarget {
        kind: Some(ha_shared::InteractTargetKind::Block),
        block_cell: Some((tx as i32, tz as i32, 0)),
        dig_down: true,
        locked: true,
        ..default()
    };
}

fn lock_place_target(world: &mut World, tx: i64, tz: i64) {
    *world.resource_mut::<LookTarget>() = LookTarget {
        place_cell: Some((tx as i32, tz as i32, 0)),
        locked: true,
        ..default()
    };
}

fn complete_hold(world: &mut World, placing_block: bool) {
    let mut h = world.resource_mut::<HoldInteractState>();
    h.active = false;
    h.completed = true;
    h.placing_block = placing_block;
    h.placing_machine = false;
}

fn unlock(world: &mut World) {
    *world.resource_mut::<LookTarget>() = LookTarget::default();
    let mut h = world.resource_mut::<HoldInteractState>();
    h.completed = false;
    h.placing_block = false;
}


// ---------------------------------------------------------------------------
// HA_NIGHTTEST — retrato nocturno: el test visual del sistema de luces
// ---------------------------------------------------------------------------

/// Planta al jugador en la ciudad más cercana, clava el reloj en la noche
/// cerrada y captura el mismo sitio a tres zooms. `HA_NIGHTPHASE=<0..1>` mueve
/// la hora (0 = medianoche) por si hace falta mirar el crepúsculo.
fn night_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 3.0 {
        return;
    }
    // El reloj se CLAVA cada frame: un ciclo dura una hora, y sin fijarlo las tres
    // capturas saldrían a horas distintas y no se podrían comparar entre sí.
    let phase = crate::flags::var("HA_NIGHTPHASE")
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.02);
    world.resource_mut::<crate::sky2d::DayClock>().t =
        crate::sky2d::DAY_SECS * (3.0 + phase);
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
    let stage = world.resource::<FlyTestState>().stage;
    let body = ha_shared::CelestialBody::Earth;

    if stage == 0 {
        let mut best: Option<(i64, (i64, i64))> = None;
        for sx in -25..25 {
            for sz in -25..25 {
                let Some(s) = ha_shared::geography::settlement_in(body, sx, sz) else { continue };
                if s.tier != ha_shared::geography::SettlementTier::City {
                    continue;
                }
                let d = s.center.0.abs() + s.center.1.abs();
                if best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, s.center));
                }
            }
        }
        let Some((_, (cx, cz))) = best else {
            eprintln!("ha-nighttest: NO HAY NINGUNA CIUDAD");
            std::process::exit(1);
        };
        teleport_a_celda(world, cx as f32, cz as f32);
        eprintln!("ha-nighttest: ciudad en ({cx}, {cz}) — fase {phase}");
        advance_stage(world, now);
        return;
    }

    // Mismo sitio, tres encuadres: la cara del jugador contra una antorcha, la
    // manzana entera (para ver cómo se funden varios pozos) y el barrio.
    let beats: [(f32, f32, &str); 3] = [
        (3.0, 0.0, "night-0-close"),
        (6.0, 60.0, "night-1-block"),
        (9.0, 260.0, "night-2-district"),
    ];
    let shots = world.resource::<FlyTestState>().shots as usize;
    if shots >= beats.len() {
        // la captura se escribe frames después de pedirla (ver `city_test_drive`)
        let entered = world.resource::<FlyTestState>().stage_entered;
        if now - entered < beats[beats.len() - 1].0 + 15.0 {
            return;
        }
        eprintln!("ha-nighttest: DONE");
        std::process::exit(0);
    }
    let (at, alt, name) = beats[shots];
    let entered = world.resource::<FlyTestState>().stage_entered;
    {
        let mut fly = world.resource_mut::<crate::creative_fly::CreativeFlight>();
        if alt > 0.0 {
            fly.phase = crate::creative_fly::Phase::Climb;
            fly.vert = alt;
            fly.alt = alt;
            fly.vy = 0.0;
        }
    }
    // Igual que la ciudad: con el mundo armado, o sale la pantalla de carga.
    if now - entered > at && mundo_listo(world) {
        // Cuántas sombras hay VIVAS. Es la diferencia entre "no se ven las
        // sombras porque son sutiles" y "el sistema no está colgando ninguna":
        // sin este número se tunea a ciegas el alpha de algo que no existe.
        let n = world.query::<&crate::light2d::GroundShadow>().iter(world).count();
        let sky = *world.resource::<crate::sky2d::SkyLight>();
        // El tamaño REAL de una sombra cualquiera: sin esto se tunea a ciegas
        // (una sombra puede existir, estar bien orientada y medir 4 px).
        let cam_p = world
            .query_filtered::<&GlobalTransform, With<crate::camera::GameCamera>>()
            .iter(world)
            .next()
            .map(|g| g.translation())
            .unwrap_or_default();
        let mut q = world.query_filtered::<
            (&Sprite, &GlobalTransform, &ViewVisibility),
            With<crate::light2d::GroundShadow>,
        >();
        let mut best: Option<(f32, Vec2, Vec3, bool, [f32; 4])> = None;
        for (sp, gt, vv) in q.iter(world) {
            let t = gt.translation();
            let d = t.truncate().distance(cam_p.truncate());
            if best.map_or(true, |(bd, ..)| d < bd) {
                let c = sp.color.to_srgba();
                best = Some((
                    d,
                    sp.custom_size.unwrap_or_default(),
                    t,
                    vv.get(),
                    [c.red, c.green, c.blue, c.alpha],
                ));
            }
        }
        eprintln!("ha-nighttest: sombra más cercana = {best:?}");
        let sz = best.map(|b| b.1);
        eprintln!(
            "ha-nighttest: {name} — sombras={n} sky(str={:.2} elev={:.2} dir={:.2},{:.2}) size={:?}",
            sky.strength, sky.elevation, sky.dir.x, sky.dir.y, sz
        );
        shot(world, &format!("{}/{name}.png", flytest_dir()));
        world.resource_mut::<FlyTestState>().shots += 1;
    }
}

// ---------------------------------------------------------------------------
// HA_ECOTEST — el mundo vive por su cuenta (se fue el anillo de fauna/NPCs)
// ---------------------------------------------------------------------------

/// Teletransporta al jugador a `(x, z)` de la superficie actual. Mismo camino que usan
/// `HA_CITYTEST`/`HA_NIGHTTEST`: se escribe el estado del sim y el visual de una vez para
/// que la cámara no interpole media región.
fn teleport_surface(world: &mut World, x: f32, z: f32) {
    let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
    if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
        sim.body.x = x;
        sim.body.z = z;
        vis.from_x = x;
        vis.to_x = x;
        vis.from_z = z;
        vis.to_z = z;
    }
}

/// **Los tres sitios de agua que hay que mirar**, buscados en el mundo de
/// verdad: una laguna (agua quieta con orilla), un cauce de río y una cascada.
///
/// No son coordenadas escritas a mano: se barre el planeta hasta encontrar cada
/// cosa. Una constante puesta a ojo deja de apuntar a un lago en cuanto alguien
/// toca el ruido del relieve, y el test seguiría "pasando" mirando un prado.
/// **LA ISLA CRUZADA POR UN RÍO.** Es la captura que el usuario tachó con una X:
/// *"hay una isla y una cascada que cruza toda la isla; no da la impresión de que
/// caiga de lo alto a lo bajo, parece que cruza de un extremo al otro"*.
///
/// Se busca tierra con cauce cuya cruz de tierra firme mida menos de 40 tiles en
/// las dos direcciones: eso es una isla, no una costa. No son coordenadas a mano
/// por lo de siempre — si alguien toca el entramado de cauces, un número escrito
/// deja de apuntar a una isla y la captura seguiría "pasando" mirando el mar.
fn isla_con_rio_pick_site() -> Option<(f32, f32)> {
    let body = ha_shared::CelestialBody::Earth;
    let tierra = |tx: i64, tz: i64| ha_shared::relief::relief_level(body, tx, tz) >= 0;
    let corrida = |tx: i64, tz: i64, dx: i64, dz: i64| {
        (1..40).take_while(|d| tierra(tx + dx * d, tz + dz * d)).count() as i64
    };
    // **Y la que se busca es la que TIENE RELIEVE.** Una islita de arena a ras de
    // mar no enseña nada del problema: lo que hay que mirar es el río bajando una
    // isla con escalones, que es donde la cadena de saltos se lee como una sola
    // cinta de agua cruzando de punta a punta.
    let mut llana: Option<(f32, f32)> = None;
    for (cx, cz) in [(3_381i64, -966i64), (0, 0), (2_400, -1_800), (-3_100, 900), (1_500, 3_300)] {
        for tz in cz - 120..cz + 120 {
            for tx in cx - 120..cx + 120 {
                if !tierra(tx, tz) || !ha_shared::geography::hay_rio(body, tx, tz) {
                    continue;
                }
                let ancho = corrida(tx, tz, 1, 0) + corrida(tx, tz, -1, 0);
                let alto = corrida(tx, tz, 0, 1) + corrida(tx, tz, 0, -1);
                if !((6..40).contains(&ancho) && (6..40).contains(&alto)) {
                    continue;
                }
                let con_escalon = (-8..8).any(|d| {
                    ha_shared::relief::alto_de_pared(body, tx, tz + d) > 0
                        || ha_shared::relief::relief_level(body, tx, tz + d) >= 1
                });
                if con_escalon {
                    return Some((tx as f32, tz as f32));
                }
                llana.get_or_insert((tx as f32, tz as f32));
            }
        }
    }
    llana
}

fn agua_pick_sites() -> [(f32, f32, &'static str); 7] {
    use ha_shared::agua::{cascada_at, sitio_de_agua, SitioDeAgua};
    let body = ha_shared::CelestialBody::Earth;
    let mut laguna = None;
    let mut rio = None;
    let mut salto = None;
    let mut playa = None;
    // Ventanas repartidas: los cauces y las cuencas se miden en miles de tiles y
    // un cuadrado alrededor del origen puede caer entero en secano.
    'busca: for (cx, cz) in [(0i64, 0i64), (2_400, -1_800), (-3_100, 900), (1_500, 3_300)] {
        for tz in cz - 220..cz + 220 {
            for tx in cx - 220..cx + 220 {
                if salto.is_none() && cascada_at(body, tx, tz).is_some() {
                    // Dos tiles por debajo del labio: encuadra el chorro entero.
                    salto = Some((tx as f32 + 1.5, tz as f32 + 2.5));
                }
                match sitio_de_agua(body, tx, tz) {
                    Some(SitioDeAgua::Remanso) if laguna.is_none() => {
                        laguna = Some((tx as f32, tz as f32))
                    }
                    Some(SitioDeAgua::Corriente) if rio.is_none() => {
                        rio = Some((tx as f32, tz as f32))
                    }
                    // La PLAYA: orilla de mar con arena. Es la familia
                    // `tiles/water_arena` en su sitio, y lo que hay que mirar es
                    // que no haya un farallón de roca con el agua al pie.
                    Some(SitioDeAgua::Ribera)
                        if playa.is_none()
                            && ha_shared::agua::orilla_de(body, tx, tz)
                                == ha_shared::agua::Orilla::Arena =>
                    {
                        playa = Some((tx as f32, tz as f32))
                    }
                    _ => {}
                }
                if laguna.is_some() && rio.is_some() && salto.is_some() && playa.is_some() {
                    break 'busca;
                }
            }
        }
    }
    let l = laguna.unwrap_or((0.0, 0.0));
    let r = rio.unwrap_or(l);
    let s = salto.unwrap_or(r);
    let p = playa.unwrap_or(l);
    let m = sierra_pick_site().unwrap_or(s);
    let a = afluente_pick_site().unwrap_or(l);
    let i = isla_con_rio_pick_site().unwrap_or(p);
    [
        (l.0, l.1, "laguna"),
        (r.0, r.1, "rio"),
        (s.0, s.1, "cascada"),
        (p.0, p.1, "playa"),
        (m.0, m.1, "sierra"),
        (a.0, a.1, "afluente"),
        (i.0, i.1, "isla"),
    ]
}

/// **DONDE UN RÍO ENTRA A UN LAGO.** Es el sitio que no existía: medido antes de
/// tocarlo (`medir_rios::los_lagos_y_sus_afluentes`), **25 de 36 lagunas no
/// tenían un solo cauce tocando su orilla**, y las cabeceras que se seguían
/// aguas abajo morían todas en seco.
///
/// Lo que hay que mirar en la captura no lo puede comprobar ningún assert: que la
/// desembocadura se LEA como una desembocadura —el cauce que se ensancha, la
/// ribera que sigue de largo sin cortar el río en dos— y no como dos manchas de
/// agua que se tocan de casualidad.
///
/// El lago se toma de la plantilla (`world_map::pond_in`), no del ruido: así el
/// driver sigue apuntando a un lago aunque cambien los cauces.
fn afluente_pick_site() -> Option<(f32, f32)> {
    let body = ha_shared::CelestialBody::Earth;
    for rz in -4..4 {
        for rx in -4..4 {
            let Some(((cx, cz), r)) = ha_shared::world_map::pond_in(body, rx, rz) else {
                continue;
            };
            if ha_shared::relief::relief_level(body, cx, cz) >= 0 {
                continue; // laguna que no llegó a llenarse
            }
            // La orilla, anillo por anillo hacia afuera: la primera celda seca
            // con cauce es la boca de un afluente.
            for k in 0..40i64 {
                let d = r as i64 + k;
                for a in (0..360).step_by(3) {
                    let ang = a as f32 * std::f32::consts::PI / 180.0;
                    let tx = cx + (d as f32 * ang.cos()) as i64;
                    let tz = cz + (d as f32 * ang.sin()) as i64;
                    if ha_shared::relief::relief_level(body, tx, tz) < 0 {
                        continue;
                    }
                    if ha_shared::geography::hay_rio(body, tx, tz) {
                        return Some((tx as f32, tz as f32));
                    }
                }
            }
        }
    }
    None
}

/// **El torrente de montaña**: una cascada de varias filas, con orilla de piedra
/// y paredes de terraza alrededor. Es el sitio que el resto del barrido no puede
/// encontrar, porque las cordilleras no están cerca del origen — las ventanas de
/// [`agua_pick_sites`] caen todas en llanura, y ahí las cascadas miden un
/// escalón y la ribera es de pasto.
///
/// Se busca en dos pasadas para no barrer el planeta entero tile a tile: primero
/// una rejilla rala hasta dar con terreno alto, después el cuadro de esa cumbre
/// completo. La coordenada no se escribe a mano a propósito: una constante deja
/// de apuntar a una montaña en cuanto alguien toca el ruido del relieve, y el
/// test seguiría "pasando" mirando un prado.
fn sierra_pick_site() -> Option<(f32, f32)> {
    use ha_shared::agua::cascada_at;
    let body = ha_shared::CelestialBody::Earth;
    let mut mejor: Option<(i64, i64, i64)> = None;
    let mut cz = -16_000i64;
    while cz < 16_000 {
        let mut cx = -16_000i64;
        while cx < 16_000 {
            if ha_shared::relief::relief_level(body, cx, cz) >= 6 {
                for tz in cz - 90..cz + 90 {
                    for tx in cx - 90..cx + 90 {
                        // La cascada tiene que estar ARRIBA, no en la
                        // desembocadura: buscando sólo "salto de dos filas" el
                        // barrido salía por el primero que encontraba, que
                        // estaba a nivel del mar dentro del mismo cuadro.
                        if ha_shared::relief::relief_level(body, tx, tz) < 5 {
                            continue;
                        }
                        // **Y se queda con el salto MÁS ALTO** del cuadro: desde
                        // la escalera de `relief::SALTO_DE_TERRAZA` los saltos
                        // van de uno a diez pisos, y capturar el primero que
                        // aparece volvería a enseñar el chorro de un tile de
                        // siempre — que es justamente lo que había que arreglar.
                        if let Some(pisos) = cascada_at(body, tx, tz) {
                            if mejor.is_none_or(|(p, _, _)| pisos > p) {
                                mejor = Some((pisos, tx, tz));
                            }
                        }
                    }
                }
            }
            // El cuadro entero mirado: si tenía algún salto, se captura el más
            // alto y no se sigue barriendo el planeta.
            if let Some((pisos, tx, tz)) = mejor {
                eprintln!("ha-aguatest: el salto más alto del cuadro baja {pisos} pisos");
                return Some((tx as f32 + 1.5, tz as f32 + 2.5 + pisos as f32 * 0.5));
            }
            cx += 900;
        }
        cz += 900;
    }
    None
}

#[derive(Resource, Clone, Copy)]
struct AguaSites([(f32, f32, &'static str); 7]);

/// Planta al jugador en los tres sitios de agua y captura cada uno.
///
/// **Invariante**: los tres tienen que EXISTIR. Si el barrido no encuentra una
/// cascada, o no las hay en el mundo o la regla que las deriva se rompió, y en
/// los dos casos el arte animado de cascada no lo ve nadie — que es exactamente
/// el estado del que venimos.
fn agua_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 3.0 {
        return;
    }
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
    let stage = world.resource::<FlyTestState>().stage;
    let entered = world.resource::<FlyTestState>().stage_entered;

    if stage == 0 {
        let sitios = agua_pick_sites();
        for (x, z, que) in sitios {
            eprintln!("ha-aguatest: {que} en ({x:.0}, {z:.0})");
        }
        world.insert_resource(AguaSites(sitios));
        teleport_surface(world, sitios[0].0, sitios[0].1);
        advance_stage(world, now);
        return;
    }
    // El streaming de chunks y la animación del agua necesitan unos segundos:
    // capturar antes deja la laguna a medio poblar y la foto no prueba nada.
    if now - entered < 6.0 {
        return;
    }
    let sitios = world.resource::<AguaSites>().0;
    if stage >= 1 && stage as usize <= sitios.len() {
        let (x, z, que) = sitios[stage as usize - 1];
        let at = player_xz(world);
        eprintln!(
            "ha-aguatest: {que} pedido ({x:.0},{z:.0}) jugador ({:.0},{:.0})",
            at.0, at.1
        );
        shot(world, &format!("{}/agua-{}-{que}.png", flytest_dir(), stage - 1));
        if (stage as usize) < sitios.len() {
            let (nx, nz, _) = sitios[stage as usize];
            teleport_surface(world, nx, nz);
        }
        advance_stage(world, now);
        return;
    }
    eprintln!("ha-aguatest: DONE");
    std::process::exit(0);
}

// ---------------------------------------------------------------------------
// HA_RELIEVETEST — el acantilado, la rampa y la ladera urbana
// ---------------------------------------------------------------------------

/// **Los tres sitios de relieve que hay que mirar**, buscados en el mundo de
/// verdad: una pared que mira al sur en el campo, un paso (rampa) y una ladera
/// DENTRO de un casco urbano —la que lleva escalera, caño y reja—.
///
/// No son coordenadas escritas a mano por el mismo motivo que las del agua: en
/// cuanto alguien toca el ruido del relieve, una constante a ojo deja de apuntar
/// a un acantilado y el test seguiría "pasando" mirando un prado.
///
/// El casco es lo caro de los tres (`settlement_covering` barre nueve celdas de
/// sitio), así que sólo se pregunta por celdas que YA son cara sur.
fn relieve_pick_sites() -> [(f32, f32, &'static str); 6] {
    use ha_shared::relief::{alto_de_pared, relief_cell, ReliefCell};
    let body = ha_shared::CelestialBody::Earth;
    // **La pared que se captura es la MÁS ALTA que se encuentre**, no la primera.
    // Es todo el sentido del rediseño: si el muestreo se queda con el primer
    // bordillo que aparece, la captura sigue enseñando el mundo plano de antes y
    // nadie se entera de si el farallón de diez pisos se dibuja bien.
    let mut pared: Option<(i64, i64, i64)> = None;
    // **LA PARED DE PASTO, aparte de la más alta.** La más alta del mundo cae
    // siempre en la sierra nevada, así que el driver retrataba cinco veces la
    // nieve y `cliff/pasto` —el acantilado más común del mapa— no se miraba
    // nunca. Se guarda la más alta cuyo bioma sea verde, que es donde se juzga
    // la costura entre la meseta y el relleno (ver `ONDA_DE_DUNA` en
    // `import-acantilado.py`).
    let mut pared_verde: Option<(i64, i64, i64)> = None;
    let mut rampa = None;
    let mut urbano = None;
    // Ventanas repartidas, **con cordillera**: un cuadrado alrededor del origen
    // cae entero en llanura (medido: ahí ninguna cara pasa de tres escalones).
    for (cx, cz) in [
        (1_900i64, -4_900i64),
        (4_092, -4_960),
        (-3_450, 3_150),
        (0, 0),
        (2_400, -1_800),
        (1_500, 3_300),
    ] {
        for tz in cz - 200..cz + 200 {
            for tx in cx - 200..cx + 200 {
                match relief_cell(body, tx, tz) {
                    ReliefCell::Ramp if rampa.is_none() => {
                        // **El canto del paso, no una celda cualquiera del
                        // corredor**, y con desnivel de verdad: es donde se ve si
                        // la subida está abierta.
                        //
                        // `alto_de_pared` no sirve para encontrarlo — sólo
                        // contesta en caras que miran al sur, y ahí las rampas
                        // están prohibidas (`relief::cara_al_sur`). Se mide el
                        // desnivel contra la vecina más baja, que es lo que el
                        // paso tiene que salvar.
                        if !ha_shared::relief::canto_de_terraza(body, tx, tz) {
                            continue;
                        }
                        let l = ha_shared::relief::relief_level(body, tx, tz);
                        let salto = [(1i64, 0i64), (-1, 0), (0, -1)]
                            .into_iter()
                            .map(|(dx, dz)| ha_shared::relief::relief_level(body, tx + dx, tz + dz))
                            .filter(|&n| n >= 0 && n < l)
                            .map(|n| {
                                ha_shared::relief::altura_de_terraza(l)
                                    - ha_shared::relief::altura_de_terraza(n)
                            })
                            .max()
                            .unwrap_or(0);
                        if salto >= 2 {
                            rampa = Some((tx as f32, tz as f32 + 3.0));
                        }
                    }
                    ReliefCell::Cliff => {
                        // Cara sur y con pared de verdad: un bordillo que se
                        // trepa no enseña nada de lo que hay que mirar.
                        let alto = alto_de_pared(body, tx, tz);
                        if alto < 2 {
                            continue;
                        }
                        if pared.is_none_or(|(a, _, _)| alto > a) {
                            pared = Some((alto, tx, tz));
                        }
                        if matches!(
                            ha_shared::geography::biome_at(body, tx, tz),
                            ha_shared::geography::Biome::Grassland
                                | ha_shared::geography::Biome::Forest
                                | ha_shared::geography::Biome::Savanna
                                | ha_shared::geography::Biome::Swamp
                                | ha_shared::geography::Biome::Highland
                        ) && pared_verde.is_none_or(|(a, _, _)| alto > a)
                        {
                            pared_verde = Some((alto, tx, tz));
                        }
                        if urbano.is_none()
                            && ha_shared::geography::settlement_covering(body, tx, tz).is_some()
                        {
                            urbano = Some((tx as f32, tz as f32 + 3.0));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if let Some((alto, _, _)) = pared {
        eprintln!("ha-relievetest: la pared más alta encontrada mide {alto} escalones");
    }
    let d = (0.0, 0.0);
    // La cámara se planta unas filas MÁS AL SUR cuanto más alto es el muro: si no,
    // la roca ocupa la pantalla entera y no se ve dónde empieza ni dónde termina.
    let pared_xy = pared
        .map(|(alto, tx, tz)| (tx as f32, tz as f32 + alto as f32 + 4.0))
        .unwrap_or(d);
    if let Some((alto, _, _)) = pared_verde {
        eprintln!("ha-relievetest: la pared de PASTO más alta mide {alto} escalones");
    }
    let pared_verde_xy = pared_verde
        .map(|(alto, tx, tz)| (tx as f32, tz as f32 + alto as f32 + 4.0))
        .unwrap_or(pared_xy);
    // **LA CUMBRE**, que no se busca por ruido: la declara la plantilla del
    // mundo (`world_map::cumbres`). Se captura el monte más alto de los tres, y
    // ahí es donde hay que mirar la nieve y el sendero que sube.
    let cumbre = ha_shared::world_map::cumbres(body)
        .into_iter()
        .map(|(n, (cx, cz))| (ha_shared::relief::relief_level(body, cx, cz), n, cx, cz))
        .max_by_key(|&(l, _, _, _)| l);
    if let Some((l, n, cx, cz)) = cumbre {
        eprintln!("ha-relievetest: la cumbre más alta es {n} ({cx},{cz}), escalón {l}");
    }
    let cumbre_xy = cumbre.map(|(_, _, cx, cz)| (cx as f32, cz as f32 + 10.0)).unwrap_or(d);
    // **LA LÍNEA DE NIEVE**, que es lo único que no se ve en ninguna de las otras
    // cuatro: la cumbre es toda blanca y la pared está por debajo. Se busca la
    // celda de nieve que tenga tierra SIN nieve a la vista, o sea la frontera —
    // que es donde se juzga si el borde es una curva de nivel o una greca
    // (`geography::pasa_la_linea_de_nieve`). Bajando desde la cumbre, que es el
    // sitio del mundo donde la frontera existe seguro.
    let nieve_xy = cumbre
        .and_then(|(_, _, cx, cz)| {
            let nieva = |x: i64, z: i64| {
                ha_shared::geography::biome_at(body, x, z) == ha_shared::geography::Biome::Snow
            };
            // Se baja en línea recta hasta salir de la nieve, y se retrata el
            // punto justo por encima de donde se salió.
            (0..1_200).find_map(|d| {
                let z = cz + d;
                (!nieva(cx, z) && nieva(cx, z - 12)).then_some((cx as f32, z as f32 - 6.0))
            })
        })
        .unwrap_or(cumbre_xy);
    [
        (pared_xy.0, pared_xy.1, "pared"),
        (pared_verde_xy.0, pared_verde_xy.1, "pared-de-pasto"),
        (rampa.unwrap_or(d).0, rampa.unwrap_or(d).1, "rampa"),
        (urbano.unwrap_or(d).0, urbano.unwrap_or(d).1, "urbano"),
        (cumbre_xy.0, cumbre_xy.1, "cumbre"),
        (nieve_xy.0, nieve_xy.1, "linea-de-nieve"),
    ]
}

#[derive(Resource, Clone, Copy)]
struct RelieveSites([(f32, f32, &'static str); 6]);

/// Planta al jugador en los tres sitios de relieve y captura cada uno.
///
/// Lo que hay que MIRAR en las capturas (ninguna de estas cosas la puede
/// comprobar un assert, y todas fueron bugs de verdad):
///
/// - la celda del acantilado es TERRAZA —pasto, arena, nieve— y no un cuadrado
///   gris pardo (`world2d::suelo_del_relieve`);
/// - el de pasto se ve de pasto y el de arena de arena, no el mismo dibujo con
///   una tira verde de cuatro píxeles arriba;
/// - la rampa tiene sus dos espolones de roca, uno a cada lado;
/// - la ladera urbana lleva escaleras y caños, y la del campo NO;
/// - la CUMBRE es blanca —`tiles/ice` con la losa teñida de `atlas-suelo.py`— y
///   su acantilado también (`cliff/nieve`), sin una franja verde en el medio; y
///   se le ve el sendero subiendo (`world_map::sendero_de_cumbre`), que en el
///   dibujo es una rampa cada tantos metros, no una pared cerrada.
fn relieve_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 3.0 {
        return;
    }
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
    let stage = world.resource::<FlyTestState>().stage;
    let entered = world.resource::<FlyTestState>().stage_entered;

    if stage == 0 {
        let sitios = relieve_pick_sites();
        for (x, z, que) in sitios {
            eprintln!("ha-relievetest: {que} en ({x:.0}, {z:.0})");
        }
        world.insert_resource(RelieveSites(sitios));
        teleport_surface(world, sitios[0].0, sitios[0].1);
        advance_stage(world, now);
        return;
    }
    if now - entered < 6.0 {
        return;
    }
    let sitios = world.resource::<RelieveSites>().0;
    if stage >= 1 && stage as usize <= sitios.len() {
        let (x, z, que) = sitios[stage as usize - 1];
        eprintln!("ha-relievetest: {que} pedido ({x:.0},{z:.0})");
        shot(world, &format!("{}/relieve-{}-{que}.png", flytest_dir(), stage - 1));
        if (stage as usize) < sitios.len() {
            let (nx, nz, _) = sitios[stage as usize];
            teleport_surface(world, nx, nz);
        }
        advance_stage(world, now);
        return;
    }
    eprintln!("ha-relievetest: DONE");
    std::process::exit(0);
}

/// Busca en la Tierra la región con MÁS capacidad de depredador (el bosque de verdad) y
/// una región YERMA lejos de todo pueblo. Determinista: el mismo par en cada corrida.
fn eco_pick_sites() -> ((f32, f32), (f32, f32)) {
    use ha_shared::ecology::{carrying_capacity, region_center};
    let body = ha_shared::CelestialBody::Earth;
    let (mut best, mut best_cap) = ((0i32, 0i32), -1.0f32);
    let mut barren: Option<(i32, i32)> = None;
    for rx in -60..60 {
        for rz in -60..60 {
            let cap = carrying_capacity(body, rx, rz);
            if cap.carnivores > best_cap {
                best_cap = cap.carnivores;
                best = (rx, rz);
            }
            if barren.is_none() && cap.is_barren() {
                // Yermo de VERDAD: la región Y sus ocho vecinas. La ventana de simulación
                // es 3×3, así que en el borde de un desierto se ve —correctamente— la
                // manada del prado de al lado; para afirmar "acá no hay nada" hay que
                // estar en mitad de la nada. Y sin asentamiento encima, o el test de "no
                // hay gente en el descampado" mediría otra cosa.
                let deep = (-1..=1).all(|dx| {
                    (-1..=1).all(|dz| carrying_capacity(body, rx + dx, rz + dz).is_barren())
                });
                let (cx, cz) = region_center(rx, rz);
                if deep
                    && ha_shared::geography::settlement_covering(body, cx as i64, cz as i64)
                        .is_none()
                {
                    barren = Some((rx, rz));
                }
            }
        }
    }
    let (bx, bz) = region_center(best.0, best.1);
    let (dx, dz) = barren.map(|(a, b)| region_center(a, b)).unwrap_or((0.0, 0.0));
    ((bx, bz), (dx, dz))
}

/// Tres estampas y tres afirmaciones:
///
/// 1. **YERMO** — en una región sin forraje y sin pueblo no hay ni un animal ni un NPC.
///    Con el anillo viejo esto era IMPOSIBLE: la fauna se sembraba donde estuvieras.
/// 2. **BOSQUE** — la región con más capacidad tiene manada de verdad, materializada en
///    sus claros, y forraje en el suelo.
/// 3. **CIUDAD** — la gente está en el asentamiento de `geography`, y llega sola al entrar.
fn eco_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 3.0 {
        return;
    }
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
    let body = ha_shared::CelestialBody::Earth;
    let stage = world.resource::<FlyTestState>().stage;
    let entered = world.resource::<FlyTestState>().stage_entered;
    // Cuánto se le da al mundo para asentarse en cada parada (la ecología corre a 4 s y el
    // descubrimiento de pueblos a 2 s: con 12 s han corrido varias veces).
    let settle: f32 = crate::flags::var("HA_ECOSETTLE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(12.0);

    // Stage 0: elegir sitios y plantarse en el YERMO.
    if stage == 0 {
        let (forest, barren) = eco_pick_sites();
        world.insert_resource(EcoSites { forest, barren, forest_herb: 0.0 });
        teleport_surface(world, barren.0, barren.1);
        eprintln!("ha-ecotest: yermo en ({:.0}, {:.0})", barren.0, barren.1);
        advance_stage(world, now);
        return;
    }
    // La etapa de ciudad usa una espera CORTA: el salto de varios kilómetros hasta el
    // asentamiento reencuadra la región, y dejar 90 s ahí sólo da tiempo a que el jugador
    // derive lejos del sitio que queríamos comprobar.
    let wait = match stage {
        4 => 0.5,
        5 => settle.min(12.0),
        _ => settle,
    };
    if now - entered < wait {
        return;
    }
    let sites = *world.resource::<EcoSites>();

    match stage {
        1 => {
            let (x, z) = sites.barren;
            let p = crate::fauna::eco_probe(world, body, x, z);
            let npcs = count_npcs_near(world, 120.0);
            let at = player_xz(world);
            eprintln!(
                "ha-ecotest: YERMO pedido ({x:.0},{z:.0}) jugador ({:.0},{:.0}) region {:?} \
                 cap {:?} pop {:?} fauna {}+{} forraje {} npcs-cerca {npcs}",
                at.0, at.1, p.region, p.cap, p.pop, p.live_herb, p.live_carn, p.forage
            );
            shot(world, &format!("{}/eco-0-yermo.png", flytest_dir()));
            if p.live_herb + p.live_carn > 0 || p.forage > 0 {
                eprintln!("ha-ecotest: INVARIANT roto — hay fauna en un yermo (¿volvió el anillo?)");
                report_bug("ecologia", "yermo sin fauna", "se materializo fauna sin capacidad");
                std::process::exit(1);
            }
            if npcs > 0 {
                eprintln!("ha-ecotest: INVARIANT roto — hay NPCs en el descampado ({npcs})");
                report_bug("sociedad", "descampado sin npcs", "se sembraron NPCs fuera de un pueblo");
                std::process::exit(1);
            }
            teleport_surface(world, sites.forest.0, sites.forest.1);
            eprintln!("ha-ecotest: bosque en ({:.0}, {:.0})", sites.forest.0, sites.forest.1);
            advance_stage(world, now);
        }
        2 => {
            let (x, z) = sites.forest;
            let p = crate::fauna::eco_probe(world, body, x, z);
            let at = player_xz(world);
            eprintln!(
                "ha-ecotest: BOSQUE pedido ({x:.0},{z:.0}) jugador ({:.0},{:.0}) region {:?} \
                 cap {:?} pop {:?} ventana3x3 {:?} cuerpo {:?} fauna {}+{} forraje {} regiones {}",
                at.0, at.1, p.region, p.cap, p.pop, p.win, p.total, p.live_herb, p.live_carn,
                p.forage, p.known_regions
            );
            shot(world, &format!("{}/eco-1-bosque.png", flytest_dir()));
            if p.live_herb == 0 {
                eprintln!("ha-ecotest: INVARIANT roto — el mejor bosque del planeta no tiene fauna");
                report_bug("ecologia", "bosque con manada", "capacidad alta sin animales materializados");
                std::process::exit(1);
            }
            if p.forage == 0 {
                eprintln!("ha-ecotest: INVARIANT roto — no hay forraje donde el bioma lo da");
                report_bug("ecologia", "forraje visible", "plantas > 0 pero nada dibujado");
                std::process::exit(1);
            }
            world.resource_mut::<EcoSites>().forest_herb = p.total.herbivores;
            advance_stage(world, now);
        }
        3 => {
            // SOSTENIBILIDAD: tras un rato largo con la manada Y los depredadores
            // materializados, el bosque no puede haberse autodestruido — ni pelado de
            // pasto ni vaciado de presas. Es lo que separa una cadena alimenticia de una
            // extinción con pasos extra.
            let (x, z) = sites.forest;
            let p = crate::fauna::eco_probe(world, body, x, z);
            eprintln!(
                "ha-ecotest: BOSQUE +{settle:.0}s pop {:?} ventana3x3 {:?} cuerpo {:?} fauna {}+{} forraje {}",
                p.pop, p.win, p.total, p.live_herb, p.live_carn, p.forage
            );
            if p.live_herb == 0 {
                eprintln!("ha-ecotest: INVARIANT roto — los depredadores vaciaron el bosque");
                report_bug("ecologia", "cadena sostenible", "herbivoros extintos con el jugador presente");
                std::process::exit(1);
            }
            if p.pop.plants <= 0.5 {
                eprintln!("ha-ecotest: INVARIANT roto — el prado quedo pelado y no rebrota");
                report_bug("ecologia", "rebrote", "plantas ~0 tras pastoreo");
                std::process::exit(1);
            }
            // La caza no puede llevarse a la mitad de la manada en un rato: si pasa, los
            // nacimientos no compensan a la predación y el mundo se vacía solo con mirarlo.
            // Se mide sobre el CUERPO entero: la ventana 3×3 baja sola cuando la manada se
            // dispersa hacia afuera, y eso es vida normal. Lo que no puede pasar es que el
            // planeta pierda la mitad de sus herbívoros mientras mirás.
            let before = sites.forest_herb;
            if before > 0.0 && p.total.herbivores < before * 0.5 {
                eprintln!(
                    "ha-ecotest: INVARIANT roto — el planeta cayo de {before:.2} a {:.2} herbivoros en {settle:.0}s",
                    p.total.herbivores
                );
                report_bug("ecologia", "predacion equilibrada", "la caza supera con creces a la natalidad");
                std::process::exit(1);
            }
            // A la ciudad más cercana: ahí tiene que haber gente, y llegar sola.
            let mut best: Option<(i64, (i64, i64))> = None;
            for sx in -25..25 {
                for sz in -25..25 {
                    let Some(s) = ha_shared::geography::settlement_in(body, sx, sz) else { continue };
                    if s.tier != ha_shared::geography::SettlementTier::City {
                        continue;
                    }
                    let d = s.center.0.abs() + s.center.1.abs();
                    if best.map_or(true, |(bd, _)| d < bd) {
                        best = Some((d, s.center));
                    }
                }
            }
            let Some((_, (cx, cz))) = best else {
                eprintln!("ha-ecotest: NO HAY NINGUNA CIUDAD");
                std::process::exit(1);
            };
            world.insert_resource(EcoCity { x: cx as f32, z: cz as f32 });
            eprintln!("ha-ecotest: ciudad en ({cx}, {cz})");
            advance_stage(world, now);
        }
        4 => {
            let c = *world.resource::<EcoCity>();
            teleport_surface(world, c.x, c.z);
            advance_stage(world, now);
        }
        _ => {
            let npcs = count_npcs_near(world, 200.0);
            let at = player_xz(world);
            eprintln!("ha-ecotest: CIUDAD jugador ({:.0},{:.0}) npcs-cerca {npcs}", at.0, at.1);
            shot(world, &format!("{}/eco-2-ciudad.png", flytest_dir()));
            if npcs == 0 {
                eprintln!("ha-ecotest: INVARIANT roto — la ciudad esta desierta");
                report_bug("sociedad", "ciudad poblada", "entrar a un asentamiento no sembro gente");
                std::process::exit(1);
            }
            eprintln!("ha-ecotest: DONE");
            std::process::exit(0);
        }
    }
}

#[derive(Resource, Clone, Copy)]
struct EcoCity {
    x: f32,
    z: f32,
}

#[derive(Resource, Clone, Copy)]
struct EcoSites {
    forest: (f32, f32),
    barren: (f32, f32),
    /// Herbívoros de la región del bosque en la primera medición — para comparar tras el
    /// rato largo y ver si la cadena se sostiene o se está comiendo a sí misma.
    forest_herb: f32,
}

/// NPCs **cerca** del jugador. Ojo: `manage_society` instancia todos los del cuerpo (los
/// lejanos van ocultos y en LOD barato), así que contar entidades a secas mediría el
/// planeta entero, no "hay gente donde estoy".
fn count_npcs_near(world: &mut World, r: f32) -> usize {
    let (px, pz) = player_xz(world);
    let mut q = world.query::<&crate::society::Npc>();
    q.iter(world)
        .filter(|n| (n.x - px) * (n.x - px) + (n.z - pz) * (n.z - pz) < r * r)
        .count()
}

fn player_xz(world: &mut World) -> (f32, f32) {
    let mut q = world.query_filtered::<&Transform, With<Player>>();
    q.get_single(world)
        .map(|t| (t.translation.x, -t.translation.y))
        .unwrap_or((0.0, 0.0))
}

// ---------------------------------------------------------------------------
// HA_BARRIOTEST — la casa que GENERA el mundo, mirada desde la vereda
// ---------------------------------------------------------------------------

/// El retrato de una casa de ciudad **generada** (no la que construye el
/// jugador: ésa la prueban `HA_FACHADATEST` y `HA_CASATEST`).
///
/// Es el ojo del sistema de fachadas: `city::roofed_rect` dice qué rectángulo
/// ocupa la casa de cada parcela y con qué clave de arte, y `building::stream_roofs`
/// dibuja ese sprite encima de las paredes. Lo que hay que MIRAR en las capturas:
///
/// 1. `barrio-0-frente.png` — la casa desde la vereda. La fachada tiene que
///    TAPAR la huella entera: si asoma un borde de pared o de piso por los
///    costados o por el fondo, el sprite es más chico que la casa.
/// 2. `barrio-1-manzana.png` — la manzana. Sirve para ver si el barrio repite
///    la misma casa y si las fachadas se pisan entre parcelas.
/// 3. `barrio-2-puerta.png` — parado en la puerta: se ve si la puerta dibujada
///    coincide con la celda `Tile::Door` por la que se entra de verdad.
/// 4. `barrio-3-adentro.png` — adentro: la fachada se funde y se ve el interior.
///
/// Escribe además, por stderr, la geometría medida de cada casa que encuentra,
/// que es lo que permite decidir sin adivinar (`ha-barrio:` en el log).
#[derive(Resource, Default)]
struct BarrioTestState {
    /// Una casa por PROGRAMA de lote: (x, z, w, d, clave). Se retrata cada una
    /// desde su vereda, porque el defecto no es el mismo en todas — la fachada
    /// que le sobra a la casa chica es la que le falta a la villa.
    casas: Vec<(i64, i64, i64, i64, String)>,
    /// Cuándo se pidió abrir las puertas del retrato en curso.
    puertas_pedidas: Option<f32>,
}

fn barrio_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 3.0 {
        return;
    }
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
    let stage = world.resource::<FlyTestState>().stage;
    let body = ha_shared::CelestialBody::Earth;

    if stage == 0 {
        // 1) La ciudad más cercana al origen.
        let mut best: Option<(i64, ha_shared::geography::Settlement)> = None;
        for sx in -25..25 {
            for sz in -25..25 {
                let Some(s) = ha_shared::geography::settlement_in(body, sx, sz) else { continue };
                if s.tier != ha_shared::geography::SettlementTier::City {
                    continue;
                }
                let d = s.center.0.abs() + s.center.1.abs();
                if best.as_ref().map_or(true, |(bd, _)| d < *bd) {
                    best = Some((d, s));
                }
            }
        }
        let Some((_, s)) = best else {
            eprintln!("ha-barrio: NO HAY NINGUNA CIUDAD");
            std::process::exit(1);
        };

        // 2) UNA casa de cada programa de vivienda. El radio de una ciudad ronda
        //    los 600 tiles y lo residencial vive en las AFUERAS (`district_at`):
        //    barrer sólo el centro devuelve tiendas y torres.
        let (cx, cz) = s.center;
        let mut casas: Vec<(i64, i64, i64, i64, String)> = Vec::new();
        'busca: for r in 1..90i64 {
            for dz in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dz.abs() != r {
                        continue;
                    }
                    let (tx, tz) = (cx + dx * 8, cz + dz * 8);
                    let Some(rect) = ha_shared::city::roofed_rect(body, &s, tx, tz) else {
                        continue;
                    };
                    // Todo lo que en `city::LOT_PROGRAMS` es vivienda.
                    let es_casa = rect.4.starts_with("building/casa_")
                        || rect.4 == "building/villa"
                        || rect.4 == "building/duplex";
                    if !es_casa || casas.iter().any(|c| c.4 == rect.4) {
                        continue;
                    }
                    eprintln!(
                        "ha-barrio: {} en ({},{}) huella {}×{}",
                        rect.4, rect.0, rect.1, rect.2, rect.3
                    );
                    casas.push((rect.0, rect.1, rect.2, rect.3, rect.4.to_string()));
                    if casas.len() >= 6 {
                        break 'busca;
                    }
                }
            }
        }
        if casas.is_empty() {
            eprintln!("ha-barrio: la ciudad ({cx},{cz}) no tiene NINGUNA casa");
            std::process::exit(1);
        }
        world.resource_mut::<BarrioTestState>().casas = casas;
        advance_stage(world, now);
        return;
    }

    // Un retrato por casa desde la vereda + el último por dentro. Lo que hay que
    // MIRAR en cada uno: que la fachada TAPE la huella (si asoma un borde de piso
    // o de pared por los costados o por el fondo, el sprite es más chico que la
    // casa), que la puerta dibujada caiga sobre la celda por la que se entra, y
    // que dos parcelas vecinas no sean la misma casa.
    let casas = world.resource::<BarrioTestState>().casas.clone();
    let n = casas.len();
    let shots = world.resource::<FlyTestState>().shots as usize;
    if shots > n + 1 {
        // Un respiro antes de salir: `save_to_disk` escribe la última captura un
        // par de frames después de pedirla, y sin esta espera el driver se iba
        // dejando el archivo a medio escribir (o sin escribir).
        if now - world.resource::<FlyTestState>().stage_entered > 1.5 {
            eprintln!("ha-barrio: DONE");
            std::process::exit(0);
        }
        return;
    }
    // Los dos últimos beats son sobre la ÚLTIMA casa: uno pegado a su puerta
    // —para ver el batiente abierto y si calza con el pixel de la fachada— y otro
    // adentro, para ver que la fachada se funde y aparece el interior.
    let en_puerta = shots == n;
    let adentro = shots == n + 1;
    // Los dos últimos van sobre la casa EN HILERA si la ciudad tiene una: es la
    // que compone su fachada de a módulos y la que trae la puerta animada, o sea
    // la que hay que mirar de cerca.
    let idx = if shots >= n {
        casas.iter().position(|c| c.4 == "building/casa_hilera").unwrap_or(n - 1)
    } else {
        shots
    };
    let (hx, hz, hw, hd, clave) = casas[idx].clone();
    let (px, pz, vista, name) = if en_puerta {
        (
            hx as f32 + hw as f32 / 2.0,
            (hz + hd) as f32 + 1.6,
            18.0,
            format!("barrio-{shots}-puerta"),
        )
    } else if adentro {
        (
            hx as f32 + hw as f32 / 2.0,
            hz as f32 + hd as f32 / 2.0,
            34.0,
            format!("barrio-{shots}-adentro"),
        )
    } else {
        // En la vereda, cuatro tiles por delante de la cara sur. El encuadre
        // acompaña al frente de la casa para que entre entera y con su vereda.
        (
            hx as f32 + hw as f32 / 2.0,
            (hz + hd) as f32 + 5.0,
            (hw as f32 * 2.6).clamp(30.0, 70.0),
            format!("barrio-{shots}-{}", clave.rsplit('/').next().unwrap_or("casa")),
        )
    };
    teleport_a_celda(world, px, pz);
    world.resource_mut::<crate::camera::CameraZoom>().pedir_encuadre(vista);
    let entered = world.resource::<FlyTestState>().stage_entered;
    // **La captura espera al MUNDO, no al reloj.** Ver `mundo_listo`: con un
    // plazo fijo este driver dejaba ocho PNG en negro y salía con código 0.
    // El medio segundo de más es el fundido del techo al volver de adentro.
    if now - entered > 0.5 && mundo_listo(world) {
        // **Y con las puertas ABIERTAS.** Se abren por pedido, como las abre el
        // jugador (`crate::puertas`), y se espera a que termine la animación:
        // así la captura muestra si el batiente del pack calza en su fachada.
        let pedidas = world.resource::<BarrioTestState>().puertas_pedidas;
        let Some(t0) = pedidas else {
            let mut q = world.query::<(&crate::building::PuertaDeFachada, &mut crate::puertas::Abrible)>();
            for (p, mut a) in q.iter_mut(world) {
                if !p.open {
                    a.pedido = true;
                }
            }
            world.resource_mut::<BarrioTestState>().puertas_pedidas = Some(now);
            return;
        };
        if now - t0 < 1.5 {
            return;
        }
        world.resource_mut::<BarrioTestState>().puertas_pedidas = None;
        shot(world, &format!("{}/{name}.png", flytest_dir()));
        world.resource_mut::<FlyTestState>().shots += 1;
        advance_stage(world, now);
        return;
    }
    if now - entered > 30.0 {
        eprintln!("ha-barrio: AVISO — 30 s sin que el chunk termine de armarse; disparo igual ({name})");
        shot(world, &format!("{}/{name}.png", flytest_dir()));
        world.resource_mut::<FlyTestState>().shots += 1;
        advance_stage(world, now);
    }
}

// ---------------------------------------------------------------------------
// HA_CITYTEST — retrato de una ciudad generada
// ---------------------------------------------------------------------------

fn city_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    // **Esperar a que el mundo EXISTA, no a que pasen tres segundos.**
    //
    // El reloj fijo alcanzaba en el escritorio, donde a los 3 s ya se está
    // jugando. En el Redmi no: la generación de terreno todavía va por la mitad,
    // así que el driver disparaba sus tres capturas contra la pantalla de carga
    // y salía con `DONE` y código 0 — o sea, PASABA sin haber visto una ciudad.
    // Es el mismo error que ya avisa `trip_test_drive`: un plazo justo no detecta
    // un bug, detecta una máquina lenta.
    if *world.resource::<State<crate::game_phase::GamePhase>>().get()
        != crate::game_phase::GamePhase::Playing
    {
        // Los tiempos de los beats se cuentan desde que se entra a JUGAR, no desde
        // que arrancó el proceso.
        world.resource_mut::<FlyTestState>().stage_entered = now;
        return;
    }
    // Y un respiro para que el anillo de chunks alrededor del jugador se arme
    // antes del teletransporte: si no, la primera captura sale a medio poblar.
    if now - world.resource::<FlyTestState>().stage_entered < 2.0 {
        return;
    }
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
    let stage = world.resource::<FlyTestState>().stage;
    let body = ha_shared::CelestialBody::Earth;

    if stage == 0 {
        // Buscar la ciudad más cercana al origen y plantarse en su centro.
        let mut best: Option<(i64, (i64, i64))> = None;
        for sx in -25..25 {
            for sz in -25..25 {
                let Some(s) = ha_shared::geography::settlement_in(body, sx, sz) else { continue };
                if s.tier != ha_shared::geography::SettlementTier::City {
                    continue;
                }
                let d = s.center.0.abs() + s.center.1.abs();
                if best.map_or(true, |(bd, _)| d < bd) {
                    best = Some((d, s.center));
                }
            }
        }
        let Some((_, (cx, cz))) = best else {
            eprintln!("ha-citytest: NO HAY NINGUNA CIUDAD");
            std::process::exit(1);
        };
        teleport_a_celda(world, cx as f32, cz as f32);
        eprintln!("ha-citytest: ciudad en ({cx}, {cz})");
        advance_stage(world, now);
        return;
    }

    // Tres alturas: a pie en la calle, sobre el barrio y sobre la región.
    let beats: [(f32, f32, &str); 3] = [
        (1.0, 0.0, "city-0-street"),
        (2.0, 220.0, "city-1-district"),
        (3.0, 600.0, "city-2-region"),
    ];
    let shots = world.resource::<FlyTestState>().shots as usize;
    if shots >= beats.len() {
        // La captura se escribe unos frames DESPUÉS de pedirla: salir en el acto
        // perdía la última (y en una pantalla virtual, lenta, todas).
        let entered = world.resource::<FlyTestState>().stage_entered;
        // la última se pidió a los ~3 s de la etapa; con render por software
        // (pantalla virtual) un frame tarda más de un segundo: 20 s de margen
        if now - entered < 20.0 {
            return;
        }
        eprintln!("ha-citytest: DONE");
        std::process::exit(0);
    }
    let (at, alt, name) = beats[shots];
    let entered = world.resource::<FlyTestState>().stage_entered;
    {
        let mut fly = world.resource_mut::<crate::creative_fly::CreativeFlight>();
        if alt > 0.0 {
            fly.phase = crate::creative_fly::Phase::Climb;
            fly.vert = alt;
            fly.alt = alt;
            fly.vy = 0.0;
        }
    }
    // Igual que el driver de barrio: la captura espera a que el anillo de chunks
    // esté armado (`mundo_listo`), no a que pase un plazo. Con el plazo, las tres
    // capturas de la ciudad salían en negro y el driver decía `DONE`.
    if now - entered > at && mundo_listo(world) {
        shot(world, &format!("{}/{name}.png", flytest_dir()));
        world.resource_mut::<FlyTestState>().shots += 1;
        return;
    }
    if now - entered > at + 30.0 {
        eprintln!("ha-citytest: AVISO — 30 s sin que el chunk termine de armarse; disparo igual ({name})");
        shot(world, &format!("{}/{name}.png", flytest_dir()));
        world.resource_mut::<FlyTestState>().shots += 1;
    }
}

// ---------------------------------------------------------------------------
// HA_TRIPTEST — el viaje de verdad: Tierra → Luna → Tierra, volando
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct TripTestState {
    stage: u8,
    stage_t: f32,
    prev_view_h: f32,
    max_jump_ratio: f32,
    /// El salto normalizado por tiempo (ver `zoom_rate_per_s`) — el que se compara contra
    /// el techo, porque el de por-frame depende de los fps y el celu va a 13.
    max_zoom_rate: f32,
    /// Última vez que se trazó el descenso final (segundos desde el arranque).
    last_trace: f32,
}

/// Altura de crucero para el trayecto: a esta altura el encuadre abarca la
/// distancia Tierra–Luna, que es la condición para poder apuntar y viajar.
const TRIP_CRUISE_ALT: f32 = 200_000.0;

/// NOTA sobre los presupuestos de tiempo de este driver: `creative_fly_step` acota
/// el paso a 1/20 s, así que en una máquina que va a 10 fps el mundo avanza a la
/// MITAD de tiempo real. Los plazos de acá están en segundos de reloj, así que
/// tienen que llevar ese margen: el ascenso a crucero son ~16 s simulados, y con
/// el tope de paso eso puede ser el doble o el triple de reloj en un PC lento. Un
/// plazo justo no detecta un bug, detecta una GPU vieja.

/// Viaje completo por el camino REAL del jugador (teclas, sin forzar estado):
/// despegar de la Tierra, cruzar hasta la Luna, aterrizar, volver a despegar y
/// regresar a la Tierra. Es literalmente el reporte del usuario ("salgo al
/// espacio y ya no puedo regresar a la Tierra") convertido en test.
///
/// Comprueba en el camino que el marco REBASA de cuerpo sin saltos de cámara:
/// el rebase mueve al jugador y a todos los discos el mismo delta, así que un
/// salto de encuadre ahí significaría que la continuidad se rompió.
fn trip_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 2.0 {
        return;
    }
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;

    // Vigilancia de continuidad del encuadre (lo que se ve, no el objetivo).
    let view_h = world.resource::<crate::camera::CameraZoom>().live_view_h;
    let frame_dt = world.resource::<Time>().delta_secs();
    {
        let mut st = world.resource_mut::<TripTestState>();
        if st.prev_view_h > 0.0 {
            let ratio = (view_h / st.prev_view_h).max(st.prev_view_h / view_h);
            let rate = zoom_rate_per_s(st.prev_view_h, view_h, frame_dt);
            if ratio > st.max_jump_ratio {
                st.max_jump_ratio = ratio;
            }
            if rate > st.max_zoom_rate {
                st.max_zoom_rate = rate;
            }
            // Dónde exactamente. Un máximo global no dice si el pico es una maniobra real
            // o una etapa del propio driver forzando un estado, y es LA pregunta al mirar
            // un reporte de "la cámara pega un salto".
            if rate > 2.5 {
                let etapa = st.stage;
                eprintln!(
                    "ha-triptest: pico de encuadre t={now:.1} etapa={etapa} {:.0} -> {view_h:.0} (x{rate:.2}/s)",
                    st.prev_view_h
                );
            }
        }
        st.prev_view_h = view_h;
    }

    let (stage, stage_t) = {
        let s = world.resource::<TripTestState>();
        (s.stage, s.stage_t)
    };
    let dt = now - stage_t;
    let alt = world.resource::<crate::creative_fly::CreativeFlight>().alt;
    let phase = world.resource::<crate::creative_fly::CreativeFlight>().phase;
    let ref_body = world.resource::<crate::universe::UniverseFrame>().ref_body;
    let advance = |w: &mut World, st: u8| {
        let mut s = w.resource_mut::<TripTestState>();
        s.stage = st;
        s.stage_t = now;
    };

    match stage {
        // 0 · despegar de la Tierra hasta altura de crucero.
        0 => {
            sostener(world, Accion::Saltar, now, 0.4);
            if alt >= TRIP_CRUISE_ALT {
                shot(world, &format!("{}/trip-0-earth-orbit.png", flytest_dir()));
                eprintln!("ha-triptest: crucero sobre la Tierra (alt={alt:.0} view_h={view_h:.0})");
                advance(world, 1);
            } else if dt > 70.0 {
                report_bug("trip", "no_climb_from_earth", &format!("alt={alt:.0}"));
                std::process::exit(1);
            }
        }
        // 1 · cruzar hasta la Luna (el marco debe rebasar solo).
        1 => {
            // CON EL ENCUADRE ABIERTO, que es como viaja una persona: no sabe dónde está
            // la Luna, así que abre el zoom para buscarla. El rebase de marco pasa acá, y
            // hasta ahora sólo se probaba con el zoom cerrado — el salto que se reportó
            // vive justo en este hueco.
            zoom_como_jugador(world);
            steer_toward(world, ha_shared::CelestialBody::Moon, now, Accion::Saltar);
            if ref_body == ha_shared::CelestialBody::Moon {
                shot(world, &format!("{}/trip-1-moon-approach.png", flytest_dir()));
                eprintln!("ha-triptest: marco rebasado a la Luna (alt={alt:.0})");
                advance(world, 2);
            } else if dt > 40.0 {
                report_bug("trip", "never_reached_moon", &format!("ref={ref_body:?}"));
                std::process::exit(1);
            }
        }
        // 2 · bajar y aterrizar en la Luna.
        2 => {
            // Aproximación: bajar Y cerrar sobre el cuerpo a la vez. La altura
            // efectiva combina ambas, así que el encuadre cierra y la velocidad
            // baja sola conforme llegás — no hace falta frenar a mano.
            steer_toward(world, ha_shared::CelestialBody::Moon, now, Accion::Usar);
            if phase == crate::creative_fly::Phase::Ground {
                let region = world.resource::<crate::scene::RegionRuntime>().active;
                if region != crate::scene::RegionId::Luna {
                    report_bug("trip", "moon_landing_wrong_region", &format!("{region:?}"));
                    std::process::exit(1);
                }
                shot(world, &format!("{}/trip-2-moon-landed.png", flytest_dir()));
                eprintln!("ha-triptest: aterrizado en la Luna, region={region:?}");
                advance(world, 3);
            } else if dt > LAND_BUDGET_S {
                report_bug("trip", "never_landed_on_moon", &format!("alt={alt:.0} phase={phase:?}"));
                std::process::exit(1);
            }
        }
        // 3 · volver a despegar (ESTE es el "ya no puedo salir").
        3 => {
            sostener(world, Accion::Saltar, now, 0.4);
            if alt >= TRIP_CRUISE_ALT {
                eprintln!("ha-triptest: de vuelta en crucero desde la Luna");
                advance(world, 4);
            } else if dt > 30.0 {
                report_bug("trip", "no_climb_from_moon", &format!("alt={alt:.0}"));
                std::process::exit(1);
            }
        }
        // 4 · regresar a la Tierra.
        4 => {
            // Ídem la ida: el regreso también se hace con el encuadre abierto.
            zoom_como_jugador(world);
            steer_toward(world, ha_shared::CelestialBody::Earth, now, Accion::Saltar);
            if ref_body == ha_shared::CelestialBody::Earth {
                shot(world, &format!("{}/trip-3-earth-return.png", flytest_dir()));
                eprintln!("ha-triptest: marco de vuelta en la Tierra");
                advance(world, 5);
            } else if dt > LAND_BUDGET_S {
                report_bug("trip", "never_returned_to_earth", &format!("ref={ref_body:?}"));
                std::process::exit(1);
            }
        }
        // 5 · aterrizar en casa y cerrar el ciclo.
        _ => {
            steer_toward(world, ha_shared::CelestialBody::Earth, now, Accion::Usar);
            // Traza del descenso final. Un snapshot al timeout no distingue "baja despacio"
            // de "baja y algo la vuelve a subir", y ésa es justo la pregunta.
            {
                let mut last = world.resource_mut::<TripTestState>();
                if now - last.last_trace > 1.0 {
                    last.last_trace = now;
                    let f = world.resource::<crate::creative_fly::CreativeFlight>();
                    eprintln!(
                        "ha-trip-descenso: t={dt:.1} vert={:.1} vy={:.1} alt={alt:.0} armed={} phase={phase:?}",
                        f.vert, f.vy, f.land_armed
                    );
                }
            }
            if phase == crate::creative_fly::Phase::Ground {
                let region = world.resource::<crate::scene::RegionRuntime>().active;
                let (max_jump, max_rate) = {
                    let s = world.resource::<TripTestState>();
                    (s.max_jump_ratio, s.max_zoom_rate)
                };
                shot(world, &format!("{}/trip-4-earth-landed.png", flytest_dir()));
                let region_ok = matches!(region, crate::scene::RegionId::Overworld { .. });
                if !region_ok {
                    report_bug("trip", "earth_landing_wrong_region", &format!("{region:?}"));
                }
                let zoom_ok = max_jump < MAX_ZOOM_JUMP_FRAME && max_rate < MAX_ZOOM_RATE_PER_S;
                if !zoom_ok {
                    report_bug(
                        "trip",
                        "zoom_discontinuo",
                        &format!("x{max_jump:.2}/frame, x{max_rate:.2}/s"),
                    );
                }
                let ok = region_ok && zoom_ok;
                eprintln!("ha-triptest: aterrizado en la Tierra, region={region:?}");
                eprintln!(
                    "ha-triptest: salto de encuadre maximo = x{max_jump:.2}/frame, x{max_rate:.2}/s"
                );
                eprintln!(
                    "ha-triptest: {}",
                    if ok { "ROUND TRIP OK" } else { "ROUND TRIP FAIL" }
                );
                // Igual que en `cycletest`: sin `DONE` la suite lo lee como "murió sin
                // DONE" por más que el viaje haya salido perfecto (ver ese comentario).
                eprintln!("ha-triptest: DONE");
                std::process::exit(if ok { 0 } else { 1 });
            } else if dt > LAND_BUDGET_S {
                // `alt` sola no alcanza para saber POR QUÉ no toca: es la hipotenusa
                // `sqrt(vert² + gap²)`, así que un mismo alt=38 puede ser "quedé 38 m
                // arriba" o "estoy al ras pero a 38 m del disco caminable", y el gate de
                // aterrizaje mira los dos por separado. Se vuelca el gate entero.
                let f = world.resource::<crate::creative_fly::CreativeFlight>();
                report_bug(
                    "trip",
                    "never_landed_on_earth",
                    &format!(
                        "alt={alt:.0} vert={:.1} vy={:.1} armed={} body={:?} phase={phase:?}",
                        f.vert, f.vy, f.land_armed, f.body
                    ),
                );
                std::process::exit(1);
            }
        }
    }

    if now > 220.0 {
        report_bug("trip", "timeout", &format!("stage={stage}"));
        std::process::exit(1);
    }
}

/// Mantiene SALTO (para no perder altura) y pulsa las teclas de movimiento que
/// apuntan hacia `target` en el plano del marco. Es lo que haría un jugador
/// mirando la pantalla: ver el disco y volar hacia él.
/// Aleja el encuadre como lo haría un jugador buscando a dónde ir.
///
/// Los drivers navegaban SABIENDO dónde está el cuerpo (`body_offset`), y eso deja sin
/// probar el camino que de verdad usa la gente: nadie sabe dónde está la Luna, así que
/// primero abre el zoom para encontrarla y recién después apunta. El reporte de que "la
/// transición Tierra↔Luna se ve rara con el zoom alejado" cae justo en ese hueco — el
/// rebase de marco nunca se había ejercitado con el encuadre abierto.
/// El gesto del que viaja: abrir un poco el encuadre para buscar a dónde va.
///
/// Antes esto CLAVABA el encuadre en 20 km absolutos, porque en ese momento la altura no
/// encuadraba nada y abrir a mano era la única forma de ver la Luna. Ahora la altura
/// vuelve a mandar (a 200 km de altura el encuadre automático ya son ~500 km: la Luna
/// entra sola), así que clavar un absoluto hacía dos cosas mal — peleaba contra la
/// altura frame a frame, y al entrar en la etapa pegaba un ×25 de golpe que el propio
/// driver anotaba como si el juego hubiera saltado.
///
/// Un pellizco hoy es un MULTIPLICADOR, así que eso es lo que se simula: ×2, sostenido.
/// Sigue cubriendo lo que importaba —el rebase de marco con el encuadre abierto, que es
/// donde vivía el salto reportado— y es continuo por construcción.
///
/// Y se llega al ×2 **a velocidad de dedo** (×1.6/s ≈ 1.5 s de gesto), no de un saque.
/// Aplicarlo en un frame metía un ×2 instantáneo que la cámara recorría a su ritmo tope
/// durante medio segundo, y el driver lo anotaba como un salto del juego: el test se
/// estaba midiendo a sí mismo.
fn zoom_como_jugador(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let mut z = world.resource_mut::<crate::camera::CameraZoom>();
    z.user_mult = (z.user_mult * 1.6f32.powf(dt)).min(2.0);
}

fn steer_toward(
    world: &mut World,
    target: ha_shared::CelestialBody,
    now: f32,
    vertical: Accion,
) {
    let t = world.resource::<crate::solar_system_clock::SolarSystemClock>().t;
    let ref_body = world.resource::<crate::universe::UniverseFrame>().ref_body;
    let goal = crate::universe::body_offset(target, ref_body, t);
    let here = {
        let mut q = world.query_filtered::<&SimState, With<Player>>();
        q.get_single(world)
            .map(|s| Vec2::new(s.body.x, -s.body.z))
            .unwrap_or(Vec2::ZERO)
    };
    let d = goal - here;
    // Un pulso de teclas fresco de vez en cuando (mismo motivo que
    // `hold_with_fresh_edges`: no perder el flanco durante un frame bloqueado).
    let pulse = {
        let mut st = world.resource_mut::<FlyTestState>();
        let due = now - st.last_press_pulse > 0.4;
        if due {
            st.last_press_pulse = now;
        }
        due
    };
    if pulse {
        release_all(world);
    }
    actuar(world, vertical); // SALTO = mantener crucero · USAR = ir bajando
    // render y = -z ⇒ W (z decreciente) va hacia arriba en pantalla.
    if d.x.abs() > d.y.abs() * 0.3 {
        actuar(world, if d.x > 0.0 { Accion::D } else { Accion::A });
    }
    if d.y.abs() > d.x.abs() * 0.3 {
        actuar(world, if d.y > 0.0 { Accion::W } else { Accion::S });
    }
}

#[derive(Resource, Default)]
struct CycleTestState {
    body_idx: usize,
    land_idx: u32,
    stage: u8,
    stage_t: f32,
}

/// Presupuesto TOTAL del ciclo (6 aterrizajes + 6 despegues sobre 3 cuerpos).
///
/// Eran 120 s, calibrados cuando el vuelo arrancaba a 30 m/s. Ahora arranca a 15
/// (`universe::CLIMB_BASE`, atado al encuadre para que la subida no pegue un tirón), o sea
/// que cada tramo tarda ~15 % más de tiempo de SIMULACIÓN. Y encima el reloj de pared corre
/// más rápido que la simulación: `creative_fly` capa el paso a 1/20 s, así que por debajo de
/// 20 fps —el driver en esta compu, y el Redmi siempre— un segundo simulado cuesta más de un
/// segundo real. Las dos cosas juntas dejaron el ciclo justo por encima del presupuesto: la
/// corrida moría a mitad de Marte habiendo hecho todo bien.
///
/// El número sale de una corrida medida (~150 s acá), con el doble de margen para un
/// aparato más lento. No es "poné un número grande": un tramo que de verdad se cuelga lo
/// caza igual el presupuesto POR TRAMO, que es mucho más ajustado.
const CYCLE_BUDGET_S: f32 = 300.0;

/// Presupuesto por TRAMO (bajar de órbita alta hasta tocar, o despegar hasta cruzar la
/// línea de espacio). Un descenso desde 1.2 radios son ~12 s de simulación en el cuerpo más
/// grande de la lista (Venus, 10.4 km); 45 s de reloj de pared aguantan el peor caso de fps
/// sin tapar un tramo realmente colgado — que es lo que este driver existe para cazar.
const CYCLE_LEG_S: f32 = 45.0;

/// Bodies to round-trip (landable, varied atmospheres) and lands per body.
const CYCLE_BODIES: [ha_shared::CelestialBody; 3] = [
    ha_shared::CelestialBody::Moon,
    ha_shared::CelestialBody::Venus,
    ha_shared::CelestialBody::Mars,
];
const CYCLE_LANDS: u32 = 2;

/// Deja el vuelo EN ÓRBITA sobre `body`, armado para aterrizar. En el marco
/// único eso es: ese cuerpo es el origen, el jugador está dentro de su disco y
/// la altura está muy por encima de la atmósfera. El descenso posterior baja por
/// el camino REAL (mantener USAR), sin forzar región ni posición.
fn place_orbit_to_land(world: &mut World, body: ha_shared::CelestialBody) {
    world.resource_mut::<crate::universe::UniverseFrame>().ref_body = body;
    let alt = ha_shared::body_radius(body) * 1.2;
    {
        let mut fly = world.resource_mut::<crate::creative_fly::CreativeFlight>();
        fly.body = body;
        fly.phase = crate::creative_fly::Phase::Orbit;
        fly.land_armed = true;
        fly.vert = alt;
        fly.alt = alt;
        fly.vy = 0.0;
        fly.pos = Vec3::ZERO;
    }
    let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
    if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
        sim.body.x = 0.0;
        sim.body.z = 0.0;
        vis.from_x = 0.0;
        vis.to_x = 0.0;
        vis.from_z = 0.0;
        vis.to_z = 0.0;
    }
    zero_flight_input(world);
}

/// Neutralise all simulated input so the orbit doesn't drift/climb on its own.
fn zero_flight_input(world: &mut World) {
    {
        let mut intent = world.resource_mut::<crate::input::MovementIntent>();
        intent.move_x = 0.0;
        intent.move_z = 0.0;
    }
    {
        let mut a = world.resource_mut::<crate::input::TouchActions>();
        a.jump = false;
        a.jump_edge = false;
        a.action = false;
    }
    release_all(world);
}

fn cycle_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 2.0 {
        return;
    }
    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
    let (body_idx, land_idx, stage, stage_t) = {
        let s = world.resource::<CycleTestState>();
        (s.body_idx, s.land_idx, s.stage, s.stage_t)
    };
    if body_idx >= CYCLE_BODIES.len() {
        // El `DONE` no es decorativo: `scripts/qa-android.sh` decide el veredicto con
        // `grep -qE 'DONE|PASS'` sobre el log. Sin esa palabra, este driver terminaba
        // perfecto —los cinco cuerpos ida y vuelta— y la suite lo reportaba como "murió
        // sin DONE". Un driver que pasa contado como fallo es tan malo como uno que falla
        // contado como verde: las dos veces el reporte miente.
        eprintln!("ha-cycletest: ALL BODIES ROUND-TRIPPED OK (no orbital soft-lock)");
        eprintln!("ha-cycletest: DONE");
        std::process::exit(0);
    }
    let body = CYCLE_BODIES[body_idx];
    let phase = world.resource::<crate::creative_fly::CreativeFlight>().phase;
    let dt = now - stage_t;
    let set = |w: &mut World, st: u8| {
        let mut s = w.resource_mut::<CycleTestState>();
        s.stage = st;
        s.stage_t = now;
    };
    match stage {
        // Setup a landing through the real orbit→descend path.
        0 => {
            place_orbit_to_land(world, body);
            set(world, 1);
        }
        // Once the landing branch handed us to Descend, drop the altitude so the
        // fall touches down fast.
        1 => {
            // Bajar MANTENIENDO USAR — el camino real del jugador. Nada de
            // forzar posiciones: si el descenso no llega al suelo solo, es un
            // bug de verdad y el test debe verlo.
            sostener(world, Accion::Usar, now, 0.4);
            if phase == crate::creative_fly::Phase::Ground {
                set(world, 2);
            } else if dt > CYCLE_LEG_S {
                // El detalle COMPLETO, no sólo `alt`. La altura efectiva es la hipotenusa
                // entre lo que bajaste (`vert`) y lo lejos que estás del borde del cuerpo
                // (el hueco lateral), así que un `alt` chico no dice cuál de las dos te
                // dejó en el aire — y ésa es justamente la pregunta.
                let (fp, fa, vert, thrust, vy, armed) = {
                    let f = world.resource::<crate::creative_fly::CreativeFlight>();
                    (f.phase, f.alt, f.vert, f.thrust, f.vy, f.land_armed)
                };
                let refb = world.resource::<crate::universe::UniverseFrame>().ref_body;
                let t = world.resource::<crate::solar_system_clock::SolarSystemClock>().t;
                let mut q = world.query_filtered::<&SimState, With<Player>>();
                let here = q
                    .get_single(world)
                    .map(|s| Vec2::new(s.body.x, -s.body.z))
                    .unwrap_or(Vec2::ZERO);
                let (near, dist) = crate::universe::nearest_landable(here, refb, t);
                let gap = dist - ha_shared::body_walk_radius(near);
                report_bug(
                    "landing_cycle",
                    "never_touched_down",
                    &format!(
                        "{body:?} land#{land_idx} phase={fp:?} alt={fa:.1} vert={vert:.1} gap={gap:.1} \
                         thrust={thrust:.2} vy={vy:.1} armed={armed} ref={refb:?} near={near:?} pos=({:.0},{:.0})",
                        here.x, here.y
                    ),
                );
                std::process::exit(1);
            }
        }
        // Touched down → assert the surface state is coherent.
        2 => {
            if phase == crate::creative_fly::Phase::Ground {
                let region = world.resource::<crate::scene::RegionRuntime>().active;
                let ctx = world.resource::<crate::scene::WorldContextRuntime>().active;
                let pbody = world.resource::<crate::world2d::ActivePlanet>().body;
                let (want_r, want_c) = crate::scene::surface_region_for(body);
                let region_ok = region == want_r;
                let ctx_ok = ctx == want_c;
                let planet_ok = matches!(body, ha_shared::CelestialBody::Earth | ha_shared::CelestialBody::Moon)
                    || pbody == Some(body);
                if !(region_ok && ctx_ok && planet_ok) {
                    report_bug("landing_cycle", "incoherent_surface_state", &format!("{body:?} land#{land_idx} region={region:?} ctx={ctx:?} planet={pbody:?}"));
                    std::process::exit(1);
                }
                eprintln!("ha-cycletest: {body:?} land#{land_idx} OK");
                let next_land = land_idx + 1;
                if next_land < CYCLE_LANDS {
                    world.resource_mut::<CycleTestState>().land_idx = next_land;
                    set(world, 3); // take off, then land again
                } else {
                    let mut s = world.resource_mut::<CycleTestState>();
                    s.land_idx = 0;
                    s.body_idx += 1;
                    s.stage = 0;
                    s.stage_t = now;
                }
            } else if dt > 20.0 {
                report_bug("landing_cycle", "never_reached_ground", &format!("{body:?} land#{land_idx}"));
                std::process::exit(1);
            }
        }
        // Despegar de VERDAD: mantener SALTO desde el suelo hasta cruzar la
        // línea de espacio. Es el otro medio ciclo del bug reportado ("aterricé
        // y ya no pude volver a salir"), así que se ejerce por el camino real.
        3 => {
            sostener(world, Accion::Saltar, now, 0.4);
            if phase == crate::creative_fly::Phase::Orbit {
                zero_flight_input(world);
                set(world, 0); // otra vez a preparar el aterrizaje
            } else if dt > CYCLE_LEG_S {
                report_bug("landing_cycle", "never_took_off_again", &format!("{body:?} phase={phase:?}"));
                std::process::exit(1);
            }
        }
        _ => {}
    }
    if now > CYCLE_BUDGET_S {
        report_bug("landing_cycle", "timeout", &format!("body_idx={body_idx} stage={stage}"));
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// HA_BUILDTEST — construcción por bordes (piso en la celda, pared en la arista)
// ---------------------------------------------------------------------------

/// Lado del cuarto de prueba, en tiles.
const BT_SIDE: i64 = 3;

#[derive(Resource, Default)]
struct BuildTestState {
    step: u32,
    next_t: f32,
    base: (i64, i64),
    /// Mira sostenida frame a frame (si no, el cursor de aim se mueve solo).
    aim: Vec2,
    /// **El joystick SOSTENIDO.** La máquina de etapas corre cada 0.05 s pero la
    /// actuación se borra cada frame, así que pedir el movimiento dentro de una
    /// etapa sólo empuja uno de cada tres frames. Da igual para acercarse a una
    /// pared; no da igual para lo que se mide por TIEMPO CONTINUO: andar un tramo
    /// de escalera fallaba por esto y no por el juego.
    empuje: Vec2,
    empuje_hasta: f32,
    /// **El botón de TREPAR, a pulsos.** Trepar el borde de un pozo dejó de ser
    /// "sostené el empuje 0.45 s" y pasó a ser una pulsación
    /// (`escaleras::trepar_borde`), así que el driver tiene que pulsar como una
    /// persona. A pulsos y no sostenido porque la trepada entra por el FLANCO del
    /// botón: un dedo apoyado sube un nivel y nada más, y el paso no se podría
    /// reintentar mientras el rumbo del personaje se asienta contra la pared.
    saltar_hasta: f32,
    walk_until: f32,
    walls_before: usize,
    start_x: f32,
    failed: bool,
    /// Reintentos del paso en curso (ver `bt_retry`).
    retries: u32,
}

/// Tope de reintentos de un paso antes de darlo por fallado.
///
/// Generoso a propósito. Los pasos que se miden por TIEMPO DE JUEGO —andar un
/// tramo de escalera, trepar el borde de un pozo— necesitan segundos de reloj, y
/// el driver corre en `debug` con backend GL: bajo carga un frame pasa de 0.2 s y
/// treinta reintentos se quedaban cortos. El resultado eran fallos intermitentes
/// que no hablaban del juego sino de cuán ocupada estaba la máquina.
const BT_MAX_RETRIES: u32 = 90;

/// **Repetir el paso hasta que la pieza esté puesta de verdad.**
///
/// El driver marca `HoldInteractState::completed` a mano, pero ese recurso lo
/// reescribe el pipeline de input real todos los frames: según en qué orden
/// caigan los sistemas ese frame, el hold se consume o se pisa antes de que
/// `apply_build_hold` lo vea. Colocar "una pieza por paso" perdía una de cada
/// tantas, y la prueba fallaba con 10 paredes de 12 sin que hubiera nada roto.
///
/// Reintentar hasta comprobar el efecto convierte al driver en determinista sin
/// tocar el juego: se mide lo que quedó puesto, no lo que se intentó.
fn bt_retry(world: &mut World, step: u32, now: f32) -> bool {
    let mut st = world.resource_mut::<BuildTestState>();
    st.retries += 1;
    if st.retries > BT_MAX_RETRIES {
        st.retries = 0;
        return false;
    }
    st.step = step;
    st.next_t = now + 0.05;
    true
}

/// Bordes del perímetro del cuarto de prueba, en orden.
fn bt_edges(base: (i64, i64)) -> Vec<(i64, i64, ha_shared::build_grid::WallAxis)> {
    use ha_shared::build_grid::WallAxis::{H, V};
    let mut out = Vec::new();
    for i in 0..BT_SIDE {
        out.push((base.0 + i, base.1, H)); // norte
        out.push((base.0 + i, base.1 + BT_SIDE, H)); // sur
        out.push((base.0, base.1 + i, V)); // oeste
        out.push((base.0 + BT_SIDE, base.1 + i, V)); // este
    }
    out
}

/// Punto de mundo que engancha (`nearest_edge`) exactamente a ese borde.
fn bt_aim_for(tx: i64, tz: i64, axis: ha_shared::build_grid::WallAxis) -> Vec2 {
    match axis {
        ha_shared::build_grid::WallAxis::H => Vec2::new(tx as f32 + 0.5, tz as f32 + 0.08),
        ha_shared::build_grid::WallAxis::V => Vec2::new(tx as f32 + 0.08, tz as f32 + 0.5),
    }
}

fn complete_build_hold(world: &mut World) {
    {
        let mut h = world.resource_mut::<HoldInteractState>();
        h.active = false;
        h.completed = true;
        h.placing_build = true;
        h.placing_block = false;
        h.placing_machine = false;
    }
}

/// **Pulsa COLOCAR, pero sólo si el fantasma ya está donde queremos.**
///
/// Construir son dos gestos desde que existe la previsualización
/// (`build::MiraDeObra`): tocar CLAVA el fantasma, la chapa lo confirma. Para un
/// driver que reintenta, aceptar a ciegas es un bug garantizado — al colocar, el
/// fantasma se corre solo al sitio siguiente, así que una segunda confirmación
/// pone la pieza en el de al lado. Se vio como *"11 paredes en el anillo, 12 en el
/// mundo"*: una quedaba fuera de la caja.
///
/// Mirar antes de aceptar es además lo que hace una persona, que es el punto
/// entero de la función nueva.
fn confirmar_si_apunta(world: &mut World, esperado: crate::build::Sitio) -> bool {
    let apunta = world.resource::<crate::build::MiraDeObra>().fijada == Some(esperado);
    if apunta {
        world.resource_mut::<crate::build::MiraDeObra>().confirmar_desde_driver();
    }
    apunta
}

fn bt_body(world: &mut World) -> ha_shared::CelestialBody {
    let ctx = world.resource::<crate::scene::WorldContextRuntime>().clone();
    let planet = *world.resource::<crate::world2d::ActivePlanet>();
    crate::world2d::active_surface_body(&ctx, &planet)
}

fn bt_wall_count(world: &mut World) -> usize {
    world.resource::<crate::walls::BuiltWalls>().list.len()
}

/// Cuántos bordes DEL ANILLO de prueba tienen pared. No es lo mismo que `bt_wall_count`, y
/// confundirlos era un bug: aquél cuenta TODAS las paredes del mundo, así que comparar su
/// total contra las 12 que el anillo espera falla apenas hay una pared previa en el mundo
/// guardado — y la hay, porque la suite no borra los datos de la app entre drivers y
/// `world_edits.json` trae lo que el usuario haya construido. Daba "18 paredes, esperadas
/// 12" con el anillo perfectamente levantado.
///
/// Para el chequeo de "la pared sin apoyo no cambió nada" el total global sigue sirviendo:
/// ahí lo que importa es la DIFERENCIA antes/después, no el valor absoluto.
fn bt_ring_wall_count(world: &mut World, base: (i64, i64)) -> usize {
    let body = bt_body(world);
    let z = world.resource::<TileWorld>().z;
    let walls = world.resource::<crate::walls::BuiltWalls>();
    bt_edges(base)
        .into_iter()
        .filter(|&(tx, tz, axis)| walls.at(body, z, tx, tz, axis).is_some())
        .count()
}

/// **Un solar despejado para el cuarto de prueba.**
///
/// Antes el cuarto se plantaba a ciegas en `(jugador+1, jugador-1)`. Si ahí caía
/// una roca, un acantilado o un charco, la prueba fallaba —"3 celdas sin piso",
/// "el jugador no avanzó hacia el muro"— sin que hubiera nada roto: el resultado
/// dependía de dónde hubiera nacido el jugador. Un driver que sale cara o cruz no
/// sólo miente, TAPA las regresiones de verdad, porque uno se acostumbra a verlo
/// en rojo.
///
/// Se busca en espiral el primer solar `BT_SIDE+5` de lado con todas sus celdas
/// libres (ni sólido, ni decor cosechable), dejando un tile de margen para que el
/// muro este tenga a dónde apoyarse.
fn bt_clear_base(world: &mut World) -> (i64, i64) {
    let (px, pz) = player_cell(world);
    let body = bt_body(world);
    let side = BT_SIDE + 5;
    let mut best = (px + 1, pz - 1);
    for r in 1..=12i64 {
        for dz in -r..=r {
            for dx in -r..=r {
                if dx.abs() != r && dz.abs() != r {
                    continue; // sólo el anillo nuevo
                }
                let cand = (px + dx, pz + dz);
                let flora = world.resource::<crate::harvest::FloraHarvest>();
                let tiles = world.resource::<TileWorld>();
                let z = tiles.z;
                let clear = (0..side).all(|oz| {
                    (0..side).all(|ox| {
                        let (cx, cz) = (cand.0 + ox, cand.1 + oz);
                        !tiles.solid_at(body, cx, cz)
                            && !crate::world2d::cell_build_blocked(flora, body, z, cx, cz)
                    })
                });
                if clear {
                    return cand;
                }
            }
        }
    }
    eprintln!("ha-verify: BUILDTEST sin solar despejado cerca — se usa {best:?}");
    best = (px + 1, pz - 1);
    best
}

/// La celda AISLADA del solar de prueba: tiene piso pero ninguna pared, así que
/// la losa que se le apoye encima depende SÓLO de ella. Es lo que hace medible el
/// derrumbe (C5): dentro del cuarto, el anillo de paredes sostiene el piso de
/// arriba y no se cae nada — que es lo correcto, pero no prueba nada.
fn bt_lone_cell(base: (i64, i64)) -> (i64, i64) {
    (base.0 + 6, base.1 + 1)
}

/// La celda donde el driver cava el pozo. Fuera del cuarto y de la losa aislada:
/// lo que se mide es el suelo pelado, no una losa colocada.
fn bt_dig_cell(base: (i64, i64)) -> (i64, i64) {
    (base.0 + 6, base.1 + 4)
}

/// Celda del PIE del tramo de prueba. En campo abierto, al sur del cuarto: un
/// tramo mide cuatro celdas y dentro del cuarto de 3×3 la cuarta cae sobre la
/// pared este, o sea que el driver empujaría contra un muro.
fn bt_stairs_cell(base: (i64, i64)) -> (i64, i64) {
    (base.0 + 1, base.1 + 6)
}

/// Índice de paleta del TRAMO de escalera (por id, no por posición: reordenar el
/// catálogo no puede hacer que la prueba coloque una vereda).
fn bt_stairs_piece() -> usize {
    ha_shared::topdown::BUILD_PIECES
        .iter()
        .position(|p| p.id == "build_stairs")
        .unwrap_or(0)
}

/// Planta al jugador en el centro de un punto del mundo, quieto.
fn bt_teleport(world: &mut World, x: f32, z: f32) {
    let mut q = world.query_filtered::<&mut SimState, With<Player>>();
    if let Ok(mut sim) = q.get_single_mut(world) {
        sim.body.x = x;
        sim.body.z = z;
        sim.body.vx = 0.0;
        sim.body.vz = 0.0;
    }
}

fn build_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 2.0 || !player_ready(world) {
        return;
    }
    // La mira se sostiene TODOS los frames: `apply_build_hold` puede consumir el
    // hold un frame después de que lo marcamos.
    {
        let aim = world.resource::<BuildTestState>().aim;
        if aim != Vec2::ZERO {
            world.resource_mut::<crate::input::AimCursor>().world = aim;
        }
    }
    // El empuje también se sostiene todos los frames, por encima del reloj de la
    // máquina de etapas (ver `BuildTestState::empuje`).
    {
        let st = world.resource::<BuildTestState>();
        let (empuje, hasta) = (st.empuje, st.empuje_hasta);
        if empuje != Vec2::ZERO && now < hasta {
            actuar(world, Accion::Mover(empuje));
        } else if empuje != Vec2::ZERO {
            world.resource_mut::<BuildTestState>().empuje = Vec2::ZERO;
        }
    }
    // Y el botón de trepar, a pulsos (ver `BuildTestState::saltar_hasta`). El
    // ciclo es 0.2 s pulsado / 0.4 s suelto: el hueco tiene que superar el búfer
    // de 0.16 s de `hold_jump`, o el sostenido nunca baja y no hay flanco nuevo.
    if now < world.resource::<BuildTestState>().saltar_hasta && (now / 0.6).fract() < 0.34 {
        actuar(world, Accion::Saltar);
    }
    if now < world.resource::<BuildTestState>().next_t {
        return;
    }
    let step = world.resource::<BuildTestState>().step;
    {
        let mut st = world.resource_mut::<BuildTestState>();
        st.step += 1;
        st.next_t = now + 0.05;
    }
    // +1: la losa AISLADA de `bt_lone_cell`, la única sin paredes alrededor y por
    // eso la única que puede quedarse sin apoyo cuando se pica lo de abajo.
    let floors = (BT_SIDE * BT_SIDE) as u32 + 1;
    let edges_n = bt_edges((0, 0)).len() as u32;

    match step {
        // Setup: madera de sobra, modo construir, pieza = piso, cuarto al lado.
        0 => {
            world.resource_mut::<crate::ui_editor::UiEditorState>().creative = false;
            world.resource_mut::<PlayerInventory>().add("wood", 999);
            world.resource_mut::<crate::input::BuildModeActive>().active = true;
            world.resource_mut::<crate::input::AimCursor>().locked = true;
            world.resource_mut::<crate::build::SelectedBuildPiece>().0 = 0;
            let base = bt_clear_base(world);
            world.resource_mut::<BuildTestState>().base = base;
        }
        // Piso: una celda por paso, por el camino real (LookTarget + hold).
        s if s >= 1 && s <= floors => {
            let base = world.resource::<BuildTestState>().base;
            let i = (s - 1) as i64;
            let (cx, cz) = if i == BT_SIDE * BT_SIDE {
                bt_lone_cell(base)
            } else {
                (base.0 + i % BT_SIDE, base.1 + i / BT_SIDE)
            };
            let body = bt_body(world);
            let done = {
                let flora = world.resource::<crate::harvest::FloraHarvest>();
                let tiles = world.resource::<TileWorld>();
                tiles.tile_at(body, cx, cz) == ha_shared::topdown::Tile::WoodFloor
                    // Una celda con un árbol encima no admite piso, y está bien:
                    // ese paso se da por cumplido en vez de reintentar 30 veces.
                    || crate::world2d::cell_build_blocked(flora, body, tiles.z, cx, cz)
            };
            if done {
                world.resource_mut::<BuildTestState>().retries = 0;
            } else {
                world.resource_mut::<crate::build::SelectedBuildPiece>().0 = 0;
                lock_place_target(world, cx, cz);
                complete_build_hold(world);
                confirmar_si_apunta(world, crate::build::Sitio::Celda(cx, cz));
                bt_retry(world, s, now);
            }
        }
        // ¿Quedó el piso? Y pasamos a la pared de madera (pieza de borde).
        s if s == floors + 1 => {
            let base = world.resource::<BuildTestState>().base;
            let body = bt_body(world);
            // Una celda con un árbol/roca encima NO admite piso (hay que talar
            // primero): eso es correcto, así que sólo exigimos las libres.
            let flora = world.resource::<crate::harvest::FloraHarvest>().clone();
            let tiles = world.resource::<TileWorld>();
            let z = tiles.z;
            let missing = (0..BT_SIDE)
                .flat_map(|dz| (0..BT_SIDE).map(move |dx| (dx, dz)))
                .filter(|&(dx, dz)| {
                    let (cx, cz) = (base.0 + dx, base.1 + dz);
                    !crate::world2d::cell_build_blocked(&flora, body, z, cx, cz)
                        && tiles.tile_at(body, cx, cz) != ha_shared::topdown::Tile::WoodFloor
                })
                .count();
            if missing > 0 {
                report_bug("build", "floor_placed", &format!("{missing} celdas libres sin piso"));
                world.resource_mut::<BuildTestState>().failed = true;
            }
            world.resource_mut::<crate::build::SelectedBuildPiece>().0 =
                crate::build::edge_palette_base();
        }
        // Paredes: un borde del perímetro por paso.
        s if s > floors + 1 && s <= floors + 1 + edges_n => {
            let base = world.resource::<BuildTestState>().base;
            let (tx, tz, axis) = bt_edges(base)[(s - floors - 2) as usize];
            let body = bt_body(world);
            let (placed, supported) = {
                let tiles = world.resource::<TileWorld>();
                let z = tiles.z;
                let has_floor = |cx: i64, cz: i64| {
                    ha_shared::build_grid::is_build_floor(tiles.tile_at(body, cx, cz))
                };
                let sup = ha_shared::build_grid::wall_supported(&has_floor, tx, tz, axis);
                let walls = world.resource::<crate::walls::BuiltWalls>();
                (walls.at(body, z, tx, tz, axis).is_some(), sup)
            };
            // Ya puesta —o imposible de poner porque no hay piso de ningún lado—:
            // el paso cumplió. Volver a colocar sobre un borde ocupado lo
            // DEMOLERÍA, que es justo el error que hay que no cometer acá.
            if placed || !supported {
                world.resource_mut::<BuildTestState>().retries = 0;
            } else {
                let aim = bt_aim_for(tx, tz, axis);
                world.resource_mut::<BuildTestState>().aim = aim;
                world.resource_mut::<crate::input::AimCursor>().world = aim;
                // El borde se toma de la mira, pero el targeting debe estar "en
                // alcance": con una celda apuntada alcanza.
                lock_place_target(world, tx, tz);
                // **Se pulsa COLOCAR UNA sola vez por borde.**
                //
                // Al aceptar, el fantasma se corre al borde siguiente
                // (`MiraDeObra::avanzar`), que es justo la comodidad que compra
                // este gesto: seguir aceptando levanta el muro entero de una fila.
                // Para un driver que REINTENTA, en cambio, una segunda
                // confirmación coloca en el borde de al lado —y ahí se veía como
                // "11 paredes en el anillo, 12 en el mundo": una quedaba fuera de
                // la caja—. Los reintentos esperan el resultado; no vuelven a
                // aceptar.
                complete_build_hold(world);
                confirmar_si_apunta(world, crate::build::Sitio::Borde(tx, tz, axis));
                bt_retry(world, s, now);
            }
        }
        // ¿Se levantó el anillo completo?
        s if s == floors + edges_n + 2 => {
            // Esperadas = los bordes con piso de algún lado (los demás los
            // rechaza la regla de apoyo, y está bien que lo haga).
            let base = world.resource::<BuildTestState>().base;
            let body = bt_body(world);
            let expected = {
                let tiles = world.resource::<TileWorld>();
                let has_floor = |cx: i64, cz: i64| {
                    ha_shared::build_grid::is_build_floor(tiles.tile_at(body, cx, cz))
                };
                bt_edges(base)
                    .into_iter()
                    .filter(|&(tx, tz, axis)| {
                        ha_shared::build_grid::wall_supported(&has_floor, tx, tz, axis)
                    })
                    .count()
            };
            // El anillo se juzga por SUS bordes; el total del mundo se guarda aparte para
            // el chequeo de diferencia que viene después.
            let n = bt_ring_wall_count(world, base);
            let total = bt_wall_count(world);
            world.resource_mut::<BuildTestState>().walls_before = total;
            // **Retrato de la caja recién levantada.** Los invariantes de acá
            // cuentan paredes; no pueden ver que se monten una encima de otra ni
            // que una pieza salga con la forma equivocada, que es exactamente
            // como se reportaron los dos últimos bugs de construcción (la pared
            // solapada y el vano dibujado como un palo). Eso se mira.
            shot(world, &format!("{}/build-anillo.png", flytest_dir()));
            if n != expected {
                report_bug(
                    "build",
                    "walls_placed",
                    &format!("{n} paredes en el anillo, esperadas {expected} (total mundo {total})"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            // Pared SIN piso a ningún lado (3 tiles al norte del cuarto, dentro
            // del alcance): la regla de apoyo debe rechazarla.
            let far = (base.0, base.1 - 3);
            let aim = bt_aim_for(far.0, far.1, ha_shared::build_grid::WallAxis::H);
            world.resource_mut::<BuildTestState>().aim = aim;
            // También en el acto, no sólo en `st.aim`: `apply_build_hold` puede
            // consumir el hold en ESTE frame, y con la mira vieja el gesto caía
            // sobre el último borde del perímetro —que sí está ocupado— y lo
            // DEMOLÍA. La prueba de "pared sin piso" terminaba midiendo un
            // derribo, y el conteo bajaba en vez de quedarse igual.
            world.resource_mut::<crate::input::AimCursor>().world = aim;
            lock_place_target(world, far.0, far.1);
            complete_build_hold(world);
            confirmar_si_apunta(
                world,
                crate::build::Sitio::Borde(far.0, far.1, ha_shared::build_grid::WallAxis::H),
            );
        }
        s if s == floors + edges_n + 3 => {
            let n = bt_wall_count(world);
            let before = world.resource::<BuildTestState>().walls_before;
            if n != before {
                report_bug(
                    "build",
                    "wall_needs_floor",
                    &format!("la pared sin piso cambió el conteo: {before} → {n}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            // Al centro del cuarto y a caminar hacia el muro este.
            unlock(world);
            world.resource_mut::<crate::input::AimCursor>().locked = false;
            world.resource_mut::<BuildTestState>().aim = Vec2::ZERO;
            let base = world.resource::<BuildTestState>().base;
            let (cx, cz) = (base.0 as f32 + 0.5, base.1 as f32 + BT_SIDE as f32 * 0.5);
            let mut q = world.query_filtered::<&mut SimState, With<Player>>();
            if let Ok(mut sim) = q.get_single_mut(world) {
                sim.body.x = cx;
                sim.body.z = cz;
                sim.body.vx = 0.0;
                sim.body.vz = 0.0;
            }
            let mut st = world.resource_mut::<BuildTestState>();
            st.start_x = cx;
            st.walk_until = now + 2.5;
        }
        // Caminar hacia el este contra la pared. Con el empuje SOSTENIDO (ver
        // `BuildTestState::empuje`): a pulsos de un frame, bajo carga el jugador
        // no llegaba ni a salir de su celda y el fallo hablaba de la máquina.
        s if s == floors + edges_n + 4 => {
            let until = world.resource::<BuildTestState>().walk_until;
            let mut st = world.resource_mut::<BuildTestState>();
            st.next_t = now + 0.05;
            st.empuje = Vec2::new(1.0, 0.0);
            st.empuje_hasta = until;
            if now < until {
                st.step = s; // repetir este paso hasta cumplir el tiempo
            }
        }
        // Veredicto del muro: no lo cruzó, pero sí avanzó hasta él.
        s if s == floors + edges_n + 5 => {
            release_all(world);
            world.resource_mut::<BuildTestState>().empuje = Vec2::ZERO;
            let base = world.resource::<BuildTestState>().base;
            let x = player_x(world);
            let wall_x = (base.0 + BT_SIDE) as f32;
            if x >= wall_x {
                report_bug(
                    "build",
                    "wall_blocks_player",
                    &format!("el jugador cruzó la pared: x={x:.2} ≥ {wall_x:.2}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            if x <= base.0 as f32 + 1.0 {
                report_bug(
                    "build",
                    "player_reaches_wall",
                    &format!("el jugador no avanzó hacia el muro: x={x:.2}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            world.resource_mut::<crate::input::AimCursor>().locked = true;
        }
        // --- SEGUNDO PISO (Project Zomboid) ------------------------------------
        // **La escalera se CAMINA.** El driver no se teletransporta a la cima ni
        // toca `tiles.z`: coloca el tramo, se planta en el pie y ANDA. Si algún
        // día alguien vuelve a poner el cambio de planta en una celda, este paso
        // es el que lo caza — con un teletransporte sí subiría, caminando no.
        s if s == floors + edges_n + 6 => {
            let base = world.resource::<BuildTestState>().base;
            let body = bt_body(world);
            let pie = bt_stairs_cell(base);
            let puesto = world
                .resource::<crate::escaleras::Escaleras>()
                .en(body, 0, pie.0, pie.1)
                .is_some();
            if puesto {
                world.resource_mut::<BuildTestState>().retries = 0;
            } else {
                world.resource_mut::<crate::build::SelectedBuildPiece>().0 = bt_stairs_piece();
                // El tramo sube hacia el ESTE, fijado con el chip: dejarlo en AUTO
                // haría que la prueba dependa de dónde quedó parado el driver.
                world.resource_mut::<crate::build::BuildFacing>().0 = Some(3);
                lock_place_target(world, pie.0, pie.1);
                complete_build_hold(world);
                confirmar_si_apunta(world, crate::build::Sitio::Celda(pie.0, pie.1));
                bt_retry(world, s, now);
            }
        }
        // ¿Entró el tramo entero, con sus cuatro escalones y su rellano?
        s if s == floors + edges_n + 7 => {
            let base = world.resource::<BuildTestState>().base;
            let body = bt_body(world);
            let paso = ha_shared::topdown::Tile::Step;
            let pie = bt_stairs_cell(base);
            let mal = (0..ha_shared::escaleras::PASOS as i64).find(|i| {
                world.resource::<TileWorld>().tile_at(body, pie.0 + i, pie.1) != paso
            });
            if let Some(i) = mal {
                report_bug("build", "stairs_placed", &format!("falta el escalón {i}"));
                world.resource_mut::<BuildTestState>().failed = true;
            }
            unlock(world);
            world.resource_mut::<crate::build::BuildFacing>().0 = None;
            // Al PIE de la escalera, no en la cima: lo que se prueba es andarla.
            bt_teleport(world, pie.0 as f32 + 0.5, pie.1 as f32 + 0.5);
            world.resource_mut::<BuildTestState>().retries = 0;
        }
        // Andarla: se empuja hacia el este hasta que la planta cambia sola.
        s if s == floors + edges_n + 8 => {
            let base = world.resource::<BuildTestState>().base;
            let body = bt_body(world);
            let z = world.resource::<TileWorld>().z;
            if z != 1 {
                // Sostener el este mientras se anda: subir cuatro escalones lleva
                // tiempo de juego, que es justamente el punto de la prueba.
                {
                    let mut st = world.resource_mut::<BuildTestState>();
                    st.empuje = Vec2::new(1.0, 0.0);
                    st.empuje_hasta = now + 1.0;
                }
                if bt_retry(world, s, now) {
                    return;
                }
                report_bug(
                    "build",
                    "stairs_walk_up",
                    &format!("andar el tramo no subió de planta: z={z}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            release_all(world);
            {
                let mut st = world.resource_mut::<BuildTestState>();
                st.empuje = Vec2::ZERO;
                st.retries = 0;
            }
            // Y llega a algún sitio: el rellano es piso pisable, no vacío.
            let pie = bt_stairs_cell(base);
            let (rx, rz) = (pie.0 + ha_shared::escaleras::PASOS as i64, pie.1);
            let landing = world.resource::<TileWorld>().tile_at_level(body, 1, rx, rz);
            if !ha_shared::build_grid::is_build_floor(landing) {
                report_bug("build", "stairs_landing", &format!("rellano = {landing:?}"));
                world.resource_mut::<BuildTestState>().failed = true;
            }
            world.resource_mut::<crate::input::AimCursor>().locked = true;
            world.resource_mut::<crate::build::SelectedBuildPiece>().0 = 0; // piso madera
        }
        // Losa SOBRE el cuarto de abajo: entra (hay casa debajo).
        s if s == floors + edges_n + 9 => {
            let base = world.resource::<BuildTestState>().base;
            let lone = bt_lone_cell(base);
            let body = bt_body(world);
            // Dos losas: una SOBRE el cuarto (la aguanta la casa) y otra sobre la
            // celda aislada (la aguanta sólo su piso). La segunda es la que
            // después tiene que derrumbarse.
            let falta = {
                let tiles = world.resource::<TileWorld>();
                let piso = ha_shared::topdown::Tile::WoodFloor;
                if tiles.tile_at(body, base.0 + 2, base.1 + 2) != piso {
                    Some((base.0 + 2, base.1 + 2))
                } else if tiles.tile_at(body, lone.0, lone.1) != piso {
                    Some(lone)
                } else {
                    None
                }
            };
            match falta {
                None => world.resource_mut::<BuildTestState>().retries = 0,
                Some((cx, cz)) => {
                    world.resource_mut::<crate::build::SelectedBuildPiece>().0 = 0;
                    lock_place_target(world, cx, cz);
                    complete_build_hold(world);
                    confirmar_si_apunta(world, crate::build::Sitio::Celda(cx, cz));
                    bt_retry(world, s, now);
                }
            }
        }
        // Losa EN EL AIRE, lejos de todo: la regla de apoyo la rechaza.
        s if s == floors + edges_n + 10 => {
            let base = world.resource::<BuildTestState>().base;
            let body = bt_body(world);
            let up = world.resource::<TileWorld>().tile_at(body, base.0 + 2, base.1 + 2);
            if up != ha_shared::topdown::Tile::WoodFloor {
                report_bug(
                    "build",
                    "upper_floor_places",
                    &format!("no se pudo construir el segundo piso sobre la casa: {up:?}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            lock_place_target(world, base.0 + 30, base.1 + 30);
            complete_build_hold(world);
            confirmar_si_apunta(world, crate::build::Sitio::Celda(base.0 + 30, base.1 + 30));
        }
        // Veredicto del aire + salir de la losa para CAER al piso de abajo.
        s if s == floors + edges_n + 11 => {
            let base = world.resource::<BuildTestState>().base;
            let body = bt_body(world);
            let air = world.resource::<TileWorld>().tile_at(body, base.0 + 30, base.1 + 30);
            if air != ha_shared::topdown::Tile::Air {
                report_bug(
                    "build",
                    "upper_floor_needs_support",
                    &format!("se construyó un piso flotante en el vacío: {air:?}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            unlock(world);
            world.resource_mut::<crate::input::AimCursor>().locked = false;
            // El retrato del segundo piso. Se saca en su PROPIO paso y el salto al
            // vacío espera al siguiente: la captura de Bevy llega uno o dos frames
            // más tarde, así que sacarla y caerse en el mismo paso retrataba el
            // piso de abajo con el cartel "PISO 2" todavía puesto — un falso
            // positivo perfecto de "los vecinos caminan sobre tu losa".
            shot(world, &format!("{}/buildtest-piso2.png", flytest_dir()));
            world.resource_mut::<BuildTestState>().next_t = now + 0.6;
        }
        // Recién ahora: salir de la losa para CAER al piso de abajo.
        s if s == floors + edges_n + 12 => {
            let base = world.resource::<BuildTestState>().base;
            bt_teleport(world, base.0 as f32 + 30.5, base.1 as f32 + 30.5);
        }
        // C4 — DEMOLER con el gesto: mantener sobre el piso lo saca y devuelve el
        // material. Se pica la losa AISLADA de la planta baja.
        s if s == floors + edges_n + 13 => {
            let base = world.resource::<BuildTestState>().base;
            let lone = bt_lone_cell(base);
            let body = bt_body(world);
            let z = world.resource::<TileWorld>().z;
            if z != 0 {
                // Todavía cayendo: esperar sin gastar el intento.
                bt_retry(world, s, now);
                return;
            }
            let sigue = world.resource::<TileWorld>().tile_at(body, lone.0, lone.1)
                == ha_shared::topdown::Tile::WoodFloor;
            if !sigue {
                world.resource_mut::<BuildTestState>().retries = 0;
            } else {
                bt_teleport(world, lone.0 as f32 + 0.5, lone.1 as f32 + 0.5);
                lock_block_target(world, lone.0, lone.1);
                complete_hold(world, false);
                bt_retry(world, s, now);
            }
        }
        // C4 + C5 — veredicto: el piso se fue, y con él la losa que aguantaba.
        s if s == floors + edges_n + 14 => {
            let base = world.resource::<BuildTestState>().base;
            let lone = bt_lone_cell(base);
            let body = bt_body(world);
            let (abajo, arriba) = {
                let tiles = world.resource::<TileWorld>();
                (
                    tiles.tile_at_level(body, 0, lone.0, lone.1),
                    tiles.tile_at_level(body, 1, lone.0, lone.1),
                )
            };
            if abajo == ha_shared::topdown::Tile::WoodFloor {
                report_bug(
                    "build",
                    "floor_can_be_demolished",
                    "mantener sobre el piso no lo saco",
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            if arriba == ha_shared::topdown::Tile::WoodFloor {
                report_bug(
                    "build",
                    "unsupported_upper_floor_collapses",
                    "la losa de arriba quedo flotando sin nada debajo",
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            unlock(world);
        }
        // ── Voxel 2D: cavar hacia abajo ─────────────────────────────────────
        // C7 — CAVAR el suelo: la celda queda hueca y la de abajo, abierta.
        s if s == floors + edges_n + 15 => {
            let base = world.resource::<BuildTestState>().base;
            let pozo = bt_dig_cell(base);
            let body = bt_body(world);
            let hecho =
                world.resource::<TileWorld>().tile_at(body, pozo.0, pozo.1) == ha_shared::topdown::Tile::Air;
            if hecho {
                world.resource_mut::<BuildTestState>().retries = 0;
            } else {
                // De pie AL LADO del pozo: cavando bajo tus propios pies te caés
                // antes de poder mirar lo que quedó.
                bt_teleport(world, pozo.0 as f32 - 0.5, pozo.1 as f32 + 0.5);
                lock_dig_target(world, pozo.0, pozo.1);
                complete_hold(world, false);
                bt_retry(world, s, now);
            }
        }
        // C7 — veredicto del pozo, y meterse en él para CAER a la cueva.
        s if s == floors + edges_n + 16 => {
            let base = world.resource::<BuildTestState>().base;
            let pozo = bt_dig_cell(base);
            let body = bt_body(world);
            let (hueco, fondo) = {
                let tiles = world.resource::<TileWorld>();
                (
                    tiles.tile_at_level(body, 0, pozo.0, pozo.1),
                    tiles.tile_at_level(body, -1, pozo.0, pozo.1),
                )
            };
            if hueco != ha_shared::topdown::Tile::Air {
                report_bug("build", "dig_leaves_a_hole", &format!("cavar dejó {hueco:?}"));
                world.resource_mut::<BuildTestState>().failed = true;
            }
            if fondo.solid() {
                report_bug(
                    "build",
                    "dig_opens_the_cavity",
                    &format!("bajo el pozo quedó roca maciza: {fondo:?}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            unlock(world);
            bt_teleport(world, pozo.0 as f32 + 0.5, pozo.1 as f32 + 0.5);
        }
        // C7 — veredicto de la caída: el pozo te deja en el subsuelo, sin cambio
        // de escena.
        s if s == floors + edges_n + 17 => {
            let z = world.resource::<TileWorld>().z;
            if z == 0 {
                if bt_retry(world, s, now) {
                    return; // la caída la resuelve `fall_through_void` en Update
                }
                report_bug("build", "pit_drops_you", "el pozo no te bajó de nivel");
                world.resource_mut::<BuildTestState>().failed = true;
            }
            world.resource_mut::<BuildTestState>().retries = 0;
            let base = world.resource::<BuildTestState>().base;
            let pozo = bt_dig_cell(base);
            // El retrato que importa: estás BAJO TIERRA y se ve tierra — sin
            // pasto, sin vecinos y sin mariposas colgados de la capa de arriba
            // (`fauna::cull_fauna`, `society`, `ambient`). Lo que dice que no hubo
            // cambio de escena no es que se siga viendo la calle: es que llegaste
            // caminando y que la luz bajó contigo (`interior::medir_encierro`).
            shot(world, &format!("{}/buildtest-tunel.png", flytest_dir()));
            bt_teleport(world, pozo.0 as f32 + 0.5, pozo.1 as f32 + 0.5);
        }
        // C7 — **del pozo se sale TREPANDO, y trepar se PULSA**. No hay grada
        // mágica en el fondo ni trepada automática: se mira el borde, el botón
        // pasa a decir TREPAR y se pulsa (`escaleras::trepar_borde`). El driver
        // hace exactamente eso — empujar para quedar de cara a la pared, y pulsar
        // —, que es lo único que puede hacer el jugador.
        s if s == floors + edges_n + 18 => {
            let base = world.resource::<BuildTestState>().base;
            let pozo = bt_dig_cell(base);
            bt_teleport(world, pozo.0 as f32 + 0.5, pozo.1 as f32 + 0.5);
            world.resource_mut::<BuildTestState>().retries = 0;
        }
        // C7 — veredicto de la salida: se trepa, y se termina FUERA del agujero.
        s if s == floors + edges_n + 19 => {
            let z = world.resource::<TileWorld>().z;
            if z != 0 {
                // Empujar hacia el oeste para quedar DE CARA a la pared del pozo
                // (`trepar_borde` mira el rumbo, no el empuje), y pulsar el botón
                // mientras tanto: la trepada es la pulsación, no el apoyo.
                {
                    let mut st = world.resource_mut::<BuildTestState>();
                    st.empuje = Vec2::new(-1.0, 0.0);
                    st.empuje_hasta = now + 1.0;
                    st.saltar_hasta = now + 1.0;
                }
                if bt_retry(world, s, now) {
                    return;
                }
                report_bug(
                    "build",
                    "pit_lets_you_out",
                    &format!("mirar el borde y pulsar TREPAR no te sacó del pozo: z={z}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            release_all(world);
            {
                let mut st = world.resource_mut::<BuildTestState>();
                st.empuje = Vec2::ZERO;
                st.saltar_hasta = 0.0;
                st.retries = 0;
            }
            let base = world.resource::<BuildTestState>().base;
            let pozo = bt_dig_cell(base);
            let body = bt_body(world);
            let (px, pz) = player_cell(world);
            if (px, pz) == pozo {
                report_bug(
                    "build",
                    "pit_lets_you_out",
                    "saliste del pozo DENTRO del pozo: te vas a volver a caer",
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            let borde = world.resource::<TileWorld>().tile_at(body, px, pz);
            if borde == ha_shared::topdown::Tile::Air || borde.solid() {
                report_bug(
                    "build",
                    "pit_lets_you_out",
                    &format!("el borde del pozo no es pisable: {borde:?}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            eprintln!("ha-verify: POZO — cavado, caída al subsuelo y salida TREPANDO el borde ({borde:?})");
            shot(world, &format!("{}/buildtest-pozo.png", flytest_dir()));
        }
        // Veredicto final.
        _ => {
            let z = world.resource::<TileWorld>().z;
            // La caída la resuelve `fall_through_void` en `Update`: se le dan los
            // frames de este arco (que se repite) antes de cantar el fallo.
            if z != 0 && step >= floors + edges_n + 22 {
                report_bug(
                    "build",
                    "void_drops_you",
                    &format!("salir de la losa no te bajó de piso: z={z}"),
                );
                world.resource_mut::<BuildTestState>().failed = true;
            }
            let walls = bt_wall_count(world);
            let failed = world.resource::<BuildTestState>().failed;
            eprintln!(
                "ha-verify: BUILDTEST {} — paredes={walls} z={z}",
                if failed { "FAIL" } else { "DONE" }
            );
            shot(world, &format!("{}/buildtest-room.png", flytest_dir()));
            // Un par de frames para que el screenshot llegue al disco.
            let mut st = world.resource_mut::<BuildTestState>();
            st.next_t = now + 1.5;
            if st.step > floors + edges_n + 23 {
                std::process::exit(if failed { 1 } else { 0 });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HA_FACHADATEST — la cara de afuera de una casa (8 ago 2026)
// ---------------------------------------------------------------------------

/// Lado del cuarto de fachada, en tiles. Tres alcanza: lo que se mira es la cara
/// del sur, y un cuarto más grande sólo aleja la cámara.
const FT_LADO: i64 = 3;

#[derive(Resource, Default)]
struct FachadaTestState {
    step: u32,
    next_t: f32,
    base: (i64, i64),
    aim: Vec2,
    reintentos: u32,
    fallo: bool,
}

/// El índice de paleta de una pieza de borde por id.
fn ft_pieza_de_borde(id: &str) -> usize {
    crate::build::edge_palette_base()
        + ha_shared::build_grid::edge_piece_index(id).unwrap_or(0)
}

/// El índice de paleta de una tapa por id.
fn ft_tapa(id: &str) -> usize {
    let i = ha_shared::build_grid::CEILING_PIECES
        .iter()
        .position(|c| c.id == id)
        .unwrap_or(0);
    crate::build::ceiling_palette_base() + i
}

/// Los bordes de la cara SUR del cuarto: la línea `z = base.z + lado`, eje H.
///
/// Es la única cara que Modern Exteriors sabe dibujar (§8), y la que
/// `walls::Cara::de` resuelve como `Cercano` — piso al norte, nada al sur.
fn ft_bordes_del_frente(base: (i64, i64)) -> Vec<(i64, i64, ha_shared::build_grid::WallAxis)> {
    (0..FT_LADO)
        .map(|dx| (base.0 + dx, base.1 + FT_LADO, ha_shared::build_grid::WallAxis::H))
        .collect()
}

fn ft_reintentar(world: &mut World, paso: u32, now: f32) {
    let mut st = world.resource_mut::<FachadaTestState>();
    st.reintentos += 1;
    if st.reintentos < 30 {
        st.step = paso;
        st.next_t = now + 0.08;
    } else {
        st.reintentos = 0;
    }
}

fn fachada_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 2.0 || !player_ready(world) {
        return;
    }
    // La mira se sostiene todos los frames (ver `build_test_drive`).
    {
        let aim = world.resource::<FachadaTestState>().aim;
        if aim != Vec2::ZERO {
            world.resource_mut::<crate::input::AimCursor>().world = aim;
        }
    }
    if now < world.resource::<FachadaTestState>().next_t {
        return;
    }
    let step = world.resource::<FachadaTestState>().step;
    {
        let mut st = world.resource_mut::<FachadaTestState>();
        st.step += 1;
        st.next_t = now + 0.05;
    }

    let pisos = (FT_LADO * FT_LADO) as u32;
    let frente_n = FT_LADO as u32;
    // Etapas: [1..pisos] piso · [+1..+frente_n] la cara del sur · [+1..+pisos] tumbado
    let fin_piso = pisos;
    let fin_frente = fin_piso + frente_n;
    let fin_tapa = fin_frente + pisos;

    match step {
        0 => {
            world.resource_mut::<crate::ui_editor::UiEditorState>().creative = false;
            // Madera para el piso y el tumbado; piedra porque la fachada cuesta
            // piedra (es una terminación, no un recurso nuevo).
            world.resource_mut::<PlayerInventory>().add("wood", 999);
            world.resource_mut::<PlayerInventory>().add("stone", 999);
            world.resource_mut::<crate::input::BuildModeActive>().active = true;
            world.resource_mut::<crate::input::AimCursor>().locked = true;
            world.resource_mut::<crate::build::SelectedBuildPiece>().0 = 0;
            let base = bt_clear_base(world);
            world.resource_mut::<FachadaTestState>().base = base;
        }
        // Piso de madera, celda por celda, por el camino real.
        s if s >= 1 && s <= fin_piso => {
            let base = world.resource::<FachadaTestState>().base;
            let i = (s - 1) as i64;
            let (cx, cz) = (base.0 + i % FT_LADO, base.1 + i / FT_LADO);
            let body = bt_body(world);
            let listo = {
                let flora = world.resource::<crate::harvest::FloraHarvest>();
                let tiles = world.resource::<TileWorld>();
                ha_shared::build_grid::is_build_floor(tiles.tile_at(body, cx, cz))
                    || crate::world2d::cell_build_blocked(flora, body, tiles.z, cx, cz)
            };
            if listo {
                world.resource_mut::<FachadaTestState>().reintentos = 0;
            } else {
                world.resource_mut::<crate::build::SelectedBuildPiece>().0 = 0;
                lock_place_target(world, cx, cz);
                complete_build_hold(world);
                confirmar_si_apunta(world, crate::build::Sitio::Celda(cx, cz));
                ft_reintentar(world, s, now);
            }
        }
        // **La cara del sur, con el material de fachada.** La del medio es la
        // PUERTA: es la que prueba que `pose_de_puerta` pide la hoja de fachada y
        // no la de interior en cuanto el jugador se le acerca.
        s if s > fin_piso && s <= fin_frente => {
            let base = world.resource::<FachadaTestState>().base;
            let i = (s - fin_piso - 1) as usize;
            let (tx, tz, axis) = ft_bordes_del_frente(base)[i];
            let body = bt_body(world);
            let puesta = {
                let z = world.resource::<TileWorld>().z;
                world.resource::<crate::walls::BuiltWalls>().at(body, z, tx, tz, axis).is_some()
            };
            if puesta {
                world.resource_mut::<FachadaTestState>().reintentos = 0;
            } else {
                let id = if i == FT_LADO as usize / 2 {
                    "edge_door_fachada_local"
                } else {
                    "edge_wall_fachada_local"
                };
                world.resource_mut::<crate::build::SelectedBuildPiece>().0 = ft_pieza_de_borde(id);
                let aim = bt_aim_for(tx, tz, axis);
                world.resource_mut::<FachadaTestState>().aim = aim;
                world.resource_mut::<crate::input::AimCursor>().world = aim;
                lock_place_target(world, tx, tz);
                complete_build_hold(world);
                confirmar_si_apunta(world, crate::build::Sitio::Borde(tx, tz, axis));
                ft_reintentar(world, s, now);
            }
        }
        // Tumbado sobre las nueve celdas: es LA combinación que estaba rota.
        s if s > fin_frente && s <= fin_tapa => {
            let base = world.resource::<FachadaTestState>().base;
            let i = (s - fin_frente - 1) as i64;
            let (cx, cz) = (base.0 + i % FT_LADO, base.1 + i / FT_LADO);
            let body = bt_body(world);
            let puesta = {
                let z = world.resource::<TileWorld>().z;
                world.resource::<crate::ceilings::BuiltCeilings>().at(body, z, cx, cz).is_some()
            };
            if puesta {
                world.resource_mut::<FachadaTestState>().reintentos = 0;
            } else {
                world.resource_mut::<crate::build::SelectedBuildPiece>().0 = ft_tapa("ceil_wood");
                lock_place_target(world, cx, cz);
                complete_build_hold(world);
                confirmar_si_apunta(world, crate::build::Sitio::Celda(cx, cz));
                ft_reintentar(world, s, now);
            }
        }
        // **¿Se levantó lo que se pidió?** Sin esto el driver miente: la primera
        // versión no colocó ni una fachada, midió la banda de las paredes de MADERA
        // que habían quedado de otra corrida en `walls.json` y cantó DONE.
        s if s == fin_tapa + 1 => {
            let base = world.resource::<FachadaTestState>().base;
            let body = bt_body(world);
            let z = world.resource::<TileWorld>().z;
            let puestas = ft_bordes_del_frente(base)
                .into_iter()
                .filter(|&(tx, tz, axis)| {
                    world
                        .resource::<crate::walls::BuiltWalls>()
                        .at(body, z, tx, tz, axis)
                        .is_some_and(|w| ha_shared::build_grid::es_fachada(w.def()))
                })
                .count();
            let tapas = (0..FT_LADO * FT_LADO)
                .filter(|i| {
                    let (cx, cz) = (base.0 + i % FT_LADO, base.1 + i / FT_LADO);
                    world.resource::<crate::ceilings::BuiltCeilings>().at(body, z, cx, cz).is_some()
                })
                .count();
            eprintln!(
                "ha-verify: FACHADA frente={puestas}/{FT_LADO} tapas={tapas}/{} base={base:?}",
                FT_LADO * FT_LADO
            );
            if puestas < FT_LADO as usize {
                report_bug(
                    "fachada",
                    "el_frente_se_coloca",
                    &format!("{puestas} de {FT_LADO} piezas de fachada en la cara sur"),
                );
                world.resource_mut::<FachadaTestState>().fallo = true;
            }
            if tapas == 0 {
                report_bug("fachada", "la_casa_se_tapa", "ninguna tapa puesta sobre el cuarto");
                world.resource_mut::<FachadaTestState>().fallo = true;
            }
        }
        // **Desde la CALLE.** El jugador se planta al sur del frente, fuera de la
        // huella: la fachada no puede fundirse (`Overhead::pared` tiene margen 0) y
        // tiene que verse por delante del tumbado.
        s if s == fin_tapa + 2 => {
            let base = world.resource::<FachadaTestState>().base;
            world.resource_mut::<crate::input::BuildModeActive>().active = false;
            world.resource_mut::<FachadaTestState>().aim = Vec2::ZERO;
            bt_teleport(
                world,
                base.0 as f32 + FT_LADO as f32 * 0.5,
                base.1 as f32 + FT_LADO as f32 + 2.5,
            );
            world.resource_mut::<FachadaTestState>().next_t = now + 1.2;
        }
        s if s == fin_tapa + 3 => {
            // **El invariante del trabajo entero**: nada de adentro de la casa puede
            // dibujarse delante de su cara exterior. Se mide sobre los sprites VIVOS,
            // no sobre la función: es lo único que prueba que el streaming les puso
            // la banda que corresponde.
            let (frente_min, tapa_max) = ft_bandas(world);
            eprintln!("ha-verify: FACHADA z_frente_min={frente_min:.3} z_tapa_max={tapa_max:.3}");
            if !(frente_min > tapa_max) {
                report_bug(
                    "fachada",
                    "frente_delante_de_su_tapa",
                    &format!("el tumbado (z={tapa_max:.3}) tapa la fachada (z={frente_min:.3})"),
                );
                world.resource_mut::<FachadaTestState>().fallo = true;
            }
            shot(world, &format!("{}/fachada-calle.png", flytest_dir()));
            world.resource_mut::<FachadaTestState>().next_t = now + 1.2;
        }
        // **Desde ADENTRO.** Cruzando la puerta la cáscara se funde y el interior se
        // lee: es la regla 8 (una transición es un fundido, no un corte).
        s if s == fin_tapa + 4 => {
            let base = world.resource::<FachadaTestState>().base;
            bt_teleport(
                world,
                base.0 as f32 + FT_LADO as f32 * 0.5,
                base.1 as f32 + FT_LADO as f32 - 0.5,
            );
            world.resource_mut::<FachadaTestState>().next_t = now + 1.5;
        }
        _ => {
            let alfa = ft_alfa_del_frente(world);
            eprintln!("ha-verify: FACHADA alfa_adentro={alfa:.3}");
            // Adentro tiene que borrarse: si no, la fachada de 4 m entierra el cuarto.
            if alfa > 0.6 {
                report_bug(
                    "fachada",
                    "el_frente_se_funde_al_entrar",
                    &format!("adentro la fachada sigue opaca (alfa={alfa:.3})"),
                );
                world.resource_mut::<FachadaTestState>().fallo = true;
            }
            shot(world, &format!("{}/fachada-adentro.png", flytest_dir()));
            let fallo = world.resource::<FachadaTestState>().fallo;
            eprintln!("ha-verify: FACHADATEST {}", if fallo { "FAIL" } else { "DONE" });
            let mut st = world.resource_mut::<FachadaTestState>();
            st.next_t = now + 1.5;
            if st.step > fin_tapa + 7 {
                std::process::exit(if fallo { 1 } else { 0 });
            }
        }
    }
}

/// `(z mínima de una cara exterior viva, z máxima de una tapa viva)`.
fn ft_bandas(world: &mut World) -> (f32, f32) {
    let mut frente = f32::INFINITY;
    let mut q = world.query_filtered::<&Transform, With<crate::walls::WallSprite>>();
    for tf in q.iter(world) {
        if tf.translation.z > crate::layers::OVERHEAD {
            frente = frente.min(tf.translation.z);
        }
    }
    let mut tapa = f32::NEG_INFINITY;
    let mut q2 = world.query_filtered::<&Transform, With<crate::ceilings::CeilingSprite>>();
    for tf in q2.iter(world) {
        tapa = tapa.max(tf.translation.z);
    }
    (frente, tapa)
}

/// El alfa **más bajo** de una cara exterior viva.
///
/// El mínimo y no el máximo: cada pieza de borde tiene su propia huella
/// (`walls::huella_que_tapa` marca UNA columna), así que de las tres columnas del
/// frente sólo se funde la que de verdad tapa al jugador — las otras dos siguen
/// opacas, y con razón. Midiendo el máximo el driver cantaba `alfa=1.000` con el
/// fundido funcionando perfectamente: el invariante estaba mal escrito, no el
/// juego.
fn ft_alfa_del_frente(world: &mut World) -> f32 {
    let mut menor = f32::INFINITY;
    let mut detalle: Vec<String> = Vec::new();
    let mut q = world
        .query_filtered::<(&Transform, &Sprite, &crate::walls::WallSprite), ()>();
    for (tf, sp, w) in q.iter(world) {
        if tf.translation.z > crate::layers::OVERHEAD {
            let a = sp.color.alpha();
            detalle.push(format!("{},{}={a:.2}", w.tx, w.tz));
            menor = menor.min(a);
        }
    }
    detalle.sort();
    eprintln!("ha-verify: FACHADA caras [{}]", detalle.join(" "));
    if menor.is_finite() {
        menor
    } else {
        1.0
    }
}

fn player_x(world: &mut World) -> f32 {
    let mut q = world.query_filtered::<&SimState, With<Player>>();
    q.get_single(world).map(|s| s.body.x).unwrap_or(0.0)
}

fn shot(world: &mut World, path: &str) {
    let path = path.to_string();
    // `save_to_disk` no crea la carpeta: si no existe, la captura se pierde en silencio y
    // el driver "pasa" sin dejar evidencia. En Android la carpeta NUNCA existe la primera
    // vez, así que esto no es defensivo de más.
    if let Some(dir) = std::path::Path::new(&path).parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("ha-verify: no se pudo crear {} ({e})", dir.display());
        }
    }
    let mut commands = world.commands();
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path.clone()));
    eprintln!("ha-verify: screenshot → {path}");
}

// ---------------------------------------------------------------------------
// ACTUACIÓN NEUTRA — los drivers pulsan lo que pulsa un jugador, no teclas
// ---------------------------------------------------------------------------

/// Lo que un driver puede hacer, dicho en el idioma del JUEGO y no del teclado.
///
/// Antes los drivers sintetizaban `KeyCode` y confiaban en que
/// `input::desktop_input::desktop_keyboard_actions` los tradujera. Eso tenía tres
/// problemas, y los tres se pagaron:
///
/// 1. **En el celu no hay teclado.** Hubo que registrar el sistema de teclado de escritorio
///    también en Android, colgado de `flags::driver_active`, sólo para que los `Space`/`F`
///    del driver no cayeran al vacío (moría en `STUCK phase=Ground armed=false`).
/// 2. **El traductor pide FLANCOS** (`just_pressed`), y un driver que escribe el recurso a
///    mano no genera flancos solo. De ahí el baile de soltar-todo-y-volver-a-pulsar cada
///    N segundos, que es ruido que no prueba nada.
/// 3. **No probaba el camino de la gente.** En el Redmi se juega con los dedos: lo que hay
///    que ejercitar es `TouchActions`, que es donde desembocan tanto el botón táctil como
///    el teclado.
///
/// Ahora el driver escribe intención y `aplicar_actuacion_del_driver` la baja a
/// `TouchActions`/`MovementIntent` — exactamente lo mismo que escribe el botón de la
/// pantalla, así que vale igual en el celu que en la compu.
#[derive(Clone, Copy, PartialEq)]
enum Accion {
    /// Saltar / ascender (el botón que en vuelo te sube). Era `KeyCode::Space`.
    Saltar,
    /// Usar / descender (el botón USAR sostenido). Era `KeyCode::KeyF` y `ShiftLeft`.
    Usar,
    /// Caminar/volar hacia una dirección. `x` = este, `y` = sur — la misma convención que
    /// el stick (`input::joystick::keyboard_input`): W era `-y`, S `+y`, A `-x`, D `+x`.
    Mover(Vec2),
}

impl Accion {
    /// Las cuatro direcciones cardinales, en el orden en que estaban los `KeyCode`.
    const D: Accion = Accion::Mover(Vec2::new(1.0, 0.0));
    const S: Accion = Accion::Mover(Vec2::new(0.0, 1.0));
    const A: Accion = Accion::Mover(Vec2::new(-1.0, 0.0));
    const W: Accion = Accion::Mover(Vec2::new(0.0, -1.0));
}

/// Lo que el driver está pulsando ESTE frame. Lo consume `aplicar_actuacion_del_driver`.
#[derive(Resource, Default)]
struct DriverActuation {
    saltar: bool,
    usar: bool,
    mover: Vec2,
    /// Estado del frame anterior — de acá salen los flancos, que antes había que fabricar
    /// a mano soltando y volviendo a pulsar.
    saltar_prev: bool,
    usar_prev: bool,
}

/// Baja la intención del driver a los recursos que lee el juego.
///
/// Va DESPUÉS de `poll_action_buttons_held`, que es quien pone en cero los bits de
/// `TouchActions` cada frame: si corriera antes, el driver escribiría y el sistema de
/// entrada lo borraría en el mismo frame. Es el mismo lugar donde estaba enganchado el
/// traductor de teclado.
fn aplicar_actuacion_del_driver(
    mut act: ResMut<DriverActuation>,
    mut actions: ResMut<crate::input::TouchActions>,
    mut buffer: ResMut<crate::input::actions::ActionInputBuffer>,
    mut intent: ResMut<crate::input::MovementIntent>,
) {
    if act.usar {
        actions.action = true;
        if !act.usar_prev {
            actions.use_edge = true;
            buffer.tap_use();
        }
    }
    if act.saltar {
        actions.jump = true;
        buffer.hold_jump();
        if !act.saltar_prev {
            actions.jump_edge = true;
        }
    }
    if act.mover != Vec2::ZERO {
        let d = act.mover.normalize_or_zero();
        intent.move_x = d.x;
        intent.move_z = d.y;
    }
    act.usar_prev = act.usar;
    act.saltar_prev = act.saltar;
    // SE BORRA CADA FRAME. Un dedo está apoyado en ESTE frame o no lo está; no hay estado
    // que sobreviva solo. La primera versión era pegajosa (se mantenía hasta un
    // `release_all` explícito, imitando a `ButtonInput`) y eso resucitó un bug viejo por
    // la puerta de atrás: `creative_fly_step` deja que USAR PISE a SALTO (`actions.action`
    // se evalúa después y manda `vy = -rate`), así que un `usar` olvidado por una etapa
    // anterior hacía que el driver despegara y aterrizara en bucle —"despegue/contacto"
    // repetido en el log— hasta morir en `mid-descent abort FAILED`. Antes no pasaba
    // porque `hold_with_fresh_edges` soltaba TODO cada 0.4 s y de paso limpiaba la
    // basura; o sea que la limpieza dependía de un efecto colateral del truco de los
    // flancos. Ahora es explícita y no hay estado viejo posible.
    act.saltar = false;
    act.usar = false;
    act.mover = Vec2::ZERO;
}

/// Pulsa una acción **para este frame**. Los drivers ya la llamaban en cada pasada de su
/// máquina de etapas, así que sostener sigue siendo "pedirla todos los frames"; lo que
/// cambia es que soltarla es no pedirla, en vez de un `release_all` explícito.
fn actuar(world: &mut World, a: Accion) {
    let mut act = world.resource_mut::<DriverActuation>();
    match a {
        Accion::Saltar => act.saltar = true,
        Accion::Usar => act.usar = true,
        Accion::Mover(d) => act.mover += d,
    }
}

fn release_all(world: &mut World) {
    let mut act = world.resource_mut::<DriverActuation>();
    act.saltar = false;
    act.usar = false;
    act.mover = Vec2::ZERO;
}

// ---------------------------------------------------------------------------
// HA_TESTER — continuous AI player + perf harness (see plugin registration).
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct TesterState {
    /// Ya se lo dejó en la superficie de la metrópolis (ver `tester_drive`).
    arrancado: bool,
    phase: u8,
    entered: f32,
    last_snap: f32,
    last_cmd_poll: f32,
    dir_idx: u8,
    laps: u32,
    cleared_at: f32,
}

/// Directional walk keys, rotated each lap so the tester roams (streaming stress
/// in every direction) instead of pacing one line.
const TESTER_DIRS: [Accion; 4] = [Accion::D, Accion::S, Accion::A, Accion::W];

fn tester_cmd_path() -> String {
    format!("{}/tester-cmd.txt", flytest_dir())
}

/// Teleport the player onto a body's surface (on-demand "go to X" support).
fn tester_teleport(world: &mut World, body: ha_shared::CelestialBody) {
    let (region, ctx) = match body {
        ha_shared::CelestialBody::Moon => {
            (crate::scene::RegionId::Luna, ha_shared::WorldContext::Luna)
        }
        _ => (
            crate::scene::RegionId::Overworld { gx: 0, gz: 0 },
            ha_shared::WorldContext::Overworld,
        ),
    };
    world.resource_mut::<crate::scene::RegionRuntime>().active = region;
    world.resource_mut::<crate::scene::WorldContextRuntime>().active = ctx;
    // Drop flight so we land as a walker.
    world.resource_mut::<crate::creative_fly::CreativeFlight>().phase =
        crate::creative_fly::Phase::Ground;
    let (sx, sz) = crate::scene::walkable_spawn(body, 0.0, 6.0);
    let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
    if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
        sim.body.x = sx;
        sim.body.z = sz;
        sim.body.y = 0.0;
        vis.from_x = sx;
        vis.to_x = sx;
        vis.from_z = sz;
        vis.to_z = sz;
    }
    eprintln!("ha-tester: teleported to {body:?} surface");
}

/// Poll `tester-cmd.txt` for a one-word directive, act on it, then clear the file
/// so it fires once. Lets a human steer the running tester without a rebuild.
fn tester_poll_command(world: &mut World, now: f32) {
    let path = tester_cmd_path();
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return;
    };
    let cmd = raw.trim().to_ascii_lowercase();
    if cmd.is_empty() {
        return;
    }
    let _ = std::fs::remove_file(&path);
    eprintln!("ha-tester: command '{cmd}'");
    match cmd.as_str() {
        "shot" => shot(world, &format!("{}/tester-ondemand.png", flytest_dir())),
        // `shot <nombre>` / `zoom <metros>`: sirven para barrer niveles de zoom y
        // verificar que la iluminación se ve igual a cualquier encuadre.
        _ if cmd.starts_with("shot ") => {
            let name = cmd[5..].trim().replace('/', "_");
            shot(world, &format!("{}/tester-{name}.png", flytest_dir()));
        }
        _ if cmd.starts_with("zoom ") => {
            if let Ok(h) = cmd[5..].trim().parse::<f32>() {
                let mut z = world.resource_mut::<crate::camera::CameraZoom>();
                z.pedir_encuadre(h);
                eprintln!("ha-tester: zoom view_h={h}");
            }
        }
        // `equip <item_id>`: pone algo en la mano sin pasar por la hotbar. Nació
        // para QA de iluminación (una antorcha es la única luz CÁLIDA que se puede
        // pasear a voluntad, y el aura de Ki es azul pálida: no sirve para juzgar
        // si el borde del charco se funde con la noche).
        _ if cmd.starts_with("equip ") => {
            let id = cmd[6..].trim().to_string();
            eprintln!("ha-tester: equip '{id}'");
            world.resource_mut::<crate::input::EquippedItem>().item_id = id;
        }
        // `torches`: clava una de cada variante en círculo alrededor del jugador.
        // QA de iluminación: es la única forma de ver las cinco a la vez.
        "torches" => {
            let (px, pz, body, zlvl) = {
                let mut q = world.query_filtered::<&SimState, With<Player>>();
                let p = q.get_single(world).map(|s| (s.body.x, s.body.z)).unwrap_or((0.0, 0.0));
                let body = {
                    let ctx = world.resource::<crate::scene::WorldContextRuntime>();
                    let planet = world.resource::<crate::world2d::ActivePlanet>();
                    crate::world2d::active_surface_body(ctx, planet)
                };
                let z = world.resource::<crate::world2d::TileWorld>().z;
                (p.0, p.1, body, z)
            };
            let mut t = world.resource_mut::<crate::torches::PlacedTorches>();
            let n = crate::torches::TORCH_VARIANTS.len();
            for i in 0..n {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                t.list.push(crate::torches::PlacedTorch {
                    body: body as u8,
                    z: zlvl,
                    x: px + a.cos() * 7.0,
                    zz: pz + a.sin() * 7.0,
                    mount: crate::torches::TorchMount::Floor,
                    variant: i as u8,
                });
            }
            eprintln!("ha-tester: {n} antorchas alrededor de ({px:.0},{pz:.0})");
        }
        // `menu` / `menu off`: abre y cierra el menú de MENU sin un dedo. Nació
        // para verificar el scroll del menú (`game_menu::scroll_game_menu`): en
        // escritorio no hay tecla que lo abra, así que sin esto la única prueba
        // del panel era el teléfono.
        "menu" | "menu on" => {
            world.resource_mut::<crate::ui_editor::UiEditorState>().menu_open = true;
            eprintln!("ha-tester: menu abierto");
        }
        "menu off" => {
            world.resource_mut::<crate::ui_editor::UiEditorState>().menu_open = false;
            eprintln!("ha-tester: menu cerrado");
        }
        // `bolsa [construir|craftear|inventario]`: abre la mochila en una
        // pestaña. Con `scroll <px>` encima es la unica forma de comprobar sin
        // telefono que la lista LLEGA HASTA EL FINAL — el bug de "la ultima fila
        // queda cortada y no me deja seguir bajando".
        _ if cmd.starts_with("bolsa") => {
            let tab = match cmd.split_whitespace().nth(1).unwrap_or("construir") {
                "craftear" | "craft" => crate::hud::BagTab::Craft,
                "inventario" | "inv" => crate::hud::BagTab::Inventory,
                _ => crate::hud::BagTab::Build,
            };
            // Tercera palabra: la categoria de CONSTRUIR. `piezas` (la ficha por
            // defecto) tiene cinco entradas y no scrollea — para probar el
            // scroll hace falta una llena, como `agua`.
            use ha_shared::placeables::PlaceCat;
            let group = match cmd.split_whitespace().nth(2) {
                Some("agua") => Some(PlaceCat::Agua),
                Some("naturaleza") => Some(PlaceCat::Naturaleza),
                Some("muebles") => Some(PlaceCat::Mueble),
                Some("calle") => Some(PlaceCat::Calle),
                Some("edificios") => Some(PlaceCat::Edificio),
                Some("patio") => Some(PlaceCat::Parcela),
                Some("vehiculos") => Some(PlaceCat::Vehiculo),
                Some("estructuras") => Some(PlaceCat::Estructura),
                _ => None,
            };
            let mut bag = world.resource_mut::<crate::hud::BagPanelState>();
            bag.open = true;
            bag.tab = tab;
            if let Some(cat) = group {
                let g = crate::hud::BagPanelState::build_group_of(cat);
                bag.set_build_group(g);
            }
            eprintln!("ha-tester: mochila abierta");
        }
        "bolsa off" => {
            world.resource_mut::<crate::hud::BagPanelState>().open = false;
        }
        // `scroll <px>`: empuja la lista abierta. Negativo sube.
        _ if cmd.starts_with("scroll ") => {
            let dy: f32 = cmd[7..].trim().parse().unwrap_or(0.0);
            let mut q = world
                .query_filtered::<&mut bevy::ui::ScrollPosition, With<crate::hud::BagScrollView>>();
            for mut sp in q.iter_mut(world) {
                sp.offset_y = (sp.offset_y + dy).max(0.0);
            }
            eprintln!("ha-tester: scroll {dy:+.0} px");
        }
        "earth" => tester_teleport(world, ha_shared::CelestialBody::Earth),
        "luna" | "moon" => tester_teleport(world, ha_shared::CelestialBody::Moon),
        "fly" | "flyup" => {
            let mut s = world.resource_mut::<TesterState>();
            s.phase = 3;
            s.entered = now;
            s.cleared_at = 0.0;
        }
        "land" | "descend" => {
            world.resource_mut::<crate::creative_fly::CreativeFlight>().phase =
                crate::creative_fly::Phase::Descend;
            let mut s = world.resource_mut::<TesterState>();
            s.phase = 4;
            s.entered = now;
        }
        // Órdenes al compañero (mano derecha): asignar tarea.
        "follow" => {
            world.resource_mut::<crate::companion::CompanionOrder>().0 =
                crate::companion::CompanionTask::Follow;
        }
        "gather" => {
            world.resource_mut::<crate::companion::CompanionOrder>().0 =
                crate::companion::CompanionTask::Gather;
        }
        "pray" => {
            world.resource_mut::<crate::companion::CompanionOrder>().0 =
                crate::companion::CompanionTask::Pray;
        }
        // Abrir/cerrar la mochila: es la pantalla que hay que poder capturar
        // para comparar contra la imagen de referencia, y sin esto la única
        // forma de verla era tocar el botón a mano en el teléfono.
        "mochila" | "bolsa" | "bag" => {
            let mut b = world.resource_mut::<crate::hud::BagPanelState>();
            b.open = !b.open;
            eprintln!("ha-tester: mochila {}", if b.open { "abierta" } else { "cerrada" });
        }
        _ => eprintln!("ha-tester: unknown command '{cmd}'"),
    }
}

fn tester_goto_phase(world: &mut World, phase: u8, now: f32) {
    let mut s = world.resource_mut::<TesterState>();
    s.phase = phase;
    s.entered = now;
    s.cleared_at = 0.0;
}

fn tester_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 9.0 {
        return; // let boot/loading finish and gameplay input unblock
    }
    if !player_ready(world) {
        return; // aún sin jugador spawneado: reintentar el frame siguiente
    }
    // **Arranca en la SUPERFICIE, en plena metrópolis.** El jugador guardado de
    // corridas viejas quedaba en el subsuelo (el tester cavaba para bajar) y lo
    // que se medía era un túnel negro, no el mundo que juega la gente. Lo que
    // tiene que aguantar 60 fps es el overworld con la ciudad más cargada.
    if !world.resource::<TesterState>().arrancado {
        world.resource_mut::<TesterState>().arrancado = true;
        world.resource_mut::<TileWorld>().z = 0;
        tester_teleport(world, ha_shared::CelestialBody::Earth);
        let body = ha_shared::CelestialBody::Earth;
        let town = ha_shared::world_map::template(body).towns.first();
        if let Some(t) = town {
            let (cx, cz) = ha_shared::world_map::town_center(body, t);
            let near = (cx as f32 * ha_shared::topdown::TILE_M, cz as f32 * ha_shared::topdown::TILE_M);
            let (sx, sz) = crate::scene::walkable_spawn(body, near.0, near.1);
            let mut q = world.query_filtered::<(&mut SimState, &mut PlayerVisual), With<Player>>();
            if let Ok((mut sim, mut vis)) = q.get_single_mut(world) {
                sim.body.x = sx;
                sim.body.z = sz;
                vis.from_x = sx;
                vis.to_x = sx;
                vis.from_z = sz;
                vis.to_z = sz;
            }
            eprintln!("ha-tester: arranca en {} ({cx},{cz})", t.name);
        }
    }

    // Live snapshot every ~3 s so a fresh frame is always on disk to inspect,
    // plus a heartbeat (perf logs `ha-perf fps=` on its own 5 s cadence).
    if now - world.resource::<TesterState>().last_snap > 3.0 {
        world.resource_mut::<TesterState>().last_snap = now;
        let (px, pz) = {
            let mut q = world.query_filtered::<&SimState, With<Player>>();
            q.get_single(world).map(|s| (s.body.x, s.body.z)).unwrap_or((0.0, 0.0))
        };
        let region = format!("{:?}", world.resource::<crate::scene::RegionRuntime>().active);
        let (phase, laps) = {
            let s = world.resource::<TesterState>();
            (s.phase, s.laps)
        };
        let ki = world.resource::<crate::flight::Flight>().ki_pct();
        eprintln!(
            "ha-tester: alive phase={phase} laps={laps} pos=({px:.0},{pz:.0}) region={region} ki={ki:.0}%"
        );
        shot(world, &format!("{}/tester-live.png", flytest_dir()));
    }

    // On-demand steering (poll at ~2 Hz to keep the syscall cheap).
    if now - world.resource::<TesterState>().last_cmd_poll > 0.5 {
        world.resource_mut::<TesterState>().last_cmd_poll = now;
        tester_poll_command(world, now);
    }

    let (phase, entered) = {
        let s = world.resource::<TesterState>();
        (s.phase, s.entered)
    };
    let dt = now - entered;

    match phase {
        // WALK — roam in the current direction; streams the world (FPS stress).
        0 => {
            let dir = TESTER_DIRS[(world.resource::<TesterState>().dir_idx as usize) % 4];
            actuar(world, dir);
            if dt > 10.0 {
                release_all(world);
                // Cavar y construir sólo si se pide (`HA_TESTER_CAVA`): cavando
                // terminaba en el subsuelo y medía un túnel en vez del mundo.
                if crate::flags::var("HA_TESTER_CAVA").is_ok() {
                    let (tx, tz) = player_cell(world);
                    dig_setup(world, (tx + 2, tz));
                    tester_goto_phase(world, 1, now);
                } else if world.resource::<TesterState>().laps % 3 == 2 {
                    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
                    tester_goto_phase(world, 3, now);
                } else {
                    let mut s = world.resource_mut::<TesterState>();
                    s.dir_idx = s.dir_idx.wrapping_add(1);
                    s.laps += 1;
                    s.phase = 0;
                    s.entered = now;
                }
            }
        }
        // DIG — break the tile in front (real dig path: lock + complete hold).
        1 => {
            equip(world, "hand");
            let (tx, tz) = {
                let (tx, tz) = player_cell(world);
                (tx + 2, tz)
            };
            lock_block_target(world, tx, tz);
            complete_hold(world, false);
            if dt > 1.2 {
                unlock(world);
                let (px, pz) = player_cell(world);
                place_setup(world, (px + 1, pz));
                tester_goto_phase(world, 2, now);
            }
        }
        // BUILD — place a stone block next to the player.
        2 => {
            let (px, pz) = player_cell(world);
            lock_place_target(world, px + 1, pz);
            complete_hold(world, true);
            if dt > 1.2 {
                unlock(world);
                // Every 3rd lap, do a full Ki flight; otherwise keep roaming.
                if world.resource::<TesterState>().laps % 3 == 2 {
                    world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
                    tester_goto_phase(world, 3, now);
                } else {
                    let mut s = world.resource_mut::<TesterState>();
                    s.dir_idx = s.dir_idx.wrapping_add(1);
                    s.laps += 1;
                    s.phase = 0;
                    s.entered = now;
                }
            }
        }
        // FLY UP — charge Ki (hold JUMP) and climb to a CLEAR orbit, hover a beat.
        3 => {
            world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
            sostener(world, Accion::Saltar, now, 0.4);
            let (fphase, armed) = {
                let f = world.resource::<crate::creative_fly::CreativeFlight>();
                (f.phase, f.land_armed)
            };
            let cover = world.resource::<crate::transition::FlightTransition>().cover;
            if fphase == crate::creative_fly::Phase::Orbit && cover < 0.05 && armed {
                let cleared = world.resource::<TesterState>().cleared_at;
                if cleared == 0.0 {
                    world.resource_mut::<TesterState>().cleared_at = now;
                    eprintln!("ha-tester: reached CLEAR orbit");
                } else {
                    release_all(world); // hover
                    if now - cleared > 1.5 {
                        shot(world, &format!("{}/tester-orbit.png", flytest_dir()));
                        tester_goto_phase(world, 4, now);
                    }
                }
            } else if dt > 40.0 {
                // Never cleared — bail flight, back to walking (no crash).
                eprintln!("ha-tester: flight timeout, resuming ground");
                release_all(world);
                world.resource_mut::<crate::ui_editor::UiEditorState>().creative = false;
                tester_goto_phase(world, 0, now);
            }
        }
        // DESCEND/LAND — hold USE (F) to actively descend to the surface, then
        // resume the loop. (Releasing everything in orbit only HOVERS.)
        _ => {
            release_all(world);
            actuar(world, Accion::Usar); // hold USE = descend while flying
            let grounded = world.resource::<crate::creative_fly::CreativeFlight>().phase
                == crate::creative_fly::Phase::Ground;
            if grounded {
                world.resource_mut::<crate::ui_editor::UiEditorState>().creative = false;
                let mut s = world.resource_mut::<TesterState>();
                s.dir_idx = s.dir_idx.wrapping_add(1);
                s.laps += 1;
                s.phase = 0;
                s.entered = now;
                eprintln!("ha-tester: landed, lap complete");
            } else if dt > 30.0 {
                // Force a low descent so it can't hang forever.
                {
                let mut f = world.resource_mut::<crate::creative_fly::CreativeFlight>();
                f.vert = 400.0;
                f.alt = 400.0;
            }
                if dt > 45.0 {
                    tester_teleport(world, ha_shared::CelestialBody::Earth);
                    tester_goto_phase(world, 0, now);
                }
            }
        }
    }
}

fn smoke_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 9.0 {
        return;
    }
    if !player_ready(world) {
        return; // aún sin jugador spawneado: reintentar el frame siguiente
    }
    let (phase, entered, done) = {
        let s = world.resource::<SmokeState>();
        (s.phase, s.entered, s.done)
    };
    if done {
        return;
    }
    let dt = now - entered;

    match phase {
        0 => {
            // Place the aim cursor a couple tiles from the player so the
            // yellow cell highlight is visible in the boot shot.
            let (px, pz) = {
                let mut q = world.query_filtered::<&SimState, With<Player>>();
                let s = q.single(world);
                (s.body.x, s.body.z)
            };
            {
                let mut c = world.resource_mut::<crate::input::AimCursor>();
                c.world = bevy::prelude::Vec2::new(px + 2.5, pz - 1.5);
                c.initialized = true;
                c.active = true;
            }
            let x = player_x(world);
            shot(world, "ha-verify-topdown-boot.png");
            // Open the unified menu for a capture.
            world.resource_mut::<crate::ui_editor::UiEditorState>().menu_open = true;
            let mut s = world.resource_mut::<SmokeState>();
            s.start_x = x;
            s.phase = 20;
            s.entered = now;
            eprintln!("ha-verify: boot (player x={x:.1})");
        }
        20 => {
            if dt > 1.0 {
                shot(world, "ha-verify-topdown-menu.png");
                eprintln!("ha-verify: menu captured");
                world.resource_mut::<crate::ui_editor::UiEditorState>().menu_open = false;
                world.resource_mut::<SmokeState>().phase = 1;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        1 => {
            // Walk east (D) for ~3 s, then check we moved.
            actuar(world, Accion::D);
            if dt > 3.0 {
                let x = player_x(world);
                let start = world.resource::<SmokeState>().start_x;
                let moved = x > start + 2.0;
                eprintln!(
                    "ha-verify: walk-east {} (Δx={:.1})",
                    if moved { "PASS" } else { "FAIL" },
                    x - start
                );
                release_all(world);
                shot(world, "ha-verify-topdown-walk.png");
                // Dig the tile in front (inject the completed break hold →
                // real consumer apply_tile_hold digs + grants).
                let (tx, tz) = player_cell(world);
                let cell = (tx + 2, tz);
                dig_setup(world, cell);
                world.resource_mut::<SmokeState>().phase = 10;
                world.resource_mut::<SmokeState>().entered = now;
                world.resource_mut::<SmokeState>().start_x = x;
            }
        }
        10 => {
            // Digging: keep the break hold locked/completed until the tile opens.
            equip(world, "hand");
            let (tx, tz) = world.resource::<SmokeState>().cell;
            lock_block_target(world, tx, tz);
            complete_hold(world, false);
            let dug = !world
                .resource::<TileWorld>()
                .solid_at(ha_shared::CelestialBody::Earth, tx, tz);
            let loot = dig_loot(world);
            if dug || loot > 0 {
                eprintln!("ha-verify: dig PASS (dug={dug}, loot={loot})");
                unlock(world);
                // Now place a block on that freshly-dug cell's neighbour.
                place_setup(world, (tx, tz));
                world.resource_mut::<SmokeState>().phase = 11;
                world.resource_mut::<SmokeState>().entered = now;
            } else if dt > 4.0 {
                eprintln!("ha-verify: dig FAIL");
                unlock(world);
                world.resource_mut::<SmokeState>().phase = 2;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        11 => {
            equip(world, "stone");
            let (tx, tz) = world.resource::<SmokeState>().cell;
            lock_place_target(world, tx, tz);
            complete_hold(world, true);
            let placed = world
                .resource::<TileWorld>()
                .solid_at(ha_shared::CelestialBody::Earth, tx, tz);
            if placed {
                eprintln!("ha-verify: place PASS");
                unlock(world);
                world.resource_mut::<SmokeState>().phase = 12;
                world.resource_mut::<SmokeState>().entered = now;
            } else if dt > 4.0 {
                eprintln!("ha-verify: place FAIL");
                unlock(world);
                world.resource_mut::<SmokeState>().phase = 12;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        12 => {
            // Build system: place a wood floor via the real build-hold consumer.
            world.resource_mut::<crate::input::BuildModeActive>().active = true;
            world.resource_mut::<crate::build::SelectedBuildPiece>().0 = 0;
            if world.resource::<PlayerInventory>().qty("wood") < 3 {
                world.resource_mut::<PlayerInventory>().add("wood", 5);
            }
            let (bx, bz) = player_cell(world);
            let (tx, tz) = (bx + 1, bz + 2);
            lock_place_target(world, tx, tz);
            // Point the aim cursor at the build cell so highlight + ghost align.
            {
                let mut c = world.resource_mut::<crate::input::AimCursor>();
                c.world = bevy::prelude::Vec2::new(tx as f32 + 0.5, tz as f32 + 0.5);
                c.initialized = true;
                c.active = true;
            }
            // **Las TRES pestañas de la mochila, una captura cada una.** Era sólo
            // CONSTRUIR; con la ventana a pantalla completa y las tarjetas con
            // icono, cualquier regresión de UI (un icono que sale como hoja de
            // atlas, un nombre que no entra, una tarjeta que desborda) se ve en
            // estas tres imágenes y en ninguna otra parte.
            // Ventanas ANCHAS (0,4 s) a propósito: con frames de 0,2 s en debug,
            // las de 0,2 s que había antes se saltaban enteras y la captura de
            // CONSTRUIR no salía nunca — el archivo quedaba con la imagen de la
            // corrida anterior, que es peor que no tenerla.
            if dt < 2.6 {
                let group = if dt < 0.9 {
                    0 // PIEZAS, la ficha por defecto
                } else {
                    // Una categoría del pack: es la que prueba de verdad que
                    // filtrar rehace la lista con los colocables importados.
                    crate::hud::BagPanelState::build_group_of(
                        ha_shared::placeables::PlaceCat::Mueble,
                    )
                };
                let mut bag = world.resource_mut::<crate::hud::BagPanelState>();
                bag.open = true;
                bag.set_build_group(group);
                bag.tab = match dt {
                    _ if dt < 1.7 => crate::hud::BagTab::Build,
                    _ if dt < 2.2 => crate::hud::BagTab::Craft,
                    _ => crate::hud::BagTab::Inventory,
                };
            }
            if dt > 0.4 && dt < 0.8 {
                shot(world, "ha-verify-bag-build.png");
            }
            if dt > 1.3 && dt < 1.6 {
                shot(world, "ha-verify-bag-build-muebles.png");
            }
            if dt > 1.2 && dt < 1.6 {
                shot(world, "ha-verify-bag-craft.png");
            }
            if dt > 2.0 && dt < 2.4 {
                shot(world, "ha-verify-bag-inventory.png");
            }
            if dt >= 2.6 {
                world.resource_mut::<crate::hud::BagPanelState>().open = false;
            }
            // Después de cerrar la mochila: si no, esta captura sale con el panel
            // encima y no muestra el fantasma de construcción, que es lo suyo.
            if dt > 2.8 && dt < 3.2 {
                shot(world, "ha-verify-topdown-build.png");
            }
            // Place only after the captures, so the ghost shows on empty ground.
            if dt > 3.3 {
                let mut h = world.resource_mut::<HoldInteractState>();
                h.active = false;
                h.completed = true;
                h.placing_build = true;
            }
            let built = world
                .resource::<TileWorld>()
                .tile_at(ha_shared::CelestialBody::Earth, tx, tz)
                == ha_shared::topdown::Tile::WoodFloor;
            if built && dt > 2.2 || dt > 5.0 {
                eprintln!("ha-verify: build {}", if built { "PASS" } else { "FAIL" });
                unlock(world);
                world.resource_mut::<crate::input::BuildModeActive>().active = false;
                world.resource_mut::<SmokeState>().phase = 13;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        13 => {
            // **El MAPA como objeto** (`map_item.rs`): se equipa, se abre en la
            // mano con tu posición marcada, y manteniendo USE queda colgado
            // retratando el sector donde estás.
            //
            // Todo dispara por ESTADO y no por ventana de reloj: en debug esto
            // corre a 8-20 FPS y un `dt > 1.4 && dt < 1.5` se pierde entre dos
            // frames (así este paso fallaba una vez sí y otra no).
            if dt < 1.2 {
                use crate::hud::inventory_ui::{SlotId, Stack};
                world
                    .resource_mut::<PlayerInventory>()
                    .add(crate::map_item::MAP_SMALL, 2);
                // Equipar se hace COMO EL JUGADOR: el ítem en la hotbar y ese
                // hueco seleccionado. Escribir `EquippedItem` a mano no sirve —
                // `equip_from_hotbar_selection` lo pisa con lo que haya elegido.
                let mut ui = world.resource_mut::<crate::hud::inventory_ui::InventoryUiState>();
                ui.hotbar[1] = Stack {
                    item_id: Some(crate::map_item::MAP_SMALL.to_string()),
                    qty: 2,
                };
                ui.selected = Some(SlotId::Hotbar(1));
            }
            if dt > 0.8 && dt < 1.0 {
                shot(world, "ha-verify-map-hand.png");
            }
            let placed = world.resource::<crate::map_item::PlacedMapWorld>().placed.len();
            // Colgarlo: el mismo camino que usa el jugador (hold completado). Se
            // insiste hasta que entre uno.
            if dt > 1.2 && placed == 0 {
                let mut h = world.resource_mut::<HoldInteractState>();
                h.active = false;
                h.completed = true;
                h.placing_map = true;
            }
            // "Está en la lista" y "se dibuja" son cosas distintas: se cuenta
            // también el sprite colgado.
            let drawn = world
                .query::<&crate::map_item::PlacedMapEntity>()
                .iter(world)
                .count();
            if placed > 0 && drawn > 0 && !world.resource::<SmokeState>().map_shot {
                world.resource_mut::<SmokeState>().map_shot = true;
                shot(world, "ha-verify-map-placed.png");
            }
            if (world.resource::<SmokeState>().map_shot && dt > 2.5) || dt > 8.0 {
                eprintln!(
                    "ha-verify: map {} (colgados={placed} sprites={drawn})",
                    if placed > 0 && drawn > 0 { "PASS" } else { "FAIL" }
                );
                world.resource_mut::<crate::input::EquippedItem>().item_id = "hammer".to_string();
                world.resource_mut::<SmokeState>().phase = 2;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        2 => {
            // Walk south (S) briefly to confirm the vertical axis works.
            actuar(world, Accion::S);
            if dt > 2.5 {
                release_all(world);
                shot(world, "ha-verify-topdown-walk2.png");
                // **Cavar para bajar.** Ya no hay boca de mina que buscar: el
                // subsuelo es macizo y la entrada la abre el jugador. Se cava la
                // celda que pisa, y `fall_through_void` hace el resto.
                let body = bt_body(world);
                let (px, pz) = player_cell(world);
                world.resource_mut::<TileWorld>().dig_down(body, px, pz);
                world.resource_mut::<SmokeState>().phase = 3;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        3 => {
            // The layer_transition should have dropped us to z=-1.
            if dt > 2.1 {
                let z = world.resource::<TileWorld>().z;
                eprintln!(
                    "ha-verify: descend {} (z={z})",
                    if z < 0 { "PASS" } else { "FAIL" }
                );
                shot(world, "ha-verify-topdown-cave.png");
                // Back to surface, then hold JUMP to Saiyan-fly into space (Fase 4).
                world.resource_mut::<TileWorld>().z = 0;
                world.resource_mut::<SmokeState>().phase = 4;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        4 => {
            // Hold thrust. The ascent is GRADUAL now (shared transition layer):
            // below the swap band the surface region stays live under the
            // closing cover, so the pass condition is "flying with a coherent
            // frame", not "instantly in Space".
            actuar(world, Accion::Saltar);
            if dt > 4.0 {
                let in_space = matches!(
                    world.resource::<crate::scene::RegionRuntime>().active,
                    crate::scene::RegionId::Space
                );
                let in_surface_frame = world
                    .resource::<crate::transition::FlightTransition>()
                    .in_surface_frame();
                let alt = world.resource::<crate::flight::Flight>().altitude_m();
                let ok = in_space || in_surface_frame;
                eprintln!(
                    "ha-verify: space-flight {} (region_space={in_space}, surface_frame={in_surface_frame}, alt={alt:.0})",
                    if ok { "PASS" } else { "FAIL" }
                );
                release_all(world);
                shot(world, "ha-verify-topdown-space.png");
                // Open the menu in Space to confirm the orbital map still shows.
                world.resource_mut::<crate::ui_editor::UiEditorState>().menu_open = true;
                world.resource_mut::<SmokeState>().phase = 21;
                world.resource_mut::<SmokeState>().entered = now;
            }
        }
        21 => {
            if dt > 1.0 {
                shot(world, "ha-verify-topdown-space-menu.png");
                eprintln!("ha-verify: space menu captured");
                eprintln!("ha-verify: SMOKE done");
                world.resource_mut::<SmokeState>().done = true;
            }
        }
        _ => {
            world.resource_mut::<SmokeState>().done = true;
        }
    }
}


// ---------------------------------------------------------------------------
// HA_CUEVATEST — entrar en una cueva caminando (biblia §5)
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct CuevaTestState {
    step: u32,
    next_t: f32,
    cueva: Option<ha_shared::cuevas::Cueva>,
    /// Encierro medido en cada beat, para exigir que CREZCA en vez de saltar.
    /// El `bool` dice si en ese beat el jugador estaba ANDANDO EL TRAMO.
    encierro: Vec<(f32, bool)>,
    /// Nivel al que se entró y en qué beat, para probar que se entró andando.
    z_al_entrar: Option<(u32, i32)>,
    empuje: Vec2,
    empuje_hasta: f32,
    failed: bool,
}

/// Cuántos beats de medio segundo se camina hacia adentro.
const CT_BEATS: u32 = 26;

fn cueva_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 2.0 || !player_ready(world) {
        return;
    }
    {
        let st = world.resource::<CuevaTestState>();
        let (empuje, hasta) = (st.empuje, st.empuje_hasta);
        if empuje != Vec2::ZERO && now < hasta {
            actuar(world, Accion::Mover(empuje));
        }
    }
    if now < world.resource::<CuevaTestState>().next_t {
        return;
    }
    let step = world.resource::<CuevaTestState>().step;
    {
        let mut st = world.resource_mut::<CuevaTestState>();
        st.step += 1;
        st.next_t = now + 0.5;
    }
    let body = bt_body(world);

    match step {
        // Buscar una boca DESPEJADA (la obra civil puede haberla tapado) y
        // plantarse en ella, en superficie.
        0 => {
            let cueva = (0..80i64)
                .flat_map(|gx| (-4..=4i64).map(move |gz| (gx, gz)))
                .filter_map(|(gx, gz)| ha_shared::cuevas::cueva_de(body, gx, gz))
                // Despejada = sus ESCALONES asoman de verdad en superficie. La
                // celda de la boca es el RELLANO —terreno normal, donde te parás
                // antes de bajar—, así que mirarla a ella no dice nada; lo que
                // puede haberse comido la obra civil son los escalones.
                .find(|c| {
                    c.tramo(body).celdas().all(|(x, z, _)| {
                        ha_shared::topdown::topdown_tile(body, 0, x, z)
                            == ha_shared::topdown::Tile::Stairwell
                    })
                });
            let Some(c) = cueva else {
                report_bug("cuevas", "hay_cuevas_en_el_mundo", "no encontré ninguna boca despejada");
                world.resource_mut::<CuevaTestState>().failed = true;
                return;
            };
            let (ax, az) = ha_shared::escaleras::eje(c.adentro);
            // Un par de celdas ANTES de la boca: se llega a ella caminando.
            bt_teleport(
                world,
                (c.boca.0 - ax * 2) as f32 + 0.5,
                (c.boca.1 - az * 2) as f32 + 0.5,
            );
            let mut st = world.resource_mut::<CuevaTestState>();
            st.cueva = Some(c);
            st.empuje = Vec2::new(ax as f32, az as f32);
            st.empuje_hasta = now + 1000.0;
            eprintln!("ha-cueva: boca en {:?}, entrando hacia {}", c.boca, c.adentro);
        }
        // Caminar hacia adentro, midiendo a cada paso.
        s if s <= CT_BEATS => {
            let z = world.resource::<TileWorld>().z;
            let enc = world.resource::<crate::interior::Interior>().encierro;
            let en_tramo = world.resource::<crate::escaleras::Subida>().tramo.is_some();
            {
                let mut st = world.resource_mut::<CuevaTestState>();
                st.encierro.push((enc, en_tramo));
                if z < 0 && st.z_al_entrar.is_none() {
                    st.z_al_entrar = Some((s, z));
                }
            }
            match s {
                2 => shot(world, &format!("{}/cueva-1-boca.png", flytest_dir())),
                // **La bajada, beat a beat.** El tramo se anda en poco más de un
                // segundo, así que una sola foto "entrando" no dice nada del
                // degradado: la serie sí. Es la prueba visual de que el overworld
                // se apaga acompañando el paso y de que el cambio de planta cae en
                // el punto oscuro (`interior::sombra_del_tramo`).
                5..=9 => shot(world, &format!("{}/cueva-baja-{s}.png", flytest_dir())),
                12 => shot(world, &format!("{}/cueva-2-entrando.png", flytest_dir())),
                20 => shot(world, &format!("{}/cueva-3-adentro.png", flytest_dir())),
                _ => {}
            }
        }
        // Veredicto.
        _ => {
            let st = world.resource::<CuevaTestState>();
            let (entrada, encierro, fallo) = (st.z_al_entrar, st.encierro.clone(), st.failed);
            if fallo {
                std::process::exit(1);
            }
            // 1. Se entró, y ANDANDO: el cambio de planta cae a mitad del paseo,
            //    no en el primer frame (eso sería un teletransporte).
            match entrada {
                None => report_bug(
                    "cuevas",
                    "se_entra_caminando",
                    "caminé hasta el fondo y nunca bajé de nivel",
                ),
                Some((beat, z)) => {
                    if z != ha_shared::cuevas::NIVEL {
                        report_bug("cuevas", "la_cueva_esta_en_un_nivel", &format!("z={z}"));
                    }
                    if beat <= 1 {
                        report_bug(
                            "cuevas",
                            "se_entra_caminando",
                            "bajé de nivel en el primer paso: eso es un portal, no una escalera",
                        );
                    }
                    eprintln!("ha-cueva: entré andando en el beat {beat} (z={z})");
                }
            }
            // 2. **El afuera se apaga en gradiente.** Es LA promesa de este
            //    sistema: si el encierro saltara de 0 a 1 en un beat, volvimos al
            //    interruptor "estás en una cueva" que se borró.
            let maximo = encierro.iter().map(|&(e, _)| e).fold(0.0f32, f32::max);
            // **El tramo queda fuera de esta cuenta, a propósito.**
            //
            // Los cuatro escalones SÍ cambian la luz rápido, y tienen que hacerlo:
            // ahí la sombra del hueco de escalera se cierra y se vuelve a abrir en
            // poco más de un segundo (`interior::sombra_del_tramo`) para que el
            // cambio de planta caiga en el punto oscuro. Eso es un gradiente
            // pegado al paso —desandás medio escalón y la luz vuelve—, no el
            // interruptor "estás en una cueva" que este test vigila. Lo que se
            // vigila es el ADENTRARSE: caminar por la galería no puede dar saltos.
            let salto = encierro
                .windows(2)
                .filter(|w| !w[0].1 && !w[1].1)
                .map(|w| (w[1].0 - w[0].0).abs())
                .fold(0.0f32, f32::max);
            eprintln!(
                "ha-cueva: encierro máx={maximo:.2} salto máx={salto:.2} muestras={}",
                encierro.len()
            );
            if maximo < 0.75 {
                report_bug(
                    "cuevas",
                    "adentro_se_hace_de_noche",
                    &format!("caminé hasta el fondo y el encierro sólo llegó a {maximo:.2}"),
                );
            }
            if salto > 0.5 {
                report_bug(
                    "cuevas",
                    "el_encierro_es_un_gradiente",
                    &format!("el encierro pegó un salto de {salto:.2} en medio segundo"),
                );
            }
            eprintln!("ha-verify: CUEVA — recorrido completo");
            std::process::exit(0);
        }
    }
}

// ---------------------------------------------------------------------------
// HA_CASATEST — la casa modular: se levanta, se amplía, se abre y se mira
// ---------------------------------------------------------------------------

/// El driver de la cáscara modular (`ha_shared::edificio` + `crate::edificios`).
///
/// Es el hermano de `HA_FACHADATEST`, que prueba la casa hecha a mano pieza por
/// pieza. Acá se prueba la otra forma de construir, la que el pack pide: **el
/// edificio se coloca ENTERO**, con bandas de nueve celdas que son un solo
/// dibujo, y después se lo amplía, se le apilan plantas y se le abre la puerta.
///
/// Lo que mide, y por qué cada cosa:
///
/// 1. **Un frente = UN módulo.** Es el destrozo que hay que impedir que vuelva:
///    la primera importación cortó las bandas en columnas de una celda y una
///    pared salía con media ventana.
/// 2. **El tabique aparece solo al ampliar.** Nadie lo coloca; es el borde que
///    quedó con edificio de los dos lados y cuartos distintos.
/// 3. **La abertura la ven los dos lados.** Una sola lista de aberturas.
/// 4. **La cáscara se dibuja de verdad**, y no sólo se calcula: se cuentan los
///    sprites VIVOS y se mira su banda de z.
/// 5. **Se funde al entrar** (regla 8: una transición no tapa la pantalla).
///
/// Deja dos capturas que hay que MIRAR: `casa-calle.png` y `casa-adentro.png`.
/// `HA_CASATEST=1 HA_FORCE_TOUCH=1`, como el resto de los drivers de obra.
#[derive(Resource, Default)]
struct CasaTestState {
    step: u32,
    next_t: f32,
    /// La celda del frente sobre la que se funda (el medio de la vereda).
    ancla: (i64, i64),
    aim: Vec2,
    reintentos: u32,
    /// El alfa de la cáscara medido DESDE LA CALLE, para comparar con el de
    /// adentro. Ver `casa_test_drive`: lo que se afirma es que se funde, y eso es
    /// una comparación, no un número absoluto.
    alfa_calle: f32,
    fallo: bool,
}

/// **Repetir el paso hasta que el efecto esté.**
///
/// Colocar son dos frames: `complete_build_hold` deja el pedido y
/// `build::fijar_mira_de_obra` recién clava el fantasma al frame siguiente, así
/// que confirmar en el mismo paso no confirma nada. Es el mismo baile que ya
/// bailan `bt_retry` y `ft_reintentar`; lo que hace determinista al driver es
/// medir el EFECTO y no el intento.
fn ct_reintentar(world: &mut World, paso: u32, now: f32) {
    let mut st = world.resource_mut::<CasaTestState>();
    st.reintentos += 1;
    if st.reintentos < 40 {
        st.step = paso;
        st.next_t = now + 0.08;
    } else {
        st.reintentos = 0;
    }
}

/// El efecto que tiene que verse para dar un paso por hecho.
fn ct_hecho(world: &mut World, paso: u32) -> bool {
    let ancla = world.resource::<CasaTestState>().ancla;
    let Some(ed) = ct_ed(world) else { return false };
    match paso {
        // Fundado: el último edificio de la lista tiene que ser el nuestro, o sea
        // uno que contenga el ancla. Sin esa comprobación, con un edificio previo
        // en la partida el paso se daba por hecho sin haber colocado nada.
        1 => ed.contiene(ancla.0, ancla.1),
        2 => ed.pisos() >= 1,
        3 => ed.pisos() >= 2,
        4 => ed.tiene_entrada(),
        5 => ed.cuartos().len() >= 2,
        _ => true,
    }
}

/// Índice de paleta de una acción de EDIFICIO.
fn ct_accion(a: crate::edificios::Accion) -> usize {
    crate::edificios::PALETTE_BASE
        + crate::edificios::ACCIONES.iter().position(|(x, _, _)| *x == a).unwrap_or(0)
}

/// Pulsa la acción sobre la celda `(cx, cz)` por el camino real del jugador.
fn ct_pulsar(world: &mut World, a: crate::edificios::Accion, cx: i64, cz: i64) {
    world.resource_mut::<crate::build::SelectedBuildPiece>().0 = ct_accion(a);
    let aim = Vec2::new(cx as f32 + 0.5, cz as f32 + 0.5);
    world.resource_mut::<CasaTestState>().aim = aim;
    world.resource_mut::<crate::input::AimCursor>().world = aim;
    lock_place_target(world, cx, cz);
    complete_build_hold(world);
    confirmar_si_apunta(world, crate::build::Sitio::Celda(cx, cz));
}

/// El edificio que se está probando, si ya está fundado.
fn ct_ed(world: &mut World) -> Option<ha_shared::edificio::Edificio> {
    world.resource::<crate::edificios::Edificios>().list.last().map(|e| e.ed.clone())
}

/// `(cuántas bandas vivas, z mínima, alfa mínimo)` de la cáscara dibujada.
fn ct_bandas(world: &mut World) -> (usize, f32, f32) {
    let mut n = 0usize;
    let mut zmin = f32::INFINITY;
    let mut alfa = f32::INFINITY;
    let mut detalle: Vec<String> = Vec::new();
    let mut q = world
        .query_filtered::<(&Transform, &Sprite, &crate::edificios::BandaSprite), ()>();
    for (tf, sp, b) in q.iter(world) {
        n += 1;
        zmin = zmin.min(tf.translation.z);
        alfa = alfa.min(sp.color.alpha());
        detalle.push(format!("m{}b{}={:.2}", b.modulo, b.piso, sp.color.alpha()));
    }
    detalle.sort();
    eprintln!("ha-verify: CASA bandas [{}]", detalle.join(" "));
    (n, zmin, if alfa.is_finite() { alfa } else { 1.0 })
}

fn casa_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 2.0 || !player_ready(world) {
        return;
    }
    // La mira se sostiene todos los frames (ver `build_test_drive`).
    {
        let aim = world.resource::<CasaTestState>().aim;
        if aim != Vec2::ZERO {
            world.resource_mut::<crate::input::AimCursor>().world = aim;
        }
    }
    if now < world.resource::<CasaTestState>().next_t {
        return;
    }
    let step = world.resource::<CasaTestState>().step;
    {
        let mut st = world.resource_mut::<CasaTestState>();
        st.step += 1;
        st.next_t = now + 0.25;
    }
    let (ax, az) = world.resource::<CasaTestState>().ancla;

    match step {
        0 => {
            world.resource_mut::<crate::ui_editor::UiEditorState>().creative = true;
            world.resource_mut::<crate::input::BuildModeActive>().active = true;
            world.resource_mut::<crate::input::AimCursor>().locked = true;
            // Un solar despejado, igual que el resto de los drivers de obra: si el
            // sitio sale a cara o cruz, el driver miente.
            let base = bt_clear_base(world);
            world.resource_mut::<CasaTestState>().ancla = (base.0 + 4, base.1 + 4);
            // **No se borra nada.** La primera versión limpiaba la lista "para
            // partir de cero" y eso le come al usuario los edificios que tenga
            // guardados: el driver corre sobre la partida de verdad y el guardado
            // con debounce escribiría la lista vacía. El driver trabaja sobre el
            // ÚLTIMO edificio (`ct_ed`), que es el que acaba de fundar.
        }
        s if (1..=5).contains(&s) => {
            if ct_hecho(world, s) {
                world.resource_mut::<CasaTestState>().reintentos = 0;
                return;
            }
            let (accion, cx, cz) = match s {
                1 => (crate::edificios::Accion::Ambiente, ax, az),
                2 | 3 => (crate::edificios::Accion::Apilar, ax, az),
                4 => (
                    crate::edificios::Accion::Abrir(ha_shared::build_grid::WallKind::Door),
                    ax,
                    // El borde del FRENTE: la puerta va sobre la vereda, y el borde
                    // más cercano a la celda de adentro pegada al sur es ése.
                    ct_ed(world).map(|e| e.celdas().iter().map(|c| c.1).max().unwrap_or(az)).unwrap_or(az),
                ),
                // Ampliar hacia el NORTE: se apunta al borde de arriba de la planta.
                _ => (
                    crate::edificios::Accion::Ampliar,
                    ax,
                    ct_ed(world).map(|e| e.celdas().iter().map(|c| c.1).min().unwrap_or(az)).unwrap_or(az),
                ),
            };
            ct_pulsar(world, accion, cx, cz);
            ct_reintentar(world, s, now);
        }
        6 => {
            // **¿Salió lo que se pidió?** Sobre la geometría, antes de mirar píxeles.
            let Some(ed) = ct_ed(world) else {
                report_bug("casa", "el_ambiente_se_funda", "no quedó ningún edificio");
                world.resource_mut::<CasaTestState>().fallo = true;
                return;
            };
            let frentes: Vec<_> =
                ed.tramos().into_iter().filter(|t| t.cara.es_frente()).collect();
            let modulos = frentes.first().map(|t| t.modulos()).unwrap_or((0, 0));
            eprintln!(
                "ha-verify: CASA cuartos={} pisos={} frentes={} modulos={modulos:?} \
                 tabiques={} entrada={}",
                ed.cuartos().len(),
                ed.pisos(),
                frentes.len(),
                ed.tabiques().len(),
                ed.tiene_entrada()
            );
            let mut fallo = false;
            if modulos != (1, 0) {
                report_bug(
                    "casa",
                    "el_frente_es_un_modulo_entero",
                    &format!("el frente mide {modulos:?} módulos: alguien rebanó la banda"),
                );
                fallo = true;
            }
            if ed.cuartos().len() < 2 || ed.tabiques().is_empty() {
                report_bug(
                    "casa",
                    "ampliar_levanta_el_tabique",
                    &format!("{} cuartos, {} tabiques", ed.cuartos().len(), ed.tabiques().len()),
                );
                fallo = true;
            }
            if !ed.tiene_entrada() {
                report_bug("casa", "la_casa_tiene_entrada", "no quedó puerta ni vano");
                fallo = true;
            }
            if ed.pisos() < 2 {
                report_bug(
                    "casa",
                    "apilar_sube_una_planta",
                    &format!("quedó en {} plantas", ed.pisos()),
                );
                fallo = true;
            }
            if fallo {
                world.resource_mut::<CasaTestState>().fallo = true;
            }
        }
        // **Desde la CALLE.** El jugador se planta al sur del frente, fuera de la
        // huella: la cáscara tiene que verse entera y por delante de los techos.
        7 => {
            let Some(ed) = ct_ed(world) else { return };
            let z1 = ed.celdas().iter().map(|c| c.1).max().unwrap_or(az) + 1;
            world.resource_mut::<crate::input::BuildModeActive>().active = false;
            world.resource_mut::<CasaTestState>().aim = Vec2::ZERO;
            bt_teleport(world, ax as f32 + 0.5, z1 as f32 + 3.5);
            world.resource_mut::<CasaTestState>().next_t = now + 1.5;
        }
        8 => {
            let (n, zmin, _) = ct_bandas(world);
            // Planta baja + 2 pisos + azotea = 4 bandas por módulo, y hay uno.
            eprintln!("ha-verify: CASA bandas_vivas={n} z_min={zmin:.3}");
            if n < 4 {
                report_bug(
                    "casa",
                    "la_cascara_se_dibuja",
                    &format!("sólo {n} bandas vivas: la casa no se ve desde la calle"),
                );
                world.resource_mut::<CasaTestState>().fallo = true;
            }
            if zmin.is_finite() && zmin <= crate::layers::OVERHEAD {
                report_bug(
                    "casa",
                    "la_cascara_va_delante_de_los_techos",
                    &format!("la banda más baja está en z={zmin:.3}, bajo OVERHEAD"),
                );
                world.resource_mut::<CasaTestState>().fallo = true;
            }
            world.resource_mut::<CasaTestState>().alfa_calle = ct_bandas(world).2;
            shot(world, &format!("{}/casa-calle.png", flytest_dir()));
            world.resource_mut::<CasaTestState>().next_t = now + 1.5;
        }
        // **Desde ADENTRO.** Cruzando la puerta la cáscara se funde: regla 8.
        9 => {
            let Some(ed) = ct_ed(world) else { return };
            let z1 = ed.celdas().iter().map(|c| c.1).max().unwrap_or(az);
            bt_teleport(world, ax as f32 + 0.5, z1 as f32 - 0.5);
            world.resource_mut::<CasaTestState>().next_t = now + 1.8;
        }
        _ => {
            let (_, _, alfa) = ct_bandas(world);
            let calle = world.resource::<CasaTestState>().alfa_calle;
            eprintln!("ha-verify: CASA alfa_calle={calle:.3} alfa_adentro={alfa:.3}");
            // **Se compara, no se mide contra un número.** El alfa absoluto de un
            // sprite no es sólo el fundido: la luz ambiental y el velo de la noche
            // también lo tocan, así que un umbral fijo mide la hora del día. Lo
            // que la regla 8 promete es que ENTRAR abre la cáscara, y eso es una
            // diferencia entre las dos capturas.
            if alfa > calle * 0.8 {
                report_bug(
                    "casa",
                    "la_cascara_se_funde_al_entrar",
                    &format!("adentro (alfa={alfa:.3}) se ve igual que desde la calle ({calle:.3})"),
                );
                world.resource_mut::<CasaTestState>().fallo = true;
            }
            shot(world, &format!("{}/casa-adentro.png", flytest_dir()));
            let fallo = world.resource::<CasaTestState>().fallo;
            eprintln!("ha-verify: CASATEST {}", if fallo { "FAIL" } else { "DONE" });
            let mut st = world.resource_mut::<CasaTestState>();
            st.next_t = now + 1.5;
            if st.step > 14 {
                std::process::exit(if fallo { 1 } else { 0 });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// HA_PROPTEST — **¿se deja tocar TODO?**
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
struct PropTestState {
    step: u32,
    next_t: f32,
    fallo: bool,
    /// Los props que hay alrededor, con el punto de pantalla donde se dibujan.
    cola: Vec<(Entity, Vec2)>,
    /// A cuál le estamos apuntando el dedo AHORA (se lee el objetivo el frame
    /// siguiente: el targeting corre en su propio sitio del schedule, y leerlo en
    /// el mismo frame en que se mueve el cursor mide el frame anterior).
    apuntando: Option<Entity>,
    idx: usize,
    enganchados: usize,
    tocables: usize,
}

/// **El driver de "todo es tocable".** Planta al jugador en un casco poblado,
/// junta los props que tiene al alcance del brazo y le apunta el dedo a uno por
/// uno —**un prop por frame**—: cada uno tiene que resolverse como objetivo.
///
/// Mide dos cosas que a ojo no se distinguen:
///
/// 1. **Cobertura** — cuántos de los props al alcance contestan al dedo. Antes
///    del 15 ago 2026 la respuesta era CERO: sólo árboles, rocas y vetas
///    declaraban objetivo, y el resto del mundo era pintura de fondo.
/// 2. **Precisión** — el objetivo que sale es el prop apuntado y no un vecino. Es
///    la otra mitad del pedido: el margen de perdón no puede volverse un imán que
///    elige por vos.
///
/// El barrido va de a un frame porque el objetivo lo calcula `targeting` en SU
/// sitio del schedule: correr el schedule a mano desde acá sería re-entrar en
/// este mismo sistema.
/// La captura del driver, **salteable**. En una máquina sin GPU real (software
/// GL) `Screenshot::primary_window` se cuelga y el driver no llega ni a dar su
/// veredicto: con `HA_NOSHOT=1` se mide igual y la vista se comprueba en el
/// teléfono, que es donde manda de todos modos (regla 12).
fn foto(world: &mut World, nombre: &str) {
    if crate::flags::var("HA_NOSHOT").is_ok() {
        eprintln!("ha-prop: (sin captura) {nombre}");
        return;
    }
    shot(world, &format!("{}/{nombre}", flytest_dir()));
}

fn prop_test_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 3.0 || !player_ready(world) {
        return;
    }

    // ── Barrido en curso: leer lo que dio el dedo del frame anterior y pasar al
    // siguiente prop. Corre TODOS los frames, sin el respiro de `next_t`.
    if let Some(esperada) = world.resource::<PropTestState>().apuntando {
        let salio = world.resource::<crate::targeting::LookTarget>().entity;
        let mut st = world.resource_mut::<PropTestState>();
        if salio == Some(esperada) {
            st.enganchados += 1;
        }
        st.apuntando = None;
    }
    let pendiente = {
        let st = world.resource::<PropTestState>();
        (st.idx < st.cola.len()).then(|| st.cola[st.idx])
    };
    if let Some((entidad, punto)) = pendiente {
        {
            let mut c = world.resource_mut::<crate::input::AimCursor>();
            // `AimCursor::world` es suelo `(x, z)`; el punto del sprite viene en
            // pantalla, así que se deshace `render_xy`.
            c.world = Vec2::new(punto.x, -punto.y);
            c.initialized = true;
            c.active = true;
            c.locked = true;
        }
        let mut st = world.resource_mut::<PropTestState>();
        st.apuntando = Some(entidad);
        st.idx += 1;
        return;
    }

    if now < world.resource::<PropTestState>().next_t {
        return;
    }
    let step = world.resource::<PropTestState>().step;
    {
        let mut st = world.resource_mut::<PropTestState>();
        st.step += 1;
        st.next_t = now + 0.4;
    }

    match step {
        // **Se mide donde el jugador aparece, sin teletransportes.** Se probó
        // saltar al centro de un casco y no vale la pena: el casco más cercano
        // está a 4.400 tiles, el salto obliga a re-streamear el mundo entero y el
        // driver se pasa minutos cargando arte. Y no hace falta — el campo
        // abierto también está sembrado de props (matas, hongos, flores,
        // troncos), que es exactamente la categoría que hasta hoy no se tocaba.
        0..=4 => {}
        // Foto del mundo sin dedo apoyado: no puede haber NINGÚN resaltado.
        5 => foto(world, "prop-1-sin-dedo.png"),
        // **Tres rondas, no una.** Al aire libre hay dos o tres props al alcance
        // del brazo, y "2/2" no prueba gran cosa. Entre ronda y ronda el jugador
        // salta 14 tiles —un paseo, no un viaje: el anillo streameado ya cubre
        // esa distancia y no hay que recargar el mundo— y se vuelve a medir.
        6 | 8 | 10 => {
            let (px, pz) = {
                let mut q = world.query_filtered::<&SimState, With<Player>>();
                let s = q.single(world);
                (s.body.x, s.body.z)
            };
            let jugador = Vec2::new(px, pz);
            let mut cerca: Vec<(Entity, Vec2)> = Vec::new();
            {
                let mut q = world.query::<(
                    Entity,
                    &GlobalTransform,
                    &crate::targeting::InteractTarget,
                )>();
                for (e, gtf, t) in q.iter(world) {
                    if t.kind != ha_shared::InteractTargetKind::Prop {
                        continue;
                    }
                    let p = gtf.translation().truncate();
                    if jugador.distance(Vec2::new(p.x, -p.y)) <= 3.5 {
                        cerca.push((e, p));
                    }
                }
            }
            cerca.truncate(24);
            let mut st = world.resource_mut::<PropTestState>();
            st.tocables += cerca.len();
            let n = cerca.len();
            st.cola = cerca;
            st.idx = 0;
            eprintln!("ha-prop: ronda con {n} props al alcance (acumulado {})", st.tocables);
        }
        // Foto CON el dedo sobre el último prop: acá se ve si el corchete lo abraza.
        7 => foto(world, "prop-2-resaltado.png"),
        // Caminata corta entre rondas.
        9 | 11 => {
            let (px, pz) = {
                let mut q = world.query_filtered::<&SimState, With<Player>>();
                let s = q.single(world);
                (s.body.x, s.body.z)
            };
            bt_teleport(world, px + 14.0, pz + 14.0);
        }
        // Veredicto.
        _ => {
            let (total, ok) = {
                let st = world.resource::<PropTestState>();
                (st.tocables, st.enganchados)
            };
            eprintln!("ha-prop: {ok}/{total} props contestaron al dedo");
            if total == 0 {
                report_bug(
                    "props",
                    "hay_props_tocables_alrededor",
                    "me paré en el casco y no había ni un prop con objetivo al alcance",
                );
                world.resource_mut::<PropTestState>().fallo = true;
            } else if ok < total {
                report_bug(
                    "props",
                    "todo_prop_contesta_al_dedo",
                    &format!("apunté a {total} props y sólo {ok} se resolvieron como objetivo"),
                );
                world.resource_mut::<PropTestState>().fallo = true;
            }
            let fallo = world.resource::<PropTestState>().fallo;
            eprintln!("ha-verify: PROPTEST {}", if fallo { "FAIL" } else { "DONE" });
            std::process::exit(i32::from(fallo));
        }
    }
}


// ---------------------------------------------------------------------------
// HA_VISTA — mirar un lugar
// ---------------------------------------------------------------------------

fn vista_drive(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs();
    if now < 2.0 || !player_ready(world) {
        return;
    }
    let stage = world.resource::<FlyTestState>().stage;
    if stage == 0 {
        let v = crate::flags::var("HA_VISTA").unwrap_or_default();
        let mut it = v.split(',').filter_map(|p| p.trim().parse::<f32>().ok());
        let (Some(x), Some(z)) = (it.next(), it.next()) else {
            eprintln!("ha-vista: HA_VISTA tiene que ser \"x,z\"");
            std::process::exit(1);
        };
        teleport_a_celda(world, x, z);
        eprintln!("ha-vista: en ({x}, {z})");
        advance_stage(world, now);
        return;
    }
    // el salto largo vuelve a mostrar la pantalla de carga: se espera a que se vaya
    let beats: [(f32, f32, &str); 2] = [(25.0, 0.0, "vista-0-a-pie"), (40.0, 120.0, "vista-1-alto")];
    let shots = world.resource::<FlyTestState>().shots as usize;
    let entered = world.resource::<FlyTestState>().stage_entered;
    if shots >= beats.len() {
        if now - entered < beats[beats.len() - 1].0 + 15.0 {
            return;
        }
        eprintln!("ha-vista: DONE");
        std::process::exit(0);
    }
    let (at, alt, name) = beats[shots];
    if alt > 0.0 {
        let mut fly = world.resource_mut::<crate::creative_fly::CreativeFlight>();
        fly.phase = crate::creative_fly::Phase::Climb;
        fly.vert = alt;
        fly.alt = alt;
        fly.vy = 0.0;
    }
    if now - entered > at && mundo_listo(world) {
        shot(world, &format!("{}/{name}.png", flytest_dir()));
        world.resource_mut::<FlyTestState>().shots += 1;
    }
}
