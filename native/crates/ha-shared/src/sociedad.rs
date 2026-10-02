//! La sociedad como juego de motivos: quién ofrece qué a quién, y qué pasa cuando dos
//! vecinos se cruzan. Puro (sin Bevy): lo usan el mundo (`ha-game/society.rs`) y el
//! banco de pruebas de abajo, que corre un pueblo entero por días.
//!
//! La receta: los lugares (`Sitio`) y los vecinos (`Vecino`) publican `Anuncio`s; cada
//! quien los puntúa con su carácter (`sims::puntaje`). Robar, agredir, ayudar, predicar y
//! detener son anuncios de los vecinos, con carga moral (`Tipo`) y riesgo por testigos.
//! Nada está guionado: el hambre empuja al hambriento, el carácter decide qué se permite,
//! los testigos recuerdan, los buenos detienen a los malos y los carismáticos fundan credos.

use crate::credos::{arrastre, prob_conversion, puede_fundar, Credos, CREDO_KI, MAX_CREDOS};
use crate::sims::{hora_de_plaza, Actividad, Anuncio, Motivo, Tipo, Vida};

/// Los lugares de una vida: adónde va a dormir, trabajar, beber, juntarse y rezar.
#[derive(Clone, Copy, Debug)]
pub struct Sitio {
    pub casa: (f32, f32),
    pub trabajo: (f32, f32),
    pub plaza: (f32, f32),
    pub pozo: (f32, f32),
    pub altar: (f32, f32),
    /// Dónde se juega: el parque más cercano (o un rincón libre de la plaza), nunca contra una pared.
    pub parque: (f32, f32),
    /// A qué distancia de `casa` ya está "en casa" (0 = el valor de cada anuncio). Una casa con
    /// puerta necesita un radio corto: a 2,5 m de la cama todavía se está en la calle.
    pub casa_uso: f32,
}

/// Foto de un vecino, lo que se ve desde afuera (y lo que sabe la fama).
#[derive(Clone, Copy, Debug)]
pub struct Vecino {
    pub uid: u64,
    pub x: f32,
    pub z: f32,
    pub moral: f32,
    pub mala_fama: f32,
    pub credo: u16,
    pub hambre: f32,
    pub riqueza: f32,
    pub salud: f32,
}

impl Vecino {
    pub fn de(uid: u64, x: f32, z: f32, v: &Vida, salud: f32) -> Self {
        Self {
            uid,
            x,
            z,
            moral: v.pers.moral,
            mala_fama: v.mala_fama,
            credo: v.credo,
            hambre: v.m.get(Motivo::Hambre),
            riqueza: v.m.get(Motivo::Riqueza),
            salud,
        }
    }
}

/// Lo que ofrecen los lugares del pueblo. `comida`: hay reservas en la despensa.
pub fn anuncios_sitio(s: &Sitio, fase: f32, comida: bool, creyente: bool, out: &mut Vec<Anuncio>) {
    out.push(
        Anuncio::nuevo(Actividad::Dormir, s.casa.0, s.casa.1)
            .gana(Motivo::Sueno, 0.0006)
            .dur(1600.0)
            .alcance(400.0)
            .uso(if s.casa_uso > 0.0 { s.casa_uso } else { 2.5 })
            .hasta_dia(),
    );
    if comida {
        out.push(Anuncio::nuevo(Actividad::Comer, s.casa.0, s.casa.1).gana(Motivo::Hambre, 0.03).dur(25.0).alcance(400.0).uso(if s.casa_uso > 0.0 { s.casa_uso } else { 1.5 }));
    }
    out.push(Anuncio::nuevo(Actividad::Beber, s.pozo.0, s.pozo.1).gana(Motivo::Sed, 0.08).dur(10.0).alcance(150.0).uso(2.5));
    out.push(Anuncio::nuevo(Actividad::Beber, s.casa.0, s.casa.1).gana(Motivo::Sed, 0.03).dur(12.0).alcance(400.0).uso(if s.casa_uso > 0.0 { s.casa_uso } else { 1.5 }));
    out.push(
        Anuncio::nuevo(Actividad::Trabajar, s.trabajo.0, s.trabajo.1)
            .gana(Motivo::Trabajo, 0.004)
            .gana(Motivo::Riqueza, 0.0015)
            .dur(600.0)
            .alcance(400.0)
            .uso(2.0),
    );
    out.push(
        Anuncio::nuevo(Actividad::Reunirse, s.plaza.0, s.plaza.1)
            .gana(Motivo::Social, 0.012)
            .gana(Motivo::Ocio, 0.006)
            .dur(300.0)
            .alcance(400.0)
            .uso(4.0)
            .bono(if hora_de_plaza(fase) { 0.5 } else { 0.0 }),
    );
    out.push(Anuncio::nuevo(Actividad::Jugar, s.parque.0, s.parque.1).gana(Motivo::Ocio, 0.02).dur(40.0));
    if creyente {
        out.push(
            Anuncio::nuevo(Actividad::Rezar, s.altar.0, s.altar.1)
                .gana(Motivo::Fe, 0.02)
                .gana(Motivo::Social, 0.008)
                .dur(90.0)
                .alcance(400.0)
                .uso(3.0)
                .bono(if hora_de_plaza(fase) { 0.4 } else { 0.0 }),
        );
    }
}

/// Radio en el que un vecino cuenta como candidato para un trato.
pub const RADIO_TRATO: f32 = 30.0;
/// Radio en el que se ve un hecho (testigos).
pub const RADIO_TESTIGO: f32 = 14.0;

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Lo que ofrecen los vecinos cercanos a `yo`: charla, ayuda, hurto, agresión, prédica.
pub fn anuncios_vecinos(yo: &Vida, uid: u64, pos: (f32, f32), vecinos: &[Vecino], credos: &Credos, out: &mut Vec<Anuncio>) {
    let mut cerca: Vec<(&Vecino, f32)> = vecinos
        .iter()
        .filter(|v| v.uid != uid)
        .map(|v| (v, dist(pos, (v.x, v.z))))
        .filter(|(_, d)| *d <= RADIO_TRATO)
        .collect();
    cerca.sort_by(|a, b| a.1.total_cmp(&b.1));
    cerca.truncate(5);
    let puede_predicar = (yo.credo != 0 && yo.credo != CREDO_KI) || (puede_fundar(yo) && credos.lista.len() < MAX_CREDOS);
    let mi_dogma = credos.dogma(yo.credo);
    for &(v, _) in &cerca {
        let lazo = yo.lazos.get(v.uid);
        let rival = credos.rivales(yo.credo, v.credo);
        let testigos = vecinos.iter().filter(|w| w.uid != uid && w.uid != v.uid && dist(pos, (w.x, w.z)) <= RADIO_TESTIGO).count() as f32;
        // Charla: con cualquiera que no se odie.
        if lazo > -0.35 && !rival {
            out.push(
                Anuncio::nuevo(Actividad::Charlar, v.x, v.z)
                    .gana(Motivo::Social, 0.05)
                    .dur(12.0)
                    .uso(1.8)
                    .alcance(RADIO_TRATO)
                    .bono(0.1 * lazo.max(0.0))
                    .tag(v.uid),
            );
        }
        // Ayuda: al que pasa hambre o está herido, si uno tiene de sobra.
        if (v.hambre < 0.35 || v.salud < 0.6) && yo.m.get(Motivo::Hambre) > 0.5 && lazo > -0.35 {
            out.push(
                Anuncio::nuevo(Actividad::Ayudar, v.x, v.z)
                    .gana(Motivo::Social, 0.02)
                    .dur(8.0)
                    .uso(1.8)
                    .alcance(RADIO_TRATO)
                    .bono((0.35 - v.hambre).max(0.0) * 0.8 + (0.6 - v.salud).max(0.0) * 0.6)
                    .tipo(Tipo::Prosocial)
                    .tag(v.uid),
            );
        }
        // Hurto: al que tiene. Los testigos y la mala fama pesan en el riesgo.
        if v.riqueza > 0.55 && lazo < 0.5 {
            let riesgo = (testigos * 0.25 + yo.mala_fama * 0.3).min(0.9);
            out.push(
                Anuncio::nuevo(Actividad::Robar, v.x, v.z)
                    .gana(Motivo::Riqueza, 0.06)
                    .gana(Motivo::Hambre, 0.03)
                    .dur(8.0)
                    .uso(1.6)
                    .alcance(RADIO_TRATO)
                    .tipo(Tipo::Antisocial)
                    .riesgo(riesgo)
                    .tag(v.uid),
            );
        }
        // Agresión: la vendetta, la guerra de credos, el matón aburrido.
        let vendetta = (-lazo - 0.35).max(0.0);
        let santa_guerra = rival && mi_dogma.map_or(false, |d| d < 0.0);
        let bully = yo.pers.moral < -0.6 && yo.m.get(Motivo::Ocio) < 0.5 && lazo < 0.3;
        if vendetta > 0.0 || santa_guerra || bully {
            out.push(
                Anuncio::nuevo(Actividad::Atacar, v.x, v.z)
                    .gana(Motivo::Ocio, 0.03)
                    .dur(6.0)
                    .uso(1.6)
                    .alcance(RADIO_TRATO)
                    .bono(vendetta * 0.3 + if santa_guerra { 0.2 } else { 0.0 })
                    .tipo(Tipo::Antisocial)
                    .riesgo((testigos * 0.15).min(0.6))
                    .tag(v.uid),
            );
        }
        // Justicia: el que tiene mala fama, detenido por quien tiene buen corazón.
        if v.mala_fama > 0.35 && yo.mala_fama < 0.3 {
            out.push(
                Anuncio::nuevo(Actividad::Detener, v.x, v.z)
                    .gana(Motivo::Seguridad, 0.05)
                    .dur(6.0)
                    .uso(1.6)
                    .alcance(RADIO_TRATO)
                    .bono(v.mala_fama * 0.6)
                    .tipo(Tipo::Prosocial)
                    .tag(v.uid),
            );
        }
        // Prédica: a quien no tiene credo (convertir) o tiene uno rival (disputar).
        if puede_predicar && (v.credo == 0 || rival) {
            out.push(
                Anuncio::nuevo(Actividad::Predicar, v.x, v.z)
                    .gana(Motivo::Fe, 0.02)
                    .gana(Motivo::Social, 0.02)
                    .dur(12.0)
                    .uso(1.8)
                    .alcance(RADIO_TRATO)
                    .bono(0.1 + 0.25 * yo.pers.carisma)
                    .tag(v.uid),
            );
        }
    }
}

/// Qué pasó tras un trato, para que el mundo lo cuente (daño, muerte, frases, marcas).
#[derive(Clone, Copy, Debug, Default)]
pub struct Resultado {
    pub act: Option<Actividad>,
    /// Daño a la salud del otro (0..1).
    pub dano: f32,
    pub antisocial: bool,
    pub prosocial: bool,
    pub convirtio: bool,
    pub fundo: u16,
}

/// Aplica el trato `act` de `actor` sobre `otro`. `azar` en `[0,1)` decide las conversiones.
pub fn resolver(act: Actividad, actor: &mut Vida, actor_uid: u64, otro: &mut Vida, otro_uid: u64, credos: &mut Credos, cuerpo: u8, azar: f32) -> Resultado {
    let mut r = Resultado { act: Some(act), ..Default::default() };
    match act {
        Actividad::Charlar => {
            let rival = credos.rivales(actor.credo, otro.credo);
            let d = if rival { -0.2 } else { 0.15 };
            actor.lazos.sumar(otro_uid, d);
            otro.lazos.sumar(actor_uid, d);
        }
        Actividad::Ayudar => {
            r.prosocial = true;
            otro.m.set(Motivo::Hambre, otro.m.get(Motivo::Hambre) + 0.35);
            actor.m.set(Motivo::Hambre, actor.m.get(Motivo::Hambre) - 0.1);
            actor.m.set(Motivo::Riqueza, actor.m.get(Motivo::Riqueza) - 0.08);
            actor.lazos.sumar(otro_uid, 0.25);
            otro.lazos.sumar(actor_uid, 0.4);
        }
        Actividad::Robar => {
            r.antisocial = true;
            let botin = 0.4f32.min(otro.m.get(Motivo::Riqueza));
            otro.m.set(Motivo::Riqueza, otro.m.get(Motivo::Riqueza) - botin);
            actor.m.set(Motivo::Riqueza, actor.m.get(Motivo::Riqueza) + botin);
            if actor.m.get(Motivo::Hambre) < 0.5 {
                otro.m.set(Motivo::Hambre, otro.m.get(Motivo::Hambre) - 0.15);
                actor.m.set(Motivo::Hambre, actor.m.get(Motivo::Hambre) + 0.3);
            }
            otro.m.set(Motivo::Seguridad, otro.m.get(Motivo::Seguridad) - 0.3);
            otro.lazos.sumar(actor_uid, -0.5);
            actor.mala_fama = (actor.mala_fama + 0.4).min(1.0);
        }
        Actividad::Atacar => {
            r.antisocial = true;
            r.dano = 0.2;
            otro.m.set(Motivo::Seguridad, otro.m.get(Motivo::Seguridad) - 0.4);
            otro.lazos.sumar(actor_uid, -0.7);
            actor.lazos.sumar(otro_uid, -0.2);
            let inocente = otro.mala_fama < 0.2;
            actor.mala_fama = (actor.mala_fama + if inocente { 0.6 } else { 0.25 }).min(1.0);
        }
        Actividad::Detener => {
            r.prosocial = true;
            r.dano = 0.05;
            otro.mala_fama = (otro.mala_fama - 0.4).max(0.0);
            otro.veda[Actividad::Robar.i()] = 300.0;
            otro.veda[Actividad::Atacar.i()] = 300.0;
            otro.lazos.sumar(actor_uid, -0.3);
            actor.m.set(Motivo::Seguridad, actor.m.get(Motivo::Seguridad) + 0.2);
        }
        Actividad::Predicar => {
            if actor.credo == 0 && puede_fundar(actor) && credos.lista.len() < MAX_CREDOS {
                r.fundo = credos.fundar(actor, actor_uid, cuerpo);
            }
            let Some(dogma) = credos.dogma(actor.credo) else { return r };
            if otro.credo == 0 {
                if azar < prob_conversion(&actor.pers, otro, dogma) {
                    otro.credo = actor.credo;
                    otro.m.set(Motivo::Fe, 1.0);
                    otro.lazos.sumar(actor_uid, 0.4);
                    actor.lazos.sumar(otro_uid, 0.3);
                    r.convirtio = true;
                } else {
                    actor.lazos.sumar(otro_uid, -0.05);
                }
            } else if credos.rivales(actor.credo, otro.credo) {
                // La herejía ajena ofende: nace el rencor entre credos.
                otro.lazos.sumar(actor_uid, -0.4);
                actor.lazos.sumar(otro_uid, -0.3);
            }
        }
        _ => {}
    }
    r
}

/// Un testigo se entera. Los de buen corazón se indignan más; el amigo de la víctima, más.
pub fn testigo(t: &mut Vida, t_uid: u64, actor_uid: u64, otro_uid: u64, r: &Resultado) {
    if t_uid == actor_uid || t_uid == otro_uid {
        return;
    }
    if r.antisocial {
        let indig = (0.15 + 0.35 * t.pers.moral.max(0.0)).min(0.5);
        let extra = if t.lazos.get(otro_uid) > 0.3 { 0.3 } else { 0.0 };
        t.lazos.sumar(actor_uid, -(indig + extra));
        let s = t.m.get(Motivo::Seguridad);
        t.m.set(Motivo::Seguridad, s - 0.1);
    } else if r.prosocial {
        t.lazos.sumar(actor_uid, 0.1);
    }
}

/// El paso común del credo sobre un creyente: la moral se acerca al dogma.
pub fn vivir_credo(v: &mut Vida, credos: &Credos, dt: f32) {
    if let Some(d) = credos.dogma(v.credo) {
        arrastre(v, d, dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sims::{noche, Clase};

    struct Agente {
        uid: u64,
        pos: (f32, f32),
        vida: Vida,
        salud: f32,
        sitio: Sitio,
    }

    #[derive(Default, Debug)]
    struct Balance {
        hurtos_buenos: u32,
        hurtos_malos: u32,
        oport_buenos: u32,
        oport_malos: u32,
        agresiones: u32,
        detenciones: u32,
        ayudas: u32,
        conversiones: u32,
        muertos: u32,
        credos_fundados: u32,
        min_hambre: f32,
    }

    /// Un pueblo de `n` vecinos, `dias` días, dt=2 s. `hambruna`: (desde, hasta) en días de
    /// la despensa vacía.
    fn pueblo(n: u64, dias: f32, semilla: u64, hambruna: Option<(f32, f32)>, tendencia: f32) -> (Vec<Agente>, Credos, Balance) {
        let plaza = (0.0f32, 0.0f32);
        let mut ag: Vec<Agente> = (0..n)
            .map(|i| {
                let uid = semilla * 1000 + i + 1;
                let a = i as f32 * 2.399_963_2;
                let casa = (plaza.0 + a.cos() * (12.0 + (i % 4) as f32 * 3.0), plaza.1 + a.sin() * (12.0 + (i % 4) as f32 * 3.0));
                let sitio = Sitio {
                    casa,
                    trabajo: (casa.0 + a.cos() * 8.0, casa.1 + a.sin() * 8.0),
                    plaza,
                    pozo: (plaza.0 - 4.0, plaza.1 + 3.0),
                    altar: (plaza.0 + 8.0, plaza.1 - 8.0),
                    parque: (plaza.0 + 5.0, plaza.1 + 5.0),
                    casa_uso: 0.0,
                };
                Agente { uid, pos: casa, vida: Vida::nueva_con(uid, tendencia), salud: 1.0, sitio }
            })
            .collect();
        let mut credos = Credos::default();
        let mut bal = Balance { min_hambre: 1.0, ..Default::default() };
        let dt = 2.0f32;
        let total = (3600.0 * dias / dt) as u32;
        let mut azar = crate::sims::hash(semilla ^ 0x5EED);
        for paso in 0..total {
            let t = paso as f32 * dt;
            let fase = (t / 3600.0 + 0.2) % 1.0;
            let dia = t / 3600.0;
            let comida = hambruna.map_or(true, |(a, b)| !(dia >= a && dia < b));
            let snap: Vec<Vecino> = ag.iter().map(|a| Vecino::de(a.uid, a.pos.0, a.pos.1, &a.vida, a.salud)).collect();
            let mut eventos: Vec<(usize, Actividad, u64)> = Vec::new();
            for (k, a) in ag.iter_mut().enumerate() {
                a.vida.tick(Clase::Aldeano, fase, dt, a.pos);
                vivir_credo(&mut a.vida, &credos, dt);
                // Seguir al objetivo; si se fue, se cae el plan.
                let obj = a.vida.objetivo();
                if obj != 0 {
                    match snap.iter().find(|v| v.uid == obj) {
                        Some(v) => a.vida.apuntar(v.x, v.z),
                        None => a.vida.abandonar(),
                    }
                }
                if a.vida.necesita_decidir(Clase::Aldeano, fase) {
                    let mut ads = Vec::with_capacity(16);
                    anuncios_sitio(&a.sitio, fase, comida, a.vida.credo != 0, &mut ads);
                    anuncios_vecinos(&a.vida, a.uid, a.pos, &snap, &credos, &mut ads);
                    a.vida.decidir(Clase::Aldeano, a.pos, fase, &ads);
                }
                if let Some((x, z, llego)) = a.vida.destino() {
                    if !llego {
                        let (dx, dz) = (x - a.pos.0, z - a.pos.1);
                        let d = (dx * dx + dz * dz).sqrt().max(1e-3);
                        let paso = (2.0 * dt).min(d);
                        a.pos = (a.pos.0 + dx / d * paso, a.pos.1 + dz / d * paso);
                    }
                }
                if let Some((act, otro)) = a.vida.por_resolver() {
                    eventos.push((k, act, otro));
                }
                bal.min_hambre = bal.min_hambre.min(a.vida.m.get(Motivo::Hambre));
            }
            for (k, act, otro_uid) in eventos {
                let Some(j) = ag.iter().position(|a| a.uid == otro_uid) else { continue };
                if j == k {
                    continue;
                }
                azar = crate::sims::hash(azar);
                let u = (azar >> 40) as f32 / (1u64 << 24) as f32;
                let (lo, hi) = if k < j { (k, j) } else { (j, k) };
                let (izq, der) = ag.split_at_mut(hi);
                let (pa, pb) = (&mut izq[lo], &mut der[0]);
                let (actor, otro) = if k < j { (pa, pb) } else { (pb, pa) };
                let hambre_antes = actor.vida.m.get(Motivo::Hambre);
                let r = resolver(act, &mut actor.vida, actor.uid, &mut otro.vida, otro.uid, &mut credos, 0, u);
                let (auid, ouid, apos) = (actor.uid, otro.uid, actor.pos);
                let moral_actor = actor.vida.pers.moral;
                otro.salud -= r.dano;
                match act {
                    Actividad::Robar => {
                        let oport = hambre_antes > 0.3;
                        if moral_actor > 0.0 {
                            bal.hurtos_buenos += 1;
                            bal.oport_buenos += oport as u32;
                        } else {
                            bal.hurtos_malos += 1;
                            bal.oport_malos += oport as u32;
                        }
                    }
                    Actividad::Atacar => bal.agresiones += 1,
                    Actividad::Detener => bal.detenciones += 1,
                    Actividad::Ayudar => bal.ayudas += 1,
                    _ => {}
                }
                if r.convirtio {
                    bal.conversiones += 1;
                }
                if r.fundo != 0 {
                    bal.credos_fundados += 1;
                }
                for w in ag.iter_mut() {
                    if dist(w.pos, apos) <= RADIO_TESTIGO {
                        testigo(&mut w.vida, w.uid, auid, ouid, &r);
                    }
                }
            }
            let antes = ag.len();
            ag.retain(|a| a.salud > 0.0);
            bal.muertos += (antes - ag.len()) as u32;
            for a in ag.iter_mut() {
                a.salud = (a.salud + dt / 900.0).min(1.0); // se curan solos
            }
            if paso % 1800 == 0 {
                let ids: Vec<u16> = credos.lista.iter().map(|c| c.id).collect();
                for id in ids {
                    let miembros: Vec<(u64, f32)> = ag.iter().filter(|a| a.vida.credo == id).map(|a| (a.uid, a.vida.pers.carisma)).collect();
                    credos.sucesion(id, &miembros);
                }
                credos.censar(|id| ag.iter().filter(|a| a.vida.credo == id).count() as u16);
                for a in ag.iter_mut() {
                    if a.vida.credo != 0 && credos.get(a.vida.credo).is_none() {
                        a.vida.credo = 0;
                    }
                }
            }
        }
        let _ = noche(0.0);
        (ag, credos, bal)
    }

    #[test]
    fn en_paz_el_pueblo_vive_sin_matarse() {
        for s in [1u64, 2, 3] {
            let (ag, _c, b) = pueblo(24, 3.0, s, None, 0.0);
            eprintln!("paz s{s}: {b:?} vivos={}", ag.len());
            assert!(b.muertos <= 3, "s{s}: {} muertos en tiempos de abundancia", b.muertos);
            assert!(b.min_hambre > 0.02, "s{s}: alguien casi muere de hambre con despensa llena ({:.2})", b.min_hambre);
        }
    }

    #[test]
    fn la_hambruna_multiplica_el_crimen() {
        let (mut paz, mut ham) = (0u32, 0u32);
        for s in [11u64, 12, 13] {
            let (_a, _c, b) = pueblo(24, 3.0, s, None, 0.0);
            paz += b.hurtos_buenos + b.hurtos_malos;
            let (_a, _c, b2) = pueblo(24, 3.0, s, Some((0.8, 2.2)), 0.0);
            ham += b2.hurtos_buenos + b2.hurtos_malos;
            eprintln!("s{s}: paz {b:?} | hambruna {b2:?}");
        }
        assert!(ham > paz + 4, "hurtos en paz {paz}, en hambruna {ham}");
    }

    #[test]
    fn los_malos_roban_mas_que_los_buenos() {
        // Con hambre roba cualquiera; lo que separa a unos de otros es robar SIN necesidad.
        let (mut buenos, mut malos) = (0u32, 0u32);
        for s in [21u64, 22, 23] {
            let (_a, _c, b) = pueblo(24, 3.0, s, Some((0.8, 2.2)), 0.0);
            eprintln!("s{s}: {b:?}");
            buenos += b.oport_buenos;
            malos += b.oport_malos;
        }
        assert!(malos > buenos * 2, "oportunistas: buenos {buenos}, malos {malos}");
    }

    #[test]
    fn un_mundo_hostil_es_mas_violento_que_uno_amable() {
        let (mut amable, mut hostil) = (0u32, 0u32);
        for s in [31u64, 32, 33] {
            let (_a, _c, b) = pueblo(24, 3.0, s, None, 0.6);
            amable += b.agresiones + b.hurtos_malos + b.hurtos_buenos;
            let (_a, _c, b) = pueblo(24, 3.0, s, None, -0.7);
            hostil += b.agresiones + b.hurtos_malos + b.hurtos_buenos;
        }
        assert!(hostil > amable + 5, "amable {amable}, hostil {hostil}");
    }

    #[test]
    fn nacen_sectas_que_reclutan() {
        let (mut fundados, mut convertidos) = (0u32, 0u32);
        for s in [41u64, 42, 43] {
            let (ag, c, b) = pueblo(30, 4.0, s, None, 0.0);
            eprintln!("sectas s{s}: {b:?} credos={:?}", c.lista.iter().map(|c| (&c.nombre, c.miembros, c.dogma)).collect::<Vec<_>>());
            fundados += b.credos_fundados;
            convertidos += b.conversiones;
            for cr in &c.lista {
                let m: Vec<f32> = ag.iter().filter(|a| a.vida.credo == cr.id).map(|a| a.vida.pers.moral).collect();
                if m.len() >= 2 {
                    let media = m.iter().sum::<f32>() / m.len() as f32;
                    assert!((media - cr.dogma).abs() < 0.9, "la secta '{}' no se parece a su dogma", cr.nombre);
                }
            }
        }
        assert!(fundados >= 2, "sólo se fundaron {fundados} credos");
        assert!(convertidos >= 3, "sólo hubo {convertidos} conversiones");
    }
}
