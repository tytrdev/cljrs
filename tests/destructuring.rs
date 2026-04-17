//! let/fn destructuring: vector, map, nested, rest, :as, :or.

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
fn vector_positional() {
    assert_eq!(run("(let [[a b c] [1 2 3]] (+ a b c))"), Value::Int(6));
}

#[test]
fn vector_with_rest() {
    assert_eq!(
        run("(let [[a & rest] [1 2 3 4]] (count rest))"),
        Value::Int(3)
    );
}

#[test]
fn vector_as_binding() {
    assert_eq!(
        run("(let [[a b :as full] [10 20]] (+ a b (count full)))"),
        Value::Int(32)
    );
}

#[test]
fn nested_vector() {
    assert_eq!(
        run("(let [[[a b] c] [[1 2] 3]] (+ a b c))"),
        Value::Int(6)
    );
}

#[test]
fn map_keys() {
    assert_eq!(
        run("(let [{:keys [x y]} {:x 3 :y 4}] (+ x y))"),
        Value::Int(7)
    );
}

#[test]
fn map_keys_with_or() {
    assert_eq!(
        run("(let [{:keys [x y] :or {y 99}} {:x 3}] (+ x y))"),
        Value::Int(102)
    );
}

#[test]
fn map_as_binding() {
    assert_eq!(
        run("(let [{:keys [a] :as m} {:a 1 :b 2}] (+ a (count (keys m))))"),
        Value::Int(3)
    );
}

#[test]
fn fn_param_destructure() {
    let src = r#"
      (defn add-point [[x y]] (+ x y))
      (add-point [10 20])
    "#;
    assert_eq!(run(src), Value::Int(30));
}

#[test]
fn fn_map_param_destructure() {
    let src = r#"
      (defn area [{:keys [w h]}] (* w h))
      (area {:w 3 :h 5})
    "#;
    assert_eq!(run(src), Value::Int(15));
}

#[test]
fn fn_nested_param() {
    let src = r#"
      (defn f [[a b] {:keys [k]}] (+ a b k))
      (f [1 2] {:k 10})
    "#;
    assert_eq!(run(src), Value::Int(13));
}
