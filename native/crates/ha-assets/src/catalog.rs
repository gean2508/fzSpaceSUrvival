//! F4 — genera `CATALOGO.md` a partir del manifest. Tabla determinista (ordenada
//! por ID, gracias al `BTreeMap`) legible por humanos Y por Claude: es lo que se
//! inyecta en contexto para que la IA sepa qué IDs existen y NUNCA invente rects.
//!
//! Proyección pura del manifest — nunca fuente de verdad. Headless (sin `image`).

use crate::manifest::{Asset, Manifest, YSort};

/// Markdown con la cabecera + una fila por asset.
pub fn generate_catalog(m: &Manifest) -> String {
    let mut out = String::new();
    out.push_str("# CATÁLOGO de assets\n\n");
    out.push_str(
        "> AUTOGENERADO por `ha-asset-tools catalog` — NO editar a mano.\n\
         > Fuente de verdad: `manifest.ron`. Regenerar tras cada scan/curado.\n\n",
    );
    out.push_str(&format!(
        "**{} assets · {} hojas · manifest v{}**\n\n",
        m.assets.len(),
        m.sheets.len(),
        m.manifest_version
    ));
    out.push_str("| ID | Descripción | Tamaño | Animado | Y-sort | Tags |\n");
    out.push_str("|----|-------------|--------|---------|--------|------|\n");
    for (id, a) in &m.assets {
        out.push_str(&format!(
            "| `{}` | {} | {}×{} tiles | {} | {} | {} |\n",
            id,
            describe(id),
            a.footprint.w,
            a.footprint.h,
            if a.anim.is_some() { "Sí" } else { "No" },
            ysort_label(a),
            a.tags.join(", "),
        ));
    }
    out
}

/// Descripción humana desde el ID: quita el prefijo de categoría y tokens
/// numéricos de cabecera, y pasa a Title Case. `exteriors/21_beach_ball` → "Beach ball".
fn describe(id: &str) -> String {
    let name = id.split_once('/').map(|(_, n)| n).unwrap_or(id);
    let words: Vec<&str> = name
        .split('_')
        .skip_while(|w| w.chars().all(|c| c.is_ascii_digit())) // 21_ de cabecera
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return name.replace('_', " ");
    }
    let mut s = String::new();
    for (i, w) in words.iter().enumerate() {
        if i == 0 {
            let mut ch = w.chars();
            if let Some(first) = ch.next() {
                s.push(first.to_ascii_uppercase());
                s.push_str(ch.as_str());
            }
        } else {
            s.push(' ');
            s.push_str(w);
        }
    }
    s
}

fn ysort_label(a: &Asset) -> String {
    match a.ysort {
        YSort::Row(r) => format!("fila {r}"),
        YSort::Offset(o) => format!("offset {o}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::*;
    use std::collections::BTreeMap;

    fn asset(sheet: &str, fw: u32, fh: u32, anim: bool, tags: &[&str]) -> Asset {
        Asset {
            sheet: sheet.into(),
            footprint: Footprint { w: fw, h: fh },
            rect: Rect::Px { x: 0, y: 0, w: fw * 16, h: fh * 16 },
            pivot: Pivot::Anchor(AnchorKind::BottomCenter),
            ppu: 16,
            ysort: YSort::Row(fh.saturating_sub(1)),
            hitbox: None,
            anim: anim.then_some(Anim { frames: 4, fps: 6.0, layout: AnimLayout::RowRight, loop_: true }),
            flags: Flags::default(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn catalog_has_header_and_rows() {
        let mut assets = BTreeMap::new();
        assets.insert("exteriors/21_beach_ball".to_string(), asset("s/ball", 1, 1, false, &["exteriors"]));
        assets.insert("npc/colono".to_string(), asset("s/npc", 1, 2, true, &["npc", "character"]));
        let m = Manifest { assets, ..Default::default() };

        let md = generate_catalog(&m);
        assert!(md.contains("| ID | Descripción | Tamaño | Animado | Y-sort | Tags |"));
        assert!(md.contains("**2 assets"));
        // ID crudo entre backticks + descripción legible sin el "21".
        assert!(md.contains("| `exteriors/21_beach_ball` | Beach ball | 1×1 tiles | No | fila 0 |"));
        assert!(md.contains("| `npc/colono` | Colono | 1×2 tiles | Sí | fila 1 | npc, character |"));
    }

    #[test]
    fn describe_strips_prefix_and_leading_digits() {
        assert_eq!(describe("exteriors/21_beach_ball"), "Beach ball");
        assert_eq!(describe("mueble/cama_doble"), "Cama doble");
        assert_eq!(describe("flora/arbol_roble"), "Arbol roble");
    }
}
