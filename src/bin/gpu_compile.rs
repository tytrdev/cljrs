//! Compile a cljrs kernel to WGSL and print it to stdout. Used to
//! bake WGSL artifacts into the docs site (so browsers can load them
//! via WebGPU without needing the cljrs runtime in wasm).
//!
//! Usage:
//!   cargo run --release --features gpu --bin gpu-compile -- demo_gpu/plasma.clj
//!
//! Writes the compiled WGSL to stdout.

use std::env;
use std::fs;
use std::process;

use cljrs::{builtins, env::Env, eval, reader, value::Value};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: gpu-compile <kernel.clj>");
        process::exit(1);
    }
    let src = fs::read_to_string(&args[1]).unwrap_or_else(|e| {
        eprintln!("read: {e}");
        process::exit(1);
    });
    let env = Env::new();
    builtins::install(&env);
    for f in reader::read_all(&src).unwrap_or_else(|e| {
        eprintln!("parse: {e}");
        process::exit(1);
    }) {
        if let Err(e) = eval::eval(&f, &env) {
            eprintln!("eval: {e}");
            process::exit(1);
        }
    }
    match env.lookup("render") {
        Ok(Value::GpuPixelKernel(k)) => {
            print!("{}", k.wgsl);
        }
        Ok(other) => {
            eprintln!(
                "`render` is {}, expected gpu-pixel-kernel (use defn-gpu-pixel)",
                other.type_name()
            );
            process::exit(1);
        }
        Err(e) => {
            eprintln!("no `render`: {e}");
            process::exit(1);
        }
    }
}
