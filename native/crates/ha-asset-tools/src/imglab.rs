//! **Image Lab** — compositor + editor de píxeles integrado al editor de assets.
//!
//! Resuelve dos cosas que pediste sin tocar el runtime del juego:
//!
//! 1. **Un asset armado con VARIOS PNG** (ej. agua: esquinas convexas en una hoja
//!    y cóncavas en otra; o un mueble estático + su puerta animada aparte). Se
//!    importan todos, cada uno queda en su **capa**, se auto-acomodan sin pisarse
//!    → UNA hoja combinada.
//! 2. **Editar el sprite** (pincel, goma, cuentagotas, balde, mover/duplicar/borrar
//!    capas, opacidad, zoom/pan, grilla + guías de frames, deshacer/rehacer).
//!
//! El resultado se **aplana a UN PNG nuevo** guardado APARTE del original bajo
//! `assets/art/edited/<nombre>.png`. El juego sigue leyendo un solo PNG por slot
//! (con rect/strip/poses del slicer de siempre) → **no cambia nada del runtime**.
//! "Usar en slot" deja ese PNG como candidato del slot seleccionado.

use std::path::{Path, PathBuf};

use image::{Rgba, RgbaImage};

/// Acción que el Lab pide al editor host (gui.rs) al terminar.
pub enum LabAction {
    /// Usar este PNG (recién guardado) como candidato del slot seleccionado.
    UseInSlot(PathBuf),
    /// Aplicar al slot seleccionado la LUZ derivada de los píxeles que el usuario
    /// pintó con el pincel 💡 (ver `ImageLab::derived_emission`).
    SetEmission(ha_assets::Emission),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Brush,
    Eraser,
    Eyedropper,
    Bucket,
    Pan,
    /// 💡 Pincel de LUZ: no pinta píxeles del PNG — marca QUÉ PARTE del sprite
    /// emite luz. De esa mancha salen el foco (centroide), el radio (extensión)
    /// y el color (promedio de lo pintado debajo) que se guardan en el manifest
    /// como `emission`. El PNG NO se toca: el runtime sigue cargando UN solo PNG
    /// por slot (nada de mapas de emisión extra).
    Light,
}

/// Una capa = buffer RGBA del tamaño del lienzo. Componer varios PNG = una capa
/// por PNG; editar = pintar sobre la capa activa.
struct Layer {
    name: String,
    buf: RgbaImage,
    visible: bool,
    opacity: f32,
}

impl Layer {
    fn new(name: impl Into<String>, w: u32, h: u32) -> Self {
        Layer { name: name.into(), buf: RgbaImage::new(w.max(1), h.max(1)), visible: true, opacity: 1.0 }
    }
}

pub struct ImageLab {
    w: u32,
    h: u32,
    layers: Vec<Layer>,
    active: usize,

    tool: Tool,
    brush: egui::Color32,
    brush_size: i32,
    tol: i32, // tolerancia del balde
    zoom: f32,
    pan: egui::Vec2,
    grid: u32,
    show_grid: bool,
    frame_cols: u32, // guías de frames (0 = sin guía)
    frame_rows: u32,

    // deshacer/rehacer: snapshots de TODAS las capas (dev-only, memoria holgada).
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,

    // estado de trazo en curso
    last_paint: Option<(i32, i32)>,
    stroke_open: bool,

    /// Máscara del pincel de LUZ (`w*h`). Vive SÓLO en el editor: no es una capa,
    /// no se aplana al PNG, no viaja al juego. Lo que viaja es la `Emission` que
    /// se deriva de ella.
    light_mask: Vec<bool>,

    // cache de textura compuesta (se rehace cuando `dirty`)
    tex: Option<egui::TextureHandle>,
    dirty: bool,

    save_name: String,
    /// último candidato importado (nombre base para "guardar" por defecto).
    pub status: String,
}

struct Snapshot {
    layers: Vec<(String, RgbaImage, bool, f32)>,
    active: usize,
    w: u32,
    h: u32,
}

impl Default for ImageLab {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageLab {
    pub fn new() -> Self {
        let mut lab = ImageLab {
            w: 64,
            h: 64,
            layers: Vec::new(),
            active: 0,
            tool: Tool::Brush,
            brush: egui::Color32::from_rgb(230, 230, 240),
            brush_size: 1,
            tol: 16,
            zoom: 8.0,
            pan: egui::Vec2::ZERO,
            grid: 16,
            show_grid: true,
            frame_cols: 0,
            frame_rows: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            last_paint: None,
            stroke_open: false,
            light_mask: Vec::new(),
            tex: None,
            dirty: true,
            save_name: "editado".into(),
            status: "Lab vacío — Importar PNG(s) o Lienzo nuevo".into(),
        };
        lab.new_canvas(64, 64);
        lab
    }

    // ── API pública (la usa gui.rs) ─────────────────────────────────────────

    /// Reemplaza todo el Lab con un lienzo vacío `w×h` (una capa base transparente).
    pub fn new_canvas(&mut self, w: u32, h: u32) {
        self.w = w.max(1);
        self.h = h.max(1);
        self.layers = vec![Layer::new("capa 1", self.w, self.h)];
        self.active = 0;
        self.undo.clear();
        self.redo.clear();
        self.light_mask.clear();
        self.dirty = true;
    }

    /// Abre un PNG como lienzo nuevo (una capa con la imagen). Para "editar este
    /// candidato" desde el modo Slots.
    pub fn open_file_as_canvas(&mut self, path: &Path) {
        match image::open(path) {
            Ok(img) => {
                let img = img.to_rgba8();
                let (w, h) = img.dimensions();
                self.new_canvas(w, h);
                self.layers[0].buf = img;
                self.layers[0].name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "capa 1".into());
                self.save_name = format!(
                    "{}_edit",
                    path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "editado".into())
                );
                self.fit_zoom();
                self.dirty = true;
                self.status = format!("cargado {}×{} para editar", w, h);
            }
            Err(e) => self.status = format!("no pude abrir el PNG: {e}"),
        }
    }

    /// Importa varios PNG y los auto-acomoda lado a lado (cada uno en su capa),
    /// con `pad` px de separación alineada. Resuelve el caso "esquinas en una hoja
    /// + otras en otra" → una hoja combinada sin conflicto.
    pub fn import_arranged(&mut self, paths: &[PathBuf], pad: u32) {
        let imgs: Vec<(String, RgbaImage)> = paths
            .iter()
            .filter_map(|p| {
                image::open(p).ok().map(|i| {
                    let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    (name, i.to_rgba8())
                })
            })
            .collect();
        if imgs.is_empty() {
            self.status = "ningún PNG legible para importar".into();
            return;
        }
        let total_w: u32 = imgs.iter().map(|(_, i)| i.width()).sum::<u32>() + pad * (imgs.len() as u32 - 1);
        let max_h: u32 = imgs.iter().map(|(_, i)| i.height()).max().unwrap_or(1);
        self.new_canvas(total_w, max_h);
        self.layers.clear();
        let mut xoff = 0i32;
        for (name, img) in &imgs {
            let mut layer = Layer::new(name.clone(), total_w, max_h);
            blit_over(&mut layer.buf, img, xoff, 0);
            self.layers.push(layer);
            xoff += img.width() as i32 + pad as i32;
        }
        self.active = 0;
        self.fit_zoom();
        self.dirty = true;
        self.status = format!("{} PNG importados y acomodados en {}×{}", imgs.len(), total_w, max_h);
    }

    /// Dibuja todo el Lab. Devuelve una acción si el usuario pidió "usar en slot".
    pub fn ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, game_assets: &str) -> Option<LabAction> {
        let mut action = None;
        self.toolbar(ui, game_assets, &mut action);
        ui.separator();
        // Layout: lienzo a la izquierda (expandible), capas a la derecha.
        let avail = ui.available_size();
        ui.horizontal_top(|ui| {
            let canvas_w = (avail.x - 230.0).max(200.0);
            ui.allocate_ui(egui::vec2(canvas_w, avail.y.max(200.0)), |ui| {
                self.canvas(ctx, ui);
            });
            ui.separator();
            ui.allocate_ui(egui::vec2(210.0, avail.y.max(200.0)), |ui| {
                self.layers_panel(ui);
            });
        });
        action
    }

    // ── UI interna ───────────────────────────────────────────────────────────

    fn toolbar(&mut self, ui: &mut egui::Ui, game_assets: &str, action: &mut Option<LabAction>) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("🆕 Lienzo").on_hover_text("lienzo vacío nuevo").clicked() {
                let (w, h) = (self.w, self.h);
                self.push_undo();
                self.new_canvas(w, h);
            }
            if ui.button("📥 Importar PNG(s)").on_hover_text("uno o varios; se acomodan lado a lado").clicked() {
                if let Some(files) = rfd::FileDialog::new().add_filter("PNG", &["png"]).pick_files() {
                    if files.len() == 1 {
                        // uno solo → como capa nueva sobre el lienzo actual
                        self.push_undo();
                        self.import_as_layer(&files[0]);
                    } else {
                        self.import_arranged(&files, self.grid.max(1));
                    }
                }
            }
            ui.separator();
            // Tamaño del lienzo (recorta/expande manteniendo el contenido arriba-izq).
            let (mut w, mut h) = (self.w, self.h);
            ui.label("lienzo");
            let cw = ui.add(egui::DragValue::new(&mut w).range(1..=4096).prefix("w "));
            let ch = ui.add(egui::DragValue::new(&mut h).range(1..=4096).prefix("h "));
            if cw.changed() || ch.changed() {
                self.resize_canvas(w, h);
            }
            ui.separator();
            ui.label("guardar como");
            ui.add(egui::TextEdit::singleline(&mut self.save_name).desired_width(140.0).hint_text("editado"));
            if ui.button("💾 Guardar PNG").on_hover_text("aplana capas → art/edited/<nombre>.png").clicked() {
                match self.save(game_assets) {
                    Ok(p) => self.status = format!("guardado: {}", p.display()),
                    Err(e) => self.status = format!("no pude guardar: {e}"),
                }
            }
            if ui
                .button("✅ Guardar y usar en slot")
                .on_hover_text("guarda y lo deja como candidato del slot seleccionado")
                .clicked()
            {
                match self.save(game_assets) {
                    Ok(p) => {
                        self.status = format!("guardado: {}", p.display());
                        *action = Some(LabAction::UseInSlot(p));
                    }
                    Err(e) => self.status = format!("no pude guardar: {e}"),
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            // Herramientas.
            for (t, label, hint) in [
                (Tool::Brush, "🖌 pincel", "pinta con el color"),
                (Tool::Eraser, "🩹 goma", "borra a transparente"),
                (Tool::Eyedropper, "🎯 cuentagotas", "toma el color de un píxel"),
                (Tool::Bucket, "🪣 balde", "rellena una zona (tolerancia)"),
                (Tool::Pan, "✋ mover vista", "arrastra para desplazar el lienzo"),
                (
                    Tool::Light,
                    "💡 luz",
                    "pinta QUÉ parte del sprite emite luz (no toca el PNG) — de ahí salen foco, radio y color",
                ),
            ] {
                if ui.selectable_label(self.tool == t, label).on_hover_text(hint).clicked() {
                    self.tool = t;
                }
            }
            ui.separator();
            ui.color_edit_button_srgba(&mut self.brush);
            // Las dos formas silenciosas de "pinto y no pasa nada": la capa
            // activa está oculta, o el color es transparente. Se avisan acá
            // mismo en vez de dejarte creyendo que el pincel está roto.
            let capa_oculta = self.layers.get(self.active).is_some_and(|l| !l.visible);
            if capa_oculta || self.brush.a() == 0 {
                ui.label(
                    egui::RichText::new(if capa_oculta {
                        "la capa activa está OCULTA: pintás y no se ve"
                    } else {
                        "el color es transparente (alfa 0)"
                    })
                    .color(egui::Color32::from_rgb(235, 190, 40))
                    .small(),
                );
            }
            ui.label("tam");
            ui.add(egui::DragValue::new(&mut self.brush_size).range(1..=64));
            if self.tool == Tool::Bucket {
                ui.label("tol");
                ui.add(egui::DragValue::new(&mut self.tol).range(0..=255));
            }
            ui.separator();
            ui.label("zoom");
            ui.add(egui::DragValue::new(&mut self.zoom).range(0.25..=64.0).speed(0.25));
            ui.checkbox(&mut self.show_grid, "grilla");
            ui.add(egui::DragValue::new(&mut self.grid).range(1..=256).prefix("px "));
            ui.separator();
            ui.label("frames").on_hover_text("guías para alinear tiles animados (0 = sin guía)");
            ui.add(egui::DragValue::new(&mut self.frame_cols).range(0..=64).prefix("col "));
            ui.add(egui::DragValue::new(&mut self.frame_rows).range(0..=64).prefix("fil "));
            ui.separator();
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("↶ deshacer")).clicked() {
                self.undo_action();
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("↷ rehacer")).clicked() {
                self.redo_action();
            }
        });
        // Fila del pincel de LUZ: sólo aparece cuando estás usándolo o ya pintaste.
        let painted = self.light_mask.iter().filter(|m| **m).count();
        if self.tool == Tool::Light || painted > 0 {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("💡 luz:").strong());
                match self.derived_emission() {
                    Some(em) => {
                        ui.label(
                            egui::RichText::new(format!(
                                "{painted} px · radio {:.1} m · foco ({:+.2}, {:+.2}) m",
                                em.radius, em.offset[0], em.offset[1]
                            ))
                            .weak()
                            .small(),
                        );
                        let [r, g, b] = em.color;
                        let mut c = [r, g, b];
                        ui.add_enabled(false, |ui: &mut egui::Ui| ui.color_edit_button_rgb(&mut c));
                        if ui
                            .button("✅ Aplicar luz al slot")
                            .on_hover_text(
                                "guarda esta emisión en el slot seleccionado del modo Slots \
                                 (podés retocarla después en su panel 💡)",
                            )
                            .clicked()
                        {
                            *action = Some(LabAction::SetEmission(em));
                            self.status = format!("luz derivada de {painted} px enviada al slot");
                        }
                    }
                    None => {
                        ui.label(
                            egui::RichText::new(
                                "pintá sobre las partes brillantes del sprite (la llama, la ventana, \
                                 el cristal). El PNG no se modifica.",
                            )
                            .weak()
                            .small(),
                        );
                    }
                }
                if painted > 0 && ui.small_button("🧽 limpiar").clicked() {
                    self.light_mask.clear();
                }
                ui.label(
                    egui::RichText::new(format!("(escala: {} px = 1 m — la 'grilla px')", self.grid.max(1)))
                        .weak()
                        .small(),
                );
            });
        }
        if !self.status.is_empty() {
            ui.label(egui::RichText::new(&self.status).weak().small());
        }
    }

    fn layers_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Capas");
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("➕").on_hover_text("capa vacía nueva").clicked() {
                self.push_undo();
                let n = self.layers.len() + 1;
                self.layers.push(Layer::new(format!("capa {n}"), self.w, self.h));
                self.active = self.layers.len() - 1;
                self.dirty = true;
            }
            if ui.small_button("⧉").on_hover_text("duplicar capa activa").clicked() {
                self.push_undo();
                self.duplicate_active();
            }
            if ui.small_button("🗑").on_hover_text("borrar capa activa").clicked() && self.layers.len() > 1 {
                self.push_undo();
                self.layers.remove(self.active);
                self.active = self.active.min(self.layers.len() - 1);
                self.dirty = true;
            }
            if ui.small_button("⬆").on_hover_text("subir en la pila").clicked() && self.active + 1 < self.layers.len() {
                self.push_undo();
                self.layers.swap(self.active, self.active + 1);
                self.active += 1;
                self.dirty = true;
            }
            if ui.small_button("⬇").on_hover_text("bajar en la pila").clicked() && self.active > 0 {
                self.push_undo();
                self.layers.swap(self.active, self.active - 1);
                self.active -= 1;
                self.dirty = true;
            }
        });
        ui.separator();
        // Mover el CONTENIDO de la capa activa (para alinear un PNG importado).
        ui.horizontal(|ui| {
            ui.label("mover capa:");
            let d = self.grid.max(1) as i32;
            if ui.small_button("◀").clicked() { self.push_undo(); self.shift_active(-d, 0); }
            if ui.small_button("▶").clicked() { self.push_undo(); self.shift_active(d, 0); }
            if ui.small_button("▲").clicked() { self.push_undo(); self.shift_active(0, -d); }
            if ui.small_button("▼").clicked() { self.push_undo(); self.shift_active(0, d); }
            ui.label(egui::RichText::new(format!("±{d}px")).weak().small());
        });
        ui.separator();
        let mut select: Option<usize> = None;
        // De arriba (última en la pila) hacia abajo, como en editores comunes.
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for i in (0..self.layers.len()).rev() {
                ui.horizontal(|ui| {
                    let vis = self.layers[i].visible;
                    if ui.selectable_label(vis, if vis { "👁" } else { "—" }).clicked() {
                        self.layers[i].visible = !vis;
                        self.dirty = true;
                    }
                    let sel = self.active == i;
                    if ui.selectable_label(sel, &self.layers[i].name).clicked() {
                        select = Some(i);
                    }
                });
                let mut op = self.layers[i].opacity;
                if ui.add(egui::Slider::new(&mut op, 0.0..=1.0).text("op").show_value(false)).changed() {
                    self.layers[i].opacity = op;
                    self.dirty = true;
                }
            }
        });
        if let Some(i) = select {
            self.active = i;
        }
    }

    fn canvas(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if self.dirty || self.tex.is_none() {
            let composed = self.composite();
            let ci = egui::ColorImage::from_rgba_unmultiplied(
                [self.w as usize, self.h as usize],
                composed.as_raw(),
            );
            self.tex = Some(ctx.load_texture("imglab_canvas", ci, egui::TextureOptions::NEAREST));
            self.dirty = false;
        }
        let Some(tex) = self.tex.clone() else { return };
        let size = ui.available_size();
        let (resp, painter) = ui.allocate_painter(size, egui::Sense::click_and_drag());
        let rect = resp.rect;
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(28));

        // Zoom con rueda (sobre el lienzo).
        if resp.hovered() {
            let scroll = ctx.input(|i| i.raw_scroll_delta.y);
            if scroll.abs() > 0.5 {
                let factor = if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 };
                self.zoom = (self.zoom * factor).clamp(0.25, 64.0);
            }
        }
        let z = self.zoom.max(0.05);
        let img_origin = rect.min + self.pan;
        let img_rect = egui::Rect::from_min_size(img_origin, egui::vec2(self.w as f32 * z, self.h as f32 * z));

        // Fondo ajedrez SÓLO bajo la imagen (para ver transparencia).
        self.paint_checker(&painter, img_rect, rect);
        painter.image(
            tex.id(),
            img_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        // Grilla + guías de frames.
        self.paint_grid(&painter, img_rect, z);

        // Coordenada de imagen bajo el puntero.
        let to_img = |p: egui::Pos2| -> (i32, i32) {
            (((p.x - img_origin.x) / z).floor() as i32, ((p.y - img_origin.y) / z).floor() as i32)
        };

        match self.tool {
            Tool::Pan => {
                if resp.dragged() {
                    self.pan += resp.drag_delta();
                }
            }
            Tool::Eyedropper => {
                if resp.clicked() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        let (x, y) = to_img(p);
                        if let Some(c) = self.pixel_at(x, y) {
                            self.brush = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
                        }
                    }
                }
            }
            Tool::Bucket => {
                if resp.clicked() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        let (x, y) = to_img(p);
                        if x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
                            self.push_undo();
                            let col = self.brush.to_array();
                            flood_fill(&mut self.layers[self.active].buf, x as u32, y as u32, Rgba(col), self.tol);
                            self.dirty = true;
                        }
                    }
                }
            }
            Tool::Brush | Tool::Eraser => {
                // El trazo arranca en el frame en que APRETÁS: esperar a que
                // egui declare el arrastre se come el primer píxel (y con él la
                // sensación de que el pincel no responde).
                let apretado =
                    resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down());
                if (apretado || resp.clicked()) && !self.stroke_open {
                    self.push_undo();
                    self.stroke_open = true;
                    self.last_paint = None;
                }
                if apretado || resp.clicked() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        let (x, y) = to_img(p);
                        self.paint_stroke_to(x, y);
                    }
                }
                if !apretado {
                    self.stroke_open = false;
                    self.last_paint = None;
                }
            }
            Tool::Light => {
                // Pinta la MÁSCARA, no los píxeles. Sin undo: es un marcado
                // reversible con 🧽 limpiar y no altera el PNG.
                if resp.dragged() || resp.clicked() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        let (x, y) = to_img(p);
                        let erase = ui.input(|i| i.modifiers.shift);
                        self.mark_light(x, y, !erase);
                    }
                }
                if resp.drag_stopped() {
                    self.last_paint = None;
                }
            }
        }
        // Overlay de la mancha de luz: amarillo translúcido sobre lo marcado.
        if !self.light_mask.is_empty() && (self.tool == Tool::Light || self.light_mask.iter().any(|m| *m)) {
            let tint = egui::Color32::from_rgba_unmultiplied(255, 220, 90, 90);
            for y in 0..self.h {
                for x in 0..self.w {
                    if !self.light_mask[(y * self.w + x) as usize] {
                        continue;
                    }
                    let p = img_origin + egui::vec2(x as f32 * z, y as f32 * z);
                    painter.rect_filled(egui::Rect::from_min_size(p, egui::vec2(z, z)), 0.0, tint);
                }
            }
        }
        ui.ctx().request_repaint(); // seguir animando/interactuando fluido
    }

    // ── pintura ────────────────────────────────────────────────────────────

    fn paint_stroke_to(&mut self, x: i32, y: i32) {
        let erase = self.tool == Tool::Eraser;
        let col = if erase { Rgba([0, 0, 0, 0]) } else { Rgba(self.brush.to_array()) };
        let size = self.brush_size.max(1);
        let buf = &mut self.layers[self.active].buf;
        match self.last_paint {
            // Interpola entre el último punto y el actual (sin huecos al arrastrar).
            Some((lx, ly)) => {
                let steps = (lx - x).abs().max((ly - y).abs()).max(1);
                for i in 0..=steps {
                    let t = i as f32 / steps as f32;
                    let px = (lx as f32 + (x - lx) as f32 * t).round() as i32;
                    let py = (ly as f32 + (y - ly) as f32 * t).round() as i32;
                    stamp(buf, px, py, size, col, erase);
                }
            }
            None => stamp(buf, x, y, size, col, erase),
        }
        self.last_paint = Some((x, y));
        self.dirty = true;
    }

    // ── pincel de LUZ ────────────────────────────────────────────────────────

    /// Marca (o desmarca con Shift) un disco de la máscara de luz. Interpola con
    /// el punto anterior para que arrastrar no deje huecos, igual que el pincel.
    fn mark_light(&mut self, x: i32, y: i32, on: bool) {
        if self.light_mask.len() != (self.w * self.h) as usize {
            self.light_mask = vec![false; (self.w * self.h) as usize];
        }
        let r = self.brush_size.max(1);
        let (w, h) = (self.w as i32, self.h as i32);
        let dot = |cx: i32, cy: i32, mask: &mut Vec<bool>| {
            let half = r / 2;
            for dy in -half..=(r - 1 - half) {
                for dx in -half..=(r - 1 - half) {
                    let (px, py) = (cx + dx, cy + dy);
                    if px >= 0 && py >= 0 && px < w && py < h {
                        mask[(py * w + px) as usize] = on;
                    }
                }
            }
        };
        match self.last_paint {
            Some((lx, ly)) => {
                let steps = (lx - x).abs().max((ly - y).abs()).max(1);
                for i in 0..=steps {
                    let t = i as f32 / steps as f32;
                    let px = (lx as f32 + (x - lx) as f32 * t).round() as i32;
                    let py = (ly as f32 + (y - ly) as f32 * t).round() as i32;
                    dot(px, py, &mut self.light_mask);
                }
            }
            None => dot(x, y, &mut self.light_mask),
        }
        self.last_paint = Some((x, y));
    }

    /// Traduce la mancha pintada a una [`ha_assets::Emission`]:
    ///
    /// - **foco** = centroide de lo marcado, relativo al centro del lienzo;
    /// - **radio** = extensión de la mancha × 2.4 (el aura se derrama más allá de
    ///   la fuente: una llama de 6 px ilumina bastante más que 6 px);
    /// - **color** = promedio de los píxeles opacos debajo de la mancha (si
    ///   pintaste sobre transparente, cae al color del pincel);
    /// - **escala** = la "grilla px" de la barra (por defecto 16 px = 1 tile = 1 m).
    ///
    /// `None` si no pintaste nada.
    pub fn derived_emission(&self) -> Option<ha_assets::Emission> {
        if self.light_mask.len() != (self.w * self.h) as usize {
            return None;
        }
        let comp = self.composite();
        let (mut sx, mut sy, mut n) = (0.0f64, 0.0f64, 0u32);
        let (mut cr, mut cg, mut cb, mut ca) = (0.0f64, 0.0f64, 0.0f64, 0u32);
        for y in 0..self.h {
            for x in 0..self.w {
                if !self.light_mask[(y * self.w + x) as usize] {
                    continue;
                }
                sx += x as f64 + 0.5;
                sy += y as f64 + 0.5;
                n += 1;
                let p = comp.get_pixel(x, y).0;
                if p[3] > 8 {
                    cr += p[0] as f64;
                    cg += p[1] as f64;
                    cb += p[2] as f64;
                    ca += 1;
                }
            }
        }
        if n == 0 {
            return None;
        }
        let (cx, cy) = (sx / n as f64, sy / n as f64);
        // Extensión: distancia máxima del centroide a un píxel marcado.
        let mut max_d = 0.0f64;
        for y in 0..self.h {
            for x in 0..self.w {
                if !self.light_mask[(y * self.w + x) as usize] {
                    continue;
                }
                let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
                max_d = max_d.max((dx * dx + dy * dy).sqrt());
            }
        }
        let ppm = self.grid.max(1) as f64; // píxeles por metro
        let color = if ca > 0 {
            [
                (cr / ca as f64 / 255.0) as f32,
                (cg / ca as f64 / 255.0) as f32,
                (cb / ca as f64 / 255.0) as f32,
            ]
        } else {
            let c = self.brush.to_array();
            [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]
        };
        Some(ha_assets::Emission {
            color,
            radius: (((max_d + 1.0) * 2.4) / ppm).clamp(0.5, 24.0) as f32,
            intensity: 1.0,
            // y de imagen crece hacia ABAJO; el mundo hacia arriba → se invierte.
            offset: [
                ((cx - self.w as f64 * 0.5) / ppm) as f32,
                ((self.h as f64 * 0.5 - cy) / ppm) as f32,
            ],
            flicker: 0.0,
            day: false,
        })
    }

    fn pixel_at(&self, x: i32, y: i32) -> Option<[u8; 4]> {
        if x < 0 || y < 0 || x as u32 >= self.w || y as u32 >= self.h {
            return None;
        }
        Some(self.composite().get_pixel(x as u32, y as u32).0)
    }

    // ── capas / lienzo ───────────────────────────────────────────────────────

    fn import_as_layer(&mut self, path: &Path) {
        match image::open(path) {
            Ok(img) => {
                let img = img.to_rgba8();
                let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                // Expandir el lienzo si el PNG es más grande.
                let (iw, ih) = img.dimensions();
                if iw > self.w || ih > self.h {
                    self.resize_canvas(self.w.max(iw), self.h.max(ih));
                }
                let mut layer = Layer::new(name, self.w, self.h);
                blit_over(&mut layer.buf, &img, 0, 0);
                self.layers.push(layer);
                self.active = self.layers.len() - 1;
                self.dirty = true;
                self.status = format!("importado como capa: {}×{}", iw, ih);
            }
            Err(e) => self.status = format!("no pude importar: {e}"),
        }
    }

    fn duplicate_active(&mut self) {
        let src = &self.layers[self.active];
        let dup = Layer { name: format!("{} copia", src.name), buf: src.buf.clone(), visible: src.visible, opacity: src.opacity };
        self.layers.insert(self.active + 1, dup);
        self.active += 1;
        self.dirty = true;
    }

    fn shift_active(&mut self, dx: i32, dy: i32) {
        self.layers[self.active].buf = shift(&self.layers[self.active].buf, dx, dy);
        self.dirty = true;
    }

    fn resize_canvas(&mut self, w: u32, h: u32) {
        let (w, h) = (w.max(1), h.max(1));
        if w == self.w && h == self.h {
            return;
        }
        for layer in &mut self.layers {
            let mut nb = RgbaImage::new(w, h);
            blit_over(&mut nb, &layer.buf, 0, 0);
            layer.buf = nb;
        }
        self.w = w;
        self.h = h;
        self.light_mask.clear(); // la mancha de luz era del tamaño viejo
        self.dirty = true;
    }

    fn fit_zoom(&mut self) {
        // Zoom cómodo por defecto: que un sprite chico se vea grande.
        let m = self.w.max(self.h) as f32;
        self.zoom = (512.0 / m).clamp(1.0, 32.0);
        self.pan = egui::Vec2::new(8.0, 8.0);
    }

    /// Aplana las capas visibles (con su opacidad) sobre transparente.
    fn composite(&self) -> RgbaImage {
        let mut out = RgbaImage::new(self.w, self.h);
        for layer in &self.layers {
            if !layer.visible || layer.opacity <= 0.0 {
                continue;
            }
            let op = layer.opacity.clamp(0.0, 1.0);
            for (x, y, px) in layer.buf.enumerate_pixels() {
                let mut p = *px;
                if op < 1.0 {
                    p[3] = (p[3] as f32 * op).round() as u8;
                }
                over(out.get_pixel_mut(x, y), p);
            }
        }
        out
    }

    fn save(&self, game_assets: &str) -> Result<PathBuf, String> {
        let out = self.composite();
        let name = sanitize(&self.save_name);
        let rel = format!("art/edited/{name}.png");
        let dest = PathBuf::from(game_assets).join(&rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        out.save(&dest).map_err(|e| e.to_string())?;
        Ok(dest)
    }

    // ── deshacer / rehacer ─────────────────────────────────────────────────

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            layers: self.layers.iter().map(|l| (l.name.clone(), l.buf.clone(), l.visible, l.opacity)).collect(),
            active: self.active,
            w: self.w,
            h: self.h,
        }
    }

    fn push_undo(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > 40 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore(&mut self, s: Snapshot) {
        self.w = s.w;
        self.h = s.h;
        self.active = s.active.min(s.layers.len().saturating_sub(1));
        self.layers = s
            .layers
            .into_iter()
            .map(|(name, buf, visible, opacity)| Layer { name, buf, visible, opacity })
            .collect();
        self.dirty = true;
    }

    fn undo_action(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore(s);
        }
    }

    fn redo_action(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(s);
        }
    }

    // ── overlays del canvas ──────────────────────────────────────────────────

    fn paint_checker(&self, painter: &egui::Painter, img_rect: egui::Rect, clip: egui::Rect) {
        let cell = 8.0f32.max(self.zoom * 2.0);
        let a = egui::Color32::from_gray(70);
        let b = egui::Color32::from_gray(90);
        let cols = (img_rect.width() / cell).ceil() as i32;
        let rows = (img_rect.height() / cell).ceil() as i32;
        for r in 0..rows {
            for c in 0..cols {
                let cr = egui::Rect::from_min_size(
                    img_rect.min + egui::vec2(c as f32 * cell, r as f32 * cell),
                    egui::vec2(cell, cell),
                )
                .intersect(img_rect)
                .intersect(clip);
                if cr.is_positive() {
                    painter.rect_filled(cr, 0.0, if (r + c) % 2 == 0 { a } else { b });
                }
            }
        }
    }

    fn paint_grid(&self, painter: &egui::Painter, img_rect: egui::Rect, z: f32) {
        if self.show_grid && (self.grid as f32 * z) >= 4.0 {
            let step = self.grid.max(1) as f32 * z;
            let gs = egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(120, 210, 255, 60));
            let mut x = img_rect.min.x;
            while x <= img_rect.max.x + 0.5 {
                painter.line_segment([egui::pos2(x, img_rect.min.y), egui::pos2(x, img_rect.max.y)], gs);
                x += step;
            }
            let mut y = img_rect.min.y;
            while y <= img_rect.max.y + 0.5 {
                painter.line_segment([egui::pos2(img_rect.min.x, y), egui::pos2(img_rect.max.x, y)], gs);
                y += step;
            }
        }
        // Guías de frames (más marcadas).
        let fs = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 170, 60));
        if self.frame_cols > 0 {
            let step = img_rect.width() / self.frame_cols as f32;
            for c in 0..=self.frame_cols {
                let x = img_rect.min.x + c as f32 * step;
                painter.line_segment([egui::pos2(x, img_rect.min.y), egui::pos2(x, img_rect.max.y)], fs);
            }
        }
        if self.frame_rows > 0 {
            let step = img_rect.height() / self.frame_rows as f32;
            for r in 0..=self.frame_rows {
                let y = img_rect.min.y + r as f32 * step;
                painter.line_segment([egui::pos2(img_rect.min.x, y), egui::pos2(img_rect.max.x, y)], fs);
            }
        }
    }
}

// ── helpers de imagen ─────────────────────────────────────────────────────────

/// Alpha-over de `src` sobre `dst` (premultiplicado on the fly).
fn over(dst: &mut Rgba<u8>, src: Rgba<u8>) {
    let sa = src[3] as f32 / 255.0;
    if sa <= 0.0 {
        return;
    }
    if sa >= 1.0 {
        *dst = src;
        return;
    }
    let da = dst[3] as f32 / 255.0;
    let oa = sa + da * (1.0 - sa);
    if oa <= 0.0 {
        *dst = Rgba([0, 0, 0, 0]);
        return;
    }
    for i in 0..3 {
        let s = src[i] as f32;
        let d = dst[i] as f32;
        dst[i] = ((s * sa + d * da * (1.0 - sa)) / oa).round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (oa * 255.0).round() as u8;
}

/// Blitea `src` sobre `dst` en el offset `(ox,oy)` (alpha-over, recortando bordes).
fn blit_over(dst: &mut RgbaImage, src: &RgbaImage, ox: i32, oy: i32) {
    let (dw, dh) = dst.dimensions();
    for (sx, sy, px) in src.enumerate_pixels() {
        let x = ox + sx as i32;
        let y = oy + sy as i32;
        if x >= 0 && y >= 0 && (x as u32) < dw && (y as u32) < dh {
            over(dst.get_pixel_mut(x as u32, y as u32), *px);
        }
    }
}

/// Traslada el contenido de un buffer por `(dx,dy)`, dejando transparente lo que
/// queda expuesto.
fn shift(buf: &RgbaImage, dx: i32, dy: i32) -> RgbaImage {
    let (w, h) = buf.dimensions();
    let mut out = RgbaImage::new(w, h);
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let sx = x - dx;
            let sy = y - dy;
            if sx >= 0 && sy >= 0 && (sx as u32) < w && (sy as u32) < h {
                out.put_pixel(x as u32, y as u32, *buf.get_pixel(sx as u32, sy as u32));
            }
        }
    }
    out
}

/// Pincel/goma cuadrado centrado en `(cx,cy)` de lado `size` (reemplaza el píxel).
fn stamp(buf: &mut RgbaImage, cx: i32, cy: i32, size: i32, color: Rgba<u8>, _erase: bool) {
    let r = (size / 2).max(0);
    let (w, h) = buf.dimensions();
    let (lo, hi) = if size % 2 == 0 { (0, size - 1) } else { (-r, r) };
    for oy in lo..=hi {
        for ox in lo..=hi {
            let x = cx + ox;
            let y = cy + oy;
            if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                buf.put_pixel(x as u32, y as u32, color);
            }
        }
    }
}

/// Relleno 4-conexo desde `(x,y)` con tolerancia `tol` por canal.
fn flood_fill(buf: &mut RgbaImage, x: u32, y: u32, to: Rgba<u8>, tol: i32) {
    let (w, h) = buf.dimensions();
    if x >= w || y >= h {
        return;
    }
    let from = *buf.get_pixel(x, y);
    if from == to {
        return;
    }
    let close = |a: &Rgba<u8>| -> bool {
        (a[0] as i32 - from[0] as i32).abs() <= tol
            && (a[1] as i32 - from[1] as i32).abs() <= tol
            && (a[2] as i32 - from[2] as i32).abs() <= tol
            && (a[3] as i32 - from[3] as i32).abs() <= tol
    };
    let mut stack = vec![(x, y)];
    while let Some((cx, cy)) = stack.pop() {
        if !close(buf.get_pixel(cx, cy)) {
            continue;
        }
        buf.put_pixel(cx, cy, to);
        if cx > 0 {
            stack.push((cx - 1, cy));
        }
        if cx + 1 < w {
            stack.push((cx + 1, cy));
        }
        if cy > 0 {
            stack.push((cx, cy - 1));
        }
        if cy + 1 < h {
            stack.push((cx, cy + 1));
        }
    }
}

fn sanitize(s: &str) -> String {
    let out: String = s
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c.to_ascii_lowercase() } else { '_' })
        .collect();
    if out.is_empty() {
        "editado".into()
    } else {
        out
    }
}
