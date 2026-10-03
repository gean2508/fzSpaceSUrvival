//! **El diseño de sonido del juego** (2 oct 2026, motor v2): cada efecto es un golpe de varias capas —un
//! transitorio, un cuerpo y una cola— en vez de un pitido de onda cuadrada.
//!
//! Reglas que sigue todo lo de acá (y que fija la skill `audio-sfx`):
//! 1. **Nunca un solo oscilador crudo.** Un efecto = transitorio (ruido filtrado o cuerda pulsada, 5-40 ms) +
//!    cuerpo (seno/FM con caída en glissando) + cola opcional (reverb corto).
//! 2. **El ruido tiene color**: rosa para aire, hojas, tela; marrón para golpes y tierra; blanco sólo en el
//!    transitorio. Siempre pasado por un filtro con barrido (`cutoff0 → cutoff1`).
//! 3. **Material = timbre**: madera = `Pluck` grave con caída seca; metal/cristal = `Bell` o FM; piedra = golpe
//!    marrón + chasquido corto; tela = ruido rosa con ataque lento.
//! 4. **Sonoridad pareja**: todo termina en `NormalizeRms` por categoría (UI baja, mundo media, impactos alta),
//!    así un click no se pierde ni una explosión revienta.
//! 5. **Sin clicks**: el motor ya entra/sale suave; los ataques nunca son 0.

use crate::*;
use std::collections::BTreeMap;

fn op(wave: Wave, at: f32, dur: f32, f0: f32, f1: f32, amp: f32) -> ToneOp {
    ToneOp { wave, at, dur, f0, f1, amp, ..ToneOp::default() }
}

fn sfx(dur: f32, tones: Vec<ToneOp>, effects: Vec<Effect>) -> SfxSpec {
    SfxSpec { dur, rate: SFX_RATE, tones, effects }
}

/// Niveles (RMS objetivo) por categoría: UI suave, mundo medio, impacto fuerte.
const RMS_UI: f32 = 0.10;
const RMS_MUNDO: f32 = 0.14;
const RMS_IMPACTO: f32 = 0.19;

fn nivel(rms: f32) -> Effect {
    Effect::NormalizeRms { rms, peak: 0.9 }
}

impl AudioSpec {
    /// El diseño de sonido completo.
    pub fn default_spec() -> Self {
        use Wave::*;
        let mut m: BTreeMap<String, SfxSpec> = BTreeMap::new();
        let mut put = |k: &str, v: SfxSpec| {
            m.insert(k.to_string(), v);
        };

        // ── interfaz ────────────────────────────────────────────────────────────────────────────
        put("click", sfx(0.09, vec![
            ToneOp { curve: 3.0, attack: 0.02, ..op(Pluck, 0.0, 0.08, 1500.0, 1500.0, 0.5) },
            ToneOp { curve: 4.0, ..op(Sine, 0.0, 0.025, 2600.0, 1700.0, 0.16) },
        ], vec![Effect::Highpass { alpha: 0.05 }, nivel(RMS_UI)]));
        put("bag_open", sfx(0.26, vec![
            ToneOp { attack: 0.35, cutoff0: 700.0, cutoff1: 3200.0, curve: 1.4, ..op(Pink, 0.0, 0.2, 0.0, 0.0, 0.5) },
            ToneOp { curve: 3.0, ..op(Pluck, 0.17, 0.08, 900.0, 900.0, 0.22) },
        ], vec![nivel(RMS_UI)]));
        put("bag_close", sfx(0.26, vec![
            ToneOp { attack: 0.2, cutoff0: 3200.0, cutoff1: 700.0, curve: 1.6, ..op(Pink, 0.0, 0.2, 0.0, 0.0, 0.5) },
            ToneOp { curve: 3.0, ..op(Pluck, 0.18, 0.07, 620.0, 620.0, 0.25) },
        ], vec![nivel(RMS_UI)]));
        put("pickup", sfx(0.42, vec![
            ToneOp { curve: 2.2, attack: 0.01, ..op(Bell, 0.0, 0.3, 1175.0, 1175.0, 0.35) },
            ToneOp { curve: 2.2, attack: 0.01, ..op(Bell, 0.07, 0.32, 1568.0, 1568.0, 0.3) },
            ToneOp { curve: 4.0, ..op(Pink, 0.0, 0.03, 0.0, 0.0, 0.12) },
        ], vec![Effect::Reverb { size: 0.25, mix: 0.18 }, nivel(RMS_UI)]));
        put("craft", sfx(0.8, {
            let mut v = vec![ToneOp { curve: 3.5, ..op(Pluck, 0.0, 0.1, 320.0, 320.0, 0.4) }];
            for (i, f) in [523.25f32, 659.25, 783.99, 1046.5].iter().enumerate() {
                v.push(ToneOp { curve: 2.0, attack: 0.01, ..op(Bell, 0.1 + i as f32 * 0.085, 0.45, *f, *f, 0.3) });
            }
            v
        }, vec![Effect::Reverb { size: 0.4, mix: 0.25 }, nivel(RMS_UI)]));
        put("stamp", sfx(0.5, {
            let mut v = Vec::new();
            for (i, f) in [330.0f32, 262.0, 196.0].iter().enumerate() {
                let at = i as f32 * 0.11;
                v.push(ToneOp { curve: 3.0, ..op(Sine, at, 0.12, *f * 0.5, *f * 0.3, 0.5) });
                v.push(ToneOp { curve: 3.5, ..op(Pluck, at, 0.1, *f, *f, 0.35) });
                v.push(ToneOp { curve: 4.0, cutoff0: 3500.0, cutoff1: 1500.0, ..op(Pink, at, 0.04, 0.0, 0.0, 0.25) });
            }
            v
        }, vec![nivel(RMS_MUNDO)]));

        // ── trabajo en el mundo ─────────────────────────────────────────────────────────────────
        put("dig", sfx(0.24, vec![
            ToneOp { attack: 0.03, curve: 2.4, cutoff0: 1100.0, cutoff1: 280.0, res: 1.2, ..op(Brown, 0.0, 0.2, 0.0, 0.0, 0.9) },
            ToneOp { curve: 3.0, ..op(Sine, 0.0, 0.14, 125.0, 50.0, 0.55) },
            ToneOp { attack: 0.05, curve: 3.5, cutoff0: 4200.0, cutoff1: 1800.0, ..op(Pink, 0.0, 0.06, 0.0, 0.0, 0.35) },
        ], vec![Effect::Saturate { drive: 1.6 }, nivel(RMS_IMPACTO)]));
        put("chop", sfx(0.34, vec![
            ToneOp { curve: 2.6, attack: 0.01, ..op(Pluck, 0.0, 0.3, 175.0, 175.0, 0.8) },
            ToneOp { curve: 3.0, ..op(Sine, 0.0, 0.12, 160.0, 65.0, 0.45) },
            ToneOp { curve: 4.0, attack: 0.04, cutoff0: 5000.0, cutoff1: 1500.0, ..op(Pink, 0.0, 0.05, 0.0, 0.0, 0.5) },
        ], vec![Effect::Reverb { size: 0.2, mix: 0.12 }, nivel(RMS_IMPACTO)]));
        put("place", sfx(0.2, vec![
            ToneOp { curve: 3.0, ..op(Sine, 0.0, 0.14, 170.0, 85.0, 0.6) },
            ToneOp { curve: 3.5, ..op(Pluck, 0.0, 0.12, 480.0, 480.0, 0.3) },
            ToneOp { curve: 4.0, cutoff0: 2500.0, cutoff1: 900.0, ..op(Pink, 0.0, 0.04, 0.0, 0.0, 0.25) },
        ], vec![nivel(RMS_MUNDO)]));
        put("hurt", sfx(0.34, vec![
            ToneOp { detune: 18.0, cutoff0: 2600.0, cutoff1: 380.0, res: 2.0, curve: 1.8, attack: 0.02, ..op(Saw, 0.0, 0.26, 340.0, 105.0, 0.55) },
            ToneOp { curve: 4.0, ..op(Noise, 0.0, 0.05, 0.0, 0.0, 0.4) },
            ToneOp { curve: 3.0, ..op(Sine, 0.0, 0.2, 100.0, 48.0, 0.5) },
        ], vec![Effect::Saturate { drive: 2.2 }, nivel(RMS_IMPACTO)]));
        put("jump", sfx(0.2, vec![
            ToneOp { attack: 0.3, curve: 1.6, ..op(Sine, 0.0, 0.14, 260.0, 640.0, 0.3) },
            ToneOp { attack: 0.4, cutoff0: 700.0, cutoff1: 2800.0, curve: 1.8, ..op(Pink, 0.0, 0.14, 0.0, 0.0, 0.3) },
        ], vec![nivel(RMS_UI)]));
        put("touchdown", sfx(0.3, vec![
            ToneOp { attack: 0.02, curve: 2.4, cutoff0: 700.0, cutoff1: 200.0, ..op(Brown, 0.0, 0.2, 0.0, 0.0, 1.0) },
            ToneOp { curve: 3.0, ..op(Sine, 0.0, 0.18, 85.0, 42.0, 0.65) },
            ToneOp { attack: 0.1, curve: 3.0, cutoff0: 3000.0, cutoff1: 900.0, ..op(Pink, 0.01, 0.07, 0.0, 0.0, 0.3) },
        ], vec![nivel(RMS_IMPACTO)]));
        put("climb_ledge", sfx(0.34, vec![
            ToneOp { attack: 0.4, cutoff0: 600.0, cutoff1: 2000.0, curve: 1.5, ..op(Pink, 0.0, 0.2, 0.0, 0.0, 0.5) },
            ToneOp { attack: 0.3, curve: 2.0, ..op(Sine, 0.05, 0.14, 140.0, 220.0, 0.22) },
            ToneOp { curve: 3.5, cutoff0: 2200.0, cutoff1: 800.0, ..op(Brown, 0.2, 0.08, 0.0, 0.0, 0.5) },
        ], vec![nivel(RMS_MUNDO)]));
        put("drop_ledge", sfx(0.3, vec![
            ToneOp { curve: 3.0, ..op(Sine, 0.0, 0.16, 135.0, 52.0, 0.6) },
            ToneOp { attack: 0.03, curve: 3.5, cutoff0: 1400.0, cutoff1: 350.0, ..op(Brown, 0.0, 0.1, 0.0, 0.0, 0.7) },
            ToneOp { curve: 4.0, cutoff0: 4000.0, cutoff1: 1500.0, ..op(Pink, 0.0, 0.04, 0.0, 0.0, 0.3) },
        ], vec![nivel(RMS_IMPACTO)]));

        // ── pasos: cada superficie suena a su material ─────────────────────────────────────────
        for (k, f, c0) in [("step_soft1", 95.0f32, 1500.0f32), ("step_soft2", 112.0, 1300.0)] {
            put(k, sfx(0.14, vec![
                ToneOp { attack: 0.18, curve: 2.2, cutoff0: c0, cutoff1: c0 * 0.4, ..op(Pink, 0.0, 0.1, 0.0, 0.0, 0.5) },
                ToneOp { curve: 3.0, ..op(Sine, 0.0, 0.07, f, f * 0.7, 0.28) },
            ], vec![nivel(0.075)]));
        }
        for (k, f) in [("step_hard1", 780.0f32), ("step_hard2", 910.0)] {
            put(k, sfx(0.1, vec![
                ToneOp { curve: 4.0, attack: 0.02, ..op(Pluck, 0.0, 0.07, f, f, 0.4) },
                ToneOp { curve: 4.0, cutoff0: 5200.0, cutoff1: 1800.0, ..op(Pink, 0.0, 0.03, 0.0, 0.0, 0.45) },
                ToneOp { curve: 3.0, ..op(Sine, 0.0, 0.05, 150.0, 90.0, 0.25) },
            ], vec![nivel(0.085)]));
        }
        for (k, f) in [("step_sand1", 2200.0f32), ("step_sand2", 1900.0)] {
            put(k, sfx(0.16, vec![
                ToneOp { attack: 0.3, curve: 2.0, cutoff0: f, cutoff1: f * 0.35, ..op(Pink, 0.0, 0.13, 0.0, 0.0, 0.5) },
                ToneOp { curve: 3.0, ..op(Brown, 0.0, 0.05, 0.0, 0.0, 0.25) },
            ], vec![nivel(0.07)]));
        }
        for (k, f) in [("step_wood1", 190.0f32), ("step_wood2", 215.0)] {
            put(k, sfx(0.16, vec![
                ToneOp { curve: 3.2, attack: 0.02, ..op(Pluck, 0.0, 0.14, f, f, 0.55) },
                ToneOp { curve: 4.0, cutoff0: 3500.0, cutoff1: 1200.0, ..op(Pink, 0.0, 0.03, 0.0, 0.0, 0.3) },
            ], vec![nivel(0.09)]));
        }
        put("step_snow", sfx(0.2, vec![
            ToneOp { attack: 0.1, curve: 3.0, cutoff0: 7000.0, cutoff1: 2500.0, ..op(Pink, 0.0, 0.06, 0.0, 0.0, 0.5) },
            ToneOp { attack: 0.1, curve: 3.0, cutoff0: 6000.0, cutoff1: 2000.0, ..op(Pink, 0.07, 0.07, 0.0, 0.0, 0.4) },
            ToneOp { curve: 3.0, ..op(Brown, 0.0, 0.08, 0.0, 0.0, 0.2) },
        ], vec![nivel(0.07)]));

        // ── interiores y vida diaria ───────────────────────────────────────────────────────────
        put("door_open", sfx(0.5, vec![
            ToneOp { detune: 25.0, vibrato: 3.0, attack: 0.3, cutoff0: 500.0, cutoff1: 1100.0, res: 4.5, curve: 1.3, ..op(Saw, 0.0, 0.4, 140.0, 215.0, 0.3) },
            ToneOp { curve: 3.5, ..op(Pluck, 0.0, 0.06, 760.0, 760.0, 0.35) },
            ToneOp { attack: 0.3, curve: 2.0, cutoff0: 600.0, cutoff1: 1500.0, ..op(Pink, 0.05, 0.3, 0.0, 0.0, 0.18) },
        ], vec![nivel(RMS_MUNDO)]));
        put("door_close", sfx(0.34, vec![
            ToneOp { detune: 20.0, attack: 0.2, cutoff0: 900.0, cutoff1: 400.0, res: 3.0, curve: 1.6, ..op(Saw, 0.0, 0.14, 215.0, 150.0, 0.2) },
            ToneOp { curve: 3.0, ..op(Sine, 0.12, 0.14, 105.0, 52.0, 0.6) },
            ToneOp { curve: 4.0, ..op(Pluck, 0.13, 0.08, 620.0, 620.0, 0.3) },
            ToneOp { curve: 4.0, cutoff0: 2500.0, cutoff1: 700.0, ..op(Brown, 0.12, 0.08, 0.0, 0.0, 0.5) },
        ], vec![nivel(RMS_MUNDO)]));
        put("stairs", sfx(0.4, {
            let mut v = Vec::new();
            for (i, f) in [190.0f32, 215.0, 245.0].iter().enumerate() {
                let at = i as f32 * 0.115;
                v.push(ToneOp { curve: 3.5, ..op(Pluck, at, 0.09, *f, *f, 0.45) });
                v.push(ToneOp { curve: 4.0, cutoff0: 3500.0, cutoff1: 1200.0, ..op(Pink, at, 0.03, 0.0, 0.0, 0.3) });
            }
            v
        }, vec![nivel(0.1)]));
        put("eat", sfx(0.6, {
            let mut v = Vec::new();
            for i in 0..3 {
                let at = i as f32 * 0.17;
                v.push(ToneOp { attack: 0.1, curve: 3.0, cutoff0: 3600.0 - i as f32 * 300.0, cutoff1: 1100.0, res: 1.4, ..op(Pink, at, 0.09, 0.0, 0.0, 0.55) });
                v.push(ToneOp { curve: 3.0, ..op(Sine, at, 0.07, 150.0 - i as f32 * 12.0, 95.0, 0.3) });
            }
            v
        }, vec![nivel(RMS_MUNDO)]));
        put("drink", sfx(0.6, {
            let mut v = Vec::new();
            for i in 0..3 {
                let at = i as f32 * 0.17;
                let f = 280.0 + i as f32 * 55.0;
                v.push(ToneOp { fm_ratio: 1.5, fm_index: 2.5, attack: 0.25, curve: 1.8, ..op(Sine, at, 0.13, f, f * 1.5, 0.35) });
                v.push(ToneOp { attack: 0.3, cutoff0: 900.0, cutoff1: 400.0, curve: 2.0, ..op(Brown, at, 0.12, 0.0, 0.0, 0.3) });
            }
            v
        }, vec![Effect::Reverb { size: 0.15, mix: 0.08 }, nivel(RMS_MUNDO)]));
        put("launch", sfx(1.2, vec![
            ToneOp { attack: 0.55, curve: 1.3, cutoff0: 200.0, cutoff1: 2600.0, res: 1.5, ..op(Pink, 0.0, 1.1, 0.0, 0.0, 0.7) },
            ToneOp { detune: 22.0, attack: 0.5, curve: 1.4, cutoff0: 220.0, cutoff1: 900.0, ..op(Saw, 0.0, 1.0, 55.0, 110.0, 0.4) },
            ToneOp { attack: 0.4, curve: 1.6, ..op(Sine, 0.0, 0.9, 70.0, 38.0, 0.5) },
        ], vec![Effect::Saturate { drive: 1.8 }, Effect::Reverb { size: 0.5, mix: 0.2 }, nivel(RMS_IMPACTO)]));

        // ── nuevos (2 oct 2026) ─────────────────────────────────────────────────────────────────
        // El subsuelo: gotas, cristales, derrumbes y golpes de pico contra mena.
        put("drip", sfx(1.1, vec![
            ToneOp { curve: 3.0, attack: 0.03, ..op(Sine, 0.0, 0.1, 1900.0, 820.0, 0.5) },
            ToneOp { curve: 3.5, ..op(Bell, 0.0, 0.25, 2400.0, 2400.0, 0.1) },
        ], vec![Effect::Reverb { size: 0.85, mix: 0.55 }, nivel(0.06)]));
        put("crystal_chime", sfx(1.6, vec![
            ToneOp { curve: 2.0, attack: 0.01, ..op(Bell, 0.0, 1.2, 2093.0, 2093.0, 0.3) },
            ToneOp { curve: 2.2, attack: 0.01, ..op(Bell, 0.12, 1.2, 2637.0, 2637.0, 0.22) },
            ToneOp { curve: 2.4, attack: 0.01, ..op(Bell, 0.31, 1.0, 3136.0, 3136.0, 0.15) },
        ], vec![Effect::Reverb { size: 0.9, mix: 0.5 }, nivel(0.06)]));
        put("crumble", sfx(0.7, {
            let mut v = vec![ToneOp { attack: 0.05, curve: 2.0, cutoff0: 1300.0, cutoff1: 260.0, res: 1.3, ..op(Brown, 0.0, 0.5, 0.0, 0.0, 0.8) }];
            for (i, f) in [118.0f32, 96.0, 140.0, 84.0].iter().enumerate() {
                v.push(ToneOp { curve: 3.5, ..op(Pluck, 0.06 + i as f32 * 0.09, 0.14, *f, *f, 0.35) });
            }
            v
        }, vec![Effect::Reverb { size: 0.5, mix: 0.2 }, nivel(RMS_MUNDO)]));
        put("ore_ping", sfx(0.5, vec![
            ToneOp { curve: 3.0, attack: 0.01, ..op(Pluck, 0.0, 0.12, 2300.0, 2300.0, 0.4) },
            ToneOp { curve: 2.5, attack: 0.01, ..op(Bell, 0.0, 0.4, 3300.0, 3300.0, 0.25) },
            ToneOp { curve: 4.0, cutoff0: 5000.0, cutoff1: 2000.0, ..op(Pink, 0.0, 0.025, 0.0, 0.0, 0.3) },
            ToneOp { curve: 3.0, ..op(Sine, 0.0, 0.1, 140.0, 80.0, 0.3) },
        ], vec![Effect::Reverb { size: 0.4, mix: 0.2 }, nivel(RMS_MUNDO)]));
        put("discover", sfx(1.3, {
            let mut v = Vec::new();
            for (i, f) in [784.0f32, 988.0, 1319.0, 1568.0].iter().enumerate() {
                v.push(ToneOp { curve: 2.0, attack: 0.01, ..op(Bell, i as f32 * 0.1, 0.9, *f, *f, 0.3) });
            }
            v.push(ToneOp { attack: 0.4, sustain: 0.3, curve: 1.5, ..op(Sine, 0.0, 0.9, 392.0, 392.0, 0.16) });
            v
        }, vec![Effect::Reverb { size: 0.7, mix: 0.4 }, nivel(RMS_UI)]));
        put("fizzle", sfx(0.5, vec![
            ToneOp { attack: 0.12, curve: 1.8, cutoff0: 3200.0, cutoff1: 380.0, res: 1.2, ..op(Pink, 0.0, 0.4, 0.0, 0.0, 0.6) },
            ToneOp { attack: 0.1, curve: 2.0, ..op(Sine, 0.0, 0.3, 320.0, 110.0, 0.2) },
        ], vec![nivel(RMS_UI)]));
        put("goal", sfx(1.2, {
            let mut v = Vec::new();
            for (i, f) in [523.25f32, 659.25, 783.99, 1046.5].iter().enumerate() {
                v.push(ToneOp { curve: 2.4, attack: 0.02, ..op(Pluck, i as f32 * 0.1, 0.45, *f, *f, 0.5) });
                v.push(ToneOp { curve: 2.2, attack: 0.01, ..op(Bell, i as f32 * 0.1, 0.8, *f * 2.0, *f * 2.0, 0.14) });
            }
            v.push(ToneOp { attack: 0.3, sustain: 0.4, curve: 1.4, ..op(Triangle, 0.38, 0.8, 523.25, 523.25, 0.18) });
            v
        }, vec![Effect::Reverb { size: 0.55, mix: 0.3 }, nivel(RMS_UI)]));
        put("deny", sfx(0.3, vec![
            ToneOp { pwm: 0.25, cutoff0: 1500.0, cutoff1: 1100.0, attack: 0.05, curve: 2.0, ..op(Square, 0.0, 0.1, 230.0, 215.0, 0.3) },
            ToneOp { pwm: 0.25, cutoff0: 1500.0, cutoff1: 1100.0, attack: 0.05, curve: 2.0, ..op(Square, 0.13, 0.13, 196.0, 170.0, 0.3) },
        ], vec![nivel(RMS_UI)]));
        put("splash", sfx(0.6, vec![
            ToneOp { attack: 0.12, curve: 1.8, cutoff0: 3800.0, cutoff1: 800.0, ..op(Pink, 0.0, 0.45, 0.0, 0.0, 0.7) },
            ToneOp { fm_ratio: 1.4, fm_index: 2.0, attack: 0.2, curve: 2.2, ..op(Sine, 0.04, 0.12, 520.0, 980.0, 0.22) },
            ToneOp { fm_ratio: 1.4, fm_index: 2.0, attack: 0.2, curve: 2.2, ..op(Sine, 0.15, 0.1, 460.0, 900.0, 0.18) },
        ], vec![Effect::Reverb { size: 0.3, mix: 0.15 }, nivel(RMS_MUNDO)]));
        put("groan", sfx(1.0, vec![
            ToneOp { detune: 30.0, vibrato: 3.0, attack: 0.25, curve: 1.4, cutoff0: 520.0, cutoff1: 280.0, res: 3.5, ..op(Saw, 0.0, 0.85, 98.0, 66.0, 0.6) },
            ToneOp { attack: 0.3, curve: 1.6, cutoff0: 600.0, cutoff1: 300.0, ..op(Brown, 0.0, 0.8, 0.0, 0.0, 0.5) },
            ToneOp { attack: 0.4, curve: 1.5, ..op(Sine, 0.0, 0.8, 49.0, 40.0, 0.3) },
        ], vec![Effect::Saturate { drive: 1.8 }, Effect::Reverb { size: 0.45, mix: 0.2 }, nivel(RMS_MUNDO)]));
        put("bite", sfx(0.3, vec![
            ToneOp { attack: 0.05, curve: 3.0, cutoff0: 3200.0, cutoff1: 900.0, res: 1.4, ..op(Pink, 0.0, 0.1, 0.0, 0.0, 0.6) },
            ToneOp { curve: 3.0, ..op(Sine, 0.0, 0.1, 210.0, 90.0, 0.4) },
            ToneOp { attack: 0.05, curve: 3.0, cutoff0: 2600.0, cutoff1: 700.0, ..op(Pink, 0.1, 0.1, 0.0, 0.0, 0.5) },
            ToneOp { curve: 3.0, ..op(Sine, 0.1, 0.1, 190.0, 80.0, 0.35) },
        ], vec![nivel(RMS_IMPACTO)]));

        // ── la marca de FZ STUDIOS (la splash) ──────────────────────────────────────────────────────
        // Ocho «plops» de pintura que suben por la pentatónica, uno por mancha (cada `0,135 s` desde `0,06 s`, como
        // pide `boot_ui::RETARDO_AUDIO`), y un acorde con cola larga cuando cae el logo. NO es un efecto del juego:
        // se renderiza a `assets/audio/splash.wav` (ver `scripts/regenerar-audio.sh`).
        put("splash_logo", sfx(2.6, {
            let mut v = Vec::new();
            for (i, semis) in [0.0f32, 2.0, 4.0, 7.0, 9.0, 12.0, 14.0, 16.0].iter().enumerate() {
                let at = 0.06 + i as f32 * 0.135;
                let f = 349.0 * 2.0f32.powf(semis / 12.0);
                v.push(ToneOp { fm_ratio: 1.5, fm_index: 1.6, attack: 0.08, curve: 3.0, ..op(Sine, at, 0.16, f * 0.62, f * 1.5, 0.45) });
                v.push(ToneOp { attack: 0.05, curve: 4.0, cutoff0: 4200.0, cutoff1: 1000.0, ..op(Pink, at, 0.04, 0.0, 0.0, 0.35) });
                v.push(ToneOp { curve: 3.0, ..op(Sine, at, 0.1, 140.0, 62.0, 0.3) });
            }
            // el acorde de la marca: do-mi-sol-re con campanas y un colchón de sierras desafinadas
            for (j, f) in [261.63f32, 329.63, 392.0, 587.33].iter().enumerate() {
                v.push(ToneOp { detune: 16.0, attack: 0.06, sustain: 0.25, curve: 1.6, cutoff0: 500.0, cutoff1: 1700.0, ..op(Saw, 1.22 + j as f32 * 0.012, 1.3, *f, *f, 0.1) });
                v.push(ToneOp { curve: 2.0, attack: 0.01, ..op(Bell, 1.22 + j as f32 * 0.03, 1.35, *f * 2.0, *f * 2.0, 0.22) });
            }
            v.push(ToneOp { curve: 2.0, attack: 0.01, ..op(Bell, 1.22, 1.35, 1046.5, 1046.5, 0.2) });
            v
        }, vec![Effect::Reverb { size: 0.7, mix: 0.32 }, Effect::Compress { threshold: 0.4, ratio: 3.0 }, Effect::NormalizeRms { rms: 0.15, peak: 0.9 }, Effect::FadeOut { secs: 0.35 }]));

        // ── el empuje (loop) ────────────────────────────────────────────────────────────────────
        let thrust = SfxSpec {
            dur: 1.2,
            rate: SFX_RATE,
            tones: vec![
                ToneOp { attack: 0.02, curve: 1.0, sustain: 0.95, cutoff0: 500.0, cutoff1: 500.0, res: 1.1, ..op(Brown, 0.0, 1.2, 0.0, 0.0, 0.8) },
                ToneOp { attack: 0.02, curve: 1.0, sustain: 0.95, cutoff0: 2200.0, cutoff1: 2200.0, ..op(Pink, 0.0, 1.2, 0.0, 0.0, 0.35) },
                ToneOp { detune: 22.0, vibrato: 2.0, attack: 0.02, curve: 1.0, sustain: 0.95, cutoff0: 300.0, cutoff1: 300.0, ..op(Saw, 0.0, 1.2, 55.0, 55.0, 0.25) },
            ],
            effects: vec![Effect::LoopCrossfade { secs: 0.05 }, nivel(0.12)],
        };

        // ── música: cada pista con su timbre y su aire ───────────────────────────────────────────
        let pradera = |name: &str, seed: u32, wave: Wave, fm: f32| TrackSpec {
            name: name.to_string(),
            mood: "surface".to_string(),
            seed,
            dur: 56.0,
            base: 220.0,
            scale: vec![0.0, 2.0, 4.0, 7.0, 9.0, 12.0, 14.0],
            rate: MUSIC_RATE,
            pad: Some(PadSpec { wave: Saw, freq_mult: 0.5, amp: 0.035, attack: 0.35, vibrato: 0.25, detune: 14.0, cutoff0: 380.0, cutoff1: 900.0, fifth: 0.6 }),
            melody: MelodySpec {
                wave,
                amp: 0.2,
                attack: 0.01,
                vibrato: 0.15,
                note_min: 2.0,
                note_max: 3.4,
                gap_min: 1.1,
                gap_max: 2.6,
                rest_prob: 0.3,
                curve: 2.2,
                fm_ratio: if fm > 0.0 { 2.0 } else { 0.0 },
                fm_index: fm,
                chord_prob: 0.25,
                harmony_prob: 0.25,
                harmony_wave: Sine,
                harmony_amp: 0.07,
                ..MelodySpec::default()
            },
            effects: vec![Effect::Chorus { depth: 0.002, rate: 0.35, mix: 0.25 }, Effect::Reverb { size: 0.75, mix: 0.4 }, Effect::Compress { threshold: 0.35, ratio: 3.0 }, Effect::NormalizeRms { rms: 0.09, peak: 0.85 }, Effect::FadeOut { secs: 4.0 }],
        };
        let tracks = vec![
            pradera("pradera-1", 0xAA01, Pluck, 0.0),
            pradera("pradera-2", 0xAA02, Bell, 0.0),
            pradera("pradera-3", 0xAA03, Sine, 3.0),
            TrackSpec {
                name: "vacio-estelar".to_string(),
                mood: "space".to_string(),
                seed: 0xBB01,
                dur: 64.0,
                base: 138.6,
                scale: vec![0.0, 3.0, 5.0, 7.0, 10.0, 12.0],
                rate: MUSIC_RATE,
                pad: Some(PadSpec { wave: Saw, freq_mult: 0.5, amp: 0.04, attack: 0.5, vibrato: 0.2, detune: 18.0, cutoff0: 260.0, cutoff1: 1200.0, fifth: 0.8 }),
                melody: MelodySpec { wave: Bell, amp: 0.18, attack: 0.01, vibrato: 0.0, note_min: 3.0, note_max: 5.0, gap_min: 2.5, gap_max: 5.0, rest_prob: 0.45, curve: 2.0, chord_prob: 0.4, harmony_prob: 0.0, ..MelodySpec::default() },
                effects: vec![Effect::Chorus { depth: 0.003, rate: 0.2, mix: 0.3 }, Effect::Reverb { size: 0.95, mix: 0.55 }, Effect::NormalizeRms { rms: 0.085, peak: 0.85 }, Effect::FadeOut { secs: 5.0 }],
            },
            TrackSpec {
                name: "subsuelo".to_string(),
                mood: "underground".to_string(),
                seed: 0xCC01,
                dur: 60.0,
                base: 82.4,
                scale: vec![0.0, 3.0, 5.0, 8.0, 10.0],
                rate: MUSIC_RATE,
                pad: Some(PadSpec { wave: Saw, freq_mult: 1.0, amp: 0.045, attack: 0.5, vibrato: 0.3, detune: 24.0, cutoff0: 160.0, cutoff1: 520.0, fifth: 0.5 }),
                melody: MelodySpec { wave: Pluck, amp: 0.24, attack: 0.01, note_min: 2.5, note_max: 4.0, gap_min: 3.0, gap_max: 6.0, rest_prob: 0.5, curve: 2.4, harmony_prob: 0.0, chord_prob: 0.0, ..MelodySpec::default() },
                effects: vec![Effect::Reverb { size: 0.95, mix: 0.6 }, Effect::Compress { threshold: 0.3, ratio: 3.0 }, Effect::NormalizeRms { rms: 0.08, peak: 0.85 }, Effect::FadeOut { secs: 4.0 }],
            },
        ];

        AudioSpec {
            sfx_volume: 0.8,
            music_volume: 0.5,
            director: DirectorSpec::default(),
            sfx: m,
            thrust,
            tracks,
        }
    }
}
