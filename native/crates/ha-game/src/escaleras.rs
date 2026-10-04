//! **Subir y bajar caminando** — el reemplazo de la grada-teletransporte.
//!
//! El sistema viejo se borró entero el 4 ago 2026 (biblia §14): un tile con el
//! poder de cambiar `tiles.z`, un pestillo para que no rebotara, una sala 7×7
//! tallada al llegar y un pozo que se trepaba de una. Nada de eso vuelve. Lo que
//! hay ahora son dos gestos, y los dos son ANDAR:
//!
//! - **El tramo** ([`ha_shared::escaleras`]): cuatro escalones en fila que unen
//!   dos plantas. Se sube andándolo, el personaje se eleva con el paso y la
//!   planta cambia al pisar el último escalón. Como no hay pestillo ni evento,
//!   desandarlo a mitad te devuelve — bajar es el mismo número al revés.
//! - **Trepar el borde** ([`trepar_borde`]): del pozo que cavaste se sale
//!   empujando contra la pared, como de un bordillo. Antes el fondo del pozo
//!   estampaba una grada mágica que subía el hueco entero de golpe; ahora se sale
//!   por donde se entró, un nivel por vez.
//!
//! ## Lo que hace que no se lea como un corte
//!
//! El cambio de planta es discreto —es un índice, `tiles.z`— pero cae en el frame
//! en el que el dibujo del personaje da el salto contrario y de la misma
//! magnitud ([`ha_shared::escaleras::elevacion`]). Los dos se anulan: en pantalla
//! el personaje sube liso y lo que cambia es el mundo alrededor. **No hay
//! fundido**, porque no hay nada que tapar; poner uno sería teatralizar un cambio
//! de escena que no ocurre (§4).
//!
//! Las celdas del tramo existen en las DOS plantas —`Step` abajo, `Stairwell`
//! arriba— así que cambiar de planta jamás mueve al jugador: no hay a dónde
//! teletransportarlo.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use ha_shared::escaleras::{self, Tramo, PASOS};
use ha_shared::topdown::{Tile, TILE_M};
use ha_shared::CelestialBody;

use crate::inventory_persist::persist_dir;
use crate::platform::is_app_paused;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::world2d::{
    active_surface_body, render_xy, surface_region_active, ActivePlanet, TileWorld,
};

/// **Trepar se PIDE.** Antes bastaba con empujar contra la pared 0.45 s y el
/// juego te izaba solo, y eso estaba mal por dos motivos, los dos reportados:
///
/// - Dentro de un pozo estás rodeado de pared por los cuatro lados, así que
///   caminar en el fondo del hueco disparaba la trepada sola y te devolvía al
///   overworld sin haberlo pedido. El pozo dejaba de ser un pozo.
/// - Y aunque no te sacara de sitio, "me apego a un borde y sube solo" es el
///   juego decidiendo por vos. Lo que se decide se pulsa.
///
/// Ahora el botón de SALTAR pasa a decir TREPAR cuando hay borde delante
/// ([`Trepada::hacia`]) y la trepada ocurre en el flanco de subida de ese botón.
/// Alejarse del borde devuelve el botón a SALTAR: el texto ES el detector.
#[derive(Resource, Default)]
pub struct Trepada {
    /// La celda del nivel de arriba a la que se puede trepar **ahora**, si la
    /// hay. Es lo que mira el botón para cambiar de texto.
    pub hacia: Option<(i64, i64)>,
    /// Estado del botón el tick anterior, para detectar el flanco. Se mira acá y
    /// no en `TouchActions::jump_edge` a propósito: ese bit lo escriben cuatro
    /// sistemas repartidos por `Update` (táctil, teclado, gamepad, drivers de QA)
    /// y este sistema corre en `FixedUpdate`, que en Bevy va ANTES — leerlo daría
    /// el flanco del frame pasado o ninguno. El sostenido `jump` viene con un
    /// búfer de 0.16 s, así que un toque siempre pilla algún tick fijo.
    jump_previo: bool,
}

/// Un tramo guardado en disco.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct TramoGuardado {
    pub body: u8,
    pub z: i32,
    pub tx: i64,
    pub tz: i64,
    pub dir: u8,
    pub pasos: u8,
}

impl From<TramoGuardado> for Tramo {
    fn from(g: TramoGuardado) -> Self {
        Tramo { body: g.body, z: g.z, tx: g.tx, tz: g.tz, dir: g.dir, pasos: g.pasos }
    }
}

impl From<Tramo> for TramoGuardado {
    fn from(t: Tramo) -> Self {
        TramoGuardado { body: t.body, z: t.z, tx: t.tx, tz: t.tz, dir: t.dir, pasos: t.pasos }
    }
}

/// **Los tramos que existen.** Los construidos por el jugador se guardan acá; los
/// de las cuevas los pone el generador y no ocupan memoria (ver
/// [`Escaleras::en`]).
///
/// Vive fuera del `TileWorld` por la misma razón que las paredes de borde
/// (`walls.rs`): un tile no tiene dónde guardar hacia qué lado sube, y adivinarlo
/// mirando los vecinos es exactamente la ambigüedad que hace que una escalera
/// lleve al sitio equivocado.
#[derive(Resource, Default)]
pub struct Escaleras {
    list: Vec<Tramo>,
    /// `(cuerpo, nivel bajo, celda)` → índice en `list`. La consulta corre una vez
    /// por frame por el jugador; recorrer el vector entero no escala a una casa de
    /// varias plantas.
    por_celda: HashMap<(u8, i32, i64, i64), usize>,
    dirty: bool,
    was_paused: bool,
    debounce: f32,
}

impl Escaleras {
    pub fn list(&self) -> &[Tramo] {
        &self.list
    }

    fn reindexar(&mut self) {
        self.por_celda.clear();
        for (i, t) in self.list.iter().enumerate() {
            for (cx, cz, _) in t.celdas() {
                self.por_celda.insert((t.body, t.z, cx, cz), i);
            }
        }
    }

    /// **El tramo cuyo nivel BAJO es `z` y que pasa por esta celda**, sea
    /// construido o natural.
    ///
    /// Se consulta primero lo construido y después el generador, igual que
    /// `TileWorld::tile_at_level` mira las ediciones antes que el terreno: lo que
    /// el jugador puso manda sobre lo que el mundo trae.
    pub fn en(&self, body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<Tramo> {
        if let Some(&i) = self.por_celda.get(&(body as u8, z, tx, tz)) {
            return Some(self.list[i]);
        }
        // Los interiores de los edificios traen sus escaleras (`interiores`); luego las cuevas naturales.
        if ha_shared::interiores::es_interior(z) {
            return ha_shared::interiores::tramo_en(body, z, tx, tz);
        }
        ha_shared::cuevas::tramo_natural(body, z, tx, tz)
    }

    /// ¿Alguna celda del tramo ya está ocupada por otro tramo? Dos escaleras
    /// cruzadas no tienen lectura posible.
    pub fn choca(&self, t: &Tramo) -> bool {
        t.celdas()
            .any(|(cx, cz, _)| self.por_celda.contains_key(&(t.body, t.z, cx, cz)))
    }

    pub fn add(&mut self, t: Tramo) {
        let i = self.list.len();
        self.list.push(t);
        for (cx, cz, _) in t.celdas() {
            self.por_celda.insert((t.body, t.z, cx, cz), i);
        }
        self.dirty = true;
    }
}

/// **Elevación del dibujo del jugador**, en metros sobre sus pies. La suma
/// `sim::interpolate_player_visual`, igual que el impulso de trepar un bordillo
/// (`vault::VaultBob`): los pies siguen en su celda y sólo el dibujo se despega
/// del suelo, así que subir no puede cambiar contra qué chocás.
#[derive(Resource, Default)]
pub struct Subida {
    pub y: f32,
    /// **El tramo que estás andando y cuánto llevás andado** (0 = pie, 1 = cima).
    ///
    /// Lo publica `caminar_tramo` para que la LUZ pueda acompañar el paso
    /// (`interior::medir_encierro`). Es lo que convierte el cambio de planta en un
    /// tránsito y no en un corte: mientras bajás, la escalera se va ensombreciendo
    /// y el overworld se apaga con ella; mientras subís, se abre. Sin este dato la
    /// luz sólo podría reaccionar DESPUÉS del cambio, que es justo lo que se leía
    /// como teletransporte.
    pub tramo: Option<(Tramo, f32)>,
    /// Último escalón pisado, para sonar un paso por escalón y no por frame.
    ultimo_paso: Option<u8>,
}

pub struct EscalerasPlugin;

impl Plugin for EscalerasPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Escaleras>()
            .init_resource::<Subida>()
            .init_resource::<Trepada>()
            .add_systems(OnEnter(crate::game_phase::GamePhase::Loading), cargar)
            .add_systems(Update, dibujar_escalones.run_if(crate::game_phase::world_active))
            .add_systems(
                Update,
                (caminar_tramo, guardar).run_if(crate::game_phase::world_active),
            )
            // **Trepar corre justo después del paso de la simulación.**
            //
            // Y lee lo que la simulación DEJÓ (`body.rot`), no la intención de
            // movimiento cruda. La primera versión leía `MovementIntent`, y eso no
            // funciona desde ningún sitio: ese recurso se escribe a lo largo de
            // `Update` (joystick, gamepad, teclado, drivers de QA) y lo suaviza un
            // lerp hacia el reposo, así que un lector de `Update` lo pilla a medio
            // escribir y uno de `FixedUpdate` —que en Bevy corre ANTES— lee el del
            // frame anterior, ya devuelto a cero.
            //
            // El rumbo del cuerpo no tiene ese problema: es la salida del mismo
            // paso que decide hacia dónde mirás, y ya trae resuelto el choque
            // contra la pared. El botón entra por el sostenido `TouchActions::jump`
            // —con búfer de 0.16 s— y no por su flanco, por la misma razón de
            // orden: ver [`Trepada::jump_previo`].
            .add_systems(
                FixedUpdate,
                trepar_borde
                    .after(crate::sim::tick_sim)
                    .run_if(crate::game_phase::world_active),
            );
    }
}

// ── Andar el tramo ───────────────────────────────────────────────────────────

/// A qué nivel pasa el jugador según dónde va del tramo `t_z` (`s`, 0 = pie, 1 = cima).
/// Puro, para poder probarlo: subir sólo si hay suelo arriba; bajar siempre.
pub(crate) fn nivel_nuevo(actual: i32, t_z: i32, s: f32, hay_suelo_arriba: bool) -> Option<i32> {
    if actual == t_z && s >= escaleras::CIMA && hay_suelo_arriba {
        Some(t_z + 1)
    } else if actual == t_z + 1 && s <= escaleras::PIE {
        Some(t_z)
    } else {
        None
    }
}

/// **El sistema entero de subir y bajar**, y es una función de dónde estás
/// parado: sin eventos, sin pestillo y sin estado que se pueda desincronizar.
///
/// Cada frame: ¿mi celda es un escalón? Si sí, ¿cuánto llevo andado del tramo? Si
/// pasé de [`escaleras::CIMA`] estoy arriba, si bajé de [`escaleras::PIE`] estoy
/// abajo, y en el medio no pasa nada. Eso es todo.
pub(crate) fn caminar_tramo(
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    escaleras: Res<Escaleras>,
    time: Res<Time>,
    mut tiles: ResMut<TileWorld>,
    mut subida: ResMut<Subida>,
    mut fade: ResMut<crate::world2d::LayerFade>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
    player: Query<&SimState, With<Player>>,
) {
    let fuera = surface_region_active(&region).is_none();
    let Ok(sim) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let tx = (sim.body.x / TILE_M).floor() as i64;
    let tz = (sim.body.z / TILE_M).floor() as i64;

    // Dos consultas y no una: el tramo que sube DESDE mi planta, y el que baja
    // hacia ella (o sea, el que tiene mi planta como cima). Son el mismo objeto
    // mirado desde sus dos puntas.
    let tramo = (!fuera)
        .then(|| {
            escaleras
                .en(body, tiles.z, tx, tz)
                .or_else(|| escaleras.en(body, tiles.z - 1, tx, tz))
        })
        .flatten();

    let Some(t) = tramo else {
        // Fuera del tramo el dibujo vuelve al suelo. Con un lerp y no de golpe:
        // salirse de una escalera por el costado no puede dar un tirón vertical.
        subida.y += (0.0 - subida.y) * (time.delta_secs() * 14.0).min(1.0);
        subida.ultimo_paso = None;
        subida.tramo = None;
        // Fuera del tramo no hay nada desvaneciéndose. Se pone en cero y no se
        // deja decaer: el fundido del tramo es una FUNCIÓN de dónde estás, no un
        // temporizador, así que salirse de la escalera por el costado devuelve el
        // dibujo entero en el acto (ver `LayerFade::tramo`).
        fade.tramo = 0.0;
        return;
    };

    let s = escaleras::progreso(&t, sim.body.x, sim.body.z);
    subida.tramo = Some((t, s));
    // **La planta se desvanece con el paso.** Ésta es la mitad "dibujo" de la
    // transición; la otra mitad es la luz (`interior::medir_encierro`). Las dos
    // salen de la MISMA curva, así que el punto en que la planta está más borrada
    // es el mismo en que está más oscura, y es el mismo en que `tiles.z` cambia.
    fade.tramo = crate::world2d::FUNDIDO_TRAMO * escaleras::meseta_del_tramo(s);
    // El compromiso. Es lo único que toca `tiles.z` en todo el módulo.
    // **Subir exige suelo arriba.** Una escalera sin rellano (datos viejos, o una obra a medias)
    // te subía a un nivel donde la celda es aire; `fall_through_void` te devolvía abajo y el
    // tramo te volvía a subir: un entra-y-sale a metralla, cuadro tras cuadro.
    let hay_suelo_arriba = {
        let arriba = tiles.tile_at_level(body, t.z + 1, tx, tz);
        arriba != Tile::Air && !arriba.solid()
    };
    let nuevo = nivel_nuevo(tiles.z, t.z, s, hay_suelo_arriba);
    if let Some(z) = nuevo {
        tiles.z = z;
        tiles.version = tiles.version.wrapping_add(1);
        // Nada que disparar acá: el fundido del tramo ya está en su máximo en este
        // instante (la meseta cubre `CIMA` y `PIE`), así que la planta que se va ya
        // estaba casi borrada y la que llega entra por la misma curva al revés. El
        // pulso de 0.28 s queda para la CAÍDA, que sí es instantánea.
    }
    subida.y = escaleras::elevacion(&t, s, tiles.z);

    // Un paso suena por escalón, donde está el pie — no por frame ni en la cabeza
    // del jugador.
    if let Some(paso) = t.paso_en(tx, tz) {
        if subida.ultimo_paso != Some(paso) {
            subida.ultimo_paso = Some(paso);
            sfx.send(crate::audio::PlaySfxAt(
                crate::audio::Sfx::Stairs,
                render_xy(sim.body.x, sim.body.z),
            ));
        }
    }
}

// ── Trepar el borde ──────────────────────────────────────────────────────────

/// **Del pozo se sale trepando, y trepar se PULSA.** Mirá la pared del pozo: el
/// botón pasa a decir TREPAR. Pulsalo y te izás al nivel de arriba, a la celda
/// que estabas mirando.
///
/// Reemplaza a la grada que el sistema viejo estampaba sola en el fondo de cada
/// pozo (y que al pisarla subía el hueco ENTERO de una). Acá se sube de a un
/// nivel, por donde vos elegiste, y el movimiento —una celda— es el mismo que
/// izarse por un bordillo: no es un teletransporte, es la trepada que ya existía
/// aplicada a la verticalidad.
///
/// El borde se busca por hacia **dónde mirás** y no por hacia dónde empujás, que
/// es lo que hacía la versión de "empujar 0.45 s". Mirando alcanza porque ahora
/// hay un botón: quieto contra la pared el juego ya no hace nada solo, y el
/// jugador puede quedarse en el fondo del pozo todo lo que quiera. Además hace
/// que el texto del botón se encienda al ACERCARTE, que es cuando hay que
/// leerlo, y se apague al darte vuelta.
///
/// Sólo bajo tierra y sólo hacia una celda **pisable**: trepar hacia dentro de la
/// roca te dejaría enterrado, que es el soft-lock al revés.
#[allow(clippy::too_many_arguments)]
fn trepar_borde(
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    acciones: Res<crate::input::TouchActions>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    escaleras: Res<Escaleras>,
    mut tiles: ResMut<TileWorld>,
    mut trepada: ResMut<Trepada>,
    mut bob: ResMut<crate::vault::VaultBob>,
    mut sfx: EventWriter<crate::audio::PlaySfxAt>,
    mut player: Query<&mut SimState, With<Player>>,
) {
    let _p = crate::debug::probe("fx_trepar_borde");
    // El flanco se calcula SIEMPRE, incluso en los caminos que salen temprano:
    // si no, soltar el botón fuera de un borde dejaría `jump_previo` en `true` y
    // el toque siguiente no contaría como flanco.
    let pulsado = acciones.jump && !trepada.jump_previo;
    trepada.jump_previo = acciones.jump;

    if fly.flying() || tiles.z >= 0 || surface_region_active(&region).is_none() {
        trepada.hacia = None;
        return;
    }
    let Ok(mut sim) = player.get_single_mut() else {
        trepada.hacia = None;
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let tx = (sim.body.x / TILE_M).floor() as i64;
    let tz = (sim.body.z / TILE_M).floor() as i64;

    // En una escalera no se trepa: ya tenés por dónde subir, y las dos cosas a la
    // vez se pelearían por `tiles.z`.
    if escaleras.en(body, tiles.z, tx, tz).is_some()
        || escaleras.en(body, tiles.z - 1, tx, tz).is_some()
    {
        trepada.hacia = None;
        return;
    }

    // Hacia dónde mira: la componente dominante de su rumbo. Nada de sumar las
    // dos componentes, que daría diagonales imposibles de encajar en una celda.
    let (mx, mz) = (sim.body.rot.cos(), sim.body.rot.sin());
    let (dx, dz) = if mx.abs() >= mz.abs() {
        (mx.signum() as i64, 0)
    } else {
        (0, mz.signum() as i64)
    };
    let (cx, cz) = (tx + dx, tz + dz);
    // La condición: acá me frena, y justo arriba de ese mismo sitio hay suelo.
    let frena = tiles.walk_blocked(body, cx, cz);
    let arriba = tiles.tile_at_level(body, tiles.z + 1, cx, cz);
    let pisable = !arriba.solid() && !arriba.swim() && arriba != Tile::Air;
    if !frena || !pisable {
        trepada.hacia = None;
        return;
    }
    trepada.hacia = Some((cx, cz));
    if !pulsado {
        return;
    }
    trepada.hacia = None;
    tiles.z += 1;
    tiles.version = tiles.version.wrapping_add(1);
    sim.body.x = (cx as f32 + 0.5) * TILE_M;
    sim.body.z = (cz as f32 + 0.5) * TILE_M;
    sim.body.vx = 0.0;
    sim.body.vz = 0.0;
    bob.climb_impact();
    sfx.send(crate::audio::PlaySfxAt(
        crate::audio::Sfx::Stairs,
        render_xy(sim.body.x, sim.body.z),
    ));
}

// ── Construir un tramo ───────────────────────────────────────────────────────

/// **Coloca un tramo entero**, con su rellano, o no coloca nada.
///
/// Devuelve `true` si entró. Todas las comprobaciones son previas: una escalera a
/// medio construir —sin rellano arriba, o cruzada con otra— es peor que ninguna,
/// porque te deja a mitad de camino sin manera de seguir ni de volver.
pub fn construir_tramo(
    escaleras: &mut Escaleras,
    tiles: &mut TileWorld,
    body: CelestialBody,
    tx: i64,
    tz: i64,
    dir: u8,
) -> bool {
    let t = Tramo { body: body as u8, z: tiles.z, tx, tz, dir, pasos: PASOS };
    if escaleras.choca(&t) {
        return false;
    }
    // Las celdas del tramo tienen que estar libres en LAS DOS plantas: abajo por
    // donde se sube, y arriba porque ahí va el hueco de la escalera.
    for (cx, cz, _) in t.celdas() {
        if tiles.tile_at_level(body, t.z, cx, cz).solid()
            || tiles.tile_at_level(body, t.z + 1, cx, cz).solid()
        {
            return false;
        }
    }
    for (cx, cz, _) in t.celdas() {
        tiles.author_tile(body, t.z, cx, cz, Tile::Step);
        tiles.author_tile(body, t.z + 1, cx, cz, Tile::Stairwell);
    }
    // **El rellano va con la escalera.** Sin él la cima da al vacío y el primer
    // paso arriba te devuelve abajo de una caída: la escalera "funcionaría" y aun
    // así no llevaría a ningún lado.
    let (rx, rz) = t.rellano();
    if !ha_shared::build_grid::is_build_floor(tiles.tile_at_level(body, t.z + 1, rx, rz)) {
        tiles.author_tile(body, t.z + 1, rx, rz, Tile::WoodFloor);
    }
    escaleras.add(t);
    true
}

// ── Persistencia ─────────────────────────────────────────────────────────────

fn escaleras_path() -> std::path::PathBuf {
    persist_dir().join("escaleras.json")
}

fn cargar(mut escaleras: ResMut<Escaleras>) {
    // **Cada partida arranca con lo SUYO.** Esto corría en `Startup`, antes de
    // elegir partida: leía la carpeta base y lo de las pruebas viejas aparecía en
    // cualquier partida nueva. Ahora corre al entrar a la partida (`Loading`) y
    // parte de cero; ver `partida.rs`.
    *escaleras = Escaleras::default();
    escaleras.reindexar();
    let Ok(raw) = std::fs::read_to_string(escaleras_path()) else {
        return;
    };
    if let Ok(list) = serde_json::from_str::<Vec<TramoGuardado>>(&raw) {
        eprintln!("ha-escalera: {} tramos cargados", list.len());
        escaleras.list = list.into_iter().map(Tramo::from).collect();
        escaleras.reindexar();
    }
}

/// Mismo debounce que antorchas y estructuras: construir una escalera no puede
/// ser una escritura a disco en mitad de un frame del Redmi.
fn guardar(time: Res<Time>, mut escaleras: ResMut<Escaleras>) {
    let paused = is_app_paused();
    let just_paused = paused && !escaleras.was_paused;
    escaleras.was_paused = paused;
    if !escaleras.dirty {
        return;
    }
    escaleras.debounce += time.delta_secs();
    if !just_paused && escaleras.debounce < 2.0 {
        return;
    }
    escaleras.debounce = 0.0;
    escaleras.dirty = false;
    let path = escaleras_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let guardados: Vec<TramoGuardado> =
        escaleras.list.iter().copied().map(TramoGuardado::from).collect();
    if let Ok(json) = serde_json::to_string(&guardados) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    /// Una escalera sin rellano no sube (el entra-y-sale a metralla del 2 oct 2026), y siempre baja.
    #[test]
    fn sin_suelo_arriba_no_se_sube_pero_siempre_se_baja() {
        use super::*;
        assert_eq!(nivel_nuevo(0, 0, 1.0, true), Some(1));
        assert_eq!(nivel_nuevo(0, 0, 1.0, false), None, "sin rellano no se sube");
        assert_eq!(nivel_nuevo(1, 0, 0.0, false), Some(0), "bajar no pide nada");
        assert_eq!(nivel_nuevo(0, 0, 0.5, true), None, "a mitad de tramo no pasa nada");
    }
    use super::*;

    fn mundo() -> (Escaleras, TileWorld, CelestialBody) {
        (Escaleras::default(), TileWorld::default(), CelestialBody::Earth)
    }

    /// Un tramo construido existe en las DOS plantas. Es lo que garantiza que
    /// cambiar de planta no mueva al jugador: la celda donde está ya es pisable
    /// arriba y abajo, así que no hay a dónde teletransportarlo.
    #[test]
    fn el_tramo_deja_celdas_pisables_en_las_dos_plantas() {
        let (mut esc, mut tiles, body) = mundo();
        assert!(construir_tramo(&mut esc, &mut tiles, body, 40, 40, 1));
        for i in 0..PASOS as i64 {
            let abajo = tiles.tile_at_level(body, 0, 40 + i, 40);
            let arriba = tiles.tile_at_level(body, 1, 40 + i, 40);
            assert_eq!(abajo, Tile::Step, "escalón {i}");
            assert_eq!(arriba, Tile::Stairwell, "hueco {i}");
            assert!(!abajo.solid() && !arriba.solid(), "el tramo se camina en las dos");
        }
    }

    /// Y llega a algún sitio: la cima da a un rellano de verdad, no al vacío.
    #[test]
    fn el_tramo_trae_su_rellano() {
        let (mut esc, mut tiles, body) = mundo();
        construir_tramo(&mut esc, &mut tiles, body, 40, 40, 1);
        let t = esc.list()[0];
        let (rx, rz) = t.rellano();
        let rellano = tiles.tile_at_level(body, t.z + 1, rx, rz);
        assert!(
            ha_shared::build_grid::is_build_floor(rellano),
            "la cima da al vacío: {rellano:?}"
        );
    }

    /// Dos escaleras cruzadas no tienen lectura posible, así que la segunda no
    /// entra — y no deja medio tramo estampado al fallar.
    #[test]
    fn no_se_cruzan_dos_tramos() {
        let (mut esc, mut tiles, body) = mundo();
        assert!(construir_tramo(&mut esc, &mut tiles, body, 40, 40, 1));
        assert!(
            !construir_tramo(&mut esc, &mut tiles, body, 42, 38, 2),
            "el segundo tramo cruza al primero"
        );
        assert_eq!(esc.list().len(), 1);
    }

    /// El tramo se encuentra desde sus DOS plantas: subiendo se lo busca en el
    /// nivel bajo, bajando en el alto. Si sólo se encontrara desde abajo, la
    /// escalera sería de ida.
    #[test]
    fn el_tramo_se_encuentra_desde_arriba_y_desde_abajo() {
        let (mut esc, mut tiles, body) = mundo();
        construir_tramo(&mut esc, &mut tiles, body, 40, 40, 1);
        assert!(esc.en(body, 0, 41, 40).is_some(), "desde la planta baja");
        assert!(esc.en(body, 0, 41, 40).is_some_and(|t| t.z == 0));
        // Bajando: quien está en la planta 1 consulta el nivel `z - 1`.
        assert!(esc.en(body, 1 - 1, 41, 40).is_some(), "desde la planta alta");
    }

    /// La invariante que reemplaza al pestillo: andar el tramo entero cambia de
    /// planta UNA vez, y desandarlo la devuelve. Se simula el sistema con la
    /// misma cuenta que hace `caminar_tramo`.
    #[test]
    fn andar_y_desandar_el_tramo_cambia_de_planta_una_sola_vez() {
        let (mut esc, mut tiles, body) = mundo();
        construir_tramo(&mut esc, &mut tiles, body, 40, 40, 1);
        let t = esc.list()[0];
        let mut z = 0;
        let mut cambios = 0;
        let paso_a_planta = |z: &mut i32, x: f32, cambios: &mut i32| {
            let s = escaleras::progreso(&t, x, 40.5);
            if *z == t.z && s >= escaleras::CIMA {
                *z = t.z + 1;
                *cambios += 1;
            } else if *z == t.z + 1 && s <= escaleras::PIE {
                *z = t.z;
                *cambios += 1;
            }
        };
        // Subir: del pie a la cima, en pasos de un décimo de celda.
        for i in 0..=40 {
            paso_a_planta(&mut z, 40.0 + i as f32 * 0.1, &mut cambios);
        }
        assert_eq!(z, 1, "andar el tramo entero sube una planta");
        assert_eq!(cambios, 1, "y sólo una vez, sin rebotes");
        // Bajar: el mismo camino al revés.
        for i in (0..=40).rev() {
            paso_a_planta(&mut z, 40.0 + i as f32 * 0.1, &mut cambios);
        }
        assert_eq!(z, 0, "desandarlo devuelve a la planta baja");
        assert_eq!(cambios, 2);
    }

    /// **El pozo es un pozo: sus paredes frenan.**
    ///
    /// Es la queja literal — *"yo hago un hueco del bloque, caigo, y alrededor los
    /// bloques tienen colisión, entonces no me deberían dejar caminar"*. Cavás una
    /// celda, caés a la de abajo, y las ocho vecinas de ese nivel son tierra
    /// maciza: para caminar más hay que picar más. Lo que te sacaba de ahí sin
    /// pedirlo era la trepada automática, no un agujero en la colisión — pero el
    /// día que alguien "arregle" la colisión del subsuelo, esto lo caza.
    #[test]
    fn el_hueco_que_cavaste_esta_rodeado_de_pared() {
        const VECINAS: [(i64, i64); 8] =
            [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)];
        let (_, mut tiles, body) = mundo();
        // Un sitio con tierra cavable arriba y subsuelo macizo debajo, él y sus
        // ocho vecinas: o sea, lejos de una galería natural, que es donde la
        // pregunta tiene sentido.
        let (tx, tz) = (0..4000i64)
            .flat_map(|x| (0..40i64).map(move |z| (x, z)))
            .find(|&(x, z)| {
                ha_shared::topdown::topdown_tile(body, 0, x, z).diggable_down()
                    && VECINAS.iter().chain([(0, 0)].iter()).all(|&(dx, dz)| {
                        ha_shared::topdown::topdown_tile(body, -1, x + dx, z + dz).solid()
                    })
            })
            .expect("hay tierra firme con subsuelo macizo cerca del origen");
        assert!(tiles.dig_down(body, tx, tz).is_some(), "no se pudo cavar");
        tiles.z = -1;
        for (dx, dz) in VECINAS {
            assert!(
                tiles.walk_blocked(body, tx + dx, tz + dz),
                "la vecina ({dx},{dz}) del fondo del pozo deja caminar: el hueco no es un hueco"
            );
        }
        // Y el fondo sí se pisa, que es la otra mitad: caer no te deja enterrado.
        assert!(!tiles.walk_blocked(body, tx, tz), "el fondo del pozo no se pisa");
    }

    /// **Quedarse quieto en mitad del tramo no hace nada.** Sin esto habría que
    /// volver a inventar el pestillo que se borró.
    #[test]
    fn el_medio_del_tramo_no_compromete_nada() {
        let (mut esc, mut tiles, body) = mundo();
        construir_tramo(&mut esc, &mut tiles, body, 40, 40, 1);
        let t = esc.list()[0];
        for _ in 0..100 {
            let s = escaleras::progreso(&t, 42.0, 40.5);
            assert!(s > escaleras::PIE && s < escaleras::CIMA);
        }
    }
}

/// Un escalón dibujado por `dibujar_escalones`.
#[derive(Component)]
struct EscalonArt;

/// **Los escalones se dibujan girados según hacia dónde sube el tramo.** El arte del pack
/// (`stairs/up`, `stairs/down`) tiene los peldaños en horizontal, o sea que sube de norte a sur;
/// un tramo que sube de izquierda a derecha (dir 1 y 3) tiene que verse con los peldaños en
/// vertical. Antes todos salían iguales y cuatro escalones en fila apuntaban «de arriba para abajo».
fn dibujar_escalones(
    mut commands: Commands,
    escaleras: Res<Escaleras>,
    tiles: Res<crate::world2d::TileWorld>,
    art: Res<crate::art2d::ArtLibrary>,
    viejos: Query<Entity, With<EscalonArt>>,
    mut visto: Local<(i32, usize, u64)>,
) {
    // Dentro de un edificio: UNA escalera dibujada entera (2 de ancho × 4 de largo) por tramo doble.
    if let Some((slot, piso)) = ha_shared::interiores::de_z(tiles.z) {
        let clave = (tiles.z, usize::MAX, tiles.version);
        if *visto == clave && !viejos.is_empty() {
            return;
        }
        *visto = clave;
        for e in &viejos {
            commands.entity(e).despawn_recursive();
        }
        let Some(plano) = ha_shared::interiores::plano_de(slot) else { return };
        let arte = "building/escalera_1";
        let mut hechas: Vec<(i32, i64)> = Vec::new();
        for t in plano.tramos.iter().filter(|t| t.piso == piso || t.piso + 1 == piso) {
            // El par de columnas empieza en la menor `tx` de los dos tramos del mismo piso.
            let tx0 = plano.tramos.iter().filter(|u| u.piso == t.piso && u.tz == t.tz).map(|u| u.tx).min().unwrap_or(t.tx);
            if hechas.contains(&(t.piso, tx0)) {
                continue;
            }
            hechas.push((t.piso, tx0));
            let largo = ha_shared::escaleras::PASOS as f32;
            // Centro del bloque 2 × PASOS: las celdas van de tz-(PASOS-1) a tz.
            let (cx, cz) = (tx0 as f32 + 1.0, t.tz as f32 + 1.0 - largo * 0.5);
            let r = crate::world2d::render_xy(cx, cz);
            let mut sprite = art.sprite_or_color(arte, Color::srgb(0.62, 0.50, 0.32), Vec2::new(2.0, largo));
            sprite.custom_size = Some(Vec2::new(2.0, largo));
            // Vista desde arriba (el hueco por el que se baja) se oscurece un poco.
            if piso == t.piso + 1 {
                sprite.color = Color::srgb(0.72, 0.72, 0.78);
            }
            commands.spawn((
                sprite,
                Transform::from_xyz(r.x, r.y, crate::layers::FLOOR + crate::layers::SOBRE_EL_PISO),
                EscalonArt,
                Name::new("Escalera"),
            ));
        }
        return;
    }
    let clave = (tiles.z, escaleras.list.len(), tiles.version);
    if *visto == clave && !viejos.is_empty() || (escaleras.list.is_empty() && viejos.is_empty()) {
        *visto = clave;
        return;
    }
    *visto = clave;
    for e in &viejos {
        commands.entity(e).despawn_recursive();
    }
    for t in escaleras.list.iter() {
        // Abajo se ve el escalón; arriba, el hueco por el que se baja.
        let arte = if tiles.z == t.z {
            "stairs/up"
        } else if tiles.z == t.z + 1 {
            "stairs/down"
        } else {
            continue;
        };
        if !art.tile_has_art(arte) {
            continue;
        }
        let giro = match t.dir {
            0 => 0.0,
            2 => std::f32::consts::PI,
            1 => -std::f32::consts::FRAC_PI_2,
            _ => std::f32::consts::FRAC_PI_2,
        };
        for (cx, cz, _) in t.celdas() {
            let (wx, wz) = (cx as f32 * TILE_M + 0.5, cz as f32 * TILE_M + 0.5);
            let r = crate::world2d::render_xy(wx, wz);
            let mut sprite = art.sprite_or_color(arte, Color::srgb(0.62, 0.50, 0.32), Vec2::splat(TILE_M));
            sprite.custom_size = Some(Vec2::splat(TILE_M));
            commands.spawn((
                sprite,
                Transform { translation: Vec3::new(r.x, r.y, crate::layers::FLOOR + crate::layers::SOBRE_EL_PISO), rotation: Quat::from_rotation_z(giro), ..default() },
                EscalonArt,
                Name::new("Escalon"),
            ));
        }
    }
}
