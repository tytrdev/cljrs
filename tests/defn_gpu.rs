//! End-to-end: `defn-gpu` in cljrs source → WGSL → wgpu dispatch → result
//! readable as a cljrs vector. This is the "Clojure on the GPU" demo.

#![cfg(feature = "gpu")]

use cljrs::{builtins, env::Env, eval, reader, value::Value};

fn run(src: &str) -> Value {
    let env = Env::new();
    builtins::install(&env);
    let forms = reader::read_all(src).expect("read");
    let mut result = Value::Nil;
    for f in forms {
        result = eval::eval(&f, &env).expect("eval");
    }
    result
}

fn skip_if_no_gpu() -> bool {
    match cljrs::gpu::Gpu::new() {
        Ok(_) => false,
        Err(e) => {
            eprintln!("skipping: {e}");
            true
        }
    }
}

#[test]
fn defn_gpu_double_elementwise() {
    if skip_if_no_gpu() { return; }
    let src = r#"
      (defn-gpu doubled ^f32 [^i32 i ^f32 v]
        (* v 2.0))
      (doubled [1.0 2.0 3.0 4.0 5.0])
    "#;
    let v = run(src);
    match v {
        Value::Vector(items) => {
            let got: Vec<f64> = items.iter().map(|x| match x {
                Value::Float(f) => *f,
                _ => panic!("expected float, got {:?}", x),
            }).collect();
            assert_eq!(got, vec![2.0, 4.0, 6.0, 8.0, 10.0]);
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn defn_gpu_composes_math() {
    if skip_if_no_gpu() { return; }
    // Kernel: clamped sin: max(-0.5, min(0.5, sin(v)))
    let src = r#"
      (defn-gpu clamp-sin ^f32 [^i32 i ^f32 v]
        (max (float -0.5) (min (float 0.5) (sin v))))
      (clamp-sin [0.0 1.0 2.0 3.0 4.0 5.0 6.0 7.0 8.0 9.0])
    "#;
    let v = run(src);
    match v {
        Value::Vector(items) => {
            assert_eq!(items.len(), 10);
            for x in items.iter() {
                let f = match x { Value::Float(f) => *f, _ => panic!() };
                assert!(f >= -0.5 - 1e-5 && f <= 0.5 + 1e-5, "{f} out of range");
            }
        }
        _ => panic!(),
    }
}

#[test]
fn defn_gpu_handles_large_input() {
    if skip_if_no_gpu() { return; }
    // 100k elements — stresses buffer upload + workgroup dispatch math.
    let src = r#"
      (defn-gpu square ^f32 [^i32 i ^f32 v]
        (* v v))
      (count (square (mapv (fn [x] (* 1.0 x)) (range 100000))))
    "#;
    let v = run(src);
    assert_eq!(v, Value::Int(100000));
}
