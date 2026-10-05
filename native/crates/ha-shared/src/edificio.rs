//! **La cáscara de un edificio**: qué celdas ocupa, dónde está su contorno y por
//! dónde se entra.
//!
//! `build_grid.rs` sabe de bordes sueltos: un tramo de muro entre dos celdas, y
//! si apoya o no. Con eso alcanza para levantar un cuarto, pero no para una casa:
//! nadie sabía *dónde termina el edificio*, así que la fachada se decidía borde
//! por borde mirando los vecinos y una puerta abierta desde adentro no existía
//! desde la vereda. Este módulo pone el objeto que faltaba.
//!
//! ## Lo que sostiene
//!
//! - **La planta** es un conjunto de celdas, agrupadas en *cuartos*. Ampliar es
//!   pegarle un cuarto más; no hay que redibujar nada.
//! - **El contorno** son los bordes con edificio de un lado y calle del otro. Se
//!   agrupan en **tramos**: corridas de bordes seguidos sobre la misma línea y
//!   con la misma cara. Un tramo es lo que se dibuja con las bandas del pack
//!   modular, que miden [`MODULO`] celdas de frente.
//! - **Los tabiques** salen solos: un borde con edificio de los dos lados pero
//!   con un cuarto distinto a cada lado es una pared interior. Nadie los coloca.
//! - **Las aberturas son UNA sola lista**. Puerta, vano y ventana viven en un
//!   único mapa indexado por borde, así que la elevación de la calle y el muro de
//!   adentro leen el MISMO dato: es imposible que una puerta esté abierta de un
//!   lado y tapiada del otro. Ése era el bug que este módulo existe para no tener.
//!
//! ## Lo que NO hace
//!
//! No dibuja ni conoce sprites (eso es `ha-game::walls`), no cobra materiales y
//! no toca el mundo: es geometría pura y determinista, como el resto de
//! `ha-shared`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::build_grid::{WallAxis, WallKind};

/// **El frente de un módulo del pack, en celdas.**
///
/// No es un número de diseño: es lo que mide la banda que dibujó el artista en
/// `5_Floor_Modular_Buildings` (ver `scripts/import-modulares.py`). Angostarla
/// parte una ventana al medio, que es exactamente el estropicio que dejó la
/// primera importación cuando cortó las bandas en columnas de 1 celda.
pub const MODULO: i64 = 7;

/// Alto de la banda de PLANTA BAJA, en celdas del pack.
pub const ALTO_PLANTA_BAJA: i64 = 3;
/// Alto de la banda de un PISO, en celdas del pack.
pub const ALTO_PISO: i64 = 4;
/// Alto de la banda de AZOTEA (incluye el aire del parapeto), en celdas.
pub const ALTO_AZOTEA: i64 = 7;

/// Qué cara del edificio mira un tramo del contorno.
///
/// El juego es cenital 3/4: la cara sur es la que da a quien mira, y es la única
/// que se lee entera. Por eso el frente y la espalda no son lo mismo aunque
/// geométricamente sean dos líneas horizontales.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Cara {
    /// Cara SUR: el edificio queda al norte del borde. Es lo que ve alguien
    /// parado en la vereda, y donde va la planta baja con su entrada.
    Frente,
    /// Cara NORTE: el edificio queda al sur. Se ve la espalda, por encima del
    /// techo.
    Espalda,
    /// Muro lateral (corre norte-sur). `este` = el edificio queda al oeste del
    /// borde, o sea que es su pared este.
    Costado { este: bool },
}

impl Cara {
    /// Nombre humano, para la paleta y los mensajes.
    pub fn label(self) -> &'static str {
        match self {
            Cara::Frente => "Frente",
            Cara::Espalda => "Espalda",
            Cara::Costado { este: true } => "Costado este",
            Cara::Costado { este: false } => "Costado oeste",
        }
    }

    /// ¿Se lee de frente? Sólo el frente merece la banda con la entrada; los
    /// costados se ven de canto y la espalda queda tapada por su propio techo.
    pub fn es_frente(self) -> bool {
        matches!(self, Cara::Frente)
    }
}

/// Un borde de la cuadrícula. Es la llave de todo lo de acá.
///
/// `WallAxis` no es `Ord`, así que el eje se guarda como el `u8` que ya define
/// `build_grid` — la lista de aberturas necesita orden estable para que dos
/// partidas con los mismos cambios den el mismo resultado.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Borde {
    pub tx: i64,
    pub tz: i64,
    pub eje: u8,
}

impl Borde {
    pub fn new(tx: i64, tz: i64, axis: WallAxis) -> Self {
        Borde { tx, tz, eje: axis.as_u8() }
    }

    pub fn axis(self) -> WallAxis {
        WallAxis::from_u8(self.eje)
    }

    /// Las dos celdas que separa: `[antes, después]` — norte/sur para `H`,
    /// oeste/este para `V`. Es el mismo criterio que `build_grid::edge_cells`.
    pub fn celdas(self) -> [(i64, i64); 2] {
        match self.axis() {
            WallAxis::H => [(self.tx, self.tz - 1), (self.tx, self.tz)],
            WallAxis::V => [(self.tx - 1, self.tz), (self.tx, self.tz)],
        }
    }
}

/// Una corrida de bordes seguidos sobre la misma línea y con la misma cara.
///
/// Es la unidad de DIBUJO: un tramo de 9 celdas es exactamente una banda del
/// pack. Uno de 18 son dos bandas; uno de 5 es una banda recortada (y el que
/// dibuja decide si la recorta o deja el módulo entero sobresaliendo).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tramo {
    /// Esquina de arranque del tramo (el borde de menor `tx`/`tz`).
    pub tx: i64,
    pub tz: i64,
    pub eje: u8,
    /// Cuántos bordes seguidos tiene.
    pub largo: i64,
    pub cara: Cara,
}

impl Tramo {
    pub fn axis(self) -> WallAxis {
        WallAxis::from_u8(self.eje)
    }

    /// Los bordes que lo componen, en orden.
    pub fn bordes(self) -> Vec<Borde> {
        (0..self.largo)
            .map(|i| match self.axis() {
                WallAxis::H => Borde { tx: self.tx + i, tz: self.tz, eje: self.eje },
                WallAxis::V => Borde { tx: self.tx, tz: self.tz + i, eje: self.eje },
            })
            .collect()
    }

    /// **Cuántos módulos enteros entran y cuánto sobra.**
    ///
    /// El pack no tiene con qué dibujar media banda, así que esto es lo que el
    /// dibujante necesita saber para no inventar pixel: `(enteros, resto)`.
    pub fn modulos(self) -> (i64, i64) {
        (self.largo / MODULO, self.largo % MODULO)
    }
}

/// Un cuarto: rectángulo de celdas `[x0, x0+w) × [z0, z0+h)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cuarto {
    pub x0: i64,
    pub z0: i64,
    pub w: i64,
    pub h: i64,
}

impl Cuarto {
    pub fn new(x0: i64, z0: i64, w: i64, h: i64) -> Self {
        Cuarto { x0, z0, w: w.max(1), h: h.max(1) }
    }

    pub fn contiene(&self, cx: i64, cz: i64) -> bool {
        cx >= self.x0 && cx < self.x0 + self.w && cz >= self.z0 && cz < self.z0 + self.h
    }

    pub fn celdas(&self) -> impl Iterator<Item = (i64, i64)> + '_ {
        (self.z0..self.z0 + self.h).flat_map(move |z| (self.x0..self.x0 + self.w).map(move |x| (x, z)))
    }
}

/// La cáscara de un edificio: sus cuartos, sus plantas y sus aberturas.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Edificio {
    cuartos: Vec<Cuarto>,
    /// Cuántas plantas tiene por encima de la baja. `0` = sólo planta baja.
    pisos: i64,
    /// **La lista única.** Fachada y pared interior la leen a las dos.
    aberturas: BTreeMap<Borde, WallKind>,
}

impl Edificio {
    /// Arranca con un solo cuarto.
    pub fn nuevo(primero: Cuarto) -> Self {
        Edificio { cuartos: vec![primero], pisos: 0, aberturas: BTreeMap::new() }
    }

    pub fn cuartos(&self) -> &[Cuarto] {
        &self.cuartos
    }

    pub fn pisos(&self) -> i64 {
        self.pisos
    }

    /// Agrega una planta más. La azotea se dibuja siempre arriba de la última.
    pub fn apilar(&mut self) {
        self.pisos += 1;
    }

    /// Saca una planta. Nunca baja de la planta baja.
    pub fn desapilar(&mut self) {
        self.pisos = (self.pisos - 1).max(0);
    }

    /// **Ampliar: pegarle un cuarto.**
    ///
    /// Devuelve `false` si el cuarto no toca al edificio (una casa no son dos
    /// cajas sueltas) o si se superpone con uno que ya está. Lo que ANTES era
    /// contorno y ahora queda entre dos cuartos se vuelve tabique solo: no hay
    /// que borrar ni recolocar paredes.
    ///
    /// Las aberturas que quedan sepultadas —una puerta de calle que pasó a ser
    /// interior— **se conservan**: siguen siendo el paso entre los dos ambientes,
    /// que es justo lo que uno quiere al agrandar por ahí. Las que quedan dentro
    /// del cuarto nuevo (o sea, ya no separan nada) se descartan.
    pub fn ampliar(&mut self, nuevo: Cuarto) -> bool {
        if self.cuartos.iter().any(|c| se_superponen(c, &nuevo)) {
            return false;
        }
        if !self.cuartos.is_empty() && !self.cuartos.iter().any(|c| se_tocan(c, &nuevo)) {
            return false;
        }
        self.cuartos.push(nuevo);
        self.podar_aberturas();
        true
    }

    /// ¿Esta celda es parte del edificio?
    pub fn contiene(&self, cx: i64, cz: i64) -> bool {
        self.cuartos.iter().any(|c| c.contiene(cx, cz))
    }

    /// A qué cuarto pertenece una celda (el primero que la contiene).
    fn cuarto_de(&self, cx: i64, cz: i64) -> Option<usize> {
        self.cuartos.iter().position(|c| c.contiene(cx, cz))
    }

    /// Todas las celdas de la planta, sin repetir y en orden.
    pub fn celdas(&self) -> BTreeSet<(i64, i64)> {
        self.cuartos.iter().flat_map(|c| c.celdas()).collect()
    }

    /// **El contorno**: bordes con edificio de un lado y afuera del otro.
    pub fn contorno(&self) -> Vec<(Borde, Cara)> {
        let mut out = Vec::new();
        for (cx, cz) in self.celdas() {
            // Norte y sur de la celda (bordes H), oeste y este (bordes V).
            for (borde, cara) in [
                (Borde::new(cx, cz, WallAxis::H), Cara::Espalda),
                (Borde::new(cx, cz + 1, WallAxis::H), Cara::Frente),
                (Borde::new(cx, cz, WallAxis::V), Cara::Costado { este: false }),
                (Borde::new(cx + 1, cz, WallAxis::V), Cara::Costado { este: true }),
            ] {
                let [a, b] = borde.celdas();
                let otra = if (a.0, a.1) == (cx, cz) { b } else { a };
                if !self.contiene(otra.0, otra.1) {
                    out.push((borde, cara));
                }
            }
        }
        out.sort_by_key(|(b, _)| *b);
        out
    }

    /// **Los tabiques**: bordes con edificio de los dos lados pero con un cuarto
    /// distinto a cada lado. Salen solos de pegar cuartos; nadie los coloca.
    pub fn tabiques(&self) -> Vec<Borde> {
        let mut out = BTreeSet::new();
        for (cx, cz) in self.celdas() {
            let mio = self.cuarto_de(cx, cz);
            for borde in [
                Borde::new(cx, cz, WallAxis::H),
                Borde::new(cx, cz + 1, WallAxis::H),
                Borde::new(cx, cz, WallAxis::V),
                Borde::new(cx + 1, cz, WallAxis::V),
            ] {
                let [a, b] = borde.celdas();
                let otra = if (a.0, a.1) == (cx, cz) { b } else { a };
                if self.contiene(otra.0, otra.1) && self.cuarto_de(otra.0, otra.1) != mio {
                    out.insert(borde);
                }
            }
        }
        out.into_iter().collect()
    }

    /// El contorno agrupado en corridas seguidas de la misma cara.
    pub fn tramos(&self) -> Vec<Tramo> {
        let mut contorno = self.contorno();
        // Para encadenar hay que recorrer CADA LÍNEA de punta a punta: primero la
        // coordenada que queda fija (la fila para los bordes `H`, la columna para
        // los `V`) y después la que corre. El orden natural de `Borde` mezcla las
        // dos y deja cada borde aislado — así salían nueve tramos de 1 donde hay
        // un frente de nueve.
        contorno.sort_by_key(|(b, _)| match b.axis() {
            WallAxis::H => (b.eje, b.tz, b.tx),
            WallAxis::V => (b.eje, b.tx, b.tz),
        });
        let mut out: Vec<Tramo> = Vec::new();
        for (borde, cara) in contorno {
            // Un tramo sigue al anterior si comparte línea, cara y arranca justo
            // donde el otro terminó.
            let sigue = out.last().is_some_and(|t| {
                t.eje == borde.eje
                    && t.cara == cara
                    && match borde.axis() {
                        WallAxis::H => t.tz == borde.tz && t.tx + t.largo == borde.tx,
                        WallAxis::V => t.tx == borde.tx && t.tz + t.largo == borde.tz,
                    }
            });
            if sigue {
                out.last_mut().expect("hay último").largo += 1;
            } else {
                out.push(Tramo { tx: borde.tx, tz: borde.tz, eje: borde.eje, largo: 1, cara });
            }
        }
        out
    }

    // — Aberturas ————————————————————————————————————————————————

    /// Abre (o cambia) una abertura. Devuelve `false` si el borde no es parte de
    /// la cáscara: no se hace un agujero en el aire.
    pub fn abrir(&mut self, borde: Borde, kind: WallKind) -> bool {
        if !self.es_de_la_cascara(borde) {
            return false;
        }
        if kind == WallKind::Wall {
            self.aberturas.remove(&borde);
        } else {
            self.aberturas.insert(borde, kind);
        }
        true
    }

    /// Vuelve a tapiar un borde.
    pub fn cerrar(&mut self, borde: Borde) {
        self.aberturas.remove(&borde);
    }

    /// Qué hay en ese borde: la abertura si la tiene, `Wall` si es cáscara maciza
    /// y `None` si el borde no pertenece al edificio.
    pub fn abertura(&self, borde: Borde) -> Option<WallKind> {
        if !self.es_de_la_cascara(borde) {
            return None;
        }
        Some(*self.aberturas.get(&borde).unwrap_or(&WallKind::Wall))
    }

    /// La lista completa, para persistirla o mostrarla.
    pub fn aberturas(&self) -> &BTreeMap<Borde, WallKind> {
        &self.aberturas
    }

    fn es_de_la_cascara(&self, borde: Borde) -> bool {
        let [a, b] = borde.celdas();
        let dentro_a = self.contiene(a.0, a.1);
        let dentro_b = self.contiene(b.0, b.1);
        if dentro_a && dentro_b {
            return self.cuarto_de(a.0, a.1) != self.cuarto_de(b.0, b.1);
        }
        dentro_a || dentro_b
    }

    /// Tira las aberturas que dejaron de separar algo (quedaron adentro de un
    /// cuarto o fuera del edificio).
    fn podar_aberturas(&mut self) {
        let vivas: BTreeMap<Borde, WallKind> = self
            .aberturas
            .iter()
            .filter(|(b, _)| self.es_de_la_cascara(**b))
            .map(|(b, k)| (*b, *k))
            .collect();
        self.aberturas = vivas;
    }

    /// **Todo lo que hay que estampar**, contorno + tabiques, con la abertura ya
    /// aplicada. Éste es el único lugar del que salen las paredes de un edificio:
    /// la elevación de la calle y el muro de adentro comen de acá.
    pub fn bordes(&self) -> Vec<(Borde, WallKind)> {
        let mut out: Vec<(Borde, WallKind)> = self
            .contorno()
            .into_iter()
            .map(|(b, _)| b)
            .chain(self.tabiques())
            .map(|b| (b, *self.aberturas.get(&b).unwrap_or(&WallKind::Wall)))
            .collect();
        out.sort_by_key(|(b, _)| *b);
        out.dedup_by_key(|(b, _)| *b);
        out
    }

    /// ¿Se puede entrar? Un recinto sin puerta ni vano es un cajón, no una casa.
    /// (Misma regla que `ha-game::cuartos`, dicha una sola vez.)
    pub fn tiene_entrada(&self) -> bool {
        self.contorno()
            .iter()
            .any(|(b, _)| self.aberturas.get(b).is_some_and(|k| k.is_entrance()))
    }

    /// Alto de la cáscara en celdas del pack: planta baja + pisos + azotea. Es lo
    /// que hay que reservar en pantalla por encima del contorno.
    pub fn alto_en_celdas(&self) -> i64 {
        ALTO_PLANTA_BAJA + self.pisos * ALTO_PISO + ALTO_AZOTEA
    }
}

fn se_superponen(a: &Cuarto, b: &Cuarto) -> bool {
    a.x0 < b.x0 + b.w && b.x0 < a.x0 + a.w && a.z0 < b.z0 + b.h && b.z0 < a.z0 + a.h
}

/// Dos cuartos se tocan si comparten una arista (no alcanza con la esquina: por
/// una esquina no se pasa).
fn se_tocan(a: &Cuarto, b: &Cuarto) -> bool {
    let solapa_x = a.x0 < b.x0 + b.w && b.x0 < a.x0 + a.w;
    let solapa_z = a.z0 < b.z0 + b.h && b.z0 < a.z0 + a.h;
    let pegado_x = a.x0 + a.w == b.x0 || b.x0 + b.w == a.x0;
    let pegado_z = a.z0 + a.h == b.z0 || b.z0 + b.h == a.z0;
    (pegado_x && solapa_z) || (pegado_z && solapa_x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn casa() -> Edificio {
        Edificio::nuevo(Cuarto::new(0, 0, MODULO, 5))
    }

    #[test]
    fn el_contorno_de_un_rectangulo_le_da_la_vuelta() {
        let e = casa();
        // 2 lados de 9 + 2 lados de 5.
        assert_eq!(e.contorno().len() as i64, 2 * MODULO + 2 * 5);
        assert!(e.tabiques().is_empty(), "un solo cuarto no tiene tabiques");
    }

    #[test]
    fn el_frente_es_la_cara_sur() {
        let e = casa();
        let frente: Vec<_> = e
            .contorno()
            .into_iter()
            .filter(|(_, c)| c.es_frente())
            .collect();
        assert_eq!(frente.len() as i64, MODULO);
        // La cara sur del cuarto [0,5) es la línea z = 5.
        assert!(frente.iter().all(|(b, _)| b.tz == 5 && b.axis() == WallAxis::H));
    }

    /// El tramo es lo que se dibuja: un frente de 9 es UNA banda del pack, no
    /// nueve columnas de 1. Es el error que trajo la primera importación.
    #[test]
    fn el_frente_es_un_tramo_de_un_modulo_entero() {
        let e = casa();
        let frente: Vec<_> = e.tramos().into_iter().filter(|t| t.cara.es_frente()).collect();
        assert_eq!(frente.len(), 1, "el frente sale de una pieza: {frente:?}");
        assert_eq!(frente[0].largo, MODULO);
        assert_eq!(frente[0].modulos(), (1, 0));
    }

    #[test]
    fn dos_modulos_de_frente_son_dos_bandas() {
        let e = Edificio::nuevo(Cuarto::new(0, 0, MODULO * 2, 5));
        let frente: Vec<_> = e.tramos().into_iter().filter(|t| t.cara.es_frente()).collect();
        assert_eq!(frente[0].modulos(), (2, 0));
    }

    /// **Pegar un cuarto crea el tabique solo.** Nadie coloca la pared del medio:
    /// es el borde que quedó con edificio de los dos lados y cuartos distintos.
    #[test]
    fn pegar_un_cuarto_levanta_el_tabique_solo() {
        let mut e = casa();
        assert!(e.ampliar(Cuarto::new(0, 5, MODULO, 4)));
        let tabiques = e.tabiques();
        assert_eq!(tabiques.len() as i64, MODULO, "el tabique corre todo el ancho");
        assert!(tabiques.iter().all(|b| b.tz == 5 && b.axis() == WallAxis::H));
        // Y el frente se mudó al sur del cuarto nuevo.
        let frente: Vec<_> = e.tramos().into_iter().filter(|t| t.cara.es_frente()).collect();
        assert_eq!(frente.len(), 1);
        assert_eq!(frente[0].tz, 9);
    }

    #[test]
    fn un_cuarto_suelto_o_encimado_no_se_pega() {
        let mut e = casa();
        assert!(!e.ampliar(Cuarto::new(50, 50, 3, 3)), "no toca: no es la misma casa");
        assert!(!e.ampliar(Cuarto::new(1, 1, 3, 3)), "se encima con lo que ya está");
        // Por la esquina tampoco: por una esquina no se pasa.
        assert!(!e.ampliar(Cuarto::new(MODULO, 5, 3, 3)));
    }

    /// **La abertura es UNA.** Se abre una vez y la ven los dos lados; no hay una
    /// puerta de fachada y otra de interior que se puedan desincronizar.
    #[test]
    fn la_abertura_la_ven_los_dos_lados() {
        let mut e = casa();
        let puerta = Borde::new(4, 5, WallAxis::H);
        assert!(e.abrir(puerta, WallKind::Door));
        assert_eq!(e.abertura(puerta), Some(WallKind::Door));
        let bordes = e.bordes();
        let hallado = bordes.iter().find(|(b, _)| *b == puerta).expect("el borde está");
        assert_eq!(hallado.1, WallKind::Door);
        assert_eq!(
            bordes.iter().filter(|(b, _)| *b == puerta).count(),
            1,
            "un borde aparece una sola vez, con una sola cosa encima"
        );
        assert!(e.tiene_entrada());
    }

    #[test]
    fn no_se_abre_un_agujero_en_el_aire() {
        let mut e = casa();
        assert!(!e.abrir(Borde::new(40, 40, WallAxis::H), WallKind::Door));
        assert_eq!(e.abertura(Borde::new(40, 40, WallAxis::H)), None);
        // Y un borde interior de un mismo cuarto tampoco es cáscara.
        assert!(!e.abrir(Borde::new(4, 2, WallAxis::H), WallKind::Window));
    }

    /// Ampliar por donde estaba la puerta de calle **conserva la puerta**: pasa a
    /// ser el paso entre los dos ambientes. Lo que se descarta es lo que dejó de
    /// separar algo.
    #[test]
    fn ampliar_conserva_la_puerta_y_tira_lo_que_ya_no_separa() {
        let mut e = casa();
        let puerta = Borde::new(4, 5, WallAxis::H);
        let ventana = Borde::new(0, 2, WallAxis::V); // costado oeste
        assert!(e.abrir(puerta, WallKind::Door));
        assert!(e.abrir(ventana, WallKind::Window));
        assert!(e.ampliar(Cuarto::new(0, 5, MODULO, 4)));
        assert_eq!(e.abertura(puerta), Some(WallKind::Door), "sigue siendo el paso");
        assert_eq!(e.abertura(ventana), Some(WallKind::Window), "el costado sigue afuera");

        // Una ventana del frente que queda sepultada por el cuarto nuevo sí se va.
        let mut e2 = casa();
        let sur = Borde::new(2, 5, WallAxis::H);
        assert!(e2.abrir(sur, WallKind::Window));
        assert!(e2.ampliar(Cuarto::new(0, 5, MODULO, 4)));
        // Sigue siendo cáscara (ahora tabique), así que se conserva.
        assert_eq!(e2.abertura(sur), Some(WallKind::Window));

        // En cambio una del costado que el cuarto nuevo tapa por completo, no.
        let mut e3 = Edificio::nuevo(Cuarto::new(0, 0, 4, 4));
        let costado = Borde::new(4, 1, WallAxis::V);
        assert!(e3.abrir(costado, WallKind::Window));
        assert!(e3.ampliar(Cuarto::new(4, 0, 4, 4)));
        assert_eq!(e3.abertura(costado), Some(WallKind::Window), "ahora es tabique");
        // Pero si el cuarto nuevo la sepulta sin dejarla de frontera, desaparece:
        let mut e4 = Edificio::nuevo(Cuarto::new(0, 0, 4, 4));
        let arriba = Borde::new(1, 0, WallAxis::H);
        assert!(e4.abrir(arriba, WallKind::Door));
        assert!(e4.ampliar(Cuarto::new(0, -4, 4, 4)));
        assert_eq!(e4.abertura(arriba), Some(WallKind::Door), "quedó de tabique");
        assert!(e4.tabiques().contains(&arriba));
    }

    #[test]
    fn una_casa_en_ele_parte_el_frente_en_dos_tramos() {
        // Planta en L: el frente queda a dos alturas distintas.
        let mut e = Edificio::nuevo(Cuarto::new(0, 0, 4, 4));
        assert!(e.ampliar(Cuarto::new(4, 0, 4, 2)));
        let frentes: Vec<_> = e.tramos().into_iter().filter(|t| t.cara.es_frente()).collect();
        assert_eq!(frentes.len(), 2, "dos alturas de frente: {frentes:?}");
        assert_eq!(frentes.iter().map(|t| t.largo).sum::<i64>(), 8);
    }

    #[test]
    fn apilar_sube_la_cascara_una_banda_por_piso() {
        let mut e = casa();
        assert_eq!(e.alto_en_celdas(), ALTO_PLANTA_BAJA + ALTO_AZOTEA);
        e.apilar();
        e.apilar();
        assert_eq!(e.pisos(), 2);
        assert_eq!(e.alto_en_celdas(), ALTO_PLANTA_BAJA + 2 * ALTO_PISO + ALTO_AZOTEA);
        e.desapilar();
        e.desapilar();
        e.desapilar();
        assert_eq!(e.pisos(), 0, "nunca por debajo de la planta baja");
    }
}
