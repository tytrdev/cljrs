//! SIMD vectorization tests for `elementwise-mojo`.
//!
//! The pedagogical contract: Readable/Optimized tiers emit a scalar
//! `for i in range(n): out[i] = ...` loop; Max tier rewrites it into
//! Mojo's `vectorize[body, nelts](n)` idiom with `SIMD[DType, w].load/store`.

use cljrs_mojo::{emit, Tier};

fn must(s: &str, needle: &str) {
    assert!(s.contains(needle), "missing {needle:?} in:\n{s}");
}
fn must_not(s: &str, needle: &str) {
    assert!(!s.contains(needle), "should not contain {needle:?} in:\n{s}");
}

// --- Phase 1: vector_add at Max ---

#[test]
fn elementwise_vector_add_max_emits_vectorize() {
    let src = r#"(elementwise-mojo vector-add [^f32 a ^f32 b] ^f32 (+ a b))"#;
    let out = emit(src, Tier::Max).unwrap();
    must(&out, "from algorithm import vectorize");
    must(&out, "from memory import UnsafePointer");
    must(&out, "from sys import simd_width_of");
    must(&out, "alias nelts_f32 = simd_width_of[DType.float32]()");
    must(&out, "fn vector_add(a: UnsafePointer[Float32], b: UnsafePointer[Float32], out: UnsafePointer[Float32], n: Int):");
    must(&out, "@parameter");
    must(&out, "fn __kernel[w: Int](i: Int):");
    must(&out, "var av = SIMD[DType.float32, w].load(a, i)");
    must(&out, "var bv = SIMD[DType.float32, w].load(b, i)");
    must(&out, ".store(out, i)");
    must(&out, "vectorize[__kernel, nelts_f32](n)");
}

// --- Phase 1: readable tier emits scalar loop (no vectorize) ---

#[test]
fn elementwise_vector_add_readable_is_scalar_loop() {
    let src = r#"(elementwise-mojo vector-add [^f32 a ^f32 b] ^f32 (+ a b))"#;
    let out = emit(src, Tier::Readable).unwrap();
    must_not(&out, "vectorize");
    must_not(&out, "simd_width_of");
    must_not(&out, "SIMD[DType");
    must(&out, "for i in range(n):");
    must(&out, "out[i] = (a[i] + b[i])");
    // Signature still uses UnsafePointer[T] — uniform API across tiers.
    must(&out, "a: UnsafePointer[Float32]");
}

// --- Phase 1: tiers diverge on the same input ---

#[test]
fn elementwise_readable_vs_max_diverge() {
    let src = r#"(elementwise-mojo vector-add [^f32 a ^f32 b] ^f32 (+ a b))"#;
    let readable = emit(src, Tier::Readable).unwrap();
    let max = emit(src, Tier::Max).unwrap();
    assert_ne!(readable, max, "tiers must produce different output");
    assert!(readable.contains("for i in range(n)"));
    assert!(max.contains("vectorize["));
}

// --- Phase 3: multi-input elementwise (axpy = a*x + y) ---

#[test]
fn elementwise_multi_input_axpy_max() {
    let src = r#"(elementwise-mojo axpy [^f32 a ^f32 x ^f32 y] ^f32 (+ (* a x) y))"#;
    let out = emit(src, Tier::Max).unwrap();
    must(&out, "a: UnsafePointer[Float32]");
    must(&out, "x: UnsafePointer[Float32]");
    must(&out, "y: UnsafePointer[Float32]");
    must(&out, "var av = SIMD[DType.float32, w].load(a, i)");
    must(&out, "var xv = SIMD[DType.float32, w].load(x, i)");
    must(&out, "var yv = SIMD[DType.float32, w].load(y, i)");
    // Body substitutes a/x/y with SIMD-loaded versions.
    must(&out, "((av * xv) + yv)");
}

// --- Phase 3: scalar param (^scalar ^f32 k) does not become a pointer ---

#[test]
fn elementwise_scalar_arg_stays_scalar() {
    let src = r#"(elementwise-mojo scale [^f32 x ^scalar ^f32 k] ^f32 (* x k))"#;
    let readable = emit(src, Tier::Readable).unwrap();
    must(&readable, "x: UnsafePointer[Float32]");
    // k is scalar Float32, not a pointer.
    must(&readable, "k: Float32");
    must_not(&readable, "k: UnsafePointer");
    must(&readable, "out[i] = (x[i] * k)");

    let max = emit(src, Tier::Max).unwrap();
    must(&max, "x: UnsafePointer[Float32]");
    must(&max, "k: Float32");
    must_not(&max, "k: UnsafePointer");
    // In the SIMD kernel, x becomes xv (loaded) but k is broadcast as-is.
    must(&max, "var xv = SIMD[DType.float32, w].load(x, i)");
    must(&max, "(xv * k)");
}

// --- Error path: unsupported op (user fn call) ---

#[test]
fn elementwise_rejects_unknown_call() {
    let src = r#"(elementwise-mojo bad [^f32 a] ^f32 (mystery-op a))"#;
    let err = emit(src, Tier::Max).unwrap_err();
    assert!(err.contains("unsupported op `mystery-op`"), "got: {err}");
}

// --- Error path: undeclared name in body ---

#[test]
fn elementwise_rejects_undeclared_name() {
    let src = r#"(elementwise-mojo bad [^f32 a] ^f32 (+ a z))"#;
    let err = emit(src, Tier::Max).unwrap_err();
    assert!(err.contains("undeclared name `z`"), "got: {err}");
}

// --- Error path: mismatched dtypes across per-element inputs ---

#[test]
fn elementwise_rejects_mixed_element_dtypes() {
    let src = r#"(elementwise-mojo bad [^f32 a ^f64 b] ^f32 (+ a b))"#;
    let err = emit(src, Tier::Max).unwrap_err();
    assert!(err.contains("share the same dtype"), "got: {err}");
}

// --- All three tiers produce something valid for a math body ---

#[test]
fn elementwise_math_body_all_tiers() {
    let src = r#"(elementwise-mojo saxpy [^f32 x ^f32 y ^scalar ^f32 a] ^f32 (+ (* a x) y))"#;
    for tier in [Tier::Readable, Tier::Optimized, Tier::Max] {
        let out = emit(src, tier).unwrap();
        must(&out, "fn saxpy(");
        must(&out, "a: Float32");
    }
    let max = emit(src, Tier::Max).unwrap();
    must(&max, "vectorize[__kernel, nelts_f32](n)");
}

