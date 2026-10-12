//! **EL PUENTE A HARDWARE REAL.** Lo que convierte las consignas del cerebro en
//! algo que un controlador de vuelo comercial obedece.
//!
//! Hasta ahora esto estaba en CERO y era el tapón declarado del proyecto: todo
//! lo demás —la física, el cerebro, el entrenamiento, la percepción— produce
//! cuatro números (`gym3d::Consignas`: alabeo, cabeceo, guiñada, altura) que no
//! salían de la computadora. Este módulo los saca.
//!
//! ## Por qué MSP y no MAVLink
//!
//! La regla 3.1 dice que el día que esto suba a un dron real, `Estabilizador3`
//! se borra y lo reemplaza el controlador comercial. El controlador elegido es
//! una placa de ~15-20 USD corriendo **Betaflight/iNav** (bitácora 7.1), y ésas
//! hablan **MSP**, no MAVLink. MAVLink es de ArduPilot/PX4, que corren en placas
//! más caras.
//!
//! Y dentro de MSP, la forma correcta de que una computadora pilote es
//! `MSP_SET_RAW_RC` (comando 200): se inyectan los canales de radio como si
//! vinieran del control remoto. Esto es deliberado y es lo que hace la
//! industria:
//!
//! - El controlador sigue haciendo estabilización, límites y failsafe. No se
//!   le pide que confíe en nosotros para nada crítico.
//! - Si el puente se calla, Betaflight dispara su propio failsafe **sin que
//!   nosotros tengamos que hacer nada bien**. Es seguridad que no depende de
//!   que nuestro código no tenga bugs, que es la única clase de seguridad que
//!   sirve.
//! - El piloto humano puede recuperar el mando con un interruptor físico.
//!
//! ## El mapeo, que es donde se puede romper algo de verdad
//!
//! Las consignas están en radianes (actitud) y metros (altura); los canales de
//! radio van de 1000 a 2000 µs con centro en 1500. La conversión NO es un
//! detalle de formato: un signo cambiado acá es un dron que se va contra el
//! piso a toda potencia. Por eso el mapeo vive en un solo lugar, tiene prueba,
//! y los límites se aplican ANTES de convertir, no después.

use crate::gym3d::Consignas;

/// Canales de radio, en microsegundos. El orden es AETR (alabeo, cabeceo,
/// acelerador, guiñada), que es el que Betaflight espera por defecto.
pub const CANALES: usize = 8;
pub const RC_MIN: u16 = 1000;
pub const RC_CENTRO: u16 = 1500;
pub const RC_MAX: u16 = 2000;

/// **Cuánta inclinación puede pedir el puente, en radianes.**
///
/// NO es el límite del aparato: es el límite de lo que la computadora tiene
/// permitido pedirle. Deliberadamente más chico que `INCLINACION_EXTREMA`
/// (1.40 rad) del simulador, porque el primer vuelo real no es el momento de
/// averiguar si el mapeo de signos está bien a 80° de inclinación.
///
/// Se sube a mano, con el aparato atado y midiendo, no porque el simulador
/// aguante más.
pub const TOPE_ANGULO: f32 = 0.35; // ~20°

/// Tope de velocidad de guiñada que el puente puede pedir, en rad/s.
pub const TOPE_GUINADA: f32 = 1.5;

/// Cada cuántos milisegundos hay que mandar un paquete. Betaflight espera RC a
/// ~50 Hz y dispara failsafe si se calla. Mandar más rápido no mejora nada y
/// satura el puerto serie.
pub const PERIODO_MS: u64 = 20;

/// Traduce las consignas del cerebro a canales de radio.
///
/// `acelerador` va aparte de `Consignas` porque la consigna de altura del
/// proyecto es una ALTURA (metros), y un controlador de vuelo en modo ángulo
/// espera ACELERADOR. Quien llama decide cómo cerrar ese lazo: con el
/// barómetro del propio controlador (modo ALTHOLD, donde el canal es velocidad
/// vertical) o con un lazo propio. No se adivina acá.
pub fn a_canales(c: &Consignas, acelerador: f32, armado: bool) -> [u16; CANALES] {
    // Los límites se aplican ANTES de convertir, sobre magnitudes físicas con
    // significado. Recortar en microsegundos después escondería que se estaba
    // pidiendo algo fuera de rango.
    let roll = (c.roll / TOPE_ANGULO).clamp(-1.0, 1.0);
    let pitch = (c.pitch / TOPE_ANGULO).clamp(-1.0, 1.0);
    let yaw = (c.yaw / TOPE_GUINADA).clamp(-1.0, 1.0);
    let acel = acelerador.clamp(0.0, 1.0);

    let centrado = |v: f32| -> u16 {
        (RC_CENTRO as f32 + v * (RC_MAX - RC_CENTRO) as f32)
            .clamp(RC_MIN as f32, RC_MAX as f32) as u16
    };

    let mut ch = [RC_CENTRO; CANALES];
    ch[0] = centrado(roll);
    ch[1] = centrado(pitch);
    // El acelerador NO está centrado: va de mínimo a máximo, y en reposo tiene
    // que quedar en el MÍNIMO. Un acelerador que arranca al medio es un dron
    // que se va para arriba apenas se arma.
    ch[2] = (RC_MIN as f32 + acel * (RC_MAX - RC_MIN) as f32) as u16;
    ch[3] = centrado(yaw);
    // AUX1 = armado. Betaflight arma con el canal por encima de ~1700.
    ch[4] = if armado { 1800 } else { RC_MIN };
    ch
}

/// Empaqueta un mensaje MSP v1.
///
/// Formato: `$M<` + largo + comando + carga + checksum XOR. El checksum cubre
/// largo, comando y carga — no la cabecera.
pub fn trama_msp(comando: u8, carga: &[u8]) -> Vec<u8> {
    let mut t = Vec::with_capacity(6 + carga.len());
    t.extend_from_slice(b"$M<");
    t.push(carga.len() as u8);
    t.push(comando);
    t.extend_from_slice(carga);
    let mut suma = carga.len() as u8 ^ comando;
    for b in carga {
        suma ^= b;
    }
    t.push(suma);
    t
}

/// `MSP_SET_RAW_RC`: los canales, cada uno en 16 bits little-endian.
pub const MSP_SET_RAW_RC: u8 = 200;

pub fn trama_rc(canales: &[u16; CANALES]) -> Vec<u8> {
    let mut carga = Vec::with_capacity(CANALES * 2);
    for c in canales {
        carga.extend_from_slice(&c.to_le_bytes());
    }
    trama_msp(MSP_SET_RAW_RC, &carga)
}

/// **El piloto de seguridad.** Decide si el puente tiene derecho a mandar algo,
/// y qué mandar cuando no lo tiene.
///
/// Existe separado del mapeo porque son dos preguntas distintas: "cómo se dice
/// esto en canales de radio" y "¿corresponde decirlo?". Mezclarlas es cómo se
/// terminan poniendo condiciones de seguridad adentro de una conversión de
/// unidades, donde nadie las revisa.
pub struct Seguridad {
    /// Cuántos ciclos seguidos sin una consigna fresca. Si el cerebro se
    /// cuelga, el puente NO puede seguir repitiendo la última orden: un dron
    /// volando con una consigna congelada se va derecho hasta que choca.
    sin_datos: u32,
    /// Después de cuántos ciclos se considera perdido el cerebro.
    tope_sin_datos: u32,
    armado: bool,
}

impl Seguridad {
    pub fn nuevo() -> Self {
        // 10 ciclos a 50 Hz = 0.2 s. Betaflight tiene su propio failsafe más
        // lento; éste es el nuestro, y llega primero a propósito.
        Seguridad { sin_datos: 0, tope_sin_datos: 10, armado: false }
    }

    /// Lo llama quien tenga una consigna nueva del cerebro.
    pub fn hay_datos(&mut self) {
        self.sin_datos = 0;
    }

    /// Armar es SIEMPRE una decisión explícita de una persona. No hay ningún
    /// camino por el que el software se arme solo, ni siquiera "si todo está
    /// bien": el costo de un falso positivo es un aparato girando hélices en
    /// una mesa.
    pub fn armar(&mut self, si: bool) {
        self.armado = si;
        if !si {
            self.sin_datos = 0;
        }
    }

    pub fn armado(&self) -> bool {
        self.armado
    }

    pub fn perdido(&self) -> bool {
        self.sin_datos >= self.tope_sin_datos
    }

    /// Un ciclo del puente: devuelve los canales que corresponde mandar.
    ///
    /// Si el cerebro se perdió, devuelve neutro con acelerador al mínimo y
    /// DESARMA. No intenta aterrizar solo ni mantener la última orden: el
    /// controlador de vuelo tiene su propio failsafe, mucho mejor probado que
    /// cualquier cosa que se escriba acá, y lo correcto es dejarlo actuar.
    pub fn ciclo(&mut self, c: &Consignas, acelerador: f32) -> [u16; CANALES] {
        if !self.tic() {
            return a_canales(&Consignas::default(), 0.0, false);
        }
        a_canales(c, acelerador, true)
    }

    /// El mismo ciclo sin el mapeo a canales, para cualquier protocolo
    /// (`crtp.rs`): cuenta un ciclo sin datos, desarma si el cerebro se
    /// perdió, y dice si corresponde mandar consignas (`true`) o parar.
    pub fn tic(&mut self) -> bool {
        self.sin_datos = self.sin_datos.saturating_add(1);
        if self.perdido() {
            self.armado = false;
        }
        self.armado
    }
}

impl Default for Seguridad {
    fn default() -> Self {
        Self::nuevo()
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// La trama MSP contra un caso calculado a mano. Si esto se rompe, el
    /// controlador descarta los paquetes en silencio y el dron no responde —
    /// un modo de falla que desde afuera parece "el cerebro no manda nada".
    #[test]
    fn la_trama_msp_tiene_el_checksum_correcto() {
        let t = trama_msp(200, &[0x01, 0x02]);
        assert_eq!(&t[..3], b"$M<");
        assert_eq!(t[3], 2, "largo");
        assert_eq!(t[4], 200, "comando");
        // checksum = largo ^ comando ^ carga
        let esperado = 2u8 ^ 200 ^ 0x01 ^ 0x02;
        assert_eq!(*t.last().unwrap(), esperado);
    }

    /// **En reposo el acelerador va al MÍNIMO, no al centro.** Es la prueba
    /// más importante del módulo: un acelerador centrado por defecto es un
    /// aparato que sube apenas se arma.
    #[test]
    fn en_reposo_el_acelerador_esta_abajo() {
        let ch = a_canales(&Consignas::default(), 0.0, false);
        assert_eq!(ch[2], RC_MIN, "acelerador en reposo tiene que ser el mínimo");
        assert_eq!(ch[0], RC_CENTRO);
        assert_eq!(ch[1], RC_CENTRO);
        assert_eq!(ch[3], RC_CENTRO);
        assert_eq!(ch[4], RC_MIN, "sin armar, AUX1 abajo");
    }

    /// Pedir más inclinación de la permitida satura en el tope, no da la
    /// vuelta ni desborda.
    #[test]
    fn el_tope_de_angulo_satura_y_no_desborda() {
        let c = Consignas { roll: 10.0, pitch: -10.0, yaw: 0.0, alt: 0.0 };
        let ch = a_canales(&c, 0.5, true);
        assert_eq!(ch[0], RC_MAX);
        assert_eq!(ch[1], RC_MIN);
        for v in ch {
            assert!((RC_MIN..=RC_MAX).contains(&v), "canal fuera de rango: {v}");
        }
    }

    /// Si el cerebro se calla, el puente desarma. No sigue repitiendo la
    /// última orden.
    #[test]
    fn si_el_cerebro_se_calla_el_puente_desarma() {
        let mut s = Seguridad::nuevo();
        s.armar(true);
        s.hay_datos();
        let c = Consignas { roll: 0.3, pitch: 0.0, yaw: 0.0, alt: 0.0 };
        let vivo = s.ciclo(&c, 0.6);
        assert!(vivo[0] > RC_CENTRO, "con datos frescos manda lo pedido");
        assert!(s.armado());

        // Nadie vuelve a avisar que hay datos.
        for _ in 0..20 {
            s.ciclo(&c, 0.6);
        }
        assert!(s.perdido());
        assert!(!s.armado(), "tiene que desarmarse solo");
        let muerto = s.ciclo(&c, 0.6);
        assert_eq!(muerto[2], RC_MIN, "acelerador al mínimo al perder el cerebro");
        assert_eq!(muerto[0], RC_CENTRO, "y actitud neutra");
    }

    /// El software NUNCA se arma solo.
    #[test]
    fn sin_armar_a_mano_no_sale_nada() {
        let mut s = Seguridad::nuevo();
        let c = Consignas { roll: 0.3, pitch: 0.3, yaw: 0.3, alt: 5.0 };
        for _ in 0..5 {
            s.hay_datos();
            let ch = s.ciclo(&c, 1.0);
            assert_eq!(ch[2], RC_MIN);
            assert_eq!(ch[4], RC_MIN);
        }
    }
}
