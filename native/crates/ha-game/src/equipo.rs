//! **Equipo: la ropa y la armadura que llevás puestas.**
//!
//! Las prendas (`ha_shared::armadura`) se hacen con pieles y lana. Con una en la mano,
//! mantener sin objetivo la viste (como comer): se guarda en su ranura, la que ya llevabas
//! vuelve a la mochila, protege de mordidas y radiación (`SimState::armadura`) y se VE en el
//! personaje, que se recompone con las capas del pack (`chargen`): el gorro de cuero es el
//! sombrero marrón, el chaleco y el abrigo son las ropas marrón y crema. Se guarda en
//! `equipo.json`.

use bevy::prelude::*;
use ha_shared::armadura::{prenda, reduccion_total, Ranura};
use serde::{Deserialize, Serialize};

use crate::chargen::{sheet_for, Chargen, ChargenJobs, Layer};
use crate::game_phase::SelectedCharacter;
use crate::inventory_persist::persist_dir;
use crate::sim::{Player, SimState};

/// ¿Esto se come o se viste? (lo que hace que mantener sin objetivo haga algo).
pub fn es_usable(item: &str) -> bool {
    ha_shared::food_restore(item).is_some() || prenda(item).is_some()
}

/// Lo que dice el botón USAR para este objeto: la acción que hace al mantenerlo (`None` = no se
/// usa solo). Una prenda se viste; lo que sacia sed y no hambre se bebe; lo demás se come.
pub fn etiqueta_de_uso(item: &str) -> Option<&'static str> {
    // Un arma de fuego se «usa» recargándola (`armas.rs` escucha el mismo botón).
    if crate::armas::es_arma(item) {
        return Some("RECARGAR");
    }
    if prenda(item).is_some() {
        return Some("VESTIR");
    }
    let (hambre, sed) = ha_shared::food_restore(item)?;
    Some(if sed > hambre { "BEBER" } else { "COMER" })
}

#[derive(Resource, Default, Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Equipo {
    puesto: [Option<String>; 3],
}

impl Equipo {
    /// Viste la prenda; devuelve la que ocupaba esa ranura (hay que devolverla a la mochila).
    pub fn vestir(&mut self, item: &str) -> Option<Option<String>> {
        let p = prenda(item)?;
        let previa = self.puesto[p.ranura.indice()].replace(item.to_string());
        Some(previa)
    }

    pub fn en(&self, r: Ranura) -> Option<&str> {
        self.puesto[r.indice()].as_deref()
    }

    pub fn reduccion(&self) -> f32 {
        let v: [Option<&str>; 3] = [self.puesto[0].as_deref(), self.puesto[1].as_deref(), self.puesto[2].as_deref()];
        reduccion_total(&v)
    }
}

fn ruta() -> std::path::PathBuf {
    persist_dir().join("equipo.json")
}

pub struct EquipoPlugin;

impl Plugin for EquipoPlugin {
    fn build(&self, app: &mut App) {
        // Para probar en el celu sin cazar: `setprop debug.ha.dame leather_cap=1,apple=2`.
        // Sin la propiedad no se registra nada.
        if crate::flags::var("HA_DAME").is_ok() {
            app.add_systems(Update, dar_de_prueba.run_if(crate::game_phase::world_active));
        }
        app.init_resource::<Equipo>()
            .add_systems(OnEnter(crate::game_phase::GamePhase::Loading), cargar)
            .add_systems(
                Update,
                (guardar_si_cambia, sincronizar_armadura, vestir_en_pantalla)
                    .run_if(crate::game_phase::world_active),
            );
    }
}

fn dar_de_prueba(
    mut hecho: Local<bool>,
    mut inv: ResMut<crate::inventory_persist::PlayerInventory>,
    mut ui: ResMut<crate::hud::inventory_ui::InventoryUiState>,
) {
    if *hecho {
        return;
    }
    *hecho = true;
    let lista = crate::flags::var("HA_DAME").unwrap_or_default();
    for par in lista.split(',') {
        if let Some((item, n)) = par.split_once('=') {
            if let Ok(n) = n.trim().parse::<u32>() {
                crate::hud::inventory_ui::add_item_to_slots(&mut ui, item.trim(), n);
            }
        }
    }
    crate::hud::inventory_ui::flush_slots_to_inventory(&ui, &mut inv);
}

fn cargar(mut equipo: ResMut<Equipo>) {
    *equipo = crate::persistent_agents::cargar_json::<Equipo>(&ruta()).unwrap_or_default();
}

fn guardar_si_cambia(equipo: Res<Equipo>) {
    if equipo.is_changed() && !equipo.is_added() {
        crate::persistent_agents::guardar_json(&ruta(), &*equipo);
    }
}

fn sincronizar_armadura(equipo: Res<Equipo>, mut jugador: Query<&mut SimState, With<Player>>) {
    let r = equipo.reduccion();
    for mut s in &mut jugador {
        if (s.armadura - r).abs() > f32::EPSILON {
            s.armadura = r;
        }
    }
}

/// Qué capas del pack representan la ropa puesta. Se buscan por NOMBRE de estilo (no por
/// número: el índice se re-exporta) y se toma el primer color.
const SOMBRERO_CUERO: &str = "SOMBRERO DETECTIVE";
const ROPA_CUERO: &str = "ROPA 16";
const ROPA_LANA: &str = "ROPA 23";

fn opcion(cg: &Chargen, capa: Layer, etiqueta: &str) -> Option<u32> {
    cg.options(capa).iter().position(|o| o.label == etiqueta).map(|i| i as u32)
}

/// El look del personaje con la ropa puesta encima de sus elecciones de creación.
pub fn look_con_ropa(base: crate::chargen::LookDef, equipo: &Equipo, cg: &Chargen) -> crate::chargen::LookDef {
    let mut look = base;
    let sombrero = match equipo.en(Ranura::Cabeza) {
        Some("leather_cap") => Some(SOMBRERO_CUERO),
        Some("gorra_roja") => Some("GORRA"),
        Some("gorra_azul") => Some("GORRA DE POLICIA"),
        Some("gorro_gris") => Some("GORRO DE LANA"),
        _ => None,
    };
    if let Some(label) = sombrero {
        if let Some(i) = opcion(cg, Layer::Accessory, label) {
            look.accessory = Some(i);
        }
    }
    // La ropa encontrada (`ropa_NN`) es EL estilo NN del creador de personajes.
    if let Some(n) = equipo.en(Ranura::Torso).and_then(|t| t.strip_prefix("ropa_")) {
        if let Some(i) = opcion(cg, Layer::Outfit, &format!("ROPA {n}")) {
            look.outfit = i;
        }
    }
    match equipo.en(Ranura::Torso) {
        Some("leather_vest") => {
            if let Some(i) = opcion(cg, Layer::Outfit, ROPA_CUERO) {
                look.outfit = i;
            }
        }
        Some("wool_coat") => {
            if let Some(i) = opcion(cg, Layer::Outfit, ROPA_LANA) {
                look.outfit = i;
            }
        }
        _ => {}
    }
    look
}

#[allow(clippy::too_many_arguments)]
fn vestir_en_pantalla(
    equipo: Res<Equipo>,
    selected: Res<SelectedCharacter>,
    cg: Res<Chargen>,
    mut jobs: ResMut<ChargenJobs>,
    mut images: ResMut<Assets<Image>>,
    assets: Res<AssetServer>,
    mut jugador: Query<&mut Sprite, With<Player>>,
    mut pendiente: Local<bool>,
) {
    if equipo.is_changed() {
        *pendiente = true;
    }
    if !*pendiente {
        return;
    }
    let Some(base) = selected.look else {
        *pendiente = false;
        return;
    };
    let look = look_con_ropa(base, &equipo, &cg);
    let Some(handle) = sheet_for(look, &cg, &mut jobs, &mut images, &assets) else {
        return; // la composición tarda unos frames: se reintenta
    };
    if let Ok(mut sprite) = jugador.get_single_mut() {
        sprite.image = handle;
        *pendiente = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_ropa_hallable_se_viste_en_su_ranura() {
        let mut e = Equipo::default();
        assert_eq!(e.vestir("ropa_07"), Some(None));
        assert_eq!(e.vestir("gorra_roja"), Some(None));
        assert_eq!(e.vestir("botas_3"), Some(None));
        assert_eq!(e.en(Ranura::Torso), Some("ropa_07"));
        assert_eq!(e.en(Ranura::Cabeza), Some("gorra_roja"));
        assert!(e.reduccion() > 0.2);
    }

    #[test]
    fn vestir_cambia_la_ranura_y_devuelve_la_anterior() {
        let mut e = Equipo::default();
        assert_eq!(e.vestir("leather_vest"), Some(None));
        assert_eq!(e.vestir("wool_coat"), Some(Some("leather_vest".to_string())));
        assert_eq!(e.en(Ranura::Torso), Some("wool_coat"));
        assert_eq!(e.vestir("apple"), None, "una manzana no se viste");
    }

    #[test]
    fn la_proteccion_sale_de_lo_puesto() {
        let mut e = Equipo::default();
        assert_eq!(e.reduccion(), 0.0);
        e.vestir("leather_cap");
        e.vestir("leather_boots");
        assert!((e.reduccion() - 0.20).abs() < 1e-6);
    }

    #[test]
    fn el_equipo_se_guarda_y_se_lee() {
        let mut e = Equipo::default();
        e.vestir("leather_vest");
        let j = serde_json::to_string(&e).unwrap();
        assert_eq!(serde_json::from_str::<Equipo>(&j).unwrap(), e);
    }

    #[test]
    fn el_boton_dice_lo_que_hace_cada_objeto() {
        assert_eq!(etiqueta_de_uso("apple"), Some("COMER"));
        assert_eq!(etiqueta_de_uso("leather_vest"), Some("VESTIR"));
        assert_eq!(etiqueta_de_uso("water_bucket"), Some("BEBER"));
        assert_eq!(etiqueta_de_uso("wood"), None);
        assert_eq!(etiqueta_de_uso("hand"), None);
    }

    #[test]
    fn solo_lo_que_se_come_o_se_viste_es_usable() {
        assert!(es_usable("apple") && es_usable("leather_cap"));
        assert!(!es_usable("wood"));
    }
}
