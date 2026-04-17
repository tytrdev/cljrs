//! Buffer / array support in the native ABI.
//!
//! These only run under the `mlir` feature — they compile a cljrs fn
//! with a `^f64-buf` param, then invoke it from Rust via NativeFn with
//! the underlying Vec's pointer passed as an i64 handle.
//!
//! This is the foundation the phase-3 GPU kernel DSL will sit on top of:
//! GPU kernels are fundamentally buffer operators, so buffer passing at
//! the native ABI is a prerequisite.

#![cfg(feature = "mlir")]

use std::sync::Arc;

use cljrs::codegen::mlir::compile::compile_native_fn;
use cljrs::native::NativeRegistry;
use cljrs::reader;
use cljrs::types::PrimType;
use cljrs::value::Value;

fn parse_body(src: &str) -> Value {
    let forms = reader::read_all(src).expect("read");
    forms.into_iter().next().expect("non-empty")
}

#[test]
fn sum_buffer_via_native_fn() {
    let body = parse_body(
        r#"(loop [i 0 acc 0.0]
             (if (>= i n) acc
               (recur (+ i 1) (+ acc (buf-get xs i)))))"#,
    );
    let params: &[(Arc<str>, PrimType)] = &[
        (Arc::from("xs"), PrimType::F64Buf),
        (Arc::from("n"), PrimType::I64),
    ];
    let native = compile_native_fn(
        "sum-buf",
        params,
        PrimType::F64,
        &body,
        &NativeRegistry::default(),
    )
    .expect("compile");

    let data: Vec<f64> = (0..1000).map(|i| i as f64).collect();
    let expected: f64 = data.iter().sum();

    let ptr_as_i64 = data.as_ptr() as usize as i64;
    let r = native
        .invoke(&[Value::Int(ptr_as_i64), Value::Int(data.len() as i64)])
        .expect("invoke");
    match r {
        Value::Float(v) => assert!((v - expected).abs() < 1e-9, "{v} vs {expected}"),
        other => panic!("expected float, got {other:?}"),
    }
}

#[test]
fn max_buffer_via_native_fn() {
    // A slightly different kernel to confirm buf-get composes with loop/recur.
    let body = parse_body(
        r#"(loop [i 1 best (buf-get xs 0)]
             (if (>= i n) best
               (let [v (buf-get xs i)]
                 (recur (+ i 1) (max best v)))))"#,
    );
    let params: &[(Arc<str>, PrimType)] = &[
        (Arc::from("xs"), PrimType::F64Buf),
        (Arc::from("n"), PrimType::I64),
    ];
    let native = compile_native_fn(
        "max-buf",
        params,
        PrimType::F64,
        &body,
        &NativeRegistry::default(),
    )
    .expect("compile");

    let data: Vec<f64> = vec![3.0, 1.0, 9.0, 4.0, 1.0, 5.0, 9.0, 2.0, 6.0, 5.0];
    let ptr_as_i64 = data.as_ptr() as usize as i64;
    let r = native
        .invoke(&[Value::Int(ptr_as_i64), Value::Int(data.len() as i64)])
        .expect("invoke");
    match r {
        Value::Float(v) => assert_eq!(v, 9.0),
        other => panic!("expected float, got {other:?}"),
    }
}
