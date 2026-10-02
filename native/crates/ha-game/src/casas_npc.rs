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
///
/// `defensivo`: lo que el barrio aprendió a las malas (`barrio::Lecciones::defensivo`): tras sufrir
/// ataques se levantan más torres y residencias de varios pisos, que son donde se aguanta una noche.
pub fn plano_para(k: u32, defensivo: u32) -> usize {
    let orden = [
        ("cabana", 3u32.saturating_sub(defensivo.min(2))),
        ("casa", 4),
        ("casa_doble", 3),
        ("tienda", 1),
        ("residencia", 2 + defensivo),
        ("galpon", 1),
        ("torre", 1 + defensivo),
        ("renta", 1 + defensivo / 2),
    ];
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
    mut npcs: Query<&mut Npc>,
    mut memoria: ResMut<crate::barrio::Memoria>,
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
    let Some((cx, cz, cuid)) = npcs.iter().find(|n| n.role == Role::Builder).map(|c| (c.town_x, c.town_z, c.uid)) else { return };
    let lecciones = memoria.de(cx, cz);
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
    let k = *contador + cuid as u32;
    // Se prueba con el plano que toca y, si no alcanza el stock, con uno más barato.
    let mut candidatos: Vec<usize> = vec![plano_para(k, lecciones.defensivo())];
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
                // `fundar_plano` agrega el edificio al final de la lista.
                let idx_ed = eds.list.len() - 1;
                // Sale tan bien techada como sepa y haya aprendido el pueblo; una mala gotea (`barrio.rs`).
                let calidad = crate::barrio::calidad_de_obra(civ.saber_de_techar(cx, cz), &lecciones, k);
                eds.list[idx_ed].calidad = calidad;
                if let Some(h) = crate::edificios::huella_de(&eds.list[idx_ed].ed) {
                    match p.uso {
                        Uso::Vivienda => mudar_vecinos(&mut npcs, (cx, cz), h),
                        // Un local o un taller se atiende: alguien del pueblo pasa a trabajar ahí.
                        Uso::Comercio | Uso::Taller => poner_encargado(&mut npcs, (cx, cz), h, k),
                        _ => {}
                    }
                }
                stock.wood -= cw;
                stock.stone -= cs;
                soc.stock.insert(body_u8, stock);
                *contador += 1;
                crate::causal::registrar("casa", format!("el pueblo levanta: {} en ({ax},{az}), calidad {calidad}", p.nombre));
                return;
            }
        }
    }
}

/// El local nuevo tiene quien lo atienda: un adulto del pueblo (el que más le toque por azar) cambia
/// su puesto de trabajo por este.
fn poner_encargado(npcs: &mut Query<&mut Npc>, pueblo: (f32, f32), local: ha_shared::hogares::Huella, k: u32) {
    let candidatos: Vec<u64> = npcs
        .iter()
        .filter(|n| (n.town_x - pueblo.0).abs() < 1.0 && (n.town_z - pueblo.1).abs() < 1.0 && !crate::society::es_nino(n.age, n.padre, n.madre))
        .map(|n| n.uid)
        .collect();
    if candidatos.is_empty() {
        return;
    }
    let uid = candidatos[(k as usize) % candidatos.len()];
    if let Some(mut n) = npcs.iter_mut().find(|n| n.uid == uid) {
        n.vivienda.puesto = Some(local);
    }
}

/// La casa recién hecha se habita: pasan a vivir ahí (hasta `FAMILIA` vecinos) los del pueblo que no
/// tienen casa o que comparten la más llena. Los pueblos no construyen para nadie: construyen para
/// quienes viven apretados.
const FAMILIA: usize = 2;

fn mudar_vecinos(npcs: &mut Query<&mut Npc>, pueblo: (f32, f32), casa: ha_shared::hogares::Huella) {
    let mut ocupacion: std::collections::HashMap<(i64, i64), usize> = std::collections::HashMap::new();
    for n in npcs.iter() {
        if (n.town_x - pueblo.0).abs() < 1.0 && (n.town_z - pueblo.1).abs() < 1.0 {
            if let Some(c) = n.vivienda.casa {
                *ocupacion.entry(c.min).or_default() += 1;
            }
        }
    }
    let mut mudados = 0;
    for mut n in npcs.iter_mut() {
        if mudados >= FAMILIA {
            break;
        }
        if (n.town_x - pueblo.0).abs() >= 1.0 || (n.town_z - pueblo.1).abs() >= 1.0 {
            continue;
        }
        // Sólo adultos (los chicos siguen con su madre).
        if crate::society::es_nino(n.age, n.padre, n.madre) {
            continue;
        }
        let apretado = match n.vivienda.casa {
            None => true,
            Some(c) => ocupacion.get(&c.min).copied().unwrap_or(0) >= 3,
        };
        if apretado {
            n.vivienda.casa = Some(casa);
            mudados += 1;
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
            let p = &PLANOS[plano_para(k, 0)];
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
        assert_eq!(plano_para(17, 0), plano_para(17, 0));
    }

    #[test]
    fn un_barrio_que_sufrio_levanta_mas_torres_y_residencias() {
        let altos = |d: u32| (0..600).filter(|k| matches!(PLANOS[plano_para(*k, d)].id, "torre" | "residencia" | "renta")).count();
        assert!(altos(6) > altos(0) + 60, "{} vs {}", altos(6), altos(0));
    }
}
