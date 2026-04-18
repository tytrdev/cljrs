//! Auto-gensym: `x#` in a syntax-quoted form becomes a fresh unique
//! symbol, shared across all occurrences within one `` `form `` block.

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
fn gensym_same_name_shares_symbol() {
    // A macro that captures `x#` twice in the same `form should see the
    // same symbol in the expansion, enabling local bindings.
    let src = r#"
      (defmacro twice [expr]
        `(let [v# ~expr] (+ v# v#)))
      (twice 21)
    "#;
    assert_eq!(run(src), Value::Int(42));
}

#[test]
fn gensym_collides_with_outer_binding() {
    // If gensym were broken (literal `v#` name), an outer `v` would
    // shadow it. Hygienic gensym means no collision.
    let src = r#"
      (defmacro double [expr]
        `(let [v# ~expr] (* v# 2)))
      (let [v 100]
        (double v))
    "#;
    assert_eq!(run(src), Value::Int(200));
}

#[test]
fn gensym_fresh_per_invocation() {
    // Two separate expansions get different fresh symbols, so nesting
    // a macro call inside its own expansion works.
    let src = r#"
      (defmacro m [x]
        `(let [t# ~x] (+ t# 1)))
      (m (m 5))
    "#;
    assert_eq!(run(src), Value::Int(7));
}

#[test]
fn manual_suffix_still_works() {
    // Names NOT ending in `#` aren't touched.
    let src = r#"
      (defmacro keep-name [v]
        `(let [x ~v] x))
      (keep-name 42)
    "#;
    assert_eq!(run(src), Value::Int(42));
}
