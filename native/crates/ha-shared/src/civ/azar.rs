//! Azar determinista y serializable: el estado viaja en el guardado, así que la misma
//! partida cargada dos veces vive la misma historia.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Azar(pub u64);

impl Azar {
    pub fn nuevo(semilla: u64) -> Self {
        Self(semilla ^ 0x5DEE_CE66_D1CE_4E5B)
    }

    pub fn u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `[0, 1)`
    pub fn f(&mut self) -> f32 {
        (self.u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn rango(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }

    pub fn p(&mut self, prob: f32) -> bool {
        self.f() < prob
    }

    pub fn idx(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.u64() % n as u64) as usize
        }
    }

    /// Campana aproximada en `[-1, 1]` (suma de tres uniformes).
    pub fn campana(&mut self) -> f32 {
        (self.f() + self.f() + self.f()) * (2.0 / 3.0) - 1.0
    }
}

/// Hash puro (sin estado) para derivar cosas del sitio: nombres, sustratos, rasgos.
pub fn hash(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

pub fn unidad(x: u64) -> f32 {
    (hash(x) >> 40) as f32 / (1u64 << 24) as f32
}
