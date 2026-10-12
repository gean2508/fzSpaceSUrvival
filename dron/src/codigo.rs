//! Cómo se le habla al cerebro: el código de sensores.
//!
//! **Esto no es un detalle de formato, es la interfaz con la arquitectura.** El
//! kernel IAFZ-64 no es una red de multiplicaciones: cada neurona calcula
//! `popcount(~(contexto ^ pesos)) >= umbral`, o sea que dispara cuando el
//! contexto se PARECE a un patrón guardado, medido en distancia de Hamming. Una
//! neurona es literalmente un detector de vecindad de Hamming y nada más.
//!
//! De ahí sale el único requisito que tiene que cumplir el código: **valores
//! numéricamente parecidos tienen que dar patrones de bits parecidos**. Si no,
//! no existe ningún patrón de pesos que signifique "estoy cerca del objetivo",
//! porque "cerca" no es un vecindario de Hamming.
//!
//! El código anterior era binario puro (`(x/rango)*niveles` escrito en base 2)
//! y fallaba eso de la peor manera posible. Medido sobre ±14 m con 6 bits
//! (`corr` entre |Δvalor| y distancia de Hamming, 281 valores × 281):
//!
//! ```text
//!   binario 6 bits       correlación 0.378
//!   termómetro 6 bits    correlación 0.963
//!   termómetro 12 bits   correlación 0.990
//! ```
//!
//! Y el caso concreto, distancia de Hamming desde "estoy encima del objetivo":
//!
//! ```text
//!            binario 6 bits   termómetro 12 bits
//!   a  0.5 m       5                 0
//!   a  2.0 m       5                 1
//!   a  8.0 m       4                 3
//!   a 14.0 m       1                 6
//! ```
//!
//! Con el código binario, estar a 14 metros era MÁS parecido a estar encima del
//! objetivo que estar a medio metro. Eso explica el orbitar que figuraba como
//! deuda #1: el genoma no estaba eligiendo orbitar, es que no tenía forma de
//! notar la diferencia entre "a 1.5 m" y "llegué".
//!
//! ## Termómetro, y por qué se paga el precio
//!
//! Un termómetro de `b` bits sólo representa `b+1` valores, contra `2^b` del
//! binario: 12 bits pasan de 4096 estados a 13. Es un desperdicio enorme de
//! entropía y vale la pena igual, porque la entropía no era el cuello de
//! botella — la EXPRESIVIDAD sí. Un código de 4096 estados que el kernel no
//! puede sintonizar vale menos que uno de 13 que sí.
//!
//! Además sale gratis un efecto útil: en un termómetro, la cantidad de bits de
//! un campo es cuánto PESA ese campo en el voto del popcount. Darle 12 bits a
//! "dónde está el objetivo" y 3 a la divergencia no es sólo resolución, es
//! prioridad.
//!
//! ## Compresión: resolución donde hace falta
//!
//! Un termómetro lineal reparte la resolución pareja, y este problema no es
//! parejo: a 10 m del objetivo alcanza con saber el rumbo, a 0.5 m hay que
//! saber el metro exacto o no se puede frenar. Por eso el valor pasa antes por
//! `|u|^0.6 · signo(u)`, que amontona niveles cerca de cero. Con 12 bits sobre
//! ±14 m los cortes quedan en:
//!
//! ```text
//!   0.22   1.39   3.26   5.70   8.67   12.1  (metros)
//! ```
//!
//! 0.22 m cerca del objetivo —del orden del ruido del GPS, 0.25 m, así que no
//! se gastan niveles en decimales que el sensor no tiene— y 3.4 m por nivel a
//! diez metros, donde da lo mismo.

/// Exponente de compresión. 1.0 = lineal. 0.6 se eligió para que el primer
/// corte (0.22 m sobre ±14 m) caiga sobre el ruido de posición del GPS
/// (`Ruido::REAL.posicion` = 0.25 m): más fino que eso son niveles gastados en
/// distinguir ruido, más grueso deja al dron sin poder apuntar la frenada.
pub const COMPRESION: f32 = 0.6;

/// Lineal: sin compresión. Para ángulos y velocidades angulares, donde la
/// resolución sí hace falta pareja (un alabeo de 30° importa tanto como uno
/// de 5°, no son "más lejos" de nada).
pub const LINEAL: f32 = 1.0;

fn comprimir(u: f32, exp: f32) -> f32 {
    let u = u.clamp(-1.0, 1.0);
    if exp == 1.0 {
        u
    } else {
        u.abs().powf(exp) * u.signum()
    }
}

/// `n` bits bajos en 1. El termómetro propiamente dicho: la distancia de
/// Hamming entre dos máscaras es exactamente la diferencia de niveles.
fn mascara(n: u32) -> u64 {
    if n == 0 {
        0
    } else {
        u64::MAX >> (64 - n.min(64))
    }
}

/// Valor CON SIGNO en [-rango, rango] → termómetro de `bits` bits. El nivel del
/// medio (cero) enciende la mitad de los bits, así que alejarse para cualquiera
/// de los dos lados cuesta Hamming y los dos lados son distinguibles.
pub fn termo_signo(x: f32, rango: f32, bits: u32, exp: f32) -> u64 {
    let u = comprimir(x / rango, exp);
    let n = ((u * 0.5 + 0.5) * bits as f32).round().clamp(0.0, bits as f32) as u32;
    mascara(n)
}

/// Valor SIN SIGNO en [0, rango] → termómetro de `bits` bits.
pub fn termo(x: f32, rango: f32, bits: u32, exp: f32) -> u64 {
    let u = comprimir((x / rango).clamp(0.0, 1.0), exp);
    let n = (u * bits as f32).round().clamp(0.0, bits as f32) as u32;
    mascara(n)
}

/// Arma la palabra de 96 bits campo por campo, a caballo entre `lo` (bits
/// 0..63) y `hi` (64..95). Lleva la cuenta sola para que agregar un campo no
/// obligue a recalcular desplazamientos a mano — que es de donde salen los
/// errores que después parecen problemas de entrenamiento.
pub struct Empaque {
    lo: u64,
    hi: u32,
    n: u32,
}

impl Default for Empaque {
    fn default() -> Self {
        Self::nuevo()
    }
}

impl Empaque {
    pub fn nuevo() -> Self {
        Empaque { lo: 0, hi: 0, n: 0 }
    }

    /// Bits usados hasta ahora. `sensores` lo verifica contra los 96 del kernel.
    pub fn usados(&self) -> u32 {
        self.n
    }

    pub fn mete(&mut self, patron: u64, bits: u32) {
        let patron = patron & mascara(bits);
        if self.n + bits <= 64 {
            self.lo |= patron << self.n;
        } else if self.n < 64 {
            // Un campo puede quedar partido entre las dos palabras.
            let cabe = 64 - self.n;
            self.lo |= (patron & mascara(cabe)) << self.n;
            self.hi |= (patron >> cabe) as u32;
        } else {
            self.hi |= (patron as u32) << (self.n - 64);
        }
        self.n += bits;
    }

    pub fn listo(&self) -> (u64, u32) {
        (self.lo, self.hi)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// La propiedad que justifica todo el archivo: Hamming = diferencia de
    /// niveles. Si esto se rompe, el cerebro deja de poder sintonizar rangos.
    #[test]
    fn hamming_sigue_al_valor() {
        let mut anterior = None;
        let mut ultimo_h = 0;
        for i in 0..=100 {
            let x = -14.0 + i as f32 * 0.28;
            let p = termo_signo(x, 14.0, 12, COMPRESION);
            let h = (p ^ termo_signo(0.0, 14.0, 12, COMPRESION)).count_ones();
            if let Some(prev) = anterior {
                // Monótona: alejarse del cero nunca acerca en Hamming.
                if x < 0.0 {
                    assert!(h <= prev, "en {x} la distancia de Hamming subió al acercarse");
                }
            }
            anterior = Some(h);
            ultimo_h = h;
        }
        assert_eq!(ultimo_h, 6, "±14 m tiene que dar la mitad de los 12 bits");
    }

    #[test]
    fn empaque_reparte_bien_entre_palabras() {
        let mut e = Empaque::nuevo();
        e.mete(mascara(60), 60);
        e.mete(mascara(12), 12); // este queda partido: 4 en lo, 8 en hi
        e.mete(mascara(24), 24);
        let (lo, hi) = e.listo();
        assert_eq!(e.usados(), 96);
        assert_eq!(lo.count_ones() + hi.count_ones(), 96);
    }
}
