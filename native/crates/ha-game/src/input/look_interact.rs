use bevy::prelude::*;
use ha_shared::{get_class, ClassDef, DEFAULT_CLASS_ID};

use super::actions;
use crate::targeting::LookTarget;
use crate::game_ui::UiGameplayBlock;
use crate::ui_editor::UiEditorState;

/// Build-mode toggle (modular pieces) — legacy switch kept for the HUD.
#[derive(Resource, Default)]
pub struct BuildModeActive {
    pub active: bool,
}

#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct HoldInteractState {
    pub active: bool,
    pub progress_ms: u32,
    pub duration_ms: u32,
    pub cancelled: bool,
    pub completed: bool,
    /// **Se soltó ANTES de completar la pulsación, sin arrastrar.** O sea: un
    /// TOQUE, no un mantenido.
    ///
    /// Existe porque el gesto de un objeto colocado tiene dos significados
    /// distintos y hasta ahora sólo había uno: tocar un cofre lo ABRE, mantener
    /// lo LEVANTA para volver a colocarlo donde quieras. Sin esta señal el único
    /// verbo disponible era el mantenido, y usar una cosa costaba lo mismo que
    /// desarmarla.
    ///
    /// Lo consume quien lo atiende (poniéndolo en `false`), igual que
    /// `completed`.
    pub tapped: bool,
    /// Hammer build gesture without interact target.
    pub placing_machine: bool,
    /// Hammer + build mode — modular snap placement.
    pub placing_build: bool,
    /// Launch / orbital beacon hold — looser drift, no machine steal.
    pub flight_hold: bool,
    pub locked_kind: Option<ha_shared::InteractTargetKind>,
    /// Hold without target + hammer: place stone extractor instead of wood.
    pub placing_stone: bool,
    /// Block item equipped + valid place slot — Minecraft-style place hold.
    pub placing_block: bool,
    /// Food equipped + no target — hold to eat (consumed in `survival_stats.rs`).
    pub eating: bool,
    /// Cofre equipado + sin objetivo — lo coloca `chests.rs`.
    pub placing_chest: bool,
    /// Antorcha equipada + sin objetivo — la coloca `torches.rs` (suelo o pared,
    /// lo decide el mundo: ver `torches::resolve_mount`).
    pub placing_torch: bool,
    /// Mapa equipado + sin objetivo — lo cuelga `map_item.rs` en la pared/mesa
    /// que tenés delante, retratando el sector donde estás.
    pub placing_map: bool,
    /// Semilla en la mano + celda sembrable — la siembra `granja.rs`.
    pub placing_crop: bool,
    /// Cubo en la mano + líquido (llenar) o suelo seco (volcar) — `cubo.rs`.
    pub usar_cubo: bool,
}

impl HoldInteractState {
    /// **¿Este hold es una COLOCACIÓN?** Quien la consume (world2d, machines,
    /// build, chests, torches, map_item) es el que limpia `completed`; el HUD no
    /// tiene que comérselo antes.
    ///
    /// Existe porque la condición estaba escrita como una LISTA suelta de tres
    /// flags en `hud::update_hud_state`: cualquier colocación nueva (el cofre, la
    /// antorcha, ahora el mapa) que no se acordara de agregarse a esa lista veía
    /// su `completed` borrado antes de llegar a su sistema — y el objeto
    /// simplemente no se colocaba, sin error ni pista.
    pub fn is_placement(&self) -> bool {
        self.placing_block
            || self.placing_machine
            || self.placing_build
            || self.placing_chest
            || self.placing_torch
            || self.placing_map
            || self.placing_crop
            || self.usar_cubo
    }
}

#[derive(Resource, Default)]
pub struct AbilityTapPulse {
    pub slot: Option<usize>,
}

#[derive(Resource, Default)]
pub struct AbilityCooldown {
    pub slot: usize,
    pub remaining_ms: f32,
    pub total_ms: f32,
}

pub struct LookInteractPlugin;

impl Plugin for LookInteractPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildModeActive>()
            .init_resource::<HoldInteractState>()
            .init_resource::<AbilityTapPulse>()
            .init_resource::<AbilityCooldown>()
            .init_resource::<EquippedItem>()
            .add_systems(
                Update,
                actions::highlight_action_buttons.run_if(
                    |editor: Res<UiEditorState>, block: Res<UiGameplayBlock>| {
                        !editor.enabled && !editor.menu_open && !block.blocked
                    },
                ),
            )
            .add_systems(FixedUpdate, (tick_hold_interact, tick_ability_cooldown));
    }
}

#[derive(Resource, Clone)]
pub struct EquippedItem {
    pub item_id: String,
    pub class: ClassDef,
}

impl Default for EquippedItem {
    fn default() -> Self {
        Self {
            item_id: "hammer".to_string(),
            class: get_class(DEFAULT_CLASS_ID),
        }
    }
}

impl HoldInteractState {
    /// El dedo/botón se levantó. `dragged` = venía arrastrando, o sea que era
    /// scroll/apuntado y no un toque.
    pub fn release(&mut self, dragged: bool) {
        if self.active && !self.completed {
            self.tapped = !dragged;
            self.cancelled = true;
            self.active = false;
            self.progress_ms = 0;
        }
    }
}

pub(crate) fn tick_hold_interact(
    time: Res<Time<Fixed>>,
    mut hold: ResMut<HoldInteractState>,
    target: Res<LookTarget>,
) {
    let _p = crate::debug::probe("fx_tick_hold_interact");
    if hold.completed || hold.cancelled {
        hold.active = false;
        return;
    }
    if !hold.active {
        return;
    }
    let target_valid = target.entity.is_some()
        || target.block_cell.is_some()
        || target.flora_cell.is_some()
        || hold.placing_machine
        || hold.placing_build
        || hold.eating
        // Plantar un cofre se hace sobre suelo vacío: no hay objetivo que
        // validar, igual que colocar una máquina.
        || hold.placing_chest
        || hold.placing_torch
        || hold.placing_map
        || (hold.placing_block && target.place_cell.is_some())
        || (hold.placing_crop && target.place_cell.is_some())
        || (hold.usar_cubo && target.place_cell.is_some())
        || (hold.flight_hold && hold.locked_kind.is_some());
    let delta_ms = (time.delta_secs() * 1000.0) as u32;
    let next = ha_shared::advance_hold_progress(
        ha_shared::HoldProgress {
            active: hold.active,
            progress_ms: hold.progress_ms,
            duration_ms: hold.duration_ms,
            cancelled: hold.cancelled,
            completed: hold.completed,
            placing_machine: hold.placing_machine,
            placing_build: hold.placing_build,
        },
        delta_ms,
        target_valid,
    );
    hold.active = next.active;
    hold.progress_ms = next.progress_ms;
    hold.completed = next.completed;
}

fn tick_ability_cooldown(time: Res<Time<Fixed>>, mut cd: ResMut<AbilityCooldown>) {
    let _p = crate::debug::probe("fx_tick_ability_cooldown");
    let delta_ms = time.delta_secs() * 1000.0;
    cd.remaining_ms = ha_shared::tick_ability_cooldown(cd.remaining_ms, delta_ms);
}
