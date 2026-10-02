//! **La mesa de experimentos del jugador.** Lo mismo que hacen los eruditos del mundo
//! (`ha_shared::civ::descubrimiento`), pero con tus manos: juntás dos materiales de la mochila,
//! elegís un calor, probás, y el mundo responde con SUS leyes ocultas (`ha_shared::civ::materia`).
//! Lo que sale se anota en tu cuaderno; lo que confirmás dos veces pasa a ser una RECETA que podés
//! repetir cuando quieras. Nadie te dice qué probar: no hay lista de «cosas por descubrir», sólo un
//! mundo con reglas.
//!
//! * El calor al que llegás depende de lo que sabés hacer: fuego de leña; con carbón en la mochila,
//!   más; y con una cerámica confirmada (un horno), más todavía.
//! * Lo que no pasa no gasta nada. Lo que pasa consume lo que juntaste y te da el material nuevo, que
//!   queda en la mochila con su nombre (`materias.rs`) y se puede volver a combinar.
//! * Persistencia: `descubrimientos.json` en la carpeta de la partida.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use ha_shared::civ::descubrimiento::{Cuaderno, PASOS_CALOR};
use ha_shared::civ::materia::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::game_phase::GamePhase;
use crate::hud::item_icons::ItemIconCache;
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::materias::*;

#[derive(Serialize, Deserialize, Default)]
struct Guardado {
    cuaderno: Cuaderno,
    derivados: Vec<Material>,
}

#[derive(Resource, Default)]
pub struct Experimentos {
    materia: Option<Materia>,
    cuaderno: Cuaderno,
    pub abierto: bool,
    sel_a: Option<String>,
    sel_b: Option<String>,
    calor: u8,
    mensaje: String,
    log: Vec<String>,
    version: u32,
    dibujado: Option<(bool, u32, usize)>,
    cargado: bool,
}

impl Experimentos {
    fn materia(&mut self) -> &mut Materia {
        let semilla = ha_shared::partida::semilla() as u64;
        self.materia.get_or_insert_with(|| Materia::nueva(semilla))
    }
    fn tocar(&mut self) {
        self.version = self.version.wrapping_add(1);
    }
}

fn ruta() -> std::path::PathBuf {
    crate::inventory_persist::persist_dir().join("descubrimientos.json")
}

fn guardar(e: &Experimentos) {
    let derivados: Vec<Material> = e
        .materia
        .as_ref()
        .map(|m| m.materiales.values().filter(|x| x.id >= N_CRUDOS).cloned().collect())
        .unwrap_or_default();
    crate::persistent_agents::guardar_json(&ruta(), &Guardado { cuaderno: e.cuaderno.clone(), derivados });
}

#[derive(Component)]
struct PanelExp;
#[derive(Component)]
pub struct BotonExperimentar;
#[derive(Component, Clone, Debug)]
enum AccionExp {
    Material(String),
    QuitarA,
    QuitarB,
    CalorMenos,
    CalorMas,
    Probar,
    Receta(u64),
    Cerrar,
}

pub struct ExperimentosPlugin;

impl Plugin for ExperimentosPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Experimentos>()
            .add_systems(
                Update,
                (cargar_cuaderno, abrir_panel, dibujar_panel, botones).chain().run_if(in_state(GamePhase::Playing)),
            )
            .add_systems(Update, exp_autoshot.run_if(in_state(GamePhase::Playing)).after(botones))
            .add_systems(OnExit(GamePhase::Playing), salir);
    }
}

/// QA de dev: `HA_EXPSHOT=<ruta.png>` (con `HA_AUTOPLAY=1`) llena la mochila de materias, abre el panel,
/// hace dos experimentos (cocer arcilla y reducir cobre) y captura la pantalla.
#[allow(clippy::too_many_arguments)]
fn exp_autoshot(
    mut e: ResMut<Experimentos>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    mut commands: Commands,
    mut exit: EventWriter<bevy::app::AppExit>,
    mut frames: Local<u32>,
) {
    let Ok(path) = crate::flags::var("HA_EXPSHOT") else { return };
    *frames += 1;
    match *frames {
        120 => {
            for (it, n) in [("clay", 4), ("coal", 4), ("copper_ore", 3), ("tin_ore", 3), ("wood", 6), ("sand", 3), ("salt", 2), ("limestone", 2), ("wool", 2)] {
                inv.add(it, n);
            }
            persist.mark_immediate();
            e.abierto = true;
            e.sel_a = Some("clay".into());
            e.sel_b = None;
            e.calor = 9;
            e.tocar();
        }
        140 => {
            probar(&mut e, &mut inv, &mut persist, 123_456_789);
            e.sel_a = Some("copper_ore".into());
            e.sel_b = Some("coal".into());
            e.calor = 12;
            probar(&mut e, &mut inv, &mut persist, 987_654_321);
            e.tocar();
        }
        175 => {
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(path));
        }
        200 => {
            exit.send(bevy::app::AppExit::Success);
        }
        _ => {}
    }
}

fn salir(mut commands: Commands, mut e: ResMut<Experimentos>, q: Query<Entity, With<PanelExp>>) {
    *e = Experimentos::default();
    vaciar_registro();
    for x in &q {
        commands.entity(x).despawn_recursive();
    }
}

fn cargar_cuaderno(mut e: ResMut<Experimentos>) {
    if e.cargado {
        return;
    }
    e.cargado = true;
    e.calor = 8;
    if let Some(g) = crate::persistent_agents::cargar_json::<Guardado>(&ruta()) {
        e.cuaderno = g.cuaderno;
        for m in g.derivados {
            registrar(&m);
            e.materia().registrar(&m);
        }
    }
}

/// Calor máximo del jugador: leña; carbón en la mochila (o carbón vegetal confirmado) suma; una
/// cerámica confirmada (un horno) suma.
fn calor_jugador(e: &Experimentos, inv: &PlayerInventory) -> u8 {
    let producibles = e.cuaderno.producibles();
    let (mut ceramica, mut carbon) = (false, inv.qty("coal") > 0);
    if let Some(m) = &e.materia {
        for id in &producibles {
            match m.get(*id).map(|x| x.clase) {
                Some(Clase::Ceramica) => ceramica = true,
                Some(Clase::Combustible) => carbon = true,
                _ => {}
            }
        }
    }
    ((calor_maximo(ceramica, carbon) * PASOS_CALOR as f32).floor() as u8).max(4)
}

fn abrir_panel(mut e: ResMut<Experimentos>, botones: Query<&Interaction, (Changed<Interaction>, With<BotonExperimentar>)>) {
    if botones.iter().any(|i| *i == Interaction::Pressed) {
        e.abierto = !e.abierto;
        e.tocar();
    }
}

fn nombre_de(e: &mut Experimentos, id: u64) -> String {
    e.materia().get(id).map(|m| m.nombre.clone()).unwrap_or_else(|| "?".into())
}

fn materiales_en_mochila(inv: &PlayerInventory) -> Vec<(String, u32)> {
    let mut v: Vec<(String, u32)> = inv.items.iter().filter(|(i, q)| **q > 0 && material_de_item(i).is_some()).map(|(i, q)| (i.clone(), *q)).collect();
    v.sort();
    v
}

#[allow(clippy::too_many_arguments)]
fn dibujar_panel(
    mut commands: Commands,
    mut e: ResMut<Experimentos>,
    inv: Res<PlayerInventory>,
    mut icons: ResMut<ItemIconCache>,
    mut images: ResMut<Assets<Image>>,
    raiz: Query<Entity, With<PanelExp>>,
) {
    let en_mochila = materiales_en_mochila(&inv);
    let quiero = (e.abierto, e.version, en_mochila.len());
    if e.dibujado == Some(quiero) {
        return;
    }
    e.dibujado = Some(quiero);
    for x in &raiz {
        commands.entity(x).despawn_recursive();
    }
    if !e.abierto {
        return;
    }
    let tmax = calor_jugador(&e, &inv);
    let calor = e.calor.clamp(4, tmax);
    let tinta = crate::ui_pack::Pack::TINTA;
    let suave = crate::ui_pack::Pack::SUAVE;
    let nombre_item = |id: &Option<String>| id.as_deref().map(crate::granja::nombre_item).unwrap_or_else(|| "—".to_string());
    let (na, nb) = (nombre_item(&e.sel_a), nombre_item(&e.sel_b));
    let recetas: Vec<(u64, String)> = {
        let firmes: Vec<(u64, _)> = e.cuaderno.entradas.iter().filter(|(_, x)| x.firme()).map(|(k, x)| (*k, x.clone())).collect();
        firmes
            .iter()
            .take(6)
            .map(|(k, r)| {
                let a = nombre_de(&mut e, r.a);
                let b = if r.b == u64::MAX { String::new() } else { format!(" + {}", nombre_de(&mut e, r.b)) };
                let res = r.resultado.map(|id| nombre_de(&mut e, id)).unwrap_or_default();
                (*k, format!("{a}{b} · calor {:.2} da {res}", r.calor as f32 / PASOS_CALOR as f32))
            })
            .collect()
    };
    let mensaje = e.mensaje.clone();
    let log = e.log.clone();
    let firmes_n = e.cuaderno.firmes().count();
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                top: Val::Px(112.0),
                width: Val::Px(420.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                padding: UiRect::all(Val::Px(8.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.05, 0.07, 0.12)),
            BorderColor(Color::srgb(0.65, 1.0, 0.85)),
            crate::ui_pack::MarcoDeHud(crate::ui_pack::marco::PANEL),
            GlobalZIndex(850),
            FocusPolicy::Block,
            Interaction::default(),
            PanelExp,
            Name::new("PanelExperimentos"),
        ))
        .with_children(|p| {
            let t = |p: &mut ChildBuilder, s: String, px: f32, c: Color| {
                p.spawn((Text::new(s), TextFont { font_size: px, ..default() }, TextColor(c)));
            };
            t(p, "EXPERIMENTOS".into(), 20.0, tinta);
            t(p, format!("Juntá dos materiales y probá con fuego. Recetas firmes: {firmes_n}."), 11.0, suave);
            // Materiales de la mochila.
            p.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(4.0), row_gap: Val::Px(4.0), ..default() })
                .with_children(|f| {
                    if en_mochila.is_empty() {
                        f.spawn((Text::new("No llevás materiales: picá piedra (menas, carbón), tierra (arcilla), arena (sal), talá (resina)."), TextFont { font_size: 11.0, ..default() }, TextColor(suave)));
                    }
                    for (item, q) in en_mochila.iter().take(12) {
                        let ic = icons.icon(item, &mut images);
                        let activo = e.sel_a.as_deref() == Some(item) || e.sel_b.as_deref() == Some(item);
                        f.spawn((
                            Button,
                            Node {
                                width: Val::Px(44.0),
                                height: Val::Px(44.0),
                                border: UiRect::all(Val::Px(2.0)),
                                justify_content: JustifyContent::Center,
                                align_items: AlignItems::Center,
                                ..default()
                            },
                            BorderColor(if activo { Color::srgb(1.0, 0.4, 0.75) } else { Color::srgb(0.65, 1.0, 0.85) }),
                            BackgroundColor(Color::srgb(0.10, 0.14, 0.22)),
                            AccionExp::Material(item.clone()),
                        ))
                        .with_children(|b| {
                            b.spawn((ic.node(), Node { width: Val::Px(32.0), height: Val::Px(32.0), ..default() }));
                            b.spawn((
                                Text::new(q.to_string()),
                                TextFont { font_size: 10.0, ..default() },
                                TextColor(Color::WHITE),
                                Node { position_type: PositionType::Absolute, right: Val::Px(1.0), bottom: Val::Px(0.0), ..default() },
                            ));
                        });
                    }
                });
            t(p, format!("A: {na}    B: {nb}    CALOR {:.2} (máx {:.2})", calor as f32 / PASOS_CALOR as f32, tmax as f32 / PASOS_CALOR as f32), 12.0, tinta);
            p.spawn(Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: Val::Px(6.0), row_gap: Val::Px(6.0), ..default() })
                .with_children(|f| {
                    for (et, a) in [("CALOR −", AccionExp::CalorMenos), ("CALOR +", AccionExp::CalorMas), ("SOLTAR A", AccionExp::QuitarA), ("SOLTAR B", AccionExp::QuitarB), ("PROBAR", AccionExp::Probar), ("CERRAR", AccionExp::Cerrar)] {
                        boton(f, et, a);
                    }
                });
            if !mensaje.is_empty() {
                t(p, mensaje.clone(), 12.0, Color::srgb(0.55, 0.33, 0.05));
            }
            for l in log.iter().rev().take(3) {
                t(p, l.clone(), 10.0, suave);
            }
            if !recetas.is_empty() {
                t(p, "RECETAS (tocá una para repetirla):".into(), 11.0, tinta);
                for (k, txt) in &recetas {
                    boton(p, txt, AccionExp::Receta(*k));
                }
            }
        });
}

fn boton(p: &mut ChildBuilder, etiqueta: &str, a: AccionExp) {
    p.spawn((
        Button,
        Node {
            min_height: Val::Px(30.0),
            border: UiRect::all(Val::Px(2.0)),
            padding: UiRect::horizontal(Val::Px(8.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BorderColor(Color::srgb(0.65, 1.0, 0.85)),
        BackgroundColor(Color::srgb(0.10, 0.14, 0.22)),
        a,
    ))
    .with_children(|b| {
        b.spawn((Text::new(etiqueta.to_string()), TextFont { font_size: 12.0, ..default() }, TextColor(Color::srgb(0.65, 1.0, 0.85))));
    });
}

/// Quita `n` del ítem; devuelve `false` si no alcanza.
fn gastar(inv: &mut PlayerInventory, item: &str, n: u32) -> bool {
    inv.remove(item, n)
}

#[allow(clippy::too_many_arguments)]
fn botones(
    time: Res<Time>,
    mut e: ResMut<Experimentos>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    botones: Query<(&Interaction, &AccionExp), Changed<Interaction>>,
) {
    for (i, a) in &botones {
        if *i != Interaction::Pressed {
            continue;
        }
        match a.clone() {
            AccionExp::Cerrar => e.abierto = false,
            AccionExp::Material(item) => {
                if e.sel_a.as_deref() == Some(&item) {
                    e.sel_a = None;
                } else if e.sel_b.as_deref() == Some(&item) {
                    e.sel_b = None;
                } else if e.sel_a.is_none() {
                    e.sel_a = Some(item);
                } else {
                    e.sel_b = Some(item);
                }
            }
            AccionExp::QuitarA => e.sel_a = None,
            AccionExp::QuitarB => e.sel_b = None,
            AccionExp::CalorMenos => e.calor = e.calor.saturating_sub(1).max(4),
            AccionExp::CalorMas => {
                let tmax = calor_jugador(&e, &inv);
                e.calor = (e.calor + 1).min(tmax);
            }
            AccionExp::Receta(k) => {
                if let Some(r) = e.cuaderno.entradas.get(&k).cloned() {
                    let dame = |x: u64| -> Option<String> {
                        if x == u64::MAX {
                            None
                        } else if x < N_CRUDOS {
                            Some(item_de_crudo(x).to_string())
                        } else {
                            // el ítem derivado que lleve ese material en la mochila
                            inv.items.keys().find(|i| derivado_de_item(i).map(|(_, id)| id) == Some(x)).cloned()
                        }
                    };
                    e.sel_a = dame(r.a);
                    e.sel_b = dame(r.b);
                    e.calor = r.calor;
                    e.mensaje = "Receta cargada: tocá PROBAR.".into();
                }
            }
            AccionExp::Probar => probar(&mut e, &mut inv, &mut persist, time.elapsed().as_micros() as u64),
        }
        e.tocar();
    }
}

fn probar(e: &mut Experimentos, inv: &mut PlayerInventory, persist: &mut InventoryPersistState, azar: u64) {
    let (Some(ia), ib) = (e.sel_a.clone(), e.sel_b.clone()) else {
        e.mensaje = "Elegí al menos un material.".into();
        return;
    };
    let (Some(a), b) = (material_de_item(&ia), ib.as_deref().and_then(material_de_item)) else {
        e.mensaje = "Eso no es una materia.".into();
        return;
    };
    if ib.as_deref() == Some(ia.as_str()) {
        e.mensaje = "Elegí dos materiales distintos.".into();
        return;
    }
    if inv.qty(&ia) < 1 || ib.as_deref().map(|x| inv.qty(x) < 1).unwrap_or(false) {
        e.mensaje = "Te falta alguno en la mochila.".into();
        return;
    }
    let tmax = calor_jugador(e, inv);
    let calor = e.calor.clamp(4, tmax);
    let ruido = 0.92 + 0.16 * (((azar >> 20) & 0xFFFF) as f32 / 65535.0);
    let t = (calor as f32 / PASOS_CALOR as f32) * ruido;
    let ctx = (azar % 8) as u8;
    let res = e.materia().reaccionar(a, b, t, false);
    match res {
        Resultado::Nada => {
            e.cuaderno.anotar(a, b, calor, None, ctx, false);
            e.mensaje = "No pasa nada.".into();
        }
        Resultado::Quema => {
            gastar(inv, &ia, 1);
            e.mensaje = "Arde: se consumió.".into();
            persist.mark_immediate();
        }
        Resultado::Obtiene(m, ley) => {
            let nuevo_nombre = !e.cuaderno.materiales.contains(&m.id) && m.id >= N_CRUDOS;
            e.materia().registrar(&m);
            registrar(&m);
            gastar(inv, &ia, 1);
            if let Some(x) = ib.as_deref() {
                gastar(inv, x, 1);
            }
            let item = item_de_derivado(&m);
            inv.add(&item, 1);
            persist.mark_immediate();
            if m.id >= N_CRUDOS {
                e.cuaderno.materiales.insert(m.id);
            }
            let firme_ahora = e.cuaderno.anotar(a, b, calor, Some((m.id, ley)), ctx, false);
            let lineas = match ley {
                "cuece" => "Se endurece al fuego",
                "carboniza" => "Tapada y a fuego lento no arde: se vuelve carbón",
                "reduce" => "Con carbón y mucho calor suelta un metal",
                "alea" => "Fundidos juntos dan un metal nuevo",
                "calcina" => "Calcinada se vuelve un polvo que liga",
                "vitrifica" => "Con un fundente se vuelve vidrio",
                _ => "Mezclados se vuelven otra cosa",
            };
            e.mensaje = format!("{lineas}: {}.{}", m.nombre, if firme_ahora { " ¡Receta confirmada!" } else { "" });
            if nuevo_nombre {
                let u = USOS.iter().map(|u| (utilidad(&m, *u), u.nombre())).fold((0.0f32, ""), |x, y| if y.0 > x.0 { y } else { x });
                e.log.push(format!("Nuevo: {} (sirve sobre todo para {}).", m.nombre, u.1));
            }
            crate::causal::registrar("experimento", format!("{} da {}", ley, m.nombre));
        }
    }
    let conf: BTreeSet<u64> = e.cuaderno.producibles();
    if e.log.len() > 12 {
        e.log.remove(0);
    }
    let _ = conf;
    guardar(e);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn con_mochila(items: &[(&str, u32)]) -> (Experimentos, PlayerInventory, InventoryPersistState) {
        let mut inv = PlayerInventory::default();
        for (i, n) in items {
            inv.add(i, *n);
        }
        (Experimentos::default(), inv, InventoryPersistState::default())
    }

    /// Lo que dice la skill del jugador: probar con las manos da lo mismo que las leyes ocultas del
    /// mundo, consume lo que se juntó, entrega el material nuevo con su nombre y se anota.
    #[test]
    fn cocer_arcilla_da_una_ceramica_que_se_anota_y_se_nombra() {
        // Con carbón en la mochila el fuego llega más alto: en algunos mundos la arcilla lo necesita.
        let (mut e, mut inv, mut persist) = con_mochila(&[("clay", 3), ("coal", 1)]);
        e.sel_a = Some("clay".into());
        let fus = e.materia().get(ARCILLA).unwrap().props.fusion;
        e.calor = (fus * PASOS_CALOR as f32).ceil() as u8 + 1;
        // Se prueba varias veces: el fuego tiene ruido y la primera puede quedar justa.
        for k in 0..3u64 {
            probar(&mut e, &mut inv, &mut persist, 0x1234_5678 + k * 7919);
        }
        let ceramicas: Vec<_> = inv.items.keys().filter(|i| derivado_de_item(i).map(|(c, _)| c) == Some("ceramica")).cloned().collect();
        assert!(!ceramicas.is_empty(), "no salió cerámica: {:?} / {}", inv.items, e.mensaje);
        assert!(nombre_derivado(&ceramicas[0]).unwrap().contains("cerámica"));
        assert!(inv.qty("clay") < 3, "no se consumió la arcilla");
        assert!(e.cuaderno.entradas.len() >= 1 && e.cuaderno.materiales.len() >= 1);
    }

    #[test]
    fn lo_que_no_pasa_no_gasta_nada() {
        let (mut e, mut inv, mut persist) = con_mochila(&[("stone", 2), ("sand", 2)]);
        e.sel_a = Some("stone".into());
        e.sel_b = Some("sand".into());
        e.calor = 5;
        probar(&mut e, &mut inv, &mut persist, 42);
        assert_eq!(inv.qty("stone"), 2);
        assert_eq!(inv.qty("sand"), 2);
        assert_eq!(e.mensaje, "No pasa nada.");
    }

    #[test]
    fn sin_los_materiales_en_la_mochila_no_se_prueba() {
        let (mut e, mut inv, mut persist) = con_mochila(&[("clay", 1)]);
        e.sel_a = Some("clay".into());
        e.sel_b = Some("coal".into());
        probar(&mut e, &mut inv, &mut persist, 1);
        assert!(e.mensaje.contains("falta"), "{}", e.mensaje);
        assert_eq!(inv.qty("clay"), 1);
    }

    #[test]
    fn el_calor_del_jugador_crece_con_carbon_y_con_un_horno() {
        let (mut e, mut inv, _) = con_mochila(&[]);
        let base = calor_jugador(&e, &inv);
        inv.add("coal", 1);
        let con_carbon = calor_jugador(&e, &inv);
        assert!(con_carbon > base);
        // Una cerámica confirmada (dos veces) es un horno.
        let id = ARCILLA;
        let fus = e.materia().get(id).unwrap().props.fusion;
        let Resultado::Obtiene(c, ley) = e.materia().reaccionar(id, None, fus, false) else { panic!() };
        e.materia().registrar(&c);
        let calor = (fus * 20.0).ceil() as u8;
        e.cuaderno.anotar(id, None, calor, Some((c.id, ley)), 1, false);
        e.cuaderno.anotar(id, None, calor, Some((c.id, ley)), 2, false);
        assert!(calor_jugador(&e, &inv) > con_carbon);
    }
}
