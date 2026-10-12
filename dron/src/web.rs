//! Visor web en 3D. La física corre en Rust; el navegador sólo dibuja.
//!
//! El reparto importa: Three.js es **exclusivamente vista**. Toda la dinámica
//! —cuerpo rígido, motores con retardo, ruido de sensores, PID— se integra acá
//! a 200 Hz y el navegador se limita a preguntar dónde están las cosas. Si el
//! visor se traba o se cierra, el vuelo sigue igual.
//!
//! Servidor HTTP a mano con `std::net`, sin dependencias.

use crate::gym3d::{consignas_hibridas, consignas_hibridas_punto, sensores, sensores_guia};
use crate::movil::{Mando, Movil};
use crate::iafz;
use crate::mision::{Avance, Mision, Punto, Seguidor};
use crate::pid3d::{Estabilizador3, Guiado};
use crate::sim3d::{Camara, Dron3, Motores, Obstaculo, Ruido, Sensores, Viento3, DT};

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

// Agente 120: fuera del rango que usa la poblacion del gym (0..96), asi el
// visor puede volar mientras el entrenamiento corre sin pisarse.
const VISOR: i32 = 120;
// Agente APARTE para persecución. NO puede ser el mismo `VISOR`: ese genoma
// se entrenó para `sensores()` (navegación a un punto), y `sensores_guia()`
// es una codificación de bits distinta — darle esos 96 bits a un genoma que
// nunca los vio es alimentarlo con ruido, y la "corrección" que emite es
// indistinguible de aleatoria. Medido en la cabina: eso es lo que hacía
// patinar al dron en las curvas y quedarse corto acelerando a fondo — no era
// la física, era el genoma equivocado corrigiendo sobre datos que nunca
// entrenó.
const VISOR_PERSEC: i32 = 121;
/// Slot del repertorio de entrega instalado (`REP_ENTREGA`).
const VISOR_REP: i32 = 98;
/// **El repertorio de entrega instalado**: celdas + módulo vertical al lado
/// (`repertorio::ruta_vertical`). Si existe, vuela ÉL en vez del genoma
/// suelto, con el mismo `repertorio::Volador` que usa el gym. Si no existe,
/// nada cambia.
const REP_ENTREGA: &str = "genomes/DRON3D_ENTREGA.repertorio.bin";

/// Cada vez que el visor arranca un vuelo nuevo (y borra la memoria de la
/// red) sube este número, para que el hilo de física rearme el `Volador`
/// con memoria limpia también en el módulo vertical.
static VUELO_NUEVO: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn reiniciar_red_visor() {
    crate::iafz::reset_memoria(VISOR);
    VUELO_NUEVO.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
// A nivel de módulo (no dentro de `volar`) porque `Mundo::armar_entrega` y
// los handlers de `/punto` y `/control?k=r` también necesitan resetear la
// memoria recurrente al reposicionar el dron — ver el comentario en
// `armar_entrega`.

pub struct Mundo {
    pub d: Dron3,
    /// El objetivo que ve la red EN ESTE INSTANTE: durante una misión es el
    /// tramo actual (incluida su altura, que en una posada es 0); si no hay
    /// misión, es `objetivo_libre`. Se recalcula cada paso — **no es
    /// autoritativo**, es un espejo para `/estado` y para el raycast del
    /// visor. Mutarlo directamente fue el bug real detrás de "el dron vuela
    /// rasante y tiembla": el tramo de posada de una entrega escribía z=0
    /// acá, la entrega terminaba, y ese 0 quedaba pegado para siempre porque
    /// nada lo devolvía a `objetivo_libre`. Todo vuelo siguiente perseguía
    /// honestamente un objetivo casi en el piso.
    pub obj: (f32, f32, f32),
    /// El objetivo de verdad: el que controla el usuario con WASD o
    /// tocando el piso. Es lo que arma la próxima entrega y lo que vuelve a
    /// regir en cuanto no hay misión en curso.
    pub objetivo_libre: (f32, f32, f32),
    pub piloto_ia: bool,
    /// Qué cerebro emite las salidas de la red en la tarea de punto/entrega:
    /// el genoma suelto, el repertorio instalado (celdas + módulo vertical) o
    /// la adaptación en vuelo. `/estado` lo publica como `cerebro` (regla
    /// 3.12: saber QUÉ se está mirando).
    pub cerebro: String,
    pub ruido: Ruido,
    pub empujon: (f32, f32),
    pub golpes: u32,
    pub segundos: f32,
    pub bateria: f32,
    /// Lo que ve y decide NUESTRA red (IAFZ-64), publicado para el visor —
    /// reemplaza al panel del conectoma de la mosca (ver CLAUDE.md, sección
    /// del experimento final: ni HS/VS ni el Complejo Central superaron de
    /// forma confiable a no ponerles nada, medido con 3 semillas).
    ///
    /// Al principio esto sólo publicaba entrada y salida —"el kernel es
    /// denso, no hay neuronas que mostrar" — pero eso estaba mal: las DOS
    /// capas binarias SÍ tienen un patrón de disparo real, un bit por
    /// neurona (`iafz::activaciones`), sólo que antes el kernel lo calculaba
    /// y lo tiraba sin exponerlo. `leer_activaciones_iafz_64` (csrc) lo deja
    /// leer sin tocar el cómputo. Ahora se publican las cinco capas reales:
    /// sensores → oculta → estrategia → salida, más la memoria recurrente
    /// (que realimenta a sensores en la próxima decisión).
    pub sensores_red: String,
    pub salidas_red: [f32; iafz::SALIDAS],
    /// Capa oculta (64 bits) y de estrategia (64 bits): un bit por neurona,
    /// 1 = disparó en la última decisión. Capa de memoria recurrente (32
    /// bits): el estado ACTUAL, el que se usa como contexto en la próxima.
    pub l1_red: String,
    pub l2_red: String,
    pub mem_red: String,
    /// Postes que el dron tiene que esquivar, con la MISMA física que el gym
    /// (`Obstaculo::choca`): rozar uno es tan fatal como tocar el suelo.
    ///
    /// El visor no los tenía. Eso significaba que el gym entrenaba esquivando
    /// postes y desde el visor era imposible ver —ni desmentir— que el dron
    /// los esquivara. Una capacidad que no se puede mirar no está terminada.
    pub obstaculos: Vec<Obstaculo>,
    /// **Lo que el dron vio en esta sesión** (`plan::Mapa`): persiste entre
    /// tareas, así que un camino ya recorrido se vuelve a hacer sabiendo
    /// dónde está cada poste. Se borra con «quitar postes».
    pub mapa: crate::plan::Mapa,
    /// Punto de partida del próximo reinicio. Se puede mover desde el visor
    /// para armar rutas A→B a mano.
    pub partida: (f32, f32, f32),
    /// Segundos desde que se estrelló. Existe aparte de `segundos` porque ese
    /// es el tiempo de VUELO: al chocar ya valía más de 1, así que la pausa de
    /// "reaparece tras un segundo" no ocurría nunca y el choque era invisible
    /// —el dron se teletransportaba al punto de partida sin que se viera nada.
    pub pausa: f32,
    /// La misión de entrega en curso, si la hay. Se arma con los puntos A y B
    /// que el usuario ya puso a mano, así que "entregar de A a B" se prueba
    /// con dos clicks y una tecla.
    ///
    /// Existe por la regla 3.3 de la biblia: una capacidad que está en el gym
    /// y no se puede mirar en el visor no está terminada. Sin esto, "el dron
    /// despega, entrega un paquete y vuelve" sería otra vez una afirmación
    /// imposible de verificar desde afuera.
    pub mision: Option<Mision>,
    pub seguidor: Seguidor,
    /// Cómo viene la misión, en castellano, para el panel.
    pub mision_estado: String,
    /// Pedido de reiniciar los lazos de control. El estabilizador y los
    /// motores viven en el hilo de física, así que quien reposiciona el dron
    /// desde HTTP no los puede tocar: deja esta bandera y el hilo la consume.
    /// Sin esto, arrancar una entrega con la integral de altura cargada del
    /// vuelo anterior daba un despegue a los saltos que no era culpa de nadie.
    pub reiniciar_control: bool,

    // --- MODO PERSECUCIÓN: el usuario maneja la moto y el dron lo sigue ---
    /// Si está puesto, el objetivo del dron deja de ser un punto y pasa a ser
    /// el PUESTO detrás de la moto (`persecucion::puesto`), recalculado cada
    /// paso igual que `obj`. Es la misma regla del gym porque usa la misma
    /// función — el visor no puede mostrar una persecución distinta de la que
    /// se entrena (regla 3.3).
    pub modo_moto: bool,
    pub moto: Movil,
    /// Lo que el usuario está apretando ahora. Lo escribe HTTP, lo consume el
    /// hilo de física.
    pub mando: Mando,
    /// Si se está grabando el manejo para usarlo como caso de entrenamiento.
    pub grabando: bool,
    /// Las muestras grabadas hasta ahora (a `movil::PASOS_POR_MUESTRA`).
    pub grabacion: Vec<Mando>,
    /// Qué tan bien lo viene siguiendo el dron, para el panel: distancia al
    /// puesto y fracción del tiempo dentro de la ventana.
    pub error_puesto: f32,
    pub pasos_a_tiro: u32,
    pub pasos_persiguiendo: u32,
    /// Último mensaje de la cabina (grabación guardada, etc.).
    pub aviso: String,
    /// El estado del entrenamiento de persecución, tal cual lo escribió
    /// `entrenarpersecucion` en OTRO proceso — JSON crudo, `"null"` si no hay
    /// ninguno corriendo. Se manda sin parsear: `persecucion::leer_estado`
    /// ya lo entrega listo.
    pub entrenamiento: String,

    // --- MODO PILOTO MANUAL: la persona vuela el DRON, no la moto ---
    /// Si está puesto, `roll_sp/pitch_sp/yaw_sp/alt_sp` —la MISMA superficie
    /// de control que usan la red y el piloto clásico— se llenan con el
    /// joystick de la persona, en vez de `consignas_red` o `Guiado`. La idea
    /// (pedida por el usuario): si un humano no puede volarlo bien con esos
    /// cuatro números, es evidencia de que el esquema de control está mal,
    /// no sólo la red.
    pub manual_dron: bool,
    /// Qué régimen (recta/curva/transitorio) es el tramo actual de la moto
    /// ahora mismo. Sólo tiene sentido en modo persecución.
    pub regimen: &'static str,
    /// Palancas del joystick del dron, en [-1, 1]. Las escribe HTTP a ~30 Hz,
    /// las consume el hilo de física a 200 Hz — igual que `mando` con la
    /// moto.
    pub mando_dron: MandoDron,

    // --- OJO REAL: la cámara de verdad alimentando la percepción ---
    /// Lo último que mandó `tools/ojo.py`: rumbo a la amenaza y proximidad,
    /// sacados de flujo óptico de una cámara FÍSICA en vez de calculados de
    /// la posición exacta de los postes.
    ///
    /// Es el primer punto del proyecto donde el mundo real toca el cerebro
    /// del dron. Hasta acá `RasgosVisuales` salía siempre de trigonometría
    /// sobre el estado exacto del simulador — que en un aparato de verdad no
    /// se lo regala nadie — y la biblia lo lista como uno de los tapones para
    /// hardware.
    pub ojo: [f32; crate::sim3d::SECTORES],
    /// Pasos de física transcurridos. Lo lleva el hilo de física; lo lee el
    /// HTTP para sellar cuándo llegó el último dato del ojo.
    pub pasos: u64,
    /// Cuándo llegó ese dato (pasos de física). Si se pone viejo, la
    /// percepción real se apaga sola: mejor volver a la simulada que volar
    /// con un rumbo congelado de hace tres segundos.
    pub ojo_visto: u64,
    /// Si la percepción de obstáculos sale de la cámara real en vez del
    /// simulador. Lo prende el usuario; se apaga solo si el ojo se queda
    /// mudo.
    pub ojo_manda: bool,

    /// Si el genoma cargado habla el mismo idioma que los sensores de HOY.
    /// Ver `iafz::VERSION_SENSORES`. Cuando es `false`, `piloto_ia` no se deja
    /// prender: la red no puede volar leyendo basura sólo porque alguien tocó
    /// `P` sin saber que el genoma es viejo. Pasó de verdad —el usuario vio el
    /// dron "no esquivar ni un poste de frente" y resultó ser exactamente
    /// esto— y el arreglo anterior (arrancar en clásico) no alcanzaba porque
    /// `P` lo pisaba en silencio.
    pub compatible: bool,

    /// **Adaptación en vuelo** (`vivo.rs`): apagada por defecto. Cuando se
    /// prende, el dron deja de volar un genoma congelado y pasa a refinarlo
    /// mientras vuela, con el ancla protegida — ver la cabecera de `vivo.rs`.
    /// Es `Option` y no un `bool` porque sembrar el ancla necesita un genoma
    /// cargado: sin genoma no hay nada que adaptar.
    pub vivo: Option<crate::vivo::Adaptacion>,
}

/// Las cuatro palancas del piloto manual, todas en [-1, 1] antes de escalar.
/// `alt` es una TASA (sube/baja), no una altura: así el stick vuelve al
/// centro y el dron no se cae — igual que el acelerador de un dron de
/// verdad, y usa los mismos topes de régimen que la cascada de altura
/// (`Estabilizador3`, 3.0 m/s subiendo / 2.5 m/s bajando, regla 3.1 de la
/// biblia) para que "lo que se siente" sea lo mismo que puede pedir la red.
#[derive(Clone, Copy, Default)]
pub struct MandoDron {
    pub roll: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub alt: f32,
}

/// Traduce las palancas del piloto manual a `(roll_sp, pitch_sp, yaw_sp,
/// alt_sp)` — la MISMA superficie que usan la red y el clásico, con las
/// MISMAS cotas de autoridad que `Guiado::default()` (0.40 rad de banqueo,
/// 1.2 rad/s de guiñada). Separado del bucle de física a propósito: es la
/// única pieza de este archivo que se puede probar con `cargo test` sin
/// levantar el servidor, y acá vivió el bug real más caro de la sesión de
/// piloto manual — ver `pruebas_manual` más abajo.
fn control_manual(md: MandoDron, alt_sp_actual: f32, dt: f32) -> (f32, f32, f32, f32) {
    const ANG_MAX_MANUAL: f32 = 0.40;
    const YAW_MAX_MANUAL: f32 = 1.2;
    // La altura es una TASA (el stick vuelve al centro solo), no una
    // posición — como el acelerador de un dron de verdad. 3.0 m/s de tope de
    // subida es el mismo régimen que ya impone la cascada de
    // `Estabilizador3` (regla 3.1), así que el stick no puede pedir nada que
    // el lazo de abajo no vaya a hacerle caso de todos modos.
    //
    // Que sea una TASA es justo lo que hizo real el bug de "no se queda
    // quieto": `alt_sp_actual` viene de una variable que vive en el hilo de
    // física desde que arranca el servicio, no del `Mundo`. Si quien llama
    // no la re-ancla a la altura real al entrar a manual (`web.rs`, bloque
    // de `reiniciar_control`), esta función construye honestamente sobre un
    // valor heredado del piloto ANTERIOR — no es un bug de esta función,
    // pero si el llamador vuelve a romper ese anclaje, la prueba de abajo
    // que fija "en reposo no cambia nada" lo va a dejar pasar igual, porque
    // acá adentro "no cambia nada" es exactamente lo correcto. El anclaje en
    // sí no es testeable sin levantar el servidor — quedó documentado en el
    // comentario de ese bloque en vez de en una prueba.
    const TASA_ALT_MANUAL: f32 = 3.0;
    let roll_sp = md.roll * ANG_MAX_MANUAL;
    let pitch_sp = md.pitch * ANG_MAX_MANUAL;
    let yaw_sp = md.yaw * YAW_MAX_MANUAL;
    let alt_sp = (alt_sp_actual + md.alt * TASA_ALT_MANUAL * dt).clamp(0.3, 20.0);
    (roll_sp, pitch_sp, yaw_sp, alt_sp)
}

#[cfg(test)]
mod pruebas_manual {
    use super::*;

    /// El bug real: soltar los sticks no dejaba el dron quieto porque la
    /// altura es una TASA integrada. Esta prueba fija la propiedad que tiene
    /// que valer siempre — mando en reposo, salida en reposo — para que
    /// nadie vuelva a convertir `alt_sp` en algo que se mueve solo.
    #[test]
    fn mando_en_reposo_no_cambia_nada() {
        let md = MandoDron::default();
        for alt_previa in [0.5, 3.0, 12.7] {
            let (roll, pitch, yaw, alt) = control_manual(md, alt_previa, 1.0 / 200.0);
            assert_eq!(roll, 0.0);
            assert_eq!(pitch, 0.0);
            assert_eq!(yaw, 0.0);
            assert_eq!(alt, alt_previa, "con el stick de altura en cero, alt_sp no puede moverse solo");
        }
    }

    /// Las cotas de autoridad tienen que ser EXACTAMENTE las de
    /// `Guiado::default()` (0.40 rad, 1.2 rad/s) — si no, "cómo se siente"
    /// no dice nada sobre si los controles de la IA están bien pensados,
    /// que es el motivo entero de que este modo exista.
    #[test]
    fn el_fondo_del_stick_no_pasa_las_cotas_de_guiado_default() {
        let md = MandoDron { roll: 1.0, pitch: -1.0, yaw: 1.0, alt: 0.0 };
        let (roll, pitch, yaw, _) = control_manual(md, 3.0, 1.0 / 200.0);
        assert!((roll - 0.40).abs() < 1e-6);
        assert!((pitch - (-0.40)).abs() < 1e-6);
        assert!((yaw - 1.2).abs() < 1e-6);
    }

    /// La altura nunca puede pedir un valor imposible (bajo el piso, o un
    /// techo absurdo) por más que el stick se mantenga a fondo mucho tiempo
    /// — `dt` grande simula "sostener el stick varios segundos" sin tener
    /// que llamar la función miles de veces.
    #[test]
    fn el_stick_de_altura_no_saca_el_setpoint_del_sobre() {
        let (_, _, _, subiendo) = control_manual(
            MandoDron { alt: 1.0, ..Default::default() }, 3.0, 100.0);
        assert!(subiendo <= 20.0);
        let (_, _, _, bajando) = control_manual(
            MandoDron { alt: -1.0, ..Default::default() }, 3.0, 100.0);
        assert!(bajando >= 0.3);
    }
}

impl Mundo {
    fn nuevo() -> Self {
        Mundo {
            d: Dron3::nuevo(0.0, 0.0, 3.0).con_aterrizaje(),
            // Objetivo lejos de la partida A PROPÓSITO. Antes los dos eran
            // (0,0,3): el dron "llegaba" apenas nacía, así que abrir la
            // página sin tocar nada mostraba un dron flotando sin hacer
            // nada — no porque no supiera navegar, sino porque no tenía
            // ninguna tarea. Con el objetivo en la ruta recta del proyecto
            // (`gym3d::ESCENARIOS[0]`), el visor demuestra "ir de A a B y
            // frenar" (la deuda del orbitar) desde el primer segundo, sin
            // que nadie tenga que saber que existe el WASD.
            obj: (6.0, 0.0, 3.0),
            objetivo_libre: (6.0, 0.0, 3.0),
            piloto_ia: true,
            cerebro: String::from("genoma"),
            ruido: Ruido::REAL,
            empujon: (0.0, 0.0),
            golpes: 0,
            segundos: 0.0,
            bateria: 1.0,
            sensores_red: String::new(),
            salidas_red: [0.0; iafz::SALIDAS],
            l1_red: String::new(),
            l2_red: String::new(),
            mem_red: String::new(),
            obstaculos: Vec::new(),
            mapa: crate::plan::Mapa::default(),
            partida: (0.0, 0.0, 3.0),
            pausa: 0.0,
            mision: None,
            seguidor: Seguidor::nuevo(false),
            mision_estado: String::new(),
            reiniciar_control: false,
            modo_moto: false,
            // Arranca a 40 m detrás del origen mirando hacia +x, para que al
            // entrar en modo moto haya pista por delante y no haya que buscar
            // dónde está nada.
            moto: Movil::en_vivo(-40.0, 0.0, 0.0),
            mando: Mando::default(),
            grabando: false,
            grabacion: Vec::new(),
            error_puesto: 0.0,
            pasos_a_tiro: 0,
            pasos_persiguiendo: 0,
            aviso: String::new(),
            entrenamiento: "null".to_string(),
            manual_dron: false,
            regimen: "",
            mando_dron: MandoDron::default(),
            ojo: [0.0; crate::sim3d::SECTORES],
            pasos: 0,
            ojo_visto: 0,
            ojo_manda: false,
            compatible: true, // optimista: se corrige apenas arranca `volar`
            vivo: None,
        }
    }

    /// Arma la misión de entrega con los puntos que el usuario ya puso: sale
    /// del suelo bajo A, cruza hasta B, se posa, despega, vuelve y se posa.
    /// Los postes que haya tirados en vivo son los mismos de la misión.
    fn armar_entrega(&mut self) {
        let (ax, ay, _) = self.partida;
        let (bx, by, bz) = self.objetivo_libre;
        let crucero = bz.max(1.5);
        self.mision = Some(Mision {
            nombre: "entrega",
            partida: (ax, ay, 0.0),
            desde_suelo: true,
            puntos: vec![
                Punto::aire(ax, ay, crucero),
                Punto::aire(bx, by, crucero),
                Punto::suelo(bx, by),
                Punto::aire(bx, by, crucero),
                Punto::aire(ax, ay, crucero),
                Punto::suelo(ax, ay),
            ],
            obstaculos: self.obstaculos.clone(),
            semilla: 7,
            pasos: 0, // el visor no tiene reloj: vuela hasta que pase algo
        });
        self.seguidor = Seguidor::nuevo(true);
        self.d = Dron3::en_suelo(ax, ay);
        self.segundos = 0.0;
        self.mision_estado = "en curso".to_string();
        self.reiniciar_control = true;
        // Sin esto la memoria recurrente (32 bits) queda pegada de la ENTREGA
        // ANTERIOR: es la única variable de estado que `reiniciar_control` no
        // toca (resetea Estabilizador3, Motores y el filtro, no el kernel).
        // Todo camino de entrenamiento/medición del proyecto (gym.rs,
        // capacidades.rs, mision.rs, etc.) arranca cada episodio con
        // `iafz::reset_memoria` — el visor era el único que no, así que cada
        // entrega repetida empezaba desde un contexto que la red NUNCA vio en
        // entrenamiento. Medido en vivo: memRed tenía 18 de 32 bits en 1 justo
        // después de una entrega CUMPLIDA, y el primer tick de la siguiente
        // entrega heredaba ese mismo patrón en vez de arrancar en limpio. Es
        // la causa más probable de "el dron esquivaba bien la primera vez y
        // después se puso tonto" — regla 3.18, un instrumento (acá, el propio
        // vuelo) que no reproducía las condiciones bajo las que se midió.
        reiniciar_red_visor();
    }
}

/// Los 96 bits de entrada de IAFZ-64, como texto "0"/"1" para el visor — 96
/// bytes a 40 Hz (~3.8 KB/s), sin dependencias de JSON para arrays binarios.
/// Reemplaza a lo que antes codificaba la actividad de 265 neuronas de la
/// mosca (`codificar_actividad`, alfabeto de 64 símbolos): esa función se
/// sacó junto con el subcircuito bio, no tiene sentido sin él.
fn codificar_sensores(lo: u64, hi: u32) -> String {
    let mut s = String::with_capacity(iafz::SENSOR_BITS as usize);
    for i in 0..64 {
        s.push(if (lo >> i) & 1 == 1 { '1' } else { '0' });
    }
    for i in 0..(iafz::SENSOR_BITS - 64) {
        s.push(if (hi >> i) & 1 == 1 { '1' } else { '0' });
    }
    s
}

/// Los `n` bits bajos de `v` como texto "0"/"1" — igual idea que
/// `codificar_sensores` pero para un solo entero (las capas internas caben
/// en un `u64`, no hace falta partirlas en dos palabras).
fn codificar_bits(v: u64, n: u32) -> String {
    (0..n).map(|i| if (v >> i) & 1 == 1 { '1' } else { '0' }).collect()
}

/// Hilo de física: 200 Hz, en tiempo real.
fn volar(mundo: Arc<Mutex<Mundo>>, genoma: String) {
    // Verificado, no cargado a secas: un genoma entrenado con otro código de
    // sensores no falla, vuela TEMBLOROSO — el modelo físico hace el trabajo y
    // la red le suma ruido encima. Eso parece un defecto del diseño y es un
    // archivo viejo. Ver `iafz::VERSION_SENSORES`.
    let mut cargado = match iafz::cargar_compatible(VISOR, &genoma) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("\n  ⚠ GENOMA INCOMPATIBLE\n    {e}\n");
            eprintln!("  El visor vuela con el PILOTO CLÁSICO hasta que haya uno al día.\n");
            // Y se ARRANCA en clásico, no sólo se avisa: si no, el usuario ve
            // un dron tembloroso y lo lee como "así vuela el proyecto".
            let mut m = mundo.lock().unwrap();
            m.piloto_ia = false;
            m.compatible = false;
            false
        }
    };
    if cargado {
        mundo.lock().unwrap().compatible = true;
    }
    let mut sello_genoma = std::fs::metadata(&genoma)
        .and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    // El repertorio de entrega instalado, si hay. Se relee cuando cambia.
    let leer_rep = || -> Option<(std::sync::Arc<Vec<Vec<u8>>>, Option<std::sync::Arc<Vec<u8>>>, Option<std::sync::Arc<Vec<u8>>>)> {
        let r = crate::repertorio::Repertorio::leer(REP_ENTREGA)?;
        let banco = std::sync::Arc::new(r.celdas.iter().map(|c| c.genoma.clone()).collect::<Vec<_>>());
        let v = std::fs::read(crate::repertorio::ruta_vertical(REP_ENTREGA)).ok()
            .filter(|b| b.len() == iafz::BRAIN_BYTES)
            .map(std::sync::Arc::new);
        let pz = std::fs::read(crate::repertorio::ruta_posar(REP_ENTREGA)).ok()
            .filter(|b| b.len() == iafz::BRAIN_BYTES)
            .map(std::sync::Arc::new);
        println!("[visor] repertorio de entrega {REP_ENTREGA}: {} celdas, módulo vertical {}, módulo posar {}",
                 banco.len(), if v.is_some() { "sí" } else { "NO" }, if pz.is_some() { "sí" } else { "NO" });
        Some((banco, v, pz))
    };
    let mut rep_entrega = leer_rep();
    let mut sello_rep = std::fs::metadata(REP_ENTREGA).and_then(|m| m.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let mut volador: Option<crate::repertorio::Volador> = None;
    let mut vuelo_visto = u32::MAX;

    const GENOMA_PERSEC: &str = "genomes/DRON3D_PERSECUCION.adn";
    let mut cargado_persec = iafz::cargar(VISOR_PERSEC, GENOMA_PERSEC);
    let mut sello_persec = std::time::SystemTime::UNIX_EPOCH;
    let mut est = Estabilizador3::nuevo();
    let mut motores = Motores::nuevo();
    let mut viento = Viento3::nuevo(7);
    let mut sensor = Sensores::nuevo(Ruido::REAL, 1234);
    let mut baliza = crate::sim3d::Baliza::nueva(4321);
    let mut camara = Camara::nueva(99);
    let mut via_ruta: Option<((f32, f32), (f32, f32))> = None;
    let mut bajada = crate::gym3d::BajadaPorDivergencia::default();
    let mut divergencia_vista = 0.0f32;
    let guiado = Guiado::default();
    // El guiado de PERSECUCIÓN es otro objeto: otra envolvente (bank hasta
    // 72.9°, ~29 m/s) y otra ley de control (con trim de crucero). Tiene que
    // ser el mismo que usa el gym o el visor mostraría un perseguidor
    // distinto del que se mide.
    let guiado_persec = Guiado::persecucion();
    let mut filtrado: Option<Dron3> = None;
    // Salida anterior de la red, para `limitar_sacudida`: el límite es de
    // VELOCIDAD del residuo, así que necesita memoria entre decisiones.
    let mut o_previa = [0.0f32; iafz::SALIDAS];
    // Para que la adaptación en vuelo sepa si SU ventana se llevó un golpe:
    // el contador es acumulado, así que lo que importa es el incremento.
    let mut golpes_previos = 0u32;
    let (mut roll_sp, mut pitch_sp, mut yaw_sp, mut alt_sp) = (0.0, 0.0, 0.0, 3.0);
    let mut paso = 0usize;

    loop {
        let inicio = std::time::Instant::now();
        if paso % 200 == 0 {
            // Y directo del ARCHIVO que este visor cargó al arrancar
            // (`genoma`, normalmente `genomes/DRON3D_64.adn`): es donde
            // escribe `entrenarmision` cada 20 generaciones. Sin esto, correr
            // `entrenarmision` mientras el visor está abierto no se veía
            // hasta reiniciar el servicio a mano.
            if let Ok(md) = std::fs::metadata(&genoma) {
                if let Ok(t) = md.modified() {
                    if t > sello_genoma {
                        sello_genoma = t;
                        cargado = iafz::cargar_compatible(VISOR, &genoma).is_ok();
                        let mut m = mundo.lock().unwrap();
                        m.compatible = cargado;
                        if !cargado { m.piloto_ia = false; }
                        drop(m);
                        // Visible en `journalctl --user -u dron-visor`: sin
                        // esto no hay forma de CONFIRMAR que la recarga en
                        // vivo pasó de verdad (regla 3.12), sólo de suponerlo.
                        println!("[visor] genoma recargado de {genoma}: {}",
                                 if cargado { "ok" } else { "FALLÓ (incompatible o ilegible)" });
                    }
                }
            }
            // Igual, pero para el genoma de persecución: se recarga solo cada
            // vez que `entrenarpersecucion` lo reescribe (cada 20
            // generaciones), así la cabina mejora en vivo sin reiniciar nada.
            if let Ok(t) = std::fs::metadata(REP_ENTREGA).and_then(|m| m.modified()) {
                if t > sello_rep {
                    sello_rep = t;
                    rep_entrega = leer_rep();
                    vuelo_visto = u32::MAX; // rearmar el Volador
                }
            }
            if let Ok(md) = std::fs::metadata(GENOMA_PERSEC) {
                if let Ok(t) = md.modified() {
                    if t > sello_persec {
                        sello_persec = t;
                        cargado_persec = iafz::cargar(VISOR_PERSEC, GENOMA_PERSEC);
                    }
                }
            }
            // Igual que arriba pero para el entrenamiento de persecución
            // (`entrenarpersecucion`), que corre en OTRO proceso y no tiene
            // forma de escribir directo en `Mundo`. La cabina lo muestra en
            // un widget chico.
            if let Ok(mut m) = mundo.lock() {
                m.entrenamiento = crate::persecucion::leer_estado();
            }
        }
        {
            let mut m = mundo.lock().unwrap();
            // Con misión en curso, el objetivo que persigue el dron —y que ve
            // la red— es el TRAMO actual, no el punto B. Es la misma regla del
            // gym porque es literalmente el mismo `Seguidor`. Se RECALCULA acá
            // cada paso a partir de `objetivo_libre`, nunca se acumula: así el
            // z=0 de un tramo de posada no puede quedar pegado después de que
            // la misión termine (ver el comentario del campo `obj`).
            // --- la moto, si la está manejando alguien ---
            // Se adelanta ANTES de calcular el objetivo, así el dron persigue
            // dónde está la moto ahora y no dónde estaba el paso pasado.
            if m.modo_moto && !m.d.caido {
                let mando = m.mando;
                m.moto.manejar(mando);
                m.moto.paso();
                // Grabar es submuestrear: una muestra cada
                // `PASOS_POR_MUESTRA` pasos de física, igual que como lo
                // reproduce `Movil::grabado`. Si esto no coincidiera, lo
                // grabado se reproduciría a otra velocidad y el caso de
                // entrenamiento no sería el que el usuario manejó.
                if m.grabando && paso % crate::movil::PASOS_POR_MUESTRA == 0 {
                    let actual = m.moto.mando_actual();
                    m.grabacion.push(actual);
                }
            }

            // El objetivo del dron: el puesto detrás de la moto si hay
            // persecución, el tramo de la misión si hay entrega, o el punto
            // libre. Se RECALCULA cada paso, nunca se acumula (ver `obj`).
            let guia_moto = m.modo_moto.then(|| crate::persecucion::puesto(&m.moto));
            let obj_calculado = match (&guia_moto, &m.mision) {
                (Some(g), _) => (g.x, g.y, g.z),
                (None, Some(mi)) => {
                    m.seguidor.punto(mi).map_or(m.objetivo_libre, |p| (p.x, p.y, p.z))
                }
                (None, None) => m.objetivo_libre,
            };
            m.obj = obj_calculado;
            let (ox, oy, oz) = m.obj;

            if m.d.caido {
                // Reaparece tras un segundo y medio: es un visor, no un
                // castigo — pero tiene que alcanzar para VER que se estrelló.
                m.pausa += DT;
                if m.pausa > 1.5 {
                    let obj = m.objetivo_libre;
                    let ia = m.piloto_ia;
                    let ruido = m.ruido;
                    let golpes = m.golpes;
                    let obs = std::mem::take(&mut m.obstaculos);
                    let partida = m.partida;
                    // El cartel de cómo terminó la misión sobrevive al
                    // reinicio: si se borrara, el dron reaparecería volando y
                    // el usuario no tendría forma de saber que la entrega se
                    // acaba de estrellar.
                    let estado = std::mem::take(&mut m.mision_estado);
                    // La persecución SOBREVIVE al choque del dron: la moto la
                    // maneja una persona y no tiene por qué teletransportarse
                    // porque el dron se mató, y sobre todo **la grabación no se
                    // puede perder** — es el trabajo del usuario. El dron
                    // reaparece en su puesto detrás de la moto, no en `partida`.
                    let moto_v = m.moto.clone();
                    let modo_v = m.modo_moto;
                    let grab_v = std::mem::take(&mut m.grabacion);
                    let grabando_v = m.grabando;
                    let mando_v = m.mando;
                    let (a_tiro_v, persig_v) = (m.pasos_a_tiro, m.pasos_persiguiendo);
                    let aviso_v = std::mem::take(&mut m.aviso);
                    // El modo manual SOBREVIVE al choque: si no, la persona
                    // que está volando el dron a mano se encontraría con que
                    // la IA (o el clásico) tomó el control solo después de
                    // estrellarse, que es justo el momento en que más
                    // confunde no saber quién está manejando.
                    let (manual_v, mando_dron_v) = (m.manual_dron, m.mando_dron);
                    // La adaptación en vuelo SOBREVIVE al choque: es lo
                    // aprendido, no parte del vuelo que se acaba de romper.
                    // Sin esto, cada caída —y en pleno aprendizaje caerse es
                    // frecuente— borraba el ancla y el repertorio entero
                    // todavía no guardado a disco, y "aprende en vivo" se
                    // reiniciaba solo cada pocos segundos sin que nadie lo
                    // pidiera. Verificado en el visor: `vivo` en `/estado`
                    // volvía a vacío 5 s después de encenderlo, sin tocar
                    // nada — coincidía con la reaparición tras la caída.
                    let vivo_v = m.vivo.take();
                    *m = Mundo::nuevo();
                    m.mision_estado = estado;
                    m.moto = moto_v;
                    m.modo_moto = modo_v;
                    m.grabacion = grab_v;
                    m.grabando = grabando_v;
                    m.mando = mando_v;
                    m.pasos_a_tiro = a_tiro_v;
                    m.pasos_persiguiendo = persig_v;
                    m.aviso = aviso_v;
                    m.objetivo_libre = obj;
                    m.piloto_ia = ia;
                    m.manual_dron = manual_v;
                    m.mando_dron = mando_dron_v;
                    m.vivo = vivo_v;
                    m.ruido = ruido;
                    m.golpes = golpes;
                    m.obstaculos = obs;
                    m.partida = partida;
                    m.d = if m.modo_moto {
                        let g = crate::persecucion::puesto(&m.moto);
                        let mut d = Dron3::nuevo(g.x, g.y, g.z).con_aterrizaje();
                        d.yaw = m.moto.rumbo;
                        d.vx = g.vx;
                        d.vy = g.vy;
                        d
                    } else {
                        Dron3::nuevo(partida.0, partida.1, partida.2).con_aterrizaje()
                    };
                    est = Estabilizador3::nuevo();
                    motores = Motores::nuevo();
                    filtrado = None;
                }
            } else {
                // Alguien reposicionó el dron desde el visor: los lazos
                // arrancan de cero, como en un vuelo nuevo.
                if std::mem::take(&mut m.reiniciar_control) {
                    est = Estabilizador3::nuevo();
                    motores = Motores::nuevo();
                    filtrado = None;
                    // `alt_sp` es una variable del HILO DE FÍSICA, viva desde
                    // que arrancó el servicio — no del `Mundo`. Sin esto,
                    // entrar a manual (o reposicionar el dron) heredaba el
                    // último setpoint de altura que había dejado el piloto
                    // ANTERIOR (IA o clásico, apuntando a cualquier `oz`
                    // vieja), y como el stick de altura es una TASA que
                    // integra sobre `alt_sp`, el dron perseguía una altura
                    // fantasma en vez de quedarse donde está — medido: con
                    // el stick en cero se estabilizaba en 2.18 m en vez de
                    // en los ~2.9 m reales, y no volvía nunca porque nada
                    // volvía a mover `alt_sp` hacia la altura verdadera.
                    alt_sp = m.d.z;
                    // Sin GPS la posición es odometría: si el dron se
                    // reposiciona y la odometría no, el dron cree estar donde
                    // estaba antes. Medido: el mapa del visor salía corrido
                    // ~5 m de los postes reales (bitácora 7.46). Se reinicia
                    // igual que en el gym, donde cada vuelo arranca sabiendo
                    // dónde está.
                    sensor = Sensores::nuevo(Ruido::REAL, 1234);
                    baliza = crate::sim3d::Baliza::nueva(4321);
                    via_ruta = None;
                }
                let crudo = sensor.leer(&m.d);
                // **El filtro pasabajos corre SIEMPRE, no sólo para el
                // clásico.** Estaba adentro de la rama clásica, así que el
                // híbrido le pasaba lecturas CRUDAS al `Guiado` de adentro
                // mientras el clásico de verdad usa filtradas — misma causa
                // y mismo arreglo que en `mision.rs`, donde eso costaba
                // x10.81 de sacudida sobre el clásico.
                {
                    let f = filtrado.get_or_insert(crudo);
                    let a = 0.08;
                    f.x += (crudo.x - f.x) * a;
                    f.y += (crudo.y - f.y) * a;
                    f.z += (crudo.z - f.z) * a;
                    f.vx += (crudo.vx - f.vx) * a;
                    f.vy += (crudo.vy - f.vy) * a;
                    f.vz += (crudo.vz - f.vz) * a;
                    f.roll = crudo.roll;
                    f.pitch = crudo.pitch;
                    f.yaw = crudo.yaw;
                    f.p = crudo.p;
                    f.q = crudo.q;
                    f.r = crudo.r;
                }
                let filt = filtrado.unwrap_or(crudo);
                // Sin GPS: dónde CREE el dron que está el objetivo (lo que ve
                // la cámara, en su marco odométrico). Igual que el gym.
                let (bx, by) = baliza.ver(&m.d, &crudo, (ox, oy, oz), &m.ruido);
                // Tareas dichas: mapa y ruta (`plan.rs`), la MISMA función
                // que el gym (`mision::planificar`, regla 3.3).
                let (bx, by) = if guia_moto.is_none()
                    && m.mision.as_ref().is_some_and(|mi| mi.planifica())
                {
                    let obs = m.obstaculos.clone();
                    let ruido_v = m.ruido;
                    let dron = m.d;
                    let mut mapa = std::mem::take(&mut m.mapa);
                    let r = crate::mision::planificar(&mut mapa, &mut via_ruta, &mut camara, &mut sensor, &mut baliza, paso,
                                                      &dron, &crudo, (bx, by, oz), &obs, &ruido_v);
                    m.mapa = mapa;
                    r
                } else {
                    (bx, by)
                };
                // Lo último que la cámara vio del anillo de proximidad. Lo
                // necesita el piloto CLÁSICO del visor, no sólo la red: desde
                // que la vara honesta esquiva (`Guiado::consignas_esquivando`)
                // un clásico ciego acá mostraría un piloto distinto del que se
                // entrena y se mide, que es el bug de la bitácora 7.7 otra vez
                // (regla 3.12, punto 4: «qué traducción de salidas usa»).
                let mut anillo_visto = [0.0f32; crate::sim3d::SECTORES];
                let mut rumbo_visto = 0.0f32;

                // NUESTRA red piensa siempre, vuele quien vuele — no sólo
                // cuando está al mando. Así el panel "qué opina la red" del
                // visor tiene sentido incluso con el piloto clásico puesto,
                // y cuando se prende con P ya arranca con la memoria
                // recurrente al día, no en frío.
                if paso % 2 == 0 {
                    // `ver_con`, no `ver`: la red tiene que VER los postes,
                    // igual que en el gym. Con `ver` volaba ciega a ellos y
                    // "esquivar" en el visor era casualidad, no capacidad.
                    let obs_vista = m.obstaculos.clone();
                    let mut vista = camara.ver_con(&m.d, ox, oy, oz, &m.ruido, &obs_vista);

                    // **La cámara REAL manda, si hay una hablando.** Se pisan
                    // sólo los dos campos que el flujo óptico puede dar de
                    // verdad; `obs_altura` NO, porque sacar elevación de una
                    // cámara monocular necesita profundidad o saber el tamaño
                    // del obstáculo, y ninguna de las dos se tiene. Inventarla
                    // sería exactamente el tipo de instrumento mentiroso que
                    // la regla 3.18 persigue.
                    //
                    // Se apaga sola si el ojo se queda mudo medio segundo: un
                    // rumbo congelado es peor que no tener percepción, porque
                    // el dron sigue esquivando un fantasma.
                    if m.ojo_manda && (paso as u64).saturating_sub(m.ojo_visto) < 100 {
                        vista.anillo = m.ojo;
                    }
                    divergencia_vista = vista.divergencia;
                    // Después del pisado de la cámara real, así que el clásico
                    // del visor esquiva con lo MISMO que ve la red.
                    anillo_visto = vista.anillo;
                    rumbo_visto = vista.obs_horizontal;

                    let c = match &guia_moto {
                        // PERSIGUIENDO: piloto híbrido, exactamente el del
                        // gym de persecución — el modelo físico pilota y la
                        // red corrige encima. Mismas dos funciones
                        // (`sensores_guia` + `consignas_hibridas`), así que
                        // lo que se ve acá es lo que se entrenó, no una
                        // imitación parecida (regla 3.3).
                        Some(g) => {
                            let g = &crate::persecucion::visto_desde(g, &m.d, &crudo);
                            let (lo, hi) = sensores_guia(&crudo, &vista, g);
                            m.sensores_red = codificar_sensores(lo, hi);
                            // Si todavía no hay genoma de persecución
                            // cargado, `o` queda en cero — y por la
                            // propiedad verificada en
                            // `persecucion::hibrido` ("con la red en
                            // cero es EXACTAMENTE el clásico"), eso
                            // significa volar clásico de verdad, no
                            // volar con pesos al azar disfrazados de red.
                            let o = if cargado_persec {
                                iafz::pensar_multi(VISOR_PERSEC, lo, hi)
                            } else {
                                [0.0; iafz::SALIDAS]
                            };
                            m.salidas_red = o;
                            let (l1, l2, mem) = iafz::activaciones(VISOR_PERSEC);
                            m.l1_red = codificar_bits(l1, iafz::NEURONAS);
                            m.l2_red = codificar_bits(l2, iafz::NEURONAS);
                            m.mem_red = codificar_bits(mem as u64, iafz::MEMORIA_BITS);
                            consignas_hibridas(&o, &crudo, g, &guiado_persec)
                        }
                        // NAVEGANDO A UN PUNTO: el piloto HÍBRIDO, el
                        // mismo de `mision.rs` y `capacidades.rs`.
                        //
                        // Acá vivía el bug más caro de la sesión de la
                        // interfaz, y es exactamente el que la regla 3.3
                        // prohíbe: esta rama seguía en `consignas_red`
                        // —la formulación VIEJA, donde la red REEMPLAZA
                        // al piloto clásico— mientras el entrenamiento
                        // (`mision.rs`) y el informe (`capacidades.rs`)
                        // ya usaban `consignas_hibridas_punto`, donde la
                        // red CORRIGE encima del modelo físico. Los
                        // mismos bits del genoma significan cosas
                        // distintas en las dos: el visor mostraba un dron
                        // que no era el que se entrenó.
                        //
                        // Medido, con `genomes/DRON3D_64.adn` instalado y
                        // objetivo (6,0,3): `capacidades` daba 5/5 en «ir
                        // de A a B» mientras el visor lo tenía dando
                        // vueltas a 2.43-4.35 m del punto sin entrar
                        // nunca. No era el genoma ni el objetivo ni el
                        // piloto: era esta línea.
                        None => {
                            let (lo, hi) = sensores(&crudo, &vista, bx, by, oz);
                            m.sensores_red = codificar_sensores(lo, hi);
                            // Con la adaptación en vuelo encendida, el genoma
                            // que decide no es siempre el mismo: alterna entre
                            // el ancla (lo que ya sabe) y el aspirante (la
                            // variante bajo prueba). Ver `vivo.rs`.
                            let vivo_on = matches!(&m.vivo, Some(v) if v.encendida());
                            let (slot, mut o) = if vivo_on {
                                let s = m.vivo.as_ref().map(|v| v.slot()).unwrap_or(VISOR);
                                m.cerebro = String::from("adaptación en vuelo");
                                (s, iafz::pensar_multi(s, lo, hi))
                            } else if let Some((banco, vert, posar_m)) = rep_entrega.as_ref() {
                                let vuelo = VUELO_NUEVO.load(std::sync::atomic::Ordering::Relaxed);
                                if volador.is_none() || vuelo != vuelo_visto {
                                    vuelo_visto = vuelo;
                                    volador = Some(crate::repertorio::Volador::nuevo(
                                        VISOR_REP, crate::repertorio::CELDAS,
                                        Some(banco.clone()), vert.clone(), posar_m.clone()));
                                }
                                let celda = crate::mision::celda_de(&vista, &filt, bx, by, oz);
                                // ¿El tramo de ahora es de aterrizaje? Lo dice
                                // la tarea, igual que en el gym.
                                let posando = m.mision.as_ref()
                                    .and_then(|mi| m.seguidor.punto(mi)).is_some_and(|p| p.posar);
                                m.cerebro = if posando && posar_m.is_some() {
                                    String::from("repertorio de entrega · MÓDULO POSAR")
                                } else {
                                    format!("repertorio de entrega · celda «{}»{}", celda.nombre(),
                                            if vert.is_some() { " + módulo vertical" } else { "" })
                                };
                                let o = volador.as_mut().map(|v| v.pensar(celda.indice(), posando, lo, hi))
                                    .unwrap_or([0.0; iafz::SALIDAS]);
                                (VISOR_REP, o)
                            } else {
                                m.cerebro = String::from("genoma");
                                (VISOR, iafz::pensar_multi(VISOR, lo, hi))
                            };
                            // **La vara honesta, en vuelo.** Con las cuatro
                            // salidas en cero el híbrido ES el piloto clásico
                            // exacto (regla 3.15), sin duplicar una línea de
                            // la cadena de control. La adaptación lo usa para
                            // dos cosas: medir contra qué está compitiendo la
                            // red en este nicho, y apagarla donde ya midió que
                            // estorba.
                            if let Some(v) = &m.vivo {
                                if !v.usar_red() {
                                    o = [0.0; iafz::SALIDAS];
                                }
                            }
                            // MISMO límite de sacudida que el gym y el
                            // informe (`mision.rs`): sin esto el visor
                            // mostraría un residuo que en el gym está
                            // prohibido.
                            crate::persecucion::limitar_sacudida(
                                &mut o, &mut o_previa,
                                crate::persecucion::tope_de_sacudida());
                            m.salidas_red = o;
                            let (l1, l2, mem) = iafz::activaciones(slot);
                            m.l1_red = codificar_bits(l1, iafz::NEURONAS);
                            m.l2_red = codificar_bits(l2, iafz::NEURONAS);
                            m.mem_red = codificar_bits(mem as u64, iafz::MEMORIA_BITS);
                            // `filt`, no `crudo`: el clásico de adentro
                            // del híbrido tiene que ver lo mismo que ve
                            // el clásico de verdad, o la identidad
                            // "red en cero ⟺ clásico" es falsa en la
                            // práctica (ver el comentario del filtro en
                            // `mision.rs`).
                            consignas_hibridas_punto(&o, &filt, bx, by, oz, &guiado, &vista)
                        }
                    };
                    if m.piloto_ia && cargado && !m.manual_dron {
                        (roll_sp, pitch_sp, yaw_sp, alt_sp) = (c.roll, c.pitch, c.yaw, c.alt);
                    }

                    // **La adaptación mide lo que ella misma voló.** Sólo
                    // cuando la red está al mando: juzgar una variante por un
                    // vuelo que pilotó otro (el clásico, o la persona) sería
                    // atribuirle mérito o culpa que no son suyos.
                    if m.piloto_ia && cargado && !m.manual_dron && guia_moto.is_none() {
                        let golpe = m.golpes > golpes_previos;
                        golpes_previos = m.golpes;
                        // Distancia ESTIMADA, no la verdadera: es lo único que
                        // un dron real tiene. `filt` es la misma lectura
                        // filtrada con la que vuela.
                        let dist = ((ox - filt.x).powi(2)
                            + (oy - filt.y).powi(2)
                            + (oz - filt.z).powi(2))
                        .sqrt();
                        let looming = vista.anillo.iter().cloned().fold(0.0f32, f32::max);
                        let vel = (filt.vx * filt.vx + filt.vy * filt.vy
                            + filt.vz * filt.vz).sqrt();
                        if let Some(v) = m.vivo.as_mut() {
                            v.observar(dist, vel, filt.z, looming, (c.roll, c.pitch), golpe);
                            // Lo aprendido se vuelca solo cada tantas
                            // adopciones: si dependiera de apagar bien la
                            // adaptación, un corte de energía se llevaría
                            // horas de vuelo aprendidas.
                            if v.toca_guardar() {
                                let r = v.repertorio.escribir(
                                    crate::repertorio::Repertorio::ARCHIVO);
                                if r.is_ok() {
                                    v.marcar_guardado();
                                }
                            }
                        }
                    }
                }

                if m.manual_dron {
                    (roll_sp, pitch_sp, yaw_sp, alt_sp) =
                        control_manual(m.mando_dron, alt_sp, DT);
                } else if !(m.piloto_ia && cargado) {
                    // Piloto clásico: guiado sobre lecturas filtradas (el
                    // filtro ya corrió arriba, para los dos pilotos).
                    let f = &filt;
                    // Persiguiendo, el clásico usa `consignas_persiguiendo` —el
                    // mismo del gym, con prealimentación de velocidad— y no
                    // `consignas`, que es el de ir a un punto quieto. Con el de
                    // punto quieto el visor mostraría al clásico quedándose 30 m
                    // atrás para siempre (el error en régimen es proporcional a
                    // la velocidad del objetivo) y eso no es lo que se mide en
                    // `dron perseguir`.
                    let (r, p, y) = match &guia_moto {
                        Some(g) => guiado_persec.consignas_persiguiendo(
                            f, &crate::persecucion::visto_desde(g, &m.d, f)),
                        None => guiado.consignas_esquivando(f, bx, by, &anillo_visto, rumbo_visto),
                    };
                    roll_sp = r;
                    pitch_sp = p;
                    yaw_sp = y;
                    // La MISMA aproximación que el gym (regla 3.3): antes el
                    // clásico del visor aterrizaba con `alt_sp = oz` a secas,
                    // sin la bajada en dos velocidades ni la alineación.
                    alt_sp = if guia_moto.is_some() { oz }
                             else { crate::gym3d::altura_de_aproximacion(f, bx, by, oz) };
                }

                // Bajada final a divergencia constante: la MISMA que el gym
                // (`mision::volar_con`, regla 3.3).
                if !m.manual_dron && guia_moto.is_none() {
                    let bajando = crate::gym3d::en_bajada_final(&filt, bx, by, oz);
                    if let Some(vz) = bajada.paso(bajando, divergencia_vista) {
                        let base = crate::gym3d::altura_de_aproximacion(&filt, bx, by, oz);
                        alt_sp = crudo.z + vz / 1.6 + (alt_sp - base);
                    }
                }
                let pedido = est.control(&crudo, alt_sp, roll_sp, pitch_sp, yaw_sp);
                let (e, tx, ty, tz) = motores.aplicar(pedido, DT);
                let v = viento.siguiente();
                let emp = m.empujon;
                // La carga que lleva según la tarea (agarrar/soltar): misma
                // cuenta que el gym, `mision::carga_tras` (regla 3.3).
                let carga = m.mision.as_ref()
                    .map_or(0.0, |mi| crate::mision::carga_tras(mi, m.seguidor.indice));
                let av = crate::sim3d::Averia { masa_extra: carga, ..crate::sim3d::Averia::NINGUNA };
                m.d.paso_averiado(e, tx, ty, tz, (v.0 + emp.0, v.1 + emp.1), &av);
                if m.obstaculos.iter().any(|o| o.choca(&m.d)) {
                    m.d.caido = true;
                }
                if m.d.caido {
                    m.pausa = 0.0;
                    if m.mision.is_some() {
                        m.mision_estado = "SE ESTRELLÓ".to_string();
                    }
                }

                // --- avance de la misión, con la MISMA regla que el gym ---
                if let Some(mi) = m.mision.clone() {
                    let dron = m.d;
                    match m.seguidor.paso(&mi, &dron) {
                        Avance::Sigue => {}
                        Avance::Llego => {
                            if m.seguidor.completa(&mi) {
                                m.mision_estado =
                                    format!("{} CUMPLIDA en {:.1} s", mi.nombre.to_uppercase(), m.segundos);
                                m.mision = None;
                            }
                        }
                        Avance::PosadaIndebida => {
                            m.mision_estado = "se posó donde no debía".to_string();
                            m.mision = None;
                        }
                    }
                }
                // --- qué tan bien lo está siguiendo, para la cabina ---
                if let Some(g) = &guia_moto {
                    let d = m.d;
                    let err = ((g.x - d.x).powi(2) + (g.y - d.y).powi(2)
                               + (g.z - d.z).powi(2)).sqrt();
                    m.error_puesto = err;
                    m.pasos_persiguiendo += 1;
                    if err < crate::persecucion::TOLERANCIA {
                        m.pasos_a_tiro += 1;
                    }
                    // El régimen del tramo (informativo; la mezcla se retiró), para
                    // que se pueda VER en la cabina cuál experto mandaría en
                    // cada momento (regla 3.3: una capacidad que no se puede
                    // mirar no está terminada). Es la MISMA función que usa
                    // el gym, no una imitación.
                    m.regimen = crate::persecucion::regimen_de(g).nombre();
                }

                m.empujon.0 *= 0.9;
                m.empujon.1 *= 0.9;
                m.segundos += DT;
                m.pasos = paso as u64;
                m.bateria = motores.bateria;
            }
        }
        paso += 1;
        let gastado = inicio.elapsed().as_secs_f32();
        if gastado < DT {
            std::thread::sleep(std::time::Duration::from_secs_f32(DT - gastado));
        }
    }
}

/// La IP de Tailscale de esta máquina, si el demonio está corriendo. Mejor
/// esfuerzo: si `tailscale` no está instalado o no hay sesión, no imprime la
/// línea y ya — el bind en 0.0.0.0 funciona igual.
fn ip_tailscale() -> Option<String> {
    let salida = std::process::Command::new("tailscale").args(["ip", "-4"]).output().ok()?;
    let ip = String::from_utf8(salida.stdout).ok()?.lines().next()?.trim().to_string();
    if ip.is_empty() { None } else { Some(ip) }
}

fn responder(mut s: TcpStream, tipo: &str, cuerpo: &[u8]) {
    let cab = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        cuerpo.len()
    );
    let _ = s.write_all(cab.as_bytes());
    let _ = s.write_all(cuerpo);
    let _ = s.flush();
}

pub fn servir(genoma: &str, puerto: u16) -> i32 {
    let mundo = Arc::new(Mutex::new(Mundo::nuevo()));
    {
        let m = Arc::clone(&mundo);
        let g = genoma.to_string();
        std::thread::spawn(move || volar(m, g));
    }

    // 0.0.0.0 a propósito: sin esto el visor sólo se ve desde esta máquina,
    // ni siquiera por Tailscale. El servidor no tiene auth — bien para un
    // tailnet privado, mal para exponerlo a la red abierta.
    let escucha = match TcpListener::bind(("0.0.0.0", puerto)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("no pude abrir el puerto {puerto}: {e}");
            return 1;
        }
    };
    println!("Visor 3D en  http://localhost:{puerto}");
    if let Some(ip) = ip_tailscale() {
        println!("  y por Tailscale en  http://{ip}:{puerto}");
    }
    println!("  WASD mueve el objetivo · Q/E altura · ESPACIO golpe · P cambia piloto · R reinicia");
    println!("  (Ctrl-C para cortar)");

    for flujo in escucha.incoming() {
        let Ok(s) = flujo else { continue };
        let mundo = Arc::clone(&mundo);
        // **Un hilo por conexión, con timeout.** Antes esto era un bucle de UN
        // solo hilo que atendía una conexión a la vez, sin límite de tiempo en
        // `read()`. Medido en vivo: el visor quedó sordo —conexión aceptada,
        // 0 bytes de respuesta, timeout— con varias conexiones acumulando
        // datos SIN LEER en la cola del kernel (`ss` mostraba Recv-Q de
        // cientos de bytes y conexiones en CLOSE-WAIT que el cliente ya había
        // cerrado). Un solo cliente que abre una conexión y no manda el
        // pedido completo —una pestaña de más, una reconexión de Tailscale,
        // lo que sea— deja a `s.read()` esperando para siempre, y como es el
        // mismo hilo que llama `accept()`, TODO el mundo se queda afuera.
        //
        // El timeout es la segunda mitad del arreglo, no cosmética: sin él,
        // un hilo por conexión evita que SE BLOQUEEN ENTRE SÍ, pero un cliente
        // colgado igual deja un hilo vivo para siempre — con suficientes
        // reconexiones fallidas (justo lo que pasaba) se acumulan hilos sin
        // límite. 5 s alcanza de sobra: el navegador manda el pedido HTTP
        // completo en el mismo paquete, no de a poco.
        std::thread::spawn(move || {
            let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            atender(s, mundo);
        });
    }
    0
}

fn atender(mut s: TcpStream, mundo: Arc<Mutex<Mundo>>) {
        let mut buf = [0u8; 2048];
        let n = s.read(&mut buf).unwrap_or(0);
        let pedido = String::from_utf8_lossy(&buf[..n]);
        let ruta = pedido.split_whitespace().nth(1).unwrap_or("/");

        if ruta.starts_with("/estado") {
            let m = mundo.lock().unwrap();
            let json = format!(
                r#"{{"x":{:.3},"y":{:.3},"z":{:.3},"roll":{:.4},"pitch":{:.4},"yaw":{:.4},"vx":{:.2},"vy":{:.2},"vz":{:.2},"ox":{:.2},"oy":{:.2},"oz":{:.2},"caido":{},"ia":{},"golpes":{},"bateria":{:.2},"t":{:.1},"sensRed":"{}","salRed":[{:.3},{:.3},{:.3},{:.3}],"l1Red":"{}","l2Red":"{}","memRed":"{}","extremo":{},"recup":{:.2},"postes":[{}],"mapa":[{}],"px":{:.2},"py":{:.2},"pz":{:.2},"apoyado":{},"mision":[{}],"tramo":{},"misionEstado":"{}","modoMoto":{},"moto":{{"x":{:.2},"y":{:.2},"z":{:.2},"rumbo":{:.4},"rapidez":{:.2},"giro":{:.4}}},"errPuesto":{:.2},"aTiro":{:.3},"grabando":{},"muestras":{},"aviso":"{}","entrenamiento":{},"manual":{},"regimen":"{}","ojoManda":{},"ojo":[{}],"ojoVivo":{},"compatible":{},"vivo":"{}","nicho":"{}","repertorio":"{}","cerebro":"{}","sinapsis":[{}]}}"#,
                m.d.x, m.d.y, m.d.z, m.d.roll, m.d.pitch, m.d.yaw,
                m.d.vx, m.d.vy, m.d.vz, m.obj.0, m.obj.1, m.obj.2,
                m.d.caido, m.piloto_ia, m.golpes, m.bateria, m.segundos,
                m.sensores_red,
                m.salidas_red[0], m.salidas_red[1], m.salidas_red[2], m.salidas_red[3],
                m.l1_red, m.l2_red, m.mem_red,
                m.d.extremo(),
                // Cuánto de la ventana de recuperación lleva gastada, 0 a 1.
                m.d.pasos_extremo as f32 / crate::sim3d::PASOS_RECUPERACION as f32,
                m.obstaculos.iter()
                    .map(|o| format!(r#"{{"x":{:.2},"y":{:.2},"r":{:.2},"h":{:.2}}}"#,
                                     o.x, o.y, o.radio, o.alto))
                    .collect::<Vec<_>>().join(","),
                m.mapa.confirmados()
                    .map(|o| format!(r#"{{"x":{:.2},"y":{:.2},"r":{:.2}}}"#, o.x, o.y, o.r))
                    .collect::<Vec<_>>().join(","),
                m.partida.0, m.partida.1, m.partida.2,
                m.d.apoyado,
                // Los tramos de la misión, para que el visor dibuje la cadena
                // entera y no sólo el punto que persigue ahora.
                m.mision.as_ref().map_or(String::new(), |mi| mi.puntos.iter()
                    .map(|p| format!(r#"{{"x":{:.2},"y":{:.2},"z":{:.2},"r":{:.2},"posar":{}}}"#,
                                     p.x, p.y, p.z, p.radio, p.posar))
                    .collect::<Vec<_>>().join(",")),
                m.seguidor.indice,
                m.mision_estado.replace('"', ""),
                m.modo_moto,
                m.moto.x, m.moto.y, m.moto.z, m.moto.rumbo, m.moto.rapidez, m.moto.giro,
                m.error_puesto,
                if m.pasos_persiguiendo > 0 {
                    m.pasos_a_tiro as f32 / m.pasos_persiguiendo as f32
                } else { 0.0 },
                m.grabando,
                m.grabacion.len(),
                m.aviso.replace('"', ""),
                m.entrenamiento,
                m.manual_dron,
                m.regimen,
                m.ojo_manda,
                m.ojo.iter().map(|v| format!("{v:.3}")).collect::<Vec<_>>().join(","),
                m.pasos.saturating_sub(m.ojo_visto) < 100,
                m.compatible,
                m.vivo.as_ref().map(|v| v.resumen()).unwrap_or_default(),
                m.vivo.as_ref().map(|v| v.contexto().nombre()).unwrap_or(""),
                m.vivo.as_ref().map(|v| v.repertorio.resumen()).unwrap_or_default(),
                m.cerebro,
                // Cuántos bits de sinapsis cambió cada neurona respecto del
                // cerebro que salió del gym. Es lo que deja MIRAR el
                // aprendizaje mientras pasa (regla 3.3) — y cuenta bits de
                // verdad, no una animación: ver `repertorio::sinapsis_cambiadas`.
                m.vivo.as_ref().map(|v| v.sinapsis_cambiadas().iter()
                    .map(|n| n.to_string()).collect::<Vec<_>>().join(","))
                    .unwrap_or_default()
            );
            responder(s, "application/json", json.as_bytes());
        } else if ruta.starts_with("/entrenamientos") {
            // Los entrenamientos que corren en OTROS procesos, leídos de su
            // propio log (ver `progreso.rs`: por qué del log y no de un
            // archivo de estado). Va aparte de `/estado` a propósito — el
            // visor lo pide cada pocos segundos, no a 40 Hz como la física.
            responder(s, "application/json", crate::progreso::json().as_bytes());
        } else if let Some(q) = ruta.strip_prefix("/tarea?t=") {
            // **Una tarea dicha, no compilada** (`mision::tarea_desde_texto`):
            // «despegar 3; ir 6 0; posar; agarrar 0.3; despegar 3; ir 0 0;
            // posar; soltar». Arranca en el punto A, con los postes puestos.
            let texto = decodificar_url(q);
            let mut m = mundo.lock().unwrap();
            let (ax, ay, _) = m.partida;
            let mapa = m.mapa.clone();
            match crate::mision::mision_de_tarea_con_mapa(&texto, (ax, ay), m.obstaculos.clone(), Some(&mapa)) {
                Ok(mut mi) => {
                    mi.pasos = 0; // el visor no tiene reloj
                    m.mision = Some(mi);
                    m.seguidor = crate::mision::Seguidor::nuevo(true);
                    m.d = Dron3::en_suelo(ax, ay);
                    m.segundos = 0.0;
                    m.mision_estado = "en curso".to_string();
                    m.reiniciar_control = true;
                    reiniciar_red_visor();
                    responder(s, "text/plain", b"ok");
                }
                Err(e) => responder(s, "text/plain", format!("error: {e}").as_bytes()),
            }
        } else if let Some(q) = ruta.strip_prefix("/punto?") {
            // Punto A (partida) y punto B (objetivo) puestos a dedo desde el
            // visor. Es la forma de que el usuario pruebe una ruta concreta en
            // vez de creerle al informe de capacidades.
            let (cual, x, y, z) = parametros_punto(q);
            let mut m = mundo.lock().unwrap();
            if cual == "b" {
                m.objetivo_libre = (x, y, z.max(0.5));
            } else {
                m.partida = (x, y, z.max(0.5));
                m.d = Dron3::nuevo(x, y, z.max(0.5)).con_aterrizaje();
                m.reiniciar_control = true;
                reiniciar_red_visor(); // ver armar_entrega
            }
            responder(s, "text/plain", b"ok");
        } else if let Some(q) = ruta.strip_prefix("/poste?") {
            // Poste agregado EN VIVO, mientras el dron vuela.
            let (_, x, y, _) = parametros_punto(q);
            let radio = valor(q, "r").unwrap_or(0.6).clamp(0.1, 3.0);
            let alto = valor(q, "h").unwrap_or(6.0).clamp(0.5, 20.0);
            let mut m = mundo.lock().unwrap();
            if m.obstaculos.len() < 40 {
                m.obstaculos.push(Obstaculo { x, y, radio, alto });
            }
            responder(s, "text/plain", b"ok");
        } else if let Some(q) = ruta.strip_prefix("/moto?") {
            // Los controles de la moto, tal como los aprieta el usuario. El
            // navegador manda esto a ~30 Hz; la física corre a 200 y usa el
            // último valor recibido, que es exactamente cómo se comporta un
            // mando de verdad (la mano no se mueve a 200 Hz).
            let mut m = mundo.lock().unwrap();
            m.mando = Mando {
                acelerador: valor(q, "acel").unwrap_or(0.0).clamp(-1.0, 1.0),
                giro: valor(q, "giro").unwrap_or(0.0).clamp(-1.0, 1.0),
            };
            responder(s, "text/plain", b"ok");
        } else if let Some(q) = ruta.strip_prefix("/ojo?") {
            // Lo que ve una cámara FÍSICA, mandado por `tools/ojo.py`.
            // `h` = rumbo a la amenaza (rad), `p` = proximidad (0..1).
            // `on=0/1` prende o apaga que esto reemplace a la visión simulada.
            let mut m = mundo.lock().unwrap();
            for k in 0..crate::sim3d::SECTORES {
                if let Some(v) = valor(q, &format!("s{k}")) {
                    m.ojo[k] = v.clamp(0.0, 1.0);
                }
            }
            if let Some(v) = valor(q, "on") {
                m.ojo_manda = v > 0.5;
            }
            m.ojo_visto = m.pasos;
            responder(s, "text/plain", b"ok");
        } else if let Some(q) = ruta.strip_prefix("/dron?") {
            // Las palancas del piloto manual del DRON, mismo patrón que
            // `/moto?`: el navegador manda a ~30 Hz, la física a 200 usa el
            // último valor. Sólo tiene efecto si `manual_dron` está prendido
            // (se prende con `/cabina?manual=1`) — si no, se guarda pero no
            // se usa, así no hace falta ordenar los dos POST en el cliente.
            let mut m = mundo.lock().unwrap();
            m.mando_dron = MandoDron {
                roll: valor(q, "roll").unwrap_or(0.0).clamp(-1.0, 1.0),
                pitch: valor(q, "pitch").unwrap_or(0.0).clamp(-1.0, 1.0),
                yaw: valor(q, "yaw").unwrap_or(0.0).clamp(-1.0, 1.0),
                alt: valor(q, "alt").unwrap_or(0.0).clamp(-1.0, 1.0),
            };
            responder(s, "text/plain", b"ok");
        } else if let Some(q) = ruta.strip_prefix("/cabina?") {
            let mut m = mundo.lock().unwrap();
            let cuerpo = if let Some(v) = valor(q, "manual") {
                // Entrar o salir del piloto manual del DRON. Al entrar, se
                // apaga la IA como base (si no, `manual_dron` manda pero al
                // salir el dron seguiría bajo lo que haya quedado puesto) y
                // se sueltan las palancas para no arrancar con un valor
                // viejo pegado.
                m.manual_dron = v > 0.5;
                m.mando_dron = MandoDron::default();
                // Sin esto, `alt_sp` entraba con el setpoint heredado del
                // piloto anterior — ver el comentario grande donde se
                // consume `reiniciar_control`, es la causa real de "no se
                // queda quieto".
                m.reiniciar_control = true;
                if m.manual_dron {
                    m.aviso = "piloto manual (vos volás el dron)".into();
                }
                "ok".to_string()
            } else if let Some(v) = valor(q, "modo") {
                // Entrar o salir del modo persecución. Al entrar, el dron se
                // pone en su puesto y los contadores arrancan de cero.
                let prender = v > 0.5;
                if prender && !m.modo_moto {
                    m.moto = Movil::en_vivo(-40.0, 0.0, 0.0);
                    let g = crate::persecucion::puesto(&m.moto);
                    let mut d = Dron3::nuevo(g.x, g.y, g.z).con_aterrizaje();
                    d.yaw = m.moto.rumbo;
                    m.d = d;
                    m.mision = None;
                    m.pasos_a_tiro = 0;
                    m.pasos_persiguiendo = 0;
                    m.reiniciar_control = true;
                    crate::iafz::reset_memoria(VISOR_PERSEC); // ver armar_entrega
                    // Arranca en CLÁSICO, no en la red. El genoma de
                    // persecución hoy sólo pasó el peldaño de línea recta a
                    // baja velocidad — en curvas y a fondo es territorio que
                    // nunca vio, y corregía con lo que en la práctica era
                    // ruido (ver el comentario de `VISOR_PERSEC` más arriba).
                    // El botón "piloto" lo prueba cuando quiera.
                    m.piloto_ia = false;
                    m.aviso = "modo persecución (piloto clásico)".into();
                }
                m.modo_moto = prender;
                "ok".to_string()
            } else if let Some(v) = valor(q, "grabar") {
                m.grabando = v > 0.5;
                if m.grabando {
                    m.grabacion.clear();
                    m.aviso = "GRABANDO".into();
                } else {
                    m.aviso = format!("{} muestras sin guardar", m.grabacion.len());
                }
                "ok".to_string()
            } else if q.starts_with("guardar") {
                // Guardar lo manejado como caso de entrenamiento. El nombre
                // lleva la fecha para que dos sesiones no se pisen.
                if m.grabacion.is_empty() {
                    m.aviso = "no hay nada grabado".into();
                } else {
                    let n = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let ruta = format!("recorridos/manejado_{n}.mandos");
                    let muestras = m.grabacion.len();
                    match crate::movil::grabacion::guardar(&m.grabacion, &ruta) {
                        Ok(()) => {
                            m.aviso = format!(
                                "guardado: {ruta} ({muestras} muestras, {:.0} s)",
                                muestras as f32 * crate::movil::PASOS_POR_MUESTRA as f32 * DT);
                            m.grabando = false;
                            m.grabacion.clear();
                        }
                        Err(e) => m.aviso = format!("no se pudo guardar: {e}"),
                    }
                }
                "ok".to_string()
            } else {
                "?".to_string()
            };
            responder(s, "text/plain", cuerpo.as_bytes());
        } else if ruta == "/cabina/manifest.json" {
            // Lo que convierte la página en una app instalable: sin esto,
            // "agregar a pantalla de inicio" abre un atajo cualquiera, no una
            // app en pantalla completa. `display: fullscreen` es lo que pide
            // el usuario explícitamente — sin barra de URL, sin pestañas.
            const MANIFIESTO: &str = r##"{
  "name": "Perseguime — cabina de persecución",
  "short_name": "Perseguime",
  "description": "Manejá la moto y mirá cómo te sigue el dron",
  "start_url": "/cabina",
  "scope": "/cabina",
  "display": "fullscreen",
  "orientation": "landscape",
  "background_color": "#05070a",
  "theme_color": "#0b0f14",
  "icons": [
    { "src": "/cabina/icon.svg", "sizes": "any", "type": "image/svg+xml", "purpose": "any maskable" }
  ]
}"##;
            responder(s, "application/manifest+json", MANIFIESTO.as_bytes());
        } else if ruta == "/cabina/sw.js" {
            // Service worker MÍNIMO: no cachea nada (la física cambia todo el
            // tiempo, cachear sería mostrar datos viejos), pero su sola
            // presencia es uno de los requisitos que Chrome pide para el
            // prompt de instalación real ("Add to Home Screen" con ícono
            // propio en vez de un simple atajo de navegador).
            const SW: &str = "self.addEventListener('fetch',()=>{});";
            responder(s, "text/javascript", SW.as_bytes());
        } else if ruta == "/cabina/icon.svg" {
            const ICONO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
<rect width="512" height="512" rx="96" fill="#0b1219"/>
<g fill="none" stroke="#6fd0ff" stroke-width="22" stroke-linecap="round">
<path d="M256 120v-40M256 120l-70 70M256 120l70 70"/>
<circle cx="256" cy="256" r="46" fill="#2e3b4a" stroke="#1c2733" stroke-width="10"/>
</g>
<g fill="#6fd0ff" opacity=".85">
<circle cx="150" cy="150" r="34"/><circle cx="362" cy="150" r="34"/>
<circle cx="150" cy="362" r="34"/><circle cx="362" cy="362" r="34"/>
</g>
<path d="M150 150L256 256M362 150L256 256M150 362L256 256M362 362L256 256"
      stroke="#1c2733" stroke-width="16"/>
<circle cx="256" cy="256" r="14" fill="#7fff9f"/>
</svg>"##;
            responder(s, "image/svg+xml", ICONO.as_bytes());
        } else if ruta.starts_with("/cabina") {
            responder(s, "text/html; charset=utf-8", CABINA.as_bytes());
        } else if let Some(t) = ruta.strip_prefix("/control?k=") {
            let tecla = t.chars().next().unwrap_or(' ');
            let mut m = mundo.lock().unwrap();
            match tecla {
                'w' => m.objetivo_libre.0 += 1.0,
                's' => m.objetivo_libre.0 -= 1.0,
                'a' => m.objetivo_libre.1 += 1.0,
                'd' => m.objetivo_libre.1 -= 1.0,
                'q' => m.objetivo_libre.2 += 0.5,
                'e' => m.objetivo_libre.2 = (m.objetivo_libre.2 - 0.5).max(0.5),
                // **Adaptación en vuelo** (`vivo.rs`). Exige las dos cosas que
                // la hacen honesta: genoma compatible —adaptar alrededor de un
                // genoma que lee basura es refinar ruido— y algo que sembrar.
                // El ancla arranca siendo EXACTAMENTE el genoma cargado, así
                // que prenderla no cambia el vuelo hasta que una variante gane
                // una comparación.
                'v' => {
                    use crate::repertorio::Repertorio;
                    if let Some(v) = m.vivo.take() {
                        // Al apagar se guarda lo aprendido. Si se perdiera acá,
                        // "aprende en vivo" duraría lo que dura la batería.
                        let mejoradas = v.repertorio.celdas_mejoradas();
                        m.aviso = match v.repertorio.escribir(Repertorio::ARCHIVO) {
                            Ok(()) => format!(
                                "adaptación APAGADA · repertorio guardado ({mejoradas}/9 celdas con algo aprendido)"),
                            Err(e) => format!("adaptación APAGADA · NO pude guardar el repertorio: {e}"),
                        };
                    } else if m.compatible {
                        // Se retoma lo aprendido en vuelos anteriores si lo hay
                        // y habla el idioma de hoy; si no, se siembra del
                        // genoma actual, que nunca es peor que el genoma solo.
                        let (a, de_donde) = match Repertorio::leer(Repertorio::ARCHIVO) {
                            Some(r) => {
                                let n = r.celdas.iter().filter(|c| c.merito.is_some()).count();
                                (crate::vivo::Adaptacion::con_repertorio(VISOR, r),
                                 format!("repertorio de vuelos anteriores ({n}/9 celdas medidas)"))
                            }
                            None => (crate::vivo::Adaptacion::sembrar(VISOR),
                                     "sembrado del genoma actual".to_string()),
                        };
                        m.vivo = Some(a);
                        m.aviso = format!("adaptación en vuelo ENCENDIDA · {de_donde}");
                    } else {
                        m.aviso = "genoma incompatible: no hay nada sano que adaptar".into();
                    }
                }
                'g' => {
                    // Golpe: velocidad angular, que es lo que de verdad tumba.
                    m.d.p += 6.0;
                    m.d.vx += 3.0;
                    m.golpes += 1;
                }
                'p' => {
                    if m.compatible {
                        m.piloto_ia = !m.piloto_ia;
                    } else {
                        // No se deja volar ciego: el genoma cargado no habla
                        // el idioma de los sensores de hoy. Sin este freno,
                        // `P` pisaba en silencio el arranque seguro en
                        // clásico — que es exactamente lo que pasó.
                        m.aviso = "genoma incompatible: reentrená antes de usar la red".into();
                    }
                }
                // Arranca (o corta) la misión de entrega entre A y B.
                'm' => {
                    if m.mision.is_some() {
                        m.mision = None;
                        m.mision_estado = "cancelada".to_string();
                    } else {
                        m.armar_entrega();
                    }
                }
                'r' => {
                    let obj = m.objetivo_libre;
                    let ia = m.piloto_ia;
                    let obs = std::mem::take(&mut m.obstaculos);
                    let partida = m.partida;
                    let (manual_v, mando_dron_v) = (m.manual_dron, m.mando_dron);
                    // Mismo motivo que en la reaparición tras la caída: `R`
                    // es "volar de nuevo", no "olvidate de lo aprendido".
                    let vivo_v = m.vivo.take();
                    *m = Mundo::nuevo();
                    m.objetivo_libre = obj;
                    m.piloto_ia = ia;
                    m.obstaculos = obs;
                    m.partida = partida;
                    m.manual_dron = manual_v;
                    m.mando_dron = mando_dron_v;
                    m.vivo = vivo_v;
                    m.d = Dron3::nuevo(partida.0, partida.1, partida.2).con_aterrizaje();
                    reiniciar_red_visor(); // ver armar_entrega
                }
                'c' => {
                    m.obstaculos.clear();
                    m.mapa = crate::plan::Mapa::default();
                }
                _ => {}
            }
            responder(s, "text/plain", b"ok");
        } else {
            responder(s, "text/html; charset=utf-8", pagina().as_bytes());
        }
}

/// Lee un parámetro numérico de una query string a mano (no hay dependencias
/// en este proyecto y no vale la pena agregarlas para esto).
fn valor(q: &str, clave: &str) -> Option<f32> {
    q.split('&')
        .find_map(|par| par.strip_prefix(clave)?.strip_prefix('=')?.parse().ok())
}

/// (cual, x, y, z) de una query tipo `cual=b&x=3&y=-2&z=4`.
fn parametros_punto(q: &str) -> (String, f32, f32, f32) {
    let cual = q
        .split('&')
        .find_map(|par| par.strip_prefix("cual="))
        .unwrap_or("b")
        .to_string();
    (
        cual,
        valor(q, "x").unwrap_or(0.0).clamp(-40.0, 40.0),
        valor(q, "y").unwrap_or(0.0).clamp(-40.0, 40.0),
        valor(q, "z").unwrap_or(3.0).clamp(0.0, 30.0),
    )
}

const HTML: &str = include_str!("visor.html");
/// La cabina de persecución: se maneja la moto y se mira cómo la sigue el
/// dron. Página aparte del visor a propósito — es otra tarea, con otro
/// layout (apaisado, para el teléfono) y otros controles.
const CABINA: &str = include_str!("cabina.html");

/// La página del visor. Hasta la sesión del experimento final de la mosca
/// llevaba la topología del conectoma incrustada (`conectoma_json`) para
/// dibujarla en un panel aparte; ese panel y el subcircuito que mostraba se
/// sacaron los dos juntos (medido: ni HS/VS ni el Complejo Central superan
/// de forma confiable a no ponerles nada — ver CLAUDE.md). El HTML ya no
/// necesita sustitución, así que `pagina()` es sólo el string estático.
fn pagina() -> &'static str {
    HTML
}

/// `%20`, `+` y compañía → texto. Para `/tarea?t=...`.
fn decodificar_url(q: &str) -> String {
    let b = q.split('&').next().unwrap_or("").as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if b[i] == b'%' && i + 2 < b.len() {
            match std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(v) => { out.push(v); i += 3; }
                None => { out.push(b'%'); i += 1; }
            }
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
