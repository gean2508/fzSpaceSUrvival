//! F3 — editor de assets standalone (egui). Programa de DESARROLLO: corre solo,
//! NUNCA junto al juego.
//!
//! Modelo (lo que pediste): la lista de la izquierda son los **slots que el
//! juego usa** (`game_slots()`): árbol, roca, agua, ítems, máquinas, jugador…
//! Elegís un slot → elegís un PNG del arte fuente (LimeZu singles) → se **copia**
//! al árbol de assets del juego y se registra en `manifest.json`. Marcás si es
//! animado (agua/personajes: tira de N frames a F fps). Guardás → el juego se
//! reskinea en el próximo build. "Catálogo" regenera `CATALOGO.md` para Claude.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ha_assets::{game_slots, list_pngs, ArtAnim, ArtManifest, SlotDef};

pub fn run_gui() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1240.0, 820.0]),
        ..Default::default()
    };
    let res = eframe::run_native(
        "ha-asset-tools — editor de assets del juego (dev)",
        options,
        Box::new(|_cc| Ok(Box::new(EditorApp::new()))),
    );
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("no pude abrir la ventana (¿hay display?): {e}");
            ExitCode::FAILURE
        }
    }
}

struct EditorApp {
    slots: Vec<SlotDef>,
    manifest: ArtManifest,
    /// Carpeta con los PNG fuente (LimeZu singles ya purgados).
    art_root: String,
    /// `manifest.json` del juego (destino).
    game_manifest: String,
    /// Raíz de assets del juego; los PNG elegidos se copian bajo `art/<key>.png`.
    game_assets: String,
    /// Ruta del `CATALOGO.md`.
    catalog_path: String,

    slot_filter: String,
    only_unbound: bool,
    png_filter: String,
    selected: Option<usize>,
    pngs: Vec<PathBuf>,

    /// Claves de los slots nativos del juego (no borrables desde el CRUD).
    core_keys: std::collections::BTreeSet<String>,
    /// Buffer del CRUD "añadir slot".
    new_slot_key: String,

    // buffers de animación del slot seleccionado
    anim_on: bool,
    anim_frames: u32,
    anim_fps: f32,

    // caches de textura (key = ruta absoluta)
    tex: Option<(String, egui::TextureHandle)>,
    status: String,
}

impl EditorApp {
    fn new() -> Self {
        let slots = game_slots();
        let core_keys = slots.iter().map(|s| s.key.to_string()).collect();
        let mut app = EditorApp {
            slots,
            manifest: ArtManifest::default(),
            art_root: "../PRUEBAS-CODIGOS".to_string(),
            game_manifest: "crates/ha-game/assets/art/manifest.json".to_string(),
            game_assets: "crates/ha-game/assets".to_string(),
            catalog_path: "assets/CATALOGO.md".to_string(),
            slot_filter: String::new(),
            only_unbound: false,
            png_filter: String::new(),
            selected: None,
            pngs: Vec::new(),
            core_keys,
            new_slot_key: String::new(),
            anim_on: false,
            anim_frames: 4,
            anim_fps: 6.0,
            tex: None,
            status: String::new(),
        };
        app.load_manifest();
        app
    }

    /// Asegura que exista un slot con esa clave (para claves del manifest que no
    /// son nativas, y para el CRUD "añadir slot"). Categoría = prefijo de la clave.
    fn ensure_slot(&mut self, key: &str) {
        if key.is_empty() || self.slots.iter().any(|s| s.key == key) {
            return;
        }
        let category: &'static str = leak(key.split('/').next().unwrap_or("custom").to_string());
        self.slots.push(SlotDef { key: leak(key.to_string()), category, animated: false });
        self.slots.sort_by(|a, b| a.category.cmp(b.category).then(a.key.cmp(b.key)));
    }

    /// Cantidad de slots sin PNG asignado (arte procedural en el juego).
    fn unbound_count(&self) -> usize {
        self.slots.iter().filter(|s| self.manifest.get(s.key).is_none()).count()
    }

    fn ui(&mut self, ctx: &egui::Context) {
        self.top_bar(ctx);
        self.left_slots(ctx);
        self.central_detail(ctx);
    }

    // ── paneles ───────────────────────────────────────────────────────────

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Arte fuente:");
                ui.add(egui::TextEdit::singleline(&mut self.art_root).desired_width(260.0));
                if ui.button("📂 Cargar PNGs").clicked() {
                    self.load_pngs();
                }
                ui.separator();
                if ui.button("💾 Guardar (actualiza el juego)").clicked() {
                    self.save_manifest();
                }
                if ui.button("📄 Catálogo").clicked() {
                    self.write_catalog();
                }
                if ui.button("↻ Recargar manifest").clicked() {
                    self.load_manifest();
                }
            });
            // CRUD: añadir un slot nuevo (clave lógica, ej. ui/mi_boton).
            ui.horizontal(|ui| {
                ui.label("＋ nuevo slot:");
                ui.add(egui::TextEdit::singleline(&mut self.new_slot_key).desired_width(220.0)
                    .hint_text("categoria/nombre (ej. ui/boton_craft)"));
                if ui.button("Añadir").clicked() {
                    let key = self.new_slot_key.trim().to_string();
                    if key.contains('/') {
                        self.ensure_slot(&key);
                        self.status = format!("slot añadido: {key}");
                        self.new_slot_key.clear();
                    } else {
                        self.status = "la clave debe ser categoria/nombre".into();
                    }
                }
            });
            let bound = self.manifest.sprites.len();
            ui.label(format!(
                "{} slots · {} con PNG · {} SIN PNG (procedural) · {} PNGs fuente",
                self.slots.len(),
                bound,
                self.unbound_count(),
                self.pngs.len()
            ));
            if !self.status.is_empty() {
                ui.label(egui::RichText::new(&self.status).weak());
            }
        });
    }

    fn left_slots(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("slots").default_width(320.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("🔎 slot");
                ui.text_edit_singleline(&mut self.slot_filter);
            });
            ui.checkbox(&mut self.only_unbound, "solo sin PNG (a asignar)");
            ui.separator();
            let filter = self.slot_filter.to_lowercase();
            let only_unbound = self.only_unbound;
            let mut clicked: Option<usize> = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut cur_cat = "";
                for (i, slot) in self.slots.iter().enumerate() {
                    if !filter.is_empty() && !slot.key.to_lowercase().contains(&filter) {
                        continue;
                    }
                    if only_unbound && self.manifest.get(slot.key).is_some() {
                        continue;
                    }
                    if slot.category != cur_cat {
                        cur_cat = slot.category;
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(cur_cat.to_uppercase()).strong());
                    }
                    let has = self.manifest.get(slot.key).is_some();
                    let mark = if has { "● " } else { "○ " };
                    let sel = self.selected == Some(i);
                    if ui.selectable_label(sel, format!("{mark}{}", slot.key)).clicked() {
                        clicked = Some(i);
                    }
                }
            });
            if let Some(i) = clicked {
                self.select_slot(ctx, i);
            }
        });
    }

    fn central_detail(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(i) = self.selected else {
                ui.centered_and_justified(|ui| {
                    ui.label("Elegí un slot del juego a la izquierda para reskinearlo.");
                });
                return;
            };
            let slot = self.slots[i];
            ui.heading(slot.key);

            // Binding actual + preview.
            match self.manifest.get(slot.key) {
                Some(v) => {
                    ui.label(format!("PNG actual: {}", v.png()));
                    if let Some(a) = v.anim() {
                        ui.label(format!("animado: {} frames @ {} fps", a.frames, a.fps));
                    }
                }
                None => {
                    ui.label(egui::RichText::new("sin PNG (usa arte procedural en el juego)").weak());
                }
            }
            if let Some((_, tex)) = self.tex.as_ref() {
                let size = tex.size_vec2() * 5.0;
                ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), size)));
            }

            ui.separator();
            // Animación.
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.anim_on, "animado (tira de frames)");
                if self.anim_on {
                    ui.label("frames");
                    ui.add(egui::DragValue::new(&mut self.anim_frames).range(1..=64));
                    ui.label("fps");
                    ui.add(egui::DragValue::new(&mut self.anim_fps).range(0.1..=60.0));
                }
                if self.manifest.get(slot.key).is_some() && ui.button("Aplicar anim").clicked() {
                    self.apply_anim(slot.key);
                }
                if self.manifest.get(slot.key).is_some() && ui.button("🗑 Quitar PNG").clicked() {
                    self.manifest.clear(slot.key);
                    self.tex = None;
                    self.status = format!("{} → procedural", slot.key);
                }
                if !self.core_keys.contains(slot.key) && ui.button("❌ Eliminar slot").clicked() {
                    self.manifest.clear(slot.key);
                    self.slots.retain(|x| x.key != slot.key);
                    self.selected = None;
                    self.tex = None;
                    self.status = format!("slot {} eliminado", slot.key);
                }
            });

            ui.separator();
            ui.label(egui::RichText::new("Elegí el PNG fuente para este slot:").strong());
            ui.horizontal(|ui| {
                ui.label("🔎 png");
                ui.text_edit_singleline(&mut self.png_filter);
            });
            if self.pngs.is_empty() {
                ui.label(egui::RichText::new("(cargá los PNGs con el botón de arriba)").weak());
            }
            let pf = self.png_filter.to_lowercase();
            let mut pick: Option<PathBuf> = None;
            egui::ScrollArea::vertical().id_salt("pngs").show(ui, |ui| {
                for p in &self.pngs {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    if !pf.is_empty() && !name.to_lowercase().contains(&pf) {
                        continue;
                    }
                    if ui.selectable_label(false, &name).clicked() {
                        pick = Some(p.clone());
                    }
                }
            });
            if let Some(p) = pick {
                self.bind_png(ctx, slot, &p);
            }
        });
    }

    // ── acciones ───────────────────────────────────────────────────────────

    fn load_manifest(&mut self) {
        match std::fs::read_to_string(&self.game_manifest) {
            Ok(s) => match ArtManifest::from_json(&s) {
                Ok(m) => {
                    self.manifest = m;
                    self.status = format!("manifest cargado: {} bindings", self.manifest.sprites.len());
                }
                Err(e) => self.status = format!("manifest ilegible: {e}"),
            },
            Err(_) => {
                self.manifest = ArtManifest::default();
                self.status = "manifest nuevo (no existía) — se creará al guardar".into();
            }
        }
        // Claves del manifest que no son slots nativos → agregarlas (custom persist).
        let keys: Vec<String> = self.manifest.sprites.keys().cloned().collect();
        for k in keys {
            self.ensure_slot(&k);
        }
    }

    fn load_pngs(&mut self) {
        let root = self.abs_art_root();
        if !root.is_dir() {
            self.status = format!("Arte fuente no es carpeta: {}", root.display());
            return;
        }
        self.pngs = list_pngs(&root);
        self.status = format!("{} PNGs fuente cargados de {}", self.pngs.len(), root.display());
    }

    fn select_slot(&mut self, ctx: &egui::Context, i: usize) {
        self.selected = Some(i);
        let slot = self.slots[i];
        // Sincronizar buffers de animación desde el binding (o el hint del slot).
        match self.manifest.get(slot.key).and_then(|v| v.anim()) {
            Some(a) => {
                self.anim_on = true;
                self.anim_frames = a.frames;
                self.anim_fps = a.fps;
            }
            None => {
                self.anim_on = slot.animated;
                self.anim_frames = 4;
                self.anim_fps = 6.0;
            }
        }
        self.tex = self.preview_bound(ctx, slot.key);
    }

    fn bind_png(&mut self, ctx: &egui::Context, slot: SlotDef, src: &Path) {
        // Copiar el PNG fuente al árbol del juego como art/<key>.png.
        let rel = format!("art/{}.png", slot.key);
        let dest = PathBuf::from(&self.game_assets).join(&rel);
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                self.status = format!("no pude crear {}: {e}", parent.display());
                return;
            }
        }
        if let Err(e) = std::fs::copy(src, &dest) {
            self.status = format!("no pude copiar el PNG: {e}");
            return;
        }
        let anim = self.anim_on.then_some(ArtAnim { frames: self.anim_frames.max(1), fps: self.anim_fps.max(0.1) });
        self.manifest.set(slot.key, rel.clone(), anim);
        self.tex = load_texture(ctx, &dest, slot.key);
        self.status = format!("{} ← {} (copiado a {})", slot.key, src.display(), rel);
    }

    fn apply_anim(&mut self, key: &str) {
        if let Some(v) = self.manifest.get(key) {
            let png = v.png().to_string();
            let anim = self.anim_on.then_some(ArtAnim { frames: self.anim_frames.max(1), fps: self.anim_fps.max(0.1) });
            self.manifest.set(key, png, anim);
            self.status = format!("anim de {key} aplicada");
        }
    }

    fn save_manifest(&mut self) {
        match self.manifest.to_json() {
            Ok(json) => {
                if let Some(parent) = PathBuf::from(&self.game_manifest).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::write(&self.game_manifest, json) {
                    Ok(()) => self.status = format!(
                        "guardado {} — rebuildeá el juego (npm run play) para verlo",
                        self.game_manifest
                    ),
                    Err(e) => self.status = format!("write: {e}"),
                }
            }
            Err(e) => self.status = format!("serialize: {e}"),
        }
    }

    fn write_catalog(&mut self) {
        // Catálogo simple de los bindings actuales (para contexto de Claude).
        let mut md = String::from("# CATÁLOGO de arte del juego (autogenerado)\n\n| slot | PNG | animado |\n|------|-----|---------|\n");
        for (key, v) in &self.manifest.sprites {
            let a = v.anim().map(|a| format!("{}f@{}fps", a.frames, a.fps)).unwrap_or_else(|| "no".into());
            md.push_str(&format!("| `{}` | {} | {} |\n", key, v.png(), a));
        }
        if let Some(parent) = PathBuf::from(&self.catalog_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&self.catalog_path, md) {
            Ok(()) => self.status = format!("catálogo → {}", self.catalog_path),
            Err(e) => self.status = format!("write catálogo: {e}"),
        }
    }

    // ── helpers ─────────────────────────────────────────────────────────────

    fn abs_art_root(&self) -> PathBuf {
        PathBuf::from(&self.art_root)
    }

    fn preview_bound(&self, ctx: &egui::Context, key: &str) -> Option<(String, egui::TextureHandle)> {
        let v = self.manifest.get(key)?;
        let path = PathBuf::from(&self.game_assets).join(v.png());
        load_texture(ctx, &path, key)
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        EditorApp::ui(self, ctx);
    }
}

/// Los slots derivados en runtime necesitan `&'static str` y se crean pocas
/// veces (abrir manifest / añadir slot), así que filtrar la cadena es aceptable.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn load_texture(ctx: &egui::Context, path: &Path, id: &str) -> Option<(String, egui::TextureHandle)> {
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
    let tex = ctx.load_texture(id, color, egui::TextureOptions::NEAREST);
    Some((id.to_string(), tex))
}
