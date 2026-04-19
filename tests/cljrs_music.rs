//! Integration tests for the cljrs.music namespace. The library is
//! shipped as part of the prelude (see install_prelude in builtins.rs)
//! so we only need a fresh Env to exercise it.

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

fn as_f64(v: &Value) -> f64 {
    match v {
        Value::Float(f) => *f,
        Value::Int(i) => *i as f64,
        other => panic!("expected number, got {other:?}"),
    }
}

fn int_vec(v: &Value) -> Vec<i64> {
    match v {
        Value::Vector(xs) => xs
            .iter()
            .map(|x| match x {
                Value::Int(i) => *i,
                other => panic!("expected int element, got {other:?}"),
            })
            .collect(),
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn midi_to_hz_a4_is_440() {
    let v = run("(cljrs.music/midi->hz 69)");
    assert!((as_f64(&v) - 440.0).abs() < 1e-6, "got {v:?}");
}

#[test]
fn note_keyword_c4_is_60() {
    let v = run("(cljrs.music/note :c4)");
    assert_eq!(v, Value::Int(60));
}

#[test]
fn note_keyword_sharp_and_flat() {
    // a#3 == bb3 == 58
    assert_eq!(run("(cljrs.music/note :a#3)"), Value::Int(58));
    assert_eq!(run("(cljrs.music/note :bb3)"), Value::Int(58));
    // eb5 == d#5 == 75
    assert_eq!(run("(cljrs.music/note :eb5)"), Value::Int(75));
}

#[test]
fn scale_c_major() {
    let v = run("(cljrs.music/scale 60 :major)");
    assert_eq!(int_vec(&v), vec![60, 62, 64, 65, 67, 69, 71, 72]);
}

#[test]
fn chord_c_maj7() {
    let v = run("(cljrs.music/chord 60 :maj7)");
    assert_eq!(int_vec(&v), vec![60, 64, 67, 71]);
}

#[test]
fn progression_c_i_iv_v7_i() {
    let v = run("(cljrs.music/progression 60 [[1 :maj] [4 :maj] [5 :7] [1 :maj]])");
    let chords = match &v {
        Value::Vector(xs) => xs.iter().map(int_vec).collect::<Vec<_>>(),
        other => panic!("expected vector, got {other:?}"),
    };
    assert_eq!(
        chords,
        vec![
            vec![60, 64, 67],          // C maj
            vec![65, 69, 72],          // F maj
            vec![67, 71, 74, 77],      // G7
            vec![60, 64, 67],          // C maj
        ]
    );
}

#[test]
fn transpose_shifts_sequence() {
    let v = run("(cljrs.music/transpose [60 62 64] 12)");
    assert_eq!(int_vec(&v), vec![72, 74, 76]);
}

#[test]
fn arpeggiate_up_and_down() {
    assert_eq!(
        int_vec(&run("(cljrs.music/arpeggiate [60 64 67] :up)")),
        vec![60, 64, 67]
    );
    assert_eq!(
        int_vec(&run("(cljrs.music/arpeggiate [60 64 67] :down)")),
        vec![67, 64, 60]
    );
    assert_eq!(
        int_vec(&run("(cljrs.music/arpeggiate [60 64 67] :updown)")),
        vec![60, 64, 67, 64]
    );
}
