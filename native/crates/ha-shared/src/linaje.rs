//! **Linaje de los vecinos**: etapas de la vida, parejas y parentesco. Puro y determinista:
//! lo usa `ha-game::society` para decidir quién se empareja, quién tiene hijos y cómo se ve
//! cada uno según su edad.
//!
//! La edad de un vecino se mide en segundos de simulación (`Npc::age`). Los fundadores
//! (sembrados al poblar, sin padres) son adultos desde el primer día: sólo quien NACE en el
//! juego pasa por la niñez.

/// Hasta esta edad (s de sim) un nacido en el juego es niño: más bajo, no trabaja, no se empareja.
pub const NINO_S: f32 = 1200.0;
/// Desde esta edad (s de sim) es anciano: ya no tiene hijos y se mueve más despacio.
pub const ANCIANO_S: f32 = 14_000.0;
/// Hasta dónde (m) una pareja se considera "junta" para tener un hijo.
pub const RADIO_PAREJA: f32 = 10.0;
/// Hasta dónde (m) dos solteros pueden cruzarse lo bastante como para enamorarse.
pub const RADIO_CORTEJO: f32 = 14.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Etapa {
    Nino,
    Adulto,
    Anciano,
}

impl Etapa {
    pub fn etiqueta(self) -> &'static str {
        match self {
            Etapa::Nino => "niño",
            Etapa::Adulto => "adulto",
            Etapa::Anciano => "anciano",
        }
    }
}

/// La etapa según la edad. `nacido` = nació dentro del juego (tiene padres); un fundador
/// nunca es niño.
pub fn etapa(edad: f32, nacido: bool) -> Etapa {
    if nacido && edad < NINO_S {
        Etapa::Nino
    } else if edad >= ANCIANO_S {
        Etapa::Anciano
    } else {
        Etapa::Adulto
    }
}

/// Qué tan grande se dibuja (1 = adulto): un recién nacido mide el 60 % y crece hasta el 100 %.
pub fn escala_por_edad(edad: f32, nacido: bool) -> f32 {
    if nacido && edad < NINO_S {
        0.6 + 0.4 * (edad / NINO_S).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// Estable por uid (la skin de la hoja de personaje se elige con el mismo uid).
pub fn es_mujer(uid: u64) -> bool {
    crate::sims::hash(uid ^ 0x5EED_F00D) & 1 == 1
}

/// Lo que se sabe de quién es hijo de quién (0 = no se sabe / fundador).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Parentesco {
    pub padre: u64,
    pub madre: u64,
}

/// ¿Son parientes cercanos (padre/hijo o hermanos)? Los fundadores no tienen padres
/// conocidos, así que entre ellos nunca hay parentesco.
pub fn son_parientes(a: u64, pa: Parentesco, b: u64, pb: Parentesco) -> bool {
    a == b
        || [pa.padre, pa.madre].contains(&b)
        || [pb.padre, pb.madre].contains(&a)
        || (pa.padre != 0 && pa.padre == pb.padre)
        || (pa.madre != 0 && pa.madre == pb.madre)
}

/// ¿Pueden formar pareja? Dos adultos (no ancianos), de distinto sexo, sin parentesco, y que
/// no se caigan mal (`afinidad` de uno por el otro, -1..1).
pub fn pueden_ser_pareja(
    a: u64,
    edad_a: f32,
    nacido_a: bool,
    pa: Parentesco,
    b: u64,
    edad_b: f32,
    nacido_b: bool,
    pb: Parentesco,
    afinidad: f32,
) -> bool {
    etapa(edad_a, nacido_a) == Etapa::Adulto
        && etapa(edad_b, nacido_b) == Etapa::Adulto
        && es_mujer(a) != es_mujer(b)
        && !son_parientes(a, pa, b, pb)
        && afinidad > -0.2
}

/// Probabilidad (0..1) de que dos solteros compatibles se emparejen en una ronda de cortejo:
/// la afinidad la sube; sin trato previo es baja pero no cero (se conocen del pueblo).
pub fn chance_de_cortejo(afinidad: f32) -> f32 {
    (0.06 + 0.5 * afinidad.max(0.0)).clamp(0.0, 0.6)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sims::{Personalidad, Vida};

    #[test]
    fn el_fundador_nunca_es_nino_y_el_nacido_si() {
        assert_eq!(etapa(0.0, false), Etapa::Adulto);
        assert_eq!(etapa(0.0, true), Etapa::Nino);
        assert_eq!(etapa(NINO_S, true), Etapa::Adulto);
        assert_eq!(etapa(ANCIANO_S, false), Etapa::Anciano);
        assert!(escala_por_edad(0.0, true) < escala_por_edad(NINO_S * 0.5, true));
        assert_eq!(escala_por_edad(5.0, false), 1.0);
    }

    #[test]
    fn el_parentesco_cierra_la_puerta() {
        let (a, b, c) = (10, 11, 12);
        let hijo = Parentesco { padre: a, madre: b };
        assert!(son_parientes(c, hijo, a, Parentesco::default()));
        let hermano = Parentesco { padre: a, madre: b };
        assert!(son_parientes(13, hermano, 14, hijo));
        assert!(!son_parientes(20, Parentesco::default(), 21, Parentesco::default()));
    }

    #[test]
    fn la_pareja_pide_adultos_de_distinto_sexo_y_sin_parentesco() {
        let mut hombre = 1u64;
        while es_mujer(hombre) {
            hombre += 1;
        }
        let mut mujer = 1u64;
        while !es_mujer(mujer) {
            mujer += 1;
        }
        let p0 = Parentesco::default();
        assert!(pueden_ser_pareja(hombre, 100.0, false, p0, mujer, 100.0, false, p0, 0.0));
        // un niño no
        assert!(!pueden_ser_pareja(hombre, 10.0, true, p0, mujer, 100.0, false, p0, 0.0));
        // enemigos no
        assert!(!pueden_ser_pareja(hombre, 100.0, false, p0, mujer, 100.0, false, p0, -0.6));
        // parientes no
        let hijo = Parentesco { padre: hombre, madre: 0 };
        assert!(!pueden_ser_pareja(mujer, 2000.0, true, hijo, hombre, 100.0, false, p0, 0.0));
    }

    #[test]
    fn la_cria_mezcla_a_los_dos_y_no_es_clon_de_ninguno() {
        let a = Vida::nueva(1);
        let b = Vida::nueva(2);
        let mut de_a = 0;
        let mut de_b = 0;
        let mut distintos = 0;
        for uid in 100..140u64 {
            let h = Vida::hija_de_pareja(&a, &b, uid);
            assert!((-1.0..=1.0).contains(&h.pers.moral));
            for i in 0..h.pers.w.len() {
                assert!((0.5..=1.5).contains(&h.pers.w[i]));
                if (h.pers.w[i] - a.pers.w[i]).abs() < (h.pers.w[i] - b.pers.w[i]).abs() {
                    de_a += 1;
                } else {
                    de_b += 1;
                }
            }
            if h.pers.w != a.pers.w && h.pers.w != b.pers.w {
                distintos += 1;
            }
        }
        assert!(de_a > 40 && de_b > 40, "el crossover tiene que salir de los dos ({de_a}/{de_b})");
        assert_eq!(distintos, 40);
        // determinista
        let x = Personalidad::cruzar(&a.pers, &b.pers, 7, 0.1);
        let y = Personalidad::cruzar(&a.pers, &b.pers, 7, 0.1);
        assert_eq!(x.w, y.w);
    }

    #[test]
    fn el_cortejo_sube_con_la_afinidad_y_nunca_es_seguro() {
        assert!(chance_de_cortejo(0.0) > 0.0);
        assert!(chance_de_cortejo(0.9) > chance_de_cortejo(0.0));
        assert!(chance_de_cortejo(1.0) <= 0.6);
    }
}
