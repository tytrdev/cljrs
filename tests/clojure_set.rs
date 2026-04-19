//! Integration tests for clojure.set. The namespace is loaded by
//! install_prelude, so a fresh Env exposes every fn under the
//! `clojure.set/...` prefix.

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

fn as_set(v: &Value) -> std::collections::BTreeSet<String> {
    match v {
        Value::Set(xs) => xs.iter().map(|x| format!("{x}")).collect(),
        other => panic!("expected set, got {other:?}"),
    }
}

#[test]
fn union_basic() {
    let v = run("(clojure.set/union #{1 2} #{2 3} #{3 4})");
    let s = as_set(&v);
    assert_eq!(s.len(), 4);
    for n in ["1", "2", "3", "4"] {
        assert!(s.contains(n), "missing {n} in {s:?}");
    }
}

#[test]
fn intersection_basic() {
    let v = run("(clojure.set/intersection #{1 2 3} #{2 3 4} #{3 5})");
    assert_eq!(as_set(&v), ["3".to_string()].into_iter().collect());
}

#[test]
fn difference_three_arg() {
    let v = run("(clojure.set/difference #{1 2 3 4} #{2} #{4})");
    let s = as_set(&v);
    assert_eq!(s, ["1".to_string(), "3".to_string()].into_iter().collect());
}

#[test]
fn subset_and_superset() {
    assert_eq!(run("(clojure.set/subset? #{1 2} #{1 2 3})"), Value::Bool(true));
    assert_eq!(run("(clojure.set/subset? #{1 4} #{1 2 3})"), Value::Bool(false));
    assert_eq!(run("(clojure.set/superset? #{1 2 3} #{1 2})"), Value::Bool(true));
}

#[test]
fn select_filters_set() {
    let v = run("(clojure.set/select even? #{1 2 3 4 5})");
    assert_eq!(as_set(&v), ["2".to_string(), "4".to_string()].into_iter().collect());
}

#[test]
fn map_invert_swaps() {
    let v = run("(clojure.set/map-invert {:a 1 :b 2})");
    match v {
        Value::Map(m) => {
            assert_eq!(m.len(), 2);
            assert_eq!(m.get(&Value::Int(1)).unwrap(), &Value::Keyword(std::sync::Arc::from("a")));
            assert_eq!(m.get(&Value::Int(2)).unwrap(), &Value::Keyword(std::sync::Arc::from("b")));
        }
        other => panic!("expected map, got {other:?}"),
    }
}

#[test]
fn rename_keys_basic() {
    let v = run("(clojure.set/rename-keys {:a 1 :b 2 :c 3} {:a :x :b :y})");
    match v {
        Value::Map(m) => {
            assert_eq!(m.get(&Value::Keyword(std::sync::Arc::from("x"))).unwrap(), &Value::Int(1));
            assert_eq!(m.get(&Value::Keyword(std::sync::Arc::from("y"))).unwrap(), &Value::Int(2));
            assert_eq!(m.get(&Value::Keyword(std::sync::Arc::from("c"))).unwrap(), &Value::Int(3));
            assert!(m.get(&Value::Keyword(std::sync::Arc::from("a"))).is_none());
        }
        other => panic!("expected map, got {other:?}"),
    }
}

#[test]
fn project_keeps_only_listed_keys() {
    let v = run(
        "(clojure.set/project #{{:a 1 :b 2 :c 3} {:a 4 :b 5 :c 6}} [:a :b])",
    );
    let s = as_set(&v);
    // Each record should contain only :a and :b, no :c.
    assert_eq!(s.len(), 2);
    for r in &s {
        assert!(!r.contains(":c"), "{r} still has :c");
    }
}

#[test]
fn rename_renames_records() {
    let v = run(
        "(clojure.set/rename #{{:a 1 :b 2}} {:a :x})",
    );
    let s = as_set(&v);
    assert_eq!(s.len(), 1);
    assert!(s.iter().next().unwrap().contains(":x"));
}

#[test]
fn index_buckets_by_key() {
    // Two records share :dept :eng, one is :dept :sales. Index by :dept.
    let n = run(
        "(count (clojure.set/index #{{:dept :eng :name \"a\"} {:dept :eng :name \"b\"} {:dept :sales :name \"c\"}} [:dept]))",
    );
    assert_eq!(n, Value::Int(2));
}

#[test]
fn join_natural_on_shared_key() {
    let n = run(
        "(count (clojure.set/join #{{:id 1 :name \"a\"} {:id 2 :name \"b\"}} #{{:id 1 :age 30} {:id 2 :age 40}}))",
    );
    assert_eq!(n, Value::Int(2));
}

#[test]
fn join_with_kmap_renames_then_joins() {
    let n = run(
        "(count (clojure.set/join #{{:id 1 :name \"a\"}} #{{:user-id 1 :age 30}} {:id :user-id}))",
    );
    assert_eq!(n, Value::Int(1));
}
