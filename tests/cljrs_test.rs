//! Smoke-tests for the cljrs.test macro layer. We author a handful of
//! deftests in cljrs (a mix of passing, failing, erroring, and thrown?
//! assertions), run them via `(run-tests)`, and verify the returned
//! summary map matches the expected counts.

use cljrs::{builtins, env::Env, eval, reader, value::Value};
use std::sync::Arc;

const SCRIPT: &str = r#"
(deftest passes-simple
  (is (= 1 1))
  (is (+ 1 1)))

(deftest passes-with-testing
  (testing "outer"
    (testing "inner"
      (is (= :a :a)))))

(deftest fails-equality
  (is (= 1 2)))

(deftest fails-truthy
  (is false "should have been true"))

(deftest errors-out
  (throw (ex-info "boom" {:why :manual})))

(deftest thrown-ok
  (is (thrown? (throw (ex-info "x" {})))))

(deftest thrown-with-type
  (is (thrown? :any (throw (ex-info "y" {})))))

(deftest thrown-but-doesnt
  (is (thrown? (+ 1 2))))
"#;

fn map_get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    if let Value::Map(m) = v {
        m.get(&Value::Keyword(Arc::from(key)))
    } else {
        None
    }
}

fn as_int(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        other => panic!("expected int, got {other:?}"),
    }
}

#[test]
fn run_tests_returns_summary_map() {
    let env = Env::new();
    builtins::install(&env);
    for f in reader::read_all(SCRIPT).unwrap() {
        eval::eval(&f, &env).expect("script eval");
    }

    let mut result = Value::Nil;
    for f in reader::read_all("(run-tests)").unwrap() {
        result = eval::eval(&f, &env).expect("run-tests");
    }

    // 8 deftests registered above.
    assert_eq!(as_int(map_get(&result, "tests").expect("tests key")), 8);

    // fails-equality, fails-truthy, thrown-but-doesnt — 3 failures.
    assert_eq!(as_int(map_get(&result, "failures").expect("failures key")), 3);

    // errors-out throws outside of `is`, so counted as an error.
    assert_eq!(as_int(map_get(&result, "errors").expect("errors key")), 1);

    // Assertions: 2 + 1 + 1 + 1 + 0 + 1 + 1 + 1 = 8.
    assert_eq!(
        as_int(map_get(&result, "assertions").expect("assertions key")),
        8
    );
}

#[test]
fn rerunning_deftest_replaces_by_name() {
    let env = Env::new();
    builtins::install(&env);

    for f in reader::read_all("(deftest only-one (is (= 1 1)))").unwrap() {
        eval::eval(&f, &env).unwrap();
    }
    // Redefine the same test.
    for f in reader::read_all("(deftest only-one (is (= 2 2)))").unwrap() {
        eval::eval(&f, &env).unwrap();
    }
    let mut result = Value::Nil;
    for f in reader::read_all("(run-tests)").unwrap() {
        result = eval::eval(&f, &env).unwrap();
    }
    assert_eq!(as_int(map_get(&result, "tests").unwrap()), 1);
    assert_eq!(as_int(map_get(&result, "failures").unwrap()), 0);
    assert_eq!(as_int(map_get(&result, "errors").unwrap()), 0);
}
