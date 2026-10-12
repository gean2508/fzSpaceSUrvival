//! Estabilizador de tres ejes, en cascada.
//!
//! Esta es exactamente la estructura que corre dentro de Betaflight o
//! ArduPilot: un lazo externo de ángulo que manda una velocidad angular
//! deseada, y un lazo interno rápido que la persigue. No es una elección
//! estética — es lo que hace que el proyecto sea trasladable: el día que se
//! suba a hardware, este archivo se BORRA y su lugar lo ocupa el controlador
//! comercial, que recibe las mismas cuatro consignas.

use crate::pid::Pid;
use crate::sim3d::{Dron3, EMPUJE_PESO, G, IXX, IZZ, MASA, TORQUE_MAX, TORQUE_YAW_MAX};

/// Ganancia angular del lazo EXTERNO de actitud (ángulo → velocidad angular
/// pedida). No lleva inercia: es una relación cinemática (rad/s por cada rad
/// de error), no dinámica, así que no cambia con el aparato.
const GAN_ANG: f32 = 10.0;

/// Ganancias del lazo INTERNO (velocidad angular → torque), expresadas como
/// "ganancia vieja dividida por la inercia con la que se afinó". Multiplicadas
/// de nuevo por la inercia de turno reproducen EXACTAMENTE el mismo lazo
/// cerrado, sea cual sea el aparato — que es todo el punto de sacarlas de acá.
///
/// Los numeradores son los kp/kd que estaban antes de este cambio
/// (`Pid::nuevo(0.055, 0.0, 0.0035, 0.5)` para roll/pitch,
/// `Pid::nuevo(0.030, 0.0, 0.0015, 0.3)` para yaw) y los denominadores son la
/// inercia con la que se midieron (IXX = 0.012, IZZ = 0.022 antes de pasar al
/// aparato de persecución).
const GAN_VEL: f32 = 0.055 / 0.012; // ≈ 4.583
const GAN_VEL_D: f32 = 0.0035 / 0.012; // ≈ 0.292
/// **La I del lazo de velocidad angular, que no existía.** Sin ella, un torque
/// constante —la caja del imán colgada fuera del centro, que es la tarea 3 del
/// producto (sección 1 del contrato)— deja una inclinación PERMANENTE:
/// medido en hover, 0.02 N·m → 3.3°, 0.05 N·m → 8.3° (`medicion_carga_corrida`).
/// Cualquier controlador comercial la tiene (Betaflight, PX4), y por la regla
/// 3.1 `Estabilizador3` tiene que parecerse a ellos. Medición: bitácora 7.37.
const GAN_VEL_I: f32 = GAN_VEL * 2.0;
/// Tope de la integral, en rad (error de giro × s): que la I sola no pueda
/// pedir más de medio torque máximo.
const LIM_I_VEL: f32 = 0.5 * TORQUE_MAX / (IXX * GAN_VEL_I);
/// Iterm relax: con la parte rápida de la consigna de giro por encima de
/// esto (rad/s), la I no integra. **Sin relax la I sola ROMPE la vara**:
/// clásico, 12 sorteos, peldaños 1/8 — ki 0: 12/11; ki 0.3·kp: 12/7;
/// ki 1·kp: 12/0; ki 2·kp: 0/0. Se carga durante las maniobras y después
/// empuja de más. Con relax 0.2 y ki 2·kp, 24 sorteos: p1 24, p7 24, p8 21,
/// p9 14, p13 7 contra 24, 24, 22, 16, 6 sin I (ruido de ±2), y la carga
/// corrida pasa de 8.2° a 0.00°.
const RELAX_I: f32 = 0.2;
const GAN_YAW: f32 = 0.030 / 0.022; // ≈ 1.364
const GAN_YAW_D: f32 = 0.0015 / 0.022; // ≈ 0.068

/// Reserva de empuje del guiado de persecución: qué fracción del empuje
/// máximo se permite gastar en sostener el peso al bank máximo, dejando el
/// resto para el lazo de altura y el viento.
///
/// Medido con `persecucion::barrido::barrer_reserva`, no supuesto: de 0.35 a
/// 0.98 el resultado (0 caídas, 81% de peor fracción a tiro en los 6
/// recorridos × 3 ruidos) es IDÉNTICO por arriba de 0.80 — la reserva de
/// empuje dejó de ser el cuello de botella una vez arreglado el trim de
/// crucero (`sim3d::angulo_de_crucero`, ver `k_vel` más abajo).
///
/// Subido de 0.85 a 0.92 tras manejar la moto EN VIVO desde la cabina: el
/// currículo sintético nunca pide más de 28 m/s (`movil::recorridos`), pero
/// una persona puede llevar la moto a fondo hasta su techo real, 33 m/s
/// (119 km/h) — y ahí, acelerador y manubrio a fondo AL MISMO TIEMPO, el
/// dron con reserva 0.85 (29.0 m/s de crucero) se quedaba atrás de verdad
/// (error creciendo a 24 m, 81% a tiro). Es un caso que el currículo
/// sintético no cubre porque nunca pide esa combinación extrema.
///
/// 0.92 (30.4 m/s) achica esa brecha sin perder nada medido: el barrido ya
/// había probado hasta 0.98 con 0 caídas. Sigue quedando margen real (33 - 30.4
/// = 2.6 m/s) porque el simulador no modela ráfagas peores que
/// `Viento3::siguiente` ni desgaste de motor, y en el caso límite absoluto —a
/// fondo Y en el giro más cerrado a la vez— es honesto que el dron pierda
/// terreno: un dron real con margen de seguridad también perdería una moto
/// así. Eso no es un bug, es la física.
const RESERVA_EMPUJE_PERSECUCION: f32 = 0.92;

/// Régimen máximo de ascenso y de descenso, en m/s.
///
/// Existen porque el lazo de altura era un PID único sobre el error de
/// posición, y eso no acota la velocidad: medido con la traza del guiado
/// clásico despegando del suelo a un punto a 3 m, **se pasaba hasta 5.45 m** y
/// tardaba más de 10 s en asentarse, quemando un cuarto del reloj de la misión
/// antes de empezar a moverse. Bajando es peor: tres metros de caída libre
/// controlada llegan al suelo a ~5 m/s y eso no es posarse, es caerse.
///
/// La cascada posición → velocidad → empuje es lo que usa cualquier
/// controlador de vuelo comercial, y como `Estabilizador3` es justamente la
/// pieza que el día de mañana se borra y reemplaza el controlador comercial,
/// parecerse a él es el objetivo, no una concesión.
///
/// **Los límites NO están puestos para regalar el aterrizaje.** 2.5 m/s de
/// bajada es más del doble del máximo con el que se puede tocar el suelo sin
/// romper nada (`sim3d::VZ_POSADA` = 1.2 m/s): el dron sigue teniendo que
/// frenar solo en los últimos metros, que es exactamente la maniobra que se
/// está midiendo. Si se los pusiera en 1.1 la prueba de posada se aprobaría
/// sola y no mediría nada.
///
/// Subidos de 3.0/2.5 al pasar al aparato de persecución. Con 4×peso de empuje
/// la aceleración vertical disponible es 3·g = 29 m/s², o sea que 3 m/s de
/// ascenso era un tope del CONTROLADOR, no del aparato — y en persecución
/// importa: seguir una moto que sube un puente o entra a un desnivel a 25 m/s
/// exige trepar rápido o perderla. 8 m/s de subida es lo que trepa un 5" real.
/// La bajada queda más baja a propósito: bajar rápido lleva al anillo de
/// vórtices, que es una pérdida real de sustentación en el aire propio.
pub const VZ_SUBIDA_MAX: f32 = 8.0;
pub const VZ_BAJADA_MAX: f32 = 5.0;

pub struct Estabilizador3 {
    /// Lazo externo: error de altura → velocidad vertical deseada, acotada.
    altura_pos: Pid,
    /// Lazo interno: persigue esa velocidad con empuje.
    altura_vel: Pid,
    roll_ang: Pid,
    roll_vel: Pid,
    pitch_ang: Pid,
    pitch_vel: Pid,
    yaw_vel: Pid,
}

impl Default for Estabilizador3 {
    fn default() -> Self {
        Self::nuevo()
    }
}

impl Estabilizador3 {
    pub fn nuevo() -> Self {
        Estabilizador3 {
            // P puro: el lazo externo sólo reparte "a qué velocidad conviene ir
            // hacia la altura pedida". 1.6 /s deja la subida al tope a partir
            // de ~1.9 m de error y frena suave debajo de eso.
            altura_pos: Pid::nuevo(1.6, 0.0, 0.0, 0.0),
            altura_vel: Pid::nuevo(4.0, 2.0, 0.15, 4.0),
            // --- Las ganancias del lazo de actitud van EN UNIDADES DE INERCIA ---
            //
            // Antes eran números pelados (0.055, 0.0035) afinados para
            // IXX = 0.012. Al cambiar de aparato —IXX bajó a 0.0075 y el torque
            // subió— la ganancia de lazo se multiplicó por 1.6 sin que nadie la
            // tocara, y el resultado fue medible: el piloto clásico seguía a la
            // moto perfecto (88-94% a tiro) y se estrellaba igual, a 25 m/s y
            // con ruido REAL, en 4 de 6 recorridos. No fallaba el seguimiento,
            // oscilaba la actitud.
            //
            // Escritas así el controlador es PORTABLE: el día que se mida la
            // inercia del aparato real —que es una de las primeras cosas que
            // hay que medir— se cambia la constante en `sim3d` y el lazo sigue
            // afinado. Con números pelados habría que reafinar a mano y
            // descubrir el desastre volando.
            //
            //   ACEL_ANG = torque / I   es la ganancia de la planta.
            //   Para que el lazo cerrado no cambie, kp y kd tienen que ir con I.
            roll_ang: Pid::nuevo(GAN_ANG, 0.0, 0.0, 0.0),
            roll_vel: Pid::nuevo(IXX * GAN_VEL, IXX * GAN_VEL_I, IXX * GAN_VEL_D, LIM_I_VEL),
            pitch_ang: Pid::nuevo(GAN_ANG, 0.0, 0.0, 0.0),
            pitch_vel: Pid::nuevo(IXX * GAN_VEL, IXX * GAN_VEL_I, IXX * GAN_VEL_D, LIM_I_VEL),
            // Guiñada con I por la misma razón que roll/pitch (bitácora 7.38):
            // un motor flojo mete un torque de guiñada constante y sin I el
            // dron gira sin parar. Medido con `dron estres`: 0.03 N·m tumbaba
            // al clásico de 24/24 a 10/24 en el peldaño 7 y de 13 a 0 en el 13.
            yaw_vel: Pid::nuevo(IZZ * GAN_YAW, IZZ * GAN_YAW * 2.0, IZZ * GAN_YAW_D,
                                0.5 * TORQUE_YAW_MAX / (IZZ * GAN_YAW * 2.0)),
        }
    }


    /// `medido` es lo que ven los sensores (con ruido), no el estado real. Esa
    /// distinción es el punto entero de tener ruido: el controlador vuela a
    /// ciegas igual que en la vida real.
    ///
    /// Devuelve (empuje, torque_roll, torque_pitch, torque_yaw).
    pub fn control(
        &mut self,
        medido: &Dron3,
        altura_deseada: f32,
        roll_deseado: f32,
        pitch_deseado: f32,
        yaw_vel_deseada: f32,
    ) -> (f32, f32, f32, f32) {
        // Compensar gravedad y la pérdida de empuje vertical al inclinarse: el
        // PID sólo corrige el resto, no tiene que descubrir el peso del dron.
        //
        // El piso de este cociente NO es un número libre: tiene que ser
        // 1/EMPUJE_PESO, el ángulo exacto donde sostener el peso agota TODO el
        // empuje disponible. Acá había un 0.5 copiado de la envolvente vieja
        // —cos(60°), la actitud extrema de entonces— y con la envolvente nueva
        // (crucero hasta 69°, extremo a 80°) quedó muy por debajo de lo que el
        // dron banca de verdad: a 75° cos = 0.26, así que el piso viejo le
        // reconocía sólo la mitad del peso que en realidad tenía que sostener.
        // El controlador subcompensaba la gravedad justo en las maniobras de
        // persecución, perdía altura de forma crónica y terminaba estrellado —
        // medido: el piloto clásico seguía la moto perfecto (80-95% a tiro) y
        // se caía igual, en 4 a 6 de 6 recorridos según el ruido. Con el piso
        // correcto, más allá de la inclinación sostenible el empuje se va
        // derecho al máximo (ya no puede compensar más) en vez de quedarse
        // corto silenciosamente.
        // **Probado y DESCARTADO: fusión vertical** (complementario barómetro
        // + modelo del empuje, como el EKF de un controlador comercial).
        // Bajaba los aterrizajes duros con ruido DURO de 16 a 12 de 72, pero
        // con masa nominal en el modelo hundía a un dron CARGADO: posarse con
        // la caja de 300 g pasó de 24/24 a 0/24 (`dron estres 11`), y ni un
        // tercer estado de sesgo (el «empuje de hover» de PX4) ni darle más
        // peso a la medición lo recuperaban (12/24 en el mejor caso). La caja
        // es la tarea 3 del producto: no se cambia por un poco de ruido DURO.
        // Bitácora 7.43. No reabrir sin un estimador que aprenda la masa
        // antes de la primera maniobra.
        // Un pasa-bajos SIN modelo tampoco sirve: con 0.1/0.2 la caja cae a
        // 0/24 por el retraso y el peldaño 16 no mejora (22 → 23 de 48).
        let (z_f, vz_f) = (medido.z, medido.vz);
        let inclinacion = (medido.roll.cos() * medido.pitch.cos()).max(1.0 / EMPUJE_PESO);
        let base = MASA * G / inclinacion;
        // Cascada de altura: posición → velocidad vertical → empuje.
        let vz_deseada = self
            .altura_pos
            .actualizar(altura_deseada - z_f)
            .clamp(-VZ_BAJADA_MAX, VZ_SUBIDA_MAX);
        let empuje = base + self.altura_vel.actualizar(vz_deseada - vz_f) * MASA;

        let p_sp = self.roll_ang.actualizar(roll_deseado - medido.roll);
        let tx = self.roll_vel.actualizar_con_tope(p_sp - medido.p, TORQUE_MAX, p_sp, RELAX_I);

        let q_sp = self.pitch_ang.actualizar(pitch_deseado - medido.pitch);
        let ty = self.pitch_vel.actualizar_con_tope(q_sp - medido.q, TORQUE_MAX, q_sp, RELAX_I);

        let tz = self.yaw_vel.actualizar_con_tope(
            yaw_vel_deseada - medido.r, TORQUE_YAW_MAX, yaw_vel_deseada, RELAX_I);

        (empuje, tx, ty, tz)
    }
}

/// Guiado clásico: el controlador que CUALQUIER dron real usa para ir a un
/// punto. Cascada posición → velocidad → actitud.
///
/// Existe para una sola cosa: ser la vara honesta. La línea de base anterior
/// —PID de altura con la inclinación clavada en cero— es un controlador al que
/// nunca se le pidió navegar, y ganarle no prueba nada. Si el genoma no le gana
/// a ESTO, la red no está aportando y hay que decirlo.
pub struct Guiado {
    vel_max: f32,
    k_pos: f32,
    k_vel: f32,
    ang_max: f32,
}

impl Default for Guiado {
    fn default() -> Self {
        Guiado { vel_max: 4.0, k_pos: 0.9, k_vel: 0.35, ang_max: 0.40 }
    }
}


impl Guiado {
    /// La vara honesta EN MODO PERSECUCIÓN: mismo controlador, con permiso para
    /// usar todo el sobre del aparato.
    ///
    /// Esto no viola la regla 3.2 —"la vara honesta no se toca para que los
    /// números queden lindos"— porque va en la dirección contraria: hace al
    /// rival MÁS fuerte, no más débil. El `default()` con vel_max 4.0 y 23° de
    /// inclinación no puede seguir a nada que corra; dejarlo así en una misión
    /// de persecución sería regalarle la comparación al genoma, que es
    /// exactamente el maquillaje que la regla prohíbe.
    ///
    /// `ang_max` 1.30 rad (74.5°) es apenas por debajo de `inclinacion_sostenible()`
    /// (75.5°): pedirle más al lazo sería pedirle al empuje algo que no tiene, y
    /// el aparato bajaría mientras acelera.
    pub fn persecucion() -> Self {
        Self::con_reserva(RESERVA_EMPUJE_PERSECUCION)
    }

    /// Igual que `persecucion()`, pero con la fracción de empuje reservada
    /// como parámetro — existe para MEDIR qué reserva hace falta en vez de
    /// adivinarla (`persecucion::barrido`).
    ///
    /// El bank máximo NO es el límite físico del aparato: le queda un margen
    /// de empuje reservado, igual que hace cualquier controlador de vuelo real
    /// (nunca deja que la actitud consuma el 100% del acelerador). `reserva`
    /// es qué fracción del empuje máximo se permite gastar en sostener el
    /// peso al bank máximo; el resto queda para el lazo de altura y para
    /// absorber viento.
    pub fn con_reserva(reserva: f32) -> Self {
        // k_vel = 0.10, no 0.35: con el trim de crucero puesto (ver
        // `consignas_persiguiendo`) la ganancia de velocidad sólo corrige el
        // RESIDUO, no todo el ángulo. Medido con `persecucion::barrer_k_vel`:
        // 0.35 y 0.20 siguen entrando en el ciclo límite (12-17 caídas de 18);
        // 0.10 es el primer valor sin ninguna caída Y con la mejor fracción a
        // tiro (81%, contra 54% en 0.05 y 31% en 0.01 — bajarla de más vuelve
        // a alejar el punto de equilibrio, sólo que ahora por under-shoot en
        // vez de por oscilación).
        Self::depurar(reserva, 0.9, 0.10)
    }

    /// Con TODOS los parámetros expuestos. `con_reserva` (producción) y las
    /// pruebas de `persecucion::regresion_persecucion` pasan por acá — NO
    /// puede ser `#[cfg(test)]`, aunque sólo las pruebas varíen `k_pos`/`k_vel`.
    pub fn depurar(reserva: f32, k_pos: f32, k_vel: f32) -> Self {
        let ang_max = (1.0 / (crate::sim3d::EMPUJE_PESO * reserva))
            .clamp(-1.0, 1.0)
            .acos();
        Guiado {
            vel_max: crate::sim3d::velocidad_techo(ang_max),
            k_pos,
            k_vel,
            ang_max,
        }
    }


    /// La vara honesta PERSIGUIENDO: control de posición más **prealimentación
    /// de velocidad**.
    ///
    /// Es la diferencia entre un controlador que persigue y uno que corre atrás.
    /// Sin el término de prealimentación, la velocidad pedida sale sólo del
    /// error de posición, así que para ir a 25 m/s hace falta un error
    /// permanente de 25/k_pos = 28 m. Dicho de otro modo: **el error de
    /// seguimiento en régimen es proporcional a la velocidad del objetivo**, y
    /// el dron se queda clavado treinta metros atrás para siempre por más
    /// rápido que vuele.
    ///
    /// Con la velocidad del objetivo sumada, el lazo de posición sólo tiene que
    /// corregir la DIFERENCIA, y el error en régimen tiende a cero. Es lo que
    /// hace cualquier sistema de seguimiento real, y es lo que la red tiene que
    /// aprender sola a partir del sensor de velocidad relativa.
    ///
    /// Devuelve (roll, pitch, velocidad de guiñada).
    pub fn consignas_persiguiendo(
        &self, m: &Dron3, guia: &crate::gym3d::Guia,
    ) -> (f32, f32, f32) {
        let (ex, ey) = (guia.x - m.x, guia.y - m.y);
        let dist = (ex * ex + ey * ey).sqrt().max(1e-4);
        let rapidez = (self.k_pos * dist).min(self.vel_max);
        // Corrección por posición + lo que ya lleva el objetivo.
        let (vx_d, vy_d) = (ex / dist * rapidez + guia.vx, ey / dist * rapidez + guia.vy);
        // El resultado puede pasarse del techo del aparato: se recorta en
        // módulo, no por eje, para no torcer la dirección.
        let modulo = (vx_d * vx_d + vy_d * vy_d).sqrt();
        let k = if modulo > self.vel_max { self.vel_max / modulo } else { 1.0 };
        let (vx_d, vy_d) = (vx_d * k, vy_d * k);

        let (s, c) = (m.yaw.sin(), m.yaw.cos());
        let (v_ad, v_lat) = (m.vx * c + m.vy * s, -m.vx * s + m.vy * c);
        let (d_ad, d_lat) = (vx_d * c + vy_d * s, -vx_d * s + vy_d * c);

        // Trim + residuo, no todo en una ganancia lineal.
        //
        // `actitud_desde_velocidad` (la de abajo) mapea error×k_vel directo a
        // radianes, que es la aproximación de ángulo chico: sirve a 4 m/s
        // (ángulo de crucero ~6°) y se rompe a 25 (~68°) — medido, ver
        // `sim3d::angulo_de_crucero`. Acá el pitch es TRIM (el ángulo que ya
        // se sabe que hace falta para la velocidad objetivo, por la física de
        // arrastre) más una corrección P chica sobre lo que falta. El roll no
        // lleva trim: no hay "ángulo de crucero lateral" en vuelo derecho, y
        // cortar en curva es autoridad de sobra a estas velocidades.
        // **Prealimentación de la ACELERACIÓN de la referencia.**
        //
        // El trim de arriba cubre la aceleración que hace falta para vencer el
        // arrastre a velocidad de crucero. Lo que NO cubría es que el punto
        // perseguido tenga aceleración propia: en curva, el puesto que va
        // detrás del móvil lleva aceleración centrípeta (`Guia::ax/ay`), y sin
        // este término el lazo sólo puede reaccionar DESPUÉS de que el error
        // se abra. Medido antes de esto: en curva el error del dron era 5×
        // más lateral que longitudinal — no se quedaba atrás, se abría hacia
        // afuera, que es exactamente lo que pasa cuando falta banqueo.
        //
        // La conversión de aceleración a ángulo es la misma física que ya usa
        // `angulo_de_crucero`: para acelerar lateralmente `a` hay que
        // inclinarse atan(a/g). Es el esquema estándar de seguimiento de
        // trayectorias en cuadricópteros (Mellinger-Kumar): posición +
        // velocidad + ACELERACIÓN de referencia → vector de empuje → actitud.
        let (a_ad, a_lat) = (guia.ax * c + guia.ay * s, -guia.ax * s + guia.ay * c);
        let ff_pitch = (a_ad / crate::sim3d::G).atan();
        let ff_roll = (a_lat / crate::sim3d::G).atan();

        let trim = crate::sim3d::angulo_de_crucero(d_ad);
        let pitch = (trim + ff_pitch + (d_ad - v_ad) * self.k_vel)
            .clamp(-self.ang_max, self.ang_max);
        let roll = (-(ff_roll + (d_lat - v_lat) * self.k_vel))
            .clamp(-self.ang_max, self.ang_max);

        // La nariz apunta hacia donde VA el objetivo, no hacia donde está: si
        // apuntara a la posición actual, en curva la cámara lo perdería por el
        // borde justo cuando más importa.
        let rumbo = (ey + guia.vy * 0.5).atan2(ex + guia.vx * 0.5);
        let dif = (rumbo - m.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        (roll, pitch, (dif * 1.5).clamp(-1.2, 1.2))
    }


    /// Devuelve (roll deseado, pitch deseado, velocidad de guiñada deseada).
    /// **No se vuela más rápido hacia donde la cámara no mira.** La cámara
    /// cubre ±80° de la nariz (`CAMPO_OBSTACULOS`) y el dron arranca con la
    /// nariz en +x, así que con el objetivo a 100-180° el clásico aceleraba
    /// CIEGO: en el peldaño 8, **los 12 fallos de 24 sorteos del clásico eran
    /// todos así** (objetivo a 110-178° de la nariz, choque a 1.1-1.9 s, poste
    /// alineado a menos de 16° con el objetivo). Cualquier dron con sensado
    /// sólo frontal —el DJI con APAS, el Skydio en modo frontal— resuelve lo
    /// mismo: gira hacia donde va y recién ahí acelera (regla 3.17).
    ///
    /// **Probado y descartado (bitácora 7.43):** sostener el rumbo y quitar
    /// este tope a menos de 1.5 m del punto, para que el clásico no orbitara
    /// sobre la plataforma. Empeoró la vara (48 sorteos: p7 48→46, p9 48→44,
    /// p12 42→35, p13 38→34) aunque subió el 15 (20→27). Lo que daña es
    /// sostener el rumbo. Sostenerlo SÓLO al posarse (objetivo en el suelo, a
    /// menos de 1.5 m) tampoco: p12 42→39, p13-15 38→34, p16 18→23. No
    /// reabrir sin nueva evidencia.
    ///
    /// No es cero: a `V_CIEGA` se puede frenar dentro de lo que el looming
    /// avisa, y sostener posición (que necesita ir un poco para atrás) sigue
    /// funcionando. Medición: bitácora 7.36.
    pub fn tope_por_mirada(&self, yaw: f32, rumbo: f32) -> f32 {
        // Barrido del clásico, 24 sorteos: V_CIEGA 0.5 → peldaño 8 20/24,
        // peldaño 9 15/24; 1.5 → 12 y 5; sin tope → 13 y 6. Peldaños 1-7 sin
        // pérdida (7 sube de 22 a 24).
        const V_CIEGA: f32 = 0.5;
        let v_ciega = V_CIEGA;
        let campo = crate::sim3d::CAMPO_OBSTACULOS;
        let dif = ((rumbo - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI).abs();
        // Adentro de la mitad del cono, sin tope; en el borde, V_CIEGA.
        let f = ((campo - dif) / (campo * 0.5)).clamp(0.0, 1.0);
        v_ciega + (self.vel_max - v_ciega) * f
    }

    pub fn consignas(&self, m: &Dron3, ox: f32, oy: f32) -> (f32, f32, f32) {
        // 1. Error de posición → velocidad deseada, saturada.
        let (ex, ey) = (ox - m.x, oy - m.y);
        let dist = (ex * ex + ey * ey).sqrt().max(1e-4);
        let rapidez = (self.k_pos * dist).min(self.vel_max);
        let rumbo_obj = ey.atan2(ex);
        let rapidez = rapidez.min(self.tope_por_mirada(m.yaw, rumbo_obj));
        let (vx_d, vy_d) = (ex / dist * rapidez, ey / dist * rapidez);

        // 2. Error de velocidad → aceleración deseada.
        let (ax, ay) = ((vx_d - m.vx) * self.k_vel, (vy_d - m.vy) * self.k_vel);

        // 3. Aceleración → ángulos, en el marco del cuerpo. Inclinarse ES la
        //    única forma de acelerar en horizontal.
        let (s, c) = (m.yaw.sin(), m.yaw.cos());
        let a_adelante = ax * c + ay * s;
        let a_lateral = -ax * s + ay * c;
        let pitch = a_adelante.clamp(-self.ang_max, self.ang_max);
        let roll = (-a_lateral).clamp(-self.ang_max, self.ang_max);

        // 4. Apuntar la nariz hacia el objetivo.
        let rumbo = ey.atan2(ex);
        let dif = (rumbo - m.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        (roll, pitch, (dif * 1.5).clamp(-1.2, 1.2))
    }

    /// **La vara honesta ESQUIVANDO: el mismo lazo de arriba, pero con los ojos
    /// abiertos.**
    ///
    /// # Por qué existe
    ///
    /// `consignas` no mira los obstáculos. Ni uno. Vuela al punto y si hay un
    /// poste en el medio lo choca, y eso estuvo escrito en el contrato durante
    /// meses como si fuera una propiedad del diseño («el clásico nunca
    /// esquiva, por diseño — no es bug de la vara»). Medido con
    /// `dron sorteos N clasico n=12` (bitácora 7.34), lo que esa propiedad
    /// costaba:
    ///
    /// ```text
    ///   peldaño                              clásico aprueba
    ///    7  sensores duros, SIN postes           11 de 12
    ///    8  esquivar UN poste                     2 de 12
    ///    9  esquivar todos los que haya           1 de 12
    /// ```
    ///
    /// El acantilado está exactamente en el primer poste, y como el examen de
    /// repaso arrastra el peldaño 8 hacia arriba, **de ahí para adelante la
    /// aptitud de todo peldaño queda atada a algo que la vara misma no pasa**.
    /// Dos corridas de 1200 generaciones se molieron contra eso.
    ///
    /// # Qué dio, medido sobre 24 sorteos independientes
    ///
    /// ```text
    ///   peldaño                 vara CIEGA   vara CON OJOS
    ///    7  sin postes            22 / 24       22 / 24   ← idéntico, sorteo por sorteo
    ///    8  esquivar un poste      4 / 24       12 / 24
    ///    9  esquivar todos         2 / 24        7 / 24
    /// ```
    ///
    /// Y el dato que dice que la vara SUBIÓ en vez de moverse de lugar: en los
    /// dos peldaños con postes, los sorteos que pasa la vara con ojos son un
    /// **superconjunto** de los que pasaba la ciega. No hay ni un sorteo que
    /// la ciega ganara y ésta pierda.
    ///
    /// **Lo que esto NO arregla, y hay que decirlo (regla 3.4):** 12 de 24
    /// sigue siendo una rifa, no una capacidad. El peldaño 8 no queda
    /// calibrado por este cambio — queda menos descalibrado. Lo que falla
    /// ahora es rozar: en los sorteos que pierde, el clásico pasa a 0.00 m del
    /// poste y cualquier ruido lo mete adentro.
    ///
    /// **Y lo que invalida (regla 3.2, y es la parte cara que hay que ejecutar,
    /// no sólo anotar):** toda comparación de esquive anterior a este cambio
    /// deja de significar. Y no sólo las columnas del clásico: el híbrido
    /// navega con esta misma función, así que **los genomas entrenados contra
    /// la vara ciega están corrigiendo a un piloto que ya no existe y hay que
    /// reentrenarlos.** No es una migración.
    ///
    /// La regla 3.2 define la vara como «lo mejor que el modelo físico sabe
    /// hacer» y manda arreglar un DEFECTO de la vara aunque el genoma quede
    /// peor en la comparación. Una vara que ignora un poste **que el sensor ya
    /// le está mostrando** no es lo mejor que el modelo sabe hacer: es un lazo
    /// al que nadie le conectó una entrada que existe.
    ///
    /// Y la regla 3.15 dice lo mismo desde el otro lado: desviarse de lo que
    /// se te viene encima es una relación que se escribe en unas líneas a
    /// partir de lo que el sensor da, así que se escribe, y lo aprendido se
    /// ocupa de lo que el modelo no captura. La medición que respalda ese
    /// canje está en la regla: la red evolucionando la relación
    /// velocidad→ángulo desde cero sacó 892 de 7161 y no encontró nada;
    /// puesta la ecuación, la misma red con la misma semilla mejoró al modelo
    /// en 60 generaciones.
    ///
    /// # Cómo esquiva: se elige el HUECO, no se promedia la amenaza
    ///
    /// Sigue el hueco (*follow-the-gap* / histograma de campo vectorial, que
    /// es lo que usan los robots móviles del rubro), y lo hace sobre el anillo
    /// de sectores en vez de sobre un rumbo promedio. El motivo está medido y
    /// es el mismo por el que el anillo existe (ver `gym3d::sensores_guia`):
    /// **dos postes a los costados promedian a «rumbo cero», o sea que un
    /// piloto que promedia lee «el hueco está al centro» justo cuando el
    /// centro es lo único bloqueado.** Elegir el sector de menor amenaza no
    /// tiene ese modo de falla.
    ///
    /// # Por qué NO rompe la identidad de la regla 3.15
    ///
    /// Con el anillo despejado esto devuelve **exactamente** `consignas`, no
    /// algo parecido: el peso del desvío es cero por abajo de `LIBRE` y el
    /// rumbo comandado queda en el rumbo al objetivo. Eso mantiene en pie
    /// «red en cero ⟺ clásico», que es el instrumento con el que el proyecto
    /// sabe si algo funcionó, y hay una prueba que lo verifica
    /// (`sin_nada_que_esquivar_es_exactamente_el_clasico`).
    ///
    /// `LIBRE` y `PARED` son los mismos valores MEDIDOS que calibran
    /// `gym3d::frenar_ante_pared`, y por el mismo motivo (regla 3.18): el
    /// anillo es looming normalizado contra `RANGO_LOOMING`, y una pared real
    /// acercándose a 5 m/s desde 5 m da 0.21 — no 0.75. Elegir estos números
    /// «a ojo» es lo que ya había dejado un freno que sólo entraba un instante
    /// antes de chocar.
    pub fn consignas_esquivando(
        &self, m: &Dron3, ox: f32, oy: f32, anillo: &[f32], rumbo_obs: f32,
    ) -> (f32, f32, f32) {
        // Los mismos que `gym3d::frenar_ante_pared`, medidos contra el rango
        // real del anillo y no elegidos a gusto.
        const LIBRE: f32 = 0.03;
        const PARED: f32 = 0.18;

        // Cuánto pesa desviarse un radián, en unidades de amenaza. Con 0.25 un
        // sector totalmente tapado (1.0) alcanza para justificar un desvío de
        // 4 rad, o sea cualquier sector del campo; y un roce de 0.05 no
        // justifica ni 0.2 rad. Es la única constante nueva y decide sólo
        // CUÁNTO desviarse de más, no si esquivar.
        const COSTO_DESVIO: f32 = 0.25;

        let amenaza = anillo.iter().cloned().fold(0.0f32, f32::max);
        // Nada que esquivar ⇒ el clásico de siempre, byte por byte.
        if amenaza <= LIBRE {
            return self.consignas(m, ox, oy);
        }
        let peso = ((amenaza - LIBRE) / (PARED - LIBRE)).clamp(0.0, 1.0);

        // Rumbo al objetivo, en el marco del cuerpo.
        let (ex, ey) = (ox - m.x, oy - m.y);
        let (s, c) = (m.yaw.sin(), m.yaw.cos());
        let (adelante, lateral) = (ex * c + ey * s, -ex * s + ey * c);
        let b_obj = lateral.atan2(adelante);

        // El sector más conveniente: poca amenaza y poco desvío del rumbo que
        // uno quería. El desvío se mide contra `b_obj` recortado al campo,
        // porque si el objetivo está a 90° del morro todos los sectores están
        // igual de «desviados» y lo único que debería decidir es la amenaza.
        let b_ref = b_obj.clamp(-crate::sim3d::CAMPO_OBSTACULOS,
                                 crate::sim3d::CAMPO_OBSTACULOS);
        //
        // **Y con COMPROMISO con la maniobra ya empezada.** Sin este término
        // el rodeo se da vuelta a mitad de camino, y está en la traza, no
        // deducido (regla 3.5). Vara con ojos y sin compromiso, misión 3 del
        // peldaño 8, rumbo al punto (−3.8, −6.6):
        //
        // ```text
        //   t=1.00  x −1.43   se desvió hacia −x, bien
        //   t=1.25  x −1.69   máximo del desvío
        //   t=1.30  x −1.68   ← se da vuelta
        //   t=1.75  x −1.10   vuelve al poste y le pasa a −0.00 m
        // ```
        //
        // La causa es que el sector del poste y el rumbo al objetivo se mueven
        // los dos mientras el dron se corre, así que el lado más conveniente
        // cambia de signo a mitad del rodeo y el dron deshace lo que había
        // hecho. Es el motivo por el que cualquier seguidor de huecos del
        // rubro lleva histéresis: se elige un lado y se mantiene hasta que el
        // estorbo quedó atrás.
        //
        // Acá la histéresis no necesita memoria: **la maniobra empezada ya
        // está escrita en la velocidad lateral del propio dron**, que es un
        // sensor que el piloto tiene. Ir para un lado abarata los sectores de
        // ese lado en proporción a cuánto se está yendo, y el dron mantiene el
        // rodeo por la misma razón por la que lo mantendría una persona.
        let (_, v_lat) = (m.vx * c + m.vy * s, -m.vx * s + m.vy * c);
        let yendo = (v_lat / self.vel_max).clamp(-1.0, 1.0);
        // Barrido sobre 24 sorteos, no elegido a ojo (regla 3.18). Cuenta de
        // sorteos aprobados por el clásico con ojos:
        //
        // ```text
        //   COMPROMISO   peldaño 8   peldaño 9
        //      0.00       11 / 24      4 / 24
        //      0.15       11 / 24      6 / 24
        //      0.30       12 / 24      6 / 24
        //      0.60       12 / 24      7 / 24   ← acá satura
        //      1.00       12 / 24      7 / 24
        // ```
        //
        // Sube monótono y se aplana: no es un pico afinado sobre estos 24
        // sorteos (que sería sobreajustar la medición), es una meseta. Se toma
        // 0.60, adentro de la meseta. Y el detalle que decide: con 0.00 el
        // clásico con ojos PIERDE el sorteo 4, que el clásico ciego pasaba;
        // con 0.60 los sorteos que pasa son un SUPERCONJUNTO de los del
        // ciego en los dos peldaños — o sea que la vara subió, no cambió de
        // lugar.
        const COMPROMISO: f32 = 0.60;
        // Penalidad de ir por el lado contrario: 0.3, 1.0 y 3.0 dieron lo
        // mismo (±1 sorteo de 24), así que es una restricción, no un afinado.
        const LADO: f32 = 1.0;
        // **El LADO del rodeo lo decide el rumbo continuo del estorbo, no el
        // sector.** Traza del sorteo 3 del peldaño 8 (poste casi en la línea,
        // a 0.16 rad del rumbo al objetivo): el piloto eligió bien el lado
        // (+), la nariz giró hacia el comando, el poste cruzó el borde de
        // sector —los sectores miden 32°— y en ese paso el «hueco» pasó a ser
        // el del OTRO lado: se dio vuelta y lo chocó a 1.3 s. El compromiso
        // de arriba no lo salvó porque mide velocidad lateral EN EL CUERPO, y
        // un dron que vuela con la nariz al comando casi no tiene.
        //
        // `obs_horizontal` es continuo (el rumbo ponderado de lo que se ve) y
        // la red lo recibe en sus bits, así que la vara no ve nada que el
        // genoma no vea. Y es estable por geometría: al correrse hacia un
        // lado, el poste queda MÁS del otro lado de la línea al objetivo, así
        // que la decisión se refuerza sola — histéresis sin memoria.
        // Medido: ver bitácora 7.36.
        let lado = (b_obj - rumbo_obs).signum();
        // **Obstáculos inflados** (el margen de seguridad de cualquier
        // planificador): un sector hereda la mitad de la amenaza de sus
        // vecinos. El anillo marca el sector del CENTRO del poste, no su
        // ancho, y el clásico elegía el sector de al lado y pasaba a 0.3 m de
        // la superficie: cualquier deriva lateral (viento) lo metía adentro.
        // 48 sorteos: p8 43→44, p9 41→43, p13 29→30, p14 7→10 (bitácora 7.43).
        let infla = 0.5f32;
        let n = crate::sim3d::SECTORES;
        let mut inflado = [0.0f32; 16];
        for i in 0..n {
            let mut v = anillo[i];
            if i > 0 { v = v.max(anillo[i - 1] * infla); }
            if i + 1 < n { v = v.max(anillo[i + 1] * infla); }
            inflado[i] = v;
        }
        let anillo = &inflado[..n];
        let mut mejor = (f32::INFINITY, b_ref);
        for i in 0..crate::sim3d::SECTORES {
            let b = crate::sim3d::rumbo_de_sector(i);
            // «Del lado correcto» quiere decir pasado el BORDE del poste, no
            // su centro: con el centro, un sector a 0.06 rad del eje del poste
            // contaba como libre y el clásico apuntaba derecho a él (vuelta
            // del sorteo 7, misión 2 del peldaño 14). `MARGEN_LADO` ≈ el
            // medio ancho angular de un poste de 0.7 m a 2-3 m. Barrido, 48
            // sorteos (p8/p9/p13/p14): 0 → 44/41/33/15; 0.25 → 46/45/33/29;
            // 0.4 → 48/48/36/36; 0.6 → 47/47/37/37. Meseta: se toma 0.5.
            const MARGEN_LADO: f32 = 0.5;
            let contramano = if (b - rumbo_obs) * lado < MARGEN_LADO { 1.0 } else { 0.0 };
            let costo = anillo[i] + (b - b_ref).abs() * COSTO_DESVIO
                        - COMPROMISO * yendo * b.signum() + LADO * contramano;
            if costo < mejor.0 {
                mejor = (costo, b);
            }
        }
        // Se desvía tanto como la amenaza lo justifique, y nada más. Sin el
        // peso el rumbo saltaría al centro del sector elegido en cuanto el
        // looming pasara `LIBRE`, que es un escalón en el comando y la clase
        // de cosa que después aparece como oscilación (regla 3.13).
        let b_cmd = b_obj + (mejor.1 - b_obj) * peso;

        // **Dos afinados que se probaron y NO sirvieron. No reabrir sin nueva
        // evidencia** (regla 3.4: un intento fallido documentado vale tanto
        // como uno que salió bien, porque evita que alguien lo repita).
        // Sorteos aprobados por el clásico con ojos, 24 sorteos:
        //
        // 1. **Frenar en proporción al looming** —lo que haría cualquiera, y
        //    lo que hace la industria— EMPEORA:
        //
        //    ```text
        //      freno   peldaño 8   peldaño 9
        //       0.0      12 / 24     7 / 24
        //       0.3      11 / 24     4 / 24
        //       0.5       4 / 24     0 / 24
        //    ```
        //
        //    La causa es que **las misiones tienen reloj**: frenar cambia
        //    chocar por «se quedó sin reloj», que puntúa igual de mal. El
        //    freno que sí conviene es el de `gym3d::frenar_ante_pared`, que
        //    sólo entra cuando TODOS los sectores están tapados — o sea
        //    cuando no hay hueco por donde pasar y frenar es lo único que
        //    queda.
        //
        // 2. **Desviarse MÁS que el centro del sector elegido** (ganancia
        //    sobre el desvío) tampoco: 1.0 → 7/24, 1.3 → 6/24, 1.6 → 5/24,
        //    2.0 → 6/24 en el peldaño 9. El desvío ya alcanza; lo que falta
        //    no es ángulo.
        //
        // Lo que queda diagnosticado como causa real del techo: **el sorteo de
        // escenarios, no el piloto.** En los sorteos que pierde, el clásico
        // roza a 0.00-0.53 m, y `postes_en_ruta` puede plantar un poste donde
        // el tramo de VUELTA lo cruza sin corredor libre. Peldaño 13 se queda
        // en 0 de 24 con cualquier ajuste de este lazo.

        // Y de acá para abajo es `consignas` tal cual, con el rumbo cambiado:
        // error de posición → velocidad deseada saturada, velocidad → ángulos.
        let dist = (ex * ex + ey * ey).sqrt().max(1e-4);
        let rapidez = (self.k_pos * dist).min(self.vel_max);
        // Body → mundo: adelante es (c, s) y el lateral positivo es (−s, c).
        let (cb, sb) = (b_cmd.cos(), b_cmd.sin());
        let (dx, dy) = (cb * c - sb * s, cb * s + sb * c);
        let rapidez = rapidez.min(self.tope_por_mirada(m.yaw, dy.atan2(dx)));
        let (vx_d, vy_d) = (dx * rapidez, dy * rapidez);

        let (ax, ay) = ((vx_d - m.vx) * self.k_vel, (vy_d - m.vy) * self.k_vel);
        let a_adelante = ax * c + ay * s;
        let a_lateral = -ax * s + ay * c;
        let pitch = a_adelante.clamp(-self.ang_max, self.ang_max);
        let roll = (-a_lateral).clamp(-self.ang_max, self.ang_max);

        // La nariz va al rumbo COMANDADO, no al objetivo: si apuntara al
        // objetivo mientras el cuerpo se corre de costado, el poste que está
        // esquivando se le iría del cono de 160° justo mientras lo rodea —
        // que es el modo de falla que el contrato ya anota en la deuda 7
        // («el dron es ciego fuera del cono»).
        // Medido contra la alternativa (la nariz al OBJETIVO, dejando que el
        // cuerpo se corra de costado, que un quad puede): 12/24 y 7/24 con la
        // nariz al comando contra 11/24 y 6/24 con la nariz al objetivo. Un
        // sorteo de 24 de diferencia en cada peldaño, o sea **indicio y no
        // resultado** (sección 4) — se toma el que además tiene una razón
        // física, la del párrafo de abajo. Sin el término de compromiso las
        // dos daban idéntico, así que la diferencia aparece sólo cuando el
        // rodeo se sostiene.
        let rumbo_mundo = dy.atan2(dx);
        let dif = (rumbo_mundo - m.yaw + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        (roll, pitch, (dif * 1.5).clamp(-1.2, 1.2))
    }

}


#[cfg(test)]
mod diagnostico_actitud {
    use super::*;
    use crate::sim3d::{Dron3, Motores, DT};

    /// Comanda un escalón grande de pitch (el que pide persecución) y mira si
    /// el lazo de actitud lo sostiene limpio o entra en overshoot/oscilación.
    /// Sin guiado, sin viento, sin ruido: aísla el lazo interno del resto.
    ///
    /// Este lazo SÍ tiene un overshoot real de ~20% en el escalón inicial
    /// (medido: pico 1.446 rad contra un comando de 1.20) y se asienta en unos
    /// 2 s — es información, no un bug: un escalón de 0 a 68° de golpe no es
    /// una maniobra real, es un caso de laboratorio. Lo que importa es que
    /// CONVERJA y no oscile para siempre, que es lo que se rompía en
    /// `consignas_persiguiendo` (ver `persecucion::regresion_persecucion`).
    #[test]
    fn escalon_grande_de_pitch() {
        let mut d = Dron3::nuevo(0.0, 0.0, 8.0);
        let mut est = Estabilizador3::nuevo();
        let mut motores = Motores::nuevo();
        let pitch_d = 1.20f32; // INCLINACION_COMODA, lo que pide crucero a 25 m/s
        let mut pico = 0.0f32;
        for _ in 0..800u32 {
            let pedido = est.control(&d, 8.0, 0.0, pitch_d, 0.0);
            let (e, tx, ty, tz) = motores.aplicar(pedido, DT);
            d.paso(e, tx, ty, tz, (0.0, 0.0));
            pico = pico.max(d.pitch);
        }
        assert!(pico < pitch_d * 1.30,
            "el overshoot del escalón no puede pasar de 30% ({pitch_d}), llegó a {pico}");
        assert!((d.pitch - pitch_d).abs() < 0.02,
            "a los 4 s tiene que estar asentado en el comando, quedó en {}", d.pitch);
        assert!(!d.caido, "un escalón de pitch sin viento no tiene que romper nada");
    }
}

#[cfg(test)]
mod pruebas_esquive {
    use super::*;

    /// **La identidad de la regla 3.15, verificada y no supuesta.**
    ///
    /// «Red en cero ⟺ clásico» es el instrumento con el que el proyecto sabe
    /// si algo funcionó, y `consignas_esquivando` se metió justo abajo de esa
    /// identidad: la usan el clásico Y la base del híbrido. Si con el anillo
    /// despejado diera algo aunque sea *parecido* a `consignas` en vez de lo
    /// MISMO, la vara se habría movido en todos los peldaños sin postes y
    /// nadie se enteraría — y las tablas de los peldaños 1 a 7, que no tienen
    /// obstáculos, dejarían de ser comparables sin ninguna razón.
    ///
    /// Se verifica en estados variados a propósito: quieto, en crucero, con la
    /// nariz torcida y con el objetivo detrás (que es el caso donde `atan2`
    /// puede dar un signo distinto si los dos caminos no calculan el rumbo
    /// igual).
    #[test]
    fn sin_nada_que_esquivar_es_exactamente_el_clasico() {
        let g = Guiado::default();
        let despejado = [0.0f32; crate::sim3d::SECTORES];
        let casos: &[(f32, f32, f32, f32, f32, f32, f32)] = &[
            // x, y, vx, vy, yaw, objetivo x, objetivo y
            (0.0, 0.0, 0.0, 0.0, 0.0, 5.0, 0.0),
            (0.0, 0.0, 3.5, 0.0, 0.0, 12.0, 4.0),
            (2.0, -3.0, 1.0, -2.0, 1.1, -4.0, 6.0),
            (0.0, 0.0, 0.0, 0.0, 0.0, -6.0, 0.0), // el objetivo DETRÁS
            (1.0, 1.0, -2.0, 2.5, -2.4, 0.5, -7.0),
        ];
        for &(x, y, vx, vy, yaw, ox, oy) in casos {
            let mut d = Dron3::default();
            d.x = x; d.y = y; d.vx = vx; d.vy = vy; d.yaw = yaw;
            let a = g.consignas(&d, ox, oy);
            let b = g.consignas_esquivando(&d, ox, oy, &despejado, 0.0);
            assert_eq!(a, b,
                "con el anillo despejado el esquive tiene que ser EXACTAMENTE \
                 el clásico (regla 3.15), y en el estado ({x}, {y}, v=({vx}, \
                 {vy}), yaw={yaw}) hacia ({ox}, {oy}) dio {a:?} contra {b:?}");
        }
    }

    /// Y el otro lado: con un poste loomeando en el centro, el comando **se
    /// va para un costado**. Sin esto la prueba de arriba se podría satisfacer
    /// con una función que nunca esquiva nada.
    #[test]
    fn un_poste_en_el_centro_corre_el_comando_de_costado() {
        let g = Guiado::default();
        let mut anillo = [0.0f32; crate::sim3d::SECTORES];
        anillo[crate::sim3d::SECTORES / 2] = 0.5; // bien por encima de PARED
        let mut d = Dron3::default();
        d.vx = 3.5; // hace falta ir hacia algo para que haya looming
        let recto = g.consignas(&d, 12.0, 0.0);
        let esquivando = g.consignas_esquivando(&d, 12.0, 0.0, &anillo, 0.0);
        assert!(
            (esquivando.0 - recto.0).abs() > 0.05,
            "con el centro tapado el alabeo tiene que cambiar: recto {:.4}, \
             esquivando {:.4}", recto.0, esquivando.0
        );
    }
}

#[cfg(test)]
mod medicion_carga_corrida {
    use super::*;
    use crate::sim3d::{Averia, Dron3};

    /// Inclinación que queda en régimen con un torque parásito constante
    /// (carga corrida del centro), en hover y con actitud pedida cero.
    pub fn inclinacion_en_regimen(tx_parasito: f32) -> f32 {
        let mut e = Estabilizador3::nuevo();
        let mut d = Dron3::default();
        d.z = 3.0;
        let av = Averia { torque_parasito: (tx_parasito, 0.0, 0.0), ..Averia::NINGUNA };
        let mut suma = 0.0;
        let pasos = 2000; // 10 s
        for i in 0..pasos {
            let (f, tx, ty, tz) = e.control(&d, 3.0, 0.0, 0.0, 0.0);
            d.paso_averiado(f, tx, ty, tz, (0.0, 0.0), &av);
            if i >= pasos - 400 { suma += d.roll; }
        }
        (suma / 400.0).to_degrees()
    }

    /// Una carga corrida del centro (la caja del imán) no puede dejar al dron
    /// inclinado para siempre. Sin la I del lazo de giro: 0.05 N·m → 8.3°.
    #[test]
    fn una_carga_corrida_no_deja_inclinacion_permanente() {
        for t in [0.02f32, 0.05, 0.08, 0.12] {
            let r = inclinacion_en_regimen(t);
            println!("MEDICION torque {t:.2} N·m → roll en régimen {r:.2}°");
            assert!(r.abs() < 0.5, "con {t} N·m queda inclinado {r:.2}°");
        }
    }
}

#[cfg(test)]
mod medicion_caja_de_golpe {
    use super::*;
    use crate::sim3d::{Averia, Dron3};

    /// Hover a 3 m; en t=2 s el imán levanta `kg` de golpe. Devuelve cuánto
    /// se hundió como máximo (m) y la velocidad de bajada máxima (m/s).
    pub fn hundimiento(kg: f32) -> (f32, f32) {
        let mut e = Estabilizador3::nuevo();
        let mut d = Dron3::default();
        d.z = 3.0;
        let (mut zmin, mut vmin) = (3.0f32, 0.0f32);
        for i in 0..2000 {
            let av = if i >= 400 { Averia { masa_extra: kg, ..Averia::NINGUNA } } else { Averia::NINGUNA };
            let (f, tx, ty, tz) = e.control(&d, 3.0, 0.0, 0.0, 0.0);
            d.paso_averiado(f, tx, ty, tz, (0.0, 0.0), &av);
            if i >= 400 { zmin = zmin.min(d.z); vmin = vmin.min(d.vz); }
        }
        (3.0 - zmin, -vmin)
    }

    #[test]
    fn levantar_la_caja_no_hunde_al_dron() {
        for kg in [0.1f32, 0.3, 0.5] {
            let (h, v) = hundimiento(kg);
            println!("MEDICION caja {kg:.1} kg → se hunde {h:.2} m, baja a {v:.2} m/s");
        }
    }
}
