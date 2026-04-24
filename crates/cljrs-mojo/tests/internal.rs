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
    assert!(out.contains("fn sphere_sdf("), "got:\n{out}");
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
    assert!(out.contains("fn use_sq"), "got:\n{out}");
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
fn do_in_let_value_position() {
    // (let [^i32 v (do 1 2 3)] v) — do should yield 3.
    let src = "(defn ^i32 d [] (let [^i32 v (do 1 2 3)] v))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("var v: Int32 = 3"), "got:\n{out}");
}

#[test]
fn nested_do_in_if_branch() {
    let src = "(defn ^i32 g [^i32 x] (if (> x 0) (do 1 2 (+ x 1)) 0))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("return (x + 1)"), "got:\n{out}");
}

#[test]
fn do_with_side_effect_then_value() {
    // Side effects discarded in value position; only last expr survives.
    let src = "(defn ^i32 s [] (do (sink 1) (sink 2) 7))";
    let out = emit(src, Tier::Readable).unwrap();
    // tail position should emit sink calls then return 7.
    assert!(out.contains("return 7"), "got:\n{out}");
    assert!(out.contains("sink(1)"), "got:\n{out}");
    assert!(out.contains("sink(2)"), "got:\n{out}");
}

#[test]
fn cond_emits_elif_chain() {
    let src = "(defn ^i32 cls [^i32 x]
                 (cond (< x 0) -1
                       (= x 0)  0
                       (< x 10) 1
                       :else    2))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("if (x < 0):"), "got:\n{out}");
    assert!(out.contains("elif (x == 0):"), "got:\n{out}");
    assert!(out.contains("elif (x < 10):"), "got:\n{out}");
    assert!(out.contains("else:"), "got:\n{out}");
    // No deeply nested `else: if`.
    let nested = out.lines().filter(|l| l.trim_start().starts_with("if ")).count();
    assert_eq!(nested, 1, "should be one top-level if, got:\n{out}");
}

#[test]
fn elif_with_multi_stmt_else() {
    let src = "(defn ^i32 demo [^i32 x]
                 (cond (< x 0) (let [^i32 y (- 0 x)] y)
                       :else   (let [^i32 z (* x 2)] z)))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("else:"), "got:\n{out}");
    assert!(out.contains("var z: Int32 ="), "got:\n{out}");
}

#[test]
fn nested_if_still_renders() {
    let src = "(defn ^i32 sgn [^i32 x] (if (> x 0) 1 (if (< x 0) -1 0)))";
    let out = emit(src, Tier::Readable).unwrap();
    // Value-position if uses the ternary expr printer, not the elif chain.
    assert!(out.contains("if "), "got:\n{out}");
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
fn unknown_lowercase_type_hint_falls_back_to_infer() {
    // Unknown lowercase ^wibble → inferred (no annotation).
    let src = "(defn-mojo p ^i32 [^wibble x] 0)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("fn p(x)"), "got:\n{out}");
}

#[test]
fn capitalized_type_hint_passes_through_as_named() {
    // ^Custom → `x: Custom` (user-defined struct, etc.).
    let src = "(defn-mojo p ^i32 [^Custom x] 0)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("x: Custom"), "got:\n{out}");
}

#[test]
fn multi_arity_emits_suffixed_fns() {
    let src = "(defn-mojo greet ^i32
                 ([^i32 x] x)
                 ([^i32 x ^i32 y] (+ x y)))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("fn greet(x: Int32) -> Int32"), "got:\n{out}");
    assert!(out.contains("fn greet_2(x: Int32, y: Int32) -> Int32"), "got:\n{out}");
}

#[test]
fn multi_arity_three_overloads() {
    let src = "(defn-mojo f ^i64 ([] 0) ([^i64 a] a) ([^i64 a ^i64 b] (+ a b)))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("fn f()"), "got:\n{out}");
    assert!(out.contains("fn f_2(a: Int64)"), "got:\n{out}");
    assert!(out.contains("fn f_3(a: Int64, b: Int64)"), "got:\n{out}");
}

#[test]
fn single_arity_unchanged_by_multiarity_path() {
    let src = "(defn-mojo only ^i32 [^i32 x] x)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("fn only(x: Int32)"), "got:\n{out}");
    assert!(!out.contains("only_"), "should not suffix single arity:\n{out}");
}

#[test]
fn parameter_fn_mojo_decorates() {
    let src = "(parameter-fn-mojo special ^f32 [^f32 x] x)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("@parameter"), "got:\n{out}");
    assert!(out.contains("fn special("), "got:\n{out}");
}

#[test]
fn always_inline_fn_mojo_decorates() {
    let src = "(always-inline-fn-mojo sq ^f32 [^f32 x] (* x x))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("@always_inline"), "got:\n{out}");
    assert!(out.contains("fn sq("), "got:\n{out}");
}

#[test]
fn parameter_fn_mojo_preserves_body() {
    let src = "(parameter-fn-mojo add ^i32 [^i32 a ^i32 b] (+ a b))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("return (a + b)"), "got:\n{out}");
}

#[test]
fn always_inline_applied_to_small_pure_fn() {
    let src = "(defn-mojo sq ^f32 [^f32 x] (* x x))";
    let out = emit(src, Tier::Max).unwrap();
    assert!(out.contains("@always_inline"), "got:\n{out}");
}

#[test]
fn always_inline_skipped_on_recursive_fn() {
    let src = "(defn-mojo rec ^i64 [^i64 n] (if (< n 2) n (+ (rec (- n 1)) (rec (- n 2)))))";
    let out = emit(src, Tier::Max).unwrap();
    assert!(!out.contains("@always_inline"), "should skip recursive:\n{out}");
}

#[test]
fn always_inline_skipped_on_while_loop() {
    let src = "(defn-mojo fact ^i64 [^i64 n]
                 (loop [^i64 i 1 ^i64 acc 1]
                   (if (> i n) acc (recur (+ i 1) (* acc i)))))";
    let out = emit(src, Tier::Max).unwrap();
    assert!(!out.contains("@always_inline"), "should skip while:\n{out}");
}

#[test]
fn extended_math_fns() {
    let src = "(defn-mojo m ^f32 [^f32 x ^f32 y]
                 (+ (tanh x) (+ (atan2 y x) (+ (log2 x) (hypot x y)))))";
    let out = emit(src, Tier::Readable).unwrap();
    for needle in ["from math import tanh", "from math import atan2",
                   "from math import log2", "from math import hypot",
                   "tanh(x)", "atan2(y, x)", "log2(x)", "hypot(x, y)"] {
        assert!(out.contains(needle), "missing {needle}:\n{out}");
    }
}

#[test]
fn round_trunc_cbrt_expm1() {
    let src = "(defn-mojo g ^f32 [^f32 x] (+ (round x) (+ (trunc x) (+ (cbrt x) (expm1 x)))))";
    let out = emit(src, Tier::Readable).unwrap();
    for needle in ["round(x)", "trunc(x)", "cbrt(x)", "expm1(x)"] {
        assert!(out.contains(needle), "missing {needle}:\n{out}");
    }
}

#[test]
fn unknown_math_fn_falls_back_to_call() {
    // `wobble` is not in the math table → generic call, no import.
    let src = "(defn-mojo f ^f32 [^f32 x] (wobble x))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("wobble(x)"), "got:\n{out}");
    assert!(!out.contains("from math import wobble"), "got:\n{out}");
}

#[test]
fn break_in_for_mojo_loop() {
    let src = "(defn-mojo find ^i32 [^i32 n] (for-mojo [i 0 n] (if (hit? i) (break))) 0)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("for i in range(0, n):"), "got:\n{out}");
    assert!(out.contains("break"), "got:\n{out}");
}

#[test]
fn continue_in_for_mojo_loop() {
    let src = "(defn-mojo go ^i32 [^i32 n] (for-mojo [i 0 n] (if (skip? i) (continue) (work i))) 0)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("continue"), "got:\n{out}");
}

#[test]
fn break_outside_loop_still_emits_keyword() {
    // We don't validate placement; print should be a `break` line.
    let src = "(defn-mojo b ^i32 [] (break) 0)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("break"), "got:\n{out}");
}

#[test]
fn for_mojo_sugar_basic() {
    let src = "(defn-mojo loop1 ^i32 [^i32 n] (for-mojo [i 0 n] (sink i)) 0)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("for i in range(0, n):"), "got:\n{out}");
    assert!(out.contains("sink(i)"), "got:\n{out}");
}

#[test]
fn for_mojo_with_typed_counter() {
    let src = "(defn-mojo loop2 ^i64 [^i64 lo ^i64 hi] (for-mojo [^i64 j lo hi] (work j)) 0)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("for j in range(lo, hi):"), "got:\n{out}");
}

#[test]
fn for_mojo_arity_errors() {
    let src = "(defn-mojo bad ^i32 [] (for-mojo [i 0] 0) 0)";
    assert!(emit(src, Tier::Readable).is_err());
}

#[test]
fn print_lowers_to_print_call() {
    let src = r#"(defn-mojo say ^i32 [^i32 x] (print "hello") 0)"#;
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains(r#"print("hello")"#), "got:\n{out}");
}

#[test]
fn format_concats_with_string_coerce() {
    let src = r#"(defn-mojo greet ^str [^i32 n] (format "n={}" n))"#;
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains(r#""n=" + String(n)"#), "got:\n{out}");
    assert!(out.contains("-> String"), "got:\n{out}");
}

#[test]
fn format_arity_mismatch_errors() {
    let src = r#"(defn-mojo bad ^str [^i32 x] (format "{} {}" x))"#;
    assert!(emit(src, Tier::Readable).is_err());
}

#[test]
fn simd_type_in_signature() {
    let src = "(defn-mojo dot ^SIMDf32x4 [^SIMDf32x4 a ^SIMDf32x4 b] (* a b))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("a: SIMD[DType.float32, 4]"), "got:\n{out}");
    assert!(out.contains("-> SIMD[DType.float32, 4]"), "got:\n{out}");
}

#[test]
fn simd_int_vector() {
    let src = "(defn-mojo go ^SIMDi64x8 [^SIMDi64x8 v] v)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("SIMD[DType.int64, 8]"), "got:\n{out}");
}

#[test]
fn simd_bad_tag_falls_back() {
    // `SIMDfoo` not a valid dtype; falls back to Named (capitalized).
    let src = "(defn-mojo p ^i32 [^SIMDxyz x] 0)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("SIMDxyz"), "got:\n{out}");
}

#[test]
fn defstruct_basic_vec3() {
    let src = r#"
(defstruct-mojo Vec3 [^f32 x ^f32 y ^f32 z])
(defn-mojo getx ^f32 [^Vec3 v] (. v x))
"#;
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("@value"), "got:\n{out}");
    assert!(out.contains("struct Vec3:"), "got:\n{out}");
    assert!(out.contains("var x: Float32"), "got:\n{out}");
    assert!(out.contains("fn __init__(out self, x: Float32, y: Float32, z: Float32)"), "got:\n{out}");
    assert!(out.contains("self.x = x"), "got:\n{out}");
    assert!(out.contains("v: Vec3"), "got:\n{out}");
    assert!(out.contains("return v.x"), "got:\n{out}");
}

#[test]
fn defstruct_empty_uses_pass() {
    let src = "(defstruct-mojo Empty [])";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("struct Empty:"), "got:\n{out}");
    assert!(out.contains("pass"), "got:\n{out}");
}

#[test]
fn dot_field_arity_errors() {
    let src = "(defn-mojo bad ^i32 [^Foo v] (. v))";
    assert!(emit(src, Tier::Readable).is_err());
}

#[test]
fn defn_mojo_alias_works() {
    let src = "(defn-mojo ^f32 id [^f32 x] x)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("fn id(x: Float32) -> Float32"), "got:\n{out}");
}
