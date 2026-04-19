//! Tests for the cljrs-authored additions to src/core.clj:
//! if-not / if-some / when-some / condp / assert / comment /
//! letfn / delay / force / ffirst / fnext / nfirst / nnext /
//! not-any? / not-every? / replace / every-pred / some-fn /
//! fnil / == / pr / prn / prn-str.

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
fn if_not_macro() {
    assert_eq!(run("(if-not false :yes :no)"), Value::Keyword("yes".into()));
    assert_eq!(run("(if-not true  :yes :no)"), Value::Keyword("no".into()));
    assert_eq!(run("(if-not true  :yes)"),     Value::Nil);
}

#[test]
fn if_some_when_some() {
    // if-some binds when value is non-nil (false IS bound).
    assert_eq!(run("(if-some [x false] (if x :t :f) :nil)"),
               Value::Keyword("f".into()));
    assert_eq!(run("(if-some [x nil] x :nope)"),
               Value::Keyword("nope".into()));
    assert_eq!(run("(when-some [x 7] (* x 2))"), Value::Int(14));
    assert_eq!(run("(when-some [x nil] :never)"), Value::Nil);
}

#[test]
fn condp_basic_and_arrow() {
    let src = r#"
      (defn t [n]
        (condp = n
          1 :one
          2 :two
          :else))
      [(t 1) (t 2) (t 99)]
    "#;
    assert_eq!(run(src), Value::Vector(imbl::Vector::from_iter([
        Value::Keyword("one".into()),
        Value::Keyword("two".into()),
        Value::Keyword("else".into()),
    ])));
}

#[test]
fn assert_throws_on_false() {
    let env = Env::new();
    builtins::install(&env);
    let forms = reader::read_all("(assert false)").unwrap();
    let mut last = Ok(Value::Nil);
    for f in forms { last = eval::eval(&f, &env); }
    assert!(last.is_err());
    // Truthy assertion returns nil.
    assert_eq!(run("(assert true)"), Value::Nil);
}

#[test]
fn comment_drops_body() {
    assert_eq!(run("(comment (this would be (an error)))"), Value::Nil);
    assert_eq!(run("(do (comment 1 2 3) :ok)"),
               Value::Keyword("ok".into()));
}

#[test]
fn letfn_mutual_recursion() {
    let src = r#"
      (letfn [(evn? [n] (if (= n 0) true  (od? (- n 1))))
              (od?  [n] (if (= n 0) false (evn? (- n 1))))]
        [(evn? 10) (od? 7)])
    "#;
    assert_eq!(run(src), Value::Vector(imbl::Vector::from_iter([
        Value::Bool(true), Value::Bool(true),
    ])));
}

#[test]
fn delay_force_caches() {
    let src = r#"
      (def counter (atom 0))
      (def d (delay (do (swap! counter inc) 42)))
      (def a (force d))
      (def b (force d))
      [a b @counter]
    "#;
    assert_eq!(run(src), Value::Vector(imbl::Vector::from_iter([
        Value::Int(42), Value::Int(42), Value::Int(1),
    ])));
}

#[test]
fn ffirst_fnext_nfirst_nnext() {
    assert_eq!(run("(ffirst [[1 2] [3 4]])"), Value::Int(1));
    assert_eq!(run("(fnext  [1 2 3])"), Value::Int(2));
    assert_eq!(run("(count (nfirst [[1 2 3] [4 5 6]]))"), Value::Int(2));
    assert_eq!(run("(count (nnext [1 2 3 4]))"), Value::Int(2));
}

#[test]
fn not_any_not_every() {
    assert_eq!(run("(not-any? neg? [1 2 3])"), Value::Bool(true));
    assert_eq!(run("(not-any? neg? [1 -2 3])"), Value::Bool(false));
    assert_eq!(run("(not-every? pos? [1 2 -3])"), Value::Bool(true));
    assert_eq!(run("(not-every? pos? [1 2 3])"), Value::Bool(false));
}

#[test]
fn replace_lookup() {
    // Sequence form: each element is looked up in smap, kept if missing.
    let r = run("(replace {1 :a 2 :b} [1 2 3 1])");
    assert_eq!(r, Value::Vector(imbl::Vector::from_iter([
        Value::Keyword("a".into()),
        Value::Keyword("b".into()),
        Value::Int(3),
        Value::Keyword("a".into()),
    ])));
}

#[test]
fn every_pred_some_fn() {
    assert_eq!(run("((every-pred pos? even?) 4)"), Value::Bool(true));
    assert_eq!(run("((every-pred pos? even?) 3)"), Value::Bool(false));
    assert_eq!(run("((some-fn neg? zero?) -3)"), Value::Bool(true));
    // Matches Clojure semantics: no pred satisfied => nil, not false.
    assert_eq!(run("((some-fn neg? zero?) 5)"), Value::Nil);
}

#[test]
fn fnil_substitutes_nil() {
    let src = r#"
      (def safe+ (fnil + 0))
      [(safe+ nil 5) (safe+ 3 4)]
    "#;
    assert_eq!(run(src), Value::Vector(imbl::Vector::from_iter([
        Value::Int(5), Value::Int(7),
    ])));
}

#[test]
fn numeric_eq() {
    assert_eq!(run("(== 1 1 1)"), Value::Bool(true));
    assert_eq!(run("(== 1 2)"), Value::Bool(false));
    // float/int comparison works numerically
    assert_eq!(run("(== 1 1.0)"), Value::Bool(true));
}

#[test]
fn prn_str_returns_string() {
    let v = run(r#"(prn-str "hi")"#);
    match v {
        Value::Str(s) => assert_eq!(&*s, "\"hi\"\n"),
        _ => panic!("expected string, got {:?}", v),
    }
}
