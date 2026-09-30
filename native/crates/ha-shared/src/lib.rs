//! Hunger Arena shared logic — Rust port of `packages/shared` (constants, palette, movement).
//! The TypeScript sim remains authoritative for netcode until full parity tests land.

pub mod assets;
pub mod blocks;
pub mod catalog_parity;
pub mod celestial_bodies;
pub mod classes;
pub mod constants;
pub mod golden_movement;
pub mod hold;
pub mod input;
pub mod inventory;
pub mod interactions;
pub mod items;
pub mod recipes;
pub mod launch_pad;
pub mod parkour;
pub mod planet_surface;
pub mod machines;
pub mod movement;
pub mod palette;
pub mod partida;
pub mod rios;
pub mod grueso;
pub mod ps1_palette;
pub mod worldgen;
pub mod amueblado;
/// Tablas de siembra del pack (generadas por `scripts/import-pack-masivo.py`).
pub mod interior_siembra;
pub mod pack_siembra;
pub mod rng;
pub mod credos;
pub mod sims;
pub mod sociedad;
pub mod civ;
pub mod types;
pub mod ui;
pub mod cosmic_scale;
pub mod placeables;
pub mod topdown;
pub mod build_grid;
pub mod edificio;
pub mod escaleras;
pub mod cuevas;
pub mod zonas;
pub mod transition;
pub mod world_edits;
pub mod fauna_brain;
pub mod flight_physics;
pub mod luna_locomotion;
pub mod luna_terrain;
pub mod surface_locomotion;
pub mod surface_deposits;
pub mod surface_biome;
pub mod surface_climate;
pub mod surface_chunk;
pub mod biome_content;
pub mod surface_flora;
pub mod building;
pub mod world;
pub mod world_context;
pub mod nav_data;
pub mod ship_mass;
pub mod solar_light;
pub mod geography;
/// Ecología por REGIÓN: capacidad de carga por bioma + cadena alimenticia
/// (plantas → herbívoros → carnívoros) que avanza con o sin el jugador delante.
/// Es lo que reemplazó al anillo de fauna alrededor del jugador.
pub mod ecology;
/// Relieve por terrazas: acantilados, rampas y niveles de líquido. Lo consume
/// `geography` para decidir el tile de cada celda.
pub mod relief;
/// Los cuerpos de agua: orilla por bioma, cascadas derivadas del relieve y qué
/// flota en cada sitio. Ver el módulo.
pub mod agua;
/// Plantilla macro de cada cuerpo: continentes, cordilleras, cuencas, islas y
/// ciudades con nombre. Es la forma del mundo; `relief` es su textura.
pub mod world_map;
/// Red vial: jerarquía de vías con anchos reales, puertos de montaña y carriles.
pub mod cercas;
pub mod roads;
/// Barrios: distritos, parcelas de tamaño real y el programa de cada lote.
pub mod city;
pub mod minor_bodies;
pub mod solar_ephemeris;
pub mod colony;
pub mod space_base;
pub mod survival;
pub mod vessel;
pub mod power;
pub mod connectivity;

pub use assets::{
    asset_kind_for_interactable, asset_kind_for_obstacle, asset_kind_for_world_prop,
    get_asset, palette_rgb, require_asset, AssetCategory, AssetClass, AssetDef, AssetRenderHints,
    PaletteKey, ASSET_CATALOG,
};
pub use classes::{
    capsule_center_above_feet, capsule_half_length, eye_height_above_feet, get_class, ClassDef,
    DEFAULT_CLASS_ID,
};
pub use interactions::{
    interaction_duration_ms, interact_radius_for, interact_reach_for, interact_target_for_kind,
    is_flight_hold_target, resolve_interaction, InteractActionKind, InteractTargetKind,
    InteractionRule, GESTURE_FLIGHT_MOVE_CANCEL_PX, GESTURE_HOLD_START_MS,
    GESTURE_MACHINE_HOLD_DELAY_MS, GESTURE_MOVE_CANCEL_PX, GESTURE_TAP_MAX_MS, INTERACT_REACH,
    INTERACTION_RULES,
};
pub use constants::*;
pub use hold::{advance_hold_progress, tick_ability_cooldown, HoldProgress};
pub use colony::{
    can_request_colonist, colony_capacity, colony_fed, machine_boost, step_colony, ColonyState,
    COLONISTS_PER_HABITAT, COLONIST_BOOST, COLONIST_SEND_COST_APPLES, SUPPLY_SEND_APPLES,
};
pub use survival::{
    apply_food, food_restore, step_survival_stats, SurvivalEnv, SurvivalStats,
    OXYGEN_DRAIN_VACUUM, OXYGEN_MAX, OXYGEN_RECHARGE, SUFFOCATE_DAMAGE,
};
pub use input::{empty_input, InputState};
pub use inventory::PlayerInventory;
pub use items::{item_display_name, item_hotbar_label};
pub use machines::{get_machine, require_machine, MachineDef, ANTENNA, HABITAT, MACHINE_CATALOG, LAUNCH_PAD, REGOLITH_DRILL, STONE_EXTRACTOR, WOOD_EXTRACTOR};
pub use launch_pad::{
    assemble_module, can_assemble_module, create_launch_pad_state, LaunchPadState,
    LAUNCH_MODULE_COSTS, LAUNCH_PAD_MODULE_COUNT,
};
pub use solar_light::{
    gameplay_world_to_flight, orbital_dock_flight_pos, sample_solar, spin_sun_for_earth_day,
    sun_center_world, sun_direction_to, sun_light_direction, SolarSample,
};
pub use space_base::{
    assemble_space_module, can_assemble_space_module, create_space_station_state,
    SpaceStationState, SPACE_MODULE_COSTS, SPACE_STATION_MODULE_COUNT,
};
pub use vessel::{
    tick_off_screen_vessels, vessel_flight_pos, ConnectivitySnapshot, PowerSnapshot,
    VesselId, VesselRegistry, VesselSimMode, VesselState, VESSEL_TICK_HZ,
};
pub use power::{ModuleAssembly, ModuleKind, BATTERY_CAPACITY_WH, SOLAR_PANEL_PEAK_W};
pub use connectivity::{earth_uplink_pos, has_line_of_sight, orbital_repeater_pos, sample_connectivity_at};
pub use world_context::{gravity_for_context, gravity_scale_for_context, WorldContext};
pub use recipes::{can_craft, craft, get_craft_recipe, CraftRecipe, CRAFT_RECIPES};
pub use movement::{clamp_body, step_movement, MovementBody};
pub use parkour::{parkour_button_hint, ParkourButtonHint};
pub use palette::{hybrid_lights, palette, Palette, HybridLights};
pub use ps1_palette::Ps1Retro;
pub use rng::Rng;
pub use types::{EntityKind, MovementState, Obstacle};
pub use ui::{get_ui_icon, require_ui_icon, UiIconDef, UiIconVariant, UI_ICON_CATALOG};
pub use cosmic_scale::{
    EARTH_GAMEPLAY_RADIUS_M, EARTH_MOON_DISTANCE_M, EARTH_RADIUS_M, HUB_EARTH_VISUAL_DISTANCE_M,
    HUB_EARTH_VISUAL_RADIUS_M, HUB_MOON_VISUAL_RADIUS_M, HUMAN_EYE_HEIGHT_M, HUMAN_HEIGHT_M,
    LEO_ALTITUDE_M, LUNA_SKY_EARTH_DISTANCE_M, LUNA_SKY_EARTH_RADIUS_M, MOON_GAMEPLAY_RADIUS_M,
    MOON_RADIUS_M, MOON_SURFACE_RADIUS_M, MOON_TO_EARTH_RADIUS_RATIO, PLAYER_COLLISION_RADIUS_M,
    SPACE_MOON_SKY_X_M, SPACE_MOON_SKY_Y_M, SPACE_MOON_SKY_Z_M, SPACE_STARFIELD_RADIUS_M,
};
pub use celestial_bodies::{
    body_atmo_top, body_center, body_def, body_gravity_surface, body_landable, body_radius, body_soi,
    body_transit_burn, body_terrain_seed, body_walk_radius, CelestialBody, EARTH_CENTER_Y,
    EARTH_SOI_M, MOON_SOI_M, EARTH_ATMO_TOP_M, EARTH_FLIGHT_RADIUS_M, MOON_FLIGHT_RADIUS_M,
    MOON_ORBIT_X_M,
};
pub use transition::{transition_profile, TransitionProfile};
pub use flight_physics::{
    altitude_above_body, altitude_above_body_at, atmosphere_drag, dominant_body, dominant_body_at,
    earth_center, gravity_accel_at, gravity_accel_at_time, gravity_accel_luna_gameplay,
    gravity_accel_planet_gameplay, in_vacuum, moon_center, parachute_drag,
    remap_luna_gameplay_to_orbital, should_auto_deploy_parachute, sun_proximity_damage,
    surface_normal, surface_normal_at, surface_world_pos, surface_world_pos_at,
    effective_safe_landing_v, FlightPhase, ROCKET_CRUISE_MAX_V, ROCKET_FUEL_BURN_PER_S,
    ROCKET_FUEL_MAX, ROCKET_MAX_V,
    ROCKET_RETRO_FUEL_BURN_PER_S, ROCKET_RETRO_THRUST_M_S2, ROCKET_ROT_SPEED, ROCKET_THRUST_M_S2,
    SAFE_LANDING_V, PARACHUTE_DEPLOY_ALT_M,
};
pub use planet_surface::{
    altitude_above_planet_gameplay, block_surface_steep_step, block_surface_unwalkable_ascent,
    follow_surface_ground, planet_dir_from_tangent, planet_displacement_at_dir,
    planet_gameplay_center, planet_gameplay_world_pos, planet_ground_y_at, planet_tangent_from_dir,
    planet_tangent_half, remap_planet_gameplay_to_orbital, remap_planet_gameplay_to_orbital_at,
    surface_relief_at, surface_slope_at,
};
pub use luna_locomotion::step_luna_movement;
pub use luna_terrain::{
    altitude_above_luna_gameplay, block_luna_steep_step, block_luna_unwalkable_ascent,
    luna_albedo_at_dir, luna_dir_from_tangent, luna_displacement_at, luna_displacement_at_dir,
    luna_gameplay_center, luna_gameplay_world_pos, luna_ground_y_at, luna_slope_at,
    luna_tangent_from_dir, luna_wrap_tangent, luna_clamp_tangent, luna_crater_rim_markers,
    LUNA_TANGENT_HALF,
};
pub use blocks::{
    cell_of, is_block_item, raycast_terrain_place_cell, BlockCell, PlacedBlockKind,
    PlacedBlockWorld, BLOCK_STEP_UP,
};
pub use surface_locomotion::{
    effective_ground_y, step_surface_creative_fly, step_surface_movement, step_surface_movement_with_blocks,
};
pub use surface_deposits::{
    generate_earth_deposits, generate_luna_deposits, generate_planet_deposits, SurfaceDeposit,
};
pub use surface_biome::{surface_biome_at, SurfaceBiome};
pub use surface_climate::{sample_climate, ClimateSample};
pub use surface_chunk::{
    chunk_center, chunk_key_at, generate_chunk_deposits, generate_chunk_flora,
    surface_chunk_load_radius, SURFACE_CHUNK_LOAD_RADIUS, SURFACE_CHUNK_SIZE,
};
pub use surface_flora::FloraProp;
pub use building::{
    damage_build_piece, preview_placement, snap_scalar, snap_yaw_quarter, BuildPieceKind,
    BuildPlacementPreview, PlacedBuildPiece, BUILD_FLOOR_THICK, BUILD_GRID, BUILD_PIECE_HP,
    BUILD_WALL_HEIGHT,
};
pub use nav_data::{
    probe_transmit_secs, probe_travel_secs, NavData, ProbeMission, ProbePhase,
    PROBE_SCAN_SECS, PROBE_SPEED_FACTOR,
};
pub use ship_mass::{
    burn_factor, clamp_ship_config, fuel_max_for_tanks, ship_config_stats, ship_mass_t,
    thrust_factor, transit_fuel_cost, ShipConfig, ShipConfigStats,
    AUTOPILOT_BURN_EFFICIENCY, FUEL_CAP_PER_TANK, MASS_REF_T, SHIP_MAX_ENGINES,
    SHIP_MAX_EXTRA_TANKS, SHIP_MAX_FINS, SHIP_MAX_LANDERS, SHIP_MAX_NOSE, TRANSIT_FUEL_MARGIN,
};
pub use solar_ephemeris::{
    body_center_at, body_intercept_point, direction_to, earth_center_at,
    gameplay_shell_center_at, moon_center_at, plan_transit, sun_position,
    transit_cruise_secs, TransitPlan,
};
pub use world::{
    collision_obstacles, generate_world, ground_height_at, ground_height_at_footprint,
    ground_height_for_locomotion, GeneratedWorld, InteractableKind, InteractableNode,
};
