//! Browser JS interop for cljrs, exposed under the `cljrs.js` namespace.
//!
//! This is wasm-only by virtue of living in the cljrs-wasm crate (which
//! is a `cdylib` for `wasm32-unknown-unknown`). Native cljrs builds never
//! see these symbols.
//!
//! Pattern mirrors `cljrs-physics`: each builtin is registered in the
//! `cljrs.js` ns via `env.define_global`. JS values that need to round-
//! trip through cljrs are wrapped in `Value::Opaque { tag: "js/element"
//! | "js/value", inner }` via the `JsValueWrapper` newtype below.
//!
//! # Send/Sync hazard
//!
//! `wasm_bindgen::JsValue` is intentionally `!Send + !Sync` because the
//! browser's main thread owns it. cljrs's `Value::Opaque.inner` is
//! `Arc<dyn Any + Send + Sync>` — so we wrap `JsValue` in a newtype and
//! `unsafe impl Send + Sync` for it. This is sound on `wasm32-unknown-
//! unknown` because that target has no real threads (Web Workers cannot
//! share JsValues by construction). Do not lift this code to a target
//! with real threads without revisiting.
//!
//! # Async callbacks
//!
//! `js/fetch-text` resolves asynchronously. We can't park a cljrs
//! computation on a JS Promise (cljrs is interpreter-driven and
//! re-entrant calls into `eval::apply` would clobber env state if the
//! caller were mid-evaluation). Instead the cljrs-side callback `Value`
//! is captured into a `wasm_bindgen_futures::spawn_local` task; when the
//! fetch resolves we `eval::apply` the callback with the response text.
//! Same trick for `js/on!`: we `Closure::wrap` the cljrs handler and
//! leak it via `Closure::forget` (one closure per listener; lifetime =
//! page lifetime, which is fine for the demo and matches the typical
//! use-case of "wire up a button once at startup").

use cljrs::env::Env;
use cljrs::error::{Error, Result};
use cljrs::eval;
use cljrs::value::{Builtin, Value};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{console, window, Event};

/// Newtype around `JsValue` so we can hand it to cljrs's `Opaque.inner`
/// (which demands `Send + Sync`). See module docs for soundness.
pub struct JsValueWrapper(pub JsValue);

// SAFETY: wasm32-unknown-unknown is single-threaded; JsValues are never
// actually shared across threads. See module docs.
unsafe impl Send for JsValueWrapper {}
unsafe impl Sync for JsValueWrapper {}

const TAG_EL: &str = "js/element";
const TAG_VAL: &str = "js/value";

pub fn install(env: &Env) {
    let prev = env.current_ns();
    // Install under both `js` (short, what demos write) and `cljrs.js`
    // (qualified; matches the project naming convention so user code in
    // a different ns can `(require '[cljrs.js :as foo])` if they want).
    env.set_current_ns("js");

    bind(env, "log", log_fn);
    bind(env, "now", now_fn);
    bind(env, "get-element", get_element_fn);
    bind(env, "set-text!", set_text_fn);
    bind(env, "set-html!", set_html_fn);
    bind(env, "on!", on_fn);
    bind(env, "local-get", local_get_fn);
    bind(env, "local-set!", local_set_fn);
    bind(env, "fetch-text", fetch_text_fn);

    // Mirror under cljrs.js so qualified-name lookups also resolve.
    env.set_current_ns("cljrs.js");
    bind(env, "log", log_fn);
    bind(env, "now", now_fn);
    bind(env, "get-element", get_element_fn);
    bind(env, "set-text!", set_text_fn);
    bind(env, "set-html!", set_html_fn);
    bind(env, "on!", on_fn);
    bind(env, "local-get", local_get_fn);
    bind(env, "local-set!", local_set_fn);
    bind(env, "fetch-text", fetch_text_fn);

    env.set_current_ns(prev.as_ref());
}

fn bind(env: &Env, name: &'static str, f: fn(&[Value]) -> Result<Value>) {
    env.define_global(name, Value::Builtin(Builtin::new_static(name, f)));
}

// --- arg helpers ------------------------------------------------------

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

fn arg_element(args: &[Value], idx: usize, name: &str) -> Result<JsValue> {
    match args.get(idx) {
        Some(Value::Opaque { tag, inner }) if tag.as_ref() == TAG_EL => {
            match Arc::clone(inner).downcast::<JsValueWrapper>() {
                Ok(w) => Ok(w.0.clone()),
                Err(_) => Err(Error::Type(format!(
                    "{name}: opaque {TAG_EL} downcast failed"
                ))),
            }
        }
        Some(v) => Err(Error::Type(format!(
            "{name}: arg {idx} must be {TAG_EL}, got {}",
            v.type_name()
        ))),
        None => Err(Error::Eval(format!("{name}: missing arg {idx}"))),
    }
}

fn arg_callable<'a>(args: &'a [Value], idx: usize, name: &str) -> Result<&'a Value> {
    match args.get(idx) {
        Some(v @ Value::Fn(_)) => Ok(v),
        Some(v @ Value::Builtin(_)) => Ok(v),
        Some(v) => Err(Error::Type(format!(
            "{name}: arg {idx} must be fn, got {}",
            v.type_name()
        ))),
        None => Err(Error::Eval(format!("{name}: missing arg {idx}"))),
    }
}

fn pr_str(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        Value::Nil => String::new(),
        other => other.to_pr_string(),
    }
}

fn opaque_el(js: JsValue) -> Value {
    Value::Opaque {
        tag: Arc::from(TAG_EL),
        inner: Arc::new(JsValueWrapper(js)),
    }
}

// --- builtins ---------------------------------------------------------

fn log_fn(args: &[Value]) -> Result<Value> {
    let msg = args
        .iter()
        .map(pr_str)
        .collect::<Vec<_>>()
        .join(" ");
    console::log_1(&JsValue::from_str(&msg));
    Ok(Value::Nil)
}

fn now_fn(_args: &[Value]) -> Result<Value> {
    Ok(Value::Float(js_sys::Date::now()))
}

fn get_element_fn(args: &[Value]) -> Result<Value> {
    let id = arg_str(args, 0, "js/get-element")?;
    let win = window().ok_or_else(|| Error::Eval("js/get-element: no window".into()))?;
    let doc = win
        .document()
        .ok_or_else(|| Error::Eval("js/get-element: no document".into()))?;
    match doc.get_element_by_id(id) {
        Some(el) => Ok(opaque_el(el.into())),
        None => Ok(Value::Nil),
    }
}

fn set_text_fn(args: &[Value]) -> Result<Value> {
    let el_js = arg_element(args, 0, "js/set-text!")?;
    let el: web_sys::Element = el_js
        .dyn_into()
        .map_err(|_| Error::Type("js/set-text!: not an Element".into()))?;
    let text = match args.get(1) {
        Some(Value::Str(s)) => s.to_string(),
        Some(other) => other.to_pr_string(),
        None => return Err(Error::Eval("js/set-text!: missing text".into())),
    };
    el.set_text_content(Some(&text));
    Ok(Value::Nil)
}

fn set_html_fn(args: &[Value]) -> Result<Value> {
    let el_js = arg_element(args, 0, "js/set-html!")?;
    let el: web_sys::Element = el_js
        .dyn_into()
        .map_err(|_| Error::Type("js/set-html!: not an Element".into()))?;
    let html = arg_str(args, 1, "js/set-html!")?;
    el.set_inner_html(html);
    Ok(Value::Nil)
}

fn on_fn(args: &[Value]) -> Result<Value> {
    let el_js = arg_element(args, 0, "js/on!")?;
    let event = arg_str(args, 1, "js/on!")?.to_string();
    let handler = arg_callable(args, 2, "js/on!")?.clone();

    let target: web_sys::EventTarget = el_js
        .dyn_into()
        .map_err(|_| Error::Type("js/on!: not an EventTarget".into()))?;

    // Wrap the cljrs callable in a Closure. The closure outlives this
    // call and is leaked (forget) so JS keeps a live function pointer
    // for the lifetime of the page. Acceptable for typical "wire up
    // once" interactions; if the user needs detach we'd return a
    // disposer handle — out of scope for the starter API.
    let cb = Closure::wrap(Box::new(move |_e: Event| {
        // Ignore eval errors; surface them via console so the demo
        // page's logs see them.
        if let Err(e) = eval::apply(&handler, &[]) {
            console::error_1(&JsValue::from_str(&format!("js/on! handler: {e}")));
        }
    }) as Box<dyn FnMut(Event)>);

    target
        .add_event_listener_with_callback(&event, cb.as_ref().unchecked_ref())
        .map_err(|_| Error::Eval("js/on!: addEventListener failed".into()))?;
    cb.forget();
    Ok(Value::Nil)
}

fn local_get_fn(args: &[Value]) -> Result<Value> {
    let k = arg_str(args, 0, "js/local-get")?;
    let win = window().ok_or_else(|| Error::Eval("js/local-get: no window".into()))?;
    let storage = win
        .local_storage()
        .map_err(|_| Error::Eval("js/local-get: no localStorage".into()))?
        .ok_or_else(|| Error::Eval("js/local-get: no localStorage".into()))?;
    match storage.get_item(k) {
        Ok(Some(v)) => Ok(Value::Str(Arc::from(v.as_str()))),
        _ => Ok(Value::Nil),
    }
}

fn local_set_fn(args: &[Value]) -> Result<Value> {
    let k = arg_str(args, 0, "js/local-set!")?;
    let v = arg_str(args, 1, "js/local-set!")?;
    let win = window().ok_or_else(|| Error::Eval("js/local-set!: no window".into()))?;
    let storage = win
        .local_storage()
        .map_err(|_| Error::Eval("js/local-set!: no localStorage".into()))?
        .ok_or_else(|| Error::Eval("js/local-set!: no localStorage".into()))?;
    storage
        .set_item(k, v)
        .map_err(|_| Error::Eval("js/local-set!: setItem failed".into()))?;
    Ok(Value::Nil)
}

fn fetch_text_fn(args: &[Value]) -> Result<Value> {
    let url = arg_str(args, 0, "js/fetch-text")?.to_string();
    let cb = arg_callable(args, 1, "js/fetch-text")?.clone();
    let win = window().ok_or_else(|| Error::Eval("js/fetch-text: no window".into()))?;

    spawn_local(async move {
        let promise = win.fetch_with_str(&url);
        let resp_js = match JsFuture::from(promise).await {
            Ok(v) => v,
            Err(_) => {
                let _ = eval::apply(&cb, &[Value::Nil]);
                return;
            }
        };
        let resp: web_sys::Response = match resp_js.dyn_into() {
            Ok(r) => r,
            Err(_) => {
                let _ = eval::apply(&cb, &[Value::Nil]);
                return;
            }
        };
        let text_promise = match resp.text() {
            Ok(p) => p,
            Err(_) => {
                let _ = eval::apply(&cb, &[Value::Nil]);
                return;
            }
        };
        let text_js = match JsFuture::from(text_promise).await {
            Ok(v) => v,
            Err(_) => {
                let _ = eval::apply(&cb, &[Value::Nil]);
                return;
            }
        };
        let text = text_js.as_string().unwrap_or_default();
        if let Err(e) = eval::apply(&cb, &[Value::Str(Arc::from(text.as_str()))]) {
            console::error_1(&JsValue::from_str(&format!("js/fetch-text cb: {e}")));
        }
    });

    Ok(Value::Nil)
}

// Silence unused-import warnings when web-sys feature surface shifts.
#[allow(dead_code)]
fn _tag_val() -> &'static str {
    TAG_VAL
}
