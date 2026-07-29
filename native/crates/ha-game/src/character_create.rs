//! Pantalla **CREA TU PERSONAJE** — se muestra en `GamePhase::CharacterCreate`,
//! entre "Nueva partida" y el gameplay. El mundo todavía NO existe (no hay Player
//! → no hay worldgen); recién al tocar JUGAR pasamos a `Playing` y `scene::spawn_player`
//! usa el `SelectedCharacter` que se arma acá.
//!
//! v1 del "generador de personaje": nombre (teclado), skin (spritesheets bajo
//! `art/player/`) y tinte de ropa. La composición por CAPAS de LimeZu (cuerpo/ojos/
//! pelo/ropa/accesorios) se arma con el Image Lab del editor de assets y aparece acá
//! como un skin más — ver memoria `imglab-multipng-editor`. Texto ASCII (la fuente
//! del juego no dibuja acentos/flechas).

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;

use crate::game_phase::{save_savegame, GamePhase, SelectedCharacter};
use crate::scene::{CHAR_COLS, CHAR_ROWS};

const NEON_CYAN: Color = Color::srgb(0.16, 0.94, 1.0);
const NEON_MAGENTA: Color = Color::srgb(1.0, 0.18, 0.62);
const VOID: Color = Color::srgb(0.04, 0.05, 0.09);
const NAME_MAX: usize = 16;

/// Tintes de ropa/piel elegibles (label ASCII + RGBA).
const TINTS: [(&str, [f32; 4]); 6] = [
    ("NORMAL", [1.0, 1.0, 1.0, 1.0]),
    ("ROJO", [1.0, 0.55, 0.55, 1.0]),
    ("VERDE", [0.55, 1.0, 0.6, 1.0]),
    ("AZUL", [0.55, 0.72, 1.0, 1.0]),
    ("DORADO", [1.0, 0.85, 0.4, 1.0]),
    ("VIOLETA", [0.8, 0.55, 1.0, 1.0]),
];

pub struct CharacterCreatePlugin;

impl Plugin for CharacterCreatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CcState>()
            .add_systems(OnEnter(GamePhase::CharacterCreate), spawn_screen)
            .add_systems(OnExit(GamePhase::CharacterCreate), despawn_screen)
            .add_systems(
                Update,
                (name_input, button_system).run_if(in_state(GamePhase::CharacterCreate)),
            );
    }
}

#[derive(Resource, Default)]
struct CcState {
    tint_idx: usize,
    skin_idx: usize,
    /// (label ASCII, ruta relativa en assets) de los skins disponibles.
    skins: Vec<(String, String)>,
    atlas: Option<Handle<TextureAtlasLayout>>,
}

#[derive(Component)]
struct CcRoot;
#[derive(Component)]
struct NameText;
#[derive(Component)]
struct TintLabel;
#[derive(Component)]
struct SkinLabel;
#[derive(Component)]
struct PreviewSprite;

#[derive(Component, Clone, Copy)]
enum CcAction {
    Play,
    Back,
    TintPrev,
    TintNext,
    SkinPrev,
    SkinNext,
}

/// Descubre los spritesheets de personaje disponibles (mismo layout 16×32). En
/// desktop lee `art/player/`; siempre incluye el default primero.
fn discover_skins() -> Vec<(String, String)> {
    let mut out = vec![("DEFAULT".to_string(), "art/player/character.png".to_string())];
    // Las 20 hojas horneadas por el **Character Generator** por capas
    // (`ha-asset-tools chargen`): mismo layout 16×32 que el default, y van
    // DENTRO del APK. Antes esta lista sólo se llenaba escaneando el disco con
    // `read_dir`, que en Android no ve los assets empaquetados → en el teléfono
    // había un solo skin ("DEFAULT") y el selector parecía roto.
    for i in 1..=20u32 {
        out.push((format!("PERSONAJE {i:02}"), format!("art/npc/char_{i:02}.png")));
    }
    #[cfg(not(target_os = "android"))]
    {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/art/player");
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("png") {
                    let file = p.file_name().unwrap().to_string_lossy().to_string();
                    if file == "character.png" {
                        continue; // ya está como DEFAULT
                    }
                    let label = p.file_stem().unwrap().to_string_lossy().to_uppercase();
                    out.push((label, format!("art/player/{file}")));
                }
            }
        }
    }
    out
}

fn spawn_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut cc: ResMut<CcState>,
    selected: Res<SelectedCharacter>,
    touch: Res<crate::input::TouchLayout>,
) {
    cc.skins = discover_skins();
    cc.skin_idx = cc.skins.iter().position(|(_, p)| *p == selected.sheet).unwrap_or(0);
    cc.tint_idx = TINTS
        .iter()
        .position(|(_, t)| *t == selected.tint)
        .unwrap_or(0);
    let layout = layouts.add(TextureAtlasLayout::from_grid(
        UVec2::new(16, 32),
        CHAR_COLS,
        CHAR_ROWS,
        None,
        None,
    ));
    cc.atlas = Some(layout.clone());

    let sheet = cc.skins.get(cc.skin_idx).map(|(_, p)| p.clone()).unwrap_or(selected.sheet.clone());
    let tint = TINTS[cc.tint_idx].1;

    // ── Escalado responsive ────────────────────────────────────────────────
    // El teléfono puede tener un alto LÓGICO chico (~400 px con scale factor
    // alto): con px fijos la columna medía más que la pantalla y los botones
    // JUGAR/VOLVER quedaban fuera de cuadro → te trababas acá. Ahora todo escala
    // con el alto real y la raíz es [contenido flexible] + [barra de botones],
    // dos hijos normales: la barra NO puede recortarse ni irse con el scroll
    // (antes era un hijo absoluto DENTRO del contenedor con overflow scroll).
    let vh = touch.height.max(240.0);
    let s = (vh / 720.0).clamp(0.45, 1.15); // factor de escala general
    let bar_h = (78.0 * s).clamp(46.0, 88.0);
    let preview_h = (vh * 0.24).clamp(64.0, 200.0);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                align_items: AlignItems::Stretch,
                ..default()
            },
            BackgroundColor(VOID),
            GlobalZIndex(1000),
            bevy::ui::FocusPolicy::Block,
            CcRoot,
            Name::new("CharacterCreate"),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: Val::Percent(100.0),
                    // Toma el alto sobrante; la barra de abajo siempre conserva
                    // el suyo. min_height 0 = puede encogerse y scrollear.
                    flex_grow: 1.0,
                    flex_shrink: 1.0,
                    flex_basis: Val::Px(0.0),
                    min_height: Val::Px(0.0),
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::FlexStart,
                    align_items: AlignItems::Center,
                    row_gap: Val::Px(8.0 * s),
                    padding: UiRect {
                        top: Val::Px(touch.insets.top as f32 + 10.0 * s),
                        bottom: Val::Px(8.0 * s),
                        left: Val::Px(touch.insets.left as f32 + 12.0),
                        right: Val::Px(touch.insets.right as f32 + 12.0),
                    },
                    // Si aun así no entra (pantalla muy baja), se desplaza en
                    // vez de recortarse.
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                Name::new("CcContent"),
            ))
            .with_children(|root| {
            root.spawn((
                Text::new("CREA TU PERSONAJE"),
                TextFont { font_size: (26.0 * s).clamp(15.0, 34.0), ..default() },
                TextColor(NEON_CYAN),
            ));

            // Preview del personaje (frame idle-down = 0). El frame es 16×32
            // (1:2) → el ancho es la mitad del alto o sale achatado.
            root.spawn((
                ImageNode::from_atlas_image(
                    asset_server.load(sheet),
                    TextureAtlas { layout, index: 0 },
                )
                .with_color(Color::srgba(tint[0], tint[1], tint[2], tint[3])),
                Node {
                    width: Val::Px(preview_h * 0.5),
                    height: Val::Px(preview_h),
                    ..default()
                },
                PreviewSprite,
            ));

            // Fila SKIN: < LABEL >
            selector_row(root, "SKIN", CcAction::SkinPrev, CcAction::SkinNext, s, |r| {
                r.spawn((
                    Text::new(cc.skins.get(cc.skin_idx).map(|(l, _)| l.clone()).unwrap_or_default()),
                    TextFont { font_size: (18.0 * s).clamp(12.0, 20.0), ..default() },
                    TextColor(NEON_MAGENTA),
                    SkinLabel,
                ));
            });

            // Fila TINTE: < LABEL >
            selector_row(root, "TINTE", CcAction::TintPrev, CcAction::TintNext, s, |r| {
                r.spawn((
                    Text::new(TINTS[cc.tint_idx].0.to_string()),
                    TextFont { font_size: (18.0 * s).clamp(12.0, 20.0), ..default() },
                    TextColor(NEON_MAGENTA),
                    TintLabel,
                ));
            });

            // Nombre (editable con teclado en desktop).
            root.spawn((
                Text::new("NOMBRE (escribi):"),
                TextFont { font_size: (14.0 * s).clamp(10.0, 16.0), ..default() },
                TextColor(Color::srgb(0.7, 0.75, 0.85)),
            ));
            root.spawn((
                Node {
                    min_width: Val::Px((240.0 * s).clamp(160.0, 260.0)),
                    height: Val::Px((40.0 * s).clamp(30.0, 44.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    padding: UiRect::horizontal(Val::Px(10.0)),
                    ..default()
                },
                BorderColor(NEON_CYAN),
                BackgroundColor(Color::srgb(0.06, 0.08, 0.12)),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new(selected.name.clone()),
                    TextFont { font_size: (22.0 * s).clamp(14.0, 24.0), ..default() },
                    TextColor(Color::WHITE),
                    NameText,
                ));
            });
            }); // fin del contenido scrolleable

            // Barra de acción al pie: hermana del contenido (no hija), con alto
            // propio → JUGAR/VOLVER siempre visibles y a un toque.
            root.spawn((
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(bar_h + touch.insets.bottom as f32),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(12.0),
                    padding: UiRect::bottom(Val::Px(touch.insets.bottom as f32)),
                    border: UiRect::top(Val::Px(2.0)),
                    ..default()
                },
                BorderColor(Color::srgba(0.16, 0.94, 1.0, 0.35)),
                BackgroundColor(Color::srgb(0.03, 0.04, 0.08)),
                Name::new("CcButtons"),
            ))
            .with_children(|row| {
                // Anchos relativos: en pantallas angostas no se salen.
                let w_play = (200.0 * s).clamp(120.0, 220.0);
                let w_back = (130.0 * s).clamp(90.0, 150.0);
                action_button(row, "> JUGAR", CcAction::Play, NEON_CYAN, w_play, s);
                action_button(row, "VOLVER", CcAction::Back, Color::srgb(0.6, 0.6, 0.7), w_back, s);
            });
        });
}

fn selector_row(
    parent: &mut ChildBuilder,
    label: &str,
    prev: CcAction,
    next: CcAction,
    s: f32,
    mid: impl FnOnce(&mut ChildBuilder),
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: Val::Px(10.0 * s),
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                Text::new(format!("{label}:")),
                TextFont { font_size: (16.0 * s).clamp(11.0, 18.0), ..default() },
                TextColor(Color::srgb(0.7, 0.75, 0.85)),
            ));
            small_button(row, "<", prev, s);
            mid(row);
            small_button(row, ">", next, s);
        });
}

fn small_button(parent: &mut ChildBuilder, label: &str, action: CcAction, s: f32) {
    let d = (38.0 * s).clamp(30.0, 42.0);
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(d),
                height: Val::Px(d),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(NEON_CYAN),
            BackgroundColor(Color::srgb(0.10, 0.14, 0.22)),
            action,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font_size: (22.0 * s).clamp(14.0, 24.0), ..default() },
                TextColor(NEON_CYAN),
            ));
        });
}

fn action_button(parent: &mut ChildBuilder, label: &str, action: CcAction, color: Color, w: f32, s: f32) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(w),
                height: Val::Px((56.0 * s).clamp(38.0, 60.0)),
                border: UiRect::all(Val::Px(3.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor(color),
            BackgroundColor(Color::srgb(0.10, 0.14, 0.22)),
            action,
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font_size: (24.0 * s).clamp(15.0, 26.0), ..default() },
                TextColor(color),
            ));
        });
}

fn despawn_screen(mut commands: Commands, q: Query<Entity, With<CcRoot>>) {
    for e in &q {
        commands.entity(e).despawn_recursive();
    }
}

/// Escribe el nombre con el teclado (desktop). Enter = JUGAR.
fn name_input(
    mut events: EventReader<KeyboardInput>,
    mut selected: ResMut<SelectedCharacter>,
    mut text: Query<&mut Text, With<NameText>>,
    mut next: ResMut<NextState<GamePhase>>,
) {
    let Ok(mut label) = text.get_single_mut() else {
        return;
    };
    let mut changed = false;
    for ev in events.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        match &ev.logical_key {
            Key::Backspace => {
                selected.name.pop();
                changed = true;
            }
            Key::Space => {
                if selected.name.len() < NAME_MAX {
                    selected.name.push(' ');
                    changed = true;
                }
            }
            Key::Enter => {
                start_game(&selected);
                next.set(GamePhase::Playing);
                return;
            }
            Key::Character(s) => {
                for c in s.chars() {
                    if (c.is_ascii_alphanumeric() || c == '_' || c == '-') && selected.name.len() < NAME_MAX {
                        selected.name.push(c);
                        changed = true;
                    }
                }
            }
            _ => {}
        }
    }
    if changed {
        label.0 = selected.name.clone();
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn button_system(
    mut interactions: Query<(&Interaction, &CcAction, &mut BackgroundColor), Changed<Interaction>>,
    mut cc: ResMut<CcState>,
    mut selected: ResMut<SelectedCharacter>,
    mut next: ResMut<NextState<GamePhase>>,
    asset_server: Res<AssetServer>,
    mut sets: ParamSet<(
        Query<&mut ImageNode, With<PreviewSprite>>,
        Query<&mut Text, With<TintLabel>>,
        Query<&mut Text, With<SkinLabel>>,
    )>,
) {
    let mut dirty_tint = false;
    let mut dirty_skin = false;
    for (interaction, action, mut bg) in &mut interactions {
        match interaction {
            Interaction::Pressed => {
                *bg = BackgroundColor(Color::srgb(0.16, 0.24, 0.36));
                match action {
                    CcAction::Play => {
                        apply_selection(&cc, &mut selected);
                        start_game(&selected);
                        next.set(GamePhase::Playing);
                        return;
                    }
                    CcAction::Back => {
                        next.set(GamePhase::MainMenu);
                        return;
                    }
                    CcAction::TintPrev => {
                        cc.tint_idx = (cc.tint_idx + TINTS.len() - 1) % TINTS.len();
                        dirty_tint = true;
                    }
                    CcAction::TintNext => {
                        cc.tint_idx = (cc.tint_idx + 1) % TINTS.len();
                        dirty_tint = true;
                    }
                    CcAction::SkinPrev => {
                        let n = cc.skins.len().max(1);
                        cc.skin_idx = (cc.skin_idx + n - 1) % n;
                        dirty_skin = true;
                    }
                    CcAction::SkinNext => {
                        let n = cc.skins.len().max(1);
                        cc.skin_idx = (cc.skin_idx + 1) % n;
                        dirty_skin = true;
                    }
                }
            }
            Interaction::Hovered => *bg = BackgroundColor(Color::srgb(0.14, 0.20, 0.30)),
            Interaction::None => *bg = BackgroundColor(Color::srgb(0.10, 0.14, 0.22)),
        }
    }

    if dirty_tint {
        let (label, tint) = TINTS[cc.tint_idx];
        selected.tint = tint;
        if let Ok(mut img) = sets.p0().get_single_mut() {
            img.color = Color::srgba(tint[0], tint[1], tint[2], tint[3]);
        }
        if let Ok(mut t) = sets.p1().get_single_mut() {
            t.0 = label.to_string();
        }
    }
    if dirty_skin {
        if let Some((label, path)) = cc.skins.get(cc.skin_idx).cloned() {
            selected.sheet = path.clone();
            if let (Ok(mut img), Some(layout)) = (sets.p0().get_single_mut(), cc.atlas.clone()) {
                img.image = asset_server.load(path);
                img.texture_atlas = Some(TextureAtlas { layout, index: 0 });
            }
            if let Ok(mut t) = sets.p2().get_single_mut() {
                t.0 = label;
            }
        }
    }
}

fn apply_selection(cc: &CcState, selected: &mut SelectedCharacter) {
    if let Some((_, path)) = cc.skins.get(cc.skin_idx) {
        selected.sheet = path.clone();
    }
    selected.tint = TINTS[cc.tint_idx].1;
}

fn start_game(selected: &SelectedCharacter) {
    save_savegame(selected);
    eprintln!("character-create: JUGAR name='{}' sheet={}", selected.name, selected.sheet);
}
