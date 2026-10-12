//! Lazo interno de estabilización: PID clásico.
//!
//! Esta es la pieza que hace viable todo el proyecto. La red binaria NUNCA
//! toca los motores: sólo dice "quiero inclinarme tanto" y "quiero estar a tal
//! altura". El PID traduce eso a empuje y torque, y es matemáticamente estable.
//!
//! El reparto no es un detalle de implementación, es la decisión de diseño:
//! - un genoma malo produce una consigna rara, y el dron se mueve raro;
//! - un genoma malo SIN esta capa produce empuje aleatorio, y el dron se cae.
//!
//! Es la diferencia entre poder evolucionar en hardware real y romper hélices.

use crate::sim3d::DT;

pub struct Pid {
    kp: f32,
    ki: f32,
    kd: f32,
    integral: f32,
    error_previo: f32,
    limite_integral: f32,
    /// Consigna pasada por un pasa-bajos, para el «iterm relax».
    consigna_lenta: f32,
}

impl Pid {
    pub fn nuevo(kp: f32, ki: f32, kd: f32, limite_integral: f32) -> Self {
        Pid { kp, ki, kd, integral: 0.0, error_previo: 0.0, limite_integral, consigna_lenta: 0.0 }
    }

    pub fn actualizar(&mut self, error: f32) -> f32 {
        self.integral = (self.integral + error * DT).clamp(-self.limite_integral, self.limite_integral);
        let derivada = (error - self.error_previo) / DT;
        self.error_previo = error;
        self.kp * error + self.ki * self.integral + self.kd * derivada
    }

    /// Sin uso hoy: cada episodio construye un estabilizador nuevo. Se deja
    /// porque un vuelo continuo con cambio de misión sí lo va a necesitar.
    /// El PID de siempre (D sobre el error) con **anti-windup condicional**:
    /// la integral no acumula mientras la salida está saturada en la
    /// dirección del error. Es lo que evita que «siga empujando cuando el
    /// error ya se fue».
    ///
    /// **Probado y descartado: la D sobre la MEDIDA** (la forma de Betaflight
    /// y PX4, «la D mira el giro, nunca el error»). En esta cascada rompe el
    /// lazo: clásico 0 de 12 en los peldaños 1 y 8, con y sin I, con y sin
    /// tope de salida. La causa es que la D sobre el error lleva adentro
    /// `kd·d(consigna)/dt`, y la consigna de giro es `GAN_ANG × error de
    /// ángulo`, así que ese término funciona como prealimentación del lazo
    /// externo; las ganancias se afinaron contando con él. Pasarla a la
    /// medida exige re-afinar la cascada entera (bitácora 7.37).
    pub fn actualizar_con_tope(&mut self, error: f32, tope: f32, consigna: f32, relax: f32) -> f32 {
        let derivada = (error - self.error_previo) / DT;
        self.error_previo = error;
        let sin_i = self.kp * error + self.kd * derivada;
        let salida = sin_i + self.ki * self.integral;
        let saturada = salida.abs() >= tope && salida.signum() == error.signum();
        // Iterm relax (Betaflight): no se integra mientras la consigna CAMBIA
        // rápido, que es cuando el error es de seguimiento y no de régimen.
        // Mide la parte rápida (consigna menos su pasa-bajos de 0.1 s), NO
        // la magnitud: con la magnitud, una carga corrida —que en régimen
        // pide un giro sostenido— apagaba la I justo cuando hacía falta
        // (medido: seguía 8.2° inclinado con 0.05 N·m).
        const TAU_RELAX: f32 = 0.1;
        self.consigna_lenta += (consigna - self.consigna_lenta) * (DT / TAU_RELAX).min(1.0);
        let rapida = consigna - self.consigna_lenta;
        let peso = (1.0 - rapida.abs() / relax).clamp(0.0, 1.0);
        if !saturada {
            self.integral = (self.integral + error * DT * peso)
                .clamp(-self.limite_integral, self.limite_integral);
        }
        sin_i + self.ki * self.integral
    }

}
