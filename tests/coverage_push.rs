//! Clojure coverage push: some-> / some->> / cond-> / cond->> / for.

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
fn some_arrow_short_circuits_on_nil() {
    assert_eq!(
        run("(some-> {:a 1} :a inc)"),
        Value::Int(2)
    );
    assert_eq!(
        run("(some-> {:a 1} :missing inc)"),
        Value::Nil
    );
}

#[test]
fn some_arrow_thread_last() {
    assert_eq!(
        run("(some->> [1 2 3] (map inc) (reduce +))"),
        Value::Int(9)
    );
}

#[test]
fn cond_arrow_conditional_steps() {
    // Conditionally apply each transformation.
    let src = r#"
      (defn transform [x upper? double?]
        (cond-> x
          upper?  (* 10)
          double? (* 2)))
      [(transform 5 true true)   ; 5 * 10 * 2 = 100
       (transform 5 true false)  ; 5 * 10 = 50
       (transform 5 false true)  ; 5 * 2 = 10
       (transform 5 false false)]
    "#;
    assert_eq!(
        run(src),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(100),
            Value::Int(50),
            Value::Int(10),
            Value::Int(5),
        ]))
    );
}

#[test]
fn for_basic() {
    assert_eq!(
        run("(for [x [1 2 3]] (* x x))"),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(1),
            Value::Int(4),
            Value::Int(9),
        ]))
    );
}

#[test]
fn for_with_when() {
    assert_eq!(
        run("(for [x (range 10) :when (even? x)] (* x x))"),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(0),
            Value::Int(4),
            Value::Int(16),
            Value::Int(36),
            Value::Int(64),
        ]))
    );
}
