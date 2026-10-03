//! **Qué está haciendo el jugador, y con qué animación se ve.**
//!
//! La hoja del personaje del generador de LimeZu trae el cuerpo de 21 animaciones (ver
//! `ha_assets::chargen_index::CHAR_ANIMS`), pero hasta el 3 oct 2026 el jugador sólo usaba tres:
//! respirar, caminar y nadar. Talaba un árbol parado, comía sin mover un dedo, recibía un mordisco
//! sin acusarlo y, si lo dejabas quieto, se quedaba respirando para siempre. Los aldeanos sí
//! usaban sus filas (`society.rs`); el jugador no.
//!
//! Este módulo decide el VERBO de la pose (`sim::face_player_sprite` le agrega la dirección) en tres
//! capas, de la más fuerte a la más débil:
//!
//! 1. **Una acción** ([`AccionJugador`]): golpear, juntar, comer, beber, disparar, recibir daño.
//!    Cada sistema que hace algo llama a [`AccionJugador::hacer`]; los que ya existían se observan
//!    desde acá (como hace `audio.rs` con los sonidos) para no meterles dependencias.
//! 2. **El arma en la mano**: parado con un arma de fuego, se apunta (`gun_idle`).
//! 3. **El reposo** ([`Reposo`]): quieto un rato saca el celular o se pone a leer; más rato, se sienta
//!    en el piso. Es lo que hacen los juegos de referencia (Stardew, Zomboid) para que un personaje
//!    quieto no parezca colgado.
//!
//! Lo que el pack NO trae en el cuerpo va como una capa hija encima, sincronizada cuadro a cuadro
//! con la fila del cuerpo: el libro (pack, `player/libro`), la comida y la taza (dibujadas,
//! `player/en_mano`) y las ondas al nadar (dibujadas, `player/ondas`). Ver
//! `scripts/dibujar-acciones-jugador.py`.
//!
//! **Costo:** un sistema que lee recursos y dos sprites hijos que casi siempre están ocultos. La
//! pose sólo se reconstruye cuando cambia (`sim::CurrentPose`), igual que antes.

use bevy::prelude::*;

use crate::audio::{PlaySfxAt, Sfx};
use crate::sim::{Player, PlayerVisual};

/// Quieto este rato (s) ⇒ saca el celular o se pone a leer.
pub const REPOSO_ENTRETENERSE_S: f32 = 9.0;
/// Quieto este rato (s) ⇒ se sienta en el piso.
pub const REPOSO_SENTARSE_S: f32 = 40.0;
/// Lo que tarda en sacar el celular: los 3 cuadros de `phone_intro` a 8 fps.
const SACAR_CELULAR_S: f32 = 3.0 / 8.0;

/// Comer y beber: los 12 cuadros de la capa en la mano a 8 fps (un bocado doble).
pub const COMER_S: f32 = 12.0 / 8.0;
/// Un golpe: los 6 cuadros de `hit`/`punch` a 12 fps.
const GOLPE_S: f32 = 0.5;
/// Recibir daño: los 3 cuadros de `hurt` a 10 fps.
const DANO_S: f32 = 0.3;
/// Un tiro: los 3 cuadros de `shoot` a 14 fps, y un poco más para que se lea.
const TIRO_S: f32 = 0.25;
/// Colocar algo: la mitad de `pick_up` (agacharse); entero tarda más que lo que se tarda en poner.
const COLOCAR_S: f32 = 0.5;
/// Mientras dura un mantenido (talar, cosechar) la acción se renueva con este margen: al soltar se
/// corta enseguida.
const MANTENIDO_S: f32 = 0.15;
/// Alcance (m) para atribuirle al jugador un sonido de colocar: lo que suena más lejos lo hizo otro.
const COLOCAR_ALCANCE_M: f32 = 4.0;

/// Altura en pantalla del cuerpo al nadar: se recorta en la cintura (40 de los 64 px de la celda).
pub const NADO_ALTO_PX: u32 = 40;

/// **La acción en curso del jugador.** Cualquiera puede pedirla; gana la de más prioridad hasta
/// que se termine.
#[derive(Resource, Default, Debug)]
pub struct AccionJugador {
    verbo: Option<&'static str>,
    resta: f32,
    /// Rumbo (como `PlayerVisual::to_rot`) hacia lo que se está tocando, si lo hay.
    rumbo: Option<f32>,
}

/// Qué acción pisa a cuál: el daño interrumpe todo; un golpe, a juntar o comer.
fn prioridad(verbo: &str) -> u8 {
    match verbo {
        "hurt" => 3,
        "shoot" | "punch" | "stab" | "hit" => 2,
        _ => 1,
    }
}

impl AccionJugador {
    /// Pide una pose por `segundos`. No pisa una de más prioridad que siga en curso.
    pub fn hacer(&mut self, verbo: &'static str, segundos: f32) {
        if let Some(v) = self.verbo {
            if v != verbo && self.resta > 0.0 && prioridad(v) > prioridad(verbo) {
                return;
            }
        }
        if self.verbo == Some(verbo) {
            self.resta = self.resta.max(segundos);
        } else {
            self.verbo = Some(verbo);
            self.resta = segundos;
            self.rumbo = None;
        }
    }

    /// Como [`hacer`](Self::hacer), mirando hacia `hacia` desde `desde` (coordenadas de MUNDO x, z).
    pub fn hacer_hacia(&mut self, verbo: &'static str, segundos: f32, desde: Vec2, hacia: Vec2) {
        self.hacer(verbo, segundos);
        if self.verbo == Some(verbo) {
            let d = hacia - desde;
            if d.length_squared() > 1e-4 {
                self.rumbo = Some(d.y.atan2(d.x));
            }
        }
    }

    /// Un golpe con lo que tenés en la mano, hacia el blanco.
    pub fn golpe(&mut self, item: &str, desde: Vec2, hacia: Vec2) {
        self.hacer_hacia(verbo_de_golpe(item), GOLPE_S, desde, hacia);
    }

    /// El verbo en curso y hacia dónde mira, si hay uno.
    pub fn actual(&self) -> Option<(&'static str, Option<f32>)> {
        self.verbo.filter(|_| self.resta > 0.0).map(|v| (v, self.rumbo))
    }

    fn pasar(&mut self, dt: f32) {
        self.resta -= dt;
        if self.resta <= 0.0 {
            self.verbo = None;
            self.rumbo = None;
            self.resta = 0.0;
        }
    }
}

/// **Con qué se pega**: a mano es un puñetazo, con un cuchillo una puñalada, con cualquier otra
/// cosa (hacha, pico, espada, palo) un golpe de brazo.
pub fn verbo_de_golpe(item: &str) -> &'static str {
    let i = item.to_ascii_lowercase();
    if i.is_empty() || i == "hand" {
        "punch"
    } else if i.contains("knife") || i.contains("cuchillo") || i.contains("daga") {
        "stab"
    } else {
        "hit"
    }
}

/// **Cuánto hace que el jugador no hace nada**, y a qué se puso a jugar mientras.
#[derive(Resource, Default, Debug)]
pub struct Reposo {
    pub quieto_s: f32,
    /// Cuántas veces se entretuvo: alterna el celular y el libro.
    pub(crate) turno: u32,
}

/// La pose de reposo para `quieto_s` segundos sin hacer nada. Pura: se testea sin la app.
pub fn pose_de_reposo(quieto_s: f32, turno: u32) -> Option<&'static str> {
    if quieto_s >= REPOSO_SENTARSE_S {
        Some("sit_floor")
    } else if quieto_s >= REPOSO_ENTRETENERSE_S {
        if turno % 2 == 0 {
            if quieto_s < REPOSO_ENTRETENERSE_S + SACAR_CELULAR_S {
                Some("phone_intro")
            } else {
                Some("phone")
            }
        } else {
            Some("read")
        }
    } else {
        None
    }
}

impl Reposo {
    pub fn pose(&self) -> Option<&'static str> {
        pose_de_reposo(self.quieto_s, self.turno)
    }
}

/// **El verbo final del cuerpo**, en el orden en que se pisan. `nadando` manda sobre todo (no se
/// puede comer ni sentarse en el agua); después la acción, después caminar, el arma y el reposo.
pub fn verbo_del_cuerpo(
    nadando: bool,
    moviendo: bool,
    accion: Option<&'static str>,
    arma_en_mano: bool,
    reposo: Option<&'static str>,
    descanso: Option<&'static str>,
) -> &'static str {
    if nadando {
        return "swim";
    }
    // Sentado o acostado en un mueble (`descanso.rs`): ni caminás ni pegás hasta que te levantás.
    if let Some(d) = descanso {
        return d;
    }
    if let Some(a) = accion.filter(|a| !moviendo || !cede_al_caminar(a)) {
        return match a {
            // El cuerpo de comer/beber es el de las manos en el pecho; lo que se lleva a la boca
            // va en la capa de encima.
            "comer" | "beber" => "hands_chest",
            otro => otro,
        };
    }
    if moviendo {
        return if arma_en_mano { "walk_gun" } else { "walk" };
    }
    if arma_en_mano {
        return "gun_idle";
    }
    reposo.unwrap_or("idle")
}

/// Las acciones LARGAS ceden al caminar: si comés y te movés, caminás (la acción sigue y vuelve al
/// frenar). Las cortas —golpe, tiro, daño— se ven aunque estés corriendo: duran medio segundo.
fn cede_al_caminar(verbo: &str) -> bool {
    matches!(verbo, "comer" | "beber" | "pick_up" | "lift" | "grab_gun")
}

/// Los sistemas que deciden la pose del jugador; `descanso` corre antes.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PoseSet;

/// La capa hija que lleva lo que el jugador tiene en la mano (libro, comida, taza).
#[derive(Component)]
pub struct CapaEnMano;

/// La capa hija de las ondas al nadar.
#[derive(Component)]
pub struct OndasNado;

pub struct PoseJugadorPlugin;

impl Plugin for PoseJugadorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AccionJugador>()
            .init_resource::<Reposo>()
            .add_systems(Startup, precargar_capas)
            .add_systems(
                Update,
                (
                    poner_capas,
                    (observar_cosecha, observar_cuerpo, observar_armas, observar_colocar),
                    pasar_tiempo,
                    sincronizar_capas,
                )
                    .chain()
                    .in_set(PoseSet)
                    .before(crate::sim::FacePlayerSet),
            );
    }
}

fn precargar_capas(art: Res<crate::art2d::ArtLibrary>) {
    art.precargar(["player/en_mano", "player/libro", "player/ondas"]);
}

/// Cuelga las dos capas del jugador la primera vez que aparece (y si un respawn las perdió).
fn poner_capas(
    mut commands: Commands,
    jugador: Query<(Entity, Option<&Children>), With<Player>>,
    capas: Query<(), Or<(With<CapaEnMano>, With<OndasNado>)>>,
) {
    for (e, hijos) in &jugador {
        let ya = hijos.map(|h| h.iter().filter(|c| capas.get(**c).is_ok()).count()).unwrap_or(0);
        if ya >= 2 {
            continue;
        }
        commands.entity(e).with_children(|p| {
            // Mismo tamaño que el cuerpo y centrada en él: la capa se dibujó en la MISMA celda de
            // 32x64, así que calza sin offsets.
            p.spawn((
                CapaEnMano,
                Sprite { custom_size: Some(crate::scene::TAMANO_JUGADOR), ..default() },
                Transform::from_xyz(0.0, 0.0, 0.002),
                Visibility::Hidden,
            ));
            let t = crate::scene::TAMANO_JUGADOR;
            // 48x16 px de la celda de 32x64 ⇒ 1,5 anchos de cuerpo y un cuarto de alto. Va en la
            // cintura, que es el borde de abajo del cuerpo recortado (ver `sim::face_player_sprite`).
            let alto_cuerpo = t.y * NADO_ALTO_PX as f32 / 64.0;
            p.spawn((
                OndasNado,
                Sprite { custom_size: Some(Vec2::new(t.x * 1.5, t.y * 16.0 / 64.0)), ..default() },
                Transform::from_xyz(0.0, -alto_cuerpo / 2.0, 0.002),
                Visibility::Hidden,
            ));
        });
    }
}

/// Talar, picar, cosechar, levantar: mientras dure el mantenido.
fn observar_cosecha(
    hold: Res<crate::input::HoldInteractState>,
    target: Res<crate::targeting::LookTarget>,
    equipped: Res<crate::input::EquippedItem>,
    jugador: Query<&Transform, With<Player>>,
    mut accion: ResMut<AccionJugador>,
) {
    use ha_shared::InteractTargetKind as K;
    if !hold.active {
        return;
    }
    let Some(kind) = hold.locked_kind.or(target.kind) else { return };
    let verbo = match kind {
        K::Tree | K::Rock | K::Block | K::OreIron | K::OreGold | K::GemBlue | K::GemRed => {
            verbo_de_golpe(&equipped.item_id)
        }
        K::Crop | K::Prop | K::Container | K::Chest | K::Water => "pick_up",
        K::Entity => "lift",
        _ => return,
    };
    let Ok(tf) = jugador.get_single() else { return };
    let desde = Vec2::new(tf.translation.x, -tf.translation.y);
    if target.kind.is_some() {
        let hacia = Vec2::new(target.hit_point.x, -target.hit_point.y);
        accion.hacer_hacia(verbo, MANTENIDO_S, desde, hacia);
    } else {
        accion.hacer(verbo, MANTENIDO_S);
    }
}

/// Daño, comer y beber: mirando los DELTAS de la supervivencia, igual que `audio::survival_sounds`.
fn observar_cuerpo(
    jugador: Query<&crate::sim::SimState, With<Player>>,
    mut prev: Local<Option<(f32, f32, f32)>>,
    mut accion: ResMut<AccionJugador>,
) {
    let Ok(sim) = jugador.get_single() else { return };
    let ahora = (sim.hp, sim.survival.hunger, sim.survival.thirst);
    let Some(p) = prev.replace(ahora) else { return };
    if ahora.0 < p.0 - 0.5 {
        accion.hacer("hurt", DANO_S);
    }
    if ahora.1 > p.1 + 2.0 {
        accion.hacer("comer", COMER_S);
    }
    if ahora.2 > p.2 + 2.0 {
        accion.hacer("beber", COMER_S);
    }
}

/// El tiro se lee del retroceso (salta en cada disparo y después decae); recargar, de la recarga.
fn observar_armas(
    armas: Res<crate::armas::EstadoArmas>,
    mut prev_kick: Local<f32>,
    mut accion: ResMut<AccionJugador>,
) {
    let kick = armas.retroceso();
    if kick > *prev_kick + 1e-4 {
        accion.hacer("shoot", TIRO_S);
    }
    *prev_kick = kick;
    if armas.recargando() {
        accion.hacer("grab_gun", MANTENIDO_S);
    }
}

/// Colocar (construir, plantar, poner un mueble): todos esos caminos suenan `Place`/`Stamp` en el
/// punto donde cae la pieza. Sólo cuenta si suena al alcance de la mano del jugador.
fn observar_colocar(
    mut sonidos: EventReader<PlaySfxAt>,
    jugador: Query<&Transform, With<Player>>,
    mut accion: ResMut<AccionJugador>,
) {
    let Ok(tf) = jugador.get_single() else {
        sonidos.clear();
        return;
    };
    let yo = tf.translation.truncate();
    for PlaySfxAt(sfx, en) in sonidos.read() {
        if matches!(sfx, Sfx::Place | Sfx::Stamp) && en.distance(yo) <= COLOCAR_ALCANCE_M {
            let desde = Vec2::new(yo.x, -yo.y);
            accion.hacer_hacia("pick_up", COLOCAR_S, desde, Vec2::new(en.x, -en.y));
        }
    }
}

/// El reloj de la acción y el del reposo. Cualquier movimiento, acción, vuelo o nado despierta.
fn pasar_tiempo(
    time: Res<Time>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    descanso: Res<crate::descanso::Descanso>,
    jugador: Query<&PlayerVisual, With<Player>>,
    mut accion: ResMut<AccionJugador>,
    mut reposo: ResMut<Reposo>,
) {
    let dt = time.delta_secs();
    accion.pasar(dt);
    let Ok(v) = jugador.get_single() else { return };
    let moviendo = (v.to_x - v.from_x).abs() > 1e-4 || (v.to_z - v.from_z).abs() > 1e-4;
    if moviendo || v.swimming || fly.flying() || accion.actual().is_some() || descanso.descansando() {
        if reposo.quieto_s >= REPOSO_ENTRETENERSE_S {
            reposo.turno = reposo.turno.wrapping_add(1);
        }
        reposo.quieto_s = 0.0;
    } else {
        reposo.quieto_s += dt;
    }
}

/// Muestra la capa que toca y la arranca en el cuadro 0 junto con el cuerpo.
#[allow(clippy::type_complexity)]
fn sincronizar_capas(
    mut commands: Commands,
    art: Res<crate::art2d::ArtLibrary>,
    accion: Res<AccionJugador>,
    reposo: Res<Reposo>,
    descanso: Res<crate::descanso::Descanso>,
    jugador: Query<&PlayerVisual, With<Player>>,
    mut mano: Query<(Entity, &mut Sprite, &mut Visibility), (With<CapaEnMano>, Without<OndasNado>)>,
    mut ondas: Query<(Entity, &mut Sprite, &mut Visibility), (With<OndasNado>, Without<CapaEnMano>)>,
    mut ultimo: Local<(Option<&'static str>, bool)>,
) {
    let Ok(v) = jugador.get_single() else { return };
    let moviendo = (v.to_x - v.from_x).abs() > 1e-4 || (v.to_z - v.from_z).abs() > 1e-4;
    // La capa sigue al CUERPO: sólo se ve cuando el cuerpo está en la pose que la sostiene.
    let capa = if v.swimming || moviendo || descanso.descansando() {
        None
    } else {
        match accion.actual().map(|(a, _)| a) {
            Some("comer") => Some(("player/en_mano", "comer")),
            Some("beber") => Some(("player/en_mano", "beber")),
            Some(_) => None,
            None => (reposo.pose() == Some("read")).then_some(("player/libro", "leer")),
        }
    };
    let pose = capa.map(|(_, p)| p);
    if ultimo.0 != pose {
        ultimo.0 = pose;
        if let Ok((e, mut sprite, mut vis)) = mano.get_single_mut() {
            match capa.and_then(|(k, p)| Some((art.pose_image(k, p).or_else(|| art.get(k))?, art.pose(k, p)?))) {
                Some((img, (_, anim))) => {
                    sprite.image = img;
                    sprite.texture_atlas = None;
                    sprite.rect = None;
                    match anim {
                        Some(a) => crate::render::try_insert(&mut commands, e, "pose_jugador/mano", a),
                        None => {
                            commands.entity(e).remove::<crate::art2d::ArtAnimated>();
                        }
                    }
                    *vis = Visibility::Inherited;
                }
                None => *vis = Visibility::Hidden,
            }
        }
    }
    if ultimo.1 != v.swimming {
        ultimo.1 = v.swimming;
        if let Ok((e, mut sprite, mut vis)) = ondas.get_single_mut() {
            let k = "player/ondas";
            match (v.swimming, art.pose_image(k, "nadar").or_else(|| art.get(k)), art.pose(k, "nadar")) {
                (true, Some(img), Some((_, anim))) => {
                    sprite.image = img;
                    sprite.texture_atlas = None;
                    sprite.rect = None;
                    if let Some(a) = anim {
                        crate::render::try_insert(&mut commands, e, "pose_jugador/ondas", a);
                    }
                    *vis = Visibility::Inherited;
                }
                _ => *vis = Visibility::Hidden,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_reposo_va_del_celular_o_el_libro_a_sentarse() {
        assert_eq!(pose_de_reposo(0.0, 0), None);
        assert_eq!(pose_de_reposo(REPOSO_ENTRETENERSE_S - 0.1, 0), None);
        assert_eq!(pose_de_reposo(REPOSO_ENTRETENERSE_S, 0), Some("phone_intro"));
        assert_eq!(pose_de_reposo(REPOSO_ENTRETENERSE_S + 1.0, 0), Some("phone"));
        assert_eq!(pose_de_reposo(REPOSO_ENTRETENERSE_S + 1.0, 1), Some("read"));
        assert_eq!(pose_de_reposo(REPOSO_SENTARSE_S, 1), Some("sit_floor"));
    }

    #[test]
    fn el_dano_pisa_al_golpe_y_el_golpe_a_juntar() {
        let mut a = AccionJugador::default();
        a.hacer("pick_up", 1.0);
        a.hacer("hit", 0.5);
        assert_eq!(a.actual().unwrap().0, "hit");
        a.hacer("hurt", 0.3);
        a.hacer("pick_up", 1.0);
        assert_eq!(a.actual().unwrap().0, "hurt");
        a.pasar(0.31);
        assert!(a.actual().is_none(), "al terminarse no queda nada colgado");
    }

    #[test]
    fn el_golpe_depende_de_lo_que_tenes_en_la_mano() {
        assert_eq!(verbo_de_golpe("hand"), "punch");
        assert_eq!(verbo_de_golpe(""), "punch");
        assert_eq!(verbo_de_golpe("axe"), "hit");
        assert_eq!(verbo_de_golpe("sword_steel"), "hit");
        assert_eq!(verbo_de_golpe("knife"), "stab");
    }

    #[test]
    fn el_agua_manda_y_el_arma_sólo_cuando_estás_parado() {
        assert_eq!(verbo_del_cuerpo(true, true, Some("hit"), true, None, None), "swim");
        assert_eq!(verbo_del_cuerpo(false, true, Some("hurt"), false, None, None), "hurt");
        assert_eq!(verbo_del_cuerpo(false, false, Some("comer"), false, None, None), "hands_chest");
        // Comer caminando: camina; pegar caminando: pega.
        assert_eq!(verbo_del_cuerpo(false, true, Some("comer"), false, None, None), "walk");
        // Sentado/acostado manda sobre caminar, acciones y arma; el agua sobre todo.
        assert_eq!(verbo_del_cuerpo(false, true, Some("hit"), true, None, Some("sit")), "sit");
        assert_eq!(verbo_del_cuerpo(true, false, None, false, None, Some("sleep")), "swim");
        assert_eq!(verbo_del_cuerpo(false, true, Some("punch"), false, None, None), "punch");
        assert_eq!(verbo_del_cuerpo(false, true, None, true, None, None), "walk_gun");
        assert_eq!(verbo_del_cuerpo(false, false, None, true, Some("read"), None), "gun_idle");
        assert_eq!(verbo_del_cuerpo(false, false, None, false, Some("read"), None), "read");
        assert_eq!(verbo_del_cuerpo(false, false, None, false, None, None), "idle");
    }

    #[test]
    fn cada_verbo_que_pide_el_jugador_existe_en_la_hoja() {
        use ha_assets::chargen_index::char_anim;
        for v in [
            "idle", "walk", "hit", "punch", "stab", "pick_up", "lift", "hurt", "shoot", "grab_gun", "gun_idle",
            "phone_intro", "phone", "read", "sit_floor", "hands_chest", "walk_gun", "sit", "sleep",
        ] {
            assert!(char_anim(v).is_some(), "la hoja no tiene `{v}`");
        }
        // El cuerpo de comer y la capa de la mano tienen que durar lo mismo, o la comida flota.
        let manos = char_anim("hands_chest").unwrap();
        assert!((manos.frames as f32 * 2.0 / manos.fps - COMER_S).abs() < 1e-3);
    }
}
