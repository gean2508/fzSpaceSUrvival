//! **La disposición de los teclados en pantalla: QWERTY.**
//!
//! Los cuatro teclados del juego (nombre del personaje, nombre del mundo,
//! bautizo de animales y consola) iban en orden alfabético, y buscar cada letra
//! en un orden que nadie usa es lento: los dedos ya saben dónde está la Q. Cada
//! panel sigue armando sus propias teclas (cada uno necesita extras distintos),
//! pero las letras salen de acá para que los cuatro coincidan.
//!
//! ## Mayúsculas como en el celular
//!
//! Los teclados que escriben nombres en castellano no duplican las filas en
//! mayúscula y minúscula (no entran QWERTY dos veces en pantalla). Hacen lo que
//! hace el teclado del teléfono: la primera letra de cada palabra sale en
//! mayúscula sola y el resto en minúscula; MAYÚS invierte la letra que viene.
//! Las teclas muestran la letra tal como la van a escribir.

use bevy::prelude::*;

/// La fila de los números, arriba de las letras como en cualquier teclado.
pub const NUMEROS: &str = "1234567890";

/// Las tres filas de letras del teclado español (con la Ñ donde va).
pub const LETRAS: [&str; 3] = ["QWERTYUIOP", "ASDFGHJKLÑ", "ZXCVBNM"];

/// Las vocales con tilde: en el teclado físico salen con la tecla muerta `´`;
/// acá van a la derecha de la última fila, que es la más corta.
pub const ACENTOS: &str = "ÁÉÍÓÚ";

/// Las filas completas de un teclado para nombres en castellano: números, las
/// tres de letras y los acentos pegados a la última.
pub fn filas_castellano() -> [String; 4] {
    [
        NUMEROS.to_string(),
        LETRAS[0].to_string(),
        LETRAS[1].to_string(),
        format!("{}{}", LETRAS[2], ACENTOS),
    ]
}

/// ¿La próxima letra sale en mayúscula? Sí al empezar el texto o una palabra
/// nueva, salvo que MAYÚS esté invirtiendo.
pub fn va_mayuscula(texto: &str, invertir: bool) -> bool {
    let empieza_palabra = texto.is_empty() || texto.ends_with(' ');
    empieza_palabra != invertir
}

/// La letra de la tecla (que se guarda en mayúscula) en el caso que toca.
/// Lo que no es letra sale igual.
pub fn en_caso(c: char, mayuscula: bool) -> char {
    let convertida = if mayuscula { c.to_uppercase().next() } else { c.to_lowercase().next() };
    convertida.unwrap_or(c)
}

/// Reescribe la etiqueta de cada tecla de letra para que muestre el caso que va
/// a escribir. `letra` dice qué letra lleva la tecla (o `None` si no es letra).
///
/// La etiqueta es el `Text` nieto del botón (botón → caja centrada → texto, ver
/// `ui_pack::boton`); se busca por ahí en vez de marcarla porque el botón lo
/// arma `ui_pack` y no sabe de teclados.
pub fn rotular<T: Component>(
    teclas: &Query<(&T, &Children)>,
    hijos: &Query<&Children>,
    textos: &mut Query<&mut Text>,
    letra: impl Fn(&T) -> Option<char>,
    mayuscula: bool,
) {
    for (tecla, hijos_boton) in teclas {
        let Some(c) = letra(tecla).filter(|c| c.is_alphabetic()) else {
            continue;
        };
        let etiqueta = en_caso(c, mayuscula).to_string();
        for caja in hijos_boton.iter() {
            let Ok(nietos) = hijos.get(*caja) else { continue };
            for n in nietos.iter() {
                if let Ok(mut t) = textos.get_mut(*n) {
                    if t.0 != etiqueta {
                        t.0.clone_from(&etiqueta);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn las_filas_son_qwerty_y_tienen_todo_el_abecedario() {
        assert!(LETRAS[0].starts_with("QWERTY"));
        let todas: String = LETRAS.concat();
        for c in "ABCDEFGHIJKLMNÑOPQRSTUVWXYZ".chars() {
            assert!(todas.contains(c), "falta la {c}");
        }
        assert_eq!(todas.chars().count(), 27, "ninguna letra repetida");
    }

    #[test]
    fn mayuscula_al_empezar_cada_palabra() {
        assert!(va_mayuscula("", false));
        assert!(!va_mayuscula("M", false));
        assert!(va_mayuscula("Doña ", false));
        // MAYÚS invierte lo automático, para un lado y para el otro.
        assert!(va_mayuscula("Mc", true));
        assert!(!va_mayuscula("", true));
        assert_eq!(en_caso('Ñ', false), 'ñ');
        assert_eq!(en_caso('Á', false), 'á');
        assert_eq!(en_caso('7', false), '7');
    }

    /// La fuente del juego tiene que dibujar cada tecla (regla 6).
    #[test]
    fn la_fuente_dibuja_cada_tecla() {
        let todas: String = filas_castellano().concat();
        let minus: String = todas.chars().map(|c| en_caso(c, false)).collect();
        for c in todas.chars().chain(minus.chars()).chain("MAYÚS".chars()) {
            assert!(crate::fuente::dibujable(c), "la fuente no dibuja '{c}'");
        }
    }
}
