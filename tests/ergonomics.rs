//! Ergonomic surface-area: keyword/map invocation, map ops, string ops,
//! functional builders (comp/partial/etc). These are the "feels like
//! Clojure" primitives that unblock most idiomatic code.

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
fn keyword_invocation() {
    assert_eq!(run(r#"(:name {:name "ty"})"#), Value::Str("ty".into()));
    assert_eq!(run(r#"(:missing {:name "ty"})"#), Value::Nil);
    assert_eq!(run(r#"(:missing {:name "ty"} :fallback)"#), Value::Keyword("fallback".into()));
}

#[test]
fn map_invocation() {
    assert_eq!(run(r#"({:a 1 :b 2} :a)"#), Value::Int(1));
    assert_eq!(run(r#"({:a 1} :z 99)"#), Value::Int(99));
}

#[test]
fn set_invocation_membership() {
    assert_eq!(run("(#{1 2 3} 2)"), Value::Int(2));
    assert_eq!(run("(#{1 2 3} 99)"), Value::Nil);
}

#[test]
fn vector_invocation_nth() {
    assert_eq!(run("([10 20 30] 1)"), Value::Int(20));
}

#[test]
fn map_ops() {
    assert_eq!(run("(get {:a 1} :a)"), Value::Int(1));
    assert_eq!(run("(get {:a 1} :z 99)"), Value::Int(99));
    assert_eq!(run("(count (keys {:a 1 :b 2}))"), Value::Int(2));
    assert_eq!(run("(count (vals {:a 1 :b 2}))"), Value::Int(2));
    assert_eq!(run("(contains? {:a 1} :a)"), Value::Bool(true));
    assert_eq!(run("(contains? {:a 1} :b)"), Value::Bool(false));
    assert_eq!(run("(get (assoc {:a 1} :b 2) :b)"), Value::Int(2));
    assert_eq!(run("(get (dissoc {:a 1 :b 2} :a) :a 999)"), Value::Int(999));
    assert_eq!(run("(:c (merge {:a 1} {:b 2} {:c 3}))"), Value::Int(3));
}

#[test]
fn vector_ops_via_assoc() {
    assert_eq!(run("(get (assoc [1 2 3] 1 99) 1)"), Value::Int(99));
    // assoc at end extends
    assert_eq!(run("(count (assoc [1 2 3] 3 99))"), Value::Int(4));
}

#[test]
fn get_in_assoc_in() {
    assert_eq!(run("(get-in {:a {:b 42}} [:a :b])"), Value::Int(42));
    assert_eq!(run("(get-in {:a {:b 42}} [:x :y] :fallback)"), Value::Keyword("fallback".into()));
    assert_eq!(
        run("(get-in (assoc-in {:a {:b 1}} [:a :c] 99) [:a :c])"),
        Value::Int(99)
    );
}

#[test]
fn update_works() {
    assert_eq!(run("(get (update {:n 10} :n inc) :n)"), Value::Int(11));
}

#[test]
fn string_ops() {
    assert_eq!(run(r#"(subs "hello" 1 4)"#), Value::Str("ell".into()));
    assert_eq!(run(r#"(str/upper-case "hi")"#), Value::Str("HI".into()));
    assert_eq!(run(r#"(str/lower-case "Hi")"#), Value::Str("hi".into()));
    assert_eq!(run(r#"(str/includes? "hello world" "world")"#), Value::Bool(true));
    assert_eq!(run(r#"(str/trim "  hi  ")"#), Value::Str("hi".into()));
    assert_eq!(run(r#"(str/join "-" ["a" "b" "c"])"#), Value::Str("a-b-c".into()));
    assert_eq!(run(r#"(count (str/split "a,b,c" ","))"#), Value::Int(3));
}

#[test]
fn type_predicates() {
    assert_eq!(run(r#"(string? "x")"#), Value::Bool(true));
    assert_eq!(run("(integer? 3)"), Value::Bool(true));
    assert_eq!(run("(float? 3)"), Value::Bool(false));
    assert_eq!(run("(float? 3.1)"), Value::Bool(true));
    assert_eq!(run("(map? {})"), Value::Bool(true));
    assert_eq!(run("(vector? [])"), Value::Bool(true));
    assert_eq!(run("(some? nil)"), Value::Bool(false));
    assert_eq!(run("(some? 0)"), Value::Bool(true));
}

#[test]
fn closure_builders() {
    assert_eq!(run("((comp inc inc) 5)"), Value::Int(7));
    assert_eq!(run("((partial + 10) 5)"), Value::Int(15));
    assert_eq!(run("((complement zero?) 0)"), Value::Bool(false));
    assert_eq!(run("((complement zero?) 1)"), Value::Bool(true));
    assert_eq!(run("((constantly 42))"), Value::Int(42));
}

#[test]
fn juxt_multi() {
    // juxt returns a vector of each fn applied to the args
    let v = run("((juxt inc dec) 10)");
    assert_eq!(v, run("[11 9]"));
}

#[test]
fn anon_fn_reader() {
    assert_eq!(run("(#(+ % 1) 10)"), Value::Int(11));
    assert_eq!(run("(#(* %1 %2) 3 4)"), Value::Int(12));
    assert_eq!(run("(count (map #(* % %) [1 2 3]))"), Value::Int(3));
    assert_eq!(run("(first (map #(* % %) [2 3]))"), Value::Int(4));
}

#[test]
fn discard_reader_macro() {
    // #_ drops the next form
    assert_eq!(run("(+ 1 #_ 99 2)"), Value::Int(3));
}

#[test]
fn higher_order_seqs() {
    assert_eq!(run("(count (distinct [1 1 2 2 3 3]))"), Value::Int(3));
    assert_eq!(run("(first (reverse [1 2 3]))"), Value::Int(3));
    assert_eq!(run("(first (sort [3 1 2]))"), Value::Int(1));
    assert_eq!(run("(count (take-while pos? [3 2 1 0 -1 2]))"), Value::Int(3));
    assert_eq!(run("(first (drop-while pos? [3 2 1 0 -1]))"), Value::Int(0));
    assert_eq!(run("(get (frequencies [:a :a :b]) :a)"), Value::Int(2));
}
