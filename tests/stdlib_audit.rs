//! Bug-hunting audit suite for clojure.core implementations.
//!
//! These tests target Clojure-semantics conformance for the ~390
//! clojure.core fns cljrs implements. Each test exercises an edge
//! case (nil-punning, empty colls, variadic boundaries, mixed-type
//! comparisons, etc.) — anything beyond happy-path that's likely
//! to surface a porting bug.
//!
//! Helpers `run` / `run_err` / `pr` mirror the pattern from
//! tests/core_clj.rs. Each test comment names the INTENT so a reader
//! can immediately see what bug a failure points at.

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
    let mut last = Ok(Value::Nil);
    for f in forms {
        last = eval::eval(&f, &env);
        if last.is_err() {
            return true;
        }
    }
    false
}

fn pr(src: &str) -> String {
    run(src).to_pr_string()
}

fn s(x: &str) -> Value {
    Value::Str(Arc::from(x))
}
fn k(x: &str) -> Value {
    Value::Keyword(Arc::from(x))
}

// =====================================================================
// Arithmetic — variadic edge cases & numeric coercion
// =====================================================================

#[test]
fn arith_plus_zero_arity_is_zero() {
    // (+) → 0 in Clojure. Catches: builtin returning nil or erroring.
    assert_eq!(run("(+)"), Value::Int(0));
}

#[test]
fn arith_star_zero_arity_is_one() {
    // (*) → 1.
    assert_eq!(run("(*)"), Value::Int(1));
}

#[test]
fn arith_plus_one_arity_is_identity() {
    // (+ 5) → 5.
    assert_eq!(run("(+ 5)"), Value::Int(5));
    assert_eq!(run("(+ 5.5)"), Value::Float(5.5));
}

#[test]
fn arith_minus_one_arity_negates() {
    // (- 5) → -5; (- 5.5) → -5.5.
    assert_eq!(run("(- 5)"), Value::Int(-5));
    assert_eq!(run("(- 5.5)"), Value::Float(-5.5));
}

#[test]
fn arith_div_one_arity_reciprocal() {
    // (/ 4) → 1/4 (a ratio).
    let v = run("(/ 4)");
    match v {
        Value::Ratio(1, 4) => {}
        Value::Float(f) if (f - 0.25).abs() < 1e-9 => {}
        other => panic!("expected 1/4, got {other:?}"),
    }
}

#[test]
fn arith_div_by_zero_errors() {
    // Integer division by zero must error (not produce inf).
    assert!(run_err("(/ 1 0)"));
}

#[test]
fn arith_minus_zero_arity_errors() {
    // (-) is an arity error in Clojure.
    assert!(run_err("(-)"));
}

#[test]
fn arith_div_zero_arity_errors() {
    // (/) is an arity error.
    assert!(run_err("(/)"));
}

#[test]
fn arith_int_float_promotion() {
    // (+ 1 1.0) → 2.0 (float promotes).
    assert_eq!(run("(+ 1 1.0)"), Value::Float(2.0));
}

#[test]
fn arith_ratio_plus_int() {
    // (+ 1/2 1) → 3/2.
    let v = run("(+ (/ 1 2) 1)");
    assert!(matches!(v, Value::Ratio(3, 2)), "got {v:?}");
}

#[test]
fn arith_ratio_demotes_to_float() {
    // Mixing ratio with float gives float.
    let v = run("(+ (/ 1 2) 0.5)");
    assert_eq!(v, Value::Float(1.0));
}

// =====================================================================
// Comparison operators
// =====================================================================

#[test]
fn equals_int_float_clojure_semantics() {
    // (= 1 1.0) is FALSE in Clojure (different types). cljrs may
    // return true (Value::PartialEq is permissive). This test
    // documents the current behavior — change expectation if/when
    // cljrs aligns with Clojure.
    let v = run("(= 1 1.0)");
    // Document: should be false per Clojure spec.
    assert_eq!(v, Value::Bool(false), "Clojure: (= 1 1.0) is false");
}

#[test]
fn double_equals_numeric_int_float() {
    // (== 1 1.0) MUST be true.
    assert_eq!(run("(== 1 1.0)"), Value::Bool(true));
}

#[test]
fn double_equals_variadic_chain() {
    // (== 1 1 1.0 1) → true; (== 1 2 1) → false.
    assert_eq!(run("(== 1 1 1.0 1)"), Value::Bool(true));
    assert_eq!(run("(== 1 2 1)"), Value::Bool(false));
}

#[test]
fn double_equals_one_arg_true() {
    // (== 5) → true.
    assert_eq!(run("(== 5)"), Value::Bool(true));
}

#[test]
fn double_equals_with_non_number_errors_or_false() {
    // (== 1 :a) — Clojure throws ClassCastException; cljrs's defn
    // currently returns false because of `(and (number? x) ...)`.
    let v = run("(== 1 :a)");
    assert_eq!(v, Value::Bool(false));
}

#[test]
fn lt_variadic_chain() {
    // (< 1 2 3) → true; (< 1 3 2) → false.
    assert_eq!(run("(< 1 2 3)"), Value::Bool(true));
    assert_eq!(run("(< 1 3 2)"), Value::Bool(false));
}

#[test]
fn lt_single_arg_true() {
    // (< 5) → true.
    assert_eq!(run("(< 5)"), Value::Bool(true));
}

#[test]
fn lt_equal_values_false() {
    // (< 1 1) → false (strict).
    assert_eq!(run("(< 1 1)"), Value::Bool(false));
    // (<= 1 1) → true.
    assert_eq!(run("(<= 1 1)"), Value::Bool(true));
}

#[test]
fn equals_mixed_collections_list_vs_vector() {
    // (= '(1 2 3) [1 2 3]) → true (Clojure: seqs are equal across
    // sequence types of same elements).
    assert_eq!(run("(= '(1 2 3) [1 2 3])"), Value::Bool(true));
}

#[test]
fn equals_set_vs_vector_false() {
    // (= #{1 2 3} [1 2 3]) → false (different category).
    assert_eq!(run("(= #{1 2 3} [1 2 3])"), Value::Bool(false));
}

#[test]
fn equals_maps_with_same_kv() {
    // Maps equal regardless of insertion order.
    assert_eq!(run("(= {:a 1 :b 2} {:b 2 :a 1})"), Value::Bool(true));
}

// =====================================================================
// Nil-punning on coll fns
// =====================================================================

#[test]
fn nil_first_returns_nil() {
    assert_eq!(run("(first nil)"), Value::Nil);
}

#[test]
fn nil_rest_returns_empty_seq() {
    // Clojure: (rest nil) → ().
    let v = run("(rest nil)");
    // Either an empty list or empty vector or nil-as-seq.
    let len = match &v {
        Value::List(xs) => xs.len(),
        Value::Vector(xs) => xs.len(),
        Value::Nil => 0,
        _ => 999,
    };
    assert_eq!(len, 0, "(rest nil) should be empty seq, got {v:?}");
}

#[test]
fn nil_seq_returns_nil() {
    // (seq nil) → nil; (seq []) → nil.
    assert_eq!(run("(seq nil)"), Value::Nil);
    assert_eq!(run("(seq [])"), Value::Nil);
    assert_eq!(run("(seq \"\")"), Value::Nil);
    assert_eq!(run("(seq {})"), Value::Nil);
    assert_eq!(run("(seq #{})"), Value::Nil);
}

#[test]
fn nil_count_zero() {
    assert_eq!(run("(count nil)"), Value::Int(0));
}

#[test]
fn nil_map_returns_empty_seq() {
    // (map inc nil) → ().
    let v = run("(count (map inc nil))");
    assert_eq!(v, Value::Int(0));
}

#[test]
fn nil_filter_returns_empty_seq() {
    assert_eq!(run("(count (filter pos? nil))"), Value::Int(0));
}

#[test]
fn nil_reduce_returns_init() {
    // (reduce + 7 nil) → 7.
    assert_eq!(run("(reduce + 7 nil)"), Value::Int(7));
}

#[test]
fn nil_into_returns_empty_target() {
    assert_eq!(run("(into [] nil)"), Value::Vector(imbl::Vector::new()));
}

#[test]
fn nil_get_returns_nil() {
    assert_eq!(run("(get nil :a)"), Value::Nil);
    assert_eq!(run("(get nil :a :default)"), k("default"));
}

#[test]
fn nil_assoc_starts_map() {
    // (assoc nil :a 1) → {:a 1}.
    assert_eq!(run("(get (assoc nil :a 1) :a)"), Value::Int(1));
}

#[test]
fn nil_keys_vals_nil() {
    assert_eq!(run("(keys nil)"), Value::Nil);
    assert_eq!(run("(vals nil)"), Value::Nil);
}

#[test]
fn nil_conj_starts_list() {
    // (conj nil 1 2) → (2 1).
    let v = run("(conj nil 1 2)");
    let items: Vec<Value> = match v {
        Value::List(xs) => xs.as_ref().clone(),
        Value::Vector(xs) => xs.iter().cloned().collect(),
        other => panic!("expected list, got {other:?}"),
    };
    assert_eq!(items, vec![Value::Int(2), Value::Int(1)]);
}

#[test]
fn nil_contains_false() {
    assert_eq!(run("(contains? nil :a)"), Value::Bool(false));
}

#[test]
fn nil_empty_q_true() {
    assert_eq!(run("(empty? nil)"), Value::Bool(true));
}

#[test]
fn nil_threading_short_circuit() {
    // (some-> nil :a :b) → nil.
    assert_eq!(run("(some-> nil :a :b)"), Value::Nil);
    // (-> nil :a) — Clojure: (:a nil) → nil; should NOT crash.
    assert_eq!(run("(-> nil :a)"), Value::Nil);
}

// =====================================================================
// Empty-collection behaviour
// =====================================================================

#[test]
fn reduce_empty_no_init_calls_zero_arity() {
    // (reduce + []) → (+) → 0.
    assert_eq!(run("(reduce + [])"), Value::Int(0));
    assert_eq!(run("(reduce * [])"), Value::Int(1));
}

#[test]
fn reduce_empty_with_init_returns_init() {
    assert_eq!(run("(reduce + 42 [])"), Value::Int(42));
}

#[test]
fn reduce_single_no_init_returns_element() {
    // (reduce + [9]) → 9 (no fn call).
    assert_eq!(run("(reduce + [9])"), Value::Int(9));
}

#[test]
fn apply_empty_seq_zero_arity() {
    // (apply + []) → 0.
    assert_eq!(run("(apply + [])"), Value::Int(0));
}

#[test]
fn empty_vec_first_nil() {
    assert_eq!(run("(first [])"), Value::Nil);
    assert_eq!(run("(last [])"), Value::Nil);
}

#[test]
fn empty_map_keys_vals() {
    // (keys {}) — Clojure: nil; some impls: ().
    let v = run("(keys {})");
    assert!(
        matches!(v, Value::Nil)
            || matches!(&v, Value::List(xs) if xs.is_empty())
            || matches!(&v, Value::Vector(xs) if xs.is_empty()),
        "expected nil or empty seq, got {v:?}"
    );
}

#[test]
fn empty_vec_butlast_nil() {
    // (butlast []) → nil in Clojure.
    let v = run("(butlast [])");
    assert!(
        matches!(v, Value::Nil) || matches!(&v, Value::List(xs) if xs.is_empty()),
        "got {v:?}"
    );
}

// =====================================================================
// Predicates — correct type discrimination
// =====================================================================

#[test]
fn integer_q_false_for_float() {
    // (integer? 1.0) → false.
    assert_eq!(run("(integer? 1.0)"), Value::Bool(false));
    assert_eq!(run("(integer? 1)"), Value::Bool(true));
}

#[test]
fn float_q_true_only_for_float() {
    assert_eq!(run("(float? 1.0)"), Value::Bool(true));
    assert_eq!(run("(float? 1)"), Value::Bool(false));
}

#[test]
fn number_q_excludes_strings() {
    assert_eq!(run("(number? \"5\")"), Value::Bool(false));
    assert_eq!(run("(number? 5)"), Value::Bool(true));
    assert_eq!(run("(number? nil)"), Value::Bool(false));
}

#[test]
fn nat_int_q_zero_is_nat() {
    // (nat-int? 0) → true; (nat-int? -1) → false.
    assert_eq!(run("(nat-int? 0)"), Value::Bool(true));
    assert_eq!(run("(nat-int? -1)"), Value::Bool(false));
    assert_eq!(run("(nat-int? 1.0)"), Value::Bool(false));
}

#[test]
fn pos_int_q_zero_excluded() {
    // (pos-int? 0) → false; (pos-int? 1) → true.
    assert_eq!(run("(pos-int? 0)"), Value::Bool(false));
    assert_eq!(run("(pos-int? 1)"), Value::Bool(true));
}

#[test]
fn neg_int_q_zero_excluded() {
    assert_eq!(run("(neg-int? 0)"), Value::Bool(false));
    assert_eq!(run("(neg-int? -1)"), Value::Bool(true));
}

#[test]
fn even_odd_floats_error_or_correct() {
    // (even? 2) true; (odd? 3) true.
    assert_eq!(run("(even? 2)"), Value::Bool(true));
    assert_eq!(run("(odd? 3)"), Value::Bool(true));
    // (even? 0) → true.
    assert_eq!(run("(even? 0)"), Value::Bool(true));
}

#[test]
fn boolean_q_only_bool() {
    assert_eq!(run("(boolean? true)"), Value::Bool(true));
    assert_eq!(run("(boolean? false)"), Value::Bool(true));
    assert_eq!(run("(boolean? nil)"), Value::Bool(false));
    assert_eq!(run("(boolean? 1)"), Value::Bool(false));
}

#[test]
fn coll_q_string_false() {
    // (coll? "abc") → false in Clojure.
    assert_eq!(run("(coll? \"abc\")"), Value::Bool(false));
    assert_eq!(run("(coll? [1])"), Value::Bool(true));
    assert_eq!(run("(coll? {})"), Value::Bool(true));
    assert_eq!(run("(coll? #{})"), Value::Bool(true));
    assert_eq!(run("(coll? nil)"), Value::Bool(false));
}

#[test]
fn seqable_q_string_true() {
    // (seqable? "abc") → true.
    assert_eq!(run("(seqable? \"abc\")"), Value::Bool(true));
    assert_eq!(run("(seqable? nil)"), Value::Bool(true));
    assert_eq!(run("(seqable? 1)"), Value::Bool(false));
}

#[test]
fn associative_q_vector_true_set_false() {
    // Vectors are associative (idx → val), sets are not.
    assert_eq!(run("(associative? [1 2])"), Value::Bool(true));
    assert_eq!(run("(associative? {:a 1})"), Value::Bool(true));
    assert_eq!(run("(associative? #{1 2})"), Value::Bool(false));
}

#[test]
fn ifn_q_keyword_set_map_true() {
    // Keywords, maps, sets are all callable in Clojure.
    assert_eq!(run("(ifn? :a)"), Value::Bool(true));
    assert_eq!(run("(ifn? {})"), Value::Bool(true));
    assert_eq!(run("(ifn? #{})"), Value::Bool(true));
    assert_eq!(run("(ifn? inc)"), Value::Bool(true));
    assert_eq!(run("(ifn? 1)"), Value::Bool(false));
}

#[test]
fn distinct_q_true_for_distinct() {
    assert_eq!(run("(distinct? 1 2 3)"), Value::Bool(true));
    assert_eq!(run("(distinct? 1 2 1)"), Value::Bool(false));
    // Single arg is true.
    assert_eq!(run("(distinct? 1)"), Value::Bool(true));
}

#[test]
fn some_q_nil_only_returns_false_for_nil() {
    assert_eq!(run("(some? nil)"), Value::Bool(false));
    assert_eq!(run("(some? false)"), Value::Bool(true));
    assert_eq!(run("(some? 0)"), Value::Bool(true));
}

#[test]
fn any_q_always_true() {
    // (any? x) → true for ALL x (Clojure 1.9+).
    assert_eq!(run("(any? nil)"), Value::Bool(true));
    assert_eq!(run("(any? 1)"), Value::Bool(true));
}

// =====================================================================
// Higher-order: comp / juxt / partial / complement / constantly
// =====================================================================

#[test]
fn comp_zero_arity_is_identity() {
    // (comp) → identity.
    assert_eq!(run("((comp) 7)"), Value::Int(7));
}

#[test]
fn comp_inc_inc_doubled() {
    assert_eq!(run("((comp inc inc) 5)"), Value::Int(7));
}

#[test]
fn comp_str_inc_compose() {
    assert_eq!(run("((comp str inc) 4)"), s("5"));
}

#[test]
fn comp_variadic_first_fn() {
    // ((comp + *) 2 3) → (+ (* 2 3)) → 6 (last fn takes all args).
    assert_eq!(run("((comp + *) 2 3)"), Value::Int(6));
}

#[test]
fn juxt_returns_vector() {
    // ((juxt :a :b) {:a 1 :b 2}) → [1 2].
    assert_eq!(
        run("((juxt :a :b) {:a 1 :b 2})"),
        Value::Vector(imbl::Vector::from_iter([Value::Int(1), Value::Int(2)]))
    );
}

#[test]
fn juxt_single_fn() {
    assert_eq!(
        run("((juxt inc) 5)"),
        Value::Vector(imbl::Vector::from_iter([Value::Int(6)]))
    );
}

#[test]
fn partial_zero_extra_args() {
    // (partial inc) → inc (essentially).
    assert_eq!(run("((partial inc) 5)"), Value::Int(6));
}

#[test]
fn partial_with_args() {
    assert_eq!(run("((partial + 1 2) 3 4)"), Value::Int(10));
}

#[test]
fn complement_inverts_truthiness() {
    assert_eq!(run("((complement nil?) 1)"), Value::Bool(true));
    assert_eq!(run("((complement nil?) nil)"), Value::Bool(false));
}

#[test]
fn constantly_ignores_args() {
    assert_eq!(run("((constantly 42))"), Value::Int(42));
    assert_eq!(run("((constantly 42) 1 2 3)"), Value::Int(42));
}

#[test]
fn identity_passthrough() {
    assert_eq!(run("(identity nil)"), Value::Nil);
    assert_eq!(run("(identity 7)"), Value::Int(7));
}

// =====================================================================
// every-pred / some-fn / fnil — high-risk targets
// =====================================================================

#[test]
fn every_pred_single_pred_all_args() {
    // ((every-pred pos?) 1 2 3) → true.
    assert_eq!(run("((every-pred pos?) 1 2 3)"), Value::Bool(true));
    assert_eq!(run("((every-pred pos?) 1 -2 3)"), Value::Bool(false));
}

#[test]
fn every_pred_two_preds() {
    assert_eq!(run("((every-pred pos? even?) 2 4 6)"), Value::Bool(true));
    assert_eq!(run("((every-pred pos? even?) 2 3 6)"), Value::Bool(false));
}

#[test]
fn every_pred_three_preds() {
    assert_eq!(
        run("((every-pred pos? even? integer?) 2 4 6)"),
        Value::Bool(true)
    );
    assert_eq!(
        run("((every-pred pos? even? integer?) 2 4 6.0)"),
        Value::Bool(false)
    );
}

#[test]
fn every_pred_no_args_true() {
    // Clojure: ((every-pred pos?)) → true (vacuous truth).
    assert_eq!(run("((every-pred pos?))"), Value::Bool(true));
}

#[test]
fn some_fn_single_pred() {
    // ((some-fn pos?) 1) → true; ((some-fn pos?) -1) → false (or nil).
    assert_eq!(run("((some-fn pos?) 1)"), Value::Bool(true));
    let v = run("((some-fn pos?) -1)");
    assert!(
        matches!(v, Value::Nil | Value::Bool(false)),
        "expected falsey, got {v:?}"
    );
}

#[test]
fn some_fn_two_preds_first_hits() {
    assert_eq!(run("((some-fn pos? neg?) 1)"), Value::Bool(true));
}

#[test]
fn some_fn_three_preds() {
    let v = run("((some-fn pos? neg? zero?) 0)");
    assert_eq!(v, Value::Bool(true));
}

#[test]
fn some_fn_returns_first_truthy_value() {
    // Clojure: some-fn returns the predicate's truthy result, not just true.
    assert_eq!(run("((some-fn :a :b) {:b 99})"), Value::Int(99));
}

#[test]
fn fnil_replaces_one_nil() {
    assert_eq!(run("((fnil inc 0) nil)"), Value::Int(1));
    assert_eq!(run("((fnil inc 0) 5)"), Value::Int(6));
}

#[test]
fn fnil_two_defaults() {
    assert_eq!(run("((fnil + 1 2) nil nil)"), Value::Int(3));
    assert_eq!(run("((fnil + 1 2) 10 nil)"), Value::Int(12));
    assert_eq!(run("((fnil + 1 2) nil 10)"), Value::Int(11));
}

#[test]
fn fnil_three_defaults() {
    assert_eq!(run("((fnil + 1 2 3) nil nil nil)"), Value::Int(6));
}

#[test]
fn fnil_passes_extra_args() {
    // ((fnil + 0) nil 1 2 3) → 6.
    assert_eq!(run("((fnil + 0) nil 1 2 3)"), Value::Int(6));
}

// =====================================================================
// Threading macros & let-family
// =====================================================================

#[test]
fn threading_first_basic() {
    assert_eq!(run("(-> 5 inc inc)"), Value::Int(7));
}

#[test]
fn threading_last_basic() {
    assert_eq!(run("(->> [1 2 3] (map inc) (reduce +))"), Value::Int(9));
}

#[test]
fn some_threading_short_circuits_on_nil() {
    assert_eq!(run("(some-> {:a {:b 1}} :a :b)"), Value::Int(1));
    assert_eq!(run("(some-> {:a nil} :a :b)"), Value::Nil);
}

#[test]
fn cond_threading_only_runs_truthy_branches() {
    let v = run("(cond-> 1 true inc false (+ 100) true (* 2))");
    assert_eq!(v, Value::Int(4));
}

#[test]
fn if_let_truthy_path() {
    assert_eq!(run("(if-let [x 5] (* x 2) :nope)"), Value::Int(10));
    assert_eq!(run("(if-let [x nil] x :nope)"), k("nope"));
}

#[test]
fn when_let_returns_nil_on_falsey() {
    assert_eq!(run("(when-let [x nil] :never)"), Value::Nil);
}

#[test]
fn as_arrow_binds_intermediate() {
    let v = run("(as-> 1 v (+ v 2) (* v 10))");
    assert_eq!(v, Value::Int(30));
}

// =====================================================================
// String / regex
// =====================================================================

#[test]
fn str_zero_args_empty() {
    assert_eq!(run("(str)"), s(""));
}

#[test]
fn str_nil_omitted() {
    // (str nil) → "" in Clojure.
    assert_eq!(run("(str nil)"), s(""));
    assert_eq!(run("(str \"a\" nil \"b\")"), s("ab"));
}

#[test]
fn str_concats_numbers() {
    assert_eq!(run("(str 1 2 3)"), s("123"));
}

#[test]
fn subs_two_arg_to_end() {
    assert_eq!(run("(subs \"hello\" 2)"), s("llo"));
}

#[test]
fn subs_three_arg_range() {
    assert_eq!(run("(subs \"hello\" 1 4)"), s("ell"));
}

#[test]
fn subs_out_of_range_errors() {
    assert!(run_err("(subs \"abc\" 0 100)"));
}

#[test]
fn count_string_codepoint() {
    assert_eq!(run("(count \"abc\")"), Value::Int(3));
    // multibyte UTF-8: "héllo" has 5 codepoints.
    assert_eq!(run("(count \"héllo\")"), Value::Int(5));
}

#[test]
fn string_split_with_regex() {
    let v = run("(str/split \"a,b,c\" #\",\")");
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([s("a"), s("b"), s("c")]))
    );
}

#[test]
fn string_join_no_sep() {
    assert_eq!(run("(str/join [1 2 3])"), s("123"));
}

#[test]
fn string_join_with_sep() {
    assert_eq!(run("(str/join \", \" [1 2 3])"), s("1, 2, 3"));
}

#[test]
fn string_blank_q() {
    assert_eq!(run("(str/blank? \"\")"), Value::Bool(true));
    assert_eq!(run("(str/blank? \"  \")"), Value::Bool(true));
    assert_eq!(run("(str/blank? nil)"), Value::Bool(true));
    assert_eq!(run("(str/blank? \"x\")"), Value::Bool(false));
}

#[test]
fn re_find_no_match_nil() {
    assert_eq!(run("(re-find #\"xyz\" \"abc\")"), Value::Nil);
}

#[test]
fn re_find_groups_returns_vector() {
    // (re-find #"(\w)(\w)" "ab") → ["ab" "a" "b"].
    let v = run("(re-find #\"(\\w)(\\w)\" \"ab\")");
    match v {
        Value::Vector(xs) => assert_eq!(xs.len(), 3),
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn re_matches_full_string() {
    assert_eq!(run("(re-matches #\"\\d+\" \"123\")"), s("123"));
    assert_eq!(run("(re-matches #\"\\d+\" \"123abc\")"), Value::Nil);
}

// =====================================================================
// Maps / sets
// =====================================================================

#[test]
fn map_get_with_default() {
    assert_eq!(run("(get {:a 1} :missing 99)"), Value::Int(99));
}

#[test]
fn map_get_nil_value_distinct_from_missing() {
    // (get {:a nil} :a :default) → nil (key present!).
    assert_eq!(run("(get {:a nil} :a :default)"), Value::Nil);
}

#[test]
fn contains_q_map_checks_key_not_value() {
    assert_eq!(run("(contains? {:a 1} :a)"), Value::Bool(true));
    assert_eq!(run("(contains? {:a 1} 1)"), Value::Bool(false));
}

#[test]
fn contains_q_vector_checks_index() {
    // (contains? [10 20 30] 1) → true; (contains? [10 20 30] 10) → false.
    assert_eq!(run("(contains? [10 20 30] 1)"), Value::Bool(true));
    assert_eq!(run("(contains? [10 20 30] 10)"), Value::Bool(false));
}

#[test]
fn assoc_in_creates_nested_map() {
    let v = run("(get-in (assoc-in {} [:a :b :c] 1) [:a :b :c])");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn update_in_applies_fn() {
    let v = run("(get-in (update-in {:a {:b 1}} [:a :b] inc) [:a :b])");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn merge_later_wins() {
    let v = run("(get (merge {:a 1} {:a 2}) :a)");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn merge_with_combines() {
    let v = run("(get (merge-with + {:a 1 :b 2} {:a 10 :b 20}) :a)");
    assert_eq!(v, Value::Int(11));
    let v2 = run("(get (merge-with + {:a 1 :b 2} {:a 10 :b 20}) :b)");
    assert_eq!(v2, Value::Int(22));
}

#[test]
fn merge_with_no_collision_passthrough() {
    let v = run("(get (merge-with + {:a 1} {:b 2}) :a)");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn merge_nil_args() {
    // (merge nil {:a 1}) → {:a 1}.
    assert_eq!(run("(get (merge nil {:a 1}) :a)"), Value::Int(1));
    // (merge {:a 1} nil) → {:a 1}.
    assert_eq!(run("(get (merge {:a 1} nil) :a)"), Value::Int(1));
}

#[test]
fn dissoc_missing_key_no_op() {
    let v = run("(count (dissoc {:a 1} :z))");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn select_keys_missing_omitted() {
    // (select-keys {:a 1 :b 2} [:a :missing]) → {:a 1}.
    let v = run("(count (select-keys {:a 1 :b 2} [:a :missing]))");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn zipmap_pairs_keys_values() {
    let v = run("(get (zipmap [:a :b :c] [1 2 3]) :b)");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn zipmap_truncates_to_shorter() {
    let v = run("(count (zipmap [:a :b :c] [1 2]))");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn frequencies_counts() {
    let v = run("(get (frequencies [:a :b :a :a :c]) :a)");
    assert_eq!(v, Value::Int(3));
}

#[test]
fn group_by_keyfn() {
    // (group-by even? [1 2 3 4]) → {true [2 4] false [1 3]}.
    let v = run("(count (get (group-by even? [1 2 3 4]) true))");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn set_uniq_membership() {
    assert_eq!(run("(count (set [1 1 2 2 3]))"), Value::Int(3));
}

#[test]
fn disj_removes_member() {
    assert_eq!(run("(count (disj #{1 2 3} 2))"), Value::Int(2));
}

#[test]
fn set_call_as_predicate() {
    // (#{:a :b} :a) → :a; (#{:a :b} :z) → nil.
    assert_eq!(run("(#{:a :b} :a)"), k("a"));
    assert_eq!(run("(#{:a :b} :z)"), Value::Nil);
}

#[test]
fn keyword_invocation_on_map() {
    // (:a {:a 1}) → 1; (:missing {:a 1} :default) → :default.
    assert_eq!(run("(:a {:a 1})"), Value::Int(1));
    assert_eq!(run("(:missing {:a 1} :default)"), k("default"));
}

#[test]
fn map_invocation_lookup() {
    // ({:a 1} :a) → 1; ({:a 1} :z :d) → :d.
    assert_eq!(run("({:a 1} :a)"), Value::Int(1));
    assert_eq!(run("({:a 1} :z :d)"), k("d"));
}

// =====================================================================
// peek/pop on vector vs list — different ends!
// =====================================================================

#[test]
fn peek_vector_returns_last() {
    assert_eq!(run("(peek [1 2 3])"), Value::Int(3));
}

#[test]
fn peek_list_returns_first() {
    assert_eq!(run("(peek '(1 2 3))"), Value::Int(1));
}

#[test]
fn pop_vector_drops_last() {
    let v = run("(pop [1 2 3])");
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([Value::Int(1), Value::Int(2)]))
    );
}

#[test]
fn pop_list_drops_first() {
    let v = run("(count (pop '(1 2 3)))");
    assert_eq!(v, Value::Int(2));
    let h = run("(first (pop '(1 2 3)))");
    assert_eq!(h, Value::Int(2));
}

#[test]
fn peek_empty_returns_nil() {
    assert_eq!(run("(peek [])"), Value::Nil);
    assert_eq!(run("(peek nil)"), Value::Nil);
}

#[test]
fn pop_empty_errors() {
    // Clojure throws IllegalStateException on empty pop.
    assert!(run_err("(pop [])"));
}

// =====================================================================
// partition / partition-all / partition-by
// =====================================================================

#[test]
fn partition_basic() {
    // (partition 2 [1 2 3 4]) → ((1 2) (3 4)).
    let v = run("(count (partition 2 [1 2 3 4]))");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn partition_drops_incomplete_tail() {
    // (partition 2 [1 2 3]) → ((1 2)).
    let v = run("(count (partition 2 [1 2 3]))");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn partition_with_pad() {
    // (partition 3 2 [:p] [1 2 3 4]) → ((1 2 3) (3 4 :p)).
    let v = run("(count (partition 3 2 [:p] [1 2 3 4]))");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn partition_step_diff_size() {
    // (partition 2 1 [1 2 3 4]) → ((1 2) (2 3) (3 4)).
    let v = run("(count (partition 2 1 [1 2 3 4]))");
    assert_eq!(v, Value::Int(3));
}

#[test]
fn partition_all_keeps_tail() {
    let v = run("(count (partition-all 2 [1 2 3]))");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn partition_all_empty_input() {
    assert_eq!(run("(count (partition-all 2 []))"), Value::Int(0));
}

#[test]
fn partition_by_groups_consecutive() {
    // (partition-by odd? [1 1 2 2 3 3]) → ((1 1) (2 2) (3 3)).
    let v = run("(count (partition-by odd? [1 1 2 2 3 3]))");
    assert_eq!(v, Value::Int(3));
}

#[test]
fn partition_by_empty() {
    assert_eq!(run("(count (partition-by odd? []))"), Value::Int(0));
}

// =====================================================================
// take / drop / take-while / drop-while / take-nth
// =====================================================================

#[test]
fn take_more_than_available() {
    assert_eq!(run("(count (take 100 [1 2 3]))"), Value::Int(3));
}

#[test]
fn take_zero_empty() {
    assert_eq!(run("(count (take 0 [1 2 3]))"), Value::Int(0));
}

#[test]
fn take_negative_empty() {
    // (take -3 ...) → ().
    assert_eq!(run("(count (take -3 [1 2 3]))"), Value::Int(0));
}

#[test]
fn drop_more_than_available_empty() {
    assert_eq!(run("(count (drop 100 [1 2 3]))"), Value::Int(0));
}

#[test]
fn drop_negative_no_op() {
    assert_eq!(run("(count (drop -2 [1 2 3]))"), Value::Int(3));
}

#[test]
fn take_while_stops_at_first_false() {
    assert_eq!(run("(count (take-while pos? [1 2 -1 3 4]))"), Value::Int(2));
}

#[test]
fn drop_while_drops_initial() {
    assert_eq!(run("(count (drop-while pos? [1 2 -1 3 4]))"), Value::Int(3));
}

#[test]
fn take_nth_every_other() {
    // (take-nth 2 [0 1 2 3 4]) → (0 2 4).
    let v = run("(count (take-nth 2 [0 1 2 3 4]))");
    assert_eq!(v, Value::Int(3));
}

#[test]
fn take_last_returns_tail() {
    let v = run("(count (take-last 2 [1 2 3 4]))");
    assert_eq!(v, Value::Int(2));
    let last = run("(last (take-last 2 [1 2 3 4]))");
    assert_eq!(last, Value::Int(4));
}

#[test]
fn drop_last_returns_butlast() {
    let v = run("(count (drop-last [1 2 3 4]))");
    assert_eq!(v, Value::Int(3));
}

// =====================================================================
// range / repeat / iterate / cycle / repeatedly
// =====================================================================

#[test]
fn range_zero_arity_takes_5() {
    // (take 5 (range)) → (0 1 2 3 4).
    let v = run("(reduce + (take 5 (range)))");
    assert_eq!(v, Value::Int(10));
}

#[test]
fn range_one_arity_exclusive() {
    let v = run("(count (range 5))");
    assert_eq!(v, Value::Int(5));
}

#[test]
fn range_two_arity_start_end() {
    let v = run("(reduce + (range 1 5))");
    assert_eq!(v, Value::Int(10)); // 1+2+3+4
}

#[test]
fn range_three_arity_step() {
    let v = run("(count (range 0 10 2))");
    assert_eq!(v, Value::Int(5));
}

#[test]
fn range_descending_step() {
    let v = run("(count (range 10 0 -2))");
    assert_eq!(v, Value::Int(5));
}

#[test]
fn range_empty_when_start_geq_end() {
    assert_eq!(run("(count (range 5 5))"), Value::Int(0));
    assert_eq!(run("(count (range 5 0))"), Value::Int(0));
}

#[test]
fn iterate_take_5() {
    let v = run("(reduce + (take 5 (iterate inc 0)))");
    assert_eq!(v, Value::Int(10));
}

#[test]
fn cycle_take_wraps() {
    let v = run("(count (take 7 (cycle [1 2 3])))");
    assert_eq!(v, Value::Int(7));
}

#[test]
fn repeat_with_count() {
    let v = run("(count (repeat 5 :x))");
    assert_eq!(v, Value::Int(5));
}

#[test]
fn repeat_infinite_take() {
    let v = run("(count (take 7 (repeat :x)))");
    assert_eq!(v, Value::Int(7));
}

// =====================================================================
// reduce-kv / reduced / reductions
// =====================================================================

#[test]
fn reduce_kv_visits_all_pairs() {
    let v = run("(reduce-kv (fn [acc k v] (+ acc v)) 0 {:a 1 :b 2 :c 3})");
    assert_eq!(v, Value::Int(6));
}

#[test]
fn reduce_short_circuits_on_reduced() {
    // (reduce (fn [_ x] (if (= x 3) (reduced :stop) x)) 0 [1 2 3 4 5]).
    let v = run("(reduce (fn [_ x] (if (= x 3) (reduced :stop) x)) 0 [1 2 3 4 5])");
    assert_eq!(v, k("stop"));
}

#[test]
fn reductions_accumulates() {
    // (reductions + [1 2 3 4]) → (1 3 6 10).
    let v = run("(last (reductions + [1 2 3 4]))");
    assert_eq!(v, Value::Int(10));
    assert_eq!(run("(count (reductions + [1 2 3 4]))"), Value::Int(4));
}

#[test]
fn reductions_with_init() {
    // (reductions + 100 [1 2 3]) → (100 101 103 106).
    assert_eq!(run("(count (reductions + 100 [1 2 3]))"), Value::Int(4));
    assert_eq!(run("(last (reductions + 100 [1 2 3]))"), Value::Int(106));
}

// =====================================================================
// bit-* ops
// =====================================================================

#[test]
fn bit_and_basic() {
    assert_eq!(run("(bit-and 12 10)"), Value::Int(8));
}

#[test]
fn bit_or_basic() {
    assert_eq!(run("(bit-or 12 10)"), Value::Int(14));
}

#[test]
fn bit_xor_basic() {
    assert_eq!(run("(bit-xor 12 10)"), Value::Int(6));
}

#[test]
fn bit_not_basic() {
    // (bit-not 0) → -1.
    assert_eq!(run("(bit-not 0)"), Value::Int(-1));
    assert_eq!(run("(bit-not -1)"), Value::Int(0));
}

#[test]
fn bit_shift_left_right() {
    assert_eq!(run("(bit-shift-left 1 4)"), Value::Int(16));
    assert_eq!(run("(bit-shift-right 32 2)"), Value::Int(8));
}

#[test]
fn bit_test_set_clear_flip() {
    assert_eq!(run("(bit-test 4 2)"), Value::Bool(true));
    assert_eq!(run("(bit-test 4 1)"), Value::Bool(false));
    assert_eq!(run("(bit-set 0 3)"), Value::Int(8));
    assert_eq!(run("(bit-clear 8 3)"), Value::Int(0));
    assert_eq!(run("(bit-flip 0 2)"), Value::Int(4));
    assert_eq!(run("(bit-flip 4 2)"), Value::Int(0));
}

#[test]
fn bit_and_not_basic() {
    // (bit-and-not 12 10) → 12 & ~10 = 1100 & 0101 = 0100 = 4.
    assert_eq!(run("(bit-and-not 12 10)"), Value::Int(4));
}

#[test]
fn unsigned_bit_shift_right_basic() {
    // unsigned shift fills with zero.
    assert_eq!(run("(unsigned-bit-shift-right -1 60)"), Value::Int(15));
}

// =====================================================================
// parse-* — bad inputs return nil, not throw
// =====================================================================

#[test]
fn parse_long_valid() {
    assert_eq!(run("(parse-long \"42\")"), Value::Int(42));
    assert_eq!(run("(parse-long \"-7\")"), Value::Int(-7));
}

#[test]
fn parse_long_invalid_returns_nil() {
    assert_eq!(run("(parse-long \"abc\")"), Value::Nil);
    assert_eq!(run("(parse-long \"\")"), Value::Nil);
    assert_eq!(run("(parse-long \"3.14\")"), Value::Nil);
}

#[test]
fn parse_double_valid() {
    assert_eq!(run("(parse-double \"3.14\")"), Value::Float(3.14));
    assert_eq!(run("(parse-double \"42\")"), Value::Float(42.0));
}

#[test]
fn parse_double_invalid_returns_nil() {
    assert_eq!(run("(parse-double \"abc\")"), Value::Nil);
    assert_eq!(run("(parse-double \"\")"), Value::Nil);
}

#[test]
fn parse_boolean_valid() {
    assert_eq!(run("(parse-boolean \"true\")"), Value::Bool(true));
    assert_eq!(run("(parse-boolean \"false\")"), Value::Bool(false));
}

#[test]
fn parse_boolean_invalid_returns_nil() {
    // Clojure: only "true"/"false" exact match returns boolean; else nil.
    assert_eq!(run("(parse-boolean \"True\")"), Value::Nil);
    assert_eq!(run("(parse-boolean \"yes\")"), Value::Nil);
    assert_eq!(run("(parse-boolean \"\")"), Value::Nil);
}

// =====================================================================
// random-uuid
// =====================================================================

#[test]
fn random_uuid_valid_v4_format() {
    // UUID v4 string: 8-4-4-4-12 hex, version digit '4'.
    let v = run("(random-uuid)");
    let s = match v {
        Value::Str(s) => s.to_string(),
        other => panic!("expected string uuid, got {other:?}"),
    };
    assert_eq!(s.len(), 36, "uuid string length");
    let parts: Vec<&str> = s.split('-').collect();
    assert_eq!(parts.len(), 5);
    assert_eq!(parts[0].len(), 8);
    assert_eq!(parts[1].len(), 4);
    assert_eq!(parts[2].len(), 4);
    assert_eq!(parts[3].len(), 4);
    assert_eq!(parts[4].len(), 12);
    // Version nibble.
    assert!(parts[2].starts_with('4'), "v4 marker missing in {s}");
}

#[test]
fn random_uuid_distinct_per_call() {
    let a = run("(random-uuid)");
    let b = run("(random-uuid)");
    assert_ne!(a, b);
}

// =====================================================================
// delay / force / realized?
// =====================================================================

#[test]
fn delay_only_evaluates_once() {
    // Use an atom counter to ensure thunk runs once.
    let v = run(
        "(let [c (atom 0)
               d (delay (do (swap! c inc) :v))]
           (force d) (force d) (force d)
           @c)",
    );
    assert_eq!(v, Value::Int(1));
}

#[test]
fn delay_realized_flips() {
    let v = run("(let [d (delay 1)] [(realized? d) (do (force d) (realized? d))])");
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([
            Value::Bool(false),
            Value::Bool(true),
        ]))
    );
}

#[test]
fn delay_q_recognizes_delay() {
    assert_eq!(run("(delay? (delay 1))"), Value::Bool(true));
    assert_eq!(run("(delay? 1)"), Value::Bool(false));
}

// =====================================================================
// atom / swap! / reset! / compare-and-set!
// =====================================================================

#[test]
fn swap_returns_new_value() {
    let v = run("(let [a (atom 0)] (swap! a inc))");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn swap_applies_extra_args() {
    let v = run("(let [a (atom 1)] (swap! a + 2 3) @a)");
    assert_eq!(v, Value::Int(6));
}

#[test]
fn compare_and_set_succeeds_only_on_match() {
    let v = run("(let [a (atom 5)] [(compare-and-set! a 5 6) (compare-and-set! a 5 7) @a])");
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([
            Value::Bool(true),
            Value::Bool(false),
            Value::Int(6),
        ]))
    );
}

#[test]
fn reset_overwrites() {
    assert_eq!(run("(let [a (atom 1)] (reset! a 99) @a)"), Value::Int(99));
}

// =====================================================================
// Higher-order on funky inputs
// =====================================================================

#[test]
fn map_two_colls_zips() {
    // (map + [1 2 3] [10 20 30]) → (11 22 33).
    let v = run("(reduce + (map + [1 2 3] [10 20 30]))");
    assert_eq!(v, Value::Int(66));
}

#[test]
fn map_uneven_colls_truncates() {
    let v = run("(count (map + [1 2 3] [10 20]))");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn map_three_colls() {
    let v = run("(reduce + (map + [1 2] [10 20] [100 200]))");
    assert_eq!(v, Value::Int(333));
}

#[test]
fn keep_drops_nil() {
    // (keep #(if (odd? %) % nil) [1 2 3 4 5]) → (1 3 5).
    let v = run("(count (keep (fn [x] (if (odd? x) x nil)) [1 2 3 4 5]))");
    assert_eq!(v, Value::Int(3));
}

#[test]
fn keep_keeps_false() {
    // Clojure: `keep` only drops nil, not false.
    let v = run("(count (keep (fn [x] (if (= x 2) false x)) [1 2 3]))");
    assert_eq!(v, Value::Int(3));
}

#[test]
fn keep_indexed_index_passed() {
    // (keep-indexed (fn [i x] (when (odd? i) x)) [:a :b :c :d]) → (:b :d).
    let v = run("(count (keep-indexed (fn [i x] (when (odd? i) x)) [:a :b :c :d]))");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn map_indexed_pairs() {
    let v = run("(first (map-indexed (fn [i x] [i x]) [:a :b]))");
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([Value::Int(0), k("a"),]))
    );
}

#[test]
fn mapcat_flattens_one_level() {
    let v = run("(count (mapcat (fn [x] [x x]) [1 2 3]))");
    assert_eq!(v, Value::Int(6));
}

// =====================================================================
// tree-seq
// =====================================================================

#[test]
fn tree_seq_counts_all_nodes() {
    // (tree-seq seq? rest '(1 (2 (3 4)))) — Clojure example.
    let v = run("(count (tree-seq vector? rest [1 [2 [3 4]] 5]))");
    // The tree has 3 internal nodes (the vectors) + leaves; tree-seq
    // visits internal + leaves. Just ensure non-zero & matches expected.
    // Outer [1 [2 [3 4]] 5] is the root (a branch), [2 [3 4]] is branch,
    // [3 4] is branch, plus leaves 1, 2, 3, 4, 5. Total = 8.
    assert_eq!(v, Value::Int(8));
}

#[test]
fn tree_seq_root_only_for_leaf() {
    let v = run("(count (tree-seq vector? rest 42))");
    assert_eq!(v, Value::Int(1));
}

// =====================================================================
// memoize
// =====================================================================

#[test]
fn memoize_caches_call() {
    let v = run(
        "(let [c (atom 0)
               f (memoize (fn [x] (swap! c inc) (* x 2)))]
           (f 5) (f 5) (f 5)
           @c)",
    );
    assert_eq!(v, Value::Int(1));
}

#[test]
fn memoize_distinct_args_separate_cache() {
    let v = run(
        "(let [c (atom 0)
               f (memoize (fn [x] (swap! c inc) x))]
           (f 1) (f 2) (f 1)
           @c)",
    );
    assert_eq!(v, Value::Int(2));
}

// =====================================================================
// case / cond
// =====================================================================

#[test]
fn case_keyword_match() {
    let v = run("(case :b :a 1 :b 2 :c 3 :default)");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn case_default_branch() {
    let v = run("(case :z :a 1 :b 2 :default)");
    assert_eq!(v, k("default"));
}

#[test]
fn case_no_default_throws() {
    assert!(run_err("(case :z :a 1 :b 2)"));
}

#[test]
fn cond_no_match_returns_nil() {
    assert_eq!(run("(cond false :a false :b)"), Value::Nil);
}

#[test]
fn cond_else_branch() {
    assert_eq!(run("(cond false :a :else :z)"), k("z"));
}

// =====================================================================
// Misc: hash, compare, sort
// =====================================================================

#[test]
fn sort_default_ascending() {
    let v = run("(vec (sort [3 1 2]))");
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
        ]))
    );
}

#[test]
fn sort_with_comparator_descending() {
    let v = run("(vec (sort > [1 3 2]))");
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(3),
            Value::Int(2),
            Value::Int(1),
        ]))
    );
}

#[test]
fn sort_by_keyfn() {
    let v = run("(vec (sort-by :a [{:a 3} {:a 1} {:a 2}]))");
    assert_eq!(
        run("(:a (first (sort-by :a [{:a 3} {:a 1} {:a 2}])))"),
        Value::Int(1)
    );
    let _ = v;
}

#[test]
fn hash_equal_values_same() {
    // hash should be consistent: equal values → same hash.
    assert_eq!(run("(= (hash 1) (hash 1))"), Value::Bool(true));
}

#[test]
fn compare_int_basic() {
    // compare returns negative / zero / positive.
    assert_eq!(run("(compare 1 1)"), Value::Int(0));
    assert!(matches!(run("(compare 1 2)"), Value::Int(n) if n < 0));
    assert!(matches!(run("(compare 2 1)"), Value::Int(n) if n > 0));
}

// =====================================================================
// distinct / dedupe / interleave / interpose
// =====================================================================

#[test]
fn distinct_removes_dupes_keeps_order() {
    assert_eq!(
        run("(vec (distinct [1 2 1 3 2 4]))"),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
            Value::Int(4),
        ]))
    );
}

#[test]
fn dedupe_only_removes_consecutive_dupes() {
    assert_eq!(run("(count (dedupe [1 1 2 2 1 3]))"), Value::Int(4));
}

#[test]
fn interleave_truncates() {
    assert_eq!(
        run("(vec (interleave [1 2 3] [:a :b]))"),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(1),
            k("a"),
            Value::Int(2),
            k("b"),
        ]))
    );
}

#[test]
fn interpose_inserts_between() {
    assert_eq!(run("(count (interpose 0 [1 2 3]))"), Value::Int(5));
}

#[test]
fn interpose_empty_unchanged() {
    assert_eq!(run("(count (interpose 0 []))"), Value::Int(0));
}

// =====================================================================
// flatten / concat / list*
// =====================================================================

#[test]
fn flatten_one_level_and_more() {
    assert_eq!(run("(count (flatten [1 [2 [3 [4]]]]))"), Value::Int(4));
}

#[test]
fn flatten_no_seq_passes_through() {
    // (flatten 1) → ()? Clojure: anything not seqable → ().
    assert_eq!(run("(count (flatten 1))"), Value::Int(0));
}

#[test]
fn concat_zero_args_empty() {
    assert_eq!(run("(count (concat))"), Value::Int(0));
}

#[test]
fn concat_nil_arg() {
    assert_eq!(run("(count (concat nil [1 2] nil [3]))"), Value::Int(3));
}

#[test]
fn list_star_constructs() {
    // (list* 1 2 [3 4]) → (1 2 3 4).
    assert_eq!(run("(count (list* 1 2 [3 4]))"), Value::Int(4));
}

// =====================================================================
// Variadic-fn / apply / fn-arity edge cases
// =====================================================================

#[test]
fn apply_with_single_seq_arg() {
    assert_eq!(run("(apply + [1 2 3 4])"), Value::Int(10));
}

#[test]
fn apply_with_intermediate_args() {
    assert_eq!(run("(apply + 1 2 [3 4])"), Value::Int(10));
}

#[test]
fn apply_to_keyword_lookup() {
    assert_eq!(run("(apply :a [{:a 99}])"), Value::Int(99));
}

#[test]
fn variadic_fn_no_extras() {
    let v = run("((fn [& xs] (count xs)))");
    assert_eq!(v, Value::Int(0));
}

#[test]
fn variadic_fn_with_extras() {
    let v = run("((fn [a & xs] (+ a (count xs))) 10 1 2 3)");
    assert_eq!(v, Value::Int(13));
}

// =====================================================================
// pr-str / prn-str / str rounding
// =====================================================================

#[test]
fn pr_str_quotes_strings() {
    assert_eq!(run("(pr-str \"a\")"), s("\"a\""));
}

#[test]
fn pr_str_nil_keyword() {
    assert_eq!(run("(pr-str nil)"), s("nil"));
    assert_eq!(run("(pr-str :foo)"), s(":foo"));
}

#[test]
fn pr_str_collection_round_trip() {
    // (read-string (pr-str x)) ~= x for primitives.
    assert_eq!(run("(read-string (pr-str 42))"), Value::Int(42));
    assert_eq!(run("(read-string (pr-str :a))"), k("a"));
    assert_eq!(run("(read-string (pr-str true))"), Value::Bool(true));
}

#[test]
fn prn_str_appends_newline() {
    let v = run("(prn-str :a)");
    let st = match v {
        Value::Str(s) => s.to_string(),
        _ => panic!(),
    };
    assert!(st.ends_with('\n'));
}

// =====================================================================
// loop / recur
// =====================================================================

#[test]
fn loop_recur_counts_to_n() {
    let v = run("(loop [i 0 acc 0] (if (= i 10) acc (recur (inc i) (+ acc i))))");
    assert_eq!(v, Value::Int(45));
}

#[test]
fn recur_in_fn_body() {
    let v = run("((fn f [n] (if (zero? n) :done (recur (dec n)))) 100)");
    assert_eq!(v, k("done"));
}

// =====================================================================
// Type / ifn? / instance? — sanity
// =====================================================================

#[test]
fn type_returns_keyword_or_string() {
    // Clojure returns a Class. cljrs likely returns a keyword/string.
    let v = run("(type 1)");
    assert!(matches!(v, Value::Keyword(_) | Value::Str(_) | Value::Symbol(_)));
}

// =====================================================================
// Numerics — quot / mod / rem semantics for negatives
// =====================================================================

#[test]
fn mod_negative_clojure_sign() {
    // Clojure: (mod -5 3) → 1 (sign of divisor).
    assert_eq!(run("(mod -5 3)"), Value::Int(1));
    assert_eq!(run("(mod 5 -3)"), Value::Int(-1));
}

#[test]
fn rem_negative_truncated() {
    // Clojure: (rem -5 3) → -2 (sign of dividend).
    assert_eq!(run("(rem -5 3)"), Value::Int(-2));
}

#[test]
fn quot_truncates_toward_zero() {
    assert_eq!(run("(quot 7 2)"), Value::Int(3));
    assert_eq!(run("(quot -7 2)"), Value::Int(-3));
}

#[test]
fn min_max_variadic() {
    assert_eq!(run("(min 3 1 2)"), Value::Int(1));
    assert_eq!(run("(max 3 1 2)"), Value::Int(3));
    assert_eq!(run("(min 5)"), Value::Int(5));
}

#[test]
fn min_max_mixed_int_float() {
    assert_eq!(run("(max 1 2.0 3)"), Value::Int(3));
}

#[test]
fn abs_handles_negatives() {
    assert_eq!(run("(abs -5)"), Value::Int(5));
    assert_eq!(run("(abs 5)"), Value::Int(5));
    assert_eq!(run("(abs -3.14)"), Value::Float(3.14));
}

#[test]
fn nan_q_only_nan() {
    // (NaN? (Math/sqrt -1)) → true. cljrs may not have Math/sqrt, use directly.
    assert_eq!(run("(NaN? (/ 0.0 0.0))"), Value::Bool(true));
    assert_eq!(run("(NaN? 1.0)"), Value::Bool(false));
}

#[test]
fn infinite_q_detects_inf() {
    assert_eq!(run("(infinite? (/ 1.0 0.0))"), Value::Bool(true));
    assert_eq!(run("(infinite? 1.0)"), Value::Bool(false));
}

// =====================================================================
// Exception flow
// =====================================================================

#[test]
fn try_catch_caught() {
    let v = run("(try (throw (ex-info \"oops\" {:k 1})) (catch :default e (ex-message e)))");
    assert_eq!(v, s("oops"));
}

#[test]
fn try_finally_runs() {
    // finally executes even on success; result is body result, not finally.
    let v = run("(let [a (atom 0)] (try :ok (finally (reset! a 1))) @a)");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn ex_data_recovers_map() {
    let v = run("(try (throw (ex-info \"x\" {:k 1})) (catch :default e (:k (ex-data e))))");
    assert_eq!(v, Value::Int(1));
}

// =====================================================================
// Defrecord (in core.clj)
// =====================================================================

#[test]
fn defrecord_constructor_and_predicate() {
    let v = run(
        "(defrecord Point [x y])
         (let [p (->Point 1 2)]
           [(:x p) (:y p)])",
    );
    assert_eq!(
        v,
        Value::Vector(imbl::Vector::from_iter([Value::Int(1), Value::Int(2)]))
    );
}

#[test]
fn defrecord_equality_by_fields() {
    let v = run(
        "(defrecord P [x])
         (= (->P 1) (->P 1))",
    );
    assert_eq!(v, Value::Bool(true));
}

// =====================================================================
// gensym uniqueness
// =====================================================================

#[test]
fn gensym_unique_per_call() {
    assert_eq!(run("(= (gensym) (gensym))"), Value::Bool(false));
}

#[test]
fn gensym_with_prefix() {
    let v = run("(gensym \"foo_\")");
    let s = match v {
        Value::Symbol(s) => s.to_string(),
        other => panic!("expected symbol, got {other:?}"),
    };
    assert!(s.starts_with("foo_"));
}

// =====================================================================
// Misc small fns
// =====================================================================

#[test]
fn nth_in_range() {
    assert_eq!(run("(nth [10 20 30] 1)"), Value::Int(20));
}

#[test]
fn nth_out_of_range_default() {
    assert_eq!(run("(nth [1 2] 5 :missing)"), k("missing"));
}

#[test]
fn nth_out_of_range_no_default_errors() {
    assert!(run_err("(nth [1 2] 5)"));
}

#[test]
fn second_basic() {
    assert_eq!(run("(second [1 2 3])"), Value::Int(2));
    assert_eq!(run("(second [1])"), Value::Nil);
}

#[test]
fn last_basic() {
    assert_eq!(run("(last [1 2 3])"), Value::Int(3));
    assert_eq!(run("(last nil)"), Value::Nil);
}

#[test]
fn ffirst_fnext_nfirst_nnext() {
    // (ffirst [[1 2] [3 4]]) → 1; (fnext ...) → [3 4]; (nnext ...) → ().
    assert_eq!(run("(ffirst [[1 2] [3 4]])"), Value::Int(1));
    assert_eq!(run("(first (fnext [1 2 3]))"), Value::Nil);
    // nfirst — rest of (first ...).
    assert_eq!(run("(count (nfirst [[1 2 3] [4 5]]))"), Value::Int(2));
    assert_eq!(run("(count (nnext [1 2 3 4]))"), Value::Int(2));
}

#[test]
fn not_basic() {
    assert_eq!(run("(not nil)"), Value::Bool(true));
    assert_eq!(run("(not false)"), Value::Bool(true));
    assert_eq!(run("(not 0)"), Value::Bool(false));
    assert_eq!(run("(not \"\")"), Value::Bool(false));
}

#[test]
fn not_eq_basic() {
    assert_eq!(run("(not= 1 2)"), Value::Bool(true));
    assert_eq!(run("(not= 1 1)"), Value::Bool(false));
}

#[test]
fn keyword_constructor_namespace() {
    assert_eq!(run("(keyword \"foo\")"), k("foo"));
    assert_eq!(run("(keyword \"ns\" \"name\")"), k("ns/name"));
}

#[test]
fn symbol_constructor() {
    let v = run("(symbol \"foo\")");
    assert_eq!(v, Value::Symbol(Arc::from("foo")));
}

#[test]
fn name_on_keyword_and_symbol() {
    assert_eq!(run("(name :foo)"), s("foo"));
    assert_eq!(run("(name :ns/foo)"), s("foo"));
    assert_eq!(run("(name \"plain\")"), s("plain"));
}

#[test]
fn namespace_on_qualified_keyword() {
    assert_eq!(run("(namespace :ns/foo)"), s("ns"));
    assert_eq!(run("(namespace :foo)"), Value::Nil);
}

#[test]
fn vec_from_collection() {
    assert_eq!(
        run("(vec '(1 2 3))"),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
        ]))
    );
    assert_eq!(run("(vec nil)"), Value::Vector(imbl::Vector::new()));
}

#[test]
fn into_vec_from_seq() {
    assert_eq!(
        run("(into [10] [20 30])"),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(10),
            Value::Int(20),
            Value::Int(30),
        ]))
    );
}

#[test]
fn into_map_from_pairs() {
    let v = run("(get (into {} [[:a 1] [:b 2]]) :b)");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn shuffle_preserves_count() {
    assert_eq!(run("(count (shuffle [1 2 3 4 5]))"), Value::Int(5));
}

#[test]
fn rand_int_in_range() {
    // (rand-int 10) ∈ [0, 10).
    let v = run("(rand-int 10)");
    if let Value::Int(n) = v {
        assert!((0..10).contains(&n));
    } else {
        panic!("expected int, got {v:?}");
    }
}

#[test]
fn rand_nth_picks_member() {
    let v = run("(rand-nth [42])");
    assert_eq!(v, Value::Int(42));
}

#[test]
fn replace_via_map() {
    // (replace {1 :one 2 :two} [1 2 3]) → [:one :two 3].
    assert_eq!(
        run("(replace {1 :one 2 :two} [1 2 3])"),
        Value::Vector(imbl::Vector::from_iter([k("one"), k("two"), Value::Int(3)]))
    );
}

#[test]
fn update_keys_transforms_keys() {
    let v = run("(get (update-keys {:a 1 :b 2} name) \"a\")");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn update_vals_transforms_vals() {
    let v = run("(get (update-vals {:a 1 :b 2} inc) :a)");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn comparator_from_predicate() {
    // ((comparator <) 1 2) → -1.
    let v = run("((comparator <) 1 2)");
    assert!(matches!(v, Value::Int(n) if n < 0));
}

#[test]
fn min_key_max_key_basic() {
    assert_eq!(run("(min-key count [1] [1 2 3] [1 2])"), Value::Vector(imbl::Vector::from_iter([Value::Int(1)])));
    let v = run("(max-key count [1] [1 2 3] [1 2])");
    assert_eq!(v, Value::Vector(imbl::Vector::from_iter([Value::Int(1), Value::Int(2), Value::Int(3)])));
}

#[test]
fn time_ms_returns_number() {
    let v = run("(time-ms)");
    assert!(matches!(v, Value::Int(_) | Value::Float(_)));
}

#[test]
fn sequence_collects_seq() {
    assert_eq!(run("(count (sequence [1 2 3]))"), Value::Int(3));
}

#[test]
fn split_at_returns_pair() {
    // (split-at 2 [1 2 3 4]) → [(1 2) (3 4)].
    assert_eq!(run("(count (split-at 2 [1 2 3 4]))"), Value::Int(2));
    assert_eq!(run("(count (first (split-at 2 [1 2 3 4])))"), Value::Int(2));
    assert_eq!(run("(count (second (split-at 2 [1 2 3 4])))"), Value::Int(2));
}

#[test]
fn split_with_predicate() {
    let v = run("(count (first (split-with pos? [1 2 -1 3])))");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn rseq_reverses_vec() {
    assert_eq!(
        run("(vec (rseq [1 2 3]))"),
        Value::Vector(imbl::Vector::from_iter([
            Value::Int(3),
            Value::Int(2),
            Value::Int(1),
        ]))
    );
}

#[test]
fn empty_vec_returns_empty_vec() {
    assert_eq!(run("(empty [1 2 3])"), Value::Vector(imbl::Vector::new()));
    assert_eq!(run("(count (empty {:a 1}))"), Value::Int(0));
    assert_eq!(run("(count (empty #{1 2}))"), Value::Int(0));
}

#[test]
fn not_empty_returns_nil_for_empty() {
    assert_eq!(run("(not-empty [])"), Value::Nil);
    assert_eq!(run("(not-empty [1])"), Value::Vector(imbl::Vector::from_iter([Value::Int(1)])));
}

#[test]
fn key_val_on_map_entry() {
    // (key (first {:a 1})) → :a.
    assert_eq!(run("(key (first {:a 1}))"), k("a"));
    assert_eq!(run("(val (first {:a 1}))"), Value::Int(1));
}

#[test]
fn _suppress_unused_pr() {
    // Keep `pr` helper referenced in case future tests want it.
    let _ = pr;
    let _ = s as fn(&str) -> Value;
}
