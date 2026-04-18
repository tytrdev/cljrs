//! defonce: bind-if-unbound. Key to live-coding patterns — editing the
//! source and re-evaluating must not wipe out state declared with
//! defonce, while normal `def` still redefines.

use cljrs::{builtins, env::Env, eval, reader, value::Value};

fn env_with(src: &str) -> Env {
    let env = Env::new();
    builtins::install(&env);
    for f in reader::read_all(src).unwrap() {
        eval::eval(&f, &env).unwrap();
    }
    env
}

fn run_in(env: &Env, src: &str) -> Value {
    let mut last = Value::Nil;
    for f in reader::read_all(src).unwrap() {
        last = eval::eval(&f, env).unwrap();
    }
    last
}

#[test]
fn defonce_binds_once() {
    let env = env_with("(defonce x 1)");
    // re-eval with a different value; existing binding wins
    let v = run_in(&env, "(defonce x 999) x");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn def_still_redefines() {
    let env = env_with("(def x 1)");
    let v = run_in(&env, "(def x 2) x");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn defonce_atom_preserves_state_across_reeval() {
    // Simulates live-coding: atom survives a re-eval that redefines an fn.
    let env = env_with(
        r#"
        (defonce state (atom {:count 0}))
        (defn bump [] (swap! state update :count inc))
        (bump)
        (bump)
        "#,
    );
    assert_eq!(run_in(&env, "(:count @state)"), Value::Int(2));

    // "Edit" the source: redefine bump, redeclare state with defonce.
    // The atom must keep its :count of 2.
    run_in(
        &env,
        r#"
        (defonce state (atom {:count 0}))
        (defn bump [] (swap! state update :count (fn [n] (+ n 10))))
        "#,
    );
    assert_eq!(run_in(&env, "(:count @state)"), Value::Int(2));
    run_in(&env, "(bump)");
    assert_eq!(run_in(&env, "(:count @state)"), Value::Int(12));
}
