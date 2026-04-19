//! Ratios: exact rational arithmetic via Value::Ratio.

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
fn ratio_literal_reads() {
    assert_eq!(run("1/3"), Value::Ratio(1, 3));
    assert_eq!(run("4/8"), Value::Ratio(1, 2));      // reduces
    assert_eq!(run("4/2"), Value::Int(2));            // simplifies to int
    assert_eq!(run("-3/6"), Value::Ratio(-1, 2));     // negative numerator
    assert_eq!(run("3/-6"), Value::Ratio(-1, 2));     // sign moves to top
}

#[test]
fn division_produces_exact_ratio() {
    assert_eq!(run("(/ 1 3)"), Value::Ratio(1, 3));
    assert_eq!(run("(/ 6 3)"), Value::Int(2));        // exact integer result
    assert_eq!(run("(/ 1 2 3)"), Value::Ratio(1, 6));  // chained
}

#[test]
fn ratio_arithmetic() {
    assert_eq!(run("(+ 1/2 1/3)"), Value::Ratio(5, 6));
    assert_eq!(run("(- 1 1/4)"), Value::Ratio(3, 4));
    assert_eq!(run("(* 2/3 3/2)"), Value::Int(1));
    assert_eq!(run("(/ 2/3 1/2)"), Value::Ratio(4, 3));
}

#[test]
fn ratio_demotes_to_float_with_float() {
    let v = run("(+ 1/2 0.25)");
    assert_eq!(v, Value::Float(0.75));
}

#[test]
fn ratio_comparisons() {
    assert_eq!(run("(< 1/3 1/2)"), Value::Bool(true));
    // Clojure-strict: ratios and floats are distinct types under =.
    // Use == for numeric coercion across types.
    assert_eq!(run("(= 1/2 0.5)"), Value::Bool(false));
    assert_eq!(run("(== 1/2 0.5)"), Value::Bool(true));
    // 4/2 simplifies to the integer 2, so this stays equal under =.
    assert_eq!(run("(= 4/2 2)"), Value::Bool(true));
}

#[test]
fn inc_dec_on_ratio() {
    assert_eq!(run("(inc 1/2)"), Value::Ratio(3, 2));
    assert_eq!(run("(dec 1/3)"), Value::Ratio(-2, 3));
}
