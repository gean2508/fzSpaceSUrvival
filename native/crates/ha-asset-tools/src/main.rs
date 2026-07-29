//! `ha-asset-tools` — CLI de dev del sistema de assets.
//!
//! F2: `scan` — barre un árbol LimeZu y genera/actualiza un `manifest.ron`
//! esqueleto (un asset por PNG single). La IA nunca toca píxeles: sólo referencia
//! los IDs lógicos que salen de acá.
//!
//! Uso:
//!   ha-asset-tools scan --root <dir> --out <manifest.ron> [--prefix P]
//!                       [--grid 16] [--max N] [--merge]

use std::path::PathBuf;
use std::process::ExitCode;

use ha_assets::{generate_catalog, scan_limezu_directory, Manifest, ScanOptions};

mod chargen;
mod gui;
mod gym;
mod gym_gui;
mod imglab;
mod sound_gui;
mod world_gui;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("scan") => cmd_scan(&args[1..]),
        Some("catalog") => cmd_catalog(&args[1..]),
        Some("gui") => gui::run_gui(),
        Some("sound") => sound_gui::run_sound_gui(),
        Some("mundo") => world_gui::run_world_gui(),
        Some("gym") => gym::run_gym(),
        Some("gym-gui") => gym_gui::run_gym_gui(),
        Some("chargen") => cmd_chargen(&args[1..]),
        Some("sound-init") => cmd_sound_init(&args[1..]),
        Some("-h") | Some("--help") | None => {
            print_usage();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("subcomando desconocido: {other}\n");
            print_usage();
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    eprintln!(
        "ha-asset-tools — sistema de assets\n\n\
         COMANDOS:\n\
         \x20 scan --root <dir> --out <manifest.ron> [--prefix P] [--grid 32] [--max N] [--merge]\n\
         \x20 catalog --manifest <manifest.ron> --out <CATALOGO.md>\n\
         \x20 gui       # editor visual de assets (dev; necesita display)\n\
         \x20 sound     # editor de sonido / mini-DAW (edita assets/audio.json)\n\
         \x20 mundo     # editor de MUNDO WYSIWYG (H1: vista del mundo procedural)\n\
         \x20 gym       # entrena la fauna por neuroevolución (headless, CLI)\n\
         \x20 gym-gui   # visor de ADN: lista genomas .adn, evalúa/muta/cruza con preview\n\
         \x20 chargen [--root <pack>] [--out <art/>] [--npcs 20] [--seed 7] [--scale 2] [--player]\n\
         \x20           # hornea personajes por CAPAS (pack LimeZu Character_Generator)\n\
         \x20 sound-init [--force]  # (re)genera assets/audio.json con el diseño default\n\n\
         \x20   --root    directorio LimeZu a barrer (recursivo)\n\
         \x20   --out     archivo a escribir\n\
         \x20   --prefix  categoría para los IDs (def: art)\n\
         \x20   --grid    tamaño de celda px (def: 32)\n\
         \x20   --max     corta tras N archivos (prueba)\n\
         \x20   --merge   funde sobre el --out existente (respeta IDs curados)\n\
         \x20   --manifest  manifest.ron de entrada (catalog)"
    );
}

/// `chargen` — hornea personajes con el generador por capas de LimeZu
/// (`PRUEBAS-CODIGOS/Character_Generator`). Ver `chargen.rs`.
fn cmd_chargen(args: &[String]) -> ExitCode {
    let mut root = PathBuf::from("PRUEBAS-CODIGOS/Character_Generator");
    let mut out = PathBuf::from("native/crates/ha-game/assets/art");
    let (mut npcs, mut seed, mut scale, mut player) = (20usize, 7u64, 2u32, false);
    let mut i = 0;
    while i < args.len() {
        let val = args.get(i + 1);
        match args[i].as_str() {
            "--root" => { root = val.map(PathBuf::from).unwrap_or(root); i += 2; }
            "--out" => { out = val.map(PathBuf::from).unwrap_or(out); i += 2; }
            "--npcs" => { npcs = val.and_then(|v| v.parse().ok()).unwrap_or(npcs); i += 2; }
            "--seed" => { seed = val.and_then(|v| v.parse().ok()).unwrap_or(seed); i += 2; }
            "--scale" => { scale = val.and_then(|v| v.parse().ok()).unwrap_or(scale); i += 2; }
            "--player" => { player = true; i += 1; }
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
    ExitCode::SUCCESS
}

fn cmd_catalog(args: &[String]) -> ExitCode {
    let mut manifest_path: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--manifest" => { manifest_path = args.get(i + 1).map(PathBuf::from); i += 2; }
            "--out" => { out = args.get(i + 1).map(PathBuf::from); i += 2; }
            other => { eprintln!("flag desconocida: {other}"); return ExitCode::FAILURE; }
        }
    }
    let Some(manifest_path) = manifest_path else { eprintln!("falta --manifest"); return ExitCode::FAILURE; };
    let Some(out) = out else { eprintln!("falta --out"); return ExitCode::FAILURE; };

    let manifest = match std::fs::read_to_string(&manifest_path).ok().map(|s| Manifest::from_ron_str(&s)) {
        Some(Ok(m)) => m,
        Some(Err(e)) => { eprintln!("parse {}: {e}", manifest_path.display()); return ExitCode::FAILURE; }
        None => { eprintln!("no pude leer {}", manifest_path.display()); return ExitCode::FAILURE; }
    };
    let md = generate_catalog(&manifest);
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&out, md) {
        eprintln!("no pude escribir {}: {e}", out.display());
        return ExitCode::FAILURE;
    }
    eprintln!("✓ catalog: {} assets → {}", manifest.assets.len(), out.display());
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

fn cmd_scan(args: &[String]) -> ExitCode {
    let mut root: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut prefix = "art".to_string();
    let mut grid: u32 = 32;
    let mut max: Option<usize> = None;
    let mut merge = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => { root = args.get(i + 1).map(PathBuf::from); i += 2; }
            "--out" => { out = args.get(i + 1).map(PathBuf::from); i += 2; }
            "--prefix" => { prefix = arg_or_bail(args.get(i + 1), "--prefix"); i += 2; }
            "--grid" => { grid = arg_or_bail(args.get(i + 1), "--grid").parse().unwrap_or(32); i += 2; }
            "--max" => { max = args.get(i + 1).and_then(|s| s.parse().ok()); i += 2; }
            "--merge" => { merge = true; i += 1; }
            other => { eprintln!("flag desconocida: {other}"); return ExitCode::FAILURE; }
        }
    }

    let Some(root) = root else { eprintln!("falta --root"); return ExitCode::FAILURE; };
    let Some(out) = out else { eprintln!("falta --out"); return ExitCode::FAILURE; };
    if !root.is_dir() {
        eprintln!("--root no es un directorio: {}", root.display());
        return ExitCode::FAILURE;
    }

    let existing: Option<Manifest> = if merge && out.exists() {
        match std::fs::read_to_string(&out).ok().and_then(|s| Manifest::from_ron_str(&s).ok()) {
            Some(m) => Some(m),
            None => { eprintln!("no pude leer/parsear el --out existente para --merge"); return ExitCode::FAILURE; }
        }
    } else {
        None
    };

    let opts = ScanOptions { grid: (grid, grid), id_prefix: prefix, max_files: max };
    let (manifest, report) = scan_limezu_directory(&root, existing.as_ref(), &opts);

    let ron = match manifest.to_ron_string() {
        Ok(s) => s,
        Err(e) => { eprintln!("serialize: {e}"); return ExitCode::FAILURE; }
    };
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&out, ron) {
        eprintln!("no pude escribir {}: {e}", out.display());
        return ExitCode::FAILURE;
    }

    eprintln!(
        "✓ scan: {} PNG leídos, {} assets añadidos, {} saltados → {} ({} assets totales)",
        report.scanned, report.added, report.skipped, out.display(), manifest.assets.len()
    );
    ExitCode::SUCCESS
}

fn arg_or_bail(v: Option<&String>, flag: &str) -> String {
    match v {
        Some(s) => s.clone(),
        None => { eprintln!("{flag} necesita un valor"); std::process::exit(2); }
    }
}
