//! Tests for control-flow additions in src/core.clj:
//! case / when-first / while / time / halt-when.
//!
//! Aim: cover happy path, edge cases, nil/empty input, multi-arity,
//! and composition with reduce/transduce where relevant.

use cljrs::{builtins, env::Env, eval, reader, value::Value};

fn run_in(env: &Env, src: &str) -> Value {
    let forms = reader::read_all(src).expect("read");
    let mut result = Value::Nil;
    for f in forms {
        result = eval::eval(&f, env).expect("eval");
    }
    result
}

fn run(src: &str) -> Value {
    let env = Env::new();
    builtins::install(&env);
    run_in(&env, src)
}

fn run_err(src: &str) -> String {
    let env = Env::new();
    builtins::install(&env);
    let forms = reader::read_all(src).expect("read");
    let mut last = Ok(Value::Nil);
    for f in forms {
        last = eval::eval(&f, &env);
    }
    match last {
        Ok(_) => panic!("expected error, got Ok"),
        Err(e) => format!("{}", e),
    }
}

// ---------- case ----------------------------------------------------------

#[test]
fn case_picks_matching_keyword_branch() {
    assert_eq!(run("(case :b :a 1 :b 2 :c 3 99)"), Value::Int(2));
}

#[test]
fn case_picks_default_when_no_match() {
    assert_eq!(run("(case :z :a 1 :b 2 :default)"),
               Value::Keyword("default".into()));
}

#[test]
fn case_no_default_throws() {
    let err = run_err("(case :z :a 1 :b 2)");
    assert!(err.contains("No matching clause"), "got: {}", err);
}

#[test]
fn case_supports_list_of_keys() {
    let src = r#"
      (defn classify [n]
        (case n
          (1 2 3) :small
          (4 5 6) :mid
          :other))
      [(classify 1) (classify 5) (classify 99)]
    "#;
    assert_eq!(run(src), Value::Vector(imbl::Vector::from_iter([
        Value::Keyword("small".into()),
        Value::Keyword("mid".into()),
        Value::Keyword("other".into()),
    ])));
}

#[test]
fn case_with_int_keys() {
    assert_eq!(run("(case 2 1 :one 2 :two 3 :three :other)"),
               Value::Keyword("two".into()));
}

#[test]
fn case_with_string_keys() {
    assert_eq!(run(r#"(case "b" "a" 1 "b" 2 0)"#), Value::Int(2));
}

#[test]
fn case_evaluates_dispatch_expression_once() {
    // If dispatch expr were re-evaluated per branch, the counter would
    // grow above 1.
    let src = r#"
      (def __c (atom 0))
      (defn bump [] (swap! __c inc) :b)
      (let [r (case (bump) :a 1 :b 2 :c 3 99)]
        [r @__c])
    "#;
    assert_eq!(run(src), Value::Vector(imbl::Vector::from_iter([
        Value::Int(2),
        Value::Int(1),
    ])));
}

#[test]
fn case_keys_are_not_evaluated() {
    // `foo` is an unbound symbol; if case evaluated keys it would error.
    assert_eq!(run("(case 'foo foo :hit :miss)"),
               Value::Keyword("hit".into()));
}

#[test]
fn case_default_with_falsy_value() {
    // The default may legitimately be nil/false.
    assert_eq!(run("(case :z :a 1 nil)"), Value::Nil);
    assert_eq!(run("(case :z :a 1 false)"), Value::Bool(false));
}

// ---------- when-first ----------------------------------------------------

#[test]
fn when_first_binds_first_when_nonempty() {
    assert_eq!(run("(when-first [x [10 20 30]] (* x 2))"), Value::Int(20));
}

#[test]
fn when_first_returns_nil_on_empty() {
    assert_eq!(run("(when-first [x []] :nope)"), Value::Nil);
    assert_eq!(run("(when-first [x nil] :nope)"), Value::Nil);
    assert_eq!(run("(when-first [x '()] :nope)"), Value::Nil);
}

#[test]
fn when_first_works_with_lazy_seq() {
    let src = "(when-first [x (map inc [1 2 3])] x)";
    assert_eq!(run(src), Value::Int(2));
}

#[test]
fn when_first_runs_multiple_body_forms() {
    let src = r#"
      (def __c (atom 0))
      (when-first [x [:a :b :c]]
        (swap! __c inc)
        (swap! __c inc)
        x)
    "#;
    assert_eq!(run(src), Value::Keyword("a".into()));
}

#[test]
fn when_first_skips_body_on_empty() {
    let src = r#"
      (def __c (atom 0))
      (when-first [x []]
        (swap! __c inc))
      @__c
    "#;
    assert_eq!(run(src), Value::Int(0));
}

// ---------- while ---------------------------------------------------------

#[test]
fn while_loops_until_false() {
    let src = r#"
      (def __c (atom 0))
      (while (< @__c 5)
        (swap! __c inc))
      @__c
    "#;
    assert_eq!(run(src), Value::Int(5));
}

#[test]
fn while_skips_body_when_test_initially_false() {
    let src = r#"
      (def __c (atom 0))
      (while false
        (swap! __c inc))
      @__c
    "#;
    assert_eq!(run(src), Value::Int(0));
}

#[test]
fn while_returns_nil() {
    assert_eq!(run("(while false :body)"), Value::Nil);
}

#[test]
fn while_with_multiple_body_forms() {
    let src = r#"
      (def __a (atom 0))
      (def __b (atom 0))
      (while (< @__a 3)
        (swap! __a inc)
        (swap! __b (fn [x] (+ x 10))))
      [@__a @__b]
    "#;
    assert_eq!(run(src), Value::Vector(imbl::Vector::from_iter([
        Value::Int(3), Value::Int(30),
    ])));
}

// ---------- time ----------------------------------------------------------

#[test]
fn time_returns_value_of_expr() {
    assert_eq!(run("(time (+ 1 2 3))"), Value::Int(6));
}

#[test]
fn time_evaluates_expr_once() {
    let src = r#"
      (def __c (atom 0))
      (time (swap! __c inc))
      @__c
    "#;
    assert_eq!(run(src), Value::Int(1));
}

#[test]
fn time_works_with_complex_expression() {
    assert_eq!(run("(time (reduce + (range 100)))"), Value::Int(4950));
}

// ---------- halt-when -----------------------------------------------------

#[test]
fn halt_when_halts_at_first_match() {
    // Returns the offending input itself.
    assert_eq!(run("(transduce (halt-when neg?) conj [] [1 2 -3 4])"),
               Value::Int(-3));
}

#[test]
fn halt_when_passes_through_when_no_match() {
    assert_eq!(run("(transduce (halt-when neg?) conj [] [1 2 3 4])"),
               Value::Vector(imbl::Vector::from_iter([
                   Value::Int(1), Value::Int(2), Value::Int(3), Value::Int(4),
               ])));
}

#[test]
fn halt_when_with_ret_fn() {
    // ret-fn receives (result, offending-input) and decides what to keep.
    let src = "(transduce (halt-when neg? (fn [r x] {:bad x :so-far r})) conj [] [1 2 -7 4])";
    let v = run(src);
    if let Value::Map(m) = &v {
        assert_eq!(m.get(&Value::Keyword("bad".into())), Some(&Value::Int(-7)));
    } else {
        panic!("expected map, got {:?}", v);
    }
}

#[test]
fn halt_when_composes_with_other_xforms() {
    // map then halt-when: should halt mid-stream.
    assert_eq!(run("(transduce (comp (map inc) (halt-when (fn [x] (= x 5)))) conj [] [1 2 4 7 9])"),
               Value::Int(5));
}

#[test]
fn halt_when_on_empty_input() {
    assert_eq!(run("(transduce (halt-when neg?) conj [] [])"),
               Value::Vector(imbl::Vector::new()));
}

#[test]
fn halt_when_first_element_matches() {
    assert_eq!(run("(transduce (halt-when (fn [_] true)) conj [] [:first :second])"),
               Value::Keyword("first".into()));
}
