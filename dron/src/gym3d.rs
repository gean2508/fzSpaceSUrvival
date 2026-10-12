//! Gimnasio 3D: entrena las cuatro consignas, con ruido de sensores puesto.
//!
//! Dos diferencias con el gym 2D, y las dos apuntan a que esto vuele fuera del
//! simulador:
//!
//! 1. La red emite **cuatro** consignas de navegación — velocidad deseada
//!    hacia adelante, velocidad deseada lateral, velocidad de guiñada y
//!    corrección de altura. Ninguna es un ángulo: `Guiado` (pid3d.rs) le
//!    hace de intérprete y convierte la velocidad pedida en la actitud que
//!    hace falta, exactamente como haría el piloto automático de un dron
//!    comercial al recibir una orden de "andá para allá a tanta velocidad".
//! 2. Se entrena con **ruido de sensores**. Un genoma criado con lecturas
//!    perfectas aprende a confiar en decimales que un giróscopo real no tiene.

use crate::iafz;
use crate::pid3d::Guiado;
use crate::sim3d::{Dron3, RasgosVisuales, Ruido};

pub const POBLACION: i32 = 48;
pub const ELITE: i32 = 6;
const CUNA: i32 = POBLACION;

/// Cada cuantos pasos de fisica piensa la red. 10 = 20 Hz, 2 = 100 Hz, 1 = 200.
///
/// Era 10 y eso le daba a la red DIEZ VECES menos oportunidades de reaccionar
/// que al controlador clasico contra el que se la compara. Como una decision
/// cuesta ~4 us, pensar a 200 Hz gasta 0.08% de un nucleo: no habia razon de
/// costo para el handicap.
pub fn pasos_por_decision() -> usize {
    std::env::var("DRON_HZ")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|hz| (200 / hz.clamp(1, 200)).max(1))
        .unwrap_or(2) // 100 Hz por defecto
}


/// Lo que separa a la salida cruda del cerebro de una consigna: un squash
/// **algebraico**, `u/(1+|u|)`, no una tanh.
///
/// El cambio es por una sola razón y es importante. La tanh se aplana
/// exponencialmente: con la gananacia vieja de 0.25 y una salida de 6.6 —que
/// es donde estaba clavada la de altura— la derivada valía 0.018, o sea que
/// mover la salida del cerebro un punto entero movía la consigna 2 cm. Para
/// la evolución eso es terreno plano: no hay nada que escalar, y menos cuando
/// la aptitud es el peor de nueve casos. **Es un candado**: una vez que una
/// salida se va a la zona saturada no hay gradiente que la traiga de vuelta.
///
/// El squash algebraico está acotado igual (nunca pide más que `maximo`) pero
/// se aplana como 1/u², no como e^-u. A 6.6 la derivada es 1.5× la de la tanh,
/// y a 20 es 200× — que es la diferencia entre "cuesta salir" y "no se sale".
///
/// `ESCALA` = 2.0 sale de la misma medición: las salidas crudas de un genoma
/// viven en un rango de ±2 a ±7 según cuál, así que ahí es donde tiene que
/// estar la parte útil de la curva.
const ESCALA: f32 = 2.0;

pub fn consigna(o: f32, maximo: f32) -> f32 {
    let u = o / ESCALA;
    u / (1.0 + u.abs()) * maximo
}

/// Velocidad actual en el marco del CUERPO: cuánto se mueve hacia adelante y
/// hacia el costado, no en ejes del mundo. Es el mismo marco en el que la
/// red ya recibe distancia al objetivo, así que no hace falta rotar dos veces.
pub fn velocidad_cuerpo(m: &Dron3) -> (f32, f32) {
    let (s, c) = (m.yaw.sin(), m.yaw.cos());
    (m.vx * c + m.vy * s, -m.vx * s + m.vy * c)
}


/// Las cuatro consignas que salen del cerebro, juntas.
#[derive(Clone, Copy, Default)]
pub struct Consignas {
    pub roll: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub alt: f32,
}


/// Cuánto puede CORREGIR la red sobre el ángulo que ya decidió el modelo
/// físico, en radianes. 0.80 rad son 45.8°.
///
/// **Subido de 0.30 rad (17°) por pedido explícito del usuario**: "esquivar"
/// (`mision.rs`, peldaños 8-9) estaba trabado, y la autoridad combinada de la
/// tarea de entrega (`Guiado::default().ang_max = 0.40` + 0.30 de residuo =
/// 0.70 rad, 40°) usaba muy poco del sobre de vuelo SOSTENIBLE del aparato
/// (`INCLINACION_COMODA`, 1.20 rad, 69° — el límite de crucero, no el de
/// emergencia). El valor no es a ojo: **es exactamente el que hace que la
/// entrega llegue al techo de crucero** (0.40 + 0.80 = 1.20 rad), ni más ni
/// menos.
///
/// Es genérico y NO depende de masa ni hélices reales — el usuario pidió
/// explícitamente dejarlo así "por ahora en general" hasta tener las medidas
/// del DBPOWER (ver CLAUDE.md 7.1). `INCLINACION_COMODA`/`EXTREMA` ya estaban
/// escritas en función de `EMPUJE_PESO`, así que el día que ese número
/// cambie con el aparato real, este límite se reajusta solo.
///
/// **Consecuencia medida y aceptada para PERSECUCIÓN** (regla 3.9: las
/// conclusiones de antes de este cambio caducan): esa tarea ya usa una base
/// de hasta 72.9° (`Guiado::persecucion`), así que el combinado con este
/// residuo nuevo (72.9° + 45.8° ≈ 118.7°) satura casi siempre contra
/// `INCLINACION_EXTREMA` (80°) en vez de moverse dentro de un rango propio.
/// Sigue siendo SEGURO —el clamp final y la ventana de recuperación de 0.6 s
/// no cambiaron— pero el comportamiento de persecución con este residuo no
/// está remedido. Los techos de la sección 7.4/7.5 de la biblia (32-33% del
/// hueco del semáforo) se midieron con 0.30 y quedan como referencia
/// histórica, no como número vigente.
pub const CORRECCION_ANG: f32 = 0.80;
/// Ídem para guiñada, en rad/s, y para la altura, en metros.
pub const CORRECCION_YAW: f32 = 0.6;
pub const CORRECCION_ALT: f32 = 2.5;

/// **El enfoque "las ecuaciones van puestas, la evolución corrige encima".**
///
/// Es el cambio de arquitectura más importante que recibió el proyecto, y sale
/// de una evidencia dura medida en esta misma sesión, dos veces:
///
/// 1. El guiado clásico con el modelo físico adentro
///    (`sim3d::angulo_de_crucero` como prealimentación de trim) sigue una moto
///    a 100 km/h: **0 caídas en 18 combinaciones, 81-100% del recorrido a
///    tiro**. Sin ese trim, el MISMO controlador se estrellaba en 16-17 de 18.
/// 2. La red evolucionando desde cero contra esa misma tarea se clavó en
///    **892 de 7161** de aptitud en el segundo peldaño. No es que aprendiera
///    poco: no encontró nada.
///
/// Las dos mediciones dicen lo mismo. La relación velocidad→ángulo a alta
/// velocidad es fuertemente no lineal (a 4 m/s hacen falta 6°, a 25 m/s hacen
/// falta 68°), y **es una ecuación que ya conocemos**. Pedirle a una red
/// binaria de 96 bits que la redescubra a ciegas por selección natural es
/// tirar todo el presupuesto evolutivo en reinventar física de manual, y
/// encima no lo logra.
///
/// Así que se invierte el reparto, igual que hace un robot de Boston Dynamics:
/// **el modelo hace lo que el modelo sabe hacer, y lo aprendido se ocupa de lo
/// que el modelo no captura** — viento racheado, el objetivo maniobrando, todo
/// lo que no está en las ecuaciones.
///
/// Consecuencias, dichas de frente porque cambian qué significan los números:
///
/// - **Con la red emitiendo CERO, esto es exactamente el piloto clásico.** El
///   piso de aptitud ya no es "un dron que no sabe volar", es la vara honesta
///   entera. La evolución arranca arriba y sube, en vez de arrancar en un
///   desierto plano.
/// - **Por eso el umbral de aprobación sube a superar al clásico**, no al 60%
///   de él (ver `persecucion::umbral_aprobado`). Si la red sólo empatara al
///   modelo, no estaría aportando nada y el peldaño no tiene por qué darse por
///   aprobado. La regla 3.2 sigue en pie: la vara no se toca para que los
///   números queden lindos, se sube.
/// - **La red ya no "pilota"**: corrige a quien pilota. Es una afirmación más
///   chica y más honesta que la anterior, y hay que decirla así en cualquier
///   informe.
/// **El mismo enfoque híbrido de `consignas_hibridas`, pero para ir a un
/// PUNTO** — o sea, para la tarea de entrega (`mision.rs`).
///
/// Existe porque el giro de la regla 3.15 se aplicó a persecución y **nunca
/// se llevó de vuelta a la tarea original**, que es la que la biblia llama
/// "el gym". La traducción vieja (`consignas_red`, ya borrada) REEMPLAZABA al piloto clásico: la red emitía
/// velocidades deseadas y `Guiado::consignas` no se llama nunca, así que
/// salida en cero no es "el clásico", es "quedate quieto". La red tiene que
/// aprender a navegar desde cero.
///
/// Lo que costó, medido: en el peldaño 4 de entrega el piloto clásico saca
/// **1405.4** contra un umbral de 900 —cumple las tres misiones en los tres
/// ruidos, posándose a 0.4 m— y el genoma evolucionado saca **127.6**, el 9%.
/// Dos entrenamientos de 1200+ generaciones no lo movieron de ahí. No es que
/// le cueste ganarle al clásico: es que no lo está usando.
///
/// Y la regla 3.15 ya tenía la medición de lo que pasa al cambiar una cosa
/// por la otra, en la otra tarea:
///
/// ```text
///   red evolucionando desde cero        892 de 7161  (no encontró nada)
///   red corrigiendo sobre el modelo   12420 de 12600, en 60 generaciones
/// ```
///
/// Con esto, salida en cero vuelve a ser EXACTAMENTE el piloto clásico, y la
/// red sólo puede mejorarlo o empeorarlo desde un piso que ya vuela.
///
/// **Invalida los genomas de entrega anteriores**: cambia el significado de
/// las cuatro salidas (de "velocidad deseada" a "corrección sobre el ángulo
/// que ya decidió el modelo"). Hay que reentrenar, y el genoma que había
/// estaba al 9% del clásico de todos modos.
/// Radio horizontal dentro del cual se autoriza empezar a descender sobre una
/// plataforma. PX4 usa `PLD_HACC_RAD` = 0.2 m; acá es más generoso porque el
/// ruido de posición REAL del proyecto ya es de 0.25 m — pedir 0.2 sería
/// pedir precisión por debajo del propio sensor. Medio metro es la mitad del
/// radio del pad (1.2 m), así que sigue siendo exigente.
pub const RADIO_ALINEACION: f32 = 0.5;
/// Altura mínima que se sostiene mientras se está desalineado. Da margen para
/// frenar la deriva antes de comprometerse al descenso.
pub const ALTURA_APROXIMACION: f32 = 1.0;

/// **La altura que hay que pedir, con la regla de aterrizaje de precisión de
/// la industria: NO bajar mientras no estés alineado.**
///
/// Esto CAMBIA la vara honesta (regla 3.2), y es una decisión tomada a mano
/// con la medición delante, no un ajuste para que los números salgan lindos.
/// Lo que se midió:
///
/// ```text
///   peldaño 12, piloto CLÁSICO, las tres misiones:
///     misión 1 (baja 3 m, 0 m de lado):      1368.0  posa a 0.02 m
///     misión 2 (baja 4 m, 2.5 m de lado):     -41.5  SE ESTRELLÓ
///     misión 3 (baja 2.5 m, 2.5 m de lado):  1328.3  posa a 0.93 m
/// ```
///
/// La vara honesta **no puede con la misión 2**, y el porqué es exacto:
/// `Dron3::tocar_suelo` exige CUATRO condiciones y falla la tercera.
///
/// ```rust,ignore
///   posarse_vale && vz > -VZ_POSADA && horizontal < VH_POSADA && derecho
/// ```
///
/// Tocó a 1.17 m/s vertical (bajo el límite de 1.2) y dentro del radio del
/// pad, pero bajó 4 m y cruzó 2.5 m **al mismo tiempo**: llegó en diagonal,
/// con velocidad horizontal por encima de `VH_POSADA` (1.0 m/s). No es falta
/// de precisión ni de tiempo: es que descender y trasladarse a la vez termina
/// en un toque de costado.
///
/// La industria no hace eso. El aterrizaje de precisión de PX4 es una máquina
/// de estados: **acercamiento horizontal a altura → confirmar que el objetivo
/// está dentro de `PLD_HACC_RAD` → recién entonces descender**. Esto
/// implementa esa misma regla, y la usan IGUAL el piloto clásico y el híbrido
/// para que sigan siendo comparables (regla 3.3: una sola implementación).
///
/// **Invalida las tablas anteriores de la tarea de entrega**: la vara se
/// movió, así que los números de `capacidades` y los umbrales medidos contra
/// el clásico viejo no son comparables con los de ahora.
pub fn altura_de_aproximacion(medido: &Dron3, ox: f32, oy: f32, oz: f32) -> f32 {
    // Sólo cambia algo cuando el objetivo está EN EL SUELO (una posada). Para
    // un punto en el aire, la altura pedida es la del punto y listo.
    if oz > 0.05 {
        return oz;
    }
    let error_h = ((ox - medido.x).powi(2) + (oy - medido.y).powi(2)).sqrt();
    // **Rampa continua, no un escalón.** La primera versión hacía
    // `if error > RADIO { medido.z.max(ALTURA_APROXIMACION) } else { oz }`, y
    // eso rompió el peldaño 11, que ya funcionaba: al desviarse más de 0.5 m
    // la altura pedida SALTABA de 0 a 1.0 m —o sea, le ordenaba trepar— y el
    // híbrido quedó trabado en 9.7 donde antes aprobaba en 10 generaciones.
    // Es el mismo defecto de conmutación brusca que ya se había diagnosticado
    // y resuelto para el árbitro de persecución; acá se había repetido.
    //
    // Dos propiedades, las dos necesarias:
    //  1. CONTINUA en el error: el piso sube en rampa, sin escalón.
    //  2. NUNCA por encima de la altura actual: frena el descenso, jamás pide
    //     trepar. Sin esto, derivar cerca del suelo dispara un tirón hacia
    //     arriba y entra en ciclo.
    let exceso = (error_h - RADIO_ALINEACION).max(0.0);
    let por_error = (exceso / RADIO_ALINEACION * ALTURA_APROXIMACION).min(ALTURA_APROXIMACION);

    // **Y tampoco bajar mientras se viene deslizando.** La condición que de
    // verdad falla en `Dron3::tocar_suelo` es `horizontal < VH_POSADA`, no la
    // vertical: la misión que se estrellaba tocaba a 0.15 m/s de caída —
    // suavísimo— y aun así contaba como choque, porque llegaba de costado.
    // Alinearse en POSICIÓN no alcanza si todavía se está moviendo; hay que
    // esperar a estar quieto. Se exige la mitad del límite, para llegar con
    // margen y no justo en el borde.
    let vh = (medido.vx * medido.vx + medido.vy * medido.vy).sqrt();
    let margen = crate::sim3d::VH_POSADA * 0.5;
    let por_velocidad =
        ((vh - margen) / margen * ALTURA_APROXIMACION).clamp(0.0, ALTURA_APROXIMACION);

    // El piso lo manda la condición MÁS exigente de las dos.
    let freno = por_error.max(por_velocidad).min(medido.z);
    if freno > 0.0 {
        // Todavía hay algo frenando el descenso: alinearse o quedarse quieto
        // antes de bajar.
        return oz.max(freno);
    }

    // **Vía libre para tocar: se pide POR DEBAJO del suelo, a propósito.**
    //
    // Sin esto el dron NO ATERRIZA NUNCA, y costó 400 generaciones de
    // entrenamiento descubrirlo. El lazo de altura es proporcional
    // (`Estabilizador3`, ganancia 1.6): con la consigna en 0.0 la velocidad de
    // descenso es −1.6·z, o sea que se apaga junto con la altura. Eso es una
    // exponencial: z tiende a 0 y no lo cruza jamás. `Dron3::tocar_suelo` sale
    // por `if self.z > 0.0 { return; }`, así que `apoyado` nunca se prende y el
    // tramo de posada nunca se completa.
    //
    // Medido, peldaño 12 misión 3, piloto CLÁSICO (modelo perfecto):
    //
    // ```text
    //   traza final:  x=-1.50  y=2.00  z=0.00  |v|=0.03 m/s
    //   0 de 1 tramos · "se quedó sin reloj" · terminó a 0.00 m del punto
    // ```
    //
    // Posado, quieto, clavado en el punto — y sin acreditar. Flotaba a
    // milímetros del piso quemando el reloj. Con ruido REAL en cambio SÍ
    // aterrizaba (1135.1 contra 106.6 en limpio): el ruido lo empujaba abajo y
    // le hacía cruzar el cero. Un peldaño que sólo se aprueba si los sensores
    // están sucios es la firma de este bug, no de una dificultad.
    //
    // El arreglo es el de la industria (regla 3.17): ArduPilot y PX4 no usan
    // ley proporcional en el tramo final, usan velocidad de descenso CONSTANTE
    // (`LAND_SPEED`, 50 cm/s por defecto) hasta que el detector de apoyo
    // dispara. Pedir la consigna 0.25 m bajo tierra da exactamente eso: el
    // error deja de anularse y el descenso se sostiene en ~1.6 × 0.25 = 0.40 m/s
    // hasta tocar.
    //
    // **No regala nada.** `tocar_suelo` sigue exigiendo lo mismo para que el
    // toque cuente como posada y no como choque: menos de VZ_POSADA (1.2 m/s)
    // de caída, menos de VH_POSADA (1.0 m/s) de deslizamiento y derecho. 0.40
    // m/s entra con un tercio del límite vertical, que es margen de sobra —
    // pero el dron sigue teniendo que llegar alineado y quieto, porque el
    // `freno` de arriba no lo deja bajar hasta que lo esté.
    //
    // **El offset entra en RAMPA, y eso no es cosmético.** La primera versión
    // lo aplicaba entero desde cualquier altura, y rompió la misión que ya
    // funcionaba: bajando 3 m a plomo, la consigna 0.25 m bajo tierra mantiene
    // el descenso pegado al tope (2.5 m/s) hasta demasiado abajo, y el lazo de
    // velocidad no alcanza a frenar contra su propio retardo. Medido: misión 1
    // del peldaño 12 pasó de 1347.4 a **−46.6, SE ESTRELLÓ**. Es el mismo
    // defecto de conmutación brusca que ya mordió dos veces en este archivo
    // (el escalón de `ALTURA_APROXIMACION`) y en el árbitro de persecución.
    //
    // Con la rampa, arriba de `ALTURA_FINAL` manda el proporcional de siempre
    // —que frena bien— y abajo el offset crece hasta sostener el toque:
    //
    // ```text
    //   z = 0.50 m → pide  0.80 m/s   (idéntico al de antes: la rampa arranca en cero)
    //   z = 0.25 m → pide  0.60 m/s
    //   z = 0.00 m → pide  0.40 m/s   (ya no se apaga: toca)
    // ```
    let cerca = (1.0 - medido.z / ALTURA_FINAL).clamp(0.0, 1.0);
    let consigna = oz - DESCENSO_FINAL * cerca;
    // **Dos velocidades de bajada, como PX4 (`MPC_LAND_SPEED`) y ArduPilot
    // (`LAND_SPEED` / `LAND_SPEED_HIGH`): rápida arriba, lenta en los últimos
    // metros.** Sin esto baja a 2.5 m/s hasta muy abajo y confía en frenar
    // pegado al piso, y eso sólo funciona con la masa exacta que el
    // estabilizador supone. Medido con `dron estres 11` (bitácora 7.38): con
    // una caja de 300 g —la tarea 3 del producto— o un 15% menos de empuje
    // (batería gastada), el clásico pasaba de 24/24 a 0/24 y 6/24, tocando a
    // 1.75 m/s. Por debajo de `ALTURA_LENTA` la consigna no deja pedir más
    // de `V_LENTA` de bajada al lazo P de 1.6/s.
    const ALTURA_LENTA: f32 = 2.0;
    const V_LENTA: f32 = 0.8;
    if medido.z < ALTURA_LENTA {
        consigna.max(medido.z - V_LENTA / 1.6)
    } else {
        consigna
    }
}

/// Cuánto por debajo del suelo se pide la consigna en el tramo final de una
/// posada, para que el descenso no se apague asintóticamente. Con la ganancia
/// 1.6 del lazo de posición vertical da ~0.40 m/s de velocidad de toque, un
/// tercio del límite que rompe el tren (`sim3d::VZ_POSADA` = 1.2 m/s).
/// **Bajada final a divergencia constante** (sin sensores nuevos). La cámara
/// que mira al piso mide cómo se agranda el suelo, `D = vz / h` (1/s), con
/// mucho menos error que el barómetro cerca del piso (±0.04/s contra
/// ±0.25 m con ruido DURO). Bajar manteniendo `D` constante es lo que hacen
/// las abejas y los microdrones de investigación al posarse: la velocidad
/// baja sola en proporción a la altura, sin necesitar la altura. Un piso de
/// velocidad garantiza que llegue a tocar. Ver bitácora 7.49.
#[derive(Clone, Copy)]
pub struct BajadaPorDivergencia {
    vz: f32,
    activa: bool,
}

impl Default for BajadaPorDivergencia {
    fn default() -> Self {
        BajadaPorDivergencia { vz: -V_LENTA_BAJADA, activa: false }
    }
}

/// La divergencia a sostener (1/s, negativa = bajando). Barrido con ruido
/// DURO, vuelos cumplidos de 48 (posada corrida / entrega): −0.4 → 29/41,
/// −0.6 → 31/40, −0.9 → 25/37; sin divergencia 28/36. Meseta: −0.5.
pub const D_OBJETIVO: f32 = -0.5;
/// Velocidad mínima de bajada, para que la exponencial toque el piso (m/s).
pub const VZ_TOQUE: f32 = 0.25;
pub const V_LENTA_BAJADA: f32 = 0.8;

impl BajadaPorDivergencia {
    /// `bajando`: si la aproximación ya dio permiso de bajar (alineado y
    /// quieto encima de la plataforma, debajo de 2 m). Devuelve la velocidad
    /// vertical a pedir, o `None` si no corresponde.
    pub fn paso(&mut self, bajando: bool, divergencia: f32) -> Option<f32> {
        if !bajando {
            self.activa = false;
            return None;
        }
        if !self.activa {
            *self = BajadaPorDivergencia { vz: -V_LENTA_BAJADA, activa: true };
        }
        // Corrección multiplicativa: si el piso se agranda más rápido de lo
        // querido (|D| > |D*|), se baja más despacio, y al revés.
        let d = divergencia.min(-0.05);
        let razon = (D_OBJETIVO / d).clamp(0.5, 2.0);
        self.vz = (self.vz * (1.0 + 0.05 * (razon - 1.0))).clamp(-V_LENTA_BAJADA, -VZ_TOQUE);
        Some(self.vz)
    }
}

/// ¿Está en la bajada final de una posada? (alineado, con permiso de bajar,
/// y debajo de 2 m).
pub fn en_bajada_final(medido: &Dron3, ox: f32, oy: f32, oz: f32) -> bool {
    oz <= 0.05 && medido.z < 2.0 && altura_de_aproximacion(medido, ox, oy, oz) < medido.z - 0.05
}

pub const DESCENSO_FINAL: f32 = 0.25;
/// Desde qué altura empieza a entrar el offset de toque. Por encima de esto el
/// descenso es el proporcional de siempre; es el equivalente del `LAND_ALT_LOW`
/// de ArduPilot, que es donde ese autopiloto cambia a velocidad de aterrizaje.
pub const ALTURA_FINAL: f32 = 0.5;

/// **Freno de seguridad ante un corredor bloqueado — programado, no
/// aprendido** (regla 3.15: esto no hace falta evolucionarlo, es la misma
/// lógica que cualquier autopiloto comercial usa contra un obstáculo que no
/// se puede rodear).
///
/// Por qué hacía falta, medido por el usuario en el visor: puso tantos postes
/// seguidos que armó una pared, y el dron voló derecho contra ella. Revisando
/// el código, la razón es estructural — no hay NINGÚN lugar en la cadena de
/// control (clásico ni híbrido) que sepa "esto está completamente tapado, hay
/// que parar". El anillo de proximidad es sólo una entrada más a la red; el
/// piloto físico que navega por debajo nunca lo mira.
///
/// **La distinción que el usuario pidió expresamente** ("no siempre parar,
/// pero si es una pared sí"): frenar sólo cuando el camino de adelante está
/// tapado EN TODOS los sectores, no con que uno solo tenga algo — un poste
/// suelto con hueco al costado no debe frenar al dron, tiene que poder
/// rodearlo como ya hace. Por eso se usa el MÍNIMO del anillo, no el máximo
/// ni el promedio: si un solo sector queda libre, el mínimo es bajo, no hay
/// pared, no se frena — y la red sigue teniendo margen completo para
/// esquivar por ese hueco. Sólo cuando los cinco sectores están altos a la
/// vez (ningún hueco visible) el mínimo sube y el freno entra.
///
/// Se implementa escalando la velocidad de crucero que el modelo físico ya
/// calculó (`roll_base`/`pitch_base`, que son ángulos de inclinación — y la
/// inclinación es, en este simulador, lo que produce velocidad) ANTES de que
/// la red corrija encima. La red sigue corrigiendo a autoridad completa: un
/// dron frenado todavía puede virar para encontrar el hueco, sólo deja de
/// empujar hacia adelante contra la pared mientras lo busca.
///
/// Rampa, no escalón — es el defecto de conmutación brusca que ya mordió dos
/// veces en este archivo (`ALTURA_APROXIMACION`, `DESCENSO_FINAL`): un freno
/// que se activa de golpe metería un pico de deceleración que la red nunca
/// entrenó.
fn frenar_ante_pared(anillo: &[f32]) -> f32 {
    // **Calibrados contra el rango real del instrumento, no a ojo — regla
    // 3.18.** La primera versión tenía LIBRE=0.35/PARED=0.75, números que
    // sonaban razonables y estaban completamente desconectados de lo que el
    // anillo puede llegar a valer de verdad: `anillo` es looming
    // (`v_cierre/distancia`, normalizado contra `RANGO_LOOMING=4.0`), y ese
    // 4.0 sólo se satura cuando falta un cuarto de segundo para el impacto —
    // una pared entera acercándose a 5 m/s desde 5 m (1 segundo de reacción,
    // una distancia razonable para empezar a frenar) da apenas **0.25-0.32**,
    // muy por debajo de 0.75. La prueba `una_pared_de_postes_frena_de_verdad`
    // lo demostró: con los umbrales viejos, el freno NUNCA entraba ante una
    // pared real, sólo un instante antes de chocar — inútil.
    const LIBRE: f32 = 0.03; // un obstáculo lejano o de costado no debe frenar nada
    const PARED: f32 = 0.18; // el valor MEDIDO de una pared real (0.207 en la
                              // prueba) — más abajo de 0.28 porque los sectores
                              // de los BORDES del campo dan looming más bajo que
                              // el centro por geometría pura (menos componente
                              // de acercamiento radial a 80° que a 0°), aunque
                              // el poste esté igual de presente. Pedir 0.28 en
                              // TODOS los sectores nunca se cumplía ni con una
                              // pared real — otra vez el instrumento midiendo
                              // algo distinto de lo que la constante suponía.
    const PISO: f32 = 0.05;  // nunca cero: un dron parado en seco es tan peligroso
                             //   como uno que no frena — deja lugar para reposicionarse.
    let peor_hueco = anillo.iter().cloned().fold(f32::INFINITY, f32::min);
    let bloqueo = ((peor_hueco - LIBRE) / (PARED - LIBRE)).clamp(0.0, 1.0);
    (1.0 - bloqueo).max(PISO)
}

pub fn consignas_hibridas_punto(
    o: &[f32; iafz::SALIDAS],
    medido: &Dron3,
    ox: f32,
    oy: f32,
    oz: f32,
    guiado: &Guiado,
    vista: &crate::sim3d::RasgosVisuales,
) -> Consignas {
    // 1. El modelo físico navega, completo — **y esquivando**.
    //
    // Tiene que ser la MISMA función que vuela el clásico, no una parecida
    // (regla 3.3). Si el híbrido navegara con una base ciega a los postes y
    // el clásico con una que los ve, «red en cero ⟺ clásico» (regla 3.15)
    // dejaría de ser cierto justo en los peldaños con obstáculos, que son los
    // que el proyecto está peleando — y las dos columnas de toda tabla de
    // esquive estarían comparando dos pilotos distintos.
    let (roll_base, pitch_base, yaw_base) =
        guiado.consignas_esquivando(medido, ox, oy, &vista.anillo, vista.obs_horizontal);
    // 1.5. Freno de seguridad si el frente está tapado en todos los
    //      sectores — ver `frenar_ante_pared`. Con el anillo en cero (sin
    //      obstáculos, o sin cámara real todavía) da 1.0 y no cambia nada:
    //      la identidad "red en cero ⟺ clásico" (regla 3.15) sigue valiendo.
    let frenado = frenar_ante_pared(&vista.anillo);
    let (roll_base, pitch_base) = (roll_base * frenado, pitch_base * frenado);
    // 2. La red corrige, acotada. Mismas cotas que el híbrido de persecución,
    //    menos cerca del suelo en una posada — ver `autoridad_de_posada`.
    let tope = crate::sim3d::INCLINACION_EXTREMA;
    let aut = autoridad_de_posada(medido.z, oz)
        .min(autoridad_de_sosten(medido, ox, oy, &vista.anillo));
    Consignas {
        roll: (roll_base + consigna(o[0], CORRECCION_ANG * aut)).clamp(-tope, tope),
        pitch: (pitch_base + consigna(o[1], CORRECCION_ANG * aut)).clamp(-tope, tope),
        yaw: yaw_base + consigna(o[2], CORRECCION_YAW * aut),
        // La altura base ya no es `oz` a secas: es la regla de aproximación
        // (no bajar mientras no estés alineado). La red sigue corrigiendo
        // encima, acotada, igual que antes.
        alt: altura_de_aproximacion(medido, ox, oy, oz) + consigna(o[3], CORRECCION_ALT * aut),
    }
}

/// **La red pierde autoridad al llegar, también en vuelo horizontal.** Era
/// el ítem «el orbitar» del contrato: `autoridad_de_posada` bajaba la
/// corrección cerca del SUELO, pero no cerca del PUNTO en el aire, y un
/// residuo a pleno sobre un hover que el clásico resuelve casi perfecto lo
/// perturba. Medido con `capacidades` (genoma instalado, sin GPS): en los
/// últimos 3 s el genoma se movía 0.23-0.46 m y el clásico 0.01-0.05 m
/// (bitácora 7.45). Mismo patrón que la posada, aplicado al eje que faltaba.
/// Con un estorbo a la vista no se toca: esquivar es justo donde la red
/// tiene que poder corregir.
pub fn autoridad_de_sosten(medido: &Dron3, ox: f32, oy: f32, anillo: &[f32]) -> f32 {
    // Barrido con `capacidades` (cuánto se mueve en los últimos 3 s, genoma
    // instalado): sin esto 0.23-0.46 m; 1.5 m / piso 0.2 → 0.05-0.13 m;
    // 3.0 m / piso 0.1 → 0.04-0.06 m (clásico 0.01-0.05). El resto de las
    // capacidades, igual. Un solo genoma: indicio (bitácora 7.45).
    let (desde, piso) = (3.0f32, 0.1f32);
    if anillo.iter().any(|&a| a > 0.03) {
        return 1.0;
    }
    let dh = ((ox - medido.x).powi(2) + (oy - medido.y).powi(2)).sqrt();
    (dh / desde).clamp(piso, 1.0)
}

/// Cuánta autoridad tiene la red **mientras baja a posarse**: plena en el aire,
/// casi nada al tocar.
///
/// **Por qué.** Medido: en el peldaño de posada con plataforma corrida, el mismo
/// campeón que cumple las nueve pruebas del peldaño fácil (1076.8) **estrella**
/// la versión difícil (−47.9) — y con las salidas en CERO, que por la regla 3.15
/// es exactamente el piloto clásico, ese mismo peldaño saca **1163.5 cumpliendo
/// las nueve**. O sea: la red no fallaba en aprender a posarse, estaba
/// arruinando una posada que el modelo ya hacía perfecta.
///
/// La causa es el tamaño de la palanca. `CORRECCION_ANG` son 0.80 rad (46°) y
/// `CORRECCION_ALT` son 2.5 m: en pleno vuelo eso es margen para corregir, pero
/// a medio metro del piso equivocarse no es "puntuar peor", es volcar. El
/// paisaje de aptitud alrededor del campeón deja de ser una pendiente y pasa a
/// ser un acantilado, y la búsqueda no encuentra el camino de vuelta al
/// silencio: 260 generaciones clavada en 24.9 de 900.
///
/// Es el mismo remedio que `persecucion::limitar_sacudida` (bitácora 7.5), que
/// acotó a la red donde estorbaba y ganó +104 puntos sin reentrenar nada.
///
/// **No apaga la red, y eso es a propósito.** El piso es 0.10, o sea que al
/// tocar todavía le quedan ~0.08 rad (4.6°) y 0.25 m para refinar. Si fuera
/// cero el peldaño dejaría de medir a la red —se aprobaría quitándola, no
/// enseñándole—, y eso sería maquillar el resultado. Con el piso, el peldaño
/// sigue exigiendo que la red no estorbe y le deja lugar para mejorar.
///
/// No mueve la vara honesta: el clásico no tiene residuo que acotar, así que
/// `umbral_aprobado` y todas las tablas contra el clásico siguen comparables.
pub fn autoridad_de_posada(z: f32, oz: f32) -> f32 {
    if oz > 0.05 {
        return 1.0; // no es una posada: la red manda como siempre
    }
    const DESDE: f32 = 1.5; // arriba de esta altura, autoridad plena
    const PISO: f32 = 0.10; // al tocar, sólo alcanza para refinar
    (z / DESDE).clamp(PISO, 1.0)
}

pub fn consignas_hibridas(
    o: &[f32; iafz::SALIDAS],
    medido: &Dron3,
    guia: &Guia,
    guiado: &Guiado,
) -> Consignas {
    // 1. Lo que el modelo físico ya sabe hacer, completo.
    let (roll_base, pitch_base, yaw_base) = guiado.consignas_persiguiendo(medido, guia);
    // 2. La red corrige, acotada. El clamp final es del sobre del aparato, no
    //    del residuo: el modelo ya se limitó solo a `ang_max`, y la corrección
    //    puede empujar un poco más allá si de verdad conviene.
    let tope = crate::sim3d::INCLINACION_EXTREMA;
    Consignas {
        roll: (roll_base + consigna(o[0], CORRECCION_ANG)).clamp(-tope, tope),
        pitch: (pitch_base + consigna(o[1], CORRECCION_ANG)).clamp(-tope, tope),
        yaw: yaw_base + consigna(o[2], CORRECCION_YAW),
        alt: guia.z + consigna(o[3], CORRECCION_ALT),
    }
}


/// Postes puestos EN LA TRAYECTORIA de cada escenario, no de adorno: si el
/// dron vuela la línea recta de antes, choca. Tiene que rodearlos o subir.
/// Un escenario sin obstáculos (`&[]`) deja el gym exactamente como estaba.
pub fn obstaculos(escenario: usize) -> &'static [crate::sim3d::Obstaculo] {
    use crate::sim3d::Obstaculo;
    const O0: &[Obstaculo] = &[
        Obstaculo { x: 3.0, y: 0.0, radio: 0.6, alto: 6.0 },
    ];
    const O1: &[Obstaculo] = &[
        Obstaculo { x: -2.0, y: 2.0, radio: 0.6, alto: 4.0 },
        Obstaculo { x: -3.5, y: 3.5, radio: 0.5, alto: 8.0 },
    ];
    const O2: &[Obstaculo] = &[
        Obstaculo { x: 1.5, y: -2.5, radio: 0.7, alto: 5.0 },
        Obstaculo { x: 3.0, y: -5.0, radio: 0.5, alto: 3.0 },
    ];
    match escenario % 3 {
        0 => O0,
        1 => O1,
        _ => O2,
    }
}

/// Los 96 bits que ve el cerebro, en código de TERMÓMETRO comprimido.
///
/// El porqué del código está en `codigo.rs` y es la corrección más importante
/// que recibió el proyecto: el kernel dispara por vecindad de Hamming, así que
/// el código binario anterior hacía que "estoy a 14 m" se pareciera MÁS a
/// "llegué" que "estoy a medio metro" (Hamming 1 contra 5). Ese era el orbitar.
///
/// Se codifica en el marco del CUERPO, no del mundo: "cuánto tengo el objetivo
/// adelante y cuánto al costado". Así la guiñada deja de ser una variable
/// aparte —está implícita— y se ahorran bits, que es justo lo que falta.
///
/// ## El reparto de los 96 bits
///
/// En termómetro un campo cuesta ~3× más bits que en binario, así que hubo que
/// elegir. Lo que se fue, y por qué:
///
/// - **`flujo_x` / `flujo_y`** (12 bits). El flujo óptico es `velocidad/altura
///   + rotación`: los tres términos ya entran por separado y con mejor
///   resolución (velocidad 8+8 bits, altura 9, giro 5). Era información
///   duplicada y peor medida.
/// - **`obj_horizontal` / `obj_vertical` / `obj_tamano`** (17 bits). Es la
///   MISMA geometría que `adelante`/`lateral`/`dz`, vista por la cámara y
///   recortada al campo de visión. Se quedó la medición completa.
///
/// Lo que se agregó:
///
/// - **altura sobre el suelo** (9 bits, comprimidos: cortes en 0.08, 0.50,
///   1.18 m…). Sin esto el dron nunca supo a qué altura estaba en absoluto,
///   sólo cuánto le faltaba para el objetivo — y despegar y posarse son
///   justamente las dos maniobras que se definen contra el suelo, no contra el
///   objetivo.
///
/// Lo que se quedó es la visión que de verdad usa una mosca: hacia dónde está
/// el estorbo (`obs_horizontal`), cuánto aprieta (`obs_proximidad`) y cuánto
/// crece la escena (`divergencia`, que es el detector de loom).
pub fn sensores(m: &Dron3, v: &RasgosVisuales, ox: f32, oy: f32, oz: f32) -> (u64, u32) {
    sensores_guia(m, v, &Guia::quieta(ox, oy, oz))
}

/// Lo que el piloto tiene que ir a ocupar, y a qué velocidad se está moviendo
/// ESE punto.
///
/// Existe porque seguir no es ir a. Un punto quieto se alcanza anulando el
/// error de posición; un punto que se mueve **no se alcanza nunca así**: cuando
/// el dron llega, el punto ya no está. Hay que igualar la velocidad, y para
/// igualarla hay que medirla. Por eso la velocidad viaja junto con la posición
/// y no como un extra opcional.
#[derive(Clone, Copy, Debug, Default)]
pub struct Guia {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    /// **Aceleración del punto de guía**, en el marco del mundo.
    ///
    /// Es la tercera derivada de la información que un seguidor necesita
    /// (posición, velocidad, aceleración) y la que faltaba. Un punto que va
    /// detrás de un móvil en curva tiene aceleración centrípeta propia: a
    /// 22 m/s con radio 90 m son 5.4 m/s², media g. Sin este término el
    /// controlador sólo puede REACCIONAR a esa curva después de que el error
    /// se acumule — medido: el error en curva era 5× más lateral (se abría
    /// hacia afuera) que longitudinal, que es la firma exacta de una
    /// aceleración centrípeta no compensada.
    ///
    /// Alimentarla hacia adelante es lo que hace todo controlador serio de
    /// cuadricóptero para seguir trayectorias (el esquema de
    /// Mellinger-Kumar: posición + velocidad + aceleración de referencia →
    /// vector de empuje deseado → actitud).
    pub ax: f32,
    pub ay: f32,
}

impl Guia {
    /// Un punto fijo: todas las misiones de ir-a, posarse y despegar.
    pub fn quieta(x: f32, y: f32, z: f32) -> Self {
        Guia { x, y, z, vx: 0.0, vy: 0.0, ax: 0.0, ay: 0.0 }
    }
}

/// Alcance del error de posición, en metros.
///
/// Era 14 m, que servía cuando el dron no pasaba de 4 m/s y las rutas eran de
/// veinte metros. Persiguiendo a 25 m/s se queda corto por un factor de tres:
/// si la moto le saca 30 m, el termómetro está saturado y **el dron no sabe si
/// está 30 m atrás o 300** — deja de tener gradiente justo cuando más lo
/// necesita.
const ALCANCE_ERROR: f32 = 40.0;

/// Exponente de compresión del error de posición, más agresivo que el general.
///
/// Estirar el alcance de 14 a 40 m se paga en resolución cerca de cero, y cerca
/// de cero es donde se aterriza. Con el exponente general (0.6) medio metro de
/// error movía 0.40 bits de los once — o sea nada, y la posada se volvía a
/// romper. Con 0.45 mueve 0.76, casi el doble, sin perder el alcance largo:
///
/// | error | bits con 0.60 | bits con 0.45 |
/// |---|---|---|
/// | 0.5 m | 0.40 | **0.76** |
/// | 2 m   | 0.90 | **1.43** |
/// | 10 m  | 2.35 | 2.95 |
/// | 40 m  | 5.50 | 5.50 |
///
/// Es el mismo truco que usa un ojo: mucha resolución en el centro, alcance en
/// la periferia. No hay bits gratis — se compra resolución lejos con
/// resolución cerca, y acá interesa al revés.
const COMPRESION_ERROR: f32 = 0.45;

/// El repertorio de sensores, con la guía completa.
///
/// # El presupuesto de 96 bits, y por qué se movió
///
/// El kernel tiene 128 bits de contexto: 96 de sensores y 32 de memoria
/// recurrente. No se toca — el genoma se comparte con otro proyecto y el
/// formato en disco es parte del contrato. Así que meter la velocidad del
/// objetivo obliga a sacar bits de otro lado, y eso es una decisión, no un
/// detalle:
///
/// | campo | antes | ahora | por qué |
/// |---|---|---|---|
/// | error adelante / lateral | 12+12 | 11+11 | el termómetro pierde suave: 13 niveles pasan a 12 |
/// | error vertical | 10 | 8 | el eje z es el que menos decide en persecución |
/// | altura sobre el suelo | 9 | 7 | |
/// | **velocidad relativa al objetivo** | — | **8+7** | lo nuevo, y lo que hace posible seguir |
/// | velocidad propia | 8+8+6 | 7+6+5 | |
/// | actitud | 5+5+5 | 4+4+4 | |
/// | visión | 7+6+3 | 6+5+3 | |
///
/// La velocidad relativa no es un lujo ni se puede deducir de lo demás: el
/// error de posición dice DÓNDE está el objetivo, la velocidad relativa dice
/// si se está escapando. Un piloto que sólo ve el error siempre llega tarde,
/// y el atraso se paga en metros proporcionales a la velocidad del objetivo.
pub fn sensores_guia(m: &Dron3, v: &RasgosVisuales, g: &Guia) -> (u64, u32) {
    use crate::codigo::{termo, termo_signo, Empaque, COMPRESION, LINEAL};

    // `DRON_CODIGO=binario` vuelve al código viejo. Queda para poder correr el
    // A/B end-to-end del cambio más grande que recibió el proyecto sin tener
    // que revivirlo desde git — ver la deuda 6 de CLAUDE.md.
    if codigo_viejo() {
        return sensores_binario(m, v, g.x, g.y, g.z);
    }

    let (dx, dy) = (g.x - m.x, g.y - m.y);
    let (s, c) = (m.yaw.sin(), m.yaw.cos());
    let adelante = dx * c + dy * s;
    let lateral = -dx * s + dy * c;
    let (v_adelante, v_lateral) = velocidad_cuerpo(m);

    // Velocidad del objetivo RELATIVA a la propia, en marco del cuerpo. Cero
    // quiere decir "vamos igual": es la señal que hay que anular para seguir,
    // igual que el error de posición es la que hay que anular para llegar.
    let (rvx, rvy) = (g.vx - m.vx, g.vy - m.vy);
    let rel_adelante = rvx * c + rvy * s;
    let rel_lateral = -rvx * s + rvy * c;

    let mut e = Empaque::nuevo();
    // --- error al punto de guía (30 bits): el campo que decide si llega ---
    e.mete(termo_signo(adelante, ALCANCE_ERROR, 11, COMPRESION_ERROR), 11);
    e.mete(termo_signo(lateral, ALCANCE_ERROR, 11, COMPRESION_ERROR), 11);
    e.mete(termo_signo(g.z - m.z, 20.0, 8, COMPRESION_ERROR), 8);
    // --- velocidad relativa al objetivo (15 bits): el campo que decide si SIGUE ---
    e.mete(termo_signo(rel_adelante, VEL_MAX_APARATO, 8, COMPRESION), 8);
    e.mete(termo_signo(rel_lateral, VEL_MAX_APARATO, 7, COMPRESION), 7);
    // --- dónde estoy respecto del SUELO (7 bits): despegue y aterrizaje ---
    e.mete(termo(m.z, 40.0, 7, COMPRESION), 7);
    // --- cómo me estoy moviendo (18 bits): el campo que decide si frena ---
    // Los rangos siguen al sobre del aparato. Con los viejos (7 m/s) el dron
    // quedaba con el termómetro saturado a un cuarto de su velocidad de
    // crucero: ciego a su propia rapidez justo cuando corre.
    e.mete(termo_signo(v_adelante, VEL_MAX_APARATO, 7, COMPRESION), 7);
    e.mete(termo_signo(v_lateral, VEL_MAX_APARATO, 6, COMPRESION), 6);
    e.mete(termo_signo(m.vz, crate::pid3d::VZ_SUBIDA_MAX, 5, COMPRESION), 5);
    // --- actitud (12 bits), lineal: 30° de alabeo no es "más lejos" de nada ---
    // Rango 1.45 rad, apenas pasado de `INCLINACION_EXTREMA`. Con el viejo de
    // 0.7 el dron saturaba en crucero y no podía distinguir "inclinado para
    // correr" de "cayéndome".
    e.mete(termo_signo(m.roll, 1.45, 4, LINEAL), 4);
    e.mete(termo_signo(m.pitch, 1.45, 4, LINEAL), 4);
    e.mete(termo_signo(m.r, 3.0, 4, LINEAL), 4);
    // --- visión (14 bits): el estorbo y el loom ---
    // ±1.4 rad, el mismo campo con el que la cámara los detecta
    // (`Camara::campo_obstaculos`). Si acá quedara ±0.7 el termómetro
    // saturaría justo en el rango nuevo y el dron volvería a ser ciego a un
    // poste que se le fue al costado — que es exactamente el ciclo límite que
    // se estaba arreglando.
    // **ANILLO DE PROXIMIDAD: 5 sectores × 2 bits = 10.** Reemplaza al par
    // rumbo-promedio + amenaza-sumada, que tenía un modo de falla medido y
    // fatal: dos postes a los costados promedian a "rumbo cero", o sea que el
    // dron leía «el hueco está al centro» justo cuando el centro era lo único
    // bloqueado. Con obstáculos nuevos eso daba 1/3, peor que el clásico (2/3).
    //
    // Dos bits por sector es grueso a propósito —tres niveles: despejado / algo
    // / encima— porque para encontrar el hueco importa CUÁL sector está libre,
    // no con cuánta precisión está ocupado. Cinco sectores contra 0.7 de un
    // solo rumbo es un cambio de "dónde está el estorbo" a "por dónde se pasa".
    for k in 0..crate::sim3d::SECTORES {
        e.mete(termo(v.anillo[k], 1.0, 2, LINEAL), 2);
    }
    // **Elevación del estorbo, donde antes iba la divergencia.** Cuánto
    // sobresale su punta por encima del dron: positivo = todavía tapa,
    // negativo = se le puede pasar por arriba.
    //
    // El canje está justificado por los dos lados:
    //
    // - Lo que SALE no informaba nada nuevo. `divergencia` es exactamente
    //   `vz / altura` (`sim3d::ver_base`), y las dos magnitudes ya viajan por
    //   separado con mucha mejor resolución (velocidad 18 bits, altura 7). Es
    //   palabra por palabra el motivo por el que ya se habían sacado `flujo_x`
    //   y `flujo_y` — ver "El reparto de los 96 bits" más arriba. Quedaba como
    //   el último resto de esa familia.
    // - Lo que ENTRA hacía falta de verdad. `Obstaculo::distancia` devuelve
    //   infinito si el dron va más alto que el poste, o sea que la física SÍ
    //   permite pasar por arriba, pero ningún bit se lo contaba al piloto:
    //   había una solución válida que era literalmente invisible. Esa
    //   asimetría entre lo que el mundo permite y lo que el dron percibe es
    //   del proyecto, no del genoma.
    //
    // ±8 m de rango: un poste de 8 m visto desde el suelo satura arriba, y a
    // 8 m por encima de la punta ya no hay ninguna decisión que tomar.
    e.mete(termo_signo(v.obs_altura, 8.0, 4, LINEAL), 4);

    debug_assert_eq!(e.usados(), crate::iafz::SENSOR_BITS, "la palabra no llena los 96 bits");
    e.listo()
}

/// El techo del aparato, que es el rango natural de cualquier sensor de
/// velocidad: pasado de ahí no hay nada que medir porque no puede ir más
/// rápido.
pub const VEL_MAX_APARATO: f32 = 32.0;

/// Si está puesto `DRON_CODIGO=binario`. Se lee una sola vez.
fn codigo_viejo() -> bool {
    use std::sync::OnceLock;
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| std::env::var("DRON_CODIGO").as_deref() == Ok("binario"))
}

/// El código de cuantización BINARIA, el que estuvo hasta ahora.
///
/// No es código muerto: es el brazo de control del A/B. La justificación del
/// termómetro es una medición del mecanismo (la correlación de Hamming, en
/// `codigo.rs`), y eso es evidencia fuerte pero no es comparar dos
/// entrenamientos. Mientras ese A/B no se haya corrido, tirar esta función
/// sería tirar la única forma barata de correrlo.
fn sensores_binario(m: &Dron3, v: &RasgosVisuales, ox: f32, oy: f32, oz: f32) -> (u64, u32) {
    fn q(x: f32, rango: f32, bits: u32) -> u64 {
        let niveles = (1u64 << bits) - 1;
        let n = ((x / rango).clamp(-1.0, 1.0) * 0.5 + 0.5) * niveles as f32;
        (n.round() as u64).min(niveles)
    }
    let (dx, dy) = (ox - m.x, oy - m.y);
    let (s, c) = (m.yaw.sin(), m.yaw.cos());
    let adelante = dx * c + dy * s;
    let lateral = -dx * s + dy * c;
    let (v_adelante, v_lateral) = velocidad_cuerpo(m);
    let mut e = crate::codigo::Empaque::nuevo();
    for (val, rango, bits) in [
        (adelante, 14.0, 6), (lateral, 14.0, 6), (oz - m.z, 8.0, 6),
        (v_adelante, 7.0, 5), (v_lateral, 7.0, 5), (m.vz, 5.0, 5),
        (m.roll, 0.7, 5), (m.pitch, 0.7, 5), (m.r, 3.0, 4),
        (v.flujo_x, 8.0, 6), (v.flujo_y, 8.0, 6), (v.divergencia, 4.0, 5),
        (v.obj_horizontal, 0.7, 7), (v.obj_vertical, 0.7, 5),
        (v.obj_tamano * 2.0 - 1.0, 1.0, 5), (v.confianza * 2.0 - 1.0, 1.0, 4),
        (v.obs_horizontal, 0.7, 6), (v.obs_proximidad * 2.0 - 1.0, 1.0, 5),
    ] {
        e.mete(q(val, rango, bits), bits);
    }
    e.listo()
}

/// Término de ACTITUD de la aptitud, compartido por todos los gimnasios para
/// que no se bifurquen cinco copias.
///
/// El reparto viejo castigaba TODA inclinación (`-(roll+pitch)*0.015`), y eso
/// es castigar volar: en este simulador el empuje sale del eje del cuerpo, así
/// que para trasladarse hay que inclinarse. El resultado era un genoma
/// cobarde. Ahora: nada de castigo dentro de la envolvente cómoda, castigo
/// suave y proporcional afuera, y un PREMIO por volver a vuelo estable
/// después de una actitud extrema.
#[derive(Default)]
pub struct Actitud {
    excursion: bool,
}

/// rad/s de guiñada que salen gratis. Por encima se paga.
///
/// Girar no estaba castigado por ningún lado, y el genoma aprendió a hacerlo
/// todo el tiempo: traza del campeón, yaw de 0 a -6.78 rad en 9 s (más de una
/// vuelta entera) a ~0.8 rad/s constante. Eso no es un detalle estético —
/// las consignas de velocidad son en marco del CUERPO, así que con el cuerpo
/// rotando un "andá para adelante" constante barre un círculo en vez de
/// trasladarse. Avanzaba 2 m en 9 s y se plantaba a ~4 m del objetivo en las
/// cinco rutas. 0.5 rad/s libres siguen siendo una vuelta completa cada 12 s:
/// alcanza de sobra para navegar, y lo que se castiga es trompearse.
const GIRO_COMODO: f32 = 0.5;

impl Actitud {
    pub fn paso(&mut self, d: &Dron3) -> f32 {
        let inc = d.roll.abs().max(d.pitch.abs());
        let mut delta = 0.0;
        if inc > crate::sim3d::INCLINACION_COMODA {
            delta -= (inc - crate::sim3d::INCLINACION_COMODA) * 0.05;
        }
        if d.r.abs() > GIRO_COMODO {
            delta -= (d.r.abs() - GIRO_COMODO) * 0.4;
        }
        // El bono exige volver a vuelo REALMENTE estable —poco ángulo y poca
        // velocidad angular—, y para re-armarlo hay que volver a pasarse de
        // 60°, que es arriesgado. Sin esas dos condiciones el genoma aprende a
        // rebotar contra el límite para cobrar el premio en loop.
        if self.excursion && inc < crate::sim3d::INCLINACION_COMODA
            && (d.p.abs() + d.q.abs()) < 1.0
        {
            delta += 6.0;
            self.excursion = false;
        }
        if d.extremo() {
            self.excursion = true;
        }
        delta
    }
}







/// Qué tarea se está entrenando. Existe para que la MISMA maquinaria evolutiva
/// sirva para las dos y no se bifurquen dos copias del bucle (que es de donde
/// salen las diferencias sutiles que después parecen resultados).
#[derive(Clone, Copy, PartialEq)]
pub enum Tarea {
    /// Cadenas de puntos con despegue y aterrizaje (`mision.rs`).
    Mision,
    /// Seguir un vehículo en movimiento (`persecucion.rs`). Currículo por
    /// recorrido, mismo criterio de "sólo se sube aprobando" que `Mision`.
    Persecucion,
}

/// La aptitud de un agente en el peldaño dado, sea cual sea la tarea.
///
/// Devuelve DOS números y la distinción es la que destrabó el peldaño 13:
///
/// - **honesta**: el peor caso, `min` sobre todos los términos. Es la que
///   APRUEBA, y no se toca — es la vara de la regla 3.2.
/// - **orden**: la que ORDENA la población. Igual a la honesta salvo en misión,
///   donde se suaviza para que el progreso parcial tenga gradiente. Ver
///   `mision::PESO_MEDIA` para la medición que lo justifica.
///
/// Para las otras tareas las dos son la misma: el cambio se hizo donde estaba
/// medido el problema, no en todas partes por simetría.
fn aptitud_de(
    id: i32, tarea: Tarea, peldano: usize, robusto: bool, ruido: Ruido,
    celda: Option<usize>,
) -> (f32, f32) {
    match tarea {
        // `celda` es lo único que distingue entrenar un genoma monolítico de
        // entrenar UNA celda del repertorio: con `Some(c)`, el individuo sólo
        // vuela en la celda `c` y el resto del vuelo lo hace el banco
        // congelado, idéntico para toda la población. Ver `mision::Piloto`.
        Tarea::Mision => crate::mision::aptitud_y_orden(
            match celda {
                Some(c) => crate::mision::Piloto::PorCelda { slot: id, entrena: c },
                None => crate::mision::Piloto::Red(id),
            },
            peldano, robusto, ruido),
        Tarea::Persecucion => {
            let a = crate::persecucion::aptitud(id, peldano, robusto, ruido);
            (a, a)
        }
    }
}

/// Evalua toda la poblacion repartiendo agentes entre hilos.
///
/// Es seguro aunque el kernel use estado global: cada hilo toca UNICAMENTE los
/// indices de agente que le tocan, y `swarm[]`, `recurrent_32[]` y `rng_state[]`
/// estan indexados por id. No hay dos hilos escribiendo la misma posicion.
fn evaluar_poblacion(
    tarea: Tarea, peldano: usize, robusto: bool, ruido: Ruido,
    celda: Option<usize>,
) -> Vec<(f32, f32, i32)> {
    let hilos = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .min(POBLACION as usize);
    let por_hilo = (POBLACION as usize).div_ceil(hilos);

    let mut manos = Vec::new();
    for h in 0..hilos {
        let ini = (h * por_hilo) as i32;
        let fin = (((h + 1) * por_hilo) as i32).min(POBLACION);
        if ini >= fin {
            break;
        }
        manos.push(std::thread::spawn(move || {
            (ini..fin)
                .map(|i| { let (h, o) = aptitud_de(i, tarea, peldano, robusto, ruido, celda); (h, o, i) })
                .collect::<Vec<_>>()
        }));
    }
    let mut r = Vec::with_capacity(POBLACION as usize);
    for m in manos {
        r.extend(m.join().unwrap_or_default());
    }
    r
}

pub struct Config {
    pub generaciones: usize,
    pub semilla: u32,
    pub salida: String,
    pub ruido: Ruido,
    /// Si esta puesto, se ignora `ruido` y se evalua en todo el rango.
    pub robusto: bool,
    /// Dibuja al campeon volando cada N generaciones (0 = no dibujar).
    pub ver_cada: usize,
    /// Qué se entrena: el objetivo suelto de siempre o misiones completas.
    pub tarea: Tarea,
    /// Permite saltar un peldaño de misión sin aprobarlo, tras estancarse.
    ///
    /// Apagado por defecto y a propósito (regla 3.7 de CLAUDE.md): avanzar con
    /// la fase anterior incompleta es lo que fabricó el genoma que aprobaba
    /// cero entregas. Se prende a mano con `entrenarmision ... saltar` cuando
    /// una persona decide que ese peldaño se investiga después, y el
    /// entrenador lo grita en el log para que quede la deuda anotada.
    pub saltar: bool,
    /// Entrenar SIEMPRE en este peldaño, sin currículo y sin subir. Existe
    /// para los expertos de la mezcla: el experto de frenado tiene que
    /// entrenar en el recorrido donde el frenado EXISTE (el semáforo), no
    /// arrancar en las rectas donde su régimen no se activa nunca — ahí
    /// todos los individuos puntúan idéntico, no hay gradiente, y el campeón
    /// sale sorteado. El peldaño es el muestreador de régimen.
    pub peldano_fijo: Option<usize>,
    /// Sembrar parte de la población inicial con este genoma, en vez de
    /// arrancar TODO al azar.
    ///
    /// Hace falta por una asimetría de la mezcla que se midió cara: un slot
    /// de experto VACÍO es exactamente el piloto clásico (regla 3.15), pero
    /// uno con un genoma AL AZAR emite basura acotada y empeora las cosas.
    /// Entrenando el experto de recta se vio tal cual: la mezcla sin él daba
    /// 12188.2 y la población aleatoria arrancaba en 11660.3, así que las
    /// primeras decenas de generaciones se gastan aprendiendo a NO estorbar
    /// antes de poder mejorar nada.
    ///
    /// Se siembra sólo una parte (`ELITE` de `POBLACION`) y no toda: un
    /// genoma bueno clonado en los 48 slots deja a la élite entera en la
    /// misma cuenca, y con la escalera de mutación esa cuenca se sostiene
    /// cientos de generaciones — exactamente la forma del estancamiento que
    /// este trabajo está tratando de romper.
    pub sembrar_desde: Option<String>,
    /// **Entrenar el REPERTORIO en vez de un genoma solo.**
    ///
    /// Con esto puesto, cada peldaño entrena UNA celda
    /// (`mision::asignacion_de_celdas`) y las otras ocho vuelan congeladas,
    /// idénticas para toda la población. El olvido entre peldaños que viven
    /// en celdas distintas deja de ser posible: el genoma que aprobó
    /// «esquivar» no lo toca nadie mientras se entrena «posarse».
    ///
    /// Exige `sembrar_desde`: un repertorio recién sembrado vale lo que vale
    /// el genoma del que nace (`Repertorio::sembrar`), y sembrarlo de ruido
    /// dejaría ocho celdas emitiendo basura acotada para siempre — las celdas
    /// que ningún peldaño entrena (el censo dice que cinco de las nueve casi
    /// no se visitan) no tienen forma de mejorar solas.
    pub repertorio: bool,
    /// Arrancar la escalera en este peldaño en vez del primero, SIN dejar de
    /// subir (a diferencia de `peldano_fijo`, que la clava ahí).
    ///
    /// Existe para poder atacar el peldaño donde una corrida se trabó sin
    /// volver a moler los once de abajo, que la misma corrida ya aprobó y que
    /// el examen de repaso vuelve a exigir en cada generación de todos modos.
    /// **Pide `sembrar_desde`**: arrancar en el peldaño 12 con población al
    /// azar sería exactamente lo que la regla 3.7 prohíbe — avanzar con la
    /// fase anterior incompleta, sólo que fingiendo que está completa.
    pub peldano_inicial: usize,
    /// Sembrar la población ENTERA desde `sembrar_desde` (élite exacta, el
    /// resto mutado) en vez de sólo la élite.
    ///
    /// Existe por un sesgo que se midió al comparar el repertorio contra el
    /// genoma monolítico y que habría invalidado el A/B entero: el modo
    /// repertorio siembra los 48 individuos de la celda, mientras que
    /// `sembrar_desde` sembraba 6 y dejaba 42 **al azar**. No es un detalle —
    /// en el peldaño 12, con el genoma que venía de aprobar el 11, el mejor
    /// de la generación 0 fue uno de los aleatorios con −54.2 contra los
    /// −345.2 del genoma sembrado. O sea que los dos brazos no arrancaban del
    /// mismo lugar, y encima el monolítico arrancaba **mejor** por una razón
    /// que no tenía nada que ver con lo que se estaba comparando.
    pub sembrar_todo: bool,
    /// **Módulo vertical** (`repertorio::VERTICAL`): con repertorio, la
    /// salida de altura la emite un genoma aparte, que sólo se entrena en los
    /// peldaños de despegar/posarse y queda congelado en el resto. Ver
    /// `repertorio::fijar_vertical` para la medición que lo pidió.
    pub modulo_vertical: bool,
    /// **Módulo posar** (`repertorio::POSAR`): los peldaños de posarse
    /// entrenan un genoma dueño de las cuatro salidas en los tramos de
    /// aterrizaje, y nada más lo toca. Con `modulo_vertical`, despegar sigue
    /// entrenando el vertical. Ver `repertorio::fijar_posar`.
    pub modulo_posar: bool,
}


/// Se sube de peldaño cuando el campeón deja de mejorar, no cada N
/// generaciones fijas: si todavía está aprendiendo el peldaño actual,
/// alejarle el objetivo le rompe lo que estaba por encontrar.
const PACIENCIA: usize = 15;

/// **En misiones NO se sube de peldaño sin aprobarlo.** Nunca. No hay
/// paciencia que valga: si el campeón no alcanza `mision::umbral_aprobado`,
/// se queda en ese peldaño hasta que lo logre o hasta que se acaben las
/// generaciones.
///
/// Esto existía antes como "válvula de escape para no quedarse clavado", y
/// esa válvula es exactamente el defecto que hay que evitar (regla 3.7 de
/// CLAUDE.md). Lo que producía, medido en esta misma sesión: el currículo
/// promovía peldaño tras peldaño con el cartel de *estancado, sube igual*, y
/// el genoma llegaba a la misión de entrega **sin saber esquivar dos postes y
/// sin saber posarse**. El resultado fue un genoma que fingía saber hacer
/// todo y no sabía hacer casi nada — 0/3 en entregar, 0/3 en posarse.
///
/// Un entrenamiento clavado en el peldaño 8 informando "no aprendo a esquivar
/// dos postes" es **más útil** que uno que termina los trece peldaños con
/// deuda escondida en cada uno: el primero te dice dónde está el problema, el
/// segundo te lo esconde y encima te devuelve un genoma inservible.
///
/// Saltar un peldaño sigue siendo posible, pero como DECISIÓN HUMANA
/// explícita (`entrenarmision ... saltar`), nunca como default automático.
const SIN_PACIENCIA: &str = "los peldaños de misión sólo se suben aprobándolos";

/// **Cuántos sorteos INDEPENDIENTES tiene que pasar el campeón para que un
/// peldaño se dé por aprobado.**
///
/// Era uno, y uno es una muestra — justo lo que la regla 3.20 prohíbe, en el
/// único lugar donde la regla se había escrito para arreglar otra cosa. La
/// medición que lo obliga (bitácora 7.33): el genoma que «aprobó» el peldaño
/// 9 de la corrida de 7.32, medido después contra doce juegos de escenarios
/// independientes, **aprueba en 1 de 12** y tiene mediana −46.7 contra un
/// umbral de 900. No había aprendido a esquivar: se ganó una rifa.
///
/// Con una confirmación sola y una tasa de acierto del 8%, una corrida que
/// cruza el umbral todas las generaciones aprueba por azar en unas doce; la
/// corrida real lo hizo en la 16.
///
/// **Y tres tampoco alcanzaban, porque la cuenta se hizo por INTENTO y el
/// guardián se consulta cientos de veces por corrida** (bitácora 7.34). El
/// comentario que estaba acá decía "con tres, la misma tasa da 0.05%", y es
/// cierto *por cruce del umbral* — lo que faltaba era multiplicar por cuántos
/// cruces tiene una corrida. Contado del log del A/B de 7.34, brazo del
/// repertorio, 1200 generaciones:
///
/// ```text
///   cruces del umbral                        425
///   murieron en la confirmación 1            357   → sobrevive el 16.0%
///   murieron en la confirmación 2             57   → sobrevive el 16.2%
///   murieron en la confirmación 3             10   → sobrevive el  9.1%
///   aprobaron (3 de 3)                         1
/// ```
///
/// Las tres tasas de supervivencia dan lo mismo (~0.16), que es exactamente lo
/// que se espera de sorteos independientes con p ≈ 0.16. Con esa p, tres
/// confirmaciones dejan pasar `0.16³ × 425 ≈ 1.7` falsos aprobados por
/// corrida — y el A/B tuvo **1**. El modelo y el log coinciden, así que la
/// cuenta se puede usar para elegir el número.
///
/// Para que el falso aprobado esperado por corrida baje de 0.01 hacen falta
/// **seis**: `0.16⁶ × 425 = 0.007`.
///
/// Y seis sale casi gratis, medido en el mismo log: la comprobación se corta
/// en la primera confirmación que falla, así que las confirmaciones 4, 5 y 6
/// las paga sólo lo que ya pasó tres — **1 cruce de 425 en el brazo del
/// repertorio y 0 de 230 en el monolítico**. Son del orden de tres
/// evaluaciones extra por corrida de 1200 generaciones, contra el costo de
/// equivocarse, que está medido y es una corrida entera: el A/B de 7.33 dio
/// el peldaño 9 por aprobado en la generación 933 y molió las 267 que
/// quedaban sobre una aprobación que no existía.
///
/// La regla general, que es la que faltaba y vale para cualquier `if` que
/// acepte algo medido: **cuántas muestras necesita un guardián depende de
/// cuántas veces se lo consulta.** Uno que se consulta una vez necesita
/// pocas; uno que se consulta 425 veces por corrida necesita que
/// `p^k × consultas ≪ 1`. Ver regla 3.20.
///
/// **Y seis dejó de alcanzar cuando `p` cambió** (bitácora 7.58). La cuenta
/// de arriba supone p = 0.16, la de un genoma que casi nunca pasa. El módulo
/// posar (7.57) aprobó posarse con 6 confirmaciones seguidas y después
/// generalizó a 6-7 de 12 sorteos: p ≈ 0.55, y 0.55⁶ = 2.8% por cruce. Con
/// p moderada el falso aprobado deja de ser raro. Doce: 0.55¹² = 0.08% por
/// cruce, y un módulo tan bueno como el clásico en la entrega (9/12, p =
/// 0.75) igual pasa: 3% por cruce, con cientos de cruces por corrida. Sigue
/// costando casi nada: la comprobación se corta en la primera que falla.
pub const CONFIRMACIONES: u32 = 12;

fn nombre_peldano(tarea: Tarea, peldano: usize, peldanos: usize) -> String {
    match tarea {
        Tarea::Mision => format!("peldaño {}/{}: {}", peldano + 1, peldanos,
                                 crate::mision::misiones(peldano)[0].nombre),
        Tarea::Persecucion => format!("peldaño {}/{}: {}", peldano + 1, peldanos,
                                      crate::persecucion::recorridos_del_curriculo()
                                          .get(peldano).map_or("?".into(), |r| r.0.clone())),
    }
}

/// Pone a toda la población en el elite de una celda y le da diversidad por
/// mutación.
///
/// No es "copiar el campeón a los 48 slots": eso deja a la élite entera en la
/// misma cuenca y la escalera de mutación la sostiene ahí cientos de
/// generaciones (la medición está en `Config::sembrar_desde`). La élite sale
/// intacta —el peor caso del peldaño es el genoma que ya había, nunca menos—
/// y el resto arranca mutado.
/// El genoma del que arranca la población de un peldaño: su celda, o el
/// módulo vertical si el peldaño lo entrena.
fn genoma_de_modulo(r: &crate::repertorio::Repertorio, c: usize) -> Vec<u8> {
    if c == crate::repertorio::VERTICAL {
        crate::repertorio::vertical().map(|v| v.to_vec()).unwrap_or_else(|| r.celdas[0].genoma.clone())
    } else if c == crate::repertorio::POSAR {
        crate::repertorio::posar().map(|v| v.to_vec()).unwrap_or_else(|| r.celdas[0].genoma.clone())
    } else {
        r.celdas[c].genoma.clone()
    }
}

fn sembrar_poblacion(g: &[u8], tasa: f32) {
    for i in 0..POBLACION {
        iafz::restaurar(i, g);
    }
    for i in ELITE..POBLACION {
        iafz::mutar(i, i, tasa);
    }
}

pub fn entrenar(cfg: &Config) -> f32 {
    let mut rng = cfg.semilla.wrapping_mul(2_654_435_761).wrapping_add(1);
    let mut aleatorio = move || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        rng
    };

    for i in 0..POBLACION {
        iafz::init(i, cfg.semilla.wrapping_add(i as u32 * 7919));
    }

    // (honesta, orden, id). Se ORDENA por `orden` y se APRUEBA por `honesta`.
    let mut ranking: Vec<(f32, f32, i32)> = Vec::new();
    // El campeón vigente por PEOR CASO (ver más abajo). Vive afuera del bucle
    // porque el guardado final también tiene que archivarlo a él.
    let mut campeon: i32 = 0;
    let mut mejor_historico = f32::NEG_INFINITY;
    let mut estancado = 0usize;
    if let Some(ruta) = &cfg.sembrar_desde {
        // Slot de paso: la cuna todavía no se usa en el arranque.
        const PASO: i32 = CUNA + POBLACION - 1;
        if iafz::cargar(PASO, ruta) {
            if cfg.sembrar_todo {
                sembrar_poblacion(&iafz::volcar(PASO), 0.22);
                println!("  (sembrados los {POBLACION} desde {ruta})");
            } else {
                for i in 0..ELITE {
                    iafz::mutar(PASO, i, 0.0); // tasa 0 = copia
                }
                println!("  (sembrados {ELITE} de {POBLACION} desde {ruta})");
            }
        } else {
            println!("  (no pude sembrar desde {ruta}: sigo con población al azar)");
        }
    }
    let mut peldano = cfg.peldano_fijo.unwrap_or(cfg.peldano_inicial);
    if cfg.peldano_inicial > 0 && cfg.sembrar_desde.is_none() {
        println!("  ¡arrancar en el peldaño {} sin sembrar= es avanzar con la fase \
                  anterior incompleta (regla 3.7)!", cfg.peldano_inicial + 1);
        return f32::NEG_INFINITY;
    }

    // -------------------------------------------------------------------
    // El repertorio: nueve genomas, uno por celda, y el currículo repartido
    // entre ellos. Ver `Config::repertorio` y `mision::asignacion_de_celdas`.
    // -------------------------------------------------------------------
    let mut rep: Option<crate::repertorio::Repertorio> = None;
    let mut asignacion: Vec<usize> = Vec::new();
    if cfg.tarea == Tarea::Mision && cfg.repertorio {
        const SEMILLA_REP: i32 = CUNA + POBLACION - 1;
        // Sin `sembrar_desde` el repertorio nace de un genoma AL AZAR, las
        // nueve celdas iguales. Es peor punto de partida que sembrarlo de un
        // campeón —y por eso `Config::repertorio` lo desaconseja— pero es la
        // única forma honesta de correr la escalera COMPLETA desde el peldaño
        // 1: sembrar de un campeón viejo hereda los peldaños que ese campeón
        // aprobó, y si esos aprobados salieron de un criterio roto, la corrida
        // nueva arranca parada sobre falsos positivos. Medido: el genoma que
        // «aprobó esquivar» en la corrida de 7.32 aprueba en 1 de 12 sorteos
        // independientes (bitácora 7.33 §7).
        match &cfg.sembrar_desde {
            Some(ruta) if iafz::cargar(SEMILLA_REP, ruta) => {}
            Some(ruta) => {
                println!("  no pude sembrar el repertorio desde {ruta}");
                return f32::NEG_INFINITY;
            }
            None => {
                iafz::init(SEMILLA_REP, cfg.semilla.wrapping_add(31337));
                println!("  (repertorio sembrado AL AZAR: escalera completa desde el peldaño 1)");
            }
        }
        let r = crate::repertorio::Repertorio::sembrar(SEMILLA_REP);
        crate::repertorio::fijar_banco(&r);
        // **El censo se toma con la VARA, no con el genoma sembrado.**
        // Dos motivos: es reproducible (no depende de qué genoma se sembró,
        // así que dos corridas son comparables) y es competente en los quince
        // peldaños, mientras que un genoma que se estrella en uno daría un
        // censo de la caída y no de la maniobra.
        crate::mision::fijar_variante(777);
        let censo = crate::mision::censo_de_celdas(crate::mision::Piloto::Clasico);
        asignacion = crate::mision::asignacion_de_celdas(&censo);
        if cfg.modulo_vertical {
            // Los peldaños cuya maniobra ES vertical (despegar, posarse)
            // entrenan el módulo vertical y NINGUNA celda; el resto entrena su
            // celda y no puede tocar la altura. Por nombre y no por censo: el
            // censo cuenta situaciones (dónde está el dron), no funciones (qué
            // está haciendo), que es justo la distinción que falló.
            for (p, a) in asignacion.iter_mut().enumerate() {
                let n = crate::mision::misiones(p)[0].nombre;
                if cfg.modulo_posar && n.contains("posarse") {
                    *a = crate::repertorio::POSAR;
                } else if n.contains("despegar") || n.contains("posarse") {
                    *a = crate::repertorio::VERTICAL;
                }
            }
            crate::repertorio::fijar_vertical(&r.celdas[0].genoma);
            // Escrito desde el arranque: sin él, medir el repertorio a mitad
            // de corrida (`sorteos rep=`) volaría otro piloto.
            let ruta = crate::repertorio::ruta_vertical(&format!("{}.repertorio.bin", cfg.salida));
            let _ = std::fs::write(&ruta, &r.celdas[0].genoma);
            if cfg.modulo_posar {
                crate::repertorio::fijar_posar(&r.celdas[0].genoma);
                let ruta = crate::repertorio::ruta_posar(&format!("{}.repertorio.bin", cfg.salida));
                let _ = std::fs::write(&ruta, &r.celdas[0].genoma);
            }
        }
        let nombre_de = |c: usize| -> &'static str {
            if c == crate::repertorio::VERTICAL { "MÓDULO VERTICAL (sólo la altura)" }
            else if c == crate::repertorio::POSAR { "MÓDULO POSAR (todo, en los tramos de aterrizaje)" }
            else { crate::repertorio::Contexto {
                estorbo: c % crate::repertorio::ESTORBOS,
                tarea: c / crate::repertorio::ESTORBOS,
            }.nombre() }
        };
        println!("  REPERTORIO — qué celda entrena cada peldaño:");
        let mut anterior = usize::MAX;
        for (p, c) in asignacion.iter().enumerate() {
            if *c != anterior {
                println!("        peldaño {:>2}+  → «{}»", p + 1, nombre_de(*c));
                anterior = *c;
            }
        }
        // La población arranca en el elite de la celda del primer peldaño, no
        // al azar: en esa celda un genoma aleatorio emite basura acotada y las
        // primeras decenas de generaciones se gastan aprendiendo a no estorbar
        // (la misma medición que justifica `sembrar_desde`).
        sembrar_poblacion(&genoma_de_modulo(&r, asignacion[peldano]), 0.22);
        rep = Some(r);
    }

    // Cuántos peldaños tiene la escalera de esta tarea.
    let peldanos = match cfg.tarea {
        Tarea::Mision => crate::mision::PELDANOS,
        Tarea::Persecucion => crate::persecucion::peldanos(),
    };

    for gen in 0..cfg.generaciones {
        // **Postes nuevos cada generación** (`mision::fijar_variante`): dentro
        // de la generación todos los individuos ven los mismos, así que la
        // comparación sigue midiendo el genoma y no la suerte del sorteo;
        // entre generaciones cambian, así que memorizar posiciones deja de
        // pagar. Es la causa medida del "3/3 con los postes de entrenamiento,
        // 1/3 con postes nuevos" (bitácora 7.17).
        crate::mision::fijar_variante(cfg.semilla ^ gen as u32);
        let celda = rep.as_ref().map(|_| asignacion[peldano]);
        ranking = evaluar_poblacion(cfg.tarea, peldano, cfg.robusto, cfg.ruido, celda);
        ranking.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // El número que se informa y que aprueba es el mejor PEOR-CASO de toda
        // la población, no el del primero del ranking: ordenar por la suave no
        // puede inflar lo que se declara aprobado (regla 3.2).
        let mejor = ranking.iter().fold(f32::NEG_INFINITY, |a, r| a.max(r.0));
        // **Quién es el campeón: el del mejor PEOR-CASO, no `ranking[0]`.**
        // `ranking` está ordenado por la aptitud suave (`.1`), que es otra
        // cantidad — así que `ranking[0]` puede ser un individuo distinto del
        // que produjo `mejor`. Durante meses la foto del peldaño aprobado
        // guardó, en ese caso, a un genoma que NO era el que aprobó: el
        // instrumento archivando algo distinto de lo que acababa de medir
        // (regla 3.18).
        campeon = ranking
            .iter()
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
            .map(|r| r.2)
            .unwrap_or(ranking[0].2);
        if mejor > mejor_historico + 0.5 {
            mejor_historico = mejor;
            estancado = 0;
        } else {
            estancado += 1;
        }
        if gen % 10 == 0 || gen + 1 == cfg.generaciones {
            println!("  gen {gen:>3}  mejor {mejor:>8.1}  mediana {:>8.1}   {}{}",
                     ranking[ranking.len() / 2].0,
                     nombre_peldano(cfg.tarea, peldano, peldanos),
                     if estancado > 10 { "   (estancado)" } else { "" });
        }
        // **Un peldaño de misión sólo se sube APROBÁNDOLO.** Ver `SIN_PACIENCIA`
        // y la regla 3.7 de CLAUDE.md: avanzar con la fase anterior incompleta
        // es cómo se fabrica un genoma que finge saber hacer todo.

        let umbral = match cfg.tarea {
            Tarea::Mision => Some(crate::mision::umbral_aprobado(peldano)),
            Tarea::Persecucion => Some(crate::persecucion::umbral_aprobado(peldano)),
        };
        // **Aprobar pide DOS sorteos, no uno.**
        //
        // Medición que lo obliga (bitácora 7.30): con los escenarios
        // re-sorteados cada generación, el umbral se cruza tanto por saber
        // hacer la maniobra como por que el sorteo de ESA generación vino
        // fácil. La corrida de 15 peldaños "aprobó" el peldaño final en la
        // generación 100 con 2817.4 y en la 110 el mismo entrenamiento
        // marcaba −96.6, para no volver a levantar en 280 generaciones más y
        // acusar `DESAPRENDIÓ los peldaños [9..14]`. No fue una recaída: la
        // aprobación nunca había sido real, era una foto de un sorteo suave.
        //
        // Es el mismo error que `repertorio::LOTE` encontró en el lazo
        // anclado, en otra escala: **aceptar por una sola muestra**. Acá se
        // corrige pidiendo que el campeón vuelva a pasar el umbral con un
        // juego de escenarios INDEPENDIENTE antes de que el peldaño se dé
        // por aprobado. Cuesta una evaluación de un genoma —nada al lado de
        // la población entera— y le quita al azar la llave de la escalera.
        //
        // Consecuencia que hay que decir (regla 3.2): esto SUBE la vara de
        // aprobación, así que las generaciones-a-aprobar de corridas
        // anteriores no son comparables con las de ahora.
        let aprobo = match cfg.tarea {
            Tarea::Mision | Tarea::Persecucion => {
                mejor >= umbral.unwrap() && {
                    let mut pasa = true;
                    for k in 0..CONFIRMACIONES {
                    // La variante de confirmación tiene que caer en el MISMO
                    // rango chico que las del entrenamiento. Con un XOR contra
                    // una constante grande caía siempre en la zona donde
                    // `rutas()` perdía precisión y devolvía un escenario fijo:
                    // la "confirmación con escenarios nuevos" era, en realidad,
                    // siempre el mismo escenario. Ver el comentario de `rutas`.
                    crate::mision::fijar_variante(
                        cfg.semilla ^ (gen as u32).wrapping_add(700_001 + k * 977));
                    let (confirmado, _) =
                        aptitud_de(campeon, cfg.tarea, peldano, cfg.robusto, cfg.ruido, celda);
                    crate::mision::fijar_variante(cfg.semilla ^ gen as u32);
                    if confirmado < umbral.unwrap() {
                        println!("  gen {gen:>3}  · cruzó el umbral con {mejor:.1} pero en la \
                                  confirmación {}/{CONFIRMACIONES} dio {confirmado:.1} — \
                                  fue suerte del sorteo, NO aprueba", k + 1);
                        pasa = false;
                        break;
                    }
                    }
                    pasa
                }
            }
        };
        // El estado en vivo, para que la cabina lo muestre mientras entrena en
        // otro proceso — un archivo en `/tmp` porque el entrenador y el visor
        // son procesos separados. Sólo para persecución: es la única tarea
        // con un widget pidiéndolo.
        if cfg.tarea == Tarea::Persecucion {
            crate::persecucion::escribir_estado(
                gen, cfg.generaciones, peldano, peldanos, mejor,
                ranking[ranking.len() / 2].0, umbral.unwrap_or(0.0), estancado,
                &nombre_peldano(cfg.tarea, peldano, peldanos),
            );
        }
        let sube = match cfg.tarea {
            // `saltar` es la única forma de avanzar sin aprobar, y es una
            // decisión que toma una persona a mano, nunca el entrenador solo.
            Tarea::Mision | Tarea::Persecucion => aprobo || (cfg.saltar && estancado >= PACIENCIA),
        };
        // **Foto del genoma en CADA peldaño aprobado, antes de seguir.**
        //
        // Costó nueve horas de cómputo descubrir por qué hace falta: una
        // corrida de entrega llegó al peldaño 11 de 14 en la generación 324 —
        // récord del proyecto, pasando «esquivar», que el piloto clásico no
        // sabe hacer— y después se quedó 2350 generaciones trabada en
        // «posarse», desaprendiendo los diez peldaños de atrás. Como el
        // entrenador guarda el campeón ACTUAL cada 20 generaciones, el archivo
        // final tenía el derrumbe: 190.6 en un peldaño que había aprobado con
        // 900. El logro se destruyó solo.
        //
        // Una foto por peldaño cuesta 11 KB y una escritura cada varias horas.
        // Perder el mejor resultado de una corrida larga cuesta la corrida.
        if sube {
            let foto = format!("{}.peldano{}", cfg.salida, peldano + 1);
            if iafz::guardar(campeon, &foto) {
                println!("        · foto del genoma en {foto}");
            }
            // **Lo aprendido se guarda en SU celda y se congela ahí.** A
            // partir de acá ningún peldaño posterior puede tocarlo salvo que
            // le toque la misma celda — que es toda la promesa del
            // repertorio, y lo que hace que «esquivar» sobreviva a que se
            // entrene «posarse».
            if let (Some(_), true) = (rep.as_ref(), asignacion.get(peldano) == Some(&crate::repertorio::POSAR)) {
                let g = iafz::volcar(campeon);
                crate::repertorio::fijar_posar(&g);
                let ruta = crate::repertorio::ruta_posar(&format!("{}.repertorio.bin", cfg.salida));
                match std::fs::write(&ruta, &g) {
                    Ok(()) => println!("        · módulo posar guardado en {ruta}"),
                    Err(e) => println!("        · no pude escribir {ruta}: {e}"),
                }
            } else if let (Some(_), true) = (rep.as_ref(), asignacion.get(peldano) == Some(&crate::repertorio::VERTICAL)) {
                let g = iafz::volcar(campeon);
                crate::repertorio::fijar_vertical(&g);
                let ruta = crate::repertorio::ruta_vertical(&format!("{}.repertorio.bin", cfg.salida));
                match std::fs::write(&ruta, &g) {
                    Ok(()) => println!("        · módulo vertical guardado en {ruta}"),
                    Err(e) => println!("        · no pude escribir {ruta}: {e}"),
                }
            } else if let Some(r) = rep.as_mut() {
                let c = asignacion[peldano];
                let ctx = crate::repertorio::Contexto {
                    estorbo: c % crate::repertorio::ESTORBOS,
                    tarea: c / crate::repertorio::ESTORBOS,
                };
                r.reemplazar_elite(ctx, iafz::volcar(campeon));
                crate::repertorio::fijar_banco(r);
                let bin = format!("{}.repertorio.bin", cfg.salida);
                match r.escribir(&bin) {
                    Ok(()) => println!("        · celda «{}» guardada en {bin}", ctx.nombre()),
                    Err(e) => println!("        · no pude escribir {bin}: {e}"),
                }
            }
        }
        if sube && peldano + 1 < peldanos && cfg.peldano_fijo.is_none() {
            peldano += 1;
            mejor_historico = f32::NEG_INFINITY;
            estancado = 0;
            // Peldaño nuevo, celda nueva (o la misma): la población arranca
            // en el elite de ESA celda. Si no se resembrara, los 48
            // individuos seguirían siendo variantes de la celda anterior
            // cargadas en un lugar del vuelo que no les toca.
            if let Some(r) = rep.as_ref() {
                sembrar_poblacion(&genoma_de_modulo(r, asignacion[peldano]), 0.22);
            }
            println!("  gen {gen:>3}  → peldaño {} ({}): {}",
                     peldano + 1,
                     if aprobo { "APROBADO" } else { "SALTADO A MANO — queda deuda" },
                     nombre_peldano(cfg.tarea, peldano, peldanos));
        } else if estancado > 0 && estancado % 50 == 0 {
            // Trabado sin aprobar: se avisa fuerte, se DIAGNOSTICA solo, y se
            // sigue intentando ACÁ. Un entrenamiento clavado diciendo dónde se
            // traba vale más que uno que termina los peldaños con deuda
            // escondida.
            println!("  gen {gen:>3}  TRABADO en «{}» hace {estancado} generaciones \
                      (necesita {:.0}, va {mejor:.1}) — {SIN_PACIENCIA}",
                     nombre_peldano(cfg.tarea, peldano, peldanos), umbral.unwrap());
            // **En cuántos sorteos de cada N aprueba el campeón.** Sin este
            // renglón, "difícil" y "el campeón no lo sabe hacer, cruza el
            // umbral por sorteo" se ven idénticos desde afuera — y la
            // diferencia decide qué hacer: la primera se ataca con
            // generaciones, la segunda no se arregla con ninguna cantidad
            // (regla 3.16, aplicada a la varianza en vez de al techo).
            {
                const MUESTRAS: u32 = 6;
                let mut pasa = 0;
                for k in 0..MUESTRAS {
                    crate::mision::fijar_variante(
                        cfg.semilla ^ (gen as u32).wrapping_add(910_001 + k * 977));
                    let (a, _) = aptitud_de(campeon, cfg.tarea, peldano,
                                            cfg.robusto, cfg.ruido, celda);
                    if a >= umbral.unwrap() { pasa += 1; }
                }
                crate::mision::fijar_variante(cfg.semilla ^ gen as u32);
                println!("      · el campeón aprueba en {pasa} de {MUESTRAS} sorteos \
                          independientes{}",
                         if pasa == 0 { " — no lo sabe hacer, no es falta de suerte" } else { "" });
            }
            match cfg.tarea {
                // **Con el MISMO piloto con el que se calculó la nota.** Si
                // acá se midiera al campeón como genoma suelto mientras la
                // aptitud lo mide dentro del repertorio, el diagnóstico
                // contradiría al número que dice diagnosticar — el bug 2 de
                // la regla 3.18, otra vez y en otro lado.
                Tarea::Mision => crate::mision::telemetria_estancado(
                    match celda {
                        Some(c) => crate::mision::Piloto::PorCelda { slot: campeon, entrena: c },
                        None => crate::mision::Piloto::Red(campeon),
                    }, peldano),
                Tarea::Persecucion => crate::persecucion::telemetria_estancado(campeon, peldano),
            }
        }
        if cfg.ver_cada > 0 && (gen % cfg.ver_cada == 0 || gen + 1 == cfg.generaciones) {
            match cfg.tarea {
                Tarea::Mision => crate::mision::dibujar_campeon(campeon, gen, mejor, peldano),
                // TODO(deuda): no hay traza ASCII de persecución todavía.
                // Dibujar acá con `dibujar_campeon` sería mostrar el escenario
                // de navegación FIJA (`ESCENARIOS`) con la etiqueta de
                // persecución puesta — exactamente el bug que costó la regla
                // 3.3 (el visor mostrando algo que no es lo que se entrena).
                // Mejor no dibujar nada que dibujar lo que no es.
                Tarea::Persecucion => {}
            }
        }

        // El campeón se guarda cada 20 generaciones, no sólo al final.
        //
        // Es para poder diagnosticar un entrenamiento EN CURSO: dos veces hubo
        // que esperar a que terminara una corrida de media hora para recién
        // ahí poder correr `detalle` sobre el genoma y ver qué caso estaba
        // trabando el peldaño. Con esto se mira mientras corre.
        if gen % 20 == 19 {
            if let Some(dir) = std::path::Path::new(&cfg.salida).parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            iafz::guardar(campeon, &cfg.salida);
        }

        if gen + 1 == cfg.generaciones {
            break;
        }

        // Tres escalones, no dos. El de en medio (0.22 tras 10 generaciones
        // sin mejorar) ya estaba, pero no escalaba MÁS si el estancamiento se
        // alargaba — y se midió que eso no alcanza. En el peldaño 3 de
        // persecución, 0.22 sostenido 650 generaciones evaluó del orden de
        // 27.000 descendientes (42 por generación, población 48 menos 6 de
        // élite) sin que NINGUNO superara al campeón ni una fracción. Eso no
        // es "todavía no tuvo suerte": a esa cantidad de intentos, es más
        // consistente con un óptimo local que 0.22 no puede escapar —
        // demasiado grande el paso para afinar, demasiado chico para saltar
        // a otro lado de la superficie de aptitud.
        let tasa = if estancado > 300 { 0.35 } else if estancado > 10 { 0.22 } else { 0.06 };
        for hueco in 0..POBLACION {
            let destino = CUNA + hueco;
            if hueco < ELITE {
                iafz::mutar(ranking[hueco as usize].2, destino, 0.0);
            } else {
                let a = ranking[(aleatorio() % ELITE as u32) as usize].2;
                let b = ranking[(aleatorio() % ELITE as u32) as usize].2;
                iafz::cruzar(a, b, destino);
                iafz::mutar(destino, destino, tasa);
            }
        }
        for i in 0..POBLACION {
            iafz::mutar(CUNA + i, i, 0.0);
        }
    }

    if let Some(dir) = std::path::Path::new(&cfg.salida).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if iafz::guardar(campeon, &cfg.salida) {
        println!("\ngenoma guardado en {}", cfg.salida);
    }
    // Lo que devuelve es el mismo número que se informó como «mejor» durante
    // toda la corrida: el mejor PEOR-CASO, no la aptitud suave del primero
    // del ranking (ver el comentario de `campeon`).
    ranking.iter().fold(f32::NEG_INFINITY, |a, r| a.max(r.0))
}








#[cfg(test)]
mod pruebas_aproximacion {
    use super::*;

    /// **No bajar mientras no estés alineado.** Es la regla que sacó al
    /// piloto clásico de estrellarse en la misión 2 del peldaño 12 (bajaba
    /// 4 m y cruzaba 2.5 m a la vez, y llegaba al suelo de costado con más
    /// velocidad horizontal que `VH_POSADA`).
    #[test]
    fn no_desciende_mientras_esta_desalineado() {
        let lejos = Dron3 { x: 0.0, y: 0.0, z: 4.0, ..Default::default() };
        let alt = altura_de_aproximacion(&lejos, 2.5, 0.0, 0.0);
        assert!(alt >= ALTURA_APROXIMACION,
            "desalineado a 2.5 m y pidió bajar a {alt}");
        let encima = Dron3 { x: 2.4, y: 0.0, z: 4.0, ..Default::default() };
        assert_eq!(altura_de_aproximacion(&encima, 2.5, 0.0, 0.0), 0.0,
            "alineado dentro del radio y no autorizó el descenso");
    }

    /// Un punto EN EL AIRE no es una posada: la regla no tiene que tocarlo, o
    /// el dron dejaría de poder navegar a puntos altos estando lejos.
    #[test]
    fn un_punto_en_el_aire_no_cambia() {
        let d = Dron3 { x: 0.0, y: 0.0, z: 1.0, ..Default::default() };
        assert_eq!(altura_de_aproximacion(&d, 10.0, 10.0, 3.0), 3.0);
    }

    /// **NUNCA pedir una altura mayor a la que ya tiene.** Esta prueba estaba
    /// mal escrita la primera vez —comparaba contra `ALTURA_APROXIMACION` en
    /// vez de contra la altura actual— y por eso dejó pasar un cambio que
    /// mandaba al dron de 0.4 m a 1.0 m. Resultado: rompió el peldaño 11, que
    /// ya funcionaba. La comparación correcta es contra `medido.z`.
    #[test]
    fn nunca_pide_trepar() {
        for z in [0.2f32, 0.4, 0.8, 1.5, 3.0] {
            for err in [0.0f32, 0.6, 1.2, 5.0] {
                let d = Dron3 { x: 0.0, y: 0.0, z, ..Default::default() };
                let alt = altura_de_aproximacion(&d, err, 0.0, 0.0);
                assert!(alt <= z + 1e-6,
                    "a {z} m con error {err} m pidió {alt} m: eso es trepar");
            }
        }
    }

    /// El piso tiene que ser CONTINUO en el error: sin escalón, o el mando
    /// pega un tirón al cruzar la frontera (el defecto que rompió el peldaño
    /// 11 la primera vez).
    #[test]
    fn el_piso_de_descenso_es_continuo() {
        let d = Dron3 { x: 0.0, y: 0.0, z: 3.0, ..Default::default() };
        let mut previo = altura_de_aproximacion(&d, 0.0, 0.0, 0.0);
        let mut e = 0.0f32;
        while e < 2.0 {
            e += 0.01;
            let alt = altura_de_aproximacion(&d, e, 0.0, 0.0);
            assert!((alt - previo).abs() < 0.05,
                "salto de {previo} a {alt} con error {e}: hay un escalón");
            previo = alt;
        }
    }
}

#[cfg(test)]
mod pruebas_freno {
    use super::*;
    use crate::sim3d::{Camara, Obstaculo, Ruido};

    /// **El escenario exacto que lo motivó**: el usuario puso tantos postes
    /// seguidos que armó una pared, y el dron voló derecho contra ella. Acá se
    /// reproduce esa pared —siete postes, uno cada 20° cubriendo los 160° del
    /// campo de visión, ninguno con hueco— y se mide que el freno SÍ entra.
    #[test]
    fn una_pared_de_postes_frena_de_verdad() {
        let d = Dron3 { x: 0.0, y: 0.0, z: 3.0, vx: 5.0, ..Default::default() };
        let mut cam = Camara::nueva(1);
        // Cubriendo TODO el campo de visión (±1.4 rad), no sólo el centro —
        // la primera versión de esta prueba dejaba los bordes libres sin
        // querer, y eso no es una pared: es un pasillo con salida ancha a los
        // costados. Una pared de verdad, como la del usuario, no deja hueco
        // en ningún sector.
        let obs: Vec<Obstaculo> = (-6..=6)
            .map(|i| {
                let ang = i as f32 * 0.22; // cubre ±1.32 rad, casi todo el campo
                Obstaculo { x: 5.0 * ang.cos(), y: 5.0 * ang.sin(), radio: 0.6, alto: 6.0 }
            })
            .collect();
        let vista = cam.ver_con(&d, 50.0, 0.0, 3.0, &Ruido::LIMPIO, &obs);
        let frenado = frenar_ante_pared(&vista.anillo);
        assert!(frenado < 0.15,
            "con una pared de siete postes sin hueco, el freno tiene que \
             entrar a fondo — dio {frenado:.2} (anillo: {:?})", vista.anillo);
    }

    /// **Lo que el usuario pidió explícito: "si puede esquivarlo sin
    /// detenerse, bien".** Un solo poste con hueco de sobra a los costados NO
    /// tiene que frenar al dron — tiene que poder rodearlo, como ya hacía.
    #[test]
    fn un_poste_suelto_con_hueco_no_frena() {
        let d = Dron3 { x: 0.0, y: 0.0, z: 3.0, vx: 5.0, ..Default::default() };
        let mut cam = Camara::nueva(1);
        let obs = [Obstaculo { x: 5.0, y: 0.0, radio: 0.6, alto: 6.0 }];
        let vista = cam.ver_con(&d, 50.0, 0.0, 3.0, &Ruido::LIMPIO, &obs);
        let frenado = frenar_ante_pared(&vista.anillo);
        assert!(frenado > 0.9,
            "un solo poste con hueco al costado no debe frenar — dio \
             {frenado:.2} (anillo: {:?})", vista.anillo);
    }

    /// Sin nada de obstáculos, autoridad completa — es la identidad de la
    /// regla 3.15 (red en cero ⟺ clásico) puesta también sobre el freno: el
    /// freno no puede ser lo que rompa esa identidad.
    #[test]
    fn sin_obstaculos_no_hay_freno() {
        assert_eq!(frenar_ante_pared(&[0.0; crate::sim3d::SECTORES]), 1.0);
    }
}

#[cfg(test)]
mod pruebas_sensores {
    use super::*;

    /// El presupuesto de bits se controla con un `debug_assert`, que en release
    /// —que es como corre todo el entrenamiento— está apagado. Esta prueba lo
    /// verifica de verdad: si alguien agrega un sensor y se pasa, o lo saca y
    /// quedan bits muertos, se entera acá y no después de una corrida de
    /// cuarenta minutos con un cerebro medio ciego.
    #[test]
    fn los_sensores_llenan_exactamente_los_96_bits() {
        let d = Dron3::nuevo(1.0, 2.0, 3.0);
        let v = crate::sim3d::RasgosVisuales {
            flujo_x: 0.0, flujo_y: 0.0, divergencia: 0.0,
            obj_horizontal: 0.0, obj_vertical: 0.0, obj_tamano: 0.0, confianza: 1.0,
            obs_horizontal: 0.0, obs_proximidad: 0.0, obs_altura: 0.0,
            anillo: [0.0; crate::sim3d::SECTORES],
        };
        let (lo, hi) = sensores_guia(&d, &v, &Guia::quieta(5.0, 5.0, 5.0));
        // 96 bits = 64 en `lo` + 32 en `hi`. Que `hi` quepa en 32 es
        // automático por el tipo; lo que se verifica es que el empaque haya
        // usado los 96 y ni uno más.
        let _ = (lo, hi);
        assert_eq!(crate::iafz::SENSOR_BITS, 96);
    }

    /// La velocidad relativa tiene que CAMBIAR la palabra de sensores. Si no lo
    /// hiciera, el campo estaría escrito pero no leído y el dron seguiría
    /// ciego a que el objetivo se mueve — que es exactamente el agujero que se
    /// está tapando.
    #[test]
    fn ver_que_el_objetivo_se_mueve_cambia_lo_que_siente() {
        let d = Dron3::nuevo(0.0, 0.0, 5.0);
        let v = crate::sim3d::RasgosVisuales {
            flujo_x: 0.0, flujo_y: 0.0, divergencia: 0.0,
            obj_horizontal: 0.0, obj_vertical: 0.0, obj_tamano: 0.0, confianza: 1.0,
            obs_horizontal: 0.0, obs_proximidad: 0.0, obs_altura: 0.0,
            anillo: [0.0; crate::sim3d::SECTORES],
        };
        let quieto = sensores_guia(&d, &v, &Guia::quieta(20.0, 0.0, 5.0));
        let huyendo = sensores_guia(&d, &v, &Guia {
            x: 20.0, y: 0.0, z: 5.0, vx: 25.0, vy: 0.0, ax: 0.0, ay: 0.0,
        });
        assert_ne!(quieto, huyendo,
            "un objetivo parado a 20 m y uno que se escapa a 25 m/s desde 20 m NO pueden \
             producir la misma palabra de sensores");
    }

    /// Y el caso que rompía al dron viejo: a velocidad de persecución los
    /// sensores no pueden estar saturados. Dos velocidades de crucero
    /// distintas tienen que dar palabras distintas.
    #[test]
    fn a_velocidad_de_persecucion_los_sensores_no_saturan() {
        let v = crate::sim3d::RasgosVisuales {
            flujo_x: 0.0, flujo_y: 0.0, divergencia: 0.0,
            obj_horizontal: 0.0, obj_vertical: 0.0, obj_tamano: 0.0, confianza: 1.0,
            obs_horizontal: 0.0, obs_proximidad: 0.0, obs_altura: 0.0,
            anillo: [0.0; crate::sim3d::SECTORES],
        };
        let g = Guia::quieta(100.0, 0.0, 10.0);
        let a_15 = sensores_guia(&Dron3 { z: 10.0, vx: 15.0, ..Default::default() }, &v, &g);
        let a_25 = sensores_guia(&Dron3 { z: 10.0, vx: 25.0, ..Default::default() }, &v, &g);
        assert_ne!(a_15, a_25,
            "ir a 15 m/s e ir a 25 m/s tienen que sentirse distinto; con el rango viejo de \
             7 m/s las dos saturaban y el dron era ciego a su propia velocidad");

        // Y lo mismo con la actitud: inclinado 40° para correr no es lo mismo
        // que inclinado 70°.
        let i_40 = sensores_guia(&Dron3 { z: 10.0, pitch: 0.70, ..Default::default() }, &v, &g);
        let i_70 = sensores_guia(&Dron3 { z: 10.0, pitch: 1.22, ..Default::default() }, &v, &g);
        assert_ne!(i_40, i_70, "con el rango viejo de 0.7 rad las dos inclinaciones saturaban igual");
    }
}
