//! F3 — editor de assets standalone (egui). Programa de DESARROLLO: corre solo,
//! NUNCA junto al juego. Sirve para seccionar/etiquetar los sprites (del juego o
//! de un mod) sobre el `manifest.ron`, previsualizarlos y regenerar el catálogo.
//!
//! Flujo: apuntás a la carpeta de arte → Escanear (genera esqueleto) →
//! seleccionás un asset → ajustás ID/footprint/pivote/hitbox/flags/tags →
//! Guardar manifest → Generar CATÁLOGO. La IA nunca toca píxeles: sólo lee IDs.

use std::path::PathBuf;
use std::process::ExitCode;

use ha_assets::{
    generate_catalog, scan_limezu_directory, AnchorKind, Anim, AnimLayout, Asset, Flags, Footprint,
    Hitbox, Manifest, Pivot, ScanOptions, Sheet, YSort,
};

const ANCHORS: [(&str, AnchorKind); 5] = [
    ("BottomCenter", AnchorKind::BottomCenter),
    ("Center", AnchorKind::Center),
    ("TopLeft", AnchorKind::TopLeft),
    ("TopCenter", AnchorKind::TopCenter),
    ("BottomLeft", AnchorKind::BottomLeft),
];

pub fn run_gui() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1180.0, 760.0]),
        ..Default::default()
    };
    let res = eframe::run_native(
        "ha-asset-tools — editor de assets (dev)",
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

#[derive(Default)]
struct EditorApp {
    manifest: Manifest,
    root: String,
    manifest_path: String,
    catalog_path: String,
    prefix: String,
    grid: u32,
    filter: String,
    selected: Option<String>,
    edit: Option<EditBuf>,
    tex: Option<(String, egui::TextureHandle)>,
    status: String,
}

struct EditBuf {
    id: String,
    fw: u32,
    fh: u32,
    pivot_anchor: usize,
    ysort_row: u32,
    solid: bool,
    harvestable: bool,
    billboard: bool,
    has_hitbox: bool,
    hb: [f32; 4],
    tags: String,
    anim: bool,
    anim_frames: u32,
    anim_fps: f32,
}

impl EditBuf {
    fn from_asset(id: &str, a: &Asset) -> Self {
        let pivot_anchor = match a.pivot {
            Pivot::Anchor(k) => ANCHORS.iter().position(|(_, kk)| *kk == k).unwrap_or(0),
            Pivot::Px { .. } => 0,
        };
        let ysort_row = match a.ysort {
            YSort::Row(r) => r,
            YSort::Offset(_) => 0,
        };
        let (has_hitbox, hb) = match a.hitbox {
            Some(Hitbox::Aabb { x, y, w, h }) => (true, [x, y, w, h]),
            None => (false, [0.0, 0.0, a.footprint.w as f32, 1.0]),
        };
        let (anim, anim_frames, anim_fps) = match a.anim {
            Some(Anim { frames, fps, .. }) => (true, frames, fps),
            None => (false, 4, 6.0),
        };
        Self {
            id: id.to_string(),
            fw: a.footprint.w,
            fh: a.footprint.h,
            pivot_anchor,
            ysort_row,
            solid: a.flags.solid,
            harvestable: a.flags.harvestable,
            billboard: a.flags.billboard,
            has_hitbox,
            hb,
            tags: a.tags.join(", "),
            anim,
            anim_frames,
            anim_fps,
        }
    }

    /// Aplica los campos sobre el asset (conserva sheet/rect/ppu del original).
    fn apply_to(&self, a: &mut Asset) {
        a.footprint = Footprint { w: self.fw.max(1), h: self.fh.max(1) };
        a.pivot = Pivot::Anchor(ANCHORS[self.pivot_anchor].1);
        a.ysort = YSort::Row(self.ysort_row);
        a.flags = Flags { solid: self.solid, harvestable: self.harvestable, billboard: self.billboard };
        a.hitbox = self.has_hitbox.then_some(Hitbox::Aabb {
            x: self.hb[0],
            y: self.hb[1],
            w: self.hb[2],
            h: self.hb[3],
        });
        a.anim = self.anim.then_some(Anim {
            frames: self.anim_frames.max(1),
            fps: self.anim_fps.max(0.1),
            layout: AnimLayout::RowRight,
            loop_: true,
        });
        a.tags = self
            .tags
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
}

impl EditorApp {
    fn new() -> Self {
        EditorApp {
            grid: 16,
            prefix: "art".to_string(),
            manifest_path: "assets/manifest.ron".to_string(),
            catalog_path: "assets/CATALOGO.md".to_string(),
            ..Default::default()
        }
    }
}

impl eframe::App for EditorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.top_bar(ctx);
        self.left_list(ctx);
        self.right_editor(ctx);
        self.central_preview(ctx);
    }
}

impl EditorApp {
    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Arte:");
                ui.text_edit_singleline(&mut self.root);
                ui.label("prefijo:");
                ui.add(egui::TextEdit::singleline(&mut self.prefix).desired_width(90.0));
                ui.label("grid:");
                ui.add(egui::DragValue::new(&mut self.grid).range(4..=256));
                if ui.button("⟳ Escanear").clicked() {
                    self.do_scan();
                }
                ui.separator();
                ui.label("manifest:");
                ui.text_edit_singleline(&mut self.manifest_path);
                if ui.button("Abrir").clicked() {
                    self.do_open();
                }
                if ui.button("💾 Guardar").clicked() {
                    self.do_save();
                }
                if ui.button("📄 Catálogo").clicked() {
                    self.do_catalog();
                }
            });
            if !self.status.is_empty() {
                ui.label(egui::RichText::new(&self.status).weak());
            }
        });
    }

    fn left_list(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("list").default_width(320.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("🔎");
                ui.text_edit_singleline(&mut self.filter);
            });
            ui.label(format!("{} assets", self.manifest.assets.len()));
            ui.separator();
            let filter = self.filter.to_lowercase();
            let ids: Vec<String> = self
                .manifest
                .assets
                .keys()
                .filter(|id| filter.is_empty() || id.to_lowercase().contains(&filter))
                .cloned()
                .collect();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for id in ids {
                    let sel = self.selected.as_deref() == Some(id.as_str());
                    if ui.selectable_label(sel, &id).clicked() {
                        self.select(ctx, &id);
                    }
                }
            });
        });
    }

    fn right_editor(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("edit").default_width(320.0).show(ctx, |ui| {
            let Some(edit) = self.edit.as_mut() else {
                ui.label("Seleccioná un asset de la lista.");
                return;
            };
            ui.heading("Editar asset");
            ui.horizontal(|ui| {
                ui.label("ID:");
                ui.text_edit_singleline(&mut edit.id);
            });
            ui.separator();
            ui.label("Footprint (celdas)");
            ui.horizontal(|ui| {
                ui.label("w");
                ui.add(egui::DragValue::new(&mut edit.fw).range(1..=64));
                ui.label("h");
                ui.add(egui::DragValue::new(&mut edit.fh).range(1..=64));
            });
            ui.horizontal(|ui| {
                ui.label("Pivote");
                egui::ComboBox::from_id_salt("pivot")
                    .selected_text(ANCHORS[edit.pivot_anchor].0)
                    .show_ui(ui, |ui| {
                        for (i, (name, _)) in ANCHORS.iter().enumerate() {
                            ui.selectable_value(&mut edit.pivot_anchor, i, *name);
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Y-sort fila");
                ui.add(egui::DragValue::new(&mut edit.ysort_row).range(0..=63));
            });
            ui.separator();
            ui.checkbox(&mut edit.solid, "solid");
            ui.checkbox(&mut edit.harvestable, "harvestable");
            ui.checkbox(&mut edit.billboard, "billboard");
            ui.separator();
            ui.checkbox(&mut edit.has_hitbox, "hitbox (AABB en celdas)");
            if edit.has_hitbox {
                ui.horizontal(|ui| {
                    for (lbl, idx) in [("x", 0), ("y", 1), ("w", 2), ("h", 3)] {
                        ui.label(lbl);
                        ui.add(egui::DragValue::new(&mut edit.hb[idx]).speed(0.1));
                    }
                });
            }
            ui.separator();
            ui.checkbox(&mut edit.anim, "animado");
            if edit.anim {
                ui.horizontal(|ui| {
                    ui.label("frames");
                    ui.add(egui::DragValue::new(&mut edit.anim_frames).range(1..=64));
                    ui.label("fps");
                    ui.add(egui::DragValue::new(&mut edit.anim_fps).range(0.1..=60.0));
                });
            }
            ui.separator();
            ui.label("Tags (coma)");
            ui.text_edit_singleline(&mut edit.tags);
            ui.separator();
            if ui.button("✔ Aplicar").clicked() {
                self.apply_edit();
            }
        });
    }

    fn central_preview(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(id) = self.selected.clone() else {
                ui.centered_and_justified(|ui| ui.label("Editor de assets — escaneá una carpeta para empezar."));
                return;
            };
            ui.heading(&id);
            if let Some(a) = self.manifest.assets.get(&id) {
                ui.label(format!(
                    "sheet: {}   rect: {:?}   footprint {}×{}",
                    a.sheet, a.rect, a.footprint.w, a.footprint.h
                ));
            }
            ui.separator();
            match self.tex.as_ref() {
                Some((tid, tex)) if *tid == id => {
                    // Escala pixel-art ×6, filtro nearest (se fijó al cargar).
                    let size = tex.size_vec2() * 6.0;
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), size)));
                }
                _ => {
                    ui.label("(sin preview — revisá la ruta de Arte)");
                }
            }
        });
    }

    // ── acciones ──────────────────────────────────────────────────────────

    fn do_scan(&mut self) {
        let root = PathBuf::from(&self.root);
        if !root.is_dir() {
            self.status = format!("Arte no es carpeta: {}", root.display());
            return;
        }
        let opts = ScanOptions {
            grid: (self.grid, self.grid),
            id_prefix: self.prefix.clone(),
            max_files: None,
        };
        let (m, report) = scan_limezu_directory(&root, Some(&self.manifest), &opts);
        self.manifest = m;
        self.status = format!(
            "scan: +{} assets ({} leídos, {} saltados) — total {}",
            report.added,
            report.scanned,
            report.skipped,
            self.manifest.assets.len()
        );
    }

    fn do_open(&mut self) {
        match std::fs::read_to_string(&self.manifest_path).ok().map(|s| Manifest::from_ron_str(&s)) {
            Some(Ok(m)) => {
                self.manifest = m;
                self.status = format!("abierto {} ({} assets)", self.manifest_path, self.manifest.assets.len());
            }
            Some(Err(e)) => self.status = format!("parse: {e}"),
            None => self.status = format!("no pude leer {}", self.manifest_path),
        }
    }

    fn do_save(&mut self) {
        match self.manifest.to_ron_string() {
            Ok(s) => {
                if let Some(p) = PathBuf::from(&self.manifest_path).parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                match std::fs::write(&self.manifest_path, s) {
                    Ok(()) => self.status = format!("guardado {}", self.manifest_path),
                    Err(e) => self.status = format!("write: {e}"),
                }
            }
            Err(e) => self.status = format!("serialize: {e}"),
        }
    }

    fn do_catalog(&mut self) {
        let md = generate_catalog(&self.manifest);
        if let Some(p) = PathBuf::from(&self.catalog_path).parent() {
            let _ = std::fs::create_dir_all(p);
        }
        match std::fs::write(&self.catalog_path, md) {
            Ok(()) => self.status = format!("catálogo → {}", self.catalog_path),
            Err(e) => self.status = format!("write catálogo: {e}"),
        }
    }

    fn select(&mut self, ctx: &egui::Context, id: &str) {
        self.selected = Some(id.to_string());
        if let Some(a) = self.manifest.assets.get(id) {
            self.edit = Some(EditBuf::from_asset(id, a));
            self.tex = self.load_preview(ctx, id, a);
        }
    }

    fn apply_edit(&mut self) {
        let Some(edit) = self.edit.as_ref() else { return };
        let Some(old_id) = self.selected.clone() else { return };
        let new_id = edit.id.trim().to_string();
        if let Some(mut asset) = self.manifest.assets.remove(&old_id) {
            edit.apply_to(&mut asset);
            let key = if new_id.is_empty() { old_id.clone() } else { new_id.clone() };
            self.manifest.assets.insert(key.clone(), asset);
            self.selected = Some(key.clone());
            self.status = format!("aplicado → {key}");
        }
    }

    fn load_preview(
        &self,
        ctx: &egui::Context,
        id: &str,
        asset: &Asset,
    ) -> Option<(String, egui::TextureHandle)> {
        let sheet: &Sheet = self.manifest.sheets.get(&asset.sheet)?;
        let path = PathBuf::from(&self.root).join(&sheet.path);
        let img = image::open(&path).ok()?.to_rgba8();
        let (w, h) = img.dimensions();
        let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], img.as_raw());
        let tex = ctx.load_texture(id, color, egui::TextureOptions::NEAREST);
        Some((id.to_string(), tex))
    }
}
