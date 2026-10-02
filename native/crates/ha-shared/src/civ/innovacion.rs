//! **Innovaciones: lo que la ciencia hace DESPUÉS de agotar el catálogo** (biblia §17).
//!
//! Medido con `pruebas::actividad_evolutiva_de_la_civilizacion`: los 44 saberes se agotan en 6 de
//! 6 semillas entre el año 300 y el 550, y la novedad del último tercio cae al 27 % de la del
//! primero. Un catálogo finito no es evolución abierta (Taylor y otros, requisito 2: «diversidad
//! ilimitada»). Acá se le pone techo infinito sin inventar contenido a mano: una **innovación** es
//! la COMBINACIÓN de dos cosas que el pueblo ya tiene —saberes u otras innovaciones— y su
//! identidad es un hash de las dos, así que combinar innovaciones con innovaciones no se acaba
//! nunca (el árbol de ensamblaje de Cronin: cada objeto vale por su historia de armado).
//!
//! * **Se descubre solo**, una vez por año y por pueblo vivo: `1 + saberes/10` intentos; cada uno
//!   elige dos piezas al azar del pozo (saberes + innovaciones propias) y, si la combinación no
//!   existía, tiene una chance que crece con la capacidad de investigar (`inv`).
//! * **Vale poco y con rendimiento decreciente**: `bonus` suma logarítmicamente a la capacidad. La
//!   innovación no desbalancea la sim: da un piso de crecimiento, no una rampa.
//! * **Se difunde por comercio** (como los saberes) y **se pierde si el pueblo muere** sin
//!   haberse difundido: la cultura que no se transmite desaparece (§17, regla 9).
//! * Tope duro por pueblo: `MAX_INNOV`. El guardado crece con él (8 B por innovación).

use super::azar::hash;
use super::cronica::Tipo;
use super::mundo::Mundo;
use super::saber::Capacidad;

pub const MAX_INNOV: usize = 4096;

/// Ids chicos (< 64) son saberes; una innovación siempre lleva el bit alto y nunca choca con ellos.
const MARCA: u64 = 1 << 63;

/// El id de la combinación de `a` y `b` (conmutativo).
pub fn combinar(a: u64, b: u64) -> u64 {
    let (x, y) = if a <= b { (a, b) } else { (b, a) };
    MARCA | (hash(x.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ hash(y)) >> 1)
}

/// Lo que suman `n` innovaciones a la capacidad: logarítmico, para que mil no rompan nada.
pub fn bonus(n: usize, c: &mut Capacidad) {
    if n == 0 {
        return;
    }
    let l = ((1 + n) as f32).ln();
    c.inv += 0.04 * l;
    c.comida += 0.01 * l;
    c.mat += 0.01 * l;
}

impl Mundo {
    pub(crate) fn innovacion_anual(&mut self) {
        let ids: Vec<u32> = self.pueblos.iter().filter(|(_, p)| p.vivo).map(|(i, _)| *i).collect();
        for id in ids {
            let (saberes, innov_n, inv) = {
                let p = &self.pueblos[&id];
                (p.saberes, p.innov.len(), p.capacidad().inv)
            };
            if innov_n >= MAX_INNOV {
                continue;
            }
            let intentos = 1 + saberes.cuenta() / 10;
            let chance = (0.12 + 0.05 * inv).clamp(0.05, 0.6);
            for _ in 0..intentos {
                let pozo_n = saberes.cuenta() as usize + self.pueblos[&id].innov.len();
                if pozo_n < 2 {
                    break;
                }
                let (a, b) = {
                    let p = &self.pueblos[&id];
                    let n_s = saberes.cuenta() as usize;
                    let tomar = |i: usize| -> u64 {
                        if i < n_s {
                            saberes.iter().nth(i).unwrap_or(0) as u64
                        } else {
                            p.innov[i - n_s]
                        }
                    };
                    (tomar(self.rng.idx(pozo_n)), tomar(self.rng.idx(pozo_n)))
                };
                if a == b || !self.rng.p(chance) {
                    continue;
                }
                let nueva = combinar(a, b);
                let p = self.pueblos.get_mut(&id).expect("pueblo vivo");
                if let Err(pos) = p.innov.binary_search(&nueva) {
                    p.innov.insert(pos, nueva);
                    let n = p.innov.len();
                    if n % 10 == 0 {
                        let nombre = p.nombre.clone();
                        self.hecho(Tipo::Descubrimiento, id, 0, format!("{nombre} suma su innovación n.º {n}."));
                    }
                }
            }
        }
        self.difundir_innovaciones();
    }

    /// Por comercio, un pueblo le pasa al socio hasta dos innovaciones que no tenía.
    fn difundir_innovaciones(&mut self) {
        let rels: Vec<(u32, u32)> = self.relaciones.iter().filter(|r| r.comercio).map(|r| (r.a, r.b)).collect();
        for (a, b) in rels {
            for (de, para) in [(a, b), (b, a)] {
                let (Some(pd), Some(pp)) = (self.pueblos.get(&de), self.pueblos.get(&para)) else { continue };
                if !pd.vivo || !pp.vivo || pp.innov.len() >= MAX_INNOV {
                    continue;
                }
                let faltan: Vec<u64> = pd.innov.iter().copied().filter(|i| pp.innov.binary_search(i).is_err()).collect();
                if faltan.is_empty() {
                    continue;
                }
                for _ in 0..2.min(faltan.len()) {
                    if !self.rng.p(0.5) {
                        continue;
                    }
                    let i = faltan[self.rng.idx(faltan.len())];
                    if let Some(pp) = self.pueblos.get_mut(&para) {
                        if let Err(pos) = pp.innov.binary_search(&i) {
                            pp.innov.insert(pos, i);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinar_es_conmutativo_distinto_y_nunca_choca_con_un_saber() {
        assert_eq!(combinar(3, 9), combinar(9, 3));
        assert_ne!(combinar(3, 9), combinar(3, 10));
        assert!(combinar(1, 2) >= MARCA, "una innovación se distingue de un saber");
        // Combinar una innovación con otra da una tercera: el árbol no tiene techo.
        let i = combinar(1, 2);
        let j = combinar(3, 4);
        assert_ne!(combinar(i, j), i);
        assert_ne!(combinar(i, j), j);
    }

    #[test]
    fn el_bonus_es_logaritmico_y_cero_sin_innovaciones() {
        let mut c = Capacidad::default();
        bonus(0, &mut c);
        assert_eq!(c.inv, 0.0);
        let (mut c10, mut c1000) = (Capacidad::default(), Capacidad::default());
        bonus(10, &mut c10);
        bonus(1000, &mut c1000);
        assert!(c1000.inv > c10.inv);
        assert!(c1000.inv < c10.inv * 4.0, "mil innovaciones no valen mil veces más");
    }
}
