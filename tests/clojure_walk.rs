//! Integration tests for clojure.walk + clojure.edn. Both namespaces
//! are loaded by install_prelude.

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

#[test]
fn postwalk_inc_all_ints() {
    let v = run("(clojure.walk/postwalk (fn [x] (if (number? x) (inc x) x)) [1 [2 3] 4])");
    let expected = run("[2 [3 4] 5]");
    assert_eq!(v, expected);
}

#[test]
fn prewalk_visits_outer_first() {
    // f rewrites lists to their first element; postwalk would never
    // see the inner list, prewalk does.
    let v = run("(clojure.walk/prewalk (fn [x] (if (number? x) (inc x) x)) {:a 1 :b [2 3]})");
    let expected = run("{:a 2 :b [3 4]}");
    assert_eq!(v, expected);
}

#[test]
fn postwalk_replace_substitutes() {
    let v = run("(clojure.walk/postwalk-replace {:a 1 :b 2} [:a :b :c])");
    let expected = run("[1 2 :c]");
    assert_eq!(v, expected);
}

#[test]
fn prewalk_replace_substitutes() {
    let v = run("(clojure.walk/prewalk-replace {1 :one} [1 [1 2]])");
    let expected = run("[:one [:one 2]]");
    assert_eq!(v, expected);
}

#[test]
fn keywordize_keys_only_strings() {
    let v = run("(clojure.walk/keywordize-keys {\"a\" 1 \"b\" {\"c\" 2}})");
    let expected = run("{:a 1 :b {:c 2}}");
    assert_eq!(v, expected);
}

#[test]
fn stringify_keys_only_keywords() {
    let v = run("(clojure.walk/stringify-keys {:a 1 :b {:c 2}})");
    let expected = run("{\"a\" 1 \"b\" {\"c\" 2}}");
    assert_eq!(v, expected);
}

#[test]
fn walk_one_layer() {
    // inner doubles, outer wraps in vector — only depth 1 affected.
    let v = run("(clojure.walk/walk inc identity [1 2 3])");
    let expected = run("[2 3 4]");
    assert_eq!(v, expected);
}

#[test]
fn postwalk_demo_returns_nil() {
    assert_eq!(run("(clojure.walk/postwalk-demo [1 2 3])"), Value::Nil);
}

#[test]
fn prewalk_demo_returns_nil() {
    assert_eq!(run("(clojure.walk/prewalk-demo [1 2 3])"), Value::Nil);
}

#[test]
fn macroexpand_all_expands_when() {
    // `when` is a macro that expands to (if test (do body...)).
    // After macroexpand-all on `(when true 1)`, the head should be `if`.
    let v = run("(first (clojure.walk/macroexpand-all (quote (when true 1))))");
    assert_eq!(v, Value::Symbol(std::sync::Arc::from("if")));
}

#[test]
fn edn_read_string_roundtrip() {
    let v = run("(clojure.edn/read-string \"[1 2 3]\")");
    assert_eq!(v, run("[1 2 3]"));
}

#[test]
fn edn_read_alias() {
    let v = run("(clojure.edn/read \"{:a 1}\")");
    assert_eq!(v, run("{:a 1}"));
}
