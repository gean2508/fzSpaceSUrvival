use bevy::prelude::*;

use bevy::ui::widget::ImageNode;

use crate::game_ui::{
    self, apply_bottom_center, ensure_game_ui_root, sync_game_ui_root, fullscreen_block_rect,
    GameUiRoot, GameUiRootReady, UiGameplayBlock,
};
use crate::debug::{span_guard, CpuSpanSlot, CpuSpanStats};
use crate::input::{CraftIntent, EquippedItem, HoldInteractState, AbilityCooldown, TouchLayout, TouchBlockRects};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::flight::Flight;
use crate::machines::{MachineWorld, PendingMachinePlacement};
use crate::scene::{RegionId, RegionRuntime};
use crate::sim::{Player, SimState};
use crate::targeting::LookTarget;
use ha_shared::{
    can_craft, craft, get_craft_recipe, item_display_name, item_hotbar_label,
    require_machine, resolve_interaction, CraftRecipe, InteractActionKind,
    InteractTargetKind, STAMINA_MAX, CRAFT_RECIPES, STONE_EXTRACTOR, WOOD_EXTRACTOR,
};
use crate::ui_theme::{self, UiButtonState, UiButtonTone};
use crate::sky2d::PlayerLanternControl;
use crate::platform;

mod icons;
pub mod item_icons;
pub mod inventory_ui;
mod survival_bars;

use icons::{
    spawn_stat_heart, HudIconAtlas, HudIconPlugin, StatHeartIcon,
};
use item_icons::ItemIconCache;
use inventory_ui::{
    add_item_to_slots, consume_from_ui_slots, ensure_inventory_ui_state, flush_slots_to_inventory,
    grid_signature, inventory_drop_while_held_system, inventory_release_keeps_held_system,
    inventory_return_held_on_close_system,
    inventory_long_press_system, inventory_signature, inventory_slot_press_system,
    inventory_slot_release_system, load_grid_from_inventory, merge_inv_gains_into_ui, InventoryGridRoot, InventoryGridSlot, InventorySlotIcon,
    InventorySlotQty, InventoryUiState, HotbarItemSlot, HotbarSlotIcon, HotbarSlotName, HotbarSlotQty, SlotId,
    InventorySlotName,
    HeldItemGhost, GRID_COLS, GRID_ROWS, HOTBAR_ITEM_SLOTS, HOTBAR_SLOTS_TOTAL,
};

#[derive(Component)]
struct LanternToggleRoot;

#[derive(Component)]
struct LanternToggleButton;

#[derive(Component)]
struct LanternToggleLabel;

pub struct PixelHudPlugin;

impl Plugin for PixelHudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(HudIconPlugin)
            .init_resource::<GameUiRootReady>()
            .init_resource::<HudSpawned>()
            .init_resource::<HudTune>()
            .init_resource::<StaminaMeter>()
            .init_resource::<BagPanelState>()
                        .init_resource::<MachinePanelState>()
            .init_resource::<ItemIconCache>()
            .init_resource::<InventoryUiState>()
            .init_resource::<UiGameplayBlock>()
            .init_resource::<HudTouchBlocks>()
            .init_resource::<HudDisplayCache>()
            .init_resource::<HotbarSelectRequest>()
            .init_resource::<survival_bars::SurvivalBarsCache>()
            .add_systems(Startup, ensure_game_ui_root)
            .init_resource::<BagShot>()
            .add_systems(Update, (mover_pulgar_inventario, barras_de_la_mochila).after(bag_panel_system))
            .add_systems(Update, (skin_ui_from_art, casillas_del_kit, marcos_del_kit, pintar_personaje_mochila, pintar_mano_mochila, capturar_mochila).after(spawn_hud_once))
            // Aparte del bloque de abajo: ese ya llegó al tope de 20 sistemas
            // por tupla que acepta `add_systems`.
            .add_systems(
                Update,
                (update_day_clock, update_sky_dial, fill_ui_icon_slots, hide_touch_controls_with_panel)
                    .after(spawn_hud_once),
            )
            .add_systems(Update, (
                sync_game_ui_root,
                spawn_hud_once.after(sync_game_ui_root),
                update_hud_layout,
                update_floating_panels_layout,
                update_hud_state,
                update_survival_hud,
                machine_panel_system.after(update_survival_hud),
                sync_ui_gameplay_block,
                bag_hotbar_system,
                bag_panel_system.after(bag_hotbar_system),
                ensure_inventory_ui_state,
                inventory_slot_press_system,
                inventory_long_press_system,
                inventory_drop_while_held_system.after(inventory_long_press_system),
                inventory_slot_release_system,
                inventory_release_keeps_held_system.after(inventory_drop_while_held_system),
                inventory_return_held_on_close_system.after(inventory_drop_while_held_system),
                sync_inventory_from_player,
                update_inventory_slot_visuals,
                update_hotbar_item_visuals,
            ))
            // Aparte y no en la tupla de arriba: `add_systems` topea en 20 y esa
            // ya estaba al tope.
            .add_systems(Update, pintar_iconos_de_ficha)
            .add_systems(Update, (
                sync_hotbar_chrome_icons,
                equip_from_hotbar_selection,
                equip_from_hotbar_click,
                equip_toast_system,
                hotbar_hammer_system,
                lantern_toggle_system,
                update_lantern_toggle_ui,
                update_held_item_ghost,
                scroll_craft_list,
                update_coord_text,
                build_filter_system.after(bag_panel_system),
                bag_tab_style_system.after(bag_panel_system),
            ))
            .add_systems(Update, survival_bars::update_survival_bars)
            .add_systems(
                Update,
                (
                    append_hud_touch_blocks.after(crate::ui_editor::append_editor_touch_blocks),
                    update_interact_hud
                        .after(crate::machines::try_place_on_hold),
                ),
            );
    }
}

#[derive(Component)]
struct HotbarRoot;

#[derive(Component)]
struct HotbarCraftButton;

#[derive(Component)]
struct HotbarSlot {
    disabled: bool,
}

#[derive(Component)]
struct BagPanelRoot;

#[derive(Component)]
struct ModalBackdrop;

#[derive(Component)]
struct ModalLayerRoot;

#[derive(Component)]
struct StatusPromptRoot;

#[derive(Resource, Default)]
pub(crate) struct HudTouchBlocks(pub Vec<[f32; 4]>);

#[derive(Component)]
struct BagPanelCloseButton;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum BagTab {
    #[default]
    Inventory,
    Craft,
    /// Construction - pick a build piece; entering build mode (`build.rs`).
    Build,
}

#[derive(Component, Clone, Copy)]
struct BagTabButton {
    tab: BagTab,
}

#[derive(Component)]
struct BagInventorySection;

#[derive(Component)]
struct BagCraftSection;

/// Container for the CONSTRUIR tab's build-piece list (owned display toggle).
#[derive(Component)]
pub(crate) struct BagBuildSection;

/// El nodo que SCROLLEA de una pestaña (el que tiene `Overflow::scroll_y`).
#[derive(Component)]
pub(crate) struct BagScrollView;

/// Cuanto recorrio el dedo en el gesto en curso sobre la mochila.
///
/// Las tarjetas de CRAFTEAR y CONSTRUIR se disparaban con
/// `Interaction::Pressed`, o sea al APOYAR el dedo: arrastrar para ver el resto
/// de la lista elegia la receta donde empezaste. El inventario ya resolvia al
/// soltar (ver `inventory_ui::PressTrack`); esto le da el mismo criterio a las
/// otras dos pestanas.
/// Px de UI que puede moverse el dedo y seguir contando como toque.
pub(crate) const BAG_UMBRAL: f32 = 12.0;

/// El único hijo de un contenedor que scrollea: envuelve TODO el contenido.
/// Existe para poder MEDIRLO — sin un nodo que lo envuelva, la altura del
/// contenido de una lista que envuelve (`FlexWrap`) no se puede leer de ningún
/// lado, y sin esa altura el tope del scroll tiene que ser un número inventado.
/// Lo era en los dos paneles: `clamp(0.0, 900.0)` en la mochila (dejaba fuera de
/// alcance cientos de piezas de CONSTRUIR) y `clamp(0.0, 1200.0)` en el cofre.
#[derive(Component)]
pub(crate) struct ScrollContent;

/// Una ficha de filtro de la paleta; `.0` es el índice en [`build_groups`].
#[derive(Component, Clone, Copy)]
struct BuildFilterChip(usize);

/// Un grupo de la paleta de CONSTRUIR.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BuildGroup {
    /// Piso, pared, puerta: las piezas de rejilla de toda la vida.
    Pieces,
    /// Prefabs multi-celda (se estampa el área entera).
    Prefabs,
    /// Paredes de borde (estilo Last Day on Earth).
    Edges,
    /// Tapas: tumbados de interior y techos modulares de exterior.
    Ceilings,
    /// **Edificio modular**: fundar un ambiente, ampliarlo, apilarle plantas y
    /// abrirle puertas. No son piezas de la grilla — la banda del pack son 9
    /// celdas de un solo dibujo, así que la cáscara se coloca entera.
    Edificio,
    /// **TODO**: el catálogo entero, sin filtrar. Es la primera ficha de la
    /// imagen de referencia y la que se ve al abrir el inventario creativo: sin
    /// ella el panel abría mostrando las cuatro piezas de construcción y parecía
    /// vacío.
    Todo,
    /// Colocables del pack, por categoría.
    Place(ha_shared::placeables::PlaceCat),
    /// Fauna y utilería de vivario — sólo surte efecto en creativo.
    Fauna,
}

/// **La paleta se navega por grupos, no scrolleando 350 tarjetas.**
///
/// La categoría del colocable ya existía en los datos (`PlaceCat`, "es la
/// pestaña del menú CONSTRUIR" dice su propio comentario) pero el menú la usaba
/// sólo como encabezado dentro de una lista plana. Con el pack importado eso son
/// ~350 tarjetas en una sola tira: aunque el scroll llegara hasta el fondo,
/// encontrar una heladera ahí adentro con el pulgar es imposible.
fn build_groups() -> Vec<(BuildGroup, &'static str)> {
    // El orden es el de la imagen de referencia: TODO primero, después las
    // categorías del catálogo, y al final las cinco que no son catálogo.
    let mut out = vec![(BuildGroup::Todo, "TODO")];
    for &cat in ha_shared::placeables::PlaceCat::all() {
        out.push((BuildGroup::Place(cat), cat.chip()));
    }
    out.push((BuildGroup::Fauna, "FAUNA"));
    out.extend([
        (BuildGroup::Pieces, "PIEZAS"),
        (BuildGroup::Edges, "PAREDES"),
        (BuildGroup::Ceilings, "TECHOS"),
        (BuildGroup::Edificio, "EDIFICIO"),
        (BuildGroup::Prefabs, "PREFABS"),
    ]);
    out
}

/// El hueco del icono de una ficha de categoría; `.0` = índice en `build_groups()`.
#[derive(Component)]
struct IconoDeFicha(usize);

/// **El objeto que representa a cada pestaña.** No se elige a dedo: para las
/// categorías de colocables se toma **el primero de la categoría**, que es el que
/// el catálogo ya puso al frente (`PLACEABLES` está ordenado por cómo aparece en
/// el menú). Para las cinco fichas que no son categorías del catálogo —piezas,
/// paredes, techos, edificio, prefabs— y para fauna se nombra un slot que las
/// dice de un vistazo.
fn arte_de_ficha(grupo: BuildGroup) -> Option<String> {
    match grupo {
        BuildGroup::Place(cat) => ha_shared::placeables::placeables_in(cat)
            .next()
            .map(|(_, a)| a.art.to_string()),
        BuildGroup::Todo => Some("ui/ic_grilla".to_string()),
        BuildGroup::Pieces => Some("tiles/stone".to_string()),
        BuildGroup::Edges => Some("wall/madera".to_string()),
        BuildGroup::Ceilings => Some("roof/teja".to_string()),
        BuildGroup::Edificio => Some("edificio/fachada".to_string()),
        BuildGroup::Prefabs => Some("prop/banco_ciudad".to_string()),
        BuildGroup::Fauna => Some("fauna/pollo".to_string()),
    }
}

/// Llena los huecos de icono de las fichas con el arte del pack. Corre sólo
/// cuando hay huecos sin llenar: la ficha se dibuja una vez por apertura del
/// panel, así que esto es un puñado de nodos y no un trabajo por cuadro.
fn pintar_iconos_de_ficha(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    mut huecos: Query<(Entity, &IconoDeFicha, &mut Node), Without<ImageNode>>,
) {
    if huecos.is_empty() {
        return;
    }
    let grupos = build_groups();
    for (e, hueco, mut node) in &mut huecos {
        let Some((grupo, _)) = grupos.get(hueco.0).copied() else { continue };
        let Some(clave) = arte_de_ficha(grupo) else { continue };
        let Some(img) = art.ui_image_node(&clave) else { continue };
        node.display = Display::Flex;
        commands.entity(e).try_insert(img);
    }
}

/// A build-piece pick button inside the CONSTRUIR tab; `.0` = `BUILD_PIECES` idx.
#[derive(Component)]
pub(crate) struct BagBuildPieceButton(pub usize);

#[derive(Component)]
struct CraftRecipeButton {
    recipe_id: u32,
    is_machine_select: bool,
}

#[derive(Component)]
struct CraftRecipeCostText {
    recipe_id: u32,
    is_machine_select: bool,
}

#[derive(Component)]
struct BagPanelBase {
    width: f32,
    height: f32,
}

#[derive(Component)]
pub(crate) struct HoldBarRoot;

#[derive(Component)]
pub(crate) struct HoldBarFill;

#[derive(Component)]
pub(crate) struct AbilityCooldownRing;

#[derive(Component)]
struct InteractPromptText;

#[derive(Component)]
struct MachinePanelRoot;

#[derive(Component)]
struct MachinePanelTitle;

#[derive(Component)]
struct MachinePanelBody;

#[derive(Component)]
struct MachinePanelCloseButton;

#[derive(Component)]
struct MachinePanelBase {
    width: f32,
    height: f32,
}

#[derive(Component)]
struct CraftStockText;

#[derive(Component)]
struct HotbarHammerSlot;

#[derive(Component)]
struct HotbarHammerIcon;

#[derive(Component)]
struct HotbarBagIcon;

#[derive(Resource, Default)]
struct HudSpawned(bool, f32);

/// Marca cada nodo de primer nivel que arma `spawn_hud_once`, para poder
/// rehacerlos todos cuando la ventana cambia de tamaño.
#[derive(Component)]
struct HudArmado;

#[derive(Resource, Default)]
struct HudTune {
    heart_px: f32,
    energy_px: f32,
}

#[derive(Resource)]
struct StaminaMeter {
    value: f32,
    max: f32,
}

impl Default for StaminaMeter {
    fn default() -> Self {
        Self {
            value: STAMINA_MAX,
            max: STAMINA_MAX,
        }
    }
}


#[derive(Resource)]
pub struct BagPanelState {
    pub open: bool,
    pub(crate) tab: BagTab,
    /// Ficha activa de la paleta de CONSTRUIR (índice en [`build_groups`]).
    /// Vive acá y no en un recurso propio porque `bag_panel_system` ya está en
    /// el tope de 16 parámetros que acepta un sistema de Bevy.
    build_group: usize,
    /// Página de la ficha activa. Ver [`BUILD_POR_PAGINA`].
    build_page: usize,
    /// La ficha cambió: hay que rehacer la lista.
    build_dirty: bool,
    pulse: Timer,
    modal: Option<Entity>,
    /// **Con qué pestaña se armó el panel que está abierto.** El panel del kit
    /// (`spawn_bag_panel`) dibuja UNA pestaña; sin esto, tocar FABRICAR o
    /// CONSTRUIR escondía la mochila y dejaba el panel vacío, con el título y el
    /// resaltado en MOCHILA (28 sep 2026). Si la pestaña cambia, se rearma.
    armado_con: Option<BagTab>,
    /// Cuanto recorrio el dedo en el gesto en curso. Vive aca por el mismo
    /// motivo que `build_group`: `bag_panel_system` esta en el tope de 16
    /// parametros. Con esto, arrastrar la lista para ver mas recetas ya no
    /// craftea la que estaba debajo del dedo al apoyar.
    recorrido: f32,
    /// La tarjeta que el dedo APOYO, todavia sin confirmar.
    ///
    /// Las recetas se disparaban con `Interaction::Pressed`, o sea al apoyar,
    /// cuando el recorrido todavia vale 0: por eso la guarda contra el arrastre
    /// nunca llegaba a activarse y scrollear la lista te crafteaba lo que
    /// hubiera debajo del dedo. Ahora se anota aca y se decide AL SOLTAR, que es
    /// el mismo patron que ya funciona en el creador de personaje.
    pendiente: Option<u32>,
    /// Hay un dedo apoyado sobre la mochila ahora mismo. Lo escribe
    /// `scroll_craft_list` (que ya lee `Touches`) para que `bag_panel_system`
    /// sepa cuando se solto sin gastar uno de sus 16 parametros.
    dedo: bool,
}

impl BagPanelState {
    /// El gesto en curso ya es un scroll, no un toque.
    pub(crate) fn es_scroll(&self) -> bool {
        self.recorrido > BAG_UMBRAL
    }
}

#[derive(Resource, Default)]
struct MachinePanelState {
    panel: Option<Entity>,
}

impl BagPanelState {
    /// Cambia la ficha activa de la paleta de CONSTRUIR. La usa el driver de QA
    /// (`debug::autodrive`) para capturar una categoría de colocables: sin esto
    /// la única captura posible es la del grupo por defecto, y el camino que se
    /// quiere ver —la lista rehecha al filtrar— no se ejercita nunca.
    pub(crate) fn set_build_group(&mut self, group: usize) {
        if self.build_group != group {
            self.build_group = group;
            self.build_dirty = true;
        }
    }

    /// Índice de la primera ficha de una categoría de colocables.
    pub(crate) fn build_group_of(cat: ha_shared::placeables::PlaceCat) -> usize {
        build_groups()
            .iter()
            .position(|(g, _)| *g == BuildGroup::Place(cat))
            .unwrap_or(0)
    }
}

impl Default for BagPanelState {
    fn default() -> Self {
        Self {
            open: false,
            tab: BagTab::Inventory,
            build_group: 0,
            build_page: 0,
            build_dirty: false,
            pulse: Timer::from_seconds(0.15, TimerMode::Once),
            modal: None,
            armado_con: None,
            recorrido: 0.0,
            pendiente: None,
            dedo: false,
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct HudDisplayCache {
    initialized: bool,
    heart_units: i32,
    energy_units: i32,
    hold_visible: bool,
    hold_pct: f32,
    cooldown_visible: bool,
    cooldown_alpha: f32,
    prompt_line: String,
    craft_stock_line: String,
    machine_panel_visible: bool,
    machine_panel_line: String,
    machine_panel_dismissed: bool,
    inv_signature: String,
    prompt_visible: bool,
    prompt_alpha: f32,
}

fn spawn_hud_once(
    mut commands: Commands,
    layout: Res<TouchLayout>,
    root: Query<Entity, With<GameUiRoot>>,
    mut spawned: ResMut<HudSpawned>,
    armados: Query<Entity, With<HudArmado>>,
    mut tune: ResMut<HudTune>,
    player: Query<&SimState, With<Player>>,
    atlas: Res<HudIconAtlas>,
    art: Res<crate::art2d::ArtLibrary>,
    mut images: ResMut<Assets<Image>>,
) {
    if player.get_single().is_err() || layout.width < 100.0 || !atlas.ready {
        return;
    }
    // **El HUD se rehace si la ventana cambió de tamaño.**
    //
    // Se armaba UNA vez, con el ancho que hubiera en ese momento, y todo lo que
    // sale de las unidades del mockup quedaba a la escala vieja: el gestor de
    // ventanas achica la ventana un instante después de crearla y el reloj se
    // iba fuera de pantalla mientras el bloque de estado quedaba al doble. En el
    // teléfono no pasa —no hay resize— pero es justo lo que hacía imposible
    // comparar contra las imágenes de referencia en el escritorio.
    if spawned.0 && (spawned.1 - layout.width).abs() < 2.0 {
        return;
    }
    let Ok(root) = root.get_single() else {
        return;
    };
    for e in &armados {
        commands.entity(e).despawn_recursive();
    }
    spawned.0 = true;
    spawned.1 = layout.width;

    let short = layout.width.min(layout.height);
    let margin = (short * 0.02).clamp(10.0, 20.0);
    let heart = (short * 0.03).clamp(15.0, 24.0);
    let gap = (heart * 0.15).clamp(2.0, 4.0);
    tune.heart_px = heart;
    tune.energy_px = heart;
    let bar_icons = survival_bars::BarIcons::nuevos(&mut images);

    commands.entity(root).with_children(|parent| {
        // **El bloque de estado, con las medidas de la imagen de referencia.**
        // Un velo oscuro de 520 × 190 en (24, 24), diez corazones del pack y las
        // tres barras con su iconito. Todo en unidades del mockup por `k`.
        let k = crate::ui_pack::k(&layout);
        parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(layout.insets.left as f32 + 24.0 * k),
                    top: Val::Px(layout.insets.top as f32 + 24.0 * k),
                    width: Val::Px(520.0 * k),
                    height: Val::Px(190.0 * k),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(14.0 * k),
                    padding: UiRect {
                        left: Val::Px(22.0 * k),
                        top: Val::Px(16.0 * k),
                        ..default()
                    },
                    ..default()
                },
                BackgroundColor(Color::srgba(0.055, 0.043, 0.078, 0.376)),
                BorderRadius::all(Val::Px(18.0 * k)),
                HudArmado,
                Name::new("HudTopLeft"),
            ))
            .with_children(|panel| {
                // La fila de corazones: diez, cada 46 unidades, con el recorte
                // del pack a su proporción. Van ABSOLUTOS porque el dibujo mide
                // más ancho que el paso — así se pegan como en la maqueta.
                panel
                    .spawn((
                        Node {
                            width: Val::Px(474.0 * k),
                            height: Val::Px(42.0 * k),
                            flex_shrink: 0.0,
                            ..default()
                        },
                        Name::new("HeartRow"),
                    ))
                    .with_children(|row| {
                        for idx in 0..10 {
                            spawn_stat_heart(row, idx, 42.0 * k, &atlas, &art);
                        }
                    });
                survival_bars::spawn_survival_bars(
                    panel,
                    466.0 * k,
                    29.0 * k,
                    18.0 * k,
                    9.0 * k,
                    &bar_icons,
                );
                spawn_status_prompt_block(panel, heart, gap);
            });

        // La fila de energía que vivía acá arriba a la derecha —seis muslos de
        // pollo— se borró el 15 ago 2026: al lado de una barra de hambre que ya
        // existía, el jugador veía comida dos veces. La energía es ahora una
        // barra más, con su rayo, en la columna de la izquierda.

        spawn_menu_button(parent, &layout);
        spawn_day_clock(parent, &layout, margin, heart, gap);
        spawn_hotbar(parent, &layout, short, heart);
        spawn_equip_toast(parent, &layout, heart);
        spawn_lantern_toggle(parent, &layout, short);
        spawn_interact_hud(parent, &layout, short);
    });
}

/// **El botón MENU, arriba al centro** — donde estuvo siempre y donde se lo
/// vuelve a pedir. Estuvo un tiempo metido dentro del header de la mochila y no
/// funcionó: para pausar, mirar el mapa o abrir opciones había que abrir primero
/// la mochila, o sea dos toques y un rodeo por una pantalla que no tiene nada que
/// ver. Arriba al centro es la única zona del HUD que no pisa nada (la vida va a
/// la izquierda, la energía y el reloj a la derecha, la hotbar abajo).
///
/// Posición fija al nacer, como el resto del HUD: recalcularla por frame marca el
/// nodo sucio y hace que taffy relayoutee el árbol entero (biblia §10, cambio 3).
fn spawn_menu_button(parent: &mut ChildBuilder, layout: &TouchLayout) {
    let w = game_ui::MENU_BUTTON_WIDTH;
    parent
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(game_ui::ui_center_left(layout, w)),
                top: Val::Px(game_ui::menu_button_top(layout)),
                width: Val::Px(w),
                height: Val::Px(game_ui::MENU_BUTTON_HEIGHT),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderRadius::all(Val::Px(8.0)),
            BorderColor(ui_theme::button_border(UiButtonTone::Primary)),
            BackgroundColor(ui_theme::button_bg(UiButtonTone::Primary, UiButtonState::Normal)),
            crate::ui_editor::UiEditorCommand::Menu,
            Name::new("InGameMenuButton"),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new("MENU"),
                TextFont { font_size: 12.0, ..default() },
                TextColor(ui_theme::text_primary()),
            ));
        });
}

/// Reloj del día (arriba a la derecha, DEBAJO de la fila de energía — misma
/// columna, mismo margen, mismo panel que el resto del HUD). Muestra la hora del
/// mundo y el tramo (`DIA`/`ATARDECER`/`NOCHE`), y una barra fina que se llena a
/// lo largo del día. Importa ahora que la noche es oscura de verdad: hay que
/// poder mirar y saber cuánto falta para que amanezca. ASCII-only.
#[derive(Component)]
struct DayClockText;

/// Las coordenadas del jugador, debajo del reloj. Es el primer ladrillo de los
/// puntos de referencia: para poder volver a casa primero hay que poder DECIR
/// donde esta.
#[derive(Component)]
struct CoordText;
#[derive(Component)]
struct DayClockPhaseText;
#[derive(Component)]
struct DayClockFill;

/// **El minigráfico del cielo**, al lado del reloj: media Tierra abajo, el astro
/// de turno recorriendo su arco por encima y estrellitas de noche. Es lo que
/// reemplaza al disco de sol que se dibujaba en el mundo (ver `sky2d`): la misma
/// información —qué hora del cielo es y cuán inclinado está el astro— pero en el
/// sitio donde se la busca, que es el reloj, y sin taparte el juego.
///
/// Son 8 nodos de UI sin texturas: dos círculos (astro), un domo y cinco puntos.
#[derive(Component)]
struct SkyDialBg;
/// El sol: se muestra sólo en su turno y se mueve por el arco.
#[derive(Component)]
struct SkyDialSun;
/// La luna: idem, de noche. `.0` es su diámetro a luna LLENA en px — el dibujo se
/// encoge con la fase, que es la lectura que el ojo acepta sin geometría de fases
/// (creciente delgada = poca luna), igual que hacía el disco del mundo.
#[derive(Component)]
struct SkyDialMoon(f32);
/// Una estrellita del dial; `.0` es su posición en % (x, y) dentro del cuadro.
#[derive(Component)]
struct SkyDialStar(Vec2);

/// Sitio del astro dentro del dial, en porcentaje del cuadro. `t` es 0 al salir,
/// 0.5 en el cenit y 1 al ponerse — el mismo parámetro que usa el arco de
/// `sky2d::arc`, así el dibujo y las sombras nunca se contradicen.
fn sky_dial_pos(t: f32) -> (f32, f32) {
    let th = std::f32::consts::PI * t.clamp(0.0, 1.0);
    // El astro sale por la DERECHA (este), como en el mundo.
    (50.0 + th.cos() * 38.0, 74.0 - th.sin() * 58.0)
}

/// **El reloj, tal cual la imagen de referencia.**
///
/// Un panel liso arriba a la derecha con el dial del cielo, la hora grande, el
/// día, dónde estás con tus coordenadas, y la barra del día debajo. Las medidas
/// son las de `mock_hud::reloj`, en las mismas unidades que el resto de la
/// interfaz nueva.
fn spawn_day_clock(
    parent: &mut ChildBuilder,
    layout: &TouchLayout,
    margin: f32,
    heart: f32,
    gap: f32,
) {
    let _ = (margin, gap);
    let k = crate::ui_pack::k(layout);
    // Coordenadas del mockup (2400 × 1080).
    let (rw, rh) = (672.0_f32, 168.0_f32);
    let rx = 2400.0 - 24.0 - rw;
    let ry = 24.0 + layout.insets.top as f32 / k.max(0.001);
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(rx * k),
                top: Val::Px(ry * k),
                width: Val::Px(rw * k),
                height: Val::Px(rh * k),
                ..default()
            },
            crate::ui_pack::SinMarco,
            // Color de respaldo: sin arte del kit el panel quedaba invisible y
            // con él se ve igual que en la maqueta (el marco lo pisa encima).
            BackgroundColor(Color::srgb(0.796, 0.706, 0.580)),
            MarcoDelKit(crate::ui_pack::marco::LISO),
            HudArmado,
            Name::new("DayClock"),
        ))
        .with_children(|panel| {
            // El dial: 156 × 108 a 30 px del borde.
            panel
                .spawn(Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(30.0 * k),
                    top: Val::Px(30.0 * k),
                    width: Val::Px(156.0 * k),
                    height: Val::Px(108.0 * k),
                    ..default()
                })
                .with_children(|d| spawn_sky_dial(d, heart));
            // La hora, grande.
            panel
                .spawn(Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(214.0 * k),
                    top: Val::Px(24.0 * k),
                    height: Val::Px(68.0 * k),
                    align_items: AlignItems::Center,
                    column_gap: Val::Px(24.0 * k),
                    ..default()
                })
                .with_children(|fila| {
                    fila.spawn((
                        Text::new("--:--"),
                        TextFont { font_size: 54.0 * k, ..default() },
                        TextColor(crate::ui_pack::Pack::TINTA),
                        TextLayout::new_with_no_wrap(),
                        DayClockText,
                    ));
                    fila.spawn((
                        Text::new("DÍA 1"),
                        TextFont { font_size: 34.0 * k, ..default() },
                        TextColor(crate::ui_pack::Pack::SUAVE),
                        TextLayout::new_with_no_wrap(),
                        DayClockPhaseText,
                    ));
                });
            // Dónde estás.
            panel
                .spawn(Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(214.0 * k),
                    top: Val::Px(90.0 * k),
                    height: Val::Px(40.0 * k),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|fila| {
                    fila.spawn((
                        Text::new("0, 0"),
                        TextFont { font_size: 27.0 * k, ..default() },
                        TextColor(crate::ui_pack::Pack::SUAVE),
                        TextLayout::new_with_no_wrap(),
                        CoordText,
                    ));
                });
            // La barra del día.
            panel
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(214.0 * k),
                        top: Val::Px(126.0 * k),
                        width: Val::Px((rw - 254.0) * k),
                        height: Val::Px(8.0 * k),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.588, 0.486, 0.384)),
                ))
                .with_children(|track| {
                    track.spawn((
                        Node { width: Val::Percent(0.0), height: Val::Percent(100.0), ..default() },
                        BackgroundColor(crate::ui_pack::Pack::ACENTO),
                        DayClockFill,
                    ));
                });
        });
}

/// Un nodo que quiere el marco `.0` del kit. Lo pinta [`marcos_del_kit`] al
/// nacer — el HUD se arma en funciones que no tienen la biblioteca de arte a
/// mano.
#[derive(Component)]
pub struct MarcoDelKit(pub &'static str);

fn marcos_del_kit(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    nuevos: Query<(Entity, &MarcoDelKit), Added<MarcoDelKit>>,
) {
    for (e, m) in &nuevos {
        if let Some(img) = art.ui_frame_node(m.0) {
            commands.entity(e).try_insert(img);
        }
    }
}

/// El dial del cielo. El orden de los hijos ES el orden de dibujo en Bevy UI, así
/// que el domo de la Tierra va ÚLTIMO: el astro se esconde detrás de él al salir y
/// al ponerse, que es la mitad de la lectura ("está bajando").
fn spawn_sky_dial(parent: &mut ChildBuilder, heart: f32) {
    let w = (heart * 2.1).clamp(38.0, 62.0);
    let h = w * 0.62;
    let astro = (w * 0.20).clamp(7.0, 13.0);
    parent
        .spawn((
            Node {
                width: Val::Px(w),
                height: Val::Px(h),
                overflow: Overflow::clip(),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            // Redondeado arriba y recto abajo: la silueta de una ventanita al
            // cielo, no otra cajita más.
            BorderRadius::px(w * 0.5, w * 0.5, 3.0, 3.0),
            BorderColor(ui_theme::accent_ink()),
            BackgroundColor(Color::srgb(0.10, 0.16, 0.30)),
            SkyDialBg,
            Name::new("SkyDial"),
        ))
        .with_children(|dial| {
            // Estrellitas: sitios fijos (no aleatorios por frame — eso titila mal)
            // y su alpha lo pone la noche.
            for p in [
                Vec2::new(16.0, 22.0),
                Vec2::new(34.0, 10.0),
                Vec2::new(52.0, 26.0),
                Vec2::new(70.0, 14.0),
                Vec2::new(84.0, 34.0),
            ] {
                dial.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(p.x),
                        top: Val::Percent(p.y),
                        width: Val::Px(2.0),
                        height: Val::Px(2.0),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                    SkyDialStar(p),
                ));
            }
            for (marker, color) in [
                (SkyDialKind::Sun, Color::srgb(1.0, 0.86, 0.42)),
                (SkyDialKind::Moon, Color::srgb(0.88, 0.92, 1.0)),
            ] {
                let mut e = dial.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        width: Val::Px(astro),
                        height: Val::Px(astro),
                        display: Display::None,
                        ..default()
                    },
                    BorderRadius::all(Val::Percent(50.0)),
                    BackgroundColor(color),
                ));
                match marker {
                    SkyDialKind::Sun => e.insert(SkyDialSun),
                    SkyDialKind::Moon => e.insert(SkyDialMoon(astro)),
                };
            }
            // La "tierrita": media Tierra asomando de abajo para arriba. Es un
            // círculo grande recortado por el cuadro del dial.
            dial.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(-24.0),
                    top: Val::Percent(62.0),
                    width: Val::Percent(148.0),
                    height: Val::Px(h * 1.2),
                    ..default()
                },
                BorderRadius::all(Val::Percent(50.0)),
                BackgroundColor(Color::srgb(0.20, 0.42, 0.22)),
            ));
        });
}

/// Sólo para elegir qué marcador insertar en el bucle de arriba.
enum SkyDialKind {
    Sun,
    Moon,
}

/// Refresca el reloj. Sólo escribe cuando el MINUTO cambia (el texto de Bevy
/// re-mide el layout en cada set) → coste despreciable.
#[allow(clippy::type_complexity)]
fn update_day_clock(
    clock: Res<crate::sky2d::DayClock>,
    weather: Res<crate::weather::Weather>,
    mut q: ParamSet<(
        Query<&mut Text, With<DayClockText>>,
        Query<(&mut Text, &mut TextColor), With<DayClockPhaseText>>,
        Query<&mut Node, With<DayClockFill>>,
    )>,
    mut last: Local<(u32, u32)>,
    mut last_fill: Local<f32>,
) {
    let (h, m) = clock.clock_hm();
    // **La barra sólo se toca cuando de verdad se movió.** Escribir un `Node` lo
    // marca sucio y taffy recalcula el ÁRBOL ENTERO de la UI; esta línea estaba
    // arriba del corte por minuto, así que pagábamos un relayout completo todos
    // los frames para siempre. La barra mide ~200 px: por debajo de un cuarto de
    // punto porcentual el cambio no llega ni a medio píxel.
    let fill = clock.phase() * 100.0;
    if (fill - *last_fill).abs() > 0.25 {
        *last_fill = fill;
        for mut node in &mut q.p2() {
            node.width = Val::Percent(fill);
        }
    }
    if (h, m) == *last {
        return;
    }
    *last = (h, m);
    let hhmm = format!("{h:02}:{m:02}");
    for mut text in &mut q.p0() {
        text.0 = hhmm.clone();
    }
    let name = clock.phase_name();
    // El tramo se tiñe como el cielo: cálido al atardecer, frío de noche.
    let col = match name {
        "ATARDECER" | "AMANECER" => Color::srgb(1.0, 0.72, 0.42),
        "NOCHE" => Color::srgb(0.62, 0.72, 1.0),
        _ => ui_theme::text_muted(),
    };
    // El clima va pegado al tramo del día ("DIA - LLUVIA"): es la misma
    // información —qué está pasando afuera— y no gasta una fila más de HUD.
    // Despejado no se escribe: lo normal no se anuncia.
    // **"DÍA 12", no "AMANECER".** Es lo que dice la imagen de referencia: el
    // tramo del día ya se lee en el dial del cielo, y lo que no se puede deducir
    // mirando es cuántos días llevás. El clima sí se agrega, porque tampoco se
    // deduce del dial.
    let line = match weather.kind {
        crate::weather::WeatherKind::Clear => format!("DÍA {}", clock.day_index() as u32 + 1),
        k => format!("DÍA {} · {}", clock.day_index() as u32 + 1, k.label()),
    };
    let _ = name;
    for (mut text, mut color) in &mut q.p1() {
        text.0 = line.clone();
        color.0 = col;
    }
}

/// Escribe las coordenadas del jugador debajo del reloj.
///
/// Redondeadas a metro y con throttle por CAMBIO: reescribir un `Text` marca el
/// arbol de UI sucio y taffy relayoutea (biblia §10), asi que solo se toca
/// cuando de verdad te moviste de celda.
fn update_coord_text(
    player: Query<&Transform, With<crate::sim::Player>>,
    mut q: Query<&mut Text, With<CoordText>>,
    mut last: Local<(i64, i64)>,
) {
    let Ok(t) = player.get_single() else { return };
    let xz = (t.translation.x.round() as i64, (-t.translation.y).round() as i64);
    if xz == *last {
        return;
    }
    *last = xz;
    for mut txt in &mut q {
        txt.0 = format!("{}, {}", xz.0, xz.1);
    }
}

/// Mueve el dial: el astro por su arco, el color del cielo y las estrellas. Corre
/// una vez por MINUTO de juego (2.5 s reales con el ciclo de 1 h): el astro avanza
/// ~1% del arco por paso, o sea imperceptiblemente escalonado, y el HUD no re-mide
/// layout todos los frames.
#[allow(clippy::type_complexity)]
fn update_sky_dial(
    clock: Res<crate::sky2d::DayClock>,
    mut q: ParamSet<(
        Query<&mut BackgroundColor, With<SkyDialBg>>,
        Query<&mut Node, With<SkyDialSun>>,
        Query<(&mut Node, &SkyDialMoon)>,
        Query<(&mut BackgroundColor, &SkyDialStar)>,
    )>,
    mut last: Local<(u32, u32)>,
) {
    let hm = clock.clock_hm();
    if hm == *last {
        return;
    }
    *last = hm;
    use crate::sky2d::{DAWN_START, NIGHT_START};
    let p = clock.phase();
    let day = clock.daylight();
    let sun_up = (DAWN_START..NIGHT_START).contains(&p);
    // Cielo del dial: la misma idea que el `ClearColor` del mundo (noche azul
    // profundo → celeste de día) con el naranja del crepúsculo encima.
    let night = Vec3::new(0.055, 0.075, 0.185);
    let noon = Vec3::new(0.33, 0.58, 0.85);
    let dusk = Vec3::new(0.80, 0.42, 0.28);
    let bg = night
        .lerp(noon, day)
        .lerp(dusk, crate::sky2d::dusk_amount(p) * 0.75);
    for mut c in &mut q.p0() {
        c.0 = Color::srgb(bg.x, bg.y, bg.z);
    }
    // Sol y luna: cada uno en su turno, en el sitio que le toca del arco.
    let (sx, sy) = sky_dial_pos((p - DAWN_START) / (NIGHT_START - DAWN_START));
    for mut node in &mut q.p1() {
        node.display = if sun_up { Display::Flex } else { Display::None };
        node.left = Val::Percent(sx);
        node.top = Val::Percent(sy);
    }
    let night_span = 1.0 - (NIGHT_START - DAWN_START);
    let (mx, my) = sky_dial_pos((p - NIGHT_START).rem_euclid(1.0) / night_span);
    let illum = clock.moon_illum();
    for (mut node, moon) in &mut q.p2() {
        // La luna nueva no se dibuja: esa noche sin luna es contenido, no un bug.
        node.display = if !sun_up && illum > 0.08 { Display::Flex } else { Display::None };
        let d = moon.0 * (0.55 + 0.45 * illum);
        node.width = Val::Px(d);
        node.height = Val::Px(d);
        node.left = Val::Percent(mx);
        node.top = Val::Percent(my);
    }
    // Las estrellas salen con la noche, y las de más arriba brillan un poco más.
    let star_a = (1.0 - day) * 0.95;
    for (mut c, star) in &mut q.p3() {
        let k = star_a * (0.65 + 0.35 * (1.0 - star.0.y / 40.0));
        c.0 = Color::srgba(1.0, 1.0, 0.95, k.clamp(0.0, 1.0));
    }
}

/// **Con un panel abierto no se dibujan los controles.** Los joysticks y los
/// botones de acción viven en la misma capa que la mochila y, ahora que la
/// ventana ocupa la pantalla entera, quedaban flotando ENCIMA de las tarjetas —
/// dos discos enormes tapando el inventario. Ya estaban desconectados
/// (`UiGameplayBlock` ignora el toque); esto sólo hace que además se vayan de la
/// vista, que es lo que el ojo espera.
fn hide_touch_controls_with_panel(
    bag: Res<BagPanelState>,
    chest: Res<crate::chests::ChestPanelState>,
    editor: Res<crate::ui_editor::UiEditorState>,
    mut sticks: Query<&mut Visibility, With<crate::input::JoystickBase>>,
    mut actions: Query<
        &mut Visibility,
        (With<crate::input::ActionUiRoot>, Without<crate::input::JoystickBase>),
    >,
) {
    let want = if bag.open || chest.is_open() || editor.menu_open || editor.enabled {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    for mut vis in sticks.iter_mut().chain(actions.iter_mut()) {
        if *vis != want {
            *vis = want;
        }
    }
}

fn spawn_status_prompt_block(parent: &mut ChildBuilder, heart: f32, gap: f32) {
    let border = ui_theme::panel_border_width();
    parent
        .spawn((
            Node {
                // Nace OCULTO: sin nada tocado no hay nada que decir, y el
                // arranque del juego no es momento de explicar controles.
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(gap * 0.6),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(6.0)),
                border: UiRect::all(Val::Px(border)),
                min_width: Val::Px(180.0),
                max_width: Val::Px(320.0),
                ..default()
            },
            BorderColor(ui_theme::accent_ink()),
            BackgroundColor(ui_theme::panel_bg(0.88)),
            StatusPromptRoot,
            Name::new("StatusPromptBlock"),
        ))
        .with_children(|block| {
            block.spawn((
                Text::new(""),
                TextFont {
                    font_size: (heart * 0.58).clamp(10.0, 14.0),
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
                InteractPromptText,
            ));
            block
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(10.0),
                        display: Display::None,
                        border: UiRect::all(Val::Px(2.0)),
                        ..default()
                    },
                    BorderColor(ui_theme::ink_outline()),
                    BackgroundColor(ui_theme::panel_bg(0.55)),
                    HoldBarRoot,
                    Name::new("HoldInteractBar"),
                ))
                .with_children(|bar| {
                    bar.spawn((
                        Node {
                            width: Val::Percent(0.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(ui_theme::accent_cyan()),
                        HoldBarFill,
                    ));
                });
        });
}

fn spawn_machine_panel(commands: &mut Commands, root: Entity, layout: &TouchLayout, heart: f32) -> Entity {
    let short = layout.width.min(layout.height);
    let panel_w = (short * 0.48).clamp(240.0, 360.0);
    let panel_h = (short * 0.28).clamp(140.0, 220.0);
    let left = game_ui::ui_center_left(layout, panel_w);
    let top = ((layout.height - panel_h) * 0.5).max(layout.insets.top as f32);
    let mut panel_entity = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|parent| {
        panel_entity = parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(left),
                    top: Val::Px(top),
                    width: Val::Px(panel_w),
                    height: Val::Px(panel_h),
                    padding: UiRect::all(Val::Px(10.0)),
                    border: UiRect::all(Val::Px(ui_theme::panel_border_width())),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(6.0),
                    overflow: Overflow::clip(),
                    ..default()
                },
                BorderColor(ui_theme::accent_ink()),
                BackgroundColor(ui_theme::panel_bg_lit(0.0, 0.92)),
                MachinePanelRoot,
                MachinePanelBase {
                    width: panel_w,
                    height: panel_h,
                },
                Name::new("MachineControlPanel"),
            ))
            .with_children(|panel| {
                panel
                    .spawn((
                        Node {
                            width: Val::Percent(100.0),
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        Name::new("MachinePanelHeader"),
                    ))
                    .with_children(|header| {
                        header.spawn((
                            Text::new("CONTROL PANEL"),
                            TextFont {
                                font_size: (heart * 0.62).clamp(11.0, 16.0),
                                ..default()
                            },
                            TextColor(ui_theme::accent_ink()),
                            MachinePanelTitle,
                        ));
                        spawn_panel_close_button(header, heart, MachinePanelCloseButton);
                    });
                panel.spawn((
                    Text::new("Approach a machine"),
                    TextFont {
                        font_size: (heart * 0.54).clamp(10.0, 14.0),
                        ..default()
                    },
                    TextColor(ui_theme::text_muted()),
                    MachinePanelBody,
                ));
            })
            .id();
    });
    panel_entity
}

/// **Las medidas de la hotbar salen del mockup**: casilla de 40 unidades,
/// separación de 4 y 6 de aire abajo, a la escala de la pantalla. Antes eran
/// proporciones propias que no casaban con las imágenes de referencia.
fn hotbar_layout_metrics(layout: &TouchLayout) -> (f32, f32, f32, f32) {
    let k = crate::ui_pack::k(layout);
    let slot = u(INV_SLOT) * k;
    let gap = u(INV_GAP) * k;
    let total = slot * HOTBAR_SLOTS_TOTAL as f32 + gap * (HOTBAR_SLOTS_TOTAL as f32 - 1.0);
    let bottom = layout.insets.bottom as f32 + u(6.0) * k;
    return (slot, gap, total, bottom);
    #[allow(unreachable_code)]
    {
    let short = layout.width.min(layout.height);
    let margin_h = layout.insets.left as f32 + layout.insets.right as f32 + 12.0;
    let usable = (layout.width - margin_h).max(220.0);
    let gap = (short * 0.004).clamp(2.0, 4.0);
    let slot_ideal = (short * 0.068).clamp(34.0, 58.0);
    let total_ideal =
        slot_ideal * HOTBAR_SLOTS_TOTAL as f32 + gap * (HOTBAR_SLOTS_TOTAL as f32 - 1.0);
    let slot = if total_ideal > usable {
        ((usable - gap * (HOTBAR_SLOTS_TOTAL as f32 - 1.0)) / HOTBAR_SLOTS_TOTAL as f32)
            .clamp(30.0, 58.0)
    } else {
        slot_ideal
    };
    let total = slot * HOTBAR_SLOTS_TOTAL as f32 + gap * (HOTBAR_SLOTS_TOTAL as f32 - 1.0);
    let bottom = layout.insets.bottom as f32 + (short * 0.022).clamp(8.0, 18.0);
    (slot, gap, total, bottom)
    }
}

fn spawn_hotbar(parent: &mut ChildBuilder, layout: &TouchLayout, _short: f32, heart: f32) {
    let (slot, gap, total, bottom) = hotbar_layout_metrics(layout);
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(game_ui::ui_center_left(layout, total)),
                bottom: Val::Px(bottom),
                width: Val::Px(total),
                height: Val::Px(slot),
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(gap),
                ..default()
            },
            // **La base oscura de la tira** (28 sep 2026): las casillas flotaban
            // sobre el pasto sin nada detrás. Mismo tono que el marco del panel
            // de la mochila, translúcido para no tapar el mundo.
            BackgroundColor(Color::srgba(0.12, 0.10, 0.14, 0.62)),
            BorderRadius::all(Val::Px(6.0)),
            HudArmado,
            HotbarRoot,
            Name::new("Hotbar"),
        ))
        .with_children(|bar| {
            for i in 0..HOTBAR_SLOTS_TOTAL {
                let is_bag = i == HOTBAR_SLOTS_TOTAL - 1;
                let is_hammer = i == 0;
                let is_item = (1..HOTBAR_SLOTS_TOTAL - 1).contains(&i);
                let enabled = is_bag || is_hammer || is_item;
                let mut e = bar.spawn((
                    Button,
                    Node {
                        width: Val::Px(slot),
                        height: Val::Px(slot),
                        min_width: Val::Px(slot),
                        min_height: Val::Px(slot),
                        flex_shrink: 0.0,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    // **Todas las casillas se ven igual.** En la imagen de
                    // referencia la hotbar es una tira pareja: el martillo no
                    // lleva borde de otro color ni la vacía se ve apagada — se
                    // ve vacía, que es distinto.
                    BackgroundColor(Color::NONE),
                    CasillaDelKit,
                    HotbarSlot { disabled: !enabled },
                ));
                if is_hammer {
                    e.insert(HotbarHammerSlot);
                    e.with_children(|slot_node| {
                        slot_node.spawn((
                            Node {
                                width: Val::Percent(86.0),
                                height: Val::Percent(86.0),
                                ..default()
                            },
                            HotbarHammerIcon,
                        ));
                    });
                } else if is_bag {
                    e.insert(HotbarCraftButton);
                    e.with_children(|slot_node| {
                        slot_node.spawn((
                            Node {
                                width: Val::Percent(86.0),
                                height: Val::Percent(86.0),
                                ..default()
                            },
                            HotbarBagIcon,
                        ));
                    });
                } else if is_item {
                    let idx = i - 1;
                    e.insert(HotbarItemSlot { index: idx });
                    // **El icono CENTRADO en la casilla** (29 sep 2026): iba
                    // arriba en una columna con el rótulo abajo, y la cantidad
                    // era un hermano de flex que lo corría a un costado. Ahora es
                    // lo mismo que la casilla de la mochila: icono al 70 % en el
                    // centro y la cantidad superpuesta abajo a la derecha.
                    e.with_children(|slot_node| {
                        slot_node.spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Percent(15.0),
                                top: Val::Percent(15.0),
                                width: Val::Percent(70.0),
                                height: Val::Percent(70.0),
                                ..default()
                            },
                            HotbarSlotIcon,
                        ));
                        slot_node.spawn((
                            Text::new(""),
                            TextFont { font_size: 0.1, ..default() },
                            TextColor(Color::NONE),
                            HotbarSlotName,
                        ));
                        slot_node
                            .spawn(Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(0.0),
                                top: Val::Percent(58.0),
                                width: Val::Percent(90.0),
                                height: Val::Percent(38.0),
                                justify_content: JustifyContent::FlexEnd,
                                align_items: AlignItems::Center,
                                ..default()
                            })
                            .with_children(|q| {
                                q.spawn((
                                    Text::new(""),
                                    TextFont {
                                        font_size: (heart * 0.34).clamp(7.0, 10.0),
                                        ..default()
                                    },
                                    TextColor(ui_theme::text_primary()),
                                    HotbarSlotQty,
                                ));
                            });
                    });
                }
            }
        });
}

fn spawn_lantern_toggle(parent: &mut ChildBuilder, layout: &TouchLayout, short: f32) {
    let (_, _, total, bottom) = hotbar_layout_metrics(layout);
    let btn_h = (short * 0.052).clamp(28.0, 40.0);
    let btn_w = btn_h * 2.1;
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(game_ui::ui_center_left(layout, total) + total + 8.0),
                bottom: Val::Px(bottom),
                width: Val::Px(btn_w),
                height: Val::Px(btn_h),
                display: Display::None,
                ..default()
            },
            HudArmado,
            LanternToggleRoot,
            Name::new("LanternToggle"),
        ))
        .with_children(|root| {
            root.spawn((
                Button,
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    border: UiRect::all(Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor(ui_theme::accent_cyan()),
                BackgroundColor(ui_theme::button_bg(
                    UiButtonTone::Use,
                    UiButtonState::Normal,
                )),
                LanternToggleButton,
            ))
            .with_children(|btn| {
                btn.spawn((
                    Text::new("LAMP ON"),
                    TextFont {
                        font_size: (btn_h * 0.28).clamp(9.0, 12.0),
                        ..default()
                    },
                    TextColor(ui_theme::text_primary()),
                    LanternToggleLabel,
                ));
            });
        });
}

fn update_hud_layout(
    layout: Res<TouchLayout>,
    mut hotbar: Query<&mut Node, (With<HotbarRoot>, Without<HotbarSlot>)>,
    mut slots: Query<&mut Node, (With<HotbarSlot>, Without<HotbarRoot>)>,
    mut hud_blocks: ResMut<HudTouchBlocks>,
    span: Res<CpuSpanStats>,
    mut last: Local<(f32, f32, f32, f32)>,
) {
    let _guard = span_guard(&span, CpuSpanSlot::Hud);
    if layout.width < 100.0 {
        return;
    }
    let (slot, gap, total, bottom) = hotbar_layout_metrics(&layout);
    let key = (layout.width, layout.height, total, bottom);
    if *last == key {
        return;
    }
    *last = key;
    if let Ok(mut node) = hotbar.get_single_mut() {
        apply_bottom_center(&mut node, &layout, total, bottom);
        node.width = Val::Px(total);
        node.height = Val::Px(slot);
        node.column_gap = Val::Px(gap);
        let left = game_ui::ui_center_left(&layout, total);
        let top = layout.height - bottom - slot;
        // La hotbar abajo y el botón MENU arriba al centro: los dos toques que no
        // puede robarles el mundo.
        hud_blocks.0 = vec![
            [left, top, left + total, top + slot],
            game_ui::menu_button_rect(&layout),
            game_ui::cam_button_rect(&layout),
        ];
    }
    for mut slot_node in &mut slots {
        slot_node.width = Val::Px(slot);
        slot_node.height = Val::Px(slot);
        slot_node.min_width = Val::Px(slot);
        slot_node.min_height = Val::Px(slot);
    }
}

fn update_hud_state(
    player: Query<&SimState, With<Player>>,
    atlas: Res<HudIconAtlas>,
    art: Res<crate::art2d::ArtLibrary>,
    mut stamina: ResMut<StaminaMeter>,
    mut cache: ResMut<HudDisplayCache>,
    mut hearts: Query<(&StatHeartIcon, &mut ImageNode)>,
    span: Res<CpuSpanStats>,
) {
    let _guard = span_guard(&span, CpuSpanSlot::Hud);
    let Ok(sim) = player.get_single() else {
        return;
    };
    if !atlas.ready {
        return;
    }

    stamina.value = sim.body.stamina;
    stamina.max = sim.body.max_stamina;

    let hp_ratio = if sim.max_hp > 0.0 {
        (sim.hp / sim.max_hp).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Diez corazones, como en la imagen de referencia: dos "unidades" cada uno.
    let heart_units = (hp_ratio * 20.0).round() as i32;

    if !cache.initialized || cache.heart_units != heart_units {
        cache.heart_units = heart_units;
        for (icon, mut image) in &mut hearts {
            let units = (heart_units - icon.idx * 2).clamp(0, 2);
            *image = crate::hud::icons::heart_node_for_units(units, &atlas, &art);
        }
    }
    // La energía ya no se dibuja acá: es una barra en `survival_bars.rs`. Lo que
    // sigue vivo de este sistema es `StaminaMeter`, que lo lee el resto del juego.
    cache.initialized = true;
}

/// **El panel de estado sólo habla cuando tiene algo que decir.**
///
/// Antes vivía en pantalla con la chuleta de controles (`<- Move | Look -> |
/// USE...`) porque siempre había una mira apuntando a algo. Sacada la mira, el
/// texto aparece únicamente mientras tocás algo, mientras una acción avanza, o
/// cuando el vuelo tiene telemetría que sí es juego (altura, Ki, cuerpo). La
/// chuleta y lo demás que es de developer queda para el modo CREATIVO.
#[allow(clippy::too_many_arguments)]
fn update_survival_hud(
    equipped: Res<EquippedItem>,
    hold: Res<HoldInteractState>,
    target: Res<LookTarget>,
    region: Res<RegionRuntime>,
    pending: Res<PendingMachinePlacement>,
    inv: Res<PlayerInventory>,
    flight: Res<Flight>,
    editor: Res<crate::ui_editor::UiEditorState>,
    names: Query<&crate::structures::StructureName>,
    props: Query<&crate::harvest::PropArt>,
    ayuda_animal: Res<crate::fauna::AyudaAnimal>,
    contenedores: Query<&crate::chests::MuebleContenedor>,
    mut cache: ResMut<HudDisplayCache>,
    mut prompts: Query<(&mut Text, &mut TextColor), With<InteractPromptText>>,
    mut prompt_roots: Query<(&mut Node, &mut BackgroundColor, &mut BorderColor), With<StatusPromptRoot>>,
) {
    // El nombre sale del objeto colocado o, si es del mundo, del catálogo de
    // slots: el farol de la vereda dice "Farol" y no "objeto".
    let target_name = target
        .entity
        .and_then(|e| {
            names
                .get(e)
                .ok()
                .map(|n| n.0.as_str())
                .or_else(|| props.get(e).ok().map(|p| p.label()))
        });
    let prompt = build_interact_prompt(
        &equipped,
        &hold,
        &target,
        &region,
        &pending,
        &inv,
        &flight,
        editor.creative,
        target_name,
        ayuda_animal.0.as_deref(),
        target
            .entity
            .and_then(|e| contenedores.get(e).ok())
            .map(|m| m.tipo.nombre()),
    );
    let visible = prompt.is_some();
    let prompt_line = prompt.unwrap_or_default();
    if cache.prompt_line != prompt_line {
        cache.prompt_line = prompt_line.clone();
        for (mut text, _) in &mut prompts {
            text.0 = prompt_line.clone();
        }
    }

    let target_alpha = if visible { 0.92 } else { 0.0 };
    if cache.prompt_visible != visible || (cache.prompt_alpha - target_alpha).abs() > 0.02 {
        cache.prompt_visible = visible;
        cache.prompt_alpha = target_alpha;
        for (mut node, mut bg, mut border) in &mut prompt_roots {
            node.display = if visible { Display::Flex } else { Display::None };
            *bg = BackgroundColor(ui_theme::panel_bg(target_alpha * 0.82));
            *border = BorderColor(ui_theme::accent_ink().with_alpha(target_alpha));
        }
        for (_, mut color) in &mut prompts {
            *color = TextColor(ui_theme::text_primary().with_alpha(0.95));
        }
    }
}

fn machine_panel_system(
    layout: Res<TouchLayout>,
    tune: Res<HudTune>,
    root: Query<Entity, With<GameUiRoot>>,
    player: Query<&Transform, With<Player>>,
    inv: Res<PlayerInventory>,
    machines: Res<MachineWorld>,
    mut cache: ResMut<HudDisplayCache>,
    mut panel_state: ResMut<MachinePanelState>,
    mut commands: Commands,
    mut panel_body: Query<&mut Text, With<MachinePanelBody>>,
    mut close_btn: Query<
        (&Interaction, &mut BackgroundColor),
        (With<MachinePanelCloseButton>, With<Button>, Changed<Interaction>),
    >,
) {
    let player_pos = player.get_single().ok().map(|tf| tf.translation);
    let machine_panel = build_machine_panel_text(player_pos, &inv, &machines);
    let near_machine = machine_panel.is_some();
    if !near_machine {
        cache.machine_panel_dismissed = false;
    }
    let panel_visible = near_machine && !cache.machine_panel_dismissed;

    if !panel_visible {
        if let Some(entity) = panel_state.panel.take() {
            commands.entity(entity).despawn_recursive();
        }
        cache.machine_panel_visible = false;
        return;
    }

    if panel_state.panel.is_none() {
        let Ok(root) = root.get_single() else {
            return;
        };
        panel_state.panel = Some(spawn_machine_panel(
            &mut commands,
            root,
            &layout,
            tune.heart_px,
        ));
        cache.machine_panel_visible = true;
    }

    if let Some(line) = machine_panel {
        if cache.machine_panel_line != line {
            cache.machine_panel_line = line.clone();
            for mut text in &mut panel_body {
                text.0 = line.clone();
            }
        }
    }

    for (interaction, mut bg) in &mut close_btn {
        if *interaction == Interaction::Pressed {
            cache.machine_panel_dismissed = true;
        }
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Primary,
            ui_theme::interaction_state(*interaction, false),
        ));
    }
}

fn update_floating_panels_layout(
    layout: Res<TouchLayout>,
    mut machine: Query<(&mut Node, &MachinePanelBase), With<MachinePanelRoot>>,
) {
    if layout.width < 100.0 {
        return;
    }
    if let Ok((mut node, base)) = machine.get_single_mut() {
        game_ui::apply_center(&mut node, &layout, base.width, base.height);
    }
}

const MACHINE_PANEL_RANGE: f32 = 4.2;

fn format_inventory_line(inv: &PlayerInventory) -> String {
    let entries = inv.sorted_entries();
    if entries.is_empty() {
        return "Stock: (empty)".to_string();
    }
    let parts: Vec<String> = entries
        .iter()
        .map(|(id, qty)| format!("{} x{qty}", item_label(id)))
        .collect();
    format!("Stock: {}", parts.join(" | "))
}

/// Nombre CORTO de un material para la línea de costo de una tarjeta. Los ids
/// largos (`iron_ingot`, `gold_ingot`) hacían que el costo ocupara tres líneas y
/// se derramara fuera de la tarjeta; con símbolos de una o dos letras el costo
/// entero entra en un renglón.
/// **El nombre corto de un ingrediente, en castellano** (28 sep 2026). Eran
/// iniciales del inglés —`W`, `S`, `T`— y la tarjeta decía "Wx3 Tx2": la regla es
/// que la UI habla castellano de verdad. Corto porque va al pie de la tarjeta.
fn item_label(id: &str) -> &str {
    match id {
        "wood" => "Madera",
        "stone" => "Piedra",
        "sticks" => "Palos",
        "iron_ingot" => "Hierro",
        "gold_ingot" => "Oro",
        "gem_blue" => "Gema",
        "leather" => "Cuero",
        "apple" => "Manzana",
        "orange" => "Naranja",
        "grape" => "Uvas",
        "map_small" => "Mapa",
        _ => ha_assets::slots::slot_label(&format!("items/{id}")).unwrap_or(id),
    }
}

/// El nombre de lo que sale de una receta, en castellano: el del slot del ítem
/// (`ha_assets::slots`), que es el que ya usa el resto del juego. El
/// `display_name` de la receta es el del catálogo compartido (inglés, espejo de
/// `packages/shared`) y no se toca desde acá.
fn nombre_de_receta(recipe: &CraftRecipe) -> &'static str {
    ha_assets::slots::slot_label(&format!("items/{}", recipe.output_id)).unwrap_or(recipe.display_name)
}

fn format_ingredients(ingredients: &[(&str, u32)]) -> String {
    ingredients
        .iter()
        .map(|(id, qty)| format!("{} x{qty}", item_label(id)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn recipe_status(inv: &PlayerInventory, recipe: &CraftRecipe) -> (bool, String) {
    if can_craft(inv, recipe) {
        return (true, format_ingredients(recipe.ingredients));
    }
    let missing: Vec<String> = recipe
        .ingredients
        .iter()
        .filter_map(|(id, need)| {
            let have = inv.qty(id);
            if have < *need {
                Some(format!("{} x{}", item_label(id), need - have))
            } else {
                None
            }
        })
        .collect();
    (false, format!("falta {}", missing.join(" ")))
}

/// Arrastre táctil + rueda del mouse en las listas de la mochila. Las tres
/// pestañas tienen `Overflow::scroll_y()` + `ScrollPosition` pero ningún sistema
/// que las mueva: sin esto, lo que quede debajo del pliegue no existe en el
/// teléfono. Con la mochila abierta el gameplay está bloqueado, así que
/// cualquier arrastre scrollea.
///
/// **El tope sale de medir el contenido, no de un número.** Antes era
/// `clamp(0.0, 900.0)` — un valor elegido cuando la lista de CONSTRUIR eran doce
/// recetas. Con el pack importado esa lista pasó a ~350 tarjetas y **todo lo que
/// caía después de los primeros 900 px quedó inalcanzable**: los objetos estaban
/// declarados, con su nombre y su icono, y el menú simplemente no dejaba llegar.
/// Es el bug de "agregaste los items y no los veo en construir".
fn scroll_craft_list(
    touches: Res<Touches>,
    mut wheel: EventReader<bevy::input::mouse::MouseWheel>,
    mut views: Query<(&mut ScrollPosition, &ComputedNode, &Children), With<BagScrollView>>,
    contents: Query<&ComputedNode, With<ScrollContent>>,
    mut bag: ResMut<BagPanelState>,
) {
    if !bag.open {
        wheel.clear();
        return;
    }
    let mut dy = 0.0f32;
    for ev in wheel.read() {
        dy -= ev.y * 26.0;
    }
    let mut hay_dedo = false;
    for t in touches.iter() {
        hay_dedo = true;
        dy -= t.delta().y;
        bag.recorrido += t.delta().length();
    }
    // El recorrido NO se limpia al soltar: `bag_panel_system` lo necesita justo
    // en ese frame para decidir si fue toque o scroll. Lo pone en cero el
    // proximo apoyo (ver `pendiente`).
    bag.dedo = hay_dedo;
    if dy.abs() < 0.5 {
        return;
    }
    for (mut sp, view, children) in &mut views {
        // `ComputedNode::size()` es en px FÍSICOS y `ScrollPosition` en lógicos:
        // sin el factor, en un teléfono de alta densidad el tope sale 2-3 veces
        // más grande y la lista se va de largo hasta quedar en blanco.
        let k = view.inverse_scale_factor();
        let view_h = view.size().y * k;
        let content_h = children
            .iter()
            .filter_map(|c| contents.get(*c).ok())
            .map(|c| c.size().y * c.inverse_scale_factor())
            .fold(0.0f32, f32::max);
        let tope = max_scroll(view_h, content_h);
        sp.offset_y = (sp.offset_y + dy).clamp(0.0, tope);
    }
}

/// Cuánto se puede bajar una lista: lo que sobresale del marco, y nada más.
/// Cero cuando el contenido entra entero — si no, la lista se puede arrastrar
/// hasta dejar la pantalla en blanco.
pub(crate) fn max_scroll(view_h: f32, content_h: f32) -> f32 {
    (content_h - view_h).max(0.0)
}

fn build_machine_panel_text(
    player_pos: Option<Vec3>,
    inv: &PlayerInventory,
    world: &MachineWorld,
) -> Option<String> {
    let pos = player_pos?;
    let mut nearby: Vec<_> = world
        .placed
        .iter()
        .filter(|m| {
            let dx = m.x - pos.x;
            let dz = m.z - pos.z;
            (dx * dx + dz * dz).sqrt() <= MACHINE_PANEL_RANGE
        })
        .collect();
    if nearby.is_empty() {
        return None;
    }
    nearby.sort_by(|a, b| {
        let da = (a.x - pos.x).powi(2) + (a.z - pos.z).powi(2);
        let db = (b.x - pos.x).powi(2) + (b.z - pos.z).powi(2);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut lines = vec![format!(
        "Your stock: W{} S{} T{}",
        inv.qty("wood"),
        inv.qty("stone"),
        inv.qty("sticks")
    )];
    for m in nearby.iter().take(3) {
        let def = require_machine(m.machine_id);
        lines.push(format!(
            "{} - stored {} {} ({}/s)",
            def.display_name,
            m.stored_output,
            def.output_item_id,
            def.output_per_second
        ));
    }
    Some(lines.join("\n"))
}

fn build_interact_prompt(
    equipped: &EquippedItem,
    hold: &HoldInteractState,
    target: &LookTarget,
    region: &RegionRuntime,
    pending: &PendingMachinePlacement,
    inv: &PlayerInventory,
    flight: &Flight,
    creative: bool,
    // Nombre del objeto colocado bajo el dedo, si lo hay.
    target_name: Option<&str>,
    // Qué se puede hacer con el animal apuntado (`fauna::rotular_animal_apuntado`).
    ayuda_animal: Option<&str>,
    // El mueble apuntado se abre (ropero, heladera…): su nombre.
    mueble_que_se_abre: Option<&str>,
) -> Option<String> {
    if flight.active {
        let alt = flight.altitude_m();
        let spd = flight.speed();
        let v_vert = flight.vertical_speed();
        let v_horiz = flight.horizontal_speed();
        let ki = flight.ki_pct();
        let body = flight.body.label();
        let vac = if flight.in_vacuum() { "VAC" } else { "ATM" };
        let charge = if flight.charging { " - CARGANDO" } else { "" };
        // NOTE: la fuente del juego es ASCII-only (no |, flechas, acentos) -> separadores ASCII.
        // Esto SÍ es juego: sin altura ni Ki no se puede pilotar. Lo que era
        // chuleta de botones se fue con el resto (JUMP/BAJAR están en pantalla).
        return Some(format!(
            "VUELO KI - {body} - alt {alt:.0}m - vert {v_vert:+.0} horiz {v_horiz:.0} m/s - {spd:.0} tot - Ki {ki:.0}% - {vac}{charge}",
        ));
    }

    if hold.active && hold.placing_machine {
        let def = require_machine(pending.machine_id);
        let pct = if hold.duration_ms > 0 {
            (hold.progress_ms as f32 / hold.duration_ms as f32 * 100.0) as u32
        } else {
            0
        };
        return Some(format!("Colocando {} - {}%", def.display_name, pct));
    }

    if hold.active && hold.eating {
        let pct = if hold.duration_ms > 0 {
            (hold.progress_ms as f32 / hold.duration_ms as f32 * 100.0) as u32
        } else {
            0
        };
        return Some(format!(
            "Comiendo {} - {}%",
            ha_shared::item_display_name(&equipped.item_id),
            pct
        ));
    }
    if target.kind.is_none()
        && !hold.active
        && ha_shared::food_restore(&equipped.item_id).is_some()
        && inv.qty(&equipped.item_id) > 0
    {
        return Some(format!(
            "Manten para comer {}",
            ha_shared::item_display_name(&equipped.item_id)
        ));
    }

    if let Some(m) = mueble_que_se_abre {
        return Some(format!("Tocá para abrir: {}", m.to_lowercase()));
    }
    if target.kind.is_none() && target.place_cell.is_some() {
        if let Some(t) = crate::cubo::ayuda(&equipped.item_id) {
            return Some(t.to_string());
        }
    }
    // — Granja —
    if hold.active && hold.placing_crop {
        return Some("Sembrando…".to_string());
    }
    if target.kind.is_none() && target.place_cell.is_some() {
        if let Some(p) = crate::granja::Planta::de_semilla(&equipped.item_id) {
            return Some(format!("Mantené para sembrar {}", p.nombre()));
        }
    }
    if target.kind == Some(InteractTargetKind::Crop) {
        let nombre = target_name.unwrap_or("el cultivo");
        return Some(if nombre.contains("listo") {
            format!("Mantené para cosechar — {nombre}")
        } else {
            format!("{nombre} — mantené para arrancarlo")
        });
    }
    if target.kind == Some(InteractTargetKind::Animal) {
        if let Some(a) = ayuda_animal {
            return Some(a.to_string());
        }
    }

    if let Some(kind) = target.kind {
        // Objeto colocado por el jugador: se levanta con el mismo gesto que se
        // pica un bloque. El nombre sale de su propio slot de arte, así que la
        // heladera dice "Heladera" y no "objeto".
        // Lo mismo vale para el prop que sembró el mundo: el gesto es idéntico,
        // así que el cartel también.
        if kind == InteractTargetKind::Entity || kind == InteractTargetKind::Prop {
            return Some(match target_name {
                Some(name) => format!("Manten para levantar {name}"),
                None => "Manten para levantar".to_string(),
            });
        }
        if kind == InteractTargetKind::Block {
            if ha_shared::is_block_item(&equipped.item_id) {
                return Some(format!(
                    "Manten para colocar {}",
                    ha_shared::item_display_name(&equipped.item_id)
                ));
            }
            // Cavar no es picar: lo que sale de acá es un POZO por el que se baja,
            // y decirlo con la palabra justa es la única pista que tiene el jugador.
            if target.dig_down {
                return Some("Manten para cavar".to_string());
            }
            return Some("Manten para picar".to_string());
        }
        if let Some(rule) = resolve_interaction(Some(equipped.item_id.as_str()), kind) {
            return Some(match rule.kind {
                InteractActionKind::Gather => {
                    format!("Manten para recoger {}", rule.item_id.unwrap_or("recurso"))
                }
                InteractActionKind::Mine => {
                    format!("Manten para picar {}", rule.item_id.unwrap_or("piedra"))
                }
                InteractActionKind::Collect => "Manten para recoger".to_string(),
                InteractActionKind::Assemble if kind == InteractTargetKind::SpaceStationModule => {
                    "Manten para instalar el modulo".to_string()
                }
                InteractActionKind::Assemble => "Manten para instalar el modulo de la nave".to_string(),
                InteractActionKind::Launch => "Manten para DESPEGAR a orbita".to_string(),
                InteractActionKind::ReturnEarth if kind == InteractTargetKind::LunarReturn => {
                    "Manten para volver al hub orbital".to_string()
                }
                InteractActionKind::ReturnEarth => "Manten para reentrar a la Tierra".to_string(),
                InteractActionKind::TravelLuna => {
                    // Earth->Moon distance is effectively fixed (~52 km flight
                    // metres) - travel-time preview without needing the clock.
                    let secs = ha_shared::transit_cruise_secs(52_000.0);
                    format!("Manten para viajar a la Luna - ~{secs:.0}s - requiere datos de nav")
                }
                InteractActionKind::TravelBody => {
                    "Manten para viajar al planeta - requiere datos de nav".to_string()
                }
                InteractActionKind::LaunchProbe => {
                    if inv.qty("probe") > 0 {
                        "Manten para lanzar la sonda".to_string()
                    } else {
                        "Falta una sonda - MOCHILA -> FABRICAR".to_string()
                    }
                }
                InteractActionKind::Attack => "Toca para golpear".to_string(),
                InteractActionKind::None => match kind {
                    InteractTargetKind::Rock
                    | InteractTargetKind::OreIron
                    | InteractTargetKind::OreGold => {
                        "Necesitas un pico - MOCHILA -> FABRICAR".to_string()
                    }
                    _ => return None,
                },
            });
        }
    }

    if ha_shared::is_block_item(&equipped.item_id) && target.place_cell.is_some() {
        return Some(format!(
            "Manten para colocar {}",
            ha_shared::item_display_name(&equipped.item_id)
        ));
    }

    // El martillo sí informa: el coste de la máquina es una decisión de juego, y
    // sin verlo no sabés si te alcanza. Sólo cuando lo tenés en la mano.
    if equipped.item_id == "hammer" {
        let def = require_machine(pending.machine_id);
        if inv.can_afford_machine(def) {
            return Some(format!(
                "Manten en el suelo para armar {} - M{} P{} R{}",
                def.display_name, def.cost_wood, def.cost_stone, def.cost_sticks
            ));
        }
        return Some(format!(
            "Faltan M{} P{} R{} para {}",
            def.cost_wood, def.cost_stone, def.cost_sticks, def.display_name
        ));
    }

    if region.active == RegionId::Space {
        return Some("Vacio orbital - vuela con Ki hacia Tierra/Luna".to_string());
    }
    if region.active == RegionId::Luna {
        return Some("Superficie lunar - manten SALTAR para cargar Ki y despegar".to_string());
    }

    // Chuleta de controles: es documentación, no juego. Sólo en CREATIVO.
    creative.then(|| platform::gameplay_controls_hint().to_string())
}

fn spawn_interact_hud(parent: &mut ChildBuilder, layout: &TouchLayout, short: f32) {
    let ring = (short * 0.055).clamp(28.0, 44.0);
    parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(layout.width * 0.58),
            top: Val::Px(layout.insets.top as f32 + (short * 0.22).clamp(72.0, 120.0)),
            width: Val::Px(ring),
            height: Val::Px(ring),
            border: UiRect::all(Val::Px(ui_theme::panel_border_width())),
            // Hidden until an ability is actually on cooldown (the update
            // system reveals it) - it was showing as a stray magenta square.
            display: Display::None,
            ..default()
        },
        BorderColor(ui_theme::accent_magenta()),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.35)),
        AbilityCooldownRing,
        Name::new("AbilityCooldownRing"),
    ));
}

fn spawn_panel_close_button(parent: &mut ChildBuilder, heart: f32, marker: impl Component) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px((heart * 1.1).clamp(24.0, 32.0)),
                height: Val::Px((heart * 1.1).clamp(24.0, 32.0)),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            // Redondo: es el botón que más se toca de un panel y una X en un
            // cuadradito se lee como diálogo de sistema.
            BorderRadius::MAX,
            BorderColor(ui_theme::button_border(UiButtonTone::Primary)),
            BackgroundColor(ui_theme::button_bg(UiButtonTone::Primary, UiButtonState::Normal)),
            marker,
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("X"),
                TextFont {
                    font_size: (heart * 0.62).clamp(11.0, 16.0),
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
            ));
        });
}

fn spawn_bag_tab_button(parent: &mut ChildBuilder, label: &str, tab: BagTab, heart: f32) {
    parent
        .spawn((
            Button,
            Node {
                flex_grow: 1.0,
                // Alto de dedo: 22 px era una pestaña que se erraba. La barra de
                // pestañas es lo primero que se toca al abrir la mochila.
                height: Val::Px((heart * 1.35).clamp(30.0, 40.0)),
                border: UiRect::all(Val::Px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            // Píldora: las tres pestañas son una sola barra de conmutadores, no
            // tres cajitas.
            BorderRadius::MAX,
            BorderColor(ui_theme::button_border(UiButtonTone::Inventory)),
            BackgroundColor(ui_theme::button_bg(UiButtonTone::Inventory, UiButtonState::Normal)),
            BagTabButton { tab },
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new(label),
                TextFont {
                    font_size: (heart * 0.52).clamp(10.0, 14.0),
                    ..default()
                },
                TextColor(ui_theme::text_primary()),
            ));
        });
}

/// **La ventana de la mochila ocupa la pantalla**, dejando sólo un borde entre
/// ella y el límite del juego. Antes era un panelito de 340-540 px centrado: en el
/// teléfono quedaba una tarjeta chica con listas de texto que había que scrollear
/// para todo, y el minimapa del menú no se veía. Con el área completa entran la
/// grilla de 6 columnas y las tarjetas con icono.
///
/// El botón MENU **no vive acá**: volvió a su sitio de siempre, arriba al centro
/// del HUD (ver [`spawn_menu_button`]). El header sólo cierra la ventana.
/// Una unidad del mockup del inventario en píxeles de ese mockup: la maqueta se
/// compuso en unidades de pixel-art de 800 × 360 y se dibujó a escala 3.
const fn u(x: f32) -> f32 {
    x * 3.0
}

// Las medidas de `mock_inv.py`, en unidades de la maqueta.
const INV_SLOT: f32 = 40.0;
const INV_GAP: f32 = 4.0;
const INV_PITCH: f32 = INV_SLOT + INV_GAP;
const INV_BORDE: f32 = 11.0;
const INV_PAD: f32 = 8.0;
const INV_COL_PERS: f32 = 152.0;
const INV_TABS_H: f32 = 36.0;
const INV_HEAD_H: f32 = 26.0;
const INV_SCROLL_W: f32 = 6.0;
/// Columnas de la grilla de la mochila: las de la barra rápida, así la primera
/// fila es la barra y las de abajo la mochila, alineadas.
const BAG_COLS: usize = HOTBAR_ITEM_SLOTS;
/// Filas que tienen que entrar sin scroll: la barra más la grilla.
const BAG_FILAS: usize = 1 + (GRID_COLS * GRID_ROWS).div_ceil(BAG_COLS);

/// El icono de lo que tenés en la mano, en la columna del personaje.
#[derive(Component)]
struct IconoManoMochila;

/// El hueco donde va el muñeco de la columna de PERSONAJE. Lo llena
/// [`pintar_personaje_mochila`], que es un sistema aparte porque
/// `bag_panel_system` ya está en el tope de parámetros que acepta Bevy.
#[derive(Component)]
pub struct HuecoPersonaje;

/// La canaleta de la grilla del inventario: dónde empieza y cuánto mide, ya en
/// píxeles de pantalla.
#[derive(Component)]
struct PulgarInventario {
    y0: f32,
    alto: f32,
}

/// **La mochila / el inventario creativo, tal cual las imágenes de referencia.**
///
/// El panel del pack centrado arriba, la columna del personaje a la izquierda
/// con su equipo y su estado, el encabezado con el buscador y la grilla de
/// `GRID_COLS` × `GRID_ROWS` con su canaleta. La hotbar queda AFUERA, siempre a la vista: es la decisión
/// del 15 ago 2026 y es lo que dibujan las dos maquetas.
///
/// En creativo la fila de pestañas son las dieciséis categorías del catálogo
/// (una por ficha, con un objeto de esa categoría como icono); en supervivencia
/// son tres: MOCHILA, FABRICAR y CONSTRUIR.
#[allow(clippy::too_many_arguments)]
fn spawn_bag_panel(
    commands: &mut Commands,
    root: Entity,
    layout: &TouchLayout,
    tune: &HudTune,
    art: &crate::art2d::ArtLibrary,
    tab: BagTab,
    creativo: bool,
    group: usize,
    page: usize,
) -> Entity {
    use crate::ui_pack::{self as up, Ancla, Pack};
    let heart = tune.heart_px;
    let BagMetrics { card, .. } = bag_metrics(layout, heart);
    // El mockup es 2400 de ancho; `k` lo lleva a esta pantalla.
    let k = up::k(layout);
    // **En creativo la fila de pestañas son las categorías del catálogo.** Es lo
    // que dibujan las dos imágenes de referencia: dieciséis fichas en creativo,
    // y MOCHILA / FABRICAR / CONSTRUIR en supervivencia. Antes lo decidía la
    // pestaña activa, así que entrar a CONSTRUIR en supervivencia te dejaba sin
    // forma de volver a la mochila.
    let tab = if creativo { BagTab::Build } else { tab };

    // Geometría, en unidades de la maqueta (800 × 360).
    //
    // **Todo entra en la pantalla** (29 sep 2026). Con la casilla fija en 40 el
    // panel medía 372 de alto en un lienzo de 360: la última fila de la mochila
    // se dibujaba ENCIMA del marco y el panel se salía por arriba y por abajo
    // ("los ítems están montados sobre el marco"). Ahora la casilla se calcula
    // para que las filas de la mochila —la barra rápida primero, después la
    // grilla— entren enteras sin scroll.
    let cols = BAG_COLS;
    let pyu = 4.0;
    let fijo = 2.0 * INV_BORDE + 2.0 * INV_PAD + INV_TABS_H + 6.0 + INV_HEAD_H + 6.0;
    let alto_libre = up::ALTO / 3.0 - 2.0 * pyu - fijo;
    let filas = BAG_FILAS as f32;
    let slot_u = ((alto_libre + INV_GAP) / filas - INV_GAP).floor().min(INV_SLOT);
    let pitch_u = slot_u + INV_GAP;
    let alto_grilla = filas * pitch_u - INV_GAP;
    let grid_w = cols as f32 * pitch_u - INV_GAP;
    let inner_w = INV_COL_PERS + 14.0 + grid_w + 4.0 + INV_SCROLL_W;
    let panel_w = inner_w + 2.0 * INV_PAD + 2.0 * INV_BORDE;
    let inner_h = INV_TABS_H + 6.0 + INV_HEAD_H + 6.0 + alto_grilla;
    let panel_h = inner_h + 2.0 * INV_PAD + 2.0 * INV_BORDE;
    let pxu = (800.0 - panel_w) / 2.0;
    let ix = pxu + INV_BORDE + INV_PAD;
    let iy = pyu + INV_BORDE + INV_PAD;

    let mut modal_entity = Entity::PLACEHOLDER;
    commands.entity(root).with_children(|parent| {
        modal_entity = parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Px(layout.width),
                    height: Val::Px(layout.height),
                    ..default()
                },
                ModalLayerRoot,
                // **Por encima de TODO el HUD** (28 sep 2026): el reloj y el
                // botón CAM se dibujaban encima de la mochila y la atravesaban.
                GlobalZIndex(50),
                Name::new("BagModalLayer"),
            ))
            .with_children(|layer| {
                layer.spawn((
                    Button,
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    // Transparente: sólo existe para capturar el toque de
                    // "cerrar tocando afuera". `SinMarco` porque el reskin del
                    // HUD le pintaba `ui/button` estirado a pantalla completa —
                    // una tabla de madera gigante tapando el juego y la hotbar.
                    // Oscurece el juego detrás: la mochila es un modal y el
                    // HUD que queda debajo no tiene que competir con ella.
                    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
                    crate::ui_pack::SinMarco,
                    ModalBackdrop,
                    Name::new("ModalBackdrop"),
                ));

                layer.spawn(up::lienzo(k)).with_children(|l| {
                    // El panel. `BagPanelRoot` lo usa la animación de apertura.
                    let panel = up::caja(u(pxu), u(pyu), u(panel_w), u(panel_h), k);
                    let base = BagPanelBase { width: u(panel_w) * k, height: u(panel_h) * k };
                    match art.ui_frame_node(up::marco::PANEL) {
                        Some(img) => {
                            l.spawn((panel, img, BagPanelRoot, base, Name::new("BagPanel")));
                        }
                        None => {
                            l.spawn((
                                panel,
                                BackgroundColor(Color::srgb(0.796, 0.706, 0.580)),
                                BagPanelRoot,
                                base,
                                Name::new("BagPanel"),
                            ));
                        }
                    }

                    // ── la fila de pestañas ───────────────────────────────
                    let tabs: Vec<(String, Option<String>, usize)> = if creativo {
                        build_groups()
                            .into_iter()
                            .enumerate()
                            .map(|(i, (g, label))| (label.to_string(), arte_de_ficha(g), i))
                            .collect()
                    } else {
                        vec![
                            ("MOCHILA".to_string(), Some("ui/ic_mochila".to_string()), 0),
                            ("FABRICAR".to_string(), Some("items/stone_axe".to_string()), 1),
                            ("CONSTRUIR".to_string(), Some("wall/stone".to_string()), 2),
                        ]
                    };
                    let tab_w = if creativo {
                        (inner_w - (tabs.len() as f32 - 1.0) * 3.0) / tabs.len() as f32
                    } else {
                        118.0
                    };
                    for (i, (nombre, clave, idx)) in tabs.iter().enumerate() {
                        let tx = ix + i as f32 * (tab_w + 3.0);
                        let activa = if creativo { *idx == group } else { i == tab as usize };
                        let marco = if activa { up::marco::SLOT } else { up::marco::SLOT_CHICO };
                        let mut ec = up::marco(l, art, marco, u(tx), u(iy), u(tab_w),
                                               u(INV_TABS_H), k);
                        if creativo {
                            ec.insert((Button, BuildFilterChip(*idx)));
                        } else {
                            ec.insert((
                                Button,
                                BagTabButton {
                                    tab: match i {
                                        0 => BagTab::Inventory,
                                        1 => BagTab::Craft,
                                        _ => BagTab::Build,
                                    },
                                },
                            ));
                        }
                        let (cx, cy) = if creativo {
                            (tx + tab_w / 2.0, iy + INV_TABS_H / 2.0)
                        } else {
                            (tx + 20.0, iy + INV_TABS_H / 2.0)
                        };
                        if let Some(clave) = clave {
                            up::icono(l, art, clave, u(cx - 12.0), u(cy - 12.0), u(24.0), k);
                        }
                        if !creativo {
                            up::texto(l, nombre.clone(), 32.0, Pack::TINTA, u(tx + 40.0),
                                      u(cy) + 3.0, Ancla::Izq, k);
                        }
                        if activa {
                            up::elegido(l, u(tx), u(iy), u(tab_w), u(INV_TABS_H), 6.0, k);
                        }
                    }

                    // ── la columna del personaje ──────────────────────────
                    let cy0 = iy + INV_TABS_H + 6.0;
                    let alto_col = INV_HEAD_H + 6.0 + alto_grilla;
                    let alto_pers = alto_col - 66.0;
                    up::marco(l, art, up::marco::LISO, u(ix), u(cy0), u(INV_COL_PERS),
                              u(alto_pers), k);
                    // El título va ADENTRO del marco, debajo de la moldura (antes
                    // se montaba sobre el borde de arriba).
                    up::texto(l, "PERSONAJE", 28.0, Pack::TINTA, u(ix + INV_COL_PERS / 2.0),
                              u(cy0 + 14.0), Ancla::Medio, k);
                    // **El muñeco, grande y centrado.** Los cuatro huecos de equipo
                    // del mockup (MANO, ESCUDO, ESPALDA, LUZ) no hacían nada —no
                    // había sistema de equipo detrás— y se leían como algo roto.
                    // Queda lo que sí existe: lo que tenés en la mano.
                    let alto_muneco = (alto_pers - 30.0 - slot_u - 18.0).max(40.0);
                    let ancho_muneco = alto_muneco / 2.0;
                    l.spawn((
                        up::caja(u(ix + (INV_COL_PERS - ancho_muneco) / 2.0), u(cy0 + 24.0),
                                 u(ancho_muneco), u(alto_muneco), k),
                        HuecoPersonaje,
                        Name::new("HuecoPersonaje"),
                    ));
                    let mx = ix + (INV_COL_PERS - slot_u) / 2.0;
                    let my = cy0 + alto_pers - slot_u - 16.0;
                    up::marco(l, art, up::marco::SLOT, u(mx), u(my), u(slot_u), u(slot_u), k);
                    l.spawn((
                        up::caja(u(mx + slot_u * 0.15), u(my + slot_u * 0.15), u(slot_u * 0.7),
                                 u(slot_u * 0.7), k),
                        ImageNode::default(),
                        IconoManoMochila,
                        Name::new("ManoMochila"),
                    ));
                    up::texto(l, "EN LA MANO", 20.0, Pack::TINTA, u(ix + INV_COL_PERS / 2.0),
                              u(my + slot_u) + 14.0, Ancla::Medio, k);

                    // ── el estado ─────────────────────────────────────────
                    let sy = cy0 + alto_pers + 6.0;
                    up::marco(l, art, up::marco::LISO, u(ix), u(sy), u(INV_COL_PERS),
                              u(60.0), k);
                    for i in 0..5 {
                        let clave = if i < 4 { "ui/heart_full" } else { "ui/heart_empty" };
                        up::icono(l, art, clave, u(ix + 8.0) + i as f32 * u(15.0),
                                  u(sy + 5.0), u(14.0), k);
                    }
                    for (j, (et, color)) in [
                        ("HAMBRE", Color::srgb(0.776, 0.573, 0.267)),
                        ("SED", Color::srgb(0.337, 0.525, 0.808)),
                        ("KI", Color::srgb(0.431, 0.784, 0.863)),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let by = sy + 22.0 + j as f32 * 13.0;
                        up::texto(l, et, 22.0, Pack::TINTA, u(ix + 8.0), u(by) + 11.0,
                                  Ancla::Izq, k);
                        up::marco(l, art, up::marco::HUECO, u(ix + 56.0), u(by),
                                  u(INV_COL_PERS - 64.0), u(8.0), k);
                        // El carril y, adentro, el relleno: el relleno va en
                        // PORCENTAJE para que `barras_de_la_mochila` lo mueva.
                        l.spawn(up::caja(u(ix + 56.0) + 6.0, u(by) + 6.0,
                                         u(INV_COL_PERS - 64.0) - 12.0, u(8.0) - 12.0, k))
                            .with_children(|carril| {
                                carril.spawn((
                                    Node {
                                        width: Val::Percent(100.0),
                                        height: Val::Percent(100.0),
                                        ..default()
                                    },
                                    BackgroundColor(color),
                                    BarraEstadoMochila(j),
                                ));
                            });
                    }

                    // ── el encabezado del catálogo ────────────────────────
                    let gx = ix + INV_COL_PERS + 14.0;
                    let hy = cy0;
                    let titulo = if creativo {
                        build_groups().get(group).map(|(_, l)| *l).unwrap_or("TODO")
                    } else {
                        match tab {
                            BagTab::Inventory => "MOCHILA",
                            BagTab::Craft => "FABRICAR",
                            BagTab::Build => "CONSTRUIR",
                        }
                    };
                    // **El título y su bajada van en una FILA.** Calcular dónde
                    // termina el título multiplicando letras por un ancho medio
                    // los superponía: el ancho de un texto lo sabe el layout,
                    // no una cuenta.
                    let sub = if creativo {
                        "modo creativo · todo a mano"
                    } else {
                        "lo que llevás encima"
                    };
                    l.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(u(gx) * k),
                            top: Val::Px((u(hy + INV_HEAD_H / 2.0) - 26.0) * k),
                            height: Val::Px(52.0 * k),
                            align_items: AlignItems::Center,
                            column_gap: Val::Px(18.0 * k),
                            ..default()
                        },
                        ZIndex(3),
                        Name::new("BagTitulo"),
                    ))
                    .with_children(|t| {
                        t.spawn((
                            Text::new(titulo.to_string()),
                            // 32 y no 40: "CONSTRUIR" a 40 se metía debajo de la
                            // lupa del buscador (captura `HA_BAGSHOT`).
                            TextFont { font_size: 32.0 * k, ..default() },
                            TextColor(Pack::TINTA),
                            TextLayout::new_with_no_wrap(),
                        ));
                        // La bajada ("lo que llevás encima") quedaba debajo
                        // del buscador, dos textos encimados (28 sep 2026). El
                        // título solo alcanza.
                        let _ = sub;
                    });
                    // Las flechas de página, a la izquierda del buscador. Sólo
                    // aparecen si la categoría no entra en una página.
                    if creativo {
                        let paginas = build_groups()
                            .get(group)
                            .map(|(g, _)| build_group_pages(*g))
                            .unwrap_or(1);
                        if paginas > 1 {
                            for (j, (ic, d)) in
                                [("ui/ic_flecha_izq", -1i32), ("ui/ic_flecha_der", 1)]
                                    .into_iter()
                                    .enumerate()
                            {
                                let px_ = gx + grid_w - 200.0 - 80.0 + j as f32 * 38.0;
                                up::marco(l, art, up::marco::SLOT_CHICO, u(px_ - 34.0), u(hy),
                                          u(30.0), u(INV_HEAD_H), k)
                                    .insert((Button, BuildPageBtn(d)));
                                up::icono(l, art, ic, u(px_ - 34.0) + u(3.0), u(hy) + u(2.0),
                                          u(22.0), k);
                            }
                        }
                    }
                    let buscar_w = 200.0;
                    let bx = gx + grid_w - buscar_w;
                    up::marco(l, art, up::marco::BOTON_ANCHO, u(bx), u(hy), u(buscar_w),
                              u(INV_HEAD_H), k);
                    up::icono(l, art, "ui/ic_lupa", u(bx) + 20.0, u(hy) + 15.0, u(13.0), k);
                    up::texto(l, "Buscar…", 32.0, Color::srgb(0.471, 0.392, 0.337),
                              u(bx) + 100.0, u(hy + INV_HEAD_H / 2.0) + 3.0, Ancla::Izq, k);

                    // ── cerrar y ajustes ──────────────────────────────────
                    up::marco(l, art, up::marco::SLOT_CHICO,
                              u(pxu + panel_w - INV_BORDE - 34.0), u(pyu - 4.0), u(30.0), u(30.0), k)
                        .insert((Button, BagPanelCloseButton));
                    up::icono(l, art, "ui/ic_equis",
                              u(pxu + panel_w - INV_BORDE - 34.0) + u(3.0),
                              u(pyu - 4.0) + u(3.0), u(24.0), k);
                    up::marco(l, art, up::marco::SLOT_CHICO,
                              u(pxu + panel_w - INV_BORDE - 68.0), u(pyu - 4.0), u(30.0), u(30.0), k);
                    up::icono(l, art, "ui/ic_engranaje",
                              u(pxu + panel_w - INV_BORDE - 68.0) + u(3.0),
                              u(pyu - 4.0) + u(3.0), u(24.0), k);

                    // ── la grilla ─────────────────────────────────────────
                    let gy = hy + INV_HEAD_H + 6.0;
                    let mut vista = up::caja(u(gx), u(gy), u(grid_w), u(alto_grilla), k);
                    vista.overflow = Overflow::scroll_y();
                    // `BagScrollView` es lo que hace que el arrastre del dedo
                    // scrollee esta lista (`scroll_craft_list`) y que el pulgar
                    // de la canaleta la siga.
                    l.spawn((vista, BagScrollView, Name::new("BagVista"))).with_children(|v| {
                        v.spawn((
                            Node {
                                width: Val::Px(u(grid_w) * k),
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                column_gap: Val::Px(u(INV_GAP) * k),
                                row_gap: Val::Px(u(INV_GAP) * k),
                                ..default()
                            },
                            ScrollContent,
                            Name::new("BagContenido"),
                        ))
                        .with_children(|cont| {
                            // **Once columnas EXACTAS**, por el mismo motivo que
                            // la grilla del creador: envolver por flex depende
                            // del redondeo y perdía una columna.
                            // **Flex con wrap y ancho EXACTO**, no CSS Grid (28 sep
                            // 2026): con `Display::Grid` las casillas salían en UNA
                            // fila, aplastadas a rayitas (captura `HA_BAGSHOT`).
                            // El ancho es el de once casillas y sus diez huecos más
                            // 2 px de holgura —el redondeo que antes perdía una
                            // columna no alcanza a mandar la undécima abajo— y
                            // cada casilla va con `flex_shrink: 0`.
                            let paso = u(slot_u) * k;
                            let hueco = u(INV_GAP) * k;
                            let ancho = cols as f32 * paso + (cols as f32 - 1.0) * hueco + 2.0;
                            let grilla = || Node {
                                width: Val::Px(ancho),
                                flex_direction: FlexDirection::Row,
                                flex_wrap: FlexWrap::Wrap,
                                align_content: AlignContent::FlexStart,
                                column_gap: Val::Px(hueco),
                                row_gap: Val::Px(hueco),
                                ..default()
                            };
                            match tab {
                                BagTab::Inventory => {
                                    cont.spawn((
                                        grilla(),
                                        BagInventorySection,
                                        InventoryGridRoot,
                                        Name::new("BagGrid"),
                                    ))
                                    .with_children(|grid| {
                                        // **La primera fila ES la barra rápida**
                                        // (29 sep 2026): cambiar lo que llevás a
                                        // mano no obliga a cerrar la mochila.
                                        for i in 0..HOTBAR_ITEM_SLOTS {
                                            spawn_bag_hotbar_cell(grid, i, u(slot_u) * k, heart);
                                        }
                                        for i in 0..(GRID_COLS * GRID_ROWS) {
                                            spawn_inventory_grid_cell(grid, i, u(slot_u) * k, heart);
                                        }
                                    });
                                }
                                BagTab::Craft => {
                                    cont.spawn((
                                        Node {
                                            width: Val::Percent(100.0),
                                            flex_direction: FlexDirection::Row,
                                            flex_wrap: FlexWrap::Wrap,
                                            column_gap: Val::Px(u(INV_GAP) * k),
                                            row_gap: Val::Px(u(INV_GAP) * k),
                                            ..default()
                                        },
                                        BagCraftSection,
                                        Name::new("BagCraft"),
                                    ))
                                    .with_children(|section| {
                                        for recipe in ha_shared::CRAFT_RECIPES.iter() {
                                            spawn_recipe_card(section, recipe, heart, card);
                                        }
                                    });
                                }
                                BagTab::Build => {
                                    cont.spawn((
                                        grilla(),
                                        BagBuildList,
                                        Name::new("BagBuild"),
                                    ))
                                    .with_children(|list| {
                                        fill_build_group(list, group, page, heart,
                                                         u(slot_u) * k);
                                    });
                                }
                            }
                        });
                    });

                    // ── la canaleta ───────────────────────────────────────
                    let sx = gx + grid_w + 4.0;
                    up::marco(l, art, up::marco::HUECO, u(sx), u(gy), u(INV_SCROLL_W),
                              u(alto_grilla), k);
                    l.spawn((
                        up::caja(u(sx) + 6.0, u(gy) + 6.0, u(INV_SCROLL_W) - 12.0, u(56.0), k),
                        BackgroundColor(Color::srgb(0.690, 0.463, 0.322)),
                        PulgarInventario {
                            y0: (u(gy) + 6.0) * k,
                            alto: (u(alto_grilla) - 12.0) * k,
                        },
                    ));
                });
            })
            .id();
    });
    modal_entity
}

/// **Lo que el armado de la mochila necesita del mundo.** Va junto porque
/// `bag_panel_system` ya estaba en el tope de parámetros que acepta un sistema
/// de Bevy y hacía falta uno más: el arte del pack.
#[derive(bevy::ecs::system::SystemParam)]
struct MochilaCtx<'w> {
    art: Res<'w, crate::art2d::ArtLibrary>,
    layout: Res<'w, TouchLayout>,
    tune: Res<'w, HudTune>,
}

/// **QA de dev**: `HA_BAGSHOT=<prefijo>` deja dos capturas del juego —el HUD
/// pelado y la mochila abierta— y sale. Es lo que permite comparar esas dos
/// pantallas contra las imágenes de referencia sin tener el teléfono a mano.
#[derive(Resource, Default)]
pub struct BagShot {
    cuadros: u32,
}

fn capturar_mochila(
    mut shot: ResMut<BagShot>,
    mut bag: ResMut<BagPanelState>,
    mut commands: Commands,
    mut exit: EventWriter<AppExit>,
    fase: Res<State<crate::game_phase::GamePhase>>,
) {
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    let Ok(prefijo) = crate::flags::var("HA_BAGSHOT") else { return };
    if !matches!(
        *fase.get(),
        crate::game_phase::GamePhase::Playing
    ) {
        return;
    }
    shot.cuadros += 1;
    match shot.cuadros {
        // Aire para que el mundo termine de aparecer: capturar antes deja el
        // HUD sobre un suelo a medio llegar y parece un bug del HUD.
        90 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("{prefijo}-hud.png")));
            eprintln!("ha-bagshot: hud");
        }
        110 => bag.open = true,
        160 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("{prefijo}-mochila.png")));
            eprintln!("ha-bagshot: mochila");
        }
        170 => bag.tab = BagTab::Craft,
        210 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("{prefijo}-fabricar.png")));
            eprintln!("ha-bagshot: fabricar");
        }
        220 => bag.tab = BagTab::Build,
        260 => {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("{prefijo}-construir.png")));
            eprintln!("ha-bagshot: construir");
        }
        300 => {
            exit.send(AppExit::Success);
        }
        _ => {}
    }
}

/// **El pulgar de la canaleta del inventario sigue al scroll.** Sin esto la
/// canaleta es un adorno: el usuario lo marcó mirando las capturas.
fn mover_pulgar_inventario(
    vistas: Query<(&ScrollPosition, &ComputedNode, &Children), With<BagScrollView>>,
    contenidos: Query<&ComputedNode, With<ScrollContent>>,
    mut pulgares: Query<(&mut Node, &PulgarInventario)>,
) {
    let Ok((sp, vista, hijos)) = vistas.get_single() else { return };
    let alto_vista = vista.size().y * vista.inverse_scale_factor();
    let alto_contenido = hijos
        .iter()
        .filter_map(|c| contenidos.get(*c).ok())
        .map(|c| c.size().y * c.inverse_scale_factor())
        .fold(0.0f32, f32::max);
    if alto_contenido <= 0.0 || alto_vista <= 0.0 {
        return;
    }
    let frac = (alto_vista / alto_contenido).clamp(0.08, 1.0);
    let tope = max_scroll(alto_vista, alto_contenido);
    let avance = if tope > 0.0 { (sp.offset_y / tope).clamp(0.0, 1.0) } else { 0.0 };
    for (mut nodo, p) in &mut pulgares {
        let alto = p.alto * frac;
        nodo.height = Val::Px(alto);
        nodo.top = Val::Px(p.y0 + (p.alto - alto) * avance);
        nodo.display = if frac >= 1.0 { Display::None } else { Display::Flex };
    }
}

/// **Las barras de la columna del personaje dicen la verdad.** Nacen llenas y
/// este sistema les pone el largo real: hambre, sed y aguante, los mismos
/// valores que muestra el HUD de afuera.
fn barras_de_la_mochila(
    player: Query<&SimState, With<Player>>,
    mut barras: Query<(&mut Node, &BarraEstadoMochila)>,
) {
    if barras.is_empty() {
        return;
    }
    let Ok(sim) = player.get_single() else { return };
    let v = [
        (sim.survival.hunger / ha_shared::HUNGER_MAX).clamp(0.0, 1.0),
        (sim.survival.thirst / ha_shared::THIRST_MAX).clamp(0.0, 1.0),
        if sim.body.max_stamina > 0.0 {
            (sim.body.stamina / sim.body.max_stamina).clamp(0.0, 1.0)
        } else {
            0.0
        },
    ];
    for (mut nodo, b) in &mut barras {
        nodo.width = Val::Percent(v[b.0.min(2)] * 100.0);
    }
}

/// La barra de HAMBRE/SED/KI de la columna del personaje (`.0` = cuál).
#[derive(Component)]
struct BarraEstadoMochila(usize);

/// Las medidas del panel viejo. Siguen existiendo porque otras piezas del HUD
/// las consultan (el tamaño de tarjeta de las recetas, la animación de apertura);
/// la GEOMETRÍA del panel nuevo sale de las unidades del mockup, no de acá.
struct BagMetrics {
    /// Borde entre la ventana y el límite de lo que ocupa el juego.
    #[allow(dead_code)]
    edge: f32,
    #[allow(dead_code)]
    panel_w: f32,
    #[allow(dead_code)]
    panel_h: f32,
    #[allow(dead_code)]
    pad: f32,
    #[allow(dead_code)]
    cell_gap: f32,
    /// Lado de una celda de la grilla del inventario.
    #[allow(dead_code)]
    cell: f32,
    /// Lado de una tarjeta de crafteo/construcción.
    card: f32,
}

fn bag_metrics(layout: &TouchLayout, heart: f32) -> BagMetrics {
    let short = layout.width.min(layout.height);
    // El asset de panel ES el fondo, asi que ocupa la ventana entera: sin margen
    // no queda el navy asomando por los bordes (doble fondo). Solo se respetan
    // los insets del notch, que si son zona muerta de verdad.
    let edge = 0.0;
    let _ = short;
    // LA VENTANA ENTERA. El asset de panel es la ventana, no un recuadro
    // adentro de otra cosa: restarle los insets dejaba una franja azul del velo
    // asomando por los costados (el notch mide ~37 px logicos). El contenido si
    // respeta los insets, via `pad`.
    let panel_w = layout.width.max(240.0);
    // **LA HOTBAR QUEDA AFUERA DEL PANEL.**
    //
    // Decisión del 15 ago 2026 sobre los mockups: *"la hotbar siempre a la vista
    // para equipar sin cerrar"*. El panel tomaba la ventana entera
    // (`layout.height`) y la tapaba, así que para cambiar de ítem había que
    // cerrar la mochila, equipar y volver a abrirla — tres gestos para uno.
    //
    // Se le resta su banda: el alto del slot más el aire de abajo, que salen del
    // MISMO sitio del que la hotbar los saca (`hotbar_layout_metrics`) y no de un
    // número a ojo. Si mañana la hotbar cambia de tamaño, esto la sigue.
    let (slot_hotbar, _, _, bottom_hotbar) = hotbar_layout_metrics(layout);
    let banda_hotbar = slot_hotbar + bottom_hotbar + 6.0;
    let panel_h = (layout.height - banda_hotbar).max(200.0);
    // Mas aire adentro del panel: ahora que el asset ocupa la ventana entera, el
    // contenido pegado al borde se comia el marco dibujado del asset.
    let pad = (heart * 0.75).clamp(14.0, 28.0);
    let cell_gap = (heart * 0.22).clamp(4.0, 9.0);
    let inner_w = panel_w - pad * 2.0;
    let cell =
        ((inner_w - cell_gap * (GRID_COLS as f32 - 1.0)) / GRID_COLS as f32).clamp(40.0, 84.0);
    let target = (short * 0.17).clamp(64.0, 132.0);
    let per_row = ((inner_w + cell_gap) / (target + cell_gap)).floor().max(2.0);
    let card = ((inner_w - cell_gap * (per_row - 1.0)) / per_row).max(52.0);
    BagMetrics { edge, panel_w, panel_h, pad, cell_gap, cell, card }
}

/// La lista de tarjetas del grupo activo (el contenido que se rehace al cambiar
/// de ficha).
#[derive(Component)]
struct BagBuildList;

/// El renglón de ayuda que cambia con el grupo.
#[derive(Component)]
struct BuildGroupHint;

/// Una ficha de filtro. Chica, redondeada y con el estado activo pintado: es la
/// misma familia visual que las pestañas de arriba, un escalón más abajo en la
/// jerarquía (pestaña = qué pantalla; ficha = qué parte de esta pantalla).
fn spawn_build_chip(parent: &mut ChildBuilder, idx: usize, label: &str, heart: f32, active: bool) {
    parent
        .spawn((
            Button,
            Node {
                height: Val::Px((heart * 1.08).clamp(20.0, 28.0)),
                padding: UiRect::horizontal(Val::Px(9.0)),
                border: UiRect::all(Val::Px(if active { 2.0 } else { 1.0 })),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderRadius::MAX,
            BorderColor(if active {
                ui_theme::accent_ink()
            } else {
                ui_theme::button_border(UiButtonTone::Inventory)
            }),
            BackgroundColor(ui_theme::button_bg(
                UiButtonTone::Inventory,
                if active { UiButtonState::Pressed } else { UiButtonState::Normal },
            )),
            BuildFilterChip(idx),
        ))
        .with_children(|chip| {
            // **Cada pestaña lleva un objeto de su categoría como icono** (mockup
            // del 15 ago 2026). Nace vacío y lo llena `pintar_iconos_de_ficha`:
            // acá no hay `ArtLibrary` a mano —`spawn_bag_panel` sólo recibe
            // `Commands`— y pasársela obligaría a tocar todos sus llamadores.
            // Es el mismo patrón que ya usa el resto del HUD para el arte del
            // pack (`ui_image_node` en `update_hotbar_item_visuals`).
            chip.spawn((
                Node {
                    width: Val::Px((heart * 0.72).clamp(12.0, 18.0)),
                    height: Val::Px((heart * 0.72).clamp(12.0, 18.0)),
                    margin: UiRect::right(Val::Px(4.0)),
                    flex_shrink: 0.0,
                    // Sin arte asignado el hueco no se ve: mejor la ficha con
                    // sólo su nombre que un cuadrado vacío al lado.
                    display: Display::None,
                    ..default()
                },
                IconoDeFicha(idx),
                Name::new("IconoDeFicha"),
            ));
            chip.spawn((
                Text::new(label.to_string()),
                TextFont { font_size: (heart * 0.44).clamp(8.0, 11.0), ..default() },
                TextColor(if active {
                    ui_theme::text_primary()
                } else {
                    ui_theme::text_muted()
                }),
            ));
        });
}

/// Qué hace el grupo activo, en un renglón. Cada familia de piezas se coloca
/// distinto y eso antes se leía (si se leía) en un rótulo perdido en el medio de
/// la lista.
fn build_group_hint(group: usize) -> &'static str {
    match build_groups().get(group).map(|(g, _)| *g) {
        Some(BuildGroup::Pieces) => "Elegi una pieza: entra modo construir. Coloca con USE. Cavar la quita.",
        Some(BuildGroup::Edges) => "Se pegan al borde del piso apuntado; volver a tocar demuele.",
        Some(BuildGroup::Ceilings) => "Tapa la celda apuntada; volver a tocar la saca. El piso de arriba ya tapa solo.",
        Some(BuildGroup::Prefabs) => "Estampa el area entera de una sola vez.",
        Some(BuildGroup::Edificio) => "Apunta a la vereda y funda; despues ampliá, apilá plantas y abrile la puerta.",
        Some(BuildGroup::Fauna) => "Solo creativo: encerrala y observa como evoluciona.",
        _ => "Se coloca entera y se rodea (no se entra). Cuesta material.",
    }
}

/// Llena la lista con las tarjetas del grupo activo.
///
/// **El índice de paleta no cambia por agrupar**: sigue siendo el de las tablas
/// (`BUILD_PIECES`, luego prefabs, luego colocables, luego bordes, luego fauna),
/// así que `build_selection` no se entera de nada de esto.
/// Tarjetas por página de la paleta de CONSTRUIR.
///
/// Existe porque el pack entero entró al catálogo: MUEBLES tiene 2.878
/// colocables. Escupirlos de una son 2.878 botones con icono y texto en un
/// frame —el tirón que la ficha vino a evitar cuando eran 350— y una tira de
/// scroll de 28.000 px que no se recorre con el dedo. Con páginas de 48 el costo
/// de abrir una ficha es constante, se mire el catálogo que se mire.
/// **Una página son cinco pantallas de grilla.** La imagen de referencia no
/// tiene paginado: tiene una grilla de 11 × 4 y una canaleta. Con 220 por página
/// el scroll alcanza para casi todo y el paginado es la excepción, no el modo de
/// uso. Las fichas van en el encabezado, nunca adentro de la grilla — ahí se
/// comían las dos primeras casillas.
const BUILD_POR_PAGINA: usize = 220;

/// Botón de paginado de la paleta: `-1` es la página anterior, `+1` la siguiente.
#[derive(Component, Clone, Copy)]
struct BuildPageBtn(i32);

/// La fila de paginado, arriba de las tarjetas: ANTERIOR · «página 3 de 60» ·
/// SIGUIENTE. Sólo aparece cuando la ficha no entra en una página.
fn spawn_build_pager(list: &mut ChildBuilder, page: usize, paginas: usize, heart: f32, card: f32) {
    list.spawn(Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::SpaceBetween,
        column_gap: Val::Px(6.0),
        margin: UiRect::bottom(Val::Px(4.0)),
        ..default()
    })
    .with_children(|fila| {
        let boton = |fila: &mut ChildBuilder, delta: i32, texto: &str, activo: bool| {
            fila.spawn((
                Button,
                Node {
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor(ui_theme::button_border(UiButtonTone::Inventory)),
                BackgroundColor(ui_theme::button_bg(
                    UiButtonTone::Inventory,
                    if activo { UiButtonState::Normal } else { UiButtonState::Disabled },
                )),
                BuildPageBtn(delta),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new(texto.to_string()),
                    TextFont { font_size: (heart * 0.44).clamp(8.0, 11.0), ..default() },
                    TextColor(if activo { ui_theme::text_primary() } else { ui_theme::text_muted() }),
                ));
            });
        };
        boton(fila, -1, "ANTERIOR", page > 0);
        fila.spawn((
            Text::new(format!("página {} de {paginas}", page + 1)),
            TextFont { font_size: (heart * 0.44).clamp(8.0, 11.0), ..default() },
            TextColor(ui_theme::text_muted()),
        ));
        boton(fila, 1, "SIGUIENTE", page + 1 < paginas);
    });
    let _ = card;
}

/// Cuántas tarjetas tiene una ficha de CONSTRUIR (para paginar y para el test
/// que exige que ninguna esté vacía).
fn build_group_len(group: BuildGroup) -> usize {
    match group {
        BuildGroup::Pieces => ha_shared::topdown::build_piece_count(),
        BuildGroup::Prefabs => ha_shared::topdown::prefab_count(),
        BuildGroup::Edges => ha_shared::build_grid::EDGE_PIECES.len(),
        BuildGroup::Ceilings => ha_shared::build_grid::CEILING_PIECES.len(),
        BuildGroup::Edificio => crate::edificios::accion_count(),
        BuildGroup::Todo => ha_shared::placeables::PLACEABLES.len(),
        BuildGroup::Place(cat) => ha_shared::placeables::placeables_in(cat).count(),
        BuildGroup::Fauna => crate::fauna::spawn_species_count() + 4,
    }
}

/// Cuántas páginas tiene una ficha (mínimo 1: una ficha vacía sigue siendo una
/// página, no cero).
fn build_group_pages(group: BuildGroup) -> usize {
    build_group_len(group).div_ceil(BUILD_POR_PAGINA).max(1)
}

fn fill_build_group(list: &mut ChildBuilder, group: usize, page: usize, heart: f32, card: f32) {
    let Some((group, _)) = build_groups().get(group).copied() else {
        return;
    };
    let paginas = build_group_pages(group);
    let page = page.min(paginas - 1);
    let desde = page * BUILD_POR_PAGINA;
    let hasta = desde + BUILD_POR_PAGINA;
    let _ = paginas;
    // La ventana se aplica CONTANDO, no cortando la tabla: cada rama recorre lo
    // suyo como siempre y sólo instancia lo que cae en la página. Así el índice
    // de paleta que recibe `build_selection` sigue siendo el de las tablas y el
    // paginado no se filtra a la lógica de colocar.
    let mut n = 0usize;
    macro_rules! carta {
        ($($arg:tt)*) => {{
            if n >= desde && n < hasta {
                spawn_build_piece_card($($arg)*);
            }
            n += 1;
        }};
    }
    let prefab_base = ha_shared::topdown::build_piece_count();
    let place_base = prefab_base + ha_shared::topdown::prefab_count();
    match group {
        BuildGroup::Pieces => {
            for (idx, piece) in ha_shared::topdown::BUILD_PIECES.iter().enumerate() {
                carta!(
                    list,
                    idx,
                    piece.label,
                    piece.cost_item,
                    piece.cost_qty,
                    build_piece_icon(piece.tile, piece.cost_item),
                    heart,
                    card,
                );
            }
        }
        BuildGroup::Prefabs => {
            for (i, p) in ha_shared::topdown::PREFABS.iter().enumerate() {
                carta!(
                    list,
                    prefab_base + i,
                    p.label,
                    p.cost_item,
                    ha_shared::topdown::prefab_cost(p),
                    UiIconSlot::item(p.cost_item),
                    heart,
                    card,
                );
            }
        }
        BuildGroup::Edges => {
            let ebase = crate::build::edge_palette_base();
            for (i, e) in ha_shared::build_grid::EDGE_PIECES.iter().enumerate() {
                carta!(
                    list,
                    ebase + i,
                    e.label,
                    e.cost_item,
                    e.cost_qty,
                    UiIconSlot::art(&[e.art], e.cost_item),
                    heart,
                    card,
                );
            }
        }
        BuildGroup::Ceilings => {
            let cbase = crate::build::ceiling_palette_base();
            for (i, c) in ha_shared::build_grid::CEILING_PIECES.iter().enumerate() {
                carta!(
                    list,
                    cbase + i,
                    c.label,
                    c.cost_item,
                    c.cost_qty,
                    UiIconSlot::art(&[c.art], c.cost_item),
                    heart,
                    card,
                );
            }
        }
        BuildGroup::Edificio => {
            // La cáscara no cuesta material: lo que cuesta es el paño y las
            // aberturas, que las sigue cobrando el sistema de paredes.
            for (i, (_, label, _)) in crate::edificios::ACCIONES.iter().enumerate() {
                carta!(
                    list,
                    crate::edificios::PALETTE_BASE + i,
                    label,
                    "sin costo",
                    0,
                    UiIconSlot::art(&[crate::edificios::icono(i)], "stone"),
                    heart,
                    card,
                );
            }
        }
        BuildGroup::Todo => {
            for (i, a) in ha_shared::placeables::PLACEABLES.iter().enumerate() {
                carta!(
                    list,
                    place_base + i,
                    a.label,
                    a.cost_item,
                    a.cost_qty,
                    UiIconSlot::art(&[a.art], a.cost_item),
                    heart,
                    card,
                );
            }
        }
        BuildGroup::Place(cat) => {
            for (i, a) in ha_shared::placeables::placeables_in(cat) {
                carta!(
                    list,
                    place_base + i,
                    a.label,
                    a.cost_item,
                    a.cost_qty,
                    // El colocable SE VE: su propio slot de arte es el icono.
                    UiIconSlot::art(&[a.art], a.cost_item),
                    heart,
                    card,
                );
            }
        }
        BuildGroup::Fauna => {
            // VIVARIO (F1): spawn de fauna RESIDENTE — sólo surte efecto en
            // creativo. Encerrala (puerta cerrada) y observá cómo evoluciona.
            for i in 0..crate::fauna::spawn_species_count() {
                carta!(
                    list,
                    crate::fauna::SPAWN_PALETTE_BASE + i,
                    crate::fauna::spawn_species_label(i),
                    "solo creativo",
                    0,
                    UiIconSlot::art(&[crate::fauna::spawn_species_art(i)], "leather"),
                    heart,
                    card,
                );
            }
            // F8: feeder de comida persistente (diseñar la presión del hábitat).
            for (sub, label, item) in [
                (crate::fauna::FEED_PALETTE_SUB, "Comida (feeder)", "apple"),
                (crate::fauna::CAMERA_PALETTE_SUB, "Camara", "iron_ingot"),
                (crate::fauna::PREDATOR_PALETTE_SUB, "Depredador", "sword_iron"),
                (crate::fauna::WATER_PALETTE_SUB, "Agua", "potion_blue"),
            ] {
                carta!(
                    list,
                    crate::fauna::SPAWN_PALETTE_BASE + sub,
                    label,
                    "solo creativo",
                    0,
                    UiIconSlot::item(item),
                    heart,
                    card,
                );
            }
        }
    }
}

/// Un rótulo de sección dentro de una sección que envuelve tarjetas. Ocupa el
/// ancho entero a propósito: eso fuerza el corte de línea del `Wrap`, así el grupo
/// siguiente empieza en su propia fila.
fn spawn_section_label(parent: &mut ChildBuilder, text: &str, heart: f32) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            margin: UiRect::top(Val::Px(4.0)),
            ..default()
        })
        .with_children(|w| {
            w.spawn((
                Text::new(text.to_string()),
                TextFont {
                    font_size: (heart * 0.44).clamp(9.0, 12.0),
                    ..default()
                },
                TextColor(ui_theme::text_muted()),
            ));
        });
}

/// **Una casilla del inventario, tal cual el mockup**: el marco del pack, el
/// objeto centrado y la cantidad abajo a la derecha. Sin nombre debajo — en la
/// maqueta la casilla es sólo el dibujo y el número, y meterle texto la vuelve
/// ilegible al tamaño real.
fn spawn_inventory_grid_cell(parent: &mut ChildBuilder, index: usize, lado: f32, heart: f32) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(lado),
                height: Val::Px(lado),
                min_width: Val::Px(lado),
                min_height: Val::Px(lado),
                flex_shrink: 0.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                overflow: Overflow::clip(),
                ..default()
            },
            // Sin color propio: lo pinta `ui/marco_slot` vía `casillas_del_kit`.
            BackgroundColor(Color::NONE),
            CasillaDelKit,
            InventoryGridSlot { index },
        ))
        .with_children(|slot| {
            slot.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(15.0),
                    top: Val::Percent(15.0),
                    width: Val::Percent(70.0),
                    height: Val::Percent(70.0),
                    ..default()
                },
                InventorySlotIcon,
            ));
            // El nombre existe pero no se dibuja: lo siguen escribiendo los
            // sistemas de siempre y se lee en el renglón de la hotbar.
            slot.spawn((
                Text::new(""),
                TextFont { font_size: 0.1, ..default() },
                TextColor(Color::NONE),
                InventorySlotName,
            ));
            slot.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Percent(58.0),
                    width: Val::Percent(92.0),
                    height: Val::Percent(38.0),
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Name::new("CantidadCasilla"),
            ))
            .with_children(|q| {
                q.spawn((
                    Text::new(""),
                    TextFont { font_size: (heart * 0.42).clamp(9.0, 16.0), ..default() },
                    TextColor(crate::ui_pack::Pack::TEXTO),
                    InventorySlotQty,
                ));
            });
        });
}

/// Una casilla de la BARRA RÁPIDA dentro de la mochila. Es la misma casilla
/// que la de la grilla, pero con las marcas de la hotbar: la pintan
/// `update_hotbar_item_visuals` y los mismos sistemas de toque/arrastre, así que
/// mover algo entre la barra y la mochila es tocar una y después la otra.
fn spawn_bag_hotbar_cell(parent: &mut ChildBuilder, index: usize, lado: f32, heart: f32) {
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(lado),
                height: Val::Px(lado),
                min_width: Val::Px(lado),
                min_height: Val::Px(lado),
                flex_shrink: 0.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::NONE),
            CasillaDelKit,
            HotbarItemSlot { index },
            Name::new("BarraEnMochila"),
        ))
        .with_children(|slot| {
            slot.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(15.0),
                    top: Val::Percent(15.0),
                    width: Val::Percent(70.0),
                    height: Val::Percent(70.0),
                    ..default()
                },
                HotbarSlotIcon,
            ));
            slot.spawn((
                Text::new(""),
                TextFont { font_size: 0.1, ..default() },
                TextColor(Color::NONE),
                HotbarSlotName,
            ));
            slot.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Percent(58.0),
                    width: Val::Percent(92.0),
                    height: Val::Percent(38.0),
                    justify_content: JustifyContent::FlexEnd,
                    align_items: AlignItems::Center,
                    ..default()
                },
                Name::new("CantidadCasilla"),
            ))
            .with_children(|q| {
                q.spawn((
                    Text::new(""),
                    TextFont { font_size: (heart * 0.42).clamp(9.0, 16.0), ..default() },
                    TextColor(crate::ui_pack::Pack::TEXTO),
                    HotbarSlotQty,
                ));
            });
        });
}

/// El icono de lo que tenés en la mano, en la columna PERSONAJE.
fn pintar_mano_mochila(
    equipped: Res<EquippedItem>,
    mut icons: ResMut<ItemIconCache>,
    mut images: ResMut<Assets<Image>>,
    mut q: Query<(&mut ImageNode, &mut Visibility, Ref<IconoManoMochila>)>,
) {
    for (mut img, mut vis, marca) in &mut q {
        if !marca.is_added() && !equipped.is_changed() {
            continue;
        }
        let id = equipped.item_id.as_str();
        if id.is_empty() || id == "hand" {
            *vis = Visibility::Hidden;
            continue;
        }
        let icon = icons.icon(id, &mut images);
        img.image = icon.image;
        img.rect = icon.rect;
        *vis = Visibility::Inherited;
    }
}

/// Marca una casilla que tiene que llevar el marco del kit (`ui/marco_slot`).
/// Lo pinta [`casillas_del_kit`] en cuanto nace, igual que `skin_ui_from_art`
/// hace con el resto del HUD.
#[derive(Component)]
pub struct CasillaDelKit;

/// Le pone su marco a las casillas recién nacidas.
fn casillas_del_kit(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    nuevas: Query<Entity, Added<CasillaDelKit>>,
) {
    for e in &nuevas {
        if let Some(img) = art.ui_frame_node(crate::ui_pack::marco::SLOT) {
            commands.entity(e).try_insert(img);
        }
    }
}

/// **El muñeco de la columna PERSONAJE.** Va aparte del armado del panel porque
/// `bag_panel_system` ya está en el tope de parámetros de Bevy.
fn pintar_personaje_mochila(
    mut commands: Commands,
    huecos: Query<Entity, Added<HuecoPersonaje>>,
    cg: Res<crate::chargen::Chargen>,
    hoja: Res<crate::ui_pack::HojaPj>,
    assets: Res<AssetServer>,
    selected: Res<crate::game_phase::SelectedCharacter>,
) {
    let Some(look) = selected.look else { return };
    for e in &huecos {
        commands.entity(e).with_children(|pila| {
            for capa in [
                crate::chargen::Layer::Body,
                crate::chargen::Layer::Eyes,
                crate::chargen::Layer::Outfit,
                crate::chargen::Layer::Hair,
                crate::chargen::Layer::Accessory,
            ] {
                let idx = match capa {
                    crate::chargen::Layer::Body => Some(look.body),
                    crate::chargen::Layer::Eyes => Some(look.eyes),
                    crate::chargen::Layer::Outfit => Some(look.outfit),
                    crate::chargen::Layer::Hair => Some(look.hair),
                    crate::chargen::Layer::Accessory => look.accessory,
                };
                let Some(o) = idx.and_then(|i| cg.options(capa).get(i as usize)) else {
                    continue;
                };
                pila.spawn((
                    ImageNode::from_atlas_image(
                        assets.load(&o.png),
                        TextureAtlas {
                            layout: hoja.0.clone(),
                            index: crate::ui_pack::celda(3),
                        },
                    ),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(0.0),
                        top: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                ));
            }
        });
    }
}

fn despawn_bag_panel(commands: &mut Commands, bag_state: &mut BagPanelState) {
    if let Some(entity) = bag_state.modal.take() {
        commands.entity(entity).despawn_recursive();
    }
}

// ── Tarjetas con icono ───────────────────────────────────────────────────────

/// **Un hueco de icono que se rellena solo.** Las tarjetas se crean en
/// `spawn_bag_panel`, que no tiene acceso a `Assets<Image>` ni al caché de iconos;
/// este componente declara QUÉ dibujar y [`fill_ui_icon_slots`] lo resuelve el
/// frame siguiente, en el orden correcto: arte del pack (con su recorte de atlas o
/// su pose representativa) y, si el pack no lo trae, el icono del ítem.
#[derive(Component, Clone)]
pub struct UiIconSlot {
    /// Slots de arte a probar, en orden.
    art: Vec<String>,
    /// Id de ítem para el último recurso (`items/<id>` del pack o raster propio).
    item: String,
}

impl UiIconSlot {
    /// Icono de un ítem del inventario (madera, espada, mapa…).
    pub fn item(item: &str) -> Self {
        Self { art: Vec::new(), item: item.to_string() }
    }

    /// Arte de un slot del pack (una pieza de construir, una máquina, un bicho),
    /// con un ítem de respaldo si ese slot todavía no tiene pixel asignado.
    pub fn art(keys: &[&str], item: &str) -> Self {
        Self {
            art: keys.iter().map(|k| k.to_string()).collect(),
            item: item.to_string(),
        }
    }
}

/// Resuelve cada [`UiIconSlot`] recién creado. Corre con `Added<>`: coste cero en
/// reposo, y así una tarjeta nunca queda con un cuadrado vacío.
fn fill_ui_icon_slots(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    mut images: ResMut<Assets<Image>>,
    mut icons: ResMut<ItemIconCache>,
    pending: Query<(Entity, &UiIconSlot), Added<UiIconSlot>>,
) {
    for (entity, slot) in &pending {
        let node = slot
            .art
            .iter()
            .find_map(|key| art.ui_icon(key))
            .unwrap_or_else(|| icons.icon(&slot.item, &mut images).node());
        commands.entity(entity).try_insert(node);
    }
}

/// Qué icono le toca a una pieza de construir: **su propio material**. El arte de
/// tile ya existe (autotile del editor), así que la tarjeta del piso de madera
/// muestra piso de madera y no la palabra "Piso madera".
fn build_piece_icon(tile: ha_shared::topdown::Tile, cost_item: &str) -> UiIconSlot {
    match crate::world2d::tile_art_key(tile) {
        Some(key) => UiIconSlot::art(&[key], cost_item),
        None => UiIconSlot::item(cost_item),
    }
}

/// **La tarjeta: icono grande, nombre DEBAJO del icono, costo al pie.** Es el
/// formato que se pidió para todas las secciones (inventario, craftear,
/// construir): antes craftear y construir eran filas de texto con el costo a la
/// derecha — se leían como un listado de configuración, no como objetos.
#[allow(clippy::too_many_arguments)]
fn spawn_icon_card(
    parent: &mut ChildBuilder,
    heart: f32,
    card: f32,
    label: &str,
    cost: String,
    icon: UiIconSlot,
    tone: UiButtonTone,
    marker: impl Bundle,
    cost_marker: impl Bundle,
) {
    let _ = heart;
    let disc = card * 0.66;
    parent
        .spawn((
            Button,
            Node {
                width: Val::Px(card),
                // 1.38 daba justo para UN renglón de nombre. Con dos (la mitad
                // del catálogo importado tiene nombre largo) el costo quedaba
                // afuera aunque el nombre ya no empuje.
                height: Val::Px(card * 1.52),
                border: UiRect::all(Val::Px(2.0)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::FlexStart,
                padding: UiRect::axes(Val::Px(3.0), Val::Px(5.0)),
                row_gap: Val::Px(3.0),
                overflow: Overflow::clip(),
                ..default()
            },
            BorderRadius::all(Val::Px(card * 0.16)),
            BorderColor(ui_theme::button_border(tone)),
            BackgroundColor(ui_theme::button_bg(tone, UiButtonState::Normal)),
            marker,
        ))
        .with_children(|btn| {
            // El icono va en un DISCO: es lo que hace que la tarjeta se lea como
            // objeto y no como celda, y casa con los botones circulares.
            btn.spawn((
                Node {
                    width: Val::Px(disc),
                    height: Val::Px(disc),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    overflow: Overflow::clip(),
                    ..default()
                },
                BorderRadius::MAX,
                BackgroundColor(ui_theme::panel_bg(0.55)),
            ))
            .with_children(|holder| {
                holder.spawn((
                    Node {
                        width: Val::Percent(84.0),
                        height: Val::Percent(84.0),
                        ..default()
                    },
                    icon,
                ));
            });
            // **El nombre vive en una caja que se come lo que sobra y recorta lo
            // que no entra.** Antes era un `Text` suelto: un nombre largo
            // ("Muelle de tablones") se partía en tres renglones y EMPUJABA el
            // costo fuera de la tarjeta, que tiene alto fijo y `clip()` — o sea
            // que el costo desaparecía. Ahora el que se recorta es el nombre.
            btn.spawn(Node {
                width: Val::Percent(100.0),
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_height: Val::Px(0.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                overflow: Overflow::clip(),
                ..default()
            })
            .with_children(|slot| {
                slot.spawn((
                    Text::new(label.to_string()),
                    TextFont {
                        font_size: (card * 0.155).clamp(8.0, 13.0),
                        ..default()
                    },
                    TextColor(ui_theme::text_primary()),
                    TextLayout::new_with_justify(JustifyText::Center),
                ));
            });
            btn.spawn((
                Text::new(cost),
                TextFont {
                    font_size: (card * 0.135).clamp(7.0, 11.0),
                    ..default()
                },
                // `text_primary` y no `text_muted`: con el marco de madera de
                // `ui/button` debajo, el gris apagado quedaba ilegible — y el
                // costo es la mitad de la información de la tarjeta.
                TextColor(ui_theme::text_primary()),
                TextLayout::new_with_justify(JustifyText::Center),
                // No se encoge nunca: el costo es la mitad de la información de
                // la tarjeta ("¿me alcanza?").
                Node { flex_shrink: 0.0, ..default() },
                cost_marker,
            ));
        });
}

/// Tarjeta de receta: el icono es **el resultado** (lo que vas a tener), no los
/// ingredientes — esos son el costo al pie, y `bag_panel_system` los reescribe con
/// el stock real ("2/3 madera").
fn spawn_recipe_card(panel: &mut ChildBuilder, recipe: &CraftRecipe, heart: f32, card: f32) {
    spawn_icon_card(
        panel,
        heart,
        card,
        nombre_de_receta(recipe),
        format_ingredients(recipe.ingredients),
        UiIconSlot::item(recipe.output_id),
        UiButtonTone::Craft,
        CraftRecipeButton { recipe_id: recipe.id, is_machine_select: false },
        CraftRecipeCostText { recipe_id: recipe.id, is_machine_select: false },
    );
}

/// "W5 S2 T1 I4" - only nonzero costs, so lunar kit shows its iron.
fn machine_cost_label(def: &ha_shared::MachineDef) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (tag, qty) in [
        ("W", def.cost_wood),
        ("S", def.cost_stone),
        ("T", def.cost_sticks),
        ("I", def.cost_iron),
    ] {
        if qty > 0 {
            parts.push(format!("{tag}{qty}"));
        }
    }
    parts.join(" ")
}

/// Tarjeta de máquina: al tocarla se cierra la mochila y quedás colocándola.
#[allow(clippy::too_many_arguments)]
fn spawn_machine_select_card(
    panel: &mut ChildBuilder,
    label: &str,
    recipe_id: u32,
    machine_id: &str,
    cost_label: String,
    heart: f32,
    card: f32,
) {
    spawn_icon_card(
        panel,
        heart,
        card,
        label,
        cost_label,
        UiIconSlot::art(&[&format!("machines/{machine_id}")], machine_id),
        UiButtonTone::Craft,
        CraftRecipeButton { recipe_id, is_machine_select: true },
        CraftRecipeCostText { recipe_id, is_machine_select: true },
    );
}

/// Tarjeta de una pieza de construir / prefab / asset grande / spawn creativo.
/// Elegirla entra en modo construir (lo atiende `build::build_tab_select`).
#[allow(clippy::too_many_arguments)]
/// **Una pieza colocable, como casilla del mockup.**
///
/// En la imagen de referencia el catálogo creativo es una grilla de casillas con
/// el objeto adentro y nada más — ni nombre ni costo, que a ese tamaño no se
/// leen. El nombre sigue existiendo como texto oculto para que los sistemas que
/// lo consultan no se rompan.
#[allow(clippy::too_many_arguments)]
fn spawn_build_piece_card(
    panel: &mut ChildBuilder,
    idx: usize,
    label: &str,
    cost_item: &str,
    cost_qty: u32,
    icon: UiIconSlot,
    heart: f32,
    card: f32,
) {
    let _ = heart;
    let cost = if cost_qty == 0 {
        cost_item.to_string()
    } else {
        format!("{cost_qty} x {cost_item}")
    };
    panel
        .spawn((
            Button,
            Node {
                width: Val::Px(card),
                height: Val::Px(card),
                flex_shrink: 0.0,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(Color::NONE),
            CasillaDelKit,
            BagBuildPieceButton(idx),
        ))
        .with_children(|c| {
            c.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(15.0),
                    top: Val::Percent(15.0),
                    width: Val::Percent(70.0),
                    height: Val::Percent(70.0),
                    ..default()
                },
                icon,
            ));
            // El nombre y el costo, invisibles: los lee el resto del HUD.
            c.spawn((
                Text::new(format!("{label} · {cost}")),
                TextFont { font_size: 0.1, ..default() },
                TextColor(Color::NONE),
            ));
        });
}

/// F7 - reskin de UI data-driven: cuando un elemento del HUD spawnea, si su
/// slot `ui/*` tiene arte asignado en el editor, se le inserta el `ImageNode`
/// (con recorte de atlas si corresponde). Sin arte, no pasa nada - el dibujo
/// procedural actual queda intacto (cero regresion). Corre con `Added<>`: costo
/// cero en reposo.
fn skin_ui_from_art(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    hotbar: Query<Entity, Added<HotbarSlot>>,
    actions: Query<(Entity, &crate::input::ActionButton), Added<crate::input::ActionButton>>,
    lantern: Query<Entity, Added<LanternToggleButton>>,
    closes: Query<
        Entity,
        Or<(Added<BagPanelCloseButton>, Added<MachinePanelCloseButton>)>,
    >,
    tabs: Query<(Entity, &BagTabButton, Option<&bevy::ui::widget::ImageNode>), Added<BagTabButton>>,
    // **Todo el resto de los botones** (tarjetas de craftear/construir, fichas de
    // grupo, botones del menú, del cofre, del creador). El arte de `ui/button` ya
    // estaba recortado en el manifest y NO lo consumía nadie: el HUD entero se
    // seguía dibujando con los colores planos de `ui_theme`. Los `Without` dejan
    // afuera a los que ya tienen su propio slot arriba — si no, el genérico les
    // pisaría el arte específico.
    #[allow(clippy::type_complexity)]
    generic: Query<
        (Entity, Option<&bevy::ui::widget::ImageNode>),
        (
            Added<Button>,
            Without<HotbarSlot>,
            Without<crate::input::ActionButton>,
            Without<LanternToggleButton>,
            Without<BagTabButton>,
            Without<BagPanelCloseButton>,
            Without<MachinePanelCloseButton>,
            Without<crate::ui_pack::SinMarco>,
        ),
    >,
    panels: Query<(Entity, Option<&bevy::ui::widget::ImageNode>), Or<(Added<BagPanelRoot>, Added<MachinePanelRoot>)>>,
) {
    for (e, ya) in &generic {
        // **El que ya trae marco, no se toca.** Las pantallas nuevas
        // (`ui_pack`) plantan su botón con la pieza del kit que les toca —
        // `ui/marco_boton`, `ui/marco_slot`, la casilla de una pestaña— y este
        // sistema les pisaba todo con `ui/button`. Se veía como que el menú
        // "perdía" el marco del mockup en cuanto el HUD arrancaba.
        if ya.is_some() {
            continue;
        }
        if let Some(img) = art.ui_frame_node("ui/button") {
            commands.entity(e).try_insert(img);
        }
    }
    for (e, ya) in &panels {
        if ya.is_some() {
            continue;
        }
        match art.ui_frame_node("ui/panel") {
            // El asset es el fondo entero: nada de color debajo.
            Some(img) => {
                commands.entity(e).try_insert((img, BackgroundColor(Color::NONE)));
            }
            // Sin arte asignada, el respaldo dibujado por codigo — un panel
            // invisible seria peor que uno feo.
            None => {
                commands.entity(e).try_insert(BackgroundColor(ui_theme::panel_bg_lit(0.0, 0.985)));
            }
        }
    }
    // (La hotbar ya NO se pinta con `ui/hotbar_slot`: esa losa beige plana pisaba
    // el marco del kit que le pone `casillas_del_kit` —según qué sistema corriera
    // último— y la tira salía como cuadrados sin borde, distinta a la mochila. La
    // casilla de la hotbar y la del inventario son la misma pieza.)
    let _ = &hotbar;
    for (e, action) in &actions {
        let key = match action.0 {
            crate::input::ActionKind::Use => "ui/use_button",
            crate::input::ActionKind::Jump => "ui/jump_button",
        };
        if let Some(img) = art.ui_image_node(key) {
            commands.entity(e).try_insert(img);
        }
    }
    // Los slots específicos que el usuario todavía no recortó caen al marco
    // genérico en vez de quedarse sin arte: es lo que hace que asignar UNA vez
    // `ui/button` vista TODOS los botones del juego, y que recortar el slot
    // propio después sea una mejora y no un requisito.
    for e in &lantern {
        if let Some(img) = art
            .ui_frame_node("ui/lantern_button")
            .or_else(|| art.ui_frame_node("ui/button"))
        {
            commands.entity(e).try_insert(img);
        }
    }
    for e in &closes {
        if let Some(img) = art.ui_image_node("ui/close_button") {
            commands.entity(e).try_insert(img);
        }
    }
    for (e, tab, img) in &tabs {
        // **La pestaña que ya trae el marco del kit se queda con él** (28 sep
        // 2026). La mochila nueva (`spawn_bag_panel`) le pone `ui/marco_slot`
        // y acá se le pisaba con la madera de `ui/button` estirada: pestañas
        // oscuras con franjas claras tachando el texto. Sólo se viste la que
        // llega sin imagen.
        if img.is_some() {
            continue;
        }
        let key = match tab.tab {
            BagTab::Inventory => "ui/tab_inventario",
            BagTab::Craft => "ui/tab_craft",
            BagTab::Build => "ui/tab_construir",
        };
        if let Some(img) = art.ui_frame_node(key).or_else(|| art.ui_frame_node("ui/button")) {
            commands.entity(e).try_insert(img);
        }
    }
}

/// Gamepad D-pad (or any other device) asks for a hotbar slot; consumed here.
#[derive(Resource, Default)]
pub(crate) struct HotbarSelectRequest {
    pub(crate) slot: Option<usize>,
}

fn bag_hotbar_system(
    keys: Res<ButtonInput<KeyCode>>,
    editor: Res<crate::ui_editor::UiEditorState>,
    mut bag_state: ResMut<BagPanelState>,
    mut ui: ResMut<InventoryUiState>,
    mut equipped: ResMut<EquippedItem>,
    mut select_req: ResMut<HotbarSelectRequest>,
    mut hotbar: Query<
        (&Interaction, &HotbarSlot, &mut BackgroundColor),
        (With<HotbarCraftButton>, Changed<Interaction>),
    >,
) {
    if let Some(idx) = select_req.slot.take() {
        select_hotbar_slot(&mut ui, &mut equipped, idx.min(7));
    }
    let desktop = !platform::uses_touch_gameplay_ui();
    if desktop && !editor.enabled && !editor.menu_open {
        if keys.just_pressed(KeyCode::KeyI) {
            bag_state.open = !bag_state.open;
            if bag_state.open {
                bag_state.tab = BagTab::Inventory;
            }
        }
        for (key, idx) in [
            (KeyCode::Digit1, 0usize),
            (KeyCode::Digit2, 1),
            (KeyCode::Digit3, 2),
            (KeyCode::Digit4, 3),
            (KeyCode::Digit5, 4),
            (KeyCode::Digit6, 5),
            (KeyCode::Digit7, 6),
            (KeyCode::Digit8, 7),
        ] {
            if keys.just_pressed(key) {
                select_hotbar_slot(&mut ui, &mut equipped, idx);
            }
        }
    }

    for (interaction, slot, mut bg) in &mut hotbar {
        if *interaction == Interaction::Pressed {
            let opening = !bag_state.open;
            bag_state.open = opening;
            if opening {
                bag_state.tab = BagTab::Inventory;
            }
        }
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Inventory,
            ui_theme::interaction_state(*interaction, slot.disabled),
        ));
    }

    if keys.just_pressed(KeyCode::Escape) && bag_state.open {
        bag_state.open = false;
    }
}

/// Marca la pestaña activa con el borde de acento y el texto encendido, no sólo
/// con el fondo. Con el fondo solo, en la pantalla del teléfono a plena luz las
/// tres pestañas se ven casi iguales y no sabés en cuál estás.
fn bag_tab_style_system(
    bag: Res<BagPanelState>,
    mut tabs: Query<(&BagTabButton, &mut BorderColor, &Children)>,
    mut text: Query<&mut TextColor>,
) {
    if !bag.open {
        return;
    }
    for (tab, mut border, children) in &mut tabs {
        let active = tab.tab == bag.tab;
        let want = if active {
            ui_theme::accent_ink()
        } else {
            ui_theme::button_border(UiButtonTone::Inventory)
        };
        if border.0 != want {
            *border = BorderColor(want);
        }
        let want_text = if active {
            ui_theme::text_primary()
        } else {
            ui_theme::text_muted()
        };
        for child in children.iter() {
            if let Ok(mut color) = text.get_mut(*child) {
                if color.0 != want_text {
                    *color = TextColor(want_text);
                }
            }
        }
    }
}

/// Las fichas de filtro de CONSTRUIR: cambia el grupo y **rehace la lista**.
///
/// Rehacerla en vez de esconder/mostrar no es capricho: con las ~350 tarjetas
/// vivas a la vez, abrir la mochila spawnea 350 botones con su icono y su texto
/// de una sentada — un tirón garantizado en el Redmi cada vez que tocás la
/// bolsa. Así vive sólo el grupo que estás mirando (entre 6 y 60 tarjetas).
#[allow(clippy::type_complexity)]
fn build_filter_system(
    mut commands: Commands,
    layout: Res<TouchLayout>,
    tune: Res<HudTune>,
    mut bag: ResMut<BagPanelState>,
    chips: Query<(&Interaction, &BuildFilterChip), Changed<Interaction>>,
    pager: Query<(&Interaction, &BuildPageBtn), Changed<Interaction>>,
    mut chip_style: Query<
        (&BuildFilterChip, &mut BorderColor, &mut BackgroundColor, &Children),
        With<Button>,
    >,
    mut chip_text: Query<&mut TextColor>,
    mut hint: Query<&mut Text, With<BuildGroupHint>>,
    list: Query<Entity, With<BagBuildList>>,
    mut scroll: Query<&mut ScrollPosition, With<BagScrollView>>,
) {
    if !bag.open {
        return;
    }
    for (interaction, chip) in &chips {
        if *interaction == Interaction::Pressed && bag.build_group != chip.0 {
            bag.build_group = chip.0;
            // Cambiar de ficha vuelve a la primera página: seguir en la 37 de la
            // ficha anterior deja la lista vacía sin explicar por qué.
            bag.build_page = 0;
            bag.build_dirty = true;
        }
    }
    for (interaction, boton) in &pager {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let paginas = build_groups()
            .get(bag.build_group)
            .map(|(g, _)| build_group_pages(*g))
            .unwrap_or(1);
        let destino = (bag.build_page as i32 + boton.0).clamp(0, paginas as i32 - 1) as usize;
        if destino != bag.build_page {
            bag.build_page = destino;
            bag.build_dirty = true;
        }
    }
    if !bag.build_dirty {
        return;
    }
    let group = bag.build_group;
    let page = bag.build_page;
    let heart = tune.heart_px;
    let card = bag_metrics(&layout, heart).card;

    // Sin lista todavía (el panel se está rearmando este frame) la marca se
    // queda puesta: si se consumiera acá, el grupo elegido no se dibujaría nunca.
    let Ok(list) = list.get_single() else { return };
    bag.build_dirty = false;
    commands.entity(list).despawn_descendants();
    commands.entity(list).with_children(|list| {
        fill_build_group(list, group, page, heart, card);
    });

    // La lista nueva empieza arriba: quedarse en el desplazamiento de la
    // anterior deja la pantalla en blanco si el grupo nuevo es más corto.
    for mut sp in &mut scroll {
        sp.offset_y = 0.0;
    }
    for (chip, mut border, mut bg, children) in &mut chip_style {
        let active = chip.0 == group;
        *border = BorderColor(if active {
            ui_theme::accent_ink()
        } else {
            ui_theme::button_border(UiButtonTone::Inventory)
        });
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Inventory,
            if active { UiButtonState::Pressed } else { UiButtonState::Normal },
        ));
        for child in children.iter() {
            if let Ok(mut color) = chip_text.get_mut(*child) {
                *color = TextColor(if active {
                    ui_theme::text_primary()
                } else {
                    ui_theme::text_muted()
                });
            }
        }
    }
    if let Ok(mut text) = hint.get_single_mut() {
        text.0 = build_group_hint(group).into();
    }
}

fn select_hotbar_slot(
    ui: &mut InventoryUiState,
    equipped: &mut EquippedItem,
    idx: usize,
) {
    if idx >= HOTBAR_ITEM_SLOTS {
        return;
    }
    ui.selected = Some(SlotId::Hotbar(idx));
    if let Some(ref id) = ui.hotbar[idx].item_id {
        if ui.hotbar[idx].qty > 0 && equipped.item_id != *id {
            equipped.item_id = id.clone();
        }
    }
}

fn bag_panel_system(
    time: Res<Time>,
    ctx: MochilaCtx,
    root: Query<Entity, With<GameUiRoot>>,
    editor_res: Res<crate::ui_editor::UiEditorState>,
    mut commands: Commands,
    mut bag_state: ResMut<BagPanelState>,
    mut craft_intent: ResMut<CraftIntent>,
    mut inv: ResMut<PlayerInventory>,
    mut ui: ResMut<InventoryUiState>,
    mut persist: ResMut<InventoryPersistState>,
    mut pending: ResMut<crate::machines::PendingMachinePlacement>,
    mut cache: ResMut<HudDisplayCache>,
    mut inventory_sections: Query<
        &mut Node,
        (
            With<BagInventorySection>,
            Without<BagCraftSection>,
            Without<BagPanelRoot>,
        ),
    >,
    mut craft_sections: Query<
        &mut Node,
        (
            With<BagCraftSection>,
            Without<BagInventorySection>,
            Without<BagPanelRoot>,
        ),
    >,
    mut ui_queries: ParamSet<(
        Query<
            (&mut Node, &mut BackgroundColor, &BagPanelBase),
            (
                With<BagPanelRoot>,
                Without<CraftRecipeButton>,
                Without<BagInventorySection>,
                Without<BagCraftSection>,
            ),
        >,
        Query<
            (&Interaction, &CraftRecipeButton, &mut BackgroundColor),
            (With<Button>, Without<BagPanelCloseButton>, Without<BagTabButton>),
        >,
        Query<
            (&CraftRecipeCostText, &mut Text),
            (With<CraftRecipeCostText>, Without<CraftStockText>),
        >,
        Query<
            (&Interaction, &mut BackgroundColor),
            (With<BagPanelCloseButton>, With<Button>, Changed<Interaction>),
        >,
        // Sin `Changed<Interaction>`: la pestaña activa se puede cambiar sin
        // tocarla (un driver, cerrar y reabrir la mochila), y con el filtro el
        // resaltado se quedaba en la pestaña anterior.
        Query<(&Interaction, &BagTabButton, &mut BackgroundColor), With<Button>>,
    )>,
) {
    let layout = &ctx.layout;
    let tune = &ctx.tune;
    let art = &ctx.art;
    let editor = &editor_res;
    // Abrir el MENU (su botón vive arriba al centro del HUD) CIERRA la mochila:
    // si no, el panel del menú quedaría flotando encima de una ventana que ocupa
    // la pantalla entera.
    if editor.menu_open && bag_state.open {
        bag_state.open = false;
    }
    if !bag_state.open {
        despawn_bag_panel(&mut commands, &mut bag_state);
        return;
    }

    if bag_state.modal.is_some() && bag_state.armado_con != Some(bag_state.tab) {
        despawn_bag_panel(&mut commands, &mut bag_state);
    }
    if bag_state.modal.is_none() {
        let Ok(root) = root.get_single() else {
            return;
        };
        bag_state.armado_con = Some(bag_state.tab);
        bag_state.modal = Some(spawn_bag_panel(
            &mut commands,
            root,
            layout,
            tune,
            art,
            bag_state.tab,
            editor.creative,
            bag_state.build_group,
            bag_state.build_page,
        ));
    }

    for (meta, mut text) in &mut ui_queries.p2() {
        if meta.is_machine_select {
            continue;
        }
        if let Some(recipe) = get_craft_recipe(meta.recipe_id) {
            let (_, line) = recipe_status(&inv, recipe);
            text.0 = line;
        }
    }

    for (interaction, mut bg) in &mut ui_queries.p3() {
        if *interaction == Interaction::Pressed {
            bag_state.open = false;
        }
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Primary,
            ui_theme::interaction_state(*interaction, false),
        ));
    }

    if !bag_state.open {
        despawn_bag_panel(&mut commands, &mut bag_state);
        return;
    }

    for (interaction, tab_btn, mut bg) in &mut ui_queries.p4() {
        if *interaction == Interaction::Pressed {
            bag_state.tab = tab_btn.tab;
        }
        let active = bag_state.tab == tab_btn.tab;
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Inventory,
            if active {
                UiButtonState::Pressed
            } else {
                ui_theme::interaction_state(*interaction, false)
            },
        ));
    }

    // SHIP section visibility is owned by `ship_builder_panel_system`.
    for mut node in &mut inventory_sections {
        node.display = if bag_state.tab == BagTab::Inventory {
            Display::Flex
        } else {
            Display::None
        };
    }
    for mut node in &mut craft_sections {
        node.display = if bag_state.tab == BagTab::Craft {
            Display::Flex
        } else {
            Display::None
        };
    }

    // ¿Se solto el dedo sin arrastrar? Entonces la tarjeta anotada cuenta como
    // TOQUE y se ejecuta. Si hubo arrastre, era scroll y se descarta.
    let confirmado = if !bag_state.dedo {
        match bag_state.pendiente.take() {
            Some(id) if !bag_state.es_scroll() => Some(id),
            _ => None,
        }
    } else {
        None
    };

    for (interaction, recipe_btn, mut bg) in &mut ui_queries.p1() {
        let can = if recipe_btn.is_machine_select {
            true
        } else if let Some(recipe) = get_craft_recipe(recipe_btn.recipe_id) {
            // Creative = craftable regardless of materials (Minecraft-style).
            editor.creative || can_craft(&inv, recipe)
        } else {
            false
        };

        // Al APOYAR solo se anota; `confirmado` de abajo decide al soltar.
        if *interaction == Interaction::Pressed && can {
            bag_state.pendiente = Some(recipe_btn.recipe_id);
            bag_state.recorrido = 0.0;
        }
        if confirmado == Some(recipe_btn.recipe_id) && can {
            if recipe_btn.is_machine_select {
                craft_intent.recipe_id = Some(recipe_btn.recipe_id);
                if recipe_btn.recipe_id == 10 {
                    pending.machine_id = WOOD_EXTRACTOR.id;
                } else if recipe_btn.recipe_id == 11 {
                    pending.machine_id = STONE_EXTRACTOR.id;
                } else if recipe_btn.recipe_id == 13 {
                    pending.machine_id = ha_shared::REGOLITH_DRILL.id;
                } else if recipe_btn.recipe_id == 14 {
                    pending.machine_id = ha_shared::ANTENNA.id;
                } else if recipe_btn.recipe_id == 15 {
                    pending.machine_id = ha_shared::HABITAT.id;
                }
                bag_state.open = false;
            } else if let Some(recipe) = get_craft_recipe(recipe_btn.recipe_id) {
                flush_slots_to_inventory(&ui, &mut inv);
                // Creative grants the output for free (no ingredient consumption).
                let ok = if editor.creative {
                    true
                } else if craft(&mut inv, recipe) {
                    consume_from_ui_slots(&mut ui, recipe.ingredients);
                    true
                } else {
                    false
                };
                if ok {
                    if !add_item_to_slots(&mut ui, recipe.output_id, recipe.output_qty) {
                        merge_inv_gains_into_ui(&mut ui, &inv);
                    }
                    flush_slots_to_inventory(&ui, &mut inv);
                    cache.inv_signature = inventory_signature(&inv);
                    persist.mark_immediate();
                }
            }
            bag_state.pulse.reset();
            *bg = BackgroundColor(ui_theme::button_bg(UiButtonTone::Craft, UiButtonState::Pressed));
        } else if can {
            *bg = BackgroundColor(ui_theme::button_bg(
                UiButtonTone::Craft,
                ui_theme::interaction_state(*interaction, false),
            ));
        } else {
            *bg = BackgroundColor(ui_theme::button_bg(UiButtonTone::Craft, UiButtonState::Disabled));
        }
    }

    if !bag_state.open {
        despawn_bag_panel(&mut commands, &mut bag_state);
        return;
    }

    bag_state.pulse.tick(time.delta());
    let progress = if bag_state.pulse.duration().as_secs_f32() > 0.0 {
        (bag_state.pulse.elapsed_secs() / bag_state.pulse.duration().as_secs_f32()).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let e = elastic_out(progress);
    if let Ok((mut node, mut bg, base)) = ui_queries.p0().get_single_mut() {
        // **El pulso ya no escala el panel.** Con la ventana a pantalla completa,
        // ese 6% de rebote la empujaba fuera del borde (y del recorte del padre):
        // el feedback de "crafteaste algo" queda en el destello del fondo, que se
        // ve igual y no mueve el layout de nada.
        node.width = Val::Px(base.width);
        node.height = Val::Px(base.height);
        // **Sin color de panel.** Esto repintaba el fondo AZUL de la paleta en
        // cada frame, deshaciendo el `Color::NONE` del spawn: el asset de madera
        // se dibujaba encima y el azul asomaba por todo el borde. El destello de
        // "crafteaste algo" ahora es un velo BLANCO transparente sobre la
        // madera, que se lee igual y no tapa el asset.
        let glow = (1.0 - e) * 0.18;
        *bg = BackgroundColor(Color::srgba(1.0, 1.0, 1.0, glow));
    }
}

fn elastic_out(t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let c4 = (2.0 * std::f32::consts::PI) / 3.0;
    (2.0_f32).powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
}

pub(crate) fn update_interact_hud(
    mut commands: Commands,
    mut hold: ResMut<HoldInteractState>,
    cooldown: Res<AbilityCooldown>,
    equipped: Res<EquippedItem>,
    target: Res<crate::targeting::LookTarget>,
    ctx: Res<crate::scene::WorldContextRuntime>,
    planet: Res<crate::world2d::ActivePlanet>,
    mut inv: ResMut<PlayerInventory>,
    mut inv_ui: ResMut<InventoryUiState>,
    mut persist: ResMut<InventoryPersistState>,
    mut flora: ResMut<crate::harvest::FloraHarvest>,
    mut cache: ResMut<HudDisplayCache>,
    mut ui: ParamSet<(
        Query<&mut Node, With<HoldBarRoot>>,
        Query<&mut Node, With<HoldBarFill>>,
        Query<(&mut Node, &mut BorderColor), With<AbilityCooldownRing>>,
    )>,
) {
    if hold.completed {
        if hold.placing_machine {
            // Machine placement consumes hold in try_place_on_hold (runs before this system).
        } else if let Some(kind) = target.kind {
            if hold_target_handled_elsewhere(kind) {
                // Launch pad / orbital travel systems consume hold.completed first.
            } else if let Some(rule) =
                ha_shared::resolve_interaction(Some(equipped.item_id.as_str()), kind)
            {
                match rule.kind {
                    ha_shared::InteractActionKind::Gather
                    | ha_shared::InteractActionKind::Mine
                    | ha_shared::InteractActionKind::Collect => {
                        hold.completed = false;
                        if let Some(item_id) = rule.item_id {
                            add_item_to_slots(&mut inv_ui, item_id, 1);
                            if rule.kind == ha_shared::InteractActionKind::Gather
                                && kind == InteractTargetKind::Tree
                                && equipped.item_id == "hand"
                            {
                                add_item_to_slots(&mut inv_ui, "sticks", 1);
                            }
                            // Harvestable decor (tree/rock/ore/gem): remove it from
                            // the world NOW - despawn the sprite - and persist the
                            // removal (per the ACTIVE body, so it stays gone on
                            // every planet and after the chunk re-streams). A tree
                            // also drops a deterministic apple.
                            if let Some((cx, cz, pid)) = target.flora_cell {
                                if kind == InteractTargetKind::Tree {
                                    let hash =
                                        cx as i64 * 7 + cz as i64 * 13 + pid as i64;
                                    if hash.rem_euclid(3) == 0 {
                                        add_item_to_slots(&mut inv_ui, "apple", 1);
                                    }
                                }
                                let body = crate::world2d::active_surface_body(&ctx, &planet);
                                flora.harvest(body, cx, cz, pid);
                                if let Some(e) = target.entity {
                                    commands.entity(e).despawn_recursive();
                                }
                            }
                            flush_slots_to_inventory(&inv_ui, &mut inv);
                            cache.inv_signature = inventory_signature(&inv);
                            persist.mark_immediate();
                        }
                    }
                    _ => {}
                }
            } else {
                hold.completed = false;
            }
        } else if !hold.is_placement() {
            // Placement holds have no target kind - their consumers
            // (world2d/machines/build/chests/torches/map) own hold.completed.
            hold.completed = false;
        }
    }

    let hold_visible = hold.active;
    let hold_pct = if hold.duration_ms > 0 {
        (hold.progress_ms as f32 / hold.duration_ms as f32).clamp(0.0, 1.0) * 100.0
    } else {
        0.0
    };
    if cache.hold_visible != hold_visible {
        cache.hold_visible = hold_visible;
        if let Ok(mut root) = ui.p0().get_single_mut() {
            root.display = if hold_visible {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
    if hold_visible && (cache.hold_pct - hold_pct).abs() > 0.5 {
        cache.hold_pct = hold_pct;
        if let Ok(mut fill) = ui.p1().get_single_mut() {
            fill.width = Val::Percent(hold_pct);
        }
    }

    let ready = cooldown.remaining_ms <= 0.0;
    let cooldown_visible = cooldown.total_ms > 0.0 && !ready;
    let cooldown_alpha = if cooldown.total_ms > 0.0 {
        let pct = 1.0 - (cooldown.remaining_ms / cooldown.total_ms).clamp(0.0, 1.0);
        if ready { 0.35 } else { 0.55 + pct * 0.35 }
    } else {
        0.35
    };
    if cache.cooldown_visible != cooldown_visible {
        cache.cooldown_visible = cooldown_visible;
        if let Ok((mut node, _)) = ui.p2().get_single_mut() {
            node.display = if cooldown_visible {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
    if cooldown_visible && (cache.cooldown_alpha - cooldown_alpha).abs() > 0.02 {
        cache.cooldown_alpha = cooldown_alpha;
        if let Ok((_, mut border)) = ui.p2().get_single_mut() {
            *border = BorderColor(ui_theme::accent_magenta().with_alpha(cooldown_alpha));
        }
    }
}

fn lantern_toggle_system(
    keys: Res<ButtonInput<KeyCode>>,
    equipped: Res<EquippedItem>,
    editor: Res<crate::ui_editor::UiEditorState>,
    gameplay_block: Res<UiGameplayBlock>,
    mut ctrl: ResMut<PlayerLanternControl>,
    buttons: Query<&Interaction, (With<LanternToggleButton>, Changed<Interaction>)>,
) {
    // Cualquier ítem que ilumine en la mano (antorcha, linterna…) puede apagarse.
    if crate::held_item::item_light(&equipped.item_id).is_none() {
        return;
    }
    if !editor.menu_open && !editor.enabled && !gameplay_block.blocked {
        if keys.just_pressed(KeyCode::KeyL) {
            ctrl.lit = !ctrl.lit;
        }
    }
    for interaction in &buttons {
        if *interaction == Interaction::Pressed {
            ctrl.lit = !ctrl.lit;
        }
    }
}

fn update_lantern_toggle_ui(
    equipped: Res<EquippedItem>,
    ctrl: Res<PlayerLanternControl>,
    layout: Res<TouchLayout>,
    mut root: Query<(&mut Node, &mut Visibility), With<LanternToggleRoot>>,
    mut labels: Query<&mut Text, With<LanternToggleLabel>>,
    mut buttons: Query<&mut BackgroundColor, With<LanternToggleButton>>,
) {
    let show = crate::held_item::item_light(&equipped.item_id).is_some();
    let short = layout.width.min(layout.height);
    let (_, _, total, bottom) = hotbar_layout_metrics(&layout);
    let btn_h = (short * 0.052).clamp(28.0, 40.0);
    let btn_w = btn_h * 2.1;

    for (mut node, mut vis) in &mut root {
        *vis = if show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        node.left = Val::Px(game_ui::ui_center_left(&layout, total) + total + 8.0);
        node.bottom = Val::Px(bottom);
        node.width = Val::Px(btn_w);
        node.height = Val::Px(btn_h);
    }
    let label = if ctrl.lit { "LAMP ON" } else { "LAMP OFF" };
    for mut text in &mut labels {
        if text.0 != label {
            text.0 = label.to_string();
        }
    }
    let tone = if ctrl.lit {
        UiButtonState::Pressed
    } else {
        UiButtonState::Normal
    };
    for mut bg in &mut buttons {
        *bg = BackgroundColor(ui_theme::button_bg(UiButtonTone::Use, tone));
    }
}

fn hotbar_hammer_system(
    mut equipped: ResMut<EquippedItem>,
    mut ui: ResMut<InventoryUiState>,
    hammer: Query<&Interaction, (With<HotbarHammerSlot>, Changed<Interaction>)>,
) {
    for interaction in &hammer {
        if *interaction == Interaction::Pressed && equipped.item_id != "hammer" {
            equipped.item_id = "hammer".to_string();
            ui.selected = None;
        }
    }
}

/// Floating icon while long-press dragging a stack.
fn update_held_item_ghost(
    mut commands: Commands,
    ui: Res<InventoryUiState>,
    layout: Res<TouchLayout>,
    root: Query<Entity, With<GameUiRoot>>,
    mut ghost: Query<(Entity, &mut Node, &mut Visibility), With<HeldItemGhost>>,
    mut images: ResMut<Assets<Image>>,
    mut icons: ResMut<ItemIconCache>,
    mut image_nodes: Query<&mut ImageNode, With<HeldItemGhost>>,
) {
    let Ok(root_e) = root.get_single() else {
        return;
    };
    let held = ui.held.as_ref();
    if held.is_none() {
        for (_, _, mut vis) in &mut ghost {
            *vis = Visibility::Hidden;
        }
        return;
    }
    let held = held.unwrap();
    let short = layout.width.min(layout.height);
    let size = (short * 0.06).clamp(32.0, 48.0);
    let bottom = layout.insets.bottom as f32 + (short * 0.14).clamp(56.0, 90.0);

    if ghost.is_empty() {
        let icon = icons.icon(&held.item_id, &mut images);
        commands.entity(root_e).with_children(|p| {
            p.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(layout.width * 0.5 - size * 0.5),
                    bottom: Val::Px(bottom),
                    width: Val::Px(size),
                    height: Val::Px(size),
                    ..default()
                },
                icon.node(),
                Visibility::Visible,
                HeldItemGhost,
                Name::new("HeldItemGhost"),
            ));
        });
        return;
    }

    for (e, mut node, mut vis) in &mut ghost {
        *vis = Visibility::Visible;
        node.left = Val::Px(layout.width * 0.5 - size * 0.5);
        node.bottom = Val::Px(bottom);
        node.width = Val::Px(size);
        node.height = Val::Px(size);
        let icon = icons.icon(&held.item_id, &mut images);
        if let Ok(mut img) = image_nodes.get_mut(e) {
            img.image = icon.image.clone();
            img.rect = icon.rect;
        }
    }
}

fn hold_target_handled_elsewhere(kind: InteractTargetKind) -> bool {
    matches!(
        kind,
        InteractTargetKind::Block
            | InteractTargetKind::Chest
            // Objeto colocado: lo levanta `structures::pick_up_structure`.
            | InteractTargetKind::Entity
            // Prop sembrado por el mundo: lo levanta `harvest::pick_up_prop`,
            // que es quien sabe qué material devolver (sale del catálogo del
            // propio objeto, no de la tabla estática de reglas).
            | InteractTargetKind::Prop
            // Granja: los resuelven `granja.rs` (cultivo) y `fauna.rs` (animal).
            | InteractTargetKind::Crop
            | InteractTargetKind::Animal
            | InteractTargetKind::LaunchModule
            | InteractTargetKind::LaunchIgnition
            | InteractTargetKind::SpaceReentry
            | InteractTargetKind::SpaceTransitLuna
            | InteractTargetKind::SpaceStationModule
            | InteractTargetKind::LunarReturn
    )
}

fn sync_ui_gameplay_block(
    bag: Res<BagPanelState>,
    chest: Res<crate::chests::ChestPanelState>,
    editor: Res<crate::ui_editor::UiEditorState>,
    phase: Res<State<crate::game_phase::GamePhase>>,
    bautizo: Res<crate::bautizo::Bautizo>,
    mut block: ResMut<UiGameplayBlock>,
) {
    // Only REAL UI blocks gameplay input. Flight must NOT block (B8): Ki thrust
    // (JUMP/USE) and steering run through the same action pipeline.
    //
    // La PANTALLA DE CARGA también bloquea: el jugador ya existe detrás de la
    // cortina, así que sin esto una tecla apoyada lo hacía caminar a ciegas
    // mientras se armaba el mundo, y aparecías en otro lado del que te dejaron.
    let loading = *phase.get() == crate::game_phase::GamePhase::Loading;
    block.blocked = loading
        || bag.open
        || chest.is_open()
        || editor.menu_open
        || editor.enabled
        || bautizo.abierto();
}

/// Public so the touch-stick systems can order themselves AFTER it - otherwise
/// the sticks may read `TouchBlockRects` before the hotbar band is appended and
/// steal taps meant for the hotbar (item never gets equipped). See `InputPlugin`.
pub(crate) fn append_hud_touch_blocks(
    layout: Res<TouchLayout>,
    bag: Res<BagPanelState>,
    chest: Res<crate::chests::ChestPanelState>,
    hud_blocks: Res<HudTouchBlocks>,
    mut blocks: ResMut<TouchBlockRects>,
) {
    blocks.0.extend(hud_blocks.0.iter().copied());
    if bag.open || chest.is_open() {
        blocks.0.push(fullscreen_block_rect(&layout));
    }
}

fn sync_inventory_from_player(
    inv: Res<PlayerInventory>,
    mut ui: ResMut<InventoryUiState>,
    mut equipped: ResMut<EquippedItem>,
    mut cache: ResMut<HudDisplayCache>,
) {
    let sig = inventory_signature(&inv);
    if !ui.loaded_from_disk {
        load_grid_from_inventory(&mut ui, &inv);
        ui.loaded_from_disk = true;
        if ui.hotbar[0].item_id.as_deref() == Some("lantern") && ui.hotbar[0].qty > 0 {
            ui.selected = Some(SlotId::Hotbar(0));
            equipped.item_id = "lantern".to_string();
        }
        cache.inv_signature = sig;
        return;
    }
    if ui.held.is_some() {
        return;
    }
    let grid_sig = grid_signature(&ui);
    if sig == cache.inv_signature && !grid_sig.is_empty() {
        return;
    }
    if flush_matches_inventory(&ui, &inv) {
        cache.inv_signature = sig;
        return;
    }
    if merge_inv_gains_into_ui(&mut ui, &inv) {
        cache.inv_signature = sig;
        return;
    }
    load_grid_from_inventory(&mut ui, &inv);
    cache.inv_signature = sig;
}

fn flush_matches_inventory(ui: &InventoryUiState, inv: &PlayerInventory) -> bool {
    let mut tmp = PlayerInventory::default();
    flush_slots_to_inventory(ui, &mut tmp);
    tmp.items == inv.items
}

fn visit_slot_entities(
    children: &[Entity],
    children_q: &Query<&Children>,
    visit: &mut impl FnMut(Entity),
) {
    let mut stack: Vec<Entity> = children.to_vec();
    while let Some(entity) = stack.pop() {
        visit(entity);
        if let Ok(kids) = children_q.get(entity) {
            stack.extend(kids.iter().copied());
        }
    }
}

fn update_inventory_slot_visuals(
    bag: Res<BagPanelState>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut icons: ResMut<ItemIconCache>,
    ui: Res<InventoryUiState>,
    span: Res<CpuSpanStats>,
    children_q: Query<&Children>,
    mut slots: Query<
        (&InventoryGridSlot, &mut BorderColor, &Children),
        Without<HotbarItemSlot>,
    >,
    mut icon_markers: Query<
        (Entity, &mut Visibility),
        (With<InventorySlotIcon>, Without<HotbarSlotIcon>),
    >,
    mut image_nodes: Query<&mut ImageNode, With<InventorySlotIcon>>,
    mut slot_texts: ParamSet<(
        Query<&mut Text, (With<InventorySlotQty>, Without<HotbarSlotQty>)>,
        Query<&mut Text, (With<InventorySlotName>, Without<HotbarSlotName>)>,
    )>,
    nuevas: Query<(), Added<InventoryGridSlot>>,
) {
    if !bag.open {
        return;
    }
    // **Sólo cuando algo cambió.** Reescribir el `ImageNode` de 36 casillas cada
    // frame las marcaba cambiadas y la UI se re-armaba entera cada frame: con la
    // mochila abierta el teléfono bajaba a 15 fps (captura del 29 sep 2026).
    if !ui.is_changed() && nuevas.is_empty() {
        return;
    }
    let _guard = span_guard(&span, CpuSpanSlot::Hud);
    for (slot, mut border, children) in &mut slots {
        let stack = &ui.grid[slot.index];
        let selected = ui.selected == Some(SlotId::Grid(slot.index));
        *border = BorderColor(if selected {
            ui_theme::accent_cyan()
        } else if ui.held.as_ref().is_some_and(|h| h.from == SlotId::Grid(slot.index)) {
            ui_theme::accent_magenta()
        } else {
            ui_theme::button_border(UiButtonTone::Inventory)
        });

        visit_slot_entities(children, &children_q, &mut |entity| {
            if let Ok((icon_e, mut vis)) = icon_markers.get_mut(entity) {
                let icon = if stack.is_empty() {
                    icons.empty_slot(&mut images)
                } else {
                    icons.icon(stack.item_id.as_deref().unwrap_or("wood"), &mut images)
                };
                *vis = Visibility::Visible;
                if let Ok(mut node) = image_nodes.get_mut(entity) {
                    // Imagen Y recorte: cambiar sólo el handle deja el slot con el
                    // rect del ítem anterior (media hoja de atlas por icono).
                    node.image = icon.image.clone();
                    node.rect = icon.rect;
                } else {
                    commands.entity(icon_e).try_insert(icon.node());
                }
            }
        });
        {
            let mut qty_labels = slot_texts.p0();
            visit_slot_entities(children, &children_q, &mut |entity| {
                if let Ok(mut text) = qty_labels.get_mut(entity) {
                    let label = if stack.qty > 1 {
                        format!("{}", stack.qty)
                    } else {
                        String::new()
                    };
                    if text.0 != label {
                        text.0 = label;
                    }
                }
            });
        }
        {
            let mut name_labels = slot_texts.p1();
            visit_slot_entities(children, &children_q, &mut |entity| {
                if let Ok(mut text) = name_labels.get_mut(entity) {
                    let label = if stack.is_empty() {
                        String::new()
                    } else {
                        item_display_name(stack.item_id.as_deref().unwrap_or("wood")).to_string()
                    };
                    if text.0 != label {
                        text.0 = label;
                    }
                }
            });
        }
    }
}

fn update_hotbar_item_visuals(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut icons: ResMut<ItemIconCache>,
    ui: Res<InventoryUiState>,
    children_q: Query<&Children>,
    mut slots: Query<
        (&HotbarItemSlot, &mut BorderColor, &mut BackgroundColor, &Children),
        Without<InventoryGridSlot>,
    >,
    mut icon_markers: Query<
        (Entity, &mut Visibility),
        (With<HotbarSlotIcon>, Without<InventorySlotIcon>),
    >,
    mut image_nodes: Query<
        &mut ImageNode,
        (
            With<HotbarSlotIcon>,
            Without<HotbarHammerIcon>,
            Without<HotbarBagIcon>,
        ),
    >,
    mut slot_texts: ParamSet<(
        Query<&mut Text, (With<HotbarSlotQty>, Without<InventorySlotQty>)>,
        Query<&mut Text, (With<HotbarSlotName>, Without<InventorySlotName>)>,
    )>,
    span: Res<CpuSpanStats>,
    nuevas: Query<(), Added<HotbarItemSlot>>,
) {
    // Igual que la grilla: tocar el `ImageNode` y el fondo de la barra cada frame
    // re-armaba la UI entera sesenta veces por segundo, jugando o no.
    if !ui.is_changed() && nuevas.is_empty() {
        return;
    }
    let _guard = span_guard(&span, CpuSpanSlot::Hud);
    for (slot, mut border, mut bg, children) in &mut slots {
        let stack = &ui.hotbar[slot.index];
        let selected = ui.selected == Some(SlotId::Hotbar(slot.index));
        *border = BorderColor(if selected {
            ui_theme::accent_cyan()
        } else if ui.held.as_ref().is_some_and(|h| h.from == SlotId::Hotbar(slot.index)) {
            ui_theme::accent_magenta()
        } else {
            ui_theme::button_border(UiButtonTone::Inventory)
        });
        // El slot equipado se lee de un vistazo: borde cian + celda encendida.
        // Sin esto "seleccionado" era un borde fino que en el teléfono no se ve.
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Inventory,
            if selected {
                UiButtonState::Pressed
            } else {
                UiButtonState::Normal
            },
        ));
        visit_slot_entities(children, &children_q, &mut |entity| {
            if let Ok((icon_e, mut vis)) = icon_markers.get_mut(entity) {
                let icon = if stack.is_empty() {
                    icons.empty_slot(&mut images)
                } else {
                    icons.icon(stack.item_id.as_deref().unwrap_or("wood"), &mut images)
                };
                *vis = Visibility::Visible;
                if let Ok(mut node) = image_nodes.get_mut(entity) {
                    // Imagen Y recorte: cambiar sólo el handle deja el slot con el
                    // rect del ítem anterior (media hoja de atlas por icono).
                    node.image = icon.image.clone();
                    node.rect = icon.rect;
                } else {
                    commands.entity(icon_e).try_insert(icon.node());
                }
            }
        });
        {
            let mut qty_labels = slot_texts.p0();
            visit_slot_entities(children, &children_q, &mut |entity| {
                if let Ok(mut text) = qty_labels.get_mut(entity) {
                    let label = if stack.qty > 1 {
                        format!("{}", stack.qty)
                    } else {
                        String::new()
                    };
                    if text.0 != label {
                        text.0 = label;
                    }
                }
            });
        }
        {
            let mut name_labels = slot_texts.p1();
            visit_slot_entities(children, &children_q, &mut |entity| {
                if let Ok(mut text) = name_labels.get_mut(entity) {
                    // **Sin rótulo debajo del icono** (29 sep 2026): eran cuatro
                    // letras en inglés ("Ston", "Stic") montadas sobre el marco de
                    // la casilla. El icono ya dice qué es.
                    let _ = item_hotbar_label;
                    let label = String::new();
                    if text.0 != label {
                        text.0 = label;
                    }
                }
            });
        }
    }
}

fn sync_hotbar_chrome_icons(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut icons: ResMut<ItemIconCache>,
    mut hammer: Query<
        (Entity, &mut ImageNode),
        (With<HotbarHammerIcon>, Without<HotbarBagIcon>, Without<HotbarSlotIcon>),
    >,
    mut bag: Query<
        (Entity, &mut ImageNode),
        (With<HotbarBagIcon>, Without<HotbarHammerIcon>, Without<HotbarSlotIcon>),
    >,
    hammer_missing: Query<
        Entity,
        (
            With<HotbarHammerIcon>,
            Without<ImageNode>,
            Without<HotbarBagIcon>,
        ),
    >,
    bag_missing: Query<
        Entity,
        (
            With<HotbarBagIcon>,
            Without<ImageNode>,
            Without<HotbarHammerIcon>,
        ),
    >,
) {
    let hammer_icon = icons.icon("hammer", &mut images);
    for e in &hammer_missing {
        commands.entity(e).try_insert(hammer_icon.node());
    }
    for (_, mut node) in &mut hammer {
        node.image = hammer_icon.image.clone();
        node.rect = hammer_icon.rect;
    }

    let bag_icon = icons.bag_slot(&mut images);
    for e in &bag_missing {
        commands.entity(e).try_insert(bag_icon.node());
    }
    for (_, mut node) in &mut bag {
        node.image = bag_icon.image.clone();
        node.rect = bag_icon.rect;
    }
}

fn equip_from_hotbar_selection(
    ui: Res<InventoryUiState>,
    mut equipped: ResMut<EquippedItem>,
) {
    let Some(SlotId::Hotbar(idx)) = ui.selected else {
        return;
    };
    let stack = &ui.hotbar[idx];
    if stack.is_empty() {
        // El slot seleccionado se vació (lo gastaste o lo mandaste a un cofre):
        // volvés a mano vacía, y el ítem desaparece de la mano del personaje.
        if equipped.item_id != "hand" && equipped.item_id != "hammer" {
            equipped.item_id = "hand".to_string();
        }
        return;
    }
    let id = stack.item_id.clone().unwrap_or_default();
    if equipped.item_id != id {
        equipped.item_id = id;
    }
}

/// Tapping/clicking a hotbar item cell selects + equips it - works for both
/// touch (mobile) and mouse (desktop), so the hotbar is usable without the bag.
fn equip_from_hotbar_click(
    bag: Res<BagPanelState>,
    mut ui: ResMut<InventoryUiState>,
    mut equipped: ResMut<EquippedItem>,
    mut build_mode: ResMut<crate::input::BuildModeActive>,
    slots: Query<(&Interaction, &HotbarItemSlot), Changed<Interaction>>,
) {
    // Con la mochila abierta el toque en la hotbar significa MOVER (destino del
    // stack seleccionado), no equipar: si equipáramos aquí pisaríamos
    // `ui.selected` y sacar un ítem del grid a la hotbar sería imposible.
    if bag.open {
        return;
    }
    for (interaction, slot) in &slots {
        if *interaction == Interaction::Pressed {
            // Picking a hotbar item leaves build mode so USE acts on the item.
            build_mode.active = false;
            select_hotbar_slot(&mut ui, &mut equipped, slot.index);
        }
    }
}

/// How long the "en mano: X" toast stays before it starts vanishing, and the
/// fade-out window (alpha ramps down over the final `FADE` seconds).
const EQUIP_TOAST_HOLD: f32 = 1.1;
const EQUIP_TOAST_FADE: f32 = 0.8;

/// The floating "en mano: <item>" label that appears when the equipped item
/// changes and then fades away (only the text fades - nothing stays on screen).
#[derive(Component)]
struct EquipToast {
    /// Seconds of visible life remaining; 0 = hidden.
    life: f32,
}

fn equip_toast_system(
    time: Res<Time>,
    equipped: Res<EquippedItem>,
    mut last: Local<Option<String>>,
    mut q: Query<(&mut EquipToast, &mut Text, &mut TextColor, &mut Node)>,
) {
    // Seed on first run so we don't flash a toast on boot.
    if last.is_none() {
        *last = Some(equipped.item_id.clone());
        return;
    }
    let changed = last.as_deref() != Some(equipped.item_id.as_str());
    if changed {
        *last = Some(equipped.item_id.clone());
    }
    for (mut toast, mut text, mut color, mut node) in &mut q {
        if changed {
            let name = item_display_name(&equipped.item_id);
            text.0 = format!("en mano: {name}");
            toast.life = EQUIP_TOAST_HOLD + EQUIP_TOAST_FADE;
            node.display = Display::Flex;
        }
        if toast.life > 0.0 {
            toast.life = (toast.life - time.delta_secs()).max(0.0);
            let alpha = (toast.life / EQUIP_TOAST_FADE).clamp(0.0, 1.0);
            *color = TextColor(ui_theme::accent_cyan().with_alpha(alpha));
            if toast.life <= 0.0 {
                node.display = Display::None;
            }
        }
    }
}

fn spawn_equip_toast(parent: &mut ChildBuilder, layout: &TouchLayout, heart: f32) {
    let (slot, gap, _, bottom) = hotbar_layout_metrics(layout);
    parent
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                bottom: Val::Px(bottom + slot + gap + 6.0),
                width: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Name::new("EquipToastRow"),
        ))
        .with_children(|row| {
            row.spawn((
                Text::new(""),
                TextFont {
                    font_size: (heart * 0.5).clamp(12.0, 18.0),
                    ..default()
                },
                TextColor(ui_theme::accent_cyan().with_alpha(0.0)),
                Node {
                    display: Display::None,
                    ..default()
                },
                EquipToast { life: 0.0 },
            ));
        });
}

#[cfg(test)]
mod bag_tests {
    use super::*;

    /// El tope del scroll sale de MEDIR, no de un número. Antes era
    /// `clamp(0.0, 900.0)`: con la paleta de CONSTRUIR en ~350 tarjetas (más de
    /// 3000 px de alto), dos tercios de los objetos del juego estaban declarados
    /// y eran inalcanzables con el dedo.
    #[test]
    fn el_scroll_llega_hasta_el_final_de_la_lista() {
        // Lista larga: se puede bajar exactamente lo que sobresale.
        assert_eq!(max_scroll(350.0, 3510.0), 3160.0);
        // Lista que entra entera: no se mueve.
        assert_eq!(max_scroll(350.0, 120.0), 0.0);
        assert_eq!(max_scroll(350.0, 350.0), 0.0);
    }

    /// Cada ficha de la paleta abre algo. Una ficha que abre una lista vacía se
    /// lee como que el juego se rompió.
    #[test]
    fn ningun_grupo_de_construir_esta_vacio() {
        for (i, (group, label)) in build_groups().iter().enumerate() {
            let n = build_group_len(*group);
            assert!(n > 0, "el grupo {label} (#{i}) de CONSTRUIR no tiene ni una pieza");
            let rotos: String =
                label.chars().filter(|c| !crate::fuente::dibujable(*c)).collect();
            assert!(rotos.is_empty(), "la fuente no dibuja [{rotos}] en la ficha {label}");
        }
    }

    /// **Ninguna ficha escupe el catálogo entero de una.** Con el pack completo
    /// importado, MUEBLES tiene 2.878 colocables: instanciarlos juntos son 2.878
    /// botones con icono y texto en un frame, que es el tirón que la paleta por
    /// fichas vino a evitar cuando eran 350. La página acota el gasto de abrir
    /// cualquier ficha, mire lo que mire.
    #[test]
    fn ninguna_pagina_de_construir_es_gigante() {
        for (group, label) in build_groups() {
            let n = build_group_len(group);
            let en_pantalla = n.min(BUILD_POR_PAGINA);
            assert!(
                en_pantalla <= BUILD_POR_PAGINA,
                "la ficha {label} dibuja {en_pantalla} tarjetas de una"
            );
            // Y el paginado tiene que cubrir la ficha entera: si la última página
            // se corta, hay objetos que no se pueden elegir desde el menú.
            assert!(
                build_group_pages(group) * BUILD_POR_PAGINA >= n,
                "{label} tiene {n} piezas y sólo {} páginas",
                build_group_pages(group)
            );
        }
    }

    /// Todos los colocables del pack entran en alguna ficha: la suma de los
    /// grupos de categoría tiene que dar la tabla entera. Si mañana se importa
    /// una tanda nueva con una categoría sin ficha, esto lo caza acá y no en el
    /// teléfono buscando una heladera que no está.
    #[test]
    fn todos_los_colocables_tienen_ficha() {
        let en_fichas: usize = build_groups()
            .iter()
            .filter_map(|(g, _)| match g {
                BuildGroup::Place(cat) => Some(ha_shared::placeables::placeables_in(*cat).count()),
                _ => None,
            })
            .sum();
        assert_eq!(
            en_fichas,
            ha_shared::placeables::PLACEABLES.len(),
            "hay colocables que no aparecen en ninguna ficha de CONSTRUIR"
        );
    }
}

/// **El texto de la UI tiene que ser DIBUJABLE — no ASCII.**
///
/// Durante mucho tiempo la regla fue "UI ASCII-only", y no era una decisión de
/// diseño: era un parche por la fuente. Bevy trae por defecto un subconjunto
/// minúsculo sin acentos, así que se escribía "Presiona ACCION" para no ver una
/// caja donde iba la `Ó`. El resultado era una UI que hablaba mal el castellano
/// por una limitación que se arregla con un archivo de 220 KB.
///
/// Ahora el juego trae su fuente (`assets/fonts/ui.ttf`, ver `fuente.rs`) y la
/// regla es la que siempre debió ser: **cada carácter que llega a la pantalla
/// tiene que existir en la fuente que embarcamos**. Los acentos pasan porque la
/// fuente los tiene; un emoji o un `▲` siguen fallando el test porque no los
/// tiene — y ahí la caja sería real.
///
/// Es estrictamente mejor que el veto ASCII en las dos direcciones: deja escribir
/// castellano de verdad, y caza cosas que el veto no cazaba (un `→` era ASCII
/// para nadie, pero un `·` sí pasaba desapercibido en revisión).
///
/// ## Por qué mira TODOS los literales y no sólo `Text::new("...")`
///
/// Buscar el patrón `Text::new("` en una línea dejaba pasar todo texto que no
/// llega a la pantalla escrito de una pieza: el armado con `format!`, el que
/// devuelve una función auxiliar y el que se guarda en una variable. Por ese
/// hueco entró `society.rs::nearest_settlement_line`, que devolvía una raya
/// larga a la brújula de asentamientos — una caja en pantalla durante meses,
/// con el test en verde.
#[cfg(test)]
mod regla_6 {
    use std::path::Path;

    /// Los glifos que la fuente embarcada sabe dibujar. Si esto falla al abrir
    /// el archivo, el test falla — una fuente que no carga es exactamente el
    /// bug que este test existe para impedir.
    use crate::fuente::dibujable;

    /// Contextos que NO terminan en pantalla: van a logcat, a un `panic!` o al
    /// mensaje de un test. Ahí una raya larga se lee bien y prohibirla sería
    /// empobrecer los diagnósticos por nada.
    const NO_ES_PANTALLA: &[&str] = &[
        "eprintln!",
        "println!",
        "print!",
        "assert",
        "panic!",
        "unreachable!",
        "todo!",
        ".expect(",
        "report_bug",
        "#[",
    ];

    /// Literales de cadena de una línea de código, respetando los escapes.
    /// Deliberadamente simple: no entiende raw strings ni literales cortados en
    /// dos líneas, y en ambos casos falla hacia "no lo mires" — un falso
    /// negativo raro es preferible a que el test sea imposible de mantener.
    fn literales(code: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut chars = code.chars().peekable();
        while let Some(c) = chars.next() {
            if c != '"' {
                continue;
            }
            let mut lit = String::new();
            let mut cerrado = false;
            while let Some(c) = chars.next() {
                match c {
                    '\\' => {
                        chars.next();
                    }
                    '"' => {
                        cerrado = true;
                        break;
                    }
                    _ => lit.push(c),
                }
            }
            if cerrado {
                out.push(lit);
            }
        }
        out
    }

    fn scan(dir: &Path, bad: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("src legible").flatten() {
            let path = entry.path();
            // Los drivers de QA (`debug/`) escriben a logcat, nunca a la
            // pantalla del jugador: ahí un `→` es un log legible, no una caja.
            if path.file_name().and_then(|n| n.to_str()) == Some("debug") {
                continue;
            }
            if path.is_dir() {
                scan(&path, bad);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let src = std::fs::read_to_string(&path).unwrap_or_default();
            // Profundidad de paréntesis que llevamos DENTRO de una llamada que no
            // se dibuja. Un `eprintln!(` que sigue en las líneas de abajo tiene
            // su texto lejos del nombre del macro, y mirando línea por línea
            // parecía texto de UI (fue el primer falso positivo que dio esto).
            let mut en_no_pantalla = 0i32;
            for (n, line) in src.lines().enumerate() {
                // Fuera los comentarios: ahí el castellano se escribe bien, con
                // acentos. (Corta también dentro de un string que contenga `//`,
                // que es un falso NEGATIVO tolerable — nunca uno positivo.)
                let code = line.split("//").next().unwrap_or("");
                if code.trim().is_empty() {
                    continue;
                }
                let abre = code.matches('(').count() as i32;
                let cierra = code.matches(')').count() as i32;
                if en_no_pantalla > 0 {
                    en_no_pantalla += abre - cierra;
                    continue;
                }
                if NO_ES_PANTALLA.iter().any(|p| code.contains(p)) {
                    // Si la llamada no cierra en esta línea, lo que sigue
                    // también es parte de ella.
                    en_no_pantalla = (abre - cierra).max(0);
                    continue;
                }
                for literal in literales(code) {
                    let rotos: String = literal.chars().filter(|c| !dibujable(*c)).collect();
                    if !rotos.is_empty() {
                        bad.push(format!(
                            "{}:{}: la fuente no dibuja [{rotos}] en {literal:?}",
                            path.display(),
                            n + 1
                        ));
                    }
                }
            }
        }
    }

    #[test]
    fn la_fuente_dibuja_todo_el_texto_de_la_ui() {
        let mut bad = Vec::new();
        scan(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(), &mut bad);
        assert!(
            bad.is_empty(),
            "texto de UI con caracteres que la fuente no dibuja (salen cajas):\n{}",
            bad.join("\n")
        );
    }
}
