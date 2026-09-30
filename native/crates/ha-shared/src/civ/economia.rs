//! Comida, materiales, tesoro, ánimo: lo que un pueblo produce y come cada día.

use super::cronica::Tipo;
use super::mundo::Mundo;
use super::pueblo::Pueblo;
use super::saber::*;
use super::DIAS_ANIO;

/// Cuánta gente aguanta el sitio con lo que sabe. Las colonias sin soporte vital
/// aguantan casi nada: viven de lo que les mandan.
pub fn carga_efectiva(p: &Pueblo) -> f32 {
    let cap = p.capacidad();
    let mut k = p.carga * (0.55 + 0.75 * cap.comida.min(2.6)) * (0.85 + 0.35 * cap.salud.min(1.2));
    if p.sustrato & S_FERTIL == 0 {
        k *= 0.8;
    }
    if p.cuerpo != 0 {
        k *= p.habit * if p.saberes.tiene(SOPORTE_VITAL) { 1.0 } else { 0.15 };
    }
    k.max(5.0)
}

impl Mundo {
    pub(crate) fn economia_dia(&mut self) {
        let ids: Vec<u32> = self.pueblos.keys().copied().collect();
        let anio_malo = {
            // sequía / plaga: una por año a lo sumo, por pueblo, sorteada el primer día
            self.dia % DIAS_ANIO == 1
        };
        for id in ids {
            let mala = anio_malo && self.rng.p(0.06);
            let en_guerra = self.guerra_de(id).is_some();
            let Some(p) = self.pueblos.get_mut(&id) else { continue };
            if !p.vivo {
                continue;
            }
            let cap = p.capacidad();
            let k = carga_efectiva(p);
            p.carga = p.carga.max(1.0);
            let soporte = p.cuerpo == 0 || p.saberes.tiene(SOPORTE_VITAL);
            let fert = if p.sustrato & S_FERTIL != 0 { 1.0 } else { 0.78 };
            let hab = if p.cuerpo == 0 { 1.0 } else { p.habit * if soporte { 1.0 } else { 0.1 } };
            let clima = 1.0 + 0.08 * ((self.dia % DIAS_ANIO) as f32 / DIAS_ANIO as f32 * std::f32::consts::TAU).sin();
            let mut prod = p.pob * (0.7 + 0.4 * cap.comida.min(2.6)) * fert * hab * clima;
            if en_guerra {
                prod *= 0.85;
            }
            if mala {
                prod *= 0.4;
                p.comida *= 0.6;
            }
            p.comida = (p.comida + prod - p.pob).clamp(0.0, p.pob * 20.0 + 50.0);
            let corto = p.pob * 1.5;
            p.hambre = if p.comida < corto { 1.0 - p.comida / corto } else { 0.0 };
            if p.comida <= 0.0 {
                p.pob *= 0.996;
            } else if p.hambre < 0.3 {
                let r = 0.0005 * (1.0 + 0.3 * cap.salud.min(1.5));
                p.pob += r * p.pob * (1.0 - p.pob / k);
            } else if p.pob > k {
                p.pob *= 0.9995;
            }
            p.pob = p.pob.max(8.0);
            p.mat = (p.mat + p.pob * 0.05 * (0.6 + cap.mat.min(3.0)) * hab.max(0.1)).min(p.pob * 40.0 + 200.0);
            let soldados = p.pob * p.leyes.servicio * 0.10;
            let ingreso = p.pob * p.leyes.impuesto * 0.05 * (1.0 + cap.com.min(2.0));
            p.tesoro += ingreso - soldados * 0.03 - p.pob * 0.003;
            p.tesoro = p.tesoro.clamp(-p.pob * 5.0, p.pob * 60.0 + 100.0);
            let deuda = if p.tesoro < 0.0 { 0.15 } else { 0.0 };
            let objetivo_moral = (0.78 - 0.6 * p.hambre - 0.6 * (p.leyes.impuesto - 0.2).max(0.0)
                + 0.12 * cap.salud.min(1.0)
                - if en_guerra { 0.15 } else { 0.0 }
                - deuda
                + 0.1 * (p.legit - 0.5))
                .clamp(0.0, 1.0);
            p.moral += (objetivo_moral - p.moral) * 0.03;
            let sev = p.leyes.pena_robo as f32 * 0.04 + p.leyes.pena_homicidio as f32 * 0.03;
            let objetivo_orden = (0.5 + 0.12 * cap.orden.min(1.5) + sev - p.crimen * 0.5 - 0.3 * p.hambre).clamp(0.0, 1.0);
            p.orden += (objetivo_orden - p.orden) * 0.03;
            p.crimen *= 0.98;
            p.legit += (p.moral - 0.5) * 0.004 + (p.orden - 0.5) * 0.002 - 0.0004;
            p.legit = p.legit.clamp(0.0, 1.0);
            if mala && p.hambre > 0.5 {
                let nombre = p.nombre.clone();
                self.hecho(Tipo::Hambruna, id, 0, format!("Mala cosecha en {}: la gente pasa hambre.", nombre));
            }
        }
    }

    /// Tropa efectiva del pueblo.
    pub fn fuerza_militar(&self, id: u32, defendiendo: bool) -> f32 {
        let Some(p) = self.pueblos.get(&id) else { return 0.0 };
        if !p.vivo {
            return 0.0;
        }
        let cap = p.capacidad();
        let soldados = p.pob * (0.02 + 0.08 * p.leyes.servicio);
        let base = soldados * (1.0 + cap.mil);
        let muro = if defendiendo { 1.0 + 0.25 * p.muralla as f32 + cap.defensa } else { 1.0 };
        let lider = p
            .gobernante
            .and_then(|g| self.personas.get(&g))
            .map(|g| 0.85 + 0.3 * (g.fuerza + g.audacia) * 0.5)
            .unwrap_or(0.8);
        base * muro * (0.5 + 0.5 * p.moral) * lider * if p.tesoro < 0.0 { 0.7 } else { 1.0 }
    }
}
