use std::sync::Arc;

use imbl::Vector as PVec;

use crate::env::Env;
use crate::error::{Error, Result};
use crate::eval;
use crate::value::{Builtin, Value};

/// Flatten any sequence-like Value into a concrete `Vec<Value>` for
/// uniform iteration. Clones are Arc bumps so this is cheap.
fn seq_items(v: &Value) -> Result<Vec<Value>> {
    match v {
        Value::Nil => Ok(Vec::new()),
        Value::List(xs) => Ok(xs.as_ref().clone()),
        Value::Vector(xs) => Ok(xs.iter().cloned().collect()),
        Value::Set(xs) => Ok(xs.iter().cloned().collect()),
        Value::Map(m) => Ok(m
            .iter()
            .map(|(k, v)| Value::Vector(PVec::from_iter([k.clone(), v.clone()])))
            .collect()),
        Value::Str(s) => Ok(s
            .chars()
            .map(|c| Value::Str(Arc::from(c.to_string().as_str())))
            .collect()),
        // Walk Cons + LazySeq chains. WARNING: this fully realizes a
        // lazy seq, so don't pass an infinite one (use take first).
        Value::Cons(_, _) | Value::LazySeq(_) => {
            let mut out = Vec::new();
            let mut cur = v.clone();
            loop {
                let resolved = match cur {
                    Value::LazySeq(l) => l.force()?,
                    other => other,
                };
                match resolved {
                    Value::Cons(h, t) => {
                        out.push((*h).clone());
                        cur = (*t).clone();
                    }
                    Value::Nil => break,
                    Value::List(xs) => {
                        out.extend(xs.iter().cloned());
                        break;
                    }
                    Value::Vector(xs) => {
                        out.extend(xs.iter().cloned());
                        break;
                    }
                    other => {
                        return Err(Error::Type(format!(
                            "lazy-seq tail produced {}",
                            other.type_name()
                        )));
                    }
                }
            }
            Ok(out)
        }
        _ => Err(Error::Type(format!(
            "expected sequence, got {}",
            v.type_name()
        ))),
    }
}

pub fn install(env: &Env) {
    // Builtins + prelude live in cljrs.core. Start there; switch to
    // `user` after so user code doesn't accidentally extend core.
    env.set_current_ns(crate::env::CORE_NS);
    for (name, f) in core_fns() {
        env.define_global(name, Value::Builtin(Builtin::new_static(name, f)));
    }
    for (alias, original) in TRANSDUCER_BUILTIN_ALIASES {
        if let Ok(v) = env.lookup(original) {
            env.define_global(alias, v);
        }
    }
    // clojure.string compatibility — every str/* is also reachable as
    // clojure.string/* so existing Clojure code that uses the canonical
    // qualified prefix runs unchanged.
    for (name, _) in core_fns() {
        if let Some(rest) = name.strip_prefix("str/") {
            if let Ok(v) = env.lookup(name) {
                env.define_global(&format!("clojure.string/{rest}"), v);
            }
        }
    }
    // Numeric constants. Bind as plain values, not zero-arity fns,
    // so user code can write `(* PI x)` directly.
    env.define_global("PI", Value::Float(std::f64::consts::PI));
    env.define_global("E",  Value::Float(std::f64::consts::E));
    env.define_global("TAU", Value::Float(std::f64::consts::TAU));
    install_prelude(env);
    env.set_current_ns(crate::env::USER_NS);
}

const TRANSDUCER_BUILTIN_ALIASES: &[(&str, &str)] = &[
    ("__map-coll", "map"),
    ("__filter-coll", "filter"),
    ("__take-coll", "take"),
    ("__drop-coll", "drop"),
    ("__take-while-coll", "take-while"),
    ("__drop-while-coll", "drop-while"),
    ("__mapcat-coll", "mapcat"),
    ("__partition-coll", "partition"),
    ("__distinct-coll", "distinct"),
    ("__interpose-coll", "interpose"),
];

/// Evaluate the cljrs-authored prelude (threading macros, conditional
/// macros, iteration macros, etc.). Bundled via `include_str!` so a
/// compiled cljrs binary needs no external file at runtime.
fn install_prelude(env: &Env) {
    const PRELUDE: &str = include_str!("core.clj");
    const TEST_NS: &str = include_str!("cljrs_test.clj");
    const MUSIC_NS: &str = include_str!("cljrs_music.clj");
    const UI_NS: &str = include_str!("cljrs_ui.clj");
    // clojure.set / clojure.walk / clojure.edn — pure cljrs ports of
    // the standard-library namespaces. See src/cljrs_{set,walk,edn}.clj.
    const SET_NS: &str = include_str!("cljrs_set.clj");
    const WALK_NS: &str = include_str!("cljrs_walk.clj");
    const EDN_NS: &str = include_str!("cljrs_edn.clj");
    for (label, src) in [
        ("core.clj", PRELUDE),
        ("cljrs_test.clj", TEST_NS),
        ("cljrs_music.clj", MUSIC_NS),
        ("cljrs_ui.clj", UI_NS),
        ("cljrs_set.clj", SET_NS),
        ("cljrs_walk.clj", WALK_NS),
        ("cljrs_edn.clj", EDN_NS),
    ] {
        let forms = match crate::reader::read_all(src) {
            Ok(f) => f,
            Err(e) => panic!("cljrs prelude {label} parse failed: {e}"),
        };
        for f in forms {
            if let Err(e) = eval::eval(&f, env) {
                panic!("cljrs prelude {label} eval failed: {e}");
            }
        }
    }
}

fn core_fns() -> Vec<(&'static str, fn(&[Value]) -> Result<Value>)> {
    vec![
        ("+", add),
        ("-", sub),
        ("*", mul),
        ("/", div),
        ("=", eq),
        ("<", lt),
        (">", gt),
        ("<=", le),
        (">=", ge),
        ("not", not_fn),
        ("not=", not_eq_fn),
        ("subvec", subvec_fn),
        ("str", to_str),
        ("println", println_fn),
        ("pr-str", pr_str_fn),
        ("count", count_fn),
        ("first", first_fn),
        ("rest", rest_fn),
        ("cons", cons_fn),
        ("concat", concat_fn),
        ("list", list_fn),
        ("vector", vector_fn),
        ("conj", conj_fn),
        ("nth", nth_fn),
        ("vec", vec_fn),
        ("nil?", nil_q),
        ("zero?", zero_q),
        ("empty?", empty_q),
        ("inc", inc_fn),
        ("dec", dec_fn),
        ("map", map_fn),
        ("filter", filter_fn),
        ("reduce", reduce_fn),
        ("range", range_fn),
        ("take", take_fn),
        ("drop", drop_fn),
        ("even?", even_q),
        ("odd?", odd_q),
        ("pos?", pos_q),
        ("neg?", neg_q),
        ("identity", identity_fn),
        ("get", get_fn),
        ("assoc", assoc_fn),
        ("dissoc", dissoc_fn),
        ("keys", keys_fn),
        ("vals", vals_fn),
        ("contains?", contains_q),
        ("find", find_fn),
        ("update", update_fn),
        ("merge", merge_fn),
        ("select-keys", select_keys_fn),
        ("keyword", keyword_fn),
        ("symbol", symbol_fn),
        ("name", name_fn),
        ("hash-map", hash_map_fn),
        ("hash-set", hash_set_fn),
        ("set", set_fn),
        ("into", into_fn),
        ("reverse", reverse_fn),
        ("sort", sort_fn),
        ("second", second_fn),
        ("last", last_fn),
        ("apply", apply_builtin),
        ("subs", subs_fn),
        ("str/split", str_split_fn),
        ("str/join", str_join_fn),
        ("str/upper-case", str_upper_fn),
        ("str/lower-case", str_lower_fn),
        ("str/replace", str_replace_fn),
        ("str/starts-with?", str_starts_with_fn),
        ("str/ends-with?", str_ends_with_fn),
        ("str/includes?", str_includes_fn),
        ("str/trim", str_trim_fn),
        ("str/blank?", str_blank_fn),
        ("str/index-of", str_index_of_fn),
        ("str/last-index-of", str_last_index_of_fn),
        ("string?", string_q),
        ("number?", number_q),
        ("integer?", integer_q),
        ("float?", float_q),
        ("map?", map_q),
        ("vector?", vector_q),
        ("set?", set_q),
        ("list?", list_q),
        ("seq?", seq_q),
        ("coll?", coll_q),
        ("keyword?", keyword_q),
        ("symbol?", symbol_q),
        ("fn?", fn_q),
        ("boolean?", boolean_q),
        ("true?", true_q),
        ("false?", false_q),
        ("some?", some_q),
        ("some", some_fn),
        ("every?", every_q),
        ("not-empty", not_empty_fn),
        ("mod", mod_fn),
        ("rem", rem_fn),
        ("quot", quot_fn),
        ("min", min_fn),
        ("max", max_fn),
        ("abs", abs_fn),
        ("repeat", repeat_fn),
        ("take-while", take_while_fn),
        ("drop-while", drop_while_fn),
        ("partition", partition_fn),
        ("interleave", interleave_fn),
        ("interpose", interpose_fn),
        ("frequencies", frequencies_fn),
        ("group-by", group_by_fn),
        ("distinct", distinct_fn),
        ("mapv", mapv_fn),
        ("filterv", filterv_fn),
        ("reduce-kv", reduce_kv_fn),
        ("update-in", update_in_fn),
        ("get-in", get_in_fn),
        ("assoc-in", assoc_in_fn),
        ("comp", comp_fn),
        ("partial", partial_fn),
        ("complement", complement_fn),
        ("juxt", juxt_fn),
        ("constantly", constantly_fn),
        ("println-str", println_str_fn),
        ("print", print_fn),
        ("print-str", print_str_fn),
        ("slurp", slurp_fn),
        ("spit", spit_fn),
        ("read-string", read_string_fn),
        ("sqrt", sqrt_fn),
        ("pow", pow_fn),
        ("sin", sin_fn),
        ("cos", cos_fn),
        ("tan", tan_fn),
        ("exp", exp_fn),
        ("log", log_fn),
        ("floor", floor_fn),
        ("ceil", ceil_fn),
        ("round", round_fn),
        ("int", int_fn),
        ("long", int_fn),
        ("double", double_fn),
        ("float", double_fn),
        ("Math/PI", pi_fn),
        ("atom", atom_fn),
        ("deref", deref_fn),
        ("reset!", reset_bang_fn),
        ("swap!", swap_bang_fn),
        ("compare-and-set!", cas_bang_fn),
        ("atom?", atom_q),
        ("throw", throw_fn),
        ("ex-info", ex_info_fn),
        ("ex-message", ex_message_fn),
        ("ex-data", ex_data_fn),
        ("re-pattern", re_pattern_fn),
        ("re-find", re_find_fn),
        ("re-matches", re_matches_fn),
        ("re-seq", re_seq_fn),
        ("gensym", gensym_fn),
        ("__lazy-seq", __lazy_seq_fn),
        ("force-seq", force_seq_fn),
        ("realized?", realized_q),
        ("seq", seq_fn),
        ("mapcat", mapcat_fn),
        ("flatten", flatten_fn),
        ("zipmap", zipmap_fn),
        ("max-key", max_key_fn),
        ("min-key", min_key_fn),
        ("update-vals", update_vals_fn),
        ("update-keys", update_keys_fn),
        ("reduced", reduced_fn),
        ("reduced?", reduced_q),
        ("unreduced", unreduced_fn),
        ("transduce", transduce_fn),
        ("__partition-all-coll", partition_all_coll_fn),
        ("__dedupe-coll", dedupe_coll_fn),
        ("__take-nth-coll", take_nth_coll_fn),
        ("__keep-coll", keep_coll_fn),
        ("__keep-indexed-coll", keep_indexed_coll_fn),
        ("__map-indexed-coll", map_indexed_coll_fn),
        ("==", num_eq_fn),
        ("bit-and", bit_and_fn),
        ("bit-or", bit_or_fn),
        ("bit-xor", bit_xor_fn),
        ("bit-not", bit_not_fn),
        ("bit-and-not", bit_and_not_fn),
        ("bit-shift-left", bit_shl_fn),
        ("bit-shift-right", bit_shr_fn),
        ("unsigned-bit-shift-right", bit_ushr_fn),
        ("bit-flip", bit_flip_fn),
        ("bit-set", bit_set_fn),
        ("bit-clear", bit_clear_fn),
        ("bit-test", bit_test_fn),
        ("parse-long", parse_long_fn),
        ("parse-double", parse_double_fn),
        ("parse-boolean", parse_boolean_fn),
        ("parse-uuid", parse_uuid_fn),
        ("unchecked-add", unchecked_add_fn),
        ("unchecked-add-int", unchecked_add_fn),
        ("unchecked-subtract", unchecked_sub_fn),
        ("unchecked-subtract-int", unchecked_sub_fn),
        ("unchecked-multiply", unchecked_mul_fn),
        ("unchecked-multiply-int", unchecked_mul_fn),
        ("unchecked-divide-int", unchecked_div_fn),
        ("unchecked-negate", unchecked_neg_fn),
        ("unchecked-negate-int", unchecked_neg_fn),
        ("unchecked-inc", unchecked_inc_fn),
        ("unchecked-inc-int", unchecked_inc_fn),
        ("unchecked-dec", unchecked_dec_fn),
        ("unchecked-dec-int", unchecked_dec_fn),
        ("unchecked-remainder-int", unchecked_rem_fn),
        ("unchecked-int", int_fn),
        ("unchecked-long", int_fn),
        ("unchecked-double", double_fn),
        ("unchecked-float", double_fn),
        ("unchecked-byte", int_fn),
        ("unchecked-short", int_fn),
        ("unchecked-char", int_fn),
        ("numerator", numerator_fn),
        ("denominator", denominator_fn),
        ("rationalize", rationalize_fn),
        ("num", num_fn),
        ("compare", compare_fn),
        ("hash", hash_fn),
        ("hash-ordered-coll", hash_ordered_coll_fn),
        ("hash-unordered-coll", hash_unordered_coll_fn),
        ("mix-collection-hash", mix_collection_hash_fn),
        ("NaN?", nan_q_fn),
        ("infinite?", infinite_q_fn),
        ("boolean", boolean_fn),
        ("instance?", instance_q_fn),
        ("identical?", identical_q_fn),
        ("ratio?", ratio_q_fn),
        ("decimal?", const_false_fn),
        ("bigdec?", const_false_fn),
        ("volatile?", atom_q),
        ("chunked-seq?", const_false_fn),
        ("any?", any_q_fn),
        ("ifn?", ifn_q_fn),
        ("ident?", ident_q_fn),
        ("simple-ident?", simple_ident_q_fn),
        ("qualified-ident?", qualified_ident_q_fn),
        ("simple-keyword?", simple_keyword_q_fn),
        ("qualified-keyword?", qualified_keyword_q_fn),
        ("simple-symbol?", simple_symbol_q_fn),
        ("qualified-symbol?", qualified_symbol_q_fn),
        ("indexed?", indexed_q_fn),
        ("counted?", counted_q_fn),
        ("seqable?", seqable_q_fn),
        ("sequential?", sequential_q_fn),
        ("associative?", associative_q_fn),
        ("reversible?", reversible_q_fn),
        ("sorted?", const_false_fn),
        ("record?", const_false_fn),
        ("map-entry?", map_entry_q_fn),
        ("uri?", const_false_fn),
        ("uuid?", uuid_q_fn),
        ("inst?", const_false_fn),
        ("int?", integer_q),
        ("double?", float_q),
        ("nat-int?", nat_int_q_fn),
        ("neg-int?", neg_int_q_fn),
        ("pos-int?", pos_int_q_fn),
        ("distinct?", distinct_q_fn),
        ("not-any?", not_any_q_fn),
        ("not-every?", not_every_q_fn),
        ("special-symbol?", special_symbol_q_fn),
        ("peek", peek_fn),
        ("pop", pop_fn),
        ("disj", disj_fn),
        ("replace", replace_fn),
        ("merge-with", merge_with_fn),
        ("rseq", rseq_fn),
        ("empty", empty_fn),
        ("vector-of", vector_of_fn),
        ("array-map", hash_map_fn),
        ("sorted-map", hash_map_fn),
        ("sorted-map-by", sorted_map_by_fn),
        ("sorted-set", sorted_set_fn),
        ("sorted-set-by", sorted_set_by_fn),
        ("subseq", subseq_fn),
        ("rsubseq", rsubseq_fn),
        ("key", key_fn),
        ("val", val_fn),
        ("find-keyword", find_keyword_fn),
        ("namespace", namespace_fn),
        ("partition-by", partition_by_fn),
        ("partition-all", partition_all_fn),
        ("partitionv", partition_fn),
        ("partitionv-all", partition_all_fn),
        ("split-at", split_at_fn),
        ("splitv-at", splitv_at_fn),
        ("split-with", split_with_fn),
        ("take-last", take_last_fn),
        ("drop-last", drop_last_fn),
        ("butlast", butlast_fn),
        ("keep", keep_coll_fn),
        ("keep-indexed", keep_indexed_coll_fn),
        ("map-indexed", map_indexed_coll_fn),
        ("dedupe", dedupe_coll_fn),
        ("take-nth", take_nth_coll_fn),
        ("tree-seq", tree_seq_fn),
        ("cycle", cycle_fn),
        ("iterate", iterate_fn),
        ("replicate", replicate_fn),
        ("reductions", reductions_fn),
        ("list*", list_star_fn),
        ("ffirst", ffirst_fn),
        ("fnext", fnext_fn),
        ("nfirst", nfirst_fn),
        ("nnext", nnext_fn),
        ("next", next_fn),
        ("nthnext", nthnext_fn),
        ("nthrest", nthrest_fn),
        ("doall", doall_fn),
        ("dorun", dorun_fn),
        ("run!", run_bang_fn),
        ("shuffle", shuffle_fn),
        ("rand", rand_fn),
        ("rand-int", rand_int_fn),
        ("rand-nth", rand_nth_fn),
        ("random-sample", random_sample_fn),
        ("random-uuid", random_uuid_fn),
        ("bounded-count", bounded_count_fn),
        ("every-pred", every_pred_fn),
        ("some-fn", some_fn_fn),
        ("fnil", fnil_fn),
        ("memoize", memoize_fn),
        ("comparator", comparator_fn),
        ("sort-by", sort_by_fn),
        ("completing", completing_fn),
        ("ensure-reduced", ensure_reduced_fn),
        ("trampoline", trampoline_fn),
        ("transient", identity_fn),
        ("persistent!", identity_fn),
        ("conj!", conj_fn),
        ("assoc!", assoc_fn),
        ("dissoc!", dissoc_fn),
        ("pop!", pop_fn),
        ("disj!", disj_fn),
        ("volatile!", atom_fn),
        ("vreset!", reset_bang_fn),
        ("vswap!", swap_bang_fn),
        ("meta", const_nil_fn),
        ("with-meta", with_meta_fn),
        ("vary-meta", vary_meta_fn),
        ("alter-meta!", const_nil_fn),
        ("reset-meta!", const_nil_fn),
        ("time-ms", time_ms_fn),
        ("type", type_fn),
        ("flush", flush_fn),
        ("newline", newline_fn),
        ("pr", pr_fn),
        ("prn", prn_fn),
        ("prn-str", prn_str_fn),
        ("ex-cause", ex_cause_fn),
        ("test", const_nil_fn),
        ("get-validator", const_nil_fn),
        ("set-validator!", set_validator_fn),
        ("methods", methods_fn),
        ("get-method", get_method_fn),
        ("prefer-method", prefer_method_fn),
        ("prefers", prefers_fn),
        ("remove-method", remove_method_fn),
        ("remove-all-methods", remove_all_methods_fn),
        ("add-tap", const_nil_fn),
        ("remove-tap", const_false_fn),
        ("tap>", const_false_fn),
        ("inst-ms", inst_ms_fn),
    ]
}

fn partition_all_coll_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let n = to_i64(&args[0])?.max(1) as usize;
    let coll = seq_items(&args[1])?;
    let mut out: Vec<Value> = Vec::new();
    for chunk in coll.chunks(n) {
        out.push(Value::List(Arc::new(chunk.to_vec())));
    }
    Ok(Value::List(Arc::new(out)))
}

fn dedupe_coll_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    let coll = seq_items(&args[0])?;
    let mut out = Vec::with_capacity(coll.len());
    let mut last: Option<Value> = None;
    for v in coll {
        if last.as_ref() != Some(&v) {
            last = Some(v.clone());
            out.push(v);
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn take_nth_coll_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let n = to_i64(&args[0])?.max(1) as usize;
    let coll = seq_items(&args[1])?;
    let out: Vec<Value> = coll.into_iter().step_by(n).collect();
    Ok(Value::List(Arc::new(out)))
}

fn keep_coll_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let f = &args[0];
    let coll = seq_items(&args[1])?;
    let mut out = Vec::new();
    for item in coll {
        let v = eval::apply(f, std::slice::from_ref(&item))?;
        if !matches!(v, Value::Nil) {
            out.push(v);
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn keep_indexed_coll_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let f = &args[0];
    let coll = seq_items(&args[1])?;
    let mut out = Vec::new();
    for (i, item) in coll.into_iter().enumerate() {
        let v = eval::apply(f, &[Value::Int(i as i64), item])?;
        if !matches!(v, Value::Nil) {
            out.push(v);
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn map_indexed_coll_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let f = &args[0];
    let coll = seq_items(&args[1])?;
    let mut out = Vec::with_capacity(coll.len());
    for (i, item) in coll.into_iter().enumerate() {
        out.push(eval::apply(f, &[Value::Int(i as i64), item])?);
    }
    Ok(Value::List(Arc::new(out)))
}

fn reduced_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    Ok(Value::Reduced(Arc::new(args[0].clone())))
}

fn reduced_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Reduced(_))))
}

fn unreduced_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Reduced(inner) => Ok((**inner).clone()),
        other => Ok(other.clone()),
    }
}

/// (transduce xform rf init coll). Composes rf with the xform, then
/// reduces over coll, finally calls rf's 1-arity on the result for
/// completion (e.g. `conj` would finalize, `+` might just pass through).
/// The 3-arg form (transduce xform rf coll) uses (rf) as init.
fn transduce_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 3 && args.len() != 4 {
        return Err(Error::Arity { expected: "3 or 4".into(), got: args.len() });
    }
    let xform = &args[0];
    let rf = &args[1];
    let (init, coll) = if args.len() == 4 {
        (args[2].clone(), &args[3])
    } else {
        // (transduce xform rf coll): init = (rf)
        (eval::apply(rf, &[])?, &args[2])
    };
    // xform is (rf) -> rf', where rf' is a multi-arity fn.
    let xrf = eval::apply(xform, std::slice::from_ref(rf))?;
    let items = seq_items(coll)?;
    let mut acc = init;
    for item in items {
        acc = eval::apply(&xrf, &[acc, item])?;
        if let Value::Reduced(inner) = &acc {
            acc = (**inner).clone();
            break;
        }
    }
    // Final completion call: xrf with 1 arg.
    eval::apply(&xrf, &[acc])
}

/// (seq coll) -> nil if empty, otherwise the seq itself (as a list).
/// Idiomatic Clojure: `(if (seq xs) ...)` to test non-emptiness.
fn seq_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    let v = resolve_seq(&args[0])?;
    let empty = match &v {
        Value::Nil => true,
        Value::List(v) => v.is_empty(),
        Value::Vector(v) => v.is_empty(),
        Value::Map(m) => m.is_empty(),
        Value::Set(s) => s.is_empty(),
        Value::Str(s) => s.is_empty(),
        Value::Cons(_, _) => false,
        _ => return Err(Error::Type(format!("seq on {}", v.type_name()))),
    };
    if empty { Ok(Value::Nil) } else { Ok(v) }
}

/// (mapcat f coll): map then concat. Common enough to warrant a builtin.
fn mapcat_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let f = &args[0];
    let coll = seq_items(&args[1])?;
    let mut out = Vec::new();
    for item in coll {
        let r = eval::apply(f, std::slice::from_ref(&item))?;
        out.extend(seq_items(&r)?);
    }
    Ok(Value::List(Arc::new(out)))
}

/// (flatten coll): one-level deep recursive concat.
fn flatten_fn(args: &[Value]) -> Result<Value> {
    let mut out = Vec::new();
    fn walk(v: &Value, out: &mut Vec<Value>) {
        match v {
            Value::List(xs) => { for x in xs.iter() { walk(x, out); } }
            Value::Vector(xs) => { for x in xs.iter() { walk(x, out); } }
            Value::Cons(h, t) => { walk(h, out); walk(t, out); }
            other => out.push(other.clone()),
        }
    }
    walk(&args[0], &mut out);
    Ok(Value::List(Arc::new(out)))
}

fn zipmap_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let ks = seq_items(&args[0])?;
    let vs = seq_items(&args[1])?;
    let mut m: imbl::HashMap<Value, Value> = imbl::HashMap::new();
    for (k, v) in ks.into_iter().zip(vs.into_iter()) {
        m.insert(k, v);
    }
    Ok(Value::Map(m))
}

fn max_key_fn(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(Error::Arity { expected: ">= 2".into(), got: args.len() });
    }
    let f = &args[0];
    let mut best = args[1].clone();
    let mut best_k = as_f64(&eval::apply(f, std::slice::from_ref(&best))?)?;
    for v in &args[2..] {
        let k = as_f64(&eval::apply(f, std::slice::from_ref(v))?)?;
        if k > best_k {
            best_k = k;
            best = v.clone();
        }
    }
    Ok(best)
}

fn min_key_fn(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(Error::Arity { expected: ">= 2".into(), got: args.len() });
    }
    let f = &args[0];
    let mut best = args[1].clone();
    let mut best_k = as_f64(&eval::apply(f, std::slice::from_ref(&best))?)?;
    for v in &args[2..] {
        let k = as_f64(&eval::apply(f, std::slice::from_ref(v))?)?;
        if k < best_k {
            best_k = k;
            best = v.clone();
        }
    }
    Ok(best)
}

fn update_vals_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let f = &args[1];
    match &args[0] {
        Value::Map(m) => {
            let mut out = imbl::HashMap::new();
            for (k, v) in m.iter() {
                out.insert(k.clone(), eval::apply(f, std::slice::from_ref(v))?);
            }
            Ok(Value::Map(out))
        }
        _ => Err(Error::Type("update-vals: expected map".into())),
    }
}

fn update_keys_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let f = &args[1];
    match &args[0] {
        Value::Map(m) => {
            let mut out = imbl::HashMap::new();
            for (k, v) in m.iter() {
                out.insert(eval::apply(f, std::slice::from_ref(k))?, v.clone());
            }
            Ok(Value::Map(out))
        }
        _ => Err(Error::Type("update-keys: expected map".into())),
    }
}

// ---- Lazy sequences ----------------------------------------------------

/// Internal: wraps a 0-arg fn thunk in a Value::LazySeq. The public
/// surface is the `(lazy-seq body...)` prelude macro which expands to
/// `(__lazy-seq (fn [] body))`.
fn __lazy_seq_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    Ok(Value::LazySeq(Arc::new(crate::value::LazySeq::new_thunk(args[0].clone()))))
}

/// Force a lazy-seq (returning its underlying head), or pass through
/// for already-eager collections. Mostly useful for tests.
fn force_seq_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    resolve_seq(&args[0])
}

fn realized_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(!matches!(&args[0], Value::LazySeq(_))))
}

/// Force a (possibly nested) lazy-seq down to a concrete head: the
/// first non-LazySeq value. Returns nil / list / vector / set / map.
fn resolve_seq(v: &Value) -> Result<Value> {
    let mut cur = v.clone();
    loop {
        match cur {
            Value::LazySeq(l) => cur = l.force()?,
            other => return Ok(other),
        }
    }
}

/// (gensym) / (gensym prefix) — produce a fresh unique symbol. Used
/// inside macros to share a hygienic name across multiple syntax-
/// quoted forms within one expansion.
fn gensym_fn(args: &[Value]) -> Result<Value> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let prefix = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        Some(Value::Symbol(s)) => s.to_string(),
        None => "G__".to_string(),
        Some(other) => return Err(Error::Type(format!(
            "gensym: expected string or symbol prefix, got {}",
            other.type_name()
        ))),
    };
    Ok(Value::Symbol(Arc::from(format!("{prefix}{n}").as_str())))
}

// ---- Regex -------------------------------------------------------------

fn re_pattern_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    let pat = match &args[0] {
        Value::Str(s) => s.clone(),
        Value::Regex(r) => return Ok(Value::Regex(r.clone())),
        _ => return Err(Error::Type("re-pattern: expected string".into())),
    };
    match regex::Regex::new(pat.as_ref()) {
        Ok(r) => Ok(Value::Regex(Arc::new(r))),
        Err(e) => Err(Error::Eval(format!("re-pattern: {e}"))),
    }
}

fn as_regex(v: &Value) -> Result<Arc<regex::Regex>> {
    match v {
        Value::Regex(r) => Ok(r.clone()),
        Value::Str(s) => regex::Regex::new(s.as_ref())
            .map(Arc::new)
            .map_err(|e| Error::Eval(format!("regex: {e}"))),
        _ => Err(Error::Type("expected regex or string".into())),
    }
}

/// First match as a string. When the pattern has capture groups, returns
/// a vector [whole-match, g1, g2, ...]. Matches Clojure's re-find shape.
fn re_find_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let r = as_regex(&args[0])?;
    let s = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(Error::Type("re-find: haystack must be a string".into())),
    };
    match r.captures(s.as_ref()) {
        Some(caps) => {
            if caps.len() == 1 {
                let m = caps.get(0).unwrap().as_str();
                Ok(Value::Str(Arc::from(m)))
            } else {
                let mut out: imbl::Vector<Value> = imbl::Vector::new();
                for i in 0..caps.len() {
                    match caps.get(i) {
                        Some(m) => out.push_back(Value::Str(Arc::from(m.as_str()))),
                        None => out.push_back(Value::Nil),
                    }
                }
                Ok(Value::Vector(out))
            }
        }
        None => Ok(Value::Nil),
    }
}

/// Match only if the pattern anchors the entire string.
fn re_matches_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let r = as_regex(&args[0])?;
    let s = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(Error::Type("re-matches: haystack must be a string".into())),
    };
    let Some(caps) = r.captures(s.as_ref()) else {
        return Ok(Value::Nil);
    };
    let whole = caps.get(0).unwrap();
    if whole.start() != 0 || whole.end() != s.len() {
        return Ok(Value::Nil);
    }
    if caps.len() == 1 {
        Ok(Value::Str(Arc::from(whole.as_str())))
    } else {
        let mut out: imbl::Vector<Value> = imbl::Vector::new();
        for i in 0..caps.len() {
            match caps.get(i) {
                Some(m) => out.push_back(Value::Str(Arc::from(m.as_str()))),
                None => out.push_back(Value::Nil),
            }
        }
        Ok(Value::Vector(out))
    }
}

/// All non-overlapping matches as a list of strings (or vectors with groups).
fn re_seq_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let r = as_regex(&args[0])?;
    let s = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err(Error::Type("re-seq: haystack must be a string".into())),
    };
    let mut out: Vec<Value> = Vec::new();
    for caps in r.captures_iter(s.as_ref()) {
        if caps.len() == 1 {
            out.push(Value::Str(Arc::from(caps.get(0).unwrap().as_str())));
        } else {
            let mut v: imbl::Vector<Value> = imbl::Vector::new();
            for i in 0..caps.len() {
                match caps.get(i) {
                    Some(m) => v.push_back(Value::Str(Arc::from(m.as_str()))),
                    None => v.push_back(Value::Nil),
                }
            }
            out.push(Value::Vector(v));
        }
    }
    Ok(Value::List(Arc::new(out)))
}

// ---- Atoms -------------------------------------------------------------

fn atom_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    Ok(Value::Atom(std::sync::Arc::new(std::sync::RwLock::new(args[0].clone()))))
}
fn deref_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    match &args[0] {
        Value::Atom(a) => Ok(a.read().unwrap().clone()),
        _ => Err(Error::Type(format!("deref on {}", args[0].type_name()))),
    }
}
fn reset_bang_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    match &args[0] {
        Value::Atom(a) => {
            *a.write().unwrap() = args[1].clone();
            Ok(args[1].clone())
        }
        _ => Err(Error::Type("reset! on non-atom".into())),
    }
}
fn swap_bang_fn(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(Error::Arity { expected: ">= 2".into(), got: args.len() });
    }
    let atom = match &args[0] {
        Value::Atom(a) => a.clone(),
        _ => return Err(Error::Type("swap! on non-atom".into())),
    };
    let f = &args[1];
    let extras = &args[2..];
    // Clone current value out, apply outside the lock, then CAS-style write.
    // Single-threaded semantics for now; multi-writer atomicity deferred.
    let current = atom.read().unwrap().clone();
    let mut fargs = Vec::with_capacity(1 + extras.len());
    fargs.push(current);
    fargs.extend_from_slice(extras);
    let new = eval::apply(f, &fargs)?;
    *atom.write().unwrap() = new.clone();
    Ok(new)
}
fn cas_bang_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 3 {
        return Err(Error::Arity { expected: "3".into(), got: args.len() });
    }
    let atom = match &args[0] {
        Value::Atom(a) => a.clone(),
        _ => return Err(Error::Type("compare-and-set! on non-atom".into())),
    };
    let mut w = atom.write().unwrap();
    if *w == args[1] {
        *w = args[2].clone();
        Ok(Value::Bool(true))
    } else {
        Ok(Value::Bool(false))
    }
}
fn atom_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Atom(_))))
}

// ---- Exceptions --------------------------------------------------------

fn throw_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    Err(Error::Thrown(args[0].clone()))
}

/// `(ex-info msg data)` — build a map-shaped exception value. cljrs
/// represents thrown exceptions as plain maps with known keys so user
/// code can destructure them in catch clauses without a new value
/// variant. Matches the spirit of Clojure's ExceptionInfo.
fn ex_info_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 && args.len() != 3 {
        return Err(Error::Arity { expected: "2 or 3".into(), got: args.len() });
    }
    let msg = args[0].clone();
    let data = args[1].clone();
    let mut m = imbl::HashMap::new();
    m.insert(Value::Keyword(Arc::from("message")), msg);
    m.insert(Value::Keyword(Arc::from("data")), data);
    if let Some(cause) = args.get(2) {
        m.insert(Value::Keyword(Arc::from("cause")), cause.clone());
    }
    Ok(Value::Map(m))
}
fn ex_message_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Map(m) => Ok(m
            .get(&Value::Keyword(Arc::from("message")))
            .cloned()
            .unwrap_or(Value::Nil)),
        Value::Str(s) => Ok(Value::Str(Arc::clone(s))),
        _ => Ok(Value::Nil),
    }
}
fn ex_data_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Map(m) => Ok(m
            .get(&Value::Keyword(Arc::from("data")))
            .cloned()
            .unwrap_or(Value::Nil)),
        _ => Ok(Value::Nil),
    }
}

// ---- Constants (as zero-arity fns) --------------------------------

fn pi_fn(args: &[Value]) -> Result<Value> {
    if !args.is_empty() {
        return Err(Error::Arity { expected: "0".into(), got: args.len() });
    }
    Ok(Value::Float(std::f64::consts::PI))
}

// ---- Map / collection ops ------------------------------------------

fn get_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 && args.len() != 3 {
        return Err(Error::Arity { expected: "2 or 3".into(), got: args.len() });
    }
    let default = args.get(2).cloned().unwrap_or(Value::Nil);
    match &args[0] {
        Value::Map(m) => Ok(m.get(&args[1]).cloned().unwrap_or(default)),
        Value::Set(s) => Ok(if s.contains(&args[1]) {
            args[1].clone()
        } else {
            default
        }),
        Value::Vector(v) => match &args[1] {
            Value::Int(i) if *i >= 0 => Ok(v.get(*i as usize).cloned().unwrap_or(default)),
            _ => Ok(default),
        },
        Value::List(v) => match &args[1] {
            Value::Int(i) if *i >= 0 => Ok(v.get(*i as usize).cloned().unwrap_or(default)),
            _ => Ok(default),
        },
        Value::Nil => Ok(default),
        Value::Str(s) => match &args[1] {
            Value::Int(i) if *i >= 0 => Ok(s
                .chars()
                .nth(*i as usize)
                .map(|c| Value::Str(Arc::from(c.to_string().as_str())))
                .unwrap_or(default)),
            _ => Ok(default),
        },
        _ => Ok(default),
    }
}

fn assoc_fn(args: &[Value]) -> Result<Value> {
    if args.len() < 3 || (args.len() - 1) % 2 != 0 {
        return Err(Error::Arity { expected: "odd >= 3".into(), got: args.len() });
    }
    match &args[0] {
        Value::Nil => {
            let mut m = imbl::HashMap::new();
            let mut i = 1;
            while i < args.len() {
                m.insert(args[i].clone(), args[i + 1].clone());
                i += 2;
            }
            Ok(Value::Map(m))
        }
        Value::Map(m) => {
            let mut out = m.clone();
            let mut i = 1;
            while i < args.len() {
                out.insert(args[i].clone(), args[i + 1].clone());
                i += 2;
            }
            Ok(Value::Map(out))
        }
        Value::Vector(v) => {
            let mut out = v.clone();
            let mut i = 1;
            while i < args.len() {
                let idx = match &args[i] {
                    Value::Int(n) if *n >= 0 => *n as usize,
                    _ => return Err(Error::Type("assoc on vector: index must be non-negative int".into())),
                };
                if idx > out.len() {
                    return Err(Error::Eval(format!("assoc: index {idx} out of range")));
                }
                if idx == out.len() {
                    out.push_back(args[i + 1].clone());
                } else {
                    out.set(idx, args[i + 1].clone());
                }
                i += 2;
            }
            Ok(Value::Vector(out))
        }
        _ => Err(Error::Type(format!("assoc on {}", args[0].type_name()))),
    }
}

fn dissoc_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Arity { expected: ">= 1".into(), got: 0 });
    }
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Map(m) => {
            let mut out = m.clone();
            for k in &args[1..] {
                out.remove(k);
            }
            Ok(Value::Map(out))
        }
        _ => Err(Error::Type(format!("dissoc on {}", args[0].type_name()))),
    }
}

fn keys_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Map(m) => Ok(Value::List(Arc::new(m.keys().cloned().collect()))),
        _ => Err(Error::Type("keys on non-map".into())),
    }
}

fn vals_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Map(m) => Ok(Value::List(Arc::new(m.values().cloned().collect()))),
        _ => Err(Error::Type("vals on non-map".into())),
    }
}

fn contains_q(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    Ok(Value::Bool(match &args[0] {
        Value::Nil => false,
        Value::Map(m) => m.contains_key(&args[1]),
        Value::Set(s) => s.contains(&args[1]),
        Value::Vector(v) => matches!(&args[1], Value::Int(i) if *i >= 0 && (*i as usize) < v.len()),
        _ => return Err(Error::Type(format!("contains? on {}", args[0].type_name()))),
    }))
}

fn find_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    match &args[0] {
        Value::Map(m) => Ok(match m.get(&args[1]) {
            Some(v) => Value::Vector(PVec::from_iter([args[1].clone(), v.clone()])),
            None => Value::Nil,
        }),
        _ => Err(Error::Type("find on non-map".into())),
    }
}

fn update_fn(args: &[Value]) -> Result<Value> {
    if args.len() < 3 {
        return Err(Error::Arity { expected: ">= 3".into(), got: args.len() });
    }
    let coll = &args[0];
    let key = &args[1];
    let f = &args[2];
    let extra = &args[3..];
    let cur = get_fn(&[coll.clone(), key.clone()])?;
    let mut fargs = Vec::with_capacity(1 + extra.len());
    fargs.push(cur);
    fargs.extend_from_slice(extra);
    let new_v = eval::apply(f, &fargs)?;
    assoc_fn(&[coll.clone(), key.clone(), new_v])
}

fn merge_fn(args: &[Value]) -> Result<Value> {
    let mut out: Option<imbl::HashMap<Value, Value>> = None;
    for a in args {
        match a {
            Value::Nil => {}
            Value::Map(m) => {
                if let Some(ref mut o) = out {
                    for (k, v) in m.iter() {
                        o.insert(k.clone(), v.clone());
                    }
                } else {
                    out = Some(m.clone());
                }
            }
            _ => return Err(Error::Type(format!("merge on {}", a.type_name()))),
        }
    }
    Ok(out.map(Value::Map).unwrap_or(Value::Nil))
}

fn select_keys_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let m = match &args[0] {
        Value::Map(m) => m,
        _ => return Err(Error::Type("select-keys: first arg must be a map".into())),
    };
    let ks = seq_items(&args[1])?;
    let mut out = imbl::HashMap::new();
    for k in &ks {
        if let Some(v) = m.get(k) {
            out.insert(k.clone(), v.clone());
        }
    }
    Ok(Value::Map(out))
}

fn keyword_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    let s: Arc<str> = match &args[0] {
        Value::Str(s) => Arc::clone(s),
        Value::Keyword(s) => Arc::clone(s),
        Value::Symbol(s) => Arc::clone(s),
        _ => return Err(Error::Type("keyword: expected string/keyword/symbol".into())),
    };
    Ok(Value::Keyword(s))
}

fn symbol_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    let s: Arc<str> = match &args[0] {
        Value::Str(s) => Arc::clone(s),
        Value::Symbol(s) => Arc::clone(s),
        Value::Keyword(s) => Arc::clone(s),
        _ => return Err(Error::Type("symbol: expected string/symbol/keyword".into())),
    };
    Ok(Value::Symbol(s))
}

fn name_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    match &args[0] {
        Value::Str(s) => Ok(Value::Str(Arc::clone(s))),
        Value::Keyword(s) => Ok(Value::Str(Arc::clone(s))),
        Value::Symbol(s) => Ok(Value::Str(Arc::clone(s))),
        _ => Err(Error::Type("name: expected string/keyword/symbol".into())),
    }
}

fn hash_map_fn(args: &[Value]) -> Result<Value> {
    if args.len() % 2 != 0 {
        return Err(Error::Eval("hash-map: even number of args required".into()));
    }
    let mut out = imbl::HashMap::new();
    let mut i = 0;
    while i < args.len() {
        out.insert(args[i].clone(), args[i + 1].clone());
        i += 2;
    }
    Ok(Value::Map(out))
}

fn hash_set_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Set(args.iter().cloned().collect()))
}

fn set_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    Ok(Value::Set(seq_items(&args[0])?.into_iter().collect()))
}

fn into_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let items = seq_items(&args[1])?;
    match &args[0] {
        Value::Vector(v) => {
            let mut out = v.clone();
            for item in items {
                out.push_back(item);
            }
            Ok(Value::Vector(out))
        }
        Value::List(_) | Value::Nil => {
            let mut out: Vec<Value> = match &args[0] {
                Value::List(v) => (**v).clone(),
                _ => Vec::new(),
            };
            for item in items {
                out.insert(0, item);
            }
            Ok(Value::List(Arc::new(out)))
        }
        Value::Set(s) => {
            let mut out = s.clone();
            for item in items {
                out.insert(item);
            }
            Ok(Value::Set(out))
        }
        Value::Map(m) => {
            let mut out = m.clone();
            for item in items {
                match item {
                    Value::Vector(pair) if pair.len() == 2 => {
                        out.insert(pair[0].clone(), pair[1].clone());
                    }
                    _ => return Err(Error::Type("into map: items must be [k v]".into())),
                }
            }
            Ok(Value::Map(out))
        }
        _ => Err(Error::Type(format!("into: bad target {}", args[0].type_name()))),
    }
}

fn reverse_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    let mut items = seq_items(&args[0])?;
    items.reverse();
    Ok(Value::List(Arc::new(items)))
}

fn sort_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    let mut items = seq_items(&args[0])?;
    items.sort_by(|a, b| {
        let av = as_f64(a).unwrap_or(f64::NAN);
        let bv = as_f64(b).unwrap_or(f64::NAN);
        av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(Value::List(Arc::new(items)))
}

fn second_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    let items = seq_items(&args[0])?;
    Ok(items.get(1).cloned().unwrap_or(Value::Nil))
}

fn last_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    let items = seq_items(&args[0])?;
    Ok(items.last().cloned().unwrap_or(Value::Nil))
}

fn apply_builtin(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(Error::Arity { expected: ">= 2".into(), got: args.len() });
    }
    let f = &args[0];
    let mut flat: Vec<Value> = Vec::new();
    for a in &args[1..args.len() - 1] {
        flat.push(a.clone());
    }
    flat.extend(seq_items(&args[args.len() - 1])?);
    eval::apply(f, &flat)
}

// ---- Strings -----------------------------------------------------------

fn as_str(v: &Value) -> Result<&str> {
    match v {
        Value::Str(s) => Ok(s.as_ref()),
        _ => Err(Error::Type(format!("expected string, got {}", v.type_name()))),
    }
}

fn subs_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 && args.len() != 3 {
        return Err(Error::Arity { expected: "2 or 3".into(), got: args.len() });
    }
    let s = as_str(&args[0])?;
    let start = to_i64(&args[1])?.max(0) as usize;
    let chars: Vec<char> = s.chars().collect();
    let end = match args.get(2) {
        Some(v) => to_i64(v)?.max(0) as usize,
        None => chars.len(),
    };
    if start > chars.len() || end > chars.len() || start > end {
        return Err(Error::Eval(format!(
            "subs: range {start}..{end} out of bounds for length {}",
            chars.len()
        )));
    }
    let out: String = chars[start..end].iter().collect();
    Ok(Value::Str(Arc::from(out.as_str())))
}

fn str_split_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let s = as_str(&args[0])?;
    // Accept either a string separator or a compiled Regex literal —
    // Clojure's clojure.string/split takes a regex; cljrs has been
    // string-only until now.
    let parts: Vec<Value> = match &args[1] {
        Value::Regex(re) => re
            .split(s)
            .map(|p| Value::Str(Arc::from(p)))
            .collect(),
        _ => s
            .split(as_str(&args[1])?)
            .map(|p| Value::Str(Arc::from(p)))
            .collect(),
    };
    Ok(Value::Vector(parts.into_iter().collect()))
}

fn str_join_fn(args: &[Value]) -> Result<Value> {
    let (sep, coll) = match args.len() {
        1 => ("", &args[0]),
        2 => (as_str(&args[0])?, &args[1]),
        n => return Err(Error::Arity { expected: "1 or 2".into(), got: n }),
    };
    let items = seq_items(coll)?;
    let parts: Vec<String> = items.iter().map(Value::to_display_string).collect();
    Ok(Value::Str(Arc::from(parts.join(sep).as_str())))
}

fn str_upper_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    Ok(Value::Str(Arc::from(as_str(&args[0])?.to_uppercase().as_str())))
}
fn str_lower_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity { expected: "1".into(), got: args.len() });
    }
    Ok(Value::Str(Arc::from(as_str(&args[0])?.to_lowercase().as_str())))
}

fn str_replace_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 3 {
        return Err(Error::Arity { expected: "3".into(), got: args.len() });
    }
    let s = as_str(&args[0])?;
    let from = as_str(&args[1])?;
    let to = as_str(&args[2])?;
    Ok(Value::Str(Arc::from(s.replace(from, to).as_str())))
}

fn str_starts_with_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(as_str(&args[0])?.starts_with(as_str(&args[1])?)))
}
fn str_ends_with_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(as_str(&args[0])?.ends_with(as_str(&args[1])?)))
}
fn str_includes_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(as_str(&args[0])?.contains(as_str(&args[1])?)))
}
fn str_trim_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Str(Arc::from(as_str(&args[0])?.trim())))
}
fn str_blank_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(match &args[0] {
        Value::Nil => true,
        Value::Str(s) => s.trim().is_empty(),
        _ => false,
    }))
}
fn str_index_of_fn(args: &[Value]) -> Result<Value> {
    let s = as_str(&args[0])?;
    let needle = as_str(&args[1])?;
    let start = match args.get(2) {
        Some(Value::Int(i)) => (*i).max(0) as usize,
        Some(_) => return Err(Error::Type("str/index-of: from-index must be int".into())),
        None => 0,
    };
    if start > s.len() {
        return Ok(Value::Nil);
    }
    Ok(match s[start..].find(needle) {
        Some(off) => Value::Int((start + off) as i64),
        None => Value::Nil,
    })
}
fn str_last_index_of_fn(args: &[Value]) -> Result<Value> {
    let s = as_str(&args[0])?;
    let needle = as_str(&args[1])?;
    Ok(match s.rfind(needle) {
        Some(off) => Value::Int(off as i64),
        None => Value::Nil,
    })
}

// ---- Type predicates ---------------------------------------------------

fn string_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(matches!(&args[0], Value::Str(_)))) }
fn number_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Int(_) | Value::Float(_))))
}
fn integer_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(matches!(&args[0], Value::Int(_)))) }
fn float_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(matches!(&args[0], Value::Float(_)))) }
fn map_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(matches!(&args[0], Value::Map(_)))) }
fn vector_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(matches!(&args[0], Value::Vector(_)))) }
fn set_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(matches!(&args[0], Value::Set(_)))) }
fn list_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(matches!(&args[0], Value::List(_)))) }
fn seq_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(
        &args[0],
        Value::List(_) | Value::Vector(_) | Value::Set(_) | Value::Map(_)
    )))
}
fn coll_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(
        &args[0],
        Value::List(_) | Value::Vector(_) | Value::Set(_) | Value::Map(_)
    )))
}
fn keyword_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(matches!(&args[0], Value::Keyword(_)))) }
fn symbol_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(matches!(&args[0], Value::Symbol(_)))) }
fn fn_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(
        &args[0],
        Value::Fn(_) | Value::Macro(_) | Value::Builtin(_) | Value::Native(_)
    )))
}
fn boolean_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(matches!(&args[0], Value::Bool(_)))) }
fn true_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(matches!(&args[0], Value::Bool(true)))) }
fn false_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(matches!(&args[0], Value::Bool(false)))) }
fn some_q(args: &[Value]) -> Result<Value> { Ok(Value::Bool(!matches!(&args[0], Value::Nil))) }

fn some_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let coll = seq_items(&args[1])?;
    for item in coll {
        let v = eval::apply(&args[0], std::slice::from_ref(&item))?;
        if v.truthy() {
            return Ok(v);
        }
    }
    Ok(Value::Nil)
}

fn every_q(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let coll = seq_items(&args[1])?;
    for item in coll {
        let v = eval::apply(&args[0], std::slice::from_ref(&item))?;
        if !v.truthy() {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

fn not_empty_fn(args: &[Value]) -> Result<Value> {
    let is_empty = match &args[0] {
        Value::Nil => true,
        Value::List(v) => v.is_empty(),
        Value::Vector(v) => v.is_empty(),
        Value::Map(m) => m.is_empty(),
        Value::Set(s) => s.is_empty(),
        Value::Str(s) => s.is_empty(),
        _ => false,
    };
    Ok(if is_empty { Value::Nil } else { args[0].clone() })
}

// ---- Math --------------------------------------------------------------

fn mod_fn(args: &[Value]) -> Result<Value> {
    let a = as_f64(&args[0])?;
    let b = as_f64(&args[1])?;
    let r = a - b * (a / b).floor();
    match (&args[0], &args[1]) {
        (Value::Int(_), Value::Int(_)) => Ok(Value::Int(r as i64)),
        _ => Ok(Value::Float(r)),
    }
}
fn rem_fn(args: &[Value]) -> Result<Value> {
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),
        _ => Ok(Value::Float(as_f64(&args[0])? % as_f64(&args[1])?)),
    }
}
fn quot_fn(args: &[Value]) -> Result<Value> {
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
        _ => Ok(Value::Float((as_f64(&args[0])? / as_f64(&args[1])?).trunc())),
    }
}

fn min_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Arity { expected: ">= 1".into(), got: 0 });
    }
    let mut best = args[0].clone();
    for a in &args[1..] {
        if as_f64(a)? < as_f64(&best)? {
            best = a.clone();
        }
    }
    Ok(best)
}
fn max_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Arity { expected: ">= 1".into(), got: 0 });
    }
    let mut best = args[0].clone();
    for a in &args[1..] {
        if as_f64(a)? > as_f64(&best)? {
            best = a.clone();
        }
    }
    Ok(best)
}

fn abs_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Int(i) => Ok(Value::Int(i.abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        _ => Err(Error::Type("abs on non-number".into())),
    }
}

fn sqrt_fn(args: &[Value]) -> Result<Value> { Ok(Value::Float(as_f64(&args[0])?.sqrt())) }
fn pow_fn(args: &[Value]) -> Result<Value> { Ok(Value::Float(as_f64(&args[0])?.powf(as_f64(&args[1])?))) }
fn sin_fn(args: &[Value]) -> Result<Value> { Ok(Value::Float(as_f64(&args[0])?.sin())) }
fn cos_fn(args: &[Value]) -> Result<Value> { Ok(Value::Float(as_f64(&args[0])?.cos())) }
fn tan_fn(args: &[Value]) -> Result<Value> { Ok(Value::Float(as_f64(&args[0])?.tan())) }
fn exp_fn(args: &[Value]) -> Result<Value> { Ok(Value::Float(as_f64(&args[0])?.exp())) }
fn log_fn(args: &[Value]) -> Result<Value> { Ok(Value::Float(as_f64(&args[0])?.ln())) }
fn floor_fn(args: &[Value]) -> Result<Value> { Ok(Value::Float(as_f64(&args[0])?.floor())) }
fn ceil_fn(args: &[Value]) -> Result<Value> { Ok(Value::Float(as_f64(&args[0])?.ceil())) }
fn round_fn(args: &[Value]) -> Result<Value> { Ok(Value::Int(as_f64(&args[0])?.round() as i64)) }
fn int_fn(args: &[Value]) -> Result<Value> {
    // Truncation toward zero (Clojure's `int` / `long` semantics on
    // floats; ints pass through). Strings parse if numeric.
    Ok(match &args[0] {
        Value::Int(i) => Value::Int(*i),
        Value::Float(f) => Value::Int(*f as i64),
        Value::Ratio(n, d) => Value::Int(*n / *d),
        Value::Bool(b) => Value::Int(if *b { 1 } else { 0 }),
        Value::Str(s) => match s.parse::<i64>() {
            Ok(i) => Value::Int(i),
            Err(_) => match s.parse::<f64>() {
                Ok(f) => Value::Int(f as i64),
                Err(_) => return Err(Error::Type(format!("int: cannot parse {s:?}"))),
            },
        },
        v => return Err(Error::Type(format!("int: cannot convert {}", v.type_name()))),
    })
}
fn double_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Float(as_f64(&args[0])?))
}

// ---- Seq utilities -----------------------------------------------------

fn repeat_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let n = to_i64(&args[0])?.max(0) as usize;
    let out: Vec<Value> = std::iter::repeat(args[1].clone()).take(n).collect();
    Ok(Value::List(Arc::new(out)))
}

fn take_while_fn(args: &[Value]) -> Result<Value> {
    let pred = &args[0];
    let coll = seq_items(&args[1])?;
    let mut out = Vec::new();
    for item in coll {
        let keep = eval::apply(pred, std::slice::from_ref(&item))?;
        if !keep.truthy() { break; }
        out.push(item);
    }
    Ok(Value::List(Arc::new(out)))
}
fn drop_while_fn(args: &[Value]) -> Result<Value> {
    let pred = &args[0];
    let coll = seq_items(&args[1])?;
    let mut out = Vec::new();
    let mut dropping = true;
    for item in coll {
        if dropping {
            let keep = eval::apply(pred, std::slice::from_ref(&item))?;
            if !keep.truthy() {
                dropping = false;
                out.push(item);
            }
        } else {
            out.push(item);
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn partition_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let n = to_i64(&args[0])?.max(1) as usize;
    let coll = seq_items(&args[1])?;
    let mut out = Vec::new();
    for chunk in coll.chunks(n) {
        if chunk.len() == n {
            out.push(Value::List(Arc::new(chunk.to_vec())));
        }
    }
    Ok(Value::List(Arc::new(out)))
}
fn interleave_fn(args: &[Value]) -> Result<Value> {
    let colls: Vec<Vec<Value>> = args.iter().map(seq_items).collect::<Result<_>>()?;
    let min_len = colls.iter().map(|c| c.len()).min().unwrap_or(0);
    let mut out = Vec::with_capacity(min_len * colls.len());
    for i in 0..min_len {
        for c in &colls {
            out.push(c[i].clone());
        }
    }
    Ok(Value::List(Arc::new(out)))
}
fn interpose_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let sep = &args[0];
    let coll = seq_items(&args[1])?;
    let mut out = Vec::new();
    for (i, item) in coll.into_iter().enumerate() {
        if i > 0 { out.push(sep.clone()); }
        out.push(item);
    }
    Ok(Value::List(Arc::new(out)))
}

fn frequencies_fn(args: &[Value]) -> Result<Value> {
    let coll = seq_items(&args[0])?;
    let mut out: imbl::HashMap<Value, Value> = imbl::HashMap::new();
    for item in coll {
        let cur = out.get(&item).and_then(|v| match v { Value::Int(i) => Some(*i), _ => None }).unwrap_or(0);
        out.insert(item, Value::Int(cur + 1));
    }
    Ok(Value::Map(out))
}
fn group_by_fn(args: &[Value]) -> Result<Value> {
    let f = &args[0];
    let coll = seq_items(&args[1])?;
    let mut out: imbl::HashMap<Value, imbl::Vector<Value>> = imbl::HashMap::new();
    for item in coll {
        let k = eval::apply(f, std::slice::from_ref(&item))?;
        out.entry(k).or_default().push_back(item);
    }
    let mapped: imbl::HashMap<Value, Value> = out.into_iter().map(|(k, v)| (k, Value::Vector(v))).collect();
    Ok(Value::Map(mapped))
}
fn distinct_fn(args: &[Value]) -> Result<Value> {
    let coll = seq_items(&args[0])?;
    let mut seen: imbl::HashSet<Value> = imbl::HashSet::new();
    let mut out = Vec::new();
    for item in coll {
        if !seen.contains(&item) {
            seen.insert(item.clone());
            out.push(item);
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn mapv_fn(args: &[Value]) -> Result<Value> {
    let f = &args[0];
    let coll = seq_items(&args[1])?;
    let mut out: imbl::Vector<Value> = imbl::Vector::new();
    for item in coll {
        out.push_back(eval::apply(f, std::slice::from_ref(&item))?);
    }
    Ok(Value::Vector(out))
}
fn filterv_fn(args: &[Value]) -> Result<Value> {
    let pred = &args[0];
    let coll = seq_items(&args[1])?;
    let mut out: imbl::Vector<Value> = imbl::Vector::new();
    for item in coll {
        let keep = eval::apply(pred, std::slice::from_ref(&item))?;
        if keep.truthy() {
            out.push_back(item);
        }
    }
    Ok(Value::Vector(out))
}
fn reduce_kv_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 3 {
        return Err(Error::Arity { expected: "3".into(), got: args.len() });
    }
    let f = &args[0];
    let mut acc = args[1].clone();
    match &args[2] {
        Value::Map(m) => {
            for (k, v) in m.iter() {
                acc = eval::apply(f, &[acc, k.clone(), v.clone()])?;
            }
        }
        Value::Vector(v) => {
            for (i, item) in v.iter().enumerate() {
                acc = eval::apply(f, &[acc, Value::Int(i as i64), item.clone()])?;
            }
        }
        _ => return Err(Error::Type("reduce-kv: expects map or vector".into())),
    }
    Ok(acc)
}

fn get_in_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 && args.len() != 3 {
        return Err(Error::Arity { expected: "2 or 3".into(), got: args.len() });
    }
    let ks = seq_items(&args[1])?;
    let mut cur = args[0].clone();
    let default = args.get(2).cloned().unwrap_or(Value::Nil);
    for k in ks {
        cur = get_fn(&[cur, k])?;
        if matches!(cur, Value::Nil) {
            return Ok(default);
        }
    }
    Ok(cur)
}
fn assoc_in_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 3 {
        return Err(Error::Arity { expected: "3".into(), got: args.len() });
    }
    let ks = seq_items(&args[1])?;
    fn helper(coll: Value, ks: &[Value], v: Value) -> Result<Value> {
        if ks.len() == 1 {
            return assoc_fn(&[coll, ks[0].clone(), v]);
        }
        let inner = get_fn(&[coll.clone(), ks[0].clone()])?;
        let inner = match inner { Value::Nil => Value::Map(imbl::HashMap::new()), x => x };
        let new_inner = helper(inner, &ks[1..], v)?;
        assoc_fn(&[coll, ks[0].clone(), new_inner])
    }
    helper(args[0].clone(), &ks, args[2].clone())
}
fn update_in_fn(args: &[Value]) -> Result<Value> {
    if args.len() < 3 {
        return Err(Error::Arity { expected: ">= 3".into(), got: args.len() });
    }
    let ks = seq_items(&args[1])?;
    let f = &args[2];
    let extra = &args[3..];
    let cur = get_in_fn(&[args[0].clone(), args[1].clone()])?;
    let mut fargs = Vec::with_capacity(1 + extra.len());
    fargs.push(cur);
    fargs.extend_from_slice(extra);
    let new_v = eval::apply(f, &fargs)?;
    let mut assoc_args: Vec<Value> = Vec::with_capacity(3);
    assoc_args.push(args[0].clone());
    assoc_args.push(Value::Vector(ks.into_iter().collect()));
    assoc_args.push(new_v);
    assoc_in_fn(&assoc_args)
}

// ---- Function-builders --------------------------------------------------

fn comp_fn(args: &[Value]) -> Result<Value> {
    let fs: Vec<Value> = args.to_vec();
    Ok(Value::Builtin(Builtin::new_closure("comp-result", move |call_args| {
        if fs.is_empty() {
            return Ok(call_args.first().cloned().unwrap_or(Value::Nil));
        }
        let last_idx = fs.len() - 1;
        let mut acc = eval::apply(&fs[last_idx], call_args)?;
        for i in (0..last_idx).rev() {
            acc = eval::apply(&fs[i], &[acc])?;
        }
        Ok(acc)
    })))
}
fn partial_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Arity { expected: ">= 1".into(), got: 0 });
    }
    let f = args[0].clone();
    let pre: Vec<Value> = args[1..].to_vec();
    Ok(Value::Builtin(Builtin::new_closure("partial-result", move |call_args| {
        let mut all = pre.clone();
        all.extend_from_slice(call_args);
        eval::apply(&f, &all)
    })))
}
fn complement_fn(args: &[Value]) -> Result<Value> {
    let f = args[0].clone();
    Ok(Value::Builtin(Builtin::new_closure("complement-result", move |call_args| {
        let v = eval::apply(&f, call_args)?;
        Ok(Value::Bool(!v.truthy()))
    })))
}
fn juxt_fn(args: &[Value]) -> Result<Value> {
    let fs: Vec<Value> = args.to_vec();
    Ok(Value::Builtin(Builtin::new_closure("juxt-result", move |call_args| {
        let mut out: imbl::Vector<Value> = imbl::Vector::new();
        for f in &fs {
            out.push_back(eval::apply(f, call_args)?);
        }
        Ok(Value::Vector(out))
    })))
}
fn constantly_fn(args: &[Value]) -> Result<Value> {
    let v = args[0].clone();
    Ok(Value::Builtin(Builtin::new_closure("constantly-result", move |_| Ok(v.clone()))))
}

// ---- I/O + printing ----------------------------------------------------

fn print_fn(args: &[Value]) -> Result<Value> {
    let mut first = true;
    for a in args {
        if !first { print!(" "); }
        first = false;
        print!("{}", a.to_display_string());
    }
    Ok(Value::Nil)
}
fn print_str_fn(args: &[Value]) -> Result<Value> {
    let parts: Vec<String> = args.iter().map(Value::to_display_string).collect();
    Ok(Value::Str(Arc::from(parts.join(" ").as_str())))
}
fn println_str_fn(args: &[Value]) -> Result<Value> {
    let parts: Vec<String> = args.iter().map(Value::to_display_string).collect();
    let mut s = parts.join(" ");
    s.push('\n');
    Ok(Value::Str(Arc::from(s.as_str())))
}
fn slurp_fn(args: &[Value]) -> Result<Value> {
    let path = as_str(&args[0])?;
    let s = std::fs::read_to_string(path).map_err(|e| Error::Eval(format!("slurp: {e}")))?;
    Ok(Value::Str(Arc::from(s.as_str())))
}
fn spit_fn(args: &[Value]) -> Result<Value> {
    let path = as_str(&args[0])?;
    let content = args[1].to_display_string();
    std::fs::write(path, content).map_err(|e| Error::Eval(format!("spit: {e}")))?;
    Ok(Value::Nil)
}
fn read_string_fn(args: &[Value]) -> Result<Value> {
    let s = as_str(&args[0])?;
    crate::reader::read_one(s)
}

#[derive(Clone, Copy, Debug)]
enum Num {
    I(i64),
    F(f64),
    R(i64, i64), // numerator, denominator (denom > 0, reduced)
}

fn to_num(v: &Value) -> Result<Num> {
    match v {
        Value::Int(i) => Ok(Num::I(*i)),
        Value::Float(f) => Ok(Num::F(*f)),
        Value::Ratio(n, d) => Ok(Num::R(*n, *d)),
        _ => Err(Error::Type(format!(
            "expected number, got {}",
            v.type_name()
        ))),
    }
}

fn num_to_value(n: Num) -> Value {
    match n {
        Num::I(i) => Value::Int(i),
        Num::F(f) => Value::Float(f),
        Num::R(n, d) => crate::reader::reduce_ratio(n, d),
    }
}

fn ratio_to_f64(n: i64, d: i64) -> f64 {
    n as f64 / d as f64
}

/// Numeric op kind. Used by fold_num to promote/combine across
/// int/float/ratio without hard-coding a single + behavior.
#[derive(Copy, Clone)]
enum NumOp { Add, Sub, Mul, Div }

fn ratio_combine(op: NumOp, an: i64, ad: i64, bn: i64, bd: i64) -> Num {
    match op {
        NumOp::Add => Num::R(an * bd + bn * ad, ad * bd),
        NumOp::Sub => Num::R(an * bd - bn * ad, ad * bd),
        NumOp::Mul => Num::R(an * bn, ad * bd),
        NumOp::Div => Num::R(an * bd, ad * bn),
    }
}

fn fold_num(args: &[Value], init: Num, op: NumOp) -> Result<Value> {
    let mut acc = init;
    for a in args {
        let n = to_num(a)?;
        acc = match (acc, n) {
            (Num::F(x), other) => Num::F(apply_float(op, x, other.as_f64())),
            (other, Num::F(y)) => Num::F(apply_float(op, other.as_f64(), y)),
            (Num::I(x), Num::I(y)) => match op {
                NumOp::Add => x.checked_add(y).map(Num::I).unwrap_or(Num::F((x as f64) + (y as f64))),
                NumOp::Sub => x.checked_sub(y).map(Num::I).unwrap_or(Num::F((x as f64) - (y as f64))),
                NumOp::Mul => x.checked_mul(y).map(Num::I).unwrap_or(Num::F((x as f64) * (y as f64))),
                NumOp::Div => {
                    if y == 0 {
                        return Err(Error::Eval("integer divide by zero".into()));
                    }
                    if x % y == 0 { Num::I(x / y) } else { ratio_combine(NumOp::Div, x, 1, y, 1) }
                }
            },
            (Num::I(x), Num::R(yn, yd)) => ratio_combine(op, x, 1, yn, yd),
            (Num::R(xn, xd), Num::I(y)) => ratio_combine(op, xn, xd, y, 1),
            (Num::R(xn, xd), Num::R(yn, yd)) => ratio_combine(op, xn, xd, yn, yd),
        };
    }
    Ok(num_to_value(acc))
}

fn apply_float(op: NumOp, a: f64, b: f64) -> f64 {
    match op {
        NumOp::Add => a + b,
        NumOp::Sub => a - b,
        NumOp::Mul => a * b,
        NumOp::Div => a / b,
    }
}

impl Num {
    fn as_f64(self) -> f64 {
        match self {
            Num::I(i) => i as f64,
            Num::F(f) => f,
            Num::R(n, d) => ratio_to_f64(n, d),
        }
    }
}

fn add(args: &[Value]) -> Result<Value> {
    fold_num(args, Num::I(0), NumOp::Add)
}

fn sub(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Arity { expected: ">= 1".into(), got: 0 });
    }
    if args.len() == 1 {
        return match to_num(&args[0])? {
            Num::I(i) => Ok(Value::Int(-i)),
            Num::F(f) => Ok(Value::Float(-f)),
            Num::R(n, d) => Ok(crate::reader::reduce_ratio(-n, d)),
        };
    }
    let first = to_num(&args[0])?;
    fold_num(&args[1..], first, NumOp::Sub)
}

fn mul(args: &[Value]) -> Result<Value> {
    fold_num(args, Num::I(1), NumOp::Mul)
}

fn div(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Arity { expected: ">= 1".into(), got: 0 });
    }
    if args.len() == 1 {
        return match to_num(&args[0])? {
            Num::I(i) if i != 0 => Ok(crate::reader::reduce_ratio(1, i)),
            Num::F(f) => Ok(Value::Float(1.0 / f)),
            Num::R(n, d) => Ok(crate::reader::reduce_ratio(d, n)),
            Num::I(_) => Err(Error::Eval("integer divide by zero".into())),
        };
    }
    let first = to_num(&args[0])?;
    fold_num(&args[1..], first, NumOp::Div)
}

fn eq(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Ok(Value::Bool(true));
    }
    let first = &args[0];
    for a in &args[1..] {
        if a != first {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

fn cmp<F>(args: &[Value], f: F) -> Result<Value>
where
    F: Fn(f64, f64) -> bool,
{
    if args.len() < 2 {
        return Ok(Value::Bool(true));
    }
    let mut prev = as_f64(&args[0])?;
    for a in &args[1..] {
        let cur = as_f64(a)?;
        if !f(prev, cur) {
            return Ok(Value::Bool(false));
        }
        prev = cur;
    }
    Ok(Value::Bool(true))
}

fn as_f64(v: &Value) -> Result<f64> {
    match to_num(v)? {
        Num::I(i) => Ok(i as f64),
        Num::F(x) => Ok(x),
        Num::R(n, d) => Ok(n as f64 / d as f64),
    }
}

fn lt(args: &[Value]) -> Result<Value> {
    cmp(args, |a, b| a < b)
}
fn gt(args: &[Value]) -> Result<Value> {
    cmp(args, |a, b| a > b)
}
fn le(args: &[Value]) -> Result<Value> {
    cmp(args, |a, b| a <= b)
}
fn ge(args: &[Value]) -> Result<Value> {
    cmp(args, |a, b| a >= b)
}

fn subvec_fn(args: &[Value]) -> Result<Value> {
    // (subvec v start) / (subvec v start end). Returns a fresh
    // imbl::Vector slice. Out-of-range = error, matching Clojure.
    if args.len() < 2 || args.len() > 3 {
        return Err(Error::Arity { expected: "2 or 3".into(), got: args.len() });
    }
    let v = match &args[0] {
        Value::Vector(xs) => xs,
        other => return Err(Error::Type(format!(
            "subvec: first arg must be a vector, got {}", other.type_name()))),
    };
    let len = v.len();
    let start = match &args[1] {
        Value::Int(i) if *i >= 0 => *i as usize,
        _ => return Err(Error::Type("subvec: start must be non-neg int".into())),
    };
    let end = match args.get(2) {
        Some(Value::Int(i)) if *i >= 0 => *i as usize,
        Some(_) => return Err(Error::Type("subvec: end must be non-neg int".into())),
        None => len,
    };
    if start > len || end > len || start > end {
        return Err(Error::Eval(format!(
            "subvec: range {start}..{end} out of bounds for length {len}")));
    }
    let mut out: imbl::Vector<Value> = imbl::Vector::new();
    for i in start..end {
        out.push_back(v[i].clone());
    }
    Ok(Value::Vector(out))
}

fn not_eq_fn(args: &[Value]) -> Result<Value> {
    // (not= ...) — variadic. Returns true unless every arg is = to the
    // first. Mirrors Clojure's spec: (not= a b c) ≡ (not (= a b c)).
    if args.is_empty() {
        return Err(Error::Arity { expected: ">=1".into(), got: 0 });
    }
    let first = &args[0];
    for a in &args[1..] {
        if first != a { return Ok(Value::Bool(true)); }
    }
    Ok(Value::Bool(false))
}

fn not_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    Ok(Value::Bool(!args[0].truthy()))
}

fn to_str(args: &[Value]) -> Result<Value> {
    let mut s = String::new();
    for a in args {
        if !matches!(a, Value::Nil) {
            s.push_str(&a.to_display_string());
        }
    }
    Ok(Value::Str(Arc::from(s.as_str())))
}

fn println_fn(args: &[Value]) -> Result<Value> {
    let mut first = true;
    for a in args {
        if !first {
            print!(" ");
        }
        first = false;
        print!("{}", a.to_display_string());
    }
    println!();
    Ok(Value::Nil)
}

fn pr_str_fn(args: &[Value]) -> Result<Value> {
    let parts: Vec<String> = args.iter().map(Value::to_pr_string).collect();
    Ok(Value::Str(Arc::from(parts.join(" ").as_str())))
}

fn count_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    let n = match &args[0] {
        Value::Nil => 0,
        Value::List(v) => v.len(),
        Value::Vector(v) => v.len(),
        Value::Map(m) => m.len(),
        Value::Set(s) => s.len(),
        Value::Str(s) => s.chars().count(),
        _ => {
            return Err(Error::Type(format!(
                "count on non-sequence: {}",
                args[0].type_name()
            )));
        }
    };
    Ok(Value::Int(n as i64))
}

fn first_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    let v = resolve_seq(&args[0])?;
    match &v {
        Value::Nil => Ok(Value::Nil),
        Value::List(v) => Ok(v.first().cloned().unwrap_or(Value::Nil)),
        Value::Vector(v) => Ok(v.front().cloned().unwrap_or(Value::Nil)),
        Value::Set(s) => Ok(s.iter().next().cloned().unwrap_or(Value::Nil)),
        Value::Cons(h, _) => Ok((**h).clone()),
        _ => Err(Error::Type(format!(
            "first on non-sequence: {}",
            v.type_name()
        ))),
    }
}

fn rest_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    let v = resolve_seq(&args[0])?;
    match &v {
        Value::Nil => Ok(Value::List(Arc::new(Vec::new()))),
        Value::List(v) => Ok(if v.is_empty() {
            Value::List(Arc::new(Vec::new()))
        } else {
            Value::List(Arc::new(v[1..].to_vec()))
        }),
        Value::Vector(v) => {
            let items: Vec<Value> = v.iter().skip(1).cloned().collect();
            Ok(Value::List(Arc::new(items)))
        }
        Value::Cons(_, t) => Ok((**t).clone()),
        _ => Err(Error::Type(format!(
            "rest on non-sequence: {}",
            v.type_name()
        ))),
    }
}

fn cons_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity {
            expected: "2".into(),
            got: args.len(),
        });
    }
    // For lazy tails / cons cells, return another Cons cell so the
    // tail isn't forced. For eager collections, prepend into a list.
    match &args[1] {
        Value::LazySeq(_) | Value::Cons(_, _) => Ok(Value::Cons(
            Arc::new(args[0].clone()),
            Arc::new(args[1].clone()),
        )),
        _ => {
            let mut out = Vec::new();
            out.push(args[0].clone());
            out.extend(seq_items(&args[1])?);
            Ok(Value::List(Arc::new(out)))
        }
    }
}

fn list_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::List(Arc::new(args.to_vec())))
}

fn concat_fn(args: &[Value]) -> Result<Value> {
    let mut out: Vec<Value> = Vec::new();
    for a in args {
        out.extend(seq_items(a)?);
    }
    Ok(Value::List(Arc::new(out)))
}

fn vector_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Vector(args.iter().cloned().collect()))
}

fn conj_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Arity {
            expected: ">= 1".into(),
            got: 0,
        });
    }
    match &args[0] {
        Value::Nil => {
            let mut out: Vec<Value> = Vec::new();
            for a in &args[1..] {
                out.insert(0, a.clone());
            }
            Ok(Value::List(Arc::new(out)))
        }
        Value::Vector(v) => {
            let mut out = v.clone();
            for a in &args[1..] {
                out.push_back(a.clone());
            }
            Ok(Value::Vector(out))
        }
        Value::List(v) => {
            let mut out = (**v).clone();
            for a in &args[1..] {
                out.insert(0, a.clone());
            }
            Ok(Value::List(Arc::new(out)))
        }
        Value::Set(s) => {
            let mut out = s.clone();
            for a in &args[1..] {
                out.insert(a.clone());
            }
            Ok(Value::Set(out))
        }
        Value::Map(m) => {
            // conj onto a map: each extra arg must be a 2-vector [k v] or a map-entry-like.
            let mut out = m.clone();
            for a in &args[1..] {
                match a {
                    Value::Vector(pair) if pair.len() == 2 => {
                        out.insert(pair[0].clone(), pair[1].clone());
                    }
                    Value::Map(sub) => {
                        for (k, v) in sub.iter() {
                            out.insert(k.clone(), v.clone());
                        }
                    }
                    _ => {
                        return Err(Error::Type(
                            "conj onto map expects [k v] vectors or maps".into(),
                        ));
                    }
                }
            }
            Ok(Value::Map(out))
        }
        _ => Err(Error::Type(format!("conj onto {}", args[0].type_name()))),
    }
}

fn nth_fn(args: &[Value]) -> Result<Value> {
    if args.len() < 2 || args.len() > 3 {
        return Err(Error::Arity {
            expected: "2 or 3".into(),
            got: args.len(),
        });
    }
    let default = args.get(2).cloned();
    let idx = match &args[1] {
        Value::Int(i) => *i,
        _ => return Err(Error::Type("nth index must be int".into())),
    };
    if idx < 0 {
        return match default {
            Some(d) => Ok(d),
            None => Err(Error::Eval("nth: negative index".into())),
        };
    }
    let i = idx as usize;
    let oob = || match default.clone() {
        Some(d) => Ok(d),
        None => Err(Error::Eval(format!("nth: index {idx} out of range"))),
    };
    match &args[0] {
        Value::List(v) => v.get(i).cloned().map_or_else(oob, Ok),
        Value::Vector(v) => v.get(i).cloned().map_or_else(oob, Ok),
        _ => Err(Error::Type("nth on non-sequence".into())),
    }
}

fn vec_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    match &args[0] {
        Value::Nil => Ok(Value::Vector(PVec::new())),
        Value::List(v) => Ok(Value::Vector(v.iter().cloned().collect())),
        Value::Vector(v) => Ok(Value::Vector(v.clone())),
        Value::Set(s) => Ok(Value::Vector(s.iter().cloned().collect())),
        _ => Err(Error::Type("vec on non-sequence".into())),
    }
}

fn nil_q(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    Ok(Value::Bool(matches!(args[0], Value::Nil)))
}

fn zero_q(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    Ok(Value::Bool(match &args[0] {
        Value::Int(0) => true,
        Value::Float(f) if *f == 0.0 => true,
        _ => false,
    }))
}

fn empty_q(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    let v = resolve_seq(&args[0])?;
    Ok(Value::Bool(match &v {
        Value::Nil => true,
        Value::List(v) => v.is_empty(),
        Value::Vector(v) => v.is_empty(),
        Value::Map(m) => m.is_empty(),
        Value::Set(s) => s.is_empty(),
        Value::Str(s) => s.is_empty(),
        Value::Cons(_, _) => false,
        _ => false,
    }))
}

fn inc_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    match to_num(&args[0])? {
        Num::I(i) => Ok(i.checked_add(1).map(Value::Int).unwrap_or_else(|| Value::Float(i as f64 + 1.0))),
        Num::F(f) => Ok(Value::Float(f + 1.0)),
        Num::R(n, d) => Ok(crate::reader::reduce_ratio(n + d, d)),
    }
}

fn dec_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    match to_num(&args[0])? {
        Num::I(i) => Ok(i.checked_sub(1).map(Value::Int).unwrap_or_else(|| Value::Float(i as f64 - 1.0))),
        Num::F(f) => Ok(Value::Float(f - 1.0)),
        Num::R(n, d) => Ok(crate::reader::reduce_ratio(n - d, d)),
    }
}

/// Owned flattening wrapper used by the stdlib seq pipeline — avoids the
/// slice-vs-iterator split since imbl::Vector doesn't expose a contiguous
/// slice.
fn as_seq(v: &Value) -> Result<Vec<Value>> {
    seq_items(v)
}

fn map_fn(args: &[Value]) -> Result<Value> {
    if args.len() < 2 {
        return Err(Error::Arity {
            expected: ">= 2".into(),
            got: args.len(),
        });
    }
    let f = &args[0];
    let coll = as_seq(&args[1])?;
    let mut out = Vec::with_capacity(coll.len());
    for item in coll {
        out.push(eval::apply(f, std::slice::from_ref(&item))?);
    }
    Ok(Value::List(Arc::new(out)))
}

fn filter_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity {
            expected: "2".into(),
            got: args.len(),
        });
    }
    let pred = &args[0];
    let coll = as_seq(&args[1])?;
    let mut out = Vec::new();
    for item in coll {
        let keep = eval::apply(pred, std::slice::from_ref(&item))?;
        if keep.truthy() {
            out.push(item.clone());
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn reduce_fn(args: &[Value]) -> Result<Value> {
    match args.len() {
        2 => {
            let f = &args[0];
            let coll = as_seq(&args[1])?;
            if coll.is_empty() {
                return eval::apply(f, &[]);
            }
            let mut acc = coll[0].clone();
            for item in &coll[1..] {
                acc = eval::apply(f, &[acc, item.clone()])?;
                if let Value::Reduced(inner) = &acc {
                    return Ok((**inner).clone());
                }
            }
            Ok(acc)
        }
        3 => {
            let f = &args[0];
            let mut acc = args[1].clone();
            let coll = as_seq(&args[2])?;
            for item in coll {
                acc = eval::apply(f, &[acc, item.clone()])?;
                if let Value::Reduced(inner) = &acc {
                    return Ok((**inner).clone());
                }
            }
            Ok(acc)
        }
        n => Err(Error::Arity {
            expected: "2 or 3".into(),
            got: n,
        }),
    }
}

fn range_fn(args: &[Value]) -> Result<Value> {
    let (start, end, step) = match args.len() {
        1 => (0i64, to_i64(&args[0])?, 1i64),
        2 => (to_i64(&args[0])?, to_i64(&args[1])?, 1i64),
        3 => (to_i64(&args[0])?, to_i64(&args[1])?, to_i64(&args[2])?),
        n => {
            return Err(Error::Arity {
                expected: "1, 2, or 3".into(),
                got: n,
            });
        }
    };
    if step == 0 {
        return Err(Error::Eval("range: step cannot be zero".into()));
    }
    let mut out = Vec::new();
    let mut i = start;
    if step > 0 {
        while i < end {
            out.push(Value::Int(i));
            i += step;
        }
    } else {
        while i > end {
            out.push(Value::Int(i));
            i += step;
        }
    }
    Ok(Value::List(Arc::new(out)))
}

fn to_i64(v: &Value) -> Result<i64> {
    match v {
        Value::Int(n) => Ok(*n),
        Value::Float(f) => Ok(*f as i64),
        _ => Err(Error::Type(format!(
            "expected integer, got {}",
            v.type_name()
        ))),
    }
}

fn take_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity {
            expected: "2".into(),
            got: args.len(),
        });
    }
    let n = to_i64(&args[0])?.max(0) as usize;
    // Walk via first/rest so infinite lazy seqs don't get eagerly
    // flattened. One force per element taken, bounded by n.
    let mut out = Vec::with_capacity(n);
    let mut cur = args[1].clone();
    for _ in 0..n {
        let resolved = resolve_seq(&cur)?;
        let done = match &resolved {
            Value::Nil => true,
            Value::List(v) => v.is_empty(),
            Value::Vector(v) => v.is_empty(),
            Value::Set(s) => s.is_empty(),
            _ => false,
        };
        if done {
            break;
        }
        out.push(first_fn(std::slice::from_ref(&resolved))?);
        cur = rest_fn(std::slice::from_ref(&resolved))?;
    }
    Ok(Value::List(Arc::new(out)))
}

fn drop_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity {
            expected: "2".into(),
            got: args.len(),
        });
    }
    let n = to_i64(&args[0])?.max(0) as usize;
    let coll = as_seq(&args[1])?;
    let dropped: Vec<Value> = coll.iter().skip(n).cloned().collect();
    Ok(Value::List(Arc::new(dropped)))
}

fn even_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(to_i64(&args[0])? % 2 == 0))
}
fn odd_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(to_i64(&args[0])? % 2 != 0))
}
fn pos_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(match &args[0] {
        Value::Int(i) => *i > 0,
        Value::Float(f) => *f > 0.0,
        Value::Ratio(n, d) => (*n > 0) == (*d > 0) && *n != 0,
        _ => {
            return Err(Error::Type(format!(
                "pos?: expected number, got {}",
                args[0].type_name()
            )));
        }
    }))
}
fn neg_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(match &args[0] {
        Value::Int(i) => *i < 0,
        Value::Float(f) => *f < 0.0,
        Value::Ratio(n, d) => (*n > 0) != (*d > 0) && *n != 0,
        _ => {
            return Err(Error::Type(format!(
                "neg?: expected number, got {}",
                args[0].type_name()
            )));
        }
    }))
}

fn identity_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    Ok(args[0].clone())
}

// ===========================================================================
// Newly-ported clojure.core fns. Conventions:
//   * Each fn is a `fn(&[Value]) -> Result<Value>` matching the existing
//     style; arity errors raised explicitly when meaningful.
//   * Where Clojure's behavior depends on JVM-only types (bigdec, ratio
//     promotion to bigint), we degrade to the closest cljrs primitive
//     and document it in a header comment.
// ===========================================================================

fn num_eq_fn(args: &[Value]) -> Result<Value> {
    if args.len() < 2 { return Ok(Value::Bool(true)); }
    let first = as_f64(&args[0])?;
    for a in &args[1..] {
        if as_f64(a)? != first { return Ok(Value::Bool(false)); }
    }
    Ok(Value::Bool(true))
}

fn bit_and_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() { return Err(Error::Arity { expected: ">= 1".into(), got: 0 }); }
    let mut acc = to_i64(&args[0])?;
    for a in &args[1..] { acc &= to_i64(a)?; }
    Ok(Value::Int(acc))
}
fn bit_or_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() { return Err(Error::Arity { expected: ">= 1".into(), got: 0 }); }
    let mut acc = to_i64(&args[0])?;
    for a in &args[1..] { acc |= to_i64(a)?; }
    Ok(Value::Int(acc))
}
fn bit_xor_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() { return Err(Error::Arity { expected: ">= 1".into(), got: 0 }); }
    let mut acc = to_i64(&args[0])?;
    for a in &args[1..] { acc ^= to_i64(a)?; }
    Ok(Value::Int(acc))
}
fn bit_not_fn(args: &[Value]) -> Result<Value> { Ok(Value::Int(!to_i64(&args[0])?)) }
fn bit_and_not_fn(args: &[Value]) -> Result<Value> {
    if args.len() < 2 { return Err(Error::Arity { expected: ">= 2".into(), got: args.len() }); }
    let mut acc = to_i64(&args[0])?;
    for a in &args[1..] { acc &= !to_i64(a)?; }
    Ok(Value::Int(acc))
}
fn bit_shl_fn(args: &[Value]) -> Result<Value> {
    let x = to_i64(&args[0])?; let n = to_i64(&args[1])? & 63;
    Ok(Value::Int(x.wrapping_shl(n as u32)))
}
fn bit_shr_fn(args: &[Value]) -> Result<Value> {
    let x = to_i64(&args[0])?; let n = to_i64(&args[1])? & 63;
    Ok(Value::Int(x.wrapping_shr(n as u32)))
}
fn bit_ushr_fn(args: &[Value]) -> Result<Value> {
    let x = to_i64(&args[0])? as u64; let n = to_i64(&args[1])? & 63;
    Ok(Value::Int(x.wrapping_shr(n as u32) as i64))
}
fn bit_flip_fn(args: &[Value]) -> Result<Value> {
    let x = to_i64(&args[0])?; let n = to_i64(&args[1])? & 63;
    Ok(Value::Int(x ^ (1i64 << n)))
}
fn bit_set_fn(args: &[Value]) -> Result<Value> {
    let x = to_i64(&args[0])?; let n = to_i64(&args[1])? & 63;
    Ok(Value::Int(x | (1i64 << n)))
}
fn bit_clear_fn(args: &[Value]) -> Result<Value> {
    let x = to_i64(&args[0])?; let n = to_i64(&args[1])? & 63;
    Ok(Value::Int(x & !(1i64 << n)))
}
fn bit_test_fn(args: &[Value]) -> Result<Value> {
    let x = to_i64(&args[0])?; let n = to_i64(&args[1])? & 63;
    Ok(Value::Bool((x >> n) & 1 == 1))
}

fn parse_long_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Str(s) => Ok(s.trim().parse::<i64>().map(Value::Int).unwrap_or(Value::Nil)),
        _ => Err(Error::Type("parse-long: expected string".into())),
    }
}
fn parse_double_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Str(s) => Ok(s.trim().parse::<f64>().map(Value::Float).unwrap_or(Value::Nil)),
        _ => Err(Error::Type("parse-double: expected string".into())),
    }
}
fn parse_boolean_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Str(s) => match s.as_ref() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Ok(Value::Nil),
        },
        _ => Err(Error::Type("parse-boolean: expected string".into())),
    }
}
fn is_uuid_str(t: &str) -> bool {
    t.len() == 36 && t.chars().enumerate().all(|(i, c)| match i {
        8 | 13 | 18 | 23 => c == '-',
        _ => c.is_ascii_hexdigit(),
    })
}
fn parse_uuid_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Str(s) => {
            let t = s.trim();
            if is_uuid_str(t) { Ok(Value::Str(Arc::from(t))) } else { Ok(Value::Nil) }
        }
        _ => Err(Error::Type("parse-uuid: expected string".into())),
    }
}

fn unchecked_add_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Int(to_i64(&args[0])?.wrapping_add(to_i64(&args[1])?)))
}
fn unchecked_sub_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Int(to_i64(&args[0])?.wrapping_sub(to_i64(&args[1])?)))
}
fn unchecked_mul_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Int(to_i64(&args[0])?.wrapping_mul(to_i64(&args[1])?)))
}
fn unchecked_div_fn(args: &[Value]) -> Result<Value> {
    let b = to_i64(&args[1])?;
    if b == 0 { return Err(Error::Eval("integer divide by zero".into())); }
    Ok(Value::Int(to_i64(&args[0])?.wrapping_div(b)))
}
fn unchecked_neg_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Int(to_i64(&args[0])?.wrapping_neg()))
}
fn unchecked_inc_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Int(to_i64(&args[0])?.wrapping_add(1)))
}
fn unchecked_dec_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Int(to_i64(&args[0])?.wrapping_sub(1)))
}
fn unchecked_rem_fn(args: &[Value]) -> Result<Value> {
    let b = to_i64(&args[1])?;
    if b == 0 { return Err(Error::Eval("integer divide by zero".into())); }
    Ok(Value::Int(to_i64(&args[0])?.wrapping_rem(b)))
}

fn numerator_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Ratio(n, _) => Ok(Value::Int(*n)),
        Value::Int(i) => Ok(Value::Int(*i)),
        _ => Err(Error::Type("numerator: expected ratio".into())),
    }
}
fn denominator_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Ratio(_, d) => Ok(Value::Int(*d)),
        Value::Int(_) => Ok(Value::Int(1)),
        _ => Err(Error::Type("denominator: expected ratio".into())),
    }
}
fn rationalize_fn(args: &[Value]) -> Result<Value> { Ok(args[0].clone()) }
fn num_fn(args: &[Value]) -> Result<Value> { Ok(args[0].clone()) }

fn compare_fn(args: &[Value]) -> Result<Value> {
    let a = &args[0]; let b = &args[1];
    let ord = match (a, b) {
        (Value::Nil, Value::Nil) => std::cmp::Ordering::Equal,
        (Value::Nil, _) => std::cmp::Ordering::Less,
        (_, Value::Nil) => std::cmp::Ordering::Greater,
        _ => {
            if let (Ok(x), Ok(y)) = (as_f64(a), as_f64(b)) {
                x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
            } else if let (Value::Str(x), Value::Str(y)) = (a, b) {
                x.as_ref().cmp(y.as_ref())
            } else if let (Value::Bool(x), Value::Bool(y)) = (a, b) {
                x.cmp(y)
            } else if let (Value::Keyword(x), Value::Keyword(y)) = (a, b) {
                x.as_ref().cmp(y.as_ref())
            } else if let (Value::Symbol(x), Value::Symbol(y)) = (a, b) {
                x.as_ref().cmp(y.as_ref())
            } else {
                a.to_pr_string().cmp(&b.to_pr_string())
            }
        }
    };
    Ok(Value::Int(match ord {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }))
}

fn hash_fn(args: &[Value]) -> Result<Value> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    args[0].hash(&mut h);
    Ok(Value::Int(h.finish() as i64))
}
fn hash_ordered_coll_fn(args: &[Value]) -> Result<Value> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let coll = seq_items(&args[0])?;
    let mut h = DefaultHasher::new();
    for x in &coll { x.hash(&mut h); }
    Ok(Value::Int(h.finish() as i64))
}
fn hash_unordered_coll_fn(args: &[Value]) -> Result<Value> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let coll = seq_items(&args[0])?;
    let mut acc: u64 = 0;
    for x in &coll {
        let mut h = DefaultHasher::new();
        x.hash(&mut h);
        acc ^= h.finish();
    }
    Ok(Value::Int(acc as i64))
}
fn mix_collection_hash_fn(args: &[Value]) -> Result<Value> {
    let h = to_i64(&args[0])?; let n = to_i64(&args[1])?;
    Ok(Value::Int(h.wrapping_mul(31).wrapping_add(n)))
}
fn nan_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Float(f) if f.is_nan())))
}
fn infinite_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Float(f) if f.is_infinite())))
}
fn boolean_fn(args: &[Value]) -> Result<Value> { Ok(Value::Bool(args[0].truthy())) }

fn const_false_fn(_args: &[Value]) -> Result<Value> { Ok(Value::Bool(false)) }
fn const_nil_fn(_args: &[Value]) -> Result<Value> { Ok(Value::Nil) }

fn instance_q_fn(args: &[Value]) -> Result<Value> {
    let want = match &args[0] {
        Value::Keyword(s) => s.to_string(),
        Value::Str(s) => s.to_string(),
        Value::Symbol(s) => s.to_string(),
        _ => return Err(Error::Type("instance?: expected class tag (keyword/string/symbol)".into())),
    };
    let actual = args[1].type_name();
    Ok(Value::Bool(want == actual))
}
fn identical_q_fn(args: &[Value]) -> Result<Value> {
    use Value::*;
    Ok(Value::Bool(match (&args[0], &args[1]) {
        (Nil, Nil) => true,
        (Bool(a), Bool(b)) => a == b,
        (Int(a), Int(b)) => a == b,
        (Str(a), Str(b)) => Arc::ptr_eq(a, b),
        (Symbol(a), Symbol(b)) => Arc::ptr_eq(a, b),
        (Keyword(a), Keyword(b)) => Arc::ptr_eq(a, b),
        (List(a), List(b)) => Arc::ptr_eq(a, b),
        (Atom(a), Atom(b)) => Arc::ptr_eq(a, b),
        (Fn(a), Fn(b)) => Arc::ptr_eq(a, b),
        (Macro(a), Macro(b)) => Arc::ptr_eq(a, b),
        _ => &args[0] == &args[1],
    }))
}
fn ratio_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Ratio(_, _))))
}
fn any_q_fn(_args: &[Value]) -> Result<Value> { Ok(Value::Bool(true)) }
fn ifn_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0],
        Value::Fn(_) | Value::Builtin(_) | Value::Native(_)
        | Value::Multi(_) | Value::Keyword(_) | Value::Symbol(_)
        | Value::Map(_) | Value::Set(_) | Value::Vector(_))))
}
fn ident_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Keyword(_) | Value::Symbol(_))))
}
fn is_qualified(s: &str) -> bool { s.contains('/') && s.len() > 1 }
fn simple_ident_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(match &args[0] {
        Value::Keyword(s) | Value::Symbol(s) => !is_qualified(s),
        _ => false,
    }))
}
fn qualified_ident_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(match &args[0] {
        Value::Keyword(s) | Value::Symbol(s) => is_qualified(s),
        _ => false,
    }))
}
fn simple_keyword_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Keyword(s) if !is_qualified(s))))
}
fn qualified_keyword_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Keyword(s) if is_qualified(s))))
}
fn simple_symbol_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Symbol(s) if !is_qualified(s))))
}
fn qualified_symbol_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Symbol(s) if is_qualified(s))))
}
fn indexed_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Vector(_) | Value::Str(_))))
}
fn counted_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0],
        Value::List(_) | Value::Vector(_) | Value::Map(_) | Value::Set(_) | Value::Str(_))))
}
fn seqable_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0],
        Value::Nil | Value::List(_) | Value::Vector(_) | Value::Map(_)
        | Value::Set(_) | Value::Str(_) | Value::Cons(_, _) | Value::LazySeq(_))))
}
fn sequential_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0],
        Value::List(_) | Value::Vector(_) | Value::Cons(_, _) | Value::LazySeq(_))))
}
fn associative_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Map(_) | Value::Vector(_))))
}
fn reversible_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Vector(_))))
}
fn map_entry_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Vector(v) if v.len() == 2)))
}
fn uuid_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Str(s) if is_uuid_str(s.as_ref()))))
}
fn nat_int_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Int(i) if *i >= 0)))
}
fn neg_int_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Int(i) if *i < 0)))
}
fn pos_int_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Int(i) if *i > 0)))
}
fn distinct_q_fn(args: &[Value]) -> Result<Value> {
    let mut seen: imbl::HashSet<Value> = imbl::HashSet::new();
    for a in args {
        if seen.contains(a) { return Ok(Value::Bool(false)); }
        seen.insert(a.clone());
    }
    Ok(Value::Bool(true))
}
fn not_any_q_fn(args: &[Value]) -> Result<Value> {
    let pred = &args[0];
    let coll = seq_items(&args[1])?;
    for v in coll {
        if eval::apply(pred, std::slice::from_ref(&v))?.truthy() { return Ok(Value::Bool(false)); }
    }
    Ok(Value::Bool(true))
}
fn not_every_q_fn(args: &[Value]) -> Result<Value> {
    let pred = &args[0];
    let coll = seq_items(&args[1])?;
    for v in coll {
        if !eval::apply(pred, std::slice::from_ref(&v))?.truthy() { return Ok(Value::Bool(true)); }
    }
    Ok(Value::Bool(false))
}
const SPECIAL_NAMES: &[&str] = &[
    "def", "if", "do", "let", "let*", "loop", "loop*", "recur", "fn", "fn*",
    "quote", "var", "throw", "try", "catch", "finally", "new", ".", "set!",
    "monitor-enter", "monitor-exit",
];
fn special_symbol_q_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(matches!(&args[0], Value::Symbol(s) if SPECIAL_NAMES.contains(&s.as_ref()))))
}

fn peek_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Vector(v) => Ok(v.last().cloned().unwrap_or(Value::Nil)),
        Value::List(v) => Ok(v.first().cloned().unwrap_or(Value::Nil)),
        _ => Err(Error::Type("peek: expected vector or list".into())),
    }
}
fn pop_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Vector(v) => {
            if v.is_empty() { return Err(Error::Eval("can't pop empty vector".into())); }
            let mut out = v.clone();
            out.pop_back();
            Ok(Value::Vector(out))
        }
        Value::List(v) => {
            if v.is_empty() { return Err(Error::Eval("can't pop empty list".into())); }
            Ok(Value::List(Arc::new(v[1..].to_vec())))
        }
        _ => Err(Error::Type("pop: expected vector or list".into())),
    }
}
fn disj_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() { return Err(Error::Arity { expected: ">= 1".into(), got: 0 }); }
    match &args[0] {
        Value::Nil => Ok(Value::Nil),
        Value::Set(s) => {
            let mut out = s.clone();
            for a in &args[1..] { out.remove(a); }
            Ok(Value::Set(out))
        }
        _ => Err(Error::Type("disj: expected set".into())),
    }
}
fn replace_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 2 { return Err(Error::Arity { expected: "2".into(), got: args.len() }); }
    let smap = &args[0];
    let lookup = |v: &Value| -> Value {
        match smap {
            Value::Map(m) => m.get(v).cloned().unwrap_or_else(|| v.clone()),
            Value::Vector(vec) => match v {
                Value::Int(i) if *i >= 0 && (*i as usize) < vec.len() => vec[*i as usize].clone(),
                _ => v.clone(),
            },
            _ => v.clone(),
        }
    };
    match &args[1] {
        Value::Vector(v) => {
            let mut out: imbl::Vector<Value> = imbl::Vector::new();
            for x in v.iter() { out.push_back(lookup(x)); }
            Ok(Value::Vector(out))
        }
        _ => {
            let coll = seq_items(&args[1])?;
            let out: Vec<Value> = coll.iter().map(lookup).collect();
            Ok(Value::List(Arc::new(out)))
        }
    }
}
fn merge_with_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() { return Err(Error::Arity { expected: ">= 1".into(), got: 0 }); }
    let f = &args[0];
    let mut acc: Option<imbl::HashMap<Value, Value>> = None;
    for m in &args[1..] {
        match m {
            Value::Nil => continue,
            Value::Map(other) => {
                let mut into = acc.take().unwrap_or_default();
                for (k, v) in other.iter() {
                    let merged = match into.get(k) {
                        Some(prev) => eval::apply(f, &[prev.clone(), v.clone()])?,
                        None => v.clone(),
                    };
                    into.insert(k.clone(), merged);
                }
                acc = Some(into);
            }
            _ => return Err(Error::Type("merge-with: expected map".into())),
        }
    }
    Ok(acc.map(Value::Map).unwrap_or(Value::Nil))
}
fn rseq_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Vector(v) => {
            if v.is_empty() { return Ok(Value::Nil); }
            let out: Vec<Value> = v.iter().rev().cloned().collect();
            Ok(Value::List(Arc::new(out)))
        }
        Value::Nil => Ok(Value::Nil),
        _ => Err(Error::Type("rseq: expected vector".into())),
    }
}
fn empty_fn(args: &[Value]) -> Result<Value> {
    Ok(match &args[0] {
        Value::Vector(_) => Value::Vector(imbl::Vector::new()),
        Value::List(_) => Value::List(Arc::new(Vec::new())),
        Value::Map(_) => Value::Map(imbl::HashMap::new()),
        Value::Set(_) => Value::Set(imbl::HashSet::new()),
        Value::Str(_) => Value::Str(Arc::from("")),
        _ => Value::Nil,
    })
}
fn vector_of_fn(args: &[Value]) -> Result<Value> {
    let mut out: imbl::Vector<Value> = imbl::Vector::new();
    for a in &args[1..] { out.push_back(a.clone()); }
    Ok(Value::Vector(out))
}
fn sorted_set_fn(args: &[Value]) -> Result<Value> {
    let mut s: imbl::HashSet<Value> = imbl::HashSet::new();
    for a in args { s.insert(a.clone()); }
    Ok(Value::Set(s))
}
fn sorted_set_by_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() { return Err(Error::Arity { expected: ">= 1".into(), got: 0 }); }
    let mut s: imbl::HashSet<Value> = imbl::HashSet::new();
    for a in &args[1..] { s.insert(a.clone()); }
    Ok(Value::Set(s))
}
fn sorted_map_by_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() { return Err(Error::Arity { expected: ">= 1".into(), got: 0 }); }
    if (args.len() - 1) % 2 != 0 { return Err(Error::Eval("sorted-map-by: odd kvs".into())); }
    let mut m: imbl::HashMap<Value, Value> = imbl::HashMap::new();
    for kv in args[1..].chunks(2) { m.insert(kv[0].clone(), kv[1].clone()); }
    Ok(Value::Map(m))
}
fn subseq_fn(args: &[Value]) -> Result<Value> {
    if args.len() < 3 { return Err(Error::Arity { expected: ">= 3".into(), got: args.len() }); }
    let test = &args[1];
    let key = &args[2];
    let key_f = as_f64(key)?;
    let items: Vec<Value> = match &args[0] {
        Value::Map(m) => m.keys().cloned().collect(),
        Value::Set(s) => s.iter().cloned().collect(),
        Value::Vector(v) => v.iter().cloned().collect(),
        _ => seq_items(&args[0])?,
    };
    let mut sorted = items;
    sorted.sort_by(|a, b| {
        match (as_f64(a), as_f64(b)) {
            (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
            _ => a.to_pr_string().cmp(&b.to_pr_string()),
        }
    });
    let mut out = Vec::new();
    for v in sorted {
        let f = as_f64(&v).unwrap_or(0.0);
        let pass = match test {
            Value::Builtin(b) => match b.name {
                "<" => f < key_f, "<=" => f <= key_f,
                ">" => f > key_f, ">=" => f >= key_f,
                _ => (eval::apply(test, &[v.clone(), key.clone()])?).truthy(),
            },
            _ => (eval::apply(test, &[v.clone(), key.clone()])?).truthy(),
        };
        if pass { out.push(v); }
    }
    Ok(Value::List(Arc::new(out)))
}
fn rsubseq_fn(args: &[Value]) -> Result<Value> {
    let r = subseq_fn(args)?;
    let coll = seq_items(&r)?;
    let rev: Vec<Value> = coll.into_iter().rev().collect();
    Ok(Value::List(Arc::new(rev)))
}
fn key_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Vector(v) if v.len() == 2 => Ok(v[0].clone()),
        _ => Err(Error::Type("key: expected map entry [k v]".into())),
    }
}
fn val_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Vector(v) if v.len() == 2 => Ok(v[1].clone()),
        _ => Err(Error::Type("val: expected map entry [k v]".into())),
    }
}
fn find_keyword_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Str(s) => Ok(Value::Keyword(s.clone())),
        Value::Keyword(s) => Ok(Value::Keyword(s.clone())),
        _ => Ok(Value::Nil),
    }
}
fn namespace_fn(args: &[Value]) -> Result<Value> {
    let s = match &args[0] {
        Value::Keyword(s) | Value::Symbol(s) => s.as_ref(),
        _ => return Err(Error::Type("namespace: expected keyword/symbol".into())),
    };
    Ok(match s.find('/') {
        Some(i) if i > 0 && i < s.len() - 1 => Value::Str(Arc::from(&s[..i])),
        _ => Value::Nil,
    })
}

fn partition_by_fn(args: &[Value]) -> Result<Value> {
    let f = &args[0];
    let coll = seq_items(&args[1])?;
    let mut out: Vec<Value> = Vec::new();
    let mut cur: Vec<Value> = Vec::new();
    let mut last_k: Option<Value> = None;
    for item in coll {
        let k = eval::apply(f, std::slice::from_ref(&item))?;
        if last_k.as_ref() == Some(&k) {
            cur.push(item);
        } else {
            if !cur.is_empty() { out.push(Value::List(Arc::new(std::mem::take(&mut cur)))); }
            cur.push(item);
            last_k = Some(k);
        }
    }
    if !cur.is_empty() { out.push(Value::List(Arc::new(cur))); }
    Ok(Value::List(Arc::new(out)))
}
fn partition_all_fn(args: &[Value]) -> Result<Value> { partition_all_coll_fn(args) }
fn split_at_fn(args: &[Value]) -> Result<Value> {
    let n = to_i64(&args[0])?.max(0) as usize;
    let coll = seq_items(&args[1])?;
    let (a, b) = coll.split_at(n.min(coll.len()));
    let mut out: imbl::Vector<Value> = imbl::Vector::new();
    out.push_back(Value::List(Arc::new(a.to_vec())));
    out.push_back(Value::List(Arc::new(b.to_vec())));
    Ok(Value::Vector(out))
}
fn splitv_at_fn(args: &[Value]) -> Result<Value> {
    let n = to_i64(&args[0])?.max(0) as usize;
    let coll = seq_items(&args[1])?;
    let (a, b) = coll.split_at(n.min(coll.len()));
    let mut out: imbl::Vector<Value> = imbl::Vector::new();
    out.push_back(Value::Vector(a.iter().cloned().collect()));
    out.push_back(Value::Vector(b.iter().cloned().collect()));
    Ok(Value::Vector(out))
}
fn split_with_fn(args: &[Value]) -> Result<Value> {
    let pred = &args[0];
    let coll = seq_items(&args[1])?;
    let mut idx = 0;
    for v in &coll {
        if !eval::apply(pred, std::slice::from_ref(v))?.truthy() { break; }
        idx += 1;
    }
    let (a, b) = coll.split_at(idx);
    let mut out: imbl::Vector<Value> = imbl::Vector::new();
    out.push_back(Value::List(Arc::new(a.to_vec())));
    out.push_back(Value::List(Arc::new(b.to_vec())));
    Ok(Value::Vector(out))
}
fn take_last_fn(args: &[Value]) -> Result<Value> {
    let n = to_i64(&args[0])?.max(0) as usize;
    let coll = seq_items(&args[1])?;
    if n == 0 { return Ok(Value::Nil); }
    let start = coll.len().saturating_sub(n);
    Ok(Value::List(Arc::new(coll[start..].to_vec())))
}
fn drop_last_fn(args: &[Value]) -> Result<Value> {
    let (n, coll) = if args.len() == 1 {
        (1usize, seq_items(&args[0])?)
    } else {
        (to_i64(&args[0])?.max(0) as usize, seq_items(&args[1])?)
    };
    let end = coll.len().saturating_sub(n);
    Ok(Value::List(Arc::new(coll[..end].to_vec())))
}
fn butlast_fn(args: &[Value]) -> Result<Value> {
    let coll = seq_items(&args[0])?;
    if coll.is_empty() { return Ok(Value::Nil); }
    Ok(Value::List(Arc::new(coll[..coll.len() - 1].to_vec())))
}
fn tree_seq_fn(args: &[Value]) -> Result<Value> {
    if args.len() != 3 { return Err(Error::Arity { expected: "3".into(), got: args.len() }); }
    fn walk(branch_q: &Value, children: &Value, node: Value, out: &mut Vec<Value>) -> Result<()> {
        out.push(node.clone());
        if eval::apply(branch_q, std::slice::from_ref(&node))?.truthy() {
            let kids = eval::apply(children, std::slice::from_ref(&node))?;
            for k in seq_items(&kids)? { walk(branch_q, children, k, out)?; }
        }
        Ok(())
    }
    let mut out: Vec<Value> = Vec::new();
    walk(&args[0], &args[1], args[2].clone(), &mut out)?;
    Ok(Value::List(Arc::new(out)))
}
fn cycle_fn(args: &[Value]) -> Result<Value> {
    // Eager-bounded: 256x or 64K cap. Pair with `take` for finite use.
    let coll = seq_items(&args[0])?;
    if coll.is_empty() { return Ok(Value::List(Arc::new(Vec::new()))); }
    let bound = (coll.len() * 256).min(1 << 16);
    let mut out: Vec<Value> = Vec::with_capacity(bound);
    for i in 0..bound { out.push(coll[i % coll.len()].clone()); }
    Ok(Value::List(Arc::new(out)))
}
fn iterate_fn(args: &[Value]) -> Result<Value> {
    // Eager-bounded: 1024 iterates. Pair with `take` for finite use.
    let f = &args[0];
    let mut x = args[1].clone();
    let mut out = Vec::with_capacity(1024);
    out.push(x.clone());
    for _ in 0..1023 {
        x = eval::apply(f, std::slice::from_ref(&x))?;
        out.push(x.clone());
    }
    Ok(Value::List(Arc::new(out)))
}
fn replicate_fn(args: &[Value]) -> Result<Value> {
    let n = to_i64(&args[0])?.max(0) as usize;
    let v = args[1].clone();
    Ok(Value::List(Arc::new(vec![v; n])))
}
fn reductions_fn(args: &[Value]) -> Result<Value> {
    let (f, init, coll) = match args.len() {
        2 => (args[0].clone(), None, seq_items(&args[1])?),
        3 => (args[0].clone(), Some(args[1].clone()), seq_items(&args[2])?),
        n => return Err(Error::Arity { expected: "2 or 3".into(), got: n }),
    };
    let has_init = init.is_some();
    let mut out = Vec::with_capacity(coll.len() + 1);
    let mut acc = match init {
        Some(v) => { out.push(v.clone()); v }
        None => {
            if coll.is_empty() { return Ok(Value::List(Arc::new(vec![eval::apply(&f, &[])?]))); }
            out.push(coll[0].clone());
            coll[0].clone()
        }
    };
    let start = if has_init { 0 } else { 1 };
    for x in &coll[start..] {
        acc = eval::apply(&f, &[acc, x.clone()])?;
        if let Value::Reduced(inner) = &acc {
            out.push((**inner).clone());
            return Ok(Value::List(Arc::new(out)));
        }
        out.push(acc.clone());
    }
    Ok(Value::List(Arc::new(out)))
}
fn list_star_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() { return Err(Error::Arity { expected: ">= 1".into(), got: 0 }); }
    let mut out: Vec<Value> = args[..args.len() - 1].to_vec();
    out.extend(seq_items(&args[args.len() - 1])?);
    Ok(Value::List(Arc::new(out)))
}
fn ffirst_fn(args: &[Value]) -> Result<Value> {
    let v = first_fn(args)?;
    first_fn(&[v])
}
fn fnext_fn(args: &[Value]) -> Result<Value> {
    let v = rest_fn(args)?;
    first_fn(&[v])
}
fn nfirst_fn(args: &[Value]) -> Result<Value> {
    let v = first_fn(args)?;
    next_fn(&[v])
}
fn nnext_fn(args: &[Value]) -> Result<Value> {
    let v = rest_fn(args)?;
    next_fn(&[v])
}
fn next_fn(args: &[Value]) -> Result<Value> {
    let r = rest_fn(args)?;
    let empty = match &r {
        Value::List(v) => v.is_empty(),
        Value::Vector(v) => v.is_empty(),
        Value::Nil => true,
        _ => false,
    };
    if empty { Ok(Value::Nil) } else { Ok(r) }
}
fn nthnext_fn(args: &[Value]) -> Result<Value> {
    let n = to_i64(&args[1])?.max(0) as usize;
    let mut cur = args[0].clone();
    for _ in 0..n {
        cur = next_fn(&[cur])?;
        if matches!(cur, Value::Nil) { return Ok(Value::Nil); }
    }
    Ok(cur)
}
fn nthrest_fn(args: &[Value]) -> Result<Value> {
    let n = to_i64(&args[1])?.max(0) as usize;
    let mut cur = args[0].clone();
    for _ in 0..n { cur = rest_fn(&[cur])?; }
    Ok(cur)
}
fn doall_fn(args: &[Value]) -> Result<Value> {
    let coll = if args.len() == 2 { &args[1] } else { &args[0] };
    let items = seq_items(coll)?;
    Ok(Value::List(Arc::new(items)))
}
fn dorun_fn(args: &[Value]) -> Result<Value> {
    let coll = if args.len() == 2 { &args[1] } else { &args[0] };
    let _ = seq_items(coll)?;
    Ok(Value::Nil)
}
fn run_bang_fn(args: &[Value]) -> Result<Value> {
    let f = &args[0];
    let coll = seq_items(&args[1])?;
    for v in coll { let _ = eval::apply(f, std::slice::from_ref(&v))?; }
    Ok(Value::Nil)
}

fn rand_state() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static SEED: AtomicU64 = AtomicU64::new(0);
    let mut s = SEED.load(Ordering::Relaxed);
    if s == 0 {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64).unwrap_or(0xDEADBEEF);
        s = now | 1;
    }
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    SEED.store(s, Ordering::Relaxed);
    s
}
fn rand_f64() -> f64 {
    (rand_state() >> 11) as f64 / ((1u64 << 53) as f64)
}
fn rand_fn(args: &[Value]) -> Result<Value> {
    let n = if args.is_empty() { 1.0 } else { as_f64(&args[0])? };
    Ok(Value::Float(rand_f64() * n))
}
fn rand_int_fn(args: &[Value]) -> Result<Value> {
    let n = to_i64(&args[0])?.max(1);
    Ok(Value::Int((rand_state() as i64 & i64::MAX).rem_euclid(n)))
}
fn rand_nth_fn(args: &[Value]) -> Result<Value> {
    let coll = seq_items(&args[0])?;
    if coll.is_empty() { return Err(Error::Eval("rand-nth: empty coll".into())); }
    let i = (rand_state() as usize) % coll.len();
    Ok(coll[i].clone())
}
fn random_sample_fn(args: &[Value]) -> Result<Value> {
    let p = as_f64(&args[0])?;
    let coll = seq_items(&args[1])?;
    let mut out = Vec::new();
    for v in coll { if rand_f64() < p { out.push(v); } }
    Ok(Value::List(Arc::new(out)))
}
fn random_uuid_fn(_args: &[Value]) -> Result<Value> {
    let a = rand_state();
    let b = rand_state();
    let high = (a & 0xFFFFFFFFFFFF0FFF) | 0x0000000000004000;
    let low = (b & 0x3FFFFFFFFFFFFFFF) | 0x8000000000000000;
    let s = format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        (high >> 32) as u32,
        ((high >> 16) & 0xFFFF) as u16,
        (high & 0xFFFF) as u16,
        ((low >> 48) & 0xFFFF) as u16,
        low & 0xFFFFFFFFFFFF,
    );
    Ok(Value::Str(Arc::from(s.as_str())))
}
fn shuffle_fn(args: &[Value]) -> Result<Value> {
    let mut coll = seq_items(&args[0])?;
    let n = coll.len();
    for i in (1..n).rev() {
        let j = (rand_state() as usize) % (i + 1);
        coll.swap(i, j);
    }
    let v: imbl::Vector<Value> = coll.into_iter().collect();
    Ok(Value::Vector(v))
}
fn bounded_count_fn(args: &[Value]) -> Result<Value> {
    let n = to_i64(&args[0])?.max(0) as usize;
    let items = seq_items(&args[1])?;
    Ok(Value::Int(items.len().min(n) as i64))
}

fn every_pred_fn(args: &[Value]) -> Result<Value> {
    let preds: Vec<Value> = args.to_vec();
    Ok(Value::Builtin(Builtin::new_closure("every-pred", move |xs: &[Value]| {
        for x in xs {
            for p in &preds {
                if !eval::apply(p, std::slice::from_ref(x))?.truthy() {
                    return Ok(Value::Bool(false));
                }
            }
        }
        Ok(Value::Bool(true))
    })))
}
fn some_fn_fn(args: &[Value]) -> Result<Value> {
    let preds: Vec<Value> = args.to_vec();
    Ok(Value::Builtin(Builtin::new_closure("some-fn", move |xs: &[Value]| {
        for x in xs {
            for p in &preds {
                let r = eval::apply(p, std::slice::from_ref(x))?;
                if r.truthy() { return Ok(r); }
            }
        }
        Ok(Value::Bool(false))
    })))
}
fn fnil_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() { return Err(Error::Arity { expected: ">= 2".into(), got: args.len() }); }
    let f = args[0].clone();
    let defaults: Vec<Value> = args[1..].to_vec();
    Ok(Value::Builtin(Builtin::new_closure("fnil", move |xs: &[Value]| {
        let mut patched: Vec<Value> = xs.to_vec();
        for (i, d) in defaults.iter().enumerate() {
            if let Some(slot) = patched.get_mut(i) {
                if matches!(slot, Value::Nil) { *slot = d.clone(); }
            }
        }
        eval::apply(&f, &patched)
    })))
}
fn memoize_fn(args: &[Value]) -> Result<Value> {
    use std::sync::Mutex;
    let f = args[0].clone();
    let cache: Arc<Mutex<imbl::HashMap<Value, Value>>> = Arc::new(Mutex::new(imbl::HashMap::new()));
    Ok(Value::Builtin(Builtin::new_closure("memoize", move |xs: &[Value]| {
        let key = Value::Vector(xs.iter().cloned().collect());
        if let Some(v) = cache.lock().unwrap().get(&key).cloned() { return Ok(v); }
        let v = eval::apply(&f, xs)?;
        cache.lock().unwrap().insert(key, v.clone());
        Ok(v)
    })))
}
fn comparator_fn(args: &[Value]) -> Result<Value> {
    let pred = args[0].clone();
    Ok(Value::Builtin(Builtin::new_closure("comparator", move |xs: &[Value]| {
        if xs.len() != 2 { return Err(Error::Arity { expected: "2".into(), got: xs.len() }); }
        if eval::apply(&pred, &[xs[0].clone(), xs[1].clone()])?.truthy() { return Ok(Value::Int(-1)); }
        if eval::apply(&pred, &[xs[1].clone(), xs[0].clone()])?.truthy() { return Ok(Value::Int(1)); }
        Ok(Value::Int(0))
    })))
}
fn sort_by_fn(args: &[Value]) -> Result<Value> {
    let (keyfn, cmp, coll) = match args.len() {
        2 => (args[0].clone(), None, &args[1]),
        3 => (args[0].clone(), Some(args[1].clone()), &args[2]),
        n => return Err(Error::Arity { expected: "2 or 3".into(), got: n }),
    };
    let items = seq_items(coll)?;
    let mut keyed: Vec<(Value, Value)> = items.into_iter()
        .map(|v| eval::apply(&keyfn, std::slice::from_ref(&v)).map(|k| (k, v)))
        .collect::<Result<_>>()?;
    if let Some(c) = cmp {
        let mut err: Option<Error> = None;
        keyed.sort_by(|a, b| {
            if err.is_some() { return std::cmp::Ordering::Equal; }
            match eval::apply(&c, &[a.0.clone(), b.0.clone()]) {
                Ok(Value::Int(n)) => n.cmp(&0),
                Ok(Value::Bool(true)) => std::cmp::Ordering::Less,
                Ok(Value::Bool(false)) => std::cmp::Ordering::Greater,
                Ok(_) => std::cmp::Ordering::Equal,
                Err(e) => { err = Some(e); std::cmp::Ordering::Equal }
            }
        });
        if let Some(e) = err { return Err(e); }
    } else {
        keyed.sort_by(|a, b| {
            match (as_f64(&a.0), as_f64(&b.0)) {
                (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                _ => a.0.to_pr_string().cmp(&b.0.to_pr_string()),
            }
        });
    }
    Ok(Value::List(Arc::new(keyed.into_iter().map(|(_, v)| v).collect())))
}
fn completing_fn(args: &[Value]) -> Result<Value> {
    let f = args[0].clone();
    let cf = if args.len() >= 2 { args[1].clone() }
             else { Value::Builtin(Builtin::new_static("identity", identity_fn)) };
    Ok(Value::Builtin(Builtin::new_closure("completing", move |xs: &[Value]| {
        match xs.len() {
            0 => eval::apply(&f, &[]),
            1 => eval::apply(&cf, xs),
            _ => eval::apply(&f, xs),
        }
    })))
}
fn ensure_reduced_fn(args: &[Value]) -> Result<Value> {
    if matches!(&args[0], Value::Reduced(_)) { Ok(args[0].clone()) }
    else { Ok(Value::Reduced(Arc::new(args[0].clone()))) }
}
fn trampoline_fn(args: &[Value]) -> Result<Value> {
    if args.is_empty() { return Err(Error::Arity { expected: ">= 1".into(), got: 0 }); }
    let mut cur = eval::apply(&args[0], &args[1..])?;
    loop {
        if matches!(cur, Value::Fn(_) | Value::Builtin(_) | Value::Native(_)) {
            cur = eval::apply(&cur, &[])?;
        } else { return Ok(cur); }
    }
}

fn with_meta_fn(args: &[Value]) -> Result<Value> { Ok(args[0].clone()) }
fn vary_meta_fn(args: &[Value]) -> Result<Value> { Ok(args[0].clone()) }

fn time_ms_fn(_args: &[Value]) -> Result<Value> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ms = SystemTime::now().duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64).unwrap_or(0);
    Ok(Value::Int(ms))
}
fn type_fn(args: &[Value]) -> Result<Value> {
    Ok(Value::Keyword(Arc::from(args[0].type_name())))
}
fn flush_fn(_args: &[Value]) -> Result<Value> {
    use std::io::Write;
    let _ = std::io::stdout().flush();
    Ok(Value::Nil)
}
fn newline_fn(_args: &[Value]) -> Result<Value> { println!(); Ok(Value::Nil) }
fn pr_fn(args: &[Value]) -> Result<Value> {
    let parts: Vec<String> = args.iter().map(Value::to_pr_string).collect();
    print!("{}", parts.join(" "));
    Ok(Value::Nil)
}
fn prn_fn(args: &[Value]) -> Result<Value> {
    let parts: Vec<String> = args.iter().map(Value::to_pr_string).collect();
    println!("{}", parts.join(" "));
    Ok(Value::Nil)
}
fn prn_str_fn(args: &[Value]) -> Result<Value> {
    let parts: Vec<String> = args.iter().map(Value::to_pr_string).collect();
    Ok(Value::Str(Arc::from(format!("{}\n", parts.join(" ")).as_str())))
}
fn ex_cause_fn(_args: &[Value]) -> Result<Value> { Ok(Value::Nil) }
fn set_validator_fn(_args: &[Value]) -> Result<Value> { Ok(Value::Nil) }
fn methods_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Multi(m) => {
            let methods = m.methods.read().unwrap().clone();
            Ok(Value::Map(methods))
        }
        _ => Err(Error::Type("methods: expected multi".into())),
    }
}
fn get_method_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Multi(m) => Ok(m.methods.read().unwrap().get(&args[1]).cloned().unwrap_or(Value::Nil)),
        _ => Err(Error::Type("get-method: expected multi".into())),
    }
}
fn prefer_method_fn(_args: &[Value]) -> Result<Value> { Ok(Value::Nil) }
fn prefers_fn(_args: &[Value]) -> Result<Value> { Ok(Value::Map(imbl::HashMap::new())) }
fn remove_method_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Multi(m) => {
            m.methods.write().unwrap().remove(&args[1]);
            Ok(args[0].clone())
        }
        _ => Err(Error::Type("remove-method: expected multi".into())),
    }
}
fn remove_all_methods_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Multi(m) => {
            m.methods.write().unwrap().clear();
            Ok(args[0].clone())
        }
        _ => Err(Error::Type("remove-all-methods: expected multi".into())),
    }
}
fn inst_ms_fn(args: &[Value]) -> Result<Value> {
    match &args[0] {
        Value::Int(i) => Ok(Value::Int(*i)),
        _ => Err(Error::Type("inst-ms: expected int (cljrs has no inst type)".into())),
    }
}
