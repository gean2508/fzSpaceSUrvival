//! Buscador de caminos A* sobre la grilla de metros, **incremental**: se avanza unos
//! cuantos nodos por cuadro, así que rodear una manzana entera no cuesta un tirón.
//!
//! Puro (sin Bevy) y ciego a QUÉ es un obstáculo: le pasan `libre(tx, tz)` (¿se puede
//! estar parado en el centro de esa celda?). Lo usan los vecinos cuando chocan contra un
//! edificio que no es el suyo (`ha-game/society.rs`); el jugador se mueve solo.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Tope de nodos expandidos por búsqueda: más allá, el destino no vale el paseo.
pub const MAX_NODOS: usize = 2500;

const VECINOS: [(i64, i64, u32); 8] =
    [(1, 0, 10), (-1, 0, 10), (0, 1, 10), (0, -1, 10), (1, 1, 14), (1, -1, 14), (-1, 1, 14), (-1, -1, 14)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Estado {
    Buscando,
    Encontrada,
    Sin,
}

pub struct Buscador {
    meta: (i64, i64),
    abiertos: BinaryHeap<Reverse<(u32, u32, (i64, i64))>>,
    costo: HashMap<(i64, i64), u32>,
    padre: HashMap<(i64, i64), (i64, i64)>,
    nodos: usize,
    estado: Estado,
    ultimo: (i64, i64),
}

fn h(a: (i64, i64), b: (i64, i64)) -> u32 {
    let (dx, dz) = ((a.0 - b.0).unsigned_abs() as u32, (a.1 - b.1).unsigned_abs() as u32);
    10 * dx.max(dz) + 4 * dx.min(dz)
}

impl Buscador {
    /// Busca de la celda de `desde` a la de `hasta` (coordenadas de mundo, metros).
    pub fn nuevo(desde: (f32, f32), hasta: (f32, f32)) -> Self {
        let ini = (desde.0.floor() as i64, desde.1.floor() as i64);
        let meta = (hasta.0.floor() as i64, hasta.1.floor() as i64);
        let mut b = Self {
            meta,
            abiertos: BinaryHeap::new(),
            costo: HashMap::new(),
            padre: HashMap::new(),
            nodos: 0,
            estado: Estado::Buscando,
            ultimo: ini,
        };
        b.costo.insert(ini, 0);
        b.abiertos.push(Reverse((h(ini, meta), 0, ini)));
        b
    }

    pub fn estado(&self) -> Estado {
        self.estado
    }

    /// Expande hasta `n` nodos. La celda de salida y la de llegada se dan por transitables:
    /// quien busca ya está parado en la primera, y la segunda puede ser el umbral de una
    /// puerta que se abre recién cuando llegue.
    pub fn paso(&mut self, n: usize, libre: &mut dyn FnMut(i64, i64) -> bool) -> Estado {
        let mut hechos = 0;
        while self.estado == Estado::Buscando && hechos < n {
            let Some(Reverse((_, g, cel))) = self.abiertos.pop() else {
                self.estado = Estado::Sin;
                break;
            };
            if self.costo.get(&cel).map_or(false, |&c| c < g) {
                continue;
            }
            self.ultimo = cel;
            if cel == self.meta {
                self.estado = Estado::Encontrada;
                break;
            }
            self.nodos += 1;
            hechos += 1;
            if self.nodos > MAX_NODOS {
                self.estado = Estado::Sin;
                break;
            }
            for (dx, dz, c) in VECINOS {
                let nx = (cel.0 + dx, cel.1 + dz);
                let ok = |p: (i64, i64), libre: &mut dyn FnMut(i64, i64) -> bool| p == self.meta || libre(p.0, p.1);
                if !ok(nx, libre) {
                    continue;
                }
                // Una diagonal no atraviesa la esquina de una pared.
                if dx != 0 && dz != 0 && !(ok((cel.0 + dx, cel.1), libre) && ok((cel.0, cel.1 + dz), libre)) {
                    continue;
                }
                let g2 = g + c;
                if self.costo.get(&nx).map_or(true, |&v| g2 < v) {
                    self.costo.insert(nx, g2);
                    self.padre.insert(nx, cel);
                    self.abiertos.push(Reverse((g2 + h(nx, self.meta), g2, nx)));
                }
            }
        }
        self.estado
    }

    /// El camino en metros (centros de celda), sin la celda de salida y sin curvas de
    /// más: se tira todo punto intermedio que se pueda saltar caminando derecho.
    pub fn camino(&self, desde: (f32, f32), libre: &mut dyn FnMut(i64, i64) -> bool) -> Vec<(f32, f32)> {
        if self.estado != Estado::Encontrada {
            return Vec::new();
        }
        let mut celdas = vec![self.meta];
        let mut c = self.meta;
        while let Some(&p) = self.padre.get(&c) {
            celdas.push(p);
            c = p;
        }
        celdas.reverse();
        let centro = |c: (i64, i64)| (c.0 as f32 + 0.5, c.1 as f32 + 0.5);
        let mut out: Vec<(f32, f32)> = Vec::new();
        let mut ancla = desde;
        let mut i = 1;
        while i < celdas.len() {
            // El punto más lejano de la cadena que se ve derecho desde el ancla.
            let mut j = i;
            for k in (i..celdas.len()).rev() {
                if derecho(ancla, centro(celdas[k]), libre) {
                    j = k;
                    break;
                }
            }
            let p = centro(celdas[j]);
            out.push(p);
            ancla = p;
            i = j + 1;
        }
        out
    }
}

/// ¿Se llega de `a` a `b` en línea recta pisando sólo celdas libres?
fn derecho(a: (f32, f32), b: (f32, f32), libre: &mut dyn FnMut(i64, i64) -> bool) -> bool {
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    let largo = (dx * dx + dz * dz).sqrt();
    let pasos = (largo / 0.35).ceil().max(1.0) as i32;
    let mut previa = (a.0.floor() as i64, a.1.floor() as i64);
    for i in 1..=pasos {
        let t = i as f32 / pasos as f32;
        let (x, z) = (a.0 + dx * t, a.1 + dz * t);
        let c = (x.floor() as i64, z.floor() as i64);
        if c == previa {
            continue;
        }
        let meta = (b.0.floor() as i64, b.1.floor() as i64);
        if c != meta && !libre(c.0, c.1) {
            return false;
        }
        // Cambiar de celda en diagonal roza la esquina: hay que poder pasar por las dos vecinas.
        if c.0 != previa.0 && c.1 != previa.1 && !(libre(c.0, previa.1) && libre(previa.0, c.1)) {
            return false;
        }
        previa = c;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn correr(desde: (f32, f32), hasta: (f32, f32), pared: &dyn Fn(i64, i64) -> bool) -> (Estado, Vec<(f32, f32)>) {
        let mut libre = |x: i64, z: i64| !pared(x, z);
        let mut b = Buscador::nuevo(desde, hasta);
        while b.paso(50, &mut libre) == Estado::Buscando {}
        let c = b.camino(desde, &mut libre);
        (b.estado(), c)
    }

    #[test]
    fn rodea_un_edificio() {
        // Un edificio de 12 x 8 entre el origen y la meta.
        let pared = |x: i64, z: i64| (0..12).contains(&x) && (0..8).contains(&z);
        let (e, camino) = correr((-3.5, 4.5), (15.5, 4.5), &pared);
        assert_eq!(e, Estado::Encontrada);
        assert!(!camino.is_empty());
        // Ningún tramo del camino cruza el edificio.
        let mut previo = (-3.5, 4.5);
        for &p in &camino {
            for i in 0..=20 {
                let t = i as f32 / 20.0;
                let (x, z) = (previo.0 + (p.0 - previo.0) * t, previo.1 + (p.1 - previo.1) * t);
                assert!(!pared(x.floor() as i64, z.floor() as i64), "el camino atraviesa la pared en {x},{z}");
            }
            previo = p;
        }
        assert_eq!(*camino.last().unwrap(), (15.5, 4.5));
    }

    #[test]
    fn en_campo_abierto_va_derecho() {
        let (e, camino) = correr((0.5, 0.5), (20.5, 3.5), &|_, _| false);
        assert_eq!(e, Estado::Encontrada);
        assert_eq!(camino.len(), 1, "un solo tramo: {camino:?}");
    }

    #[test]
    fn un_recinto_cerrado_no_tiene_camino() {
        let pared = |x: i64, z: i64| (x.abs() == 3 || z.abs() == 3) && x.abs() <= 3 && z.abs() <= 3;
        let (e, camino) = correr((0.5, 0.5), (10.5, 0.5), &pared);
        assert_eq!(e, Estado::Sin);
        assert!(camino.is_empty());
    }
}
