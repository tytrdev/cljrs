//! Browser UI bridge: wires `cljrs.ui/__ui-mount` and
//! `cljrs.ui/__ui-hydrate` to a small JS shim (`window.cljrsUi`,
//! defined in `docs/_ui.js`) that wraps Preact.
//!
//! Pattern mirrors `js_bridge`: each builtin is registered under both
//! the short `ui` and qualified `cljrs.ui` namespaces. cljrs hiccup
//! values (Vectors, Maps, Keywords, Strs, Ints, Floats, Fns) are
//! converted to JS arrays / objects / strings / numbers / wrapped
//! callbacks recursively. Event-handler props (keys whose name starts
//! with "on-") become JS functions that, when called, `eval::apply`
//! the cljrs callable.
//!
//! Closure lifetime: every wrapped event handler is `Closure::wrap`ed
//! and `forget`ed (page-lifetime leak). Same trade-off as `js/on!` —
//! diff'd Preact re-renders create a new closure per render call,
//! so very large render counts will leak. Acceptable for the demo
//! page; revisit if we hit it.

use cljrs::env::Env;
use cljrs::error::{Error, Result};
use cljrs::eval;
use cljrs::value::{Builtin, Value};
use js_sys::{Array, Function, Object, Reflect};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{console, window};

pub fn install(env: &Env) {
    let prev = env.current_ns();
    env.set_current_ns("ui");
    bind(env, "__ui-mount", ui_mount_fn);
    bind(env, "__ui-hydrate", ui_hydrate_fn);

    env.set_current_ns("cljrs.ui");
    bind(env, "__ui-mount", ui_mount_fn);
    bind(env, "__ui-hydrate", ui_hydrate_fn);

    env.set_current_ns(prev.as_ref());
}

fn bind(env: &Env, name: &'static str, f: fn(&[Value]) -> Result<Value>) {
    env.define_global(name, Value::Builtin(Builtin::new_static(name, f)));
}

fn arg_str<'a>(args: &'a [Value], idx: usize, name: &str) -> Result<&'a str> {
    match args.get(idx) {
        Some(Value::Str(s)) => Ok(s.as_ref()),
        Some(v) => Err(Error::Type(format!(
            "{name}: arg {idx} must be string, got {}",
            v.type_name()
        ))),
        None => Err(Error::Eval(format!("{name}: missing arg {idx}"))),
    }
}

/// Call into `window.cljrsUi[method](root_id, tree)`. Returns Nil on
/// success; surfaces a cljrs error if the global shim is missing.
fn call_shim(method: &str, root_id: &str, hiccup: &Value) -> Result<Value> {
    let win = window().ok_or_else(|| Error::Eval(format!("ui/{method}: no window")))?;
    let shim = Reflect::get(&win, &JsValue::from_str("cljrsUi"))
        .map_err(|_| Error::Eval(format!("ui/{method}: window.cljrsUi missing")))?;
    if shim.is_undefined() || shim.is_null() {
        return Err(Error::Eval(format!(
            "ui/{method}: window.cljrsUi not loaded (include docs/_ui.js)"
        )));
    }
    let f = Reflect::get(&shim, &JsValue::from_str(method))
        .map_err(|_| Error::Eval(format!("ui/{method}: cljrsUi.{method} missing")))?;
    let f: Function = f
        .dyn_into()
        .map_err(|_| Error::Eval(format!("ui/{method}: cljrsUi.{method} not a function")))?;
    let tree = hiccup_to_js(hiccup);
    f.call2(&shim, &JsValue::from_str(root_id), &tree)
        .map_err(|e| {
            console::error_1(&e);
            Error::Eval(format!("ui/{method}: shim threw"))
        })?;
    Ok(Value::Nil)
}

fn ui_mount_fn(args: &[Value]) -> Result<Value> {
    let root = arg_str(args, 0, "ui/__ui-mount")?;
    let hv = args.get(1).cloned().unwrap_or(Value::Nil);
    call_shim("mount", root, &hv)
}

fn ui_hydrate_fn(args: &[Value]) -> Result<Value> {
    let root = arg_str(args, 0, "ui/__ui-hydrate")?;
    let hv = args.get(1).cloned().unwrap_or(Value::Nil);
    call_shim("hydrate", root, &hv)
}

// ---------------------------------------------------------------
// Hiccup → JS conversion
// ---------------------------------------------------------------
//
// Hiccup vector  -> JS array [tag, propsObj, ...children]
// Map            -> JS plain object (keys stringified, callable values
//                   wrapped in Closure for event handlers).
// Keyword :foo   -> the string "foo" (for tag names and prop keys).
// Str/Int/Float  -> JS string/number.
// Vector/List    -> JS array (recursive).
// Fn/Builtin/Native -> wrapped 0..1-arg callback.
// Other          -> JSON-ish stringified via to_pr_string.

fn hiccup_to_js(v: &Value) -> JsValue {
    match v {
        Value::Nil => JsValue::NULL,
        Value::Bool(b) => JsValue::from_bool(*b),
        Value::Int(i) => JsValue::from_f64(*i as f64),
        Value::Float(f) => JsValue::from_f64(*f),
        Value::Str(s) => JsValue::from_str(s),
        Value::Keyword(k) => JsValue::from_str(k),
        Value::Symbol(s) => JsValue::from_str(s),
        Value::Vector(items) => {
            let arr = Array::new();
            for item in items.iter() {
                arr.push(&hiccup_to_js(item));
            }
            arr.into()
        }
        Value::List(items) => {
            let arr = Array::new();
            for item in items.iter() {
                arr.push(&hiccup_to_js(item));
            }
            arr.into()
        }
        Value::Map(m) => {
            let obj = Object::new();
            for (k, val) in m.iter() {
                let key = match k {
                    Value::Keyword(s) | Value::Str(s) | Value::Symbol(s) => s.to_string(),
                    other => other.to_pr_string(),
                };
                let js_val = if is_callable(val) && key.starts_with("on-") {
                    wrap_handler(val)
                } else {
                    hiccup_to_js(val)
                };
                let _ = Reflect::set(&obj, &JsValue::from_str(&key), &js_val);
            }
            obj.into()
        }
        Value::Cons(_, _) | Value::LazySeq(_) => {
            // Realize the seq into a flat Vec by walking head/tail.
            let arr = Array::new();
            let mut cur = v.clone();
            loop {
                match cur {
                    Value::Cons(h, t) => {
                        arr.push(&hiccup_to_js(&*h));
                        cur = (*t).clone();
                    }
                    Value::LazySeq(ls) => match ls.force() {
                        Ok(forced) => cur = forced,
                        Err(_) => break,
                    },
                    Value::Vector(items) => {
                        for it in items.iter() {
                            arr.push(&hiccup_to_js(it));
                        }
                        break;
                    }
                    Value::List(items) => {
                        for it in items.iter() {
                            arr.push(&hiccup_to_js(it));
                        }
                        break;
                    }
                    Value::Nil => break,
                    _ => break,
                }
            }
            arr.into()
        }
        Value::Fn(_) | Value::Builtin(_) | Value::Native(_) => wrap_handler(v),
        other => JsValue::from_str(&other.to_pr_string()),
    }
}

fn is_callable(v: &Value) -> bool {
    matches!(v, Value::Fn(_) | Value::Builtin(_) | Value::Native(_))
}

/// Wrap a cljrs callable in a JS function. The closure is leaked
/// (page-lifetime) — same as `js/on!`. Accepts 0..1 JS arg; if Preact
/// passes an event we drop it.
fn wrap_handler(v: &Value) -> JsValue {
    let handler = v.clone();
    let cb = Closure::wrap(Box::new(move |_arg: JsValue| {
        if let Err(e) = eval::apply(&handler, &[]) {
            console::error_1(&JsValue::from_str(&format!("ui handler: {e}")));
        }
    }) as Box<dyn FnMut(JsValue)>);
    let f: JsValue = cb.as_ref().clone();
    cb.forget();
    f
}
