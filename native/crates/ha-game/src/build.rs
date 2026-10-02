//! 2D construction system — build mode, ghost preview, placement.
//!
//! **Modelo estilo Last Day on Earth (dos capas):**
//! - *Pisos* (y escaleras) ocupan la CELDA: se colocan libres en cualquier parte
//!   del mapa, la grilla es global (1 tile = 1 m).
//! - *Paredes / puertas / ventanas* ocupan el BORDE entre celdas: el dedo/cursor
//!   se engancha (`build_grid::nearest_edge`) a la arista más cercana de la
//!   celda apuntada, y la pieza sólo entra si hay piso construido de al menos un
//!   lado (`wall_supported`). Apuntar a un borde YA ocupado convierte el gesto
//!   en DEMOLER (el ghost se pone naranja) y devuelve el material.
//!
//! Las reglas puras viven en `ha_shared::build_grid`; el estado y los sprites de
//! las paredes en `crate::walls`.
//!
//! Pieces live in the bag's **CONSTRUIR** tab (`hud.rs`) — the screen stays
//! clean. Picking a piece there enters build mode and closes the bag. While
//! active, the aimed cell (`CursorHighlight`) becomes a placement slot: hold USE
//! / right-click to place the piece (spending its resource cost). A translucent
//! ghost shows validity (green = placeable, red = not enough materials). A small
//! on-screen chip is the ONLY persistent build UI — tap it (or pick a hotbar
//! item) to leave build mode. Removing a piece is just digging it (ha-shared
//! `Tile::dug_item` refunds the material). Desktop: `B` toggles build mode.

use bevy::prelude::*;

use ha_shared::build_grid::{
    ceiling_piece, ceiling_supported, edge_piece, edge_piece_count,
    is_build_floor, nearest_edge, prefab_edges, wall_supported, CeilingPiece, EdgePiece, WallAxis,
    WallKind,
};
use ha_shared::topdown::{
    build_piece, build_piece_count, large_asset, large_asset_count, prefab, prefab_cost,
    prefab_count, BuildPiece, LargeAsset, Prefab, Tile, TILE_M,
};

use crate::game_ui::GameUiRoot;
use crate::hud::{BagBuildPieceButton, BagBuildSection, BagPanelState, BagTab};
use crate::input::{BuildModeActive, HoldInteractState};
use crate::inventory_persist::{InventoryPersistState, PlayerInventory};
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::targeting::LookTarget;
use crate::ui_theme::{self, UiButtonState, UiButtonTone};
use crate::world2d::{
    active_surface_body, render_xy, surface_region_active, ActivePlanet, TileWorld,
};

/// Currently selected build entry — combined palette: indices below
/// `build_piece_count()` are single pieces (`BUILD_PIECES`), the rest are
/// multi-tile prefabs (`PREFABS`). Body-agnostic: places wherever building is
/// allowed (Earth, Moon, Mars, stations).
#[derive(Resource, Default)]
pub struct SelectedBuildPiece(pub usize);

/// What the combined palette index resolves to.
pub enum BuildSelection {
    Piece(&'static BuildPiece),
    Prefab(&'static Prefab),
    /// Asset grande (A2): sprite único + footprint, colocado como estructura.
    Large(&'static LargeAsset),
    /// Pieza de BORDE (pared/puerta/ventana): se pega a la arista entre celdas.
    Edge(&'static EdgePiece),
    /// TAPA (tumbado o techo modular): ocupa la celda, pero por encima.
    Ceiling(&'static CeilingPiece),
}

/// Índice de paleta donde arrancan las piezas de borde.
pub fn edge_palette_base() -> usize {
    build_piece_count() + prefab_count() + large_asset_count()
}

/// Índice de paleta donde arrancan las tapas (techos y tumbados).
pub fn ceiling_palette_base() -> usize {
    edge_palette_base() + edge_piece_count()
}

/// Combined palette lookup: cell pieces, prefabs, large assets (A2), edges, tapas.
pub fn build_selection(idx: usize) -> Option<BuildSelection> {
    let pc = build_piece_count();
    let prc = prefab_count();
    let lc = large_asset_count();
    let ec = edge_piece_count();
    if idx < pc {
        return build_piece(idx).map(BuildSelection::Piece);
    }
    if idx < pc + prc {
        return prefab(idx - pc).map(BuildSelection::Prefab);
    }
    if idx < pc + prc + lc {
        return large_asset(idx - pc - prc).map(BuildSelection::Large);
    }
    if idx < pc + prc + lc + ec {
        return edge_piece(idx - pc - prc - lc).map(BuildSelection::Edge);
    }
    ceiling_piece(idx - pc - prc - lc - ec).map(BuildSelection::Ceiling)
}

/// **¿Esta celda del nivel activo admite fundar algo?** En el suelo (`z <= 0`)
/// siempre: hay terreno. En altura sólo sobre lo construido —tu casa, o la casa
/// generada del pueblo que estás ampliando— o enganchándose a una losa vecina del
/// mismo piso (`build_grid::floor_supported`, regla de Project Zomboid).
///
/// Lo que aguanta desde abajo son las TRES capas que existen: el tile (piso o
/// pared de asentamiento), la pared de BORDE que rodea la celda, y la estructura
/// colocada. Si faltara alguna, techar tu propia casa fallaría según con qué la
/// hayas levantado, que es la peor clase de regla: la que depende de por dónde
/// entraste.
fn floor_supported_here(
    tiles: &TileWorld,
    walls: &crate::walls::BuiltWalls,
    structures: &crate::structures::BuiltStructures,
    body: ha_shared::CelestialBody,
    z: i32,
    tx: i64,
    tz: i64,
) -> bool {
    ha_shared::build_grid::floor_supported(
        z,
        &|cx, cz| {
            tiles.supports_upper_floor(body, z - 1, cx, cz)
                || walls.any_edge_of_cell(body, z - 1, cx, cz)
                || structures.blocks(body, z - 1, cx, cz)
        },
        &|cx, cz| ha_shared::build_grid::is_floor_piece(tiles.tile_at(body, cx, cz)),
        tx,
        tz,
    )
}

/// Footprint min corner for a prefab aimed at cell `(tx, tz)` (centered).
fn prefab_min(p: &Prefab, tx: i64, tz: i64) -> (i64, i64) {
    (tx - p.w / 2, tz - p.h / 2)
}

/// **Hacia dónde mira lo que estás por colocar.**
///
/// `None` = AUTOMÁTICO: el objeto se planta de espaldas a la pared que tenga al
/// lado. Es lo que uno espera al amoblar —la cabecera contra el muro, la
/// heladera pegada al rincón— y evita tener que girar a mano cada mueble.
/// `Some(r)` = lo fijaste vos con GIRAR y manda sobre el automático, para el caso
/// en que quieras la cama en medio del cuarto.
#[derive(Resource, Default)]
pub struct BuildFacing(pub Option<u8>);

impl BuildFacing {
    /// El botón cicla AUTO → sur → oeste → norte → este → AUTO.
    fn cycle(&mut self) {
        self.0 = match self.0 {
            None => Some(0),
            Some(3) => None,
            Some(r) => Some(r + 1),
        };
    }
    fn label(&self) -> &'static str {
        match self.0 {
            None => "AUTO",
            Some(0) => "SUR",
            Some(1) => "OESTE",
            Some(2) => "NORTE",
            _ => "ESTE",
        }
    }
}

// ── La mira de obra: apuntar, mirar, confirmar ──────────────────────────────

/// Dónde quedó CLAVADO el fantasma: una celda o una arista, según qué pieza sea.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sitio {
    Celda(i64, i64),
    Borde(i64, i64, WallAxis),
}

/// **Apuntar y colocar dejaron de ser el mismo gesto.**
///
/// Hasta acá, tocar el mundo en modo construir COLOCABA en el acto y el fantasma
/// era un rectángulo de color que seguía al dedo. En un teléfono eso significa
/// que **nunca ves dónde va a caer la pieza**: el dedo tapa justo la celda que
/// estás apuntando, y para cuando lo levantás la pared ya está puesta. El
/// reporte fue directo: *"así puedo ver dónde se va a colocar la pared/objeto"*.
///
/// Ahora son dos gestos:
///
/// 1. **Tocás** → el fantasma se CLAVA ahí (`fijada`) y se dibuja con el sprite
///    de verdad de la pieza, traslúcido. Ya no sigue al dedo, así que lo podés
///    mirar con la mano fuera de la pantalla.
/// 2. **COLOCAR / CANCELAR / GIRAR** → confirmás, descartás o rotás.
///
/// Y después de colocar, el fantasma **se corre solo** a la celda siguiente
/// ([`Self::avance`]): levantar un muro de seis metros es tocar una vez y aceptar
/// seis, que era el otro pedido (*"que el fantasma se vaya hacia el lado así si
/// quiero seguir construyendo solo sigo aceptando"*). Tocar en otro sitio lo
/// vuelve a clavar donde digas.
#[derive(Resource, Default)]
pub struct MiraDeObra {
    /// Dónde está clavado el fantasma. `None` = todavía no apuntaste.
    pub fijada: Option<Sitio>,

    /// Hacia dónde se corre después de aceptar, en celdas. Se aprende del segundo
    /// toque (la dirección en la que te moviste) y, la primera vez, del rumbo del
    /// jugador: la obra se aleja de vos, que es como uno construye.
    avance: (i64, i64),
    /// Lo pidió el botón COLOCAR y todavía no se resolvió.
    confirmar: bool,
}

impl MiraDeObra {
    /// Clava el fantasma en un sitio nuevo, aprendiendo hacia dónde vas.
    fn fijar(&mut self, sitio: Sitio, rumbo: (i64, i64)) {
        if let (Some(previo), Sitio::Celda(nx, nz)) = (self.fijada, sitio) {
            if let Sitio::Celda(px, pz) = previo {
                let (dx, dz) = (nx - px, nz - pz);
                if (dx, dz) != (0, 0) {
                    // La componente dominante: dos toques en diagonal no dejan un
                    // avance diagonal, que no encaja en una cuadrícula.
                    self.avance = if dx.abs() >= dz.abs() {
                        (dx.signum(), 0)
                    } else {
                        (0, dz.signum())
                    };
                }
            }
        }
        if self.avance == (0, 0) {
            self.avance = rumbo;
        }
        // **Una pared avanza A LO LARGO de su propia línea.** Un muro se levanta
        // en fila; dejar que el avance fuera libre haría que aceptar dos veces
        // pusiera la segunda pieza en una arista perpendicular, que no es nunca lo
        // que se quiere.
        if let Sitio::Borde(_, _, axis) = sitio {
            self.avance = match axis {
                WallAxis::H => (self.avance.0.signum().max(-1), 0),
                WallAxis::V => (0, self.avance.1.signum().max(-1)),
            };
            if self.avance == (0, 0) {
                self.avance = match axis {
                    WallAxis::H => (1, 0),
                    WallAxis::V => (0, 1),
                };
            }
        }
        self.fijada = Some(sitio);
    }

    /// Corre el fantasma una celda en el sentido de la obra.
    fn avanzar(&mut self) {
        let (dx, dz) = self.avance;
        self.fijada = match self.fijada {
            Some(Sitio::Celda(x, z)) => Some(Sitio::Celda(x + dx, z + dz)),
            Some(Sitio::Borde(x, z, axis)) => Some(Sitio::Borde(x + dx, z + dz, axis)),
            None => None,
        };
    }

    /// **El botón COLOCAR, para los drivers de QA.** Es el único camino que
    /// tienen a `confirmar`, que es privado a propósito: la confirmación la
    /// escribe una chapa que el dedo toca, y dejar el campo abierto invitaría a
    /// que cualquier sistema del juego colocara por su cuenta.
    pub fn confirmar_desde_driver(&mut self) {
        self.confirmar = true;
    }

    pub fn celda(&self) -> Option<(i64, i64)> {
        match self.fijada {
            Some(Sitio::Celda(x, z)) => Some((x, z)),
            _ => None,
        }
    }

    pub fn borde(&self) -> Option<(i64, i64, WallAxis)> {
        match self.fijada {
            Some(Sitio::Borde(x, z, a)) => Some((x, z, a)),
            _ => None,
        }
    }
}

/// **Anclaje a la pared**: de espaldas al muro que tenga la celda al lado. Se
/// mira en orden norte, este, sur, oeste y se toma el primero; sin paredes
/// alrededor queda mirando al sur, que es el reposo de todos los sprites.
fn auto_facing(walls: &crate::walls::BuiltWalls, body: ha_shared::CelestialBody, z: i32, tx: i64, tz: i64) -> u8 {
    use ha_shared::build_grid::WallAxis::{H, V};
    if walls.at(body, z, tx, tz, H).is_some() {
        return 0; // muro al norte → mira al sur
    }
    if walls.at(body, z, tx + 1, tz, V).is_some() {
        return 1; // muro al este → mira al oeste
    }
    if walls.at(body, z, tx, tz + 1, H).is_some() {
        return 2; // muro al sur → mira al norte
    }
    if walls.at(body, z, tx, tz, V).is_some() {
        return 3; // muro al oeste → mira al este
    }
    0
}

/// **¿Esta celda está ADENTRO?** Un mueble de interior (`PlaceCat::Mueble`) no va
/// en mitad de la calle: pide estar bajo techo —el que pusiste o la losa de la
/// planta de arriba— o en un rincón con al menos dos paredes.
///
/// Las dos condiciones existen porque las dos son "adentro" para el jugador: un
/// cuarto terminado tiene tapa, y uno a medio hacer todavía no, pero ya tiene
/// esquinas. Exigir sólo techo obligaría a techar antes de poder amoblar.
fn cell_is_indoors(
    ceilings: &crate::ceilings::BuiltCeilings,
    roofs: &crate::building::BuiltRoofs,
    walls: &crate::walls::BuiltWalls,
    tiles: &TileWorld,
    body: ha_shared::CelestialBody,
    z: i32,
    tx: i64,
    tz: i64,
) -> bool {
    if crate::ceilings::covered(ceilings, roofs, tiles, body, z, tx, tz) {
        return true;
    }
    use ha_shared::build_grid::WallAxis::{H, V};
    let sides = [
        walls.at(body, z, tx, tz, H).is_some(),
        walls.at(body, z, tx, tz + 1, H).is_some(),
        walls.at(body, z, tx, tz, V).is_some(),
        walls.at(body, z, tx + 1, tz, V).is_some(),
    ];
    sides.iter().filter(|s| **s).count() >= 2
}

#[derive(Resource, Default)]
struct BuildIndicatorSpawned(bool);

#[derive(Component)]
struct BuildIndicator;
#[derive(Component)]
struct BuildIndicatorLabel;
#[derive(Component)]
struct BuildGhost;
/// Chapa "en qué piso estoy". Ver `sync_floor_indicator`.
#[derive(Component)]
struct FloorIndicator;
/// Chapa GIRAR: toca y el objeto que estás por colocar cambia de orientación.
#[derive(Component)]
struct FacingChip;
#[derive(Component)]
struct FacingChipLabel;
/// Chapa COLOCAR: acepta lo que el fantasma está mostrando.
#[derive(Component)]
struct ChapaColocar;
/// Chapa CANCELAR: descarta el fantasma sin colocar nada.
#[derive(Component)]
struct ChapaCancelar;

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(bevy::prelude::OnExit(crate::game_phase::GamePhase::Playing), (crate::salir::reiniciar::<BuildIndicatorSpawned>,));
        app.init_resource::<SelectedBuildPiece>()
            .init_resource::<BuildFacing>()
            .init_resource::<MiraDeObra>()
            .init_resource::<BuildIndicatorSpawned>()
            .add_systems(Startup, spawn_build_ghost)
            .add_systems(
                Update,
                (
                    ensure_build_indicator,
                    desktop_build_hotkey,
                    desktop_rotate_hotkey,
                    build_tab_select,
                    sync_bag_build_section,
                    sync_build_indicator,
                    sync_floor_indicator,
                    sync_facing_chip,
                    // Tocar APUNTA, el botón COLOCA. El orden es el gesto: primero
                    // se clava la mira con el toque de este frame, después se lee
                    // el botón, y recién ahí se coloca.
                    fijar_mira_de_obra.before(apply_build_hold),
                    chapas_de_obra.before(apply_build_hold),
                    apply_build_hold.before(crate::hud::update_interact_hud),
                    sync_build_ghost.after(apply_build_hold),
                ),
            );
    }
}

/// `B` toggles build mode on desktop (mobile enters via the bag's CONSTRUIR tab).
fn desktop_build_hotkey(keys: Res<ButtonInput<KeyCode>>, mut build_mode: ResMut<BuildModeActive>) {
    if crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyB) {
        build_mode.active = !build_mode.active;
    }
}

/// `R` gira lo que estás por colocar (escritorio). En el celular lo hace la
/// chapa GIRAR, que es táctil y vive arriba-izquierda, lejos de los joysticks.
fn desktop_rotate_hotkey(keys: Res<ButtonInput<KeyCode>>, mut facing: ResMut<BuildFacing>) {
    if crate::platform::uses_touch_gameplay_ui() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyR) {
        facing.cycle();
    }
}

/// Picking a piece in the bag's CONSTRUIR tab selects it, enters build mode and
/// closes the bag so you can place it right away.
///
/// **Al SOLTAR, y sólo si el dedo no arrastró** (29 sep 2026). Elegía al apoyar:
/// cualquier intento de scrollear la lista empezaba sobre una pieza, la elegía y
/// cerraba la mochila ("toco para hacer scroll y se cierra la ventana"). Es el
/// mismo criterio que ya usaban la mochila y las recetas.
fn build_tab_select(
    mut buttons: Query<(&Interaction, &BagBuildPieceButton, &mut BackgroundColor), Changed<Interaction>>,
    touches: Res<Touches>,
    mut selected: ResMut<SelectedBuildPiece>,
    mut build_mode: ResMut<BuildModeActive>,
    mut bag: ResMut<BagPanelState>,
    mut pendiente: Local<Option<(usize, f32)>>,
) {
    for (interaction, button, mut bg) in &mut buttons {
        if *interaction == Interaction::Pressed {
            *pendiente = Some((button.0, 0.0));
        }
        *bg = BackgroundColor(ui_theme::button_bg(
            UiButtonTone::Craft,
            ui_theme::interaction_state(*interaction, false),
        ));
    }
    let Some((pieza, recorrido)) = pendiente.as_mut() else { return };
    if !bag.open {
        *pendiente = None;
        return;
    }
    let mut dedo = false;
    for t in touches.iter() {
        dedo = true;
        *recorrido += t.delta().length();
    }
    if dedo {
        return;
    }
    // Soltó: toque si no arrastró más que el umbral.
    if *recorrido <= crate::hud::BAG_UMBRAL {
        selected.0 = *pieza;
        build_mode.active = true;
        bag.open = false;
    }
    *pendiente = None;
}

/// Show/hide the CONSTRUIR section with its tab (display toggle owned here).
fn sync_bag_build_section(
    bag: Res<BagPanelState>,
    mut sections: Query<&mut Node, With<BagBuildSection>>,
) {
    // Sólo al cambiar de pestaña: escribir `display` cada frame marca el `Node`
    // sucio y taffy relayoutea toda la UI, aunque el valor sea el mismo.
    let want = if bag.tab == BagTab::Build {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut sections {
        if node.display != want {
            node.display = want;
        }
    }
}

/// The single persistent build-mode chip — upper-left (clear of the joystick
/// zones), hidden unless building. Tapping it leaves build mode.
fn ensure_build_indicator(
    mut commands: Commands,
    layout: Res<crate::input::TouchLayout>,
    root: Query<Entity, With<GameUiRoot>>,
    mut spawned: ResMut<BuildIndicatorSpawned>,
) {
    if spawned.0 || layout.width < 100.0 {
        return;
    }
    let Ok(root) = root.get_single() else {
        return;
    };
    spawned.0 = true;
    // Chapa de PISO: nace oculta y sólo aparece fuera de la planta baja. Sin
    // esto la verticalidad es adivinanza — el mundo de abajo se ve atenuado por
    // el hueco, pero "estoy en el 2º" y "estoy en una cueva" no se distinguen, y
    // colocar en el piso equivocado es el error más fácil de cometer.
    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(8.0),
                    top: Val::Px(128.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor(ui_theme::accent_cyan()),
                BackgroundColor(ui_theme::button_bg(UiButtonTone::Primary, UiButtonState::Pressed)),
                Visibility::Hidden,
                FloorIndicator,
                Name::new("FloorIndicator"),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new(""),
                    TextFont { font_size: 13.0, ..default() },
                    TextColor(ui_theme::text_primary()),
                ));
            });
    });
    // Chapa GIRAR: el celular no tiene tecla `R`, y sin ella amoblar de espaldas
    // a la pared sería imposible de corregir a mano. Arriba-izquierda como el
    // resto (biblia §7: ahí no la roban los joysticks).
    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(8.0),
                    top: Val::Px(156.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor(ui_theme::accent_cyan()),
                BackgroundColor(ui_theme::button_bg(UiButtonTone::Craft, UiButtonState::Pressed)),
                Visibility::Hidden,
                FacingChip,
                Name::new("FacingChip"),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("GIRAR: AUTO"),
                    TextFont { font_size: 13.0, ..default() },
                    TextColor(ui_theme::text_primary()),
                    FacingChipLabel,
                ));
            });
    });
    // **COLOCAR y CANCELAR: el visto bueno y la equis.**
    //
    // Van debajo de GIRAR y en la misma columna a propósito: las tres son las
    // decisiones de la pieza que estás por poner, así que se leen juntas. Sólo
    // aparecen con un fantasma clavado (`chapas_de_obra`) — sin nada apuntado
    // serían dos botones que no hacen nada.
    //
    // Con palabras y no con símbolos: la fuente que embarca el juego
    // (`fuente.rs`) no trae ✓ ni ✗, y lo que no trae sale caja. Lo caza el test
    // `la_fuente_dibuja_todo_el_texto_de_la_ui` (biblia §2, regla 6).
    for (etiqueta, arriba, tono, marca) in [
        ("COLOCAR", 184.0, UiButtonTone::Primary, 0u8),
        ("CANCELAR", 212.0, UiButtonTone::Attack, 1u8),
    ] {
        commands.entity(root).with_children(|parent| {
            let mut chapa = parent.spawn((
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(8.0),
                    top: Val::Px(arriba),
                    padding: UiRect::axes(Val::Px(10.0), Val::Px(5.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor(ui_theme::accent_cyan()),
                BackgroundColor(ui_theme::button_bg(tono, UiButtonState::Pressed)),
                Visibility::Hidden,
                Name::new(etiqueta),
            ));
            chapa.with_children(|b| {
                b.spawn((
                    Text::new(etiqueta),
                    TextFont { font_size: 14.0, ..default() },
                    TextColor(ui_theme::text_primary()),
                ));
            });
            if marca == 0 {
                chapa.insert(ChapaColocar);
            } else {
                chapa.insert(ChapaCancelar);
            }
        });
    }
    commands.entity(root).with_children(|parent| {
        parent
            .spawn((
                Button,
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(8.0),
                    top: Val::Px(100.0),
                    padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BorderColor(ui_theme::accent_cyan()),
                BackgroundColor(ui_theme::button_bg(UiButtonTone::Primary, UiButtonState::Pressed)),
                Visibility::Hidden,
                BuildIndicator,
                Name::new("BuildIndicator"),
            ))
            .with_children(|b| {
                b.spawn((
                    Text::new("CONSTRUIR (tocar: salir)"),
                    TextFont {
                        font_size: 13.0,
                        ..default()
                    },
                    TextColor(ui_theme::text_primary()),
                    BuildIndicatorLabel,
                ));
            });
    });
}

fn sync_build_indicator(
    mut build_mode: ResMut<BuildModeActive>,
    selected: Res<SelectedBuildPiece>,
    taps: Query<&Interaction, (With<BuildIndicator>, Changed<Interaction>)>,
    mut chips: Query<&mut Visibility, With<BuildIndicator>>,
    mut label: Query<&mut Text, With<BuildIndicatorLabel>>,
) {
    // Tap the chip → leave build mode.
    for interaction in &taps {
        if *interaction == Interaction::Pressed {
            build_mode.active = false;
        }
    }
    let want = if build_mode.active {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut chips {
        if *v != want {
            *v = want;
        }
    }
    if build_mode.active {
        if let Ok(mut text) = label.get_single_mut() {
            let is_edificio = selected.0 >= crate::edificios::PALETTE_BASE;
            let is_spawn = !is_edificio && selected.0 >= crate::fauna::SPAWN_PALETTE_BASE;
            let name = if is_edificio {
                crate::edificios::accion_label(selected.0 - crate::edificios::PALETTE_BASE)
            } else if is_spawn {
                crate::fauna::spawn_species_label(selected.0 - crate::fauna::SPAWN_PALETTE_BASE)
            } else {
                match build_selection(selected.0) {
                    Some(BuildSelection::Piece(p)) => p.label,
                    Some(BuildSelection::Prefab(p)) => p.label,
                    Some(BuildSelection::Large(a)) => a.label,
                    Some(BuildSelection::Edge(p)) => p.label,
                    Some(BuildSelection::Ceiling(p)) => p.label,
                    None => "",
                }
            };
            let verb = if is_spawn { "SPAWN" } else { "CONSTRUIR" };
            let line = format!("{verb}: {name}  (tocar: salir)");
            if text.0 != line {
                text.0 = line;
            }
        }
    }
}

/// **En qué piso estás**, en una línea y sólo cuando no es obvio. Planta baja =
/// sin chapa: el 99 % del juego pasa ahí y una etiqueta permanente sería ruido.
///
/// La verticalidad ya no es sólo la mina: `z > 0` son los pisos que construís
/// (Project Zomboid) y `z < 0` el subsuelo. Que la chapa sea la MISMA para los
/// dos es a propósito — es un solo eje, no dos sistemas.
fn sync_floor_indicator(
    tiles: Res<TileWorld>,
    mut chips: Query<(&mut Visibility, &Children), With<FloorIndicator>>,
    mut labels: Query<&mut Text>,
) {
    let z = tiles.z;
    for (mut vis, children) in &mut chips {
        let want = if z == 0 { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
        if z == 0 {
            continue;
        }
        // ASCII-only: la fuente del juego no dibuja acentos ni flechas.
        let line = if z > 0 {
            format!("PISO {}", z + 1)
        } else {
            format!("SUBSUELO {}", -z)
        };
        for child in children.iter() {
            if let Ok(mut text) = labels.get_mut(*child) {
                if text.0 != line {
                    text.0 = line.clone();
                }
            }
        }
    }
}

/// La chapa GIRAR: sólo aparece con algo ORIENTABLE en la mano (un colocable),
/// porque girar un piso o una pared no significa nada y una chapa que no hace
/// nada es peor que ninguna.
fn sync_facing_chip(
    build_mode: Res<BuildModeActive>,
    selected: Res<SelectedBuildPiece>,
    mut facing: ResMut<BuildFacing>,
    taps: Query<&Interaction, (With<FacingChip>, Changed<Interaction>)>,
    mut chips: Query<&mut Visibility, With<FacingChip>>,
    mut label: Query<&mut Text, With<FacingChipLabel>>,
) {
    for interaction in &taps {
        if *interaction == Interaction::Pressed {
            facing.cycle();
        }
    }
    let orientable = build_mode.active
        && matches!(build_selection(selected.0), Some(BuildSelection::Large(_)));
    let want = if orientable { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut chips {
        if *v != want {
            *v = want;
        }
    }
    if orientable {
        if let Ok(mut text) = label.get_single_mut() {
            let line = format!("GIRAR: {}", facing.label());
            if text.0 != line {
                text.0 = line;
            }
        }
    }
}

/// Escritores de eventos de colocación creativa (fauna/comida/cámara) agrupados en un
/// `SystemParam` para no pasar del límite de 16 parámetros de sistema de Bevy.
#[derive(bevy::ecs::system::SystemParam)]
struct CreativeSpawnWriters<'w> {
    resident: EventWriter<'w, crate::fauna::SpawnResidentEvent>,
    food: EventWriter<'w, crate::fauna::SpawnFoodEvent>,
    camera: EventWriter<'w, crate::camera_feed::PlaceCameraEvent>,
}

/// Todo lo que la colocación ESCRIBE, junto (mismo motivo: límite de params).
#[derive(bevy::ecs::system::SystemParam)]
struct BuildTargets<'w, 's> {
    tiles: ResMut<'w, TileWorld>,
    /// Los tramos de escalera: colocar uno estampa celdas en DOS niveles y
    /// registra el tramo, así que va con el resto de las capas del mundo.
    escaleras: ResMut<'w, crate::escaleras::Escaleras>,
    /// Dónde estás parado — con el chip de orientación en AUTO, la escalera sube
    /// alejándose de vos.
    player: Query<'w, 's, &'static crate::sim::SimState, With<crate::sim::Player>>,
    roofs: ResMut<'w, crate::building::BuiltRoofs>,
    structures: ResMut<'w, crate::structures::BuiltStructures>,
    walls: ResMut<'w, crate::walls::BuiltWalls>,
    ceilings: ResMut<'w, crate::ceilings::BuiltCeilings>,
    /// La cáscara de los edificios modulares: se coloca ENTERA, no borde por
    /// borde (una banda del pack son 9 celdas de un solo dibujo).
    edificios: ResMut<'w, crate::edificios::Edificios>,
}

/// **Snapping de pared**: borde de la cuadrícula al que se engancha la mira.
/// Sólo dentro del alcance de construcción — lo garantiza el propio targeting,
/// que sólo publica `place_cell`/`block_cell` dentro de `TILE_REACH`.
fn aimed_edge(
    cursor: &crate::input::AimCursor,
    target: &LookTarget,
) -> Option<(i64, i64, WallAxis)> {
    if target.place_cell.is_none() && target.block_cell.is_none() {
        return None;
    }
    Some(nearest_edge(cursor.world.x, cursor.world.y))
}

/// **Tocar el mundo ya no coloca: APUNTA.**
///
/// Consume el mismo gesto de siempre (el hold que arma `look_interact`) y lo
/// único que hace es clavar el fantasma. La colocación de verdad la dispara
/// COLOCAR (`apply_build_hold`), que corre justo después y por eso este sistema
/// va `.before`.
///
/// Que sea el MISMO gesto importa: no hay un modo nuevo que aprender, sólo que el
/// dedo dejó de ser irreversible.
fn fijar_mira_de_obra(
    mut hold: ResMut<HoldInteractState>,
    build_mode: Res<crate::input::BuildModeActive>,
    selected: Res<SelectedBuildPiece>,
    target: Res<LookTarget>,
    cursor: Res<crate::input::AimCursor>,
    player: Query<&crate::sim::SimState, With<crate::sim::Player>>,
    mut mira: ResMut<MiraDeObra>,
) {
    // Salir de construir suelta la mira: un fantasma clavado en un modo que ya
    // no estás usando es una promesa colgada.
    if !build_mode.active {
        mira.fijada = None;
        return;
    }
    if !hold.completed || !hold.placing_build {
        return;
    }
    hold.completed = false;
    hold.placing_build = false;
    // El rumbo del jugador, en cardinal: es el avance por defecto la primera vez.
    let rumbo = player
        .get_single()
        .ok()
        .map(|s| {
            let (mx, mz) = (s.body.rot.cos(), s.body.rot.sin());
            if mx.abs() >= mz.abs() {
                (mx.signum() as i64, 0)
            } else {
                (0, mz.signum() as i64)
            }
        })
        .unwrap_or((1, 0));
    let sitio = match build_selection(selected.0) {
        Some(BuildSelection::Edge(_)) => {
            aimed_edge(&cursor, &target).map(|(x, z, a)| Sitio::Borde(x, z, a))
        }
        _ => target.place_cell.map(|(x, z, _)| Sitio::Celda(x as i64, z as i64)),
    };
    if let Some(sitio) = sitio {
        mira.fijar(sitio, rumbo);
    }
}

/// **COLOCAR / CANCELAR**, y las teclas equivalentes en escritorio.
///
/// Son chapas y no un gesto en el mundo a propósito: el mundo ya está ocupado por
/// el fantasma, y lo que se necesita es un sitio donde el dedo pueda decidir sin
/// tapar lo que está mirando.
#[allow(clippy::type_complexity)]
fn chapas_de_obra(
    keys: Res<ButtonInput<KeyCode>>,
    build_mode: Res<crate::input::BuildModeActive>,
    mut mira: ResMut<MiraDeObra>,
    mut facing: ResMut<BuildFacing>,
    aceptar: Query<&Interaction, (With<ChapaColocar>, Changed<Interaction>)>,
    cancelar: Query<&Interaction, (With<ChapaCancelar>, Changed<Interaction>)>,
    mut visibles: Query<
        &mut Visibility,
        Or<(With<ChapaColocar>, With<ChapaCancelar>)>,
    >,
) {
    let hay_fantasma = build_mode.active && mira.fijada.is_some();
    if aceptar.iter().any(|i| *i == Interaction::Pressed)
        || (hay_fantasma && !crate::platform::uses_touch_gameplay_ui() && keys.just_pressed(KeyCode::Enter))
    {
        mira.confirmar = true;
    }
    if cancelar.iter().any(|i| *i == Interaction::Pressed)
        || (hay_fantasma && keys.just_pressed(KeyCode::Escape))
    {
        mira.fijada = None;
        mira.confirmar = false;
        // Cancelar devuelve también la orientación a AUTO: si giraste un mueble
        // para ESTA colocación y la descartaste, arrastrar el giro a la siguiente
        // es la clase de estado invisible que hace que el juego "haga cosas raras".
        facing.0 = None;
    }
    let want = if hay_fantasma { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut visibles {
        if *v != want {
            *v = want;
        }
    }
}

/// **Hacia dónde sube el tramo que estás colocando.**
///
/// Con el chip de orientación en AUTO, se aleja de vos: una escalera se planta
/// mirando hacia donde va a subir, y eso es lo que hace la mano sola. Si fijaste
/// una orientación, manda ésa — mismo chip que ya orienta puertas y paredes, así
/// que no hay un modo nuevo que aprender.
fn stair_dir(
    facing: &BuildFacing,
    player: &Query<&'static crate::sim::SimState, With<crate::sim::Player>>,
    tx: i64,
    tz: i64,
) -> u8 {
    // El chip cicla SUR/OESTE/NORTE/ESTE; `escaleras::eje` numera N/E/S/O.
    if let Some(r) = facing.0 {
        return match r % 4 {
            0 => 2, // sur
            1 => 3, // oeste
            2 => 0, // norte
            _ => 1, // este
        };
    }
    let Ok(sim) = player.get_single() else { return 2 };
    let dx = (tx as f32 + 0.5) - sim.body.x;
    let dz = (tz as f32 + 0.5) - sim.body.z;
    if dx.abs() >= dz.abs() {
        if dx >= 0.0 { 1 } else { 3 }
    } else if dz >= 0.0 {
        2
    } else {
        0
    }
}

/// **Coloca lo que el fantasma está mostrando.** Sólo cuando lo pediste con
/// COLOCAR, y siempre en el sitio CLAVADO ([`MiraDeObra`]) — nunca en el que el
/// dedo esté tocando ahora.
#[allow(clippy::too_many_arguments)]
fn apply_build_hold(
    mut mira: ResMut<MiraDeObra>,
    selected: Res<SelectedBuildPiece>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    editor: Res<crate::ui_editor::UiEditorState>,
    flora: Res<crate::harvest::FloraHarvest>,
    art: Res<crate::art2d::ArtLibrary>,
    mut world: BuildTargets,
    facing: Res<BuildFacing>,
    mut inv: ResMut<PlayerInventory>,
    mut persist: ResMut<InventoryPersistState>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
    mut spawners: CreativeSpawnWriters,
) {
    if !mira.confirmar {
        return;
    }
    mira.confirmar = false;
    let Some(sitio) = mira.fijada else {
        return;
    };
    // **El fantasma se corre ANTES de colocar**, y no después, por una razón
    // aburrida y buena: la colocación tiene una docena de salidas tempranas (sin
    // material, celda ocupada, apoyo que falta) y encadenar el avance a cada una
    // sería garantizar que alguna se olvide. El sitio ya está leído en `sitio`,
    // así que adelantar la mira no cambia dónde cae la pieza.
    //
    // Y avanza también cuando la pieza NO entra: es lo que se quiere al levantar
    // un muro contra una esquina — seguís aceptando y el fantasma sigue de largo
    // por donde no se puede, poniéndose rojo, hasta que vuelve a poderse.
    mira.avanzar();
    let punto = match sitio {
        Sitio::Celda(x, z) => Vec2::new(x as f32 + 0.5, z as f32 + 0.5),
        Sitio::Borde(x, z, axis) => {
            let (cx, cz) = ha_shared::build_grid::edge_center(x, z, axis);
            Vec2::new(cx, cz)
        }
    };
    // Las piezas de BORDE no usan la celda: se enganchan a la arista clavada.
    if let Some(BuildSelection::Edge(piece)) = build_selection(selected.0) {
        let Sitio::Borde(ex, ez, axis) = sitio else {
            return; // pieza de borde con la mira en una celda: no hay dónde
        };
        apply_edge_build(
            piece,
            selected.0 - edge_palette_base(),
            (ex, ez, axis),
            punto,
            &ctx,
            &planet,
            editor.creative,
            &mut world,
            &mut inv,
            &mut persist,
            &mut sfx,
        );
        return;
    }
    let Sitio::Celda(tx, tz) = sitio else {
        return;
    };
    // TAPAS: ocupan la celda apuntada, pero por encima. Mismas dos reglas que el
    // borde —tocar una ocupada DEMUELE y devuelve el material, si no se coloca
    // exigiendo apoyo—, así techar se aprende sabiendo levantar una pared.
    if let Some(BuildSelection::Ceiling(piece)) = build_selection(selected.0) {
        apply_ceiling_build(
            piece,
            selected.0 - ceiling_palette_base(),
            tx as i64,
            tz as i64,
            &ctx,
            &planet,
            editor.creative,
            &mut world,
            &mut inv,
            &mut persist,
            &mut sfx,
            punto,
        );
        return;
    }
    // EDIFICIO MODULAR: fundar un ambiente, ampliarlo, apilarle una planta o
    // abrirle una puerta. No son piezas de la grilla —una banda del pack son 9
    // celdas de un solo dibujo—, así que se atienden antes de `build_selection`.
    //
    // Va ANTES que la fauna a propósito: `PALETTE_BASE` (20.000) también es
    // `>= SPAWN_PALETTE_BASE` (10.000), y si se preguntara al revés cada toque de
    // esta ficha spawnearía un bicho.
    if selected.0 >= crate::edificios::PALETTE_BASE {
        let Some(accion) = crate::edificios::accion(selected.0 - crate::edificios::PALETTE_BASE)
        else {
            return;
        };
        let body = active_surface_body(&ctx, &planet) as u8;
        let z = world.tiles.z;
        if crate::edificios::aplicar(
            &mut world.edificios,
            body,
            z,
            tx as i64,
            tz as i64,
            punto.x,
            punto.y,
            accion,
        )
        .is_some()
        {
            // **El adentro** (C8.1): fundar o ampliar un ambiente estampa su piso. La cáscara es
            // la elevación de la calle; sin esto, cruzar la puerta te dejaba sobre el pasto.
            if matches!(accion, crate::edificios::Accion::Ambiente | crate::edificios::Accion::Ampliar) && crate::flags::var("HA_SIN_PISO").is_err() {
                crate::edificios::estampar_piso(&mut world.tiles, &world.edificios, body, z, tx as i64, tz as i64);
            }
            sfx.send(crate::audio::PlaySfxAt(
                crate::audio::Sfx::Place,
                render_xy(punto.x, punto.y),
            ));
        }
        return;
    }
    // VIVARIO (F1): índices ≥ SPAWN_PALETTE_BASE son fauna residente (creativo), no
    // piezas — spawnea en el centro de la celda apuntada. Ver PLAN_VIVARIO_*.
    if selected.0 >= crate::fauna::SPAWN_PALETTE_BASE {
        if editor.creative {
            let sub = selected.0 - crate::fauna::SPAWN_PALETTE_BASE;
            let (x, z) = (tx as f32 + 0.5, tz as f32 + 0.5);
            if sub == crate::fauna::FEED_PALETTE_SUB {
                spawners.food.send(crate::fauna::SpawnFoodEvent { x, z }); // F8: feeder
            } else if sub == crate::fauna::CAMERA_PALETTE_SUB {
                // F7: dejar una cámara de vigilancia en este punto del cuerpo actual.
                let cbody = active_surface_body(&ctx, &planet) as u8;
                spawners
                    .camera
                    .send(crate::camera_feed::PlaceCameraEvent { body: cbody, x, z });
            } else if sub == crate::fauna::WATER_PALETTE_SUB {
                // F8: abrevadero — coloca un tile de agua.
                let body = active_surface_body(&ctx, &planet);
                world.tiles.place_tile(body, tx as i64, tz as i64, Tile::Water);
            } else if sub == crate::fauna::PREDATOR_PALETTE_SUB {
                // F8: depredador — residente forzado hostil (acecha).
                spawners.resident.send(crate::fauna::SpawnResidentEvent {
                    x,
                    z,
                    species: 0,
                    hostile: true,
                });
            } else {
                spawners.resident.send(crate::fauna::SpawnResidentEvent {
                    x,
                    z,
                    species: sub,
                    hostile: false,
                });
            }
            sfx.send(crate::audio::PlaySfxAt(
                crate::audio::Sfx::Place,
                render_xy(punto.x, punto.y),
            ));
        }
        return;
    }
    let Some(selection) = build_selection(selected.0) else {
        return;
    };
    // Creative = Minecraft-style: build free, no resource cost.
    let creative = editor.creative;
    let body = active_surface_body(&ctx, &planet);
    let z = world.tiles.z;
    match selection {
        BuildSelection::Piece(piece) if piece.tile == Tile::Step => {
            // **La escalera se coloca ENTERA.** Un escalón suelto no es nada —
            // ni sube, ni tiene hacia dónde subir— y dejar construirlos de a uno
            // obligaría a adivinar por cuál de las dos puntas se sube. Un gesto,
            // un tramo, su rellano, y el coste de los cuatro escalones.
            let costo = piece.cost_qty * ha_shared::escaleras::PASOS as u32;
            if !creative && inv.qty(piece.cost_item) < costo {
                return;
            }
            let dir = stair_dir(&facing, &world.player, tx as i64, tz as i64);
            if crate::escaleras::construir_tramo(
                &mut world.escaleras,
                &mut world.tiles,
                body,
                tx as i64,
                tz as i64,
                dir,
            ) {
                sfx.send(crate::audio::PlaySfxAt(
                    crate::audio::Sfx::Stamp,
                    render_xy(punto.x, punto.y),
                ));
                if !creative {
                    inv.remove(piece.cost_item, costo);
                }
                persist.mark_immediate();
            }
        }
        BuildSelection::Piece(piece) => {
            if !creative && inv.qty(piece.cost_item) < piece.cost_qty {
                return;
            }
            // No se construye sobre decor cosechable (árbol/roca/mineral): talá o
            // picá primero.
            if crate::world2d::cell_build_blocked(&flora, body, z, tx as i64, tz as i64) {
                return;
            }
            // En altura, la losa necesita apoyo (la casa de abajo o una losa vecina).
            if !floor_supported_here(
                &world.tiles,
                &world.walls,
                &world.structures,
                body,
                z,
                tx as i64,
                tz as i64,
            ) {
                return;
            }
            if world.tiles.place_tile(body, tx as i64, tz as i64, piece.tile) {
                sfx.send(crate::audio::PlaySfxAt(
                    crate::audio::Sfx::Place,
                    render_xy(punto.x, punto.y),
                ));
                if !creative {
                    inv.remove(piece.cost_item, piece.cost_qty);
                }
                persist.mark_immediate();
            }
        }
        // Prefab: stamp del área COMPLETA de una (ghost ya mostró el footprint).
        // Piso en TODAS las celdas + paredes en el perímetro (bordes), con su
        // puerta: la habitación queda caminable de pared a pared, como en LDoE.
        BuildSelection::Prefab(p) => {
            let qty = prefab_cost(p);
            if !creative && inv.qty(p.cost_item) < qty {
                return;
            }
            let (min_x, min_z) = prefab_min(p, tx as i64, tz as i64);
            // Validar TODO el footprint antes de gastar: ninguna celda sólida NI
            // con decor cosechable (no se pone un cuarto sobre un árbol/roca).
            for dz in 0..p.h {
                for dx in 0..p.w {
                    if world.tiles.solid_at(body, min_x + dx, min_z + dz)
                        || crate::world2d::cell_build_blocked(
                            &flora,
                            body,
                            z,
                            min_x + dx,
                            min_z + dz,
                        )
                        || !floor_supported_here(
                            &world.tiles,
                            &world.walls,
                            &world.structures,
                            body,
                            z,
                            min_x + dx,
                            min_z + dz,
                        )
                    {
                        return;
                    }
                }
            }
            for dz in 0..p.h {
                for dx in 0..p.w {
                    world.tiles.place_tile(body, min_x + dx, min_z + dz, Tile::WoodFloor);
                }
            }
            let wall_piece = edge_piece_of_kind(WallKind::Wall, p.cost_item);
            let door_piece = edge_piece_of_kind(WallKind::Door, p.cost_item);
            for (ex, ez, axis, kind) in prefab_edges(p, min_x, min_z) {
                let piece = if kind == WallKind::Door { door_piece } else { wall_piece };
                world.walls.add(crate::walls::PlacedWall {
                    body: body as u8,
                    z,
                    tx: ex,
                    tz: ez,
                    axis,
                    piece,
                });
            }
            sfx.send(crate::audio::PlaySfxAt(
                crate::audio::Sfx::Stamp,
                render_xy(punto.x, punto.y),
            ));
            if !creative {
                inv.remove(p.cost_item, qty);
            }
            // Techo del prefab → sistema de techo translúcido (F4).
            world.roofs.add(crate::building::RoofRect {
                body: body as u8,
                z,
                min: (min_x, min_z),
                w: p.w,
                h: p.h,
                art: Some(room_art_key(p)),
            });
            persist.mark_immediate();
        }
        // Asset grande (A2): registra una estructura-sprite + su base sólida.
        // Centrado en la celda apuntada, como un prefab.
        BuildSelection::Large(a) => {
            if !creative && inv.qty(a.cost_item) < a.cost_qty {
                return;
            }
            // Orientación: la que fijaste con GIRAR, o de espaldas a la pared.
            let rot = facing
                .0
                .unwrap_or_else(|| auto_facing(&world.walls, body, z, tx as i64, tz as i64));
            let (fw, fh) = ha_shared::world_edits::footprint_rotated(a.w, a.h, rot);
            // Un mueble de interior no se planta en la calle.
            if !creative
                && a.cat == ha_shared::placeables::PlaceCat::Mueble
                && !cell_is_indoors(
                    &world.ceilings, &world.roofs, &world.walls, &world.tiles, body, z,
                    tx as i64, tz as i64,
                )
            {
                return;
            }
            let (min_x, min_z) = (tx as i64 - fw / 2, tz as i64 - fh / 2);
            // Footprint despejado: nada sólido, ni decor cosechable, ni otra
            // estructura (no se apilan).
            for dz in 0..fh {
                for dx in 0..fw {
                    let (cx, cz) = (min_x + dx, min_z + dz);
                    if world.tiles.solid_at(body, cx, cz)
                        || crate::world2d::cell_build_blocked(&flora, body, z, cx, cz)
                        || world.structures.blocks(body, z, cx, cz)
                        || !floor_supported_here(
                            &world.tiles,
                            &world.walls,
                            &world.structures,
                            body,
                            z,
                            cx,
                            cz,
                        )
                    {
                        return;
                    }
                }
            }
            // Colisión DINÁMICA: si el asset tiene huella pintada (A1), guardamos sus
            // celdas reales (sólo eso bloquea); si no, base completa (`solid` vacío).
            let solid = art
                .collision_footprint(&[a.art])
                .map(|fp| {
                    fp.solid
                        .iter()
                        .copied()
                        .filter(|&(c, r)| (c as i64) < a.w && (r as i64) < a.h)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            // `w`/`h` se guardan SIN girar: son la medida del asset. Quien dibuja
            // y quien colisiona aplican `rot` (`footprint_rotated`), así que la
            // orientación es un dato y no una copia deformada del tamaño.
            world.structures.add(crate::structures::PlacedStructure {
                body: body as u8,
                z,
                cx: min_x,
                cz: min_z,
                w: a.w,
                h: a.h,
                art: a.art.to_string(),
                solid,
                rot,
            });
            sfx.send(crate::audio::PlaySfxAt(
                crate::audio::Sfx::Stamp,
                render_xy(punto.x, punto.y),
            ));
            if !creative {
                inv.remove(a.cost_item, a.cost_qty);
            }
            persist.mark_immediate();
        }
        // Ya resueltos arriba (borde y tapa tienen su propio camino).
        BuildSelection::Edge(_) | BuildSelection::Ceiling(_) => {}
    }
}

/// **Colocación / demolición de una TAPA.** Las mismas dos reglas que el borde:
/// 1. Si la celda ya tiene tapa colocada → la saca y devuelve el material.
/// 2. Si no, exige apoyo (`ceiling_supported`: piso construido o una pared en
///    algún borde de la celda) y material; en creativo se saltea todo.
///
/// La tapa DERIVADA de la losa de arriba no se toca acá: ésa no es una pieza, es
/// el piso del vecino de arriba, y se quita quitando esa losa.
#[allow(clippy::too_many_arguments)]
fn apply_ceiling_build(
    piece: &'static CeilingPiece,
    piece_idx: usize,
    tx: i64,
    tz: i64,
    ctx: &WorldContextRuntime,
    planet: &ActivePlanet,
    creative: bool,
    world: &mut BuildTargets,
    inv: &mut PlayerInventory,
    persist: &mut InventoryPersistState,
    sfx: &mut EventWriter<crate::audio::PlaySfxAt>,
    punto: Vec2,
) {
    let body = active_surface_body(ctx, planet);
    let z = world.tiles.z;
    if let Some(old) = world.ceilings.remove(body, z, tx, tz) {
        if !creative {
            if let Some(def) = ceiling_piece(old.piece) {
                inv.add(def.cost_item, def.cost_qty);
            }
        }
        sfx.send(crate::audio::PlaySfxAt(
            crate::audio::Sfx::Dig,
            render_xy(punto.x, punto.y),
        ));
        persist.mark_immediate();
        return;
    }
    if !creative {
        if inv.qty(piece.cost_item) < piece.cost_qty {
            return;
        }
        let has_floor = |cx: i64, cz: i64| is_build_floor(world.tiles.tile_at(body, cx, cz));
        let has_wall = |cx: i64, cz: i64| world.walls.any_edge_of_cell(body, z, cx, cz);
        if !ceiling_supported(&has_floor, &has_wall, tx, tz) {
            return;
        }
    }
    world.ceilings.add(crate::ceilings::PlacedCeiling {
        body: body as u8,
        z,
        tx,
        tz,
        piece: piece_idx,
    });
    sfx.send(crate::audio::PlaySfxAt(
        crate::audio::Sfx::Place,
        render_xy(punto.x, punto.y),
    ));
    if !creative {
        inv.remove(piece.cost_item, piece.cost_qty);
    }
    persist.mark_immediate();
}

/// Índice de paleta de la pieza de borde de ese tipo hecha del material pedido
/// (madera/piedra); cae a la primera de ese tipo. Lo usan los prefabs para
/// levantar sus paredes con el mismo material que cobran.
fn edge_piece_of_kind(kind: WallKind, cost_item: &str) -> usize {
    (0..edge_piece_count())
        .find(|&i| {
            let p = edge_piece(i).unwrap();
            p.kind == kind && p.cost_item == cost_item
        })
        .or_else(|| (0..edge_piece_count()).find(|&i| edge_piece(i).unwrap().kind == kind))
        .unwrap_or(0)
}

/// **Colocación / demolición de una pieza de BORDE.** Reglas (LDoE):
/// 1. Si el borde apuntado ya tiene algo → lo demuele y devuelve el material.
/// 2. Si no, exige piso construido de al menos un lado (`wall_supported`) y
///    material suficiente; en creativo se saltea ambas cosas.
#[allow(clippy::too_many_arguments)]
fn apply_edge_build(
    piece: &'static EdgePiece,
    piece_idx: usize,
    arista: (i64, i64, WallAxis),
    punto: Vec2,
    ctx: &WorldContextRuntime,
    planet: &ActivePlanet,
    creative: bool,
    world: &mut BuildTargets,
    inv: &mut PlayerInventory,
    persist: &mut InventoryPersistState,
    sfx: &mut EventWriter<crate::audio::PlaySfxAt>,
) {
    let (tx, tz, axis) = arista;
    let body = active_surface_body(ctx, planet);
    let z = world.tiles.z;
    // 1. Demoler lo que haya en ese borde (devuelve el material).
    if let Some(old) = world.walls.remove(body, z, tx, tz, axis) {
        if !creative {
            let def = old.def();
            inv.add(def.cost_item, def.cost_qty);
        }
        sfx.send(crate::audio::PlaySfxAt(
            crate::audio::Sfx::Dig,
            render_xy(punto.x, punto.y),
        ));
        persist.mark_immediate();
        return;
    }
    // 2. Colocar.
    if !creative {
        if inv.qty(piece.cost_item) < piece.cost_qty {
            return;
        }
        let has_floor = |cx: i64, cz: i64| is_build_floor(world.tiles.tile_at(body, cx, cz));
        if !wall_supported(&has_floor, tx, tz, axis) {
            return;
        }
    }
    world.walls.add(crate::walls::PlacedWall {
        body: body as u8,
        z,
        tx,
        tz,
        axis,
        piece: piece_idx,
    });
    sfx.send(crate::audio::PlaySfxAt(
        crate::audio::Sfx::Place,
        render_xy(punto.x, punto.y),
    ));
    if !creative {
        inv.remove(piece.cost_item, piece.cost_qty);
    }
    persist.mark_immediate();
}

/// Slot de arte `room/*` del prefab (el usuario recorta la pose Techo ahí).
fn room_art_key(p: &Prefab) -> &'static str {
    match p.id {
        "room_small" => "room/room_small",
        "room_medium" => "room/room_medium",
        "room_large" => "room/room_large",
        _ => "room/casa_1",
    }
}

fn spawn_build_ghost(mut commands: Commands) {
    commands.spawn((
        BuildGhost,
        Sprite::from_color(Color::srgba(0.25, 1.0, 0.35, 0.6), Vec2::splat(1.0)),
        Transform::from_xyz(0.0, 0.0, crate::layers::OBRA),
        Visibility::Hidden,
    ));
}

/// **El fantasma se dibuja con el PIXEL de la pieza, no con un rectángulo.**
///
/// Un cuadrado verde te dice dónde, pero no QUÉ ni de qué tamaño, y con la escala
/// recién arreglada (`walls::medida_real`) eso importa el doble: una pared mide
/// 1,5 × 3 m y sobresale de su celda. Con el sprite de verdad, traslúcido y
/// teñido, lo que ves antes de aceptar es exactamente lo que va a quedar.
///
/// El tinte va sobre el sprite (`Sprite::color` multiplica la textura): verde
/// entra, naranja demuele, rojo no se puede.
///
/// Se reconstruye sólo cuando cambia algo (pieza, sitio, orientación): armar un
/// sprite por frame es barato pero no gratis, y esto corre mientras se construye.
fn ghost_tinte(ok: bool, demoler: bool) -> Color {
    if demoler {
        Color::srgba(1.0, 0.72, 0.35, 0.85)
    } else if ok {
        Color::srgba(0.55, 1.0, 0.65, 0.75)
    } else {
        Color::srgba(1.0, 0.45, 0.4, 0.7)
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_build_ghost(
    build_mode: Res<BuildModeActive>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    mira: Res<MiraDeObra>,
    art: Res<crate::art2d::ArtLibrary>,
    selected: Res<SelectedBuildPiece>,
    inv: Res<PlayerInventory>,
    tiles: Res<TileWorld>,
    flora: Res<crate::harvest::FloraHarvest>,
    structures: Res<crate::structures::BuiltStructures>,
    walls: Res<crate::walls::BuiltWalls>,
    ceilings: Res<crate::ceilings::BuiltCeilings>,
    roofs: Res<crate::building::BuiltRoofs>,
    facing: Res<BuildFacing>,
    mut ghost: Query<(&mut Transform, &mut Visibility, &mut Sprite), With<BuildGhost>>,
) {
    let Ok((mut tf, mut vis, mut sprite)) = ghost.get_single_mut() else {
        return;
    };
    let placeable = build_mode.active && surface_region_active(&region).is_some();
    let Some(selection) = build_selection(selected.0).filter(|_| placeable) else {
        *vis = Visibility::Hidden;
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    // Pieza de BORDE: el ghost es la propia arista (fina), verde si entra,
    // naranja si ahí ya hay algo (el gesto pasa a DEMOLER), roja si no se puede.
    if let BuildSelection::Edge(piece) = selection {
        let Some((ex, ez, axis)) = mira.borde() else {
            *vis = Visibility::Hidden;
            return;
        };
        *vis = Visibility::Visible;
        let demolish = walls.at(body, tiles.z, ex, ez, axis).is_some();
        let has_floor = |cx: i64, cz: i64| is_build_floor(tiles.tile_at(body, cx, cz));
        let ok = inv.qty(piece.cost_item) >= piece.cost_qty
            && wall_supported(&has_floor, ex, ez, axis);
        // **El fantasma ES la pieza, por el mismo camino que la pieza.**
        //
        // Acá vivía el bug del vano: esto dibujaba siempre la pose `front` entera
        // y `walls` dibujaba otra cosa según la cara que le tocara al borde — para
        // un borde vertical, una franja recortada del MEDIO del dibujo. Y el medio
        // de un vano es el hueco: veías el marco completo, aceptabas, y aparecía
        // *un palo con una puntita blanca*. Mientras el fantasma y la pared no
        // salgan de la misma función, una de las dos miente.
        let cara = crate::walls::cara_de_borde(&has_floor, ex, ez, axis);
        let (nuevo, size) = crate::walls::vista_de_pieza(&art, piece, cara);
        *sprite = nuevo;
        sprite.custom_size = Some(size);
        sprite.color = ghost_tinte(ok, demolish);
        // Mismo apoyo y mismo crecimiento que la pared de verdad
        // (`walls::wall_transform`), en la banda del fantasma — que va por encima
        // de las fachadas: levantando un muro de sur a norte, el fantasma se
        // escondía detrás del frente que acababas de poner.
        let t = crate::walls::wall_transform(ex, ez, axis, size, cara);
        tf.translation = Vec3::new(t.translation.x, t.translation.y, crate::layers::OBRA);
        return;
    }
    let Some((tx, tz)) = mira.celda() else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Visible;
    // TAPA: ghost de la celda entera. Naranja si ahí ya hay una (el gesto pasa a
    // DEMOLER), verde si entra, rojo si falta apoyo o material.
    if let BuildSelection::Ceiling(piece) = selection {
        let (cx, cz) = (tx as i64, tz as i64);
        let demolish = ceilings.at(body, tiles.z, cx, cz).is_some();
        let has_floor = |x: i64, z: i64| is_build_floor(tiles.tile_at(body, x, z));
        let has_wall = |x: i64, z: i64| walls.any_edge_of_cell(body, tiles.z, x, z);
        let ok = inv.qty(piece.cost_item) >= piece.cost_qty
            && ceiling_supported(&has_floor, &has_wall, cx, cz);
        // Una tapa es una SUPERFICIE: mide la celda por definición (si no, no
        // embaldosa). Ver `ArtLibrary::tamano_real`.
        let medida = Vec2::splat(TILE_M);
        *sprite = art
            .pose_sprite(piece.art, "center", medida)
            .unwrap_or_else(|| Sprite::from_color(Color::WHITE, medida));
        sprite.custom_size = Some(medida);
        sprite.color = ghost_tinte(ok, demolish);
        let r = render_xy((tx as f32 + 0.5) * TILE_M, (tz as f32 + 0.5) * TILE_M);
        tf.translation = Vec3::new(r.x, r.y, crate::layers::OBRA);
        return;
    }
    // Una celda bloquea si es sólida, tiene decor cosechable (talar/picar antes)
    // o —en un piso alto— no tiene nada debajo que la aguante.
    let blocked = |cx: i64, cz: i64| {
        tiles.solid_at(body, cx, cz)
            || crate::world2d::cell_build_blocked(&flora, body, tiles.z, cx, cz)
            || structures.blocks(body, tiles.z, cx, cz)
            || !floor_supported_here(&tiles, &walls, &structures, body, tiles.z, cx, cz)
    };
    // Pieza: celda 1×1. Prefab: ghost del footprint COMPLETO w×h, centrado en la
    // celda apuntada. `ok` = materiales suficientes Y todo el footprint despejado.
    let (ok, size, center, arte) = match selection {
        BuildSelection::Piece(piece) => (
            (inv.qty(piece.cost_item) >= piece.cost_qty) && !blocked(tx as i64, tz as i64),
            // Un piso o una escalera son SUPERFICIE: la celda manda.
            Vec2::splat(TILE_M),
            (tx as f32 + 0.5, tz as f32 + 0.5),
            crate::world2d::tile_art_key(piece.tile).map(|k| (k, "center")),
        ),
        BuildSelection::Prefab(p) => {
            let (min_x, min_z) = prefab_min(p, tx as i64, tz as i64);
            let clear = (0..p.h).all(|dz| (0..p.w).all(|dx| !blocked(min_x + dx, min_z + dz)));
            (
                (inv.qty(p.cost_item) >= prefab_cost(p)) && clear,
                Vec2::new(p.w as f32 * TILE_M, p.h as f32 * TILE_M),
                (
                    min_x as f32 + p.w as f32 * 0.5,
                    min_z as f32 + p.h as f32 * 0.5,
                ),
                Some((room_art_key(p), "center")),
            )
        }
        BuildSelection::Large(a) => {
            // El ghost muestra la huella YA GIRADA y aplica la regla de interior:
            // el rojo tiene que significar "asi no entra", incluido "esto va
            // adentro de una casa".
            let rot = facing
                .0
                .unwrap_or_else(|| auto_facing(&walls, body, tiles.z, tx as i64, tz as i64));
            let (fw, fh) = ha_shared::world_edits::footprint_rotated(a.w, a.h, rot);
            let (min_x, min_z) = (tx as i64 - fw / 2, tz as i64 - fh / 2);
            let clear = (0..fh).all(|dz| (0..fw).all(|dx| !blocked(min_x + dx, min_z + dz)));
            let indoors = a.cat != ha_shared::placeables::PlaceCat::Mueble
                || cell_is_indoors(
                    &ceilings, &roofs, &walls, &tiles, body, tiles.z, tx as i64, tz as i64,
                );
            let pose = ha_shared::world_edits::FACING_POSES[(rot % 4) as usize];
            (
                (inv.qty(a.cost_item) >= a.cost_qty) && clear && indoors,
                // Un OBJETO se dibuja a su tamaño real; la huella (`fw × fh`)
                // sigue siendo de tiles enteros porque es lo que ocupa, no lo que
                // mide. Ver `ArtLibrary::tamano_real`.
                art.tamano_real(a.art, Some(pose), Vec2::new(fw as f32 * TILE_M, fh as f32 * TILE_M)),
                (min_x as f32 + fw as f32 * 0.5, min_z as f32 + fh as f32 * 0.5),
                Some((a.art, pose)),
            )
        }
        // Resueltos arriba (borde y tapa tienen su propio ghost).
        BuildSelection::Edge(_) | BuildSelection::Ceiling(_) => return,
    };
    // El pixel de la pieza, teñido. Sin arte asignado queda el quad de color de
    // siempre, que es lo único que se puede dibujar sin pixel del usuario.
    *sprite = arte
        .and_then(|(k, pose)| art.pose_sprite(k, pose, size).or_else(|| art.pose_sprite(k, "center", size)))
        .unwrap_or_else(|| Sprite::from_color(Color::WHITE, size));
    sprite.custom_size = Some(size);
    sprite.color = ghost_tinte(ok, false);
    let r = render_xy(center.0, center.1);
    tf.translation = Vec3::new(r.x, r.y, crate::layers::OBRA);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **El fantasma se corre solo hacia donde estás construyendo.**
    ///
    /// El pedido, textual: *"una vez colocado, que el fantasma se vaya hacia el
    /// lado así si quiero seguir construyendo solo sigo aceptando"*. La dirección
    /// no se elige en ningún menú: se aprende del segundo toque.
    #[test]
    fn el_fantasma_aprende_hacia_donde_vas_y_avanza_solo() {
        let mut m = MiraDeObra::default();
        // Primer toque: sin historia, manda el rumbo del jugador (acá, el norte).
        m.fijar(Sitio::Celda(10, 10), (0, -1));
        assert_eq!(m.celda(), Some((10, 10)));
        m.avanzar();
        assert_eq!(m.celda(), Some((10, 9)), "sin historia sigue tu rumbo");
        // Segundo toque hacia el este: a partir de ahí la obra va al este.
        m.fijar(Sitio::Celda(20, 20), (0, -1));
        m.fijar(Sitio::Celda(21, 20), (0, -1));
        m.avanzar();
        assert_eq!(m.celda(), Some((22, 20)), "aprendió que vas al este");
        m.avanzar();
        assert_eq!(m.celda(), Some((23, 20)), "y sigue: aceptar cuatro veces es un muro");
    }

    /// **Dos toques en diagonal no dejan un avance diagonal.** No hay media celda
    /// en una cuadrícula: se toma la componente dominante o el muro sale escalonado
    /// sin que nadie lo haya pedido.
    #[test]
    fn el_avance_es_cardinal_aunque_toques_en_diagonal() {
        let mut m = MiraDeObra::default();
        m.fijar(Sitio::Celda(0, 0), (1, 0));
        m.fijar(Sitio::Celda(5, 2), (1, 0)); // dx=5, dz=2 → manda el este
        m.avanzar();
        assert_eq!(m.celda(), Some((6, 2)));
    }

    /// **Una pared avanza a lo largo de SU línea.** Un muro se levanta en fila; si
    /// el avance fuera libre, aceptar dos veces pondría la segunda pieza en una
    /// arista perpendicular, que no es nunca lo que se quiere.
    #[test]
    fn una_pared_avanza_por_su_propio_eje() {
        // Muro horizontal: la fila corre en x, pase lo que pase con el rumbo.
        let mut m = MiraDeObra::default();
        m.fijar(Sitio::Borde(4, 4, WallAxis::H), (0, 1));
        m.avanzar();
        assert_eq!(m.borde(), Some((5, 4, WallAxis::H)), "un muro E-O crece en x");
        // Muro vertical: la fila corre en z.
        let mut m = MiraDeObra::default();
        m.fijar(Sitio::Borde(4, 4, WallAxis::V), (1, 0));
        m.avanzar();
        assert_eq!(m.borde(), Some((4, 5, WallAxis::V)), "un muro N-S crece en z");
    }

    /// **La tarjeta que tocás pone EXACTAMENTE lo que dice.**
    ///
    /// El menú dibuja los colocables agrupados por categoría (`hud.rs`) pero el
    /// índice de paleta sigue siendo el de la tabla. Es la clase de acoplamiento
    /// que se rompe en silencio: reordenás una categoría, el menú se ve perfecto y
    /// tocar "Heladera" planta un colectivo. Acá se recorre la paleta igual que la
    /// dibuja el HUD y se exige que el índice devuelva el mismo objeto.
    #[test]
    fn cada_tarjeta_del_menu_coloca_su_propio_objeto() {
        let lbase = build_piece_count() + prefab_count();
        let mut vistas = 0;
        for &cat in ha_shared::placeables::PlaceCat::all() {
            for (i, esperado) in ha_shared::placeables::placeables_in(cat) {
                let Some(BuildSelection::Large(puesto)) = build_selection(lbase + i) else {
                    panic!("la tarjeta {} ({}) no resuelve a un colocable", lbase + i, esperado.label);
                };
                assert_eq!(
                    puesto.art, esperado.art,
                    "la tarjeta de '{}' coloca '{}'",
                    esperado.label, puesto.label
                );
                vistas += 1;
            }
        }
        assert_eq!(vistas, large_asset_count(), "el menu no muestra la tabla entera");
    }

    /// Las cuatro familias de la paleta no pueden pisarse: si los rangos se
    /// solaparan, una pieza de borde se colocaría como estructura (o al revés).
    #[test]
    fn los_rangos_de_la_paleta_no_se_solapan() {
        let pc = build_piece_count();
        let prc = prefab_count();
        let lc = large_asset_count();
        for i in 0..pc {
            assert!(matches!(build_selection(i), Some(BuildSelection::Piece(_))), "{i} no es pieza");
        }
        for i in pc..pc + prc {
            assert!(matches!(build_selection(i), Some(BuildSelection::Prefab(_))), "{i} no es prefab");
        }
        for i in pc + prc..pc + prc + lc {
            assert!(matches!(build_selection(i), Some(BuildSelection::Large(_))), "{i} no es colocable");
        }
        let ebase = edge_palette_base();
        for i in 0..ha_shared::build_grid::edge_piece_count() {
            assert!(
                matches!(build_selection(ebase + i), Some(BuildSelection::Edge(_))),
                "{} no es borde",
                ebase + i
            );
        }
        // El spawn de fauna vive MUY por encima de la paleta: si algún día la
        // tabla creciera hasta ahí, tocar un colocable spawnearía una vaca.
        assert!(
            ebase + ha_shared::build_grid::edge_piece_count() < crate::fauna::SPAWN_PALETTE_BASE,
            "la paleta alcanzo al rango de spawn de fauna"
        );
    }
}
