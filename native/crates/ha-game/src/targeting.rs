//! 2D targeting — **qué hay debajo del dedo**.
//!
//! Fills the same `LookTarget` contract the HUD/interaction systems already
//! consume: entity targets (trees, rocks, ore, gems, machines, pad modules),
//! diggable tiles (`block_cell`) and placement slots (`place_cell`).
//!
//! Sin dedo apoyado NO hay objetivo (ver `AimCursor::targeting`): antes había una
//! mira que siempre estaba en algún lado, así que siempre había algo "apuntado" y
//! el HUD siempre tenía algo que decir. Ahora la pantalla se queda callada hasta
//! que tocás.

use bevy::prelude::*;
use ha_shared::topdown::TILE_M;
use ha_shared::{interact_reach_for, resolve_interaction, InteractTargetKind};

use crate::input::{AimCursor, EquippedItem, HoldInteractState, AIM_MAX_REACH};
use crate::scene::{RegionId, RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::world2d::{active_surface_body, render_xy, ActivePlanet, TileWorld};

/// Reach for digging/placing tiles (m) — el mismo brazo que valida el toque.
const TILE_REACH: f32 = AIM_MAX_REACH;
/// Radio de picado alrededor del centro del sprite (m), para lo que no declara
/// tamaño. Los que sí lo declaran se pican por su RECTÁNGULO: tocar la copa de un
/// árbol tiene que agarrar el árbol, y la copa está dos metros por encima de su
/// centro.
const POINT_PICK_RADIUS: f32 = 1.35;
/// **Margen de perdón del dedo** (m). La cuenta, no el ojo: a pie la cámara
/// encuadra 30 m de alto (`universe::view_h_for_alt`) sobre los ~1080 px del lado
/// corto del Redmi, o sea **36 px por metro**, y un tile es 1 m (`TILE_M`). La
/// yema apoya un óvalo de 7-10 mm ≈ 40-60 px: **el dedo es más grande que el
/// banco de la vereda**, y el punto que reporta Android es su centroide, que cae
/// medio tile de donde el jugador cree que apretó. Por eso apuntar a un prop
/// fallaba más veces de las que acertaba. 0,55 m ≈ 20 px ≈ medio dedo.
///
/// No es un imán: sólo se usa en una SEGUNDA pasada, cuando el dedo no cayó
/// dentro de ningún rectángulo. Si tocaste un árbol de los tres que están juntos,
/// ese árbol gana por rectángulo y el margen no llega ni a mirarse — que es
/// exactamente lo que hay que conservar: el dedo elige, no el algoritmo.
const PICK_FORGIVENESS: f32 = 0.55;

#[derive(Component, Clone, Copy, Debug)]
pub struct InteractTarget {
    pub kind: InteractTargetKind,
    pub id: u32,
}

#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct LookTarget {
    pub entity: Option<Entity>,
    pub kind: Option<InteractTargetKind>,
    pub distance: f32,
    pub hit_point: Vec3,
    pub block_cell: Option<(i32, i32, i32)>,
    pub place_cell: Option<(i32, i32, i32)>,
    /// El bloque apuntado se CAVA (se saca el suelo y queda el pozo), no se pica
    /// de costado. Lo decide el tile: si es suelo pisable y llevás pico, apuntarlo
    /// es cavar hacia abajo — ver `TileWorld::dig_down`.
    pub dig_down: bool,
    /// Tree bookkeeping (chunk cx, 0, wrapped col) — consumed by `FloraHarvest`.
    pub flora_cell: Option<(i32, i32, u32)>,
    /// Debug/autodrive: freeze the target (targeting skips its update).
    pub locked: bool,
}

pub struct TargetingPlugin;

impl Plugin for TargetingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(bevy::prelude::OnExit(crate::game_phase::GamePhase::Playing), (crate::salir::reiniciar::<LookTarget>,));
        app.init_resource::<LookTarget>().add_systems(
            Update,
            update_look_target
                .after(crate::input::InputSet::Pointer)
                .before(crate::input::InputSet::Gesture),
        );
    }
}

/// Decor that a harvest physically removes from the world (and persists as
/// removed): trees, rocks and surface ore/gem deposits. Machines, containers,
/// launch modules etc. have their own lifecycle and are NOT removed this way.
pub fn is_removable_decor(kind: InteractTargetKind) -> bool {
    use InteractTargetKind::*;
    // `Prop` entra acá porque un banco levantado tiene que quedarse levantado:
    // sin `flora_cell` el chunk lo vuelve a sembrar en cuanto te alejás.
    matches!(kind, Tree | Rock | OreIron | OreGold | GemBlue | GemRed | Prop)
}

/// **Los props se dan de baja en su propio casillero.** El set de removidos es
/// `body → (cx, cz) → ids`, y todo el decor vivía en la clave `(0,0)` con el `id`
/// = hash de la celda. Un prop y una veta pueden caer en la MISMA celda —el
/// depósito no mira si hay una mata encima—, así que compartían id: levantar el
/// arbusto se llevaba puesta la veta de hierro que había debajo. `(1,0)` no es
/// una celda, es un espacio de nombres.
pub const PROP_REMOVED_KEY: (i32, i32) = (1, 0);

/// La clave con la que se anota (y se consulta) la baja de un objetivo removible.
pub fn removed_key(kind: InteractTargetKind) -> (i32, i32) {
    if kind == InteractTargetKind::Prop {
        PROP_REMOVED_KEY
    } else {
        (0, 0)
    }
}

/// **Cavar el suelo es picar, con lo que tengas.** Vale cualquier cosa que ya
/// sepa picar un bloque —la mano incluida, como en Minecraft—, y cada una tarda
/// lo suyo: mano 750 ms, hacha 450, pico 350 (`interactions.rs`).
///
/// Antes esto exigía pico, y estaba mal: el suelo está en todas partes pero el
/// jugador no tiene forma de adivinar que la herramienta cambia lo que SIGNIFICA
/// apuntar al piso. El resultado fue "no puedo picar y no veo nada" — la
/// mecánica existía y era invisible. Contra el toque perdido protege el
/// mantenido, que es largo a propósito, no una traba de inventario.
fn can_dig_down(item_id: &str) -> bool {
    // **Con comida o ropa en la mano no se cava.** `resolve_interaction` cae a la regla de la
    // MANO para cualquier ítem sin regla propia, así que una manzana "picaba" el suelo y el
    // mantener nunca llegaba a `comer`/`vestir` (medido en el Redmi el 1 oct 2026: mantener
    // con el gorro en la mano abría un pozo).
    if crate::equipo::es_usable(item_id) {
        return false;
    }
    resolve_interaction(Some(item_id), InteractTargetKind::Block)
        .is_some_and(|r| r.duration_ms > 0)
}

/// **Cuánto acertó el dedo a este sprite.** `Some(0)` = cayó DENTRO del
/// rectángulo (o del radio, si el sprite no declara tamaño); `Some(1)` = entró
/// por el margen de [`PICK_FORGIVENESS`]; `None` = ni cerca.
///
/// El grado es lo que mantiene juntas las dos cosas que el usuario pidió: que el
/// toque siga siendo DIRECTO —de tres árboles pegados gana el que tocaste, porque
/// su grado es 0 y el de los vecinos 1— y que igual no haya que pelear con un
/// prop de un tile, porque cuando no acertaste a ninguno el margen sí decide.
fn grado_de_toque(pick: Vec2, center: Vec2, size: Option<Vec2>) -> Option<u8> {
    let (dentro, cerca) = match size {
        Some(s) => {
            let half = s.abs() * 0.5;
            let dx = (pick.x - center.x).abs();
            let dy = (pick.y - center.y).abs();
            (
                dx <= half.x && dy <= half.y,
                dx <= half.x + PICK_FORGIVENESS && dy <= half.y + PICK_FORGIVENESS,
            )
        }
        None => {
            let d = center.distance(pick);
            (d <= POINT_PICK_RADIUS, d <= POINT_PICK_RADIUS + PICK_FORGIVENESS)
        }
    };
    match (dentro, cerca) {
        (true, _) => Some(0),
        (false, true) => Some(1),
        _ => None,
    }
}

pub(crate) fn target_allowed_in_region(kind: InteractTargetKind, region: RegionId) -> bool {
    match region {
        RegionId::Overworld { .. } => matches!(
            kind,
            InteractTargetKind::LaunchModule
                | InteractTargetKind::LaunchIgnition
                | InteractTargetKind::Tree
                | InteractTargetKind::Rock
                | InteractTargetKind::Container
                | InteractTargetKind::Chest
                | InteractTargetKind::OreIron
                | InteractTargetKind::OreGold
                | InteractTargetKind::GemBlue
                | InteractTargetKind::GemRed
                | InteractTargetKind::Water
                | InteractTargetKind::Block
                | InteractTargetKind::Entity
                | InteractTargetKind::Prop
                | InteractTargetKind::Crop
                | InteractTargetKind::Animal
        ),
        RegionId::Luna | RegionId::Planet(_) | RegionId::Cave(_) => matches!(
            kind,
            InteractTargetKind::LaunchModule
                | InteractTargetKind::LaunchIgnition
                | InteractTargetKind::Rock
                | InteractTargetKind::Container
                | InteractTargetKind::Chest
                | InteractTargetKind::OreIron
                | InteractTargetKind::OreGold
                | InteractTargetKind::GemBlue
                | InteractTargetKind::GemRed
                | InteractTargetKind::Block
                | InteractTargetKind::Prop
                | InteractTargetKind::Crop
                | InteractTargetKind::Animal
        ),
        RegionId::Space => matches!(
            kind,
            InteractTargetKind::SpaceReentry
                | InteractTargetKind::SpaceTransitLuna
                | InteractTargetKind::SpaceTransitBody
                | InteractTargetKind::SpaceStationModule
        ),
        _ => false,
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_look_target(
    cursor: Res<AimCursor>,
    hold: Res<HoldInteractState>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    equipped: Res<EquippedItem>,
    build_mode: Res<crate::input::BuildModeActive>,
    player: Query<(&Transform, &SimState), With<Player>>,
    targets: Query<(
        Entity,
        &GlobalTransform,
        &InteractTarget,
        Option<&Sprite>,
        Option<&crate::harvest::PropArt>,
    )>,
    mut current: ResMut<LookTarget>,
) {
    if current.locked {
        return;
    }
    // Con un arma de fuego en la mano el mundo no se talla ni se cosecha: se apunta y se dispara (`armas.rs`).
    if crate::armas::es_arma(&equipped.item_id) {
        *current = LookTarget::default();
        return;
    }
    // **Una acción en curso CONGELA su objetivo.** El dedo tiembla, y el árbol
    // que empezaste a talar tiene que seguir siendo ése hasta que termines o
    // sueltes (soltar lo cancela desde el gesto). Sin esto, un temblor de dos
    // píxeles a mitad del hachazo lo transfería al arbusto de al lado.
    if hold.active || hold.completed {
        return;
    }
    if !cursor.targeting() {
        *current = LookTarget::default();
        return;
    }
    let Ok((player_tf, sim)) = player.get_single() else {
        *current = LookTarget::default();
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let origin = player_tf.translation.truncate();

    // Free aim cursor: world ground `(x, z)`. `pick` is its screen position.
    let cursor_world = cursor.world;
    let pick = render_xy(cursor_world.x, cursor_world.y);

    // `grado`: 0 = el dedo cayó DENTRO del rectángulo, 1 = entró por el margen de
    // perdón. Ordena antes que la distancia, así una pasada nunca le roba el
    // objetivo a la otra.
    let mut best: Option<(Entity, InteractTargetKind, f32, Vec2, u32, u8)> = None;
    let mut consider =
        |entity: Entity, kind: InteractTargetKind, center: Vec2, id: u32, d: f32, grado: u8| {
            if !target_allowed_in_region(kind, region.active) {
                return;
            }
            let reach = interact_reach_for(kind).max(TILE_REACH);
            if origin.distance(center) > reach {
                return;
            }
            let mejor = best
                .map(|(_, _, bd, _, _, bg)| (grado, d) < (bg, bd))
                .unwrap_or(true);
            if mejor {
                best = Some((entity, kind, d, center, id, grado));
            }
        };

    // Lo tocado: se pica por el RECTÁNGULO del sprite cuando declara tamaño (así
    // tocar la copa agarra el árbol y no el pasto de atrás), y por un radio
    // alrededor del centro cuando no. Entre varios candidatos gana el que tiene
    // el centro más cerca del dedo.
    //
    // Y si el dedo no cayó dentro de NADA, se vuelve a mirar con el margen de
    // [`PICK_FORGIVENESS`] (grado 1). Es la diferencia entre "tocaste al lado del
    // banco, no pasa nada" y "tocaste el banco": lo primero era el 90 % de los
    // intentos sobre un prop de un tile.
    // Con una semilla en la mano, las matas silvestres (pasto, brotes, flores) no
    // son objetivo: el pasto del mundo está sembrado de ellas y bloqueaban el
    // surco. Sembrar encima las desmaleza (`granja::sembrar_al_completar`).
    let sembrando = crate::granja::Planta::de_semilla(&equipped.item_id).is_some();
    // **Con el cubo, manda el líquido (o el suelo donde volcarlo).** La orilla
    // está llena de rocas flotantes, nenúfares y matas que caen sobre el agua, y
    // cualquiera le robaba el toque: el cubo nunca se llenaba (lo cazó
    // `HA_CUBOTEST`). Si la celda bajo el dedo sirve para el cubo, sólo un animal
    // (ordeñar) le gana.
    let celda_para_el_cubo = crate::cubo::es_cubo(&equipped.item_id) && tiles.z == 0 && {
        let (tx, tz) = ((cursor_world.x / TILE_M).floor() as i64, (cursor_world.y / TILE_M).floor() as i64);
        crate::cubo::celda_valida(&equipped.item_id, tiles.tile_at(body, tx, tz))
    };
    for (entity, gtf, target, sprite, prop) in &targets {
        if celda_para_el_cubo && target.kind != InteractTargetKind::Animal {
            continue;
        }
        if sembrando
            && target.kind == InteractTargetKind::Prop
            && prop.is_some_and(|p| crate::granja::es_planta_silvestre(&p.0))
        {
            continue;
        }
        let center = gtf.translation().truncate();
        // El tamaño que SE VE: `custom_size` por la escala del transform. Las crías
        // de la granja se dibujan a media escala con el `custom_size` del adulto, y
        // sin esto el dedo las medía como adultas — tocar la cría agarraba al padre
        // que tenía al lado (lo cazó `HA_GRANJATEST`).
        let escala = gtf.compute_transform().scale.truncate().abs();
        let size = sprite.and_then(|s| s.custom_size).map(|s| s * escala);
        let Some(grado) = grado_de_toque(pick, center, size) else {
            continue;
        };
        // **En modo CONSTRUIR el margen no juega.** Ahí la intención es la CELDA:
        // se está colocando una hilera y cada toque tiene que caer donde apuntó.
        // Un prop vecino que se cuela por el margen no es una ayuda, es una
        // pieza que no se colocó. El rectángulo exacto sí sigue valiendo — así se
        // puede sacar del medio el arbusto que estorba sin salir del modo.
        if grado > 0 && build_mode.active {
            continue;
        }
        // **Sembrando, lo mismo con los cultivos**: la intención es la celda. Sin
        // esto, sembrar la hilera de al lado agarraba por el margen la planta
        // vecina y el surco nuevo no salía (lo cazó `HA_GRANJATEST`).
        if grado > 0 && sembrando && target.kind == InteractTargetKind::Crop {
            continue;
        }
        // El alcance se mide al PIE del sprite, no a su centro: un árbol alto
        // tiene el centro un par de metros "arriba" en pantalla, y medir hasta
        // ahí lo dejaba fuera del brazo estando parado al lado del tronco.
        let foot = match size {
            Some(s) => Vec2::new(center.x, center.y - s.y.abs() * 0.5 + 0.3),
            None => center,
        };
        // **Sembrando manda el suelo.** Con una semilla en la mano lo que se toca
        // es la celda: la copa de un árbol o el techo de un puesto que caen encima
        // del pasto no pueden robarle el toque al surco. Sólo cuenta el PIE del
        // objeto (su último metro), que es lo que de verdad ocupa esa celda.
        if sembrando
            && target.kind != InteractTargetKind::Crop
            && size.is_some_and(|s| s.y.abs() > 1.2)
            && pick.y > center.y - size.map_or(0.0, |s| s.y.abs() * 0.5) + 1.0
        {
            continue;
        }
        consider(entity, target.kind, foot, target.id, center.distance(pick), grado);
    }

    if let Some((entity, kind, distance, hit, id, _)) = best {
        // Harvestable decor (trees, rocks, ore, gems) use a flat per-body removed
        // set at chunk key (0,0); `id` = packed global cell. Setting `flora_cell`
        // is what tells the HUD to REMOVE the prop from the world on harvest.
        let flora_cell = is_removable_decor(kind).then(|| {
            let (kx, kz) = removed_key(kind);
            (kx, kz, id)
        });
        *current = LookTarget {
            entity: Some(entity),
            kind: Some(kind),
            distance,
            hit_point: hit.extend(0.0),
            block_cell: None,
            place_cell: None,
            dig_down: false,
            flora_cell,
            locked: false,
        };
        return;
    }

    // Tile targeting (dig / place) on surface regions only.
    if matches!(region.active, RegionId::Space | RegionId::Transit) {
        *current = LookTarget::default();
        return;
    }

    // Tile under the aim cursor (world ground coords → grid cell).
    let aim = pick;
    let _ = sim;
    if origin.distance(aim) <= TILE_REACH {
        let tx = (cursor_world.x / TILE_M).floor() as i64;
        let tz = (cursor_world.y / TILE_M).floor() as i64;
        // **El cubo apunta a la celda**: al líquido para llenarse, al suelo seco
        // para volcarse. Va antes que todo lo demás porque el agua honda es
        // "sólida" y con cualquier otra herramienta sería un bloque a picar. Tu
        // propia celda no: tocarte con un cubo lleno es beberlo.
        let propia = ((sim.body.x / TILE_M).floor() as i64, (sim.body.z / TILE_M).floor() as i64) == (tx, tz);
        if !propia
            && tiles.z == 0
            && crate::cubo::celda_valida(&equipped.item_id, tiles.tile_at(body, tx, tz))
        {
            *current = LookTarget {
                entity: None,
                kind: None,
                distance: origin.distance(aim),
                hit_point: aim.extend(0.0),
                block_cell: None,
                place_cell: Some((tx as i32, tz as i32, 0)),
                dig_down: false,
                flora_cell: None,
                locked: false,
            };
            return;
        }
        if tiles.solid_at(body, tx, tz) {
            let t = tiles.tile_at(body, tx, tz);
            if t != ha_shared::topdown::Tile::Bedrock
                && !crate::equipo::es_usable(&equipped.item_id)
                && target_allowed_in_region(InteractTargetKind::Block, region.active)
            {
                *current = LookTarget {
                    entity: None,
                    kind: Some(InteractTargetKind::Block),
                    distance: origin.distance(aim),
                    hit_point: aim.extend(0.0),
                    block_cell: Some((tx as i32, tz as i32, 0)),
                    place_cell: None,
                    dig_down: false,
                    flora_cell: None,
                    locked: false,
                };
                return;
            }
        } else if crate::granja::Planta::de_semilla(&equipped.item_id).is_some()
            && !build_mode.active
            && tiles.z == 0
            && crate::granja::suelo_sembrable(tiles.tile_at(body, tx, tz))
            // Tu propia celda no: tocarte a vos mismo con una zanahoria es
            // comerla (el mantener sin objetivo de `resolve_hold`).
            && ((sim.body.x / TILE_M).floor() as i64, (sim.body.z / TILE_M).floor() as i64) != (tx, tz)
        {
            // **Semilla en la mano: el suelo apuntado es un surco posible.** Va
            // antes que cavar porque sembrar es lo que significa tocar el pasto
            // con una semilla — nadie cava con un puñado de trigo. Si ya hay un
            // cultivo ahí, el cultivo es entidad y ganó arriba.
            *current = LookTarget {
                entity: None,
                kind: None,
                distance: origin.distance(aim),
                hit_point: aim.extend(0.0),
                block_cell: None,
                place_cell: Some((tx as i32, tz as i32, 0)),
                dig_down: false,
                flora_cell: None,
                locked: false,
            };
            return;
        } else if can_dig_down(&equipped.item_id)
            && !build_mode.active
            && !ha_shared::is_block_item(&equipped.item_id)
            && tiles.tile_at(body, tx, tz).diggable_down()
            && target_allowed_in_region(InteractTargetKind::Block, region.active)
        {
            // **Cavar hacia abajo** (voxel 2D): el suelo apuntado no es sólido, así
            // que picarlo de costado no significa nada — lo que significa es sacar
            // el piso y abrir el pozo. Pide PICO: con la mano abierta apuntar al
            // pasto seguiría sin ofrecer nada, que es como se sentía el juego hasta
            // ahora, y nadie quiere agujerear el pueblo por rozar la pantalla.
            *current = LookTarget {
                entity: None,
                kind: Some(InteractTargetKind::Block),
                distance: origin.distance(aim),
                hit_point: aim.extend(0.0),
                block_cell: Some((tx as i32, tz as i32, 0)),
                place_cell: None,
                dig_down: true,
                flora_cell: None,
                locked: false,
            };
            return;
        } else if build_mode.active
            || ha_shared::is_block_item(&equipped.item_id)
            || crate::world2d::tile_for_block_item(&equipped.item_id).is_some()
        {
            // Empty cell → placement slot. Block items need a solid neighbour;
            // BUILD mode places anywhere (floors on open ground), any tool.
            let adjacent_solid = tiles.solid_at(body, tx - 1, tz)
                || tiles.solid_at(body, tx + 1, tz)
                || tiles.solid_at(body, tx, tz - 1)
                || tiles.solid_at(body, tx, tz + 1);
            let px = (sim.body.x / TILE_M).floor() as i64;
            let pz = (sim.body.z / TILE_M).floor() as i64;
            let overlaps_player = tx == px && tz == pz;
            if (build_mode.active || adjacent_solid) && !overlaps_player {
                *current = LookTarget {
                    entity: None,
                    kind: None,
                    distance: origin.distance(aim),
                    hit_point: aim.extend(0.0),
                    block_cell: None,
                    place_cell: Some((tx as i32, tz as i32, 0)),
                    dig_down: false,
                    flora_cell: None,
                    locked: false,
                };
                return;
            }
        }
    }

    *current = LookTarget::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un tile de arte con su centro donde lo pone el mundo.
    fn prop(x: f32, y: f32) -> (Vec2, Option<Vec2>) {
        (Vec2::new(x, y), Some(Vec2::splat(1.0)))
    }

    /// **El dedo elige, no el algoritmo.** Tres props pegados: el que tocaste
    /// devuelve grado 0 y los vecinos grado 1, así que el desempate por grado lo
    /// resuelve antes de mirar distancias. Es la garantía que pidió el usuario —
    /// con varios árboles juntos no hay que pelear para agarrar el que querés.
    #[test]
    fn el_rectangulo_tocado_le_gana_al_margen_del_vecino() {
        let (izq, s) = prop(0.0, 0.0);
        let (medio, _) = prop(1.0, 0.0);
        let (der, _) = prop(2.0, 0.0);
        // Dedo justo en el prop del medio. Los dos vecinos caen DENTRO del margen
        // (0,5 + 0,55 = 1,05 m y están a 1 m), así que sin el grado el desempate
        // por distancia los pondría a competir con el que de verdad tocaste.
        let dedo = Vec2::new(1.0, 0.0);
        assert_eq!(grado_de_toque(dedo, medio, s), Some(0));
        assert_eq!(grado_de_toque(dedo, izq, s), Some(1), "el vecino sólo entra por margen");
        assert_eq!(grado_de_toque(dedo, der, s), Some(1), "el vecino sólo entra por margen");
    }

    /// El margen existe: tocar justo al lado de un prop de un tile lo agarra
    /// igual, que es lo que hace que no sea una batalla en el teléfono.
    #[test]
    fn tocar_al_lado_de_un_prop_chico_igual_lo_agarra() {
        let (p, s) = prop(0.0, 0.0);
        // 0,7 m del centro: fuera del medio tile del rectángulo, dentro del margen.
        assert_eq!(grado_de_toque(Vec2::new(0.7, 0.0), p, s), Some(1));
    }

    /// Y no es un imán: más allá del margen no hay objetivo. Sin esto el dedo
    /// engancharía props que no se ven ni cerca del punto tocado.
    #[test]
    fn el_margen_tiene_final() {
        let (p, s) = prop(0.0, 0.0);
        let lejos = 0.5 + PICK_FORGIVENESS + 0.01;
        assert_eq!(grado_de_toque(Vec2::new(lejos, 0.0), p, s), None);
    }

    /// Los props del mundo se sacan y se quedan sacados: sin esto el chunk los
    /// vuelve a sembrar al re-streamear.
    #[test]
    fn un_prop_es_decor_removible() {
        assert!(is_removable_decor(InteractTargetKind::Prop));
    }

    /// Un prop y una veta pueden caer en la MISMA celda, o sea con el mismo `id`.
    /// Si compartieran casillero, levantar el arbusto se llevaría puesta la veta
    /// de hierro que tiene debajo.
    #[test]
    fn el_prop_y_la_veta_no_comparten_casillero() {
        assert_ne!(
            removed_key(InteractTargetKind::Prop),
            removed_key(InteractTargetKind::OreIron),
        );
        assert_eq!(removed_key(InteractTargetKind::Tree), (0, 0));
    }
}
