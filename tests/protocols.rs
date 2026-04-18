//! defrecord + defprotocol. Records are tagged maps; protocols dispatch
//! on the :__type key (or on Clojure-type keyword for non-records).

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
fn defrecord_constructor_and_predicate() {
    let src = r#"
      (defrecord Point [x y])
      (def p (->Point 3 4))
      [(:x p) (:y p) (Point? p) (Point? 42)]
    "#;
    assert_eq!(
        run(src),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(3),
            Value::Int(4),
            Value::Bool(true),
            Value::Bool(false),
        ]))
    );
}

#[test]
fn defrecord_map_constructor() {
    let src = r#"
      (defrecord User [name age])
      (def u (map->User {:name "ty" :age 42}))
      [(:name u) (User? u)]
    "#;
    if let Value::Vector(xs) = run(src) {
        assert_eq!(xs[0], Value::Str("ty".into()));
        assert_eq!(xs[1], Value::Bool(true));
    } else {
        panic!();
    }
}

#[test]
fn defprotocol_dispatch_on_record_type() {
    let src = r#"
      (defprotocol Shape
        (area [s])
        (name-of [s]))

      (defrecord Circle [r]
        Shape
        (area [s] (* 3.14159 (* (:r s) (:r s))))
        (name-of [s] "circle"))

      (defrecord Rect [w h]
        Shape
        (area [s] (* (:w s) (:h s)))
        (name-of [s] "rect"))

      (def c (->Circle 2.0))
      (def r (->Rect 3 5))
      [(name-of c) (area r) (name-of r)]
    "#;
    if let Value::Vector(xs) = run(src) {
        assert_eq!(xs[0], Value::Str("circle".into()));
        assert_eq!(xs[1], Value::Int(15));
        assert_eq!(xs[2], Value::Str("rect".into()));
    } else {
        panic!();
    }
}

#[test]
fn protocol_on_plain_types() {
    // Protocols also dispatch on built-in type keywords.
    let src = r#"
      (defprotocol Show (show [x]))
      (defmethod show :int [x] (str "number: " x))
      (defmethod show :string [x] (str "text: " x))
      [(show 42) (show "hi")]
    "#;
    if let Value::Vector(xs) = run(src) {
        assert_eq!(xs[0], Value::Str("number: 42".into()));
        assert_eq!(xs[1], Value::Str("text: hi".into()));
    } else {
        panic!();
    }
}
