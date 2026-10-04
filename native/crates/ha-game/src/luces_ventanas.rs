//! **Ventanas que se prenden de noche**: desde afuera se ven cuartos ocupados y desocupados.
//!
//! Las fachadas del pack son una imagen única, así que las ventanas se miden con
//! `scripts/medir-ventanas.py` (`art/ventanas.json`, rectángulos en píxeles del dibujo) y cada una lleva un
//! recorte cálido encima, por encima del velo de noche, igual que las ventanas con pose `Light`. Cuál se
//! prende y cuál no sale de `ha_shared::interiores::ventana_prendida`: la misma gente que duerme adentro.

use std::collections::HashMap;
use std::sync::OnceLock;

use bevy::prelude::*;
use ha_shared::interiores::Lote;

use crate::game_phase::GamePhase;
use crate::sky2d::DayClock;

const JSON: &str = include_str!("../assets/art/ventanas.json");

pub struct Medidas {
    pub tam: Vec2,
    pub v: Vec<[f32; 4]>,
}

fn tabla() -> &'static HashMap<String, Medidas> {
    static T: OnceLock<HashMap<String, Medidas>> = OnceLock::new();
    T.get_or_init(|| {
        let raw: serde_json::Value = serde_json::from_str(JSON).unwrap_or(serde_json::Value::Null);
        let mut out = HashMap::new();
        if let Some(obj) = raw.as_object() {
            for (k, e) in obj {
                let tam = e["tam"].as_array().map(|a| Vec2::new(a[0].as_f64().unwrap_or(1.0) as f32, a[1].as_f64().unwrap_or(1.0) as f32));
                let v: Vec<[f32; 4]> = e["ventanas"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|r| {
                                let r = r.as_array()?;
                                Some([r[0].as_f64()? as f32, r[1].as_f64()? as f32, r[2].as_f64()? as f32, r[3].as_f64()? as f32])
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if let (Some(tam), false) = (tam, v.is_empty()) {
                    out.insert(k.clone(), Medidas { tam, v });
                }
            }
        }
        out
    })
}

/// Las ventanas medidas de un edificio. La clave puede traer variante (`building/x#3`): comparten geometría.
pub fn medidas_de(key: &str) -> Option<&'static Medidas> {
    let base = key.split('#').next().unwrap_or(key);
    tabla().get(base)
}

#[derive(Component)]
pub struct VentanaLuz {
    pub lote: Lote,
    pub idx: usize,
}

/// Cuelga las ventanas medidas de `clave` como hijas del sprite de la fachada (`size` = tamaño con que se dibuja).
pub fn colgar(ent: &mut EntityCommands, clave: &str, size: Vec2, lote: Lote) {
    let Some(m) = medidas_de(clave) else { return };
    let esc = Vec2::new(size.x / m.tam.x, size.y / m.tam.y);
    ent.with_children(|p| {
        for (idx, r) in m.v.iter().enumerate() {
            let w = r[2] * esc.x;
            let h = r[3] * esc.y;
            let cx = (r[0] + r[2] * 0.5) * esc.x - size.x * 0.5;
            let cy = size.y * 0.5 - (r[1] + r[3] * 0.5) * esc.y;
            p.spawn((
                VentanaLuz { lote, idx },
                Sprite::from_color(Color::srgba(1.0, 0.80, 0.38, 0.0), Vec2::new(w, h)),
                // Por encima del velo de noche, como las ventanas «Light» del pack.
                Transform::from_xyz(cx, cy, crate::light2d::GLOW_Z - 1.0),
                Visibility::Hidden,
            ));
        }
    });
}

pub struct LucesVentanasPlugin;

impl Plugin for LucesVentanasPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, actualizar.run_if(in_state(GamePhase::Playing)));
    }
}

fn actualizar(time: Res<Time>, day: Res<DayClock>, mut acc: Local<f32>, mut q: Query<(&VentanaLuz, &mut Sprite, &mut Visibility)>) {
    *acc += time.delta_secs();
    if *acc < 1.0 {
        return;
    }
    *acc = 0.0;
    let hora = day.phase() * 24.0;
    // Las que cambian lo hacen de a poco: no se prenden todas en el mismo segundo.
    for (v, mut s, mut vis) in &mut q {
        let on = ha_shared::interiores::ventana_prendida(&v.lote, v.idx, hora);
        let a = s.color.alpha();
        let objetivo = if on { 0.85 } else { 0.0 };
        let nuevo = a + (objetivo - a) * 0.35;
        if (nuevo - a).abs() > 0.01 {
            s.color = s.color.with_alpha(nuevo);
        }
        let ver = nuevo > 0.04;
        let quiero = if ver { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != quiero {
            *vis = quiero;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_ventanas_medidas_existen_y_estan_dentro_del_dibujo() {
        assert!(tabla().len() > 50, "ventanas medidas: {}", tabla().len());
        for (k, m) in tabla() {
            for r in &m.v {
                assert!(r[0] >= 0.0 && r[1] >= 0.0 && r[0] + r[2] <= m.tam.x + 1.0 && r[1] + r[3] <= m.tam.y + 1.0, "{k}: {r:?} fuera de {:?}", m.tam);
            }
        }
        assert!(medidas_de("building/hospital").is_some());
        assert!(medidas_de("building/hospital#2").is_some(), "la variante comparte geometría");
    }
}
