//! **Tienda + Pase de batalla — espejo del catálogo** de
//! `packages/shared/src/tienda.ts`.
//!
//! El servidor cobra con el catálogo de TS; el juego dibuja la tienda con éste.
//! Que digan lo mismo lo vigila el módulo `tienda` de `catalog_parity` (si se
//! cambia un precio de un lado, el test del otro se planta). Sólo cosmética:
//! nada de lo que se vende cambia el daño, la vida ni el vuelo (biblia §17).

/// Los ids de las armas, en el orden de `ha-game/src/armas.rs::ARMAS`.
pub const TIENDA_ARMAS: [&str; 9] = [
    "pistola", "thompson", "ppsh", "mp40", "suomi", "pps43", "ak", "escopeta", "raygun",
];

pub struct CamoTienda {
    pub id: &'static str,
    pub nombre: &'static str,
    /// 0 = no se vende (exclusivo del pase).
    pub gemas_por_arma: u32,
    pub gemas_pack: u32,
}

/// Los camuflajes que NO caen como botín.
pub const TIENDA_CAMOS: [CamoTienda; 4] = [
    CamoTienda { id: "oro", nombre: "Oro macizo", gemas_por_arma: 120, gemas_pack: 780 },
    CamoTienda { id: "neon", nombre: "Neón", gemas_por_arma: 100, gemas_pack: 650 },
    CamoTienda { id: "hielo", nombre: "Hielo eterno", gemas_por_arma: 100, gemas_pack: 650 },
    CamoTienda { id: "dragon", nombre: "Dragón de Ki", gemas_por_arma: 0, gemas_pack: 0 },
];

pub struct AuraTienda {
    pub id: &'static str,
    pub nombre: &'static str,
    pub gemas: u32,
    pub monedas: u32,
    /// Afuera → medio → núcleo (lo que pinta `creative_fly::sync_player_aura`).
    pub tonos: [[f32; 3]; 3],
}

pub const TIENDA_AURAS: [AuraTienda; 7] = [
    AuraTienda { id: "saiyan", nombre: "Saiyan dorado", gemas: 0, monedas: 0, tonos: [[1.0, 0.42, 0.06], [1.0, 0.66, 0.14], [1.0, 0.92, 0.62]] },
    AuraTienda { id: "dios_azul", nombre: "Dios azul", gemas: 300, monedas: 0, tonos: [[0.05, 0.35, 1.0], [0.2, 0.62, 1.0], [0.75, 0.95, 1.0]] },
    AuraTienda { id: "rosado", nombre: "Rosado", gemas: 300, monedas: 0, tonos: [[0.85, 0.08, 0.45], [1.0, 0.35, 0.7], [1.0, 0.82, 0.93]] },
    AuraTienda { id: "legendario", nombre: "Legendario", gemas: 0, monedas: 6000, tonos: [[0.1, 0.75, 0.15], [0.35, 1.0, 0.35], [0.85, 1.0, 0.7]] },
    AuraTienda { id: "carmesi", nombre: "Carmesí", gemas: 0, monedas: 4000, tonos: [[0.8, 0.04, 0.04], [1.0, 0.25, 0.12], [1.0, 0.7, 0.55]] },
    AuraTienda { id: "instinto", nombre: "Instinto plateado", gemas: 600, monedas: 0, tonos: [[0.55, 0.6, 0.75], [0.8, 0.85, 0.95], [1.0, 1.0, 1.0]] },
    AuraTienda { id: "ego", nombre: "Ego violeta", gemas: 0, monedas: 0, tonos: [[0.4, 0.05, 0.75], [0.62, 0.25, 1.0], [0.92, 0.75, 1.0]] },
];

pub struct PaqueteGemas {
    pub id: &'static str,
    pub gemas: u32,
    pub extra: u32,
    /// Precio en centavos de dólar (el servidor lo pasa a la moneda de la cuenta).
    pub centavos: u32,
}

pub const TIENDA_GEMAS: [PaqueteGemas; 4] = [
    PaqueteGemas { id: "gemas_100", gemas: 100, extra: 0, centavos: 99 },
    PaqueteGemas { id: "gemas_520", gemas: 500, extra: 20, centavos: 499 },
    PaqueteGemas { id: "gemas_1080", gemas: 1000, extra: 80, centavos: 999 },
    PaqueteGemas { id: "gemas_2200", gemas: 2000, extra: 200, centavos: 1999 },
];

pub struct Temporada {
    pub id: &'static str,
    pub nombre: &'static str,
    pub desde: &'static str,
    pub hasta: &'static str,
    pub niveles: u32,
    pub xp_por_nivel: u32,
    pub gemas_premium: u32,
}

pub const PASE_TEMPORADA: Temporada = Temporada {
    id: "t1",
    nombre: "Temporada 1 · Despertar del Ki",
    desde: "2026-10-01T00:00:00Z",
    hasta: "2027-01-01T00:00:00Z",
    niveles: 30,
    xp_por_nivel: 1000,
    gemas_premium: 950,
};

/// XP por cosa informada y el techo diario (`PASE_XP` de TS).
pub const XP_META: u32 = 120;
pub const XP_BAJA: u32 = 15;
pub const XP_MINUTO: u32 = 10;
pub const XP_TOPE_DIARIO: u32 = 3000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TipoItem {
    Camo,
    Pack,
    Aura,
    Pase,
}

impl TipoItem {
    pub fn como_str(self) -> &'static str {
        match self {
            TipoItem::Camo => "camo",
            TipoItem::Pack => "pack",
            TipoItem::Aura => "aura",
            TipoItem::Pase => "pase",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ItemTienda {
    pub id: String,
    pub tipo: TipoItem,
    pub nombre: String,
    pub gemas: u32,
    pub monedas: u32,
    pub da: Vec<String>,
}

/// El catálogo entero, en el mismo orden que el de TS.
pub fn catalogo() -> &'static [ItemTienda] {
    static C: std::sync::OnceLock<Vec<ItemTienda>> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let mut out = Vec::new();
        for c in &TIENDA_CAMOS {
            for a in TIENDA_ARMAS {
                let id = format!("camo:{a}:{}", c.id);
                out.push(ItemTienda {
                    id: id.clone(),
                    tipo: TipoItem::Camo,
                    nombre: c.nombre.to_string(),
                    gemas: c.gemas_por_arma,
                    monedas: 0,
                    da: vec![id],
                });
            }
            if c.gemas_pack > 0 {
                out.push(ItemTienda {
                    id: format!("pack:{}", c.id),
                    tipo: TipoItem::Pack,
                    nombre: format!("Arsenal {}", c.nombre),
                    gemas: c.gemas_pack,
                    monedas: 0,
                    da: TIENDA_ARMAS.iter().map(|a| format!("camo:{a}:{}", c.id)).collect(),
                });
            }
        }
        for a in &TIENDA_AURAS {
            let id = format!("aura:{}", a.id);
            out.push(ItemTienda {
                id: id.clone(),
                tipo: TipoItem::Aura,
                nombre: a.nombre.to_string(),
                gemas: a.gemas,
                monedas: a.monedas,
                da: vec![id],
            });
        }
        let id = format!("pase:{}", PASE_TEMPORADA.id);
        out.push(ItemTienda {
            id: id.clone(),
            tipo: TipoItem::Pase,
            nombre: format!("Pase premium · {}", PASE_TEMPORADA.nombre),
            gemas: PASE_TEMPORADA.gemas_premium,
            monedas: 0,
            da: vec![id],
        });
        out
    })
}

pub fn item(id: &str) -> Option<&'static ItemTienda> {
    catalogo().iter().find(|i| i.id == id)
}

/// Lo que se tiene sin comprar nada.
pub const DE_REGALO: [&str; 1] = ["aura:saiyan"];

/// Gratis y premium de un nivel del pase (`recompensaPase` de TS).
pub fn recompensa_pase(nivel: u32) -> (String, String) {
    let especial: Option<(&str, &str)> = match nivel {
        1 => Some(("monedas:300", "camo:pistola:dragon")),
        5 => Some(("monedas:600", "camo:thompson:dragon")),
        8 => Some(("aura:carmesi", "camo:mp40:dragon")),
        10 => Some(("gemas:50", "camo:ak:dragon")),
        12 => Some(("monedas:800", "camo:ppsh:dragon")),
        15 => Some(("monedas:1000", "aura:ego")),
        18 => Some(("monedas:800", "camo:suomi:dragon")),
        20 => Some(("gemas:50", "camo:pps43:dragon")),
        25 => Some(("monedas:1500", "camo:escopeta:dragon")),
        30 => Some(("gemas:100", "camo:raygun:dragon")),
        _ => None,
    };
    if let Some((g, p)) = especial {
        return (g.to_string(), p.to_string());
    }
    let gratis = format!("monedas:{}", 100 + 20 * nivel);
    let premium = if nivel % 3 == 0 { "gemas:100".to_string() } else { format!("monedas:{}", 300 + 30 * nivel) };
    (gratis, premium)
}

/// Nivel del pase para una XP (0 = todavía ninguno).
pub fn nivel_pase(xp: u32) -> u32 {
    (xp / PASE_TEMPORADA.xp_por_nivel).min(PASE_TEMPORADA.niveles)
}

/// Cómo se lee una recompensa para la pantalla: "300 monedas", "Ego violeta"…
pub fn describir(recompensa: &str) -> String {
    if let Some(n) = recompensa.strip_prefix("monedas:") {
        return format!("{n} monedas");
    }
    if let Some(n) = recompensa.strip_prefix("gemas:") {
        return format!("{n} gemas");
    }
    if let Some(a) = recompensa.strip_prefix("aura:") {
        if let Some(def) = TIENDA_AURAS.iter().find(|x| x.id == a) {
            return format!("Aura {}", def.nombre);
        }
    }
    if let Some((arma, camo)) = partir_camo(recompensa) {
        let nombre = TIENDA_CAMOS.iter().find(|c| c.id == camo).map(|c| c.nombre).unwrap_or(camo);
        return format!("{nombre} · {arma}");
    }
    recompensa.to_string()
}

/// `camo:<arma>:<camo>` → (arma, camo).
pub fn partir_camo(id: &str) -> Option<(&str, &str)> {
    let resto = id.strip_prefix("camo:")?;
    resto.split_once(':')
}

pub fn aura(id: &str) -> Option<&'static AuraTienda> {
    TIENDA_AURAS.iter().find(|a| a.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todo_lo_del_pase_existe_en_el_catalogo() {
        for n in 1..=PASE_TEMPORADA.niveles {
            let (g, p) = recompensa_pase(n);
            for r in [g, p] {
                if r.starts_with("monedas:") || r.starts_with("gemas:") {
                    continue;
                }
                assert!(item(&r).is_some(), "nivel {n}: {r} no está en el catálogo");
            }
        }
    }

    #[test]
    fn lo_exclusivo_del_pase_no_se_vende() {
        for i in catalogo() {
            if i.id.ends_with(":dragon") || i.id == "aura:ego" || i.id == "aura:saiyan" {
                assert_eq!((i.gemas, i.monedas), (0, 0), "{} no debería tener precio", i.id);
            }
        }
    }

    #[test]
    fn el_pase_completo_devuelve_mas_gemas_de_las_que_cuesta() {
        let gemas: u32 = (1..=PASE_TEMPORADA.niveles)
            .map(|n| recompensa_pase(n).1)
            .filter_map(|r| r.strip_prefix("gemas:").and_then(|n| n.parse::<u32>().ok()))
            .sum();
        assert!(gemas >= PASE_TEMPORADA.gemas_premium, "{gemas}");
    }

    #[test]
    fn el_pack_sale_mas_barato_que_las_nueve_sueltas() {
        for c in &TIENDA_CAMOS {
            if c.gemas_pack > 0 {
                assert!(c.gemas_pack < c.gemas_por_arma * 9, "{}", c.id);
            }
        }
    }
}
