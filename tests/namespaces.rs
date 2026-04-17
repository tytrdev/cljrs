//! Arc-1 namespaces: cosmetic. (ns ...) is accepted, (load-file) / (require)
//! work for multi-file programs. Real ns isolation comes with Arc 2.

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
    // A file starting with (ns ...) should run without error.
    let src = r#"
        (ns my.math)
        (defn square [n] (* n n))
        (square 7)
    "#;
    assert_eq!(run(src), Value::Int(49));
}

#[test]
fn load_file_runs_source_in_current_env() {
    // Write a temp file and load it.
    let dir = std::env::temp_dir();
    let path = dir.join("cljrs_ns_test.clj");
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(f, "(defn double [n] (* 2 n))").unwrap();
    writeln!(f, "(def PI 3)").unwrap();
    drop(f);

    let env = Env::new();
    builtins::install(&env);
    let forms = reader::read_all(&format!(
        "(load-file \"{}\") (double PI)",
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
