//! Editor de sonido standalone (egui) — el "mini-DAW" del juego.
//! Programa de DESARROLLO: corre solo, NUNCA junto al juego ni en el APK.
//!
//! Edita `native/assets/audio.json` (fuente de verdad del audio): capas de
//! tono de cada SFX, cadena de efectos (reverb/eco/filtro/bitcrush), pistas
//! musicales generativas (escala, semilla, densidad), volúmenes y silencios
//! del director. Guardar escribe el JSON y lo verifica releyéndolo; el juego
//! lo embebe al recompilar (`./build-y-publicar.sh`).

use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use ha_audio::{
    render_sfx, render_track, AudioSpec, Effect, MelodySpec, PadSpec, SfxSpec, ToneOp, TrackSpec,
    Wave,
};

const DEFAULT_PATH: &str = "assets/audio.json";

pub fn run_sound_gui() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_title("Editor de sonido — mini-DAW del juego"),
        ..Default::default()
    };
    let res = eframe::run_native(
        "ha-sound-editor",
        options,
        Box::new(|_| Ok(Box::new(SoundApp::new()) as Box<dyn eframe::App>)),
    );
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("editor de sonido: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Qué está seleccionado en el panel izquierdo.
#[derive(Clone, PartialEq)]
enum Sel {
    Mix,
    Sfx(String),
    Thrust,
    Track(usize),
}

struct SoundApp {
    path: String,
    spec: AudioSpec,
    defaults: AudioSpec,
    sel: Sel,
    status: String,
    dirty: bool,
    // Salida de audio (perezosa) + sink activo para poder frenar.
    audio: Option<(rodio::OutputStream, rodio::OutputStreamHandle)>,
    sink: Option<rodio::Sink>,
    // Cache de la forma de onda del ítem seleccionado (clave = JSON del ítem).
    wave_key: String,
    wave_buf: Vec<f32>,
    wave_rate: u32,
}

impl SoundApp {
    fn new() -> Self {
        let defaults = AudioSpec::default_spec();
        let (spec, status) = match std::fs::read_to_string(DEFAULT_PATH) {
            Ok(s) => match AudioSpec::from_json(&s) {
                Ok(spec) => (spec, format!("cargado {DEFAULT_PATH}")),
                Err(e) => (defaults.clone(), format!("JSON invalido ({e}) — defaults")),
            },
            Err(_) => (defaults.clone(), "sin audio.json — arrancando del default".into()),
        };
        Self {
            path: DEFAULT_PATH.to_string(),
            spec,
            defaults,
            sel: Sel::Mix,
            status,
            dirty: false,
            audio: None,
            sink: None,
            wave_key: String::new(),
            wave_buf: Vec::new(),
            wave_rate: ha_audio::SFX_RATE,
        }
    }

    fn save(&mut self) {
        let json = self.spec.to_json();
        if let Err(e) = std::fs::write(&self.path, &json) {
            self.status = format!("ERROR guardando {}: {e}", self.path);
            return;
        }
        // Verificación: releer del disco y parsear.
        match std::fs::read_to_string(&self.path).map_err(|e| e.to_string())
            .and_then(|s| AudioSpec::from_json(&s).map_err(|e| e.to_string()))
        {
            Ok(back) if back.sfx.len() == self.spec.sfx.len() => {
                self.dirty = false;
                self.status = format!(
                    "guardado {} ({} SFX, {} pistas) — recompila con ./build-y-publicar.sh para oirlo en el juego",
                    self.path, back.sfx.len(), back.tracks.len()
                );
            }
            Ok(_) => self.status = "guardado pero la verificacion no coincide".into(),
            Err(e) => self.status = format!("guardado pero NO verifica: {e}"),
        }
    }

    fn play(&mut self, rate: u32, buf: Vec<f32>, vol: f32) {
        if self.audio.is_none() {
            self.audio = rodio::OutputStream::try_default().ok();
        }
        let Some((_, handle)) = &self.audio else {
            self.status = "sin salida de audio en esta maquina".into();
            return;
        };
        if let Some(s) = self.sink.take() {
            s.stop();
        }
        // Soft-clip como en el juego (mismo render final que el WAV).
        let buf: Vec<f32> = buf.iter().map(|s| s.tanh()).collect();
        match rodio::Sink::try_new(handle) {
            Ok(sink) => {
                sink.set_volume(vol);
                sink.append(rodio::buffer::SamplesBuffer::new(1, rate, buf));
                self.sink = Some(sink);
            }
            Err(e) => self.status = format!("no pude reproducir: {e}"),
        }
    }

    fn stop(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
    }

    /// Cachea la forma de onda del SFX seleccionado (rerender sólo si cambió).
    fn ensure_sfx_wave(&mut self, key_prefix: &str, spec: &SfxSpec) {
        let key = format!("{key_prefix}:{}", serde_json::to_string(spec).unwrap_or_default());
        if key != self.wave_key {
            self.wave_key = key;
            self.wave_buf = render_sfx(spec);
            self.wave_rate = spec.rate;
        }
    }
}

impl eframe::App for SoundApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.top_bar(ctx);
        self.left_list(ctx);
        self.central(ctx);
    }
}

// ---------------------------------------------------------------------------
// UI
// ---------------------------------------------------------------------------

/// Nombre humano de cada SFX (para la lista).
fn sfx_label(key: &str) -> &'static str {
    match key {
        "click" => "click de UI",
        "dig" => "cavar",
        "chop" => "talar",
        "place" => "colocar bloque",
        "stamp" => "estampar prefab",
        "step_soft1" => "paso blando 1",
        "step_soft2" => "paso blando 2",
        "step_hard1" => "paso duro 1",
        "step_hard2" => "paso duro 2",
        "bag_open" => "abrir mochila",
        "bag_close" => "cerrar mochila",
        "craft" => "craftear",
        "eat" => "comer",
        "drink" => "beber",
        "hurt" => "dano",
        "jump" => "saltar",
        "touchdown" => "aterrizar",
        "door_open" => "abrir puerta",
        "door_close" => "cerrar puerta",
        "stairs" => "escaleras",
        "launch" => "despegue",
        "pickup" => "recoger item",
        _ => "(sfx)",
    }
}

const MOODS: [(&str, &str); 3] = [
    ("surface", "superficie"),
    ("space", "espacio"),
    ("underground", "subsuelo"),
];

impl SoundApp {
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("🎛 Editor de sonido");
                ui.separator();
                let save = egui::Button::new(egui::RichText::new("💾 Guardar (actualiza el juego al recompilar)").strong());
                if ui.add(save).clicked() {
                    self.save();
                }
                if ui.button("↩ recargar del disco").clicked() {
                    *self = {
                        let mut a = SoundApp::new();
                        a.sel = self.sel.clone();
                        a
                    };
                }
                if ui.button("⏹ silencio").clicked() {
                    self.stop();
                }
                if self.dirty {
                    ui.label(egui::RichText::new("● cambios sin guardar").color(egui::Color32::YELLOW));
                }
            });
            ui.label(
                egui::RichText::new(format!("{} · archivo: {}", self.status, self.path))
                    .weak()
                    .small(),
            );
        });
    }

    fn left_list(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("list").default_width(240.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let sel = self.sel.clone();
                if ui.selectable_label(sel == Sel::Mix, "🎚 Mezcla y director").clicked() {
                    self.sel = Sel::Mix;
                }
                ui.separator();
                ui.label(egui::RichText::new("🔊 EFECTOS (SFX)").strong());
                let keys: Vec<String> = self.spec.sfx.keys().cloned().collect();
                for k in keys {
                    let lbl = format!("{k} — {}", sfx_label(&k));
                    if ui.selectable_label(sel == Sel::Sfx(k.clone()), lbl).clicked() {
                        self.sel = Sel::Sfx(k);
                    }
                }
                if ui.selectable_label(sel == Sel::Thrust, "thrust — loop del cohete").clicked() {
                    self.sel = Sel::Thrust;
                }
                ui.separator();
                ui.label(egui::RichText::new("🎵 PISTAS MUSICALES").strong());
                for i in 0..self.spec.tracks.len() {
                    let t = &self.spec.tracks[i];
                    let mood = MOODS.iter().find(|(m, _)| *m == t.mood).map(|(_, l)| *l).unwrap_or("?");
                    let lbl = format!("{} ({mood}, {:.0}s)", t.name, t.dur);
                    if ui.selectable_label(sel == Sel::Track(i), lbl).clicked() {
                        self.sel = Sel::Track(i);
                    }
                }
                if ui.button("➕ nueva pista").clicked() {
                    let n = self.spec.tracks.len();
                    self.spec.tracks.push(TrackSpec {
                        name: format!("pista-{}", n + 1),
                        mood: "surface".into(),
                        seed: now_seed(),
                        dur: 40.0,
                        base: 220.0,
                        scale: vec![0.0, 2.0, 4.0, 7.0, 9.0, 12.0],
                        rate: ha_audio::MUSIC_RATE,
                        pad: Some(PadSpec::default()),
                        melody: MelodySpec::default(),
                        effects: vec![Effect::FadeOut { secs: 3.0 }],
                    });
                    self.sel = Sel::Track(n);
                    self.dirty = true;
                }
            });
        });
    }

    fn central(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                match self.sel.clone() {
                    Sel::Mix => self.mix_panel(ui),
                    Sel::Sfx(k) => self.sfx_panel(ui, &k),
                    Sel::Thrust => self.thrust_panel(ui),
                    Sel::Track(i) => self.track_panel(ui, i),
                }
            });
        });
    }

    fn mix_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Mezcla y director de musica");
        ui.add_space(6.0);
        let before = serde_json::to_string(&(
            self.spec.sfx_volume,
            self.spec.music_volume,
            &self.spec.director,
        ))
        .unwrap_or_default();
        ui.add(egui::Slider::new(&mut self.spec.sfx_volume, 0.0..=1.0).text("volumen SFX"));
        ui.add(egui::Slider::new(&mut self.spec.music_volume, 0.0..=1.0).text("volumen musica"));
        ui.add_space(8.0);
        ui.label("El director elige una pista del contexto (superficie/espacio/subsuelo) y despues deja MINUTOS de silencio (filosofia Minecraft):");
        ui.add(egui::Slider::new(&mut self.spec.director.first_delay, 0.0..=120.0).text("primer tema (s desde el arranque)"));
        ui.add(egui::Slider::new(&mut self.spec.director.silence_min, 10.0..=600.0).text("silencio minimo entre pistas (s)"));
        ui.add(egui::Slider::new(&mut self.spec.director.silence_max, 10.0..=900.0).text("silencio maximo entre pistas (s)"));
        if self.spec.director.silence_max < self.spec.director.silence_min {
            self.spec.director.silence_max = self.spec.director.silence_min;
        }
        let after = serde_json::to_string(&(
            self.spec.sfx_volume,
            self.spec.music_volume,
            &self.spec.director,
        ))
        .unwrap_or_default();
        if before != after {
            self.dirty = true;
        }
    }

    fn sfx_panel(&mut self, ui: &mut egui::Ui, key: &str) {
        ui.heading(format!("SFX: {key} — {}", sfx_label(key)));
        let Some(mut spec) = self.spec.sfx.get(key).cloned() else { return };
        let mut changed = sfx_editor(ui, &mut spec, false);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button(egui::RichText::new("▶ Escuchar").strong()).clicked() {
                let buf = render_sfx(&spec);
                let vol = self.spec.sfx_volume;
                self.play(spec.rate, buf, vol);
            }
            if ui.button("↩ restaurar default de este SFX").clicked() {
                if let Some(d) = self.defaults.sfx.get(key) {
                    spec = d.clone();
                    changed = true;
                }
            }
        });
        self.ensure_sfx_wave(key, &spec);
        waveform(ui, &self.wave_buf, self.wave_rate);
        if changed {
            self.spec.sfx.insert(key.to_string(), spec);
            self.dirty = true;
        }
    }

    fn thrust_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Loop de empuje del cohete (loopeable)");
        ui.label(egui::RichText::new("Suena en bucle mientras propulsas; el crossfade circular cierra el loop sin click.").weak());
        let mut spec = self.spec.thrust.clone();
        let mut changed = sfx_editor(ui, &mut spec, true);
        ui.horizontal(|ui| {
            if ui.button(egui::RichText::new("▶ Escuchar (3 vueltas)").strong()).clicked() {
                let one = render_sfx(&spec);
                let mut buf = Vec::with_capacity(one.len() * 3);
                for _ in 0..3 {
                    buf.extend_from_slice(&one);
                }
                let vol = self.spec.sfx_volume * 0.5;
                self.play(spec.rate, buf, vol);
            }
            if ui.button("↩ restaurar default").clicked() {
                spec = self.defaults.thrust.clone();
                changed = true;
            }
        });
        self.ensure_sfx_wave("thrust", &spec);
        waveform(ui, &self.wave_buf, self.wave_rate);
        if changed {
            self.spec.thrust = spec;
            self.dirty = true;
        }
    }

    fn track_panel(&mut self, ui: &mut egui::Ui, i: usize) {
        if i >= self.spec.tracks.len() {
            self.sel = Sel::Mix;
            return;
        }
        let mut t = self.spec.tracks[i].clone();
        let before = serde_json::to_string(&t).unwrap_or_default();

        ui.heading(format!("Pista: {}", t.name));
        ui.horizontal(|ui| {
            ui.label("nombre:");
            ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(180.0));
            ui.label("contexto:");
            egui::ComboBox::from_id_salt("mood")
                .selected_text(MOODS.iter().find(|(m, _)| *m == t.mood).map(|(_, l)| *l).unwrap_or("?"))
                .show_ui(ui, |ui| {
                    for (m, l) in MOODS {
                        ui.selectable_value(&mut t.mood, m.to_string(), l);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("semilla:");
            ui.add(egui::DragValue::new(&mut t.seed));
            if ui.button("🎲 otra").clicked() {
                t.seed = now_seed();
            }
            ui.label("duracion (s):");
            ui.add(egui::DragValue::new(&mut t.dur).range(10.0..=180.0).speed(0.5));
            ui.label("nota base (Hz):");
            ui.add(egui::DragValue::new(&mut t.base).range(40.0..=880.0).speed(0.5));
        });

        ui.add_space(4.0);
        ui.label(egui::RichText::new("Escala (semitonos sobre la base) — define el 'color' melodico:").strong());
        ui.horizontal_wrapped(|ui| {
            let mut remove: Option<usize> = None;
            for (j, deg) in t.scale.iter_mut().enumerate() {
                ui.add(egui::DragValue::new(deg).range(-24.0..=36.0).speed(0.1));
                if ui.small_button("x").clicked() {
                    remove = Some(j);
                }
                ui.add_space(6.0);
            }
            if let Some(j) = remove {
                if t.scale.len() > 1 {
                    t.scale.remove(j);
                }
            }
            if ui.button("➕ grado").clicked() {
                t.scale.push(12.0);
            }
        });
        ui.label(egui::RichText::new("presets:").weak());
        ui.horizontal(|ui| {
            if ui.button("pentatonica mayor").clicked() {
                t.scale = vec![0.0, 2.0, 4.0, 7.0, 9.0, 12.0, 14.0];
            }
            if ui.button("pentatonica menor").clicked() {
                t.scale = vec![0.0, 3.0, 5.0, 7.0, 10.0, 12.0];
            }
            if ui.button("oscura (subsuelo)").clicked() {
                t.scale = vec![0.0, 3.0, 5.0, 8.0, 10.0];
            }
        });

        ui.add_space(6.0);
        let mut has_pad = t.pad.is_some();
        ui.checkbox(&mut has_pad, "pad de fondo (colchon grave)");
        if has_pad && t.pad.is_none() {
            t.pad = Some(PadSpec::default());
        }
        if !has_pad {
            t.pad = None;
        }
        if let Some(pad) = &mut t.pad {
            ui.indent("pad", |ui| {
                wave_combo(ui, "onda pad", &mut pad.wave);
                ui.add(egui::Slider::new(&mut pad.freq_mult, 0.25..=2.0).text("mult. de frecuencia (0.5 = octava abajo)"));
                ui.add(egui::Slider::new(&mut pad.amp, 0.0..=0.3).text("volumen"));
                ui.add(egui::Slider::new(&mut pad.vibrato, 0.0..=3.0).text("vibrato"));
            });
        }

        ui.add_space(6.0);
        egui::CollapsingHeader::new("🎼 melodia (generador de notas)").default_open(true).show(ui, |ui| {
            let m = &mut t.melody;
            wave_combo(ui, "timbre", &mut m.wave);
            ui.add(egui::Slider::new(&mut m.amp, 0.0..=0.5).text("volumen de nota"));
            ui.add(egui::Slider::new(&mut m.attack, 0.0..=1.0).text("ataque (suavidad de entrada)"));
            ui.add(egui::Slider::new(&mut m.vibrato, 0.0..=3.0).text("vibrato"));
            ui.horizontal(|ui| {
                ui.label("duracion de nota (s):");
                ui.add(egui::DragValue::new(&mut m.note_min).range(0.1..=8.0).speed(0.05));
                ui.label("a");
                ui.add(egui::DragValue::new(&mut m.note_max).range(0.1..=8.0).speed(0.05));
            });
            ui.horizontal(|ui| {
                ui.label("espera entre notas (s):");
                ui.add(egui::DragValue::new(&mut m.gap_min).range(0.05..=8.0).speed(0.05));
                ui.label("a");
                ui.add(egui::DragValue::new(&mut m.gap_max).range(0.05..=8.0).speed(0.05));
            });
            ui.add(egui::Slider::new(&mut m.rest_prob, 0.0..=0.9).text("probabilidad de silencio (el aire ES la pista)"));
            ui.add(egui::Slider::new(&mut m.harmony_prob, 0.0..=1.0).text("probabilidad de armonia (nota grave extra)"));
            ui.add(egui::Slider::new(&mut m.harmony_amp, 0.0..=0.3).text("volumen de la armonia"));
        });

        effects_ui(ui, &mut t.effects);

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(egui::RichText::new("▶ Escuchar pista completa").strong()).clicked() {
                let buf = render_track(&t);
                self.wave_key = format!("track:{}", serde_json::to_string(&t).unwrap_or_default());
                self.wave_buf = buf.clone();
                self.wave_rate = t.rate;
                let vol = self.spec.music_volume;
                self.play(t.rate, buf, vol);
            }
            if ui.button("👯 duplicar").clicked() {
                let mut c = t.clone();
                c.name = format!("{}-copia", c.name);
                c.seed = now_seed();
                self.spec.tracks.push(c);
                self.dirty = true;
            }
            if ui.button("🗑 borrar pista").clicked() {
                self.spec.tracks.remove(i);
                self.sel = Sel::Mix;
                self.dirty = true;
                return;
            }
        });
        if self.wave_key.starts_with("track:") && !self.wave_buf.is_empty() {
            waveform(ui, &self.wave_buf, self.wave_rate);
        }

        if i < self.spec.tracks.len() {
            let after = serde_json::to_string(&t).unwrap_or_default();
            if after != before {
                self.spec.tracks[i] = t;
                self.dirty = true;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Widgets compartidos
// ---------------------------------------------------------------------------

fn now_seed() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ d.as_secs() as u32)
        .unwrap_or(0xC0FFEE)
}

fn wave_combo(ui: &mut egui::Ui, label: &str, wave: &mut Wave) {
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(label)
            .selected_text(wave.label())
            .show_ui(ui, |ui| {
                for w in Wave::ALL {
                    ui.selectable_value(wave, w, w.label());
                }
            });
    });
}

/// Editor de un `SfxSpec` (capas de tono + efectos). Devuelve `true` si cambió.
fn sfx_editor(ui: &mut egui::Ui, spec: &mut SfxSpec, is_loop: bool) -> bool {
    let before = serde_json::to_string(spec).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("duracion total (s):");
        ui.add(egui::DragValue::new(&mut spec.dur).range(0.02..=5.0).speed(0.01));
        if is_loop {
            ui.label(egui::RichText::new("(largo del loop)").weak());
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Capas de tono (se suman — cada fila es un 'instrumento'):").strong());
    let mut remove: Option<usize> = None;
    for (j, op) in spec.tones.iter_mut().enumerate() {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("capa {}", j + 1));
                egui::ComboBox::from_id_salt(("wave", j))
                    .selected_text(op.wave.label())
                    .show_ui(ui, |ui| {
                        for w in Wave::ALL {
                            ui.selectable_value(&mut op.wave, w, w.label());
                        }
                    });
                if ui.small_button("🗑").clicked() {
                    remove = Some(j);
                }
            });
            ui.horizontal(|ui| {
                ui.label("inicio (s):");
                ui.add(egui::DragValue::new(&mut op.at).range(0.0..=5.0).speed(0.01));
                ui.label("dura (s):");
                ui.add(egui::DragValue::new(&mut op.dur).range(0.01..=5.0).speed(0.01));
                ui.label("Hz inicio:");
                ui.add(egui::DragValue::new(&mut op.f0).range(0.0..=4000.0).speed(1.0));
                ui.label("→ fin:");
                ui.add(egui::DragValue::new(&mut op.f1).range(0.0..=4000.0).speed(1.0));
            });
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut op.amp, 0.0..=1.0).text("volumen"));
                ui.add(egui::Slider::new(&mut op.attack, 0.0..=1.0).text("ataque"));
                ui.add(egui::Slider::new(&mut op.vibrato, 0.0..=4.0).text("vibrato"));
            });
            if op.wave == Wave::Noise {
                ui.label(egui::RichText::new("(ruido: las frecuencias no aplican)").weak().small());
            }
        });
    }
    if let Some(j) = remove {
        spec.tones.remove(j);
    }
    if ui.button("➕ agregar capa de tono").clicked() {
        spec.tones.push(ToneOp {
            wave: Wave::Triangle,
            at: 0.0,
            dur: spec.dur.min(0.2),
            f0: 440.0,
            f1: 440.0,
            amp: 0.25,
            attack: 0.05,
            vibrato: 0.0,
        });
    }

    effects_ui(ui, &mut spec.effects);
    serde_json::to_string(spec).unwrap_or_default() != before
}

/// Editor de la cadena de efectos (se aplican en orden).
fn effects_ui(ui: &mut egui::Ui, effects: &mut Vec<Effect>) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new("Cadena de efectos (en orden):").strong());
    let mut remove: Option<usize> = None;
    let mut swap: Option<(usize, usize)> = None;
    let len = effects.len();
    for (j, fx) in effects.iter_mut().enumerate() {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                let name = match fx {
                    Effect::Lowpass { .. } => "filtro grave (paso-bajo)",
                    Effect::Echo { .. } => "eco",
                    Effect::Reverb { .. } => "reverberacion",
                    Effect::Bitcrush { .. } => "bitcrush (mas 8-bit)",
                    Effect::FadeOut { .. } => "fundido de salida",
                    Effect::LoopCrossfade { .. } => "cierre de loop",
                    Effect::Normalize { .. } => "normalizar",
                };
                ui.label(egui::RichText::new(name).strong());
                if j > 0 && ui.small_button("▲").clicked() {
                    swap = Some((j - 1, j));
                }
                if j + 1 < len && ui.small_button("▼").clicked() {
                    swap = Some((j, j + 1));
                }
                if ui.small_button("🗑").clicked() {
                    remove = Some(j);
                }
            });
            match fx {
                Effect::Lowpass { alpha } => {
                    ui.add(egui::Slider::new(alpha, 0.02..=1.0).text("apertura (bajo = mas apagado)"));
                }
                Effect::Echo { delay, feedback, mix } => {
                    ui.add(egui::Slider::new(delay, 0.02..=1.0).text("retardo (s)"));
                    ui.add(egui::Slider::new(feedback, 0.0..=0.9).text("repeticiones"));
                    ui.add(egui::Slider::new(mix, 0.0..=1.0).text("mezcla"));
                }
                Effect::Reverb { size, mix } => {
                    ui.add(egui::Slider::new(size, 0.0..=1.0).text("tamano de sala"));
                    ui.add(egui::Slider::new(mix, 0.0..=1.0).text("mezcla"));
                }
                Effect::Bitcrush { bits, downsample } => {
                    ui.add(egui::Slider::new(bits, 1..=16).text("bits"));
                    ui.add(egui::Slider::new(downsample, 1..=16).text("submuestreo"));
                }
                Effect::FadeOut { secs } => {
                    ui.add(egui::Slider::new(secs, 0.05..=10.0).text("segundos"));
                }
                Effect::LoopCrossfade { secs } => {
                    ui.add(egui::Slider::new(secs, 0.005..=0.2).text("borde (s)"));
                }
                Effect::Normalize { peak } => {
                    ui.add(egui::Slider::new(peak, 0.1..=1.0).text("pico"));
                }
            }
        });
    }
    if let Some(j) = remove {
        effects.remove(j);
    }
    if let Some((a, b)) = swap {
        effects.swap(a, b);
    }
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ reverb").clicked() {
            effects.push(Effect::Reverb { size: 0.5, mix: 0.3 });
        }
        if ui.button("+ eco").clicked() {
            effects.push(Effect::Echo { delay: 0.15, feedback: 0.4, mix: 0.5 });
        }
        if ui.button("+ filtro grave").clicked() {
            effects.push(Effect::Lowpass { alpha: 0.2 });
        }
        if ui.button("+ bitcrush").clicked() {
            effects.push(Effect::Bitcrush { bits: 6, downsample: 3 });
        }
        if ui.button("+ fundido").clicked() {
            effects.push(Effect::FadeOut { secs: 1.0 });
        }
        if ui.button("+ normalizar").clicked() {
            effects.push(Effect::Normalize { peak: 0.9 });
        }
        if ui.button("+ cierre de loop").clicked() {
            effects.push(Effect::LoopCrossfade { secs: 0.02 });
        }
    });
}

/// Dibuja la forma de onda (min/max por columna) + duración.
fn waveform(ui: &mut egui::Ui, buf: &[f32], rate: u32) {
    ui.add_space(6.0);
    let h = 84.0;
    let w = ui.available_width().max(120.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, egui::Color32::from_gray(22));
    if buf.is_empty() {
        return;
    }
    let n = buf.len();
    let cols = rect.width() as usize;
    let mid = rect.center().y;
    let half = h * 0.48;
    for x in 0..cols {
        let a = x * n / cols;
        let b = (((x + 1) * n / cols).max(a + 1)).min(n);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for s in &buf[a..b] {
            lo = lo.min(*s);
            hi = hi.max(*s);
        }
        let x = rect.left() + x as f32;
        p.line_segment(
            [
                egui::pos2(x, mid - hi.clamp(-1.0, 1.0) * half),
                egui::pos2(x, mid - lo.clamp(-1.0, 1.0) * half),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 230, 140)),
        );
    }
    ui.label(
        egui::RichText::new(format!("{:.2} s @ {} Hz", n as f32 / rate as f32, rate))
            .weak()
            .small(),
    );
}
