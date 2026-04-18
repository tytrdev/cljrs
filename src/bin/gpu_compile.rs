//! Compile a cljrs kernel to WGSL and print it to stdout. Used to
//! bake WGSL artifacts into the docs site (so browsers can load them
//! via WebGPU without needing the cljrs runtime in wasm).
//!
//! Usage:
//!   gpu-compile <kernel.clj>                 optimized SSA form
//!   gpu-compile --inline <kernel.clj>        readable form (nested exprs)

use std::env;
use std::fs;
use std::process;

use cljrs::{
    builtins,
    env::Env,
    eval,
    gpu::emit::{emit_pixel_with, EmitOptions},
    reader,
    types::unwrap_tagged,
    value::Value,
};

fn main() {
    let argv: Vec<String> = env::args().collect();
    let mut inline = false;
    let mut path: Option<String> = None;
    for a in argv.iter().skip(1) {
        match a.as_str() {
            "--inline" | "-i" => inline = true,
            "--optimized" | "--ssa" => inline = false,
            _ if path.is_none() => path = Some(a.clone()),
            _ => {
                eprintln!("unexpected arg: {a}");
                process::exit(1);
            }
        }
    }
    let Some(path) = path else {
        eprintln!("usage: gpu-compile [--inline|--optimized] <kernel.clj>");
        process::exit(1);
    };
    let src = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("read: {e}");
        process::exit(1);
    });
    let env = Env::new();
    builtins::install(&env);
    // Eval everything except the final defn-gpu-pixel so macros are
    // registered. Then re-compile the kernel body with our chosen
    // options. (The stored Value::GpuPixelKernel always uses the
    // default optimized form, so we can't just read its .wgsl.)
    let forms = reader::read_all(&src).unwrap_or_else(|e| {
        eprintln!("parse: {e}");
        process::exit(1);
    });
    let mut kernel_form: Option<Value> = None;
    for f in forms {
        if is_defn_gpu_pixel(&f) {
            kernel_form = Some(f);
        } else if let Err(e) = eval::eval(&f, &env) {
            eprintln!("eval: {e}");
            process::exit(1);
        }
    }
    let Some(kf) = kernel_form else {
        eprintln!("no (defn-gpu-pixel render ...) in file");
        process::exit(1);
    };
    // Extract the body and params from the kernel form.
    let (param_names, body) = match parse_gpu_pixel(&kf, &env) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            process::exit(1);
        }
    };
    let opts = EmitOptions { inline };
    let param_refs: [&str; 9] = std::array::from_fn(|i| param_names[i].as_str());
    match emit_pixel_with(&param_refs, &body, opts) {
        Ok(wgsl) => print!("{wgsl}"),
        Err(e) => {
            eprintln!("emit: {e}");
            process::exit(1);
        }
    }
}

fn is_defn_gpu_pixel(v: &Value) -> bool {
    if let Value::List(xs) = v
        && let Some(Value::Symbol(s)) = xs.first()
        && s.as_ref() == "defn-gpu-pixel"
    {
        return true;
    }
    false
}

/// Pull (param-names, body) out of a `(defn-gpu-pixel name [p0..p8] body...)`
/// form. Expand macros in the body so helper macros used by the kernel
/// are inlined, matching what the runtime emitter does.
fn parse_gpu_pixel(
    form: &Value,
    env: &Env,
) -> Result<([String; 9], Value), String> {
    let Value::List(xs) = form else {
        return Err("kernel form must be a list".into());
    };
    if xs.len() < 4 {
        return Err("defn-gpu-pixel: expected (defn-gpu-pixel name [params] body)".into());
    }
    let Value::Vector(params) = &xs[2] else {
        return Err("defn-gpu-pixel: params must be a vector".into());
    };
    if params.len() != 9 {
        return Err("defn-gpu-pixel: need exactly 9 params".into());
    }
    let mut names: Vec<String> = Vec::with_capacity(9);
    for p in params.iter() {
        let inner = unwrap_tagged(p).map(|(_, i)| i).unwrap_or(p);
        match inner {
            Value::Symbol(s) => names.push(s.to_string()),
            _ => return Err("param must be a symbol".into()),
        }
    }
    // Wrap multi-form body in implicit (do ...).
    let body: Value = if xs.len() == 4 {
        xs[3].clone()
    } else {
        let mut do_form = Vec::with_capacity(xs.len() - 2);
        do_form.push(Value::Symbol(std::sync::Arc::from("do")));
        do_form.extend(xs[3..].iter().cloned());
        Value::List(std::sync::Arc::new(do_form))
    };
    let expanded = eval::macroexpand_all(&body, env).map_err(|e| format!("macroexpand: {e}"))?;
    let name_arr: [String; 9] = std::array::from_fn(|i| names[i].clone());
    Ok((name_arr, expanded))
}
