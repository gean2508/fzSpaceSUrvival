//! Menú de NPC — tocá un NPC en el mundo y se abre un panel con su nombre/rol,
//! sus **últimos 5 mensajes** y botones para seguir la conversación. El panel es
//! UNO solo (compartido), pero sus botones cambian según el ROL del NPC
//! (`society::npc_role`): un FIEL no ofrece lo mismo que un HOSTIL o un ALDEANO.
//!
//! Las burbujas de diálogo del mundo siguen su curso (aparecen y se desvanecen,
//! ver `dialog.rs`); este menú es la vista persistente para conversar. ASCII-only
//! (la fuente del juego no dibuja acentos — memoria `game-font-ascii-only`).

use bevy::app::AppExit;
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use ha_shared::Ps1Retro;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::ui::FocusPolicy;
use bevy::window::PrimaryWindow;

use crate::camera::GameCamera;
use crate::companion::{Companion, CompanionOrder, CompanionTask};
use crate::dialog::{say, say_from, SpeechQueue};
use crate::input::{touch_blocked, touch_in_move_zone, TouchBlockRects, TouchLayout};
use crate::inventory_persist::PlayerInventory;
use crate::sim::Player;
use crate::society::{
    npc_line, npc_name, npc_role, push_history, Npc, Religion, Society, CALM_CEILING,
    CALM_COOLDOWN, CALM_STEP, CONVERT_AT, CONVERT_COOLDOWN, CONVERT_MIN_REP, TALK_RANGE,
    TRADE_APPLE_COST,
};

/// Radio (unidades de mundo) alrededor del NPC donde un toque lo selecciona.
const PICK_R: f32 = 1.6;
const CONVERT_STEP: f32 = 0.3;
/// Espera entre sermones **mientras el culto no existe todavía**. `CONVERT_COOLDOWN`
/// (60 s) está pensado para que no conviertas al pueblo entero a fuerza de clicks
/// —para eso está el contagio de vecino a vecino—, pero aplicado al PRIMER fiel
/// dejaba fundar la religión detrás de dos esperas de un minuto. Fundar tiene que
/// costar una charla, no un trámite; a partir del primer creyente manda el
/// cooldown largo.
const FOUND_COOLDOWN: f32 = 8.0;

const NEON_CYAN: Color = Color::srgb(Ps1Retro::TEXT[0], Ps1Retro::TEXT[1], Ps1Retro::TEXT[2]);
const NEON_MAGENTA: Color = Color::srgb(Ps1Retro::SELECT[0], Ps1Retro::SELECT[1], Ps1Retro::SELECT[2]);
const PANEL_BG: Color = Color::srgb(0.05, 0.07, 0.12);
const BTN_IDLE: Color = Color::srgb(0.10, 0.14, 0.22);

pub struct NpcMenuPlugin;

impl Plugin for NpcMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(bevy::prelude::OnExit(crate::game_phase::GamePhase::Playing), (crate::salir::reiniciar::<NpcMenu>, crate::salir::reiniciar::<Encargos>));
        app.init_resource::<NpcMenu>()
            .init_resource::<Encargos>()
            .init_resource::<NpcShot>()
            .add_systems(
                Update,
                (pick_npc, close_menu_by_distance, npc_menu_autoshot, sync_menu, menu_buttons, menu_saber)
                    .chain(),
            )
            .add_systems(
                Update,
                (pick_companero, sync_menu_companero, menu_companero).chain().after(menu_buttons),
            );
    }
}

/// QA de dev: `HA_NPCSHOT=<ruta.png>` (con `HA_AUTOPLAY=1`) abre el menú del NPC
/// más cercano, le hace decir un par de líneas y captura el panel. Usa la captura
/// in-engine → sin herramienta externa del SO.
#[derive(Resource, Default)]
struct NpcShot {
    frames: u32,
    done: bool,
}

#[allow(clippy::too_many_arguments)]
fn npc_menu_autoshot(
    mut shot: ResMut<NpcShot>,
    mut menu: ResMut<NpcMenu>,
    mut npcs: Query<(&mut Npc, &Transform)>,
    player: Query<&Transform, With<Player>>,
    religion: Res<Religion>,
    soc: Res<Society>,
    mut speech: ResMut<SpeechQueue>,
    mut commands: Commands,
    mut exit: EventWriter<AppExit>,
    selected: Res<crate::game_phase::SelectedCharacter>,
) {
    let Ok(path) = crate::flags::var("HA_NPCSHOT") else {
        return;
    };
    if shot.done {
        return;
    }
    shot.frames += 1;
    // Esperar a que la sociedad haya spawneado cerca del jugador.
    if shot.frames == 120 {
        let Ok(ptf) = player.get_single() else {
            shot.frames = 60; // reintentar
            return;
        };
        let pw = ptf.translation.truncate();
        let mut best: Option<(u64, f32)> = None;
        for (npc, tf) in &npcs {
            let d = tf.translation.truncate().distance(pw);
            if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((npc.uid, d));
            }
        }
        if let Some((uid, _)) = best {
            // Semilla de historial para que el panel muestre conversación.
            if let Some((mut npc, _)) = npcs.iter_mut().find(|(n, _)| n.uid == uid) {
                let rep = soc.rep(npc.family);
                for k in 0..3u64 {
                    let line = npc_line(rep, npc.converted, npc.faith, religion.founded, &religion.name, &selected.name, npc.uid + k);
                    push_history(&mut npc.history, &line);
                    say(&mut speech, npc.x, npc.z, line, Color::srgb(0.92, 0.96, 1.0));
                }
            }
            menu.open_uid = Some(uid);
            menu.refresh = true;
            eprintln!("ha-npcshot: menu abierto para uid {uid}");
        } else {
            eprintln!("ha-npcshot: no hay NPCs cerca");
        }
    }
    if shot.frames == 140 {
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
    if shot.frames >= 165 {
        shot.done = true;
        exit.send(AppExit::Success);
    }
}

#[derive(Resource, Default)]
struct NpcMenu {
    /// NPC abierto (uid) o `None` = cerrado.
    open_uid: Option<u64>,
    /// uid ya renderizado (para reconstruir sólo al cambiar).
    shown_uid: Option<u64>,
    /// forzar reconstrucción (tras Hablar, etc.).
    refresh: bool,
    /// Grilla de expresiones del retrato. Se crea UNA vez: el panel se
    /// reconstruye en cada "HABLAR" y armar un layout nuevo cada vez iba dejando
    /// `TextureAtlasLayout` huérfanos para toda la partida.
    portrait_layout: Option<Handle<TextureAtlasLayout>>,
    /// El menú abierto es el del COMPAÑERO (no hay uid de vecino).
    companero: bool,
    /// Ya se armó el panel del compañero.
    companero_shown: bool,
}

#[derive(Component)]
struct NpcMenuRoot;
#[derive(Component)]
struct NpcMenuHistory;
#[derive(Component)]
struct CompaneroAccion(CompanionTask);
#[derive(Component)]
struct CompaneroCerrar;
#[derive(Component)]
struct CompaneroCharlar;

/// Fama mínima de su familia con vos para que acepte ser tu pareja.
const PAREJA_MIN_REP: f32 = 0.3;
/// Cuánto sube la fama de la familia cada trato hecho.
const TRADE_REP_GAIN: f32 = 0.08;

#[derive(Component, Clone, Copy)]
enum NpcAction {
    Talk,
    Convert,
    Calm,
    Trade,
    Pareja,
    /// El aldeano te pide algo según su oficio; entregarlo da fama y premio.
    Encargo,
    /// Cambia lo que llevás (comida, material, cuero…) por lingotes de oro.
    Vender,
    /// Gasta lingotes de oro en una prenda.
    Comprar,
    /// Preguntarle en qué anda: lo que prueba, lo que sabe. Charlar así hace amistad.
    Preguntar,
    /// Pedirle que te enseñe una receta de las que sabe (si te aprecia lo bastante).
    Aprender,
    /// Enseñarle al pueblo una receta que descubriste vos.
    Ensenar,
    Close,
}

/// Lo que cada aldeano pidió y todavía no se le entregó.
#[derive(Resource, Default)]
pub struct Encargos {
    pendientes: std::collections::HashMap<u64, (&'static str, u32)>,
    /// Segundo (de reloj) hasta el que ese aldeano no pide otra cosa.
    descanso: std::collections::HashMap<u64, f32>,
}

const ENCARGO_REP: f32 = 0.15;
const ENCARGO_DESCANSO: f32 = 240.0;

/// Cuántas unidades de este ítem valen UN lingote de oro al venderlas (`None` = no se compra).
pub fn unidades_por_oro(item: &str) -> Option<u32> {
    Some(match item {
        "apple" | "orange" | "grape" | "carrot" | "egg" | "wheat" | "sticks" | "wood" | "stone" | "sand" => 12,
        "bread" | "pie" | "soup" | "meat" | "beef" | "pork" | "chicken" | "mutton" | "rabbit_meat" | "pumpkin" => 6,
        "leather" | "wool" | "feather" | "bone" | "iron_ingot" => 4,
        "gem_blue" | "gem_red" => 1,
        _ => return None,
    })
}

/// Lingotes de oro que cuesta una prenda hallable.
pub const PRECIO_PRENDA: u32 = 2;

/// Qué pide un aldeano: sale de su uid y de su oficio, siempre lo mismo para el mismo.
fn pedido_para(uid: u64, role: &str) -> (&'static str, u32) {
    let lista: &[(&str, u32)] = match role {
        "MINERO" => &[("stone", 8), ("iron_ingot", 1), ("torch", 2)],
        "GRANJERO" => &[("wheat", 4), ("apple", 5), ("bread", 2)],
        "CONSTRUCTOR" => &[("wood", 10), ("stone", 6), ("sticks", 8)],
        _ => &[("apple", 4), ("wood", 6), ("leather", 2), ("bread", 1)],
    };
    lista[(sims_hash(uid) % lista.len() as u64) as usize]
}

fn sims_hash(x: u64) -> u64 {
    let mut h = x.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 29;
    h.wrapping_mul(0xBF58_476D_1CE4_E5B9) >> 8
}

/// Premio por entregar: más fama y una piedra preciosa de vez en cuando.
fn premio_para(uid: u64) -> (&'static str, u32) {
    if sims_hash(uid ^ 77) % 3 == 0 { ("gem_blue", 1) } else { ("apple", 3) }
}

/// Selección por toque/click: convierte el puntero a mundo y abre el NPC más
/// cercano dentro de `PICK_R`. Ignora clicks sobre el propio panel (que los
/// maneja `menu_buttons`).
#[allow(clippy::too_many_arguments)]
fn pick_npc(
    mouse: Res<ButtonInput<MouseButton>>,
    mut touches: EventReader<TouchInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    npcs: Query<(&Npc, &Transform)>,
    over_menu: Query<&Interaction, With<NpcMenuRoot>>,
    touch_layout: Res<TouchLayout>,
    blocks: Res<TouchBlockRects>,
    mut menu: ResMut<NpcMenu>,
) {
    // Si el puntero está sobre el panel, no seleccionar (es interacción de UI).
    if over_menu.iter().any(|i| *i != Interaction::None) {
        touches.clear();
        return;
    }
    // D5: un dedo en el joystick (o sobre cualquier panel de UI) no debe abrir el menú de
    // NPC aunque haya uno cerca — antes cualquier `TouchInput::Started` crudo lo abría.
    let screen = if mouse.just_pressed(MouseButton::Left) {
        windows.get_single().ok().and_then(|w| w.cursor_position())
    } else {
        touches
            .read()
            .find(|t| {
                t.phase == TouchPhase::Started
                    && !touch_in_move_zone(t.position, &touch_layout)
                    && !touch_blocked(t.position, &blocks.0)
            })
            .map(|t| t.position)
    };
    let Some(screen) = screen else {
        return;
    };
    let Ok((camera, cam_tf)) = cam.get_single() else {
        return;
    };
    let Ok(win) = windows.get_single() else {
        return;
    };
    // Píxeles de ventana → mundo teniendo en cuenta la escala de render: con la
    // cámara apuntando a una imagen más chica, pasarle el toque crudo desviaría
    // la selección justo en proporción a la escala. Ver `render::scale`.
    let Some(world) = crate::render::scale::window_to_world_2d(camera, cam_tf, win, screen) else {
        return;
    };
    // NPC más cercano al punto tocado (en espacio de render = Transform.xy).
    let mut best: Option<(u64, f32)> = None;
    for (npc, tf) in &npcs {
        let d = tf.translation.truncate().distance(world);
        if d <= PICK_R && best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((npc.uid, d));
        }
    }
    if let Some((uid, _)) = best {
        menu.open_uid = Some(uid);
        menu.refresh = true;
        menu.companero = false;
    }
}

/// D6: cierra el panel si te alejás del NPC — antes sólo cerraba cuando la entidad dejaba
/// de existir, así que podías irte caminando y el panel quedaba abierto para siempre.
fn close_menu_by_distance(mut menu: ResMut<NpcMenu>, player: Query<&Transform, With<Player>>, npcs: Query<&Npc>) {
    let Some(uid) = menu.open_uid else {
        return;
    };
    let Ok(pt) = player.get_single() else {
        return;
    };
    let Some(npc) = npcs.iter().find(|n| n.uid == uid) else {
        return; // lo cierra `sync_menu` cuando note que ya no existe
    };
    let (px, pz) = (pt.translation.x, -pt.translation.y);
    let d2 = (npc.x - px) * (npc.x - px) + (npc.z - pz) * (npc.z - pz);
    if d2 > TALK_RANGE * TALK_RANGE {
        menu.open_uid = None;
    }
}

/// (Re)construye el panel cuando cambia el NPC abierto o se pide refresh.
#[allow(clippy::too_many_arguments)]
fn sync_menu(
    mut commands: Commands,
    mut menu: ResMut<NpcMenu>,
    root: Query<Entity, With<NpcMenuRoot>>,
    npcs: Query<&Npc>,
    soc: Res<Society>,
    religion: Res<Religion>,
    cg: Res<crate::chargen::Chargen>,
    mut jobs: ResMut<crate::chargen::ChargenJobs>,
    mut images: ResMut<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    assets: Res<AssetServer>,
    mut civ: ResMut<crate::civilizacion::Civ>,
) {
    let changed = menu.open_uid != menu.shown_uid || menu.refresh;
    if !changed {
        return;
    }
    menu.refresh = false;
    menu.shown_uid = menu.open_uid;
    for e in &root {
        commands.entity(e).despawn_recursive();
    }
    let Some(uid) = menu.open_uid else {
        return;
    };
    let Some(npc) = npcs.iter().find(|n| n.uid == uid) else {
        // El NPC ya no existe (se fue del cuerpo / murió) → cerrar.
        menu.open_uid = None;
        menu.shown_uid = None;
        return;
    };
    let rep = soc.rep(npc.family);
    let role = npc_role(npc.faith, npc.converted, rep);
    let job = crate::society::role_label(npc.role);
    let name = npc_name(uid, npc.family);
    // Qué está haciendo, de qué credo es y qué fama tiene: la vida interior a la vista.
    let estado = {
        let mut t = npc.vida.actividad().verbo(ha_shared::sims::Clase::Aldeano).to_string();
        if npc.vida.credo == ha_shared::credos::CREDO_KI {
            t.push_str("  |  fiel del Ki");
        } else if let Some(c) = soc.credos.get(npc.vida.credo) {
            t.push_str(&format!("  |  {} ({} miembros)", c.nombre, c.miembros));
        }
        if npc.vida.mala_fama > 0.5 {
            t.push_str("  |  malhechor conocido");
        } else if npc.vida.pers.moral > 0.6 {
            t.push_str("  |  buena persona");
        }
        match ha_shared::linaje::etapa(npc.age, npc.padre != 0 || npc.madre != 0) {
            ha_shared::linaje::Etapa::Nino => t.push_str("  |  nino"),
            ha_shared::linaje::Etapa::Anciano => t.push_str("  |  anciano"),
            ha_shared::linaje::Etapa::Adulto => {}
        }
        if npc.pareja == crate::society::JUGADOR_UID {
            t.push_str("  |  tu pareja");
        } else if npc.pareja != 0 {
            t.push_str(&format!("  |  pareja de {}", npc_name(npc.pareja, npc.family)));
        }
        if npc.padre == crate::society::JUGADOR_UID {
            t.push_str(&format!("\nhijo/a tuyo y de {}", npc_name(npc.madre, npc.family)));
        } else if npc.madre != 0 {
            t.push_str(&format!("\nhijo/a de {} y {}", npc_name(npc.madre, npc.family), npc_name(npc.padre, npc.family)));
        }
        if let Some(f) = civ.ficha_npc(uid, npc.x, npc.z) {
            t.push_str(&format!("\n{f}"));
        }
        t
    };
    let history: Vec<String> = npc.history.clone();
    let founded = religion.founded;
    let converted = npc.converted;
    // Se puede proponer pareja a un adulto soltero que te tiene aprecio y que no sea tu hijo/a.
    let puede_pareja = npc.pareja == 0
        && npc.padre != crate::society::JUGADOR_UID
        && ha_shared::linaje::etapa(npc.age, npc.padre != 0 || npc.madre != 0) == ha_shared::linaje::Etapa::Adulto
        && ha_shared::linaje::es_mujer(uid)
        && rep >= PAREJA_MIN_REP;

    // RETRATO — la misma cara que tiene su cuerpo en el mundo (el índice guarda
    // con qué capas se horneó cada hoja `npc/char_XX`), con la expresión que le
    // toca por cómo te trata y cómo está. Se compone una vez y queda cacheado.
    let expression =
        crate::chargen::Expression::for_npc(
            role,
            npc.vida.m.get(ha_shared::sims::Motivo::Hambre),
            npc.vida.m.get(ha_shared::sims::Motivo::Sueno),
        );
    let portrait = crate::chargen::portrait_for(
        cg.look_for_uid(uid),
        &cg,
        &mut jobs,
        &mut images,
        &assets,
    )
    .map(|handle| {
        let layout = menu
            .portrait_layout
            .get_or_insert_with(|| {
                layouts.add(TextureAtlasLayout::from_grid(
                    UVec2::new(cg.index.portrait_cell[0], cg.index.portrait_cell[1]),
                    cg.index.portrait_cols.max(1),
                    cg.index.portrait_rows.max(1),
                    None,
                    None,
                ))
            })
            .clone();
        ImageNode::from_atlas_image(handle, TextureAtlas { layout, index: expression.index() })
    });

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                // Debajo de la energía Y del reloj del día (`hud::spawn_day_clock`,
                // misma esquina): si sube más, los tapa.
                top: Val::Px(112.0),
                width: Val::Px(300.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(12.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor(NEON_CYAN),
            crate::ui_pack::MarcoDeHud(crate::ui_pack::marco::PANEL),
            GlobalZIndex(800),
            FocusPolicy::Block,
            Interaction::default(),
            NpcMenuRoot,
            Name::new("NpcMenu"),
        ))
        .with_children(|root| {
            // Cabecera: retrato a la izquierda, nombre/rol/oficio a la derecha.
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.0),
                ..default()
            })
            .with_children(|head| {
                if let Some(image) = portrait {
                    head.spawn((
                        image,
                        Node { width: Val::Px(64.0), height: Val::Px(64.0), flex_shrink: 0.0, ..default() },
                        Name::new("NpcPortrait"),
                    ));
                }
                head.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(2.0),
                    ..default()
                })
                .with_children(|col| {
                    col.spawn((
                        Text::new(name.clone()),
                        TextFont { font_size: 20.0, ..default() },
                        TextColor(crate::ui_pack::Pack::TINTA),
                    ));
                    col.spawn((
                        Text::new(format!("[{role}]  {job}")),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(crate::ui_pack::Pack::SUAVE),
                    ));
                    col.spawn((
                        Text::new(estado.clone()),
                        TextFont { font_size: 12.0, ..default() },
                        TextColor(Color::srgb(0.55, 0.33, 0.05)),
                    ));
                });
            });
            root.spawn((
                Text::new("últimos mensajes:"),
                TextFont { font_size: 12.0, ..default() },
                TextColor(crate::ui_pack::Pack::ETIQUETA),
            ));
            // Historial (viejo → nuevo). Vacío = todavía no dijo nada cerca tuyo.
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(3.0),
                    min_height: Val::Px(80.0),
                    ..default()
                },
                NpcMenuHistory,
            ))
            .with_children(|list| {
                if history.is_empty() {
                    list.spawn((
                        Text::new("(silencio... toca HABLAR)"),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(crate::ui_pack::Pack::SUAVE),
                    ));
                } else {
                    for line in &history {
                        list.spawn((
                            Text::new(format!("- {line}")),
                            TextFont { font_size: 14.0, ..default() },
                            TextColor(crate::ui_pack::Pack::TINTA),
                        ));
                    }
                }
            });
            // Botones según rol.
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: Val::Px(6.0),
                    row_gap: Val::Px(6.0),
                    margin: UiRect::top(Val::Px(6.0)),
                    ..default()
                },
                Name::new("NpcMenuButtons"),
            ))
            .with_children(|row| {
                npc_button(row, "HABLAR", NpcAction::Talk, NEON_CYAN);
                if role != "HOSTIL" {
                    npc_button(row, "PREGUNTAR", NpcAction::Preguntar, Color::srgb(0.75, 0.95, 1.0));
                    npc_button(row, "ENSEÑAME", NpcAction::Aprender, Color::srgb(0.7, 1.0, 0.75));
                    npc_button(row, "ENSEÑARLE", NpcAction::Ensenar, Color::srgb(0.85, 0.95, 0.6));
                }
                if role == "HOSTIL" {
                    npc_button(row, "CALMAR", NpcAction::Calm, Color::srgb(1.0, 0.7, 0.4));
                } else {
                    npc_button(row, "COMERCIAR", NpcAction::Trade, NEON_CYAN);
                    npc_button(row, "ENCARGO", NpcAction::Encargo, Color::srgb(0.7, 1.0, 0.75));
                    npc_button(row, "VENDER", NpcAction::Vender, Color::srgb(1.0, 0.85, 0.4));
                    npc_button(row, "COMPRAR", NpcAction::Comprar, Color::srgb(1.0, 0.85, 0.4));
                }
                // Sin culto fundado el botón decía nada y no había forma de
                // fundarlo hablando: sólo juntando gente encima tuyo. Ahora
                // PREDICAR es el camino, y el primer convencido funda el culto.
                if !converted {
                    let (label, action) = if founded {
                        ("CONVERTIR", NpcAction::Convert)
                    } else {
                        ("PREDICAR", NpcAction::Convert)
                    };
                    npc_button(row, label, action, Color::srgb(1.0, 0.86, 0.35));
                }
                if puede_pareja {
                    npc_button(row, "SER PAREJA", NpcAction::Pareja, Color::srgb(1.0, 0.6, 0.75));
                }
                npc_button(row, "CERRAR", NpcAction::Close, NEON_MAGENTA);
            });
        });
}

/// Amistad mínima (fama de su familia con vos) para que te enseñe una receta.
const APRENDER_MIN_REP: f32 = 0.22;
/// Cuánto sube la fama cada vez que le preguntás por lo suyo, y hasta dónde sólo charlando.
const PREGUNTAR_REP: f32 = 0.05;
const PREGUNTAR_TOPE: f32 = 0.35;
const PREGUNTAR_DESCANSO: f32 = 25.0;

/// PREGUNTAR / ENSEÑAME: lo que cada aldeano sabe viene del cuaderno de su pueblo (la
/// civilización), y lo que te enseña entra en TU cuaderno de experimentos.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn menu_saber(
    interactions: Query<(&Interaction, &NpcAction), Changed<Interaction>>,
    mut menu: ResMut<NpcMenu>,
    mut npcs: Query<(Entity, &mut Npc)>,
    mut soc: ResMut<Society>,
    mut speech: ResMut<SpeechQueue>,
    mut civ: ResMut<crate::civilizacion::Civ>,
    mut exp: ResMut<crate::experimentos::Experimentos>,
    memoria: Res<crate::barrio::Memoria>,
    eds: Res<crate::edificios::Edificios>,
    mut sfx_ui: EventWriter<crate::audio::PlaySfx>,
    time: Res<Time>,
    mut descanso: Local<std::collections::HashMap<u64, f32>>,
) {
    let Some(uid) = menu.open_uid else { return };
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed || !matches!(action, NpcAction::Preguntar | NpcAction::Aprender | NpcAction::Ensenar) {
            continue;
        }
        let Some((e, mut npc)) = npcs.iter_mut().find(|(_, n)| n.uid == uid) else { continue };
        let ahora = time.elapsed_secs();
        let rep = soc.rep(npc.family);
        let Some(saber) = civ.saber_de_npc(uid, npc.x, npc.z, false) else {
            say_from(&mut speech, e, npc.x, npc.z, "Yo? Yo sólo vivo acá.", Color::srgb(0.92, 0.96, 1.0), true);
            continue;
        };
        let amable = Color::srgb(0.75, 0.95, 1.0);
        let linea: String = match action {
            NpcAction::Preguntar => {
                let mut t = match saber.oficio {
                    ha_shared::civ::persona::Oficio::Erudito => format!("Estudio las cosas de {}: pruebo qué pasa al juntarlas con fuego.", saber.pueblo),
                    ha_shared::civ::persona::Oficio::Artesano => format!("Trabajo con las manos en {}; aprendí a transformar lo que hay.", saber.pueblo),
                    o => format!("Soy {} de {}.", o.etiqueta(), saber.pueblo),
                };
                if let Some(r) = saber.recetas.iter().max_by_key(|r| r.entrada.ok) {
                    t += &format!(" Lo último que me salió: {}.", r.resultado.nombre);
                    if r.entrada.ritual {
                        t += &format!(" Eso sí: sólo sale {}.", crate::saber_vivo::CIRCUNSTANCIAS[(r.entrada.ctx_creido as usize) % 8]);
                    }
                } else {
                    t += " Todavía no me salió nada que valga.";
                }
                if let Some(p) = npc.vivienda.puesto {
                    if let Some(i) = eds.en(0, 0, p.min.0 + p.w / 2, p.min.1 + p.h / 2) {
                        if eds.list[i].rubro > 0 {
                            let r = crate::edificios::RUBROS[(eds.list[i].rubro as usize - 1) % crate::edificios::RUBROS.len()].1;
                            t += &format!(" Atiendo la {r}.");
                        }
                    }
                }
                if let Some(n) = crate::barrio::noticias(&memoria.de(npc.town_x, npc.town_z)) {
                    t += &format!(" Del barrio: {n}");
                }
                if descanso.get(&uid).copied().unwrap_or(0.0) <= ahora {
                    descanso.insert(uid, ahora + PREGUNTAR_DESCANSO);
                    soc.adjust_capped(npc.family, PREGUNTAR_REP, PREGUNTAR_TOPE);
                }
                t
            }
            NpcAction::Ensenar => {
                let mut dicho = "Eso ya lo sabemos todos acá, o todavía no tenés nada firme que enseñar.".to_string();
                for (e, m) in exp.para_ensenar() {
                    if let Some(pueblo) = civ.ensenar_al_pueblo(uid, npc.x, npc.z, &e, &m) {
                        soc.adjust_capped(npc.family, 0.12, 1.0);
                        dicho = format!("¿{}? ¡Qué ingenioso! Se lo cuento a todo {pueblo}.", m.nombre);
                        break;
                    }
                }
                dicho
            }
            _ => {
                if rep < APRENDER_MIN_REP {
                    sfx_ui.send(crate::audio::PlaySfx(crate::audio::Sfx::Deny));
                    "No te conozco lo bastante como para enseñarte lo mío. Charlemos más.".to_string()
                } else if let Some(r) = saber.recetas.iter().find(|r| !exp.sabe(&r.entrada)) {
                    let aprendido = exp.aprender(&saber.nombre, r);
                    sfx_ui.send(crate::audio::PlaySfx(crate::audio::Sfx::Discover));
                    soc.adjust_capped(npc.family, 0.02, 1.0);
                    format!("Mirá: {aprendido}.{}", if r.entrada.ritual { " Ojo, yo lo hago siempre así; a lo mejor no hace falta." } else { "" })
                } else {
                    "Ya sabés todo lo que yo sé hacer.".to_string()
                }
            }
        };
        crate::causal::registrar("ensenanza", format!("{} le dijo: {}", saber.nombre, linea.chars().take(60).collect::<String>()));
        push_history(&mut npc.history, &linea);
        say_from(&mut speech, e, npc.x, npc.z, linea, amable, true);
        menu.refresh = true;
    }
}

fn npc_button(parent: &mut ChildBuilder, label: &str, action: NpcAction, color: Color) {
    parent
        .spawn((
            Button,
            Node {
                height: Val::Px(38.0),
                border: UiRect::all(Val::Px(2.0)),
                padding: UiRect::horizontal(Val::Px(12.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(color),
            BackgroundColor(BTN_IDLE),
            action,
        ))
        .with_children(|b| {
            b.spawn((Text::new(label), TextFont { font_size: 15.0, ..default() }, TextColor(color)));
        });
}

/// Acciones de los botones — dependen del rol del NPC.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn menu_buttons(
    interactions: Query<(&Interaction, &NpcAction), Changed<Interaction>>,
    mut menu: ResMut<NpcMenu>,
    mut npcs: Query<(Entity, &mut Npc)>,
    mut soc: ResMut<Society>,
    mut religion: ResMut<Religion>,
    mut speech: ResMut<SpeechQueue>,
    mut inv: ResMut<PlayerInventory>,
    selected: Res<crate::game_phase::SelectedCharacter>,
    mut encargos: ResMut<Encargos>,
    time: Res<Time>,
) {
    let Some(uid) = menu.open_uid else {
        return;
    };
    for (interaction, action) in &interactions {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            NpcAction::Close => {
                menu.open_uid = None;
                menu.refresh = true;
            }
            // Las resuelve `menu_saber` (necesita el mundo civil y el cuaderno).
            NpcAction::Preguntar | NpcAction::Aprender | NpcAction::Ensenar => {}
            NpcAction::Talk => {
                if let Some((e, mut npc)) = npcs.iter_mut().find(|(_, n)| n.uid == uid) {
                    let rep = soc.rep(npc.family);
                    let line = npc_line(
                        rep,
                        npc.converted,
                        npc.faith,
                        religion.founded,
                        &religion.name,
                        &selected.name,
                        npc.uid.wrapping_add(npc.history.len() as u64 + 1),
                    );
                    push_history(&mut npc.history, &line);
                    let col = if npc.converted {
                        Color::srgb(1.0, 0.9, 0.4)
                    } else if rep < 0.0 {
                        Color::srgb(1.0, 0.55, 0.45)
                    } else {
                        Color::srgb(0.92, 0.96, 1.0)
                    };
                    // E1/E2: lo que te dice A VOS gana prioridad y sigue al NPC.
                    say_from(&mut speech, e, npc.x, npc.z, line, col, true);
                    menu.refresh = true;
                }
            }
            NpcAction::Calm => {
                if let Some((e, mut npc)) = npcs.iter_mut().find(|(_, n)| n.uid == uid) {
                    let (x, z) = (npc.x, npc.z);
                    if npc.calm_cd > 0.0 {
                        say_from(&mut speech, e, x, z, "...", Color::srgb(0.7, 0.75, 0.8), true);
                    } else {
                        // C2: cooldown real + techo — hablando no se pasa de `CALM_CEILING`;
                        // para reconciliarse de verdad hace falta HACER algo, no spamear.
                        npc.calm_cd = CALM_COOLDOWN;
                        soc.adjust_capped(npc.family, CALM_STEP, CALM_CEILING);
                        say_from(&mut speech, e, x, z, "...está bien. Por ahora.", Color::srgb(1.0, 0.8, 0.5), true);
                    }
                    menu.refresh = true;
                }
            }
            NpcAction::Convert => {
                if let Some((e, mut npc)) = npcs.iter_mut().find(|(_, n)| n.uid == uid) {
                    let (x, z) = (npc.x, npc.z);
                    let rep = soc.rep(npc.family);
                    if npc.convert_cd > 0.0 {
                        say_from(&mut speech, e, x, z, "...", Color::srgb(0.7, 0.75, 0.8), true);
                    } else if rep <= CONVERT_MIN_REP {
                        // C3: no se convierte a quien te enfrenta — dos clicks ya no bastan
                        // para voltear a un HOSTIL.
                        say_from(&mut speech, e, x, z, "Jamas!", Color::srgb(1.0, 0.5, 0.45), true);
                    } else {
                        npc.convert_cd =
                            if religion.founded { CONVERT_COOLDOWN } else { FOUND_COOLDOWN };
                        npc.faith = (npc.faith + CONVERT_STEP).min(1.0);
                        if npc.faith >= CONVERT_AT && !npc.converted {
                            npc.converted = true;
                            // El PRIMER converso funda el culto. Antes la religión
                            // nacía de juntar un rebaño de aldeanos alrededor tuyo,
                            // que es justo la conducta que no queremos premiar.
                            let first = !religion.founded;
                            religion.found();
                            // D7: NO incrementar `religion.followers` acá — `tick_society`
                            // ya lo recalcula de cero cada tick contando a los conversos.
                            let line = if first {
                                format!("¡Creo en el Ki! Fundamos {}.", religion.name)
                            } else {
                                "¡Creo en el Ki!".to_string()
                            };
                            say_from(&mut speech, e, x, z, line, Color::srgb(1.0, 0.9, 0.4), true);
                        } else {
                            say_from(&mut speech, e, x, z, "Contame más...", Color::srgb(1.0, 0.86, 0.5), true);
                        }
                    }
                    menu.refresh = true;
                }
            }
            NpcAction::Pareja => {
                if let Some((e, mut npc)) = npcs.iter_mut().find(|(_, n)| n.uid == uid) {
                    let (x, z) = (npc.x, npc.z);
                    if npc.pareja == 0 {
                        npc.pareja = crate::society::JUGADOR_UID;
                        soc.stats.parejas += 1;
                        push_history(&mut npc.history, "Me quedo con vos.");
                        say_from(&mut speech, e, x, z, "Me quedo con vos.", Color::srgb(1.0, 0.7, 0.85), true);
                    }
                    menu.refresh = true;
                }
            }
            NpcAction::Vender | NpcAction::Comprar => {
                let Some((e, npc)) = npcs.iter().find(|(_, n)| n.uid == uid) else { continue };
                let (x, z) = (npc.x, npc.z);
                let en = crate::i18n::idioma_actual() == crate::i18n::Idioma::En;
                let color = Color::srgb(1.0, 0.9, 0.5);
                if matches!(action, NpcAction::Vender) {
                    // Se venden TODAS las unidades que completan al menos un lingote.
                    let mut oro = 0u32;
                    let mut quitar: Vec<(String, u32)> = Vec::new();
                    for (id, q) in inv.items.iter() {
                        if let Some(por) = unidades_por_oro(id) {
                            let lotes = q / por;
                            if lotes > 0 && id != "gem_blue" && id != "gem_red" || (por == 1 && *q > 1) {
                                let lotes = if por == 1 { q - 1 } else { lotes };
                                oro += lotes;
                                quitar.push((id.clone(), lotes * por));
                            }
                        }
                    }
                    if oro == 0 {
                        let f = if en { "You have nothing worth a gold ingot." } else { "No llevás nada que valga un lingote." };
                        say_from(&mut speech, e, x, z, f, color, true);
                    } else {
                        for (id, n) in quitar {
                            inv.remove(&id, n);
                        }
                        inv.add("gold_ingot", oro);
                        crate::causal::registrar("venta", format!("{oro} lingotes por mercadería"));
                        let f = if en { format!("Deal! {oro} gold ingots.") } else { format!("¡Trato hecho! {oro} lingotes de oro.") };
                        say_from(&mut speech, e, x, z, f, color, true);
                    }
                } else if inv.qty("gold_ingot") >= PRECIO_PRENDA {
                    inv.remove("gold_ingot", PRECIO_PRENDA);
                    let n = (sims_hash(uid ^ (time.elapsed().as_millis() as u64)) % 100_000) as u32;
                    let prenda = ha_shared::armadura::prenda_al_azar(n);
                    inv.add(prenda, 1);
                    crate::causal::registrar("compra", format!("prenda {prenda} por {PRECIO_PRENDA} lingotes"));
                    let nombre = crate::granja::nombre_item(prenda);
                    let f = if en { format!("Here: {}.", crate::i18n::tr(&nombre)) } else { format!("Tomá: {nombre}.") };
                    say_from(&mut speech, e, x, z, f, color, true);
                } else {
                    let f = if en { format!("A garment costs {PRECIO_PRENDA} gold ingots.") } else { format!("Una prenda sale {PRECIO_PRENDA} lingotes de oro.") };
                    say_from(&mut speech, e, x, z, f, color, true);
                }
            }
            NpcAction::Encargo => {
                if let Some((e, npc)) = npcs.iter().find(|(_, n)| n.uid == uid) {
                    let (x, z, familia) = (npc.x, npc.z, npc.family);
                    let en = crate::i18n::idioma_actual() == crate::i18n::Idioma::En;
                    let ahora = time.elapsed_secs();
                    let color = Color::srgb(0.7, 1.0, 0.75);
                    let rol = "";
                    if encargos.descanso.get(&uid).is_some_and(|t| *t > ahora) {
                        let f = if en { "Thanks, I'm all set for now." } else { "Gracias, por ahora estoy bien." };
                        say_from(&mut speech, e, x, z, f, color, true);
                        continue;
                    }
                    let (item, n) = *encargos.pendientes.entry(uid).or_insert_with(|| pedido_para(uid, rol));
                    let nombre = crate::i18n::tr(&crate::granja::nombre_item(item)).into_owned();
                    if inv.qty(item) >= n {
                        inv.remove(item, n);
                        let (pi, pn) = premio_para(uid);
                        inv.add(pi, pn);
                        soc.adjust(familia, ENCARGO_REP);
                        encargos.pendientes.remove(&uid);
                        crate::causal::registrar("encargo", format!("entregado a {uid}: {n} de {item}"));
                        encargos.descanso.insert(uid, ahora + ENCARGO_DESCANSO);
                        let f = if en {
                            format!("Perfect, thank you! Take {pn} {}.", crate::i18n::tr(&crate::granja::nombre_item(pi)))
                        } else {
                            format!("¡Perfecto, gracias! Tomá {pn} de {}.", crate::granja::nombre_item(pi))
                        };
                        say_from(&mut speech, e, x, z, f, color, true);
                    } else {
                        let tengo = inv.qty(item);
                        let f = if en {
                            format!("I need {n} of {nombre}. You have {tengo}.")
                        } else {
                            format!("Necesito {n} de {nombre}. Llevás {tengo}.")
                        };
                        say_from(&mut speech, e, x, z, f, color, true);
                    }
                }
            }
            NpcAction::Trade => {
                if let Some((e, npc)) = npcs.iter().find(|(_, n)| n.uid == uid) {
                    let (x, z) = (npc.x, npc.z);
                    let familia = npc.family;
                    // C4/S6: comercio real contra el `Stock` de la colonia — ya no es un
                    // stub fijo; el NPC ofrece lo que la colonia tiene de sobra.
                    let body = soc.active_body().unwrap_or(0);
                    if inv.qty("apple") < TRADE_APPLE_COST {
                        say_from(&mut speech, e, x, z, "Traeme manzanas y hablamos.", Color::srgb(0.9, 0.96, 1.0), true);
                    } else if let Some((item, amount)) = soc.try_trade(body) {
                        inv.remove("apple", TRADE_APPLE_COST);
                        inv.add(item, amount as u32);
                        // Cada trato hecho mejora la fama de su familia con vos: sin esto la fama sólo
                        // podía bajar y nadie llegaba jamás a aceptar ser tu pareja.
                        soc.adjust(familia, TRADE_REP_GAIN);
                        let label = if item == "wood" { "madera" } else { "piedra" };
                        say_from(
                            &mut speech,
                            e,
                            x,
                            z,
                            // Regla 6: la fuente no dibuja acentos ("Tené" salia con
                            // una caja en el medio).
                            format!("Tomá, {amount:.0} de {label}."),
                            Color::srgb(0.7, 1.0, 0.75),
                            true,
                        );
                    } else {
                        say_from(&mut speech, e, x, z, "No tengo nada para comerciar aún.", Color::srgb(0.9, 0.96, 1.0), true);
                    }
                }
            }
        }
    }
}

/// Un toque sobre el compañero abre SU menú (órdenes y charla). Mismo criterio que `pick_npc`:
/// el toque cuenta si no cae sobre el joystick ni sobre otro panel de UI.
#[allow(clippy::too_many_arguments)]
fn pick_companero(
    mouse: Res<ButtonInput<MouseButton>>,
    mut touches: EventReader<TouchInput>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cam: Query<(&Camera, &GlobalTransform), With<GameCamera>>,
    companion: Query<&Transform, With<Companion>>,
    over_menu: Query<&Interaction, With<NpcMenuRoot>>,
    touch_layout: Res<TouchLayout>,
    blocks: Res<TouchBlockRects>,
    mut menu: ResMut<NpcMenu>,
) {
    if over_menu.iter().any(|i| *i != Interaction::None) {
        return;
    }
    let screen = if mouse.just_pressed(MouseButton::Left) {
        windows.get_single().ok().and_then(|w| w.cursor_position())
    } else {
        touches
            .read()
            .find(|t| {
                t.phase == TouchPhase::Started
                    && !touch_in_move_zone(t.position, &touch_layout)
                    && !touch_blocked(t.position, &blocks.0)
            })
            .map(|t| t.position)
    };
    let (Some(screen), Ok((camera, cam_tf)), Ok(win), Ok(ctf)) =
        (screen, cam.get_single(), windows.get_single(), companion.get_single())
    else {
        return;
    };
    let Some(world) = crate::render::scale::window_to_world_2d(camera, cam_tf, win, screen) else {
        return;
    };
    if ctf.translation.truncate().distance(world) <= PICK_R && menu.open_uid.is_none() {
        menu.companero = true;
        menu.companero_shown = false;
    }
}

fn boton_companero() -> Node {
    Node {
        height: Val::Px(34.0),
        border: UiRect::all(Val::Px(2.0)),
        padding: UiRect::horizontal(Val::Px(8.0)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

/// El panel del compañero: estado, la orden vigente y los botones para darle otra.
#[allow(clippy::too_many_arguments)]
fn sync_menu_companero(
    mut commands: Commands,
    mut menu: ResMut<NpcMenu>,
    order: Res<CompanionOrder>,
    companion: Query<&Companion>,
    root: Query<Entity, With<NpcMenuRoot>>,
    mut last_order: Local<Option<CompanionTask>>,
) {
    if !menu.companero {
        if menu.companero_shown {
            menu.companero_shown = false;
            for e in &root {
                commands.entity(e).despawn_recursive();
            }
        }
        return;
    }
    // Abrió un vecino por encima: se va el del compañero.
    if menu.open_uid.is_some() {
        menu.companero = false;
        return;
    }
    let cambio = *last_order != Some(order.0);
    if menu.companero_shown && !cambio {
        return;
    }
    *last_order = Some(order.0);
    menu.companero_shown = true;
    for e in &root {
        commands.entity(e).despawn_recursive();
    }
    let Ok(c) = companion.get_single() else {
        menu.companero = false;
        return;
    };
    let estado = c.estado();
    let orden = match order.0 {
        CompanionTask::Follow => "te sigue",
        CompanionTask::Gather => "explora por la zona",
        CompanionTask::Pray => "reza cerca del ídolo",
    };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                // A la IZQUIERDA y compacto: del lado derecho tapaba SALTAR / COMER.
                left: Val::Px(12.0),
                top: Val::Px(120.0),
                width: Val::Px(390.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                padding: UiRect::all(Val::Px(8.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor(NEON_CYAN),
            crate::ui_pack::MarcoDeHud(crate::ui_pack::marco::PANEL),
            GlobalZIndex(800),
            FocusPolicy::Block,
            Interaction::default(),
            NpcMenuRoot,
            Name::new("CompaneroMenu"),
        ))
        .with_children(|root| {
            root.spawn((Text::new("Compañero"), TextFont { font_size: 20.0, ..default() }, TextColor(crate::ui_pack::Pack::TINTA)));
            root.spawn((
                Text::new(format!("{estado}  |  orden: {orden}")),
                TextFont { font_size: 13.0, ..default() },
                TextColor(Color::srgb(0.55, 0.33, 0.05)),
            ));
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(6.0),
                row_gap: Val::Px(6.0),
                margin: UiRect::top(Val::Px(6.0)),
                ..default()
            })
            .with_children(|row| {
                for (label, task) in [
                    ("SEGUIRME", CompanionTask::Follow),
                    ("EXPLORAR", CompanionTask::Gather),
                    ("REZAR", CompanionTask::Pray),
                ] {
                    let color = if order.0 == task { NEON_MAGENTA } else { NEON_CYAN };
                    row.spawn((Button, boton_companero(), BorderColor(color), BackgroundColor(BTN_IDLE), CompaneroAccion(task)))
                        .with_children(|b| {
                            b.spawn((Text::new(label), TextFont { font_size: 14.0, ..default() }, TextColor(color)));
                        });
                }
                row.spawn((Button, boton_companero(), BorderColor(NEON_CYAN), BackgroundColor(BTN_IDLE), CompaneroCharlar))
                    .with_children(|b| {
                        b.spawn((Text::new("CHARLAR"), TextFont { font_size: 14.0, ..default() }, TextColor(NEON_CYAN)));
                    });
                row.spawn((Button, boton_companero(), BorderColor(NEON_MAGENTA), BackgroundColor(BTN_IDLE), CompaneroCerrar))
                    .with_children(|b| {
                        b.spawn((Text::new("CERRAR"), TextFont { font_size: 14.0, ..default() }, TextColor(NEON_MAGENTA)));
                    });
            });
        });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn menu_companero(
    acciones: Query<(&Interaction, &CompaneroAccion), Changed<Interaction>>,
    charlar: Query<&Interaction, (With<CompaneroCharlar>, Changed<Interaction>)>,
    cerrar: Query<&Interaction, (With<CompaneroCerrar>, Changed<Interaction>)>,
    mut menu: ResMut<NpcMenu>,
    mut order: ResMut<CompanionOrder>,
    mut speech: ResMut<SpeechQueue>,
    companion: Query<(Entity, &Companion, &Transform)>,
    player: Query<&Transform, (With<Player>, Without<Companion>)>,
) {
    for (i, a) in &acciones {
        if *i == Interaction::Pressed {
            order.0 = a.0;
            if let Ok((e, c, _)) = companion.get_single() {
                let (x, z) = c.pos();
                let frase = match a.0 {
                    CompanionTask::Follow => "Voy con vos.",
                    CompanionTask::Gather => "Voy a mirar por ahí.",
                    CompanionTask::Pray => "Rezo un rato.",
                };
                say_from(&mut speech, e, x, z, frase, Color::srgb(0.7, 1.0, 0.9), true);
            }
        }
    }
    for i in &charlar {
        if *i == Interaction::Pressed {
            if let Ok((e, c, _)) = companion.get_single() {
                let (x, z) = c.pos();
                let frases = ["A tus órdenes.", "Qué lindo día para andar.", "Cuenta conmigo.", "Si te falta algo, avisame."];
                let k = (x.abs() * 7.0 + z.abs() * 3.0) as usize % frases.len();
                say_from(&mut speech, e, x, z, frases[k], Color::srgb(0.7, 1.0, 0.9), true);
            }
        }
    }
    for i in &cerrar {
        if *i == Interaction::Pressed {
            menu.companero = false;
        }
    }
    // Te alejaste: se cierra solo.
    if menu.companero {
        if let (Ok((_, _, ct)), Ok(pt)) = (companion.get_single(), player.get_single()) {
            if ct.translation.truncate().distance(pt.translation.truncate()) > TALK_RANGE * 2.0 {
                menu.companero = false;
            }
        }
    }
}

#[cfg(test)]
mod economia_tests {
    use super::*;

    /// §17 regla 1 (contabilidad conservada): no hay ciclo de dinero gratis. Lo que se COMPRA
    /// (prendas) no se puede VENDER, y todo lo vendible rinde menos de un lingote por unidad:
    /// vender y recomprar nunca deja ganancia.
    #[test]
    fn vender_y_recomprar_nunca_da_ganancia() {
        for id in ha_shared::armadura::hallables_ids() {
            assert!(unidades_por_oro(id).is_none(), "{id} se compra, no puede venderse");
        }
        for id in ["apple", "bread", "leather", "wood", "stone", "meat", "iron_ingot", "gem_blue"] {
            let por = unidades_por_oro(id).expect(id);
            assert!(por >= 1, "{id} rinde más de un lingote por unidad");
        }
        // El oro tampoco se vende por oro.
        assert!(unidades_por_oro("gold_ingot").is_none());
        // Una prenda cuesta más que lo que da vender una unidad de lo más barato.
        assert!(PRECIO_PRENDA >= 2);
    }
}
