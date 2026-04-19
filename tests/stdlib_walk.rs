//! Bug-hunting suite for clojure.walk and clojure.edn.

use cljrs::{builtins, env::Env, eval, reader, value::Value};
use std::sync::Arc;

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

fn s(x: &str) -> Value {
    Value::Str(Arc::from(x))
}
fn k(x: &str) -> Value {
    Value::Keyword(Arc::from(x))
}

// --- walk ---------------------------------------------------------------

#[test]
fn walk_vector_inner_outer() {
    // (walk inc identity [1 2 3]) → [2 3 4].
    let v = run("(clojure.walk/walk inc identity [1 2 3])");
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(2),
            Value::Int(3),
            Value::Int(4),
        ]))
    );
}

#[test]
fn walk_list_returns_list() {
    let v = run("(clojure.walk/walk inc identity '(1 2 3))");
    assert!(matches!(v, Value::List(_)), "got {v:?}");
}

#[test]
fn walk_set_keeps_set() {
    let v = run("(clojure.walk/walk inc identity #{1 2 3})");
    assert!(matches!(v, Value::Set(_)));
}

#[test]
fn walk_map_inner_on_pairs() {
    // inner gets [k v] pairs.
    let v = run(
        "(clojure.walk/walk
            (fn [pair] [(first pair) (inc (second pair))])
            identity
            {:a 1 :b 2})",
    );
    assert_eq!(run("(:a (clojure.walk/walk
            (fn [pair] [(first pair) (inc (second pair))])
            identity
            {:a 1 :b 2}))"), Value::Int(2));
    let _ = v;
}

#[test]
fn walk_nonseq_just_outer() {
    // (walk inc str 5) — 5 is not a coll, inner skipped, outer applied.
    assert_eq!(run("(clojure.walk/walk inc str 5)"), s("5"));
}

// --- postwalk / prewalk ------------------------------------------------

#[test]
fn postwalk_increments_all_ints_in_nested() {
    let v = run(
        "(clojure.walk/postwalk
            (fn [x] (if (integer? x) (inc x) x))
            [1 [2 [3 [4]]]])",
    );
    // Should yield [2 [3 [4 [5]]]].
    let inner = run(
        "(get-in (clojure.walk/postwalk
                    (fn [x] (if (integer? x) (inc x) x))
                    [1 [2 [3 [4]]]])
                 [1 1 1 0])",
    );
    assert_eq!(inner, Value::Int(5));
    let _ = v;
}

#[test]
fn postwalk_on_map_visits_kv_and_pairs() {
    // Increment each int in a nested map.
    let v = run(
        "(get-in (clojure.walk/postwalk
                    (fn [x] (if (integer? x) (* x 10) x))
                    {:a 1 :b {:c 2}})
                 [:b :c])",
    );
    assert_eq!(v, Value::Int(20));
}

#[test]
fn prewalk_replace_substitutes() {
    let v = run("(clojure.walk/prewalk-replace {:a 1 :b 2} [:a :b :c])");
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(1),
            Value::Int(2),
            k("c"),
        ]))
    );
}

#[test]
fn postwalk_replace_substitutes_nested() {
    let v = run("(get (clojure.walk/postwalk-replace {:old :new} {:k :old}) :k)");
    assert_eq!(v, k("new"));
}

#[test]
fn postwalk_on_leaf_passes_through() {
    let v = run("(clojure.walk/postwalk identity 5)");
    assert_eq!(v, Value::Int(5));
}

// --- keywordize-keys / stringify-keys ----------------------------------

#[test]
fn keywordize_keys_converts_string_keys() {
    let v = run(r#"(get (clojure.walk/keywordize-keys {"a" 1 "b" 2}) :a)"#);
    assert_eq!(v, Value::Int(1));
}

#[test]
fn keywordize_keys_recursive() {
    let v = run(r#"(get-in (clojure.walk/keywordize-keys {"a" {"b" 1}}) [:a :b])"#);
    assert_eq!(v, Value::Int(1));
}

#[test]
fn keywordize_keys_leaves_non_string_alone() {
    // Numeric keys preserved.
    let v = run("(get (clojure.walk/keywordize-keys {1 :v}) 1)");
    assert_eq!(v, k("v"));
}

#[test]
fn stringify_keys_converts_keyword_keys() {
    let v = run(r#"(get (clojure.walk/stringify-keys {:a 1}) "a")"#);
    assert_eq!(v, Value::Int(1));
}

#[test]
fn stringify_keys_recursive() {
    let v = run(r#"(get-in (clojure.walk/stringify-keys {:a {:b 1}}) ["a" "b"])"#);
    assert_eq!(v, Value::Int(1));
}

#[test]
fn keywordize_then_stringify_round_trip() {
    let v = run(
        r#"(= {"a" 1}
              (clojure.walk/stringify-keys
                (clojure.walk/keywordize-keys {"a" 1})))"#,
    );
    assert_eq!(v, Value::Bool(true));
}

// --- macroexpand-all ---------------------------------------------------

#[test]
fn macroexpand_all_expands_when() {
    // (when x y) → (if x (do y))
    let v = run("(clojure.walk/macroexpand-all '(when true 1))");
    // Should be a list whose head is `if`.
    let s = format!("{}", v.to_pr_string());
    assert!(s.contains("if"), "expected 'if' in expansion, got {s}");
}

// --- demo (just checks no-crash) ---------------------------------------

#[test]
fn postwalk_demo_returns_nil() {
    assert_eq!(run("(clojure.walk/postwalk-demo [1 2 3])"), Value::Nil);
}

#[test]
fn prewalk_demo_returns_nil() {
    assert_eq!(run("(clojure.walk/prewalk-demo [1 2 3])"), Value::Nil);
}

// --- clojure.edn -------------------------------------------------------

#[test]
fn edn_read_string_int() {
    assert_eq!(run(r#"(clojure.edn/read-string "42")"#), Value::Int(42));
}

#[test]
fn edn_read_string_float() {
    assert_eq!(run(r#"(clojure.edn/read-string "3.14")"#), Value::Float(3.14));
}

#[test]
fn edn_read_string_nil_bool() {
    assert_eq!(run(r#"(clojure.edn/read-string "nil")"#), Value::Nil);
    assert_eq!(run(r#"(clojure.edn/read-string "true")"#), Value::Bool(true));
    assert_eq!(run(r#"(clojure.edn/read-string "false")"#), Value::Bool(false));
}

#[test]
fn edn_read_string_keyword_symbol() {
    assert_eq!(run(r#"(clojure.edn/read-string ":foo")"#), k("foo"));
    let v = run(r#"(clojure.edn/read-string "bar")"#);
    assert_eq!(v, Value::Symbol(Arc::from("bar")));
}

#[test]
fn edn_read_string_string_with_escapes() {
    assert_eq!(run(r#"(clojure.edn/read-string "\"a\\nb\"")"#), s("a\nb"));
}

#[test]
fn edn_read_string_vector() {
    assert_eq!(
        run(r#"(clojure.edn/read-string "[1 2 3]")"#),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
        ]))
    );
}

#[test]
fn edn_read_string_map() {
    let v = run(r#"(get (clojure.edn/read-string "{:a 1}") :a)"#);
    assert_eq!(v, Value::Int(1));
}

#[test]
fn edn_read_string_set() {
    let v = run("(count (clojure.edn/read-string \"#{1 2 3}\"))");
    assert_eq!(v, Value::Int(3));
}

#[test]
fn edn_read_string_nested() {
    let v = run(r#"(get-in (clojure.edn/read-string "{:a [1 {:b 2}]}") [:a 1 :b])"#);
    assert_eq!(v, Value::Int(2));
}

#[test]
fn edn_read_two_arg_opts() {
    // clojure.edn/read-string takes (opts s) too.
    assert_eq!(
        run(r#"(clojure.edn/read-string {} "42")"#),
        Value::Int(42)
    );
}

#[test]
fn edn_read_alias_for_read_string() {
    assert_eq!(run(r#"(clojure.edn/read "42")"#), Value::Int(42));
}
