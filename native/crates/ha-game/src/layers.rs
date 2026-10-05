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
//! | [`FACHADA_BASE`] | 10.0 + z·span | la cara EXTERIOR de un edificio |
//! | [`OBRA`] | 19.0 | fantasma de construcción y corchetes del cursor |
//! | [`GLOBO`] | 20.0 | globos de diálogo |
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

/// **Un pelo por encima de [`FLOOR`], para arte plano que se dibuja SOBRE otro
/// arte plano.** Sigue muy por debajo de [`YSORT_MIN`], así que nada de esto
/// puede tapar a nadie: es el desempate dentro de la banda de piso.
///
/// Lo estrena la boca y la poza de una cascada. Las dos son líquido acostado —van
/// a `FLOOR` como el agua del río, y así tiene que ser (§"el suelo no tapa al
/// jugador")— sólo que caen EN celdas que además dibujan su propio tile de agua,
/// y las dos pedían `FLOOR` exacto. Dos sprites con el mismo z no tienen orden:
/// hoy la poza gana porque `spawn_chunk_cascadas` corre después del bucle de
/// tiles del chunk, y eso deja de ser cierto en cuanto la poza cruza a la celda
/// del chunk de al lado — el reventón parpadeaba debajo del agua según de qué
/// lado de la frontera cayera. Con un desempate explícito el orden ya no depende
/// de en qué orden se spawnearon.
pub const SOBRE_EL_PISO: f32 = 0.02;

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

/// **El PISO de la banda Y-sort**: por debajo de esto, un sprite es arte PLANO
/// —agua, asfalto, la pintura de la calzada, una alfombra, un decal del editor—
/// y por encima es algo que se levanta del suelo.
///
/// Existe porque "¿esto es plano?" hacía falta fuera del dibujo: la SOMBRA es lo
/// que delata a un piso que se cree objeto. `attach_ground_shadows` le colgaba
/// sombra a todo sprite de un tile de alto, así que la línea divisoria de la
/// calzada proyectaba la suya sobre el asfalto — una raya pintada con sombra, y
/// justo al lado el asfalto (que es el mismo suelo) sin ninguna. Se veía como si
/// la pintura flotara medio metro sobre la calle.
///
/// El número sale de la escalera, no del ojo: la banda Y-sort son los 60 km del
/// mundo repartidos en ±1.5 alrededor de [`YSORT_BASE`], o sea `[2.0, 5.0]`, y
/// debajo quedan [`FLOOR`] (1.0), [`BASEMENT`] y [`GROUND`].
pub const YSORT_MIN: f32 = YSORT_BASE - 1.5;

/// Sesgo de "un pelo adelante/atrás" DENTRO de la banda Y-sort, en unidades de
/// z. Vale ~¼ de tile, así que empata contra el mismo tile pero nunca le gana a
/// la fila de adelante. Los offsets a ojo que había (`+0.0005`, `−0.1`, `+1.0`)
/// estaban en metros de mundo por accidente — `+1.0` era **un kilómetro** de
/// desempate — y con la escala nueva habrían tapado el mundo entero.
pub const HAIR: f32 = YSORT_SPAN * 0.25;

/// Techos y pisos superiores: por encima de todo el mundo Y-sorted. Se
/// transparentan con [`Overhead`].
pub const OVERHEAD: f32 = 6.0;

/// **CENTRO de la banda de las caras EXTERIORES de un edificio.**
///
/// Un techo es la TAPA de la casa y la fachada es su CARA: en cenital 3/4 la
/// cara está más cerca de la cámara que la tapa, siempre. Con la fachada en la
/// banda Y-sort (`[2.0, 5.0]`) el tumbado —que va a [`OVERHEAD`] y cubre
/// exactamente las celdas sobre las que la pared del sur se dibuja, porque el
/// sprite crece hacia el norte— la tapaba entera: **una casa techada, mirada
/// desde la vereda, era una losa de tumbado y nada más**. Pasaba con `wall/wood`
/// de 2 m igual que con una fachada de 3; no lo notó nadie porque hasta ahora la
/// cara de afuera se veía como un interior dado vuelta y no había qué mirar.
///
/// Sigue Y-sorteando DENTRO de la banda (mismo [`YSORT_SPAN`]) para que dos
/// fachadas que se solapan —dos casas a menos de tres tiles— no se peleen. Los
/// 60 km del mundo entran en ±1.5 → `[8.5, 11.5]`.
pub const FACHADA_BASE: f32 = 10.0;

/// Fantasma de obra y corchetes del cursor: por encima de las fachadas, porque
/// levantar un muro de sur a norte escondía el fantasma detrás de lo ya puesto.
pub const OBRA: f32 = 19.0;

/// Globos de diálogo: por encima de todo lo del mundo, fachadas incluidas. Un
/// globo tapado no se lee, y eso vale contra un árbol y contra una casa.
pub const GLOBO: f32 = 20.0;

/// Y-sort de un sprite parado en `world_z` (más al sur = más adelante).
#[inline]
pub fn ysort(world_z: f32) -> f32 {
    YSORT_BASE + world_z * YSORT_SPAN
}

/// Cuánto más al NORTE se ordena el relieve (canto + cara de un escalón) que su
/// propia celda, en metros de mundo.
///
/// El relieve Y-sortea por la celda del CANTO, y su dibujo cuelga hacia el SUR
/// (la cara de roca cae sobre la terraza de abajo). Lo único plano que cae al sur
/// del pie de un objeto es su SOMBRA, y con el canto ordenado en su fila, la losa
/// de pasto de una terraza más abajo le sacaba un trozo en seco a la sombra del
/// árbol de arriba (captura del usuario, 3 oct 2026).
///
/// Correrlo atrás no puede esconder a nadie: todo lo que se para crece hacia el
/// NORTE desde su pie, así que algo al norte del canto nunca pisa la cara, y lo
/// que está al sur ya le ganaba y le sigue ganando. Dos escalones apilados se
/// corren igual, así que entre ellos no cambia nada. 16 m cubre la sombra más
/// larga del amanecer.
pub const RELIEVE_ATRAS_M: f32 = 16.0;

/// Y-sort del RELIEVE (canto y cara de un escalón) cuyo canto está en `world_z`.
/// Ver [`RELIEVE_ATRAS_M`].
#[inline]
pub fn relieve(world_z: f32) -> f32 {
    ysort(world_z - RELIEVE_ATRAS_M)
}

/// Y-sort de una cara EXTERIOR parada en `world_z`, en la banda [`FACHADA_BASE`].
#[inline]
pub fn fachada(world_z: f32) -> f32 {
    FACHADA_BASE + world_z * YSORT_SPAN
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

/// **¿Este asset ES suelo?** Lo plano (`Floor`) y el glow (`Light`) no se
/// levantan de la baldosa: son parte de ella. Se usa para decidir qué puede ser
/// objetivo del dedo — un charco o una alfombra no se "levantan", y si fueran
/// tocables le robarían el toque a la celda que hay debajo (cavar y colocar
/// dejarían de funcionar sobre cualquier baldosa decorada).
#[inline]
pub fn es_piso(layer: Option<ArtLayer>) -> bool {
    matches!(layer, Some(ArtLayer::Floor) | Some(ArtLayer::Light))
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
    /// Margen (en tiles) alrededor de la huella que ya cuenta como "adentro".
    ///
    /// Los TECHOS quieren 1: el fundido tiene que arrancar al cruzar la pared y
    /// no un paso después. Las PAREDES quieren 0, y por lo contrario: con margen,
    /// pasar caminando por la vereda de una casa la volvía translúcida desde
    /// afuera. La diferencia no es un detalle de gusto — es que un techo lo
    /// cruzás y una pared la rodeás.
    pub margen: i64,
    /// Margen HORIZONTAL, en tiles. Por defecto es [`Self::margen`].
    ///
    /// Existe por las fachadas. Un muro se funde por columna, y con paredes de 2 m
    /// eso alcanzaba: la que te tapa se abre y las vecinas no molestan. Con una
    /// cara exterior de 4 m no alcanza — parado adentro se te abre la columna que
    /// tenés encima y las de al lado siguen opacas, así que el cuarto se lee **en
    /// franjas**, como mirando por una ranura. El frente de una casa se abre
    /// entero o no se abre.
    ///
    /// Es un eje aparte y no el `margen` de siempre porque el vertical tiene que
    /// seguir en 0: con margen en z, pasar por la vereda volvería translúcida la
    /// casa desde afuera, que es el bug que `Overhead::pared` vino a arreglar.
    pub margen_x: i64,
    /// Alpha al que va este sprite con el jugador debajo. Por defecto
    /// [`INSIDE_ALPHA`]; las caras exteriores usan [`FACHADA_ALPHA`].
    ///
    /// Existe porque las capas se MULTIPLICAN. Adentro de una casa, la franja
    /// sur tiene encima el tumbado *y* la fachada: dos veces 0,30 no dan 0,30,
    /// dan `1 − 0.7²` ≈ 0,51 de opacidad, y el interior se ve turbio justo en la
    /// fila donde uno entra. Con la fachada más transparente que el tumbado el
    /// producto vuelve a rondar el 0,3 de siempre.
    pub alpha_dentro: f32,
    /// **La línea de pared sobre la que está parado este sprite** (z de su pie),
    /// si es parte de una casa. Con ella el frente se abre ENTERO al entrar —como
    /// en Project Zomboid—: estando dentro de un recinto (cuarto cerrado o
    /// edificio), se funde todo sprite parado sobre una pared de ESE recinto,
    /// no sólo el que tenés encima. Sin esto la planta baja se abría y los pisos
    /// de arriba seguían opacos (`CASATEST`/`FACHADATEST`, 27 sep 2026).
    pub pie: Option<i64>,
}

impl Overhead {
    /// Huella de TECHO: se abre un paso antes de que llegues.
    pub fn techo(min: (i64, i64), size: (i64, i64)) -> Self {
        Self { min, size, margen: 1, margen_x: 1, alpha_dentro: INSIDE_ALPHA, pie: None }
    }

    /// Huella de PARED: sólo se abre si de verdad estás debajo de lo que tapa.
    pub fn pared(min: (i64, i64), size: (i64, i64)) -> Self {
        Self { min, size, margen: 0, margen_x: 0, alpha_dentro: INSIDE_ALPHA, pie: None }
    }

    /// Esta huella es la cara EXTERIOR de un edificio: se dibuja por encima del
    /// tumbado, así que adentro tiene que borrarse más que él **y a lo ancho**
    /// (ver [`Self::margen_x`]).
    /// Marca el sprite como parte de una casa, parado sobre la línea `z`.
    pub fn con_pie(mut self, z: i64) -> Self {
        self.pie = Some(z);
        self
    }

    pub fn exterior(mut self) -> Self {
        self.alpha_dentro = FACHADA_ALPHA;
        self.margen_x = FRENTE_QUE_SE_ABRE;
        self
    }
}

/// Cuántas columnas a cada lado de la que te tapa se abren con ella, en una cara
/// exterior. Cuatro es el frente de una casa chica: alcanza para que el cuarto se
/// lea entero y no tanto como para abrir la casa del vecino.
pub const FRENTE_QUE_SE_ABRE: i64 = 4;

/// Alpha de un `Overhead` con el jugador debajo. No es 0: se sigue leyendo la
/// silueta del techo, que es lo que te dice que estás adentro de algo.
pub const INSIDE_ALPHA: f32 = 0.30;

/// Alpha de una cara EXTERIOR con el jugador adentro. Más bajo que
/// [`INSIDE_ALPHA`] porque se apila sobre el tumbado (ver `Overhead::alpha_dentro`):
/// `1 − (1−0.30)·(1−0.14)` ≈ 0,40, que sigue dejando leer el interior.
pub const FACHADA_ALPHA: f32 = 0.14;

pub struct LayersPlugin;

impl Plugin for LayersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, fade_overhead);
    }
}

/// Fade suave de los sprites [`Overhead`] según si el jugador está bajo su
/// huella. Corre sobre las pocas entidades marcadas (los techos vivos alrededor
/// del jugador), así que no cuesta nada.
#[allow(clippy::too_many_arguments)]
fn fade_overhead(
    time: Res<Time>,
    player: Query<&Transform, With<crate::sim::Player>>,
    cuartos: Option<Res<crate::cuartos::Cuartos>>,
    edificios: Option<Res<crate::edificios::Edificios>>,
    tiles: Option<Res<crate::world2d::TileWorld>>,
    ctx: Option<Res<crate::scene::WorldContextRuntime>>,
    planet: Option<Res<crate::world2d::ActivePlanet>>,
    mut over: Query<(&Overhead, &mut Sprite), Without<crate::sim::Player>>,
) {
    let Ok(pt) = player.get_single() else { return };
    let tile = ha_shared::topdown::TILE_M;
    let ptx = (pt.translation.x / tile).floor() as i64;
    // Render y = −z: el z de mundo del jugador se recupera invirtiendo el signo.
    let ptz = (-pt.translation.y / tile).floor() as i64;
    let rate = (time.delta_secs() * 9.0).min(1.0);

    // El recinto donde está parado el jugador: caja (x0, z0, x1, z1) inclusiva.
    let recinto = (|| {
        let (Some(tiles), Some(ctx), Some(planet)) = (tiles.as_ref(), ctx.as_ref(), planet.as_ref()) else {
            return None;
        };
        let body = crate::world2d::active_surface_body(ctx, planet);
        let z = tiles.z;
        if let Some(c) = cuartos.as_ref().and_then(|c| c.en(body, z, ptx, ptz)) {
            let (mut x0, mut z0, mut x1, mut z1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
            for &(x, zz) in &c.celdas {
                x0 = x0.min(x);
                z0 = z0.min(zz);
                x1 = x1.max(x);
                z1 = z1.max(zz);
            }
            return Some((x0, z0, x1, z1));
        }
        let eds = edificios.as_ref()?;
        let i = eds.en(body as u8, z, ptx, ptz)?;
        let (mut x0, mut z0, mut x1, mut z1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
        for c in eds.list[i].ed.cuartos() {
            x0 = x0.min(c.x0);
            z0 = z0.min(c.z0);
            x1 = x1.max(c.x0 + c.w - 1);
            z1 = z1.max(c.z0 + c.h - 1);
        }
        Some((x0, z0, x1, z1))
    })();

    for (o, mut sprite) in &mut over {
        let debajo = ptx >= o.min.0 - o.margen_x
            && ptx < o.min.0 + o.size.0 + o.margen_x
            && ptz >= o.min.1 - o.margen
            && ptz < o.min.1 + o.size.1 + o.margen;
        // ¿Está parado sobre una pared del recinto donde estás? (las líneas de
        // pared van de z0, el borde norte, a z1+1, el borde sur)
        let de_tu_casa = match (o.pie, recinto) {
            (Some(pie), Some((x0, z0, x1, z1))) => {
                pie >= z0 && pie <= z1 + 1 && o.min.0 <= x1 + 1 && o.min.0 + o.size.0 >= x0
            }
            _ => false,
        };
        let target = if debajo || de_tu_casa { o.alpha_dentro } else { 1.0 };
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

        // **Ningún techo puede taparle la cara a su propia casa.** La fachada más
        // al norte del mundo sigue estando por delante del tumbado más al sur.
        assert!(OVERHEAD < fachada(-edge), "el techo se comió la banda de fachadas");
        // Y el fantasma y el globo se ven siempre, esté donde esté la casa.
        assert!(fachada(edge) < OBRA, "una fachada tapa el fantasma de obra");
        assert!(OBRA < GLOBO, "el fantasma tapa el globo de diálogo");
    }

    /// **Una fachada Y-sortea entre fachadas.** Dos casas vecinas se solapan por
    /// el sprite alto del frente: si la banda fuera plana, cuál va delante lo
    /// decidiría el orden de spawn y titilaría al mover la cámara.
    #[test]
    fn dos_fachadas_vecinas_se_ordenan_entre_si() {
        let tile = ha_shared::topdown::TILE_M;
        assert!(fachada(10.0 * tile) < fachada(13.0 * tile));
        assert!(fachada(10.0 * tile) > fachada(7.0 * tile));
    }

    /// **La cara exterior se borra más que el tumbado, porque se apila sobre él.**
    /// Dos capas al 0,30 dan 0,51 de opacidad compuesta y el interior queda turbio.
    #[test]
    fn la_fachada_no_enturbia_el_interior() {
        let compuesta = |a: f32, b: f32| 1.0 - (1.0 - a) * (1.0 - b);
        let con_fachada = compuesta(INSIDE_ALPHA, Overhead::pared((0, 0), (1, 1)).exterior().alpha_dentro);
        let dos_tumbados = compuesta(INSIDE_ALPHA, INSIDE_ALPHA);
        assert!(
            con_fachada < dos_tumbados,
            "tumbado + fachada ({con_fachada}) tapa más que dos tumbados ({dos_tumbados})"
        );
        // Y sigue leyéndose la silueta: no es invisible.
        assert!(con_fachada > INSIDE_ALPHA);
        assert_eq!(Overhead::pared((0, 0), (1, 1)).alpha_dentro, INSIDE_ALPHA);
        assert_eq!(Overhead::techo((0, 0), (1, 1)).margen, 1);
    }

    /// **El frente de una casa se abre entero, pero sólo desde adentro.**
    ///
    /// Las dos mitades de la regla, y las dos vienen de un bug visto:
    /// a lo ANCHO tiene que abrirse varias columnas, porque con una fachada de 4 m
    /// abrir sólo la que te tapa deja el cuarto leyéndose en franjas; a lo LARGO
    /// tiene que seguir en 0, porque con margen en z pasar por la vereda volvía
    /// translúcida la casa desde afuera.
    #[test]
    fn el_frente_se_abre_a_lo_ancho_y_no_desde_la_vereda() {
        let f = Overhead::pared((10, 6), (1, 4)).exterior();
        assert_eq!(f.margen_x, FRENTE_QUE_SE_ABRE);
        assert_eq!(f.margen, 0, "con margen en z la casa se abre desde la calle");
        // Una pared común no cambió de comportamiento.
        let p = Overhead::pared((10, 6), (1, 2));
        assert_eq!((p.margen_x, p.margen), (0, 0));
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
