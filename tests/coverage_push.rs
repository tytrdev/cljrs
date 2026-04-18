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

#[test]
fn for_multi_binding() {
    // Cartesian product, filtered.
    assert_eq!(
        run("(count (for [x (range 4) y (range 4) :when (< x y)] [x y]))"),
        Value::Int(6)
    );
}

#[test]
fn for_let_clause() {
    assert_eq!(
        run("(for [x (range 5) :let [sq (* x x)] :when (> sq 4)] sq)"),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(9),
            Value::Int(16),
        ]))
    );
}

#[test]
fn mapcat_flattens_one_level() {
    assert_eq!(
        run("(mapcat (fn [x] [x x]) [1 2 3])"),
        Value::List(std::sync::Arc::new(vec![
            Value::Int(1), Value::Int(1),
            Value::Int(2), Value::Int(2),
            Value::Int(3), Value::Int(3),
        ]))
    );
}

#[test]
fn flatten_recursive() {
    assert_eq!(
        run("(count (flatten [[1 2] [3 [4 5]] 6]))"),
        Value::Int(6)
    );
}

#[test]
fn seq_returns_nil_on_empty() {
    assert_eq!(run("(seq [])"), Value::Nil);
    assert_eq!(run("(seq nil)"), Value::Nil);
    assert_eq!(run("(if (seq [1]) :yes :no)"), Value::Keyword("yes".into()));
    assert_eq!(run("(if (seq []) :yes :no)"), Value::Keyword("no".into()));
}

#[test]
fn zipmap_basic() {
    let v = run(r#"(zipmap [:a :b :c] [1 2 3])"#);
    if let Value::Map(m) = v {
        assert_eq!(m.get(&Value::Keyword("b".into())), Some(&Value::Int(2)));
    } else {
        panic!();
    }
}

#[test]
fn max_min_key() {
    assert_eq!(
        run(r#"(max-key count "ab" "z" "qrs" "wxyz")"#),
        Value::Str("wxyz".into())
    );
    assert_eq!(
        run(r#"(min-key count "ab" "z" "qrs" "wxyz")"#),
        Value::Str("z".into())
    );
}

#[test]
fn update_vals_keys() {
    let v = run("(get (update-vals {:a 1 :b 2} inc) :a)");
    assert_eq!(v, Value::Int(2));
    let v = run(r#"(contains? (update-keys {:a 1} name) "a")"#);
    assert_eq!(v, Value::Bool(true));
}
