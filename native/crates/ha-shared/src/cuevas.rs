//! **Cuevas que se encuentran andando** — bocas en la ladera y una galería.
//!
//! El 3 ago 2026 se borraron las cuevas procedurales, y con razón: *"odio las
//! cuevas y cómo entramos a la cueva"*. El problema **nunca fue que existieran**;
//! fue cómo se entraba. Caminabas sobre un agujero que había puesto el generador
//! y la pantalla entera pasaba a dibujar otra cosa. Eso es cambiar de escena, y
//! este juego no hace eso (§4).
//!
//! Lo que vuelve es la cueva; lo que no vuelve es el corte. Tres decisiones lo
//! sostienen, y las tres son la misma:
//!
//! 1. **La boca es un tramo de escalera**, no un portal. Se baja andando los
//!    cuatro escalones que tiene cualquier otra escalera del juego
//!    ([`crate::escaleras`]), y desde la superficie se ve como lo que es: un
//!    hueco de escalera cortado en la ladera.
//! 2. **Toda la cueva está en UN nivel** (`z = −1`). Se cambia de planta una sola
//!    vez, al entrar; a partir de ahí adentrarse es caminar y nada más. No hay un
//!    segundo cambio de capa esperándote en el fondo.
//! 3. **Lo que se apaga es el afuera, en gradiente.** No lo hace este módulo: lo
//!    hace la distancia a la boca (`ha-game/src/interior.rs`). Acá sólo se decide
//!    la forma del hueco.
//!
//! ## La forma
//!
//! ```text
//!    ladera                 pasillo                  cámara
//!   ┌──────┐   ╔════════════════════════════╗   ╔══════════════╗
//!   │ boca ├───╢                            ╟───╢      ◍       ║
//!   └──┬───┘   ╚════════════════════════════╝   ╚══════════════╝
//!      │ tramo de 4 escalones                         radio R
//!      ▼ z = −1
//! ```
//!
//! Un segmento y un disco: la pertenencia se resuelve con dos distancias, sin
//! recorrer nada. Es lo que permite preguntarle a cada celda "¿sos cueva?" dentro
//! del presupuesto de generación (`generating_a_chunk_is_cheap`, < 8 µs/celda) —
//! el ruido con umbral del sistema viejo costaba varias octavas por celda y por
//! nivel.

use crate::escaleras::{Tramo, PASOS};
use crate::topdown::Tile;
use crate::CelestialBody;

/// Nivel donde vive la galería. Uno solo, a propósito (ver el encabezado).
pub const NIVEL: i32 = -1;

/// Lado de la celda gruesa que reparte las cuevas, en tiles. Con 160 hay una boca
/// cada ~160 m: bastante lejos como para que encontrar una sea encontrar algo, y
/// bastante cerca como para que salir a caminar valga la pena.
const CELDA: i64 = 160;

/// Cuánto del interior de la celda gruesa puede ocupar la boca. Deja un margen
/// para que la galería de una cueva no invada la celda vecina.
const MARGEN: i64 = 40;

/// Ancho del pasillo, en tiles (radio: 1 → tres celdas de ancho).
const RADIO_PASILLO: i64 = 1;

/// Largo mínimo y máximo del pasillo, en tiles.
const LARGO_MIN: i64 = 16;
const LARGO_MAX: i64 = 30;

/// Radio mínimo y máximo de la cámara del fondo.
const CAMARA_MIN: i64 = 4;
const CAMARA_MAX: i64 = 7;

/// Hash determinista de una celda gruesa. Mismo espíritu que el resto del
/// generador: sin estado, sin tablas, mismo resultado en cualquier máquina.
fn hash(body: CelestialBody, gx: i64, gz: i64, sal: u64) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for v in [body as u64, gx as u64, gz as u64, sal] {
        h ^= v.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        h = h.wrapping_mul(0x100_0000_01b3);
        h ^= h >> 29;
    }
    h
}

/// Una cueva: dónde está su boca, hacia dónde entra y cuánto mide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cueva {
    /// Celda de la boca EN SUPERFICIE (el rellano del tramo, donde pisás antes de bajar).
    pub boca: (i64, i64),
    /// Hacia dónde se ADENTRA la cueva (0 N, 1 E, 2 S, 3 O) — el opuesto del
    /// sentido de subida del tramo.
    pub adentro: u8,
    /// Largo del pasillo, en tiles, medido desde el pie del tramo.
    pub largo: i64,
    /// Radio de la cámara del fondo.
    pub camara: i64,
}

/// Vector unitario de un sentido, en celdas.
#[inline]
fn eje(dir: u8) -> (i64, i64) {
    crate::escaleras::eje(dir)
}

thread_local! {
    /// **Memo de una sola entrada.** `cueva_de` se pregunta por cada celda que
    /// genera el mundo, y generar un chunk son 256 celdas seguidas que caen todas
    /// en la MISMA celda gruesa (16 tiles contra 160). Sin esto se recalculaban
    /// cinco hashes por celda para volver a contestar lo mismo, y el guardián de
    /// coste del generador (`generating_a_cell_costs_only_a_few_terrain_probes`)
    /// lo cazó en cuanto entraron las cuevas.
    ///
    /// Una entrada alcanza justamente porque el acceso es espacialmente coherente;
    /// una caché grande sería más memoria para el mismo acierto.
    static MEMO: std::cell::Cell<Option<((u8, i64, i64, u32), Option<Cueva>)>> =
        const { std::cell::Cell::new(None) };
}

/// **¿Hay cueva en esta celda gruesa?** Una de cada tres, para que el mapa no sea
/// un queso gruyer: encontrar una boca tiene que ser un hallazgo.
pub fn cueva_de(body: CelestialBody, gx: i64, gz: i64) -> Option<Cueva> {
    // La época de la semilla entra en la clave: otra partida, otras cuevas.
    let clave = (body as u8, gx, gz, crate::partida::epoca());
    if let Some((k, v)) = MEMO.with(|m| m.get()) {
        if k == clave {
            return v;
        }
    }
    let v = cueva_de_sin_memo(body, gx, gz);
    MEMO.with(|m| m.set(Some((clave, v))));
    v
}

fn cueva_de_sin_memo(body: CelestialBody, gx: i64, gz: i64) -> Option<Cueva> {
    // Los cuerpos sin subsuelo tallable no tienen cuevas: la roca de la Luna y el
    // hielo son otro material, y hasta que tengan su propio interior no se abren
    // agujeros que llevan a un sitio sin arte.
    if !matches!(body, CelestialBody::Earth) {
        return None;
    }
    let h = hash(body, gx, gz, 0xcafe_0001);
    if h % 3 != 0 {
        return None;
    }
    let dentro = |sal: u64, rango: i64| (hash(body, gx, gz, sal) % rango as u64) as i64;
    let bx = gx * CELDA + MARGEN + dentro(1, CELDA - MARGEN * 2);
    let bz = gz * CELDA + MARGEN + dentro(2, CELDA - MARGEN * 2);
    let adentro = (dentro(3, 4) % 4) as u8;
    let largo = LARGO_MIN + dentro(4, LARGO_MAX - LARGO_MIN + 1);
    let camara = CAMARA_MIN + dentro(5, CAMARA_MAX - CAMARA_MIN + 1);
    Some(Cueva { boca: (bx, bz), adentro, largo, camara })
}

impl Cueva {
    /// **El tramo de la boca.** Une la superficie (`0`) con la galería
    /// ([`NIVEL`]), y sube hacia AFUERA: su pie es el escalón más hondo y su
    /// rellano, la celda de ladera por la que llegaste.
    pub fn tramo(&self, body: CelestialBody) -> Tramo {
        // El tramo sube hacia afuera = el opuesto de `adentro`.
        let dir = (self.adentro + 2) % 4;
        let (ax, az) = eje(self.adentro);
        // El rellano del tramo es la boca; el pie queda `PASOS` celdas hacia
        // adentro. `Tramo::rellano()` cuenta desde el pie en sentido de subida.
        let n = PASOS as i64;
        Tramo {
            body: body as u8,
            z: NIVEL,
            tx: self.boca.0 + ax * n,
            tz: self.boca.1 + az * n,
            dir,
            pasos: PASOS,
        }
    }

    /// Centro de la cámara del fondo.
    fn centro_camara(&self) -> (i64, i64) {
        let (ax, az) = eje(self.adentro);
        let d = PASOS as i64 + self.largo;
        (self.boca.0 + ax * d, self.boca.1 + az * d)
    }

    /// **¿Esta celda es hueco de la galería?** Pasillo (segmento) o cámara
    /// (disco). Dos distancias, sin recorridos.
    pub fn es_galeria(&self, tx: i64, tz: i64) -> bool {
        let (ax, az) = eje(self.adentro);
        // Coordenadas locales: `u` avanza hacia adentro desde la boca, `v` cruza.
        let (dx, dz) = (tx - self.boca.0, tz - self.boca.1);
        let u = dx * ax + dz * az;
        let v = dx * az - dz * ax; // perpendicular
        let pie = PASOS as i64;
        if (pie..=pie + self.largo).contains(&u) && v.abs() <= RADIO_PASILLO {
            return true;
        }
        let (cx, cz) = self.centro_camara();
        let (ex, ez) = (tx - cx, tz - cz);
        ex * ex + ez * ez <= self.camara * self.camara
    }

    /// La celda de la boca por la que entra la luz del día. La usa el gradiente de
    /// oscuridad: es el punto desde el que se mide "cuánto me adentré".
    pub fn abertura(&self) -> (i64, i64) {
        self.boca
    }
}

/// Las cuevas que pueden alcanzar esta celda: la suya y las ocho vecinas, porque
/// una galería larga cruza el borde de su celda gruesa.
fn candidatas(body: CelestialBody, tx: i64, tz: i64) -> impl Iterator<Item = Cueva> {
    let gx = tx.div_euclid(CELDA);
    let gz = tz.div_euclid(CELDA);
    (-1..=1)
        .flat_map(move |dz| (-1..=1).map(move |dx| (dx, dz)))
        .filter_map(move |(dx, dz)| cueva_de(body, gx + dx, gz + dz))
}

/// **El tile natural de una celda que pertenece a una cueva**, o `None` si ahí no
/// hay cueva. Lo consulta `topdown::topdown_tile` antes de devolver roca maciza.
///
/// - En [`NIVEL`]: suelo de cueva en la galería, escalón en el tramo.
/// - En superficie: el hueco de la escalera, que es como se VE la boca desde
///   arriba — un corte oscuro en la ladera, no un agujero abstracto.
pub fn tile_natural(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<Tile> {
    if z == 0 {
        // **En superficie basta MIRAR LA PROPIA celda gruesa.** La boca vive a
        // `MARGEN` (40) tiles del borde de la suya y su tramo mide cuatro celdas,
        // así que nunca se sale de ella: preguntarle a las ocho vecinas sería
        // ocho veces el trabajo para nueve veces la misma respuesta. Y esto corre
        // por cada celda de superficie que se genera, que es el camino más
        // caliente del generador.
        let c = cueva_de(body, tx.div_euclid(CELDA), tz.div_euclid(CELDA))?;
        return c.tramo(body).paso_en(tx, tz).map(|_| Tile::Stairwell);
    }
    if z != NIVEL {
        return None;
    }
    // Bajo tierra sí hacen falta las vecinas: una galería llega a 41 tiles de su
    // boca y cruza el borde de su celda gruesa.
    for c in candidatas(body, tx, tz) {
        if c.tramo(body).paso_en(tx, tz).is_some() {
            return Some(Tile::Step);
        }
        if c.es_galeria(tx, tz) {
            return Some(Tile::CaveFloor);
        }
    }
    None
}

/// **El tramo natural que pasa por esta celda**, si lo hay. Es la mitad
/// "generador" de `escaleras::Escaleras::en`: las bocas de cueva son infinitas y
/// no caben en una lista, así que se calculan.
pub fn tramo_natural(body: CelestialBody, z: i32, tx: i64, tz: i64) -> Option<Tramo> {
    if z != NIVEL {
        return None;
    }
    candidatas(body, tx, tz)
        .map(|c| c.tramo(body))
        .find(|t| t.paso_en(tx, tz).is_some())
}

/// **La abertura al cielo más cercana entre las cuevas naturales**, en tiles, o
/// `None` si no hay ninguna cerca. La usa el gradiente de encierro: adentrarse es
/// alejarse de la luz que entra por la boca.
pub fn boca_mas_cercana(body: CelestialBody, tx: i64, tz: i64) -> Option<(i64, i64)> {
    candidatas(body, tx, tz)
        .map(|c| c.abertura())
        .min_by_key(|&(bx, bz)| (bx - tx).pow(2) + (bz - tz).pow(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Que existan: un barrido corto tiene que encontrar bocas. Si esto falla, la
    /// cueva volvió a ser inencontrable y "salir a explorar" no significa nada.
    #[test]
    fn el_mundo_tiene_cuevas_y_no_demasiadas() {
        let body = CelestialBody::Earth;
        let total = (0..12i64)
            .flat_map(|gx| (0..12i64).map(move |gz| (gx, gz)))
            .filter(|&(gx, gz)| cueva_de(body, gx, gz).is_some())
            .count();
        assert!(total > 20, "casi no hay cuevas: {total} de 144 celdas");
        assert!(total < 100, "el mapa es un queso gruyer: {total} de 144");
    }

    /// **Se entra andando.** La boca es el rellano de un tramo cuyo nivel bajo es
    /// la galería: bajar es andar cuatro escalones, no pisar un portal.
    #[test]
    fn la_boca_es_un_tramo_que_se_camina() {
        let body = CelestialBody::Earth;
        let c = (0..40i64)
            .find_map(|gx| cueva_de(body, gx, 0))
            .expect("alguna cueva cerca del origen");
        let t = c.tramo(body);
        assert_eq!(t.z, NIVEL, "el tramo une la galería con la superficie");
        assert_eq!(t.pasos, PASOS);
        assert_eq!(t.rellano(), c.boca, "el rellano de arriba ES la boca");
        // Y los cuatro escalones son celdas distintas y en línea.
        let celdas: Vec<_> = t.celdas().map(|(x, z, _)| (x, z)).collect();
        assert_eq!(celdas.len(), PASOS as usize);
        assert!(celdas.iter().all(|&c| c != t.rellano()));
    }

    /// La galería es un pasillo que llega a una cámara, y ambos se caminan. Si el
    /// pasillo no conectara con el pie del tramo, la cueva sería una boca ciega.
    #[test]
    fn el_pasillo_arranca_en_el_pie_del_tramo_y_llega_a_la_camara() {
        let body = CelestialBody::Earth;
        let c = (0..40i64)
            .find_map(|gx| cueva_de(body, gx, 0))
            .expect("alguna cueva");
        let t = c.tramo(body);
        // El pie del tramo es la primera celda de galería.
        assert!(c.es_galeria(t.tx, t.tz), "el pie del tramo no conecta con el pasillo");
        // Todo el eje del pasillo, celda a celda, es hueco: se camina de punta a punta.
        let (ax, az) = eje(c.adentro);
        for u in (PASOS as i64)..=(PASOS as i64 + c.largo) {
            let (x, z) = (c.boca.0 + ax * u, c.boca.1 + az * u);
            assert!(c.es_galeria(x, z), "el pasillo se corta en u={u}");
        }
        // Y el centro de la cámara también.
        let (cx, cz) = c.centro_camara();
        assert!(c.es_galeria(cx, cz));
    }

    /// La superficie sólo se abre en la boca. Si la galería asomara arriba, el
    /// mundo estaría lleno de agujeros por los que caerse sin haber cavado nada.
    #[test]
    fn en_superficie_solo_asoma_el_hueco_de_la_escalera() {
        let body = CelestialBody::Earth;
        let c = (0..40i64)
            .find_map(|gx| cueva_de(body, gx, 0))
            .expect("alguna cueva");
        let t = c.tramo(body);
        for (x, z, _) in t.celdas() {
            assert_eq!(tile_natural(body, 0, x, z), Some(Tile::Stairwell));
        }
        // Una celda de pasillo bien adentro NO abre nada en superficie.
        let (ax, az) = eje(c.adentro);
        let u = PASOS as i64 + c.largo / 2;
        let (px, pz) = (c.boca.0 + ax * u, c.boca.1 + az * u);
        assert_eq!(tile_natural(body, 0, px, pz), None, "la galería asomó en superficie");
        assert_eq!(tile_natural(body, NIVEL, px, pz), Some(Tile::CaveFloor));
    }

    /// El tramo se encuentra por su celda desde el generador: es lo que permite
    /// que `Escaleras::en` no tenga que guardar una lista infinita.
    #[test]
    fn el_tramo_natural_se_encuentra_por_su_celda() {
        let body = CelestialBody::Earth;
        let c = (0..40i64)
            .find_map(|gx| cueva_de(body, gx, 0))
            .expect("alguna cueva");
        let t = c.tramo(body);
        for (x, z, _) in t.celdas() {
            assert_eq!(tramo_natural(body, NIVEL, x, z), Some(t));
        }
        assert_eq!(tramo_natural(body, 0, t.tx, t.tz), None, "sólo vive en su nivel bajo");
    }

    /// Preguntarle a una celda si es cueva tiene que ser barato: se hace por cada
    /// celda de cada chunk que se genera. El presupuesto del generador entero es
    /// de 8 µs/celda (`generating_a_chunk_is_cheap`) y esto es una fracción.
    #[test]
    fn preguntar_por_la_cueva_es_barato() {
        let body = CelestialBody::Earth;
        // El barrido se centra en una cueva de verdad: medir sólo roca maciza
        // mediría el atajo, no el trabajo.
        let c = (0..40i64)
            .find_map(|gx| cueva_de(body, gx, 0))
            .expect("alguna cueva");
        let t0 = std::time::Instant::now();
        let n = 40_000i64;
        let lado = 200;
        let mut vivas = 0;
        for i in 0..n {
            let x = c.boca.0 - lado / 2 + i % lado;
            let z = c.boca.1 - lado / 2 + i / lado;
            if tile_natural(body, NIVEL, x, z).is_some() {
                vivas += 1;
            }
        }
        let por_celda = t0.elapsed().as_secs_f64() * 1e6 / n as f64;
        assert!(por_celda < 2.0, "{por_celda:.2} µs/celda es demasiado");
        assert!(vivas > 0, "el barrido no tocó ninguna cueva; el test no mide nada");
    }
}
