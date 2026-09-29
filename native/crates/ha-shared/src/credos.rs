//! Credos y sectas: la parte de la sociedad que no cabe en una barra.
//!
//! Un credo lo funda alguien carismático; su **dogma** (-1 severo/violento .. +1 piadoso)
//! sale del carácter del fundador y va tirando de la moral de sus miembros. Recluta por
//! carisma y por necesidad del oyente (el solo, el pobre). Dos credos de dogma opuesto se
//! odian. Todo puro y determinista; el mundo (ha-game) sólo decide quién habla con quién.

use crate::sims::{Motivo, Personalidad, Vida};
use serde::{Deserialize, Serialize};

/// La religión que funda el jugador (`Religion` en ha-game): mismo mecanismo de fe, dogma fijo.
pub const CREDO_KI: u16 = u16::MAX;
pub const DOGMA_KI: f32 = 0.6;
/// Tope de credos vivos a la vez: el mundo no se llena de sectas de a dos.
pub const MAX_CREDOS: usize = 6;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Credo {
    pub id: u16,
    pub nombre: String,
    /// -1 (culto de sangre) .. +1 (hermandad de caridad).
    pub dogma: f32,
    /// uid del líder, 0 si quedó acéfalo.
    pub lider: u64,
    /// Cuántos miembros tiene (lo recuenta el mundo cada tanto).
    #[serde(default)]
    pub miembros: u16,
    /// Planeta donde nació (índice de cuerpo), para el mapa de la sociedad.
    #[serde(default)]
    pub cuerpo: u8,
}

impl Credo {
    /// Nombre con sabor según el dogma. `semilla` elige entre las variantes.
    pub fn nombre_para(dogma: f32, semilla: u64) -> String {
        const BUENOS: [&str; 5] = ["Hermandad", "Orden", "Camino", "Casa", "Círculo"];
        const MALOS: [&str; 5] = ["Culto", "Legión", "Sombra", "Secta", "Colmillo"];
        const NEUTROS: [&str; 4] = ["Congregación", "Comunidad", "Escuela", "Logia"];
        const DE: [&str; 10] = [
            "del Alba", "de la Estrella Errante", "del Anillo de Hielo", "de la Llama Quieta", "del Vacío",
            "de los Cielos Abiertos", "de la Luna Sorda", "del Viento Solar", "de la Órbita Rota", "de la Última Cosecha",
        ];
        let h = crate::sims::hash(semilla ^ 0xC4ED0);
        let cab = if dogma > 0.25 {
            BUENOS[(h % 5) as usize]
        } else if dogma < -0.25 {
            MALOS[(h % 5) as usize]
        } else {
            NEUTROS[(h % 4) as usize]
        };
        format!("{} {}", cab, DE[((h >> 8) % 10) as usize])
    }

    /// Dos credos de dogmas opuestos no se toleran.
    pub fn rival_de(&self, otro: &Credo) -> bool {
        self.id != otro.id && (self.dogma - otro.dogma).abs() > 1.1
    }
}

/// ¿Puede alguien fundar un credo? Carisma alto y el pecho lleno de algo que decir:
/// o le sobra bondad, o le sobra rencor, o está muy solo.
pub fn puede_fundar(v: &Vida) -> bool {
    v.credo == 0 && v.pers.carisma > 0.72 && (v.pers.moral.abs() > 0.35 || v.m.get(Motivo::Social) < 0.3)
}

/// Probabilidad (0..1) de que `oyente` se convierta tras escuchar una prédica de
/// `predicador` del credo de dogma `dogma`. Pesa el carisma, la necesidad de pertenecer
/// y que el mensaje le suene (moral parecida).
pub fn prob_conversion(predicador: &Personalidad, oyente: &Vida, dogma: f32) -> f32 {
    if oyente.credo != 0 {
        return 0.0;
    }
    let necesidad = 0.35 + 0.4 * (1.0 - oyente.m.get(Motivo::Social)) + 0.25 * (1.0 - oyente.m.get(Motivo::Riqueza));
    let afinidad = 1.0 - ((oyente.pers.moral - dogma).abs() / 2.0);
    (predicador.carisma * necesidad * afinidad * afinidad).clamp(0.0, 0.9)
}

/// El dogma tira de la moral del creyente: despacio, pero para siempre.
pub fn arrastre(v: &mut Vida, dogma: f32, dt: f32) {
    if v.credo != 0 {
        v.pers.moral += (dogma - v.pers.moral) * (dt / 20_000.0).min(0.5);
        v.pers.moral = v.pers.moral.clamp(-1.0, 1.0);
    }
}

#[derive(Clone, Default, Serialize, Deserialize, Debug)]
pub struct Credos {
    pub lista: Vec<Credo>,
    pub sig: u16,
}

impl Credos {
    pub fn get(&self, id: u16) -> Option<&Credo> {
        if id == 0 {
            return None;
        }
        self.lista.iter().find(|c| c.id == id)
    }

    /// Dogma del credo `id` (el del jugador incluido), si existe.
    pub fn dogma(&self, id: u16) -> Option<f32> {
        if id == CREDO_KI {
            return Some(DOGMA_KI);
        }
        self.get(id).map(|c| c.dogma)
    }

    /// Dos credos vivos que se odian.
    pub fn rivales(&self, a: u16, b: u16) -> bool {
        match (self.dogma(a), self.dogma(b)) {
            (Some(x), Some(y)) => a != b && (x - y).abs() > 1.1,
            _ => false,
        }
    }

    /// Si el líder ya no está entre los vivos, sube el más carismático de los miembros.
    /// `miembros`: `(uid, carisma)` de los que pertenecen a `id`.
    pub fn sucesion(&mut self, id: u16, miembros: &[(u64, f32)]) {
        let Some(c) = self.lista.iter_mut().find(|c| c.id == id) else { return };
        if miembros.iter().any(|m| m.0 == c.lider) {
            return;
        }
        c.lider = miembros.iter().copied().fold((0u64, -1.0f32), |b, m| if m.1 > b.1 { m } else { b }).0;
    }

    /// Funda un credo con `fundador` de líder; queda inscripto en él.
    pub fn fundar(&mut self, fundador: &mut Vida, uid: u64, cuerpo: u8) -> u16 {
        self.sig = self.sig.max(1);
        let id = self.sig;
        self.sig += 1;
        let dogma = (fundador.pers.moral * 1.1).clamp(-1.0, 1.0);
        self.lista.push(Credo {
            id,
            nombre: Credo::nombre_para(dogma, uid.wrapping_add(id as u64)),
            dogma,
            lider: uid,
            miembros: 1,
            cuerpo,
        });
        fundador.credo = id;
        id
    }

    /// Recuento de miembros; un credo sin nadie se disuelve. Devuelve los ids disueltos.
    pub fn censar(&mut self, cuenta: impl Fn(u16) -> u16) -> Vec<u16> {
        for c in self.lista.iter_mut() {
            c.miembros = cuenta(c.id);
        }
        let muertos: Vec<u16> = self.lista.iter().filter(|c| c.miembros == 0).map(|c| c.id).collect();
        self.lista.retain(|c| c.miembros > 0);
        muertos
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sims::Vida;

    fn carismatico(uid: u64, moral: f32) -> Vida {
        let mut v = Vida::nueva(uid);
        v.pers.carisma = 0.95;
        v.pers.moral = moral;
        v
    }

    #[test]
    fn solo_el_carismatico_con_algo_que_decir_funda() {
        assert!(puede_fundar(&carismatico(1, 0.8)));
        assert!(puede_fundar(&carismatico(2, -0.8)));
        let mut soso = carismatico(3, 0.8);
        soso.pers.carisma = 0.3;
        assert!(!puede_fundar(&soso));
        let mut ya = carismatico(4, 0.8);
        ya.credo = 7;
        assert!(!puede_fundar(&ya));
    }

    #[test]
    fn el_solo_y_afin_se_convierte_mas() {
        let p = carismatico(1, 0.9).pers;
        let mut solo = Vida::nueva(10);
        solo.pers.moral = 0.8;
        solo.m.set(Motivo::Social, 0.1);
        let mut feliz = Vida::nueva(11);
        feliz.pers.moral = -0.8;
        feliz.m.set(Motivo::Social, 1.0);
        assert!(prob_conversion(&p, &solo, 0.9) > 3.0 * prob_conversion(&p, &feliz, 0.9));
        let mut creyente = solo;
        creyente.credo = 2;
        assert_eq!(prob_conversion(&p, &creyente, 0.9), 0.0);
    }

    #[test]
    fn el_dogma_tira_de_la_moral() {
        let mut v = Vida::nueva(5);
        v.pers.moral = -0.5;
        v.credo = 1;
        for _ in 0..200_000 {
            arrastre(&mut v, 0.9, 1.0);
        }
        assert!(v.pers.moral > 0.5, "moral {}", v.pers.moral);
        let mut sin = Vida::nueva(6);
        sin.pers.moral = -0.5;
        arrastre(&mut sin, 0.9, 1e6);
        assert_eq!(sin.pers.moral, -0.5);
    }

    #[test]
    fn fundar_inscribe_nombra_y_disuelve() {
        let mut cs = Credos::default();
        let mut f = carismatico(9, -0.9);
        let id = cs.fundar(&mut f, 9, 2);
        assert_eq!(f.credo, id);
        let c = cs.get(id).unwrap();
        assert!(c.dogma < -0.5 && c.lider == 9);
        assert!(!c.nombre.is_empty());
        let mut f2 = carismatico(10, 0.9);
        let id2 = cs.fundar(&mut f2, 10, 2);
        assert!(cs.get(id).unwrap().rival_de(cs.get(id2).unwrap()));
        let muertos = cs.censar(|i| if i == id { 0 } else { 3 });
        assert_eq!(muertos, vec![id]);
        assert!(cs.get(id).is_none() && cs.get(id2).is_some());
    }

    #[test]
    fn los_nombres_cambian_con_el_dogma() {
        let b = Credo::nombre_para(0.9, 1);
        let m = Credo::nombre_para(-0.9, 1);
        assert_ne!(b, m);
        assert!(["Hermandad", "Orden", "Camino", "Casa", "Círculo"].iter().any(|p| b.starts_with(p)));
        assert!(["Culto", "Legión", "Sombra", "Secta", "Colmillo"].iter().any(|p| m.starts_with(p)));
    }
}
