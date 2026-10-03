//! Renderiza TODO el audio del spec a WAVs (para auditarlo con `scripts/sfx-lab.py` o escucharlo):
//!   cargo run -p ha-audio --bin render_all -- <carpeta_salida> [audio.json]
//! Sin `audio.json` usa el diseño por defecto (`AudioSpec::default_spec`).

use ha_audio::{hash_pista, hash_sfx, lowpass, render_sfx, render_track, wav_bytes, AudioSpec, MUFFLE_CUTOFF_HZ, MUFFLE_PASSES};

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
    // `SFX_DIR=<carpeta>`: deja cada efecto (con sus tres niveles de sordina) donde lo carga el juego, más el índice con la
    // huella de cada diseño. El juego NO sintetiza nada al arrancar: carga estos archivos.
    if let Ok(dir) = std::env::var("SFX_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        let mut indice: std::collections::BTreeMap<String, String> = Default::default();
        let mut todos: Vec<(String, &ha_audio::SfxSpec)> = spec.sfx.iter().map(|(k, v)| (k.clone(), v)).collect();
        todos.push(("_thrust".to_string(), &spec.thrust));
        for (k, s) in todos {
            let seco = render_sfx(s);
            for lvl in 0..3 {
                let buf = if lvl == 0 { seco.clone() } else { lowpass(&seco, s.rate, MUFFLE_CUTOFF_HZ[lvl], MUFFLE_PASSES[lvl]) };
                std::fs::write(format!("{dir}/{k}_{lvl}.wav"), wav_bytes(s.rate, &buf)).unwrap();
            }
            indice.insert(k, format!("{:08x}", hash_sfx(s)));
        }
        std::fs::write(format!("{dir}/../sfx.json"), serde_json::to_string_pretty(&indice).unwrap()).unwrap();
    }
    let t_sfx = t0.elapsed();
    let mut indice: Vec<(String, u32)> = Vec::new();
    for t in &spec.tracks {
        let t1 = std::time::Instant::now();
        let buf = render_track(t);
        eprintln!("  pista {:16} {:6.2}s ({:.0}s de audio)", t.name, t1.elapsed().as_secs_f32(), t.dur);
        let wav = wav_bytes(t.rate, &buf);
        std::fs::write(format!("{out}/mus_{}.wav", t.name), &wav).unwrap();
        // `MUSICA_DIR=<carpeta>`: además deja la pista donde la carga el juego, con su huella en el índice.
        if let Ok(dir) = std::env::var("MUSICA_DIR") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(format!("{dir}/{}.wav", t.name), &wav).unwrap();
            indice.push((t.name.clone(), hash_pista(t)));
        }
    }
    if let Ok(dir) = std::env::var("MUSICA_DIR") {
        let json: std::collections::BTreeMap<_, _> = indice.iter().map(|(n, h)| (n.clone(), format!("{h:08x}"))).collect();
        std::fs::write(format!("{dir}/../musica.json"), serde_json::to_string_pretty(&json).unwrap()).unwrap();
    }
    eprintln!("render_all: {} SFX en {:.2}s, {} pistas en {:.2}s (total {:.2}s)", spec.sfx.len() + 1, t_sfx.as_secs_f32(), spec.tracks.len(), (t0.elapsed() - t_sfx).as_secs_f32(), t0.elapsed().as_secs_f32());
}
