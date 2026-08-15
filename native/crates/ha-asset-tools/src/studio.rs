//! **HA Studio** — UN solo programa con todos los editores del juego, en
//! *espacios de trabajo* al estilo Blender (Layout / Sculpting / Shading…): la
//! misma ventana cambia de herramienta, no de aplicación.
//!
//! ```text
//! ┌ 🧩 Assets │ 🖌 Imagen │ 🗺 Mundo │ 🔊 Sonido │ 🧬 Fauna ┐   F1…F5
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! Por qué en un solo programa: todos editan el MISMO juego y se pisan los datos.
//! Assets escribe `art/manifest.json`; Mundo lo lee para su paleta; Sonido escribe
//! `audio.json`; Fauna evalúa los `.adn`. Tenerlos en ventanas separadas obligaba a
//! cerrar/abrir y a "recargar" a mano. Acá el cambio de espacio **sincroniza solo**
//! (al entrar a Mundo se relee el manifest de assets, así el asset que acabás de
//! crear ya está en la paleta).
//!
//! Cada espacio se construye la PRIMERA vez que lo abrís (arrancar el Studio no
//! paga el coste de leer el mundo, los sonidos y los genomas si sólo venías a
//! recortar un sprite).

use std::process::ExitCode;

use crate::gui::EditorApp;
use crate::gym_gui::GymApp;
use crate::sound_gui::SoundApp;
use crate::world_gui::WorldApp;

/// Espacio de trabajo activo. Uno por herramienta del juego.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Space {
    Assets,
    Lab,
    Mundo,
    Sonido,
    Fauna,
}

impl Space {
    const ALL: [Space; 5] = [Space::Assets, Space::Lab, Space::Mundo, Space::Sonido, Space::Fauna];

    /// Etiqueta de la pestaña (icono + nombre) y tecla de función que la abre.
    fn tab(self) -> (&'static str, &'static str) {
        match self {
            Space::Assets => ("🧩 Assets", "F1"),
            Space::Lab => ("🖌 Imagen", "F2"),
            Space::Mundo => ("🗺 Mundo", "F3"),
            Space::Sonido => ("🔊 Sonido", "F4"),
            Space::Fauna => ("🧬 Fauna", "F5"),
        }
    }

    /// Una línea de qué hace este espacio y qué archivo del juego escribe — que el
    /// programa se explique solo es más útil que cualquier manual aparte.
    fn hint(self) -> &'static str {
        match self {
            Space::Assets => "Objetos del juego y su arte · variantes por material · escribe art/manifest.json",
            Space::Lab => "Compositor multi-PNG + píxeles + pincel de LUZ · su salida vuelve como candidato del objeto",
            Space::Mundo => "Mapa WYSIWYG: pintás terreno y plantás tus assets · escribe world_edits.json",
            Space::Sonido => "Mini-DAW procedural: cada sonido del juego · escribe assets/audio.json",
            Space::Fauna => "ADN de la fauna/NPC: evaluás, mutás y cruzás genomas .adn",
        }
    }

    fn key(self) -> egui::Key {
        match self {
            Space::Assets => egui::Key::F1,
            Space::Lab => egui::Key::F2,
            Space::Mundo => egui::Key::F3,
            Space::Sonido => egui::Key::F4,
            Space::Fauna => egui::Key::F5,
        }
    }
}

pub fn run_studio() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1520.0, 940.0])
            .with_min_inner_size([960.0, 600.0]),
        ..Default::default()
    };
    let res = eframe::run_native(
        "HA Studio — assets · mundo · sonido · fauna",
        options,
        Box::new(|cc| {
            style(&cc.egui_ctx);
            Ok(Box::new(Studio::new()))
        }),
    );
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("no pude abrir la ventana (¿hay display?): {e}");
            ExitCode::FAILURE
        }
    }
}

/// Un poco de aire y esquinas redondeadas: los editores son densos (grillas,
/// listas largas, sliders) y el default de egui los pega demasiado.
fn style(ctx: &egui::Context) {
    let mut s = (*ctx.style()).clone();
    s.spacing.item_spacing = egui::vec2(8.0, 6.0);
    s.spacing.button_padding = egui::vec2(8.0, 4.0);
    for w in [
        &mut s.visuals.widgets.noninteractive,
        &mut s.visuals.widgets.inactive,
        &mut s.visuals.widgets.hovered,
        &mut s.visuals.widgets.active,
    ] {
        w.rounding = egui::Rounding::same(4.0);
    }
    s.visuals.selection.bg_fill = egui::Color32::from_rgb(38, 92, 140);
    ctx.set_style(s);
}

struct Studio {
    space: Space,
    /// El editor de assets es el corazón del Studio (y el único que ya trae el
    /// Image Lab adentro) → se crea siempre.
    assets: EditorApp,
    // Los demás, la primera vez que se abren.
    world: Option<WorldApp>,
    sound: Option<SoundApp>,
    gym: Option<GymApp>,
}

impl Studio {
    fn new() -> Self {
        Self { space: Space::Assets, assets: EditorApp::new(), world: None, sound: None, gym: None }
    }

    /// Cambia de espacio y **sincroniza lo que comparten**: entrar a Mundo relee
    /// `art/manifest.json`, así los objetos/variantes que acabás de editar en
    /// Assets ya aparecen en la paleta del mapa sin apretar "↻ assets".
    fn go(&mut self, space: Space) {
        self.space = space;
        match space {
            Space::Assets => self.assets.focus_slots(),
            Space::Lab => self.assets.focus_lab(),
            Space::Mundo => {
                let world = self.world.get_or_insert_with(WorldApp::new);
                world.reload_art();
            }
            Space::Sonido => {
                self.sound.get_or_insert_with(SoundApp::new);
            }
            Space::Fauna => {
                self.gym.get_or_insert_with(GymApp::new);
            }
        }
    }

    /// Barra de espacios de trabajo: siempre visible, siempre en el mismo lugar.
    fn tab_bar(&mut self, ctx: &egui::Context) {
        let mut pending = None;
        egui::TopBottomPanel::top("studio_spaces").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                for space in Space::ALL {
                    let (label, key) = space.tab();
                    // El Lab es un modo del editor de assets: la pestaña sigue su
                    // estado real para que nunca queden dos pestañas "activas".
                    let active = match space {
                        Space::Assets => self.space == Space::Assets && !self.assets.in_lab(),
                        Space::Lab => self.space == Space::Lab || self.assets.in_lab(),
                        s => self.space == s,
                    };
                    let txt = egui::RichText::new(label).size(15.0);
                    if ui
                        .selectable_label(active, txt)
                        .on_hover_text(format!("{}\n\n{}", key, space.hint()))
                        .clicked()
                    {
                        pending = Some(space);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("F1…F5 cambian de espacio")
                            .weak()
                            .small(),
                    );
                });
            });
            ui.label(egui::RichText::new(self.space.hint()).weak().small());
            ui.add_space(2.0);
        });
        // Atajos: sólo si no estás tipeando en un campo de texto.
        let typing = ctx.memory(|m| m.focused().is_some());
        if !typing {
            for space in Space::ALL {
                if ctx.input(|i| i.key_pressed(space.key())) {
                    pending = Some(space);
                }
            }
        }
        if let Some(space) = pending {
            self.go(space);
        }
    }
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tab_bar(ctx);
        match self.space {
            // Assets y Lab son el mismo editor (el Lab es su segundo modo): un solo
            // `ui` para los dos, así "usar en objeto" sigue funcionando igual.
            Space::Assets | Space::Lab => self.assets.ui(ctx),
            Space::Mundo => {
                if let Some(w) = self.world.as_mut() {
                    w.ui(ctx);
                }
            }
            Space::Sonido => {
                if let Some(s) = self.sound.as_mut() {
                    s.ui(ctx);
                }
            }
            Space::Fauna => {
                if let Some(g) = self.gym.as_mut() {
                    g.ui(ctx);
                }
            }
        }
    }
}
