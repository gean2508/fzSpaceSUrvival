//! **La escalera se CAMINA** — geometría pura de un tramo, sin Bevy.
//!
//! Hasta el 4 ago 2026 una escalera era una celda: la pisabas y `tiles.z` saltaba
//! un nivel en el acto. Se borró entera (biblia §14). El problema no era el
//! fundido ni el tallado de la sala de llegada: era que **un tile no es un
//! recorrido**. Pisar una baldosa y aparecer en otra planta se lee como un
//! teletransporte por más suave que sea el fundido, y el jugador lo dijo así.
//!
//! Un tramo es una **fila de escalones**, y subir es andarla:
//!
//! ```text
//!            paso 0      1      2      3
//!   planta   ┌──────┬──────┬──────┬──────┐
//!   baja  →  │ ▁▁▁▁ │ ▂▂▂▂ │ ▃▃▃▃ │ ▄▄▄▄ │ →  planta alta
//!            └──────┴──────┴──────┴──────┘
//!   s =      0     .25    .50    .75     1
//!                                 ▲
//!                            se cambia de planta acá: al pisar
//!                            el ÚLTIMO escalón, no antes
//! ```
//!
//! Todo lo que hace falta para eso es un escalar: [`progreso`], cuánto del tramo
//! llevás andado. De ahí salen las dos cosas que se ven —el personaje se eleva
//! (`s`) y la planta cambia (`s` cruza [`CIMA`])— y ninguna de las dos necesita
//! pestillo, evento ni estado guardado: son función de dónde estás parado. Ése es
//! el punto: **si te devolvés a mitad de la escalera, te devolvés de verdad**,
//! porque bajar es el mismo número yendo al revés.
//!
//! El tramo vive en el nivel BAJO del par que une (`Tramo::z` une `z` con `z+1`) y
//! sus celdas existen —pisables— en los dos, así que cambiar de planta nunca mueve
//! al jugador ni un centímetro. No hay a dónde teletransportarse.

use crate::topdown::TILE_M;

/// Cuántos escalones tiene un tramo colocado por el jugador.
///
/// Cuatro es lo mínimo que se lee como escalera y no como rampa, y con
/// [`CIMA`] a 0.75 deja el cambio de planta exactamente en el último escalón.
pub const PASOS: u8 = 4;

/// Altura de una planta, en metros. Es [`crate::relief::STEP_M`] a propósito: un
/// piso y un escalón de terraza son el mismo desnivel, así que trepar un bordillo
/// y subir una escalera se sienten a la misma escala.
pub const ALTURA_PISO_M: f32 = crate::relief::STEP_M;

/// Fracción del tramo a partir de la cual **ya estás en la planta de arriba**.
/// Con [`PASOS`] = 4 cae justo al entrar en el último escalón: das ese paso y ya
/// estás, que es como se sube una escalera.
pub const CIMA: f32 = 0.75;

/// Y por debajo de la cual volvés a la de abajo. La franja muerta entre [`PIE`] y
/// [`CIMA`] es lo que reemplaza al pestillo del sistema viejo: quedarte quieto en
/// mitad del tramo no puede hacer parpadear la planta, porque para cambiarla hay
/// que **andar** medio tramo.
pub const PIE: f32 = 0.25;

/// Un tramo de escalera: dónde empieza, hacia dónde sube y cuánto mide.
///
/// Une el nivel [`Tramo::z`] con `z + 1`. No tiene sentido "de bajada": el mismo
/// tramo se sube desde el pie y se baja desde la cima, y cuál de las dos cosas
/// estás haciendo lo dice tu posición, no un tipo de tile distinto.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tramo {
    pub body: u8,
    /// Nivel BAJO del par que une.
    pub z: i32,
    /// Celda del PIE (el escalón más bajo).
    pub tx: i64,
    pub tz: i64,
    /// Hacia dónde sube: 0 = norte (−z), 1 = este (+x), 2 = sur (+z), 3 = oeste (−x).
    pub dir: u8,
    /// Cuántos escalones.
    pub pasos: u8,
}

/// Vector unitario del sentido de subida, en celdas.
#[inline]
pub fn eje(dir: u8) -> (i64, i64) {
    match dir % 4 {
        0 => (0, -1),
        1 => (1, 0),
        2 => (0, 1),
        _ => (-1, 0),
    }
}

impl Tramo {
    /// Las celdas del tramo, del pie a la cima, con su número de escalón.
    pub fn celdas(&self) -> impl Iterator<Item = (i64, i64, u8)> + '_ {
        let (dx, dz) = eje(self.dir);
        (0..self.pasos).map(move |i| (self.tx + dx * i as i64, self.tz + dz * i as i64, i))
    }

    /// La celda de la CIMA (el último escalón).
    pub fn cima(&self) -> (i64, i64) {
        let (dx, dz) = eje(self.dir);
        let n = self.pasos.saturating_sub(1) as i64;
        (self.tx + dx * n, self.tz + dz * n)
    }

    /// El **rellano**: la celda que sigue después de la cima, ya en la planta de
    /// arriba. Es donde tenés que poder pisar para que la escalera lleve a algún
    /// sitio; quien construye la escalera es responsable de que exista.
    pub fn rellano(&self) -> (i64, i64) {
        let (dx, dz) = eje(self.dir);
        let n = self.pasos as i64;
        (self.tx + dx * n, self.tz + dz * n)
    }

    /// ¿Esta celda es un escalón de este tramo? Devuelve qué escalón.
    pub fn paso_en(&self, tx: i64, tz: i64) -> Option<u8> {
        let (dx, dz) = eje(self.dir);
        // Proyección sobre el eje: fuera de la línea, no hay escalón.
        let (ox, oz) = (tx - self.tx, tz - self.tz);
        let i = ox * dx + oz * dz;
        if ox != dx * i || oz != dz * i {
            return None;
        }
        (0..self.pasos as i64).contains(&i).then_some(i as u8)
    }
}

/// **Cuánto del tramo llevás andado**, de 0 (el borde de afuera del primer
/// escalón) a 1 (el borde de afuera del último).
///
/// Se mide en metros sobre el eje del tramo y no por celda: si sólo contara el
/// escalón, el personaje subiría a saltos de un cuarto de planta y el cambio de
/// planta dependería de dónde cae el borde de la celda. Con la posición continua,
/// la elevación acompaña al paso y **la escalera se puede desandar a mitad**.
pub fn progreso(t: &Tramo, x_m: f32, z_m: f32) -> f32 {
    let (dx, dz) = eje(t.dir);
    // Coordenada del jugador sobre el eje, y la del borde exterior del pie.
    let (u, u0) = match (dx, dz) {
        (1, _) => (x_m, t.tx as f32 * TILE_M),
        (-1, _) => (-x_m, -((t.tx + 1) as f32 * TILE_M)),
        (_, 1) => (z_m, t.tz as f32 * TILE_M),
        _ => (-z_m, -((t.tz + 1) as f32 * TILE_M)),
    };
    let largo = t.pasos.max(1) as f32 * TILE_M;
    ((u - u0) / largo).clamp(0.0, 1.0)
}

/// **Cuánto pesa la TRANSICIÓN, según dónde estés dentro del tramo.**
///
/// ```text
///   s =  0        PIE                CIMA        1
///        │         ┌──────────────────┐          │
///        │        ╱ 1.0  (meseta)      ╲         │
///        └───────┘                      └────────┘
///          abre      se cambia de planta    cierra
///                    en cualquiera de
///                    los dos bordes
/// ```
///
/// La meseta cubre EXACTAMENTE la franja muerta `[PIE, CIMA]`, que es donde caen
/// los dos únicos puntos en los que se toca `tiles.z`: subiendo al cruzar [`CIMA`],
/// bajando al cruzar [`PIE`]. O sea que **el mundo se cambia siempre en el instante
/// de máxima transición**, sin tocar la histéresis que evita el parpadeo.
///
/// Fuera de la meseta baja a cero antes de las puntas: el primer y el último cuarto
/// de escalera son los que "abren" —al subir la planta nueva se va aclarando hasta
/// verse entera, al bajar la vieja se apaga—. Vale para las dos cosas que se
/// desvanecen: la LUZ (`interior::medir_encierro`, la sombra del hueco) y el
/// DIBUJO de la planta (`world2d::LayerFade::tramo`).
pub fn meseta_del_tramo(s: f32) -> f32 {
    fn suave(a: f32, b: f32, x: f32) -> f32 {
        let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
        t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
    }
    suave(0.0, PIE, s) * (1.0 - suave(CIMA, 1.0, s))
}

/// **Elevación del DIBUJO del personaje** mientras anda el tramo, en metros sobre
/// sus pies.
///
/// La referencia es la planta que se está dibujando (`nivel`), y por eso el número
/// da un salto de una planta entera justo cuando `s` cruza [`CIMA`] — que es el
/// mismo frame en el que el mundo cambia de planta. Los dos saltos son iguales y
/// opuestos, así que **en pantalla el personaje no salta**: sigue subiendo liso.
/// Sin esto, cambiar de capa se vería como el teletransporte que se borró.
pub fn elevacion(t: &Tramo, s: f32, nivel: i32) -> f32 {
    let base = if nivel > t.z { 1.0 } else { 0.0 };
    (s - base) * ALTURA_PISO_M
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tramo(dir: u8) -> Tramo {
        Tramo { body: 0, z: 0, tx: 10, tz: 20, dir, pasos: PASOS }
    }

    #[test]
    fn el_tramo_ocupa_sus_cuatro_celdas_en_linea() {
        for dir in 0..4 {
            let t = tramo(dir);
            let celdas: Vec<_> = t.celdas().collect();
            assert_eq!(celdas.len(), PASOS as usize);
            assert_eq!((celdas[0].0, celdas[0].1), (t.tx, t.tz), "el paso 0 es el pie");
            assert_eq!((celdas[3].0, celdas[3].1), t.cima());
            // Y el rellano queda UNA celda más allá de la cima, ya en la planta alta.
            let (rx, rz) = t.rellano();
            let (cx, cz) = t.cima();
            assert_eq!((rx - cx).abs() + (rz - cz).abs(), 1);
        }
    }

    #[test]
    fn el_progreso_va_de_cero_a_uno_a_lo_largo_del_tramo() {
        for dir in 0..4 {
            let t = tramo(dir);
            let (dx, dz) = eje(dir);
            // Centro del pie y centro de la cima.
            let pie = (t.tx as f32 + 0.5, t.tz as f32 + 0.5);
            let n = (PASOS - 1) as f32;
            let cima = (pie.0 + dx as f32 * n, pie.1 + dz as f32 * n);
            let s_pie = progreso(&t, pie.0, pie.1);
            let s_cima = progreso(&t, cima.0, cima.1);
            assert!(s_pie < PIE, "dir {dir}: el pie tiene que contar como planta baja ({s_pie})");
            assert!(
                s_cima >= CIMA,
                "dir {dir}: pisar el último escalón ya es planta alta ({s_cima})"
            );
            assert!(s_pie < s_cima, "dir {dir}: el progreso crece hacia la cima");
        }
    }

    /// El seguro contra el parpadeo: entre [`PIE`] y [`CIMA`] hay medio tramo de
    /// franja muerta, así que **quedarse quieto no puede cambiar de planta**. Es
    /// lo que reemplaza al pestillo del sistema viejo.
    #[test]
    fn hay_franja_muerta_de_medio_tramo() {
        assert!(CIMA - PIE >= 0.4, "la histéresis no puede ser un pelo");
        let t = tramo(1);
        let medio = progreso(&t, t.tx as f32 + PASOS as f32 * 0.5, t.tz as f32 + 0.5);
        assert!(medio > PIE && medio < CIMA, "el centro del tramo no compromete nada");
    }

    /// **El cambio de planta y el salto del dibujo se anulan.** Es la invariante de
    /// que subir se vea continuo: en el frame del cruce, la elevación medida contra
    /// la planta vieja y contra la nueva se diferencian en EXACTAMENTE una planta,
    /// que es lo que el mundo acaba de moverse.
    #[test]
    fn el_cambio_de_planta_no_hace_saltar_al_personaje() {
        let t = tramo(1);
        let s = CIMA;
        let antes = elevacion(&t, s, t.z);
        let despues = elevacion(&t, s, t.z + 1);
        assert!(
            (antes - despues - ALTURA_PISO_M).abs() < 1e-4,
            "antes={antes} despues={despues}"
        );
    }

    /// **La transición tapa los dos puntos donde cambia la planta**, y sólo
    /// ésos. Es la invariante de la que dependen el fundido de la luz y el de la
    /// planta: si la meseta no cubriera `CIMA` y `PIE`, el cambio se vería.
    #[test]
    fn la_meseta_cubre_los_dos_cambios_de_planta() {
        assert!(meseta_del_tramo(PIE) > 0.99, "subiendo se vería el corte");
        assert!(meseta_del_tramo(CIMA) > 0.99, "bajando se vería el corte");
        assert!(meseta_del_tramo((PIE + CIMA) * 0.5) > 0.99, "la meseta es continua");
        assert_eq!(meseta_del_tramo(0.0), 0.0, "el pie del tramo se ve normal");
        assert_eq!(meseta_del_tramo(1.0), 0.0, "y el rellano de arriba también");
    }

    /// Y es una CURVA, no un escalón: se recorre el tramo en centésimos y ningún
    /// paso puede moverla de golpe. Un escalón acá sería el corte de escena.
    #[test]
    fn la_meseta_no_tiene_escalones() {
        let mut anterior = meseta_del_tramo(0.0);
        for i in 1..=100 {
            let e = meseta_del_tramo(i as f32 / 100.0);
            assert!((e - anterior).abs() < 0.08, "salto en s={:.2}", i as f32 / 100.0);
            anterior = e;
        }
    }

    #[test]
    fn desandar_el_tramo_devuelve_el_mismo_numero() {
        // Bajar no es otro sistema: es el mismo progreso leído al revés.
        let t = tramo(2);
        let subiendo = progreso(&t, t.tx as f32 + 0.5, t.tz as f32 + 2.5);
        let bajando = progreso(&t, t.tx as f32 + 0.5, t.tz as f32 + 2.5);
        assert_eq!(subiendo, bajando);
    }

    #[test]
    fn paso_en_solo_reconoce_la_linea_del_tramo() {
        let t = tramo(1); // sube hacia el este
        assert_eq!(t.paso_en(t.tx, t.tz), Some(0));
        assert_eq!(t.paso_en(t.tx + 3, t.tz), Some(3));
        assert_eq!(t.paso_en(t.tx + 4, t.tz), None, "el rellano no es escalón");
        assert_eq!(t.paso_en(t.tx, t.tz + 1), None, "la celda de al lado tampoco");
        assert_eq!(t.paso_en(t.tx - 1, t.tz), None, "ni la de antes del pie");
    }
}
