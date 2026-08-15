//! **La fuente de la UI.** Un solo sitio, aplicada a todo el texto del juego.
//!
//! Antes el juego usaba la fuente que Bevy trae por defecto (`default_font`), que
//! es un subconjunto minúsculo: no dibuja acentos, ni `¿`, ni `·`, ni flechas.
//! De ahí salía la regla "UI ASCII-only" — que no era una decisión de diseño sino
//! un parche: se escribía "Presiona ACCION" para no ver una caja donde iba la
//! `Ó`. El arreglo de verdad es traer una fuente con los glifos, y esta es.
//!
//! ## Por qué se parchea `TextFont` y no se pasa la fuente en cada sitio
//!
//! Hay unos cincuenta `TextFont { .. }` repartidos por el HUD, el menú, las
//! fichas de construir, la telemetría y la creación de personaje. Pasarles la
//! fuente a mano es cincuenta sitios que hay que acordarse de tocar, y el
//! cincuenta y uno que alguien agregue mañana vuelve a salir con la fuente
//! pelada. En vez de eso, un sistema mira los `TextFont` **recién agregados** y
//! le pone la nuestra a los que quedaron con la de fábrica.
//!
//! Cuesta nada: el filtro `Added` no recorre el texto que ya existe, así que en
//! régimen el sistema no ve ninguna entidad. Y no toca `Node`, que es lo que
//! dispara el relayout caro de taffy (biblia §10).

use bevy::prelude::*;
use bevy::text::TextFont;

/// La fuente de la UI, ya cargada. Se resuelve una vez en el arranque.
#[derive(Resource)]
pub struct FuenteUi(pub Handle<Font>);

pub struct FuentePlugin;

impl Plugin for FuentePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, cargar)
            // `PostUpdate` y no `Update`: así agarra el texto que spawneó
            // cualquier sistema de este frame, sin importar el orden entre ellos.
            .add_systems(PostUpdate, aplicar);
    }
}

fn cargar(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(FuenteUi(server.load("fonts/ui.ttf")));
}

/// Los mismos bytes que carga el `AssetServer`, embebidos para poder
/// PREGUNTARLE a la fuente qué sabe dibujar sin esperar a que cargue el asset.
static TTF: &[u8] = include_bytes!("../assets/fonts/ui.ttf");

/// ¿La fuente que embarcamos sabe dibujar este carácter?
///
/// Es el reemplazo de la vieja regla "UI ASCII-only". Preguntar por la fuente en
/// vez de por el rango ASCII es mejor en las dos direcciones: `á` pasa (la fuente
/// la tiene) y `▲` no (no la tiene), mientras que la regla vieja se equivocaba en
/// ambos casos. El set se arma una sola vez.
pub fn dibujable(c: char) -> bool {
    use ab_glyph::{Font, FontRef};
    use std::collections::HashSet;
    use std::sync::OnceLock;
    static GLIFOS: OnceLock<HashSet<char>> = OnceLock::new();
    let glifos = GLIFOS.get_or_init(|| {
        let f = FontRef::try_from_slice(TTF).expect("assets/fonts/ui.ttf no es una fuente valida");
        f.codepoint_ids().map(|(_, c)| c).collect()
    });
    // El espacio y los saltos de línea no tienen por qué tener glifo propio.
    c.is_whitespace() || glifos.contains(&c)
}

/// Cambia por `?` sólo lo que la fuente NO puede dibujar. Antes esto tiraba todo
/// lo no-ASCII, así que una frase de NPC bien escrita salía como "Est? cansado".
pub fn solo_dibujable(t: &str) -> String {
    t.chars().map(|c| if dibujable(c) { c } else { '?' }).collect()
}

fn aplicar(fuente: Option<Res<FuenteUi>>, mut nuevos: Query<&mut TextFont, Added<TextFont>>) {
    let Some(fuente) = fuente else { return };
    for mut tf in &mut nuevos {
        // Sólo los que no eligieron una fuente a propósito: `Handle::default()`
        // es la de fábrica de Bevy, la que dibuja cajas.
        if tf.font == Handle::default() {
            tf.font = fuente.0.clone();
        }
    }
}
