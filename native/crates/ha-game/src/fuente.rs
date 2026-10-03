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
            .add_systems(PostUpdate, aplicar)
            // Después del layout: necesita el tamaño REAL del botón ya resuelto.
            .add_systems(
                PostUpdate,
                encoger_texto_de_botones.after(bevy::ui::UiSystem::Layout),
            );
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

// ---------------------------------------------------------------------------
// Que la letra no se salga del botón
// ---------------------------------------------------------------------------

/// Marca un nodo que funciona como botón sin ser un `Button` de Bevy (los
/// discos táctiles de SALTAR/USAR), para que su texto también se ajuste.
#[derive(Component)]
pub struct BotonConTexto;

/// El tamaño que pidió quien armó el texto y el que le dejó el ajuste. Si
/// alguien escribe otro `font_size` encima, ése pasa a ser el pedido nuevo.
#[derive(Component, Clone, Copy, Debug)]
pub struct TamanoPedido {
    pub pedido: f32,
    aplicado: f32,
}

/// Interlineado que usa Bevy por defecto (`LineHeight::RelativeToFont(1.2)`).
const INTERLINEA: f32 = 1.2;
/// Por debajo de esto ya no se lee: preferible que roce el borde.
const LETRA_MINIMA: f32 = 7.0;

/// Ancho de una línea de texto a `px` de tamaño, medido con la fuente que
/// embarcamos (avance de cada glifo + kerning), en las mismas unidades que el
/// `font_size` de Bevy (px por em).
pub fn ancho_linea(t: &str, px: f32) -> f32 {
    use ab_glyph::{Font, FontRef};
    use std::sync::OnceLock;
    static FUENTE: OnceLock<FontRef<'static>> = OnceLock::new();
    let f = FUENTE.get_or_init(|| {
        FontRef::try_from_slice(TTF).expect("assets/fonts/ui.ttf no es una fuente valida")
    });
    let em = f.units_per_em().unwrap_or(1000.0).max(1.0);
    let mut ancho = 0.0;
    let mut previo = None;
    for c in t.chars() {
        let id = f.glyph_id(c);
        if let Some(p) = previo {
            ancho += f.kern_unscaled(p, id);
        }
        ancho += f.h_advance_unscaled(id);
        previo = Some(id);
    }
    ancho / em * px
}

/// Cuántas líneas ocupa `t` envuelto por palabras en `ancho` (como lo envuelve
/// Bevy), y el ancho de la línea más larga. Una palabra que no entra sola
/// cuenta igual como línea (y hace que el ancho máximo supere `ancho`).
fn envolver(t: &str, px: f32, ancho: f32) -> (usize, f32) {
    let espacio = ancho_linea(" ", px);
    let (mut lineas, mut mas_larga) = (0usize, 0.0f32);
    for parrafo in t.split('\n') {
        let mut actual = 0.0f32;
        let mut vacia = true;
        for palabra in parrafo.split_whitespace() {
            let w = ancho_linea(palabra, px);
            if vacia {
                actual = w;
                vacia = false;
            } else if actual + espacio + w <= ancho {
                actual += espacio + w;
            } else {
                mas_larga = mas_larga.max(actual);
                lineas += 1;
                actual = w;
            }
        }
        mas_larga = mas_larga.max(actual);
        lineas += 1;
    }
    (lineas, mas_larga)
}

/// ¿Entra `t` a `px` en una caja de `ancho × alto`? En un botón redondo el
/// espacio útil es la cuerda del círculo a la altura del bloque de texto, no
/// el cuadrado entero: un SALTAR que "entra" en el cuadrado se sale del disco.
fn entra(t: &str, px: f32, ancho: f32, alto: f32, redondo: bool) -> bool {
    // Margen por la diferencia entre esta medida y la del motor de texto de
    // Bevy (redondeos, kerning por GPOS): sin él, un texto que el motor dibuja
    // justo adentro se achicaría medio punto por nada.
    let ancho = ancho * 1.03 + 0.5;
    let linea = px * INTERLINEA;
    if !redondo {
        let (n, w) = envolver(t, px, ancho);
        return w <= ancho && n as f32 * linea <= alto + 0.5;
    }
    let d = ancho.min(alto);
    let mut util = d;
    // Dos pasadas: el ancho útil depende de cuántas líneas hay, y al revés.
    for _ in 0..2 {
        let (n, _) = envolver(t, px, util);
        let h = n as f32 * linea;
        if h >= d {
            return false;
        }
        util = (d * d - h * h).sqrt();
    }
    let (n, w) = envolver(t, px, util);
    w <= util && n as f32 * linea < d
}

/// El tamaño de letra más grande, hasta `pedido`, con el que `t` entra.
pub fn tamano_que_entra(t: &str, pedido: f32, ancho: f32, alto: f32, redondo: bool) -> f32 {
    let mut px = pedido;
    while px > LETRA_MINIMA {
        if entra(t, px, ancho, alto, redondo) {
            return px;
        }
        px -= 0.5;
    }
    LETRA_MINIMA.min(pedido)
}

/// **Las letras de un botón no se salen del botón.** Cada texto hijo directo de
/// un `Button` (o de un [`BotonConTexto`]) se achica hasta que entra en el
/// interior del botón —descontados borde y relleno, y en los redondos la
/// cuerda del disco—. Mide con la fuente real, así cubre el inglés y las
/// palabras que cambian en vivo (SALTAR → TREPAR → IZARSE, USAR → VESTIR).
///
/// Sólo ACHICA respecto de lo que pidió quien armó el texto; nunca agranda por
/// encima. Y sólo trabaja cuando cambió el texto, la letra o el tamaño del
/// botón: en régimen no mide nada, y no toca `Node` (biblia §10).
fn encoger_texto_de_botones(
    mut commands: Commands,
    mut textos: Query<(Entity, Ref<Text>, &Parent, &mut TextFont, Option<&mut TamanoPedido>)>,
    botones: Query<(Ref<ComputedNode>, &Node), Or<(With<Button>, With<BotonConTexto>)>>,
) {
    for (e, text, padre, mut tf, ajuste) in &mut textos {
        let Ok((caja, nodo)) = botones.get(padre.get()) else { continue };
        // Un botón de ancho automático crece con su texto: nunca se desborda,
        // y "ajustarlo" sería perseguirse la cola (la letra achica el botón, el
        // botón achica la letra...). Sólo los que tienen ancho propio.
        if nodo.width == Val::Auto && nodo.max_width == Val::Auto {
            continue;
        }
        let sin_ajuste = ajuste.is_none();
        if !(text.is_changed() || caja.is_changed() || tf.is_changed() || sin_ajuste) {
            continue;
        }
        let inv = caja.inverse_scale_factor();
        let size = caja.size() * inv;
        let inset = caja.content_inset();
        let ancho = size.x - (inset.left + inset.right) * inv;
        let alto = size.y - (inset.top + inset.bottom) * inv;
        if ancho < 4.0 || alto < 4.0 || text.0.trim().is_empty() {
            continue; // escondido o todavía sin layout
        }
        let radio = caja.border_radius().top_left * inv;
        let redondo = radio * 2.0 >= size.x.min(size.y) * 0.95;

        let pedido = match &ajuste {
            Some(a) if (a.aplicado - tf.font_size).abs() < 0.01 => a.pedido,
            _ => tf.font_size,
        };
        let px = tamano_que_entra(&text.0, pedido, ancho, alto, redondo);
        if (tf.font_size - px).abs() > 0.01 {
            tf.font_size = px;
        }
        let nuevo = TamanoPedido { pedido, aplicado: px };
        match ajuste {
            Some(mut a) => {
                if a.pedido != nuevo.pedido || a.aplicado != nuevo.aplicado {
                    *a = nuevo;
                }
            }
            None => {
                commands.entity(e).insert(nuevo);
            }
        }
    }
}

#[cfg(test)]
mod tests_que_entra {
    use super::*;

    #[test]
    fn la_medida_crece_con_el_texto_y_con_la_letra() {
        let a = ancho_linea("SALTAR", 12.0);
        assert!(a > 20.0 && a < 80.0, "SALTAR a 12 px mide {a}");
        assert!(ancho_linea("SALTAR", 24.0) > a * 1.9);
        assert!(ancho_linea("SALTARES", 12.0) > a);
    }

    /// El caso del reporte: el disco de SALTAR en el teléfono (~46 px lógicos)
    /// con la letra mínima vieja (11) se salía por los dos lados.
    #[test]
    fn saltar_entra_en_el_disco_del_telefono() {
        let d = 46.0 - 2.0 * 3.0;
        let px = tamano_que_entra("SALTAR", 18.0, d, d, true);
        assert!(px >= LETRA_MINIMA);
        assert!(entra("SALTAR", px, d, d, true));
        let linea = px * INTERLINEA;
        let cuerda = (d * d - linea * linea).sqrt();
        // (con el mismo margen de medida que usa `entra`)
        assert!(
            ancho_linea("SALTAR", px) <= cuerda * 1.03 + 0.5,
            "{px} px no entra en la cuerda {cuerda}"
        );
    }

    #[test]
    fn nunca_agranda_lo_que_ya_entra() {
        assert_eq!(tamano_que_entra("OK", 12.0, 200.0, 40.0, false), 12.0);
    }

    #[test]
    fn las_frases_largas_se_envuelven_antes_de_achicar() {
        // Dos palabras que juntas no entran pero por separado sí: dos líneas,
        // sin achicar, si el alto da.
        let px = 14.0;
        let w = ancho_linea("FABRICAR", px).max(ancho_linea("TODO", px)) + 1.0;
        assert_eq!(tamano_que_entra("FABRICAR TODO", px, w, px * INTERLINEA * 2.0 + 1.0, false), px);
    }
}
