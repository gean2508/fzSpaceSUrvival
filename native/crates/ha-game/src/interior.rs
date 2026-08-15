//! **El encierro es un gradiente, no un interruptor.**
//!
//! Éste es el módulo que hace que entrar en una cueva se sienta entrar. No dibuja
//! nada: calcula **un número**, [`Interior::encierro`], que vale 0 a cielo abierto
//! y 1 enterrado sin ver la luz, y deja que el resto del juego reaccione a él.
//!
//! ```text
//!   boca                     pasillo                       cámara
//!   ┃ encierro 0 ─────────────────────────────────────────► 1
//!   ┃ ░░░░░░░▒▒▒▒▒▒▒▒▓▓▓▓▓▓▓▓████████████████████████████████
//!   ┃  luz del día        se apaga            sólo tu antorcha
//!   ▲
//!   la luz entra por acá (`luz_de_boca`)
//! ```
//!
//! ## Por qué un número y no un modo "estás en una cueva"
//!
//! Un modo hay que ENTRAR y SALIR de él, y esos dos bordes son exactamente donde
//! el jugador ve el corte. El sistema viejo tenía ese modo: al bajar se apagaba el
//! día entero de golpe, y eso —más que el fundido— era lo que se leía como cambiar
//! de escena. Se borró en agosto y no vuelve.
//!
//! Un número no tiene bordes: se mide a cada paso, así que la oscuridad entra
//! caminando, del brazo de lo que la causa. Y como es **distancia a la abertura**
//! y no "¿es una cueva?", vale igual para la boca natural, para el pozo que
//! cavaste y para el sótano de tu casa. Un solo vocabulario para las tres
//! (biblia §4: un marco, no tres).
//!
//! ## Lo que mueve
//!
//! - **El velo** (`light2d::darkness`): adentrarse cierra la noche encima tuyo, y
//!   la antorcha vuelve a ser lo que decide si ves. Es el mecanismo que ya
//!   existía para la noche, sin uno nuevo.
//! - **La luz de la boca** ([`luz_de_boca`]): el día se DERRAMA por la abertura.
//!   Una sola luz, movida y no respawneada, con la intensidad del sol de arriba:
//!   por eso la entrada de una cueva está clara a mediodía y negra a la noche, sin
//!   ninguna regla que diga eso en ningún lado.

use bevy::prelude::*;

use ha_shared::topdown::{Tile, TILE_M};

use crate::light2d::Light2d;
use crate::scene::{RegionRuntime, WorldContextRuntime};
use crate::sim::{Player, SimState};
use crate::world2d::{
    active_surface_body, render_xy, surface_region_active, ActivePlanet, TileWorld,
};

/// Hasta esta distancia de la abertura (m) todavía es "estar en la entrada": la
/// luz de afuera manda y no se oscurece nada. Poco más de un tramo de escalera,
/// para que bajar los cuatro escalones no te deje ya a oscuras.
const CERCA_M: f32 = 5.0;

/// Y a partir de ésta ya no llega nada de fuera. Dieciséis metros: lo bastante
/// como para que la penumbra dure varios pasos y se lea como un gradiente, no
/// como un umbral.
const LEJOS_M: f32 = 16.0;

/// Radio de búsqueda de aberturas, en tiles. Más allá de [`LEJOS_M`] el resultado
/// ya está saturado en 1, así que buscar más lejos no cambiaría nada.
const RADIO_BUSQUEDA: i64 = 20;

/// Cuánto tarda el encierro en alcanzar su valor (1/s). Bastante rápido como para
/// acompañar el paso, bastante lento como para que un pozo que aparece detrás de
/// una esquina no encienda la pantalla de golpe.
const SEGUIMIENTO: f32 = 2.5;

/// Radio del charco de luz que entra por la boca, en metros. Generoso: es luz de
/// día colándose por un agujero, no una vela.
const RADIO_BOCA: f32 = 11.0;

/// **Techo del encierro dentro de una casa.** Una habitación con paredes y tapa
/// no es una galería a treinta metros de la boca: por las juntas, la ventana y la
/// propia puerta entra bastante más luz. Sin este tope el fondo de un cuarto
/// grande quedaba tan negro como una mina, y lo que el jugador construyó dejaba
/// de verse justo cuando entraba a verlo.
const TECHO_INTERIOR: f32 = 0.62;

/// **El encierro de un cuarto, a `d` metros de su puerta más cercana.**
///
/// Es la misma curva que la boca de la cueva ([`CERCA_M`]…[`LEJOS_M`]), con el
/// tope de [`TECHO_INTERIOR`]. Vive aparte del sistema para poder probar la
/// propiedad que de verdad importa: que **no tenga escalones**. Un salto en esta
/// curva es, literalmente, el corte de escena que el juego no hace.
pub fn encierro_de_cuarto(d: f32) -> f32 {
    ((d - CERCA_M) / (LEJOS_M - CERCA_M)).clamp(0.0, 1.0) * TECHO_INTERIOR
}

#[derive(Resource, Default)]
pub struct Interior {
    /// 0 = a cielo abierto · 1 = enterrado, sin línea con la superficie.
    pub encierro: f32,
    /// La abertura al cielo más cercana, en celdas. Es de donde entra la luz.
    pub abertura: Option<(i64, i64)>,
}

/// Marca la única luz de boca que existe. Se mueve, no se respawnea: crear y
/// destruir una entidad por frame mientras caminás es justo la clase de churn que
/// se nota en el Redmi.
#[derive(Component)]
struct LuzDeBoca;

pub struct InteriorPlugin;

impl Plugin for InteriorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Interior>().add_systems(
            Update,
            (
                // Después de `caminar_tramo`: la luz de la escalera se calcula con
                // el progreso de ESTE frame (`Subida::tramo`), no con el del
                // anterior. Un frame de retraso en un gradiente que dura cuatro
                // escalones no se ve, pero el que se ve es el del cambio de planta
                // —que también lo publica ese sistema—, y ahí sí importa.
                medir_encierro.after(crate::escaleras::caminar_tramo),
                luz_de_boca.after(medir_encierro),
            )
                .run_if(crate::game_phase::world_active),
        );
    }
}

/// **¿Esta celda deja pasar el cielo?** Las tres formas que tiene el mundo de
/// estar abierto por arriba, y las tres son la misma cosa vista de tres maneras:
///
/// - [`Tile::Air`] en superficie: el pozo que cavaste.
/// - [`Tile::Stairwell`]: el hueco de una escalera — la boca de una cueva, o la
///   que baja al sótano de tu casa.
///
/// Que sea la MISMA pregunta para las tres es lo que hace que no haya un sistema
/// de cuevas y otro de sótanos: hay aberturas, y la luz entra por ellas.
fn abierta_al_cielo(tiles: &TileWorld, body: ha_shared::CelestialBody, tx: i64, tz: i64) -> bool {
    matches!(tiles.tile_at_level(body, 0, tx, tz), Tile::Air | Tile::Stairwell)
}

/// La abertura más cercana al jugador, buscando en anillos desde su celda. Corta
/// en cuanto encuentra una: el primer anillo que da resultado ya es el más
/// cercano, así que en la boca de la cueva el bucle da dos vueltas y sale.
fn abertura_cercana(
    tiles: &TileWorld,
    body: ha_shared::CelestialBody,
    tx: i64,
    tz: i64,
) -> Option<(i64, i64)> {
    // La boca natural se sabe sin buscar: el generador la calcula (`cuevas.rs`).
    // Se mete como candidata inicial para acotar cuánto hay que barrer.
    let natural = ha_shared::cuevas::boca_mas_cercana(body, tx, tz);
    let tope = natural
        .map(|(bx, bz)| (bx - tx).abs().max((bz - tz).abs()).min(RADIO_BUSQUEDA))
        .unwrap_or(RADIO_BUSQUEDA);
    for r in 0..=tope {
        let mut mejor: Option<(i64, i64)> = None;
        for dz in -r..=r {
            for dx in -r..=r {
                if r > 0 && dx.abs() != r && dz.abs() != r {
                    continue; // ya lo miró el anillo de adentro
                }
                let (cx, cz) = (tx + dx, tz + dz);
                if abierta_al_cielo(tiles, body, cx, cz) {
                    mejor = Some((cx, cz));
                    break;
                }
            }
            if mejor.is_some() {
                break;
            }
        }
        if mejor.is_some() {
            return mejor;
        }
    }
    natural
}

/// **La sombra del hueco de escalera.** Cuánto llega a oscurecerse el tramo en su
/// punto medio, para un cambio de planta que va del cielo abierto a la tierra.
///
/// No es un fundido a negro ni una cortina (§8: una transición no tapa la
/// pantalla): es que **un hueco de escalera está oscuro**, que es verdad y por eso
/// se puede dibujar. Lo que compra es que el cambio de `tiles.z` —que es discreto
/// y siempre lo va a ser— cae dentro de esta sombra en vez de a plena luz.
const SOMBRA_TRAMO: f32 = 0.92;

/// **Cuánto pesa la sombra del tramo, según dónde estés dentro de él.**
///
/// Es la curva compartida de la transición ([`ha_shared::escaleras::meseta_del_tramo`]):
/// la misma que desvanece el DIBUJO de la planta (`world2d::LayerFade::tramo`).
/// Que sean la misma y no dos parecidas es el punto — así el instante en que el
/// mundo está más oscuro, el más borrado y el del cambio de `tiles.z` son
/// exactamente el mismo instante, y no tres que hay que mantener alineados a mano.
fn sombra_del_tramo(s: f32) -> f32 {
    ha_shared::escaleras::meseta_del_tramo(s)
}

/// Mide el encierro este frame y lo persigue con un lerp.
#[allow(clippy::too_many_arguments)]
fn medir_encierro(
    time: Res<Time>,
    region: Res<RegionRuntime>,
    ctx: Res<WorldContextRuntime>,
    planet: Res<ActivePlanet>,
    tiles: Res<TileWorld>,
    subida: Res<crate::escaleras::Subida>,
    cuartos: Res<crate::cuartos::Cuartos>,
    ceilings: Res<crate::ceilings::BuiltCeilings>,
    roofs: Res<crate::building::BuiltRoofs>,
    mut interior: ResMut<Interior>,
    player: Query<&SimState, With<Player>>,
) {
    let seguir = |actual: f32, objetivo: f32, dt: f32, ritmo: f32| {
        actual + (objetivo - actual) * (dt * ritmo).min(1.0)
    };
    let dt = time.delta_secs();

    if surface_region_active(&region).is_none() {
        interior.encierro = seguir(interior.encierro, 0.0, dt, SEGUIMIENTO);
        interior.abertura = None;
        return;
    }
    let Ok(sim) = player.get_single() else {
        return;
    };
    let body = active_surface_body(&ctx, &planet);
    let tx = (sim.body.x / TILE_M).floor() as i64;
    let tz = (sim.body.z / TILE_M).floor() as i64;

    // **En la escalera la luz la manda el TRAMO, no la planta.**
    //
    // Ésta es la diferencia entre subir una escalera y que te teletransporten. El
    // cambio de planta es y va a seguir siendo discreto —`tiles.z` es un índice—,
    // así que lo que tiene que ser continuo es todo lo demás: la elevación del
    // personaje ya lo era (`escaleras::elevacion`) y ahora también la luz.
    //
    // Se mide el encierro de las DOS plantas que une el tramo y se cruza entre
    // ellas con el paso: bajando, el overworld se apaga a lo largo de los cuatro
    // escalones; subiendo, se enciende. Encima va la sombra del hueco
    // ([`sombra_del_tramo`]), que es lo que esconde el instante del cambio.
    if let Some((t, s)) = subida.tramo {
        let abajo = objetivo_de_nivel(
            &tiles, &cuartos, &ceilings, &roofs, body, t.z, tx, tz, sim,
        );
        let arriba = objetivo_de_nivel(
            &tiles, &cuartos, &ceilings, &roofs, body, t.z + 1, tx, tz, sim,
        );
        // La sombra se escala por el SALTO entre las dos plantas: una escalera
        // que va del salón al primer piso de tu casa no tiene por qué oscurecerse
        // (no hay nada que disimular, las dos plantas se ven igual), y una que
        // baja del pasto a la mina sí. Un solo número decide las dos cosas.
        let salto = (arriba.0 - abajo.0).abs();
        let cruce = abajo.0 + (arriba.0 - abajo.0) * s;
        let objetivo = cruce.max(SOMBRA_TRAMO * salto * sombra_del_tramo(s));
        // Más rápido que el lerp de siempre: acá el objetivo YA es una función
        // continua de dónde estás parado, así que perseguirlo despacio sólo
        // añadiría retraso entre el pie y la luz.
        interior.encierro = seguir(interior.encierro, objetivo, dt, SEGUIMIENTO * 3.6);
        interior.abertura = if s >= 0.5 { arriba.1 } else { abajo.1 };
        return;
    }

    // **Adentro de tu casa también estás adentro.**
    //
    // Antes esto devolvía 0 en cuanto `z >= 0`: bajo tierra había interior y en
    // la superficie no, así que la casa que el jugador levanta —con sus cuatro
    // paredes, su puerta y su techo— se iluminaba como un descampado. Pero el
    // encierro nunca fue "¿estoy bajo tierra?": es **cuánta luz de afuera me
    // llega**, y a un cuarto techado le llega por la puerta igual que a una
    // galería por su boca.
    //
    // Por eso el cuarto entra por el MISMO número y no por un modo "interior":
    // un modo tiene dos bordes —entrar y salir— y esos bordes son justo donde se
    // ve el corte (§4). Acá cruzar la puerta no dispara nada; simplemente estás
    // más cerca o más lejos de la abertura, y la luz acompaña al paso. Sin
    // pantalla de carga, sin fundido y sin cambiar de escena, que es la
    // continuidad que el interior tiene que tener.
    //
    // Techado y con puerta, las dos: un recinto SIN tapa tiene el cielo encima y
    // apagarlo sería mentir; uno sin puerta no es un cuarto (`cuartos.rs`).
    let (objetivo, abertura) =
        objetivo_de_nivel(&tiles, &cuartos, &ceilings, &roofs, body, tiles.z, tx, tz, sim);
    interior.encierro = seguir(interior.encierro, objetivo, dt, SEGUIMIENTO);
    interior.abertura = abertura;
}

/// **El encierro que te tocaría estando parado acá, en la planta `z`.**
///
/// Es el cuerpo de [`medir_encierro`] con el nivel como PARÁMETRO en vez de leerlo
/// de `tiles.z`, y existe por la escalera: para cruzar la luz entre dos plantas
/// hay que poder preguntar por la que todavía no pisás. Sin lerp ni estado — eso
/// lo pone quien llama.
#[allow(clippy::too_many_arguments)]
fn objetivo_de_nivel(
    tiles: &TileWorld,
    cuartos: &crate::cuartos::Cuartos,
    ceilings: &crate::ceilings::BuiltCeilings,
    roofs: &crate::building::BuiltRoofs,
    body: ha_shared::CelestialBody,
    z: i32,
    tx: i64,
    tz: i64,
    sim: &SimState,
) -> (f32, Option<(i64, i64)>) {
    if z >= 0 {
        let dentro = cuartos
            .en(body, z, tx, tz)
            .filter(|_| crate::ceilings::covered(ceilings, roofs, tiles, body, z, tx, tz));
        let objetivo = match dentro {
            None => 0.0,
            Some(cuarto) => {
                // La abertura de un cuarto es su puerta más cercana. Misma cuenta
                // que la boca de la cueva, mismo gradiente, mismo vocabulario.
                let mas_cerca = cuarto.puertas.iter().map(|&(bx, bz, eje)| {
                    let (cx, cz) = ha_shared::build_grid::edge_center(bx, bz, eje);
                    let (dx, dz) = (cx * TILE_M - sim.body.x, cz * TILE_M - sim.body.z);
                    (dx * dx + dz * dz).sqrt()
                });
                encierro_de_cuarto(mas_cerca.fold(f32::INFINITY, f32::min))
            }
        };
        let abertura = dentro.and_then(|c| {
            c.puertas.first().map(|&(bx, bz, eje)| {
                let (cx, cz) = ha_shared::build_grid::edge_center(bx, bz, eje);
                (cx.floor() as i64, cz.floor() as i64)
            })
        });
        return (objetivo, abertura);
    }

    let abertura = abertura_cercana(tiles, body, tx, tz);
    let objetivo = match abertura {
        None => 1.0, // sin línea con la superficie: negro
        Some((bx, bz)) => {
            let dx = (bx as f32 + 0.5) * TILE_M - sim.body.x;
            let dz = (bz as f32 + 0.5) * TILE_M - sim.body.z;
            let d = (dx * dx + dz * dz).sqrt();
            // Y cada nivel que bajás cuenta como alejarte otro tanto: un pozo
            // hondo es oscuro aunque estés justo debajo del agujero.
            let hondo = (-z - 1) as f32 * ha_shared::escaleras::ALTURA_PISO_M;
            crate::universe::smootherstep(CERCA_M, LEJOS_M, d + hondo)
        }
    };
    (objetivo, abertura)
}

/// **La luz que entra por la boca.** Una sola entidad, que aparece cuando hay
/// abertura cerca y se apaga cuando no.
///
/// Su intensidad es la del día de arriba, así que la entrada de una cueva está
/// clara a mediodía y negra a las tres de la mañana **sin ninguna regla que diga
/// eso**: es la misma luz del sol, colándose por un agujero. Lo mismo hace que
/// una tormenta oscurezca la boca (`weather::daylight_mul` ya entra en
/// `DayClock::daylight`).
fn luz_de_boca(
    mut commands: Commands,
    clock: Res<crate::sky2d::DayClock>,
    interior: Res<Interior>,
    mut luz: Query<(Entity, &mut Transform, &mut Light2d), With<LuzDeBoca>>,
) {
    let objetivo = interior.abertura.filter(|_| interior.encierro > 0.01);
    match (objetivo, luz.get_single_mut()) {
        (Some((bx, bz)), Ok((_, mut tf, mut l))) => {
            let p = render_xy((bx as f32 + 0.5) * TILE_M, (bz as f32 + 0.5) * TILE_M);
            tf.translation = Vec3::new(p.x, p.y, tf.translation.z);
            l.intensity = clock.daylight() * 1.6;
        }
        (Some((bx, bz)), Err(_)) => {
            let p = render_xy((bx as f32 + 0.5) * TILE_M, (bz as f32 + 0.5) * TILE_M);
            commands.spawn((
                LuzDeBoca,
                Transform::from_xyz(p.x, p.y, 0.0),
                GlobalTransform::default(),
                Visibility::Visible,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                Light2d {
                    // Blanco frío tirando a azul: es cielo, no una llama. Que se
                    // distinga de la antorcha a simple vista es media lectura de
                    // "por acá se sale".
                    color: Color::srgb(0.82, 0.90, 1.0),
                    radius: RADIO_BOCA,
                    intensity: clock.daylight() * 1.6,
                    flicker: 0.0,
                    day: true,
                    draw_glow: false,
                    ..default()
                },
            ));
        }
        (None, Ok((e, _, _))) => {
            commands.entity(e).despawn_recursive();
        }
        (None, Err(_)) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **En la puerta se ve como afuera.** Si cruzar el umbral ya te dejara a
    /// oscuras, el gradiente no sería un gradiente: sería el interruptor que este
    /// módulo existe para no tener.
    #[test]
    fn en_la_puerta_no_hay_encierro() {
        assert_eq!(encierro_de_cuarto(0.0), 0.0);
        assert_eq!(encierro_de_cuarto(CERCA_M), 0.0);
    }

    /// **La curva no tiene escalones.** Se camina de la puerta al fondo en pasos
    /// de 10 cm y ningún paso puede cambiar el encierro de golpe: un salto acá es
    /// el corte de escena que el juego no hace (§4).
    #[test]
    fn del_umbral_al_fondo_no_hay_ningun_salto() {
        let paso = 0.1;
        let mut anterior = encierro_de_cuarto(0.0);
        let mut d = 0.0;
        while d < LEJOS_M * 2.0 {
            d += paso;
            let e = encierro_de_cuarto(d);
            assert!(
                (e - anterior).abs() < 0.02,
                "salto de {:.3} a {d:.1} m de la puerta",
                (e - anterior).abs()
            );
            anterior = e;
        }
    }

    /// **El cambio de planta cae en el punto oscuro del tramo.**
    ///
    /// Es la invariante de la que depende que bajar por la grada deje de leerse
    /// como un teletransporte. `caminar_tramo` toca `tiles.z` en exactamente dos
    /// valores de `s` —`CIMA` subiendo, `PIE` bajando— y la sombra del hueco tiene
    /// que estar en su máximo en los dos, no en el medio ni en uno solo.
    #[test]
    fn la_sombra_tapa_los_dos_puntos_donde_cambia_la_planta() {
        use ha_shared::escaleras::{CIMA, PIE};
        assert!(sombra_del_tramo(PIE) > 0.99, "subiendo se ve el corte");
        assert!(sombra_del_tramo(CIMA) > 0.99, "bajando se ve el corte");
        // Y en medio del tramo también, claro: la meseta es continua.
        assert!(sombra_del_tramo((PIE + CIMA) * 0.5) > 0.99);
    }

    /// **Y las puntas quedan limpias.** Si la sombra no volviera a cero antes del
    /// rellano, salir de la escalera te dejaría el mundo apagado un rato: la queja
    /// sería la contraria pero el fallo, el mismo.
    #[test]
    fn el_tramo_abre_y_cierra_en_sus_puntas() {
        assert_eq!(sombra_del_tramo(0.0), 0.0, "el pie del tramo se ve normal");
        assert_eq!(sombra_del_tramo(1.0), 0.0, "y la cima también");
    }

    /// **Andar el tramo no da ningún escalón de luz.** Se recorre entero en pasos
    /// de un centésimo y ninguno puede mover la sombra de golpe — es el mismo test
    /// que `del_umbral_al_fondo_no_hay_ningun_salto`, aplicado a la escalera, que
    /// es donde el usuario vio el corte.
    ///
    /// El tope es 0.08 y no los 0.02 del cuarto porque acá la rampa es a propósito
    /// más corta: la del cuarto se reparte en once metros de pasillo y ésta tiene
    /// que abrir y cerrar dentro de cuatro escalones. Lo que se prueba es que sea
    /// una CURVA —una smootherstep no puede pasar de `1.875·h/w` por paso— y no un
    /// escalón, que daría 1.0 de una.
    #[test]
    fn la_sombra_del_tramo_es_un_gradiente() {
        let mut anterior = sombra_del_tramo(0.0);
        for i in 1..=100 {
            let e = sombra_del_tramo(i as f32 / 100.0);
            assert!(
                (e - anterior).abs() < 0.08,
                "salto de {:.3} en s={:.2}",
                (e - anterior).abs(),
                i as f32 / 100.0
            );
            anterior = e;
        }
    }

    /// Adentrarse oscurece, y sólo hasta el tope: un cuarto techado no es una
    /// mina: por la puerta y las juntas sigue entrando luz.
    #[test]
    fn adentrarse_oscurece_pero_no_hasta_la_mina() {
        assert!(encierro_de_cuarto(LEJOS_M) > encierro_de_cuarto(CERCA_M + 1.0));
        assert_eq!(encierro_de_cuarto(LEJOS_M), TECHO_INTERIOR);
        // Y por lejos que esté el fondo, nunca llega al negro de la galería.
        assert!(encierro_de_cuarto(1_000.0) < 1.0);
    }
}
