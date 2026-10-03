//! **El compositor**: arma una pieza entera —progresión de acordes, bajo, arpegio, motivo de melodía y colchón— en
//! vez de tirar notas sueltas al azar (2 oct 2026, motor v3).
//!
//! Se escribió escuchando lo que hace sonar a dos bandas sonoras que el proyecto toma de referencia:
//! * **Minecraft (C418, y después Lena Raine / Aaron Cherof)**: minimalismo ambiental, armonía modal (jónico,
//!   dórico, lidio), piano de fieltro con mucha reverb, motivos de 3-5 notas que se repiten con variaciones,
//!   **mucho silencio** y nada de batería.
//! * **Project Zomboid (Zach Beever)**: piano y cuerdas melancólicos, guitarra acústica, colchones oscuros que
//!   crecen despacio y disonancias suaves (segundas, séptimas, acordes suspendidos) para el miedo de la noche.
//!
//! Las piezas son ORIGINALES (no copian ninguna melodía): lo que se toma de ellas son los recursos —modo, tempo,
//! timbre, densidad, silencio—, y todo es determinista por semilla.

use crate::{Lcg, Synth, ToneOp, TrackSpec, Wave};
use serde::{Deserialize, Serialize};

/// Un instrumento dentro de una pieza.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Voice {
    pub wave: Wave,
    pub amp: f32,
    /// Octava relativa a la tónica de la pista (−2 = dos octavas abajo).
    pub octave: i32,
    pub attack: f32,
    pub curve: f32,
    pub detune: f32,
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub cutoff: f32,
    pub res: f32,
    /// Duración de cada nota, en tiempos (corcheas = 0,5).
    pub beats: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Self { wave: Wave::Piano, amp: 0.25, octave: 0, attack: 0.004, curve: 1.2, detune: 0.0, fm_ratio: 0.0, fm_index: 0.0, cutoff: 0.0, res: 0.8, beats: 2.0 }
    }
}

/// La partitura generativa de una pista.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ComposeSpec {
    pub tempo: f32,
    pub beats: u32,
    /// Los siete grados del modo, en semitonos sobre la tónica.
    pub mode: Vec<f32>,
    /// Grado (0-6) de la raíz de cada acorde de la progresión.
    pub progression: Vec<i32>,
    pub bars_per_chord: u32,
    /// Probabilidades de adornar el acorde: séptima, novena añadida y «sus2» (segunda en lugar de tercera).
    pub seventh: f32,
    pub add9: f32,
    pub sus: f32,
    pub bass: Option<Voice>,
    pub pad: Option<Voice>,
    pub arp: Option<Voice>,
    pub arp_density: f32,
    /// Patrón de notas del acorde (índice dentro del acorde; ≥ largo sube una octava) por paso de la grilla.
    pub arp_pattern: Vec<i32>,
    pub steps_per_beat: u32,
    pub lead: Option<Voice>,
    /// Probabilidad de que una frase de `phrase_bars` compases tenga melodía (el resto es silencio).
    pub lead_phrase: f32,
    pub phrase_bars: u32,
    pub motif_len: u32,
    /// Probabilidad de repetir el motivo anterior (transportado) en vez de inventar uno.
    pub motif_repeat: f32,
    /// Variación de intensidad por nota (0 = todas iguales) y desfase humano de tiempos (s).
    pub dynamics: f32,
    pub humanize: f32,
    /// Compases de silencio completo a propósito, al inicio (el aire ES la pista).
    pub intro_bars: u32,
}

impl Default for ComposeSpec {
    fn default() -> Self {
        Self {
            tempo: 66.0,
            beats: 4,
            mode: vec![0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 11.0],
            progression: vec![0, 5, 3, 4],
            bars_per_chord: 2,
            seventh: 0.2,
            add9: 0.3,
            sus: 0.1,
            bass: Some(Voice { octave: -2, amp: 0.3, beats: 7.5, curve: 1.0, ..Voice::default() }),
            pad: None,
            arp: Some(Voice { octave: -1, amp: 0.2, beats: 3.0, ..Voice::default() }),
            arp_density: 0.5,
            arp_pattern: vec![0, 1, 2, 1, 3, 2, 1, 2],
            steps_per_beat: 2,
            lead: Some(Voice { octave: 0, amp: 0.26, beats: 2.5, ..Voice::default() }),
            lead_phrase: 0.7,
            phrase_bars: 2,
            motif_len: 4,
            motif_repeat: 0.5,
            dynamics: 0.25,
            humanize: 0.012,
            intro_bars: 1,
        }
    }
}

/// Un motivo: (compás-relativo en tiempos, paso de escala respecto del primer sonido, duración en tiempos).
type Motivo = Vec<(f32, i32, f32)>;

fn semis(mode: &[f32], idx: i32) -> f32 {
    let n = mode.len() as i32;
    mode[idx.rem_euclid(n) as usize] + 12.0 * idx.div_euclid(n) as f32
}

fn op_de(v: &Voice, base: f32, mode: &[f32], idx: i32, at: f32, dur: f32, vel: f32) -> ToneOp {
    let f = base * 2.0f32.powf((semis(mode, idx) + 12.0 * v.octave as f32) / 12.0);
    ToneOp {
        wave: v.wave,
        at,
        dur: dur.max(0.05),
        f0: f,
        f1: f,
        amp: v.amp * vel,
        attack: v.attack,
        curve: v.curve,
        detune: v.detune,
        fm_ratio: v.fm_ratio,
        fm_index: v.fm_index,
        cutoff0: v.cutoff,
        cutoff1: v.cutoff,
        res: v.res,
        ..ToneOp::default()
    }
}

/// Las notas del acorde del grado `d` (índices de escala): tríada, con séptima/novena/sus según azar.
fn acorde(c: &ComposeSpec, d: i32, rng: &mut Lcg) -> Vec<i32> {
    let mut v = vec![d, if rng.f32() < c.sus { d + 1 } else { d + 2 }, d + 4];
    if rng.f32() < c.seventh {
        v.push(d + 6);
    }
    if rng.f32() < c.add9 {
        v.push(d + 8);
    }
    v
}

pub fn render_composition(t: &TrackSpec, c: &ComposeSpec) -> Vec<f32> {
    let mut s = Synth::new(t.rate, t.dur.max(1.0));
    let mut rng = Lcg(t.seed);
    let spb = 60.0 / c.tempo.max(30.0);
    let barra = spb * c.beats.max(1) as f32;
    let barras = ((t.dur - 2.0) / barra).floor().max(1.0) as u32;
    let mode: &[f32] = if c.mode.len() == 7 { &c.mode } else { &[0.0, 2.0, 4.0, 5.0, 7.0, 9.0, 11.0] };
    let prog: &[i32] = if c.progression.is_empty() { &[0] } else { &c.progression };
    let bpc = c.bars_per_chord.max(1);
    let humano = |rng: &mut Lcg, v: f32| (v + rng.range(-c.humanize, c.humanize)).max(0.0);
    let vel = |rng: &mut Lcg| 1.0 - c.dynamics * rng.f32();
    let mut motivo: Option<Motivo> = None;
    let mut tonos_actuales: Vec<i32> = vec![0, 2, 4];

    for b in c.intro_bars..barras {
        let t_barra = b as f32 * barra;
        let chord_i = (b / bpc) as usize % prog.len();
        let nuevo_acorde = b % bpc == 0;
        let grado = prog[chord_i];
        if nuevo_acorde {
            tonos_actuales = acorde(c, grado, &mut rng);
            let dur_acorde = barra * bpc as f32;
            if let Some(v) = &c.bass {
                let at = humano(&mut rng, t_barra);
                s.tone(&op_de(v, t.base, mode, grado, at, (v.beats * spb).min(dur_acorde), 0.9 + 0.1 * rng.f32()));
            }
            if let Some(v) = &c.pad {
                for (k, idx) in tonos_actuales.iter().enumerate() {
                    let at = t_barra + k as f32 * 0.05;
                    s.tone(&op_de(v, t.base, mode, *idx, at, dur_acorde + 0.8, 0.8 + 0.2 * rng.f32()));
                }
            }
        } else if let Some(v) = &c.bass {
            // el bajo vuelve a tocar a mitad del acorde, más suave
            if rng.f32() < 0.45 {
                let at = humano(&mut rng, t_barra);
                s.tone(&op_de(v, t.base, mode, grado, at, v.beats * spb * 0.6, 0.6));
            }
        }

        // arpegio sobre la grilla de la barra
        if let Some(v) = &c.arp {
            let pasos = c.beats.max(1) * c.steps_per_beat.max(1);
            let paso_s = spb / c.steps_per_beat.max(1) as f32;
            let pat: &[i32] = if c.arp_pattern.is_empty() { &[0, 1, 2] } else { &c.arp_pattern };
            for p in 0..pasos {
                if rng.f32() > c.arp_density {
                    continue;
                }
                let n = pat[(p as usize + b as usize) % pat.len()];
                let len = tonos_actuales.len() as i32;
                let idx = tonos_actuales[n.rem_euclid(len) as usize] + 7 * n.div_euclid(len);
                let at = humano(&mut rng, t_barra + p as f32 * paso_s);
                s.tone(&op_de(v, t.base, mode, idx, at, v.beats * spb, vel(&mut rng)));
            }
        }

        // melodía: una frase cada `phrase_bars` compases (o silencio)
        if let Some(v) = &c.lead {
            if b % c.phrase_bars.max(1) == 0 {
                if rng.f32() < c.lead_phrase {
                    let m: Motivo = match (&motivo, rng.f32() < c.motif_repeat) {
                        (Some(m), true) => m.clone(),
                        _ => {
                            let largo = c.motif_len.clamp(2, 8);
                            let mut at_b = if rng.f32() < 0.5 { 0.0 } else { 1.0 };
                            let mut paso = 0i32;
                            let mut m = Vec::new();
                            for i in 0..largo {
                                let dur = [1.0f32, 1.5, 2.0, 3.0][(rng.next() % 4) as usize];
                                m.push((at_b, paso, dur));
                                at_b += dur + if rng.f32() < 0.3 { 1.0 } else { 0.0 };
                                if i + 1 < largo {
                                    paso += [-2, -1, -1, 1, 1, 2, 3][(rng.next() % 7) as usize];
                                    paso = paso.clamp(-3, 6);
                                }
                            }
                            m
                        }
                    };
                    motivo = Some(m.clone());
                    // el motivo se apoya en una nota del acorde vigente
                    let ancla = tonos_actuales[(rng.next() as usize) % tonos_actuales.len()] + 7;
                    let limite = barra * c.phrase_bars.max(1) as f32 - 0.2;
                    for (at_b, paso, dur) in m {
                        let at = t_barra + at_b * spb;
                        if at - t_barra > limite || at + 1.0 >= t.dur - 2.5 {
                            continue;
                        }
                        let at = humano(&mut rng, at);
                        s.tone(&op_de(v, t.base, mode, ancla + paso, at, dur * spb + 0.9, 0.85 + 0.15 * rng.f32()));
                    }
                }
            }
        }
    }

    let mut buf = s.buf;
    for fx in &t.effects {
        crate::apply_effect(&mut buf, t.rate, fx);
    }
    crate::pulir(&mut buf, t.rate);
    buf
}
