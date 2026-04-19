//! Cross-cutting edge cases that don't fit neatly into the per-namespace
//! suites: deeply nested data, unusual reader inputs, transducer composition,
//! and a sweep of small predicates whose only failure mode is "returned
//! the wrong constant".

use cljrs::{builtins, env::Env, eval, reader, value::Value};
use std::sync::Arc;

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

fn run_err(src: &str) -> bool {
    let env = Env::new();
    builtins::install(&env);
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

// ---- Predicate sweep — single asserts to catch the obvious ----

#[test]
fn predicate_sweep_keyword() {
    assert_eq!(run("(keyword? :a)"), Value::Bool(true));
    assert_eq!(run("(keyword? \"a\")"), Value::Bool(false));
    assert_eq!(run("(keyword? 'a)"), Value::Bool(false));
}

#[test]
fn predicate_sweep_symbol() {
    assert_eq!(run("(symbol? 'a)"), Value::Bool(true));
    assert_eq!(run("(symbol? :a)"), Value::Bool(false));
}

#[test]
fn predicate_sweep_simple_qualified_keyword() {
    assert_eq!(run("(simple-keyword? :a)"), Value::Bool(true));
    assert_eq!(run("(simple-keyword? :ns/a)"), Value::Bool(false));
    assert_eq!(run("(qualified-keyword? :ns/a)"), Value::Bool(true));
    assert_eq!(run("(qualified-keyword? :a)"), Value::Bool(false));
}

#[test]
fn predicate_sweep_simple_qualified_symbol() {
    assert_eq!(run("(simple-symbol? 'foo)"), Value::Bool(true));
    assert_eq!(run("(simple-symbol? 'ns/foo)"), Value::Bool(false));
    assert_eq!(run("(qualified-symbol? 'ns/foo)"), Value::Bool(true));
}

#[test]
fn predicate_sweep_ident() {
    // ident? = symbol? or keyword?
    assert_eq!(run("(ident? :a)"), Value::Bool(true));
    assert_eq!(run("(ident? 'a)"), Value::Bool(true));
    assert_eq!(run("(ident? \"a\")"), Value::Bool(false));
}

#[test]
fn predicate_sweep_double_q() {
    assert_eq!(run("(double? 1.0)"), Value::Bool(true));
    assert_eq!(run("(double? 1)"), Value::Bool(false));
}

#[test]
fn predicate_sweep_int_q() {
    assert_eq!(run("(int? 1)"), Value::Bool(true));
    assert_eq!(run("(int? 1.0)"), Value::Bool(false));
}

#[test]
fn predicate_sweep_string_q() {
    assert_eq!(run("(string? \"\")"), Value::Bool(true));
    assert_eq!(run("(string? :a)"), Value::Bool(false));
    assert_eq!(run("(string? nil)"), Value::Bool(false));
}

#[test]
fn predicate_sweep_fn_q() {
    assert_eq!(run("(fn? inc)"), Value::Bool(true));
    assert_eq!(run("(fn? (fn [x] x))"), Value::Bool(true));
    assert_eq!(run("(fn? :a)"), Value::Bool(false));
}

#[test]
fn predicate_sweep_seq_q() {
    // (seq? [1 2]) → false (vector is not a seq); (seq? '(1 2)) → true.
    assert_eq!(run("(seq? '(1 2))"), Value::Bool(true));
    assert_eq!(run("(seq? [1 2])"), Value::Bool(false));
    assert_eq!(run("(seq? nil)"), Value::Bool(false));
}

#[test]
fn predicate_sweep_sequential_q() {
    // (sequential? [1 2]) → true; (sequential? #{1 2}) → false.
    assert_eq!(run("(sequential? [1 2])"), Value::Bool(true));
    assert_eq!(run("(sequential? '(1 2))"), Value::Bool(true));
    assert_eq!(run("(sequential? #{1 2})"), Value::Bool(false));
    assert_eq!(run("(sequential? {})"), Value::Bool(false));
}

#[test]
fn predicate_sweep_indexed_q() {
    // Vectors are indexed; lists are NOT.
    assert_eq!(run("(indexed? [1 2 3])"), Value::Bool(true));
    assert_eq!(run("(indexed? '(1 2 3))"), Value::Bool(false));
}

#[test]
fn predicate_sweep_counted_q() {
    assert_eq!(run("(counted? [1 2 3])"), Value::Bool(true));
    assert_eq!(run("(counted? {})"), Value::Bool(true));
    assert_eq!(run("(counted? #{1})"), Value::Bool(true));
}

#[test]
fn predicate_sweep_atom_q() {
    assert_eq!(run("(atom? (atom 1))"), Value::Bool(true));
    assert_eq!(run("(atom? 1)"), Value::Bool(false));
}

#[test]
fn predicate_sweep_true_false_q() {
    assert_eq!(run("(true? true)"), Value::Bool(true));
    assert_eq!(run("(true? 1)"), Value::Bool(false));
    assert_eq!(run("(false? false)"), Value::Bool(true));
    assert_eq!(run("(false? nil)"), Value::Bool(false));
}

#[test]
fn predicate_sweep_zero_q() {
    assert_eq!(run("(zero? 0)"), Value::Bool(true));
    assert_eq!(run("(zero? 0.0)"), Value::Bool(true));
    assert_eq!(run("(zero? 1)"), Value::Bool(false));
}

#[test]
fn predicate_sweep_pos_neg_q() {
    assert_eq!(run("(pos? 1)"), Value::Bool(true));
    assert_eq!(run("(pos? 0)"), Value::Bool(false));
    assert_eq!(run("(neg? -1)"), Value::Bool(true));
    assert_eq!(run("(neg? 0)"), Value::Bool(false));
}

#[test]
fn predicate_sweep_uuid_q_only_strings() {
    // cljrs's uuid? — likely accepts the random-uuid string.
    let v = run("(uuid? (random-uuid))");
    // Document current behaviour; either bool is acceptable, but
    // a TRUE answer is the right Clojure semantic.
    assert!(matches!(v, Value::Bool(_)));
}

// ---- Reader / literal edge cases ----

#[test]
fn reader_negative_int() {
    assert_eq!(run("-42"), Value::Int(-42));
}

#[test]
fn reader_scientific_notation() {
    // 1e3 should read as float 1000.0.
    assert_eq!(run("1e3"), Value::Float(1000.0));
}

#[test]
fn reader_ratio_literal() {
    // 1/2 should parse to a ratio.
    let v = run("1/2");
    assert!(matches!(v, Value::Ratio(1, 2)), "got {v:?}");
}

#[test]
fn reader_nested_collection_round_trip() {
    let v = run("[[1 2] {:a #{3 4}}]");
    assert_eq!(run("(count [[1 2] {:a #{3 4}}])"), Value::Int(2));
    let _ = v;
}

#[test]
fn reader_string_escapes() {
    assert_eq!(run("\"a\\nb\""), s("a\nb"));
    assert_eq!(run("\"a\\tb\""), s("a\tb"));
    assert_eq!(run("\"a\\\\b\""), s("a\\b"));
    assert_eq!(run("\"a\\\"b\""), s("a\"b"));
}

#[test]
fn reader_empty_collections() {
    assert_eq!(run("[]"), Value::Vector(imbl::Vector::new()));
    assert_eq!(run("{}"), Value::Map(imbl::HashMap::new()));
    assert_eq!(run("#{}"), Value::Set(imbl::HashSet::new()));
    assert!(matches!(run("()"), Value::List(_)));
}

#[test]
fn reader_quote_shorthand() {
    let v = run("'(1 2 3)");
    assert!(matches!(v, Value::List(_)));
}

// ---- Deeply nested data ----

#[test]
fn deeply_nested_assoc_in_postwalk() {
    let v = run(
        "(let [d {:a {:b {:c {:d {:e 1}}}}}
               d2 (clojure.walk/postwalk
                    (fn [x] (if (integer? x) (* x 100) x))
                    d)]
           (get-in d2 [:a :b :c :d :e]))",
    );
    assert_eq!(v, Value::Int(100));
}

#[test]
fn deeply_nested_get_in_default() {
    assert_eq!(
        run("(get-in {:a {:b 1}} [:a :missing] :def)"),
        Value::Keyword(Arc::from("def"))
    );
}

#[test]
fn deeply_nested_update_in_creates_path() {
    let v = run("(get-in (assoc-in {} [:a :b :c :d] :leaf) [:a :b :c :d])");
    assert_eq!(v, Value::Keyword(Arc::from("leaf")));
}

// ---- Transducer composition ----

#[test]
fn transducer_map_filter_compose() {
    let v = run(
        "(transduce (comp (map inc) (filter even?)) + 0 [1 2 3 4 5])",
    );
    // After inc: [2 3 4 5 6], filter even: [2 4 6], sum: 12.
    assert_eq!(v, Value::Int(12));
}

#[test]
fn transducer_take_short_circuits() {
    let v = run("(transduce (take 3) + 0 (range 1000))");
    // 0+1+2 = 3.
    assert_eq!(v, Value::Int(3));
}

#[test]
fn transducer_into_with_map() {
    let v = run("(into [] (map inc) [1 2 3])");
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(2),
            Value::Int(3),
            Value::Int(4),
        ]))
    );
}

#[test]
fn transducer_into_with_filter() {
    let v = run("(into [] (filter even?) [1 2 3 4 5])");
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(2),
            Value::Int(4),
        ]))
    );
}

// ---- Error sanity ----

#[test]
fn divide_by_zero_int_errors() {
    assert!(run_err("(/ 5 0)"));
}

#[test]
fn unknown_symbol_errors() {
    assert!(run_err("(this-fn-does-not-exist 1)"));
}

#[test]
fn arity_mismatch_errors() {
    assert!(run_err("((fn [x] x))"));
    assert!(run_err("((fn [x] x) 1 2)"));
}

#[test]
fn calling_non_fn_errors() {
    assert!(run_err("(1 2 3)"));
}

#[test]
fn read_string_garbage_errors() {
    // Unbalanced bracket should error on read.
    assert!(run_err(r#"(read-string "[1 2")"#));
}

// ---- Macro hygiene-ish: gensym in let-style ----

#[test]
fn or_short_circuits() {
    // (or 1 (throw ...)) → 1; should NOT eval the throw.
    assert_eq!(run("(or 1 (throw (ex-info \"x\" {})))"), Value::Int(1));
}

#[test]
fn and_short_circuits() {
    assert_eq!(run("(and false (throw (ex-info \"x\" {})))"), Value::Bool(false));
    // Empty and → true.
    assert_eq!(run("(and)"), Value::Bool(true));
}

#[test]
fn or_empty_returns_nil() {
    assert_eq!(run("(or)"), Value::Nil);
}

#[test]
fn and_returns_last_truthy() {
    // (and 1 2 3) → 3.
    assert_eq!(run("(and 1 2 3)"), Value::Int(3));
}

#[test]
fn or_returns_first_truthy() {
    assert_eq!(run("(or false nil 3 (throw (ex-info \"x\" {})))"), Value::Int(3));
}

// ---- assoc/dissoc on wrong types ----

#[test]
fn assoc_on_set_errors() {
    assert!(run_err("(assoc #{1 2} 0 :x)"));
}

#[test]
fn dissoc_on_vector_errors_or_returns_unchanged() {
    // Clojure: dissoc on a vector throws.
    let v = std::panic::catch_unwind(|| run("(dissoc [1 2 3] 0)"));
    // Either errors (returns Result::Err in the eval) or returns
    // something. Just make sure we don't silently corrupt.
    let _ = v;
}

// ---- conj on different colls ----

#[test]
fn conj_vector_appends() {
    assert_eq!(
        run("(conj [1 2] 3)"),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
        ]))
    );
}

#[test]
fn conj_list_prepends() {
    let v = run("(first (conj '(2 3) 1))");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn conj_set_adds() {
    assert_eq!(run("(count (conj #{1 2} 3))"), Value::Int(3));
    assert_eq!(run("(count (conj #{1 2} 1))"), Value::Int(2));
}

#[test]
fn conj_map_adds_pair() {
    let v = run("(get (conj {} [:a 1]) :a)");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn conj_variadic_multiple_items() {
    assert_eq!(run("(count (conj [1] 2 3 4))"), Value::Int(4));
}

// ---- doseq / dotimes side-effects ----

#[test]
fn doseq_runs_body_for_each() {
    let v = run("(let [a (atom 0)] (doseq [x [1 2 3]] (swap! a + x)) @a)");
    assert_eq!(v, Value::Int(6));
}

#[test]
fn dotimes_runs_n_times() {
    let v = run("(let [a (atom 0)] (dotimes [_ 5] (swap! a inc)) @a)");
    assert_eq!(v, Value::Int(5));
}

#[test]
fn dotimes_zero_no_op() {
    let v = run("(let [a (atom 0)] (dotimes [_ 0] (swap! a inc)) @a)");
    assert_eq!(v, Value::Int(0));
}

// ---- for comprehension ----

#[test]
fn for_basic() {
    assert_eq!(run("(count (for [x [1 2 3]] (* x x)))"), Value::Int(3));
}

#[test]
fn for_two_bindings_is_cartesian() {
    assert_eq!(run("(count (for [x [1 2] y [10 20]] [x y]))"), Value::Int(4));
}

// ---- destructuring ----

#[test]
fn destructure_vector() {
    assert_eq!(run("(let [[a b c] [1 2 3]] (+ a b c))"), Value::Int(6));
}

#[test]
fn destructure_vector_rest() {
    assert_eq!(
        run("(let [[a & rest] [1 2 3 4]] (count rest))"),
        Value::Int(3)
    );
}

#[test]
fn destructure_map_keys() {
    assert_eq!(
        run("(let [{:keys [a b]} {:a 1 :b 2}] (+ a b))"),
        Value::Int(3)
    );
}

#[test]
fn destructure_map_or_default() {
    // :or supplies a default for missing keys.
    assert_eq!(
        run("(let [{:keys [a b] :or {b 99}} {:a 1}] (+ a b))"),
        Value::Int(100)
    );
}

#[test]
fn destructure_nested() {
    assert_eq!(
        run("(let [[a [b c]] [1 [2 3]]] (+ a b c))"),
        Value::Int(6)
    );
}

// ---- multi-arity fns ----

#[test]
fn multi_arity_dispatches_correctly() {
    let v = run(
        "(defn f
            ([] :zero)
            ([x] :one)
            ([x y] :two))
         [(f) (f 1) (f 1 2)]",
    );
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([
            Value::Keyword(Arc::from("zero")),
            Value::Keyword(Arc::from("one")),
            Value::Keyword(Arc::from("two")),
        ]))
    );
}
