//! cljrs benchmark driver.
//!
//! Usage: cargo run --release --bin bench -- <file.clj> [iters]
//!
//! Evaluates all forms in the file except the last as setup, then wraps the
//! last form in `(fn [] ...)` and calls the resulting function `iters` times.
//! Reports per-iter ns for direct comparison with other implementations.

use std::env;
use std::fs;
use std::process;
use std::sync::Arc;
use std::time::Instant;

use cljrs::{builtins, env::Env, eval, reader, value::Value};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: bench <file.clj> [iters]");
        process::exit(2);
    }
    let path = &args[1];
    let iters: u64 = args
        .get(2)
        .map(|s| s.parse().expect("iters must be a positive integer"))
        .unwrap_or(100);

    let src = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("failed to read {path}: {e}");
        process::exit(1);
    });

    let forms = reader::read_all(&src).unwrap_or_else(|e| {
        eprintln!("parse error: {e}");
        process::exit(1);
    });

    if forms.is_empty() {
        eprintln!("no forms in {path}");
        process::exit(1);
    }

    let env = Env::new();
    builtins::install(&env);

    let (bench_form, setup) = forms.split_last().expect("non-empty");
    for f in setup {
        eval::eval(f, &env).unwrap_or_else(|e| {
            eprintln!("setup eval error: {e}");
            process::exit(1);
        });
    }

    // Wrap the bench form in a zero-arg fn, then call the fn N times.
    // This mirrors the JVM harness and avoids re-parsing/lookup costs
    // outside of what the function call itself does.
    let fn_form = Value::List(Arc::new(vec![
        Value::Symbol(Arc::from("fn")),
        Value::Vector(imbl::Vector::new()),
        bench_form.clone(),
    ]));
    let callable = eval::eval(&fn_form, &env).unwrap_or_else(|e| {
        eprintln!("wrap eval error: {e}");
        process::exit(1);
    });

    // Warmup
    for _ in 0..3 {
        eval::apply(&callable, &[]).unwrap_or_else(|e| {
            eprintln!("warmup error: {e}");
            process::exit(1);
        });
    }

    let start = Instant::now();
    let mut last = Value::Nil;
    for _ in 0..iters {
        last = eval::apply(&callable, &[]).unwrap_or_else(|e| {
            eprintln!("bench error: {e}");
            process::exit(1);
        });
    }
    let elapsed = start.elapsed();
    let per_iter_ns = elapsed.as_nanos() as f64 / iters as f64;
    let total_ms = elapsed.as_secs_f64() * 1000.0;

    // Trailing `result=` is consumed by bench/run.sh's cross-impl correctness
    // check — if two impls disagree on the value, speed doesn't matter.
    println!(
        "cljrs  {path:<40}  iters={iters:<8}  total={total_ms:>10.2}ms  per-iter={per_iter_ns:>14.0}ns  result={last}"
    );
}
