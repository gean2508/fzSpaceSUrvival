//! Renderiza TODO el audio del spec a WAVs (para auditarlo con `scripts/sfx-lab.py` o escucharlo):
//!   cargo run -p ha-audio --bin render_all -- <carpeta_salida> [audio.json]
//! Sin `audio.json` usa el diseño por defecto (`AudioSpec::default_spec`).

use ha_audio::{render_sfx, render_track, wav_bytes, AudioSpec};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("uso: render_all <carpeta_salida> [audio.json]");
    let spec = match args.next() {
        Some(p) => AudioSpec::from_json(&std::fs::read_to_string(p).expect("leer audio.json")).expect("audio.json válido"),
        None => AudioSpec::default_spec(),
    };
    std::fs::create_dir_all(&out).unwrap();
    let t0 = std::time::Instant::now();
    for (k, s) in &spec.sfx {
        let buf = render_sfx(s);
        std::fs::write(format!("{out}/sfx_{k}.wav"), wav_bytes(s.rate, &buf)).unwrap();
    }
    let thrust = render_sfx(&spec.thrust);
    std::fs::write(format!("{out}/sfx__thrust.wav"), wav_bytes(spec.thrust.rate, &thrust)).unwrap();
    let t_sfx = t0.elapsed();
    for t in &spec.tracks {
        let buf = render_track(t);
        std::fs::write(format!("{out}/mus_{}.wav", t.name), wav_bytes(t.rate, &buf)).unwrap();
    }
    eprintln!("render_all: {} SFX en {:.2}s, {} pistas en {:.2}s (total {:.2}s)", spec.sfx.len() + 1, t_sfx.as_secs_f32(), spec.tracks.len(), (t0.elapsed() - t_sfx).as_secs_f32(), t0.elapsed().as_secs_f32());
}
