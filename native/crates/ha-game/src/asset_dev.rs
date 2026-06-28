//! Debug-only bridge: in-game asset panel ↔ `tools/asset-viewer/dev/asset_overrides.json`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use bevy::prelude::*;
use ha_shared::ASSET_CATALOG;

use crate::assets::{AssetFactory, AssetSource};
use crate::game_ui::{apply_center, apply_top_center, GameUiRoot};
use crate::input::TouchLayout;
use crate::materials::ToonMaterial;
use crate::ui_editor::UiEditorState;
use crate::ui_theme::{self, UiButtonTone};

const VIEWER_PORT: u16 = 5199;
const PANEL_W: f32 = 148.0;
const PANEL_H: f32 = 248.0;

pub struct AssetDevPlugin;

impl Plugin for AssetDevPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AssetDevState>()
            .init_resource::<AssetDevPanels>()
            .add_systems(
                Update,
                (
                    poll_dev_overrides,
                    sync_asset_dev_panel,
                    asset_dev_buttons,
                    update_asset_dev_labels,
                ),
            );
    }
}

#[derive(Resource, Debug)]
pub struct AssetDevState {
    pub panel_open: bool,
    pub kind_index: usize,
    pub last_message: String,
    signal_mtime: Option<SystemTime>,
}

impl Default for AssetDevState {
    fn default() -> Self {
        Self {
            panel_open: false,
            kind_index: 0,
            last_message: String::new(),
            signal_mtime: None,
        }
    }
}

#[derive(Resource, Default)]
struct AssetDevPanels {
    panel: Option<Entity>,
}

#[derive(Component)]
struct AssetDevPanel;

#[derive(Component)]
struct AssetDevStatusLabel;

#[derive(Component, Clone, Copy)]
enum AssetDevCmd {
    Close,
    PrevKind,
    NextKind,
    OpenViewer,
    SpawnPreview,
    ClearOverride,
    Reload,
}

#[derive(Component)]
struct AssetDevPreview;

pub fn dev_overrides_path() -> Option<PathBuf> {
    if let Ok(repo) = std::env::var("HA_REPO_ROOT") {
        let p = PathBuf::from(repo).join("tools/asset-viewer/dev/asset_overrides.json");
        if p.is_file() {
            return p.canonicalize().ok();
        }
    }
    let rel = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tools/asset-viewer/dev/asset_overrides.json");
    rel.canonicalize().ok().filter(|p| p.is_file())
}

pub fn dev_signal_path() -> Option<PathBuf> {
    dev_overrides_path().map(|p| p.with_file_name(".override_signal"))
}

fn catalog_kinds() -> Vec<&'static str> {
    ASSET_CATALOG.iter().map(|a| a.kind).collect()
}

fn current_kind(state: &AssetDevState) -> &'static str {
    let kinds = catalog_kinds();
    kinds
        .get(state.kind_index % kinds.len().max(1))
        .copied()
        .unwrap_or("prop_crate")
}

fn open_viewer_for_kind(kind: &str) {
    let url = format!("http://127.0.0.1:{VIEWER_PORT}/?kind={kind}");
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("xdg-open").arg(&url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(&url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn();
    }
    eprintln!("ha-asset-dev: open viewer → {url}");
}

fn poll_dev_overrides(
    mut state: ResMut<AssetDevState>,
    mut factory: ResMut<AssetFactory>,
) {
    let Some(signal) = dev_signal_path() else {
        return;
    };
    let Ok(meta) = std::fs::metadata(&signal) else {
        return;
    };
    let Ok(mtime) = meta.modified() else {
        return;
    };
    if state.signal_mtime == Some(mtime) {
        return;
    }
    state.signal_mtime = Some(mtime);

    if let Some(paths) = read_dev_overrides() {
        let count = paths.len();
        factory.invalidate_all_dev(&paths);
        state.last_message = format!("Overrides recargados ({count} kinds)");
        eprintln!("ha-asset-dev: {}" , state.last_message);
    }
}

pub fn read_dev_overrides() -> Option<HashMap<String, PathBuf>> {
    let path = dev_overrides_path()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let raw: HashMap<String, String> = serde_json::from_str(&text).ok()?;
    let repo = std::env::var("HA_REPO_ROOT")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            path.parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
                .map(Path::to_path_buf)
        });

    let mut out = HashMap::new();
    for (kind, p) in raw {
        let resolved = if Path::new(&p).is_absolute() {
            PathBuf::from(&p)
        } else if let Some(ref root) = repo {
            root.join(&p)
        } else {
            continue;
        };
        if resolved.is_file() {
            out.insert(kind, resolved);
        } else {
            eprintln!(
                "ha-asset-dev: override '{kind}' path missing: {}",
                resolved.display()
            );
        }
    }
    Some(out)
}

fn sync_asset_dev_panel(
    mut commands: Commands,
    mut dev: ResMut<AssetDevState>,
    editor: Res<UiEditorState>,
    root: Query<Entity, With<GameUiRoot>>,
    mut panels: ResMut<AssetDevPanels>,
) {
    let need = dev.panel_open && editor.menu_open;
    if !need {
        if let Some(e) = panels.panel.take() {
            commands.entity(e).despawn_recursive();
        }
        return;
    }
    if panels.panel.is_some() {
        return;
    }
    let Ok(root) = root.get_single() else {
        return;
    };
    panels.panel = Some(spawn_asset_panel(&mut commands, root, &mut dev));
}

fn spawn_asset_panel(commands: &mut Commands, root: Entity, dev: &mut AssetDevState) -> Entity {
    let kind = current_kind(dev);
    dev.last_message = format!("Kind: {kind}");
    let mut panel_entity = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|parent| {
        panel_entity = parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(PANEL_W),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    padding: UiRect::all(Val::Px(6.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor(ui_theme::accent_ink()),
                BackgroundColor(ui_theme::panel_bg(0.9)),
                AssetDevPanel,
                Name::new("AssetDevPanel"),
            ))
            .with_children(|panel| {
                spawn_dev_btn(panel, "CLOSE", AssetDevCmd::Close);
                panel.spawn((
                    Text::new(format!("KIND: {kind}")),
                    TextFont {
                        font_size: 10.0,
                        ..default()
                    },
                    TextColor(ui_theme::accent_ink()),
                    AssetDevStatusLabel,
                ));
                spawn_dev_btn(panel, "◀ KIND", AssetDevCmd::PrevKind);
                spawn_dev_btn(panel, "KIND ▶", AssetDevCmd::NextKind);
                spawn_dev_btn(panel, "VIEWER", AssetDevCmd::OpenViewer);
                spawn_dev_btn(panel, "PREVIEW", AssetDevCmd::SpawnPreview);
                spawn_dev_btn(panel, "CLEAR", AssetDevCmd::ClearOverride);
                spawn_dev_btn(panel, "RELOAD", AssetDevCmd::Reload);
            })
            .id();
    });
    panel_entity
}

fn spawn_dev_btn(parent: &mut ChildBuilder, label: &str, cmd: AssetDevCmd) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Percent(100.0),
                height: Val::Px(24.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BorderColor(ui_theme::button_border(UiButtonTone::Primary)),
            BackgroundColor(ui_theme::button_bg(
                UiButtonTone::Primary,
                ui_theme::interaction_state(Interaction::None, false),
            )),
            cmd,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont {
                    font_size: 10.0,
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
            ));
        });
}

fn asset_dev_buttons(
    mut dev: ResMut<AssetDevState>,
    mut factory: ResMut<AssetFactory>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut toon: ResMut<Assets<ToonMaterial>>,
    mut std: ResMut<Assets<StandardMaterial>>,
    player: Query<&Transform, With<crate::sim::Player>>,
    previews: Query<Entity, With<AssetDevPreview>>,
    mut buttons: Query<
        (&Interaction, &AssetDevCmd, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>),
    >,
) {
    for (interaction, cmd, mut bg) in &mut buttons {
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Primary,
            ui_theme::interaction_state(*interaction, false),
        ));
        if *interaction != Interaction::Pressed {
            continue;
        }
        let kind = current_kind(&dev).to_string();
        match cmd {
            AssetDevCmd::Close => {
                dev.panel_open = false;
            }
            AssetDevCmd::PrevKind => {
                let n = catalog_kinds().len().max(1);
                dev.kind_index = (dev.kind_index + n - 1) % n;
                dev.last_message = format!("Kind: {}", current_kind(&dev));
            }
            AssetDevCmd::NextKind => {
                let n = catalog_kinds().len().max(1);
                dev.kind_index = (dev.kind_index + 1) % n;
                dev.last_message = format!("Kind: {}", current_kind(&dev));
            }
            AssetDevCmd::OpenViewer => {
                open_viewer_for_kind(&kind);
                dev.last_message = "Viewer abierto en navegador".into();
            }
            AssetDevCmd::SpawnPreview => {
                for e in &previews {
                    commands.entity(e).despawn_recursive();
                }
                factory.invalidate(&kind);
                let asset = factory.require(&kind, &mut meshes, &mut toon, &mut std);
                let src = match asset.source {
                    AssetSource::GlbFile => "GLB",
                    AssetSource::Procedural => "procedural",
                };
                let Ok(player_tf) = player.get_single() else {
                    dev.last_message = "Sin jugador para preview".into();
                    continue;
                };
                let forward = player_tf.forward();
                let pos = player_tf.translation + forward * 4.0 + Vec3::Y * 0.5;
                let mut entity = commands.spawn((
                    Mesh3d(asset.mesh),
                    Transform::from_translation(pos).with_rotation(player_tf.rotation),
                    AssetDevPreview,
                    Name::new(format!("AssetPreview:{kind}")),
                ));
                asset.material.insert_on(&mut entity);
                dev.last_message = format!("Preview {kind} ({src})");
            }
            AssetDevCmd::ClearOverride => {
                if clear_dev_override(&kind) {
                    factory.invalidate(&kind);
                    dev.last_message = format!("Override borrado: {kind}");
                } else {
                    dev.last_message = format!("Sin override para {kind}");
                }
            }
            AssetDevCmd::Reload => {
                factory.invalidate(&kind);
                if let Some(paths) = read_dev_overrides() {
                    factory.invalidate_all_dev(&paths);
                }
                dev.last_message = format!("Recargado: {kind}");
            }
        }
    }
}

fn clear_dev_override(kind: &str) -> bool {
    let Some(path) = dev_overrides_path() else {
        return false;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(mut map) = serde_json::from_str::<HashMap<String, String>>(&text) else {
        return false;
    };
    if map.remove(kind).is_none() {
        return false;
    }
    let body = serde_json::to_string_pretty(&map).unwrap_or_else(|_| "{}".into());
    let _ = std::fs::write(&path, format!("{body}\n"));
    if let Some(sig) = dev_signal_path() {
        let _ = std::fs::write(sig, format!("{}\n", std::time::SystemTime::now().elapsed().unwrap_or_default().as_millis()));
    }
    true
}

fn update_asset_dev_labels(
    dev: Res<AssetDevState>,
    mut labels: Query<&mut Text, With<AssetDevStatusLabel>>,
    mut panel: Query<&mut Node, With<AssetDevPanel>>,
    layout: Res<TouchLayout>,
) {
    for mut text in &mut labels {
        let kind = current_kind(&dev);
        let line = if dev.last_message.is_empty() {
            format!("KIND: {kind}")
        } else {
            format!("KIND: {kind}\n{}", dev.last_message)
        };
        if text.0 != line {
            text.0 = line;
        }
    }
    if let Ok(mut node) = panel.get_single_mut() {
        apply_center(&mut node, &layout, PANEL_W, PANEL_H);
    }
}
