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

use ha_assets::{scan_limezu_directory, Manifest, ScanOptions};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("scan") => cmd_scan(&args[1..]),
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
         \x20 scan --root <dir> --out <manifest.ron> [--prefix P] [--grid 16] [--max N] [--merge]\n\n\
         \x20   --root    directorio LimeZu a barrer (recursivo)\n\
         \x20   --out     manifest.ron a escribir\n\
         \x20   --prefix  categoría para los IDs (def: art)\n\
         \x20   --grid    tamaño de celda px (def: 16)\n\
         \x20   --max     corta tras N archivos (prueba)\n\
         \x20   --merge   funde sobre el --out existente (respeta IDs curados)"
    );
}

fn cmd_scan(args: &[String]) -> ExitCode {
    let mut root: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut prefix = "art".to_string();
    let mut grid: u32 = 16;
    let mut max: Option<usize> = None;
    let mut merge = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => { root = args.get(i + 1).map(PathBuf::from); i += 2; }
            "--out" => { out = args.get(i + 1).map(PathBuf::from); i += 2; }
            "--prefix" => { prefix = arg_or_bail(args.get(i + 1), "--prefix"); i += 2; }
            "--grid" => { grid = arg_or_bail(args.get(i + 1), "--grid").parse().unwrap_or(16); i += 2; }
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
