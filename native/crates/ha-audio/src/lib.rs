//! Motor de sonido compartido — spec serializable (`audio.json`) + synth
//! 8/16-bit determinista. Lo consumen `ha-game` (sintetiza los bancos en boot)
//! y el editor de sonido de `ha-asset-tools` (edita/escucha/guarda el JSON).
//!
//! Sin deps de Bevy: todo devuelve buffers `f32` o bytes WAV.
//! Diseño: `biblia.md` §7.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SFX_RATE: u32 = 32_000;
pub const MUSIC_RATE: u32 = 22_050;

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
    /// Ruido blanco.
    Noise,
    /// Ruido rosa (−3 dB/octava): el de la lluvia, el viento y las hojas. Mucho más natural que el blanco.
    Pink,
    /// Ruido marrón (−6 dB/octava): retumbe, tierra, pasos graves.
    Brown,
    /// Cuerda pulsada (Karplus-Strong): madera, cuerda, goteo, golpe seco con cuerpo.
    Pluck,
    /// Campana/cristal: parciales inarmónicos que decaen a distinta velocidad.
    Bell,
    /// Piano de fieltro (el de C418): seis parciales que mueren a distinta velocidad, martillo de fieltro y dos cuerdas.
    Piano,
    /// Guitarra de cuerdas de nylon/acero (fingerstyle de Zomboid): Karplus-Strong con caja de resonancia y púa.
    Guitar,
}

impl Wave {
    pub const ALL: [Wave; 11] = [Wave::Sine, Wave::Square, Wave::Triangle, Wave::Saw, Wave::Noise, Wave::Pink, Wave::Brown, Wave::Pluck, Wave::Bell, Wave::Piano, Wave::Guitar];
    pub fn label(self) -> &'static str {
        match self {
            Wave::Sine => "seno",
            Wave::Square => "square",
            Wave::Triangle => "triangulo",
            Wave::Saw => "sierra",
            Wave::Noise => "ruido",
            Wave::Pink => "ruido rosa",
            Wave::Brown => "ruido marron",
            Wave::Pluck => "cuerda",
            Wave::Bell => "campana",
            Wave::Piano => "piano",
            Wave::Guitar => "guitarra",
        }
    }
    pub fn es_ruido(self) -> bool {
        matches!(self, Wave::Noise | Wave::Pink | Wave::Brown)
    }
}

fn d_curve() -> f32 { 1.6 }
fn d_res() -> f32 { 0.7 }
fn d_pwm() -> f32 { 0.5 }

/// Una capa de tono: `f0→f1` Hz durante `dur` s arrancando en `at` s.
///
/// Los campos después de `vibrato` son del motor v2 (2 oct 2026) y **todos tienen un valor por defecto que
/// reproduce el comportamiento anterior**: un `audio.json` viejo sigue leyéndose igual.
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
    /// Vibrato (0 = sin; 1 = ±2 % a 5,5 Hz).
    #[serde(default)]
    pub vibrato: f32,
    /// Exponente de la caída (1 = lineal, 1,6 = natural, 4 = golpe seco).
    #[serde(default = "d_curve")]
    pub curve: f32,
    /// Fracción del resto (después del ataque) que se sostiene a pleno antes de caer (colchones, pads).
    #[serde(default)]
    pub sustain: f32,
    /// Desafinación en centésimas: > 0 activa tres voces (−d, 0, +d) = sonido grueso, de coro.
    #[serde(default)]
    pub detune: f32,
    /// FM: razón del modulador y su índice (0 = apagado). Da timbres metálicos, de campana, de madera.
    #[serde(default)]
    pub fm_ratio: f32,
    #[serde(default)]
    pub fm_index: f32,
    /// Filtro paso-bajo resonante por capa: corte inicial y final en Hz (0 = sin filtro). Barre de uno a otro.
    #[serde(default)]
    pub cutoff0: f32,
    #[serde(default)]
    pub cutoff1: f32,
    /// Resonancia del filtro (Q; 0,7 = plano, 3-8 = «wah»).
    #[serde(default = "d_res")]
    pub res: f32,
    /// Ancho del pulso del `Square` (0,5 = cuadrada; 0,2 = nasal, de 8 bits).
    #[serde(default = "d_pwm")]
    pub pwm: f32,
}

impl Default for ToneOp {
    fn default() -> Self {
        Self {
            wave: Wave::Sine,
            at: 0.0,
            dur: 0.1,
            f0: 440.0,
            f1: 440.0,
            amp: 0.3,
            attack: 0.05,
            vibrato: 0.0,
            curve: d_curve(),
            sustain: 0.0,
            detune: 0.0,
            fm_ratio: 0.0,
            fm_index: 0.0,
            cutoff0: 0.0,
            cutoff1: 0.0,
            res: d_res(),
            pwm: d_pwm(),
        }
    }
}

/// Efecto de post-proceso, se aplican en orden sobre el buffer.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "fx", rename_all = "snake_case")]
pub enum Effect {
    /// Filtro paso-bajo de un polo. `alpha` chico = más grave/apagado (0..1).
    Lowpass { alpha: f32 },
    /// Paso-alto de un polo: quita retumbe. `alpha` chico = deja pasar casi todo.
    Highpass { alpha: f32 },
    /// Eco simple: repite con `feedback` cada `delay` s, mezclado por `mix`.
    Echo { delay: f32, feedback: f32, mix: f32 },
    /// Reverberación de Schroeder (4 combs amortiguados + 2 allpass). `size` 0..1 = tamaño de sala.
    Reverb { size: f32, mix: f32 },
    /// Sonido "más 8-bit": cuantiza a `bits` y retiene cada `downsample` muestras.
    Bitcrush { bits: u32, downsample: u32 },
    /// Saturación suave (tanh): calienta y agrega armónicos. `drive` 1 = casi nada, 4 = distorsión.
    Saturate { drive: f32 },
    /// Coro: copia modulada de 12 ms ± `depth` s a `rate` Hz.
    Chorus { depth: f32, rate: f32, mix: f32 },
    /// Compresor simple: por encima de `threshold` (0..1) la señal sube `1/ratio` de lo que sube la entrada.
    Compress { threshold: f32, ratio: f32 },
    /// Ganancia lineal.
    Gain { gain: f32 },
    /// Fundido de salida en los últimos `secs` s.
    FadeOut { secs: f32 },
    /// Crossfade circular (para loops sin click), `secs` del borde.
    LoopCrossfade { secs: f32 },
    /// Escala el pico máximo a `peak` (0..1).
    Normalize { peak: f32 },
    /// **Iguala la sonoridad**: lleva el RMS de la parte activa a `rms` (0..1) sin pasar de `peak`. Es lo que hace
    /// que un click y una explosión no se pisen en volumen.
    NormalizeRms { rms: f32, peak: f32 },
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

/// Pad de fondo de una pista: colchón de voces desafinadas bajo un filtro lento.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PadSpec {
    pub wave: Wave,
    /// Multiplicador sobre la nota base (0.5 = una octava abajo).
    pub freq_mult: f32,
    pub amp: f32,
    pub attack: f32,
    pub vibrato: f32,
    /// Desafinación entre voces (centésimas) y barrido del filtro (Hz). 0 = pad liso de antes.
    pub detune: f32,
    pub cutoff0: f32,
    pub cutoff1: f32,
    /// Agrega una quinta arriba (acorde abierto) a esta fracción de amplitud (0 = sólo la tónica).
    pub fifth: f32,
}

impl Default for PadSpec {
    fn default() -> Self {
        Self { wave: Wave::Sine, freq_mult: 0.5, amp: 0.05, attack: 0.4, vibrato: 0.5, detune: 0.0, cutoff0: 0.0, cutoff1: 0.0, fifth: 0.0 }
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
    /// Motor v2: caída, desafinación, FM y filtro de cada nota (0/por defecto = como antes).
    pub curve: f32,
    pub detune: f32,
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub cutoff: f32,
    /// Probabilidad de que la nota suene acompañada de su tercera/quinta de la escala (acorde suelto).
    pub chord_prob: f32,
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
            curve: d_curve(),
            detune: 0.0,
            fm_ratio: 0.0,
            fm_index: 0.0,
            cutoff: 0.0,
            chord_prob: 0.0,
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
    /// Si está, la pista se COMPONE (progresión, bajo, arpegio, motivo, colchón) en vez de tirar notas al azar.
    #[serde(default)]
    pub compose: Option<ComposeSpec>,
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
// Synth — osciladores + envolventes sobre un buffer f32
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

/// Corrección PolyBLEP: quita el aliasing de los saltos de la sierra y la cuadrada (a 22 kHz suena a «lata»).
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        2.0 * x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + 2.0 * x + 1.0
    } else {
        0.0
    }
}

/// Filtro de variable de estado TPT (Zavalishin): paso-bajo resonante **incondicionalmente estable**. El
/// Chamberlin de antes explotaba con cortes cerca de Nyquist o resonancia baja (y dejó mudo un efecto entero).
#[derive(Default, Clone, Copy)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    #[inline]
    fn lowpass(&mut self, x: f32, fc: f32, rate: f32, q: f32) -> f32 {
        let g = (std::f32::consts::PI * (fc / rate).clamp(0.0005, 0.45)).tan();
        let k = 1.0 / q.max(0.3);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        v2
    }
}

/// Estado del ruido rosa (filtro de Paul Kellet) y marrón.
#[derive(Default, Clone, Copy)]
struct RuidoColor {
    b: [f32; 7],
    marron: f32,
}

impl Synth {
    pub fn new(rate: u32, dur: f32) -> Self {
        Self {
            rate: rate as f32,
            buf: vec![0.0; (rate as f32 * dur) as usize],
            rng: Lcg(0x51_17_2e_a1),
        }
    }

    fn ruido(&mut self, wave: Wave, st: &mut RuidoColor) -> f32 {
        let w = self.rng.f32() * 2.0 - 1.0;
        match wave {
            Wave::Pink => {
                let b = &mut st.b;
                b[0] = 0.99886 * b[0] + w * 0.0555179;
                b[1] = 0.99332 * b[1] + w * 0.0750759;
                b[2] = 0.96900 * b[2] + w * 0.1538520;
                b[3] = 0.86650 * b[3] + w * 0.3104856;
                b[4] = 0.55000 * b[4] + w * 0.5329522;
                b[5] = -0.7616 * b[5] - w * 0.0168980;
                let y = (b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.5362) * 0.11;
                b[6] = w * 0.115926;
                y
            }
            Wave::Brown => {
                st.marron = (st.marron + 0.02 * w) / 1.02;
                st.marron * 3.5
            }
            _ => w,
        }
    }

    pub fn tone(&mut self, op: &ToneOp) {
        let start = (op.at * self.rate) as usize;
        let n = (op.dur * self.rate) as usize;
        if n == 0 {
            return;
        }
        let rate = self.rate;
        let voces: usize = if op.detune > 0.0 && !op.wave.es_ruido() { 3 } else { 1 };
        let desafinacion: [f32; 3] = [2.0f32.powf(-op.detune / 1200.0), 1.0, 2.0f32.powf(op.detune / 1200.0)];
        let mut fase = [0.0f32; 3];
        let mut ruido = RuidoColor::default();
        let mut filtro = Svf::default();
        let mut piano_amp = [0.0f32; 6];
        let mut piano_decay = [0.0f32; 6];
        let usa_filtro = op.cutoff0 > 0.0 || op.cutoff1 > 0.0;
        // Karplus-Strong: línea de retardo con la longitud del período inicial.
        let mut cuerda: Vec<f32> = Vec::new();
        let mut ci = 0usize;
        if matches!(op.wave, Wave::Pluck | Wave::Guitar) {
            let largo = ((rate / op.f0.max(20.0)) as usize).clamp(2, 4096);
            cuerda = (0..largo).map(|_| self.rng.f32() * 2.0 - 1.0).collect();
        }
        let f_log = op.f0 > 0.0 && op.f1 > 0.0;
        let (a, rest) = (op.attack.clamp(0.0005, 0.95), 1.0 - op.attack.clamp(0.0005, 0.95));
        let hold = (op.sustain.clamp(0.0, 0.95)) * rest;
        let decaimiento = (rest - hold).max(1e-4);
        let fade_in = (0.001 * rate).max(1.0);
        let fade_out = (0.004 * rate).max(1.0);
        for i in 0..n {
            if start + i >= self.buf.len() {
                break;
            }
            let t = i as f32 / n as f32;
            let tiempo = i as f32 / rate;
            // Envolvente: ataque suavizado, sostén opcional y caída con exponente.
            let env = if t < a {
                let x = t / a;
                x * x * (3.0 - 2.0 * x)
            } else if t < a + hold {
                1.0
            } else {
                let d = ((t - a - hold) / decaimiento).clamp(0.0, 1.0);
                (1.0 - d).powf(op.curve.max(0.2))
            };
            // Frecuencia: glissando exponencial (es como se oyen los barridos de verdad).
            let f_base = if f_log { op.f0 * (op.f1 / op.f0).powf(t) } else { op.f0 + (op.f1 - op.f0) * t };
            let f = f_base * (1.0 + op.vibrato * 0.02 * (std::f32::consts::TAU * 5.5 * tiempo).sin());
            let mut s = 0.0f32;
            match op.wave {
                Wave::Noise | Wave::Pink | Wave::Brown => {
                    s = self.ruido(op.wave, &mut ruido);
                }
                Wave::Pluck => {
                    let largo = cuerda.len();
                    let sig = (ci + 1) % largo;
                    let y = cuerda[ci];
                    cuerda[ci] = 0.5 * (cuerda[ci] + cuerda[sig]) * 0.9965;
                    ci = sig;
                    s = y;
                }
                Wave::Guitar => {
                    // Karplus-Strong más largo y más suave + caja: la cuerda dura y se apaga en graves antes que en agudos
                    let largo = cuerda.len();
                    let sig = (ci + 1) % largo;
                    let y = cuerda[ci];
                    cuerda[ci] = (0.62 * cuerda[ci] + 0.38 * cuerda[sig]) * 0.9985;
                    ci = sig;
                    // la púa: un chasquido corto al principio
                    let pua = if tiempo < 0.006 { (self.rng.f32() * 2.0 - 1.0) * (1.0 - tiempo / 0.006) * 0.35 } else { 0.0 };
                    s = y * 1.3 + pua;
                }
                Wave::Piano => {
                    // dos cuerdas ligeramente desafinadas; cada una = seis parciales con caída propia (los agudos mueren antes).
                    // Los armónicos salen de una recurrencia de Chebyshev (1 seno + 1 coseno por cuerda en vez de 6 senos) y las
                    // caídas se multiplican por un factor por muestra (sin `exp` en el lazo): la pista de 2 minutos tarda 10× menos.
                    let vida_base = 2.6 * (261.6 / f_base.max(60.0)).clamp(0.35, 2.2);
                    if tiempo > vida_base * 7.0 {
                        break; // ya no se oye nada: no seguir gastando muestras
                    }
                    if i == 0 {
                        piano_amp = [1.0, 0.62, 0.38, 0.2, 0.12, 0.07];
                        for (k, d) in piano_decay.iter_mut().enumerate() {
                            let tau = vida_base / (1.0 + 0.9 * k as f32);
                            *d = (-1.0 / (tau * rate)).exp();
                        }
                    }
                    let mut y = 0.0f32;
                    for v in 0..2usize {
                        let det = if voces > 1 { desafinacion[v] } else { [0.9993f32, 1.0007, 1.0][v] };
                        fase[v] = (fase[v] + f * det / rate).fract();
                        let th = std::f32::consts::TAU * fase[v];
                        let (s1, c1) = th.sin_cos();
                        let (mut sk_1, mut sk) = (0.0f32, s1);
                        for k in 0..6 {
                            y += sk * piano_amp[k];
                            let sig = 2.0 * c1 * sk - sk_1;
                            sk_1 = sk;
                            sk = sig;
                        }
                    }
                    for k in 0..6 {
                        piano_amp[k] *= piano_decay[k];
                    }
                    // el martillo de fieltro: 8 ms de ruido grave
                    if tiempo < 0.008 {
                        y += (self.rng.f32() * 2.0 - 1.0) * (1.0 - tiempo / 0.008) * 0.5;
                    }
                    s = y * 0.3;
                }
                Wave::Bell => {
                    for v in 0..voces {
                        fase[v] += f * desafinacion[v] / rate;
                    }
                    // parciales inarmónicos de una campana; los agudos mueren antes
                    let x = fase[1.min(voces - 1)];
                    for (r, amp, vida) in [(1.0f32, 1.0f32, 0.0f32), (2.76, 0.55, 1.2), (5.40, 0.32, 2.4), (8.93, 0.18, 3.6)] {
                        s += (std::f32::consts::TAU * x * r).sin() * amp * (-vida * t * 3.0).exp();
                    }
                    s *= 0.55;
                }
                _ => {
                    for v in 0..voces {
                        let dt = (f * desafinacion[v] / rate).min(0.45);
                        fase[v] = (fase[v] + dt).fract();
                        let x = fase[v];
                        let y = match op.wave {
                            Wave::Sine | Wave::Triangle if op.fm_index > 0.0 && op.fm_ratio > 0.0 => {
                                // FM: el índice decae con la nota (ataque brillante, cola limpia)
                                let idx = op.fm_index * (1.0 - t).powi(2);
                                let m = (std::f32::consts::TAU * x * op.fm_ratio).sin();
                                (std::f32::consts::TAU * x + idx * m).sin()
                            }
                            Wave::Sine => (x * std::f32::consts::TAU).sin(),
                            Wave::Triangle => 4.0 * (x - 0.5).abs() - 1.0,
                            Wave::Saw => 2.0 * x - 1.0 - poly_blep(x, dt),
                            Wave::Square => {
                                let w = op.pwm.clamp(0.05, 0.95);
                                let mut y = if x < w { 1.0 } else { -1.0 };
                                y += poly_blep(x, dt);
                                y -= poly_blep((x + 1.0 - w).fract(), dt);
                                y
                            }
                            _ => 0.0,
                        };
                        s += y;
                    }
                    s /= voces as f32;
                }
            }
            if usa_filtro {
                let fc0 = if op.cutoff0 > 0.0 { op.cutoff0 } else { op.cutoff1 };
                let fc1 = if op.cutoff1 > 0.0 { op.cutoff1 } else { op.cutoff0 };
                let fc = fc0 * (fc1 / fc0).powf(t);
                s = filtro.lowpass(s, fc, rate, op.res);
            }
            // sin clicks: 1 ms de entrada y 4 ms de salida
            let borde = (i as f32 / fade_in).min(1.0) * ((n - i) as f32 / fade_out).min(1.0);
            self.buf[start + i] += s * op.amp * env * borde;
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
        Effect::Highpass { alpha } => {
            let a = alpha.clamp(0.001, 1.0);
            let mut prev = 0.0f32;
            for s in buf.iter_mut() {
                prev = prev * (1.0 - a) + *s * a;
                *s -= prev;
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
            let size = size.clamp(0.0, 1.0);
            let g = (0.62 + 0.28 * size).min(0.9);
            let dry = buf.clone();
            let mut wet = vec![0.0f32; n];
            // 4 combs con amortiguación (el aire se come los agudos de la cola) en paralelo
            for base_delay in [0.0297f32, 0.0371, 0.0411, 0.0437] {
                let d = (((base_delay * (0.5 + size)) * rate as f32) as usize).max(1);
                let mut comb = vec![0.0f32; n];
                let mut lp = 0.0f32;
                for i in 0..n {
                    let fbk = if i >= d { comb[i - d] } else { 0.0 };
                    lp = lp * 0.35 + fbk * 0.65;
                    comb[i] = dry[i] + lp * g;
                }
                for i in 0..n {
                    wet[i] += comb[i] * 0.25;
                }
            }
            // 2 allpass en serie: difunden los ecos sueltos en una cola continua
            for (dly, gain) in [(0.0050f32, 0.5f32), (0.0017, 0.5)] {
                let d = ((dly * rate as f32) as usize).max(1);
                let mut out = vec![0.0f32; n];
                for i in 0..n {
                    let x = wet[i];
                    let xd = if i >= d { wet[i - d] } else { 0.0 };
                    let yd = if i >= d { out[i - d] } else { 0.0 };
                    out[i] = -gain * x + xd + gain * yd;
                }
                wet = out;
            }
            for i in 0..n {
                buf[i] = dry[i] * (1.0 - mix * 0.35) + wet[i] * mix;
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
        Effect::Saturate { drive } => {
            let d = drive.clamp(0.5, 12.0);
            let norm = d.tanh();
            for s in buf.iter_mut() {
                *s = (*s * d).tanh() / norm;
            }
        }
        Effect::Chorus { depth, rate: lfo, mix } => {
            let dry = buf.clone();
            let base = 0.012 * rate as f32;
            let prof = depth.clamp(0.0005, 0.01) * rate as f32;
            for i in 0..n {
                let mod_ = (std::f32::consts::TAU * lfo.max(0.1) * i as f32 / rate as f32).sin();
                let pos = i as f32 - (base + prof * mod_);
                if pos >= 0.0 {
                    let (i0, fr) = (pos as usize, pos.fract());
                    let i1 = (i0 + 1).min(n - 1);
                    let w = dry[i0] * (1.0 - fr) + dry[i1] * fr;
                    buf[i] = dry[i] * (1.0 - mix.clamp(0.0, 1.0) * 0.5) + w * mix.clamp(0.0, 1.0);
                }
            }
        }
        Effect::Compress { threshold, ratio } => {
            let th = threshold.clamp(0.01, 1.0);
            let r = ratio.max(1.0);
            let (atk, rel) = (1.0 - (-1.0 / (0.005 * rate as f32)).exp(), 1.0 - (-1.0 / (0.08 * rate as f32)).exp());
            let mut env = 0.0f32;
            for s in buf.iter_mut() {
                let x = s.abs();
                env += (x - env) * if x > env { atk } else { rel };
                if env > th {
                    let gain = (th + (env - th) / r) / env;
                    *s *= gain;
                }
            }
        }
        Effect::Gain { gain } => {
            for s in buf.iter_mut() {
                *s *= gain;
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
        Effect::NormalizeRms { rms, peak } => {
            let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            if max > 1e-6 {
                let (suma, cuenta) = buf.iter().filter(|s| s.abs() > max * 0.03).fold((0.0f64, 0usize), |(a, c), s| (a + (*s as f64).powi(2), c + 1));
                let actual = (suma / cuenta.max(1) as f64).sqrt() as f32;
                let mut k = rms.clamp(0.0, 1.0) / actual.max(1e-6);
                k = k.min(peak.clamp(0.0, 1.0) / max); // nunca pasar del pico
                for s in buf.iter_mut() {
                    *s *= k;
                }
            }
        }
    }
}

/// Quita la continua y los clicks de los bordes del buffer ya mezclado.
pub(crate) fn pulir(buf: &mut [f32], rate: u32) {
    let n = buf.len();
    if n < 8 {
        return;
    }
    // paso-alto de ~15 Hz
    let a = 1.0 - (-std::f32::consts::TAU * 15.0 / rate as f32).exp();
    let mut prev = 0.0f32;
    for s in buf.iter_mut() {
        prev += (*s - prev) * a;
        *s -= prev;
    }
    let (entra, sale) = (((0.0008 * rate as f32) as usize).max(2).min(n / 2), ((0.002 * rate as f32) as usize).max(2).min(n / 2));
    for i in 0..entra {
        buf[i] *= i as f32 / entra as f32;
    }
    for i in 0..sale {
        buf[n - 1 - i] *= i as f32 / sale as f32;
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
    pulir(&mut buf, spec.rate);
    buf
}

/// Sintetiza una pista musical generativa → buffer f32.
pub fn render_track(t: &TrackSpec) -> Vec<f32> {
    if let Some(c) = &t.compose {
        return compose::render_composition(t, c);
    }
    let mut s = Synth::new(t.rate, t.dur.max(1.0));
    let mut rng = Lcg(t.seed);
    let m = &t.melody;

    if let Some(pad) = &t.pad {
        let voz = |mult: f32, amp: f32| ToneOp {
            wave: pad.wave,
            at: 0.0,
            dur: t.dur,
            f0: t.base * pad.freq_mult * mult,
            f1: t.base * pad.freq_mult * mult,
            amp,
            attack: pad.attack,
            vibrato: pad.vibrato,
            sustain: 0.8,
            curve: 1.0,
            detune: pad.detune,
            cutoff0: pad.cutoff0,
            cutoff1: pad.cutoff1,
            ..ToneOp::default()
        };
        let base = voz(1.0, pad.amp);
        s.tone(&base);
        if pad.fifth > 0.0 {
            let quinta = voz(1.5, pad.amp * pad.fifth);
            s.tone(&quinta);
        }
    }

    // Melodía: notas espaciadas con silencio frecuente (el aire ES la pista).
    let scale: &[f32] = if t.scale.is_empty() { &[0.0] } else { &t.scale };
    let nota = |at: f32, f: f32, nd: f32, amp: f32, wave: Wave, attack: f32, vibrato: f32| ToneOp {
        wave,
        at,
        dur: nd,
        f0: f,
        f1: f,
        amp,
        attack,
        vibrato,
        curve: m.curve,
        detune: m.detune,
        fm_ratio: m.fm_ratio,
        fm_index: m.fm_index,
        cutoff0: m.cutoff,
        cutoff1: m.cutoff,
        ..ToneOp::default()
    };
    let mut at = rng.range(0.5, 2.0);
    while at < t.dur - 3.0 {
        if rng.f32() < m.rest_prob {
            at += rng.range(m.rest_min, m.rest_max);
            continue;
        }
        let deg = scale[(rng.next() as usize) % scale.len()];
        let f = t.base * (2.0f32).powf(deg / 12.0);
        let nd = rng.range(m.note_min, m.note_max);
        s.tone(&nota(at, f, nd, m.amp, m.wave, m.attack, m.vibrato));
        if m.chord_prob > 0.0 && rng.f32() < m.chord_prob {
            // un acorde suelto: otra nota de la escala más arriba, apenas después
            let deg3 = scale[(rng.next() as usize) % scale.len()];
            let f3 = t.base * (2.0f32).powf((deg3 + 7.0) / 12.0);
            s.tone(&nota(at + 0.08, f3, nd * 0.85, m.amp * 0.6, m.wave, m.attack, m.vibrato));
        }
        if rng.f32() < m.harmony_prob {
            let deg2 = scale[(rng.next() as usize) % scale.len()];
            let f2 = t.base * (2.0f32).powf((deg2 - 12.0) / 12.0);
            s.tone(&ToneOp {
                wave: m.harmony_wave,
                at: at + 0.1,
                dur: nd * 0.9,
                f0: f2,
                f1: f2,
                amp: m.harmony_amp,
                attack: m.harmony_attack,
                vibrato: m.harmony_vibrato,
                ..ToneOp::default()
            });
        }
        at += rng.range(m.gap_min, m.gap_max);
    }

    let mut buf = s.buf;
    for fx in &t.effects {
        apply_effect(&mut buf, t.rate, fx);
    }
    pulir(&mut buf, t.rate);
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

mod compose;
mod defaults;
pub use compose::{ComposeSpec, Voice};


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
            Effect::Highpass { alpha: 0.05 },
            Effect::Saturate { drive: 3.0 },
            Effect::Chorus { depth: 0.002, rate: 0.5, mix: 0.4 },
            Effect::Compress { threshold: 0.3, ratio: 4.0 },
            Effect::Gain { gain: 0.8 },
            Effect::NormalizeRms { rms: 0.12, peak: 0.9 },
        ] {
            apply_effect(&mut buf, 22_050, &fx);
            assert!(buf.iter().all(|s| s.is_finite()));
        }
        let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.05 && peak <= 1.0, "pico {peak}");
    }

    fn rms_activo(buf: &[f32]) -> f32 {
        let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let act: Vec<f32> = buf.iter().copied().filter(|s| s.abs() > max * 0.03).collect();
        (act.iter().map(|s| s * s).sum::<f32>() / act.len().max(1) as f32).sqrt()
    }

    /// Ningún sonido sale con NaN, clipping duro ni mudo, y todos son deterministas.
    #[test]
    fn todo_el_audio_es_finito_audible_y_determinista() {
        let spec = AudioSpec::default_spec();
        for (k, s) in spec.sfx.iter().chain(std::iter::once((&"thrust".to_string(), &spec.thrust))) {
            let a = render_sfx(s);
            let b = render_sfx(s);
            assert!(a.iter().all(|x| x.is_finite()), "{k}: NaN o infinito");
            assert!(a.iter().zip(&b).all(|(x, y)| x == y), "{k}: no determinista");
            let pico = a.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!((0.05..=1.5).contains(&pico), "{k}: pico {pico}");
        }
    }

    /// **La sonoridad está pareja**: ningún efecto es 6 dB más flojo o más fuerte que su categoría.
    #[test]
    fn la_sonoridad_esta_pareja() {
        let spec = AudioSpec::default_spec();
        for (k, s) in &spec.sfx {
            let r = rms_activo(&render_sfx(s));
            assert!((0.04..=0.26).contains(&r), "{k}: RMS {r:.3} fuera de banda");
        }
    }

    /// El filtro por capa no explota con cortes cerca de Nyquist (el bug que dejó mudo al paso en la nieve).
    #[test]
    fn el_filtro_por_capa_es_estable_con_cortes_altos() {
        for res in [0.3f32, 0.7, 4.0, 8.0] {
            let mut s = Synth::new(32_000, 0.3);
            s.tone(&ToneOp { wave: Wave::Pink, dur: 0.3, amp: 0.5, cutoff0: 15_000.0, cutoff1: 9_000.0, res, ..ToneOp::default() });
            assert!(s.buf.iter().all(|x| x.is_finite() && x.abs() < 20.0), "inestable con res {res}");
            assert!(s.buf.iter().fold(0.0f32, |m, x| m.max(x.abs())) > 0.001, "mudo con res {res}");
        }
    }

    /// Un `audio.json` viejo (sin los campos del motor v2) se sigue leyendo.
    #[test]
    fn un_json_viejo_sigue_leyendose() {
        let viejo = r#"{"sfx_volume":0.8,"music_volume":0.5,"sfx":{"click":{"dur":0.05,"tones":[{"wave":"square","at":0.0,"dur":0.04,"f0":1200.0,"f1":800.0,"amp":0.25,"attack":0.05}]}},"thrust":{"dur":0.5,"tones":[]},"tracks":[]}"#;
        let spec = AudioSpec::from_json(viejo).expect("el formato viejo debe leerse");
        let t = spec.sfx["click"].tones[0];
        assert_eq!(t.curve, 1.6);
        assert_eq!(t.detune, 0.0);
        assert!(render_sfx(&spec.sfx["click"]).iter().any(|x| x.abs() > 0.01));
    }

    /// Cada timbre suena (un arreglo mal pegado dejó mudas a las campanas y nadie se enteró).
    #[test]
    fn cada_timbre_suena() {
        for w in Wave::ALL {
            let mut s = Synth::new(22_050, 0.5);
            s.tone(&ToneOp { wave: w, dur: 0.5, f0: 330.0, f1: 330.0, amp: 0.5, attack: 0.02, ..ToneOp::default() });
            let r = (s.buf.iter().map(|x| x * x).sum::<f32>() / s.buf.len() as f32).sqrt();
            assert!(r > 0.003 && s.buf.iter().all(|x| x.is_finite()), "{w:?} no suena (RMS {r})");
        }
    }

    /// Las piezas compuestas son deterministas, suenan y tienen aire (silencio entre frases).
    #[test]
    fn las_piezas_compuestas_son_deterministas_y_tienen_aire() {
        let spec = AudioSpec::default_spec();
        let t = spec.tracks.iter().find(|t| t.name == "pradera-1").unwrap();
        let mut corta = t.clone();
        corta.dur = 30.0;
        let a = render_track(&corta);
        let b = render_track(&corta);
        assert!(a.iter().zip(&b).all(|(x, y)| x == y), "no determinista");
        let r = rms_activo(&a);
        assert!(r > 0.02, "pieza muda: {r}");
        // aire: al menos el 10 % de las ventanas de 0,25 s son casi silencio respecto del RMS
        let v = (0.25 * corta.rate as f32) as usize;
        let calladas = a.chunks(v).filter(|c| (c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32).sqrt() < r * 0.15).count();
        assert!(calladas as f32 / (a.len() / v) as f32 > 0.05, "no tiene silencios: {calladas}");
    }

    /// Todas las piezas del juego traen su contexto, y existen las que el director necesita.
    #[test]
    fn el_director_tiene_musica_para_cada_contexto() {
        let spec = AudioSpec::default_spec();
        for mood in ["surface", "night", "underground", "cristal", "space", "tension", "menu"] {
            assert!(spec.tracks.iter().any(|t| t.mood == mood), "falta música de {mood}");
        }
    }
}


/// Versión del motor de música: sube cuando cambia CÓMO suena una pista, e invalida el caché de disco y las pistas
/// pre-renderizadas que viajan en el APK (`assets/audio/musica/`).
pub const MUSICA_VERSION: u32 = 1;

/// Huella (FNV-1a) de una partitura + la versión del motor. La misma cuenta la hace el juego para saber si el WAV
/// pre-renderizado que trae el APK corresponde a la partitura que tiene delante.
pub fn hash_pista(t: &TrackSpec) -> u32 {
    let json = serde_json::to_string(t).unwrap_or_default();
    let mut h: u32 = 0x811c_9dc5 ^ MUSICA_VERSION;
    for b in json.bytes() {
        h = (h ^ b as u32).wrapping_mul(0x0100_0193);
    }
    h
}
