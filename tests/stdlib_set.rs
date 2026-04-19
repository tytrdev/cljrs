//! Bug-hunting suite for clojure.set — tests union/intersection/
//! difference/subset?/superset?/select/map-invert/rename-keys/
//! project/rename/index/join with edge cases.

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

// --- union --------------------------------------------------------------

#[test]
fn union_zero_args_empty() {
    assert_eq!(run("(clojure.set/union)"), Value::Set(imbl::HashSet::new()));
}

#[test]
fn union_one_arg_returns_set() {
    assert_eq!(run("(count (clojure.set/union #{1 2 3}))"), Value::Int(3));
}

#[test]
fn union_two_disjoint() {
    assert_eq!(run("(count (clojure.set/union #{1 2} #{3 4}))"), Value::Int(4));
}

#[test]
fn union_two_overlapping() {
    assert_eq!(run("(count (clojure.set/union #{1 2 3} #{2 3 4}))"), Value::Int(4));
}

#[test]
fn union_three_args() {
    assert_eq!(
        run("(count (clojure.set/union #{1} #{2} #{3 1}))"),
        Value::Int(3)
    );
}

#[test]
fn union_with_nil() {
    assert_eq!(run("(count (clojure.set/union nil #{1 2}))"), Value::Int(2));
    assert_eq!(run("(count (clojure.set/union #{1 2} nil))"), Value::Int(2));
}

// --- intersection -------------------------------------------------------

#[test]
fn intersection_two_overlap() {
    assert_eq!(
        run("(count (clojure.set/intersection #{1 2 3} #{2 3 4}))"),
        Value::Int(2)
    );
}

#[test]
fn intersection_no_overlap_empty() {
    assert_eq!(
        run("(count (clojure.set/intersection #{1 2} #{3 4}))"),
        Value::Int(0)
    );
}

#[test]
fn intersection_three_sets() {
    assert_eq!(
        run("(count (clojure.set/intersection #{1 2 3} #{2 3 4} #{3 4 5}))"),
        Value::Int(1)
    );
}

#[test]
fn intersection_with_nil_empty() {
    assert_eq!(
        run("(count (clojure.set/intersection nil #{1 2}))"),
        Value::Int(0)
    );
}

#[test]
fn intersection_one_arg_identity() {
    assert_eq!(
        run("(count (clojure.set/intersection #{1 2 3}))"),
        Value::Int(3)
    );
}

// --- difference ---------------------------------------------------------

#[test]
fn difference_two_basic() {
    assert_eq!(
        run("(count (clojure.set/difference #{1 2 3 4} #{2 4}))"),
        Value::Int(2)
    );
}

#[test]
fn difference_three_args_removes_all() {
    assert_eq!(
        run("(count (clojure.set/difference #{1 2 3 4} #{2} #{4}))"),
        Value::Int(2)
    );
}

#[test]
fn difference_one_arg_identity() {
    assert_eq!(
        run("(count (clojure.set/difference #{1 2 3}))"),
        Value::Int(3)
    );
}

#[test]
fn difference_nil_first_empty() {
    assert_eq!(
        run("(count (clojure.set/difference nil #{1 2}))"),
        Value::Int(0)
    );
}

#[test]
fn difference_nil_second_unchanged() {
    assert_eq!(
        run("(count (clojure.set/difference #{1 2 3} nil))"),
        Value::Int(3)
    );
}

// --- subset? / superset? -----------------------------------------------

#[test]
fn subset_q_basic_true() {
    assert_eq!(
        run("(clojure.set/subset? #{1 2} #{1 2 3})"),
        Value::Bool(true)
    );
}

#[test]
fn subset_q_equal_sets_true() {
    assert_eq!(
        run("(clojure.set/subset? #{1 2 3} #{1 2 3})"),
        Value::Bool(true)
    );
}

#[test]
fn subset_q_empty_subset_of_anything() {
    assert_eq!(run("(clojure.set/subset? #{} #{1 2})"), Value::Bool(true));
    assert_eq!(run("(clojure.set/subset? #{} #{})"), Value::Bool(true));
}

#[test]
fn subset_q_non_subset_false() {
    assert_eq!(
        run("(clojure.set/subset? #{1 4} #{1 2 3})"),
        Value::Bool(false)
    );
}

#[test]
fn superset_q_basic() {
    assert_eq!(
        run("(clojure.set/superset? #{1 2 3} #{1 2})"),
        Value::Bool(true)
    );
    assert_eq!(
        run("(clojure.set/superset? #{1 2} #{1 2 3})"),
        Value::Bool(false)
    );
}

// --- select -------------------------------------------------------------

#[test]
fn select_filters_set() {
    assert_eq!(
        run("(count (clojure.set/select even? #{1 2 3 4 5}))"),
        Value::Int(2)
    );
}

#[test]
fn select_empty_input() {
    assert_eq!(
        run("(count (clojure.set/select even? #{}))"),
        Value::Int(0)
    );
}

#[test]
fn select_all_match() {
    assert_eq!(
        run("(count (clojure.set/select pos? #{1 2 3}))"),
        Value::Int(3)
    );
}

// --- map-invert ---------------------------------------------------------

#[test]
fn map_invert_basic() {
    let v = run("(get (clojure.set/map-invert {:a 1 :b 2}) 1)");
    assert_eq!(v, Value::Keyword(std::sync::Arc::from("a")));
}

#[test]
fn map_invert_empty() {
    assert_eq!(
        run("(count (clojure.set/map-invert {}))"),
        Value::Int(0)
    );
}

#[test]
fn map_invert_collisions_last_wins() {
    // {:a 1 :b 1} → {1 :a} or {1 :b}; only one survives.
    assert_eq!(
        run("(count (clojure.set/map-invert {:a 1 :b 1}))"),
        Value::Int(1)
    );
}

// --- rename-keys --------------------------------------------------------

#[test]
fn rename_keys_basic() {
    let v = run("(get (clojure.set/rename-keys {:a 1 :b 2} {:a :x}) :x)");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn rename_keys_missing_key_no_op() {
    let v = run("(count (clojure.set/rename-keys {:a 1} {:missing :x}))");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn rename_keys_multiple() {
    let v = run("(clojure.set/rename-keys {:a 1 :b 2} {:a :x :b :y})");
    let cnt = run("(count (clojure.set/rename-keys {:a 1 :b 2} {:a :x :b :y}))");
    assert_eq!(cnt, Value::Int(2));
    let xv = run("(:x (clojure.set/rename-keys {:a 1 :b 2} {:a :x :b :y}))");
    assert_eq!(xv, Value::Int(1));
    let _ = v;
}

#[test]
fn rename_keys_nil_map_returns_nil() {
    assert_eq!(run("(clojure.set/rename-keys nil {:a :x})"), Value::Nil);
}

// --- project / rename --------------------------------------------------

#[test]
fn project_picks_keys() {
    let v = run("(count (clojure.set/project #{{:a 1 :b 2 :c 3} {:a 4 :b 5 :c 6}} [:a :b]))");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn project_collapses_duplicates() {
    // Two records that project to the same map → set of one.
    let v = run("(count (clojure.set/project #{{:a 1 :b 9} {:a 1 :b 8}} [:a]))");
    assert_eq!(v, Value::Int(1));
}

#[test]
fn project_empty_set() {
    assert_eq!(run("(count (clojure.set/project #{} [:a]))"), Value::Int(0));
}

#[test]
fn rename_renames_keys_in_each_record() {
    let v = run("(:x (first (clojure.set/rename #{{:a 1}} {:a :x})))");
    assert_eq!(v, Value::Int(1));
}

// --- index --------------------------------------------------------------

#[test]
fn index_groups_by_key_subset() {
    // Build: 3 records grouped by :a.
    let v = run(
        "(count (clojure.set/index
                  #{{:a 1 :b 1} {:a 1 :b 2} {:a 2 :b 3}}
                  [:a]))",
    );
    // Two distinct :a values → two buckets.
    assert_eq!(v, Value::Int(2));
}

#[test]
fn index_bucket_sizes() {
    let v = run(
        "(count (get (clojure.set/index
                        #{{:a 1 :b 1} {:a 1 :b 2} {:a 2 :b 3}}
                        [:a])
                     {:a 1}))",
    );
    assert_eq!(v, Value::Int(2));
}

#[test]
fn index_empty_input() {
    assert_eq!(run("(count (clojure.set/index #{} [:a]))"), Value::Int(0));
}

// --- join ---------------------------------------------------------------

#[test]
fn join_natural_on_shared_key() {
    // r1: a=1 keyed records; r2: a=1 keyed records; natural join on :a.
    let v = run(
        "(count (clojure.set/join
                  #{{:a 1 :x 10} {:a 2 :x 20}}
                  #{{:a 1 :y 100} {:a 2 :y 200}}))",
    );
    // 2 matches, since each :a in r1 finds exactly one in r2.
    assert_eq!(v, Value::Int(2));
}

#[test]
fn join_no_shared_keys_cartesian() {
    // r1 has :a only, r2 has :b only — no shared keys → cartesian product.
    let v = run(
        "(count (clojure.set/join
                  #{{:a 1} {:a 2}}
                  #{{:b 10} {:b 20}}))",
    );
    assert_eq!(v, Value::Int(4));
}

#[test]
fn join_with_kmap() {
    // r1.a maps to r2.aa.
    let v = run(
        "(count (clojure.set/join
                  #{{:a 1 :x :p}}
                  #{{:aa 1 :y :q}}
                  {:a :aa}))",
    );
    assert_eq!(v, Value::Int(1));
}

#[test]
fn join_empty_relations() {
    assert_eq!(run("(count (clojure.set/join #{} #{}))"), Value::Int(0));
    assert_eq!(
        run("(count (clojure.set/join #{} #{{:a 1}}))"),
        Value::Int(0)
    );
}
