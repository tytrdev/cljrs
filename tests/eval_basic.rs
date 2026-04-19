use std::sync::Arc;

use cljrs::{builtins, env::Env, eval, reader, value::Value};

fn run(src: &str) -> Value {
    let env = Env::new();
    builtins::install(&env);
    let forms = reader::read_all(src).expect("read failed");
    let mut result = Value::Nil;
    for f in forms {
        result = eval::eval(&f, &env).expect("eval failed");
    }
    result
}

#[test]
fn literals() {
    assert!(matches!(run("nil"), Value::Nil));
    assert_eq!(run("true"), Value::Bool(true));
    assert_eq!(run("false"), Value::Bool(false));
    assert_eq!(run("42"), Value::Int(42));
    assert_eq!(run("-7"), Value::Int(-7));
    match run("3.14") {
        Value::Float(f) => assert!((f - 3.14).abs() < 1e-9),
        v => panic!("expected float, got {v:?}"),
    }
    match run(r#""hello""#) {
        Value::Str(s) => assert_eq!(&*s, "hello"),
        v => panic!("expected string, got {v:?}"),
    }
    assert_eq!(run(":foo"), Value::Keyword(Arc::from("foo")));
}

#[test]
fn arithmetic() {
    assert_eq!(run("(+ 1 2)"), Value::Int(3));
    assert_eq!(run("(+ 1 2 3 4 5)"), Value::Int(15));
    assert_eq!(run("(- 10 3)"), Value::Int(7));
    assert_eq!(run("(- 5)"), Value::Int(-5));
    assert_eq!(run("(* 3 4)"), Value::Int(12));
    assert_eq!(run("(/ 10 2)"), Value::Int(5));
    assert_eq!(run("(+ 1.5 2.5)"), Value::Float(4.0));
    assert_eq!(run("(* 2 3.0)"), Value::Float(6.0));
    // integer overflow promotes to float via fold_num's None branch
    assert!(matches!(run("(* 9999999999 9999999999)"), Value::Float(_)));
}

#[test]
fn truthiness() {
    assert_eq!(run("(if nil :t :f)"), Value::Keyword(Arc::from("f")));
    assert_eq!(run("(if false :t :f)"), Value::Keyword(Arc::from("f")));
    assert_eq!(run("(if 0 :t :f)"), Value::Keyword(Arc::from("t")));
    assert_eq!(run(r#"(if "" :t :f)"#), Value::Keyword(Arc::from("t")));
    assert_eq!(run("(if [] :t :f)"), Value::Keyword(Arc::from("t")));
    assert!(matches!(run("(if false 42)"), Value::Nil));
}

#[test]
fn def_and_lookup() {
    assert_eq!(run("(def x 42) x"), Value::Int(42));
    assert_eq!(run("(def x 1) (def y 2) (+ x y)"), Value::Int(3));
}

#[test]
fn let_bindings() {
    assert_eq!(run("(let [x 1 y 2] (+ x y))"), Value::Int(3));
    assert_eq!(run("(let [x 1 x 2] x)"), Value::Int(2));
    assert_eq!(run("(let [x 1] (let [y 2] (+ x y)))"), Value::Int(3));
    assert_eq!(run("(let [x 1 y (* x 10)] y)"), Value::Int(10));
}

#[test]
fn fn_and_closures() {
    assert_eq!(run("((fn [x] (* x x)) 5)"), Value::Int(25));
    assert_eq!(run("(let [n 10] ((fn [x] (+ x n)) 5))"), Value::Int(15));
    assert_eq!(run("((fn [& xs] (count xs)) 1 2 3)"), Value::Int(3));
    assert_eq!(run("((fn [a & xs] (count xs)) 0 1 2 3)"), Value::Int(3));
}

#[test]
fn recursion_via_defn() {
    let src = r#"
        (defn fact [n]
          (if (<= n 1) 1 (* n (fact (- n 1)))))
        (fact 5)
    "#;
    assert_eq!(run(src), Value::Int(120));
}

#[test]
fn recursion_via_named_fn() {
    let src = r#"
        ((fn fact [n]
           (if (<= n 1) 1 (* n (fact (- n 1)))))
         6)
    "#;
    assert_eq!(run(src), Value::Int(720));
}

#[test]
fn fibonacci() {
    let src = r#"
        (defn fib [n]
          (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))
        (fib 10)
    "#;
    assert_eq!(run(src), Value::Int(55));
}

#[test]
fn quote_and_list() {
    assert_eq!(run("(count '(1 2 3 4))"), Value::Int(4));
    assert_eq!(run("(first '(a b c))"), Value::Symbol(Arc::from("a")));
}

#[test]
fn collections() {
    assert_eq!(run("(count [1 2 3])"), Value::Int(3));
    assert_eq!(run("(first [10 20 30])"), Value::Int(10));
    assert_eq!(run("(first (rest [10 20 30]))"), Value::Int(20));
    assert_eq!(run("(count (cons 0 [1 2 3]))"), Value::Int(4));
    assert_eq!(run("(first (cons 0 [1 2 3]))"), Value::Int(0));
    assert_eq!(run("(count (conj [1 2] 3 4))"), Value::Int(4));
    assert_eq!(run("(nth [10 20 30] 1)"), Value::Int(20));
}

#[test]
fn equality() {
    assert_eq!(run("(= 1 1)"), Value::Bool(true));
    assert_eq!(run("(= 1 2)"), Value::Bool(false));
    assert_eq!(run("(= [1 2 3] [1 2 3])"), Value::Bool(true));
    assert_eq!(run("(= [1 2 3] '(1 2 3))"), Value::Bool(true));
    assert_eq!(run(r#"(= "hi" "hi")"#), Value::Bool(true));
    assert_eq!(run("(= :foo :foo)"), Value::Bool(true));
    assert_eq!(run("(= :foo :bar)"), Value::Bool(false));
    // Clojure-strict: (= 1 1.0) is false (different numeric types).
    // Use (== 1 1.0) for numeric coercion.
    assert_eq!(run("(= 1 1.0)"), Value::Bool(false));
    assert_eq!(run("(== 1 1.0)"), Value::Bool(true));
}

#[test]
fn do_block() {
    assert_eq!(run("(do 1 2 3)"), Value::Int(3));
    assert_eq!(run("(do (def x 10) (def y 20) (+ x y))"), Value::Int(30));
}

#[test]
fn higher_order() {
    let src = r#"
        (defn apply-twice [f x] (f (f x)))
        (apply-twice inc 5)
    "#;
    assert_eq!(run(src), Value::Int(7));
}

#[test]
fn comments_and_whitespace() {
    let src = r#"
        ; this is a comment
        (+ 1 2 3) ; trailing
    "#;
    assert_eq!(run(src), Value::Int(6));
}

#[test]
fn map_literal() {
    let v = run("{:a 1 :b 2}");
    match v {
        Value::Map(m) => assert_eq!(m.len(), 2),
        _ => panic!("expected map"),
    }
}

#[test]
fn mutual_recursion() {
    let src = r#"
        (defn even? [n] (if (= n 0) true  (odd?  (- n 1))))
        (defn odd?  [n] (if (= n 0) false (even? (- n 1))))
        (even? 10)
    "#;
    assert_eq!(run(src), Value::Bool(true));
}
