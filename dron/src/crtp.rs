//! **El puente al LiteWing** (y a cualquier dron con firmware derivado de
//! Crazyflie: ESP-Drone, Crazyflie): las cuatro consignas del cerebro a
//! paquetes CRTP por WiFi (UDP).
//!
//! Es el hermano de `puente.rs` (MSP, para controladoras Betaflight): el
//! cerebro no cambia, cambia cómo se le habla a la controladora. Y el
//! protocolo de Crazyflie tiene justo la consigna que produce el cerebro
//! (regla 3.1): **alabeo, cabeceo, velocidad de guiñada y ALTURA** —el
//! «zDistance setpoint» del commander genérico—, así que la altura la sostiene
//! la controladora con su propio sensor (el ToF del módulo de flujo) y no hay
//! que traducir a acelerador.
//!
//! Formato, verificado contra la documentación de ESP-Drone y el código de
//! `cflib` (la biblioteca oficial de Crazyflie), el 2026-09-26:
//!
//! - UDP al dron en `192.168.43.42:2390` (su punto de acceso WiFi);
//! - cada datagrama: cabecera CRTP (puerto << 4 | canal) + datos + un byte de
//!   suma de control (suma de todos los bytes, módulo 256);
//! - consigna de altura: puerto 7 (commander genérico), canal 0,
//!   `<Bffff` = tipo, alabeo°, cabeceo°, guiñada°/s, altura m. Tipo 2 y
//!   guiñada negada en el protocolo viejo (≤ 8, el de ESP-Drone), tipo 9 sin
//!   negar en el nuevo;
//! - desbloqueo: una consigna vieja (puerto 3, `<fffH`) toda en cero — el
//!   firmware no acepta consignas hasta recibir una con empuje cero;
//! - parar: commander genérico tipo 0 (motores apagados).
//!
//! **Lo que NO está verificado y se verifica el primer día, con las hélices
//! sacadas y el dron atado:** los SIGNOS de alabeo y cabeceo entre nuestro
//! simulador y el firmware (`dron litewing --signos` hace una secuencia lenta
//! para mirarlos), y qué versión del protocolo habla el LiteWing. Tampoco la
//! telemetría de vuelta (el subsistema de log de CRTP): sin el dron no hay
//! contra qué probar la tabla de variables, y escribirla a ciegas es el tipo
//! de instrumento no verificado que la regla 3.18 prohíbe.

use crate::gym3d::Consignas;
use crate::puente::{Seguridad, TOPE_ANGULO, TOPE_GUINADA};

pub const PUERTO_COMMANDER: u8 = 3;
pub const PUERTO_COMMANDER_GENERICO: u8 = 7;
pub const TIPO_PARAR: u8 = 0;
pub const TIPO_ALTURA_VIEJO: u8 = 2;
pub const TIPO_ALTURA_NUEVO: u8 = 9;
/// Techo de altura para las primeras pruebas, adentro de una casa (m).
pub const TOPE_ALTURA: f32 = 1.5;
pub const DIRECCION: &str = "192.168.43.42:2390";

/// Signo de cada eje entre el simulador y el firmware. **A verificar con el
/// dron atado** (`--signos`); se cambia acá y en ningún otro lado.
pub const SIGNO_ALABEO: f32 = 1.0;
pub const SIGNO_CABECEO: f32 = 1.0;

/// Un datagrama CRTP completo: cabecera, datos y suma de control.
pub fn paquete(puerto: u8, canal: u8, datos: &[u8]) -> Vec<u8> {
    assert!(datos.len() <= 31, "un paquete CRTP lleva como mucho 31 bytes de datos");
    let mut v = Vec::with_capacity(datos.len() + 2);
    v.push(((puerto & 0x0F) << 4) | (canal & 0x03));
    v.extend_from_slice(datos);
    let suma = v.iter().fold(0u8, |a, b| a.wrapping_add(*b));
    v.push(suma);
    v
}

/// La consigna vieja en cero que desbloquea el commander.
pub fn desbloqueo() -> Vec<u8> {
    let mut d = Vec::with_capacity(14);
    for f in [0.0f32, 0.0, 0.0] {
        d.extend_from_slice(&f.to_le_bytes());
    }
    d.extend_from_slice(&0u16.to_le_bytes());
    paquete(PUERTO_COMMANDER, 0, &d)
}

/// Motores apagados.
pub fn parar() -> Vec<u8> {
    paquete(PUERTO_COMMANDER_GENERICO, 0, &[TIPO_PARAR])
}

/// Las cuatro consignas del cerebro como consigna de altura CRTP, con los
/// mismos topes que el puente MSP (20° de inclinación) y un techo de altura.
pub fn consigna_altura(c: &Consignas, protocolo_viejo: bool) -> Vec<u8> {
    let roll = (c.roll * SIGNO_ALABEO).clamp(-TOPE_ANGULO, TOPE_ANGULO).to_degrees();
    let pitch = (c.pitch * SIGNO_CABECEO).clamp(-TOPE_ANGULO, TOPE_ANGULO).to_degrees();
    let mut yaw = c.yaw.clamp(-TOPE_GUINADA, TOPE_GUINADA).to_degrees();
    if protocolo_viejo {
        yaw = -yaw;
    }
    let z = c.alt.clamp(0.0, TOPE_ALTURA);
    let mut d = vec![if protocolo_viejo { TIPO_ALTURA_VIEJO } else { TIPO_ALTURA_NUEVO }];
    for f in [roll, pitch, yaw, z] {
        d.extend_from_slice(&f.to_le_bytes());
    }
    paquete(PUERTO_COMMANDER_GENERICO, 0, &d)
}

/// El enlace: la misma capa de seguridad que el puente MSP (nunca se arma
/// solo; si el cerebro se calla 0.2 s, desarma) y un desbloqueo al armar.
pub struct Enlace {
    pub seguridad: Seguridad,
    pub protocolo_viejo: bool,
    desbloqueado: bool,
}

impl Enlace {
    pub fn nuevo(protocolo_viejo: bool) -> Self {
        Enlace { seguridad: Seguridad::nuevo(), protocolo_viejo, desbloqueado: false }
    }

    /// Un ciclo: el datagrama que corresponde mandar ahora.
    pub fn ciclo(&mut self, c: &Consignas) -> Vec<u8> {
        if !self.seguridad.tic() {
            self.desbloqueado = false;
            return parar();
        }
        if !self.desbloqueado {
            self.desbloqueado = true;
            return desbloqueo();
        }
        consigna_altura(c, self.protocolo_viejo)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn suma_ok(p: &[u8]) -> bool {
        let (datos, suma) = p.split_at(p.len() - 1);
        datos.iter().fold(0u8, |a, b| a.wrapping_add(*b)) == suma[0]
    }

    #[test]
    fn la_consigna_de_altura_tiene_el_formato_de_cflib() {
        let c = Consignas { roll: 0.1, pitch: -0.2, yaw: 0.5, alt: 1.0 };
        let p = consigna_altura(&c, true);
        assert_eq!(p.len(), 1 + 17 + 1, "cabecera + <Bffff + suma");
        assert_eq!(p[0], 0x70, "puerto 7, canal 0");
        assert_eq!(p[1], TIPO_ALTURA_VIEJO);
        let f = |i: usize| f32::from_le_bytes(p[2 + 4 * i..6 + 4 * i].try_into().unwrap());
        assert!((f(0) - 0.1f32.to_degrees()).abs() < 1e-4);
        assert!((f(1) + 0.2f32.to_degrees()).abs() < 1e-4);
        assert!((f(2) + 0.5f32.to_degrees()).abs() < 1e-4, "guiñada negada en el protocolo viejo");
        assert_eq!(f(3), 1.0);
        assert!(suma_ok(&p));
    }

    #[test]
    fn los_topes_de_seguridad_se_respetan() {
        let c = Consignas { roll: 1.5, pitch: -1.5, yaw: 9.0, alt: 20.0 };
        let p = consigna_altura(&c, false);
        let f = |i: usize| f32::from_le_bytes(p[2 + 4 * i..6 + 4 * i].try_into().unwrap());
        assert!(f(0) <= TOPE_ANGULO.to_degrees() + 1e-3 && f(1) >= -TOPE_ANGULO.to_degrees() - 1e-3);
        assert!(f(2) <= TOPE_GUINADA.to_degrees() + 1e-3);
        assert_eq!(f(3), TOPE_ALTURA);
    }

    /// Nunca se arma solo; armado, primero desbloquea y después manda
    /// consignas; si el cerebro se calla, vuelve a mandar «parar».
    #[test]
    fn el_enlace_no_se_arma_solo_y_para_si_el_cerebro_se_calla() {
        let c = Consignas { roll: 0.0, pitch: 0.0, yaw: 0.0, alt: 0.5 };
        let mut e = Enlace::nuevo(true);
        e.seguridad.hay_datos();
        assert_eq!(e.ciclo(&c), parar(), "sin armar, parar");
        e.seguridad.armar(true);
        e.seguridad.hay_datos();
        assert_eq!(e.ciclo(&c), desbloqueo());
        e.seguridad.hay_datos();
        assert_eq!(e.ciclo(&c)[0], 0x70);
        for _ in 0..20 {
            e.ciclo(&c); // sin `hay_datos`: el cerebro se calló
        }
        assert_eq!(e.ciclo(&c), parar());
        assert!(!e.seguridad.armado(), "tiene que desarmar");
    }

    /// Por la red de verdad: un «dron» falso en localhost recibe los
    /// datagramas y los encuentra bien formados.
    #[test]
    fn llega_por_udp_bien_formado() {
        let dron = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        dron.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        let yo = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let c = Consignas { roll: 0.05, pitch: 0.0, yaw: 0.0, alt: 0.8 };
        yo.send_to(&consigna_altura(&c, true), dron.local_addr().unwrap()).unwrap();
        let mut b = [0u8; 64];
        let n = dron.recv(&mut b).unwrap();
        assert_eq!(n, 19);
        assert!(suma_ok(&b[..n]));
        assert_eq!(b[0] >> 4, PUERTO_COMMANDER_GENERICO);
    }
}
