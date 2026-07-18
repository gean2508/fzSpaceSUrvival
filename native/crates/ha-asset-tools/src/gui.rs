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

use ha_assets::{game_slots, list_pngs, ArtAnim, ArtManifest, CellRect, Pose, SlotDef};

pub fn run_gui() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 560.0]),
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

    // caches de textura
    tex: Option<(String, egui::TextureHandle)>,       // binding actual (como se ve en el juego)
    selected_png: Option<PathBuf>,                    // PNG fuente elegido (candidato)
    src_tex: Option<(String, egui::TextureHandle)>,   // preview del candidato

    // slicer (canvas de hoja)
    sheet_grid: u32,                  // celda px de la hoja candidata (default 32)
    sheet_zoom: f32,
    sel_rect: Option<CellRect>,       // selección actual en celdas
    drag_start_cell: Option<(u32, u32)>,
    pose_name: String,                // nombre para "guardar como pose"
    tile_material: String,            // prefijo de material para el picker de tiles (water, water_sand…)
    tile_inner_corner: bool,          // esquina cóncava (interior) vs convexa
    anim_preview_frame: usize,        // frame en curso del preview reproducido
    anim_preview_t: f32,              // acumulador de tiempo del preview

    status: String,
}

impl EditorApp {
    fn new() -> Self {
        let slots = game_slots();
        let core_keys = slots.iter().map(|s| s.key.to_string()).collect();
        // Rutas ancladas a `CARGO_MANIFEST_DIR` (constante de COMPILACIÓN) en vez
        // de relativas al cwd: si el binario se lanza desde un directorio distinto
        // al esperado (doble-click, otra terminal, cargo run desde la raíz del
        // repo…), las relativas escribían "con éxito" en un lugar que el juego
        // nunca lee — guardabas y no pasaba nada, ni en el editor ni en el juego.
        // Estas son absolutas y correctas siempre.
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")); // .../ha-asset-tools
        let native_root = manifest_dir.parent().and_then(Path::parent).unwrap_or(manifest_dir); // .../native
        let game_manifest = native_root.join("crates/ha-game/assets/art/manifest.json");
        let game_assets = native_root.join("crates/ha-game/assets");
        let catalog_path = native_root.join("assets/CATALOGO.md");
        let repo_root = native_root.parent().unwrap_or(native_root);
        let art_root = repo_root.join("PRUEBAS-CODIGOS");
        let mut app = EditorApp {
            slots,
            manifest: ArtManifest::default(),
            art_root: art_root.display().to_string(),
            game_manifest: game_manifest.display().to_string(),
            game_assets: game_assets.display().to_string(),
            catalog_path: catalog_path.display().to_string(),
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
            selected_png: None,
            src_tex: None,
            sheet_grid: 32,
            sheet_zoom: 3.0,
            sel_rect: None,
            drag_start_cell: None,
            pose_name: String::new(),
            tile_material: String::new(),
            tile_inner_corner: false,
            anim_preview_frame: 0,
            anim_preview_t: 0.0,
            status: String::new(),
        };
        app.load_manifest(None);
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
        self.right_pngs(ctx);
        self.central_detail(ctx);
    }

    // ── paneles ───────────────────────────────────────────────────────────

    fn top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Arte fuente:");
                ui.add(egui::TextEdit::singleline(&mut self.art_root).desired_width(240.0));
                if ui.button("📁 Elegir carpeta…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.art_root = dir.display().to_string();
                        self.load_pngs();
                    }
                }
                if ui.button("📂 Cargar PNGs").clicked() {
                    self.load_pngs();
                }
                ui.separator();
                if ui.button("💾 Guardar (actualiza el juego)").clicked() {
                    self.save_manifest(ctx);
                }
                if ui.button("📄 Catálogo").clicked() {
                    self.write_catalog();
                }
                if ui.button("↻ Recargar manifest").clicked() {
                    self.load_manifest(Some(ctx));
                }
            });
            ui.label(egui::RichText::new(format!("manifest del juego: {}", self.game_manifest)).weak().small());
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
            // Agrupar índices por categoría (respetando filtros) → árbol.
            let mut groups: Vec<(&'static str, Vec<usize>)> = Vec::new();
            for (i, slot) in self.slots.iter().enumerate() {
                if !filter.is_empty() && !slot.key.to_lowercase().contains(&filter) {
                    continue;
                }
                if only_unbound && self.manifest.get(slot.key).is_some() {
                    continue;
                }
                match groups.last_mut() {
                    Some((cat, v)) if *cat == slot.category => v.push(i),
                    _ => groups.push((slot.category, vec![i])),
                }
            }
            let mut clicked: Option<usize> = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (cat, idxs) in &groups {
                    let bound = idxs
                        .iter()
                        .filter(|&&i| self.manifest.get(self.slots[i].key).is_some())
                        .count();
                    let header = egui::RichText::new(format!(
                        "▸ {}  ({}/{})",
                        cat.to_uppercase(),
                        bound,
                        idxs.len()
                    ))
                    .strong()
                    .color(egui::Color32::from_rgb(120, 210, 255));
                    egui::CollapsingHeader::new(header)
                        .default_open(true)
                        .show(ui, |ui| {
                            let n = idxs.len();
                            for (row, &i) in idxs.iter().enumerate() {
                                let slot = self.slots[i];
                                let leaf = slot.key.split_once('/').map(|(_, n)| n).unwrap_or(slot.key);
                                let branch = if row + 1 == n { "└─" } else { "├─" };
                                let has = self.manifest.get(slot.key).is_some();
                                let dot = if has { "●" } else { "○" };
                                let mut txt = egui::RichText::new(format!("{branch} {dot} {leaf}")).monospace();
                                if has {
                                    txt = txt.color(egui::Color32::from_rgb(120, 230, 140));
                                }
                                let sel = self.selected == Some(i);
                                if ui.selectable_label(sel, txt).clicked() {
                                    clicked = Some(i);
                                }
                            }
                        });
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
            // TODO el contenido va en un scroll: si la ventana es chica o hay
            // mucho contenido (poses, canvas), nada queda inalcanzable/desbordado.
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            self.central_detail_content(ctx, ui, i);
            });
        });
    }

    fn central_detail_content(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, i: usize) {
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
            // Previews lado a lado, acotados (la lista de abajo mantiene scroll).
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("actual (juego)").weak());
                    match self.tex.as_ref() {
                        Some((_, tex)) => {
                            let sz = preview_size(tex, 220.0);
                            ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), sz)));
                        }
                        None => { ui.label(egui::RichText::new("— procedural —").weak()); }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("candidato").weak());
                    match self.src_tex.as_ref() {
                        Some((_, tex)) => {
                            let sz = preview_size(tex, 220.0);
                            ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), sz)));
                        }
                        None => { ui.label(egui::RichText::new("(elegí un PNG abajo)").weak()); }
                    }
                });
            });
            // Poses guardadas del slot — SIEMPRE visibles (no dependen de tener un
            // candidato elegido), con thumbnail recortado del sheet ya asignado y
            // botón para volver a cargarla en el canvas y ajustarla.
            let saved_poses: Vec<(String, Pose)> = self
                .manifest
                .poses(slot.key)
                .map(|m| m.iter().map(|(k, v)| (k.clone(), *v)).collect())
                .unwrap_or_default();
            if !saved_poses.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new(format!("poses guardadas ({})", saved_poses.len())).strong());
                let grid = self.manifest.get(slot.key).map(|v| v.grid()).unwrap_or((32, 32));
                let mut remove: Option<String> = None;
                let mut edit: Option<(String, Pose)> = None;
                egui::Grid::new("poses_grid").striped(true).show(ui, |ui| {
                    for (pn, pose) in &saved_poses {
                        if let Some((_, tex)) = self.tex.as_ref() {
                            let img = tex.size_vec2();
                            let uv = egui::Rect::from_min_max(
                                egui::pos2(
                                    (pose.rect.col * grid.0) as f32 / img.x,
                                    (pose.rect.row * grid.1) as f32 / img.y,
                                ),
                                egui::pos2(
                                    ((pose.rect.col + pose.rect.w) * grid.0) as f32 / img.x,
                                    ((pose.rect.row + pose.rect.h) * grid.1) as f32 / img.y,
                                ),
                            );
                            ui.add(
                                egui::Image::new(egui::load::SizedTexture::new(tex.id(), egui::vec2(48.0, 48.0)))
                                    .uv(uv),
                            );
                        } else {
                            ui.label("—");
                        }
                        let anim_tag = if pose.anim.is_some() { " 🎞" } else { "" };
                        ui.label(format!("{pn}{anim_tag}"));
                        if ui.small_button("✎ editar").clicked() {
                            edit = Some((pn.clone(), *pose));
                        }
                        if ui.small_button("✖").clicked() {
                            remove = Some(pn.clone());
                        }
                        ui.end_row();
                    }
                });
                if let Some(pn) = remove {
                    self.manifest.remove_pose(slot.key, &pn);
                }
                if let Some((pn, pose)) = edit {
                    self.pose_name = pn;
                    self.sel_rect = Some(pose.rect);
                    match pose.anim {
                        Some(a) => {
                            self.anim_on = true;
                            self.anim_frames = a.frames;
                            self.anim_fps = a.fps;
                        }
                        None => self.anim_on = false,
                    }
                    self.status = "pose cargada en el canvas — ajustá el recorte y volvé a Guardar".into();
                }
            }

            if let Some(png) = self.selected_png.clone() {
                let name = png.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.label(format!("candidato: {name}"));
                    if ui.button("✔ Asignar PNG entero").clicked() {
                        self.bind_png(ctx, slot, &png);
                        self.selected_png = None;
                        self.src_tex = None;
                    }
                });

                // ── Slicer: recortar la hoja en regiones / poses / animación ──
                egui::CollapsingHeader::new("✂ Recortar hoja (grilla + arrastre)")
                    .default_open(true)
                    .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("celda px");
                        ui.add(egui::DragValue::new(&mut self.sheet_grid).range(4..=256));
                        ui.label("zoom");
                        ui.add(egui::DragValue::new(&mut self.sheet_zoom).range(0.5..=12.0).speed(0.1));
                        if let Some(sr) = self.sel_rect {
                            ui.label(format!("sel: col{} row{} · {}×{} ({} celdas)", sr.col, sr.row, sr.w, sr.h, sr.cells()));
                        }
                    });
                    self.sheet_canvas(ui);
                    if let Some(sr) = self.sel_rect {
                        if sr.cells() > 1 {
                            self.anim_strip_preview(ui, sr);
                        }
                    }

                    let grid = (self.sheet_grid.max(1), self.sheet_grid.max(1));
                    if let Some(sr) = self.sel_rect {
                        ui.horizontal(|ui| {
                            if ui.button("Guardar recorte estático").clicked() {
                                if let Some(rel) = self.ensure_sheet_copied(slot, &png) {
                                    self.manifest.set_rect(slot.key, rel, grid, sr);
                                    self.tex = self.preview_bound(ctx, slot.key);
                                    self.status = format!("{}: recorte {}×{}", slot.key, sr.w, sr.h);
                                }
                            }
                            if ui.button("Guardar como animación (strip)")
                                .on_hover_text("los frames salen del rango de celdas; fps abajo")
                                .clicked()
                            {
                                if let Some(rel) = self.ensure_sheet_copied(slot, &png) {
                                    self.manifest.set_strip(slot.key, rel, grid, sr, self.anim_fps.max(0.1));
                                    self.tex = self.preview_bound(ctx, slot.key);
                                    self.status = format!("{}: animación {} frames", slot.key, sr.cells());
                                }
                            }
                        });
                        // Cruz de dirección — pensado para atlas de personaje/NPC
                        // (idle/walk × 4 direcciones), pero el nombre es libre:
                        // sirve para CUALQUIER atlas con estados (máquina abierta/
                        // cerrada, puerta, etc.) escribiendo el nombre a mano.
                        ui.horizontal(|ui| {
                            ui.label("pose:");
                            ui.add(egui::TextEdit::singleline(&mut self.pose_name).desired_width(110.0).hint_text("idle_down…"));
                        });
                        ui.horizontal(|ui| {
                            ui.label("acción:");
                            for verb in ["idle", "walk"] {
                                let active = self.pose_name.starts_with(verb);
                                if ui.selectable_label(active, verb).clicked() {
                                    let dir = pose_dir_suffix(&self.pose_name);
                                    self.pose_name = format!("{verb}_{dir}");
                                }
                            }
                            ui.separator();
                            ui.label("dirección:");
                            for (dir, arrow) in [("up", "↑"), ("left", "←"), ("down", "↓"), ("right", "→")] {
                                let active = self.pose_name.ends_with(dir);
                                if ui.selectable_label(active, arrow).clicked() {
                                    let verb = if self.pose_name.starts_with("walk") { "walk" } else { "idle" };
                                    self.pose_name = format!("{verb}_{dir}");
                                }
                            }
                        });
                        // Variantes de tile/autotile: assets que ocupan VARIOS
                        // cuadraditos (agua sola, agua+tierra por lado, agua+arena,
                        // esquinas de tierra, y sus versiones animadas). Cada
                        // variante es OTRA pose del mismo slot — no pisa a las demás.
                        egui::CollapsingHeader::new("🧩 variantes de tile / autotile (agua, bordes, esquinas…)")
                            .default_open(false)
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(
                                    "Recortá en el canvas de arriba la celda de esta variante, elegí \
                                     la forma acá, y Guardar como pose. Repetí para cada pieza \
                                     (centro, cada borde, cada esquina) — todas quedan guardadas juntas."
                                ).weak());
                                ui.horizontal(|ui| {
                                    ui.label("material:");
                                    ui.add(egui::TextEdit::singleline(&mut self.tile_material).desired_width(110.0).hint_text("water, water_sand…"));
                                    ui.checkbox(&mut self.tile_inner_corner, "esquina cóncava (interior)");
                                });
                                egui::Grid::new("tile_shapes").spacing([4.0, 4.0]).show(ui, |ui| {
                                    for row in TILE_DIRS.chunks(3) {
                                        for &(id, arrow) in row {
                                            let shape = tile_shape_name(id, self.tile_inner_corner);
                                            let full = if self.tile_material.trim().is_empty() {
                                                shape
                                            } else {
                                                format!("{}_{}", self.tile_material.trim(), shape)
                                            };
                                            let active = self.pose_name == full;
                                            if ui.selectable_label(active, arrow).on_hover_text(&full).clicked() {
                                                self.pose_name = full;
                                            }
                                        }
                                        ui.end_row();
                                    }
                                });
                            });

                        let animated = self.anim_on;
                        if ui.button("Guardar como pose").clicked() && !self.pose_name.trim().is_empty() {
                            if let Some(rel) = self.ensure_sheet_copied(slot, &png) {
                                let anim = animated.then_some(ArtAnim { frames: sr.cells().max(1), fps: self.anim_fps.max(0.1) });
                                let name = self.pose_name.trim().to_string();
                                self.manifest.set_pose(slot.key, rel, grid, &name, Pose { rect: sr, anim });
                                self.status = format!("{}: pose {name}", slot.key);
                            }
                        }
                    }
                });
            }

            ui.separator();
            // Animación.
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.anim_on, "animado (tira de frames)");
                if self.anim_on {
                    ui.label("frames");
                    ui.add(egui::DragValue::new(&mut self.anim_frames).range(1..=64));
                    ui.label("fps").on_hover_text("cuadros por segundo — velocidad de la animación");
                    ui.add(egui::DragValue::new(&mut self.anim_fps).range(0.1..=60.0))
                        .on_hover_text("cuadros por segundo — más alto = más rápido");
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
    }

    /// Panel derecho dedicado a elegir el PNG fuente. Lista **virtualizada**
    /// (`show_rows`) → rinde miles de PNGs sin trabarse y siempre visible.
    fn right_pngs(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("pngs").default_width(300.0).show(ctx, |ui| {
            ui.heading("PNGs fuente");
            ui.horizontal(|ui| {
                ui.label("🔎");
                ui.text_edit_singleline(&mut self.png_filter);
            });
            ui.label(format!("{} PNGs (click = preview)", self.pngs.len()));
            ui.separator();
            if self.pngs.is_empty() {
                ui.label(egui::RichText::new("Elegí carpeta / Cargar PNGs arriba").weak());
                return;
            }
            let pf = self.png_filter.to_lowercase();
            let filtered: Vec<&PathBuf> = self
                .pngs
                .iter()
                .filter(|p| {
                    pf.is_empty()
                        || p.file_name()
                            .map(|n| n.to_string_lossy().to_lowercase().contains(&pf))
                            .unwrap_or(false)
                })
                .collect();
            let row_h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
            let mut pick: Option<PathBuf> = None;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(
                ui,
                row_h,
                filtered.len(),
                |ui, range| {
                    for i in range {
                        let p = filtered[i];
                        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        let sel = self.selected_png.as_deref() == Some(p.as_path());
                        if ui.selectable_label(sel, name).clicked() {
                            pick = Some((*p).clone());
                        }
                    }
                },
            );
            if let Some(p) = pick {
                self.src_tex = load_texture(ctx, &p, "src_preview");
                self.selected_png = Some(p);
            }
        });
    }

    // ── acciones ───────────────────────────────────────────────────────────

    /// `ctx` presente (fuera del constructor) → también refresca la textura del
    /// slot seleccionado, para que "Recargar" se vea reflejado sin reabrir nada.
    fn load_manifest(&mut self, ctx: Option<&egui::Context>) {
        match std::fs::read_to_string(&self.game_manifest) {
            Ok(s) => match ArtManifest::from_json(&s) {
                Ok(m) => {
                    self.manifest = m;
                    self.status = format!("manifest cargado ({}): {} bindings", self.game_manifest, self.manifest.sprites.len());
                }
                Err(e) => self.status = format!("manifest ilegible: {e}"),
            },
            Err(_) => {
                self.manifest = ArtManifest::default();
                self.status = format!("manifest nuevo (no existía en {}) — se creará al guardar", self.game_manifest);
            }
        }
        // Claves del manifest que no son slots nativos → agregarlas (custom persist).
        let keys: Vec<String> = self.manifest.sprites.keys().cloned().collect();
        for k in keys {
            self.ensure_slot(&k);
        }
        if let Some(ctx) = ctx {
            if let Some(i) = self.selected {
                let key = self.slots[i].key;
                self.tex = self.preview_bound(ctx, key);
            }
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

    /// Copia la hoja fuente al árbol del juego bajo `art/atlas/<nombre>_<hash>.png`
    /// — **content-addressed**: si otro slot ya recortó de este MISMO atlas
    /// (mismos bytes), reusa el archivo en vez de duplicarlo. Esto es lo que
    /// hace viable tener un atlas de UI enorme (corazones, barras, botones,
    /// portraits…) y recortar decenas de piezas de él sin inflar el APK con
    /// una copia completa por cada pieza. Devuelve la ruta relativa a
    /// registrar en el manifest (compartida entre todos los slots que usen
    /// este mismo atlas).
    fn ensure_sheet_copied(&mut self, _slot: SlotDef, src: &Path) -> Option<String> {
        let bytes = match std::fs::read(src) {
            Ok(b) => b,
            Err(e) => {
                self.status = format!("no pude leer la hoja fuente: {e}");
                return None;
            }
        };
        let hash = fnv1a32(&bytes);
        let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "atlas".to_string());
        let rel = format!("art/atlas/{}_{hash:08x}.png", sanitize_filename(&stem));
        let dest = PathBuf::from(&self.game_assets).join(&rel);
        if dest.exists() {
            self.status = format!("atlas ya copiado, reusado: {rel}");
            return Some(rel);
        }
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                self.status = format!("no pude crear {}: {e}", parent.display());
                return None;
            }
        }
        match std::fs::write(&dest, &bytes) {
            Ok(()) => Some(rel),
            Err(e) => {
                self.status = format!("no pude copiar la hoja: {e}");
                None
            }
        }
    }

    /// Preview que REPRODUCE la animación del rango `sr` (1:1 con el juego):
    /// cicla las celdas del rango a `anim_fps`.
    fn anim_strip_preview(&mut self, ui: &mut egui::Ui, sr: CellRect) {
        let Some((_, tex)) = self.src_tex.clone() else {
            return;
        };
        let img = tex.size_vec2();
        let grid = self.sheet_grid.max(1) as f32;
        let frames = sr.cells().max(1);
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        self.anim_preview_t += dt * self.anim_fps.max(0.1);
        if self.anim_preview_t >= 1.0 {
            let steps = self.anim_preview_t.floor() as usize;
            self.anim_preview_frame = (self.anim_preview_frame + steps) % frames as usize;
            self.anim_preview_t = self.anim_preview_t.fract();
        }
        let f = (self.anim_preview_frame as u32) % frames;
        let fc = sr.col + (f % sr.w.max(1));
        let fr = sr.row + (f / sr.w.max(1));
        let uv = egui::Rect::from_min_max(
            egui::pos2(fc as f32 * grid / img.x, fr as f32 * grid / img.y),
            egui::pos2((fc as f32 + 1.0) * grid / img.x, (fr as f32 + 1.0) * grid / img.y),
        );
        ui.horizontal(|ui| {
            ui.label("▶ preview animado:");
            ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), egui::vec2(96.0, 96.0))).uv(uv));
            ui.label(format!("{} frames @ {:.0} fps", frames, self.anim_fps));
        });
        ui.ctx().request_repaint(); // sigue animando
    }

    /// Canvas de la hoja candidata: dibuja el PNG con grilla y deja seleccionar
    /// un rango de celdas arrastrando (snap a la grilla) → `self.sel_rect`.
    fn sheet_canvas(&mut self, ui: &mut egui::Ui) {
        let Some((_, tex)) = self.src_tex.clone() else {
            ui.label(egui::RichText::new("elegí un PNG abajo para recortarlo").weak());
            return;
        };
        let img_px = tex.size_vec2();
        let grid = self.sheet_grid.max(1) as f32;
        let cols = (img_px.x / grid).ceil().max(1.0) as u32;
        let rows = (img_px.y / grid).ceil().max(1.0) as u32;
        let zoom = self.sheet_zoom.max(0.25);
        let cell_px = grid * zoom;
        let disp = egui::vec2(cols as f32 * cell_px, rows as f32 * cell_px);

        egui::ScrollArea::both().id_salt("canvas").max_height(360.0).show(ui, |ui| {
            let (rect, resp) = ui.allocate_exact_size(disp, egui::Sense::click_and_drag());
            let painter = ui.painter_at(rect);
            painter.image(
                tex.id(),
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            let gs = egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(120, 210, 255, 70));
            for c in 0..=cols {
                let x = rect.min.x + c as f32 * cell_px;
                painter.line_segment([egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)], gs);
            }
            for r in 0..=rows {
                let y = rect.min.y + r as f32 * cell_px;
                painter.line_segment([egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)], gs);
            }
            let cell_of = |p: egui::Pos2| -> (u32, u32) {
                let cx = ((p.x - rect.min.x) / cell_px).floor().clamp(0.0, (cols - 1) as f32) as u32;
                let cy = ((p.y - rect.min.y) / cell_px).floor().clamp(0.0, (rows - 1) as f32) as u32;
                (cx, cy)
            };
            if resp.drag_started() {
                if let Some(p) = resp.interact_pointer_pos() {
                    self.drag_start_cell = Some(cell_of(p));
                }
            }
            if resp.dragged() || resp.drag_stopped() {
                if let (Some((sx, sy)), Some(p)) = (self.drag_start_cell, resp.interact_pointer_pos()) {
                    let (ex, ey) = cell_of(p);
                    let col = sx.min(ex);
                    let row = sy.min(ey);
                    self.sel_rect = Some(CellRect { col, row, w: sx.max(ex) - col + 1, h: sy.max(ey) - row + 1 });
                }
            }
            if resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    let (c, r) = cell_of(p);
                    self.sel_rect = Some(CellRect { col: c, row: r, w: 1, h: 1 });
                }
            }
            if let Some(sr) = self.sel_rect {
                let sel = egui::Rect::from_min_size(
                    egui::pos2(rect.min.x + sr.col as f32 * cell_px, rect.min.y + sr.row as f32 * cell_px),
                    egui::vec2(sr.w as f32 * cell_px, sr.h as f32 * cell_px),
                );
                painter.rect_filled(sel, 0.0, egui::Color32::from_rgba_unmultiplied(255, 220, 60, 40));
                painter.rect_stroke(sel, 0.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 220, 60)));
            }
        });
    }

    fn save_manifest(&mut self, ctx: &egui::Context) {
        match self.manifest.to_json() {
            Ok(json) => {
                if let Some(parent) = PathBuf::from(&self.game_manifest).parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::write(&self.game_manifest, &json) {
                    Ok(()) => {
                        // Round-trip: releo del disco lo que acabo de escribir, en
                        // vez de confiar en el estado en memoria — así "guardado"
                        // significa "esto es literalmente lo que hay en el archivo",
                        // y refresco la textura del slot actual para verlo YA en
                        // el editor (no hace falta reabrir).
                        match std::fs::read_to_string(&self.game_manifest).ok().map(|s| ArtManifest::from_json(&s)) {
                            Some(Ok(m)) => {
                                self.manifest = m;
                                if let Some(i) = self.selected {
                                    let key = self.slots[i].key;
                                    self.tex = self.preview_bound(ctx, key);
                                }
                                self.status = format!(
                                    "✔ guardado y verificado en {} — rebuildeá el juego (./editor_assets play) para verlo ahí",
                                    self.game_manifest
                                );
                            }
                            _ => {
                                self.status = format!(
                                    "⚠ escribí {} pero no pude releerlo para verificar",
                                    self.game_manifest
                                );
                            }
                        }
                    }
                    Err(e) => self.status = format!("✘ no pude escribir {}: {e}", self.game_manifest),
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

/// Grilla 3×3 del picker de tiles/autotile — el layout visual replica la
/// posición de la tierra/vecino respecto al agua (centro = agua sola).
const TILE_DIRS: [(&str, &str); 9] = [
    ("nw", "↖"), ("n", "↑"), ("ne", "↗"),
    ("w", "←"), ("c", "●"), ("e", "→"),
    ("sw", "↙"), ("s", "↓"), ("se", "↘"),
];

/// Nombre de forma para el picker de tiles: centro/borde/esquina (convexa o,
/// con `inner`, cóncava — para el caso "agua envuelve una esquina de tierra").
fn tile_shape_name(id: &str, inner: bool) -> String {
    match id {
        "c" => "center".to_string(),
        "n" | "s" | "e" | "w" => format!("edge_{id}"),
        _ if inner => format!("innercorner_{id}"),
        _ => format!("corner_{id}"),
    }
}

/// Dirección al final de un nombre de pose ("walk_left" → "left"); "down" si
/// no matchea ninguna de las 4 — usado por la cruz de dirección del slicer.
fn pose_dir_suffix(name: &str) -> &'static str {
    for d in ["down", "up", "left", "right"] {
        if name.ends_with(d) {
            return d;
        }
    }
    "down"
}

/// Escala la textura para que su lado mayor sea `max_px` (preserva aspecto), así
/// un preview nunca se come toda el área y la lista de abajo mantiene scroll.
fn preview_size(tex: &egui::TextureHandle, max_px: f32) -> egui::Vec2 {
    let s = tex.size_vec2();
    let m = s.x.max(s.y).max(1.0);
    s * (max_px / m)
}

/// Los slots derivados en runtime necesitan `&'static str` y se crean pocas
/// veces (abrir manifest / añadir slot), así que filtrar la cadena es aceptable.
/// FNV-1a de 32 bits sobre los bytes del PNG fuente — hash de contenido para
/// el dedupe de atlas (mismo archivo → mismo destino, sin dependencias extra).
fn fnv1a32(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

fn sanitize_filename(s: &str) -> String {
    let out: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    if out.is_empty() {
        "atlas".to_string()
    } else {
        out
    }
}

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
