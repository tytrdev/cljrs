//! Integration tests for src/cljrs_string.clj — pure-cljrs
//! implementations of clojure.string vars not provided natively.
//!
//! The file is currently not auto-loaded by install_prelude (the
//! include_str! line lives in src/builtins.rs which this agent does
//! not own). Tests load it explicitly via load-file so the
//! implementations can be validated end-to-end. Once wired into the
//! prelude, callers reach these as `clojure.string/foo` directly.

use cljrs::{builtins, env::Env, eval, reader, value::Value};

const STRING_NS_SRC: &str = include_str!("../src/cljrs_string.clj");

fn run(src: &str) -> Value {
    let env = Env::new();
    builtins::install(&env);
    // Manually install the cljrs.string ns. We evaluate the file
    // contents directly instead of going through load-file so that
    // the test doesn't depend on CWD.
    for f in reader::read_all(STRING_NS_SRC).expect("read string ns") {
        eval::eval(&f, &env).expect("eval string ns");
    }
    // Restore current ns to cljrs.core after the (ns clojure.string)
    // form switched it.
    env.set_current_ns("cljrs.core");

    let mut result = Value::Nil;
    for f in reader::read_all(src).expect("read") {
        result = eval::eval(&f, &env).expect("eval");
    }
    result
}

fn s(x: &str) -> Value {
    Value::Str(std::sync::Arc::from(x))
}

#[test]
fn capitalize_basic() {
    assert_eq!(run(r#"(clojure.string/capitalize "hello")"#), s("Hello"));
    assert_eq!(run(r#"(clojure.string/capitalize "HELLO")"#), s("Hello"));
    assert_eq!(run(r#"(clojure.string/capitalize "h")"#), s("H"));
    assert_eq!(run(r#"(clojure.string/capitalize "")"#), s(""));
}

#[test]
fn triml_trimr_remove_only_one_side() {
    assert_eq!(run(r#"(clojure.string/triml "   abc   ")"#), s("abc   "));
    assert_eq!(run(r#"(clojure.string/trimr "   abc   ")"#), s("   abc"));
    assert_eq!(run(r#"(clojure.string/triml "")"#), s(""));
    assert_eq!(run(r#"(clojure.string/trimr "")"#), s(""));
    assert_eq!(run(r#"(clojure.string/triml "\t\nx")"#), s("x"));
    assert_eq!(run(r#"(clojure.string/trimr "x\r\n")"#), s("x"));
}

#[test]
fn trim_newline_drops_trailing_newlines_only() {
    assert_eq!(run(r#"(clojure.string/trim-newline "abc\n")"#), s("abc"));
    assert_eq!(run(r#"(clojure.string/trim-newline "abc\r\n")"#), s("abc"));
    assert_eq!(run(r#"(clojure.string/trim-newline "abc")"#), s("abc"));
    // Preserves internal newlines and leading whitespace.
    assert_eq!(run(r#"(clojure.string/trim-newline "  a\nb\n")"#), s("  a\nb"));
}

#[test]
fn split_lines_basic() {
    assert_eq!(
        run(r#"(clojure.string/split-lines "a\nb\nc")"#),
        Value::Vector(imbl::Vector::from_iter([s("a"), s("b"), s("c")]))
    );
    assert_eq!(
        run(r#"(clojure.string/split-lines "a\r\nb")"#),
        Value::Vector(imbl::Vector::from_iter([s("a"), s("b")]))
    );
    // Trailing newline produces no extra empty entry.
    assert_eq!(
        run(r#"(clojure.string/split-lines "a\nb\n")"#),
        Value::Vector(imbl::Vector::from_iter([s("a"), s("b")]))
    );
}

#[test]
fn reverse_basic() {
    assert_eq!(run(r#"(clojure.string/reverse "abc")"#), s("cba"));
    assert_eq!(run(r#"(clojure.string/reverse "")"#), s(""));
    assert_eq!(run(r#"(clojure.string/reverse "a")"#), s("a"));
    // Non-ASCII codepoints round-trip.
    assert_eq!(run(r#"(clojure.string/reverse "héllo")"#), s("olléh"));
}

#[test]
fn escape_remaps_chars() {
    // Map "<", ">", "&" to HTML entities; pass others through.
    let v = run(r#"
      (clojure.string/escape "a<b>c&d"
        {"<" "&lt;" ">" "&gt;" "&" "&amp;"})
    "#);
    assert_eq!(v, s("a&lt;b&gt;c&amp;d"));

    // Unmapped string returns unchanged.
    assert_eq!(
        run(r#"(clojure.string/escape "abc" {"x" "Y"})"#),
        s("abc")
    );
}

#[test]
fn re_quote_replacement_escapes_dollars_and_backslashes() {
    // $1 becomes \$1 (backslash + dollar) so it's a literal in
    // regex-aware replace contexts.
    assert_eq!(
        run(r#"(clojure.string/re-quote-replacement "$1")"#),
        s("\\$1")
    );
    // Backslash doubles.
    assert_eq!(
        run(r#"(clojure.string/re-quote-replacement "a\\b")"#),
        s("a\\\\b")
    );
    // Plain text passes through.
    assert_eq!(
        run(r#"(clojure.string/re-quote-replacement "hello")"#),
        s("hello")
    );
}

#[test]
fn replace_first_string_match() {
    assert_eq!(
        run(r#"(clojure.string/replace-first "aaa" "a" "X")"#),
        s("Xaa")
    );
    // No match: returns unchanged.
    assert_eq!(
        run(r#"(clojure.string/replace-first "abc" "z" "X")"#),
        s("abc")
    );
}

#[test]
fn replace_first_regex_match() {
    assert_eq!(
        run(r#"(clojure.string/replace-first "a1b2c3" #"\d" "X")"#),
        s("aXb2c3")
    );
    // Anchored multi-char regex.
    assert_eq!(
        run(r#"(clojure.string/replace-first "foo bar foo" #"foo" "BAZ")"#),
        s("BAZ bar foo")
    );
}
