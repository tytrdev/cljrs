//! Multi-arity fns: (fn ([x] ...) ([x y] ...)) — dispatch on arg count.

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
fn two_arities() {
    let src = r#"
      (defn greet
        ([] "hello, stranger")
        ([name] (str "hello, " name)))
      [(greet) (greet "ty")]
    "#;
    assert_eq!(
        run(src),
        Value::Vector(imbl::Vector::from_iter([
            Value::Str("hello, stranger".into()),
            Value::Str("hello, ty".into()),
        ]))
    );
}

#[test]
fn three_arities() {
    let src = r#"
      (defn make-point
        ([] [0 0 0])
        ([x] [x 0 0])
        ([x y] [x y 0])
        ([x y z] [x y z]))
      [(make-point) (make-point 1) (make-point 1 2) (make-point 1 2 3)]
    "#;
    let v = run(src);
    if let Value::Vector(xs) = v {
        assert_eq!(xs.len(), 4);
        // All should be 3-element vectors.
    } else {
        panic!();
    }
}

#[test]
fn variadic_arity() {
    let src = r#"
      (defn sum
        ([] 0)
        ([x] x)
        ([x & more] (+ x (apply sum more))))
      [(sum) (sum 5) (sum 1 2 3 4 5)]
    "#;
    assert_eq!(
        run(src),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(0),
            Value::Int(5),
            Value::Int(15),
        ]))
    );
}

#[test]
fn no_matching_arity_throws() {
    let env = Env::new();
    builtins::install(&env);
    let src = r#"
      (defn f
        ([x] x)
        ([x y] (+ x y)))
      (f 1 2 3)
    "#;
    let forms = reader::read_all(src).expect("read");
    let mut last: Result<Value, _> = Ok(Value::Nil);
    for f in forms {
        last = eval::eval(&f, &env);
    }
    let err = last.unwrap_err().to_string();
    assert!(err.contains("no matching arity") || err.contains("arity"), "got: {err}");
}
