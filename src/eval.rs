use std::sync::Arc;

use crate::env::Env;
use crate::error::{Error, Result};
use crate::types::{PrimType, parse_type_name, unwrap_tagged};
use crate::value::{Lambda, Value};

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
        | Value::Native(_) => Ok(form.clone()),
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

const SPECIAL_FORMS: &[&str] = &[
    "quote",
    "def",
    "if",
    "do",
    "let",
    "fn",
    "defn",
    "defn-native",
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
];

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
        _ => Err(Error::Type(format!("not callable: {}", f.type_name()))),
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
    let mut cur = env.clone();
    let mut i = 0;
    while i < bindings.len() {
        let name = match &bindings[i] {
            Value::Symbol(s) => Arc::clone(s),
            _ => return Err(Error::Eval("let binding name must be a symbol".into())),
        };
        let val = eval(&bindings[i + 1], &cur)?;
        cur = cur.push_scope(vec![(name, val)]);
        i += 2;
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
    let (params_val, body_start) = match &args[0] {
        Value::Vector(_) => (&args[0], 1usize),
        Value::Symbol(s) => {
            if args.len() < 2 {
                return Err(Error::Eval(
                    "fn with name requires params vector".into(),
                ));
            }
            if name.is_none() {
                name = Some(s.clone());
            }
            (&args[1], 2usize)
        }
        _ => return Err(Error::Eval("fn requires params vector".into())),
    };
    let params_vec: imbl::Vector<Value> = match params_val {
        Value::Vector(v) => v.clone(),
        _ => return Err(Error::Eval("fn params must be a vector".into())),
    };

    let mut params: Vec<Arc<str>> = Vec::new();
    let mut variadic: Option<Arc<str>> = None;
    let mut i = 0;
    while i < params_vec.len() {
        match &params_vec[i] {
            Value::Symbol(s) if s.as_ref() == "&" => {
                if i + 1 >= params_vec.len() {
                    return Err(Error::Eval("& must be followed by a symbol".into()));
                }
                variadic = match &params_vec[i + 1] {
                    Value::Symbol(r) => Some(r.clone()),
                    _ => return Err(Error::Eval("& rest name must be a symbol".into())),
                };
                break;
            }
            Value::Symbol(s) => params.push(s.clone()),
            _ => return Err(Error::Eval("fn params must be symbols".into())),
        }
        i += 1;
    }

    let body: Vec<Value> = args[body_start..].to_vec();
    Ok(Value::Fn(Arc::new(Lambda {
        params,
        variadic,
        body: Arc::new(body),
        env: env.clone(),
        name,
    })))
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

/// `(ns name)` / `(in-ns name)` — Arc-1 namespaces are cosmetic. Real
/// namespace isolation (each ns with its own bindings, :refer/:as) is
/// Arc 2 work. For now we accept the form so existing Clojure code with
/// `(ns my.ns)` headers parses and runs; all vars share a single flat
/// global map.
fn sf_ns(_args: &[Value], _env: &Env) -> Result<Value> {
    Ok(Value::Nil)
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
