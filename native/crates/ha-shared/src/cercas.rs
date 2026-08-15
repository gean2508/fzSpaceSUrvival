//! **LA FORMA DE UNA CERCA — hacia dónde sigue, no de qué lado queda el relleno.**
//!
//! Una cerca es una LÍNEA y un suelo es un ÁREA, y por eso no comparten autotile.
//! `world2d::autotile_shape` contesta *"¿de qué lado se acaba el material?"* y
//! devuelve `edge_n`, `corner_ne`, `inner_sw`… Eso describe el borde de una
//! mancha de tierra. A una cerca hay que preguntarle otra cosa: **¿hacia dónde
//! sigue la línea?** — y la respuesta es una máscara de conexión (`ns`, `ew`,
//! `ne`, `nes`, `nesw`).
//!
//! Preguntarle la pregunta equivocada es lo que producía el bug que el usuario
//! reportó: `Tile::Fence` pedía `edge_*`, la cerca no tiene ninguna de esas poses,
//! y `tile_sprite` caía al recorte base — **el mismo dibujo horizontal en los
//! cuatro lados de la parcela**, o sea una valla horizontal estirada en los
//! tramos verticales.
//!
//! Los nombres que salen de acá son los que publica `scripts/import-cercas.py`.
//!
//! ## El sufijo de lado (`@w`, `@e`, `@n`, `@s`)
//!
//! El pack dibuja los dos tramos verticales por separado: uno con los postes
//! contra el borde IZQUIERDO de la celda y otro contra el DERECHO — son el lado
//! oeste y el lado este de un cerco. Lo mismo con los horizontales. Elegir mal no
//! es grave (son 16 px de corrimiento) pero se nota en una parcela: los postes
//! quedan del lado de adentro.
//!
//! Por eso [`forma`] recibe hacia dónde queda **el afuera** y agrega el sufijo. Si
//! no se sabe (una cerca suelta puesta desde el editor), se pide sin sufijo y el
//! juego cae a la pose sin lado, que siempre existe.

/// Las cuatro conexiones de una celda de cerca: ¿el vecino de ese lado también es
/// cerca?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vecinos {
    pub n: bool,
    pub e: bool,
    pub s: bool,
    pub w: bool,
}

impl Vecinos {
    pub fn new(n: bool, e: bool, s: bool, w: bool) -> Self {
        Self { n, e, s, w }
    }

    /// La máscara de conexión, en el orden canónico `nesw`.
    ///
    /// Vacía = poste suelto: la cerca no sigue para ningún lado.
    pub fn mascara(self) -> &'static str {
        match (self.n, self.e, self.s, self.w) {
            (true, true, true, true) => "nesw",
            (true, true, true, false) => "nes",
            (true, true, false, true) => "new",
            (true, false, true, true) => "nsw",
            (false, true, true, true) => "esw",
            (true, false, true, false) => "ns",
            (false, true, false, true) => "ew",
            (true, true, false, false) => "ne",
            (true, false, false, true) => "nw",
            (false, true, true, false) => "es",
            (false, false, true, true) => "sw",
            (true, false, false, false) => "n",
            (false, true, false, false) => "e",
            (false, false, true, false) => "s",
            (false, false, false, true) => "w",
            (false, false, false, false) => "solo",
        }
    }

    /// ¿Es un tramo recto? Sólo esos llevan sufijo de lado.
    fn tramo(self) -> Option<Eje> {
        match (self.n, self.e, self.s, self.w) {
            (true, false, true, false) => Some(Eje::Vertical),
            (false, true, false, true) => Some(Eje::Horizontal),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Eje {
    Vertical,
    Horizontal,
}

/// Hacia dónde queda **el afuera** de lo que la cerca encierra.
///
/// Es lo que decide de qué lado de su celda se dibujan los postes. En una parcela
/// residencial, el afuera del tramo oeste es el oeste.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Afuera {
    N,
    E,
    S,
    W,
}

impl Afuera {
    fn letra(self) -> char {
        match self {
            Afuera::N => 'n',
            Afuera::E => 'e',
            Afuera::S => 's',
            Afuera::W => 'w',
        }
    }
}

/// **La cadena de poses a pedirle al arte, de la más específica a la más general.**
///
/// Se devuelve una CADENA y no una sola pose porque no toda familia del pack
/// dibujó todas las formas: `Wall_1` no trae tes ni cruces, y `Fence_2` no trae
/// los codos con el brazo del lado contrario. Pidiendo en orden, una familia
/// incompleta cae a lo más parecido que sí tenga en vez de quedarse sin dibujar.
///
/// El orden es: forma con lado → forma sin lado → el tramo del eje que domina →
/// `solo`. Nunca se cae de un eje al otro: **antes un codo dibujado de tramo que
/// un tramo horizontal en una línea vertical**, que es el bug que esto viene a
/// arreglar.
pub fn cadena(v: Vecinos, afuera: Option<Afuera>) -> Vec<String> {
    let base = v.mascara();
    let mut out = Vec::with_capacity(4);
    if let (Some(_), Some(a)) = (v.tramo(), afuera) {
        out.push(format!("{base}@{}", a.letra()));
    }
    out.push(base.to_string());
    // Respaldo por EJE. Una forma que conecta arriba y abajo se parece más a un
    // tramo vertical que a cualquier otra cosa; una que conecta a los costados,
    // a uno horizontal. Con las dos o con ninguna no hay eje que mande y se cae
    // al poste suelto.
    let vertical = v.n || v.s;
    let horizontal = v.e || v.w;
    match (vertical, horizontal) {
        (true, false) => out.push("ns".into()),
        (false, true) => out.push("ew".into()),
        // Un codo o una te: el eje dominante es el que tiene las dos puntas.
        (true, true) => {
            if v.n && v.s {
                out.push("ns".into());
            } else if v.e && v.w {
                out.push("ew".into());
            }
        }
        (false, false) => {}
    }
    out.push("solo".into());
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **La regla entera, en un test**: un tramo vertical jamás pide una pose
    /// horizontal, ni siquiera de respaldo. Es el bug que reportó el usuario
    /// ("no pongas horizontales en vertical") y la única forma de que no vuelva
    /// es que la cadena no lo permita.
    #[test]
    fn un_tramo_vertical_nunca_cae_en_una_pose_horizontal() {
        let v = Vecinos::new(true, false, true, false);
        for pose in cadena(v, Some(Afuera::W)) {
            assert!(
                !pose.starts_with("ew"),
                "un tramo vertical pidió `{pose}`, que es horizontal"
            );
        }
        let h = Vecinos::new(false, true, false, true);
        for pose in cadena(h, Some(Afuera::S)) {
            assert!(
                !pose.starts_with("ns"),
                "un tramo horizontal pidió `{pose}`, que es vertical"
            );
        }
    }

    #[test]
    fn el_tramo_pide_primero_su_lado() {
        let v = Vecinos::new(true, false, true, false);
        assert_eq!(cadena(v, Some(Afuera::W))[0], "ns@w");
        assert_eq!(cadena(v, Some(Afuera::E))[0], "ns@e");
        // Sin saber el afuera no se inventa un lado.
        assert_eq!(cadena(v, None)[0], "ns");
    }

    /// Un codo no lleva sufijo de lado: el pack dibujó uno solo por esquina.
    #[test]
    fn un_codo_no_pide_lado() {
        let codo = Vecinos::new(true, true, false, false);
        assert_eq!(cadena(codo, Some(Afuera::W))[0], "ne");
    }

    /// Toda combinación de vecinos tiene nombre y toda cadena termina en algo que
    /// el importador publica siempre.
    #[test]
    fn las_dieciseis_combinaciones_tienen_forma() {
        let mut vistas = std::collections::HashSet::new();
        for m in 0..16u8 {
            let v = Vecinos::new(m & 1 != 0, m & 2 != 0, m & 4 != 0, m & 8 != 0);
            vistas.insert(v.mascara());
            let c = cadena(v, None);
            assert_eq!(c.last().map(String::as_str), Some("solo"));
        }
        assert_eq!(vistas.len(), 16, "dos vecindades distintas comparten nombre");
    }
}
