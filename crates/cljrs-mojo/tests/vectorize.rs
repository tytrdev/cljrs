//! SIMD vectorization tests for `elementwise-mojo` and `reduce-mojo`.
//!
//! Contract: Readable/Optimized tiers emit a scalar
//! `for i in range(n): out[i] = ...` loop. Max tier emits a manual
//! SIMD-chunked loop (`while i + nelts_T <= n: ... i += nelts_T`)
//! with a scalar tail, using `ptr.load[width=W](i)` /
//! `ptr.store[width=W](i, val)` pointer methods. This shape is what
//! the current Mojo nightly accepts — `vectorize[]` rejected capturing
//! closures that cljrs transpilation always produces.

use cljrs_mojo::{emit, Tier};

fn must(s: &str, needle: &str) {
    assert!(s.contains(needle), "missing {needle:?} in:\n{s}");
}
fn must_not(s: &str, needle: &str) {
    assert!(!s.contains(needle), "should not contain {needle:?} in:\n{s}");
}

// --- Phase 1: vector_add at Max ---

#[test]
fn elementwise_vector_add_max_emits_simd_loop() {
    let src = r#"(elementwise-mojo vector-add [^f32 a ^f32 b] ^f32 (+ a b))"#;
    let out = emit(src, Tier::Max).unwrap();
    must(&out, "from std.memory import UnsafePointer");
    must(&out, "from std.sys import simd_width_of");
    must(&out, "alias nelts_f32 = simd_width_of[DType.float32]()");
    must(&out, "fn vector_add(a: UnsafePointer[Float32, MutAnyOrigin], b: UnsafePointer[Float32, MutAnyOrigin], dst: UnsafePointer[Float32, MutAnyOrigin], n: Int):");
    must(&out, "var i = 0");
    must(&out, "while i + nelts_f32 <= n:");
    must(&out, "var av = a.load[width=nelts_f32](i)");
    must(&out, "var bv = b.load[width=nelts_f32](i)");
    must(&out, "dst.store[width=nelts_f32](i,");
    must(&out, "i += nelts_f32");
    must(&out, "while i < n:");
    must(&out, "dst[i] = (a[i] + b[i])");
}

// --- Phase 1: readable tier emits scalar loop (no SIMD) ---

#[test]
fn elementwise_vector_add_readable_is_scalar_loop() {
    let src = r#"(elementwise-mojo vector-add [^f32 a ^f32 b] ^f32 (+ a b))"#;
    let out = emit(src, Tier::Readable).unwrap();
    must_not(&out, "simd_width_of");
    must_not(&out, "SIMD[DType");
    must_not(&out, ".load[width=");
    must(&out, "for i in range(n):");
    must(&out, "dst[i] = (a[i] + b[i])");
    // Signature uses UnsafePointer[T, MutAnyOrigin] — uniform across tiers.
    must(&out, "a: UnsafePointer[Float32, MutAnyOrigin]");
}

// --- Phase 1: tiers diverge on the same input ---

#[test]
fn elementwise_readable_vs_max_diverge() {
    let src = r#"(elementwise-mojo vector-add [^f32 a ^f32 b] ^f32 (+ a b))"#;
    let readable = emit(src, Tier::Readable).unwrap();
    let max = emit(src, Tier::Max).unwrap();
    assert_ne!(readable, max, "tiers must produce different output");
    assert!(readable.contains("for i in range(n)"));
    assert!(max.contains("while i + nelts_f32 <= n:"));
    assert!(max.contains(".load[width="));
}

// --- Phase 3: multi-input elementwise (axpy = a*x + y) ---

#[test]
fn elementwise_multi_input_axpy_max() {
    let src = r#"(elementwise-mojo axpy [^f32 a ^f32 x ^f32 y] ^f32 (+ (* a x) y))"#;
    let out = emit(src, Tier::Max).unwrap();
    must(&out, "a: UnsafePointer[Float32, MutAnyOrigin]");
    must(&out, "x: UnsafePointer[Float32, MutAnyOrigin]");
    must(&out, "y: UnsafePointer[Float32, MutAnyOrigin]");
    must(&out, "var av = a.load[width=nelts_f32](i)");
    must(&out, "var xv = x.load[width=nelts_f32](i)");
    must(&out, "var yv = y.load[width=nelts_f32](i)");
    // Body substitutes a/x/y with SIMD-loaded versions.
    must(&out, "(av * xv + yv)");
}

// --- Phase 3: scalar param (^scalar ^f32 k) does not become a pointer ---

#[test]
fn elementwise_scalar_arg_stays_scalar() {
    let src = r#"(elementwise-mojo scale [^f32 x ^scalar ^f32 k] ^f32 (* x k))"#;
    let readable = emit(src, Tier::Readable).unwrap();
    must(&readable, "x: UnsafePointer[Float32, MutAnyOrigin]");
    // k is scalar Float32, not a pointer.
    must(&readable, "k: Float32");
    must_not(&readable, "k: UnsafePointer");
    must(&readable, "dst[i] = (x[i] * k)");

    let max = emit(src, Tier::Max).unwrap();
    must(&max, "x: UnsafePointer[Float32, MutAnyOrigin]");
    must(&max, "k: Float32");
    must_not(&max, "k: UnsafePointer");
    // In the SIMD kernel, x becomes xv (loaded) but k is broadcast as-is.
    must(&max, "var xv = x.load[width=nelts_f32](i)");
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
    must(&max, "while i + nelts_f32 <= n:");
    must(&max, ".load[width=nelts_f32]");
}


// ---------------- Reductions ----------------

#[test]
fn reduce_sum_readable_is_scalar_loop() {
    let src = r#"(reduce-mojo sum-sq [^f32 x] ^f32 (* x x) 0.0)"#;
    let out = emit(src, Tier::Readable).unwrap();
    must(&out, "fn sum_sq(x: UnsafePointer[Float32, MutAnyOrigin], n: Int) -> Float32:");
    must(&out, "var acc: Float32 = 0.0");
    must(&out, "for i in range(n):");
    must(&out, "acc += (x[i] * x[i])");
    must(&out, "return acc");
    must_not(&out, "reduce_add");
}

#[test]
fn reduce_sum_max_uses_simd_accumulator() {
    let src = r#"(reduce-mojo sum-sq [^f32 x] ^f32 (* x x) 0.0)"#;
    let out = emit(src, Tier::Max).unwrap();
    must(&out, "alias nelts_f32 = simd_width_of[DType.float32]()");
    must(&out, "fn sum_sq(x: UnsafePointer[Float32, MutAnyOrigin], n: Int) -> Float32:");
    must(&out, "var acc = SIMD[DType.float32, nelts_f32](0.0)");
    must(&out, "while i + nelts_f32 <= n:");
    must(&out, "var xv = x.load[width=nelts_f32](i)");
    must(&out, "acc += (xv * xv)");
    must(&out, "var tail: Float32 = acc.reduce_add()");
    must(&out, "return tail");
}

#[test]
fn reduce_product() {
    let src = r#"(reduce-mojo prod [^f32 x] ^f32 ^mul x 1.0)"#;
    let readable = emit(src, Tier::Readable).unwrap();
    must(&readable, "acc *= x[i]");
    let max = emit(src, Tier::Max).unwrap();
    must(&max, "reduce_mul()");
    must(&max, "tail *= ");
}

#[test]
fn reduce_min_and_max() {
    let src_min = r#"(reduce-mojo minv [^f32 x] ^f32 ^min x 1.0e30)"#;
    let out = emit(src_min, Tier::Readable).unwrap();
    must(&out, "acc = min(acc, x[i])");
    let max_tier = emit(src_min, Tier::Max).unwrap();
    must(&max_tier, "reduce_min()");
    must(&max_tier, "tail = min(tail, ");

    let src_max = r#"(reduce-mojo maxv [^f32 x] ^f32 ^max x -1.0e30)"#;
    let out = emit(src_max, Tier::Readable).unwrap();
    must(&out, "acc = max(acc, x[i])");
}

#[test]
fn reduce_dot_product_two_inputs() {
    let src = r#"(reduce-mojo dot [^f32 a ^f32 b] ^f32 (* a b) 0.0)"#;
    let readable = emit(src, Tier::Readable).unwrap();
    must(&readable, "fn dot(a: UnsafePointer[Float32, MutAnyOrigin], b: UnsafePointer[Float32, MutAnyOrigin], n: Int) -> Float32:");
    must(&readable, "acc += (a[i] * b[i])");
    let max = emit(src, Tier::Max).unwrap();
    must(&max, "var av = a.load[width=nelts_f32](i)");
    must(&max, "var bv = b.load[width=nelts_f32](i)");
    must(&max, "(av * bv)");
    must(&max, "reduce_add()");
}

#[test]
fn reduce_readable_vs_max_diverge() {
    let src = r#"(reduce-mojo s [^f32 x] ^f32 (* x x) 0.0)"#;
    let readable = emit(src, Tier::Readable).unwrap();
    let max = emit(src, Tier::Max).unwrap();
    assert_ne!(readable, max);
}

#[test]
fn reduce_rejects_bad_body() {
    let src = r#"(reduce-mojo bad [^f32 x] ^f32 (mystery x) 0.0)"#;
    let err = emit(src, Tier::Max).unwrap_err();
    assert!(err.contains("unsupported op `mystery`"), "got: {err}");
}

#[test]
fn reduce_and_elementwise_coexist_at_max() {
    let src = r#"
(elementwise-mojo vector-add [^f32 a ^f32 b] ^f32 (+ a b))
(reduce-mojo sum-sq [^f32 x] ^f32 (* x x) 0.0)
"#;
    let out = emit(src, Tier::Max).unwrap();
    must(&out, "fn vector_add(");
    must(&out, "fn sum_sq(");
    must(&out, "while i + nelts_f32 <= n:");
    // Alias should appear exactly once.
    let count = out.matches("alias nelts_f32").count();
    assert_eq!(count, 1, "expected one alias line, got {count}:\n{out}");
}


// ---------------- GPU kernels ----------------

#[test]
fn gpu_elementwise_vector_add_emits_kernel_shape() {
    let src = r#"(elementwise-gpu-mojo vector-add [^f32 a ^f32 b] ^f32 (+ a b))"#;
    let out = emit(src, Tier::Readable).unwrap();
    must(&out, "from gpu import thread_idx, block_idx, block_dim");
    must(&out, "fn vector_add(a: UnsafePointer[Float32, MutAnyOrigin], b: UnsafePointer[Float32, MutAnyOrigin], dst: UnsafePointer[Float32, MutAnyOrigin], n: Int):");
    must(&out, "var i = block_idx.x * block_dim.x + thread_idx.x");
    must(&out, "if i < n:");
    must(&out, "dst[i] = (a[i] + b[i])");
}

#[test]
fn gpu_elementwise_all_tiers_emit_kernel() {
    let src = r#"(elementwise-gpu-mojo scale [^f32 x] ^f32 (* x x))"#;
    for tier in [Tier::Readable, Tier::Optimized, Tier::Max] {
        let out = emit(src, tier).unwrap();
        must(&out, "fn scale(x: UnsafePointer[Float32, MutAnyOrigin], dst: UnsafePointer[Float32, MutAnyOrigin], n: Int):");
        must(&out, "var i = block_idx.x * block_dim.x + thread_idx.x");
    }
}

#[test]
fn gpu_elementwise_rejects_bad_body() {
    let src = r#"(elementwise-gpu-mojo bad [^f32 a] ^f32 (mystery a))"#;
    let err = emit(src, Tier::Readable).unwrap_err();
    assert!(err.contains("unsupported op `mystery`"), "got: {err}");
}
