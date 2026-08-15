//! Motor de sonido compartido — spec serializable (`audio.json`) + synth
//! 8/16-bit determinista. Lo consumen `ha-game` (sintetiza los bancos en boot)
//! y el editor de sonido de `ha-asset-tools` (edita/escucha/guarda el JSON).
//!
//! Sin deps de Bevy: todo devuelve buffers `f32` o bytes WAV.
//! Diseño: `biblia.md` §7.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SFX_RATE: u32 = 22_050;
pub const MUSIC_RATE: u32 = 16_000;

// ---------------------------------------------------------------------------
// Spec — la "partitura" editable de TODO el audio del juego
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wave {
    Sine,
    Square,
    Triangle,
    Saw,
    Noise,
}

impl Wave {
    pub const ALL: [Wave; 5] = [Wave::Sine, Wave::Square, Wave::Triangle, Wave::Saw, Wave::Noise];
    pub fn label(self) -> &'static str {
        match self {
            Wave::Sine => "seno",
            Wave::Square => "square",
            Wave::Triangle => "triangulo",
            Wave::Saw => "sierra",
            Wave::Noise => "ruido",
        }
    }
}

/// Una capa de tono: `f0→f1` Hz durante `dur` s arrancando en `at` s.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ToneOp {
    pub wave: Wave,
    pub at: f32,
    pub dur: f32,
    pub f0: f32,
    pub f1: f32,
    pub amp: f32,
    /// Fracción de la duración que ocupa el ataque (0..1).
    pub attack: f32,
    #[serde(default)]
    pub vibrato: f32,
}

/// Efecto de post-proceso, se aplican en orden sobre el buffer.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "fx", rename_all = "snake_case")]
pub enum Effect {
    /// Filtro paso-bajo de un polo. `alpha` chico = más grave/apagado (0..1).
    Lowpass { alpha: f32 },
    /// Eco simple: repite con `feedback` cada `delay` s, mezclado por `mix`.
    Echo { delay: f32, feedback: f32, mix: f32 },
    /// Reverberación (4 combs). `size` 0..1 = tamaño de sala, `mix` 0..1.
    Reverb { size: f32, mix: f32 },
    /// Sonido "más 8-bit": cuantiza a `bits` y retiene cada `downsample` muestras.
    Bitcrush { bits: u32, downsample: u32 },
    /// Fundido de salida en los últimos `secs` s.
    FadeOut { secs: f32 },
    /// Crossfade circular (para loops sin click), `secs` del borde.
    LoopCrossfade { secs: f32 },
    /// Escala el pico máximo a `peak` (0..1).
    Normalize { peak: f32 },
}

/// Un efecto de sonido completo: capas de tono + cadena de efectos.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SfxSpec {
    pub dur: f32,
    #[serde(default = "d_sfx_rate")]
    pub rate: u32,
    pub tones: Vec<ToneOp>,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

fn d_sfx_rate() -> u32 { SFX_RATE }
fn d_music_rate() -> u32 { MUSIC_RATE }

/// Pad de fondo de una pista (colchón casi inaudible sobre la tónica).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PadSpec {
    pub wave: Wave,
    /// Multiplicador sobre la nota base (0.5 = una octava abajo).
    pub freq_mult: f32,
    pub amp: f32,
    pub attack: f32,
    pub vibrato: f32,
}

impl Default for PadSpec {
    fn default() -> Self {
        Self { wave: Wave::Sine, freq_mult: 0.5, amp: 0.05, attack: 0.4, vibrato: 0.5 }
    }
}

/// Parámetros del generador de melodía (notas escasas estilo C418).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct MelodySpec {
    pub wave: Wave,
    pub amp: f32,
    pub attack: f32,
    pub vibrato: f32,
    /// Duración de cada nota (rango aleatorio, s).
    pub note_min: f32,
    pub note_max: f32,
    /// Espera entre notas (s).
    pub gap_min: f32,
    pub gap_max: f32,
    /// Probabilidad de silencio en vez de nota (0..1) y su duración.
    pub rest_prob: f32,
    pub rest_min: f32,
    pub rest_max: f32,
    /// Armonía ocasional una octava abajo.
    pub harmony_prob: f32,
    pub harmony_wave: Wave,
    pub harmony_amp: f32,
    pub harmony_attack: f32,
    pub harmony_vibrato: f32,
}

impl Default for MelodySpec {
    fn default() -> Self {
        Self {
            wave: Wave::Triangle,
            amp: 0.16,
            attack: 0.35,
            vibrato: 0.6,
            note_min: 1.2,
            note_max: 2.6,
            gap_min: 0.9,
            gap_max: 2.2,
            rest_prob: 0.35,
            rest_min: 0.8,
            rest_max: 2.4,
            harmony_prob: 0.3,
            harmony_wave: Wave::Sine,
            harmony_amp: 0.09,
            harmony_attack: 0.4,
            harmony_vibrato: 0.4,
        }
    }
}

/// Una pista musical generativa (determinista por semilla).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackSpec {
    pub name: String,
    /// Contexto donde puede sonar: "surface" | "space" | "underground".
    pub mood: String,
    pub seed: u32,
    pub dur: f32,
    /// Nota base en Hz (tónica).
    pub base: f32,
    /// Escala en semitonos sobre la base.
    pub scale: Vec<f32>,
    #[serde(default = "d_music_rate")]
    pub rate: u32,
    #[serde(default)]
    pub pad: Option<PadSpec>,
    #[serde(default)]
    pub melody: MelodySpec,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

/// Tiempos del director de música (silencios entre pistas, en s).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct DirectorSpec {
    pub first_delay: f32,
    pub silence_min: f32,
    pub silence_max: f32,
}

impl Default for DirectorSpec {
    fn default() -> Self {
        Self { first_delay: 25.0, silence_min: 90.0, silence_max: 240.0 }
    }
}

/// Spec completo — lo que vive en `native/assets/audio.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioSpec {
    pub sfx_volume: f32,
    pub music_volume: f32,
    #[serde(default)]
    pub director: DirectorSpec,
    /// SFX por nombre lógico (ver `ha-game` `Sfx::key()`).
    pub sfx: BTreeMap<String, SfxSpec>,
    /// Loop de empuje del cohete (loopeable).
    pub thrust: SfxSpec,
    pub tracks: Vec<TrackSpec>,
}

impl AudioSpec {
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("AudioSpec siempre serializa")
    }
}

// ---------------------------------------------------------------------------
// Synth — osciladores + envolventes sobre un buffer f32 (portado de audio.rs)
// ---------------------------------------------------------------------------

/// LCG determinista (mismo sonido en cada máquina/arranque).
pub struct Lcg(pub u32);

impl Lcg {
    pub fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0
    }
    pub fn f32(&mut self) -> f32 {
        (self.next() >> 8) as f32 / 16_777_216.0
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.f32() * (hi - lo)
    }
}

pub struct Synth {
    pub rate: f32,
    pub buf: Vec<f32>,
    rng: Lcg,
}

impl Synth {
    pub fn new(rate: u32, dur: f32) -> Self {
        Self {
            rate: rate as f32,
            buf: vec![0.0; (rate as f32 * dur) as usize],
            rng: Lcg(0x51_17_2e_a1),
        }
    }

    pub fn tone(&mut self, op: &ToneOp) {
        let start = (op.at * self.rate) as usize;
        let n = (op.dur * self.rate) as usize;
        let mut phase = 0.0f32;
        for i in 0..n {
            let Some(slot) = self.buf.get_mut(start + i) else { break };
            let t = i as f32 / n.max(1) as f32;
            let f = op.f0 + (op.f1 - op.f0) * t;
            let f = f * (1.0 + op.vibrato * (t * 37.0).sin() * 0.02);
            phase += f / self.rate;
            let x = phase.fract();
            let s = match op.wave {
                Wave::Sine => (x * std::f32::consts::TAU).sin(),
                Wave::Square => {
                    if x < 0.5 { 1.0 } else { -1.0 }
                }
                Wave::Triangle => 4.0 * (x - 0.5).abs() - 1.0,
                Wave::Saw => 2.0 * x - 1.0,
                Wave::Noise => self.rng.f32() * 2.0 - 1.0,
            };
            // Envolvente: ataque lineal hasta `attack`, luego caída suave.
            let env = if t < op.attack {
                t / op.attack.max(1e-4)
            } else {
                let d = (t - op.attack) / (1.0 - op.attack).max(1e-4);
                (1.0 - d).powf(1.6)
            };
            *slot += s * op.amp * env;
        }
    }
}

// ---------------------------------------------------------------------------
// Efectos
// ---------------------------------------------------------------------------

pub fn apply_effect(buf: &mut Vec<f32>, rate: u32, fx: &Effect) {
    let n = buf.len();
    match *fx {
        Effect::Lowpass { alpha } => {
            let a = alpha.clamp(0.01, 1.0);
            let mut prev = 0.0f32;
            for s in buf.iter_mut() {
                prev = prev * (1.0 - a) + *s * a;
                *s = prev;
            }
        }
        Effect::Echo { delay, feedback, mix } => {
            let d = ((delay.max(0.005) * rate as f32) as usize).max(1);
            let fb = feedback.clamp(0.0, 0.95);
            for i in d..n {
                let w = buf[i - d] * fb;
                buf[i] += w * mix.clamp(0.0, 1.0);
            }
        }
        Effect::Reverb { size, mix } => {
            let mix = mix.clamp(0.0, 1.0);
            let g = (0.45 + 0.35 * size.clamp(0.0, 1.0)).min(0.85);
            let dry = buf.clone();
            let mut wet = vec![0.0f32; n];
            for base_delay in [0.0297f32, 0.0371, 0.0411, 0.0437] {
                let d = (((base_delay * (0.5 + size)) * rate as f32) as usize).max(1);
                let mut comb = vec![0.0f32; n];
                for i in 0..n {
                    let fbk = if i >= d { comb[i - d] * g } else { 0.0 };
                    comb[i] = dry[i] + fbk;
                }
                for i in 0..n {
                    wet[i] += comb[i] * 0.25;
                }
            }
            for i in 0..n {
                buf[i] = dry[i] * (1.0 - mix * 0.4) + wet[i] * mix;
            }
        }
        Effect::Bitcrush { bits, downsample } => {
            let levels = (1u32 << bits.clamp(1, 16)) as f32;
            let ds = downsample.max(1) as usize;
            let mut held = 0.0f32;
            for (i, s) in buf.iter_mut().enumerate() {
                if i % ds == 0 {
                    held = (*s * levels).round() / levels;
                }
                *s = held;
            }
        }
        Effect::FadeOut { secs } => {
            let fade = ((secs * rate as f32) as usize).min(n);
            for i in 0..fade {
                let t = i as f32 / fade.max(1) as f32;
                buf[n - 1 - i] *= t;
            }
        }
        Effect::LoopCrossfade { secs } => {
            let fade = ((secs * rate as f32) as usize).min(n / 2).max(1);
            for i in 0..fade {
                let t = i as f32 / fade as f32;
                let j = n - fade + i;
                buf[j] = buf[j] * (1.0 - t) + buf[i] * t;
            }
        }
        Effect::Normalize { peak } => {
            let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            if max > 1e-6 {
                let k = peak.clamp(0.0, 1.0) / max;
                for s in buf.iter_mut() {
                    *s *= k;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

/// Sintetiza un SFX completo (tonos + efectos) → buffer f32.
pub fn render_sfx(spec: &SfxSpec) -> Vec<f32> {
    let mut s = Synth::new(spec.rate, spec.dur.max(0.01));
    for op in &spec.tones {
        s.tone(op);
    }
    let mut buf = s.buf;
    for fx in &spec.effects {
        apply_effect(&mut buf, spec.rate, fx);
    }
    buf
}

/// Sintetiza una pista musical generativa → buffer f32.
pub fn render_track(t: &TrackSpec) -> Vec<f32> {
    let mut s = Synth::new(t.rate, t.dur.max(1.0));
    let mut rng = Lcg(t.seed);
    let m = &t.melody;

    if let Some(pad) = &t.pad {
        s.tone(&ToneOp {
            wave: pad.wave,
            at: 0.0,
            dur: t.dur,
            f0: t.base * pad.freq_mult,
            f1: t.base * pad.freq_mult,
            amp: pad.amp,
            attack: pad.attack,
            vibrato: pad.vibrato,
        });
    }

    // Melodía: notas espaciadas con silencio frecuente (el aire ES la pista).
    let scale: &[f32] = if t.scale.is_empty() { &[0.0] } else { &t.scale };
    let mut at = rng.range(0.5, 2.0);
    while at < t.dur - 3.0 {
        if rng.f32() < m.rest_prob {
            at += rng.range(m.rest_min, m.rest_max);
            continue;
        }
        let deg = scale[(rng.next() as usize) % scale.len()];
        let f = t.base * (2.0f32).powf(deg / 12.0);
        let nd = rng.range(m.note_min, m.note_max);
        s.tone(&ToneOp {
            wave: m.wave, at, dur: nd, f0: f, f1: f,
            amp: m.amp, attack: m.attack, vibrato: m.vibrato,
        });
        if rng.f32() < m.harmony_prob {
            let deg2 = scale[(rng.next() as usize) % scale.len()];
            let f2 = t.base * (2.0f32).powf((deg2 - 12.0) / 12.0);
            s.tone(&ToneOp {
                wave: m.harmony_wave, at: at + 0.1, dur: nd * 0.9, f0: f2, f1: f2,
                amp: m.harmony_amp, attack: m.harmony_attack, vibrato: m.harmony_vibrato,
            });
        }
        at += rng.range(m.gap_min, m.gap_max);
    }

    let mut buf = s.buf;
    for fx in &t.effects {
        apply_effect(&mut buf, t.rate, fx);
    }
    buf
}

/// WAV 16-bit mono con soft-clip (formato que Bevy y rodio decodifican).
pub fn wav_bytes(rate: u32, buf: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(44 + buf.len() * 2);
    let data_len = (buf.len() * 2) as u32;
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in buf {
        let c = s.tanh(); // soft clip
        b.extend_from_slice(&((c * 32_000.0) as i16).to_le_bytes());
    }
    b
}

// ---------------------------------------------------------------------------
// Defaults — transcripción 1:1 del diseño original (biblia.md §7)
// ---------------------------------------------------------------------------

fn t(wave: Wave, at: f32, dur: f32, f0: f32, f1: f32, amp: f32, attack: f32, vibrato: f32) -> ToneOp {
    ToneOp { wave, at, dur, f0, f1, amp, attack, vibrato }
}

fn sfx(dur: f32, tones: Vec<ToneOp>) -> SfxSpec {
    SfxSpec { dur, rate: SFX_RATE, tones, effects: vec![] }
}

impl AudioSpec {
    /// El diseño original completo — mismo sonido que antes del editor.
    pub fn default_spec() -> Self {
        use Wave::*;
        let mut sfx_map = BTreeMap::new();
        let mut put = |k: &str, v: SfxSpec| {
            sfx_map.insert(k.to_string(), v);
        };

        put("click", sfx(0.05, vec![t(Square, 0.0, 0.04, 1200.0, 800.0, 0.25, 0.05, 0.0)]));
        put("dig", sfx(0.14, vec![
            t(Noise, 0.0, 0.09, 0.0, 0.0, 0.35, 0.02, 0.0),
            t(Sine, 0.0, 0.1, 110.0, 70.0, 0.5, 0.02, 0.0),
        ]));
        put("chop", sfx(0.16, vec![
            t(Noise, 0.0, 0.06, 0.0, 0.0, 0.4, 0.01, 0.0),
            t(Triangle, 0.0, 0.12, 220.0, 140.0, 0.45, 0.01, 0.0),
        ]));
        put("place", sfx(0.1, vec![
            t(Square, 0.0, 0.07, 220.0, 220.0, 0.28, 0.02, 0.0),
            t(Square, 0.0, 0.07, 330.0, 330.0, 0.18, 0.02, 0.0),
        ]));
        let mut stamp = Vec::new();
        for (i, f) in [330.0f32, 262.0, 196.0].iter().enumerate() {
            let at = i as f32 * 0.1;
            stamp.push(t(Square, at, 0.08, *f, *f, 0.28, 0.02, 0.0));
            stamp.push(t(Noise, at, 0.04, 0.0, 0.0, 0.18, 0.02, 0.0));
        }
        put("stamp", sfx(0.34, stamp));
        for (k, f) in [("step_soft1", 90.0f32), ("step_soft2", 110.0)] {
            put(k, sfx(0.05, vec![
                t(Noise, 0.0, 0.04, 0.0, 0.0, 0.10, 0.15, 0.0),
                t(Sine, 0.0, 0.04, f, f * 0.8, 0.12, 0.1, 0.0),
            ]));
        }
        for (k, f) in [("step_hard1", 240.0f32), ("step_hard2", 280.0)] {
            put(k, sfx(0.04, vec![
                t(Noise, 0.0, 0.03, 0.0, 0.0, 0.14, 0.05, 0.0),
                t(Triangle, 0.0, 0.03, f, f * 0.7, 0.10, 0.05, 0.0),
            ]));
        }
        put("bag_open", sfx(0.16, vec![
            t(Square, 0.0, 0.06, 520.0, 520.0, 0.2, 0.05, 0.0),
            t(Square, 0.07, 0.07, 780.0, 780.0, 0.2, 0.05, 0.0),
        ]));
        put("bag_close", sfx(0.16, vec![
            t(Square, 0.0, 0.06, 780.0, 780.0, 0.2, 0.05, 0.0),
            t(Square, 0.07, 0.07, 520.0, 520.0, 0.2, 0.05, 0.0),
        ]));
        put("craft", sfx(0.4, [392.0f32, 494.0, 587.0].iter().enumerate()
            .map(|(i, f)| t(Triangle, i as f32 * 0.11, 0.14, *f, *f, 0.3, 0.1, 0.0))
            .collect()));
        let mut eat = Vec::new();
        for i in 0..3 {
            eat.push(t(Noise, i as f32 * 0.14, 0.07, 0.0, 0.0, 0.25, 0.2, 0.0));
            eat.push(t(Sine, i as f32 * 0.14, 0.07, 160.0, 120.0, 0.2, 0.2, 0.0));
        }
        put("eat", sfx(0.42, eat));
        put("drink", sfx(0.4, (0..3).map(|i| {
            let f = 300.0 + i as f32 * 60.0;
            t(Sine, i as f32 * 0.12, 0.1, f, f * 1.4, 0.22, 0.3, 1.0)
        }).collect()));
        put("hurt", sfx(0.16, vec![t(Square, 0.0, 0.13, 440.0, 180.0, 0.35, 0.02, 0.0)]));
        put("jump", sfx(0.1, vec![t(Sine, 0.0, 0.08, 300.0, 500.0, 0.15, 0.3, 0.0)]));
        put("touchdown", sfx(0.18, vec![
            t(Sine, 0.0, 0.15, 90.0, 60.0, 0.5, 0.02, 0.0),
            t(Noise, 0.0, 0.05, 0.0, 0.0, 0.15, 0.05, 0.0),
        ]));
        put("door_open", sfx(0.3, vec![t(Saw, 0.0, 0.25, 150.0, 220.0, 0.16, 0.2, 3.0)]));
        put("door_close", sfx(0.18, vec![
            t(Saw, 0.0, 0.1, 220.0, 160.0, 0.14, 0.2, 2.0),
            t(Sine, 0.1, 0.07, 90.0, 70.0, 0.35, 0.05, 0.0),
        ]));
        put("stairs", sfx(0.28, vec![t(Noise, 0.0, 0.25, 0.0, 0.0, 0.2, 0.45, 0.0)]));
        put("launch", sfx(0.8, vec![
            t(Noise, 0.0, 0.75, 0.0, 0.0, 0.4, 0.5, 0.0),
            t(Sine, 0.0, 0.6, 130.0, 60.0, 0.3, 0.4, 0.0),
        ]));
        put("pickup", sfx(0.14, vec![
            t(Square, 0.0, 0.05, 880.0, 880.0, 0.2, 0.05, 0.0),
            t(Square, 0.06, 0.07, 1320.0, 1320.0, 0.2, 0.05, 0.0),
        ]));
        // TREPAR un bordillo: raspón de tela y grava contra el borde. Todo ruido
        // con ataque lento — un tono definido sonaría a "objeto", y esto es
        // fricción. El retumbe grave por debajo es el peso del cuerpo subiendo.
        put("climb_ledge", sfx(0.22, vec![
            t(Noise, 0.0, 0.16, 0.0, 0.0, 0.20, 0.35, 0.0),
            t(Sine, 0.05, 0.12, 150.0, 210.0, 0.16, 0.30, 0.0),
        ]));
        // DESCOLGARSE: golpe seco de los dos pies. Grave que cae rápido + el
        // chasquido de la grava encima; más corto y más fuerte que trepar, que es
        // lo que separa "me dejé caer" de "subí a pulso".
        put("drop_ledge", sfx(0.2, vec![
            t(Sine, 0.0, 0.14, 130.0, 55.0, 0.42, 0.01, 0.0),
            t(Noise, 0.0, 0.07, 0.0, 0.0, 0.22, 0.02, 0.0),
        ]));

        // Loop de empuje: ruido+retumbe → paso-bajo → crossfade circular.
        let thrust = SfxSpec {
            dur: 1.0,
            rate: SFX_RATE,
            tones: vec![
                t(Noise, 0.0, 1.0, 0.0, 0.0, 0.30, 0.02, 0.0),
                t(Sine, 0.0, 1.0, 55.0, 55.0, 0.20, 0.02, 2.0),
            ],
            effects: vec![
                Effect::Lowpass { alpha: 0.14 },
                Effect::LoopCrossfade { secs: 0.02 },
            ],
        };

        let track = |name: &str, mood: &str, seed: u32, dur: f32, base: f32, scale: &[f32]| TrackSpec {
            name: name.to_string(),
            mood: mood.to_string(),
            seed,
            dur,
            base,
            scale: scale.to_vec(),
            rate: MUSIC_RATE,
            pad: Some(PadSpec::default()),
            melody: MelodySpec::default(),
            effects: vec![Effect::FadeOut { secs: 3.0 }],
        };
        let tracks = vec![
            track("pradera-1", "surface", 0xAA01, 42.0, 220.0, &[0.0, 2.0, 4.0, 7.0, 9.0, 12.0, 14.0]),
            track("pradera-2", "surface", 0xAA02, 42.0, 220.0, &[0.0, 2.0, 4.0, 7.0, 9.0, 12.0, 14.0]),
            track("pradera-3", "surface", 0xAA03, 42.0, 220.0, &[0.0, 2.0, 4.0, 7.0, 9.0, 12.0, 14.0]),
            track("vacio-estelar", "space", 0xBB01, 55.0, 138.6, &[0.0, 3.0, 5.0, 7.0, 10.0, 12.0]),
            track("subsuelo", "underground", 0xCC01, 48.0, 164.8, &[0.0, 3.0, 5.0, 8.0, 10.0]),
        ];

        AudioSpec {
            sfx_volume: 0.8,
            music_volume: 0.5,
            director: DirectorSpec::default(),
            sfx: sfx_map,
            thrust,
            tracks,
        }
    }
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_spec_roundtrip_json() {
        let spec = AudioSpec::default_spec();
        let json = spec.to_json();
        let back = AudioSpec::from_json(&json).expect("parsea su propio JSON");
        assert_eq!(back.sfx.len(), spec.sfx.len());
        assert_eq!(back.tracks.len(), spec.tracks.len());
        assert_eq!(back.to_json(), json);
    }

    #[test]
    fn render_sfx_not_silent() {
        let spec = AudioSpec::default_spec();
        for (k, s) in &spec.sfx {
            let buf = render_sfx(s);
            assert!(!buf.is_empty(), "{k} vacio");
            let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.01, "{k} silencioso (peak {peak})");
        }
        let thrust = render_sfx(&spec.thrust);
        assert!(thrust.iter().any(|s| s.abs() > 0.01));
    }

    #[test]
    fn render_track_not_silent_and_deterministic() {
        let spec = AudioSpec::default_spec();
        let t = &spec.tracks[0];
        let a = render_track(t);
        let b = render_track(t);
        assert_eq!(a.len(), b.len());
        assert!(a.iter().zip(&b).all(|(x, y)| x == y), "no determinista");
        assert!(a.iter().any(|s| s.abs() > 0.01));
    }

    #[test]
    fn effects_dont_explode() {
        let mut buf: Vec<f32> = (0..22050).map(|i| ((i as f32) * 0.05).sin() * 0.3).collect();
        for fx in [
            Effect::Lowpass { alpha: 0.2 },
            Effect::Echo { delay: 0.1, feedback: 0.5, mix: 0.5 },
            Effect::Reverb { size: 0.6, mix: 0.4 },
            Effect::Bitcrush { bits: 5, downsample: 3 },
            Effect::FadeOut { secs: 0.2 },
            Effect::LoopCrossfade { secs: 0.05 },
            Effect::Normalize { peak: 0.9 },
        ] {
            apply_effect(&mut buf, 22_050, &fx);
            assert!(buf.iter().all(|s| s.is_finite()));
        }
        let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.5 && peak <= 1.0);
    }
}
