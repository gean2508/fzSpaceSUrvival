//! **El cerebro que va en el chip** (`csrc/iafz_64_abordo.c`), visto desde
//! acá. Sólo existe para comprobar que es el MISMO cerebro que se entrena:
//! si diera un número distinto, el genoma instalado en el aparato sería otro
//! dron que el que se midió (regla 3.3). Ver `dron chip` para lo que cuesta
//! correrlo en un procesador de 32 bits.

#[repr(C)]
pub struct Estado {
    pub sensores_lo: u64,
    pub sensores_hi: u32,
    pub memoria: u32,
    pub salidas: [f32; 4],
}

extern "C" {
    fn pensar_abordo(cerebro: *const u8, estado: *mut Estado);
}

/// Una decisión del cerebro de a bordo sobre un genoma volcado.
pub fn pensar(cerebro: &[u8], e: &mut Estado) {
    assert_eq!(cerebro.len(), crate::iafz::BRAIN_BYTES);
    unsafe { pensar_abordo(cerebro.as_ptr(), e) }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Bit a bit igual que el kernel del gym: 20 genomas al azar, 300
    /// decisiones cada uno, con la memoria recurrente corriendo (la de a
    /// bordo la lleva el `Estado`; la del gym, el kernel).
    #[test]
    fn igual_al_kernel_del_gym() {
        const SLOT: i32 = 126;
        let mut x = 0x9E37_79B9_7F4A_7C15u64;
        let mut azar = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
        for g in 0..20u32 {
            crate::iafz::init(SLOT, 1000 + g);
            crate::iafz::reset_memoria(SLOT);
            let cerebro = crate::iafz::volcar(SLOT);
            let mut e = Estado { sensores_lo: 0, sensores_hi: 0, memoria: 0, salidas: [0.0; 4] };
            for paso in 0..300 {
                let (lo, hi) = (azar(), azar() as u32);
                let gym = crate::iafz::pensar_multi(SLOT, lo, hi);
                e.sensores_lo = lo;
                e.sensores_hi = hi;
                pensar(&cerebro, &mut e);
                for k in 0..4 {
                    assert_eq!(gym[k].to_bits(), e.salidas[k].to_bits(),
                        "genoma {g}, decisión {paso}, salida {k}: gym {} contra chip {}", gym[k], e.salidas[k]);
                }
            }
        }
    }
}
