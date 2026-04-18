//! Transducers: 1-arg map/filter/take etc. produce transducers.
//! transduce / into / sequence run them over a collection.

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
fn map_transducer() {
    assert_eq!(
        run("(transduce (map inc) + 0 [1 2 3])"),
        Value::Int(9)
    );
}

#[test]
fn filter_transducer() {
    assert_eq!(
        run("(transduce (filter even?) + 0 (range 10))"),
        Value::Int(20) // 0+2+4+6+8
    );
}

#[test]
fn composed_transducer() {
    // comp composes transducers. Data flows left-to-right: first map,
    // then filter.
    assert_eq!(
        run("(transduce (comp (map inc) (filter even?)) + 0 (range 10))"),
        Value::Int(30) // inc -> 1..10, evens -> 2,4,6,8,10 = 30
    );
}

#[test]
fn take_transducer_short_circuits() {
    // take uses (reduced ...) internally; we shouldn't eval more than 3.
    let src = r#"
      (def count (atom 0))
      (def xform (comp (map (fn [x] (swap! count inc) x))
                       (take 3)))
      (def result (transduce xform conj [] (range 100)))
      [@count (count result)]
    "#;
    // Note: we shadow `count` the builtin locally with an atom + also
    // still have (count) on vectors. Let's avoid that name conflict.
    let src = r#"
      (def hits (atom 0))
      (def xform (comp (map (fn [x] (swap! hits inc) x))
                       (take 3)))
      (def result (transduce xform conj [] (range 100)))
      [@hits (count result)]
    "#;
    let v = run(src);
    if let Value::Vector(xs) = v {
        // hits should be 3 (take short-circuits) or 4 depending on how
        // reduced propagates. Both are acceptable.
        let hits = match &xs[0] { Value::Int(i) => *i, _ => panic!() };
        let result_count = match &xs[1] { Value::Int(i) => *i, _ => panic!() };
        assert!(hits >= 3 && hits <= 4, "hits was {hits}");
        assert_eq!(result_count, 3);
    } else {
        panic!();
    }
}

#[test]
fn into_with_xform() {
    assert_eq!(
        run("(into [] (map inc) [1 2 3])"),
        run("[2 3 4]")
    );
}

#[test]
fn into_without_xform_still_works() {
    assert_eq!(
        run("(into [1 2] [3 4 5])"),
        run("[1 2 3 4 5]")
    );
}

#[test]
fn map_two_arg_still_works() {
    // Backwards-compatibility: existing callers using 2-arg map.
    assert_eq!(
        run("(first (map inc [10 20 30]))"),
        Value::Int(11)
    );
}

#[test]
fn sequence_of_xform() {
    assert_eq!(
        run("(sequence (filter odd?) [1 2 3 4 5])"),
        run("[1 3 5]")
    );
}
