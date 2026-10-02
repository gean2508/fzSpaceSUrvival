//! Vida tipo Los Sims — el motor de decisión de aldeanos, compañero y fauna.
//!
//! Es la CAPA ALTA del cerebro híbrido (biblia §6): decide **qué quiere hacer** cada
//! criatura; el movimiento fino (esquivar, orientarse) queda abajo. Es la receta de
//! Maxis, sin misterio:
//!
//! 1. **Motivos** (`Motivos`): barras 0..1 (1 = satisfecho) que decaen solas a ritmos
//!    distintos. Hambre, sed, sueño, social, ocio, seguridad, trabajo.
//! 2. **Objetos que se anuncian** (`Anuncio`): la criatura no "sabe" qué hace una
//!    cama; la cama dice "yo te doy +sueño". El mundo (casa, plaza, agua, pasto, otro
//!    vecino) publica anuncios y el motor los puntúa.
//! 3. **Utilidad** (`puntaje`): lo que el anuncio promete, recortado a lo que de verdad
//!    falta (`min(promesa, 1 - nivel)`), por una curva de urgencia (cuadrática: casi no
//!    importa con la barra llena, grita cuando está vacía), por la personalidad, por la
//!    cercanía. Los motivos altos (ocio, social) valen más cuanto más BIEN está el resto
//!    (Maslow): con hambre nadie quiere jugar.
//! 4. **Azar con temperatura** (`decidir`): no se elige siempre lo mejor —eso es un
//!    robot—, se sortea con Boltzmann como en Los Sims 3. Una necesidad crítica manda:
//!    ahí se elige entre lo que la resuelve.
//! 5. **Compromiso**: una decisión se cumple hasta terminar (`Plan`); sólo se corta si
//!    otra necesidad se vuelve crítica. Histéresis para que no titubee.
//!
//! Todo es determinista dado el `semilla` de cada `Vida`; sin `Instant`, sin `rand`.
//! Los tests de abajo simulan un día entero de un aldeano.

use serde::{Deserialize, Serialize};

pub const N_MOTIVOS: usize = 9;
pub const N_ACT: usize = 18;

/// Por debajo de esto, el motivo básico es una URGENCIA: sólo cuenta lo que lo resuelve.
pub const CRITICO: f32 = 0.10;
/// Por encima de esto el motivo se da por saciado y la actividad termina.
pub const SACIADO: f32 = 0.97;
/// Puntaje mínimo para que algo valga el viaje: sin necesidad no hay interés.
pub const UMBRAL: f32 = 0.03;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Motivo {
    Hambre,
    Sed,
    Sueno,
    Social,
    Ocio,
    Seguridad,
    Trabajo,
    /// Devoción: sólo decae en quien pertenece a un credo.
    Fe,
    /// Sustento material: el trabajo lo llena, el robo también.
    Riqueza,
}

impl Motivo {
    pub const TODOS: [Motivo; N_MOTIVOS] = [
        Motivo::Hambre,
        Motivo::Sed,
        Motivo::Sueno,
        Motivo::Social,
        Motivo::Ocio,
        Motivo::Seguridad,
        Motivo::Trabajo,
        Motivo::Fe,
        Motivo::Riqueza,
    ];

    #[inline]
    pub fn i(self) -> usize {
        self as usize
    }

    pub fn nombre(self) -> &'static str {
        match self {
            Motivo::Hambre => "Hambre",
            Motivo::Sed => "Sed",
            Motivo::Sueno => "Sueño",
            Motivo::Social => "Social",
            Motivo::Ocio => "Ocio",
            Motivo::Seguridad => "Seguridad",
            Motivo::Trabajo => "Trabajo",
            Motivo::Fe => "Fe",
            Motivo::Riqueza => "Riqueza",
        }
    }

    /// Los de abajo de la pirámide: urgen cuando faltan. Los otros esperan a que el
    /// resto esté bien.
    pub fn es_basico(self) -> bool {
        matches!(self, Motivo::Hambre | Motivo::Sed | Motivo::Sueno | Motivo::Seguridad)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Clase {
    Aldeano,
    Herbivoro,
    Carnivoro,
    Companero,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Actividad {
    Vagar,
    Comer,
    Beber,
    Dormir,
    Descansar,
    Charlar,
    Jugar,
    Trabajar,
    Rezar,
    Huir,
    Cazar,
    Seguir,
    Reunirse,
    Robar,
    Ayudar,
    Atacar,
    Predicar,
    /// Un buen vecino que va a detener a quien hizo daño.
    Detener,
}

impl Actividad {
    #[inline]
    pub fn i(self) -> usize {
        self as usize
    }

    /// ¿Es un trato con otro vecino (hay `tag`, y un efecto sobre él)?
    pub fn es_social(self) -> bool {
        matches!(
            self,
            Actividad::Charlar | Actividad::Ayudar | Actividad::Robar | Actividad::Atacar | Actividad::Predicar | Actividad::Detener
        )
    }

    /// Lo que se le ve hacer, en castellano, para el panel y los rótulos.
    pub fn verbo(self, clase: Clase) -> &'static str {
        match (self, clase) {
            (Actividad::Vagar, Clase::Aldeano) => "Anda sin apuro",
            (Actividad::Vagar, _) => "Anda por ahí",
            (Actividad::Comer, Clase::Aldeano) => "Va a comer",
            (Actividad::Comer, _) => "Pasta",
            (Actividad::Beber, _) => "Va a beber",
            (Actividad::Dormir, _) => "Duerme",
            (Actividad::Descansar, _) => "Descansa",
            (Actividad::Charlar, Clase::Aldeano | Clase::Companero) => "Charla",
            (Actividad::Charlar, _) => "Busca a la manada",
            (Actividad::Jugar, Clase::Aldeano) => "Pasea",
            (Actividad::Jugar, Clase::Companero) => "Merodea",
            (Actividad::Jugar, _) => "Explora",
            (Actividad::Trabajar, _) => "Trabaja",
            (Actividad::Rezar, _) => "Reza",
            (Actividad::Huir, _) => "Huye",
            (Actividad::Cazar, Clase::Carnivoro) => "Acecha",
            (Actividad::Cazar, _) => "Caza",
            (Actividad::Seguir, _) => "Te acompaña",
            (Actividad::Reunirse, _) => "Se junta con el pueblo",
            (Actividad::Robar, _) => "Roba",
            (Actividad::Ayudar, _) => "Ayuda a un vecino",
            (Actividad::Atacar, _) => "Agrede",
            (Actividad::Predicar, _) => "Predica",
            (Actividad::Detener, _) => "Detiene a un malhechor",
        }
    }
}

/// Fase del día (0 = medianoche, 0.5 = mediodía) → cuánto de noche es (0..1).
pub fn noche(fase: f32) -> f32 {
    let x = fase.rem_euclid(1.0);
    if !(0.20..0.86).contains(&x) {
        1.0
    } else if x < 0.27 {
        1.0 - (x - 0.20) / 0.07
    } else if x < 0.78 {
        0.0
    } else {
        (x - 0.78) / 0.08
    }
}

/// Cuánto de horario laboral es (0..1): de la mañana al atardecer, con rampas.
pub fn jornada(fase: f32) -> f32 {
    let x = fase.rem_euclid(1.0);
    if !(0.27..0.78).contains(&x) {
        0.0
    } else if x < 0.30 {
        (x - 0.27) / 0.03
    } else if x < 0.74 {
        1.0
    } else {
        1.0 - (x - 0.74) / 0.04
    }
}

/// Hora del corro de mediodía (plaza y santuario): un evento social del pueblo.
pub fn hora_de_plaza(fase: f32) -> bool {
    (0.45..0.55).contains(&fase.rem_euclid(1.0))
}

// ---------------------------------------------------------------------------
// Perfil por clase: cuánto decae cada motivo y cuáles existen.
// ---------------------------------------------------------------------------

struct Perfil {
    /// Por segundo de juego. 0 = no decae solo (lo fija el mundo o no existe).
    decae: [f32; N_MOTIVOS],
    activos: [bool; N_MOTIVOS],
}

fn perfil(c: Clase) -> Perfil {
    match c {
        // Un día son 3600 s: hambre cada ~25 min, sed ~18, sueño ~50 (y de noche el
        // doble de rápido), social ~12, ocio ~15, trabajo sólo en jornada.
        Clase::Aldeano => Perfil {
            decae: [1.0 / 1500.0, 1.0 / 1100.0, 1.0 / 5500.0, 1.0 / 700.0, 1.0 / 900.0, 0.0, 1.0 / 900.0, 1.0 / 1500.0, 1.0 / 2400.0],
            activos: [true; N_MOTIVOS],
        },
        // Hambre y sed del animal las lleva su metabolismo (`fauna.rs`): acá sólo el resto.
        Clase::Herbivoro => Perfil {
            decae: [0.0, 0.0, 1.0 / 1800.0, 1.0 / 300.0, 1.0 / 240.0, 0.0, 0.0, 0.0, 0.0],
            activos: [true, true, true, true, true, true, false, false, false],
        },
        Clase::Carnivoro => Perfil {
            decae: [0.0, 0.0, 1.0 / 1800.0, 0.0, 1.0 / 300.0, 0.0, 0.0, 0.0, 0.0],
            activos: [true, true, true, false, true, true, false, false, false],
        },
        Clase::Companero => Perfil {
            decae: [0.0, 0.0, 1.0 / 2400.0, 1.0 / 400.0, 1.0 / 500.0, 0.0, 0.0, 0.0, 0.0],
            activos: [false, false, true, true, true, true, false, false, false],
        },
    }
}

// ---------------------------------------------------------------------------
// Motivos y personalidad
// ---------------------------------------------------------------------------

pub fn hash(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn unit(x: u64) -> f32 {
    (hash(x) >> 40) as f32 / (1u64 << 24) as f32
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub struct Motivos {
    pub n: [f32; N_MOTIVOS],
}

impl Default for Motivos {
    fn default() -> Self {
        Self { n: [1.0; N_MOTIVOS] }
    }
}

impl Motivos {
    /// Cada criatura arranca con barras distintas: si no, todo el pueblo tiene sed a la
    /// vez y se forma una cola al pozo.
    pub fn sembrar(semilla: u64) -> Self {
        let mut n = [1.0; N_MOTIVOS];
        for (i, v) in n.iter_mut().enumerate() {
            *v = 0.55 + 0.45 * unit(semilla.wrapping_mul(31).wrapping_add(i as u64));
        }
        n[Motivo::Seguridad.i()] = 1.0;
        n[Motivo::Trabajo.i()] = 1.0;
        n[Motivo::Fe.i()] = 1.0;
        Self { n }
    }

    #[inline]
    pub fn get(&self, m: Motivo) -> f32 {
        self.n[m.i()]
    }

    #[inline]
    pub fn set(&mut self, m: Motivo, v: f32) {
        self.n[m.i()] = v.clamp(0.0, 1.0);
    }

    /// Promedio de los motivos básicos que existen para esta clase (0..1).
    pub fn bienestar(&self, clase: Clase) -> f32 {
        let p = perfil(clase);
        let (mut s, mut k) = (0.0, 0.0);
        for m in Motivo::TODOS {
            if m.es_basico() && p.activos[m.i()] {
                s += self.n[m.i()];
                k += 1.0;
            }
        }
        if k > 0.0 {
            s / k
        } else {
            1.0
        }
    }

    /// El motivo activo que más urge (el de nivel más bajo).
    pub fn peor(&self, clase: Clase) -> (Motivo, f32) {
        let p = perfil(clase);
        let mut mejor = (Motivo::Ocio, 1.0);
        for m in Motivo::TODOS {
            if p.activos[m.i()] && self.n[m.i()] < mejor.1 {
                mejor = (m, self.n[m.i()]);
            }
        }
        mejor
    }
}

/// Peso por motivo (0.6..1.4): el glotón, el sociable, el vago. Se hereda con deriva.
#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub struct Personalidad {
    pub w: [f32; N_MOTIVOS],
    /// -1 (cruel) .. +1 (generoso). Manda sobre qué actividades se permite.
    pub moral: f32,
    /// 0..1: cuánto convence a otros (recluta, funda credos).
    pub carisma: f32,
}

impl Default for Personalidad {
    fn default() -> Self {
        Self { w: [1.0; N_MOTIVOS], moral: 0.3, carisma: 0.5 }
    }
}

impl Personalidad {
    pub fn sembrar(semilla: u64) -> Self {
        Self::sembrar_con(semilla, 0.0)
    }

    /// `tendencia` desplaza la moral media del lugar: un mundo hostil cría más gente dura.
    pub fn sembrar_con(semilla: u64, tendencia: f32) -> Self {
        let mut w = [1.0; N_MOTIVOS];
        for (i, v) in w.iter_mut().enumerate() {
            *v = 0.6 + 0.8 * unit(semilla.wrapping_mul(131).wrapping_add(977 + i as u64));
        }
        // Triangular: la mayoría en el medio, pocos santos y pocos monstruos.
        let tri = unit(semilla.wrapping_add(5001)) + unit(semilla.wrapping_add(5002)) - 1.0;
        let moral = (0.25 + tendencia + tri * 0.9).clamp(-1.0, 1.0);
        // El codicioso lo es más cuanto peor persona; el bueno pesa más el vínculo.
        w[Motivo::Riqueza.i()] = (w[Motivo::Riqueza.i()] + (-moral).max(0.0) * 0.6).clamp(0.5, 1.6);
        w[Motivo::Social.i()] = (w[Motivo::Social.i()] + moral.max(0.0) * 0.3).clamp(0.5, 1.6);
        let carisma = unit(semilla.wrapping_add(6003));
        Self { w, moral, carisma }
    }

    /// Hijo de DOS: cada gen sale de uno u otro (crossover uniforme, elegido por la semilla), con
    /// una deriva chica encima. La moral y el carisma se mezclan en vez de elegirse.
    pub fn cruzar(a: &Personalidad, b: &Personalidad, semilla: u64, deriva: f32) -> Self {
        let mut w = a.w;
        for (i, v) in w.iter_mut().enumerate() {
            let base = if unit(semilla.wrapping_add(300 + i as u64)) < 0.5 { a.w[i] } else { b.w[i] };
            let r = unit(semilla.wrapping_add(7 + i as u64)) * 2.0 - 1.0;
            *v = (base + r * deriva).clamp(0.5, 1.5);
        }
        let rm = unit(semilla.wrapping_add(91)) * 2.0 - 1.0;
        let rc = unit(semilla.wrapping_add(92)) * 2.0 - 1.0;
        let mezcla = 0.35 + 0.3 * unit(semilla.wrapping_add(93));
        Self {
            w,
            moral: (a.moral * mezcla + b.moral * (1.0 - mezcla) + rm * deriva * 2.0).clamp(-1.0, 1.0),
            carisma: (a.carisma * mezcla + b.carisma * (1.0 - mezcla) + rc * deriva).clamp(0.0, 1.0),
        }
    }

    /// Hijo de `padre`: mismo carácter con una deriva chica.
    pub fn heredar(padre: &Personalidad, semilla: u64, deriva: f32) -> Self {
        let mut w = padre.w;
        for (i, v) in w.iter_mut().enumerate() {
            let r = unit(semilla.wrapping_add(7 + i as u64)) * 2.0 - 1.0;
            *v = (*v + r * deriva).clamp(0.5, 1.5);
        }
        let rm = unit(semilla.wrapping_add(91)) * 2.0 - 1.0;
        let rc = unit(semilla.wrapping_add(92)) * 2.0 - 1.0;
        Self {
            w,
            moral: (padre.moral + rm * deriva * 2.0).clamp(-1.0, 1.0),
            carisma: (padre.carisma + rc * deriva).clamp(0.0, 1.0),
        }
    }
}

// ---------------------------------------------------------------------------
// Anuncios y planes
// ---------------------------------------------------------------------------

/// Carga moral de una actividad: lo que hace quien la elige y cómo la mira su conciencia.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tipo {
    Neutro,
    /// Robar, agredir: sólo lo elige quien es capaz (o está desesperado).
    Antisocial,
    /// Ayudar, detener a un malhechor: lo elige quien tiene buen corazón.
    Prosocial,
}

/// Lo que un objeto/lugar/vecino ofrece. Se arma en el momento de decidir con lo que
/// hay a la vista; no se guarda.
#[derive(Clone, Copy, Debug)]
pub struct Anuncio {
    pub act: Actividad,
    pub x: f32,
    pub z: f32,
    /// A esta distancia ya "lo está usando".
    pub uso_r: f32,
    /// Motivo que sube por segundo mientras lo usa.
    pub gana: [f32; N_MOTIVOS],
    /// Cuánto puede durar (s).
    pub dur: f32,
    /// Más lejos que esto ni se considera; más cerca puntúa más.
    pub alcance: f32,
    /// Empujón fijo (evento social, lealtad, tradición).
    pub bono: f32,
    /// A quién apunta (uid del vecino, del bocado…). 0 = a nadie.
    pub tag: u64,
    /// Sigue hasta que amanezca (dormir).
    pub hasta_dia: bool,
    pub tipo: Tipo,
    /// 0..1: probabilidad de que lo pesquen (testigos, guardias). Sólo pesa en lo antisocial.
    pub riesgo: f32,
}

impl Anuncio {
    pub fn nuevo(act: Actividad, x: f32, z: f32) -> Self {
        Self {
            act,
            x,
            z,
            uso_r: 1.5,
            gana: [0.0; N_MOTIVOS],
            dur: 20.0,
            alcance: 60.0,
            bono: 0.0,
            tag: 0,
            hasta_dia: false,
            tipo: Tipo::Neutro,
            riesgo: 0.0,
        }
    }
    pub fn tipo(mut self, t: Tipo) -> Self {
        self.tipo = t;
        self
    }
    pub fn riesgo(mut self, r: f32) -> Self {
        self.riesgo = r;
        self
    }
    pub fn gana(mut self, m: Motivo, por_s: f32) -> Self {
        self.gana[m.i()] = por_s;
        self
    }
    pub fn dur(mut self, s: f32) -> Self {
        self.dur = s;
        self
    }
    pub fn alcance(mut self, m: f32) -> Self {
        self.alcance = m;
        self
    }
    pub fn uso(mut self, r: f32) -> Self {
        self.uso_r = r;
        self
    }
    pub fn bono(mut self, b: f32) -> Self {
        self.bono = b;
        self
    }
    pub fn tag(mut self, t: u64) -> Self {
        self.tag = t;
        self
    }
    pub fn hasta_dia(mut self) -> Self {
        self.hasta_dia = true;
        self
    }
}

/// La decisión en curso.
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub act: Actividad,
    pub x: f32,
    pub z: f32,
    pub uso_r: f32,
    pub gana: [f32; N_MOTIVOS],
    pub resta: f32,
    pub tag: u64,
    pub hasta_dia: bool,
    pub llego: bool,
    /// Ya se aplicó el efecto sobre el otro (una sola vez por plan).
    pub hecho: bool,
    /// Segundos caminando hacia ahí (para abandonar si no llega).
    pub viaje: f32,
}

impl Plan {
    fn de(a: &Anuncio) -> Self {
        Self {
            act: a.act,
            x: a.x,
            z: a.z,
            uso_r: a.uso_r,
            gana: a.gana,
            resta: a.dur,
            tag: a.tag,
            hasta_dia: a.hasta_dia,
            llego: false,
            hecho: false,
            viaje: 0.0,
        }
    }

    /// El motivo que más le sube: el que da por terminada la actividad.
    pub fn principal(&self) -> Option<Motivo> {
        let mut mejor: Option<(Motivo, f32)> = None;
        for m in Motivo::TODOS {
            let g = self.gana[m.i()];
            if g > 0.0 && mejor.map_or(true, |(_, b)| g > b) {
                mejor = Some((m, g));
            }
        }
        mejor.map(|(m, _)| m)
    }
}

// ---------------------------------------------------------------------------
// Utilidad
// ---------------------------------------------------------------------------

/// Cuánto importa un motivo según cómo está. Cuadrática: llena → casi nada, vacía → grita.
fn urgencia(m: Motivo, nivel: f32, bienestar: f32) -> f32 {
    let falta = 1.0 - nivel;
    let base = 0.25 + 4.0 * falta * falta;
    if m.es_basico() {
        base + if nivel < CRITICO { 3.0 } else { 0.0 }
    } else {
        // Maslow: ocio, social y trabajo sólo se sienten cuando lo básico está cubierto.
        base * (0.1 + 0.9 * bienestar * bienestar)
    }
}

/// Puntaje de un anuncio para quien está en `pos`. 0 si no le sirve o está fuera de alcance.
pub fn puntaje(clase: Clase, m: &Motivos, pers: &Personalidad, a: &Anuncio, pos: (f32, f32), fase: f32) -> f32 {
    let d = ((a.x - pos.0).powi(2) + (a.z - pos.1).powi(2)).sqrt();
    if d > a.alcance {
        return 0.0;
    }
    let p = perfil(clase);
    let bien = m.bienestar(clase);
    let mut s = 0.0;
    for mo in Motivo::TODOS {
        let i = mo.i();
        if !p.activos[i] || a.gana[i] <= 0.0 {
            continue;
        }
        let util = (a.gana[i] * a.dur).min(1.0 - m.n[i]);
        s += util * urgencia(mo, m.n[i], bien) * pers.w[i];
    }
    if s <= 0.0 && a.bono <= 0.0 {
        return 0.0;
    }
    let cerca = 0.15 + 0.85 * (1.0 - d / a.alcance.max(0.1));
    // Ritmo circadiano: la cama tienta de noche y casi nada de día.
    let ritmo = if a.act == Actividad::Dormir { 0.1 + 1.5 * noche(fase) * noche(fase) } else { 1.0 };
    let total = s * cerca * ritmo + a.bono;
    total * filtro_moral(m, pers, a)
}

/// La conciencia: cuánto deja pasar el carácter de la actividad. El santo no roba ni
/// muerto de hambre —salvo que de verdad se esté muriendo—; el cruel casi no lo piensa.
pub fn filtro_moral(m: &Motivos, pers: &Personalidad, a: &Anuncio) -> f32 {
    match a.tipo {
        Tipo::Neutro => 1.0,
        Tipo::Antisocial => {
            let desesperado = m.get(Motivo::Hambre) < CRITICO && a.gana[Motivo::Hambre.i()] > 0.0;
            let f = if pers.moral < -0.1 {
                0.4 + (-pers.moral) * 1.2
            } else if desesperado {
                0.25
            } else {
                ((0.3 - pers.moral) * 0.15).max(0.0)
            };
            f * (1.0 - a.riesgo.clamp(0.0, 0.95))
        }
        Tipo::Prosocial => (pers.moral + 0.3).clamp(0.0, 1.3),
    }
}

// ---------------------------------------------------------------------------
// Vida: el estado de una criatura
// ---------------------------------------------------------------------------

pub const N_LAZOS: usize = 6;

/// Memoria de trato con otros vecinos: `-1` enemigo jurado .. `+1` amigo del alma.
/// Guarda pocos (los que más pesan); lo demás se olvida, como en la vida.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, Default)]
pub struct Lazos {
    pub id: [u64; N_LAZOS],
    pub af: [f32; N_LAZOS],
}

impl Lazos {
    pub fn get(&self, uid: u64) -> f32 {
        if uid == 0 {
            return 0.0;
        }
        (0..N_LAZOS).find(|&i| self.id[i] == uid).map_or(0.0, |i| self.af[i])
    }

    pub fn sumar(&mut self, uid: u64, d: f32) {
        if uid == 0 {
            return;
        }
        let i = (0..N_LAZOS).find(|&i| self.id[i] == uid).unwrap_or_else(|| {
            (0..N_LAZOS)
                .find(|&i| self.id[i] == 0)
                .unwrap_or_else(|| (0..N_LAZOS).min_by(|&a, &b| self.af[a].abs().total_cmp(&self.af[b].abs())).unwrap_or(0))
        });
        if self.id[i] != uid {
            self.id[i] = uid;
            self.af[i] = 0.0;
        }
        self.af[i] = (self.af[i] + d).clamp(-1.0, 1.0);
    }

    /// El olvido: el cariño y el rencor se enfrían despacio.
    pub fn enfriar(&mut self, dt: f32) {
        for i in 0..N_LAZOS {
            if self.id[i] != 0 {
                self.af[i] -= self.af[i].signum() * (dt / 5400.0).min(self.af[i].abs());
                if self.af[i].abs() < 0.01 {
                    self.id[i] = 0;
                    self.af[i] = 0.0;
                }
            }
        }
    }

    /// El que más quiere (`> 0`), si hay.
    pub fn amigo(&self) -> Option<(u64, f32)> {
        (0..N_LAZOS).filter(|&i| self.id[i] != 0 && self.af[i] > 0.25).max_by(|&a, &b| self.af[a].total_cmp(&self.af[b])).map(|i| (self.id[i], self.af[i]))
    }

    /// El que más odia, si hay.
    pub fn enemigo(&self) -> Option<(u64, f32)> {
        (0..N_LAZOS).filter(|&i| self.id[i] != 0 && self.af[i] < -0.35).min_by(|&a, &b| self.af[a].total_cmp(&self.af[b])).map(|i| (self.id[i], self.af[i]))
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub struct Vida {
    pub m: Motivos,
    pub pers: Personalidad,
    /// Credo al que pertenece (0 = ninguno).
    #[serde(default)]
    pub credo: u16,
    /// 0..1: fama de malhechor. Los vecinos desconfían y los buenos lo van a detener.
    #[serde(default)]
    pub mala_fama: f32,
    #[serde(default)]
    pub lazos: Lazos,
    #[serde(skip)]
    pub plan: Option<Plan>,
    /// Segundos que le faltan a cada actividad para volver a considerarse (se traba, se aburre).
    #[serde(skip)]
    pub veda: [f32; N_ACT],
    #[serde(skip)]
    pub dec_cd: f32,
    #[serde(skip)]
    pub sem: u32,
}

impl Default for Vida {
    fn default() -> Self {
        Self::nueva(1)
    }
}

impl Vida {
    pub fn nueva(uid: u64) -> Self {
        Self::nueva_con(uid, 0.0)
    }

    pub fn nueva_con(uid: u64, tendencia: f32) -> Self {
        Self {
            m: Motivos::sembrar(uid),
            pers: Personalidad::sembrar_con(uid, tendencia),
            credo: 0,
            mala_fama: 0.0,
            lazos: Lazos::default(),
            plan: None,
            veda: [0.0; N_ACT],
            dec_cd: 0.0,
            sem: (hash(uid ^ 0xA5A5) as u32) | 1,
        }
    }

    /// Cría de `padre`: carácter heredado, barras propias.
    pub fn hija_de(padre: &Vida, uid: u64) -> Self {
        let mut v = Vida::nueva(uid);
        v.pers = Personalidad::heredar(&padre.pers, uid, 0.12);
        v.credo = padre.credo; // se cría en la fe de la casa
        v
    }

    /// Cría de una pareja: carácter cruzado de los dos; se cría en el credo de la madre (o el del
    /// padre si ella no tiene).
    pub fn hija_de_pareja(madre: &Vida, padre: &Vida, uid: u64) -> Self {
        let mut v = Vida::nueva(uid);
        v.pers = Personalidad::cruzar(&madre.pers, &padre.pers, uid, 0.12);
        v.credo = if madre.credo != 0 { madre.credo } else { padre.credo };
        v
    }

    fn azar(&mut self) -> f32 {
        let mut x = if self.sem == 0 { 0x9E37_79B9 } else { self.sem };
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.sem = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Avanza `dt`: las barras decaen; si ya llegó a lo que iba, se llenan.
    pub fn tick(&mut self, clase: Clase, fase: f32, dt: f32, pos: (f32, f32)) {
        let p = perfil(clase);
        let (noc, jor) = (noche(fase), jornada(fase));
        // Dormido el cuerpo gasta poco: si no, la cama nunca alcanza.
        let dormido = self.plan.map_or(false, |pl| pl.act == Actividad::Dormir && pl.llego);
        for m in Motivo::TODOS {
            let i = m.i();
            if !p.activos[i] || p.decae[i] <= 0.0 {
                continue;
            }
            if m == Motivo::Fe && self.credo == 0 {
                continue; // sin credo no hay devoción que se apague
            }
            let mut r = p.decae[i] * if dormido { 0.2 } else { 1.0 };
            match m {
                Motivo::Sueno => r *= 1.0 + 1.5 * noc,
                Motivo::Trabajo => r *= jor,
                _ => {}
            }
            self.m.n[i] = (self.m.n[i] - r * dt).max(0.0);
        }
        // La sensación de seguridad vuelve sola si nadie la amenaza.
        if p.activos[Motivo::Seguridad.i()] {
            let i = Motivo::Seguridad.i();
            self.m.n[i] = (self.m.n[i] + dt / 400.0).min(1.0);
        }
        self.dec_cd -= dt;
        self.mala_fama = (self.mala_fama - dt / 1800.0).max(0.0);
        self.lazos.enfriar(dt);
        for v in self.veda.iter_mut() {
            *v = (*v - dt).max(0.0);
        }
        if let Some(pl) = &mut self.plan {
            if !pl.llego {
                pl.viaje += dt;
                let d2 = (pl.x - pos.0).powi(2) + (pl.z - pos.1).powi(2);
                if d2 <= pl.uso_r * pl.uso_r {
                    pl.llego = true;
                }
            }
            if pl.llego {
                pl.resta -= dt;
                for i in 0..N_MOTIVOS {
                    if pl.gana[i] > 0.0 {
                        self.m.n[i] = (self.m.n[i] + pl.gana[i] * dt).min(1.0);
                    }
                }
            }
        }
    }

    /// ¿Terminó (o hay que abandonar) lo que hacía?
    pub fn terminado(&self, fase: f32) -> bool {
        let Some(pl) = &self.plan else { return true };
        if pl.viaje > 150.0 {
            return true;
        }
        if !pl.llego {
            return false;
        }
        if pl.resta <= 0.0 {
            return true;
        }
        if let Some(m) = pl.principal() {
            if self.m.get(m) >= SACIADO {
                // Dormir no termina a las 3 de la mañana sólo porque ya descansó.
                return !(pl.hasta_dia && noche(fase) > 0.7);
            }
        }
        false
    }

    /// El motivo básico crítico que la actividad actual NO está resolviendo, si lo hay.
    fn urgencia_ajena(&self, clase: Clase) -> Option<Motivo> {
        let p = perfil(clase);
        for m in Motivo::TODOS {
            if m.es_basico() && p.activos[m.i()] && self.m.get(m) < CRITICO {
                let resuelve = self.plan.map_or(false, |pl| pl.llego && pl.gana[m.i()] > 0.0);
                if !resuelve {
                    return Some(m);
                }
            }
        }
        None
    }

    /// ¿Toca elegir de nuevo? Sin plan, con plan terminado, por una urgencia ajena, o de
    /// vez en cuando para que una mejor oferta pueda ganarle a lo que hace (no si duerme).
    pub fn necesita_decidir(&self, clase: Clase, fase: f32) -> bool {
        if self.terminado(fase) {
            return true;
        }
        if self.urgencia_ajena(clase).is_some() {
            return true;
        }
        let durmiendo = self.plan.map_or(false, |p| p.act == Actividad::Dormir && p.llego);
        self.dec_cd <= 0.0 && !durmiendo
    }

    /// Elige entre los anuncios y arma el plan. Devuelve la actividad elegida.
    pub fn decidir(&mut self, clase: Clase, pos: (f32, f32), fase: f32, ads: &[Anuncio]) -> Actividad {
        self.dec_cd = 3.0 + 3.0 * self.azar();
        let critico = self.urgencia_ajena(clase);
        let vigente = if self.terminado(fase) { None } else { self.plan };

        let mut puntos: Vec<(usize, f32)> = Vec::with_capacity(ads.len());
        for (k, a) in ads.iter().enumerate() {
            if self.veda[a.act.i()] > 0.0 {
                continue;
            }
            if let Some(cr) = critico {
                if a.gana[cr.i()] <= 0.0 {
                    continue;
                }
            }
            let mut s = puntaje(clase, &self.m, &self.pers, a, pos, fase);
            if let Some(v) = vigente {
                if v.act == a.act && v.tag == a.tag {
                    s *= 1.3; // histéresis
                }
            }
            if s > UMBRAL {
                puntos.push((k, s));
            }
        }
        // Una urgencia sin oferta que la resuelva no paraliza: se elige entre todo.
        if puntos.is_empty() && critico.is_some() {
            for (k, a) in ads.iter().enumerate() {
                if self.veda[a.act.i()] > 0.0 {
                    continue;
                }
                let s = puntaje(clase, &self.m, &self.pers, a, pos, fase);
                if s > UMBRAL {
                    puntos.push((k, s));
                }
            }
        }
        if puntos.is_empty() {
            self.plan = Some(Plan {
                act: Actividad::Vagar,
                x: pos.0,
                z: pos.1,
                uso_r: 1e6,
                gana: [0.0; N_MOTIVOS],
                resta: 4.0 + 4.0 * self.azar(),
                tag: 0,
                hasta_dia: false,
                llego: true,
                hecho: false,
                viaje: 0.0,
            });
            return Actividad::Vagar;
        }
        let mejor = puntos.iter().map(|p| p.1).fold(0.0f32, f32::max);
        let elegido = if critico.is_some() {
            puntos.iter().copied().fold((0usize, 0.0f32), |b, p| if p.1 > b.1 { p } else { b }).0
        } else {
            // Boltzmann: a más oferta buena, menos azar relativo.
            let t = (0.35 * mejor).max(0.02);
            let pesos: Vec<f32> = puntos.iter().map(|p| ((p.1 - mejor) / t).exp()).collect();
            let total: f32 = pesos.iter().sum();
            let mut r = self.azar() * total;
            let mut k = puntos[0].0;
            for (i, w) in pesos.iter().enumerate() {
                if r <= *w {
                    k = puntos[i].0;
                    break;
                }
                r -= *w;
            }
            k
        };
        let a = &ads[elegido];
        let sigue = vigente.map_or(false, |v| v.act == a.act && v.tag == a.tag);
        if !sigue {
            self.plan = Some(Plan::de(a));
        }
        a.act
    }

    /// Si ya llegó a un trato con otro y todavía no lo aplicó, lo marca como aplicado y
    /// devuelve `(actividad, uid del otro)`. Una vez por plan.
    pub fn por_resolver(&mut self) -> Option<(Actividad, u64)> {
        let p = self.plan.as_mut()?;
        if p.llego && !p.hecho && p.act.es_social() && p.tag != 0 {
            p.hecho = true;
            return Some((p.act, p.tag));
        }
        None
    }

    /// El otro se movió: el destino lo sigue. Si ya no está, el plan se cae.
    pub fn apuntar(&mut self, x: f32, z: f32) {
        if let Some(p) = &mut self.plan {
            p.x = x;
            p.z = z;
        }
    }

    pub fn abandonar(&mut self) {
        self.plan = None;
        self.dec_cd = 0.0;
    }

    /// A quién apunta el plan actual (0 = a nadie).
    pub fn objetivo(&self) -> u64 {
        self.plan.map_or(0, |p| if p.act.es_social() { p.tag } else { 0 })
    }

    /// Lo que hace ahora (o `Vagar`).
    pub fn actividad(&self) -> Actividad {
        self.plan.map_or(Actividad::Vagar, |p| p.act)
    }

    /// ¿Está usando el objeto (ya llegó)?
    pub fn usando(&self) -> bool {
        self.plan.map_or(false, |p| p.llego)
    }

    /// Adónde va, si va a alguna parte: `(x, z, llegó)`.
    pub fn destino(&self) -> Option<(f32, f32, bool)> {
        let p = self.plan?;
        if p.act == Actividad::Vagar {
            return None;
        }
        Some((p.x, p.z, p.llego))
    }

    /// Se topó con algo yendo hacia ahí: si insiste, se rinde y prueba otra cosa un rato.
    pub fn trabar(&mut self) {
        if let Some(p) = &mut self.plan {
            if !p.llego {
                p.viaje += 8.0;
                if p.viaje > 24.0 {
                    self.veda[p.act.i()] = 25.0;
                    p.viaje = 1000.0; // forzar re-decisión
                }
            }
        }
    }

    /// Cambia el plan por uno impuesto de afuera (una orden, un susto) sin perder las barras.
    pub fn imponer(&mut self, a: &Anuncio) {
        self.plan = Some(Plan::de(a));
        self.dec_cd = 3.0;
    }

    /// Frase corta que dice el motivo más apremiante, o `None` si está bien.
    pub fn queja(&self, clase: Clase) -> Option<&'static str> {
        let (m, v) = self.m.peor(clase);
        if v > 0.22 {
            return None;
        }
        Some(match m {
            Motivo::Hambre => "Tengo hambre...",
            Motivo::Sed => "Qué sed tengo.",
            Motivo::Sueno => "Me caigo de sueño.",
            Motivo::Social => "Me siento solo.",
            Motivo::Ocio => "Qué aburrimiento.",
            Motivo::Seguridad => "Esto no me gusta.",
            Motivo::Trabajo => "Se me junta el trabajo.",
            Motivo::Fe => "Necesito rezar.",
            Motivo::Riqueza => "Estoy en la ruina.",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noche_y_jornada_son_coherentes() {
        assert_eq!(noche(0.0), 1.0);
        assert_eq!(noche(0.5), 0.0);
        assert!(noche(0.24) > 0.0 && noche(0.24) < 1.0);
        assert_eq!(jornada(0.1), 0.0);
        assert_eq!(jornada(0.5), 1.0);
        assert_eq!(jornada(0.9), 0.0);
        assert!(hora_de_plaza(0.5) && !hora_de_plaza(0.6));
    }

    #[test]
    fn lo_lleno_no_puntua_y_lo_vacio_grita() {
        let p = Personalidad::default();
        let comida = Anuncio::nuevo(Actividad::Comer, 0.0, 0.0).gana(Motivo::Hambre, 0.03).dur(25.0);
        let mut m = Motivos::default();
        assert_eq!(puntaje(Clase::Aldeano, &m, &p, &comida, (0.0, 0.0), 0.5), 0.0, "lleno: la heladera no dice nada");
        m.set(Motivo::Hambre, 0.5);
        let medio = puntaje(Clase::Aldeano, &m, &p, &comida, (0.0, 0.0), 0.5);
        m.set(Motivo::Hambre, 0.05);
        let vacio = puntaje(Clase::Aldeano, &m, &p, &comida, (0.0, 0.0), 0.5);
        assert!(vacio > medio * 2.0, "la urgencia crece más que lineal ({medio} vs {vacio})");
    }

    #[test]
    fn lo_cercano_gana_a_lo_lejano() {
        let p = Personalidad::default();
        let mut m = Motivos::default();
        m.set(Motivo::Sed, 0.3);
        let agua = |x| Anuncio::nuevo(Actividad::Beber, x, 0.0).gana(Motivo::Sed, 0.08).dur(10.0).alcance(50.0);
        let cerca = puntaje(Clase::Aldeano, &m, &p, &agua(3.0), (0.0, 0.0), 0.5);
        let lejos = puntaje(Clase::Aldeano, &m, &p, &agua(40.0), (0.0, 0.0), 0.5);
        assert!(cerca > lejos);
        assert_eq!(puntaje(Clase::Aldeano, &m, &p, &agua(60.0), (0.0, 0.0), 0.5), 0.0, "fuera de alcance");
    }

    #[test]
    fn con_hambre_nadie_quiere_jugar() {
        let p = Personalidad::default();
        let juego = Anuncio::nuevo(Actividad::Jugar, 5.0, 0.0).gana(Motivo::Ocio, 0.02).dur(40.0);
        let mut bien = Motivos::default();
        bien.set(Motivo::Ocio, 0.2);
        let mut mal = bien;
        mal.set(Motivo::Hambre, 0.1);
        mal.set(Motivo::Sed, 0.2);
        assert!(
            puntaje(Clase::Aldeano, &bien, &p, &juego, (0.0, 0.0), 0.5)
                > puntaje(Clase::Aldeano, &mal, &p, &juego, (0.0, 0.0), 0.5) * 1.5
        );
    }

    #[test]
    fn la_urgencia_critica_manda() {
        let mut v = Vida::nueva(7);
        v.m.n = [1.0; N_MOTIVOS];
        v.m.set(Motivo::Sed, 0.05);
        v.m.set(Motivo::Ocio, 0.0);
        let ads = [
            Anuncio::nuevo(Actividad::Jugar, 1.0, 0.0).gana(Motivo::Ocio, 0.05).dur(30.0).bono(5.0),
            Anuncio::nuevo(Actividad::Beber, 30.0, 0.0).gana(Motivo::Sed, 0.08).dur(10.0).alcance(50.0),
        ];
        for _ in 0..20 {
            assert_eq!(v.decidir(Clase::Aldeano, (0.0, 0.0), 0.5, &ads), Actividad::Beber);
        }
    }

    #[test]
    fn un_plan_se_cumple_y_termina() {
        let mut v = Vida::nueva(3);
        v.m.set(Motivo::Sed, 0.3);
        let ads = [Anuncio::nuevo(Actividad::Beber, 0.0, 0.0).gana(Motivo::Sed, 0.1).dur(20.0)];
        v.decidir(Clase::Aldeano, (0.0, 0.0), 0.5, &ads);
        assert_eq!(v.actividad(), Actividad::Beber);
        assert!(!v.terminado(0.5));
        for _ in 0..12 {
            v.tick(Clase::Aldeano, 0.5, 1.0, (0.0, 0.0));
        }
        assert!(v.terminado(0.5), "la sed se saciaría: {:?}", v.m.get(Motivo::Sed));
    }

    #[test]
    fn la_personalidad_hereda_con_deriva() {
        let a = Personalidad::sembrar(42);
        let h = Personalidad::heredar(&a, 43, 0.1);
        let mut dif = 0.0;
        for i in 0..N_MOTIVOS {
            assert!((a.w[i] - h.w[i]).abs() <= 0.1 + 1e-5);
            dif += (a.w[i] - h.w[i]).abs();
        }
        assert!(dif > 0.0);
    }

    // --- Un día entero de un aldeano en un pueblo de juguete -----------------

    struct Mundo {
        casa: (f32, f32),
        trabajo: (f32, f32),
        plaza: (f32, f32),
        agua: (f32, f32),
    }

    fn anuncios(mu: &Mundo, fase: f32, v: &Vida, vecino: Option<(f32, f32, u64)>) -> Vec<Anuncio> {
        let mut a = vec![
            Anuncio::nuevo(Actividad::Dormir, mu.casa.0, mu.casa.1)
                .gana(Motivo::Sueno, 0.0006)
                .dur(1600.0)
                .alcance(200.0)
                .uso(2.5)
                .hasta_dia(),
            Anuncio::nuevo(Actividad::Comer, mu.casa.0, mu.casa.1).gana(Motivo::Hambre, 0.03).dur(25.0).alcance(200.0),
            Anuncio::nuevo(Actividad::Trabajar, mu.trabajo.0, mu.trabajo.1)
                .gana(Motivo::Trabajo, 0.004)
                .dur(600.0)
                .alcance(200.0),
            Anuncio::nuevo(Actividad::Beber, mu.agua.0, mu.agua.1).gana(Motivo::Sed, 0.08).dur(10.0).alcance(80.0),
            Anuncio::nuevo(Actividad::Beber, mu.casa.0, mu.casa.1).gana(Motivo::Sed, 0.03).dur(12.0).alcance(200.0),
            Anuncio::nuevo(Actividad::Reunirse, mu.plaza.0, mu.plaza.1)
                .gana(Motivo::Social, 0.012)
                .gana(Motivo::Ocio, 0.006)
                .dur(300.0)
                .alcance(200.0)
                .bono(if hora_de_plaza(fase) { 0.5 } else { 0.0 }),
            Anuncio::nuevo(Actividad::Jugar, mu.casa.0 + 9.0, mu.casa.1 + 4.0).gana(Motivo::Ocio, 0.02).dur(40.0),
        ];
        if let Some((x, z, tag)) = vecino {
            a.push(Anuncio::nuevo(Actividad::Charlar, x, z).gana(Motivo::Social, 0.05).dur(15.0).uso(1.8).alcance(40.0).tag(tag));
        }
        let _ = v;
        a
    }

    /// Simula `dias` días a 1 s por paso; devuelve segundos por actividad y cuántas veces
    /// comió/bebió, y el mínimo que llegó a tener cada barra.
    fn simular(dias: f32, semilla: u64) -> ([f32; N_ACT], u32, u32, f32, f32) {
        let mu = Mundo { casa: (0.0, 0.0), trabajo: (18.0, 6.0), plaza: (-12.0, 10.0), agua: (25.0, -20.0) };
        let mut v = Vida::nueva(semilla);
        let mut pos = mu.casa;
        let mut por_act = [0.0f32; N_ACT];
        let (mut comidas, mut tragos) = (0u32, 0u32);
        let (mut min_hambre, mut min_sueno) = (1.0f32, 1.0f32);
        let mut ant = Actividad::Vagar;
        let total = (3600.0 * dias) as u32;
        for t in 0..total {
            let fase = (t as f32 / 3600.0 + 0.2) % 1.0; // arranca al amanecer
            v.tick(Clase::Aldeano, fase, 1.0, pos);
            // Un vecino cualquiera anda por la plaza.
            let vecino = Some((-12.0 + ((t / 90) % 5) as f32, 10.0, 99u64));
            if v.necesita_decidir(Clase::Aldeano, fase) {
                let ads = anuncios(&mu, fase, &v, vecino);
                v.decidir(Clase::Aldeano, pos, fase, &ads);
            }
            let act = v.actividad();
            if act != ant {
                if act == Actividad::Comer {
                    comidas += 1;
                }
                if act == Actividad::Beber {
                    tragos += 1;
                }
                ant = act;
            }
            por_act[act.i()] += 1.0;
            if let Some((x, z, llego)) = v.destino() {
                if !llego {
                    let (dx, dz) = (x - pos.0, z - pos.1);
                    let d = (dx * dx + dz * dz).sqrt().max(1e-3);
                    let paso = 2.0f32.min(d);
                    pos = (pos.0 + dx / d * paso, pos.1 + dz / d * paso);
                }
            }
            min_hambre = min_hambre.min(v.m.get(Motivo::Hambre));
            min_sueno = min_sueno.min(v.m.get(Motivo::Sueno));
        }
        (por_act, comidas, tragos, min_hambre, min_sueno)
    }

    #[test]
    fn un_dia_de_aldeano_tiene_forma_de_dia() {
        for semilla in [1u64, 2, 3, 17, 4242] {
            let (act, comidas, tragos, min_h, min_s) = simular(2.0, semilla);
            let h = |a: Actividad| act[a.i()] / 3600.0 / 2.0 * 24.0; // horas por día
            assert!(h(Actividad::Dormir) > 4.0 && h(Actividad::Dormir) < 12.0, "s{semilla}: duerme {:.1} h", h(Actividad::Dormir));
            assert!(h(Actividad::Trabajar) > 3.0, "s{semilla}: trabaja {:.1} h", h(Actividad::Trabajar));
            assert!(comidas >= 2 * 2, "s{semilla}: comió {comidas} veces en 2 días");
            assert!(tragos >= 2 * 2, "s{semilla}: bebió {tragos} veces en 2 días");
            let sociales = h(Actividad::Charlar) + h(Actividad::Reunirse);
            assert!(sociales > 0.5, "s{semilla}: vida social {sociales:.1} h");
            assert!(min_h > 0.0 || comidas > 0, "s{semilla}");
            assert!(min_h > 0.03, "s{semilla}: casi se muere de hambre ({min_h:.2})");
            assert!(min_s > 0.03, "s{semilla}: se durmió de pie ({min_s:.2})");
        }
    }

    #[test]
    fn de_noche_duerme_en_casa() {
        let mu = Mundo { casa: (0.0, 0.0), trabajo: (18.0, 6.0), plaza: (-12.0, 10.0), agua: (25.0, -20.0) };
        let mut v = Vida::nueva(5);
        let mut pos = (-12.0, 10.0);
        // Empieza a las 21:00 en la plaza.
        let mut durmiendo_en_casa = 0.0;
        for t in 0..(3600.0 * 0.28) as u32 {
            let fase = (0.875 + t as f32 / 3600.0) % 1.0;
            v.tick(Clase::Aldeano, fase, 1.0, pos);
            if v.necesita_decidir(Clase::Aldeano, fase) {
                let ads = anuncios(&mu, fase, &v, None);
                v.decidir(Clase::Aldeano, pos, fase, &ads);
            }
            if let Some((x, z, llego)) = v.destino() {
                if !llego {
                    let (dx, dz) = (x - pos.0, z - pos.1);
                    let d = (dx * dx + dz * dz).sqrt().max(1e-3);
                    let paso = 2.0f32.min(d);
                    pos = (pos.0 + dx / d * paso, pos.1 + dz / d * paso);
                }
            }
            if fase > 0.95 || fase < 0.15 {
                if v.actividad() == Actividad::Dormir && v.usando() && pos.0.abs() < 3.0 && pos.1.abs() < 3.0 {
                    durmiendo_en_casa += 1.0;
                }
            }
        }
        assert!(durmiendo_en_casa > 600.0, "durmió en casa sólo {durmiendo_en_casa} s de la noche");
    }

    // --- Moral: buenos, malos y desesperados ------------------------------------

    fn robo(x: f32, z: f32) -> Anuncio {
        Anuncio::nuevo(Actividad::Robar, x, z)
            .gana(Motivo::Riqueza, 0.05)
            .gana(Motivo::Hambre, 0.02)
            .dur(15.0)
            .tipo(Tipo::Antisocial)
            .riesgo(0.3)
            .tag(7)
    }

    fn veces_que_roba(moral: f32, hambre: f32, riqueza: f32) -> usize {
        let mut roba = 0;
        for s in 0..200u64 {
            let mut v = Vida::nueva(1000 + s);
            v.pers.moral = moral;
            v.m.set(Motivo::Hambre, hambre);
            v.m.set(Motivo::Riqueza, riqueza);
            let ads = [robo(3.0, 0.0), Anuncio::nuevo(Actividad::Jugar, 5.0, 0.0).gana(Motivo::Ocio, 0.02).dur(30.0)];
            if v.decidir(Clase::Aldeano, (0.0, 0.0), 0.5, &ads) == Actividad::Robar {
                roba += 1;
            }
        }
        roba
    }

    #[test]
    fn el_bueno_no_roba_pero_el_cruel_si() {
        let bueno = veces_que_roba(0.8, 0.9, 0.3);
        let cruel = veces_que_roba(-0.8, 0.9, 0.3);
        assert!(bueno <= 10, "el bueno robó {bueno}/200");
        assert!(cruel >= 120, "el cruel robó sólo {cruel}/200");
    }

    #[test]
    fn el_bueno_muerto_de_hambre_roba_mas() {
        let saciado = veces_que_roba(0.6, 0.9, 0.9);
        let muriendo = veces_que_roba(0.6, 0.05, 0.9);
        assert!(muriendo > saciado + 60, "saciado {saciado}, muriendo {muriendo}");
    }

    #[test]
    fn lo_prosocial_tienta_al_bueno_no_al_cruel() {
        let ayuda = |moral: f32| {
            let mut v = Vida::nueva(3);
            v.pers.moral = moral;
            v.m.set(Motivo::Social, 0.4);
            let a = Anuncio::nuevo(Actividad::Ayudar, 2.0, 0.0).gana(Motivo::Social, 0.03).dur(20.0).tipo(Tipo::Prosocial);
            puntaje(Clase::Aldeano, &v.m, &v.pers, &a, (0.0, 0.0), 0.5)
        };
        assert!(ayuda(0.9) > 4.0 * ayuda(-0.9).max(0.001));
    }

    #[test]
    fn los_lazos_se_juntan_y_se_enfrian() {
        let mut l = Lazos::default();
        l.sumar(5, 0.6);
        l.sumar(6, -0.8);
        assert_eq!(l.amigo().map(|a| a.0), Some(5));
        assert_eq!(l.enemigo().map(|a| a.0), Some(6));
        for uid in 10..30 {
            l.sumar(uid, 0.05);
        }
        assert!(l.get(5) > 0.5 || l.get(6) < -0.5, "los lazos fuertes no se pierden por ruido");
        l.enfriar(1e6);
        assert_eq!(l.get(5), 0.0);
        assert!(l.amigo().is_none());
    }
}
