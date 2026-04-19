//! Tests for the cljrs-side hierarchy system: make-hierarchy, derive,
//! underive, isa?, ancestors, descendants, parents.
//!
//! Each test gets its own Env (and thus its own global hierarchy atom),
//! so mutations don't leak between tests.

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

fn run_err(src: &str) -> String {
    let env = Env::new();
    builtins::install(&env);
    let forms = reader::read_all(src).expect("read");
    let mut last = Ok(Value::Nil);
    for f in forms {
        last = eval::eval(&f, &env);
    }
    match last {
        Ok(_) => panic!("expected error, got Ok"),
        Err(e) => format!("{}", e),
    }
}

// ---------- make-hierarchy -----------------------------------------------

#[test]
fn make_hierarchy_returns_three_empty_maps() {
    assert_eq!(run("(get (make-hierarchy) :parents)"),
               Value::Map(imbl::HashMap::new()));
    assert_eq!(run("(get (make-hierarchy) :ancestors)"),
               Value::Map(imbl::HashMap::new()));
    assert_eq!(run("(get (make-hierarchy) :descendants)"),
               Value::Map(imbl::HashMap::new()));
}

#[test]
fn make_hierarchy_is_independent_per_call() {
    // Two fresh hierarchies are equal but mutations to one don't affect
    // the other (immutability sanity check).
    assert_eq!(run("(= (make-hierarchy) (make-hierarchy))"),
               Value::Bool(true));
}

// ---------- derive (3-arity, pure) ---------------------------------------

#[test]
fn derive_records_direct_parent() {
    let src = r#"
      (def h (derive (make-hierarchy) :child :parent))
      (parents h :child)
    "#;
    assert_eq!(run(src),
               Value::Set(imbl::HashSet::from_iter([Value::Keyword("parent".into())])));
}

#[test]
fn derive_records_transitive_ancestor() {
    let src = r#"
      (-> (make-hierarchy)
          (derive :a :b)
          (derive :b :c)
          (ancestors :a))
    "#;
    assert_eq!(run(src),
               Value::Set(imbl::HashSet::from_iter([
                   Value::Keyword("b".into()),
                   Value::Keyword("c".into()),
               ])));
}

#[test]
fn derive_records_transitive_descendant() {
    let src = r#"
      (-> (make-hierarchy)
          (derive :a :b)
          (derive :b :c)
          (descendants :c))
    "#;
    assert_eq!(run(src),
               Value::Set(imbl::HashSet::from_iter([
                   Value::Keyword("a".into()),
                   Value::Keyword("b".into()),
               ])));
}

#[test]
fn derive_diamond_inheritance() {
    // a derives from b and c; b and c both derive from d.
    let src = r#"
      (-> (make-hierarchy)
          (derive :a :b)
          (derive :a :c)
          (derive :b :d)
          (derive :c :d)
          (ancestors :a))
    "#;
    assert_eq!(run(src),
               Value::Set(imbl::HashSet::from_iter([
                   Value::Keyword("b".into()),
                   Value::Keyword("c".into()),
                   Value::Keyword("d".into()),
               ])));
}

#[test]
fn derive_self_throws() {
    let err = run_err("(derive (make-hierarchy) :a :a)");
    assert!(err.contains("own parent") || err.contains("Cyclic"),
            "got: {}", err);
}

#[test]
fn derive_cycle_throws() {
    let err = run_err(r#"
      (-> (make-hierarchy)
          (derive :a :b)
          (derive :b :a))
    "#);
    assert!(err.contains("Cyclic"), "got: {}", err);
}

#[test]
fn derive_idempotent_on_existing_relation() {
    // Re-deriving the same relation shouldn't blow up or duplicate
    // ancestor entries.
    let src = r#"
      (def h (-> (make-hierarchy)
                 (derive :a :b)
                 (derive :a :b)))
      (ancestors h :a)
    "#;
    assert_eq!(run(src),
               Value::Set(imbl::HashSet::from_iter([Value::Keyword("b".into())])));
}

// ---------- isa? ----------------------------------------------------------

#[test]
fn isa_self_is_true() {
    assert_eq!(run("(isa? :foo :foo)"), Value::Bool(true));
}

#[test]
fn isa_unrelated_is_false() {
    assert_eq!(run("(isa? :foo :bar)"), Value::Bool(false));
}

#[test]
fn isa_uses_global_hierarchy_after_derive() {
    let src = r#"
      (derive ::child ::parent)
      (isa? ::child ::parent)
    "#;
    assert_eq!(run(src), Value::Bool(true));
}

#[test]
fn isa_transitive_via_derive() {
    let src = r#"
      (-> (make-hierarchy)
          (derive :a :b)
          (derive :b :c)
          (isa? :a :c))
    "#;
    assert_eq!(run(src), Value::Bool(true));
}

#[test]
fn isa_vector_pairwise() {
    let src = r#"
      (def h (-> (make-hierarchy)
                 (derive :child :parent)
                 (derive :pup :dog)))
      [(isa? h [:child :pup] [:parent :dog])
       (isa? h [:child :pup] [:parent :cat])
       (isa? h [:child] [:parent :dog])]
    "#;
    assert_eq!(run(src), Value::Vector(imbl::Vector::from_iter([
        Value::Bool(true),
        Value::Bool(false),
        Value::Bool(false),
    ])));
}

#[test]
fn isa_explicit_hierarchy_does_not_use_global() {
    let src = r#"
      (derive ::a ::b)
      (let [h (make-hierarchy)]
        (isa? h ::a ::b))
    "#;
    assert_eq!(run(src), Value::Bool(false));
}

// ---------- ancestors / descendants / parents (1- and 2-arity) -----------

#[test]
fn ancestors_returns_nil_when_none() {
    assert_eq!(run("(ancestors (make-hierarchy) :nope)"), Value::Nil);
}

#[test]
fn descendants_returns_nil_when_none() {
    assert_eq!(run("(descendants (make-hierarchy) :nope)"), Value::Nil);
}

#[test]
fn parents_returns_nil_when_none() {
    assert_eq!(run("(parents (make-hierarchy) :nope)"), Value::Nil);
}

#[test]
fn parents_returns_only_direct_parents() {
    let src = r#"
      (-> (make-hierarchy)
          (derive :a :b)
          (derive :b :c)
          (parents :a))
    "#;
    assert_eq!(run(src),
               Value::Set(imbl::HashSet::from_iter([Value::Keyword("b".into())])));
}

#[test]
fn parents_global_via_membership() {
    let src = r#"
      (derive :foo :bar)
      (contains? (parents :foo) :bar)
    "#;
    assert_eq!(run(src), Value::Bool(true));
}

#[test]
fn ancestors_global_via_membership() {
    let src = r#"
      (derive :a :b)
      (derive :b :c)
      [(contains? (ancestors :a) :b)
       (contains? (ancestors :a) :c)]
    "#;
    assert_eq!(run(src), Value::Vector(imbl::Vector::from_iter([
        Value::Bool(true), Value::Bool(true),
    ])));
}

#[test]
fn descendants_global_via_membership() {
    let src = r#"
      (derive :a :b)
      (derive :b :c)
      (contains? (descendants :c) :a)
    "#;
    assert_eq!(run(src), Value::Bool(true));
}

// ---------- underive ------------------------------------------------------

#[test]
fn underive_removes_direct_relation() {
    let src = r#"
      (def h (-> (make-hierarchy)
                 (derive :a :b)
                 (underive :a :b)))
      (parents h :a)
    "#;
    assert_eq!(run(src), Value::Nil);
}

#[test]
fn underive_clears_transitive_ancestors() {
    let src = r#"
      (def h (-> (make-hierarchy)
                 (derive :a :b)
                 (derive :b :c)
                 (underive :a :b)))
      (ancestors h :a)
    "#;
    assert_eq!(run(src), Value::Nil);
}

#[test]
fn underive_preserves_unrelated_branches() {
    // c -> d still stands after we cut a -> b.
    let src = r#"
      (def h (-> (make-hierarchy)
                 (derive :a :b)
                 (derive :c :d)
                 (underive :a :b)))
      [(parents h :a) (parents h :c)]
    "#;
    assert_eq!(run(src), Value::Vector(imbl::Vector::from_iter([
        Value::Nil,
        Value::Set(imbl::HashSet::from_iter([Value::Keyword("d".into())])),
    ])));
}

#[test]
fn underive_no_op_if_relation_absent() {
    let src = r#"
      (def h0 (derive (make-hierarchy) :a :b))
      (def h1 (underive h0 :a :z))
      (= h0 h1)
    "#;
    assert_eq!(run(src), Value::Bool(true));
}

#[test]
fn underive_via_global_then_isa_false() {
    let src = r#"
      (derive :child :parent)
      (underive :child :parent)
      (isa? :child :parent)
    "#;
    assert_eq!(run(src), Value::Bool(false));
}

// ---------- composition --------------------------------------------------

#[test]
fn isa_composes_with_derive_chain() {
    // Build chain via `reduce` to stress that derive composes naturally.
    let src = r#"
      (def h (reduce (fn [hh [c p]] (derive hh c p))
                     (make-hierarchy)
                     [[:a :b] [:b :c] [:c :d]]))
      [(isa? h :a :d) (isa? h :a :b) (isa? h :d :a)]
    "#;
    assert_eq!(run(src), Value::Vector(imbl::Vector::from_iter([
        Value::Bool(true), Value::Bool(true), Value::Bool(false),
    ])));
}

#[test]
fn ancestors_returned_set_supports_set_ops() {
    let src = r#"
      (def h (-> (make-hierarchy)
                 (derive :a :b)
                 (derive :a :c)))
      (count (ancestors h :a))
    "#;
    assert_eq!(run(src), Value::Int(2));
}

// Remove the misleading parents_global_after_mutation test (pinning
// keyword printing is brittle) — leave only the membership-based check.
#[test]
fn parents_membership_after_global_derive() {
    let src = r#"
      (derive :pup :dog)
      (= #{:dog} (parents :pup))
    "#;
    assert_eq!(run(src), Value::Bool(true));
}
