//! Small CLI that reads a cljrs source file and emits its Mojo
//! translation at one or all tiers. Used by the bench harness to
//! ship the transpiled companion files alongside the cljrs sources.
//!
//! Usage:
//!   emit-mojo <src.clj> [readable|optimized|max]        # single tier to stdout
//!   emit-mojo <src.clj> --all                           # writes src.mojo.readable/.optimized/.max next to src

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use cljrs_mojo::{emit, Tier};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: emit-mojo <src.clj> [readable|optimized|max|--all]");
        return ExitCode::from(2);
    }
    let src_path = PathBuf::from(&args[0]);
    let mode = args.get(1).cloned().unwrap_or_else(|| "readable".into());
    let src = match fs::read_to_string(&src_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("read {}: {}", src_path.display(), e);
            return ExitCode::from(1);
        }
    };
    if mode == "--all" {
        let stem = src_path.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
        let dir = src_path.parent().unwrap_or(std::path::Path::new("."));
        for (tier, suffix) in [
            (Tier::Readable, "readable"),
            (Tier::Optimized, "optimized"),
            (Tier::Max, "max"),
        ] {
            match emit(&src, tier) {
                Ok(out) => {
                    let path = dir.join(format!("{stem}.mojo.{suffix}"));
                    if let Err(e) = fs::write(&path, &out) {
                        eprintln!("write {}: {}", path.display(), e);
                        return ExitCode::from(1);
                    }
                    println!("wrote {}", path.display());
                }
                Err(e) => {
                    eprintln!("emit {suffix}: {e}");
                    return ExitCode::from(1);
                }
            }
        }
        return ExitCode::from(0);
    }
    let tier = match mode.as_str() {
        "readable" => Tier::Readable,
        "optimized" => Tier::Optimized,
        "max" => Tier::Max,
        other => {
            eprintln!("unknown tier: {other}");
            return ExitCode::from(2);
        }
    };
    match emit(&src, tier) {
        Ok(out) => {
            print!("{out}");
            ExitCode::from(0)
        }
        Err(e) => {
            eprintln!("emit error: {e}");
            ExitCode::from(1)
        }
    }
}
