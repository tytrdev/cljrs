//! Real namespaces: ns / in-ns switch, qualified lookups, :as aliases,
//! :refer, and cross-namespace shadowing.

use std::io::Write;

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
fn ns_form_is_accepted() {
    let src = r#"
        (ns my.math)
        (defn square [n] (* n n))
        (square 7)
    "#;
    assert_eq!(run(src), Value::Int(49));
}

#[test]
fn ns_switches_and_qualifies_def() {
    let src = r#"
      (ns my.ns)
      (def x 42)
      my.ns/x
    "#;
    assert_eq!(run(src), Value::Int(42));
}

#[test]
fn two_namespaces_isolated() {
    let src = r#"
      (ns a)
      (def v 1)
      (ns b)
      (def v 2)
      [a/v b/v]
    "#;
    assert_eq!(
        run(src),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(1),
            Value::Int(2),
        ]))
    );
}

#[test]
fn unqualified_falls_back_to_core() {
    let src = r#"
      (ns my.app)
      (+ 1 2 3)
    "#;
    assert_eq!(run(src), Value::Int(6));
}

#[test]
fn alias_via_require() {
    let src = r#"
      (ns a)
      (def x 10)
      (ns b (:require [a :as al]))
      al/x
    "#;
    assert_eq!(run(src), Value::Int(10));
}

#[test]
fn refer_installs_into_current_ns() {
    let src = r#"
      (ns a)
      (defn greet [x] (str "hi, " x))
      (ns b (:require [a :refer [greet]]))
      (greet "ty")
    "#;
    assert_eq!(run(src), Value::Str("hi, ty".into()));
}

#[test]
fn current_namespace_shadows_core() {
    let src = r#"
      (ns shadow)
      (defn mymap [& _] :shadowed)
      (mymap 1 2 3)
    "#;
    assert_eq!(run(src), Value::Keyword("shadowed".into()));
}

#[test]
fn core_still_reachable_when_fully_qualified() {
    let src = r#"
      (ns qtest)
      (cljrs.core/+ 1 2 3)
    "#;
    assert_eq!(run(src), Value::Int(6));
}

#[test]
fn load_file_runs_source_in_current_env() {
    let dir = std::env::temp_dir();
    let path = dir.join("cljrs_ns_test_rev.clj");
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(f, "(defn dbl [n] (* 2 n))").unwrap();
    writeln!(f, "(def PI 3)").unwrap();
    drop(f);

    let env = Env::new();
    builtins::install(&env);
    let forms = reader::read_all(&format!(
        "(load-file \"{}\") (dbl PI)",
        path.display()
    ))
    .unwrap();
    let mut result = Value::Nil;
    for fr in forms {
        result = eval::eval(&fr, &env).unwrap();
    }
    assert_eq!(result, Value::Int(6));

    std::fs::remove_file(&path).ok();
}
