//! **Ropa y armadura** hechas con las pieles y la lana de los animales.
//!
//! Cada prenda ocupa una ranura del cuerpo y le quita una fracción al daño que recibís de
//! animales (mordidas) y de la radiación. No protege del hambre, de la sed ni de la asfixia:
//! eso no entra por la piel.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ranura {
    Cabeza,
    Torso,
    Pies,
}

impl Ranura {
    pub const TODAS: [Ranura; 3] = [Ranura::Cabeza, Ranura::Torso, Ranura::Pies];

    pub fn indice(self) -> usize {
        match self {
            Ranura::Cabeza => 0,
            Ranura::Torso => 1,
            Ranura::Pies => 2,
        }
    }

    pub fn nombre(self) -> &'static str {
        match self {
            Ranura::Cabeza => "cabeza",
            Ranura::Torso => "torso",
            Ranura::Pies => "pies",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Prenda {
    pub id: &'static str,
    pub ranura: Ranura,
    /// Fracción de daño que absorbe (0..1).
    pub reduccion: f32,
}

pub const PRENDAS: &[Prenda] = &[
    Prenda { id: "leather_cap", ranura: Ranura::Cabeza, reduccion: 0.10 },
    Prenda { id: "leather_vest", ranura: Ranura::Torso, reduccion: 0.25 },
    Prenda { id: "wool_coat", ranura: Ranura::Torso, reduccion: 0.20 },
    Prenda { id: "leather_boots", ranura: Ranura::Pies, reduccion: 0.10 },
];

/// Tope de la protección total: con todo puesto nunca sos invulnerable.
pub const REDUCCION_MAX: f32 = 0.5;

/// **La ropa que se ENCUENTRA** (ropero, cadáveres): no se fabrica, así que no tiene receta.
/// `ropa_01..33` es un estilo del creador de personajes cada una, `gorra_*`/`gorro_*` son
/// sombreros y `botas_1..7` calzado. Se arman una vez; la protección sube de a poco con el
/// número para que haya prendas mejores que otras sin tabla a mano.
pub fn es_hallable(id: &str) -> bool {
    id.starts_with("ropa_") || id.starts_with("gorra_") || id.starts_with("gorro_") || id.starts_with("botas_")
}

fn hallables() -> &'static Vec<Prenda> {
    use std::sync::OnceLock;
    static T: OnceLock<Vec<Prenda>> = OnceLock::new();
    T.get_or_init(|| {
        let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
        let mut v = Vec::new();
        for n in 1..=33u32 {
            v.push(Prenda { id: leak(format!("ropa_{n:02}")), ranura: Ranura::Torso, reduccion: 0.08 + 0.02 * (n % 5) as f32 });
        }
        for id in ["gorra_roja", "gorra_azul", "gorro_gris"] {
            v.push(Prenda { id, ranura: Ranura::Cabeza, reduccion: 0.08 });
        }
        for n in 1..=7u32 {
            v.push(Prenda { id: leak(format!("botas_{n}")), ranura: Ranura::Pies, reduccion: 0.06 + 0.01 * n as f32 });
        }
        v
    })
}

/// Todas las ropas encontrables (para sortear botín).
pub fn hallables_ids() -> impl Iterator<Item = &'static str> {
    hallables().iter().map(|p| p.id)
}

/// Una prenda encontrable al azar (para el botín): `n` es cualquier número.
pub fn prenda_al_azar(n: u32) -> &'static str {
    let v = hallables();
    v[(n as usize) % v.len()].id
}

pub fn prenda(item: &str) -> Option<&'static Prenda> {
    PRENDAS.iter().find(|p| p.id == item).or_else(|| if es_hallable(item) { hallables().iter().find(|p| p.id == item) } else { None })
}

/// Protección total de lo que lleva puesto, una prenda por ranura (en el orden de
/// [`Ranura::TODAS`]).
pub fn reduccion_total(puesto: &[Option<&str>; 3]) -> f32 {
    puesto
        .iter()
        .flatten()
        .filter_map(|id| prenda(id))
        .map(|p| p.reduccion)
        .sum::<f32>()
        .min(REDUCCION_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cada_prenda_tiene_ranura_y_receta() {
        for p in PRENDAS {
            assert!(p.reduccion > 0.0 && p.reduccion < 0.5);
            assert!(
                crate::recipes::CRAFT_RECIPES.iter().any(|r| r.output_id == p.id),
                "{} no tiene receta",
                p.id
            );
        }
    }

    #[test]
    fn la_ropa_hallable_es_prenda_sin_receta_y_con_nombre() {
        let mut n = 0;
        for id in hallables_ids() {
            let p = prenda(id).expect("hallable sin prenda");
            assert!(p.reduccion > 0.0 && p.reduccion < 0.3, "{id}");
            assert!(!crate::recipes::CRAFT_RECIPES.iter().any(|r| r.output_id == id), "{id} no se fabrica");
            n += 1;
        }
        assert_eq!(n, 33 + 3 + 7);
        assert!(prenda("ropa_99").is_none());
    }

    #[test]
    fn la_proteccion_suma_y_tiene_tope() {
        assert_eq!(reduccion_total(&[None, None, None]), 0.0);
        let t = reduccion_total(&[Some("leather_cap"), Some("leather_vest"), Some("leather_boots")]);
        assert!((t - 0.45).abs() < 1e-6);
        assert!(reduccion_total(&[Some("leather_cap"), Some("leather_vest"), Some("leather_boots")]) <= REDUCCION_MAX);
        assert_eq!(reduccion_total(&[Some("pan"), None, None]), 0.0, "lo que no es prenda no protege");
    }

    #[test]
    fn el_abrigo_y_el_chaleco_comparten_torso() {
        assert_eq!(prenda("wool_coat").unwrap().ranura, prenda("leather_vest").unwrap().ranura);
    }
}
