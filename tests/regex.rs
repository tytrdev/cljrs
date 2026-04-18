//! Regex literals (#"..."), re-find, re-matches, re-seq.

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
fn regex_literal_parses() {
    let v = run(r#"#"\d+""#);
    assert!(matches!(v, Value::Regex(_)));
}

#[test]
fn re_find_returns_first_match() {
    let v = run(r#"(re-find #"\d+" "abc 42 xyz 99")"#);
    assert_eq!(v, Value::Str("42".into()));
}

#[test]
fn re_find_with_groups_returns_vector() {
    // [whole "42"] expected: whole, first group, the two groups
    let v = run(r#"(re-find #"(\w+)=(\d+)" "port=42 host=foo")"#);
    match v {
        Value::Vector(xs) => {
            assert_eq!(xs.len(), 3);
            assert_eq!(xs[0], Value::Str("port=42".into()));
            assert_eq!(xs[1], Value::Str("port".into()));
            assert_eq!(xs[2], Value::Str("42".into()));
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn re_matches_only_full_string() {
    assert_eq!(
        run(r#"(re-matches #"\d+" "42")"#),
        Value::Str("42".into())
    );
    assert_eq!(run(r#"(re-matches #"\d+" "abc 42")"#), Value::Nil);
}

#[test]
fn re_seq_all_matches() {
    let v = run(r#"(re-seq #"\d+" "1 22 333 4444")"#);
    match v {
        Value::List(xs) => {
            assert_eq!(xs.len(), 4);
            assert_eq!(xs[0], Value::Str("1".into()));
            assert_eq!(xs[3], Value::Str("4444".into()));
        }
        _ => panic!("expected list"),
    }
}

#[test]
fn re_pattern_from_string() {
    let src = r#"
      (def digits (re-pattern "\\d+"))
      (re-find digits "count = 87 widgets")
    "#;
    assert_eq!(run(src), Value::Str("87".into()));
}
