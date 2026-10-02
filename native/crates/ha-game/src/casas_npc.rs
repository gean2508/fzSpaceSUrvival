//! **Los pueblos construyen casas** — con los mismos planos que el jugador (`ha_shared::planos`).
//!
//! Hasta acá el constructor de un pueblo sólo ponía mobiliario de parque. Ahora, cuando la colonia
//! juntó madera y piedra y tiene un Constructor despierto, levanta un edificio entero: elige un plano
//! (más seguido viviendas; de vez en cuando una tienda, un galpón o una torre), busca un sitio de
//! terreno natural alrededor del pueblo, y lo levanta con piso, puertas, ventanas y muebles. El
//! pueblo crece a lo ancho, con casas distintas, y se ve.
//!
//! * **El acabado depende de lo que el pueblo descubrió** (`civilizacion::Civ::tema_cerca`): si en
//!   el mundo de civilización el pueblo más cercano sabe hacer cerámica, sus casas son de ladrillo; si
//!   trabaja metales, azules; si no, de hormigón. Lo descubierto se VE en la calle.
//! * **El costo es el mismo plano a un cuarto del precio** (la economía de los NPC no es la del
//!   jugador: producen en lote y no pagan por separado el acabado).
//! * Cupo: una obra cada `CADA_S` segundos y `CASAS_MAX` por pueblo, para no inundar el mundo ni el
//!   frame.

use bevy::prelude::*;
use ha_shared::planos::{Uso, PLANOS};

use crate::edificios::{fundar_plano, Edificios};
use crate::scene::WorldContextRuntime;
use crate::society::{Npc, Role, Society};
use crate::structures::BuiltStructures;
use crate::world2d::{active_surface_body, ActivePlanet, TileWorld};

const CADA_S: f32 = 40.0;
const CASAS_MAX: usize = 10;
const DESCUENTO: f32 = 4.0;

pub struct CasasNpcPlugin;

impl Plugin for CasasNpcPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, construir_casas.run_if(crate::game_phase::world_active));
    }
}

/// El plano que toca: viviendas casi siempre; algún comercio, taller o torre.
pub fn plano_para(k: u32) -> usize {
    let orden = [("cabana", 3), ("casa", 4), ("casa_doble", 3), ("tienda", 1), ("residencia", 2), ("galpon", 1), ("torre", 1), ("renta", 1)];
    let total: u32 = orden.iter().map(|o| o.1).sum();
    let mut r = ha_shared::worldgen::hash_cell(0xCA5A, k as i64, 7) % total;
    for (id, peso) in orden {
        if r < peso {
            return PLANOS.iter().position(|p| p.id == id).unwrap_or(0);
        }
        r -= peso;
    }
    0
}

#[allow(clippy::too_many_arguments)]
fn construir_casas(
    time: Res<Time>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    civ: Res<crate::civilizacion::Civ>,
    mut soc: ResMut<Society>,
    npcs: Query<&Npc>,
    mut tiles: ResMut<TileWorld>,
    mut structures: ResMut<BuiltStructures>,
    mut eds: ResMut<Edificios>,
    mut acc: Local<f32>,
    mut contador: Local<u32>,
) {
    *acc += time.delta_secs();
    if *acc < CADA_S {
        return;
    }
    *acc = 0.0;
    if tiles.z != 0 {
        return;
    }
    let body = active_surface_body(&ctx, &planet);
    let body_u8 = body as u8;
    let Some(constructor) = npcs.iter().find(|n| n.role == Role::Builder) else { return };
    let (cx, cz) = (constructor.town_x, constructor.town_z);
    // ¿Cuántos edificios ya hay cerca? (los del pueblo generado también cuentan como lugar ocupado).
    let cerca = eds
        .list
        .iter()
        .filter(|e| e.body == body_u8 && e.ed.celdas().iter().next().map(|c| ((c.0 as f32 - cx).abs() < 70.0) && ((c.1 as f32 - cz).abs() < 70.0)).unwrap_or(false))
        .count();
    if cerca >= CASAS_MAX {
        return;
    }
    let mut stock = soc.stock(body_u8);
    let k = *contador + constructor.uid as u32;
    // Se prueba con el plano que toca y, si no alcanza el stock, con uno más barato.
    let mut candidatos: Vec<usize> = vec![plano_para(k)];
    candidatos.extend(["cabana", "casa"].iter().filter_map(|id| PLANOS.iter().position(|p| p.id == *id)));
    let tema = civ.tema_cerca(cx, cz);
    for idx in candidatos {
        let p = &PLANOS[idx];
        let (cw, cs) = (p.madera as f32 / DESCUENTO, p.piedra as f32 / DESCUENTO);
        if stock.wood < cw || stock.stone < cs {
            continue;
        }
        for i in 0..48u32 {
            let ang = (k + i) as f32 * 2.399_963_2;
            let r = 14.0 + ((k + i) % 7) as f32 * 4.0;
            let (ax, az) = ((cx + ang.cos() * r).floor() as i64, (cz + ang.sin() * r).floor() as i64);
            if fundar_plano(&mut tiles, &mut structures, &mut eds, body_u8, 0, ax, az, p, tema, k, true).is_ok() {
                stock.wood -= cw;
                stock.stone -= cs;
                soc.stock.insert(body_u8, stock);
                *contador += 1;
                crate::causal::registrar("casa", format!("el pueblo levanta: {} en ({ax},{az})", p.nombre));
                let _ = Uso::Vivienda;
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn los_pueblos_construyen_variado_y_sobre_todo_viviendas() {
        let mut por_uso = std::collections::BTreeMap::new();
        let mut ids = std::collections::BTreeSet::new();
        for k in 0..400 {
            let p = &PLANOS[plano_para(k)];
            *por_uso.entry(format!("{:?}", p.uso)).or_insert(0u32) += 1;
            ids.insert(p.id);
        }
        assert!(ids.len() >= 6, "casi no hay variedad: {ids:?}");
        assert!(por_uso["Vivienda"] > por_uso.get("Comercio").copied().unwrap_or(0) * 3, "{por_uso:?}");
        for u in ["Vivienda", "Comercio", "Taller", "Torre"] {
            assert!(por_uso.contains_key(u), "nunca se construye {u}");
        }
    }

    #[test]
    fn un_plano_se_arma_en_el_mismo_lugar_siempre() {
        assert_eq!(plano_para(17), plano_para(17));
    }
}
