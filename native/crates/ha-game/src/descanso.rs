//! **Sentarse y dormir en los muebles.**
//!
//! La hoja del personaje trae `sit` (de perfil, sentado en algo) y `sleep` (la cabeza sobre la almohada), y el
//! catálogo de slots ya sabe qué es una cama y qué es un asiento (`Cama`, `Silla`, `Sillón`, `Sofá`, `Banco`…
//! en castellano). Lo que faltaba era el verbo: `Interaccion::verbo` del manifest ("SENTARSE", "DORMIR") no lo
//! ejecutaba nadie, y por eso el jugador no se podía sentar en el banco de la plaza ni acostarse en la cama que
//! había armado.
//!
//! **Cómo se usa:** un TOQUE sobre el mueble (mantener sigue levantándolo, como todo lo demás). Te acerca al
//! mueble, te acomoda encima y entra la pose. Te levantás con cualquier movimiento, saltando, si te lastiman o
//! si te vas volando. Descansar **cura un poco** (sentado 2/s, dormido 3/s): es el premio de armarse una cama.
//! No adelanta el reloj del día — eso sería otra feature, y un nombre que prometa más de lo que hace
//! (`biblia §2`, «lo que se nos escapó»).
//!
//! **Qué es un mueble de uso** se decide UNA vez por la etiqueta del slot (`uso_de`), con caché: el sembrado
//! de un chunk no puede pagar una búsqueda por prop (`biblia §10`).

use std::collections::HashMap;
use std::sync::OnceLock;

use bevy::prelude::*;
use ha_shared::InteractTargetKind;

use crate::audio::{PlaySfx, Sfx};
use crate::sim::{Player, PlayerVisual, SimState};

/// Hasta dónde (m) podés estar del mueble para usarlo con un toque.
pub const ALCANCE_M: f32 = 3.6;
/// Vida que se recupera por segundo sentado.
pub const CURA_SENTADO: f32 = 2.0;
/// Vida que se recupera por segundo dormido.
pub const CURA_DORMIDO: f32 = 3.0;
/// Cuánto (m) hacia el norte del borde del pie queda el cuerpo sentado: encima del asiento, no delante.
const SENTADO_SOBRE_EL_PIE: f32 = 0.28;
/// Qué parte del alto del sprite de una cama (desde el pie) es el colchón donde te acostás.
const CAMA_FRACCION_ALTO: f32 = 0.38;
/// La figura dormida mide 34 de los 64 px de la celda y está 11 px por ENCIMA del centro de la celda
/// (bbox y 4..38, medido sobre `art/player/character.png`). Se baja esos px para que el centro de la
/// figura caiga sobre el colchón: 11/64 del alto del jugador.
pub const DORMIDO_BAJAR_M: f32 = 11.0 / 64.0 * 1.9;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Uso {
    /// Silla, sillón, sofá, banco, puf…
    Asiento,
    /// Cama, colchón.
    Cama,
}

/// Qué uso tiene un slot de arte, por la etiqueta que el catálogo le puso en castellano.
pub fn uso_de(key: &str) -> Option<Uso> {
    static INDICE: OnceLock<HashMap<&'static str, Uso>> = OnceLock::new();
    let base = key.split('#').next().unwrap_or(key);
    INDICE
        .get_or_init(|| {
            ha_assets::slots::game_slots()
                .into_iter()
                .filter_map(|s| uso_por_etiqueta(s.label).map(|u| (s.key, u)))
                .collect()
        })
        .get(base)
        .copied()
}

/// La regla, pura y testeable: sale de cómo el catálogo nombra las cosas ("Cama de hospital", "Silla de
/// oficina", "Banco de trabajo"…).
pub fn uso_por_etiqueta(label: &str) -> Option<Uso> {
    let l = label.trim();
    // Lo que se llama como un asiento pero no se usa como uno.
    if l.starts_with("Silla de ruedas") || l.starts_with("Banco de trabajo") || l.starts_with("Banqueta de piano") {
        return None;
    }
    if l.starts_with("Cama") || l.starts_with("Colchón") {
        return Some(Uso::Cama);
    }
    const ASIENTOS: [&str; 7] = ["Silla", "Sillón", "Sofá", "Banco", "Banqueta", "Puf", "Reposera"];
    ASIENTOS.iter().any(|p| l.starts_with(p)).then_some(Uso::Asiento)
}

/// Un mueble sobre el que se puede descansar. `x` y `pie` son coordenadas de MUNDO (x, z): el centro del
/// sprite y la fila del pie (la misma que lo Y-sortea). `alto` es lo que mide el sprite, en metros.
#[derive(Component, Clone, Copy, Debug)]
pub struct MuebleDeUso {
    pub uso: Uso,
    pub x: f32,
    pub pie: f32,
    pub alto: f32,
}

impl MuebleDeUso {
    pub fn nuevo(key: &str, x: f32, pie: f32, alto: f32) -> Option<Self> {
        uso_de(key).map(|uso| Self { uso, x, pie, alto })
    }

    /// Dónde queda el cuerpo (x, z de mundo) al usarlo.
    pub fn sitio(&self) -> Vec2 {
        let dz = match self.uso {
            Uso::Asiento => SENTADO_SOBRE_EL_PIE,
            Uso::Cama => (self.alto * CAMA_FRACCION_ALTO).max(0.6),
        };
        Vec2::new(self.x, self.pie - dz)
    }
}

#[derive(Clone, Copy, Debug)]
struct Sentado {
    mueble: Entity,
    uso: Uso,
    /// z de mundo con el que se Y-sortea al jugador: el del mueble, para que quede ENCIMA de él.
    orden_z: f32,
    hp_antes: f32,
}

/// Si el jugador está sentado o acostado, y dónde.
#[derive(Resource, Default, Debug)]
pub struct Descanso {
    actual: Option<Sentado>,
}

impl Descanso {
    /// El verbo de la hoja que corresponde (`sit` / `sleep`).
    pub fn pose(&self) -> Option<&'static str> {
        self.actual.map(|s| match s.uso {
            Uso::Asiento => "sit",
            Uso::Cama => "sleep",
        })
    }

    /// z de mundo con el que Y-sortear al jugador, si descansa sobre algo.
    pub fn orden_z(&self) -> Option<f32> {
        self.actual.map(|s| s.orden_z)
    }

    /// Cuánto bajar el dibujo (m) para que la figura quede sobre el mueble.
    pub fn bajar_m(&self) -> f32 {
        match self.actual {
            Some(Sentado { uso: Uso::Cama, .. }) => DORMIDO_BAJAR_M,
            _ => 0.0,
        }
    }

    pub fn descansando(&self) -> bool {
        self.actual.is_some()
    }

    /// Te levanta (lo usan los drivers de QA; el juego te levanta en `levantarse`).
    pub(crate) fn levantar(&mut self) {
        self.actual = None;
    }
}

/// Los sistemas del descanso, para que los drivers de QA se ordenen antes.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct DescansoSet;

pub struct DescansoPlugin;

impl Plugin for DescansoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Descanso>().add_systems(
            Update,
            (usar_mueble, levantarse, curar)
                .chain()
                .in_set(DescansoSet)
                .before(crate::pose_jugador::PoseSet),
        );
    }
}

/// Un toque sobre un mueble de uso: sentarse / acostarse (o levantarse si ya estás ahí).
#[allow(clippy::too_many_arguments)]
fn usar_mueble(
    mut hold: ResMut<crate::input::HoldInteractState>,
    target: Res<crate::targeting::LookTarget>,
    muebles: Query<&MuebleDeUso>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    mut descanso: ResMut<Descanso>,
    mut sfx: EventWriter<PlaySfx>,
    mut player: Query<(&mut SimState, &mut PlayerVisual), With<Player>>,
) {
    if !hold.tapped || !matches!(target.kind, Some(InteractTargetKind::Prop | InteractTargetKind::Entity)) {
        return;
    }
    let Some(e) = target.entity else { return };
    let Ok(m) = muebles.get(e) else { return };
    let Ok((mut sim, mut visual)) = player.get_single_mut() else { return };
    hold.tapped = false;
    if descanso.actual.map(|s| s.mueble) == Some(e) {
        descanso.actual = None;
        return;
    }
    let lejos = Vec2::new(sim.body.x, sim.body.z).distance(Vec2::new(m.x, m.pie)) > ALCANCE_M;
    if lejos || fly.flying() || visual.swimming {
        sfx.send(PlaySfx(Sfx::Deny));
        return;
    }
    let sitio = m.sitio();
    sim.body.x = sitio.x;
    sim.body.z = sitio.y;
    visual.from_x = sitio.x;
    visual.to_x = sitio.x;
    visual.from_z = sitio.y;
    visual.to_z = sitio.y;
    descanso.actual = Some(Sentado { mueble: e, uso: m.uso, orden_z: m.pie, hp_antes: sim.hp });
    sfx.send(PlaySfx(match m.uso {
        Uso::Asiento => Sfx::Place,
        Uso::Cama => Sfx::BagClose,
    }));
}

/// Te levantás con cualquier movimiento, un salto, un golpe o si te ponés a volar.
fn levantarse(
    intent: Res<crate::input::MovementIntent>,
    actions: Res<crate::input::TouchActions>,
    fly: Res<crate::creative_fly::CreativeFlight>,
    mut descanso: ResMut<Descanso>,
    player: Query<(&SimState, &PlayerVisual), With<Player>>,
) {
    let Some(s) = descanso.actual else { return };
    let Ok((sim, visual)) = player.get_single() else {
        descanso.actual = None;
        return;
    };
    let mueve = intent.move_x.abs() + intent.move_z.abs() > 0.25;
    let herido = sim.hp < s.hp_antes - 0.5;
    if mueve || actions.jump_edge || herido || fly.flying() || visual.swimming {
        descanso.actual = None;
    } else if let Some(a) = descanso.actual.as_mut() {
        // Lo que curás no cuenta como estar herido: se sigue la vida más alta vista.
        a.hp_antes = a.hp_antes.max(sim.hp);
    }
}

/// Descansar cura.
fn curar(time: Res<Time>, descanso: Res<Descanso>, mut player: Query<&mut SimState, With<Player>>) {
    let Some(s) = descanso.actual else { return };
    let Ok(mut sim) = player.get_single_mut() else { return };
    let por_s = match s.uso {
        Uso::Asiento => CURA_SENTADO,
        Uso::Cama => CURA_DORMIDO,
    };
    sim.hp = (sim.hp + por_s * time.delta_secs()).min(sim.max_hp);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_etiquetas_del_catalogo_se_clasifican() {
        assert_eq!(uso_por_etiqueta("Cama"), Some(Uso::Cama));
        assert_eq!(uso_por_etiqueta("Cama de hospital"), Some(Uso::Cama));
        assert_eq!(uso_por_etiqueta("Colchón verde"), Some(Uso::Cama));
        assert_eq!(uso_por_etiqueta("Silla 12"), Some(Uso::Asiento));
        assert_eq!(uso_por_etiqueta("Sofá"), Some(Uso::Asiento));
        assert_eq!(uso_por_etiqueta("Banco de plaza"), Some(Uso::Asiento));
        // Se llaman como un asiento y no lo son.
        assert_eq!(uso_por_etiqueta("Banco de trabajo 3"), None);
        assert_eq!(uso_por_etiqueta("Silla de ruedas"), None);
        assert_eq!(uso_por_etiqueta("Colchoneta"), None, "la colchoneta del gimnasio no es una cama");
        assert_eq!(uso_por_etiqueta("Escritorio"), None);
    }

    #[test]
    fn el_catalogo_tiene_camas_y_asientos_de_verdad() {
        assert_eq!(uso_de("furniture/cama"), Some(Uso::Cama));
        assert_eq!(uso_de("furniture/silla"), Some(Uso::Asiento));
        assert_eq!(uso_de("prop/banco"), Some(Uso::Asiento));
        assert_eq!(uso_de("furniture/sofa#1_rojo"), Some(Uso::Asiento), "la variante usa el uso de la base");
        let (mut camas, mut asientos) = (0, 0);
        for s in ha_assets::slots::game_slots() {
            match uso_por_etiqueta(s.label) {
                Some(Uso::Cama) => camas += 1,
                Some(Uso::Asiento) => asientos += 1,
                None => {}
            }
        }
        assert!(camas >= 15, "camas: {camas}");
        assert!(asientos >= 150, "asientos: {asientos}");
    }

    #[test]
    fn el_cuerpo_queda_sobre_el_mueble_y_la_cama_mas_adentro() {
        let silla = MuebleDeUso { uso: Uso::Asiento, x: 10.0, pie: 20.5, alto: 1.5 };
        let cama = MuebleDeUso { uso: Uso::Cama, x: 10.0, pie: 20.5, alto: 3.0 };
        assert!(silla.sitio().y < silla.pie && silla.pie - silla.sitio().y < 0.5);
        assert!(cama.sitio().y < silla.sitio().y, "acostado va más al norte, sobre el colchón");
        assert_eq!(cama.sitio().x, 10.0);
    }

    #[test]
    fn descansar_cura_mas_dormido_y_se_ordena_encima_del_mueble() {
        let d = Descanso { actual: Some(Sentado { mueble: Entity::PLACEHOLDER, uso: Uso::Cama, orden_z: 7.5, hp_antes: 50.0 }) };
        assert_eq!(d.pose(), Some("sleep"));
        assert_eq!(d.orden_z(), Some(7.5));
        assert!(d.bajar_m() > 0.0);
        assert!(CURA_DORMIDO > CURA_SENTADO);
        assert_eq!(Descanso::default().pose(), None);
        assert_eq!(Descanso::default().bajar_m(), 0.0);
    }
}
