//! **Fase de los semáforos** (5 oct 2026): una función pura del reloj, compartida por el dibujo (qué lámpara está prendida)
//! y por el tránsito (quién frena). Los dos ejes nunca están en verde a la vez: el que no tiene paso está en rojo.

/// La lámpara prendida de un semáforo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Luz {
    Roja,
    Amarilla,
    Verde,
}

/// Segundos de un ciclo completo.
pub const CICLO_S: f32 = 30.0;
const VERDE_S: f32 = 12.0;
const AMARILLA_S: f32 = 3.0;

/// La luz del eje este-oeste (`ew`) o norte-sur en el instante `t_s`. El eje E-O va desfasado medio ciclo.
pub fn luz(t_s: f32, ew: bool) -> Luz {
    let p = (t_s + if ew { CICLO_S * 0.5 } else { 0.0 }).rem_euclid(CICLO_S);
    if p < VERDE_S {
        Luz::Verde
    } else if p < VERDE_S + AMARILLA_S {
        Luz::Amarilla
    } else {
        Luz::Roja
    }
}

/// ¿Se puede entrar al cruce viniendo por ese eje? (el amarillo no: se frena).
pub fn hay_paso(t_s: f32, ew: bool) -> bool {
    luz(t_s, ew) == Luz::Verde
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_dos_ejes_nunca_tienen_verde_a_la_vez() {
        let mut t = 0.0;
        while t < 120.0 {
            assert!(!(hay_paso(t, true) && hay_paso(t, false)), "t={t}");
            t += 0.25;
        }
    }

    #[test]
    fn cada_eje_pasa_por_las_tres_luces() {
        for ew in [true, false] {
            let vistas: std::collections::HashSet<_> = (0..300).map(|i| format!("{:?}", luz(i as f32 * 0.1, ew))).collect();
            assert_eq!(vistas.len(), 3);
        }
    }
}
