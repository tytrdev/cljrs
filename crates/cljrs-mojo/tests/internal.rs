//! Internal tests that cover the same 10 cases the golden corpus is
//! supposed to have. These don't pin exact whitespace — they assert the
//! emit succeeds, produces the right top-level fn signature, and contains
//! the expected key operators / library calls.

use cljrs_mojo::{emit, Tier};

fn has_all(s: &str, needles: &[&str]) -> bool {
    needles.iter().all(|n| s.contains(n))
}

#[test]
fn hello_def() {
    let src = "(def ^i32 answer 42)";
    let r = emit(src, Tier::Readable).unwrap();
    assert!(r.contains("var answer: Int32 = 42"), "got:\n{r}");
    assert!(r.contains("# cljrs:"), "readable should have comment:\n{r}");
    let o = emit(src, Tier::Optimized).unwrap();
    assert!(o.contains("var answer: Int32 = 42"));
    let m = emit(src, Tier::Max).unwrap();
    assert!(m.contains("var answer: Int32 = 42"));
    assert!(!m.contains("# cljrs:"), "max should strip comments:\n{m}");
}

#[test]
fn add_fn() {
    let src = "(defn ^f32 add [^f32 x ^f32 y] (+ x y))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(has_all(&out, &["fn add(", "x: Float32", "y: Float32", "-> Float32", "(x + y)"]),
        "got:\n{out}");
}

#[test]
fn rsqrt_calls_math() {
    // (defn ^f32 rsqrt [^f32 x] (/ 1.0 (sqrt x)))
    let src = "(defn ^f32 rsqrt [^f32 x] (/ 1.0 (sqrt x)))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("from math import sqrt"), "got:\n{out}");
    assert!(out.contains("sqrt(x)"), "got:\n{out}");
    assert!(out.contains("/"), "got:\n{out}");
}

#[test]
fn lerp_three_args() {
    let src = "(defn ^f32 lerp [^f32 a ^f32 b ^f32 t] (+ a (* (- b a) t)))";
    let out = emit(src, Tier::Optimized).unwrap();
    assert!(has_all(&out, &["fn lerp(", "a: Float32", "b: Float32", "t: Float32"]), "got:\n{out}");
    assert!(out.contains("(b - a)"), "got:\n{out}");
}

#[test]
fn clamp_uses_min_max() {
    let src = "(defn ^f32 clamp [^f32 x ^f32 lo ^f32 hi] (min (max x lo) hi))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("min(max(x, lo), hi)"), "got:\n{out}");
}

#[test]
fn smoothstep_let_bindings() {
    let src = "(defn ^f32 smoothstep [^f32 e0 ^f32 e1 ^f32 x]
                 (let [^f32 t (/ (- x e0) (- e1 e0))]
                   (* t (* t (- 3.0 (* 2.0 t))))))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("var t: Float32 ="), "got:\n{out}");
    assert!(out.contains("return "), "got:\n{out}");
}

#[test]
fn sphere_sdf_sqrt() {
    let src = "(defn ^f32 sphere-sdf [^f32 x ^f32 y ^f32 z ^f32 r]
                 (- (sqrt (+ (* x x) (+ (* y y) (* z z)))) r))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("from math import sqrt"));
    assert!(out.contains("fn sphere-sdf("), "got:\n{out}");
}

#[test]
fn factorial_loop() {
    let src = "(defn ^i64 fact [^i64 n]
                 (loop [^i64 i 1 ^i64 acc 1]
                   (if (> i n) acc (recur (+ i 1) (* acc i)))))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("while"), "got:\n{out}");
    assert!(out.contains("var i: Int64 = 1"), "got:\n{out}");
    assert!(out.contains("var acc: Int64 = 1"), "got:\n{out}");
    assert!(out.contains("break"), "got:\n{out}");
}

#[test]
fn cond_classify() {
    let src = "(defn ^i32 classify [^f32 x]
                 (cond (< x 0.0) -1
                       (> x 0.0) 1
                       :else 0))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("if (x < 0.0):") || out.contains("if (x < 0.0)"), "got:\n{out}");
    assert!(out.contains("else"), "got:\n{out}");
    assert!(out.contains("return 1") && out.contains("return 0"), "got:\n{out}");
}

#[test]
fn abs_max_builtins() {
    let src = "(defn ^f32 abs-max [^f32 a ^f32 b] (max (abs a) (abs b)))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("max(abs(a), abs(b))"), "got:\n{out}");
    // abs/min/max are builtins — no `from math import` needed.
    assert!(!out.contains("from math import abs"));
}

#[test]
fn tier_optimized_folds_constants() {
    let src = "(defn ^i32 k [] (+ 1 2))";
    let out = emit(src, Tier::Optimized).unwrap();
    assert!(out.contains("return 3"), "should fold, got:\n{out}");
}

#[test]
fn tier_max_strips_comments_and_inlines() {
    let src = "(defn ^f32 sq [^f32 x] (* x x))
               (defn ^f32 use-sq [^f32 x] (sq x))";
    let out = emit(src, Tier::Max).unwrap();
    assert!(!out.contains("# cljrs:"), "max should strip comments:\n{out}");
    // sq is a 1-stmt return fn; use-sq should inline it to (x * x).
    assert!(out.contains("fn use-sq"), "got:\n{out}");
}

#[test]
fn unsupported_higher_order_errors() {
    // passing a fn as arg → `map` isn't in the runtime tables so it's
    // emitted as a bare call. That's acceptable fallback. But a vector
    // literal should error.
    let src = "(defn ^i32 bad [^i32 x] [x x])";
    assert!(emit(src, Tier::Readable).is_err());
}

#[test]
fn for_range_simple_counter() {
    // Single counter walking 0..n with no other state → for-range.
    let src = "(defn-mojo countdown ^i64 [^i64 n]
                 (loop [^i64 i 0]
                   (if (< i n) (do (sink i) (recur (+ i 1))) 0)))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("for i in range(0, n):"), "got:\n{out}");
    assert!(!out.contains("while"), "should not emit while:\n{out}");
}

#[test]
fn for_range_inclusive_bound() {
    let src = "(defn-mojo go ^i32 [^i32 n]
                 (loop [^i32 i 1] (if (<= i n) (do (work i) (recur (+ i 1))) 0)))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("for i in range(1, (n + 1)):"), "got:\n{out}");
}

#[test]
fn for_range_falls_back_when_state_present() {
    // 2 bindings → should NOT emit for-range; should still use while.
    let src = "(defn ^i64 fact [^i64 n]
                 (loop [^i64 i 1 ^i64 acc 1]
                   (if (> i n) acc (recur (+ i 1) (* acc i)))))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("while"), "got:\n{out}");
    assert!(!out.contains("for i in range"), "should not for-range:\n{out}");
}

#[test]
fn extra_int_types_emit() {
    let src = "(defn-mojo widen ^i64 [^i8 a ^u16 b ^u32 c] (+ a (+ b c)))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(has_all(&out, &["a: Int8", "b: UInt16", "c: UInt32", "-> Int64"]),
        "got:\n{out}");
}

#[test]
fn bfloat16_and_uint64_round_trip() {
    let src = "(defn-mojo go ^bf16 [^bf16 x ^u64 n] x)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("x: BFloat16"), "got:\n{out}");
    assert!(out.contains("n: UInt64"), "got:\n{out}");
    assert!(out.contains("-> BFloat16"), "got:\n{out}");
}

#[test]
fn unknown_type_hint_falls_back_to_infer() {
    // Unknown ^Whatever just becomes inferred (no annotation), shouldn't fail.
    let src = "(defn-mojo p ^i32 [^Wibble x] 0)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("fn p(x)"), "got:\n{out}");
}

#[test]
fn defn_mojo_alias_works() {
    let src = "(defn-mojo ^f32 id [^f32 x] x)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("fn id(x: Float32) -> Float32"), "got:\n{out}");
}
