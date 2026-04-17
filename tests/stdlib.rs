//! Core stdlib builtins — eager map/filter/reduce/range/take/drop and
//! arithmetic predicates.

use std::sync::Arc;

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
fn range_variants() {
    assert_eq!(run("(count (range 10))"), Value::Int(10));
    assert_eq!(run("(first (range 10))"), Value::Int(0));
    assert_eq!(run("(first (rest (range 10)))"), Value::Int(1));
    assert_eq!(run("(count (range 5 15))"), Value::Int(10));
    assert_eq!(run("(first (range 5 15))"), Value::Int(5));
    assert_eq!(run("(count (range 0 10 2))"), Value::Int(5));
}

#[test]
fn map_squares() {
    let src = "(count (map (fn [x] (* x x)) (range 100)))";
    assert_eq!(run(src), Value::Int(100));
    // Verify actual values
    let src = "(first (map (fn [x] (* x x)) [1 2 3 4]))";
    assert_eq!(run(src), Value::Int(1));
    let src = "(first (rest (map (fn [x] (* x x)) [1 2 3 4])))";
    assert_eq!(run(src), Value::Int(4));
}

#[test]
fn filter_evens() {
    let src = "(count (filter even? (range 10)))";
    assert_eq!(run(src), Value::Int(5));
}

#[test]
fn reduce_sum() {
    let src = "(reduce + 0 (range 101))";
    assert_eq!(run(src), Value::Int(5050));
}

#[test]
fn reduce_no_init() {
    let src = "(reduce + [1 2 3 4 5])";
    assert_eq!(run(src), Value::Int(15));
}

#[test]
fn take_drop() {
    assert_eq!(run("(count (take 3 [1 2 3 4 5]))"), Value::Int(3));
    assert_eq!(run("(first (take 3 [10 20 30 40]))"), Value::Int(10));
    assert_eq!(run("(count (drop 2 [1 2 3 4 5]))"), Value::Int(3));
    assert_eq!(run("(first (drop 2 [10 20 30 40]))"), Value::Int(30));
}

#[test]
fn predicates() {
    assert_eq!(run("(even? 4)"), Value::Bool(true));
    assert_eq!(run("(even? 5)"), Value::Bool(false));
    assert_eq!(run("(odd? 5)"), Value::Bool(true));
    assert_eq!(run("(pos? 1)"), Value::Bool(true));
    assert_eq!(run("(pos? -1)"), Value::Bool(false));
    assert_eq!(run("(neg? -1)"), Value::Bool(true));
}

#[test]
fn pipeline_sum_of_squared_odds_under_10() {
    let src = "(reduce + 0 (map (fn [x] (* x x)) (filter odd? (range 10))))";
    // odds under 10: 1 3 5 7 9 -> squares: 1 9 25 49 81 -> sum: 165
    assert_eq!(run(src), Value::Int(165));
}

#[test]
fn identity_works() {
    assert_eq!(run("(identity 42)"), Value::Int(42));
    let src = "(count (map identity [1 2 3]))";
    assert_eq!(run(src), Value::Int(3));
    let _ = Arc::<str>::from("unused"); // keep unused import quiet
}
