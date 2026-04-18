//! lazy-seq, iterate, repeatedly, cycle — deferred seq machinery.

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
fn lazy_seq_is_lazy() {
    // The body shouldn't run until first/rest forces it.
    let src = r#"
      (def flag (atom :unset))
      (def seq (lazy-seq (reset! flag :forced) [1 2 3]))
      @flag
    "#;
    assert_eq!(run(src), Value::Keyword("unset".into()));
}

#[test]
fn lazy_seq_forces_on_first() {
    let src = r#"
      (def flag (atom :unset))
      (def s (lazy-seq (reset! flag :forced) [1 2 3]))
      (first s)
      @flag
    "#;
    assert_eq!(run(src), Value::Keyword("forced".into()));
}

#[test]
fn iterate_take() {
    // Powers of 2.
    let src = "(take 6 (iterate (fn [x] (* 2 x)) 1))";
    assert_eq!(
        run(src),
        Value::List(std::sync::Arc::new(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(4),
            Value::Int(8),
            Value::Int(16),
            Value::Int(32),
        ]))
    );
}

#[test]
fn cycle_take() {
    let src = "(take 7 (cycle [1 2 3]))";
    assert_eq!(
        run(src),
        Value::List(std::sync::Arc::new(vec![
            Value::Int(1), Value::Int(2), Value::Int(3),
            Value::Int(1), Value::Int(2), Value::Int(3),
            Value::Int(1),
        ]))
    );
}

#[test]
fn repeatedly_take() {
    // repeatedly of a constant-returning fn.
    let src = r#"
      (count (take 5 (repeatedly (fn [] 42))))
    "#;
    assert_eq!(run(src), Value::Int(5));
}

#[test]
fn lazy_seq_is_cached() {
    // Forcing twice should only run the thunk once.
    let src = r#"
      (def calls (atom 0))
      (def s (lazy-seq (swap! calls inc) [1 2 3]))
      (first s)
      (first s)
      @calls
    "#;
    assert_eq!(run(src), Value::Int(1));
}
