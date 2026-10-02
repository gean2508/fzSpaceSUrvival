//! `ha-asset-tools` — HA Studio: los editores del juego (assets, imagen, mundo,
//! sonido, fauna) más un par de comandos de horneado.
//!
//! Sin argumentos abre el Studio. Ver `biblia.md` §8.
//!
//! Los subcomandos `scan` y `catalog` se borraron (31 jul 2026): generaban
//! `manifest.ron` y `CATALOGO.md`, un sistema paralelo que nunca se usó (0
//! assets). El pixel se cura a mano en el editor y la verdad vive en
//! `crates/ha-game/assets/art/manifest.json`.

use std::path::PathBuf;
use std::process::ExitCode;

mod chargen;
mod gui;
mod gym;
mod gym_gui;
mod imglab;
mod slot_audit;
mod sound_gui;
mod studio;
mod world_gui;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        // `studio` = TODOS los editores en una sola ventana (espacios de trabajo
        // al estilo Blender). Es el default: los subcomandos de abajo siguen
        // abriendo cada editor solo, por si querés dos ventanas a la vez.
        Some("studio") => studio::run_studio(),
        Some("gui") => gui::run_gui(),
        Some("sound") => sound_gui::run_sound_gui(),
        Some("mundo") => world_gui::run_world_gui(),
        Some("gym") => gym::run_gym(),
        Some("gym-medir") => gym::run_medir(),
        Some("gym-gui") => gym_gui::run_gym_gui(),
        Some("chargen") => cmd_chargen(&args[1..]),
        Some("sound-init") => cmd_sound_init(&args[1..]),
        Some("-h") | Some("--help") => {
            print_usage();
            ExitCode::SUCCESS
        }
        // Sin argumentos: abrí el Studio (lo que el usuario quiere el 99% de las
        // veces). La ayuda sigue a un `--help` de distancia.
        None => studio::run_studio(),
        Some(other) => {
            eprintln!("subcomando desconocido: {other}\n");
            print_usage();
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    eprintln!(
        "ha-asset-tools — HA Studio, los editores del juego\n\n\
         COMANDOS:\n\
         \x20 studio    # TODO EN UNO: assets + imagen + mundo + sonido + fauna (default)\n\
         \x20 gui       # sólo el editor de assets/objetos (ventana aparte)\n\
         \x20 sound     # editor de sonido / mini-DAW (edita assets/audio.json)\n\
         \x20 mundo     # editor de MUNDO WYSIWYG (H1: vista del mundo procedural)\n\
         \x20 gym       # entrena la fauna por neuroevolución (headless, CLI)\n\
         \x20 gym-medir # mide el FAUNA.adn instalado contra la vara (persecución pura) en mundos nuevos\n\
         \x20 gym-gui   # visor de ADN: lista genomas .adn, evalúa/muta/cruza con preview\n\
         \x20 chargen [--root <pack>] [--out <art/>] [--npcs 20] [--seed 7] [--scale 2] [--player]\n\
         \x20           # hornea personajes por CAPAS (pack LimeZu Character_Generator)\n\
         \x20 chargen --capas [--ui-root <pack Modern UI>]\n\
         \x20           # ademas exporta las capas recortadas + retratos al juego\n\
         \x20           # (es lo que come el creador de personaje y el menu de NPC)\n\
         \x20 sound-init [--force]  # (re)genera assets/audio.json con el diseño default"
    );
}

/// `chargen` — hornea personajes con el generador por capas de LimeZu
/// (`PRUEBAS-CODIGOS/Character_Generator`). Ver `chargen.rs`.
fn cmd_chargen(args: &[String]) -> ExitCode {
    let mut root = PathBuf::from("PRUEBAS-CODIGOS/Character_Generator");
    let mut out = PathBuf::from("native/crates/ha-game/assets/art");
    let mut ui_root =
        PathBuf::from("PRUEBAS-CODIGOS/modernuserinterface-win/32x32/Portrait_Generator_32x32");
    // `scale` 1 = sin downscale. Era 2 (media escala, celda 16×32) y ahora todo
    // el arte del juego es el del pack a 32×32: la celda es 32×64.
    let (mut npcs, mut seed, mut scale, mut player) = (20usize, 7u64, 1u32, false);
    let mut capas = false;
    let mut i = 0;
    while i < args.len() {
        let val = args.get(i + 1);
        match args[i].as_str() {
            "--root" => { root = val.map(PathBuf::from).unwrap_or(root); i += 2; }
            "--out" => { out = val.map(PathBuf::from).unwrap_or(out); i += 2; }
            "--ui-root" => { ui_root = val.map(PathBuf::from).unwrap_or(ui_root); i += 2; }
            "--npcs" => { npcs = val.and_then(|v| v.parse().ok()).unwrap_or(npcs); i += 2; }
            "--seed" => { seed = val.and_then(|v| v.parse().ok()).unwrap_or(seed); i += 2; }
            "--scale" => { scale = val.and_then(|v| v.parse().ok()).unwrap_or(scale); i += 2; }
            "--player" => { player = true; i += 1; }
            "--capas" => { capas = true; i += 1; }
            other => { eprintln!("flag desconocida: {other}"); return ExitCode::FAILURE; }
        }
    }
    let cat = match chargen::Catalog::load(&root, "32x32") {
        Ok(c) => c,
        Err(e) => { eprintln!("{e}"); return ExitCode::FAILURE; }
    };
    eprintln!(
        "chargen: {} cuerpos · {} ojos · {} ropas · {} peinados · {} accesorios",
        cat.count(chargen::LayerKind::Body, false),
        cat.count(chargen::LayerKind::Eyes, false),
        cat.count(chargen::LayerKind::Outfit, false),
        cat.count(chargen::LayerKind::Hairstyle, false),
        cat.count(chargen::LayerKind::Accessory, false),
    );
    let keys = match chargen::bake(&cat, &out, npcs, seed, scale, player) {
        Ok(k) => k,
        Err(e) => { eprintln!("chargen: {e}"); return ExitCode::FAILURE; }
    };
    // El manifest vive junto al arte (`art/manifest.json`) — sin registrar la
    // grilla + el frame de reposo, el juego dibujaría la hoja entera.
    let manifest = out.join("manifest.json");
    if let Err(e) = chargen::register_in_manifest(&manifest, &keys) {
        eprintln!("chargen: horneado OK pero no pude actualizar el manifest: {e}");
        return ExitCode::FAILURE;
    }
    eprintln!(
        "chargen: {} personajes horneados en {} · manifest actualizado",
        keys.len(),
        out.display()
    );

    if capas {
        // Las capas se exportan desde `32x32`, la resolución nativa del pack y
        // la única que usa el juego: la celda es 32×64 y no hay downscale.
        let half = match chargen::Catalog::load(&root, "32x32") {
            Ok(c) => c,
            Err(e) => { eprintln!("chargen --capas: {e}"); return ExitCode::FAILURE; }
        };
        let portraits = match chargen::PortraitPack::load(&ui_root) {
            Ok(p) => p,
            Err(e) => { eprintln!("chargen --capas: {e}"); return ExitCode::FAILURE; }
        };
        // Los looks del horneado de arriba, para que el retrato de cada aldeano
        // sea EL SUYO y no una cara al azar.
        let looks = chargen::npc_looks(&cat, npcs, seed);
        let index = match chargen::export_layers(&half, &portraits, &out, &looks) {
            Ok(idx) => idx,
            Err(e) => { eprintln!("chargen --capas: {e}"); return ExitCode::FAILURE; }
        };
        let idx_path = out.join("chargen").join("index.json");
        let json = match index.to_json() {
            Ok(j) => j,
            Err(e) => { eprintln!("chargen --capas: {e}"); return ExitCode::FAILURE; }
        };
        if let Err(e) = std::fs::write(&idx_path, json + "\n") {
            eprintln!("chargen --capas: {}: {e}", idx_path.display());
            return ExitCode::FAILURE;
        }
        eprintln!(
            "chargen --capas: {} pieles · {} ojos · {} ropas · {} peinados · {} accesorios ({} con retrato) -> {}",
            index.bodies.len(),
            index.eyes.len(),
            index.outfits.len(),
            index.hair.len(),
            index.accessories.len(),
            index.accessories.iter().filter(|a| a.portrait.is_some()).count(),
            idx_path.display()
        );
    }
    ExitCode::SUCCESS
}

fn cmd_sound_init(args: &[String]) -> ExitCode {
    let path = std::path::Path::new("assets/audio.json");
    if path.exists() && !args.iter().any(|a| a == "--force") {
        eprintln!("{} ya existe (usa --force para pisarlo con el default)", path.display());
        return ExitCode::FAILURE;
    }
    let _ = std::fs::create_dir_all("assets");
    match std::fs::write(path, ha_audio::AudioSpec::default_spec().to_json()) {
        Ok(()) => {
            eprintln!("✓ escrito {} (diseño default)", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("no pude escribir {}: {e}", path.display());
            ExitCode::FAILURE
        }
    }
}
