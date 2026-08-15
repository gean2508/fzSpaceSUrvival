//! Visor de "gimnasio" de NPCs — lista los genomas `.adn` ya evolucionados
//! (fauna, vivario, sociedad), corre un episodio corto en la MISMA arena/sensor
//! que `gym.rs` (determinismo gym↔juego, `biblia.md` §6) para ver cómo
//! se comporta cada uno, y deja mutar/cruzar y comparar el fitness antes de
//! guardar. Es la herramienta de "entrená hasta el comportamiento esperado"
//! interactiva; `gym.rs` sigue siendo el entrenador headless de fondo.
//!
//! Uso: `./editor_assets gym-gui` (o `cargo run -p ha-asset-tools -- gym-gui`).

use std::path::PathBuf;
use std::process::ExitCode;

use ha_shared::fauna_brain::{sense, solid_at as solid_body};
use ha_shared::CelestialBody;

const BODY: CelestialBody = CelestialBody::Earth;
fn solid_at(x: f32, z: f32) -> bool {
    solid_body(BODY, x, z)
}

// Mismos parámetros de arena/física que `gym.rs`, así el episodio que ves acá
// es comparable al que entrenó el genoma (mismo sensor, mismo mundo sintético).
const EVAL_STEPS: usize = 400;
const EVAL_SEED: u32 = 7;
const TURN: f32 = 2.4;
const SPEED: f32 = 3.2;
const EAT_DIST2: f32 = 0.6;
const FOODS: usize = 40;
const TRAIL_EVERY: usize = 4; // un punto de rastro cada N pasos (liviano de dibujar)

/// Ids de trabajo del visor — proceso aparte del juego, así que no colisionan
/// con los rangos reservados (fauna/NPCs/compañero) del juego real.
const VIEW_SLOT: i32 = 0;
const MUTANT_SLOT: i32 = 1;
const CROSS_SLOT: i32 = 2;

fn make_foods(seed: u32) -> Vec<(f32, f32)> {
    // Idéntico a `gym.rs::make_foods` — mismo paisaje de comida determinista.
    let mut s = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let mut rng = || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        (s as f32 / u32::MAX as f32) - 0.5
    };
    (0..FOODS)
        .map(|_| (rng() * 40.0, rng() * 40.0))
        .filter(|&(x, z)| !solid_at(x, z))
        .collect()
}

struct EvalResult {
    fitness: f32,
    eaten: f32,
    walls: f32,
    energy: f32,
    trail: Vec<(f32, f32)>,
}

/// Corre UN agente con el genoma ya cargado en `id` por `EVAL_STEPS` pasos y
/// devuelve fitness + rastro de posiciones (para dibujarlo).
fn run_eval(id: i32) -> EvalResult {
    let mut foods = make_foods(EVAL_SEED);
    let (mut x, mut z, mut heading, mut energy) = (0.0f32, 0.0f32, 0.0f32, 1.0f32);
    let (mut eaten, mut walls) = (0.0f32, 0.0f32);
    let mut trail = vec![(0.0f32, 0.0f32)];
    let dt = 0.06f32;
    for step in 0..EVAL_STEPS {
        if energy <= 0.0 || foods.is_empty() {
            break;
        }
        let s = sense(BODY, x, z, heading, energy, &foods, &[]);
        let steer = iafz_sys::think(id, s).tanh();
        heading += steer * TURN * dt;
        let (nx, nz) = (x + heading.cos() * SPEED * dt, z + heading.sin() * SPEED * dt);
        if solid_at(nx, nz) {
            heading += 1.3;
            walls += 1.0;
        } else {
            x = nx;
            z = nz;
        }
        energy -= dt * 0.03;
        if let Some(i) = foods.iter().position(|&(fx, fz)| {
            let (dx, dz) = (x - fx, z - fz);
            dx * dx + dz * dz < EAT_DIST2
        }) {
            foods.swap_remove(i);
            eaten += 1.0;
            energy = (energy + 0.35).min(1.0);
        }
        if step % TRAIL_EVERY == 0 {
            trail.push((x, z));
        }
    }
    let fitness = eaten * 100.0 + energy * 20.0 - walls * 0.5;
    EvalResult { fitness, eaten, walls, energy, trail }
}

struct GenomeEntry {
    path: PathBuf,
    label: String,
    size: u64,
}

/// Barre los directorios de persistencia conocidos (relativos a `native/`,
/// que es el cwd cuando se corre vía `./editor_assets`) en busca de `.adn`.
fn scan_genomes() -> Vec<GenomeEntry> {
    let dirs = ["society", "vivarium", "crates/iafz-sys/genomes"];
    let mut out = Vec::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d) else { continue };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("adn") {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
            let label = match stem.strip_prefix("g_") {
                Some(uid) => format!("NPC uid {uid}  ({d})"),
                None => format!("{stem}  ({d})"),
            };
            out.push(GenomeEntry { path, label, size });
        }
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

pub(crate) struct GymApp {
    genomes: Vec<GenomeEntry>,
    selected: Option<usize>,
    second: Option<usize>,
    mutate_rate: f32,
    baseline: Option<EvalResult>,
    candidate: Option<EvalResult>,
    status: String,
}

impl GymApp {
    pub(crate) fn new() -> Self {
        Self {
            genomes: scan_genomes(),
            selected: None,
            second: None,
            mutate_rate: 0.06,
            baseline: None,
            candidate: None,
            status: String::new(),
        }
    }

    fn load_selected_into(&self, slot: i32) -> bool {
        let Some(i) = self.selected else { return false };
        let Some(path) = self.genomes[i].path.to_str() else { return false };
        iafz_sys::load(slot, path)
    }
}

impl eframe::App for GymApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

impl GymApp {
    /// Dibuja este espacio de trabajo. Lo llama el shell del Studio
    /// (`studio.rs`) o `eframe` cuando el editor corre solo.
    pub(crate) fn ui(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("gym_genomes").min_width(260.0).show(ctx, |ui| {
            ui.heading("Genomas (.adn)");
            ui.label(egui::RichText::new("society/ · vivarium/ · iafz-sys/genomes/").weak().small());
            if ui.button("Reescanear").clicked() {
                self.genomes = scan_genomes();
            }
            ui.separator();
            egui::ScrollArea::vertical().id_salt("gym_left_scroll").show(ui, |ui| {
                for i in 0..self.genomes.len() {
                    let selected = self.selected == Some(i);
                    let resp = ui.selectable_label(selected, &self.genomes[i].label);
                    if resp.clicked() {
                        self.selected = Some(i);
                        self.baseline = None;
                        self.candidate = None;
                        self.status.clear();
                    }
                    ui.label(
                        egui::RichText::new(format!("  {} bytes", self.genomes[i].size)).weak().small(),
                    );
                }
                if self.genomes.is_empty() {
                    ui.label("no encontré ningún .adn (¿corriste el juego alguna vez?)");
                }
            });
        });

        egui::SidePanel::right("gym_cross").min_width(220.0).show(ctx, |ui| {
            ui.heading("Cruzar con...");
            ui.label(egui::RichText::new("segundo padre para 'Cruzar'").weak().small());
            ui.separator();
            egui::ScrollArea::vertical().id_salt("gym_right_scroll").show(ui, |ui| {
                for i in 0..self.genomes.len() {
                    let selected = self.second == Some(i);
                    if ui.selectable_label(selected, &self.genomes[i].label).clicked() {
                        self.second = Some(i);
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(i) = self.selected else {
                ui.label("Elegí un genoma de la izquierda para inspeccionarlo.");
                return;
            };
            ui.heading(self.genomes[i].label.clone());
            ui.label(format!("archivo: {}", self.genomes[i].path.display()));
            ui.add_space(6.0);

            if ui.button("Evaluar (correr episodio)").clicked() {
                if self.load_selected_into(VIEW_SLOT) {
                    iafz_sys::reset_echo(VIEW_SLOT);
                    self.baseline = Some(run_eval(VIEW_SLOT));
                    self.status.clear();
                } else {
                    self.status = "no pude cargar el genoma".to_string();
                }
            }
            if let Some(b) = &self.baseline {
                ui.label(format!(
                    "ORIGINAL   fitness {:.1}   comida {:.0}   choques {:.0}   energia {:.0}%",
                    b.fitness,
                    b.eaten,
                    b.walls,
                    b.energy * 100.0
                ));
                draw_trail(ui, &b.trail, egui::Color32::from_rgb(120, 200, 255));
            }

            ui.add_space(10.0);
            ui.separator();
            ui.heading("Reentrenar / experimentar");
            ui.add(egui::Slider::new(&mut self.mutate_rate, 0.0..=0.3).text("tasa de mutación"));
            ui.horizontal(|ui| {
                if ui.button("Mutar y evaluar").clicked() {
                    if self.load_selected_into(VIEW_SLOT) {
                        iafz_sys::mutate(VIEW_SLOT, MUTANT_SLOT, self.mutate_rate);
                        iafz_sys::reset_echo(MUTANT_SLOT);
                        self.candidate = Some(run_eval(MUTANT_SLOT));
                        self.status.clear();
                    }
                }
                if ui.button("Cruzar con el de la derecha y evaluar").clicked() {
                    if let Some(j) = self.second {
                        let p2 = self.genomes[j].path.to_str().unwrap_or("").to_string();
                        if self.load_selected_into(VIEW_SLOT) && iafz_sys::load(CROSS_SLOT, &p2) {
                            iafz_sys::crossover(VIEW_SLOT, CROSS_SLOT, MUTANT_SLOT);
                            iafz_sys::reset_echo(MUTANT_SLOT);
                            self.candidate = Some(run_eval(MUTANT_SLOT));
                            self.status.clear();
                        } else {
                            self.status = "no pude cargar alguno de los dos genomas".to_string();
                        }
                    } else {
                        self.status = "elegí un segundo genoma en el panel de la derecha".to_string();
                    }
                }
            });

            if let Some(c) = &self.candidate {
                let delta = self.baseline.as_ref().map(|b| c.fitness - b.fitness);
                let delta_txt = delta.map(|d| format!("   ({d:+.1} vs original)")).unwrap_or_default();
                ui.label(format!(
                    "CANDIDATO  fitness {:.1}{delta_txt}   comida {:.0}   choques {:.0}   energia {:.0}%",
                    c.fitness,
                    c.eaten,
                    c.walls,
                    c.energy * 100.0
                ));
                draw_trail(ui, &c.trail, egui::Color32::from_rgb(255, 180, 90));

                if ui.button("Guardar candidato sobre el genoma seleccionado").clicked() {
                    let path = self.genomes[i].path.to_str().unwrap_or("").to_string();
                    if iafz_sys::save(MUTANT_SLOT, &path) {
                        self.status = format!("guardado en {path}");
                        self.baseline = self.candidate.take();
                    } else {
                        self.status = "no pude guardar".to_string();
                    }
                }
            }

            if !self.status.is_empty() {
                ui.add_space(6.0);
                ui.colored_label(egui::Color32::from_rgb(255, 210, 90), &self.status);
            }
        });
    }
}

/// Dibuja el rastro de posiciones del episodio como una polilínea sobre un
/// lienzo chico — "cómo se comporta" en una sola mirada, sin leer números.
fn draw_trail(ui: &mut egui::Ui, trail: &[(f32, f32)], color: egui::Color32) {
    ui.add_space(4.0);
    let (rect, _resp) = ui.allocate_exact_size(egui::vec2(360.0, 220.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(20, 20, 24));
    if trail.len() < 2 {
        return;
    }
    let (mut minx, mut maxx, mut minz, mut maxz) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for &(x, z) in trail {
        minx = minx.min(x);
        maxx = maxx.max(x);
        minz = minz.min(z);
        maxz = maxz.max(z);
    }
    let (dx, dz) = ((maxx - minx).max(1.0), (maxz - minz).max(1.0));
    let margin = 10.0;
    let to_pt = |x: f32, z: f32| {
        let u = (x - minx) / dx;
        let v = (z - minz) / dz;
        egui::pos2(
            rect.left() + margin + u * (rect.width() - 2.0 * margin),
            rect.top() + margin + v * (rect.height() - 2.0 * margin),
        )
    };
    let stroke = egui::Stroke::new(1.5, color);
    for w in trail.windows(2) {
        painter.line_segment([to_pt(w[0].0, w[0].1), to_pt(w[1].0, w[1].1)], stroke);
    }
    // Marca el punto de partida (verde) y el final (el color del rastro, más grande).
    painter.circle_filled(to_pt(trail[0].0, trail[0].1), 3.0, egui::Color32::from_rgb(120, 230, 140));
    let last = trail[trail.len() - 1];
    painter.circle_filled(to_pt(last.0, last.1), 4.0, color);
}

pub fn run_gym_gui() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 640.0])
            .with_title("NPC Gym — visor de ADN"),
        ..Default::default()
    };
    let res = eframe::run_native(
        "ha-npc-gym",
        options,
        Box::new(|_| Ok(Box::new(GymApp::new()) as Box<dyn eframe::App>)),
    );
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("npc gym: {e}");
            ExitCode::FAILURE
        }
    }
}
