//! **Trenes bala entre ciudades** (pedido del usuario, 4 oct 2026: «que el tren interconecte absolutamente todas
//! las ciudades»).
//!
//! La red es el **árbol de expansión mínima** de todos los pueblos y ciudades del cuerpo (Prim sobre la distancia
//! entre centros): cada nodo queda unido a la red, sin bucles ni cruces absurdos. Cada tramo es un riel recto de
//! estación a estación; el tren va y viene con una parada en cada punta. Todo es función pura del reloj
//! (`segundos`), así que todos los clientes ven el mismo tren sin hablar con el servidor.
//! Las aldeas (`Hamlet`) quedan fuera: no tienen estación.

use crate::geography::{settlement_in, SettlementTier};
use crate::CelestialBody;
use std::sync::{Mutex, OnceLock};

/// Velocidad máxima del tren bala (tiles/s): ~430 km/h a escala 1 tile = 1 m. El promedio es la mitad.
pub const V_MAX: f32 = 120.0;
/// Segundos parado en cada estación.
pub const PARADA_S: f32 = 20.0;
/// Casillas de sitio que se barren alrededor del origen (el mundo de la Tierra llega a unos ±21.000 tiles).
const BARRIDO: i64 = 10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Linea {
    /// Estación de un extremo y estación del otro.
    pub a: (f32, f32),
    pub b: (f32, f32),
    /// Identificador estable (para escalonar horarios).
    pub id: u32,
}

impl Linea {
    pub fn largo(&self) -> f32 {
        (self.b.0 - self.a.0).hypot(self.b.1 - self.a.1)
    }
    /// Versor de `a` hacia `b`.
    pub fn dir(&self) -> (f32, f32) {
        let l = self.largo().max(1e-3);
        ((self.b.0 - self.a.0) / l, (self.b.1 - self.a.1) / l)
    }
    /// Segundos que dura el viaje de una punta a la otra (con arranque y frenada suaves).
    pub fn viaje_s(&self) -> f32 {
        self.largo() / (V_MAX * 0.5)
    }
    pub fn ciclo_s(&self) -> f32 {
        2.0 * (self.viaje_s() + PARADA_S)
    }
    /// Distancia del punto `(x, z)` al riel (segmento).
    pub fn distancia_a(&self, x: f32, z: f32) -> f32 {
        let (dx, dz) = self.dir();
        let t = ((x - self.a.0) * dx + (z - self.a.1) * dz).clamp(0.0, self.largo());
        (x - (self.a.0 + dx * t)).hypot(z - (self.a.1 + dz * t))
    }
}

/// Ciudades y pueblos de un cuerpo: `(nombre, centro, es_ciudad)`. Sirve a la brújula del HUD.
pub fn ciudades(body: CelestialBody) -> Vec<(String, (f32, f32), bool)> {
    static C: OnceLock<Mutex<Vec<(u8, Vec<(String, (f32, f32), bool)>)>>> = OnceLock::new();
    let cache = C.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(c) = cache.lock() {
        if let Some((_, v)) = c.iter().find(|(b, _)| *b == body as u8) {
            return v.clone();
        }
    }
    let v = calcular_ciudades(body);
    if let Ok(mut c) = cache.lock() {
        c.push((body as u8, v.clone()));
    }
    v
}

fn calcular_ciudades(body: CelestialBody) -> Vec<(String, (f32, f32), bool)> {
    let mut v = Vec::new();
    for sx in -BARRIDO..=BARRIDO {
        for sz in -BARRIDO..=BARRIDO {
            if let Some(s) = settlement_in(body, sx, sz) {
                let ciudad = s.tier == SettlementTier::City;
                if ciudad || s.tier == SettlementTier::Town {
                    let nombre = s.name.map(|n| n.to_string()).unwrap_or_else(|| if ciudad { "Ciudad".into() } else { "Pueblo".into() });
                    v.push((nombre, (s.center.0 as f32, s.center.1 as f32), ciudad));
                }
            }
        }
    }
    v
}

/// Todas las líneas de la red de un cuerpo. Se calcula una vez por cuerpo.
pub fn red(body: CelestialBody) -> Vec<Linea> {
    static C: OnceLock<Mutex<Vec<(u8, Vec<Linea>)>>> = OnceLock::new();
    let cache = C.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(c) = cache.lock() {
        if let Some((_, v)) = c.iter().find(|(b, _)| *b == body as u8) {
            return v.clone();
        }
    }
    let v = calcular_red(body);
    if let Ok(mut c) = cache.lock() {
        c.push((body as u8, v.clone()));
    }
    v
}

fn calcular_red(body: CelestialBody) -> Vec<Linea> {
    let mut nodos: Vec<(f32, f32)> = Vec::new();
    for sx in -BARRIDO..=BARRIDO {
        for sz in -BARRIDO..=BARRIDO {
            if let Some(s) = settlement_in(body, sx, sz) {
                if matches!(s.tier, SettlementTier::City | SettlementTier::Town) {
                    nodos.push((s.center.0 as f32, s.center.1 as f32));
                }
            }
        }
    }
    if nodos.len() < 2 {
        return Vec::new();
    }
    // Prim: de a un nodo por vez, el más cercano a lo ya conectado.
    let mut dentro = vec![false; nodos.len()];
    dentro[0] = true;
    let mut lineas = Vec::new();
    for _ in 1..nodos.len() {
        let mut mejor: Option<(f32, usize, usize)> = None;
        for (i, _) in nodos.iter().enumerate().filter(|(i, _)| dentro[*i]) {
            for (j, _) in nodos.iter().enumerate().filter(|(j, _)| !dentro[*j]) {
                let d = (nodos[i].0 - nodos[j].0).hypot(nodos[i].1 - nodos[j].1);
                if mejor.map_or(true, |(m, _, _)| d < m) {
                    mejor = Some((d, i, j));
                }
            }
        }
        let Some((_, i, j)) = mejor else { break };
        dentro[j] = true;
        let id = lineas.len() as u32;
        lineas.push(Linea { a: nodos[i], b: nodos[j], id });
    }
    lineas
}

/// Las líneas cuyo riel pasa a menos de `radio` tiles de `(x, z)`.
pub fn lineas_cerca(body: CelestialBody, x: f32, z: f32, radio: f32) -> Vec<Linea> {
    red(body).into_iter().filter(|l| l.distancia_a(x, z) <= radio).collect()
}

/// Distancia recorrida desde `a` en el instante `t` (segundos) y si va de `a` hacia `b`.
pub fn recorrido(l: &Linea, t: f32) -> (f32, bool) {
    let viaje = l.viaje_s();
    let ph = (t + l.id as f32 * 7.3).rem_euclid(l.ciclo_s());
    let suave = |u: f32| u * u * (3.0 - 2.0 * u);
    if ph < PARADA_S {
        (0.0, true)
    } else if ph < PARADA_S + viaje {
        (l.largo() * suave((ph - PARADA_S) / viaje), true)
    } else if ph < 2.0 * PARADA_S + viaje {
        (l.largo(), false)
    } else {
        (l.largo() * (1.0 - suave((ph - 2.0 * PARADA_S - viaje) / viaje)), false)
    }
}

/// Posición `(x, z)` del centro del tren y su rumbo (versor) en el instante `t`.
pub fn posicion(l: &Linea, t: f32) -> ((f32, f32), (f32, f32)) {
    let (s, ida) = recorrido(l, t);
    let (dx, dz) = l.dir();
    ((l.a.0 + dx * s, l.a.1 + dz * s), if ida { (dx, dz) } else { (-dx, -dz) })
}

/// ¿Está el punto `(x, z)` bajo el tren (largo `largo_tren`)? Sirve para el atropello.
pub fn bajo_el_tren(l: &Linea, t: f32, largo_tren: f32, x: f32, z: f32) -> bool {
    let ((tx, tz), (dx, dz)) = posicion(l, t);
    let (rx, rz) = (x - tx, z - tz);
    let along = rx * dx + rz * dz;
    let lateral = (rx * -dz + rz * dx).abs();
    lateral < 1.8 && along.abs() < largo_tren * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn una_linea() -> Linea {
        Linea { a: (0.0, 0.0), b: (3000.0, 4000.0), id: 3 }
    }

    #[test]
    fn el_tren_para_en_las_dos_estaciones_y_va_y_viene() {
        let l = una_linea();
        assert_eq!(l.largo(), 5000.0);
        let (mut ida, mut vuelta, mut t) = (false, false, 0.0);
        while t < l.ciclo_s() {
            let (s, este) = recorrido(&l, t);
            assert!(s >= -0.01 && s <= l.largo() + 0.01);
            ida |= este && s > 100.0;
            vuelta |= !este && s < l.largo() - 100.0;
            t += 0.5;
        }
        assert!(ida && vuelta);
    }

    #[test]
    fn el_tren_no_se_teletransporta() {
        let l = una_linea();
        let mut prev = recorrido(&l, 0.0).0;
        let mut t = 0.1;
        while t < l.ciclo_s() * 2.0 {
            let s = recorrido(&l, t).0;
            assert!((s - prev).abs() < V_MAX * 0.1 * 1.01, "salto de {} en t={t}", (s - prev).abs());
            prev = s;
            t += 0.1;
        }
    }

    #[test]
    fn la_distancia_al_riel_y_el_atropello_se_miden_en_la_diagonal() {
        let l = una_linea();
        assert!(l.distancia_a(1500.0, 2000.0) < 0.01);
        assert!((l.distancia_a(1500.0 + 5.0 * 0.8, 2000.0 - 5.0 * 0.6) - 5.0).abs() < 0.01);
        let t = -(l.id as f32) * 7.3 + 1.0; // en la estación `a`
        assert!(bajo_el_tren(&l, t, 40.0, 0.0, 0.0));
        assert!(!bajo_el_tren(&l, t, 40.0, 100.0, 0.0));
    }

    /// **Todas las ciudades y pueblos de la Tierra quedan en UNA sola red** (árbol: n-1 líneas, sin islas).
    #[test]
    fn la_red_de_la_tierra_conecta_todas_las_ciudades() {
        let body = CelestialBody::Earth;
        let mut nodos = 0usize;
        for sx in -BARRIDO..=BARRIDO {
            for sz in -BARRIDO..=BARRIDO {
                if let Some(s) = settlement_in(body, sx, sz) {
                    if matches!(s.tier, SettlementTier::City | SettlementTier::Town) {
                        nodos += 1;
                    }
                }
            }
        }
        let lineas = red(body);
        assert!(nodos >= 10, "muy pocas ciudades: {nodos}");
        assert_eq!(lineas.len(), nodos - 1, "{} líneas para {nodos} ciudades", lineas.len());
        // Conectividad por unión de extremos.
        let mut grupo: Vec<usize> = (0..lineas.len() * 2).collect();
        fn raiz(g: &mut Vec<usize>, i: usize) -> usize {
            if g[i] != i {
                let r = raiz(g, g[i]);
                g[i] = r;
            }
            g[i]
        }
        let puntos: Vec<(f32, f32)> = lineas.iter().flat_map(|l| [l.a, l.b]).collect();
        for i in 0..puntos.len() {
            for j in 0..i {
                if puntos[i] == puntos[j] {
                    let (ri, rj) = (raiz(&mut grupo, i), raiz(&mut grupo, j));
                    grupo[ri] = rj;
                }
            }
        }
        for l in 0..lineas.len() {
            let (ra, rb) = (raiz(&mut grupo, 2 * l), raiz(&mut grupo, 2 * l + 1));
            grupo[ra] = rb;
        }
        let r0 = raiz(&mut grupo, 0);
        assert!((0..grupo.len()).all(|i| raiz(&mut grupo, i) == r0), "la red tiene islas");
    }
}
