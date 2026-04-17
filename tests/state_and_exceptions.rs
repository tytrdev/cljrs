//! Atoms + try/catch/throw + ex-info. The two state/control pillars
//! needed for Clojure-feeling code and Bret-Victor live coding.

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

fn run_err(src: &str) -> String {
    let env = Env::new();
    builtins::install(&env);
    let forms = reader::read_all(src).expect("read");
    let mut last: Result<Value, _> = Ok(Value::Nil);
    for f in forms {
        last = eval::eval(&f, &env);
        if last.is_err() {
            break;
        }
    }
    last.unwrap_err().to_string()
}

#[test]
fn atom_basic() {
    let src = r#"
      (def a (atom 0))
      (reset! a 42)
      @a
    "#;
    assert_eq!(run(src), Value::Int(42));
}

#[test]
fn atom_swap() {
    let src = r#"
      (def a (atom 10))
      (swap! a + 5)
      (swap! a (fn [v] (* v 2)))
      @a
    "#;
    assert_eq!(run(src), Value::Int(30));
}

#[test]
fn atom_cas() {
    let src = r#"
      (def a (atom 7))
      [(compare-and-set! a 7 99)
       (compare-and-set! a 7 100)
       @a]
    "#;
    // first succeeds, second fails because value is now 99
    assert_eq!(
        run(src),
        Value::Vector(imbl::Vector::from_iter([
            Value::Bool(true),
            Value::Bool(false),
            Value::Int(99),
        ]))
    );
}

#[test]
fn atom_predicate() {
    assert_eq!(run("(atom? (atom 1))"), Value::Bool(true));
    assert_eq!(run("(atom? 1)"), Value::Bool(false));
}

#[test]
fn try_catch_user_throw() {
    let src = r#"
      (try
        (throw (ex-info "boom" {:code 42}))
        (catch _ e
          (ex-data e)))
    "#;
    // ex-data on an ex-info map returns the data arg
    let v = run(src);
    match v {
        Value::Map(m) => {
            let k = Value::Keyword(std::sync::Arc::from("code"));
            assert_eq!(m.get(&k), Some(&Value::Int(42)));
        }
        other => panic!("expected map, got {other:?}"),
    }
}

#[test]
fn try_catches_eval_error() {
    let src = r#"
      (try
        (+ 1 :oops)
        (catch _ e :caught))
    "#;
    assert_eq!(run(src), Value::Keyword("caught".into()));
}

#[test]
fn try_finally_runs_on_throw() {
    let src = r#"
      (def flag (atom :init))
      (try
        (try
          (throw "x")
          (finally (reset! flag :ran)))
        (catch _ _ nil))
      @flag
    "#;
    assert_eq!(run(src), Value::Keyword("ran".into()));
}

#[test]
fn throw_without_catch_propagates() {
    let err = run_err(r#"(throw "boom")"#);
    assert!(err.contains("boom"), "got: {err}");
}

#[test]
fn ex_message_accessor() {
    let src = r#"(ex-message (ex-info "hello" {}))"#;
    assert_eq!(run(src), Value::Str("hello".into()));
}
