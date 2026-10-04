//! **Clases de infectado y puntos** (la sensación de Black Ops 2 Zombies: matar a lo loco, esquivar,
//! que cada baja pague). No hay rondas: el mundo es abierto y la presión la marca la pandemia
//! (`pandemia.rs`), no un reloj de rondas.
//!
//! * Tres clases: el **común**, el **corredor** (corre más, aguanta menos) y el **tanque** (lento,
//!   aguanta el doble). Cuanto más avanzada la pandemia, más raras aparecen.
//! * Puntos por baja según la clase, con un plus por avance de la pandemia.

/// Clase de infectado. Define velocidad, vida y lo que vale la baja.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clase {
    Comun,
    Corredor,
    Tanque,
}

impl Clase {
    /// Multiplicador de velocidad sobre la base de `infectados` (`VEL`).
    pub fn velocidad(self) -> f32 {
        match self {
            Clase::Comun => 1.0,
            Clase::Corredor => 1.6,
            Clase::Tanque => 0.7,
        }
    }

    /// Multiplicador de vida sobre `infectados::VIDA`.
    pub fn vida(self) -> f32 {
        match self {
            Clase::Comun => 1.0,
            Clase::Corredor => 0.7,
            Clase::Tanque => 2.2,
        }
    }
}

/// Clase de un infectado según el avance de la pandemia (0-100) y un sorteo `r` en [0, 1).
pub fn clase_de(avance: f32, r: f32) -> Clase {
    if avance >= 60.0 && r < 0.10 {
        Clase::Tanque
    } else if avance >= 25.0 && r < 0.30 {
        Clase::Corredor
    } else {
        Clase::Comun
    }
}

/// Lo que vale una baja: 50 el común, 70 el corredor, 120 el tanque, y hasta +100 por avance.
pub fn puntos_de(clase: Clase, avance: f32) -> u32 {
    let base = match clase {
        Clase::Comun => 50,
        Clase::Corredor => 70,
        Clase::Tanque => 120,
    };
    base + (avance.clamp(0.0, 100.0) as u32 / 10) * 10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_clases_raras_aparecen_con_el_avance() {
        assert_eq!(clase_de(0.0, 0.0), Clase::Comun);
        assert_eq!(clase_de(30.0, 0.1), Clase::Corredor);
        assert_eq!(clase_de(80.0, 0.05), Clase::Tanque);
        assert_eq!(clase_de(80.0, 0.5), Clase::Comun);
    }

    #[test]
    fn el_tanque_paga_mas_y_el_avance_suma() {
        assert!(puntos_de(Clase::Tanque, 0.0) > puntos_de(Clase::Corredor, 0.0));
        assert!(puntos_de(Clase::Comun, 80.0) > puntos_de(Clase::Comun, 0.0));
        assert_eq!(puntos_de(Clase::Comun, 0.0), 50);
    }
}
