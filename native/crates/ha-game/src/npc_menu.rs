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
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::ui::FocusPolicy;
use bevy::window::PrimaryWindow;

use crate::camera::GameCamera;
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

const NEON_CYAN: Color = Color::srgb(0.16, 0.94, 1.0);
const NEON_MAGENTA: Color = Color::srgb(1.0, 0.18, 0.62);
const PANEL_BG: Color = Color::srgb(0.05, 0.07, 0.12);
const BTN_IDLE: Color = Color::srgb(0.10, 0.14, 0.22);

pub struct NpcMenuPlugin;

impl Plugin for NpcMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NpcMenu>()
            .init_resource::<NpcShot>()
            .add_systems(
                Update,
                (pick_npc, close_menu_by_distance, npc_menu_autoshot, sync_menu, menu_buttons)
                    .chain(),
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
) {
    let Ok(path) = std::env::var("HA_NPCSHOT") else {
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
                    let line = npc_line(rep, npc.converted, npc.faith, religion.founded, &religion.name, npc.uid + k);
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
}

#[derive(Component)]
struct NpcMenuRoot;
#[derive(Component)]
struct NpcMenuHistory;

#[derive(Component, Clone, Copy)]
enum NpcAction {
    Talk,
    Convert,
    Calm,
    Trade,
    Close,
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
    let Ok(world) = camera.viewport_to_world_2d(cam_tf, screen) else {
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
    let history: Vec<String> = npc.history.clone();
    let founded = religion.founded;
    let converted = npc.converted;

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(12.0),
                // Debajo de la energía Y del reloj del día (`hud::spawn_day_clock`,
                // misma esquina): si sube más, los tapa.
                top: Val::Px(150.0),
                width: Val::Px(300.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                padding: UiRect::all(Val::Px(12.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor(NEON_CYAN),
            GlobalZIndex(800),
            FocusPolicy::Block,
            Interaction::default(),
            NpcMenuRoot,
            Name::new("NpcMenu"),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new(format!("{name}  [{role}]  {job}")),
                TextFont { font_size: 20.0, ..default() },
                TextColor(NEON_CYAN),
            ));
            root.spawn((
                Text::new("ultimos mensajes:"),
                TextFont { font_size: 12.0, ..default() },
                TextColor(Color::srgb(0.6, 0.65, 0.75)),
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
                        Text::new("(silencio... tocá HABLAR)"),
                        TextFont { font_size: 13.0, ..default() },
                        TextColor(Color::srgb(0.5, 0.55, 0.65)),
                    ));
                } else {
                    for line in &history {
                        list.spawn((
                            Text::new(format!("- {line}")),
                            TextFont { font_size: 14.0, ..default() },
                            TextColor(Color::srgb(0.9, 0.94, 1.0)),
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
                if role == "HOSTIL" {
                    npc_button(row, "CALMAR", NpcAction::Calm, Color::srgb(1.0, 0.7, 0.4));
                } else {
                    npc_button(row, "COMERCIAR", NpcAction::Trade, NEON_CYAN);
                }
                if founded && !converted {
                    npc_button(row, "CONVERTIR", NpcAction::Convert, Color::srgb(1.0, 0.86, 0.35));
                }
                npc_button(row, "CERRAR", NpcAction::Close, NEON_MAGENTA);
            });
        });
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
    religion: Res<Religion>,
    mut speech: ResMut<SpeechQueue>,
    mut inv: ResMut<PlayerInventory>,
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
            NpcAction::Talk => {
                if let Some((e, mut npc)) = npcs.iter_mut().find(|(_, n)| n.uid == uid) {
                    let rep = soc.rep(npc.family);
                    let line = npc_line(
                        rep,
                        npc.converted,
                        npc.faith,
                        religion.founded,
                        &religion.name,
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
                        say_from(&mut speech, e, x, z, "...esta bien. Por ahora.", Color::srgb(1.0, 0.8, 0.5), true);
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
                        npc.convert_cd = CONVERT_COOLDOWN;
                        npc.faith = (npc.faith + CONVERT_STEP).min(1.0);
                        if npc.faith >= CONVERT_AT && !npc.converted {
                            npc.converted = true;
                            // D7: NO incrementar `religion.followers` acá — `tick_society`
                            // ya lo recalcula de cero cada tick contando a los conversos.
                            say_from(&mut speech, e, x, z, "Creo en el Ki!", Color::srgb(1.0, 0.9, 0.4), true);
                        } else {
                            say_from(&mut speech, e, x, z, "Cuentame mas...", Color::srgb(1.0, 0.86, 0.5), true);
                        }
                    }
                    menu.refresh = true;
                }
            }
            NpcAction::Trade => {
                if let Some((e, npc)) = npcs.iter().find(|(_, n)| n.uid == uid) {
                    let (x, z) = (npc.x, npc.z);
                    // C4/S6: comercio real contra el `Stock` de la colonia — ya no es un
                    // stub fijo; el NPC ofrece lo que la colonia tiene de sobra.
                    let body = soc.active_body().unwrap_or(0);
                    if inv.qty("apple") < TRADE_APPLE_COST {
                        say_from(&mut speech, e, x, z, "Traeme manzanas y hablamos.", Color::srgb(0.9, 0.96, 1.0), true);
                    } else if let Some((item, amount)) = soc.try_trade(body) {
                        inv.remove("apple", TRADE_APPLE_COST);
                        inv.add(item, amount as u32);
                        let label = if item == "wood" { "madera" } else { "piedra" };
                        say_from(
                            &mut speech,
                            e,
                            x,
                            z,
                            format!("Tené, {amount:.0} de {label}."),
                            Color::srgb(0.7, 1.0, 0.75),
                            true,
                        );
                    } else {
                        say_from(&mut speech, e, x, z, "No tengo nada para comerciar aun.", Color::srgb(0.9, 0.96, 1.0), true);
                    }
                }
            }
        }
    }
}
