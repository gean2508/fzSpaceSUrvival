//! **El objetivo se marca EN EL MUNDO, no en el HUD.**
//!
//! De la propuesta de interfaz del 15 ago 2026: *"se marca el objetivo EN EL
//! MUNDO (elipse al pie y su nombre) y aparece un solo botón"*. Antes lo único
//! que decía qué estabas tocando era un renglón de texto arriba a la izquierda,
//! a media pantalla de distancia del árbol: había que mirar a un lado para saber
//! qué iba a pasar del otro.
//!
//! Son dos piezas y las dos van pegadas a la cosa:
//!
//! 1. **La elipse al pie.** Una sombra clara en el suelo, del ancho del sprite.
//!    Va debajo del objeto y encima del piso, así que no lo tapa — es la misma
//!    idea que la sombra de contacto del pixel art, pero encendida.
//! 2. **El nombre, arriba.** Chico, sobre la cabeza del objeto.
//!
//! ## Por qué una elipse y no un anillo alrededor del jugador
//!
//! Un anillo centrado en el jugador ya se probó y se prohibió: la vida se ancla
//! al mundo, no a la cámara. Esto es lo contrario y por eso vale — no dice "acá
//! estás vos", dice "esto es lo que vas a tocar".
//!
//! ## Por qué se reusan dos entidades y no se spawnea por cuadro
//!
//! El objetivo cambia con el dedo, o sea muchas veces por segundo. Spawnear y
//! despawnear un sprite y un texto a ese ritmo ensucia el árbol y dispara
//! relayouts; las dos entidades nacen una vez y después sólo se mueven y se
//! muestran (biblia §10).

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use ha_shared::InteractTargetKind;

use crate::targeting::LookTarget;

/// Marcador de las dos entidades del HUD de objetivo. Existe para que la query
/// de los objetos del mundo (`Without<Marcador>`) no se cruce con la que mueve
/// el marcador: sin el filtro, Bevy ve `&Transform` y `&mut Transform` sobre el
/// mismo conjunto y se planta al armar el sistema.
#[derive(Component)]
struct Marcador;

/// El alto de la elipse contra su ancho. Una sombra en 3/4 es un óvalo aplastado:
/// a 1.0 sería un círculo visto desde arriba, que es la proyección de otro juego.
const APLASTADO: f32 = 0.42;

/// Las dos entidades del marcador, creadas una sola vez.
#[derive(Resource)]
struct Marca {
    elipse: Entity,
    nombre: Entity,
}

pub struct ObjetivoPlugin;

impl Plugin for ObjetivoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, crear)
            .add_systems(Update, seguir.after(crate::input::InputSet::Gesture));
    }
}

fn crear(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let tex = images.add(elipse_tex());
    let elipse = commands
        .spawn((
            Sprite {
                image: tex,
                color: Color::srgba(1.0, 0.94, 0.55, 0.75),
                custom_size: Some(Vec2::new(1.0, APLASTADO)),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, crate::layers::YSORT_MIN),
            Visibility::Hidden,
            Marcador,
            Name::new("ObjetivoElipse"),
        ))
        .id();
    let nombre = commands
        .spawn((
            Text2d::new(""),
            TextFont { font_size: 15.0, ..default() },
            TextColor(Color::srgb(1.0, 0.97, 0.85)),
            Transform::from_xyz(0.0, 0.0, crate::layers::GLOBO),
            Visibility::Hidden,
            Marcador,
            Name::new("ObjetivoNombre"),
        ))
        .id();
    commands.insert_resource(Marca { elipse, nombre });
}

/// El nombre humano de lo que estás tocando. Castellano de verdad (regla 6): el
/// jugador lee "Roca", no `Rock`.
fn nombre_de(kind: InteractTargetKind) -> &'static str {
    match kind {
        InteractTargetKind::Rock => "Roca",
        InteractTargetKind::Tree => "Árbol",
        InteractTargetKind::Water => "Agua",
        InteractTargetKind::Entity => "Construcción",
        InteractTargetKind::Container => "Contenedor",
        InteractTargetKind::Chest => "Cofre",
        InteractTargetKind::OreIron => "Veta de hierro",
        InteractTargetKind::OreGold => "Veta de oro",
        InteractTargetKind::GemBlue => "Gema azul",
        InteractTargetKind::GemRed => "Gema roja",
        InteractTargetKind::Block => "Suelo",
        InteractTargetKind::Prop => "Objeto",
        InteractTargetKind::LaunchModule => "Módulo de lanzamiento",
        InteractTargetKind::LaunchIgnition => "Encendido",
        InteractTargetKind::SpaceReentry => "Reentrada",
        InteractTargetKind::SpaceTransitLuna => "Rumbo a la Luna",
        _ => "Objetivo",
    }
}

#[allow(clippy::type_complexity)]
fn seguir(
    objetivo: Res<LookTarget>,
    marca: Option<Res<Marca>>,
    objetos: Query<(&Transform, Option<&Sprite>), Without<Marcador>>,
    mut piezas: ParamSet<(
        Query<(&mut Transform, &mut Visibility), With<Marcador>>,
        Query<&mut Text2d, With<Marcador>>,
    )>,
) {
    let Some(marca) = marca else { return };
    let Some(kind) = objetivo.kind else {
        esconder(&mut piezas, &marca);
        return;
    };
    // El pie y el ancho salen del sprite del objeto cuando hay entidad; si el
    // objetivo es un tile (el suelo) no hay sprite y se usa el punto tocado.
    let (centro, ancho, alto) = match objetivo.entity.and_then(|e| objetos.get(e).ok()) {
        Some((tf, sprite)) => {
            let medida = sprite
                .and_then(|s| s.custom_size)
                .unwrap_or(Vec2::new(1.0, 1.0));
            (tf.translation.truncate(), medida.x, medida.y)
        }
        None => (objetivo.hit_point.truncate(), 1.0, 1.0),
    };
    let pie = centro.y - alto * 0.5;

    {
        let mut q = piezas.p0();
        if let Ok((mut tf, mut vis)) = q.get_mut(marca.elipse) {
            tf.translation.x = centro.x;
            tf.translation.y = pie;
            tf.scale = Vec3::new(ancho.max(0.4), 1.0, 1.0);
            *vis = Visibility::Visible;
        }
        if let Ok((mut tf, mut vis)) = q.get_mut(marca.nombre) {
            tf.translation.x = centro.x;
            tf.translation.y = centro.y + alto * 0.5 + 0.35;
            *vis = Visibility::Visible;
        }
    }
    if let Ok(mut t) = piezas.p1().get_mut(marca.nombre) {
        let quiero = nombre_de(kind);
        // Sólo se escribe cuando cambia: tocar un `Text2d` cada cuadro lo
        // remaquetea aunque diga lo mismo.
        if t.0 != quiero {
            t.0 = quiero.to_string();
        }
    }
}

#[allow(clippy::type_complexity)]
fn esconder(
    piezas: &mut ParamSet<(
        Query<(&mut Transform, &mut Visibility), With<Marcador>>,
        Query<&mut Text2d, With<Marcador>>,
    )>,
    marca: &Marca,
) {
    let mut q = piezas.p0();
    for e in [marca.elipse, marca.nombre] {
        if let Ok((_, mut vis)) = q.get_mut(e) {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        }
    }
}

/// La textura de la elipse: un óvalo relleno con el borde difuminado, para que
/// no se lea como una pastilla pegada sino como una luz en el piso.
fn elipse_tex() -> Image {
    const N: u32 = 64;
    let mut data = vec![0u8; (N * N * 4) as usize];
    let r = (N as f32 - 1.0) / 2.0;
    for y in 0..N {
        for x in 0..N {
            let dx = (x as f32 - r) / r;
            let dy = (y as f32 - r) / r;
            let d = (dx * dx + dy * dy).sqrt();
            // Anillo: fuerte en el borde, tenue adentro. Así marca el contorno
            // del pie sin pintar de amarillo lo que está apoyado.
            let a = if d > 1.0 {
                0.0
            } else if d > 0.72 {
                1.0 - ((d - 0.72) / 0.28).powi(2)
            } else {
                0.18
            };
            let i = ((y * N + x) * 4) as usize;
            data[i] = 255;
            data[i + 1] = 245;
            data[i + 2] = 190;
            data[i + 3] = (a * 255.0) as u8;
        }
    }
    Image::new(
        Extent3d { width: N, height: N, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Todo objetivo tocable tiene nombre en castellano.** El fallback existe,
    /// pero si una variante nueva cae ahí el jugador lee "Objetivo" y no sabe qué
    /// tiene bajo el dedo. Este test fija los que sí o sí tienen que estar.
    #[test]
    fn los_objetivos_de_siempre_tienen_nombre_propio() {
        for k in [
            InteractTargetKind::Rock,
            InteractTargetKind::Tree,
            InteractTargetKind::Water,
            InteractTargetKind::Chest,
            InteractTargetKind::Prop,
            InteractTargetKind::Block,
        ] {
            assert_ne!(nombre_de(k), "Objetivo", "{k:?} se quedó sin nombre propio");
        }
    }

    /// La elipse tiene que ser un ANILLO: si saliera rellena taparía lo que está
    /// apoyado en el suelo, que es justo lo que estás por tocar.
    #[test]
    fn la_elipse_es_un_anillo_y_no_una_mancha() {
        let img = elipse_tex();
        let n = 64usize;
        let alpha = |x: usize, y: usize| img.data[(y * n + x) * 4 + 3];
        let centro = alpha(n / 2, n / 2);
        let borde = alpha(n / 2, (n as f32 * 0.94) as usize - 1);
        assert!(
            (centro as u16) < (borde as u16),
            "el centro ({centro}) no es más tenue que el borde ({borde})"
        );
    }
}
