//! cljrs.ml — CPU autograd exposed as cljrs builtins.
//!
//! Lives in the `cljrs.ml` namespace. Tensors are opaque handles
//! (`Value::Opaque` with tag `ml/tensor`) — the cljrs side never sees
//! the f32 buffer directly except through `(ml/tolist t)` /
//! `(ml/scalar t)`. Mirror of `cljrs-physics` install pattern.

pub mod autograd;

use autograd::{Shape, Tensor};
use cljrs::env::Env;
use cljrs::error::{Error, Result};
use cljrs::value::{Builtin, Value};
use std::sync::Arc;

const TAG: &str = "ml/tensor";

pub fn install(env: &Env) {
    let prev = env.current_ns();
    env.set_current_ns("cljrs.ml");
    bind(env, "tensor", tensor_fn);
    bind(env, "param", param_fn);
    bind(env, "randn", randn_fn);
    bind(env, "zeros", zeros_fn);
    bind(env, "matmul", matmul_fn);
    bind(env, "add", add_fn);
    bind(env, "add-bias", add_bias_fn);
    bind(env, "sub", sub_fn);
    bind(env, "relu", relu_fn);
    bind(env, "tanh", tanh_fn);
    bind(env, "mse", mse_fn);
    bind(env, "backward!", backward_fn);
    bind(env, "sgd-step!", sgd_step_fn);
    bind(env, "scalar", scalar_fn);
    bind(env, "tolist", tolist_fn);
    bind(env, "shape", shape_fn);
    bind(env, "set-data!", set_data_fn);
    env.set_current_ns(prev.as_ref());
}

// --- helpers -------------------------------------------------------------

fn bind(env: &Env, name: &'static str, f: fn(&[Value]) -> Result<Value>) {
    env.define_global(name, Value::Builtin(Builtin::new_static(name, f)));
}

fn opaque(t: Tensor) -> Value {
    Value::Opaque {
        tag: Arc::from(TAG),
        inner: Arc::new(t) as Arc<dyn std::any::Any + Send + Sync>,
    }
}

fn arg_tensor(args: &[Value], idx: usize, name: &str) -> Result<Tensor> {
    match args.get(idx) {
        Some(Value::Opaque { tag, inner }) if tag.as_ref() == TAG => {
            match Arc::clone(inner).downcast::<Tensor>() {
                Ok(a) => Ok((*a).clone()),
                Err(_) => Err(Error::Type(format!("{name}: opaque tensor downcast failed"))),
            }
        }
        Some(v) => Err(Error::Type(format!(
            "{name}: arg {idx} must be tensor, got {}",
            v.type_name()
        ))),
        None => Err(Error::Eval(format!("{name}: missing arg {idx}"))),
    }
}

fn arg_f32(args: &[Value], idx: usize, name: &str) -> Result<f32> {
    match args.get(idx) {
        Some(Value::Float(f)) => Ok(*f as f32),
        Some(Value::Int(i)) => Ok(*i as f32),
        Some(v) => Err(Error::Type(format!(
            "{name}: arg {idx} must be number, got {}",
            v.type_name()
        ))),
        None => Err(Error::Eval(format!("{name}: missing arg {idx}"))),
    }
}

fn arg_usize(args: &[Value], idx: usize, name: &str) -> Result<usize> {
    match args.get(idx) {
        Some(Value::Int(i)) if *i >= 0 => Ok(*i as usize),
        Some(v) => Err(Error::Type(format!(
            "{name}: arg {idx} must be non-negative int, got {}",
            v.type_name()
        ))),
        None => Err(Error::Eval(format!("{name}: missing arg {idx}"))),
    }
}

fn flatten_numbers(v: &Value, out: &mut Vec<f32>) -> Result<()> {
    match v {
        Value::Float(f) => { out.push(*f as f32); Ok(()) }
        Value::Int(i)   => { out.push(*i as f32); Ok(()) }
        Value::Vector(xs) => {
            for x in xs.iter() { flatten_numbers(x, out)?; }
            Ok(())
        }
        Value::List(xs) => {
            for x in xs.iter() { flatten_numbers(x, out)?; }
            Ok(())
        }
        _ => Err(Error::Type(format!("expected number / vector of numbers, got {}", v.type_name()))),
    }
}

fn infer_shape(v: &Value) -> Option<(usize, usize)> {
    // Returns (rows, cols). 1-D vectors are (1, n). 2-D vector-of-vectors is (m, n).
    if let Value::Vector(rows) = v {
        if rows.is_empty() { return Some((1, 0)); }
        if let Value::Vector(first) = &rows[0] {
            let cols = first.len();
            for r in rows.iter() {
                if let Value::Vector(rr) = r {
                    if rr.len() != cols { return None; }
                } else { return None; }
            }
            return Some((rows.len(), cols));
        }
        return Some((1, rows.len()));
    }
    None
}

// --- builtins ------------------------------------------------------------

/// (ml/tensor data) — data is a 1-D or 2-D vector of numbers.
/// (ml/tensor rows cols data) — explicit shape.
fn tensor_fn(args: &[Value]) -> Result<Value> {
    make_tensor(args, "tensor", false)
}

/// (ml/param data) — same as tensor but flagged as a learnable
/// parameter. SGD only updates params.
fn param_fn(args: &[Value]) -> Result<Value> {
    make_tensor(args, "param", true)
}

fn make_tensor(args: &[Value], name: &str, is_param: bool) -> Result<Value> {
    let (shape, data) = match args.len() {
        1 => {
            let v = &args[0];
            let sh = infer_shape(v).ok_or_else(|| {
                Error::Type(format!("{name}: cannot infer shape from {}", v.type_name()))
            })?;
            let mut data = Vec::with_capacity(sh.0 * sh.1);
            flatten_numbers(v, &mut data)?;
            if data.len() != sh.0 * sh.1 {
                return Err(Error::Type(format!(
                    "{name}: shape ({}x{}) needs {} numbers, got {}",
                    sh.0, sh.1, sh.0 * sh.1, data.len()
                )));
            }
            (Shape::new(sh.0, sh.1), data)
        }
        3 => {
            let r = arg_usize(args, 0, name)?;
            let c = arg_usize(args, 1, name)?;
            let mut data = Vec::with_capacity(r * c);
            flatten_numbers(&args[2], &mut data)?;
            if data.len() != r * c {
                return Err(Error::Type(format!(
                    "{name}: shape ({r}x{c}) needs {} numbers, got {}",
                    r * c, data.len()
                )));
            }
            (Shape::new(r, c), data)
        }
        _ => return Err(Error::Eval(format!("{name}: arity 1 or 3"))),
    };
    Ok(opaque(Tensor::leaf(shape, data, is_param)))
}

/// (ml/randn rows cols stddev?) — Gaussian-ish via Box-Muller from a
/// deterministic xorshift RNG seeded by call-count (so re-running the
/// same script yields the same init). Param-flagged.
fn randn_fn(args: &[Value]) -> Result<Value> {
    let r = arg_usize(args, 0, "randn")?;
    let c = arg_usize(args, 1, "randn")?;
    let std = if args.len() >= 3 { arg_f32(args, 2, "randn")? } else { 1.0 };
    // Local 128-bit xorshift seeded from a tiny global counter so each
    // call is distinct but deterministic across a session.
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEED: AtomicU64 = AtomicU64::new(0xC0FFEE_u64);
    let mut s = SEED.fetch_add(0x9E3779B97F4A7C15, Ordering::Relaxed) | 1;
    let mut next = || {
        s ^= s << 13; s ^= s >> 7; s ^= s << 17;
        // map u64 -> (0,1)
        (s >> 11) as f32 / (1u64 << 53) as f32
    };
    let n = r * c;
    let mut data = Vec::with_capacity(n);
    while data.len() < n {
        let u1 = next().max(1e-7);
        let u2 = next();
        let mag = (-2.0_f32 * u1.ln()).sqrt() * std;
        let z0 = mag * (2.0 * std::f32::consts::PI * u2).cos();
        let z1 = mag * (2.0 * std::f32::consts::PI * u2).sin();
        data.push(z0);
        if data.len() < n { data.push(z1); }
    }
    Ok(opaque(Tensor::leaf(Shape::new(r, c), data, true)))
}

fn zeros_fn(args: &[Value]) -> Result<Value> {
    let r = arg_usize(args, 0, "zeros")?;
    let c = arg_usize(args, 1, "zeros")?;
    Ok(opaque(Tensor::leaf(Shape::new(r, c), vec![0.0; r * c], true)))
}

fn matmul_fn(args: &[Value]) -> Result<Value> {
    let a = arg_tensor(args, 0, "matmul")?;
    let b = arg_tensor(args, 1, "matmul")?;
    if a.shape().cols != b.shape().rows {
        return Err(Error::Eval(format!(
            "matmul: ({}x{}) · ({}x{}) — inner dims must match",
            a.shape().rows, a.shape().cols, b.shape().rows, b.shape().cols
        )));
    }
    Ok(opaque(autograd::matmul(&a, &b)))
}

fn add_fn(args: &[Value]) -> Result<Value> {
    let a = arg_tensor(args, 0, "add")?;
    let b = arg_tensor(args, 1, "add")?;
    if a.shape() != b.shape() {
        return Err(Error::Eval("add: shape mismatch".into()));
    }
    Ok(opaque(autograd::add(&a, &b)))
}

fn add_bias_fn(args: &[Value]) -> Result<Value> {
    let a = arg_tensor(args, 0, "add-bias")?;
    let b = arg_tensor(args, 1, "add-bias")?;
    if b.shape().rows != 1 || b.shape().cols != a.shape().cols {
        return Err(Error::Eval(format!(
            "add-bias: bias must be (1x{}), got ({}x{})",
            a.shape().cols, b.shape().rows, b.shape().cols
        )));
    }
    Ok(opaque(autograd::add_bias(&a, &b)))
}

fn sub_fn(args: &[Value]) -> Result<Value> {
    let a = arg_tensor(args, 0, "sub")?;
    let b = arg_tensor(args, 1, "sub")?;
    if a.shape() != b.shape() {
        return Err(Error::Eval("sub: shape mismatch".into()));
    }
    Ok(opaque(autograd::sub(&a, &b)))
}

fn relu_fn(args: &[Value]) -> Result<Value> {
    Ok(opaque(autograd::relu(&arg_tensor(args, 0, "relu")?)))
}

fn tanh_fn(args: &[Value]) -> Result<Value> {
    Ok(opaque(autograd::tanh(&arg_tensor(args, 0, "tanh")?)))
}

fn mse_fn(args: &[Value]) -> Result<Value> {
    let p = arg_tensor(args, 0, "mse")?;
    let t = arg_tensor(args, 1, "mse")?;
    if p.shape() != t.shape() {
        return Err(Error::Eval("mse: shape mismatch".into()));
    }
    Ok(opaque(autograd::mse(&p, &t)))
}

fn backward_fn(args: &[Value]) -> Result<Value> {
    let loss = arg_tensor(args, 0, "backward!")?;
    autograd::backward(&loss);
    Ok(Value::Nil)
}

fn sgd_step_fn(args: &[Value]) -> Result<Value> {
    let params_v = match args.get(0) {
        Some(Value::Vector(xs)) => xs,
        Some(v) => return Err(Error::Type(format!(
            "sgd-step!: arg 0 must be vector of tensors, got {}", v.type_name()))),
        None => return Err(Error::Eval("sgd-step!: missing params".into())),
    };
    let lr = arg_f32(args, 1, "sgd-step!")?;
    let mut params = Vec::with_capacity(params_v.len());
    for v in params_v.iter() {
        match v {
            Value::Opaque { tag, inner } if tag.as_ref() == TAG => {
                let t = Arc::clone(inner).downcast::<Tensor>()
                    .map_err(|_| Error::Type("sgd-step!: bad tensor in params".into()))?;
                params.push((*t).clone());
            }
            other => return Err(Error::Type(format!(
                "sgd-step!: param must be tensor, got {}", other.type_name()))),
        }
    }
    autograd::sgd_step(&params, lr);
    Ok(Value::Nil)
}

fn scalar_fn(args: &[Value]) -> Result<Value> {
    let t = arg_tensor(args, 0, "scalar")?;
    let d = t.data();
    if d.is_empty() { return Ok(Value::Float(0.0)); }
    Ok(Value::Float(d[0] as f64))
}

fn tolist_fn(args: &[Value]) -> Result<Value> {
    let t = arg_tensor(args, 0, "tolist")?;
    let d = t.data();
    let v: imbl::Vector<Value> = d.iter().map(|x| Value::Float(*x as f64)).collect();
    Ok(Value::Vector(v))
}

fn shape_fn(args: &[Value]) -> Result<Value> {
    let t = arg_tensor(args, 0, "shape")?;
    let s = t.shape();
    Ok(Value::Vector(imbl::vector![
        Value::Int(s.rows as i64),
        Value::Int(s.cols as i64)
    ]))
}

/// (ml/set-data! tensor [..numbers..]) — overwrite a tensor's buffer in
/// place. Used for streaming new mini-batch inputs without rebuilding
/// the graph wrapper. Caller must match the existing shape.
fn set_data_fn(args: &[Value]) -> Result<Value> {
    let t = arg_tensor(args, 0, "set-data!")?;
    let mut buf = Vec::with_capacity(t.shape().numel());
    flatten_numbers(args.get(1).ok_or_else(||
        Error::Eval("set-data!: missing data".into()))?, &mut buf)?;
    if buf.len() != t.shape().numel() {
        return Err(Error::Eval(format!(
            "set-data!: shape ({}x{}) needs {} numbers, got {}",
            t.shape().rows, t.shape().cols, t.shape().numel(), buf.len())));
    }
    let mut d = t.0.data.borrow_mut();
    d.copy_from_slice(&buf);
    Ok(Value::Nil)
}
