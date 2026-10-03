//! **El menú de pausa** — lo que abre el botón MENÚ en partida.
//!
//! Hasta el 3 oct 2026 era el menú "estilo GTA V" de la época de las naves: un
//! fondo azul opaco, botones de borde cian y, ocupando media pantalla, un mapa
//! ORBITAL del sistema solar con el marcador de "tu nave". En un sandbox de
//! vuelo Ki ese mapa no decía nada (siempre eras un punto magenta pegado a la
//! Tierra) y el estilo no tenía nada que ver con el resto de la interfaz. Se
//! rehízo con el kit del pack (`ui_pack`), en el mismo lienzo de 2400 × 1080 que
//! el menú principal:
//!
//! - **El juego se ve atenuado detrás** (`ui_pack::velo`): pausar no es salir.
//! - **REANUDAR primero, grande y en amarillo**: es lo que se quiere el 90 % de
//!   las veces que se abre una pausa.
//! - A la izquierda, **tu personaje** con sus barras; a la derecha las opciones
//!   en casillas con icono (objetivos de toque grandes, como en la mochila).
//! - Las herramientas de prueba (saltos de mundo, PERF, SONDA) sólo en
//!   escritorio, en una fila chica aparte.
//!
//! El mapa del terreno ya vive en el juego como objeto (`map_item.rs`); el
//! orbital no se reemplazó por nada a propósito.
//!
//! Los botones reusan los mismos componentes de siempre (`UiEditorCommand`,
//! `BotonMundo`, `BotonMultijugador`, `BotonExperimentar`, `ColonyButton`):
//! la lógica no se duplica, sólo cambió cómo se dibujan.

use bevy::prelude::*;
use bevy::ui::FocusPolicy;
use ha_shared::{body_def, CelestialBody};

use crate::art2d::ArtLibrary;
use crate::input::TouchLayout;
use crate::scene::RegionId;
use crate::sim::SimState;
use crate::ui_editor::{UiEditorCommand as C, UiEditorState};
use crate::ui_pack::{self as up, Ancla, Pack};

pub struct GameMenuPlugin;

impl Plugin for GameMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            bevy::prelude::OnExit(crate::game_phase::GamePhase::Playing),
            (crate::salir::reiniciar::<GameMenuState>,),
        );
        app.init_resource::<GameMenuState>().add_systems(
            Update,
            (
                sync_game_menu,
                sync_menu_edit_mode.after(sync_game_menu),
                barras_de_la_pausa,
                update_luna_base_monitor,
                foto_de_la_pausa,
            )
                .chain(),
        );
    }
}

#[derive(Resource, Default)]
struct GameMenuState {
    spawned: bool,
    root: Option<Entity>,
}

#[derive(Component)]
struct GameMenuRoot;
/// Los dos paneles (personaje y pausa): se esconden en EDITAR UI.
#[derive(Component)]
struct MenuPaneles;
/// La barra de edición de la UI — visible sólo en EDITAR UI.
#[derive(Component)]
struct EditToolbar;
/// La línea del campamento lunar (antena) — vacía si no hay antena.
#[derive(Component)]
struct LunaBaseMonitor;
/// El relleno de una barra del personaje (0 hambre, 1 sed, 2 ki).
#[derive(Component)]
struct BarraPausa(usize);

/// Todo lo que el menú necesita para armarse, juntado en un solo parámetro.
#[derive(bevy::ecs::system::SystemParam)]
struct Insumos<'w, 's> {
    art: Res<'w, ArtLibrary>,
    layout: Res<'w, TouchLayout>,
    assets: Res<'w, AssetServer>,
    hoja: Res<'w, up::HojaPj>,
    halo: Res<'w, up::HaloTex>,
    cg: Option<Res<'w, crate::chargen::Chargen>>,
    pj: Res<'w, crate::game_phase::SelectedCharacter>,
    reloj: Res<'w, crate::sky2d::DayClock>,
    region: Res<'w, crate::scene::RegionRuntime>,
    jugador: Query<'w, 's, (&'static Transform, &'static SimState), With<crate::sim::Player>>,
}

fn sync_game_menu(
    editor: Res<UiEditorState>,
    civ: Res<crate::civilizacion::Civ>,
    mut state: ResMut<GameMenuState>,
    mut commands: Commands,
    ins: Insumos,
) {
    // UN menú: se ve mientras esté abierto. EDITAR UI lo deja (se vuelve
    // transparente en `sync_menu_edit_mode`); sólo AJUSTES y el panel MUNDO lo
    // reemplazan.
    let want = editor.menu_open && !editor.settings_open && !civ.panel;
    if want && !state.spawned {
        state.root = Some(spawn_menu(&mut commands, &ins, editor.creative));
        state.spawned = true;
    } else if !want && state.spawned {
        if let Some(root) = state.root.take() {
            commands.entity(root).despawn_recursive();
        }
        state.spawned = false;
    }
}

/// EDITAR UI: el MISMO menú se vuelve transparente (paneles fuera, velo fuera)
/// para que se vean los controles de pantalla y sus manijas — no hay un segundo
/// panel. Queda sólo la barra de edición abajo.
#[allow(clippy::type_complexity)]
fn sync_menu_edit_mode(
    editor: Res<UiEditorState>,
    mut roots: Query<(&mut BackgroundColor, &mut FocusPolicy), With<GameMenuRoot>>,
    mut sets: ParamSet<(Query<&mut Node, With<MenuPaneles>>, Query<&mut Node, With<EditToolbar>>)>,
) {
    let Ok((mut bg, mut fp)) = roots.get_single_mut() else {
        return;
    };
    let (velo, foco, paneles, barra) = if editor.enabled {
        (Color::NONE, FocusPolicy::Pass, Display::None, Display::Flex)
    } else {
        (up::VELO, FocusPolicy::Block, Display::Flex, Display::None)
    };
    if bg.0 != velo {
        bg.0 = velo;
    }
    if *fp != foco {
        *fp = foco;
    }
    // `Display` y no `Visibility`: un nodo oculto sigue ocupando su lugar, y
    // tocar un `Node` sin que cambie relayoutea el árbol entero (§10, cambio 3).
    for mut n in &mut sets.p0() {
        if n.display != paneles {
            n.display = paneles;
        }
    }
    for mut n in &mut sets.p1() {
        if n.display != barra {
            n.display = barra;
        }
    }
}

/// El cuerpo donde estás parado, o `None` en el espacio / en tránsito.
fn surface_body(region: &RegionId) -> Option<CelestialBody> {
    match region {
        RegionId::Overworld { .. } => Some(CelestialBody::Earth),
        RegionId::Luna => Some(CelestialBody::Moon),
        RegionId::Planet(b) => Some(*b),
        _ => None,
    }
}

fn body_menu_name(b: CelestialBody) -> String {
    match b {
        CelestialBody::Earth => "TIERRA".to_string(),
        CelestialBody::Moon => "LUNA".to_string(),
        other => body_def(other).label.to_uppercase(),
    }
}

/// El icono de la chapa de la barra: el globo que corresponde.
fn icono_del_cuerpo(b: Option<CelestialBody>) -> &'static str {
    match b {
        Some(CelestialBody::Moon) => "ui/menu_luna",
        None => "ui/menu_espacio",
        _ => up::CUERPO_TIERRA,
    }
}

/// Las coordenadas del mockup de cada pieza de la pausa.
mod geo {
    /// Panel del personaje.
    pub const PJ: (f32, f32, f32, f32) = (60.0, 186.0, 900.0, 828.0);
    /// Panel de la pausa.
    pub const PAUSA: (f32, f32, f32, f32) = (1020.0, 186.0, 1320.0, 828.0);
    /// Margen interior de un panel del kit.
    pub const MARGEN: f32 = 42.0;
    /// Separación entre casillas.
    pub const GAP: f32 = 24.0;
}

fn spawn_menu(commands: &mut Commands, ins: &Insumos, creativo: bool) -> Entity {
    let k = up::k(&ins.layout);
    let art = &*ins.art;
    let cuerpo = surface_body(&ins.region.active);
    let ficha = crate::partida::ficha_activa();
    let (h, m) = ins.reloj.clock_hm();
    let dia = ins.reloj.day_index() as u32 + 1;

    commands
        .spawn((
            up::velo(),
            GlobalZIndex(900),
            FocusPolicy::Block,
            GameMenuRoot,
            Name::new("GameMenu"),
        ))
        .with_children(|root| {
            root.spawn(up::lienzo(k)).with_children(|l| {
                // ── La barra de arriba: dónde estás y cuándo ─────────────────
                let fichas = [
                    up::Ficha {
                        icono: "ui/ic_casa",
                        etiqueta: "PARTIDA",
                        valor: ficha.as_ref().map(|f| f.nombre.clone()).unwrap_or_else(|| "Sin nombre".into()),
                    },
                    up::Ficha { icono: "ui/ic_reloj", etiqueta: "DÍA", valor: format!("{dia} · {h:02}:{m:02}") },
                    up::Ficha {
                        icono: "ui/ic_grilla",
                        etiqueta: "SEMILLA",
                        valor: ficha
                            .as_ref()
                            .map(|f| crate::boot_ui::separar_miles(f.semilla))
                            .unwrap_or_else(|| "—".into()),
                    },
                    up::Ficha {
                        icono: "ui/ic_persona",
                        etiqueta: "MODO",
                        valor: if creativo { "Creativo".into() } else { "Supervivencia".into() },
                    },
                ];
                let nombre_cuerpo = cuerpo.map(body_menu_name).unwrap_or_else(|| "ESPACIO".into());
                up::barra_estado_en(l, art, icono_del_cuerpo(cuerpo), &nombre_cuerpo, &fichas, k);

                l.spawn((up::caja(0.0, 0.0, up::ANCHO, up::ALTO, k), MenuPaneles))
                    .with_children(|p| {
                        panel_personaje(p, ins, cuerpo, k);
                        panel_pausa(p, art, k);
                    });

                // ── El campamento lunar (sólo con antena) ────────────────────
                up::texto_marcado(l, "", 26.0, Pack::TEXTO, 60.0, 1046.0, Ancla::Izq, k, LunaBaseMonitor);
                for (i, (et, accion)) in [
                    ("COLONO (5 manz.)", crate::colony::ColonyAction::SendColonist),
                    ("VÍVERES (3 manz.)", crate::colony::ColonyAction::SendSupplies),
                ]
                .into_iter()
                .enumerate()
                {
                    up::boton(
                        l, art, 1640.0 + i as f32 * 340.0, 1022.0, 320.0, 56.0, Some(et), None, false, 24.0,
                        (
                            crate::colony::ColonyButton(accion),
                            BackgroundColor(Color::NONE),
                            Visibility::Hidden,
                            Name::new("ColonyUplinkRow"),
                        ),
                        k,
                    );
                }

                barra_de_edicion(l, art, k);
            });
        })
        .id()
}

/// **Tu personaje**: el muñeco con su claro de luz, el nombre, dónde estás y las
/// tres barras. Es lo que hace que la pausa se sienta tuya y no un formulario.
fn panel_personaje(p: &mut ChildBuilder, ins: &Insumos, cuerpo: Option<CelestialBody>, k: f32) {
    let art = &*ins.art;
    let (x, y, w, hh) = geo::PJ;
    let cx = x + w / 2.0;
    up::marco(p, art, up::marco::PANEL, x, y, w, hh, k);

    up::halo(p, &ins.halo, cx, 500.0, 250.0, k);
    up::sombra(p, cx, 664.0, 92.0, 15.0, k);
    if let (Some(cg), Some(look)) = (ins.cg.as_deref(), ins.pj.look.as_ref()) {
        if cg.available() {
            let escala = 5.0;
            let cel = Vec2::new(cg.index.cell[0] as f32, cg.index.cell[1] as f32) * escala;
            up::muneco(p, look, cg, &ins.assets, &ins.hoja, 3, cx - cel.x / 2.0, 684.0 - cel.y, escala, k);
        }
    }
    let nombre = if ins.pj.name.trim().is_empty() { "Viajero".to_string() } else { ins.pj.name.clone() };
    up::texto(p, nombre, 50.0, Pack::TINTA, cx, 728.0, Ancla::Medio, k);

    let donde = match (cuerpo, ins.jugador.get_single()) {
        (Some(b), Ok((tf, _))) => format!(
            "{} · {:.0}, {:.0}",
            body_def(b).label,
            tf.translation.x,
            -tf.translation.y
        ),
        (None, _) => "En el espacio".to_string(),
        (Some(b), Err(_)) => body_def(b).label.to_string(),
    };
    up::texto(p, donde, 28.0, Pack::SUAVE, cx, 778.0, Ancla::Medio, k);

    let valores = ins.jugador.get_single().map(|(_, s)| valores_de(s)).unwrap_or([1.0; 3]);
    for (i, (et, color)) in [("HAMBRE", up::BARRA_HAMBRE), ("SED", up::BARRA_SED), ("KI", up::BARRA_KI)]
        .into_iter()
        .enumerate()
    {
        let by = 826.0 + i as f32 * 58.0;
        up::texto(p, et, 28.0, Pack::ETIQUETA, x + 66.0, by + 21.0, Ancla::Izq, k);
        up::barra(p, art, x + 250.0, by, w - 316.0, 42.0, color, valores[i], BarraPausa(i), k);
    }
}

/// Hambre, sed y aguante del jugador, de 0 a 1 (lo mismo que muestra la mochila).
fn valores_de(sim: &SimState) -> [f32; 3] {
    [
        (sim.survival.hunger / ha_shared::HUNGER_MAX).clamp(0.0, 1.0),
        (sim.survival.thirst / ha_shared::THIRST_MAX).clamp(0.0, 1.0),
        if sim.body.max_stamina > 0.0 { (sim.body.stamina / sim.body.max_stamina).clamp(0.0, 1.0) } else { 0.0 },
    ]
}

/// **El panel PAUSA**: REANUDAR arriba y grande, las opciones en casillas con
/// icono, y MENÚ PRINCIPAL abajo (lejos de REANUDAR: salir no puede estar a un
/// dedo resbalado de volver al juego).
fn panel_pausa(p: &mut ChildBuilder, art: &ArtLibrary, k: f32) {
    let (x, y, w, hh) = geo::PAUSA;
    let cx = x + w / 2.0;
    up::marco(p, art, up::marco::PANEL, x, y, w, hh, k);
    let ix = x + geo::MARGEN;
    let iw = w - geo::MARGEN * 2.0;

    up::boton(p, art, ix, y + 84.0, iw, 150.0, Some("REANUDAR"), None, true, 56.0,
              (C::CloseMenu, BackgroundColor(Color::NONE)), k);

    // Seis casillas, 3 × 2.
    let tw = (iw - geo::GAP * 2.0) / 3.0;
    let th = 196.0;
    let y0 = y + 84.0 + 150.0 + geo::GAP + 6.0;
    let casillas: [(&str, &str); 6] = [
        ("AJUSTES", "ui/ic_engranaje"),
        ("MUNDO", up::CUERPO_TIERRA),
        ("MULTIJUGADOR", "ui/ic_persona"),
        ("CREATIVO", "ui/menu_creativo"),
        ("EXPERIMENTAR", "ui/ic_interrogacion"),
        ("EDITAR UI", "ui/ic_lapiz"),
    ];
    for (i, (et, ic)) in casillas.into_iter().enumerate() {
        let tx = ix + (i % 3) as f32 * (tw + geo::GAP);
        let ty = y0 + (i / 3) as f32 * (th + geo::GAP);
        let mut casilla = up::marco(p, art, up::marco::SLOT, tx, ty, tw, th, k);
        casilla.insert((Button, BackgroundColor(Color::NONE), Name::new(format!("GameMenuBtn_{et}"))));
        match et {
            "AJUSTES" => casilla.insert(C::OpenSettings),
            "MUNDO" => casilla.insert(crate::civilizacion::BotonMundo),
            "MULTIJUGADOR" => casilla.insert(crate::multijugador::BotonMultijugador),
            "CREATIVO" => casilla.insert(C::ToggleCreative),
            "EXPERIMENTAR" => casilla.insert(crate::experimentos::BotonExperimentar),
            _ => casilla.insert(C::Toggle),
        };
        up::icono(p, art, ic, tx + tw / 2.0 - 46.0, ty + 26.0, 92.0, k);
        up::texto(p, et, 32.0, Pack::TINTA, tx + tw / 2.0, ty + th - 38.0, Ancla::Medio, k);
    }

    // La fila de abajo: MENÚ PRINCIPAL (y en escritorio, las herramientas).
    let yb = y + hh - 42.0 - 112.0;
    #[cfg(target_os = "android")]
    let ancho_salir = iw;
    #[cfg(not(target_os = "android"))]
    let ancho_salir = 560.0;
    up::boton(p, art, ix, yb, ancho_salir, 112.0, Some("MENÚ PRINCIPAL"), None, false, 38.0,
              (C::SalirAlMenu, BackgroundColor(Color::NONE)), k);
    // Los saltos de mundo, el medidor y la sonda son herramientas de prueba: en
    // el teléfono no aparecen (29 sep 2026).
    #[cfg(not(target_os = "android"))]
    {
        let herramientas: [(&str, C); 5] = [
            ("TIERRA", C::GoEarth),
            ("LUNA", C::GoLuna),
            ("ESPACIO", C::GoSpace),
            ("PERF", C::TogglePerf),
            ("SONDA", C::ToggleProbe),
        ];
        let x0 = ix + ancho_salir + geo::GAP;
        let bw = (iw - ancho_salir - geo::GAP - 4.0 * 10.0) / 5.0;
        for (i, (et, cmd)) in herramientas.into_iter().enumerate() {
            up::boton(p, art, x0 + i as f32 * (bw + 10.0), yb, bw, 112.0, Some(et), None, false, 24.0,
                      (cmd, BackgroundColor(Color::NONE)), k);
        }
    }

    up::cartel(p, art, "PAUSA", cx, y - 48.0, 420.0, 96.0, 54.0, k);
    // La cruz de cerrar, en la esquina: el icono del kit ya es un botón entero.
    if let Some(img) = art.ui_icon("ui/ic_equis") {
        p.spawn((
            up::caja(x + w - 118.0, y + 6.0, 96.0, 96.0, k),
            img,
            Button,
            BackgroundColor(Color::NONE),
            C::CloseMenu,
            ZIndex(5),
            Name::new("GameMenuBtn_CERRAR"),
        ));
    }
}

/// **La barra de EDITAR UI**: elegir qué control mover, escalarlo y guardarlo.
/// Va abajo, sobre el juego transparente, y LISTO vuelve a la pausa.
fn barra_de_edicion(l: &mut ChildBuilder, art: &ArtLibrary, k: f32) {
    let (x, y, w, h) = (180.0, 852.0, 2040.0, 210.0);
    let mut nodo = up::caja(x, y, w, h, k);
    nodo.display = Display::None;
    let mut barra = match art.ui_frame_node(up::marco::LISO) {
        Some(img) => l.spawn((nodo, img, EditToolbar)),
        None => l.spawn((nodo, BackgroundColor(Color::srgb(0.796, 0.706, 0.580)), EditToolbar)),
    };
    let botones: [(&str, C, bool); 8] = [
        ("> STICK", C::SelectMove, false),
        ("> USAR", C::SelectUse, false),
        ("> SALTAR", C::SelectJump, false),
        ("-", C::ScaleDown, false),
        ("+", C::ScaleUp, false),
        ("RESET", C::Reset, false),
        ("GUARDAR", C::Save, false),
        ("LISTO", C::Toggle, true),
    ];
    barra.with_children(|b| {
        up::texto(b, "EDITAR UI · elegí un control, arrastralo y escalalo", 30.0, Pack::TINTA, w / 2.0, 46.0,
                  Ancla::Medio, k);
        let gap = 14.0;
        let bw = (w - 60.0 - gap * 7.0) / 8.0;
        for (i, (et, cmd, fuerte)) in botones.into_iter().enumerate() {
            up::boton(b, art, 30.0 + i as f32 * (bw + gap), 84.0, bw, 96.0, Some(et), None, fuerte, 30.0,
                      (cmd, BackgroundColor(Color::NONE)), k);
        }
    });
}

/// Las barras del personaje dicen la verdad mientras la pausa está abierta (el
/// juego no se detiene: en multijugador no puede).
fn barras_de_la_pausa(
    menu: Res<GameMenuState>,
    player: Query<&SimState, With<crate::sim::Player>>,
    mut barras: Query<(&mut Node, &BarraPausa)>,
    mut ultimo: Local<[f32; 3]>,
) {
    if !menu.spawned {
        return;
    }
    let Ok(sim) = player.get_single() else { return };
    let v = valores_de(sim);
    // Sólo si se movió algo visible: tocar un `Node` relayoutea todo (§10).
    if v.iter().zip(ultimo.iter()).all(|(a, b)| (a - b).abs() < 0.004) {
        return;
    }
    *ultimo = v;
    for (mut nodo, b) in &mut barras {
        nodo.width = Val::Percent(v[b.0.min(2)] * 100.0);
    }
}

/// Antena lunar — con el menú abierto, el estado del campamento: taladros,
/// hierro juntado mientras no estabas y colonos.
fn update_luna_base_monitor(
    menu: Res<GameMenuState>,
    machines: Res<crate::machines::MachineWorld>,
    colony: Res<crate::colony::ColonyRuntime>,
    mut monitor: Query<&mut Text, With<LunaBaseMonitor>>,
) {
    if !menu.spawned {
        return;
    }
    let Ok(mut text) = monitor.get_single_mut() else {
        return;
    };
    let line = if machines.has_antenna_on(CelestialBody::Moon) {
        let drills = machines
            .on_body(CelestialBody::Moon)
            .filter(|m| m.machine_id == ha_shared::REGOLITH_DRILL.id)
            .count();
        let habitats = machines
            .on_body(CelestialBody::Moon)
            .filter(|m| m.machine_id == ha_shared::HABITAT.id)
            .count() as u32;
        let banked: u32 = machines.on_body(CelestialBody::Moon).map(|m| m.stored_output).sum();
        let crew = colony.state.colonists;
        let cap = ha_shared::colony_capacity(habitats);
        let fed = if crew == 0 {
            ""
        } else if ha_shared::colony_fed(&colony.state) {
            " (trabajando x1,5)"
        } else {
            " (SIN VÍVERES)"
        };
        format!(
            "Base Luna · antena OK · taladros {drills} · hierro {banked} · colonos {crew}/{cap}{fed} · víveres {:.0}",
            colony.state.food_stock
        )
    } else {
        String::new()
    };
    if text.0 != line {
        text.0 = line;
    }
}

/// `HA_PAUSASHOT=/ruta.png`: abre la pausa en partida, la fotografía y cierra el
/// juego. Es como se compara contra la maqueta sin el teléfono (biblia §7).
fn foto_de_la_pausa(
    mut cuadros: Local<u32>,
    fase: Res<State<crate::game_phase::GamePhase>>,
    mut editor: ResMut<UiEditorState>,
    mut commands: Commands,
    mut exit: EventWriter<AppExit>,
) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    let Ok(ruta) = crate::flags::var("HA_PAUSASHOT") else { return };
    if *fase.get() != crate::game_phase::GamePhase::Playing {
        return;
    }
    *cuadros += 1;
    match *cuadros {
        90 => editor.menu_open = true,
        130 => {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(ruta.clone()));
            eprintln!("ha-pausashot: pausa");
        }
        140 => editor.enabled = true,
        175 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(ruta.replace(".png", "-editar.png")));
            eprintln!("ha-pausashot: editar");
        }
        200 => {
            exit.send(AppExit::Success);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Las seis casillas, REANUDAR y la fila de abajo entran en el panel.
    #[test]
    fn la_pausa_entra_en_su_panel() {
        let (x, y, w, h) = geo::PAUSA;
        let iw = w - geo::MARGEN * 2.0;
        let th = 196.0;
        let y0 = y + 84.0 + 150.0 + geo::GAP + 6.0;
        let fin_casillas = y0 + 2.0 * th + geo::GAP;
        let yb = y + h - 42.0 - 112.0;
        assert!(fin_casillas <= yb - 8.0, "las casillas pisan MENÚ PRINCIPAL: {fin_casillas} > {yb}");
        assert!(x + geo::MARGEN + iw <= up::ANCHO);
        assert!(y + h <= up::ALTO, "el panel se sale del lienzo");
        let (px, _, pw, _) = geo::PJ;
        assert!(px + pw <= x, "los dos paneles se pisan");
    }
}
