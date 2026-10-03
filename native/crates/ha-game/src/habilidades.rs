//! **Habilidades que suben con el uso** (Project Zomboid: lo que hacés mucho, lo
//! hacés mejor).
//!
//! Cuatro oficios —Tala, Minería, Caza y Huerta— con nivel 0–10. No hay hooks en
//! cada sistema de recolección: el XP sale de **lo que entra a la mochila** (se
//! mira la mochila dos veces por segundo y cada unidad nueva de un ítem del oficio
//! suma 1). Así cualquier forma nueva de conseguir madera o cuero cuenta sola.
//!
//! Y sube algo REAL: cada nivel da +4 % de probabilidad de que lo que acabás de
//! juntar venga con una unidad de más.
//!
//! Anti-trampa barata: lo que sale de la mochila nunca resta ni suma (el "antes"
//! se actualiza), y un solo repaso no puede dar más de `TOPE_POR_REPASO` XP, así
//! mover una pila de un cofre a la mochila no regala niveles.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::game_phase::GamePhase;
use crate::i18n::{idioma_actual, Idioma};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};

const REPASO: f32 = 0.5;
const TOPE_POR_REPASO: u32 = 12;
const NIVEL_MAX: u32 = 10;
const BONUS_POR_NIVEL: f32 = 0.04;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Oficio {
    Tala,
    Mineria,
    Caza,
    Huerta,
}

const OFICIOS: [Oficio; 4] = [Oficio::Tala, Oficio::Mineria, Oficio::Caza, Oficio::Huerta];

impl Oficio {
    fn idx(self) -> usize {
        self as usize
    }
    pub fn nombre(self, en: bool) -> &'static str {
        match (self, en) {
            (Oficio::Tala, false) => "Tala",
            (Oficio::Tala, true) => "Woodcutting",
            (Oficio::Mineria, false) => "Minería",
            (Oficio::Mineria, true) => "Mining",
            (Oficio::Caza, false) => "Caza",
            (Oficio::Caza, true) => "Hunting",
            (Oficio::Huerta, false) => "Huerta",
            (Oficio::Huerta, true) => "Farming",
        }
    }
}

/// A qué oficio entrena cada ítem que entra a la mochila.
fn oficio_de(item: &str) -> Option<Oficio> {
    Some(match item {
        "wood" | "sticks" | "resin" => Oficio::Tala,
        "stone" | "clay" | "coal" | "copper_ore" | "tin_ore" | "iron_ore" | "limestone" | "salt" | "gem_blue" | "gem_red" | "iron_ingot" | "gold_ingot" => Oficio::Mineria,
        "leather" | "meat" | "beef" | "pork" | "chicken" | "mutton" | "rabbit_meat" | "feather" | "bone" | "wool" => {
            Oficio::Caza
        }
        "wheat" | "carrot" | "pumpkin" | "apple" | "orange" | "grape" | "egg" => Oficio::Huerta,
        _ => return None,
    })
}

/// Nivel para un XP: el nivel `n` pide `4·n²` (4, 16, 36, 64…).
pub fn nivel(xp: u32) -> u32 {
    (((xp as f32) / 4.0).sqrt().floor() as u32).min(NIVEL_MAX)
}

#[derive(Resource, Default)]
pub struct Habilidades {
    xp: [u32; 4],
    antes: HashMap<String, u32>,
    cargado: bool,
    acc: f32,
    /// Subida de nivel reciente: texto y segundos que le quedan.
    aviso: Option<(String, f32)>,
}

impl Habilidades {
    /// El nivel del oficio más alto (para las llaves del OASIS).
    pub fn maximo(&self) -> u32 {
        OFICIOS.iter().map(|o| self.nivel(*o)).max().unwrap_or(0)
    }

    pub fn nivel(&self, o: Oficio) -> u32 {
        nivel(self.xp[o.idx()])
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Guardado {
    xp: [u32; 4],
}

fn archivo() -> Option<std::path::PathBuf> {
    crate::partida::activa().map(|id| crate::partida::carpeta(&id).join("habilidades.json"))
}

fn guardar(h: &Habilidades) {
    let Some(p) = archivo() else { return };
    if let Ok(t) = serde_json::to_string(&Guardado { xp: h.xp }) {
        let _ = std::fs::write(p, t);
    }
}

fn cargar() -> [u32; 4] {
    archivo()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Guardado>(&t).ok())
        .map(|g| g.xp)
        .unwrap_or_default()
}

/// Un repaso: suma XP por lo ganado y devuelve los ítems que ganan unidad extra.
fn repasar(h: &mut Habilidades, inv: &PlayerInventory, azar: impl Fn(u64) -> f32) -> (Vec<&'static str>, Vec<(Oficio, u32)>) {
    let mut extras = Vec::new();
    let mut subidas = Vec::new();
    let mut ganado = [0u32; 4];
    let mut ahora: HashMap<String, u32> = HashMap::new();
    for (id, q) in &inv.items {
        let Some(o) = oficio_de(id) else { continue };
        let antes = h.antes.get(id).copied().unwrap_or(*q);
        if *q > antes {
            let d = *q - antes;
            ganado[o.idx()] += d;
            let lvl = nivel(h.xp[o.idx()]);
            if azar(id.len() as u64 + *q as u64) < lvl as f32 * BONUS_POR_NIVEL {
                if let Some(s) = ITEMS.iter().find(|s| **s == id.as_str()) {
                    extras.push(*s);
                }
            }
        }
        ahora.insert(id.clone(), *q);
    }
    // Lo que sale (gastado o movido a un cofre) no resta: el "antes" se pone al día.
    h.antes = ahora;
    for o in OFICIOS {
        let g = ganado[o.idx()].min(TOPE_POR_REPASO);
        if g == 0 || h.cargado.then_some(()).is_none() {
            continue;
        }
        let antes = nivel(h.xp[o.idx()]);
        h.xp[o.idx()] += g;
        let despues = nivel(h.xp[o.idx()]);
        if despues > antes {
            subidas.push((o, despues));
        }
    }
    (extras, subidas)
}

/// Los ítems que pueden venir de más (estáticos para no clonar `String`).
const ITEMS: [&str; 20] = [
    "wood", "sticks", "stone", "gem_blue", "gem_red", "iron_ingot", "gold_ingot", "leather", "meat", "beef", "pork",
    "chicken", "mutton", "rabbit_meat", "feather", "bone", "wool", "wheat", "carrot", "apple",
];

#[derive(Component)]
struct HabilidadesUi;

pub struct HabilidadesPlugin;

impl Plugin for HabilidadesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Habilidades>()
            .add_systems(Update, (tick_habilidades, pintar_habilidades).chain().run_if(in_state(GamePhase::Playing)))
            .add_systems(OnExit(GamePhase::Playing), salir);
    }
}

fn salir(mut commands: Commands, mut h: ResMut<Habilidades>, ui: Query<Entity, With<HabilidadesUi>>) {
    *h = Habilidades::default();
    for e in &ui {
        commands.entity(e).despawn_recursive();
    }
}

fn tick_habilidades(
    time: Res<Time>,
    mut h: ResMut<Habilidades>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
) {
    if !h.cargado {
        // Primer repaso: sólo fija el "antes" (lo que ya traías no es XP).
        h.xp = cargar();
        let inv_ref: &PlayerInventory = &inv;
        let _ = repasar(&mut h, inv_ref, |_| 1.0);
        h.cargado = true;
        return;
    }
    let dt = time.delta_secs();
    if let Some((_, t)) = h.aviso.as_mut() {
        *t -= dt;
        if *t <= 0.0 {
            h.aviso = None;
        }
    }
    h.acc += dt;
    if h.acc < REPASO {
        return;
    }
    h.acc = 0.0;
    let sem = time.elapsed().as_micros() as u64;
    let azar = |k: u64| {
        let mut x = sem ^ k.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        x ^= x >> 31;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        ((x >> 40) as f32) / (1u64 << 24) as f32
    };
    let inv_ref: &PlayerInventory = &inv;
    let antes_xp: u32 = h.xp.iter().sum();
    let (extras, subidas) = repasar(&mut h, inv_ref, azar);
    let cambio = h.xp.iter().sum::<u32>() != antes_xp;
    for it in extras {
        inv.add(it, 1);
        // La unidad regalada no cuenta como ganada.
        let q = inv.qty(it);
        h.antes.insert(it.to_string(), q);
        persist.mark_dirty();
    }
    if let Some((o, n)) = subidas.last() {
        let en = idioma_actual() == Idioma::En;
        let txt = if en { format!("{} level {n}!", o.nombre(true)) } else { format!("¡{} nivel {n}!", o.nombre(false)) };
        h.aviso = Some((txt, 3.5));
    }
    if cambio {
        guardar(&h);
    }
}

fn pintar_habilidades(
    mut commands: Commands,
    h: Res<Habilidades>,
    ui: Query<Entity, With<HabilidadesUi>>,
    mut textos: Query<&mut Text, With<HabilidadesUi>>,
) {
    if ui.is_empty() {
        commands.spawn((
            Text::new(""),
            TextFont { font_size: 11.0, ..default() },
            TextColor(Color::srgb(0.8, 0.9, 0.85)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(112.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-170.0)),
                width: Val::Px(340.0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            TextLayout::new_with_justify(JustifyText::Center),
            GlobalZIndex(40),
            HabilidadesUi,
            Name::new("Habilidades"),
        ));
        return;
    }
    let en = idioma_actual() == Idioma::En;
    // Sólo se habla cuando se sube de nivel: la línea fija «Huerta 1 · Tala 2» estaba siempre en pantalla, sin que
    // nadie la hubiera pedido, y se leía como un cartel perdido.
    let linea: Vec<String> = h.aviso.iter().map(|(a, _)| a.clone()).collect();
    let t = linea.join("  ·  ");
    for mut x in &mut textos {
        if x.0 != t {
            x.0 = t.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_nivel_crece_con_el_cuadrado_y_topa() {
        assert_eq!(nivel(0), 0);
        assert_eq!(nivel(3), 0);
        assert_eq!(nivel(4), 1);
        assert_eq!(nivel(16), 2);
        assert_eq!(nivel(400), 10);
        assert_eq!(nivel(1_000_000), NIVEL_MAX);
    }

    #[test]
    fn lo_que_ya_traias_no_es_xp_y_lo_ganado_si() {
        let mut h = Habilidades::default();
        let mut inv = PlayerInventory::default();
        inv.add("wood", 20);
        let _ = repasar(&mut h, &inv, |_| 1.0); // fija el "antes"
        h.cargado = true;
        assert_eq!(h.xp, [0; 4]);
        inv.add("wood", 5);
        let _ = repasar(&mut h, &inv, |_| 1.0);
        assert_eq!(h.xp[Oficio::Tala.idx()], 5);
        // Gastar no resta ni da XP.
        inv.remove("wood", 10);
        let _ = repasar(&mut h, &inv, |_| 1.0);
        assert_eq!(h.xp[Oficio::Tala.idx()], 5);
        // Un pilón enorme movido de un cofre da, como mucho, el tope.
        inv.add("wood", 500);
        let _ = repasar(&mut h, &inv, |_| 1.0);
        assert_eq!(h.xp[Oficio::Tala.idx()], 5 + TOPE_POR_REPASO);
    }

    #[test]
    fn los_items_extra_son_items_con_oficio() {
        for i in ITEMS {
            assert!(oficio_de(i).is_some(), "{i} no entrena ningún oficio");
        }
    }
}
