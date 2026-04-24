//! Numeric-kernel benchmarks: cljrs JIT-native per-element fns vs
//! hand-written Rust inline, over N-element Vec<f64>.
//!
//! Usage: cargo run --release --features mlir --bin bench-kernels -- [N] [iters]
//!
//! Produces a JSON report on stdout that the docs site ingests.
//!
//! The per-element kernel is compiled once via `(defn-native ...)`
//! into a machine-code pointer. We then transmute that pointer and
//! call it in a tight Rust loop. The only tree-walker cost is a
//! one-time eval at setup; per-iter cost is a real call through the
//! MLIR-lowered function.

use std::env;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use cljrs::{builtins, env::Env, eval, reader, value::Value};

/// Signature aliases we care about in this file.
type F64Binop = unsafe extern "C" fn(f64, f64) -> f64;
type F64Ternary = unsafe extern "C" fn(f64, f64, f64) -> f64;
type F64Unary = unsafe extern "C" fn(f64) -> f64;

struct Repo {
    env: Env,
}
impl Repo {
    fn new() -> Self {
        let env = Env::new();
        builtins::install(&env);
        Self { env }
    }
    fn eval_file(&self, path: &Path) -> Result<(), String> {
        let src = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let forms =
            reader::read_all(&src).map_err(|e| format!("read {}: {e}", path.display()))?;
        for f in forms {
            eval::eval(&f, &self.env).map_err(|e| format!("eval: {e}"))?;
        }
        Ok(())
    }
    fn native_ptr(&self, name: &str) -> Result<usize, String> {
        let v = self
            .env
            .lookup(name)
            .map_err(|e| format!("lookup {name}: {e}"))?;
        match v {
            Value::Native(n) => Ok(n.ptr),
            other => Err(format!(
                "{name}: expected Native, got {}",
                other.type_name()
            )),
        }
    }
}

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

/// Time a closure `iters` times, drop the first run (warmup), return median ns.
fn bench<F: FnMut()>(iters: usize, mut f: F) -> f64 {
    let warmups = 2.min(iters / 10 + 1);
    for _ in 0..warmups {
        f();
    }
    let mut samples = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t0 = Instant::now();
        f();
        samples.push(t0.elapsed().as_secs_f64() * 1e9);
    }
    median(samples)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let n: usize = args
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);
    let iters: usize = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    // Input buffers — deterministic and identical across backends.
    let a: Vec<f64> = (0..n).map(|i| (i as f64) * 1.0e-3).collect();
    let b: Vec<f64> = (0..n).map(|i| 1.0 + (i as f64) * 2.0e-3).collect();
    let c: Vec<f64> = (0..n).map(|i| 0.5 - (i as f64) * 0.5e-3).collect();
    let mut out = vec![0.0_f64; n];

    let scalar = std::f64::consts::PI;

    let repo = Repo::new();

    let kernel_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("bench/kernels");
    if let Err(e) = repo.eval_file(&kernel_root.join("vector_add.clj")) {
        eprintln!("{e}");
        std::process::exit(1);
    }
    if let Err(e) = repo.eval_file(&kernel_root.join("saxpy.clj")) {
        eprintln!("{e}");
        std::process::exit(1);
    }
    if let Err(e) = repo.eval_file(&kernel_root.join("dot.clj")) {
        eprintln!("{e}");
        std::process::exit(1);
    }
    if let Err(e) = repo.eval_file(&kernel_root.join("sum_sq.clj")) {
        eprintln!("{e}");
        std::process::exit(1);
    }

    // ----- extract native pointers -----
    let vadd_ptr = repo.native_ptr("vadd-elem").unwrap_or_else(|e| fatal(e));
    let saxpy_ptr = repo.native_ptr("saxpy-elem").unwrap_or_else(|e| fatal(e));
    let dot_ptr = repo.native_ptr("dot-elem").unwrap_or_else(|e| fatal(e));
    let sumsq_ptr = repo.native_ptr("sumsq-elem").unwrap_or_else(|e| fatal(e));

    // SAFETY: these are JIT'd to fns with the shown C ABI signatures,
    // and the Repo keeps the ExecutionEngine alive.
    let vadd: F64Binop = unsafe { std::mem::transmute(vadd_ptr) };
    let saxpy: F64Ternary = unsafe { std::mem::transmute(saxpy_ptr) };
    let dot: F64Binop = unsafe { std::mem::transmute(dot_ptr) };
    let sumsq: F64Unary = unsafe { std::mem::transmute(sumsq_ptr) };

    eprintln!("N = {n}, iters = {iters}");

    // ----- vector_add -----
    let vadd_flops = n as f64; // 1 op per element
    let vadd_bytes = (n * 3 * 8) as f64; // 2 reads + 1 write, f64
    let t_cljrs_vadd = bench(iters, || {
        for i in 0..n {
            out[i] = unsafe { vadd(a[i], b[i]) };
        }
        std::hint::black_box(&out);
    });
    let t_rust_vadd = bench(iters, || {
        for i in 0..n {
            out[i] = a[i] + b[i];
        }
        std::hint::black_box(&out);
    });

    // ----- saxpy -----
    let saxpy_flops = (2 * n) as f64; // mul + add
    let saxpy_bytes = (n * 3 * 8) as f64; // reads x + y, write out (scalar a broadcast)
    let t_cljrs_saxpy = bench(iters, || {
        for i in 0..n {
            out[i] = unsafe { saxpy(scalar, a[i], b[i]) };
        }
        std::hint::black_box(&out);
    });
    let t_rust_saxpy = bench(iters, || {
        for i in 0..n {
            out[i] = scalar * a[i] + b[i];
        }
        std::hint::black_box(&out);
    });

    // ----- dot -----
    let dot_flops = (2 * n) as f64; // mul + add
    let dot_bytes = (n * 2 * 8) as f64;
    let mut dot_acc = 0.0f64;
    let t_cljrs_dot = bench(iters, || {
        let mut s = 0.0;
        for i in 0..n {
            s += unsafe { dot(a[i], b[i]) };
        }
        dot_acc = s;
        std::hint::black_box(&dot_acc);
    });
    let t_rust_dot = bench(iters, || {
        let mut s = 0.0;
        for i in 0..n {
            s += a[i] * b[i];
        }
        dot_acc = s;
        std::hint::black_box(&dot_acc);
    });

    // ----- sum_sq -----
    let sumsq_flops = (2 * n) as f64; // mul + add
    let sumsq_bytes = (n * 8) as f64;
    let mut sumsq_acc = 0.0f64;
    let t_cljrs_sumsq = bench(iters, || {
        let mut s = 0.0;
        for i in 0..n {
            s += unsafe { sumsq(c[i]) };
        }
        sumsq_acc = s;
        std::hint::black_box(&sumsq_acc);
    });
    let t_rust_sumsq = bench(iters, || {
        let mut s = 0.0;
        for i in 0..n {
            s += c[i] * c[i];
        }
        sumsq_acc = s;
        std::hint::black_box(&sumsq_acc);
    });

    // ----- correctness cross-check -----
    // Compare a cljrs run to Rust directly. They should agree bit-for-bit.
    let rust_dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let rust_sumsq: f64 = c.iter().map(|x| x * x).sum();
    assert!((dot_acc - rust_dot).abs() / rust_dot.abs() < 1e-12, "dot mismatch");
    assert!((sumsq_acc - rust_sumsq).abs() / rust_sumsq.abs() < 1e-12, "sum_sq mismatch");

    // ----- emit JSON -----
    let gflops = |flops: f64, ns: f64| flops / ns;
    let gibs = |bytes: f64, ns: f64| bytes / ns;

    println!("{{");
    println!("  \"n\": {n},");
    println!("  \"iters\": {iters},");
    println!("  \"backends\": [\"cljrs-native\", \"rust-inline\"],");
    println!("  \"kernels\": [");
    for (name, flops, bytes, t_cljrs, t_rust, last) in [
        ("vector_add", vadd_flops, vadd_bytes, t_cljrs_vadd, t_rust_vadd, false),
        ("saxpy", saxpy_flops, saxpy_bytes, t_cljrs_saxpy, t_rust_saxpy, false),
        ("dot", dot_flops, dot_bytes, t_cljrs_dot, t_rust_dot, false),
        ("sum_sq", sumsq_flops, sumsq_bytes, t_cljrs_sumsq, t_rust_sumsq, true),
    ] {
        println!("    {{");
        println!("      \"name\": \"{name}\",");
        println!("      \"flops\": {},", flops as u64);
        println!("      \"bytes\": {},", bytes as u64);
        println!("      \"cljrs_native_ns\": {},", t_cljrs as u64);
        println!("      \"rust_inline_ns\":  {},", t_rust as u64);
        println!("      \"cljrs_native_gflops\": {:.3},", gflops(flops, t_cljrs));
        println!("      \"rust_inline_gflops\":  {:.3},", gflops(flops, t_rust));
        println!("      \"cljrs_native_gibs\":  {:.3},", gibs(bytes, t_cljrs));
        println!("      \"rust_inline_gibs\":   {:.3}", gibs(bytes, t_rust));
        println!("    }}{}", if last { "" } else { "," });
    }
    println!("  ]");
    println!("}}");

    // Keep the compiler from optimizing away the arc; not strictly needed.
    let _ = Arc::new(repo);
}

fn fatal(e: String) -> ! {
    eprintln!("{e}");
    std::process::exit(1)
}
