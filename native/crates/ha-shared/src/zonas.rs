//! **ZONAS** — vos repartís los barrios, el juego reparte lo que va dentro.
//!
//! El generador de ciudad ya sabe llenar un barrio: tiene distritos
//! ([`District`]) y un catálogo de parcelas con su tamaño y su peso
//! ([`crate::city::LOT_PROGRAMS`]), así que sabe que en el comercio van tiendas y
//! mercados y en la residencia casas y dúplex. Lo que NO sabía es dónde querés
//! vos que esté cada barrio: eso lo decidía un anillo alrededor del centro más un
//! sorteo, y por eso todas las ciudades salían con la misma cebolla.
//!
//! Una zona es **un rectángulo con un carácter**. Nada más:
//!
//! ```text
//!   ┌──────────────────── RESIDENCIAL ─────────────────────┐
//!   │                                                      │
//!   │   ┌──── COMERCIAL ────┐        ┌── PARQUE ──┐        │
//!   │   │  tiendas, mercado │        │  plaza,    │        │
//!   │   └───────────────────┘        │  arboleda  │        │
//!   │                                └────────────┘        │
//!   │            casas, dúplex, villas                     │
//!   └──────────────────────────────────────────────────────┘
//! ```
//!
//! ## Las sub-zonas salen gratis: gana la más chica
//!
//! No hay campo "padre" ni árbol que mantener. Si dibujás un rectángulo dentro de
//! otro, el de adentro es más chico y **gana** ([`Zonas::clase_en`]). Meter una
//! zona comercial dentro de la residencial es, literalmente, dibujarla adentro.
//!
//! Se eligió así a propósito: un árbol de zonas obliga a declarar la jerarquía al
//! crear cada una, y a mantenerla cuando movés un rectángulo. Con "gana la más
//! chica" la jerarquía **es** la geometría, que es lo que estás mirando mientras
//! dibujás, y arrastrar una zona no puede dejar el árbol mintiendo.
//!
//! ## Lo que NO hace
//!
//! Una zona no coloca nada. No estampa casas ni calles: le cambia el CARÁCTER a
//! las manzanas que cubre, y el generador de siempre hace el resto con su propio
//! sorteo. Por eso dos zonas residenciales del mismo tamaño no salen iguales, y
//! por eso pintar una zona sobre una ciudad ya generada no borra nada — reparte
//! distinto lo que ya sabía hacer.

use serde::{Deserialize, Serialize};

use crate::city::District;

/// Un rectángulo del mundo con un carácter asignado a mano.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Zona {
    /// Cómo la llamás vos ("Barrio Norte", "Puerto viejo"). Sólo para el editor:
    /// el juego no lo lee, pero perderlo haría imposible reconocer una zona en
    /// una lista de veinte rectángulos.
    #[serde(default)]
    pub nombre: String,
    pub body: u8,
    /// Esquina mínima, en tiles.
    pub min: (i64, i64),
    /// Tamaño en tiles. Siempre positivo — el editor normaliza el arrastre.
    pub size: (i64, i64),
    pub clase: ClaseZona,
}

impl Zona {
    pub fn contiene(&self, tx: i64, tz: i64) -> bool {
        tx >= self.min.0
            && tx < self.min.0 + self.size.0
            && tz >= self.min.1
            && tz < self.min.1 + self.size.1
    }

    /// Área en tiles. Es el criterio de desempate: gana la más chica.
    pub fn area(&self) -> i64 {
        self.size.0.max(0) * self.size.1.max(0)
    }
}

/// El carácter de una zona.
///
/// Es un espejo de [`District`] a propósito, y no el `District` a secas: éste es
/// **contenido autorado que se guarda en disco**, así que necesita nombre humano,
/// orden estable y serde propio. Que un `enum` interno del generador cambie de
/// variantes no puede romper el mapa que dibujaste.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClaseZona {
    Residencial,
    Comercial,
    Centro,
    Industrial,
    Parque,
    Civico,
}

impl ClaseZona {
    /// Todas, en el orden en que se muestran en el editor.
    pub const TODAS: [ClaseZona; 6] = [
        ClaseZona::Residencial,
        ClaseZona::Comercial,
        ClaseZona::Centro,
        ClaseZona::Industrial,
        ClaseZona::Parque,
        ClaseZona::Civico,
    ];

    /// Nombre humano. La lista se lee "Residencial", no `Residencial`.
    pub fn label(self) -> &'static str {
        match self {
            ClaseZona::Residencial => "Residencial",
            ClaseZona::Comercial => "Comercial",
            ClaseZona::Centro => "Centro",
            ClaseZona::Industrial => "Industrial",
            ClaseZona::Parque => "Parque",
            ClaseZona::Civico => "Cívico",
        }
    }

    /// Qué se construye ahí. Una frase, para que elegir no sea adivinar.
    pub fn ayuda(self) -> &'static str {
        match self {
            ClaseZona::Residencial => "Casas, dúplex, villas. Con patio, garaje y pileta.",
            ClaseZona::Comercial => "Tiendas, mercados, gasolineras, estacionamientos.",
            ClaseZona::Centro => "Torres, oficinas y hoteles. Manzanas macizas.",
            ClaseZona::Industrial => "Naves, depósitos, graneros.",
            ClaseZona::Parque => "Plazas, canchas y arboledas. El respiro.",
            ClaseZona::Civico => "Hospital, universidad, escuela, comisaría, estación.",
        }
    }

    /// Color con el que el editor la pinta (RGB 0..1). Distintos entre sí incluso
    /// en gris: se dibujan translúcidos y superpuestos.
    pub fn color(self) -> [f32; 3] {
        match self {
            ClaseZona::Residencial => [0.42, 0.72, 0.40],
            ClaseZona::Comercial => [0.30, 0.58, 0.90],
            ClaseZona::Centro => [0.85, 0.72, 0.25],
            ClaseZona::Industrial => [0.62, 0.40, 0.72],
            ClaseZona::Parque => [0.20, 0.80, 0.62],
            ClaseZona::Civico => [0.92, 0.42, 0.38],
        }
    }

    pub fn district(self) -> District {
        match self {
            ClaseZona::Residencial => District::Residential,
            ClaseZona::Comercial => District::Commercial,
            ClaseZona::Centro => District::Downtown,
            ClaseZona::Industrial => District::Industrial,
            ClaseZona::Parque => District::Park,
            ClaseZona::Civico => District::Civic,
        }
    }
}

/// Las zonas autoradas, ya indexadas para consultarlas por celda.
#[derive(Default, Clone, Debug)]
pub struct Zonas {
    list: Vec<Zona>,
}

impl Zonas {
    pub fn new(list: Vec<Zona>) -> Self {
        Self { list }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn list(&self) -> &[Zona] {
        &self.list
    }

    /// **El carácter de esta celda**, o `None` si no la cubre ninguna zona.
    ///
    /// Gana la zona más CHICA que la contenga: ésa es toda la regla de las
    /// sub-zonas (ver el encabezado del módulo). Con empate de área gana la
    /// última dibujada, que es la que acabás de poner.
    pub fn clase_en(&self, body: u8, tx: i64, tz: i64) -> Option<ClaseZona> {
        let mut mejor: Option<(i64, ClaseZona)> = None;
        for z in &self.list {
            if z.body != body || !z.contiene(tx, tz) {
                continue;
            }
            let a = z.area();
            if mejor.map(|(ba, _)| a <= ba).unwrap_or(true) {
                mejor = Some((a, z.clase));
            }
        }
        mejor.map(|(_, c)| c)
    }
}

// ---------------------------------------------------------------------------
// El registro global — cómo llegan las zonas al generador
// ---------------------------------------------------------------------------

/// **¿Hay alguna zona autorada?** Se consulta antes que nada.
///
/// El generador pregunta por el distrito de cada manzana, así que esta pregunta
/// está en el camino caliente. En un mundo sin zonas —el caso normal hasta que
/// abrís el editor— la respuesta es una carga atómica relajada y se acabó: ni
/// candado, ni recorrido, ni rama que prediga mal.
static HAY_ZONAS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static ZONAS: std::sync::RwLock<Option<Zonas>> = std::sync::RwLock::new(None);

/// Instala las zonas autoradas. Lo llama el juego al cargar `world_edits.json`, y
/// el editor cada vez que dibujás una (para que el lienzo muestre el efecto).
///
/// Global y no un parámetro porque el generador es un montón de funciones puras
/// que se llaman desde todas partes (`topdown_tile`, el mapa satelital, el disco
/// del planeta, el editor) y encadenarles un `&Zonas` a todas sería propagar un
/// argumento por veinte firmas para algo que, en una partida, no cambia nunca.
pub fn instalar(zonas: Zonas) {
    let hay = !zonas.is_empty();
    if let Ok(mut w) = ZONAS.write() {
        *w = Some(zonas);
    }
    HAY_ZONAS.store(hay, std::sync::atomic::Ordering::Relaxed);
}

/// El carácter autorado de una celda, si hay zonas y alguna la cubre.
pub fn clase_en(body: u8, tx: i64, tz: i64) -> Option<ClaseZona> {
    if !HAY_ZONAS.load(std::sync::atomic::Ordering::Relaxed) {
        return None;
    }
    ZONAS.read().ok()?.as_ref()?.clase_en(body, tx, tz)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z(nombre: &str, min: (i64, i64), size: (i64, i64), clase: ClaseZona) -> Zona {
        Zona { nombre: nombre.into(), body: 0, min, size, clase }
    }

    #[test]
    fn una_zona_cubre_su_rectangulo_y_nada_mas() {
        let zs = Zonas::new(vec![z("centro", (0, 0), (10, 10), ClaseZona::Centro)]);
        assert_eq!(zs.clase_en(0, 0, 0), Some(ClaseZona::Centro));
        assert_eq!(zs.clase_en(0, 9, 9), Some(ClaseZona::Centro));
        assert_eq!(zs.clase_en(0, 10, 0), None, "el borde derecho es exclusivo");
        assert_eq!(zs.clase_en(0, -1, 5), None);
        assert_eq!(zs.clase_en(1, 5, 5), None, "otro cuerpo, otro mundo");
    }

    /// **La sub-zona gana.** Es la regla entera de la jerarquía: dibujar adentro
    /// ES anidar. Si esto falla, meter un barrio comercial dentro del residencial
    /// deja de funcionar y hay que inventar un árbol de zonas.
    #[test]
    fn la_zona_mas_chica_gana() {
        let zs = Zonas::new(vec![
            z("barrio", (0, 0), (100, 100), ClaseZona::Residencial),
            z("galería", (40, 40), (10, 10), ClaseZona::Comercial),
        ]);
        assert_eq!(zs.clase_en(0, 5, 5), Some(ClaseZona::Residencial));
        assert_eq!(zs.clase_en(0, 45, 45), Some(ClaseZona::Comercial), "la de adentro manda");
        // Y a tres niveles, que es lo que se pidió: zona dentro de zona dentro de zona.
        let zs = Zonas::new(vec![
            z("ciudad", (0, 0), (500, 500), ClaseZona::Residencial),
            z("barrio", (100, 100), (100, 100), ClaseZona::Industrial),
            z("manzana", (140, 140), (12, 12), ClaseZona::Parque),
        ]);
        assert_eq!(zs.clase_en(0, 10, 10), Some(ClaseZona::Residencial));
        assert_eq!(zs.clase_en(0, 110, 110), Some(ClaseZona::Industrial));
        assert_eq!(zs.clase_en(0, 145, 145), Some(ClaseZona::Parque));
    }

    /// Con áreas iguales gana la última: acabás de dibujarla encima, querés verla.
    #[test]
    fn con_empate_gana_la_ultima_dibujada() {
        let zs = Zonas::new(vec![
            z("a", (0, 0), (10, 10), ClaseZona::Parque),
            z("b", (0, 0), (10, 10), ClaseZona::Industrial),
        ]);
        assert_eq!(zs.clase_en(0, 5, 5), Some(ClaseZona::Industrial));
    }

    /// Cada clase lleva a un distrito distinto del generador: si dos cayeran en el
    /// mismo, pintar dos zonas distintas daría el mismo barrio.
    #[test]
    fn cada_clase_es_un_distrito_distinto() {
        let mut vistos = std::collections::HashSet::new();
        for c in ClaseZona::TODAS {
            assert!(vistos.insert(c.district()), "{:?} repite distrito", c);
            assert!(!c.label().is_empty());
            assert!(!c.ayuda().is_empty(), "{:?} sin ayuda: elegir sería adivinar", c);
        }
    }

    /// Sin zonas instaladas el generador no paga nada: la consulta global sale por
    /// la carga atómica y ni siquiera toca el candado.
    #[test]
    fn sin_zonas_la_consulta_global_no_hace_nada() {
        // (El registro es global al proceso; este test sólo comprueba el camino
        // corto con el registro en su estado inicial o vacío.)
        instalar(Zonas::default());
        assert_eq!(clase_en(0, 0, 0), None);
    }
}
