//! **Trenes bala entre ciudades** (pedido del usuario, 4 oct 2026).
//!
//! Una línea une dos ciudades vecinas (casillas de `SITE_PERIOD` contiguas en el eje X) por un riel recto de este a
//! oeste a media altura entre sus centros; el tren va y viene con una parada en cada estación. Todo es función pura
//! del reloj (`segundos`), así que todos los clientes ven el mismo tren sin hablar con el servidor.

use crate::geography::{settlement_in, SettlementTier, SITE_PERIOD};
use crate::CelestialBody;

/// Velocidad máxima del tren bala (tiles/s) — a mitad de camino; el promedio es la mitad.
pub const V_MAX: f32 = 70.0;
/// Segundos parado en cada estación.
pub const PARADA_S: f32 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Linea {
    /// Estación oeste y estación este (x), y el z común del riel.
    pub x0: f32,
    pub x1: f32,
    pub z: f32,
    /// Identificador estable (para sembrar variantes).
    pub id: u32,
}

impl Linea {
    pub fn largo(&self) -> f32 {
        self.x1 - self.x0
    }
    /// Segundos que dura el viaje de una punta a la otra (con arranque y frenada suaves).
    pub fn viaje_s(&self) -> f32 {
        self.largo() / (V_MAX * 0.5)
    }
    pub fn ciclo_s(&self) -> f32 {
        2.0 * (self.viaje_s() + PARADA_S)
    }
}

/// La línea que sale al este de la casilla `(sx, sz)`, si las dos ciudades la justifican.
pub fn linea_de_casilla(body: CelestialBody, sx: i64, sz: i64) -> Option<Linea> {
    let a = settlement_in(body, sx, sz)?;
    let b = settlement_in(body, sx + 1, sz)?;
    let grande = |t: SettlementTier| matches!(t, SettlementTier::City | SettlementTier::Town);
    if !grande(a.tier) || !grande(b.tier) {
        return None;
    }
    let z = (a.center.1 + b.center.1) as f32 * 0.5;
    // El riel tiene que pasar por dentro del casco de las dos (para que la estación quede en la ciudad).
    let tope = a.radius.min(b.radius) as f32 * 0.7;
    if (z - a.center.1 as f32).abs() > tope || (z - b.center.1 as f32).abs() > tope {
        return None;
    }
    let (x0, x1) = (a.center.0 as f32, b.center.0 as f32);
    if x1 - x0 < 400.0 {
        return None;
    }
    Some(Linea { x0, x1, z: z.round(), id: (sx * 31 + sz) as u32 })
}

/// Las líneas cuyo riel pasa a menos de `radio` tiles de `(x, z)`.
pub fn lineas_cerca(body: CelestialBody, x: f32, z: f32, radio: f32) -> Vec<Linea> {
    let (cx, cz) = ((x as i64).div_euclid(SITE_PERIOD), (z as i64).div_euclid(SITE_PERIOD));
    let mut v = Vec::new();
    for dz in -1..=1 {
        for dx in -2..=1 {
            if let Some(l) = linea_de_casilla(body, cx + dx, cz + dz) {
                if (l.z - z).abs() <= radio && x >= l.x0 - radio && x <= l.x1 + radio {
                    v.push(l);
                }
            }
        }
    }
    v
}

/// Posición x del TREN en el instante `t` (segundos) y si va hacia el este.
pub fn posicion(l: &Linea, t: f32) -> (f32, bool) {
    let viaje = l.viaje_s();
    let ph = (t + l.id as f32 * 7.3).rem_euclid(l.ciclo_s());
    let suave = |u: f32| u * u * (3.0 - 2.0 * u);
    if ph < PARADA_S {
        (l.x0, true)
    } else if ph < PARADA_S + viaje {
        (l.x0 + l.largo() * suave((ph - PARADA_S) / viaje), true)
    } else if ph < 2.0 * PARADA_S + viaje {
        (l.x1, false)
    } else {
        (l.x1 - l.largo() * suave((ph - 2.0 * PARADA_S - viaje) / viaje), false)
    }
}

/// ¿Está el punto `(x, z)` bajo el tren (largo `largo_tren`)? Sirve para el atropello.
pub fn bajo_el_tren(l: &Linea, t: f32, largo_tren: f32, x: f32, z: f32) -> bool {
    let (tx, _) = posicion(l, t);
    (z - l.z).abs() < 1.8 && (x - tx).abs() < largo_tren * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn una_linea() -> Linea {
        Linea { x0: 0.0, x1: 2600.0, z: 10.0, id: 3 }
    }

    #[test]
    fn el_tren_para_en_las_dos_estaciones_y_va_y_viene() {
        let l = una_linea();
        let (x, _) = posicion(&l, -3.0 * 7.3 + 1.0);
        assert!((x - l.x0).abs() < 1.0);
        let mut visto_ida = false;
        let mut visto_vuelta = false;
        let mut t = 0.0;
        while t < l.ciclo_s() {
            let (x, este) = posicion(&l, t);
            assert!(x >= l.x0 - 0.01 && x <= l.x1 + 0.01);
            if este && x > l.x0 + 100.0 { visto_ida = true; }
            if !este && x < l.x1 - 100.0 { visto_vuelta = true; }
            t += 0.5;
        }
        assert!(visto_ida && visto_vuelta);
    }

    #[test]
    fn el_tren_no_se_teletransporta() {
        let l = una_linea();
        let mut prev = posicion(&l, 0.0).0;
        let mut t = 0.1;
        while t < l.ciclo_s() * 2.0 {
            let x = posicion(&l, t).0;
            assert!((x - prev).abs() < V_MAX * 0.1 * 1.01, "salto de {} en t={t}", (x - prev).abs());
            prev = x;
            t += 0.1;
        }
    }

    #[test]
    fn en_la_tierra_hay_alguna_linea() {
        let body = CelestialBody::Earth;
        let n = (-6..6).flat_map(|sx| (-6..6).map(move |sz| (sx, sz))).filter(|&(sx, sz)| linea_de_casilla(body, sx, sz).is_some()).count();
        assert!(n >= 1, "ninguna línea de tren en la Tierra");
    }
}
