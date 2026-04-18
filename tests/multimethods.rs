//! defmulti / defmethod — open dispatch on the dispatch fn's result.

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
fn multi_keyword_dispatch() {
    let src = r#"
      (defmulti area :shape)
      (defmethod area :circle [s] (* 3.14159 (* (:r s) (:r s))))
      (defmethod area :square [s] (* (:side s) (:side s)))
      (defmethod area :default [_] -1)
      [(area {:shape :circle :r 2.0})
       (area {:shape :square :side 3})
       (area {:shape :unknown})]
    "#;
    let v = run(src);
    if let Value::Vector(xs) = v {
        assert_eq!(xs.len(), 3);
        // circle: pi * 4
        assert!(matches!(&xs[0], Value::Float(f) if (*f - 12.56636).abs() < 1e-3));
        assert_eq!(xs[1], Value::Int(9));
        assert_eq!(xs[2], Value::Int(-1));
    } else {
        panic!("expected vector");
    }
}

#[test]
fn multi_type_dispatch() {
    let src = r#"
      (defn kind [v]
        (cond
          (integer? v) :int
          (string? v)  :str
          :else        :other))
      (defmulti describe kind)
      (defmethod describe :int [x] (str "int: " x))
      (defmethod describe :str [x] (str "str: " x))
      [(describe 42) (describe "hi")]
    "#;
    let v = run(src);
    if let Value::Vector(xs) = v {
        assert_eq!(xs[0], Value::Str("int: 42".into()));
        assert_eq!(xs[1], Value::Str("str: hi".into()));
    } else {
        panic!();
    }
}

#[test]
fn multi_no_method_errors() {
    let env = Env::new();
    builtins::install(&env);
    let src = r#"
      (defmulti m identity)
      (defmethod m :a [_] 1)
      (m :b)
    "#;
    let forms = reader::read_all(src).expect("read");
    let mut last: Result<Value, _> = Ok(Value::Nil);
    for f in forms {
        last = eval::eval(&f, &env);
    }
    let err = last.unwrap_err().to_string();
    assert!(err.contains("no method"), "got: {err}");
}

#[test]
fn multi_methods_are_open() {
    // We can add a method after first use.
    let src = r#"
      (defmulti op :tag)
      (defmethod op :add [m] (+ (:a m) (:b m)))
      (def before (op {:tag :add :a 1 :b 2}))
      (defmethod op :mul [m] (* (:a m) (:b m)))
      (def after (op {:tag :mul :a 3 :b 4}))
      [before after]
    "#;
    assert_eq!(
        run(src),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(3),
            Value::Int(12),
        ]))
    );
}
