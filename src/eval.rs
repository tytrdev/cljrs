use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::env::Env;
use crate::error::{Error, Result};
use crate::types::{PrimType, parse_type_name, unwrap_tagged};
use crate::value::{Lambda, Value};

/// Monotonically increasing counter for fresh symbol names created by
/// destructuring and auto-gensym. Process-global; collisions with user
/// code are avoided by prefixing with `__gs_`.
static FRESH_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_sym(prefix: &str) -> Value {
    let n = FRESH_COUNTER.fetch_add(1, Ordering::Relaxed);
    Value::Symbol(Arc::from(format!("__gs_{prefix}_{n}").as_str()))
}

/// Expand a single binding pattern/expr pair into a flat sequence of
/// symbol-to-expr pairs suitable for `sf_let` to consume. Handles:
///   `sym`                          — identity
///   `[a b c]`                      — positional from a seq, incl. `& rest` and `:as full`
///   `{:keys [a b]}` / `{:as m}`    — map access, incl. `:or {a default}`
/// Recurses into nested patterns.
fn expand_destructure(pattern: &Value, expr: &Value, out: &mut Vec<(Value, Value)>) -> Result<()> {
    match pattern {
        Value::Symbol(_) => {
            out.push((pattern.clone(), expr.clone()));
            Ok(())
        }
        Value::Vector(items) => {
            let t = fresh_sym("vec");
            out.push((t.clone(), expr.clone()));
            // Scan for & rest and :as
            let mut i = 0usize;
            let mut position = 0i64;
            while i < items.len() {
                let item = &items[i];
                if let Value::Symbol(s) = item {
                    if s.as_ref() == "&" {
                        if i + 1 >= items.len() {
                            return Err(Error::Eval(
                                "destructure: '&' must be followed by a symbol".into(),
                            ));
                        }
                        let rest_expr = Value::List(Arc::new(vec![
                            Value::Symbol(Arc::from("drop")),
                            Value::Int(position),
                            t.clone(),
                        ]));
                        expand_destructure(&items[i + 1], &rest_expr, out)?;
                        i += 2;
                        continue;
                    }
                    if s.as_ref() == ":as" {
                        if i + 1 >= items.len() {
                            return Err(Error::Eval(
                                "destructure: ':as' must be followed by a symbol".into(),
                            ));
                        }
                        out.push((items[i + 1].clone(), t.clone()));
                        i += 2;
                        continue;
                    }
                }
                // Keyword ':as form (reader produces Keyword, not Symbol)
                if let Value::Keyword(k) = item
                    && k.as_ref() == "as"
                {
                    if i + 1 >= items.len() {
                        return Err(Error::Eval(
                            "destructure: ':as' must be followed by a symbol".into(),
                        ));
                    }
                    out.push((items[i + 1].clone(), t.clone()));
                    i += 2;
                    continue;
                }
                let getter = Value::List(Arc::new(vec![
                    Value::Symbol(Arc::from("get")),
                    t.clone(),
                    Value::Int(position),
                    Value::Nil,
                ]));
                expand_destructure(item, &getter, out)?;
                position += 1;
                i += 1;
            }
            Ok(())
        }
        Value::Map(pairs) => {
            let t = fresh_sym("map");
            out.push((t.clone(), expr.clone()));
            // Gather :or defaults first (so we can reference them as each binding is expanded).
            let or_defaults: imbl::HashMap<Value, Value> = pairs
                .iter()
                .find_map(|(k, v)| {
                    if matches!(k, Value::Keyword(s) if s.as_ref() == "or") {
                        match v {
                            Value::Map(m) => Some(m.clone()),
                            _ => None,
                        }
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            for (k, v) in pairs.iter() {
                match k {
                    Value::Keyword(s) => match s.as_ref() {
                        "as" => {
                            out.push((v.clone(), t.clone()));
                        }
                        "or" => {} // consumed above
                        "keys" => {
                            let Value::Vector(ks) = v else {
                                return Err(Error::Eval(
                                    "destructure: :keys must be followed by a vector of symbols"
                                        .into(),
                                ));
                            };
                            for key_sym in ks.iter() {
                                let Value::Symbol(name) = key_sym else {
                                    return Err(Error::Eval(
                                        "destructure: :keys entries must be symbols".into(),
                                    ));
                                };
                                let kw = Value::Keyword(Arc::clone(name));
                                let default = or_defaults.get(key_sym).cloned().unwrap_or(Value::Nil);
                                let getter = Value::List(Arc::new(vec![
                                    Value::Symbol(Arc::from("get")),
                                    t.clone(),
                                    kw,
                                    default,
                                ]));
                                out.push((key_sym.clone(), getter));
                            }
                        }
                        "strs" => {
                            let Value::Vector(ks) = v else {
                                return Err(Error::Eval(
                                    "destructure: :strs must be followed by a vector".into(),
                                ));
                            };
                            for key_sym in ks.iter() {
                                let Value::Symbol(name) = key_sym else {
                                    return Err(Error::Eval(
                                        "destructure: :strs entries must be symbols".into(),
                                    ));
                                };
                                let ks = Value::Str(Arc::clone(name));
                                let default = or_defaults.get(key_sym).cloned().unwrap_or(Value::Nil);
                                let getter = Value::List(Arc::new(vec![
                                    Value::Symbol(Arc::from("get")),
                                    t.clone(),
                                    ks,
                                    default,
                                ]));
                                out.push((key_sym.clone(), getter));
                            }
                        }
                        _ => {
                            return Err(Error::Eval(format!(
                                "destructure: unsupported map-pattern key :{}",
                                s.as_ref()
                            )));
                        }
                    },
                    _ => {
                        // {pattern key-expr} — bind pattern to (get t key-expr [default])
                        let default = or_defaults.get(k).cloned().unwrap_or(Value::Nil);
                        let getter = Value::List(Arc::new(vec![
                            Value::Symbol(Arc::from("get")),
                            t.clone(),
                            v.clone(),
                            default,
                        ]));
                        expand_destructure(k, &getter, out)?;
                    }
                }
            }
            Ok(())
        }
        _ => Err(Error::Eval(format!(
            "destructure: unsupported pattern {}",
            pattern.type_name()
        ))),
    }
}

pub fn eval(form: &Value, env: &Env) -> Result<Value> {
    match form {
        Value::Nil
        | Value::Bool(_)
        | Value::Int(_)
        | Value::Float(_)
        | Value::Str(_)
        | Value::Keyword(_)
        | Value::Fn(_)
        | Value::Macro(_)
        | Value::Builtin(_)
        | Value::Native(_)
        | Value::Atom(_)
        | Value::Regex(_)
        | Value::Multi(_)
        | Value::LazySeq(_)
        | Value::Cons(_, _)
        | Value::Reduced(_)
        | Value::Ratio(_, _) => Ok(form.clone()),
        #[cfg(feature = "gpu")]
        Value::GpuKernel(_) | Value::GpuPixelKernel(_) => Ok(form.clone()),
        Value::Symbol(s) => env.lookup(s),
        Value::Vector(xs) => {
            let mut out: imbl::Vector<Value> = imbl::Vector::new();
            for x in xs.iter() {
                out.push_back(eval(x, env)?);
            }
            Ok(Value::Vector(out))
        }
        Value::Map(pairs) => {
            let mut out: imbl::HashMap<Value, Value> = imbl::HashMap::new();
            for (k, v) in pairs.iter() {
                let k = eval(k, env)?;
                let v = eval(v, env)?;
                out.insert(k, v);
            }
            Ok(Value::Map(out))
        }
        Value::Set(xs) => {
            let mut out: imbl::HashSet<Value> = imbl::HashSet::new();
            for x in xs.iter() {
                out.insert(eval(x, env)?);
            }
            Ok(Value::Set(out))
        }
        Value::List(xs) => eval_list(xs, env),
    }
}

fn eval_list(xs: &[Value], env: &Env) -> Result<Value> {
    if xs.is_empty() {
        return Ok(Value::List(Arc::new(Vec::new())));
    }

    if let Value::Symbol(s) = &xs[0] {
        match s.as_ref() {
            "quote" => return sf_quote(&xs[1..]),
            "def" => return sf_def(&xs[1..], env),
            "if" => return sf_if(&xs[1..], env),
            "do" => return sf_do(&xs[1..], env),
            "let" => return sf_let(&xs[1..], env),
            "fn" => return sf_fn(&xs[1..], env, None),
            "defn" => return sf_defn(&xs[1..], env),
            "defn-native" => return sf_defn_native(&xs[1..], env),
            #[cfg(feature = "gpu")]
            "defn-gpu" => return sf_defn_gpu(&xs[1..], env),
            #[cfg(feature = "gpu")]
            "defn-gpu-pixel" => return sf_defn_gpu_pixel(&xs[1..], env),
            "defmulti" => return sf_defmulti(&xs[1..], env),
            "defmethod" => return sf_defmethod(&xs[1..], env),
            "defmacro" => return sf_defmacro(&xs[1..], env),
            "macroexpand" => return sf_macroexpand(&xs[1..], env),
            "macroexpand-1" => return sf_macroexpand_1(&xs[1..], env),
            "loop" => return sf_loop(&xs[1..], env),
            "recur" => return sf_recur(&xs[1..], env),
            "__tagged__" => return sf_tagged(&xs[1..], env),
            "ns" => return sf_ns(&xs[1..], env),
            "in-ns" => return sf_ns(&xs[1..], env),
            "load-file" => return sf_load_file(&xs[1..], env),
            "require" => return sf_require(&xs[1..], env),
            "try" => return sf_try(&xs[1..], env),
            _ => {}
        }
    }

    // Macro expansion: if head resolves to Value::Macro, expand with unevaluated
    // args and re-evaluate the result in the same env.
    if let Value::Symbol(s) = &xs[0]
        && let Ok(Value::Macro(lam)) = env.lookup(s)
    {
        let expanded = invoke_lambda(&lam, &xs[1..])?;
        return eval(&expanded, env);
    }

    let f = eval(&xs[0], env)?;
    let mut args = Vec::with_capacity(xs.len() - 1);
    for x in &xs[1..] {
        args.push(eval(x, env)?);
    }
    apply(&f, &args)
}

/// Clojure semantics: keywords, maps, and sets can be invoked as functions.
///   (:k m)           — lookup key `:k` in map `m` (nil if missing, 2-arg default supported)
///   (m :k)           — same, reversed
///   (m :k default)   — lookup with fallback
///   (#{1 2 3} x)     — set membership test (returns the member or nil)
///   (vec i)          — nth into the vector
fn invoke_collection(f: &Value, args: &[Value]) -> Result<Value> {
    match f {
        Value::Keyword(_) | Value::Symbol(_) => {
            if args.is_empty() || args.len() > 2 {
                return Err(Error::Arity {
                    expected: "1 or 2".into(),
                    got: args.len(),
                });
            }
            let default = args.get(1).cloned().unwrap_or(Value::Nil);
            match &args[0] {
                Value::Map(m) => Ok(m.get(f).cloned().unwrap_or(default)),
                Value::Nil => Ok(default),
                _ => Ok(default),
            }
        }
        Value::Map(m) => {
            if args.is_empty() || args.len() > 2 {
                return Err(Error::Arity {
                    expected: "1 or 2".into(),
                    got: args.len(),
                });
            }
            let default = args.get(1).cloned().unwrap_or(Value::Nil);
            Ok(m.get(&args[0]).cloned().unwrap_or(default))
        }
        Value::Set(s) => {
            if args.len() != 1 {
                return Err(Error::Arity {
                    expected: "1".into(),
                    got: args.len(),
                });
            }
            Ok(if s.contains(&args[0]) {
                args[0].clone()
            } else {
                Value::Nil
            })
        }
        Value::Vector(v) => {
            if args.len() != 1 {
                return Err(Error::Arity {
                    expected: "1".into(),
                    got: args.len(),
                });
            }
            let i = match &args[0] {
                Value::Int(n) => *n,
                _ => return Err(Error::Type("vector invocation: index must be int".into())),
            };
            if i < 0 {
                return Err(Error::Eval(format!("vector lookup: negative index {i}")));
            }
            v.get(i as usize)
                .cloned()
                .ok_or_else(|| Error::Eval(format!("vector lookup: index {i} out of range")))
        }
        _ => Err(Error::Type(format!("not callable: {}", f.type_name()))),
    }
}

const SPECIAL_FORMS: &[&str] = &[
    "quote",
    "def",
    "if",
    "do",
    "let",
    "fn",
    "defn",
    "defn-native",
    "defn-gpu",
    "defn-gpu-pixel",
    "defmulti",
    "defmethod",
    "defmacro",
    "macroexpand",
    "macroexpand-1",
    "loop",
    "recur",
    "__tagged__",
    "ns",
    "in-ns",
    "load-file",
    "require",
    "try",
    "catch",
    "finally",
    "throw",
];

/// Recursively macro-expand every form in an AST — walk the tree,
/// expand any macro call to fixpoint, and descend into the result's
/// sub-forms. Used by the GPU DSL (and anywhere else that takes a
/// body and compiles it without going through `eval`).
///
/// Unlike `eval`, nothing is executed; only macro calls are rewritten.
/// Special forms are preserved intact — their semantics are implemented
/// by the consumer (e.g. the GPU emitter).
pub fn macroexpand_all(form: &Value, env: &Env) -> Result<Value> {
    // Expand any top-level macro call to fixpoint first.
    let mut cur = form.clone();
    while let Some(next) = try_macro_expand_once(&cur, env)? {
        cur = next;
    }
    // Then recurse into sub-forms.
    match cur {
        Value::List(xs) => {
            let mut out = Vec::with_capacity(xs.len());
            // Don't re-expand inside `quote` — its argument is data, not
            // code, and some macro libraries stash symbols called `quote`
            // intentionally. This mirrors how most Clojure-family
            // implementations preserve quoted forms verbatim.
            if let Some(Value::Symbol(s)) = xs.first()
                && s.as_ref() == "quote"
            {
                return Ok(Value::List(xs));
            }
            for f in xs.iter() {
                out.push(macroexpand_all(f, env)?);
            }
            Ok(Value::List(Arc::new(out)))
        }
        Value::Vector(xs) => {
            let mut out: imbl::Vector<Value> = imbl::Vector::new();
            for f in xs.iter() {
                out.push_back(macroexpand_all(f, env)?);
            }
            Ok(Value::Vector(out))
        }
        _ => Ok(cur),
    }
}

fn try_macro_expand_once(form: &Value, env: &Env) -> Result<Option<Value>> {
    let Value::List(xs) = form else {
        return Ok(None);
    };
    if xs.is_empty() {
        return Ok(None);
    }
    let Value::Symbol(s) = &xs[0] else {
        return Ok(None);
    };
    if SPECIAL_FORMS.contains(&s.as_ref()) {
        return Ok(None);
    }
    let Ok(v) = env.lookup(s) else {
        return Ok(None);
    };
    let Value::Macro(lam) = v else {
        return Ok(None);
    };
    let expanded = invoke_lambda(&lam, &xs[1..])?;
    Ok(Some(expanded))
}

pub fn apply(f: &Value, args: &[Value]) -> Result<Value> {
    match f {
        Value::Builtin(b) => (b.f)(args),
        Value::Fn(lam) => invoke_lambda(lam, args),
        Value::Native(nf) => nf.invoke(args),
        Value::Keyword(_) | Value::Map(_) | Value::Set(_) | Value::Vector(_) => {
            invoke_collection(f, args)
        }
        Value::Multi(m) => invoke_multi(m, args),
        #[cfg(feature = "gpu")]
        Value::GpuKernel(k) => invoke_gpu_kernel(k, args),
        _ => Err(Error::Type(format!("not callable: {}", f.type_name()))),
    }
}

/// Multimethod dispatch: run the dispatch fn on the call args, look up
/// the result in the methods table, fall back to `:default`.
fn invoke_multi(m: &Arc<crate::value::MultiFn>, args: &[Value]) -> Result<Value> {
    let dval = apply(&m.dispatch, args)?;
    let methods = m.methods.read().unwrap();
    let method = methods.get(&dval).cloned().or_else(|| {
        methods
            .get(&Value::Keyword(Arc::from("default")))
            .cloned()
    });
    drop(methods);
    match method {
        Some(f) => apply(&f, args),
        None => Err(Error::Eval(format!(
            "no method in multifn `{}` for dispatch value {}",
            m.name,
            dval.to_pr_string()
        ))),
    }
}

/// Run a compiled GPU kernel. Expects a single argument — a seq-able
/// collection of numbers (vector/list). Uploads it as f32, dispatches,
/// reads back into a cljrs vector of Value::Float.
#[cfg(feature = "gpu")]
fn invoke_gpu_kernel(
    k: &Arc<crate::gpu::GpuKernel>,
    args: &[Value],
) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    let items: Vec<f32> = match &args[0] {
        Value::Vector(v) => v
            .iter()
            .map(value_to_f32)
            .collect::<Result<Vec<_>>>()?,
        Value::List(v) => v
            .iter()
            .map(value_to_f32)
            .collect::<Result<Vec<_>>>()?,
        _ => {
            return Err(Error::Type(format!(
                "gpu kernel input must be a vector or list, got {}",
                args[0].type_name()
            )));
        }
    };
    let gpu = crate::gpu::global_gpu().map_err(Error::Eval)?;
    let out = k.run_f32(&gpu, &items).map_err(Error::Eval)?;
    let mut v: imbl::Vector<Value> = imbl::Vector::new();
    for x in out {
        v.push_back(Value::Float(x as f64));
    }
    Ok(Value::Vector(v))
}

#[cfg(feature = "gpu")]
fn value_to_f32(v: &Value) -> Result<f32> {
    match v {
        Value::Int(i) => Ok(*i as f32),
        Value::Float(f) => Ok(*f as f32),
        _ => Err(Error::Type(format!(
            "gpu kernel input: non-number {}",
            v.type_name()
        ))),
    }
}

fn invoke_lambda(lam: &Arc<Lambda>, args: &[Value]) -> Result<Value> {
    let mut current: Vec<Value> = args.to_vec();
    let n = lam.params.len();
    let capacity = n + lam.variadic.is_some() as usize + lam.name.is_some() as usize;
    loop {
        let mut scope: Vec<(Arc<str>, Value)> = Vec::with_capacity(capacity);

        if let Some(name) = &lam.name {
            scope.push((Arc::clone(name), Value::Fn(Arc::clone(lam))));
        }

        if let Some(rest) = &lam.variadic {
            if current.len() < n {
                return Err(Error::Arity {
                    expected: format!(">= {n}"),
                    got: current.len(),
                });
            }
            for (p, a) in lam.params.iter().zip(&current[..n]) {
                scope.push((Arc::clone(p), a.clone()));
            }
            let rest_list: Vec<Value> = current[n..].to_vec();
            scope.push((Arc::clone(rest), Value::List(Arc::new(rest_list))));
        } else {
            if current.len() != n {
                return Err(Error::Arity {
                    expected: format!("{n}"),
                    got: current.len(),
                });
            }
            for (p, a) in lam.params.iter().zip(&current) {
                scope.push((Arc::clone(p), a.clone()));
            }
        }

        let new_env = lam.env.push_scope(scope);
        let mut result = Value::Nil;
        let mut recur_vals: Option<Vec<Value>> = None;
        for form in lam.body.iter() {
            match eval(form, &new_env) {
                Ok(v) => result = v,
                Err(Error::Recur(vals)) => {
                    recur_vals = Some(vals);
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        match recur_vals {
            None => return Ok(result),
            Some(vals) => current = vals,
        }
    }
}

fn sf_quote(args: &[Value]) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    Ok(args[0].clone())
}

fn sf_def(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity {
            expected: "2".into(),
            got: args.len(),
        });
    }
    let name = match &args[0] {
        Value::Symbol(s) => s.clone(),
        _ => return Err(Error::Eval("def requires symbol as first arg".into())),
    };
    let val = eval(&args[1], env)?;
    env.define_global(&name, val.clone());
    Ok(val)
}

fn sf_if(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() < 2 || args.len() > 3 {
        return Err(Error::Arity {
            expected: "2 or 3".into(),
            got: args.len(),
        });
    }
    let cond = eval(&args[0], env)?;
    if cond.truthy() {
        eval(&args[1], env)
    } else if args.len() == 3 {
        eval(&args[2], env)
    } else {
        Ok(Value::Nil)
    }
}

fn sf_do(args: &[Value], env: &Env) -> Result<Value> {
    let mut result = Value::Nil;
    for f in args {
        result = eval(f, env)?;
    }
    Ok(result)
}

fn sf_let(args: &[Value], env: &Env) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Eval("let requires bindings vector".into()));
    }
    let bindings = match &args[0] {
        Value::Vector(v) => v,
        _ => return Err(Error::Eval("let bindings must be a vector".into())),
    };
    if bindings.len() % 2 != 0 {
        return Err(Error::Eval("let bindings must have even count".into()));
    }
    // Expand destructuring into a flat list of (symbol, expr) pairs.
    let mut expanded: Vec<(Value, Value)> = Vec::with_capacity(bindings.len() / 2);
    let mut i = 0;
    while i < bindings.len() {
        expand_destructure(&bindings[i], &bindings[i + 1], &mut expanded)?;
        i += 2;
    }
    let mut cur = env.clone();
    for (pat, expr) in expanded {
        let name = match pat {
            Value::Symbol(s) => s,
            _ => return Err(Error::Eval("let: post-destructure name must be symbol".into())),
        };
        let val = eval(&expr, &cur)?;
        cur = cur.push_scope(vec![(name, val)]);
    }
    let mut result = Value::Nil;
    for f in &args[1..] {
        result = eval(f, &cur)?;
    }
    Ok(result)
}

fn sf_fn(args: &[Value], env: &Env, explicit_name: Option<Arc<str>>) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Eval("fn requires params vector".into()));
    }
    let mut name = explicit_name;
    // Skip past optional fn name.
    let start = match &args[0] {
        Value::Symbol(s) => {
            if args.len() < 2 {
                return Err(Error::Eval("fn with name requires params vector".into()));
            }
            if name.is_none() {
                name = Some(s.clone());
            }
            1
        }
        _ => 0,
    };
    // Multi-arity fn: `(fn ([x] ...) ([x y] ...))` — every arg from
    // `start` is a list (params-vec body...). Rewrite as a single
    // variadic fn with an arity-dispatch cond.
    if args.len() > start && matches!(&args[start], Value::List(xs) if xs.first().map(|f| matches!(f, Value::Vector(_))).unwrap_or(false))
        && args[start..].iter().all(|a| matches!(a, Value::List(xs) if xs.first().map(|f| matches!(f, Value::Vector(_))).unwrap_or(false)))
    {
        return build_multi_arity_fn(&args[start..], env, name);
    }
    let (params_val, body_start) = match &args[start] {
        Value::Vector(_) => (&args[start], start + 1),
        _ => return Err(Error::Eval("fn requires params vector".into())),
    };
    let params_vec: imbl::Vector<Value> = match params_val {
        Value::Vector(v) => v.clone(),
        _ => return Err(Error::Eval("fn params must be a vector".into())),
    };

    let mut params: Vec<Arc<str>> = Vec::new();
    let mut variadic: Option<Arc<str>> = None;
    // Destructuring patterns get replaced with fresh param names; the
    // original pattern gets rebound at the top of the body via a synthetic
    // `let`. We accumulate those rebindings here in source order.
    let mut destructure_pairs: Vec<Value> = Vec::new();
    let mut i = 0;
    while i < params_vec.len() {
        match &params_vec[i] {
            Value::Symbol(s) if s.as_ref() == "&" => {
                if i + 1 >= params_vec.len() {
                    return Err(Error::Eval("& must be followed by a symbol".into()));
                }
                variadic = match &params_vec[i + 1] {
                    Value::Symbol(r) => Some(r.clone()),
                    Value::Vector(_) | Value::Map(_) => {
                        // destructure the rest arg: `& [a b]` or `& {:keys [a b]}`
                        let fresh = match fresh_sym("rest") {
                            Value::Symbol(s) => s,
                            _ => unreachable!(),
                        };
                        destructure_pairs.push(params_vec[i + 1].clone());
                        destructure_pairs.push(Value::Symbol(Arc::clone(&fresh)));
                        Some(fresh)
                    }
                    _ => return Err(Error::Eval("& rest name must be a symbol or pattern".into())),
                };
                break;
            }
            Value::Symbol(s) => params.push(s.clone()),
            Value::Vector(_) | Value::Map(_) => {
                // Destructuring pattern: allocate a fresh param name and
                // schedule a destructuring let around the body.
                let fresh = match fresh_sym("p") {
                    Value::Symbol(s) => s,
                    _ => unreachable!(),
                };
                destructure_pairs.push(params_vec[i].clone());
                destructure_pairs.push(Value::Symbol(Arc::clone(&fresh)));
                params.push(fresh);
            }
            _ => return Err(Error::Eval("fn params must be symbols or patterns".into())),
        }
        i += 1;
    }

    let mut body: Vec<Value> = args[body_start..].to_vec();
    if !destructure_pairs.is_empty() {
        // Wrap the body in a single `let` that rebinds each pattern.
        // The original body forms become the let body.
        let let_bindings = Value::Vector(destructure_pairs.into_iter().collect());
        let mut let_form: Vec<Value> = Vec::with_capacity(2 + body.len());
        let_form.push(Value::Symbol(Arc::from("let")));
        let_form.push(let_bindings);
        let_form.extend(body);
        body = vec![Value::List(Arc::new(let_form))];
    }
    Ok(Value::Fn(Arc::new(Lambda {
        params,
        variadic,
        body: Arc::new(body),
        env: env.clone(),
        name,
    })))
}

/// Rewrite a multi-arity fn into a single variadic fn that dispatches
/// on (count args). Each arity's params-vec is re-used for
/// let-destructuring against the captured rest arg.
///
///   (fn ([x] x) ([x y] (+ x y)))
/// becomes
///   (fn [& __argsN__]
///     (cond
///       (= (count __argsN__) 1) (let [[x] __argsN__] x)
///       (= (count __argsN__) 2) (let [[x y] __argsN__] (+ x y))
///       :else (throw "no matching arity")))
///
/// Variadic arities (one with `& rest`) become an `:else` fallback.
fn build_multi_arity_fn(
    arities: &[Value],
    env: &Env,
    name: Option<Arc<str>>,
) -> Result<Value> {
    let args_sym = match fresh_sym("args") {
        Value::Symbol(s) => s,
        _ => unreachable!(),
    };
    // Build the cond clauses and optional variadic else-branch.
    let mut clauses: Vec<Value> = Vec::new();
    let mut variadic_branch: Option<Value> = None;

    for arity in arities {
        let xs = match arity {
            Value::List(v) => v.clone(),
            _ => return Err(Error::Eval("multi-arity fn: each arity must be a list".into())),
        };
        let params_vec = match xs.first() {
            Some(Value::Vector(pv)) => pv.clone(),
            _ => return Err(Error::Eval("multi-arity fn: arity must start with a params vector".into())),
        };
        let body: Vec<Value> = xs[1..].to_vec();
        // Is this arity variadic? Look for `&` in params.
        let has_amp = params_vec
            .iter()
            .any(|p| matches!(p, Value::Symbol(s) if s.as_ref() == "&"));
        let min_arity = if has_amp {
            // count params before `&`
            params_vec
                .iter()
                .take_while(|p| !matches!(p, Value::Symbol(s) if s.as_ref() == "&"))
                .count()
        } else {
            params_vec.len()
        };

        // Body wrapped in a destructuring let.
        let let_bindings = Value::Vector(
            vec![
                Value::Vector(params_vec.clone()),
                Value::Symbol(Arc::clone(&args_sym)),
            ]
            .into_iter()
            .collect(),
        );
        let mut let_form = vec![Value::Symbol(Arc::from("let")), let_bindings];
        let_form.extend(body);
        let body_expr = Value::List(Arc::new(let_form));

        if has_amp {
            // Variadic arity: matches (>= count min). Use as else fallback,
            // but only if not already set (last one wins).
            let guard = Value::List(Arc::new(vec![
                Value::Symbol(Arc::from(">=")),
                Value::List(Arc::new(vec![
                    Value::Symbol(Arc::from("count")),
                    Value::Symbol(Arc::clone(&args_sym)),
                ])),
                Value::Int(min_arity as i64),
            ]));
            variadic_branch = Some(body_expr.clone());
            clauses.push(guard);
            clauses.push(body_expr);
        } else {
            let guard = Value::List(Arc::new(vec![
                Value::Symbol(Arc::from("=")),
                Value::List(Arc::new(vec![
                    Value::Symbol(Arc::from("count")),
                    Value::Symbol(Arc::clone(&args_sym)),
                ])),
                Value::Int(min_arity as i64),
            ]));
            clauses.push(guard);
            clauses.push(body_expr);
        }
    }
    // Fallthrough: throw if no match (only reached when all arities are
    // fixed and none matched).
    if variadic_branch.is_none() {
        clauses.push(Value::Keyword(Arc::from("else")));
        clauses.push(Value::List(Arc::new(vec![
            Value::Symbol(Arc::from("throw")),
            Value::Str(Arc::from("no matching arity for multi-fn call")),
        ])));
    }

    let mut cond_form = vec![Value::Symbol(Arc::from("cond"))];
    cond_form.extend(clauses);
    let body = Value::List(Arc::new(cond_form));

    // Rebuild as (fn [name] [& args] body) and pass to sf_fn.
    let params_vec = Value::Vector(
        vec![
            Value::Symbol(Arc::from("&")),
            Value::Symbol(Arc::clone(&args_sym)),
        ]
        .into_iter()
        .collect(),
    );
    let mut synth: Vec<Value> = Vec::new();
    if let Some(n) = &name {
        synth.push(Value::Symbol(Arc::clone(n)));
    }
    synth.push(params_vec);
    synth.push(body);
    sf_fn(&synth, env, name)
}

fn sf_loop(args: &[Value], env: &Env) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Eval("loop requires bindings vector".into()));
    }
    let bindings: imbl::Vector<Value> = match &args[0] {
        Value::Vector(v) => v.clone(),
        _ => return Err(Error::Eval("loop bindings must be a vector".into())),
    };
    if bindings.len() % 2 != 0 {
        return Err(Error::Eval("loop bindings must have even count".into()));
    }

    let mut names: Vec<Arc<str>> = Vec::new();
    let mut cur = env.clone();
    let mut i = 0;
    while i < bindings.len() {
        let name = match &bindings[i] {
            Value::Symbol(s) => Arc::clone(s),
            _ => return Err(Error::Eval("loop binding name must be a symbol".into())),
        };
        let val = eval(&bindings[i + 1], &cur)?;
        cur = cur.push_scope(vec![(Arc::clone(&name), val)]);
        names.push(name);
        i += 2;
    }

    let body = &args[1..];
    loop {
        let mut result = Value::Nil;
        let mut recur_vals: Option<Vec<Value>> = None;
        for f in body {
            match eval(f, &cur) {
                Ok(v) => result = v,
                Err(Error::Recur(vals)) => {
                    recur_vals = Some(vals);
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        match recur_vals {
            None => return Ok(result),
            Some(vals) => {
                if vals.len() != names.len() {
                    return Err(Error::Arity {
                        expected: format!("recur target takes {}", names.len()),
                        got: vals.len(),
                    });
                }
                let scope: Vec<(Arc<str>, Value)> = names
                    .iter()
                    .zip(vals)
                    .map(|(n, v)| (Arc::clone(n), v))
                    .collect();
                cur = env.push_scope(scope);
            }
        }
    }
}

fn sf_recur(args: &[Value], env: &Env) -> Result<Value> {
    let mut vals = Vec::with_capacity(args.len());
    for a in args {
        vals.push(eval(a, env)?);
    }
    Err(Error::Recur(vals))
}

fn sf_defn(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() < 2 {
        return Err(Error::Arity {
            expected: ">= 2".into(),
            got: args.len(),
        });
    }
    let name = match &args[0] {
        Value::Symbol(s) => s.clone(),
        _ => return Err(Error::Eval("defn requires a name symbol".into())),
    };
    let fn_val = sf_fn(&args[1..], env, Some(name.clone()))?;
    env.define_global(&name, fn_val.clone());
    Ok(fn_val)
}

/// `(ns name & ref-specs)` — switch the current namespace. Optional
/// `(:require [target :as alias] ...)` specs install per-namespace
/// aliases so qualified lookups through `alias/name` resolve into the
/// target ns. Unknown ref-keys are ignored (not errors) to stay
/// forward-compatible with Clojure ns forms that include richer specs.
fn sf_ns(args: &[Value], env: &Env) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::Eval("ns requires a name".into()));
    }
    let name = match &args[0] {
        Value::Symbol(s) => s.to_string(),
        _ => return Err(Error::Eval("ns name must be a symbol".into())),
    };
    env.set_current_ns(&name);
    // Optional :require specs.
    for spec in &args[1..] {
        if let Value::List(xs) = spec
            && let Some(Value::Keyword(k)) = xs.first()
            && k.as_ref() == "require"
        {
            for req in &xs[1..] {
                apply_require_spec(env, &name, req)?;
            }
        }
    }
    Ok(Value::Nil)
}

fn apply_require_spec(env: &Env, consumer: &str, spec: &Value) -> Result<()> {
    match spec {
        // Simple (require 'foo.bar) — load only, no alias.
        Value::Symbol(s) => {
            try_load_ns(env, s.as_ref())?;
            Ok(())
        }
        Value::List(_) | Value::Vector(_) => {
            let items: Vec<Value> = match spec {
                Value::List(v) => v.as_ref().clone(),
                Value::Vector(v) => v.iter().cloned().collect(),
                _ => unreachable!(),
            };
            if items.is_empty() {
                return Ok(());
            }
            let target = match &items[0] {
                Value::Symbol(s) => s.to_string(),
                _ => return Err(Error::Eval("require: target must be a symbol".into())),
            };
            try_load_ns(env, &target)?;
            // Walk key/value pairs for :as / :refer.
            let mut i = 1;
            while i + 1 <= items.len() {
                if i + 1 >= items.len() {
                    break;
                }
                let k = &items[i];
                let v = &items[i + 1];
                if let Value::Keyword(kw) = k {
                    match kw.as_ref() {
                        "as" => {
                            if let Value::Symbol(a) = v {
                                env.add_alias(consumer, a.as_ref(), &target);
                            }
                        }
                        "refer" => {
                            if let Value::Vector(vs) = v {
                                for n in vs.iter() {
                                    if let Value::Symbol(nm) = n {
                                        let key = format!("{target}/{nm}");
                                        if let Ok(val) = env.lookup(&key) {
                                            // Install a copy into consumer's ns.
                                            let local = format!("{consumer}/{nm}");
                                            env.define_global(&local, val);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                i += 2;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Try to load a namespace by mapping `foo.bar` to `foo/bar.clj` via
/// load-file. Failure is soft: if the file isn't found we assume the
/// namespace was established some other way.
fn try_load_ns(env: &Env, ns: &str) -> Result<()> {
    let path = ns.replace('.', "/").replace('-', "_") + ".clj";
    if std::path::Path::new(&path).exists() {
        sf_load_file(&[Value::Str(Arc::from(path.as_str()))], env)?;
    }
    Ok(())
}

/// `(load-file "path")` — read the given cljrs source file and evaluate
/// every form in the current env. Minimum viable multi-file support.
/// Path is resolved relative to the process's current working directory.
fn sf_load_file(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    let path = eval(&args[0], env)?;
    let path_str = match &path {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(Error::Type(format!(
                "load-file: path must be a string, got {}",
                path.type_name()
            )));
        }
    };
    let src = std::fs::read_to_string(path_str.as_ref()).map_err(|e| {
        Error::Eval(format!("load-file: failed to read `{path_str}`: {e}"))
    })?;
    let forms = crate::reader::read_all(&src)?;
    let mut last = Value::Nil;
    for f in forms {
        last = eval(&f, env)?;
    }
    Ok(last)
}

/// `(require 'some.ns)` — Arc-1 interprets this as "load the file
/// whose path is derived from the ns name". We look up cljrs source
/// at `some/ns.clj` relative to CWD. Real namespace semantics (with
/// :as aliases, refer lists, isolation) are Arc 2.
fn sf_require(args: &[Value], env: &Env) -> Result<Value> {
    for a in args {
        // Accept (require 'ns) — a quoted symbol — or a string path.
        let evaluated = eval(a, env)?;
        let name: String = match &evaluated {
            Value::Symbol(s) => s.to_string(),
            Value::Str(s) => s.to_string(),
            _ => {
                return Err(Error::Type(format!(
                    "require: expected symbol or string, got {}",
                    evaluated.type_name()
                )));
            }
        };
        let path = name.replace('.', "/").replace('-', "_") + ".clj";
        sf_load_file(&[Value::Str(Arc::from(path.as_str()))], env)?;
    }
    Ok(Value::Nil)
}

/// Transparent type-hint eval. `(__tagged__ Tag form)` evaluates `form`
/// and discards the tag — the tag only matters to macros/special forms
/// that inspect structure (notably `defn-native`). This keeps existing
/// tree-walker code working unchanged in the presence of `^Type` hints.
fn sf_tagged(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity {
            expected: "2".into(),
            got: args.len(),
        });
    }
    eval(&args[1], env)
}

/// `(defn-native name ^RetType [^Type param ...] body...)`
///
/// Phase 1 (now): parses + validates the typed signature. Every param must
/// carry a `^Type` hint; types must be one of {i64, f64, bool, long, double}.
/// Return type is optional (defaults to i64). The fn is then installed via
/// the normal tree-walker path so it runs correctly today.
///
/// Phase 2: this special form will invoke the MLIR codegen pipeline to JIT
/// the body into native machine code and register it as a Value::Native.
fn sf_defn_native(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() < 2 {
        return Err(Error::Arity {
            expected: ">= 2".into(),
            got: args.len(),
        });
    }
    let name = match &args[0] {
        Value::Symbol(s) => Arc::clone(s),
        _ => return Err(Error::Eval("defn-native requires a name symbol".into())),
    };

    let (return_type, params_vec): (PrimType, imbl::Vector<Value>) = {
        let a1 = &args[1];
        if let Some((tag, inner)) = unwrap_tagged(a1) {
            let rt = parse_type_name(tag)?;
            let pv = match inner {
                Value::Vector(v) => v.clone(),
                _ => {
                    return Err(Error::Eval(
                        "defn-native: return-type hint must wrap a params vector".into(),
                    ));
                }
            };
            (rt, pv)
        } else if let Value::Vector(v) = a1 {
            (PrimType::I64, v.clone())
        } else {
            return Err(Error::Eval(
                "defn-native: expected params vector (optionally ^Type-tagged)".into(),
            ));
        }
    };

    let mut typed_params: Vec<(Arc<str>, PrimType)> = Vec::with_capacity(params_vec.len());
    for p in params_vec.iter() {
        let (tag, inner) = unwrap_tagged(p).ok_or_else(|| {
            Error::Eval("defn-native: every param must be ^Type name".into())
        })?;
        let ty = parse_type_name(tag)?;
        let nm = match inner {
            Value::Symbol(s) => Arc::clone(s),
            _ => {
                return Err(Error::Eval(
                    "defn-native: param name must be a symbol".into(),
                ));
            }
        };
        typed_params.push((nm, ty));
    }

    // With the `mlir` feature on, JIT-compile the body to native code and
    // register a `Value::Native`. Without the feature, fall through to the
    // tree-walker `defn` path so tests still pass.
    #[cfg(feature = "mlir")]
    {
        // defn-native bodies can have multiple forms; wrap in an implicit
        // `do` so the emitter sees a single form.
        let body_form = if args.len() == 3 {
            args[2].clone()
        } else {
            let mut do_form = Vec::with_capacity(args.len() - 1);
            do_form.push(Value::Symbol(Arc::from("do")));
            do_form.extend(args[2..].iter().cloned());
            Value::List(Arc::new(do_form))
        };

        // Snapshot previously-defined natives so the new fn's body can
        // call them via MLIR's cross-fn resolution.
        let registry = env.snapshot_natives();
        match crate::codegen::mlir::compile::compile_native_fn(
            &name,
            &typed_params,
            return_type,
            &body_form,
            &registry,
        ) {
            Ok(native_fn) => {
                let v = Value::Native(Arc::new(native_fn));
                env.define_global(&name, v.clone());
                return Ok(v);
            }
            Err(e) => {
                // MLIR codegen refused this body (unsupported form, etc.).
                // Surface the error rather than silently falling back —
                // a silent fallback hides perf regressions.
                return Err(Error::Eval(format!(
                    "defn-native `{name}` failed to compile: {e}"
                )));
            }
        }
    }

    #[cfg(not(feature = "mlir"))]
    {
        let _ = return_type;
        let fn_params_vec: Vec<Value> = typed_params
            .iter()
            .map(|(n, _)| Value::Symbol(Arc::clone(n)))
            .collect();
        let mut fn_args: Vec<Value> = Vec::with_capacity(1 + args.len().saturating_sub(2));
        fn_args.push(Value::Vector(fn_params_vec.into_iter().collect()));
        fn_args.extend(args[2..].iter().cloned());
        let fn_val = sf_fn(&fn_args, env, Some(Arc::clone(&name)))?;
        env.define_global(&name, fn_val.clone());
        Ok(fn_val)
    }
}

/// `(defn-gpu name ^f32 [^i32 i ^f32 v] body...)` — compile body to WGSL
/// and register a GPU kernel callable from cljrs. Elementwise-f32 ABI
/// only in this pass: one input buffer, one output buffer, same length.
/// Later we'll extend to multiple buffers, uniforms, and reductions.
#[cfg(feature = "gpu")]
fn sf_defn_gpu(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() < 3 {
        return Err(Error::Arity {
            expected: ">= 3".into(),
            got: args.len(),
        });
    }
    let name = match &args[0] {
        Value::Symbol(s) => Arc::clone(s),
        _ => return Err(Error::Eval("defn-gpu requires a name symbol".into())),
    };

    // Return type + params vector. Follow the same `^Type [^Type p...]`
    // shape as defn-native so it reads identical.
    let params_val: &Value = if let Some((_, inner)) = unwrap_tagged(&args[1]) {
        // optional ^f32 return hint — we only accept f32 today; ignore.
        inner
    } else {
        &args[1]
    };
    let params = match params_val {
        Value::Vector(v) => v,
        _ => return Err(Error::Eval("defn-gpu: expected params vector".into())),
    };
    if params.len() != 2 {
        return Err(Error::Eval(
            "defn-gpu (v0): exactly two params required — [^i32 idx ^f32 value]".into(),
        ));
    }
    // Unwrap the index param.
    let (idx_name, val_name) = {
        let (_, p0) = unwrap_tagged(&params[0]).ok_or_else(|| {
            Error::Eval("defn-gpu: first param must be ^i32 name".into())
        })?;
        let (_, p1) = unwrap_tagged(&params[1]).ok_or_else(|| {
            Error::Eval("defn-gpu: second param must be ^f32 name".into())
        })?;
        let idx_name = match p0 {
            Value::Symbol(s) => s.to_string(),
            _ => return Err(Error::Eval("defn-gpu: idx name must be a symbol".into())),
        };
        let val_name = match p1 {
            Value::Symbol(s) => s.to_string(),
            _ => return Err(Error::Eval("defn-gpu: value name must be a symbol".into())),
        };
        (idx_name, val_name)
    };

    // Wrap multi-form bodies in an implicit `do`.
    let body: Value = if args.len() == 3 {
        args[2].clone()
    } else {
        let mut do_form = Vec::with_capacity(args.len() - 1);
        do_form.push(Value::Symbol(Arc::from("do")));
        do_form.extend(args[2..].iter().cloned());
        Value::List(Arc::new(do_form))
    };

    let wgsl = crate::gpu::emit::emit_elementwise(&idx_name, &val_name, &body)
        .map_err(|e| Error::Eval(format!("defn-gpu `{name}`: {e}")))?;
    let kernel = crate::gpu::GpuKernel::from_wgsl(name.to_string(), wgsl);
    let v = Value::GpuKernel(Arc::new(kernel));
    env.define_global(&name, v.clone());
    Ok(v)
}

/// `(defn-gpu-pixel name [x y w h t-ms s0 s1 s2 s3] body...)` —
/// compile a 2D pixel-shader kernel. Body returns a packed u32 color
/// (0x00RRGGBB). Params are all i32; no type hints needed (they're
/// fixed by the ABI). The kernel is stored as a `Value::GpuPixelKernel`
/// and dispatched by `render_frame` at the host level (not via normal
/// `apply` — pixel kernels need uniforms + 2D dispatch).
#[cfg(feature = "gpu")]
fn sf_defn_gpu_pixel(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() < 3 {
        return Err(Error::Arity { expected: ">= 3".into(), got: args.len() });
    }
    let name = match &args[0] {
        Value::Symbol(s) => Arc::clone(s),
        _ => return Err(Error::Eval("defn-gpu-pixel: requires a name symbol".into())),
    };
    let params_vec = match &args[1] {
        Value::Vector(v) => v,
        _ => return Err(Error::Eval("defn-gpu-pixel: expected [x y w h t s0 s1 s2 s3]".into())),
    };
    if params_vec.len() != 9 {
        return Err(Error::Eval(
            "defn-gpu-pixel: param vector must be exactly [x y width height t-ms s0 s1 s2 s3]".into(),
        ));
    }
    let mut names: Vec<String> = Vec::with_capacity(9);
    for p in params_vec.iter() {
        // Accept bare symbols or tagged `^i32 sym` for consistency; the
        // tag is ignored because the ABI is fixed.
        let sym = if let Some((_, inner)) = unwrap_tagged(p) { inner } else { p };
        match sym {
            Value::Symbol(s) => names.push(s.to_string()),
            _ => return Err(Error::Eval("defn-gpu-pixel: params must be symbols".into())),
        }
    }
    let param_refs: [&str; 9] = std::array::from_fn(|i| names[i].as_str());

    let body: Value = if args.len() == 3 {
        args[2].clone()
    } else {
        let mut do_form = Vec::with_capacity(args.len() - 1);
        do_form.push(Value::Symbol(Arc::from("do")));
        do_form.extend(args[2..].iter().cloned());
        Value::List(Arc::new(do_form))
    };

    // Expand macros in the body so the GPU DSL can use defmacro-defined
    // helpers (e.g. a sample-mandel macro that factors out an AA loop).
    let expanded = macroexpand_all(&body, env)?;
    let wgsl = crate::gpu::emit::emit_pixel(&param_refs, &expanded)
        .map_err(|e| Error::Eval(format!("defn-gpu-pixel `{name}`: {e}")))?;
    let kernel = crate::gpu::GpuPixelKernel::from_wgsl(name.to_string(), wgsl);
    let v = Value::GpuPixelKernel(Arc::new(kernel));
    env.define_global(&name, v.clone());
    Ok(v)
}

/// `(defmulti name dispatch-fn)` — introduce a multimethod. Stores a
/// `Value::Multi` under `name`. Dispatch fn is evaluated immediately
/// (it's a normal fn applied to each call's args).
fn sf_defmulti(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() != 2 {
        return Err(Error::Arity { expected: "2".into(), got: args.len() });
    }
    let name = match &args[0] {
        Value::Symbol(s) => Arc::clone(s),
        _ => return Err(Error::Eval("defmulti: name must be a symbol".into())),
    };
    let dispatch = eval(&args[1], env)?;
    let multi = crate::value::MultiFn {
        name: Arc::clone(&name),
        dispatch,
        methods: std::sync::RwLock::new(imbl::HashMap::new()),
    };
    let v = Value::Multi(Arc::new(multi));
    env.define_global(&name, v.clone());
    Ok(v)
}

/// `(defmethod name dispatch-val [params] body...)` — register a method
/// on the multimethod named `name`. Looks up the multi, evaluates the
/// fn (a lambda over the params), and inserts into the methods map.
fn sf_defmethod(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() < 3 {
        return Err(Error::Arity { expected: ">= 3".into(), got: args.len() });
    }
    let name = match &args[0] {
        Value::Symbol(s) => s.clone(),
        _ => return Err(Error::Eval("defmethod: name must be a symbol".into())),
    };
    let dispatch_val = eval(&args[1], env)?;
    // The rest of args is `[params] body...` — build a fn value.
    let fn_val = sf_fn(&args[2..], env, None)?;
    let multi_val = env.lookup(&name)?;
    let multi = match &multi_val {
        Value::Multi(m) => m.clone(),
        _ => return Err(Error::Eval(format!(
            "defmethod: `{name}` is not a multimethod"
        ))),
    };
    multi.methods.write().unwrap().insert(dispatch_val, fn_val);
    Ok(multi_val)
}

fn sf_defmacro(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() < 2 {
        return Err(Error::Arity {
            expected: ">= 2".into(),
            got: args.len(),
        });
    }
    let name = match &args[0] {
        Value::Symbol(s) => s.clone(),
        _ => return Err(Error::Eval("defmacro requires a name symbol".into())),
    };
    let fn_val = sf_fn(&args[1..], env, Some(name.clone()))?;
    let lam = match fn_val {
        Value::Fn(lam) => lam,
        _ => unreachable!("sf_fn returned non-Fn"),
    };
    let macro_val = Value::Macro(lam);
    env.define_global(&name, macro_val.clone());
    Ok(macro_val)
}

fn sf_macroexpand_1(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    let form = eval(&args[0], env)?;
    match try_macro_expand_once(&form, env)? {
        Some(f) => Ok(f),
        None => Ok(form),
    }
}

/// `(try body... (catch _ binding body...) (finally body...))` —
/// the `_` placeholder is where Clojure names an exception class; we
/// don't have types yet, so any thrown value matches the first catch.
/// `finally` body always runs after try+catch, for side effects only.
fn sf_try(args: &[Value], env: &Env) -> Result<Value> {
    let mut body: Vec<&Value> = Vec::new();
    let mut catch: Option<(Arc<str>, Vec<Value>)> = None;
    let mut finally: Vec<Value> = Vec::new();

    for f in args {
        if let Value::List(xs) = f
            && let Some(Value::Symbol(head)) = xs.first()
        {
            match head.as_ref() {
                "catch" => {
                    if xs.len() < 3 {
                        return Err(Error::Eval(
                            "try: (catch _ binding body...) required".into(),
                        ));
                    }
                    let bind_name = match &xs[2] {
                        Value::Symbol(s) => Arc::clone(s),
                        _ => return Err(Error::Eval("try: catch binding must be a symbol".into())),
                    };
                    let catch_body: Vec<Value> = xs[3..].to_vec();
                    catch = Some((bind_name, catch_body));
                    continue;
                }
                "finally" => {
                    finally = xs[1..].to_vec();
                    continue;
                }
                _ => {}
            }
        }
        body.push(f);
    }

    let run_finally = |env: &Env| -> Result<()> {
        for f in &finally {
            eval(f, env)?;
        }
        Ok(())
    };

    // Run body.
    let mut result = Value::Nil;
    let mut thrown: Option<Error> = None;
    for f in &body {
        match eval(f, env) {
            Ok(v) => result = v,
            Err(e) => {
                thrown = Some(e);
                break;
            }
        }
    }

    if let Some(err) = thrown {
        // Only catch Thrown (user-raised); let Recur and others propagate.
        let payload = match err {
            Error::Thrown(v) => v,
            Error::Eval(s) | Error::Type(s) | Error::Read(s) => Value::Str(Arc::from(s.as_str())),
            Error::Unbound(s) => Value::Str(Arc::from(format!("unbound: {s}").as_str())),
            Error::Arity { expected, got } => Value::Str(Arc::from(
                format!("arity: expected {expected}, got {got}").as_str(),
            )),
            other => return Err(other),
        };
        if let Some((name, catch_body)) = catch {
            let catch_env = env.push_scope(vec![(name, payload)]);
            let mut catch_result = Value::Nil;
            let catch_outcome: Result<Value> = (|| {
                for f in &catch_body {
                    catch_result = eval(f, &catch_env)?;
                }
                Ok(catch_result)
            })();
            run_finally(env)?;
            return catch_outcome;
        }
        run_finally(env)?;
        return Err(Error::Thrown(payload));
    }

    run_finally(env)?;
    Ok(result)
}

fn sf_macroexpand(args: &[Value], env: &Env) -> Result<Value> {
    if args.len() != 1 {
        return Err(Error::Arity {
            expected: "1".into(),
            got: args.len(),
        });
    }
    let mut cur = eval(&args[0], env)?;
    while let Some(f) = try_macro_expand_once(&cur, env)? {
        cur = f;
    }
    Ok(cur)
}
