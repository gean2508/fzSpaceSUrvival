//! F7 — Cámara de vigilancia colocable. La cámara es una pieza que dejás en un PUNTO
//! (en persona, o mandada por sonda/cohete con antena) y desde donde estés te conectás para
//! ver EN VIVO ese punto: los seres vivos reales cerca de ahí (animales, NPCs), de forma
//! VISUAL (no telemetría). No muestra todo el planeta — sólo su área, como una cámara de
//! seguridad. Lee `sightings::WorldSightings` (posiciones reales que publican fauna/sociedad)
//! → no renderiza un segundo mundo. Ver `PLAN_CAMARA_SONDA.md`.
//!
//! NOTA (gameplay): la versión "pieza de nave + antena" (lanzar/orbitar) se apoya en este
//! mismo feed; acá se coloca en creativo/en-persona. La antena habilita la conexión.

use std::path::PathBuf;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use ha_shared::CelestialBody;

use crate::sightings::WorldSightings;

/// Radio (m) que abarca la cámara alrededor de su punto (como una cámara de seguridad).
const CAM_VIEW_R: f32 = 22.0;

pub struct CameraFeedPlugin;

impl Plugin for CameraFeedPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldSightings>()
            .init_resource::<CameraFeedState>()
            .init_resource::<PlacedCameras>()
            .add_event::<PlaceCameraEvent>()
            .add_systems(Startup, spawn_camera_ui)
            .add_systems(
                Update,
                (
                    load_cameras_once,
                    place_cameras,
                    auto_cameras_from_probes,
                    camera_feed_buttons,
                    sync_camera_feed.after(camera_feed_buttons),
                    rebuild_feed_dots.after(sync_camera_feed),
                    move_feed_dots,
                ),
            );
    }
}

/// Cámara colocada en un punto del mundo.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct PlacedCamera {
    body: u8,
    x: f32,
    z: f32,
}

#[derive(Resource, Default)]
struct PlacedCameras {
    cams: Vec<PlacedCamera>,
    loaded: bool,
}

#[derive(Serialize, Deserialize, Default)]
struct CamerasSave {
    cams: Vec<PlacedCamera>,
}

/// Pedido de colocar una cámara (lo emite `build.rs` desde la paleta creativa).
#[derive(Event)]
pub struct PlaceCameraEvent {
    pub body: u8,
    pub x: f32,
    pub z: f32,
}

#[derive(Resource, Default)]
struct CameraFeedState {
    open: bool,
    sel: usize,
    shown: Option<usize>,
    rebuild_acc: f32,
}

#[derive(Component)]
struct CameraToggleButton;
#[derive(Component)]
struct CameraToggleLabel;
#[derive(Component)]
struct CameraFeedRoot;
#[derive(Component)]
struct CameraFeedViewport;
#[derive(Component)]
struct CameraFeedTitle;
#[derive(Component)]
struct CameraNextButton;
#[derive(Component)]
struct CameraCloseButton;

#[derive(Component)]
struct FeedDot {
    bx: f32, // base viewport pos [0,1]
    by: f32,
    phase: f32,
}

fn cameras_path() -> PathBuf {
    crate::inventory_persist::persist_dir().join("cameras.json")
}

fn load_cameras_once(mut cams: ResMut<PlacedCameras>) {
    if cams.loaded {
        return;
    }
    cams.loaded = true;
    if let Ok(raw) = std::fs::read_to_string(cameras_path()) {
        if let Ok(save) = serde_json::from_str::<CamerasSave>(&raw) {
            cams.cams = save.cams;
        }
    }
}

fn save_cameras(cams: &PlacedCameras) {
    let save = CamerasSave { cams: cams.cams.clone() };
    if let Ok(json) = serde_json::to_string(&save) {
        let _ = std::fs::write(cameras_path(), json);
    }
}

/// F7.2b — cámara remota "de sonda": cada cuerpo MAPEADO con sonda (`NavData.mapped`, la
/// antena implícita) obtiene una cámara automática ubicada donde haya vida (primer
/// avistamiento) → podés ver un planeta remoto sin ir en persona. Sólo agrega una vez.
fn auto_cameras_from_probes(
    nav: Res<crate::nav_computer::NavComputer>,
    sightings: Res<WorldSightings>,
    mut cams: ResMut<PlacedCameras>,
) {
    if !cams.loaded {
        return;
    }
    for body in &nav.0.mapped {
        if *body == CelestialBody::Earth {
            continue;
        }
        let b = *body as u8;
        if cams.cams.iter().any(|c| c.body == b) {
            continue;
        }
        let pos = sightings
            .fauna
            .get(&b)
            .and_then(|v| v.first())
            .or_else(|| sightings.npc.get(&b).and_then(|v| v.first()))
            .map(|s| (s.x, s.z));
        if let Some((x, z)) = pos {
            cams.cams.push(PlacedCamera { body: b, x, z });
            save_cameras(&cams);
        }
    }
}

fn place_cameras(mut cams: ResMut<PlacedCameras>, mut ev: EventReader<PlaceCameraEvent>) {
    let mut changed = false;
    for e in ev.read() {
        cams.cams.push(PlacedCamera { body: e.body, x: e.x, z: e.z });
        changed = true;
    }
    if changed {
        save_cameras(&cams);
    }
}

fn spawn_camera_ui(mut commands: Commands) {
    // Botón "CAM" — arriba-izquierda (zona segura de toque).
    commands
        .spawn((
            Button,
            CameraToggleButton,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(52.0),
                left: Val::Px(8.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BorderColor(Color::srgba(0.5, 0.8, 1.0, 0.8)),
            BackgroundColor(Color::srgba(0.05, 0.10, 0.16, 0.85)),
            crate::game_ui::GameplayUi,
        ))
        .with_children(|p| {
            p.spawn((
                CameraToggleLabel,
                Text::new("CAM"),
                TextFont { font_size: 14.0, ..default() },
                TextColor(Color::srgba(0.6, 0.85, 1.0, 0.95)),
            ));
        });

    // Ventana del feed (oculta).
    commands
        .spawn((
            CameraFeedRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Percent(9.0),
                left: Val::Percent(7.0),
                width: Val::Percent(86.0),
                height: Val::Percent(76.0),
                flex_direction: FlexDirection::Column,
                border: UiRect::all(Val::Px(3.0)),
                padding: UiRect::all(Val::Px(6.0)),
                ..default()
            },
            BorderColor(Color::srgba(0.4, 0.75, 1.0, 0.9)),
            BackgroundColor(Color::srgba(0.02, 0.05, 0.09, 0.95)),
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((
                CameraFeedTitle,
                Text::new("CAMARA"),
                TextFont { font_size: 15.0, ..default() },
                TextColor(Color::srgba(0.7, 0.9, 1.0, 0.95)),
            ));
            root.spawn((
                CameraFeedViewport,
                Node {
                    position_type: PositionType::Relative,
                    width: Val::Percent(100.0),
                    flex_grow: 1.0,
                    margin: UiRect::vertical(Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BorderColor(Color::srgba(0.3, 0.5, 0.7, 0.6)),
                BackgroundColor(Color::srgba(0.04, 0.07, 0.05, 0.92)),
            ));
            root.spawn((Node {
                width: Val::Percent(100.0),
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },))
                .with_children(|bar| {
                    bar_button(bar, "Siguiente camara", CameraNextButton);
                    bar_button(bar, "Cerrar", CameraCloseButton);
                });
        });
}

fn bar_button<M: Component>(parent: &mut ChildBuilder, label: &str, marker: M) {
    parent
        .spawn((
            Button,
            marker,
            Node {
                padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BorderColor(Color::srgba(0.4, 0.7, 0.9, 0.8)),
            BackgroundColor(Color::srgba(0.08, 0.14, 0.20, 0.9)),
        ))
        .with_children(|p| {
            p.spawn((
                Text::new(label.to_string()),
                TextFont { font_size: 13.0, ..default() },
                TextColor(Color::srgba(0.75, 0.9, 1.0, 0.95)),
            ));
        });
}

#[allow(clippy::type_complexity)]
fn camera_feed_buttons(
    cams: Res<PlacedCameras>,
    mut state: ResMut<CameraFeedState>,
    toggle: Query<&Interaction, (Changed<Interaction>, With<CameraToggleButton>)>,
    next: Query<&Interaction, (Changed<Interaction>, With<CameraNextButton>)>,
    close: Query<&Interaction, (Changed<Interaction>, With<CameraCloseButton>)>,
) {
    for i in &toggle {
        if *i == Interaction::Pressed {
            state.open = !state.open;
        }
    }
    for i in &next {
        if *i == Interaction::Pressed {
            let n = cams.cams.len().max(1);
            state.sel = (state.sel + 1) % n;
        }
    }
    for i in &close {
        if *i == Interaction::Pressed {
            state.open = false;
        }
    }
}

#[allow(clippy::type_complexity)]
fn sync_camera_feed(
    cams: Res<PlacedCameras>,
    mut state: ResMut<CameraFeedState>,
    mut root: Query<&mut Visibility, With<CameraFeedRoot>>,
    mut title: Query<&mut Text, With<CameraFeedTitle>>,
    mut toggle_label: Query<&mut Text, (With<CameraToggleLabel>, Without<CameraFeedTitle>)>,
) {
    if let Ok(mut lbl) = toggle_label.get_single_mut() {
        let want = format!("CAM ({})", cams.cams.len());
        if lbl.0 != want {
            lbl.0 = want;
        }
    }
    let show = state.open && !cams.cams.is_empty();
    if let Ok(mut vis) = root.get_single_mut() {
        let want = if show { Visibility::Visible } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
    if !show {
        state.shown = None;
        return;
    }
    if state.sel >= cams.cams.len() {
        state.sel = 0;
    }
    if let Ok(mut t) = title.get_single_mut() {
        let c = cams.cams[state.sel];
        let body = CelestialBody::all()
            .iter()
            .copied()
            .find(|b| *b as u8 == c.body)
            .unwrap_or(CelestialBody::Earth);
        let want = format!("CAMARA {}/{}  -  {body:?} ({:.0},{:.0})", state.sel + 1, cams.cams.len(), c.x, c.z);
        if t.0 != want {
            t.0 = want;
        }
    }
}

/// Reconstruye los puntos del feed desde los avistamientos reales cerca de la cámara
/// (throttle ~1 s: refleja las posiciones actualizadas por fauna/sociedad).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn rebuild_feed_dots(
    time: Res<Time>,
    mut state: ResMut<CameraFeedState>,
    cams: Res<PlacedCameras>,
    sightings: Res<WorldSightings>,
    mut commands: Commands,
    viewport: Query<Entity, With<CameraFeedViewport>>,
    dots: Query<Entity, With<FeedDot>>,
) {
    if !state.open || cams.cams.is_empty() {
        return;
    }
    state.rebuild_acc += time.delta_secs();
    let changed_cam = state.shown != Some(state.sel);
    if !changed_cam && state.rebuild_acc < 1.0 {
        return;
    }
    state.rebuild_acc = 0.0;
    state.shown = Some(state.sel);
    let cam = cams.cams[state.sel];
    let near = sightings.near(cam.body, cam.x, cam.z, CAM_VIEW_R);
    let Ok(vp) = viewport.get_single() else {
        return;
    };
    for e in &dots {
        commands.entity(e).despawn_recursive();
    }
    commands.entity(vp).with_children(|p| {
        for (i, s) in near.iter().enumerate() {
            // Offset relativo a la cámara → posición en el viewport [0,1].
            let bx = ((s.x - cam.x) / CAM_VIEW_R * 0.5 + 0.5).clamp(0.02, 0.98);
            let by = ((s.z - cam.z) / CAM_VIEW_R * 0.5 + 0.5).clamp(0.02, 0.98);
            let px = if s.hostile { 13.0 } else { 9.0 };
            let c = if s.hostile {
                Color::srgb((s.color[0] + 0.3).min(1.0), s.color[1] * 0.6, s.color[2] * 0.6)
            } else {
                Color::srgb(s.color[0], s.color[1], s.color[2])
            };
            p.spawn((
                FeedDot { bx, by, phase: i as f32 * 1.7 },
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(bx * 100.0),
                    top: Val::Percent(by * 100.0),
                    width: Val::Px(px),
                    height: Val::Px(px),
                    ..default()
                },
                BackgroundColor(c),
                BorderRadius::all(Val::Percent(50.0)),
            ));
        }
    });
}

/// Movimiento leve de los puntos (respiración/idle) para que el feed se vea vivo entre
/// actualizaciones — no altera la posición real.
fn move_feed_dots(time: Res<Time>, state: Res<CameraFeedState>, mut q: Query<(&FeedDot, &mut Node)>) {
    if !state.open {
        return;
    }
    let t = time.elapsed_secs();
    for (d, mut node) in &mut q {
        let ox = (t * 0.8 + d.phase).sin() * 0.01;
        let oy = (t * 0.7 + d.phase * 1.3).cos() * 0.01;
        node.left = Val::Percent(((d.bx + ox) * 100.0).clamp(1.0, 99.0));
        node.top = Val::Percent(((d.by + oy) * 100.0).clamp(1.0, 99.0));
    }
}
