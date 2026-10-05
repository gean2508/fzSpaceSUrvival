//! Full-screen UI root — all gameplay HUD is parented here so center = screen center,
//! independent of the 38/62 touch split (that split is input-only, not layout).

use bevy::prelude::*;
use bevy::ui::FocusPolicy;

use crate::input::TouchLayout;

#[derive(Component)]
pub struct GameUiRoot;

/// UI que sólo tiene sentido DENTRO de la partida: el HUD entero, los sticks y
/// botones táctiles, el velo de nubes, el overlay de reentrada, el feed de
/// cámaras. Marcada así, `hide_gameplay_ui_outside_playing` la apaga en splash,
/// menú y creación de personaje.
///
/// **Por qué hace falta si el menú ya dibuja un panel opaco encima.** "Encima" es
/// una garantía de ORDEN, no de contenido: todo esto vive en la misma capa de UI
/// que el menú y se sigue midiendo, extrayendo y dibujando durante el arranque.
/// Basta un panel que no cubra hasta el borde, un empate de orden o un frame
/// suelto en una transición para que el gameplay asome — el parpadeo que se veía
/// al abrir el juego. Apagado, esos frames no tienen nada que mostrar.
#[derive(Component)]
#[component(on_add = ocultar_al_nacer)]
pub struct GameplayUi;

/// **Nace apagada** salvo que ya se esté jugando. `hide_gameplay_ui_outside_playing` corre en `Update`, o sea
/// DESPUÉS del `Startup` que crea los sticks y el HUD: con el `Visibility` por defecto (heredado = visible) hay
/// una ventana en la que el gameplay existe encendido mientras el splash todavía no se pintó — el destello de la
/// UI de juego antes del logo. Con el gancho, ninguna raíz marcada puede dibujarse un solo cuadro antes de su fase.
fn ocultar_al_nacer(mut world: bevy::ecs::world::DeferredWorld, entity: Entity, _id: bevy::ecs::component::ComponentId) {
    let jugando = world
        .get_resource::<State<crate::game_phase::GamePhase>>()
        .is_some_and(|s| *s.get() == crate::game_phase::GamePhase::Playing);
    if !jugando {
        world.commands().entity(entity).insert(Visibility::Hidden);
    }
}

#[derive(Resource, Default)]
pub struct GameUiRootReady(bool);

/// When true, raw touch gameplay (move stick, look, USE/JUMP) is suppressed.
#[derive(Resource, Default, Clone, Copy)]
pub struct UiGameplayBlock {
    pub blocked: bool,
}

const MENU_W: f32 = 96.0;

/// Horizontal offset from parent left so a widget of `width` is screen-centered.
pub fn ui_center_left(layout: &TouchLayout, width: f32) -> f32 {
    layout.width * 0.5 - width * 0.5
}

/// Full-screen touch block rect (top-left origin, same space as `TouchInput`).
pub fn fullscreen_block_rect(layout: &TouchLayout) -> [f32; 4] {
    [0.0, 0.0, layout.width, layout.height]
}

/// Apaga/enciende toda la UI marcada [`GameplayUi`] según la fase. Corre cada
/// frame a propósito (es una query de un puñado de raíces y sólo escribe cuando
/// el valor cambia): así da igual en qué orden se creen las raíces respecto de la
/// transición de estado — una que aparezca tarde queda apagada igual.
pub fn hide_gameplay_ui_outside_playing(
    phase: Res<State<crate::game_phase::GamePhase>>,
    mut ui: Query<&mut Visibility, With<GameplayUi>>,
) {
    let want = if *phase.get() == crate::game_phase::GamePhase::Playing {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut vis in &mut ui {
        if *vis != want {
            *vis = want;
        }
    }
}

pub fn ensure_game_ui_root(mut commands: Commands, mut ready: ResMut<GameUiRootReady>) {
    if ready.0 {
        return;
    }
    ready.0 = true;
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        FocusPolicy::Pass,
        GameUiRoot,
        GameplayUi,
        Name::new("GameUiRoot"),
    ));
}

pub fn sync_game_ui_root(
    layout: Res<TouchLayout>,
    mut q: Query<&mut Node, With<GameUiRoot>>,
    mut last: Local<(f32, f32)>,
) {
    if layout.width < 1.0 {
        return;
    }
    let key = (layout.width, layout.height);
    if *last == key {
        return;
    }
    *last = key;
    if let Ok(mut node) = q.get_single_mut() {
        node.width = Val::Px(layout.width);
        node.height = Val::Px(layout.height);
    }
}

/// Apply true screen-center positioning to a top-anchored bar/button.
pub fn apply_top_center(node: &mut Node, layout: &TouchLayout, width: f32, top: f32) {
    node.left = Val::Px(ui_center_left(layout, width));
    node.right = Val::Auto;
    node.top = Val::Px(top);
    node.bottom = Val::Auto;
    node.width = Val::Px(width);
    node.margin = UiRect::ZERO;
}

/// Apply true screen-center positioning to a bottom-anchored bar (hotbar).
pub fn apply_bottom_center(node: &mut Node, layout: &TouchLayout, width: f32, bottom: f32) {
    node.left = Val::Px(ui_center_left(layout, width));
    node.right = Val::Auto;
    node.bottom = Val::Px(bottom);
    node.top = Val::Auto;
    node.width = Val::Px(width);
    node.margin = UiRect::ZERO;
}

pub const MENU_BUTTON_WIDTH: f32 = MENU_W;
pub const MENU_BUTTON_HEIGHT: f32 = 34.0;

/// Borde de arriba del botón MENU, respetando el notch.
pub fn menu_button_top(layout: &TouchLayout) -> f32 {
    layout.insets.top as f32 + 6.0
}

/// Rect del botón MENU en el mismo espacio que `TouchInput` (origen arriba a la
/// izquierda). Vive acá y no en `hud.rs` porque lo necesitan los dos lados: el
/// que lo dibuja y el que bloquea el toque, para que el tap de abrir el menú no
/// se lo lleve además la mirada/interacción del mundo.
pub fn menu_button_rect(layout: &TouchLayout) -> [f32; 4] {
    let left = ui_center_left(layout, MENU_W);
    let top = menu_button_top(layout);
    [left, top, left + MENU_W, top + MENU_BUTTON_HEIGHT]
}

/// Ancho del botón de cámaras (entra "CAM (99)").
pub const CAM_BUTTON_WIDTH: f32 = 84.0;

/// Rect del botón de CÁMARAS: a la derecha de MENU, a su misma altura. Estaba fijo
/// en (8, 52) px, o sea ENCIMA de las barras de vida y energía (captura del 27 sep
/// 2026); arriba al centro no tapa nada y ya es zona de toque protegida.
pub fn cam_button_rect(layout: &TouchLayout) -> [f32; 4] {
    let [_, top, menu_right, bottom] = menu_button_rect(layout);
    let left = menu_right + 8.0;
    [left, top, left + CAM_BUTTON_WIDTH, bottom]
}

/// Center a floating panel on the full screen (mobile + desktop).
pub fn apply_center(node: &mut Node, layout: &TouchLayout, width: f32, height: f32) {
    node.left = Val::Px(ui_center_left(layout, width));
    node.top = Val::Px(((layout.height - height) * 0.5).max(layout.insets.top as f32));
    node.right = Val::Auto;
    node.bottom = Val::Auto;
    node.width = Val::Px(width);
    node.height = Val::Px(height);
    node.margin = UiRect::ZERO;
}
