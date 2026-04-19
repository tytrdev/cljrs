//! Mirrors the `Repl::call_floats` fast-path in `crates/cljrs-wasm`. The
//! wasm crate is cdylib-only so its wasm-bindgen methods can't be cargo-
//! tested directly; this test exercises the exact same lookup + apply +
//! flatten pipeline against the native tree-walker to lock in semantics.
//!
//! The synth demo depends on this to push Float32 audio params through
//! a Clojure fn at audio-rate without reparsing source.

use cljrs::{builtins, env::Env, eval, reader, value::Value};

fn call_floats(env: &Env, name: &str, args: Vec<f32>) -> Vec<f32> {
    let f = match env.lookup(name) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let arg_vals: Vec<Value> = args.into_iter().map(|x| Value::Float(x as f64)).collect();
    let result = match eval::apply(&f, &arg_vals) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    fn push_num(out: &mut Vec<f32>, v: &Value) -> bool {
        match v {
            Value::Float(f) => { out.push(*f as f32); true }
            Value::Int(i) => { out.push(*i as f32); true }
            _ => false,
        }
    }
    let mut out: Vec<f32> = Vec::new();
    if let Value::Vector(xs) = &result {
        for x in xs.iter() {
            if push_num(&mut out, x) { continue; }
            if let Value::Vector(inner) = x {
                for y in inner.iter() {
                    push_num(&mut out, y);
                }
            }
        }
    }
    out
}

fn env_with(src: &str) -> Env {
    let env = Env::new();
    builtins::install(&env);
    for f in reader::read_all(src).unwrap() {
        eval::eval(&f, &env).expect("script eval");
    }
    env
}

#[test]
fn call_floats_flat_vec_result() {
    let env = env_with("(defn f [x y] [(* x 2.0) (+ y 1.0)])");
    let out = call_floats(&env, "f", vec![3.0, 4.0]);
    assert_eq!(out.len(), 2);
    assert!((out[0] - 6.0).abs() < 1e-5, "got {}", out[0]);
    assert!((out[1] - 5.0).abs() < 1e-5, "got {}", out[1]);
}

#[test]
fn call_floats_flattens_nested_vec() {
    let env = env_with("(defn g [a] [[a (* a 2.0)] [(* a 3.0) (* a 4.0)]])");
    let out = call_floats(&env, "g", vec![1.5]);
    assert_eq!(out.len(), 4);
    assert!((out[0] - 1.5).abs() < 1e-5);
    assert!((out[1] - 3.0).abs() < 1e-5);
    assert!((out[2] - 4.5).abs() < 1e-5);
    assert!((out[3] - 6.0).abs() < 1e-5);
}

#[test]
fn call_floats_missing_fn_returns_empty() {
    let env = env_with("");
    let out = call_floats(&env, "nope", vec![1.0]);
    assert!(out.is_empty());
}
