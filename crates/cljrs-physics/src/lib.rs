//! Rapier physics bindings for cljrs.
//!
//! Exposes two sibling namespaces — `cljrs.physics.d2` and
//! `cljrs.physics.d3` — with identical fn names, differing only in
//! the arity of position vectors ([x y] vs [x y z]).
//!
//! The `World` value is opaque and mutable. Conventions:
//!   - map-based constructors (`(world {:gravity [...]})`)
//!   - `!` suffix on mutators (`step!`, `apply-impulse!`)
//!   - body/collider handles are plain ints (generational indices
//!     under the hood; we just marshal the u32 index for ergonomics)

use cljrs::env::Env;
use cljrs::error::{Error, Result};
use cljrs::value::{Builtin, Value};
use std::sync::{Arc, Mutex};

mod d2;
mod d3;

pub fn install(env: &Env) {
    let prev = env.current_ns();
    env.set_current_ns("cljrs.physics.d2");
    d2::install(env);
    env.set_current_ns("cljrs.physics.d3");
    d3::install(env);
    env.set_current_ns(prev.as_ref());
}

// --- helpers shared by both dimensions --------------------------------

fn bind(env: &Env, name: &'static str, f: fn(&[Value]) -> Result<Value>) {
    env.define_global(name, Value::Builtin(Builtin::new_static(name, f)));
}

fn arg_map<'a>(
    args: &'a [Value],
    idx: usize,
    name: &str,
) -> Result<&'a imbl::HashMap<Value, Value>> {
    match args.get(idx) {
        Some(Value::Map(m)) => Ok(m),
        Some(v) => Err(Error::Type(format!(
            "{name}: arg {idx} must be map, got {}",
            v.type_name()
        ))),
        None => Err(Error::Eval(format!("{name}: missing arg {idx}"))),
    }
}

fn arg_opaque<'a, T: 'static + Send + Sync>(
    args: &'a [Value],
    idx: usize,
    tag: &str,
    name: &str,
) -> Result<Arc<T>> {
    match args.get(idx) {
        Some(Value::Opaque { tag: t, inner }) if t.as_ref() == tag => {
            match Arc::clone(inner).downcast::<T>() {
                Ok(a) => Ok(a),
                Err(_) => Err(Error::Type(format!(
                    "{name}: opaque value tag matched '{tag}' but downcast failed"
                ))),
            }
        }
        Some(v) => Err(Error::Type(format!(
            "{name}: arg {idx} must be {tag}, got {}",
            v.type_name()
        ))),
        None => Err(Error::Eval(format!("{name}: missing arg {idx}"))),
    }
}

fn arg_u32(args: &[Value], idx: usize, name: &str) -> Result<u32> {
    match args.get(idx) {
        Some(Value::Int(i)) if *i >= 0 && *i <= u32::MAX as i64 => Ok(*i as u32),
        Some(v) => Err(Error::Type(format!(
            "{name}: arg {idx} must be non-negative int, got {}",
            v.type_name()
        ))),
        None => Err(Error::Eval(format!("{name}: missing arg {idx}"))),
    }
}

fn map_get<'a>(
    m: &'a imbl::HashMap<Value, Value>,
    k: &str,
) -> Option<&'a Value> {
    m.get(&Value::Keyword(Arc::from(k)))
}

fn as_f32(v: &Value) -> Result<f32> {
    match v {
        Value::Float(f) => Ok(*f as f32),
        Value::Int(i) => Ok(*i as f32),
        _ => Err(Error::Type(format!(
            "expected number, got {}",
            v.type_name()
        ))),
    }
}

fn as_kw<'a>(v: &'a Value) -> Result<&'a str> {
    match v {
        Value::Keyword(k) => Ok(k.as_ref()),
        _ => Err(Error::Type(format!(
            "expected keyword, got {}",
            v.type_name()
        ))),
    }
}

fn vec_components(v: &Value) -> Result<Vec<f32>> {
    match v {
        Value::Vector(xs) => xs.iter().map(as_f32).collect(),
        Value::List(xs) => xs.iter().map(as_f32).collect(),
        _ => Err(Error::Type(format!(
            "expected [x ...] vector, got {}",
            v.type_name()
        ))),
    }
}

fn f32_vec(components: &[f32]) -> Value {
    Value::Vector(components.iter().map(|x| Value::Float(*x as f64)).collect())
}

// Opaque constructor: wraps any Send+Sync value behind a Mutex so the
// physics pipeline can mutate through an &Value (since eval returns
// cloned Values).
fn opaque<T: Send + Sync + 'static>(tag: &'static str, inner: T) -> Value {
    Value::Opaque {
        tag: Arc::from(tag),
        inner: Arc::new(Mutex::new(inner)) as Arc<dyn std::any::Any + Send + Sync>,
    }
}

type OpaqueMutex<T> = Arc<Mutex<T>>;

fn arg_world<T: 'static + Send + Sync>(
    args: &[Value],
    idx: usize,
    tag: &'static str,
    name: &str,
) -> Result<OpaqueMutex<T>> {
    arg_opaque::<Mutex<T>>(args, idx, tag, name)
}
