//! Local player sim @ 20 Hz — top-down 3/4 locomotion on the XZ ground plane.
//! `body.x`/`body.z` are the ground plane; render maps to screen `(x, -z)`.

use bevy::prelude::*;
use ha_shared::topdown::step_topdown_movement;
use ha_shared::{collision_obstacles, generate_world, InputState, WorldContext};

use crate::game_time::configure_game_clock;
use crate::game_time::fixed_delta_secs;
use crate::input::{MovementIntent, TouchActions};
use crate::flight::Flight;
use crate::scene::WorldContextRuntime;
use crate::world2d::{active_surface_body, render_xy, sort_z, ActivePlanet};

/// Legacy anchor kept for compat.
pub const CAVE_INTERIOR_ORIGIN: Vec3 = Vec3::new(0.0, -22.0, 0.0);
pub const JETPACK_ITEM: &str = "jetpack";

#[derive(Component)]
pub struct Player;

/// Ground shadow shown ONLY while flying above visible terrain (creative
/// Climb/Descend — biblia §11-bis) to sell the illusion of altitude. Hidden
/// on foot (unchanged baseline look) and in Orbit (no ground in view to fall
/// on). Synced in `creative_fly.rs::sync_player_shadow`.
#[derive(Component)]
pub struct PlayerFlightShadow;

/// Aura de Ki — CAPA EXTERNA: el derrame naranja translúcido y ancho, el que da
/// volumen. Es un halo radial (`light2d::GlowTexture`), no un cuadro: un
/// `Sprite::from_color` amarillo se leía justo como lo describió el usuario, "un
/// cuadro amarillo cutre", porque un quad de color plano NO tiene borde suave.
///
/// Las tres capas del aura, de afuera hacia adentro:
/// [`PlayerAura`] (naranja ancho) → [`PlayerAuraMid`] (ámbar) →
/// [`PlayerAuraCore`] (dorado casi blanco). Sincronizadas en
/// `creative_fly.rs::sync_player_aura`.
#[derive(Component)]
pub struct PlayerAura;

/// Aura de Ki — capa MEDIA (ámbar): el cuerpo de la llama.
#[derive(Component)]
pub struct PlayerAuraMid;

/// Aura de Ki — NÚCLEO: el punto caliente, dorado tirando a blanco.
#[derive(Component)]
pub struct PlayerAuraCore;

/// Un segmento de la ESTELA de Ki: un halo que se deja atrás al volar y se apaga
/// encogiéndose. Los spawnea/apaga `creative_fly.rs` (`spawn_ki_trail` /
/// `tick_ki_trail`).
#[derive(Component)]
pub struct KiTrail {
    /// Segundos que le quedan de vida.
    pub life: f32,
    /// Vida total, para normalizar el desvanecido.
    pub max_life: f32,
    /// Lado del quad al nacer (m de mundo).
    pub size: f32,
}

/// Sprites que la luz NO debe teñir: los efectos que ya son luz (aura, estela) y
/// la sombra de vuelo. Sin esto, `light2d::light_sprites` les captura el color en
/// `PostUpdate` (`LitBase`), y como el aura se repinta cada frame en `Update` el
/// color capturado queda viejo → el aura late con un frame de retraso y titila.
#[derive(Component)]
pub struct UnlitSprite;

#[derive(Resource, Clone)]
pub struct WorldCollision(pub Vec<ha_shared::types::Obstacle>);

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        let world = generate_world(42);
        app.insert_resource(Time::<Fixed>::from_hz(ha_shared::TICK_RATE as f64))
            .insert_resource(WorldLayout(world.clone()))
            .insert_resource(WorldCollision(collision_obstacles(&world)))
            .add_systems(Startup, configure_game_clock)
            .add_systems(FixedUpdate, tick_sim)
            .add_systems(Update, (interpolate_player_visual, face_player_sprite.in_set(FacePlayerSet)));
    }
}

#[derive(Resource, Clone)]
pub struct WorldLayout(pub ha_shared::GeneratedWorld);

#[derive(Component)]
pub struct SimState {
    pub body: ha_shared::MovementBody,
    pub hp: f32,
    pub max_hp: f32,
    pub survival: ha_shared::SurvivalStats,
    pub prev_action: bool,
    pub prev_jump: bool,
    /// Fracción de daño que absorbe la ropa puesta (`equipo.rs`).
    pub armadura: f32,
}

#[derive(Component, Default)]
pub struct PlayerVisual {
    pub from_x: f32,
    pub from_z: f32,
    pub from_y: f32,
    pub from_rot: f32,
    pub to_x: f32,
    pub to_z: f32,
    pub to_y: f32,
    pub to_rot: f32,
    pub radius: f32,
    /// El jugador está en líquido de poco fondo. Lo escribe `tick_sim` desde el
    /// estado del cuerpo y lo lee `face_player_sprite` para pedir la pose `swim_*`.
    pub swimming: bool,
}

pub(crate) fn tick_sim(
    time: Res<Time<Fixed>>,
    intent: Res<MovementIntent>,
    actions: Res<TouchActions>,
    flight: Res<Flight>,
    world_ctx: Res<WorldContextRuntime>,
    planet_ctx: Res<ActivePlanet>,
    obs: crate::obstaculos::Obstaculos,
    editor: Res<crate::ui_editor::UiEditorState>,
    manejando: Res<crate::vehiculos::Manejando>,
    autos: Query<&crate::vehiculos::Vehiculo>,
    mut query: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    let _p = crate::debug::probe("fx_tick_sim");
    // Al volante, el que se mueve es el vehículo (`vehiculos::fisica_vehiculos`).
    if flight.blocks_walking() || manejando.activo() {
        return;
    }
    let dt = fixed_delta_secs(&time);
    let body_surface = active_surface_body(&world_ctx, &planet_ctx);

    for (mut sim, mut visual) in &mut query {
        visual.from_x = visual.to_x;
        visual.from_z = visual.to_z;
        visual.from_y = visual.to_y;
        visual.from_rot = visual.to_rot;

        sim.prev_action = actions.action;

        let input = InputState {
            seq: 0,
            move_x: intent.move_x,
            move_z: intent.move_z,
            aim_x: intent.aim_x,
            aim_z: intent.aim_z,
            abilities: actions.abilities,
            sprint: if intent.sprint { 1 } else { 0 },
            action: if actions.action { 1 } else { 0 },
        };

        match world_ctx.active {
            WorldContext::Space => {
                // In vacuum the rocket owns movement.
            }
            _ if editor.creative => {
                // Creative: free 8-dir move with no collision (fly over water/rock).
                let no_solid = |_tx: i64, _tz: i64| false;
                step_topdown_movement(&mut sim.body, &input, dt, &no_solid);
            }
            _ => {
                // Solid tiles PLUS decor collision: a tree blocks its own cell so
                // you can't walk through the trunk (keeps the 3/4 illusion), while
                // the canopy stays visual — you pass "behind" it. Per-asset the
                // user can flip this in the editor ("bloquea el paso"), so e.g.
                // rocks/boulders can be made solid too.
                // `walk_blocked` en vez de `solid_at`: un acantilado de un solo
                // escalón es un bordillo que se trepa y se baja, así que dejó de
                // frenar a quien va a pie. Los desniveles altos siguen siendo
                // pared — para ésos hay que buscar la rampa.
                // La CERCA no frena por celda: frena su pie (ver `cerca_frena`).
                let solid = |tx: i64, tz: i64| obs.solido(body_surface, tx, tz);
                // Paredes de BORDE (construcción LDoE): viven en la arista
                // entre celdas, así que van por fuera del grid de sólidos.
                //
                // El DECOR también: su colisión es la huella que pintaste sobre el
                // sprite (hasta ¼ de tile), no la celda entera — por eso se prueba
                // en metros y no en el grid. Un farol delgado frena en su poste y
                // rozar un árbol deja de sentirse un choque fantasma.
                let r = ha_shared::topdown::PLAYER_RADIUS;
                // Los vehículos también frenan a quien camina: un auto es una pared con ruedas.
                let wall_hit = |x: f32, z: f32| {
                    obs.caja_frena(body_surface, x, z, r) || crate::vehiculos::frena_a_pie(autos.iter(), x, z, r)
                };
                // Nadar: el bajío no frena, pero se cruza a media velocidad y con
                // la pose de natación. Sale del TILE (que ya incluye tus ediciones),
                // así que rellenar un charco con bloques deja de mojarte.
                let swim = |tx: i64, tz: i64| {
                    let _p = crate::debug::probe("ob_swim");
                    obs.tiles().tile_at(body_surface, tx, tz).swim()
                };
                // **DESATASCO.** Si YA estás encimado con algo que frena (cargaste
                // una partida vieja y ahí ahora hay un mueble, te teletransportaste
                // encima de una cerca), cualquier paso seguiría encimado y quedabas
                // clavado para siempre. Mientras dure el encime, lo que frena por
                // metros no cuenta — salís caminando y desde ahí todo vuelve a frenar.
                let (x0, z0) = (sim.body.x, sim.body.z);
                let encimado = (input.move_x != 0.0 || input.move_z != 0.0) && wall_hit(x0, z0);
                let nada = |_: f32, _: f32| false;
                ha_shared::topdown::step_topdown_movement_full(
                    &mut sim.body,
                    &input,
                    dt,
                    &solid,
                    if encimado { &nada } else { &wall_hit },
                    &swim,
                );
            }
        }
        sim.body.y = 0.0;

        visual.to_x = sim.body.x;
        visual.to_z = sim.body.z;
        visual.to_y = 0.0;
        visual.swimming = sim.body.state == ha_shared::types::MOV_SWIMMING;
        visual.to_rot = sim.body.rot;
        visual.radius = sim.body.radius;
    }
}

fn interpolate_player_visual(
    time: Res<Time<Fixed>>,
    flight: Res<Flight>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    zoom: Res<crate::camera::CameraZoom>,
    bob: Res<crate::vault::VaultBob>,
    subida: Res<crate::escaleras::Subida>,
    descanso: Res<crate::descanso::Descanso>,
    mut query: Query<(&PlayerVisual, &mut Transform, &mut Visibility), With<Player>>,
) {
    // Volando: el personaje NUNCA cambia de sitio (marco único — `universe.rs`),
    // sólo deja de encoger. Se dibuja con TAMAÑO DE PANTALLA CONSTANTE, así que
    // a cualquier altura seguís viéndote: la sensación de subir la da el mundo
    // que se aleja (el zoom por altura), no el sprite.
    //
    // Que el personaje se volviera un punto sub-píxel al alejarse era la mitad
    // del bug "no veo nada y los controles no responden" — respondían, pero no
    // había forma de saber dónde estabas.
    if fly.flying() {
        let t = time.overstep_fraction() as f32;
        // 1 = igual de grande que pisando el suelo. En vista de mapa se achica
        // un poco (marcador, no protagonista) pero nunca por debajo de ~2/3.
        // Con el encuadre VIVO, no con el objetivo: la proyección lo persigue con
        // un lerp, así que dimensionar el sprite con el objetivo lo hacía crecer
        // antes que el mundo (un pop de tamaño en cada cambio de altura). Y desde
        // que `view_h` es el encuadre del JUGADOR y no el trinquete de la altura,
        // usarlo acá directamente dejaría al personaje del tamaño de a pie —o sea,
        // sub-píxel— en cuanto la altura abriera la cámara.
        //
        // **EL JUGADOR Y LA CÁMARA SUBEN JUNTOS** (28 sep 2026). Pedido textual:
        // *"el jugador y la cámara se alejan por igual; lo que cambiaría sería
        // que si el jugador pellizca para alejar más, no que se aleje solo"*. El
        // tamaño se medía contra `GROUND_VIEW_H` fijo y no contra el encuadre que
        // tenías a pie: con el pellizco acercado (`user_mult` < 1) el personaje
        // se achicaba al despegar hasta que la altura llegaba a 30 m, y encima
        // perdía otro 35 % desde los 200 m. Ahora la escala es la parte de la
        // ALTURA del encuadre (`live / user_mult`) sobre la de a pie: en
        // pantalla mide lo mismo que parado con ese pellizco, a cualquier
        // altura. La altura sólo aleja el SUELO; el pellizco achica todo.
        // Sólo en vista de sistema solar pasa a marcador (un poco más chico).
        let live = zoom.live_view_h;
        let de_altura = live / zoom.user_mult.max(1e-3);
        let screen_f = 1.0 - 0.35 * crate::universe::smootherstep(20_000.0, 200_000.0, de_altura);
        let s = escala_en_vuelo(de_altura, live, screen_f);
        for (visual, mut transform, mut vis) in &mut query {
            *vis = Visibility::Visible;
            let wx = visual.from_x.lerp(visual.to_x, t);
            let wz = visual.from_z.lerp(visual.to_z, t);
            let r = render_xy(wx, wz);
            // Siempre por delante del terreno y de los discos planetarios.
            transform.translation = Vec3::new(r.x, r.y, 900.0);
            transform.rotation = Quat::IDENTITY;
            transform.scale = Vec3::new(s, s, 1.0);
        }
        return;
    }
    let t = time.overstep_fraction() as f32;
    if flight.active && flight.phase != ha_shared::FlightPhase::Surface {
        for (_, _, mut vis) in &mut query {
            *vis = Visibility::Hidden;
        }
        return;
    }

    for (visual, mut transform, mut vis) in &mut query {
        *vis = Visibility::Visible;
        let x = visual.from_x.lerp(visual.to_x, t);
        let z = visual.from_z.lerp(visual.to_z, t);
        let r = render_xy(x, z);
        // Y-sort by the player's own base z, with a TINY epsilon (< one tile
        // step) so it draws above same-cell flat ground art but still sorts
        // fairly against trees/walls: a tree to the SOUTH (larger z) now draws
        // OVER the player — real 3/4 depth, canopies can hide you.
        //
        // `bob.y` despega el DIBUJO del suelo al trepar o descolgarse de un
        // bordillo (`vault.rs`). Va sólo en la posición, nunca en `sort_z`: los
        // pies siguen en su celda, así que izarse por encima de un borde no puede
        // cambiar qué te tapa ni contra qué chocás.
        // `subida.y` es lo mismo para la escalera: mientras andás el tramo el
        // dibujo se eleva con el paso, y el salto que da al cambiar de planta
        // cancela EXACTAMENTE el que da el mundo. Por eso subir se ve continuo
        // sin necesidad de fundir nada (`escaleras.rs`).
        transform.translation =
            Vec3::new(
                r.x,
                r.y + bob.y + subida.y - descanso.bajar_m(),
                // Acostado/sentado en un mueble se ordena como EL MUEBLE (+ un pelo): si no, el
                // respaldo o la cama (cuyo pie está más al sur) te taparían entero.
                sort_z(descanso.orden_z().unwrap_or(z)) + crate::layers::HAIR,
            );
        // SIN espejo por escala. La dirección la dibuja la POSE (`walk_left` ya mira al oeste) y, si una
        // hoja sólo trae un lado, `face_player_sprite` espeja con `flip_x`. Acá había además un
        // `scale.x = -1` al ir al oeste: los dos espejos se cancelaban y el jugador caminaba al oeste
        // MIRANDO AL ESTE (lo cazó `HA_POSETEST` el 3 oct 2026: `idle_left` con escala −1).
        transform.rotation = Quat::IDENTITY;
        transform.scale = Vec3::ONE;
    }
}

/// Cuánto más grande que a pie puede verse el personaje volando (en pantalla), pase lo que pase con el pellizco.
const VUELO_MAX_REL_PANTALLA: f32 = 2.5;

/// **Escala del personaje volando.** La altura lo agranda (`de_altura / GROUND_VIEW_H`) para que se vea a cualquier altura,
/// pero el encuadre real (`live`) lo manda el pellizco: con un pellizco muy cerrado (o dos dedos libres que la pantalla
/// leyó como pellizco) el encuadre queda en 10 m y el personaje —agrandado por la altura— llenaba la pantalla (reporte del
/// 3 oct 2026, celu). Se acota a ×2,5 del tamaño que tiene en pantalla pisando el suelo.
pub(crate) fn escala_en_vuelo(de_altura: f32, live: f32, screen_f: f32) -> f32 {
    let g = crate::universe::GROUND_VIEW_H;
    let por_altura = (de_altura / g * screen_f).max(1.0);
    // tamaño relativo en pantalla = s * g / live  →  s <= REL * live / g
    let tope = (VUELO_MAX_REL_PANTALLA * live / g).max(1.0);
    por_altura.min(tope)
}

/// Donde se elige la pose del jugador: `pose_jugador` decide antes.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FacePlayerSet;

/// Clave del slot de arte del jugador (coincide con `art/player/character.png`
/// hardcodeado en `spawn_player` — el editor escribe sus poses ahí mismo).
const PLAYER_ART_KEY: &str = "player/character";

/// Última pose aplicada (evita reconstruir el atlas/rect todos los frames si
/// no cambió). Vacía en el spawn.
#[derive(Component, Default)]
pub struct CurrentPose(String);


impl CurrentPose {
    /// `"<pose>#<flip>"` de la última pose aplicada (lo leen los drivers de QA).
    pub fn nombre(&self) -> &str {
        &self.0
    }
}

fn pose_direction(to_rot: f32) -> &'static str {
    let (dx, dz) = (to_rot.cos(), to_rot.sin());
    if dz.abs() >= dx.abs() {
        if dz > 0.0 { "down" } else { "up" }
    } else if dx < 0.0 {
        "left"
    } else {
        "right"
    }
}

/// Direcciones candidatas para el rumbo, de la más específica a la más general:
/// la **diagonal** (octante) y después la **cardinal** (cuadrante). Si el atlas
/// trae `walk_down_left` se usa; si sólo trae `walk_left`, cae ahí sola. Así un
/// pack de 8 direcciones se aprovecha entero sin romper los de 4.
fn pose_directions(to_rot: f32) -> [&'static str; 2] {
    let (dx, dz) = (to_rot.cos(), to_rot.sin());
    let card = pose_direction(to_rot);
    // Octante: diagonal sólo si ambas componentes son comparables (±22.5°..67.5°).
    let (ax, az) = (dx.abs(), dz.abs());
    let diagonal = ax > 0.383 && az > 0.383;
    if !diagonal {
        return [card, card];
    }
    let vert = if dz > 0.0 { "down" } else { "up" };
    let horiz = if dx < 0.0 { "left" } else { "right" };
    [
        match (vert, horiz) {
            ("down", "left") => "down_left",
            ("down", _) => "down_right",
            (_, "left") => "up_left",
            _ => "up_right",
        },
        card,
    ]
}

/// Orient the character sprite to the facing direction. Si el slot
/// `player/character` trae **poses** del editor (idle/walk × 4 direcciones),
/// las usa (con mirror left↔right si sólo se sliceó un lado). Si no hay poses
/// definidas, cae al atlas LimeZu de 3 frames hardcodeado (row-0: 0=down,
/// 1=up, 2=side+flip) — comportamiento previo intacto.
pub(crate) fn face_player_sprite(
    art: Res<crate::art2d::ArtLibrary>,
    mut commands: Commands,
    accion: Res<crate::pose_jugador::AccionJugador>,
    reposo: Res<crate::pose_jugador::Reposo>,
    descanso: Res<crate::descanso::Descanso>,
    equipped: Res<crate::input::EquippedItem>,
    mut query: Query<(Entity, &PlayerVisual, &mut Sprite, &mut CurrentPose), With<Player>>,
) {
    let use_poses = art.has_poses(PLAYER_ART_KEY);
    for (entity, visual, mut sprite, mut current) in &mut query {
        if !use_poses {
            if sprite.texture_atlas.is_none() {
                continue;
            }
            let (dx, dz) = (visual.to_rot.cos(), visual.to_rot.sin());
            let (index, flip) = if dz.abs() >= dx.abs() {
                if dz > 0.0 { (0, false) } else { (1, false) }
            } else {
                (2, dx < 0.0)
            };
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index = index;
            }
            sprite.flip_x = flip;
            continue;
        }

        let moving = (visual.to_x - visual.from_x).abs() > 1e-4 || (visual.to_z - visual.from_z).abs() > 1e-4;
        // QUÉ hace lo decide `pose_jugador` (acción > caminar > arma > reposo; el agua manda sobre
        // todo). Acá sólo se le pone la dirección. Una acción con blanco (talar, pegarle a un
        // infectado) mira al blanco aunque el último paso haya sido hacia otro lado.
        let en_curso = accion.actual();
        let verb = crate::pose_jugador::verbo_del_cuerpo(
            visual.swimming,
            moving,
            en_curso.map(|(v, _)| v),
            crate::armas::es_arma(&equipped.item_id),
            reposo.pose(),
            descanso.pose(),
        );
        let rumbo = en_curso.and_then(|(_, r)| r).unwrap_or(visual.to_rot);
        let [diag, card] = pose_directions(rumbo);

        // Orden de preferencia: diagonal → cardinal → mismo verbo espejado →
        // el mismo verbo de frente (las filas de UNA dirección del pack: celular,
        // leer, dormir) → idle en esa dirección → idle_down. Nunca deja al
        // personaje sin sprite.
        let mut flip = false;
        let mut wanted = format!("{verb}_{diag}");
        if !art.has_pose(PLAYER_ART_KEY, &wanted) {
            wanted = format!("{verb}_{card}");
        }
        if !art.has_pose(PLAYER_ART_KEY, &wanted) {
            // Sólo se sliceó un lado (ej. "right") — espejar para el opuesto.
            let mirror = match card {
                "left" => Some("right"),
                "right" => Some("left"),
                _ => None,
            };
            if let Some(m) = mirror {
                let alt = format!("{verb}_{m}");
                if art.has_pose(PLAYER_ART_KEY, &alt) {
                    wanted = alt;
                    flip = true;
                }
            }
        }
        if !art.has_pose(PLAYER_ART_KEY, &wanted) {
            wanted = format!("{verb}_down");
        }
        if !art.has_pose(PLAYER_ART_KEY, &wanted) {
            wanted = format!("idle_{card}");
        }
        if !art.has_pose(PLAYER_ART_KEY, &wanted) {
            wanted = "idle_down".to_string();
        }

        let state_key = format!("{wanted}#{flip}");
        if current.0 == state_key {
            continue;
        }
        current.0 = state_key;
        sprite.flip_x = flip;
        // Nadando, la pose es el cuerpo cortado en la cintura (`art2d`: celda de 32x40): el quad se
        // achica en la misma proporción para no estirarlo, y el personaje queda hundido hasta la
        // cintura con las ondas (`pose_jugador::OndasNado`) en el borde del corte.
        let t = crate::scene::TAMANO_JUGADOR;
        sprite.custom_size = Some(if wanted.starts_with("swim") {
            Vec2::new(t.x, t.y * crate::pose_jugador::NADO_ALTO_PX as f32 / 64.0)
        } else {
            t
        });

        // Personaje armado con VARIAS hojas (un PNG por dirección/estado): la
        // pose trae su propia imagen. Con una sola hoja esto devuelve la misma
        // y no cambia nada.
        if let Some(img) = art.pose_image(PLAYER_ART_KEY, &wanted) {
            if sprite.image != img {
                sprite.image = img;
                sprite.texture_atlas = None; // el layout se rehace para la hoja nueva
            }
        }
        if let Some(([x, y, w, h], anim)) = art.pose(PLAYER_ART_KEY, &wanted) {
            match anim {
                Some(anim) => {
                    sprite.rect = None;
                    crate::render::try_insert(&mut commands, entity, "pose/sim", anim);
                }
                None => {
                    commands.entity(entity).remove::<crate::art2d::ArtAnimated>();
                    sprite.texture_atlas = None;
                    sprite.rect = Some(Rect::from_corners(
                        Vec2::new(x as f32, y as f32),
                        Vec2::new((x + w) as f32, (y + h) as f32),
                    ));
                }
            }
        }
    }
}

/// Compat shim: top-down ground is flat (y=0).
pub fn earth_surface_y(_tx: f32, _tz: f32) -> f32 {
    0.0
}


/// **La cerca frena donde se dibuja**, no en toda su celda: el murete en la mitad
/// sur, el tramo norte-sur en su franja (`ha_shared::cercas::pie_de_cerca`).
pub(crate) fn cerca_frena(es_cerca: &dyn Fn(i64, i64) -> bool, caja: [f32; 4]) -> bool {
    let (x0, x1) = (caja[0].floor() as i64, caja[2].floor() as i64);
    let (z0, z1) = (caja[1].floor() as i64, caja[3].floor() as i64);
    for tz in z0..=z1 {
        for tx in x0..=x1 {
            if !es_cerca(tx, tz) {
                continue;
            }
            let v = ha_shared::cercas::Vecinos::new(
                es_cerca(tx, tz - 1),
                es_cerca(tx + 1, tz),
                es_cerca(tx, tz + 1),
                es_cerca(tx - 1, tz),
            );
            for p in ha_shared::cercas::pie_de_cerca(v) {
                let r = [tx as f32 + p[0], tz as f32 + p[1], tx as f32 + p[2], tz as f32 + p[3]];
                if r[0] < caja[2] && caja[0] < r[2] && r[1] < caja[3] && caja[1] < r[3] {
                    return true;
                }
            }
        }
    }
    false
}


#[cfg(test)]
mod tests_vuelo {
    use super::*;

    #[test]
    fn el_personaje_volando_nunca_llena_la_pantalla() {
        let g = crate::universe::GROUND_VIEW_H;
        // Pellizco normal (mult 1): crece con la altura, igual que antes.
        assert!((escala_en_vuelo(g * 4.0, g * 4.0, 1.0) - 4.0).abs() < 1e-3);
        // Pellizco cerrado al máximo a 135 m de "altura de encuadre": el encuadre queda en 13 m.
        let s = escala_en_vuelo(g * 4.5, 13.0, 1.0);
        assert!(s * g / 13.0 <= VUELO_MAX_REL_PANTALLA + 1e-3, "tamaño relativo {}", s * g / 13.0);
        // Y jamás por debajo de 1.
        assert!(escala_en_vuelo(1.0, 1.0, 1.0) >= 1.0);
    }
}
