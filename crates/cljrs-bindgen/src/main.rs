//! CLI: `cljrs-bindgen <manifest.toml> [--out <dir>]`.
//!
//! Reads the manifest, generates the shim crate (Cargo.toml + src/lib.rs)
//! into `--out` (default: ./out/<crate-name>). The output crate is
//! standalone — point Cargo at it via `path = "..."` from the workspace
//! root or vendor it under `crates/`.

use cljrs_bindgen::{generate_install_rs, Manifest};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "cljrs-bindgen <manifest.toml> [--out <dir>]\n\
             Generates a Rust shim crate with `pub fn install(env: &Env)`."
        );
        return ExitCode::from(if args.is_empty() { 2 } else { 0 });
    }
    let mut manifest_path: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" | "-o" => {
                out_dir = it.next().map(PathBuf::from);
            }
            other => {
                if manifest_path.is_none() {
                    manifest_path = Some(PathBuf::from(other));
                } else {
                    eprintln!("unexpected arg: {other}");
                    return ExitCode::from(2);
                }
            }
        }
    }
    let mp = match manifest_path {
        Some(p) => p,
        None => {
            eprintln!("missing manifest path");
            return ExitCode::from(2);
        }
    };
    let src = match fs::read_to_string(&mp) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {}: {e}", mp.display());
            return ExitCode::from(1);
        }
    };
    let manifest = match Manifest::from_str(&src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("manifest error: {e}");
            return ExitCode::from(1);
        }
    };
    let lib_rs = match generate_install_rs(&manifest) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("codegen error: {e}");
            return ExitCode::from(1);
        }
    };
    let out = out_dir.unwrap_or_else(|| {
        PathBuf::from("./out").join(format!("cljrs-{}", manifest.crate_.name))
    });
    if let Err(e) = fs::create_dir_all(out.join("src")) {
        eprintln!("mkdir {}: {e}", out.display());
        return ExitCode::from(1);
    }
    let cljrs_path = std::env::var("CLJRS_PATH")
        .ok()
        .unwrap_or_else(|| "../..".to_string());
    let cargo_toml = render_cargo_toml(&manifest, &cljrs_path);
    if let Err(e) = fs::write(out.join("Cargo.toml"), cargo_toml) {
        eprintln!("write Cargo.toml: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = fs::write(out.join("src").join("lib.rs"), lib_rs) {
        eprintln!("write lib.rs: {e}");
        return ExitCode::from(1);
    }
    eprintln!(
        "wrote {} ({} fns, {} methods) -> {}",
        manifest.crate_.name,
        manifest.fns.len(),
        manifest.methods.len(),
        out.display()
    );
    ExitCode::SUCCESS
}

/// Render a minimal Cargo.toml. The cljrs path is left relative to a
/// workspace root checkout — the user can edit the path or replace with
/// a git/version pin if they're vendoring outside the cljrs tree.
fn render_cargo_toml(m: &Manifest, cljrs_path: &str) -> String {
    format!(
        "[package]\n\
         name = \"cljrs-{name}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2024\"\n\
         publish = false\n\
         \n\
         # Standalone — opt out of any enclosing workspace by default.\n\
         # Delete this block if vendoring inside the cljrs workspace.\n\
         [workspace]\n\
         \n\
         [lib]\n\
         path = \"src/lib.rs\"\n\
         \n\
         [dependencies]\n\
         cljrs = {{ path = \"{cljrs}\" }}\n\
         {name} = \"{ver}\"\n",
        name = m.crate_.name,
        ver = m.crate_.version,
        cljrs = cljrs_path,
    )
}
