//! Smoke tests for the newly-ported clojure.core fns.
//! Each block exercises one cluster (numeric, predicates, collections,
//! sequences, functional, misc) at the value level — we trust the
//! existing tests for prior coverage and only verify the new surface.

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

// ---- numeric / bitwise ---------------------------------------------------

#[test]
fn num_eq_coerces_across_int_float() {
    assert_eq!(run("(== 1 1.0)"), Value::Bool(true));
    assert_eq!(run("(== 1 2)"), Value::Bool(false));
    assert_eq!(run("(== 1 1 1.0)"), Value::Bool(true));
}

#[test]
fn bit_ops() {
    assert_eq!(run("(bit-and 240 15)"), Value::Int(0));
    assert_eq!(run("(bit-or 240 15)"), Value::Int(255));
    assert_eq!(run("(bit-xor 255 15)"), Value::Int(240));
    assert_eq!(run("(bit-not 0)"), Value::Int(-1));
    assert_eq!(run("(bit-shift-left 1 4)"), Value::Int(16));
    assert_eq!(run("(bit-shift-right 16 4)"), Value::Int(1));
    assert_eq!(run("(unsigned-bit-shift-right -1 1)"), Value::Int(i64::MAX));
    assert_eq!(run("(bit-set 0 3)"), Value::Int(8));
    assert_eq!(run("(bit-clear 255 0)"), Value::Int(254));
    assert_eq!(run("(bit-flip 0 0)"), Value::Int(1));
    assert_eq!(run("(bit-test 8 3)"), Value::Bool(true));
    assert_eq!(run("(bit-test 8 0)"), Value::Bool(false));
}

#[test]
fn parse_helpers() {
    assert_eq!(run("(parse-long \"42\")"), Value::Int(42));
    assert_eq!(run("(parse-long \"abc\")"), Value::Nil);
    assert_eq!(run("(parse-double \"3.14\")"), Value::Float(3.14));
    assert_eq!(run("(parse-boolean \"true\")"), Value::Bool(true));
    assert_eq!(run("(parse-boolean \"false\")"), Value::Bool(false));
    assert_eq!(run("(parse-boolean \"x\")"), Value::Nil);
}

#[test]
fn unchecked_arith_aliases() {
    assert_eq!(run("(unchecked-add 2 3)"), Value::Int(5));
    assert_eq!(run("(unchecked-multiply 6 7)"), Value::Int(42));
    assert_eq!(run("(unchecked-inc 5)"), Value::Int(6));
    assert_eq!(run("(unchecked-dec 5)"), Value::Int(4));
    assert_eq!(run("(unchecked-negate 5)"), Value::Int(-5));
}

#[test]
fn ratio_helpers() {
    assert_eq!(run("(numerator (/ 3 4))"), Value::Int(3));
    assert_eq!(run("(denominator (/ 3 4))"), Value::Int(4));
}

#[test]
fn nan_and_inf() {
    assert_eq!(run("(NaN? (/ 0.0 0.0))"), Value::Bool(true));
    assert_eq!(run("(infinite? (/ 1.0 0.0))"), Value::Bool(true));
    assert_eq!(run("(NaN? 1.0)"), Value::Bool(false));
}

#[test]
fn boolean_coercion() {
    assert_eq!(run("(boolean nil)"), Value::Bool(false));
    assert_eq!(run("(boolean false)"), Value::Bool(false));
    assert_eq!(run("(boolean 0)"), Value::Bool(true));
    assert_eq!(run("(boolean \"\")"), Value::Bool(true));
}

// ---- predicates ---------------------------------------------------------

#[test]
fn instance_q_uses_type_keyword() {
    assert_eq!(run("(instance? :string \"hi\")"), Value::Bool(true));
    assert_eq!(run("(instance? :vector [1])"), Value::Bool(true));
    assert_eq!(run("(instance? :map {:a 1})"), Value::Bool(true));
    assert_eq!(run("(instance? :string 1)"), Value::Bool(false));
}

#[test]
fn ratio_predicate() {
    assert_eq!(run("(ratio? (/ 1 3))"), Value::Bool(true));
    assert_eq!(run("(ratio? 1)"), Value::Bool(false));
}

#[test]
fn ident_predicates() {
    assert_eq!(run("(ident? :a)"), Value::Bool(true));
    assert_eq!(run("(ident? 'a)"), Value::Bool(true));
    assert_eq!(run("(ident? \"a\")"), Value::Bool(false));
    assert_eq!(run("(simple-keyword? :a)"), Value::Bool(true));
    assert_eq!(run("(qualified-keyword? :ns/a)"), Value::Bool(true));
    assert_eq!(run("(qualified-keyword? :a)"), Value::Bool(false));
}

#[test]
fn int_classification() {
    assert_eq!(run("(nat-int? 0)"), Value::Bool(true));
    assert_eq!(run("(nat-int? -1)"), Value::Bool(false));
    assert_eq!(run("(pos-int? 5)"), Value::Bool(true));
    assert_eq!(run("(neg-int? -3)"), Value::Bool(true));
}

#[test]
fn distinct_q_and_neg_preds() {
    assert_eq!(run("(distinct? 1 2 3)"), Value::Bool(true));
    assert_eq!(run("(distinct? 1 2 1)"), Value::Bool(false));
    assert_eq!(run("(not-any? neg? [1 2 3])"), Value::Bool(true));
    assert_eq!(run("(not-every? pos? [1 -2 3])"), Value::Bool(true));
}

// ---- collections --------------------------------------------------------

#[test]
fn peek_pop_vector_list() {
    assert_eq!(run("(peek [1 2 3])"), Value::Int(3));
    assert_eq!(run("(peek (list 1 2 3))"), Value::Int(1));
    assert_eq!(run("(count (pop [1 2 3]))"), Value::Int(2));
    assert_eq!(run("(first (pop (list 1 2 3)))"), Value::Int(2));
}

#[test]
fn disj_set() {
    assert_eq!(run("(count (disj #{1 2 3} 2))"), Value::Int(2));
    assert_eq!(run("(contains? (disj #{1 2 3} 2) 2)"), Value::Bool(false));
}

#[test]
fn replace_vector() {
    assert_eq!(
        run("(replace {1 :a 2 :b} [1 2 3])"),
        run("[:a :b 3]"),
    );
}

#[test]
fn merge_with_combines_values() {
    assert_eq!(
        run("(merge-with + {:a 1 :b 2} {:a 10 :c 3})"),
        run("{:a 11 :b 2 :c 3}"),
    );
}

#[test]
fn rseq_reverses_vector() {
    assert_eq!(run("(first (rseq [1 2 3]))"), Value::Int(3));
    assert_eq!(run("(rseq [])"), Value::Nil);
}

#[test]
fn empty_returns_empty_of_type() {
    assert_eq!(run("(count (empty [1 2 3]))"), Value::Int(0));
    assert_eq!(run("(count (empty {:a 1}))"), Value::Int(0));
    assert_eq!(run("(count (empty #{1 2}))"), Value::Int(0));
}

#[test]
fn key_val_on_entry() {
    assert_eq!(run("(key [:a 1])"), run(":a"));
    assert_eq!(run("(val [:a 1])"), Value::Int(1));
}

#[test]
fn namespace_extracts_prefix() {
    assert_eq!(run("(namespace :foo/bar)"), run("\"foo\""));
    assert_eq!(run("(namespace :bar)"), Value::Nil);
}

// ---- sequences ----------------------------------------------------------

#[test]
fn partition_by_groups_runs() {
    assert_eq!(
        run("(count (partition-by odd? [1 1 2 2 3]))"),
        Value::Int(3),
    );
}

#[test]
fn split_at_and_split_with() {
    assert_eq!(run("(count (first (split-at 2 [1 2 3 4])))"), Value::Int(2));
    assert_eq!(run("(count (first (split-with odd? [1 3 5 4 6])))"), Value::Int(3));
}

#[test]
fn take_drop_butlast_last_helpers() {
    assert_eq!(run("(first (take-last 2 [1 2 3 4]))"), Value::Int(3));
    assert_eq!(run("(count (drop-last 2 [1 2 3 4]))"), Value::Int(2));
    assert_eq!(run("(count (butlast [1 2 3]))"), Value::Int(2));
}

#[test]
fn tree_seq_walks_depth_first() {
    let src = "(count (tree-seq vector? seq [1 [2 [3 4]] 5]))";
    // root + 1 + sub + 2 + sub + 3 + 4 + 5 → 8 nodes
    assert_eq!(run(src), Value::Int(8));
}

#[test]
fn replicate_and_reductions() {
    assert_eq!(run("(count (replicate 5 :x))"), Value::Int(5));
    assert_eq!(run("(last (reductions + [1 2 3 4]))"), Value::Int(10));
    assert_eq!(run("(first (reductions + 100 [1 2]))"), Value::Int(100));
}

#[test]
fn list_star_splices_tail() {
    assert_eq!(run("(count (list* 1 2 [3 4 5]))"), Value::Int(5));
    assert_eq!(run("(first (list* 0 [1 2 3]))"), Value::Int(0));
}

#[test]
fn ffirst_fnext_etc() {
    assert_eq!(run("(ffirst [[1 2] [3 4]])"), Value::Int(1));
    assert_eq!(run("(fnext [1 2 3])"), Value::Int(2));
    assert_eq!(run("(next [1])"), Value::Nil);
    assert_eq!(run("(first (next [1 2]))"), Value::Int(2));
}

// ---- functional ---------------------------------------------------------

#[test]
fn every_pred_combines_predicates() {
    assert_eq!(run("((every-pred number? pos?) 5)"), Value::Bool(true));
    assert_eq!(run("((every-pred number? pos?) -5)"), Value::Bool(false));
    assert_eq!(run("((every-pred number? pos?) :x)"), Value::Bool(false));
}

#[test]
fn some_fn_returns_first_truthy() {
    // some-fn is currently shadowed by a cljrs prelude impl; just test
    // the truthy path. The fall-through value (false vs nil) is
    // implementation-defined for now.
    assert_eq!(run("((some-fn number? string?) \"hi\")"), Value::Bool(true));
}

#[test]
fn fnil_substitutes_default() {
    assert_eq!(run("((fnil + 0) nil 5)"), Value::Int(5));
    assert_eq!(run("((fnil + 0) 3 5)"), Value::Int(8));
}

#[test]
fn memoize_caches_results() {
    let src = "(let [n (atom 0)
                    f (memoize (fn [x] (swap! n inc) (* x x)))]
                (f 5) (f 5) (f 5)
                @n)";
    assert_eq!(run(src), Value::Int(1));
}

#[test]
fn comparator_from_pred() {
    assert_eq!(run("((comparator <) 1 2)"), Value::Int(-1));
    assert_eq!(run("((comparator <) 2 1)"), Value::Int(1));
    assert_eq!(run("((comparator <) 1 1)"), Value::Int(0));
}

#[test]
fn sort_by_with_keyfn() {
    let src = "(vec (sort-by count [\"aa\" \"a\" \"aaa\"]))";
    assert_eq!(run(src), run("[\"a\" \"aa\" \"aaa\"]"));
}

// ---- random / shuffle --------------------------------------------------

#[test]
fn shuffle_preserves_count() {
    assert_eq!(run("(count (shuffle [1 2 3 4 5]))"), Value::Int(5));
}

#[test]
fn rand_int_in_range() {
    let v = run("(rand-int 10)");
    if let Value::Int(i) = v {
        assert!(i >= 0 && i < 10);
    } else {
        panic!("rand-int returned non-int");
    }
}

#[test]
fn random_uuid_is_uuid_shape() {
    let v = run("(random-uuid)");
    if let Value::Str(s) = v {
        assert_eq!(s.len(), 36);
        assert_eq!(run(&format!("(uuid? \"{}\")", s)), Value::Bool(true));
    } else {
        panic!("random-uuid returned non-string");
    }
}

// ---- misc ---------------------------------------------------------------

#[test]
fn type_returns_keyword() {
    assert_eq!(run("(type 1)"), run(":int"));
    assert_eq!(run("(type \"x\")"), run(":string"));
}

#[test]
fn compare_basic() {
    assert_eq!(run("(compare 1 2)"), Value::Int(-1));
    assert_eq!(run("(compare 2 1)"), Value::Int(1));
    assert_eq!(run("(compare nil nil)"), Value::Int(0));
}

#[test]
fn transient_aliases_persistent() {
    assert_eq!(run("(persistent! (conj! (transient []) 1 2))"), run("[1 2]"));
}

#[test]
fn volatile_acts_like_atom() {
    let src = "(let [v (volatile! 0)] (vswap! v inc) (vswap! v inc) @v)";
    assert_eq!(run(src), Value::Int(2));
}

#[test]
fn meta_returns_nil_for_now() {
    // cljrs has no metadata; with-meta passes value through, meta is nil.
    assert_eq!(run("(meta {})"), Value::Nil);
    assert_eq!(run("(with-meta [1 2] {:tag :foo})"), run("[1 2]"));
}
