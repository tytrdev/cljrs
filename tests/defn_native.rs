//! Tests for the `defn-native` special form and the `^Type` reader syntax.
//! Phase-1 checks: parsing, validation, and correct fallthrough execution
//! via the tree-walker. Phase 2 will add tests that assert these fns are
//! genuinely JIT-compiled to native code.

use cljrs::{builtins, env::Env, eval, reader, value::Value};

fn run(src: &str) -> Value {
    let env = Env::new();
    builtins::install(&env);
    let forms = reader::read_all(src).expect("read failed");
    let mut result = Value::Nil;
    for f in forms {
        result = eval::eval(&f, &env).expect("eval failed");
    }
    result
}

fn run_result(src: &str) -> Result<Value, String> {
    let env = Env::new();
    builtins::install(&env);
    let forms = reader::read_all(src).map_err(|e| e.to_string())?;
    let mut result = Value::Nil;
    for f in forms {
        result = eval::eval(&f, &env).map_err(|e| e.to_string())?;
    }
    Ok(result)
}

#[test]
fn type_hint_is_transparent_in_tree_walker() {
    // `^i64 42` reads as (__tagged__ i64 42) → tree-walker evaluates the
    // inner form and ignores the tag.
    assert_eq!(run("^i64 42"), Value::Int(42));
    assert_eq!(run("(+ ^i64 1 ^i64 2)"), Value::Int(3));
}

#[test]
fn defn_native_with_return_and_param_hints() {
    let src = r#"
        (defn-native square ^i64 [^i64 n]
          (* n n))
        (square 7)
    "#;
    assert_eq!(run(src), Value::Int(49));
}

#[test]
fn defn_native_without_return_hint_defaults_ok() {
    let src = r#"
        (defn-native add ^i64 [^i64 a ^i64 b]
          (+ a b))
        (add 10 32)
    "#;
    assert_eq!(run(src), Value::Int(42));
}

#[test]
fn defn_native_loop_recur_via_helper_fn() {
    // Phase M1.1a: idiomatic loop/recur compiles natively. The emitter
    // spawns a helper fn in the same module; recur is a tail self-call;
    // LLVM -O3 TCOs it into a machine-level loop.
    let src = r#"
        (defn-native sum-to ^i64 [^i64 n]
          (loop [i 0 acc 0]
            (if (> i n) acc (recur (+ i 1) (+ acc i)))))
        (sum-to 100)
    "#;
    assert_eq!(run(src), Value::Int(5050));
}

#[cfg(feature = "mlir")]
#[test]
fn defn_native_cross_fn_calls() {
    // M1.1b: one defn-native calls another. Emitter sees the call,
    // looks up the other fn in the registry, emits `func.call @other`
    // + a forward declaration. compile.rs registers the already-JIT'd
    // fn's pointer with the new ExecutionEngine before lookup.
    let src = r#"
        (defn-native square ^i64 [^i64 n] (* n n))
        (defn-native sum-of-squares ^i64 [^i64 n]
          (if (= n 0) 0 (+ (square n) (sum-of-squares (- n 1)))))
        (sum-of-squares 10)
    "#;
    // 1 + 4 + 9 + 16 + 25 + 36 + 49 + 64 + 81 + 100 = 385
    assert_eq!(run(src), Value::Int(385));
}

#[test]
fn defn_native_fib_recursive() {
    let src = r#"
        (defn-native fib ^i64 [^i64 n]
          (if (< n 2)
            n
            (+ (fib (- n 1)) (fib (- n 2)))))
        (fib 10)
    "#;
    assert_eq!(run(src), Value::Int(55));
}

#[test]
fn defn_native_accepts_long_alias() {
    // Clojure-compatible `^long` should resolve to i64.
    let src = r#"
        (defn-native inc1 ^long [^long n] (+ n 1))
        (inc1 41)
    "#;
    assert_eq!(run(src), Value::Int(42));
}

#[test]
fn defn_native_rejects_untyped_params() {
    let res = run_result(
        r#"(defn-native bad [x] x)"#,
    );
    assert!(res.is_err(), "expected error for untyped param, got {res:?}");
    let err = res.unwrap_err();
    assert!(
        err.contains("every param must be ^Type name"),
        "unexpected error: {err}"
    );
}

#[test]
fn defn_native_rejects_unknown_type() {
    let res = run_result(
        r#"(defn-native bad ^widget [^widget x] x)"#,
    );
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.contains("unknown type hint"), "unexpected error: {err}");
}

#[test]
fn defn_native_rejects_non_vector_params() {
    let res = run_result(r#"(defn-native bad ^i64 not-a-vec (+ 1 2))"#);
    assert!(res.is_err());
}

#[test]
fn defn_native_requires_name_symbol() {
    let res = run_result(r#"(defn-native 42 ^i64 [^i64 n] n)"#);
    assert!(res.is_err());
}

/// f64 now works under both feature paths.
#[test]
fn float_hint_parses_and_adds() {
    let src = r#"
        (defn-native plus ^f64 [^f64 a ^f64 b] (+ a b))
        (plus 1.5 2.5)
    "#;
    assert_eq!(run(src), Value::Float(4.0));
}

/// Bool at FFI boundary isn't wired yet; tree-walker accepts it.
#[cfg(not(feature = "mlir"))]
#[test]
fn bool_hint_parses_tree_walker_only() {
    let src = r#"
        (defn-native always-true ^bool [^bool x] x)
        (always-true true)
    "#;
    assert_eq!(run(src), Value::Bool(true));
}

/// With the `mlir` feature on, bool params/return still error — phase-2
/// doesn't wire them at the FFI boundary (LLVM i1 ABI is finicky).
#[cfg(feature = "mlir")]
#[test]
fn mlir_feature_rejects_bool_at_boundary() {
    let res_bool = run_result(r#"(defn-native yes ^bool [^bool x] x)"#);
    assert!(res_bool.is_err());
}
