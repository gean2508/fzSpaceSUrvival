//! **Capas de dibujo del mundo 2D — fuente única de verdad del orden en z.**
//!
//! Antes cada sistema elegía su z a mano (`1.0` acá, `sort_z(z)` allá, `6.0` en
//! techos) y la regla de "qué va delante de qué" vivía repartida en diez
//! archivos. El resultado eran los bugs clásicos de 3/4: el AGUA tapando al
//! jugador (porque `Tile::solid()` —"bloquea el paso"— se usaba como "es alto"),
//! y todo lo pintado en el editor de mundo tapándolo también (porque no había
//! forma de declarar "esto es piso").
//!
//! Acá está la escalera completa, de atrás hacia adelante:
//!
//! | banda | z | qué vive ahí |
//! |---|---|---|
//! | cielo/espacio | < 0 | `sky2d`, discos planetarios |
//! | [`GROUND`] | 0.0 | malla de terreno del chunk |
//! | [`BASEMENT`] | 0.5 | subsuelo visible bajo el piso actual |
//! | [`FLOOR`] | 1.0 | arte PLANO: agua, pisos, caminos, alfombras, decals |
//! | Y-sort | [`YSORT_BASE`] + z·[`YSORT_SPAN`] | paredes, props, actores, jugador |
//! | [`OVERHEAD`] | 6.0 | techos y pisos superiores (se transparentan) |
//! | UI/luz | ≥ 40 | velo nocturno, halos, HUD |
//!
//! La banda de un asset la elige su [`ArtLayer`], que el usuario etiqueta en el
//! **editor de assets** (combo "capa" del slot). El editor de mundo no tiene que
//! saber nada: pinta un charco marcado `Floor` y el charco queda bajo los pies.
//!
//! ## Capa ≠ colisión
//!
//! La capa dice **qué te tapa**; la huella (`CollisionFootprint`) dice **qué te
//! frena**. Son ejes distintos a propósito, y de esa separación salen los
//! efectos de profundidad que se buscan: una pared con sólo su fila del PIE
//! marcada sólida se dibuja Y-sorted por esa fila, así que caminás por detrás de
//! su parte alta (que te tapa) sin poder atravesar su base. Un techo `Roof` no
//! colisiona nunca y se desvanece cuando estás debajo.

use bevy::prelude::*;

pub use ha_assets::ArtLayer;

/// Malla de terreno del chunk (quads vertex-color). El fondo de todo lo jugable.
pub const GROUND: f32 = 0.0;

/// Subsuelo/sótano asomando bajo el piso actual: encima del terreno, debajo del
/// arte de piso, así que nunca compite con nada que pises.
pub const BASEMENT: f32 = 0.5;

/// Arte PLANO pegado al suelo: agua, pisos de madera, caminos, alfombras,
/// plataformas y cualquier decal del editor de mundo. **Siempre bajo los pies
/// del jugador** — es el arreglo del bug del agua.
pub const FLOOR: f32 = 1.0;

/// CENTRO de la banda Y-sorted: todo lo que se levanta del suelo (paredes,
/// props, árboles, NPCs, fauna, jugador) se ordena entre sí por su z de mundo,
/// repartido simétricamente alrededor de este valor.
pub const YSORT_BASE: f32 = 3.5;

/// Metros de mundo → z de sort.
///
/// **No es un número arbitrario**: el mundo caminable llega a ±30 km del origen
/// (`cosmic_scale::EARTH_GAMEPLAY_RADIUS_M`), así que con el `0.001` de antes un
/// sprite a 4 km al norte caía en z negativo — DETRÁS de la malla de terreno —
/// y a 4 km al sur atravesaba la banda de techos. Nadie lo notó porque se juega
/// cerca del origen, pero era una bomba de tiempo: el mismo prop se veía bien o
/// desaparecía según en qué parte del planeta estuvieras. Con `5e-5` los 60 km
/// completos entran en ±1.5 de z, o sea la banda `[2.0, 5.0]`, sin tocar el piso
/// (1.0) ni los techos (6.0). Precisión de sobra: a z≈4 el ULP de f32 es ~5e-7,
/// unas 100 posiciones distinguibles por metro.
pub const YSORT_SPAN: f32 = 5.0e-5;

/// Sesgo de "un pelo adelante/atrás" DENTRO de la banda Y-sort, en unidades de
/// z. Vale ~¼ de tile, así que empata contra el mismo tile pero nunca le gana a
/// la fila de adelante. Los offsets a ojo que había (`+0.0005`, `−0.1`, `+1.0`)
/// estaban en metros de mundo por accidente — `+1.0` era **un kilómetro** de
/// desempate — y con la escala nueva habrían tapado el mundo entero.
pub const HAIR: f32 = YSORT_SPAN * 0.25;

/// Techos y pisos superiores: por encima de todo el mundo Y-sorted y del ghost
/// de construcción (5.7). Se transparentan con [`Overhead`].
pub const OVERHEAD: f32 = 6.0;

/// Y-sort de un sprite parado en `world_z` (más al sur = más adelante).
#[inline]
pub fn ysort(world_z: f32) -> f32 {
    YSORT_BASE + world_z * YSORT_SPAN
}

/// **La regla**: z de dibujo de un asset según su capa y el z de mundo de su PIE.
///
/// `Floor`/`Basement` son planos → banda fija, sin Y-sort (dos pisos pintados uno
/// al lado del otro no pelean entre sí). `Wall`/`Object` Y-sortean. `Roof` va a
/// la banda de arriba. `Light` es el glow, que `light2d` maneja en su propia
/// banda; acá se comporta como plano para que el quad no tape a nadie.
#[inline]
pub fn depth(layer: Option<ArtLayer>, foot_z: f32) -> f32 {
    match layer.unwrap_or(ArtLayer::Object) {
        // El desempate por z dentro de la banda plana es MICROSCÓPICO (nanómetros
        // de z): sólo evita el z-fighting entre dos decals que se solapan, sin
        // acercarse jamás a la banda del jugador.
        ArtLayer::Floor | ArtLayer::Light => FLOOR + foot_z * 1e-6,
        ArtLayer::Basement => BASEMENT + foot_z * 1e-6,
        ArtLayer::Wall | ArtLayer::Object => ysort(foot_z),
        ArtLayer::Roof => OVERHEAD,
    }
}

/// Marca un sprite de la banda [`OVERHEAD`] con la huella (en TILES) que cubre:
/// cuando el jugador entra ahí abajo, el sprite se desvanece a [`INSIDE_ALPHA`] y
/// podés verte a vos mismo. Es el mecanismo genérico del techo — lo usan tanto
/// los edificios generados como cualquier asset que el usuario marque `Roof` en
/// el editor de assets y plante desde el editor de mundo.
///
/// Con esto, una casa de varios pisos se arma como varios assets apilados: los
/// de abajo `Wall`/`Object` (Y-sorted, te tapan si estás detrás) y los de arriba
/// `Roof` (se van cuando estás debajo) — sin código por edificio.
#[derive(Component, Clone, Copy)]
pub struct Overhead {
    /// Esquina mínima de la huella, en tiles.
    pub min: (i64, i64),
    /// Tamaño de la huella, en tiles.
    pub size: (i64, i64),
}

/// Alpha de un `Overhead` con el jugador debajo. No es 0: se sigue leyendo la
/// silueta del techo, que es lo que te dice que estás adentro de algo.
pub const INSIDE_ALPHA: f32 = 0.30;

/// Margen (tiles) alrededor de la huella que ya cuenta como "adentro", para que
/// el fade arranque al cruzar la pared y no un paso después.
const INSIDE_MARGIN: i64 = 1;

pub struct LayersPlugin;

impl Plugin for LayersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, fade_overhead);
    }
}

/// Fade suave de los sprites [`Overhead`] según si el jugador está bajo su
/// huella. Corre sobre las pocas entidades marcadas (los techos vivos alrededor
/// del jugador), así que no cuesta nada.
fn fade_overhead(
    time: Res<Time>,
    player: Query<&Transform, With<crate::sim::Player>>,
    mut over: Query<(&Overhead, &mut Sprite), Without<crate::sim::Player>>,
) {
    let Ok(pt) = player.get_single() else { return };
    let tile = ha_shared::topdown::TILE_M;
    let ptx = (pt.translation.x / tile).floor() as i64;
    // Render y = −z: el z de mundo del jugador se recupera invirtiendo el signo.
    let ptz = (-pt.translation.y / tile).floor() as i64;
    let rate = (time.delta_secs() * 9.0).min(1.0);
    for (o, mut sprite) in &mut over {
        let inside = ptx >= o.min.0 - INSIDE_MARGIN
            && ptx < o.min.0 + o.size.0 + INSIDE_MARGIN
            && ptz >= o.min.1 - INSIDE_MARGIN
            && ptz < o.min.1 + o.size.1 + INSIDE_MARGIN;
        let target = if inside { INSIDE_ALPHA } else { 1.0 };
        let a = sprite.color.alpha();
        sprite.color.set_alpha(a + (target - a) * rate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_art_never_covers_the_player() {
        // El bug del agua: un tile plano al SUR del jugador (z mayor) no puede
        // ganarle en z por más lejos que esté.
        let player = ysort(0.0);
        for z in [1.0, 50.0, 500.0, 3_000.0] {
            assert!(
                depth(Some(ArtLayer::Floor), z) < player,
                "piso a z={z} tapa al jugador"
            );
        }
    }

    #[test]
    fn upright_art_still_y_sorts() {
        // Y lo que SÍ se levanta del suelo sigue tapándote desde el sur.
        let player = ysort(10.0);
        assert!(depth(Some(ArtLayer::Object), 11.0) > player);
        assert!(depth(Some(ArtLayer::Wall), 9.0) < player);
    }

    #[test]
    fn bands_do_not_overlap_anywhere_in_the_world() {
        // El mundo caminable llega a ±30 km del origen. En NINGÚN punto de él el
        // Y-sort puede caer bajo la banda de piso (se iría detrás del terreno) ni
        // alcanzar la de techos. Esto es lo que rompía el `0.001` de antes.
        let edge = ha_shared::cosmic_scale::EARTH_GAMEPLAY_RADIUS_M;
        assert!(ysort(edge) + HAIR < OVERHEAD, "el Y-sort se comió la banda de techos");
        assert!(ysort(-edge) - HAIR > FLOOR, "el Y-sort se comió la banda de piso");
        assert_eq!(depth(Some(ArtLayer::Roof), 999.0), OVERHEAD);
    }

    #[test]
    fn hair_bias_never_beats_the_next_row() {
        // El sesgo de "un pelo adelante" desempata contra el MISMO tile y pierde
        // contra el de adelante: si no, el jugador se dibujaría sobre la pared
        // que tiene delante en vez de detrás de ella.
        let z = 120.0;
        assert!(ysort(z) + HAIR > ysort(z));
        assert!(ysort(z) + HAIR < ysort(z + ha_shared::topdown::TILE_M));
    }

    #[test]
    fn missing_layer_behaves_like_before() {
        // Compat: un slot sin capa etiquetada sigue siendo un objeto Y-sorted.
        assert_eq!(depth(None, 12.5), ysort(12.5));
    }
}
