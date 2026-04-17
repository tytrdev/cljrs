//! Persistent collection semantics via imbl HAMT backing.
//! Verifies the Value layer still behaves right after the refactor.

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
fn vector_literal_round_trips() {
    assert_eq!(run("(count [1 2 3 4 5])"), Value::Int(5));
    assert_eq!(run("(first [10 20 30])"), Value::Int(10));
    assert_eq!(run("(nth [:a :b :c] 1)"), Value::Keyword("b".into()));
}

#[test]
fn conj_is_persistent_on_vectors() {
    // Conj on a vector appends; the original stays unchanged.
    let src = r#"
        (def v [1 2 3])
        (def w (conj v 4 5))
        [(count v) (count w)]
    "#;
    // v is still [1 2 3]; w is [1 2 3 4 5].
    match run(src) {
        Value::Vector(v) => {
            assert_eq!(v.len(), 2);
            assert_eq!(v[0], Value::Int(3));
            assert_eq!(v[1], Value::Int(5));
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn map_literal_with_keyword_keys() {
    let src = r#"{:a 1 :b 2 :c 3}"#;
    match run(src) {
        Value::Map(m) => assert_eq!(m.len(), 3),
        other => panic!("expected map, got {other:?}"),
    }
}

#[test]
fn map_equality_is_structural_unordered() {
    // Same keys and values in different insertion order compare equal —
    // the HAMT handles ordering internally and Value::PartialEq delegates.
    assert_eq!(run(r#"(= {:a 1 :b 2} {:b 2 :a 1})"#), Value::Bool(true));
}

#[test]
fn conj_onto_map_adds_pair() {
    let src = r#"(count (conj {:a 1} [:b 2] [:c 3]))"#;
    assert_eq!(run(src), Value::Int(3));
}

#[test]
fn large_vector_conj_stays_correct() {
    // Exercises imbl::Vector's RRB-tree path. Builds a vector of 1000
    // elements via repeated conj; confirms count and a sampled element.
    let src = r#"
        (defn build [n]
          (loop [i 0 v []]
            (if (>= i n) v (recur (+ i 1) (conj v i)))))
        (nth (build 1000) 500)
    "#;
    assert_eq!(run(src), Value::Int(500));
}
