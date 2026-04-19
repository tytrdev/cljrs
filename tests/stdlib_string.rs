//! Bug-hunting suite for clojure.string — both native (str/...) and
//! the cljrs.string fns ported in src/cljrs_string.clj.

use cljrs::{builtins, env::Env, eval, reader, value::Value};
use std::sync::Arc;

// cljrs_string.clj is not (yet) wired into the prelude — load it
// explicitly so the cljrs.string/* fns under test resolve.
const STRING_NS_SRC: &str = include_str!("../src/cljrs_string.clj");

fn fresh_env() -> Env {
    let env = Env::new();
    builtins::install(&env);
    for f in reader::read_all(STRING_NS_SRC).expect("read string ns") {
        eval::eval(&f, &env).expect("eval string ns");
    }
    env.set_current_ns("cljrs.core");
    env
}

fn run(src: &str) -> Value {
    let env = fresh_env();
    let forms = reader::read_all(src).expect("read");
    let mut result = Value::Nil;
    for f in forms {
        result = eval::eval(&f, &env).expect("eval");
    }
    result
}

fn run_err(src: &str) -> bool {
    let env = fresh_env();
    let forms = match reader::read_all(src) {
        Ok(f) => f,
        Err(_) => return true,
    };
    for f in forms {
        if eval::eval(&f, &env).is_err() {
            return true;
        }
    }
    false
}

fn s(x: &str) -> Value {
    Value::Str(Arc::from(x))
}

// --- trim family --------------------------------------------------------

#[test]
fn trim_handles_empty() {
    assert_eq!(run(r#"(str/trim "")"#), s(""));
}

#[test]
fn trim_only_whitespace() {
    assert_eq!(run(r#"(str/trim "   \n\t  ")"#), s(""));
}

#[test]
fn trim_no_change_when_clean() {
    assert_eq!(run(r#"(str/trim "abc")"#), s("abc"));
}

#[test]
fn trim_internal_whitespace_kept() {
    assert_eq!(run(r#"(str/trim "  a b c  ")"#), s("a b c"));
}

#[test]
fn triml_drops_leading_only() {
    assert_eq!(run(r#"(clojure.string/triml "  abc  ")"#), s("abc  "));
}

#[test]
fn trimr_drops_trailing_only() {
    assert_eq!(run(r#"(clojure.string/trimr "  abc  ")"#), s("  abc"));
}

#[test]
fn trim_newline_drops_only_newlines() {
    assert_eq!(run(r#"(clojure.string/trim-newline "abc\n")"#), s("abc"));
    assert_eq!(run(r#"(clojure.string/trim-newline "abc\r\n")"#), s("abc"));
    assert_eq!(run(r#"(clojure.string/trim-newline "abc")"#), s("abc"));
    // Should NOT strip spaces.
    assert_eq!(run(r#"(clojure.string/trim-newline "abc ")"#), s("abc "));
}

// --- case manipulation --------------------------------------------------

#[test]
fn upper_case_basic() {
    assert_eq!(run(r#"(str/upper-case "hello")"#), s("HELLO"));
    assert_eq!(run(r#"(str/upper-case "")"#), s(""));
    assert_eq!(run(r#"(str/upper-case "Hi 5!")"#), s("HI 5!"));
}

#[test]
fn lower_case_basic() {
    assert_eq!(run(r#"(str/lower-case "HELLO")"#), s("hello"));
    assert_eq!(run(r#"(str/lower-case "")"#), s(""));
}

#[test]
fn capitalize_basic() {
    assert_eq!(run(r#"(clojure.string/capitalize "hello")"#), s("Hello"));
    assert_eq!(run(r#"(clojure.string/capitalize "HELLO")"#), s("Hello"));
    assert_eq!(run(r#"(clojure.string/capitalize "h")"#), s("H"));
    assert_eq!(run(r#"(clojure.string/capitalize "")"#), s(""));
}

// --- substring queries --------------------------------------------------

#[test]
fn starts_with_q_basic() {
    assert_eq!(run(r#"(str/starts-with? "hello" "he")"#), Value::Bool(true));
    assert_eq!(
        run(r#"(str/starts-with? "hello" "world")"#),
        Value::Bool(false)
    );
    // Empty prefix → always true.
    assert_eq!(run(r#"(str/starts-with? "hello" "")"#), Value::Bool(true));
}

#[test]
fn ends_with_q_basic() {
    assert_eq!(run(r#"(str/ends-with? "hello" "lo")"#), Value::Bool(true));
    assert_eq!(run(r#"(str/ends-with? "hello" "world")"#), Value::Bool(false));
}

#[test]
fn includes_q_basic() {
    assert_eq!(run(r#"(str/includes? "hello" "ell")"#), Value::Bool(true));
    assert_eq!(run(r#"(str/includes? "hello" "xyz")"#), Value::Bool(false));
    assert_eq!(run(r#"(str/includes? "hello" "")"#), Value::Bool(true));
}

#[test]
fn index_of_basic() {
    assert_eq!(run(r#"(str/index-of "hello" "ll")"#), Value::Int(2));
    assert_eq!(run(r#"(str/index-of "hello" "xyz")"#), Value::Nil);
    assert_eq!(run(r#"(str/index-of "hello" "h")"#), Value::Int(0));
}

#[test]
fn last_index_of_basic() {
    assert_eq!(run(r#"(str/last-index-of "abcabc" "b")"#), Value::Int(4));
    assert_eq!(run(r#"(str/last-index-of "abc" "z")"#), Value::Nil);
}

// --- blank? -------------------------------------------------------------

#[test]
fn blank_q_nil_empty_whitespace() {
    assert_eq!(run("(str/blank? nil)"), Value::Bool(true));
    assert_eq!(run(r#"(str/blank? "")"#), Value::Bool(true));
    assert_eq!(run(r#"(str/blank? "   ")"#), Value::Bool(true));
    assert_eq!(run(r#"(str/blank? "\n\t")"#), Value::Bool(true));
    assert_eq!(run(r#"(str/blank? "x")"#), Value::Bool(false));
    assert_eq!(run(r#"(str/blank? " a ")"#), Value::Bool(false));
}

// --- split / split-lines ------------------------------------------------

#[test]
fn split_basic_string_sep() {
    assert_eq!(
        run(r#"(str/split "a,b,c" #",")"#),
        Value::Vector(imbl::Vector::from_iter([s("a"), s("b"), s("c")]))
    );
}

#[test]
fn split_with_limit() {
    // (str/split "a,b,c,d" #"," 2) → ["a" "b,c,d"].
    let v = run(r#"(str/split "a,b,c,d" #"," 2)"#);
    assert_eq!(v, Value::Vector(imbl::Vector::from_iter([s("a"), s("b,c,d")])));
}

#[test]
fn split_no_match_one_piece() {
    let v = run(r#"(str/split "abc" #",")"#);
    assert_eq!(v, Value::Vector(imbl::Vector::from_iter([s("abc")])));
}

#[test]
fn split_lines_basic() {
    assert_eq!(
        run(r#"(clojure.string/split-lines "a\nb\nc")"#),
        Value::Vector(imbl::Vector::from_iter([s("a"), s("b"), s("c")]))
    );
}

#[test]
fn split_lines_handles_crlf() {
    assert_eq!(
        run(r#"(clojure.string/split-lines "a\r\nb\r\nc")"#),
        Value::Vector(imbl::Vector::from_iter([s("a"), s("b"), s("c")]))
    );
}

#[test]
fn split_lines_drops_trailing_empty() {
    // "a\n" → ["a"], NOT ["a" ""].
    let v = run(r#"(clojure.string/split-lines "a\n")"#);
    assert_eq!(v, Value::Vector(imbl::Vector::from_iter([s("a")])));
}

#[test]
fn split_lines_empty_string() {
    let v = run(r#"(clojure.string/split-lines "")"#);
    // Clojure: (split-lines "") → [""]  (single empty entry).
    let len = match v {
        Value::Vector(xs) => xs.len(),
        _ => 999,
    };
    assert!(len <= 1, "expected 0 or 1 entries, got {len}");
}

// --- join ---------------------------------------------------------------

#[test]
fn join_no_sep() {
    assert_eq!(run("(str/join [1 2 3])"), s("123"));
}

#[test]
fn join_with_sep() {
    assert_eq!(run(r#"(str/join "-" [1 2 3])"#), s("1-2-3"));
}

#[test]
fn join_empty_coll() {
    assert_eq!(run(r#"(str/join "," [])"#), s(""));
}

#[test]
fn join_nil_in_coll_treated_as_empty() {
    // Clojure: (join "," [1 nil 2]) → "1,,2".
    assert_eq!(run(r#"(str/join "," [1 nil 2])"#), s("1,,2"));
}

// --- replace ------------------------------------------------------------

#[test]
fn replace_string_match() {
    assert_eq!(
        run(r#"(str/replace "abcabc" "b" "X")"#),
        s("aXcaXc")
    );
}

#[test]
fn replace_no_match_no_change() {
    assert_eq!(run(r#"(str/replace "abc" "z" "X")"#), s("abc"));
}

#[test]
fn replace_regex() {
    assert_eq!(
        run(r#"(str/replace "a1b2c3" #"\d" "X")"#),
        s("aXbXcX")
    );
}

#[test]
fn replace_first_string_match() {
    assert_eq!(
        run(r#"(clojure.string/replace-first "abcabc" "b" "X")"#),
        s("aXcabc")
    );
}

#[test]
fn replace_first_regex_match() {
    assert_eq!(
        run(r#"(clojure.string/replace-first "a1b2c3" #"\d" "X")"#),
        s("aXb2c3")
    );
}

#[test]
fn replace_first_no_match() {
    assert_eq!(
        run(r#"(clojure.string/replace-first "abc" "z" "X")"#),
        s("abc")
    );
}

// --- escape -------------------------------------------------------------

#[test]
fn escape_swaps_via_map() {
    let v = run(r#"(clojure.string/escape "a<b>c" {"<" "&lt;" ">" "&gt;"})"#);
    assert_eq!(v, s("a&lt;b&gt;c"));
}

#[test]
fn escape_unmapped_chars_passthrough() {
    let v = run(r#"(clojure.string/escape "abc" {"x" "y"})"#);
    assert_eq!(v, s("abc"));
}

#[test]
fn escape_empty_string() {
    assert_eq!(run(r#"(clojure.string/escape "" {})"#), s(""));
}

// --- re-quote-replacement ----------------------------------------------

#[test]
fn re_quote_replacement_escapes_dollar() {
    // "$1" should become "\\$1".
    let v = run(r#"(clojure.string/re-quote-replacement "$1")"#);
    assert_eq!(v, s(r"\$1"));
}

#[test]
fn re_quote_replacement_escapes_backslash() {
    let v = run(r#"(clojure.string/re-quote-replacement "a\\b")"#);
    assert_eq!(v, s(r"a\\b"));
}

// --- reverse ------------------------------------------------------------

#[test]
fn string_reverse_basic() {
    assert_eq!(run(r#"(clojure.string/reverse "abc")"#), s("cba"));
    assert_eq!(run(r#"(clojure.string/reverse "")"#), s(""));
    assert_eq!(run(r#"(clojure.string/reverse "a")"#), s("a"));
}

#[test]
fn string_reverse_unicode() {
    // Multi-byte UTF-8 reversal by codepoint.
    assert_eq!(run(r#"(clojure.string/reverse "héllo")"#), s("olléh"));
}

// --- subs / count / nth on strings -------------------------------------

#[test]
fn subs_to_end() {
    assert_eq!(run(r#"(subs "hello" 2)"#), s("llo"));
}

#[test]
fn subs_full_range() {
    assert_eq!(run(r#"(subs "hello" 0 5)"#), s("hello"));
}

#[test]
fn subs_empty_slice() {
    assert_eq!(run(r#"(subs "hello" 2 2)"#), s(""));
}

#[test]
fn subs_negative_index_errors() {
    assert!(run_err(r#"(subs "hello" -1)"#));
}

#[test]
fn subs_end_before_start_errors() {
    assert!(run_err(r#"(subs "hello" 3 1)"#));
}

#[test]
fn count_string_unicode_codepoints() {
    assert_eq!(run(r#"(count "héllo")"#), Value::Int(5));
    assert_eq!(run(r#"(count "")"#), Value::Int(0));
}
