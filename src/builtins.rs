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
        _ => Err(Error::Type(format!(
            "expected sequence, got {}",
            v.type_name()
        ))),
    }
}

pub fn install(env: &Env) {
    for (name, f) in core_fns() {
        env.define_global(name, Value::Builtin(Builtin::new_static(name, f)));
    }
    install_prelude(env);
}

/// Evaluate the cljrs-authored prelude (threading macros, conditional
/// macros, iteration macros, etc.). Bundled via `include_str!` so a
/// compiled cljrs binary needs no external file at runtime.
fn install_prelude(env: &Env) {
    const PRELUDE: &str = include_str!("core.clj");
    let forms = match crate::reader::read_all(PRELUDE) {
        Ok(f) => f,
        Err(e) => panic!("cljrs prelude parse failed: {e}"),
    };
    for f in forms {
        if let Err(e) = eval::eval(&f, env) {
            panic!("cljrs prelude eval failed: {e}");
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
    ]
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
    let sep = as_str(&args[1])?;
    let parts: Vec<Value> = s
        .split(sep)
        .map(|p| Value::Str(Arc::from(p)))
        .collect();
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

#[derive(Clone, Copy)]
enum Num {
    I(i64),
    F(f64),
}

fn to_num(v: &Value) -> Result<Num> {
    match v {
        Value::Int(i) => Ok(Num::I(*i)),
        Value::Float(f) => Ok(Num::F(*f)),
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
    }
}

fn fold_num<F, G>(args: &[Value], init: Num, fi: F, ff: G) -> Result<Value>
where
    F: Fn(i64, i64) -> Option<i64>,
    G: Fn(f64, f64) -> f64,
{
    let mut acc = init;
    for a in args {
        let n = to_num(a)?;
        acc = match (acc, n) {
            (Num::I(x), Num::I(y)) => match fi(x, y) {
                Some(r) => Num::I(r),
                None => Num::F(ff(x as f64, y as f64)),
            },
            (Num::I(x), Num::F(y)) => Num::F(ff(x as f64, y)),
            (Num::F(x), Num::I(y)) => Num::F(ff(x, y as f64)),
            (Num::F(x), Num::F(y)) => Num::F(ff(x, y)),
        };
    }
    Ok(num_to_value(acc))
}

fn add(args: &[Value]) -> Result<Value> {
    fold_num(args, Num::I(0), |a, b| a.checked_add(b), |a, b| a + b)
}

fn sub(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Arity {
            expected: ">= 1".into(),
            got: 0,
        });
    }
    if args.len() == 1 {
        return match to_num(&args[0])? {
            Num::I(i) => Ok(Value::Int(-i)),
            Num::F(f) => Ok(Value::Float(-f)),
        };
    }
    let first = to_num(&args[0])?;
    fold_num(&args[1..], first, |a, b| a.checked_sub(b), |a, b| a - b)
}

fn mul(args: &[Value]) -> Result<Value> {
    fold_num(args, Num::I(1), |a, b| a.checked_mul(b), |a, b| a * b)
}

fn div(args: &[Value]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Arity {
            expected: ">= 1".into(),
            got: 0,
        });
    }
    if args.len() == 1 {
        return match to_num(&args[0])? {
            Num::I(i) if i != 0 && 1 % i == 0 => Ok(Value::Int(1 / i)),
            Num::I(i) => Ok(Value::Float(1.0 / i as f64)),
            Num::F(f) => Ok(Value::Float(1.0 / f)),
        };
    }
    let first = to_num(&args[0])?;
    fold_num(
        &args[1..],
        first,
        |a, b| {
            if b != 0 && a % b == 0 {
                Some(a / b)
            } else {
                None
            }
        },
        |a, b| a / b,
    )
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
    if args.len() != 2 {
        return Err(Error::Arity {
            expected: "2".into(),
            got: args.len(),
        });
    }
    let idx = match &args[1] {
        Value::Int(i) => *i,
        _ => return Err(Error::Type("nth index must be int".into())),
    };
    if idx < 0 {
        return Err(Error::Eval("nth: negative index".into()));
    }
    let i = idx as usize;
    match &args[0] {
        Value::List(v) => v
            .get(i)
            .cloned()
            .ok_or_else(|| Error::Eval(format!("nth: index {idx} out of range"))),
        Value::Vector(v) => v
            .get(i)
            .cloned()
            .ok_or_else(|| Error::Eval(format!("nth: index {idx} out of range"))),
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
                // Clojure's reduce with no init on empty coll: call f with no args
                return eval::apply(f, &[]);
            }
            let mut acc = coll[0].clone();
            for item in &coll[1..] {
                acc = eval::apply(f, &[acc, item.clone()])?;
            }
            Ok(acc)
        }
        3 => {
            let f = &args[0];
            let mut acc = args[1].clone();
            let coll = as_seq(&args[2])?;
            for item in coll {
                acc = eval::apply(f, &[acc, item.clone()])?;
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
    Ok(Value::Bool(to_i64(&args[0])? > 0))
}
fn neg_q(args: &[Value]) -> Result<Value> {
    Ok(Value::Bool(to_i64(&args[0])? < 0))
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
