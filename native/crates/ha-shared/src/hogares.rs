//! **Dónde vive y dónde trabaja la gente**: los edificios REALES de la ciudad.
//!
//! Hasta el 30 sep 2026 la sociedad sembraba a los vecinos en espiral alrededor de
//! la plaza y llamaba "casa" al punto donde habían nacido — o sea, en medio de una
//! calle o adentro de una pared. Ahora el hogar es una casa del generador
//! (`city::roofed_rect`) y el puesto de trabajo es la puerta de un comercio, y los
//! dos salen de la MISMA fuente que dibuja la ciudad: si la casa está, la puerta
//! está, y el vecino sabe dónde.
//!
//! Puro y determinista (sin Bevy): lo consumen `ha-game/society.rs` y los tests.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::celestial_bodies::CelestialBody;
use crate::city::{lot_at, puerta_del_frente, roofed_rect, District};
use crate::geography::Settlement;

/// La huella de un edificio con puerta: lo mínimo que un vecino recuerda de su casa o su
/// puesto (es lo que se guarda en la partida).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Huella {
    pub min: (i64, i64),
    pub w: i64,
    pub h: i64,
    /// Celda de la puerta (es una celda de la pared del frente, la del sur).
    pub puerta: (i64, i64),
}

impl Huella {
    /// El escalón de afuera: el centro de la celda pegada a la puerta, del lado de la
    /// calle. Es donde un vecino espera a que le abran y de donde sale a caminar.
    pub fn escalon(&self) -> (f32, f32) {
        (self.puerta.0 as f32 + 0.5, self.puerta.1 as f32 + 1.5)
    }

    /// El centro de la celda de la puerta.
    pub fn umbral(&self) -> (f32, f32) {
        (self.puerta.0 as f32 + 0.5, self.puerta.1 as f32 + 0.5)
    }

    /// Dónde está el vecino `uid` cuando está adentro: la fila pegada a la pared del frente,
    /// repartida a lo ancho por `uid` (dos que duermen en la misma casa no se pisan). Nunca
    /// la celda de la puerta.
    pub fn adentro(&self, uid: u64) -> (f32, f32) {
        let ancho = (self.w - 2).max(1) as u64;
        let mut x = self.min.0 + 1 + (uid % ancho) as i64;
        if x == self.puerta.0 && ancho > 1 {
            x = self.min.0 + 1 + ((uid + 1) % ancho) as i64;
        }
        (x as f32 + 0.5, (self.puerta.1 - 1) as f32 + 0.5)
    }

    /// ¿El punto `(x, z)` está dentro del edificio (con `margen` metros de holgura)?
    pub fn contiene(&self, x: f32, z: f32, margen: f32) -> bool {
        x >= self.min.0 as f32 - margen
            && x <= (self.min.0 + self.w) as f32 + margen
            && z >= self.min.1 as f32 - margen
            && z <= (self.min.1 + self.h) as f32 + margen
    }

    /// ¿Está el punto del lado de ADENTRO de la puerta? La fila de la puerta cuenta como
    /// afuera hasta su mitad: quien está parado en el umbral se ve desde la calle.
    pub fn dentro(&self, x: f32, z: f32) -> bool {
        x >= self.min.0 as f32
            && x <= (self.min.0 + self.w) as f32
            && z >= self.min.1 as f32
            && z < self.puerta.1 as f32 + 0.5
    }

    /// El pasillo de la puerta, de la mitad del umbral a un paso afuera: quien está ahí ya
    /// se comprometió a cruzar.
    pub fn en_el_paso(&self, x: f32, z: f32) -> bool {
        (x - (self.puerta.0 as f32 + 0.5)).abs() < 0.25 && z > self.puerta.1 as f32 && z < self.puerta.1 as f32 + 2.1
    }
}

/// Un edificio con puerta: su huella y qué es.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edificio {
    pub huella: Huella,
    pub distrito: District,
    /// Clave lógica del programa del lote (`building/casa_chica`, `building/tienda`…).
    pub clave: &'static str,
}

impl std::ops::Deref for Edificio {
    type Target = Huella;
    fn deref(&self) -> &Huella {
        &self.huella
    }
}

impl Edificio {
    pub fn es_vivienda(&self) -> bool {
        self.distrito == District::Residential
    }

    /// ¿Se puede dormir acá? Una casa sí; también los edificios que el pack dibuja con plantas
    /// de vivienda encima (torre, hotel, tienda con departamentos, dúplex): el centro de una
    /// ciudad no tiene una sola casa y su gente vive ahí.
    pub fn es_hogar(&self) -> bool {
        self.es_vivienda() || matches!(self.clave, "building/torre" | "building/hotel" | "building/tienda" | "building/duplex")
    }

    /// ¿Es un puesto de trabajo? Todo lo que no es casa ni parque: comercio, oficina, servicio.
    pub fn es_puesto(&self) -> bool {
        !self.es_vivienda() && self.distrito != District::Park
    }
}

/// El próximo punto por el que hay que pasar para ir de `pos` a `meta` cruzando las puertas de
/// `huellas`: para entrar se va al escalón, después al umbral y recién ahí adentro; para salir,
/// al umbral y después afuera. Sin puertas de por medio devuelve `meta` tal cual.
pub fn siguiente_punto(pos: (f32, f32), meta: (f32, f32), huellas: &[Huella]) -> (f32, f32) {
    for h in huellas {
        let (aqui, alla) = (h.dentro(pos.0, pos.1), h.dentro(meta.0, meta.1));
        let u = h.umbral();
        // El punto de adentro que mira a la puerta: desde ahí se cruza recto, sin rozar la pared.
        let zaguan = (u.0, h.puerta.1 as f32 - 0.5);
        let en_la_pared = pos.1 >= h.puerta.1 as f32;
        if aqui && !alla {
            if en_la_pared {
                return (u.0, u.1 + 0.35);
            }
            return if (pos.0 - u.0).abs() > 0.2 { zaguan } else { (u.0, u.1 + 0.35) };
        }
        if aqui && alla && en_la_pared && !h.en_el_paso(pos.0, pos.1) {
            return zaguan;
        }
        if !aqui && alla {
            if h.en_el_paso(pos.0, pos.1) {
                return if pos.1 > u.1 + 0.2 { u } else { zaguan };
            }
            return h.escalon();
        }
    }
    meta
}

/// El edificio que tiene la celda `(tx, tz)`, con su puerta.
pub fn edificio_en(body: CelestialBody, s: &Settlement, tx: i64, tz: i64) -> Option<Edificio> {
    let (x, z, w, h, clave) = roofed_rect(body, s, tx, tz)?;
    if tx < x || tz < z || tx >= x + w || tz >= z + h {
        return None;
    }
    let distrito = lot_at(body, s, tx, tz)?.program().district;
    Some(Edificio { huella: Huella { min: (x, z), w, h, puerta: (puerta_del_frente(clave, x, w), z + h - 1) }, distrito, clave })
}

/// Los edificios con puerta más cercanos al centro del casco, del más cercano al más lejano: se
/// barre hasta juntar `hogares` donde dormir y `puestos` donde trabajar (o hasta `radio` tiles).
///
/// Se barre en ANILLOS crecientes y se corta apenas hay suficientes: una ciudad entera son
/// 20.000 consultas de lote (~200 ms), pero para poblarla alcanzan las primeras docenas de
/// edificios, que están a la vuelta de la plaza. La retícula es de 4 tiles (una casa no mide
/// menos) y se deduplica por esquina.
pub fn edificios_alrededor(body: CelestialBody, s: &Settlement, radio: i64, hogares: usize, puestos: usize) -> Vec<Edificio> {
    const PASO: i64 = 4;
    let (cx, cz) = s.center;
    let mut vistos: HashSet<(i64, i64)> = HashSet::new();
    let mut out: Vec<Edificio> = Vec::new();
    let mut anillo = 0;
    let (mut n_hogares, mut n_puestos) = (0usize, 0usize);
    while anillo <= radio && (n_hogares < hogares || n_puestos < puestos) {
        let mut dz = -anillo;
        while dz <= anillo {
            // En los bordes del anillo se recorre todo; adentro, sólo las dos columnas de los lados.
            let borde = dz == -anillo || dz + PASO > anillo;
            let mut dx = -anillo;
            while dx <= anillo {
                if borde || dx == -anillo || dx + PASO > anillo {
                    if let Some(e) = edificio_en(body, s, cx + dx, cz + dz) {
                        if vistos.insert(e.min) {
                            n_hogares += e.es_hogar() as usize;
                            n_puestos += e.es_puesto() as usize;
                            out.push(e);
                        }
                    }
                }
                dx += PASO;
            }
            dz += PASO;
        }
        anillo += PASO;
    }
    let d2 = |e: &Edificio| {
        let (ex, ez) = (e.min.0 + e.w / 2 - cx, e.min.1 + e.h / 2 - cz);
        ex * ex + ez * ez
    };
    out.sort_by_key(|e| (d2(e), e.min));
    out
}

/// El centro del parque (plaza, cancha, arboleda) más cercano al centro del casco, si hay.
pub fn parque_cerca(body: CelestialBody, s: &Settlement, radio: i64) -> Option<(f32, f32)> {
    let (cx, cz) = s.center;
    let mut mejor: Option<(i64, (f32, f32))> = None;
    let mut dz = -radio;
    while dz <= radio {
        let mut dx = -radio;
        while dx <= radio {
            if let Some(l) = lot_at(body, s, cx + dx, cz + dz) {
                if l.program().district == District::Park {
                    let c = (l.min.0 as f32 + l.w as f32 * 0.5, l.min.1 as f32 + l.h as f32 * 0.5);
                    let d = (c.0 as i64 - cx).pow(2) + (c.1 as i64 - cz).pow(2);
                    if mejor.map_or(true, |(m, _)| d < m) {
                        mejor = Some((d, c));
                    }
                }
            }
            dx += 3;
            }
        dz += 3;
    }
    mejor.map(|(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geography::{settlement_in, SITE_PERIOD};

    fn casco(body: CelestialBody, n: usize) -> Vec<Settlement> {
        let mut v = Vec::new();
        'busca: for sz in -3..=3 {
            for sx in -3..=3 {
                if let Some(s) = settlement_in(body, sx, sz) {
                    v.push(s);
                    if v.len() >= n {
                        break 'busca;
                    }
                }
            }
        }
        let _ = SITE_PERIOD;
        v
    }

    #[test]
    fn cada_pueblo_tiene_casas_y_sus_puertas_estan_en_su_pared() {
        let cascos = casco(CelestialBody::Earth, 4);
        assert!(!cascos.is_empty(), "la Tierra tiene pueblos");
        for s in &cascos {
            let todos = edificios_alrededor(CelestialBody::Earth, s, 260, 6, 4);
            assert!(todos.iter().any(|e| e.es_hogar()), "todo pueblo tiene dónde dormir");
            for e in &todos {
                assert_eq!(e.puerta.1, e.min.1 + e.h - 1, "la puerta está en la pared sur");
                assert!(e.puerta.0 > e.min.0 && e.puerta.0 < e.min.0 + e.w - 1 || e.w < 3);
                let (ax, az) = e.adentro(7);
                assert!(e.contiene(ax, az, 0.0));
                assert_ne!((ax.floor() as i64, az.floor() as i64), e.puerta);
            }
        }
    }

    #[test]
    fn el_escalon_de_afuera_se_puede_pisar() {
        let mut vistos = 0;
        let mut libres = 0;
        for s in casco(CelestialBody::Earth, 4) {
            for e in edificios_alrededor(CelestialBody::Earth, &s, 260, 6, 4).into_iter().filter(|e| e.es_hogar()) {
                let (ex, ez) = e.escalon();
                vistos += 1;
                if !crate::topdown::solid_at(CelestialBody::Earth, 0, ex.floor() as i64, ez.floor() as i64) {
                    libres += 1;
                }
            }
        }
        assert!(vistos > 0);
        // Un escalón sobre una cerca o un cantero es raro, no la norma: la sociedad busca los
        // que sí se pisan (`Edificio::escalon` + `solid_at`) y se salta el resto.
        assert!(libres * 10 >= vistos * 6, "sólo {libres} de {vistos} escalones libres");
    }

    #[test]
    fn el_barrido_es_determinista() {
        let s = casco(CelestialBody::Earth, 1).remove(0);
        assert_eq!(edificios_alrededor(CelestialBody::Earth, &s, 80, 6, 4), edificios_alrededor(CelestialBody::Earth, &s, 80, 6, 4));
    }

    /// Camina de `desde` a `meta` de a pasos de 0,15 m siguiendo `siguiente_punto` y sin
    /// atravesar celdas sólidas. `true` si llega a 0,6 m.
    fn camina(e: &Edificio, desde: (f32, f32), meta: (f32, f32)) -> bool {
        let (mut x, mut z) = desde;
        for _ in 0..400 {
            if ((meta.0 - x).powi(2) + (meta.1 - z).powi(2)).sqrt() < 0.6 {
                return true;
            }
            let p = siguiente_punto((x, z), meta, &[e.huella]);
            let (dx, dz) = (p.0 - x, p.1 - z);
            let d = (dx * dx + dz * dz).sqrt().max(1e-3);
            let paso = 0.15_f32.min(d);
            let (nx, nz) = (x + dx / d * paso, z + dz / d * paso);
            if crate::topdown::solid_at(CelestialBody::Earth, 0, nx.floor() as i64, nz.floor() as i64) {
                return false;
            }
            x = nx;
            z = nz;
        }
        false
    }

    #[test]
    fn de_la_calle_a_la_cama_y_de_vuelta_por_la_puerta() {
        let (mut vistos, mut entran, mut salen) = (0, 0, 0);
        for s in casco(CelestialBody::Earth, 4) {
            for e in edificios_alrededor(CelestialBody::Earth, &s, 260, 6, 4).into_iter().filter(|e| e.es_hogar()) {
                let (ex, ez) = e.escalon();
                if crate::topdown::solid_at(CelestialBody::Earth, 0, ex.floor() as i64, ez.floor() as i64) {
                    continue; // escalón sobre un cantero: la sociedad busca otro
                }
                vistos += 1;
                let cama = e.adentro(3);
                entran += camina(&e, (ex + 1.0, ez + 1.0), cama) as i32;
                salen += camina(&e, cama, (ex - 1.0, ez + 1.0)) as i32;
            }
        }
        assert!(vistos > 3);
        assert!(entran * 10 >= vistos * 8, "entran {entran} de {vistos}");
        assert!(salen * 10 >= vistos * 8, "salen {salen} de {vistos}");
    }
}
