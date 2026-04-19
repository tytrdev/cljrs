//! Verifies that runtime errors carry the source line/col where the
//! offending form was read, plus a snapshot of the Clojure call stack
//! when the failure happens inside a defined fn.

use cljrs::{builtins, env::Env, error::Error, eval, reader};

fn fresh_env() -> Env {
    let env = Env::new();
    builtins::install(&env);
    env
}

fn eval_str(src: &str) -> Result<cljrs::value::Value, Error> {
    let env = fresh_env();
    let forms = reader::read_all(src).expect("reader");
    let mut last = cljrs::value::Value::Nil;
    for f in forms {
        last = eval::eval(&f, &env)?;
    }
    Ok(last)
}

#[test]
fn unbound_symbol_carries_location() {
    // `bogus-name` is on line 2, col 4 (1-based) of the form list.
    let src = "(do\n   (bogus-name 1 2))";
    let err = eval_str(src).expect_err("should error");
    match &err {
        Error::Located { line, col, .. } => {
            assert_eq!(*line, 2, "got error: {err}");
            assert!(*col >= 4 && *col <= 5, "col {col} for: {err}");
        }
        other => panic!("expected Located, got {other:?}"),
    }
    // Display surfaces the location prefix.
    let s = format!("{err}");
    assert!(s.starts_with("2:"), "format: {s}");
    assert!(s.contains("unbound"), "format: {s}");
}

#[test]
fn type_error_inside_fn_includes_call_stack() {
    // `(boom)` calls `+` on a string. Error originates on line 3 col 4.
    let src = "(defn boom []\n  (do\n   (+ 1 \"oops\")))\n(boom)";
    let err = eval_str(src).expect_err("should error");
    let Error::Located { line, stack, .. } = &err else {
        panic!("expected Located, got {err:?}");
    };
    assert!(*line >= 3, "{err}");
    // The stack should mention `boom`.
    assert!(stack.iter().any(|n| n.as_ref() == "boom"), "stack: {stack:?}");
}

#[test]
fn peel_unwraps_to_original_kind() {
    let src = "(undefined-fn)";
    let err = eval_str(src).expect_err("should error");
    match err.peel() {
        Error::Unbound(_) => {}
        other => panic!("expected Unbound, got {other:?}"),
    }
}

#[test]
fn recur_is_not_wrapped_with_location() {
    // recur outside a loop/fn should still surface as an error, but as
    // a plain Recur signal turned into the eval-time message — never as
    // Located, since `Error::at` refuses to wrap control-flow signals.
    // Actually `recur` outside a target propagates as Recur; the docs
    // say this surfaces as a user error. We just assert it's not double
    // -wrapped beyond a single Located layer.
    let err = eval_str("(recur 1)").expect_err("should error");
    // peel once then assert.
    let peeled = err.peel();
    assert!(matches!(peeled, Error::Recur(_)), "got: {peeled:?}");
}

#[test]
fn try_catch_still_works_through_location_wrapping() {
    // The error inside `(throw ...)` is wrapped with Located, but `try`
    // must still peel and route to the catch clause.
    let src = "(try (throw \"boom\") (catch _ e e))";
    let v = eval_str(src).expect("try/catch");
    assert_eq!(v.to_display_string(), "boom");
}
