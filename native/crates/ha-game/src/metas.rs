//! **Metas: el hilo que le da algo que hacer al mundo abierto.**
//!
//! Un sandbox sin objetivos es un mundo bonito y vacío: el jugador no sabe qué
//! probar ni cuándo va bien. Esto es una cadena corta de metas que enseña el
//! circuito del juego en orden —juntar → herramientas → refugio de luz → ropa →
//! hierro → cosecha → pueblo → cielo → Luna— y paga cada paso con algo útil.
//!
//! Diseño (biblia §16):
//! * **Una sola meta activa**, siempre visible arriba al centro con su barra.
//!   Sin menú de misiones que abrir: cuando completás una, aparece la siguiente.
//! * **Se miden con lo que ya existe** (mochila, contexto de mundo, estadísticas
//!   de sociedad): ninguna meta pide un contador nuevo en otro sistema.
//! * Lo cumplido se guarda por partida (`metas.json`) — es del MUNDO, no del
//!   personaje, igual que el resto del progreso.
//! * En multijugador, completar una meta se anuncia en la sala: se juega
//!   mirando qué logró el otro.
//! * Es una capa de lectura: no toca la sim ni agrega costo por cuadro (se mira
//!   dos veces por segundo).

use bevy::prelude::*;
use ha_shared::WorldContext;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::game_phase::GamePhase;
use crate::i18n::{idioma_actual, Idioma};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::scene::WorldContextRuntime;

const REVISAR_CADA: f32 = 0.5;
const PUNTOS_POR_META: u32 = 10;

/// Qué hay que cumplir.
#[derive(Clone, Copy)]
enum Cond {
    /// Tener al menos `n` de ese ítem.
    Tener(&'static str, u32),
    /// Tener alguno de estos (cualquiera cuenta como 1).
    Alguno(&'static [&'static str]),
    /// Estar en este contexto de mundo.
    Contexto(WorldContext),
    /// Parejas formadas en el pueblo.
    Parejas(u32),
    /// Infectados abatidos en total (llaves del OASIS).
    Bajas(u32),
    /// Alijos abiertos.
    Alijos(u32),
    /// Nivel mínimo en CUALQUIER oficio (`habilidades.rs`).
    Nivel(u32),
}

struct Meta {
    es: &'static str,
    en: &'static str,
    pista_es: &'static str,
    pista_en: &'static str,
    cond: Cond,
    premio: &'static [(&'static str, u32)],
}

/// La cadena, en orden. Agregar una al final no rompe partidas viejas: se guardan
/// los índices cumplidos y la nueva aparece cuando llegue su turno.
const CADENA: &[Meta] = &[
    Meta { es: "Juntá madera", en: "Gather wood", pista_es: "Mantené sobre un árbol", pista_en: "Hold on a tree", cond: Cond::Tener("wood", 4), premio: &[("apple", 2)] },
    Meta { es: "Fabricá un pico", en: "Craft a pickaxe", pista_es: "MOCHILA · FABRICAR", pista_en: "BACKPACK · CRAFT", cond: Cond::Alguno(&["wood_pickaxe", "stone_pickaxe", "pickaxe"]), premio: &[("bread", 1)] },
    Meta { es: "Juntá piedra", en: "Gather stone", pista_es: "Picá una roca con el pico", pista_en: "Mine a rock with the pickaxe", cond: Cond::Tener("stone", 6), premio: &[("apple", 2)] },
    Meta { es: "Pico de piedra", en: "Stone pickaxe", pista_es: "Más rápido y llega a más", pista_en: "Faster and reaches more", cond: Cond::Alguno(&["stone_pickaxe", "pickaxe"]), premio: &[("torch", 2)] },
    Meta { es: "Hacé una antorcha", en: "Make a torch", pista_es: "De noche la luz es vida", pista_en: "At night, light is life", cond: Cond::Alguno(&["torch", "lantern"]), premio: &[("apple", 3)] },
    Meta { es: "Conseguí cuero", en: "Get leather", pista_es: "De los animales", pista_en: "From animals", cond: Cond::Tener("leather", 2), premio: &[("bread", 2)] },
    Meta { es: "Vestite", en: "Get dressed", pista_es: "Gorro, chaleco, abrigo o botas", pista_en: "Cap, vest, coat or boots", cond: Cond::Alguno(&["leather_cap", "leather_vest", "wool_coat", "leather_boots"]), premio: &[("torch", 3)] },
    Meta { es: "Lingotes de hierro", en: "Iron ingots", pista_es: "Mineral + fuego", pista_en: "Ore + fire", cond: Cond::Tener("iron_ingot", 2), premio: &[("gem_blue", 1)] },
    Meta { es: "Cosechá trigo", en: "Harvest wheat", pista_es: "Sembrá semillas y regá", pista_en: "Plant seeds and water", cond: Cond::Tener("wheat", 3), premio: &[("bread", 3)] },
    Meta { es: "Hacé pan", en: "Bake bread", pista_es: "Trigo en FABRICAR", pista_en: "Wheat in CRAFT", cond: Cond::Tener("bread", 4), premio: &[("apple", 4)] },
    Meta { es: "Un pueblo con pareja", en: "A couple in town", pista_es: "Ayudá a que un NPC se junte", pista_en: "Help an NPC pair up", cond: Cond::Parejas(1), premio: &[("gem_blue", 1)] },
    Meta { es: "Salí al espacio", en: "Reach space", pista_es: "Mantené SALTAR en el aire", pista_en: "Hold JUMP in the air", cond: Cond::Contexto(WorldContext::Space), premio: &[("gem_red", 1)] },
    // LLAVES del OASIS: desafíos grandes con premio único. Se anuncian a la sala.
    Meta { es: "LLAVE DE COBRE: 10 infectados", en: "COPPER KEY: 10 infected", pista_es: "Sobreviví las noches", pista_en: "Survive the nights", cond: Cond::Bajas(10), premio: &[("gem_blue", 2), ("gold_ingot", 1)] },
    Meta { es: "LLAVE DE JADE: nivel 3", en: "JADE KEY: level 3", pista_es: "Dominá un oficio", pista_en: "Master a trade", cond: Cond::Nivel(3), premio: &[("gem_red", 1), ("gold_ingot", 2)] },
    Meta { es: "LLAVE DE CRISTAL: 40 infectados", en: "CRYSTAL KEY: 40 infected", pista_es: "Que la sala te cuente leyenda", pista_en: "Become a legend", cond: Cond::Bajas(40), premio: &[("gem_red", 3), ("gem_blue", 3)] },
    Meta { es: "Abrí un alijo", en: "Open a cache", pista_es: "Seguí la brújula: lo cuidan infectados", pista_en: "Follow the compass: infected guard it", cond: Cond::Alijos(1), premio: &[("lantern", 1), ("bread", 2)] },
    Meta { es: "Cinco alijos", en: "Five caches", pista_es: "Explorá en todas direcciones", pista_en: "Explore in every direction", cond: Cond::Alijos(5), premio: &[("gem_red", 2), ("gold_ingot", 2)] },
    Meta { es: "Pisá la Luna", en: "Land on the Moon", pista_es: "Volá con Ki hacia la Luna", pista_en: "Fly with Ki to the Moon", cond: Cond::Contexto(WorldContext::Luna), premio: &[("gem_red", 2)] },
];

pub fn rango(puntos: u32) -> (&'static str, &'static str) {
    match puntos {
        0..=29 => ("Novato", "Rookie"),
        30..=69 => ("Explorador", "Explorer"),
        70..=109 => ("Pionero", "Pioneer"),
        _ => ("Leyenda", "Legend"),
    }
}

#[derive(Resource, Default)]
pub struct Metas {
    hechas: BTreeSet<usize>,
    cargado: bool,
    acc: f32,
    /// Aviso de "¡meta cumplida!" y cuánto le queda en pantalla.
    aviso: Option<(String, f32)>,
    version: u32,
}

#[derive(Serialize, Deserialize, Default)]
struct Guardado {
    hechas: Vec<usize>,
}

impl Metas {
    pub fn puntos(&self) -> u32 {
        self.hechas.len() as u32 * PUNTOS_POR_META
    }

    /// La primera meta que falta.
    fn actual(&self) -> Option<usize> {
        (0..CADENA.len()).find(|i| !self.hechas.contains(i))
    }
}

fn archivo() -> Option<std::path::PathBuf> {
    crate::partida::activa().map(|id| crate::partida::carpeta(&id).join("metas.json"))
}

fn cargar() -> BTreeSet<usize> {
    archivo()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<Guardado>(&t).ok())
        .map(|g| g.hechas.into_iter().filter(|i| *i < CADENA.len()).collect())
        .unwrap_or_default()
}

fn guardar(m: &Metas) {
    let Some(p) = archivo() else { return };
    let g = Guardado { hechas: m.hechas.iter().copied().collect() };
    if let Ok(t) = serde_json::to_string(&g) {
        let _ = std::fs::write(p, t);
    }
}

/// `(actual, necesario)` de una condición.
fn progreso(c: Cond, inv: &PlayerInventory, ctx: WorldContext, parejas: u32, bajas: u32, nivel: u32, alijos: u32) -> (u32, u32) {
    match c {
        Cond::Tener(id, n) => (inv.qty(id).min(n), n),
        Cond::Alguno(ids) => (ids.iter().any(|i| inv.qty(i) > 0) as u32, 1),
        Cond::Contexto(w) => ((ctx == w) as u32, 1),
        Cond::Parejas(n) => (parejas.min(n), n),
        Cond::Bajas(n) => (bajas.min(n), n),
        Cond::Nivel(n) => (nivel.min(n), n),
        Cond::Alijos(n) => (alijos.min(n), n),
    }
}

#[derive(Component)]
struct MetasUi;
#[derive(Component)]
struct MetasTitulo;
#[derive(Component)]
struct MetasPista;
#[derive(Component)]
struct MetasBarra;
#[derive(Component)]
struct MetasAviso;

pub struct MetasPlugin;

impl Plugin for MetasPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Metas>()
            .add_systems(Update, (revisar_metas, pintar_metas).chain().run_if(in_state(GamePhase::Playing)))
            .add_systems(OnExit(GamePhase::Playing), salir);
    }
}

fn salir(mut commands: Commands, mut m: ResMut<Metas>, ui: Query<Entity, With<MetasUi>>) {
    *m = Metas::default();
    for e in &ui {
        commands.entity(e).despawn_recursive();
    }
}

#[allow(clippy::too_many_arguments)]
fn revisar_metas(
    time: Res<Time>,
    mut m: ResMut<Metas>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    ctx: Res<WorldContextRuntime>,
    soc: Res<crate::society::Society>,
    mut multi: ResMut<crate::multijugador::Multi>,
    bajas: Res<crate::infectados::Bajas>,
    hab: Res<crate::habilidades::Habilidades>,
    alijos: Res<crate::alijos::Alijos>,
) {
    if !m.cargado {
        m.hechas = cargar();
        m.cargado = true;
        m.version += 1;
    }
    m.acc += time.delta_secs();
    if let Some((_, t)) = m.aviso.as_mut() {
        *t -= time.delta_secs();
        if *t <= 0.0 {
            m.aviso = None;
            m.version += 1;
        }
    }
    if m.acc < REVISAR_CADA {
        return;
    }
    m.acc = 0.0;
    let parejas = soc.stats_parejas();
    // Puede cumplirse más de una de golpe (ya tenías el pico): se cobran todas.
    for _ in 0..CADENA.len() {
        let Some(i) = m.actual() else { return };
        let meta = &CADENA[i];
        let (cur, need) = progreso(meta.cond, &inv, ctx.active, parejas, bajas.total, hab.maximo(), alijos.abiertos());
        if cur < need {
            return;
        }
        m.hechas.insert(i);
        for (item, n) in meta.premio {
            inv.add(item, *n);
        }
        persist.mark_dirty();
        guardar(&m);
        let en = idioma_actual() == Idioma::En;
        let nombre = if en { meta.en } else { meta.es };
        let txt = if en { format!("Goal complete: {nombre}!") } else { format!("¡Meta cumplida: {nombre}!") };
        crate::causal::registrar("meta", nombre.to_string());
        multi.decir(&if en { format!("Goal: {nombre}") } else { format!("Meta: {nombre}") });
        m.aviso = Some((txt, 4.0));
        m.version += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn pintar_metas(
    mut commands: Commands,
    m: Res<Metas>,
    inv: Res<PlayerInventory>,
    ctx: Res<WorldContextRuntime>,
    soc: Res<crate::society::Society>,
    bajas: Res<crate::infectados::Bajas>,
    hab: Res<crate::habilidades::Habilidades>,
    alijos: Res<crate::alijos::Alijos>,
    ui: Query<Entity, With<MetasUi>>,
    mut titulo: Query<&mut Text, (With<MetasTitulo>, Without<MetasPista>, Without<MetasAviso>)>,
    mut pista: Query<&mut Text, (With<MetasPista>, Without<MetasTitulo>, Without<MetasAviso>)>,
    mut aviso: Query<(&mut Text, &mut Visibility), (With<MetasAviso>, Without<MetasTitulo>, Without<MetasPista>)>,
    mut barra: Query<&mut Node, With<MetasBarra>>,
) {
    if ui.is_empty() {
        spawn_ui(&mut commands);
        return;
    }
    let en = idioma_actual() == Idioma::En;
    let (rg_es, rg_en) = rango(m.puntos());
    let (t, p, frac) = match m.actual() {
        Some(i) => {
            let meta = &CADENA[i];
            let (cur, need) = progreso(meta.cond, &inv, ctx.active, soc.stats_parejas(), bajas.total, hab.maximo(), alijos.abiertos());
            let nombre = if en { meta.en } else { meta.es };
            let pista = if en { meta.pista_en } else { meta.pista_es };
            let cuenta = if need > 1 { format!("  {cur}/{need}") } else { String::new() };
            (
                format!("{} {}/{} · {nombre}{cuenta}", if en { "GOAL" } else { "META" }, i + 1, CADENA.len(), ),
                format!("{pista}  ·  {} {}", if en { rg_en } else { rg_es }, m.puntos()),
                cur as f32 / need.max(1) as f32,
            )
        }
        None => (
            if en { "ALL GOALS DONE — keep exploring!".to_string() } else { "TODAS LAS METAS CUMPLIDAS — ¡seguí explorando!".to_string() },
            format!("{} {}", if en { rg_en } else { rg_es }, m.puntos()),
            1.0,
        ),
    };
    for mut x in &mut titulo {
        if x.0 != t {
            x.0 = t.clone();
        }
    }
    for mut x in &mut pista {
        if x.0 != p {
            x.0 = p.clone();
        }
    }
    for mut n in &mut barra {
        n.width = Val::Percent((frac * 100.0).clamp(0.0, 100.0));
    }
    for (mut x, mut v) in &mut aviso {
        match &m.aviso {
            Some((s, _)) => {
                if x.0 != *s {
                    x.0 = s.clone();
                }
                *v = Visibility::Inherited;
            }
            None => *v = Visibility::Hidden,
        }
    }
}

fn spawn_ui(commands: &mut Commands) {
    let acento = Color::srgb(0.65, 1.0, 0.85);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(6.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-170.0)),
                width: Val::Px(340.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(6.0)),
                border: UiRect::all(Val::Px(1.0)),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.07, 0.12, 0.72)),
            BorderColor(acento),
            // Sin `Button`/`Interaction`: es de sólo lectura y no le roba toques al mundo.
            GlobalZIndex(40),
            MetasUi,
            Name::new("Metas"),
        ))
        .with_children(|p| {
            p.spawn((Text::new(""), TextFont { font_size: 14.0, ..default() }, TextColor(acento), MetasTitulo));
            p.spawn(Node {
                width: Val::Percent(100.0),
                height: Val::Px(4.0),
                ..default()
            })
            .insert(BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.12)))
            .with_children(|b| {
                b.spawn((
                    Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() },
                    BackgroundColor(Color::srgb(1.0, 0.85, 0.4)),
                    MetasBarra,
                ));
            });
            p.spawn((Text::new(""), TextFont { font_size: 11.0, ..default() }, TextColor(Color::srgb(0.8, 0.8, 0.85)), MetasPista));
            p.spawn((
                Text::new(""),
                TextFont { font_size: 15.0, ..default() },
                TextColor(Color::srgb(1.0, 0.85, 0.4)),
                Visibility::Hidden,
                MetasAviso,
            ));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_cadena_usa_items_que_existen_y_cada_premio_tambien() {
        for m in CADENA {
            let ids: Vec<&str> = match m.cond {
                Cond::Tener(i, _) => vec![i],
                Cond::Alguno(l) => l.to_vec(),
                _ => vec![],
            };
            for id in ids.into_iter().chain(m.premio.iter().map(|(i, _)| *i)) {
                assert!(
                    ha_assets::slots::slot_label(&format!("items/{id}")).is_some(),
                    "la meta «{}» usa un ítem sin slot: {id}",
                    m.es
                );
            }
            assert!(!m.es.is_empty() && !m.en.is_empty() && !m.pista_es.is_empty() && !m.pista_en.is_empty());
        }
    }

    #[test]
    fn se_cobra_en_orden_y_el_progreso_se_mide() {
        let mut inv = PlayerInventory::default();
        assert_eq!(progreso(CADENA[0].cond, &inv, WorldContext::Overworld, 0, 0, 0, 0), (0, 4));
        inv.add("wood", 9);
        assert_eq!(progreso(CADENA[0].cond, &inv, WorldContext::Overworld, 0, 0, 0, 0), (4, 4));
        assert_eq!(progreso(CADENA.iter().find(|m| matches!(m.cond, Cond::Contexto(WorldContext::Space))).unwrap().cond, &inv, WorldContext::Space, 0, 0, 0, 0), (1, 1));
        let mut m = Metas::default();
        assert_eq!(m.actual(), Some(0));
        m.hechas.insert(0);
        assert_eq!(m.actual(), Some(1));
        assert_eq!(rango(0).0, "Novato");
    }

    #[test]
    fn la_fuente_dibuja_las_metas() {
        for m in CADENA {
            for t in [m.es, m.en, m.pista_es, m.pista_en] {
                let rotos: Vec<char> = t.chars().filter(|c| !crate::fuente::dibujable(*c)).collect();
                assert!(rotos.is_empty(), "«{t}» trae caracteres que la fuente no dibuja: {rotos:?}");
            }
        }
    }
}
