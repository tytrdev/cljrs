//! Backend-agnostic representation of a JIT-compiled cljrs function.
//!
//! [`NativeFn`] holds a raw code pointer plus its type signature. Any
//! codegen backend (currently MLIR; GPU / other backends later) can
//! produce one by providing an opaque holder that keeps the backing
//! resources alive while the pointer is in use.
//!
//! Phase-2 invocation scope: homogeneous i64 or f64 signatures, arity
//! 0–4. Bool is used internally (comparison results / if conditions)
//! but not as a fn parameter or return type — LLVM's i1 ABI at function
//! boundaries is platform-wonky; we'll revisit in a later phase.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::types::PrimType;
use crate::value::Value;

/// Signature + fn pointer for a JIT-compiled native function, shared
/// across the emitter (which emits calls + forward declarations) and the
/// compiler (which registers the pointer with the new ExecutionEngine).
#[derive(Clone)]
pub struct NativeSig {
    pub arg_types: Vec<PrimType>,
    pub ret_type: PrimType,
    pub ptr: usize,
}

/// Snapshot of every native fn currently bound in an Env, indexed by
/// cljrs-level name (not the sanitized MLIR name). Used by the emitter
/// to resolve cross-fn calls.
#[derive(Default, Clone)]
pub struct NativeRegistry {
    pub by_name: HashMap<String, NativeSig>,
}

impl NativeRegistry {
    pub fn get(&self, name: &str) -> Option<&NativeSig> {
        self.by_name.get(name)
    }
}

pub struct NativeFn {
    pub name: Arc<str>,
    pub arg_types: Vec<PrimType>,
    pub ret_type: PrimType,
    /// Function pointer as `usize` — Send+Sync without unsafe impls on
    /// this struct. Transmuted to `extern "C" fn(...)` at call time.
    pub ptr: usize,
    /// Opaque keep-alive for backing resources (e.g., ExecutionEngine +
    /// MLIR Context). Must outlive any invocation.
    _holder: Box<dyn Any + Send + Sync>,
}

impl NativeFn {
    pub fn new(
        name: Arc<str>,
        arg_types: Vec<PrimType>,
        ret_type: PrimType,
        ptr: usize,
        holder: Box<dyn Any + Send + Sync>,
    ) -> Self {
        Self {
            name,
            arg_types,
            ret_type,
            ptr,
            _holder: holder,
        }
    }

    /// Invoke the native function from the tree-walker.
    pub fn invoke(&self, args: &[Value]) -> Result<Value> {
        if args.len() != self.arg_types.len() {
            return Err(Error::Arity {
                expected: format!("{}", self.arg_types.len()),
                got: args.len(),
            });
        }

        let all_int_abi = self.arg_types.iter().all(|t| t.is_int_abi());
        let all_f64 = self.arg_types.iter().all(|t| matches!(t, PrimType::F64));

        match (all_int_abi, all_f64, self.ret_type) {
            (true, _, PrimType::I64) => invoke_i64(self.ptr, &self.name, args, &self.arg_types),
            (true, _, PrimType::F64) => invoke_iargs_fret(self.ptr, &self.name, args, &self.arg_types),
            (_, true, PrimType::F64) => invoke_f64(self.ptr, &self.name, args),
            (_, true, PrimType::I64) => invoke_fargs_iret(self.ptr, &self.name, args),
            _ => Err(Error::Eval(format!(
                "native fn `{}`: arg mix not supported yet (args={:?}, ret={:?})",
                self.name, self.arg_types, self.ret_type
            ))),
        }
    }
}

/// Extract an i64-ABI arg value (handles regular i64 and F64Buf).
fn extract_int_abi(name: &str, idx: usize, arg_ty: PrimType, v: &Value) -> Result<i64> {
    match (arg_ty, v) {
        (PrimType::I64, Value::Int(n)) => Ok(*n),
        (PrimType::F64Buf, Value::Int(n)) => Ok(*n),
        (PrimType::Bool, Value::Bool(b)) => Ok(if *b { 1 } else { 0 }),
        (PrimType::Bool, Value::Int(n)) => Ok(*n),
        (ty, other) => Err(Error::Type(format!(
            "native fn `{name}`: arg {idx}: expected {}, got {}",
            ty.as_str(),
            other.type_name()
        ))),
    }
}

fn extract_f64(name: &str, idx: usize, v: &Value) -> Result<f64> {
    match v {
        Value::Float(x) => Ok(*x),
        // Promote int args when the fn takes floats — matches Clojure's implicit coercion.
        Value::Int(n) => Ok(*n as f64),
        other => Err(Error::Type(format!(
            "native fn `{name}` arg {idx}: expected number, got {}",
            other.type_name()
        ))),
    }
}

fn invoke_i64(
    ptr: usize,
    name: &str,
    args: &[Value],
    arg_types: &[PrimType],
) -> Result<Value> {
    let mut xs = Vec::with_capacity(args.len());
    for (i, a) in args.iter().enumerate() {
        xs.push(extract_int_abi(name, i, arg_types[i], a)?);
    }
    // SAFETY: caller of compile_native_fn guarantees the pointer is a valid
    // JIT-compiled function matching this signature, and the holder keeps
    // it alive. i64 maps to C's int64_t under SysV / AAPCS, matching
    // MLIR/LLVM's default lowering.
    let r: i64 = unsafe {
        match xs.len() {
            0 => std::mem::transmute::<usize, extern "C" fn() -> i64>(ptr)(),
            1 => std::mem::transmute::<usize, extern "C" fn(i64) -> i64>(ptr)(xs[0]),
            2 => std::mem::transmute::<usize, extern "C" fn(i64, i64) -> i64>(ptr)(xs[0], xs[1]),
            3 => std::mem::transmute::<usize, extern "C" fn(i64, i64, i64) -> i64>(ptr)(
                xs[0], xs[1], xs[2],
            ),
            4 => std::mem::transmute::<usize, extern "C" fn(i64, i64, i64, i64) -> i64>(ptr)(
                xs[0], xs[1], xs[2], xs[3],
            ),
            5 => std::mem::transmute::<usize, extern "C" fn(i64, i64, i64, i64, i64) -> i64>(
                ptr,
            )(xs[0], xs[1], xs[2], xs[3], xs[4]),
            6 => std::mem::transmute::<usize, extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64>(
                ptr,
            )(xs[0], xs[1], xs[2], xs[3], xs[4], xs[5]),
            7 => std::mem::transmute::<
                usize,
                extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6]
            ),
            8 => std::mem::transmute::<
                usize,
                extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> i64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7]
            ),
            9 => std::mem::transmute::<
                usize,
                extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8]
            ),
            10 => std::mem::transmute::<
                usize,
                extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9]
            ),
            11 => std::mem::transmute::<
                usize,
                extern "C" fn(
                    i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64,
                ) -> i64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9],
                xs[10],
            ),
            12 => std::mem::transmute::<
                usize,
                extern "C" fn(
                    i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64,
                ) -> i64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9],
                xs[10], xs[11],
            ),
            13 => std::mem::transmute::<
                usize,
                extern "C" fn(
                    i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64,
                ) -> i64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9],
                xs[10], xs[11], xs[12],
            ),
            14 => std::mem::transmute::<
                usize,
                extern "C" fn(
                    i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64,
                    i64,
                ) -> i64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9],
                xs[10], xs[11], xs[12], xs[13],
            ),
            15 => std::mem::transmute::<
                usize,
                extern "C" fn(
                    i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64,
                    i64, i64,
                ) -> i64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9],
                xs[10], xs[11], xs[12], xs[13], xs[14],
            ),
            16 => std::mem::transmute::<
                usize,
                extern "C" fn(
                    i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64,
                    i64, i64, i64,
                ) -> i64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9],
                xs[10], xs[11], xs[12], xs[13], xs[14], xs[15],
            ),
            n => {
                return Err(Error::Eval(format!(
                    "native fn `{name}`: arity {n} > 16 not supported"
                )));
            }
        }
    };
    Ok(Value::Int(r))
}

/// i64-ABI args (regular i64 / f64-buf pointer / bool) returning f64.
/// Primary use case: buffer-reading kernels that return floats.
fn invoke_iargs_fret(
    ptr: usize,
    name: &str,
    args: &[Value],
    arg_types: &[PrimType],
) -> Result<Value> {
    let mut xs = Vec::with_capacity(args.len());
    for (i, a) in args.iter().enumerate() {
        xs.push(extract_int_abi(name, i, arg_types[i], a)?);
    }
    // SAFETY: same contract as invoke_i64.
    let r: f64 = unsafe {
        match xs.len() {
            1 => std::mem::transmute::<usize, extern "C" fn(i64) -> f64>(ptr)(xs[0]),
            2 => std::mem::transmute::<usize, extern "C" fn(i64, i64) -> f64>(ptr)(xs[0], xs[1]),
            3 => std::mem::transmute::<usize, extern "C" fn(i64, i64, i64) -> f64>(ptr)(
                xs[0], xs[1], xs[2],
            ),
            4 => std::mem::transmute::<usize, extern "C" fn(i64, i64, i64, i64) -> f64>(ptr)(
                xs[0], xs[1], xs[2], xs[3],
            ),
            5 => std::mem::transmute::<usize, extern "C" fn(i64, i64, i64, i64, i64) -> f64>(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4],
            ),
            6 => std::mem::transmute::<
                usize,
                extern "C" fn(i64, i64, i64, i64, i64, i64) -> f64,
            >(ptr)(xs[0], xs[1], xs[2], xs[3], xs[4], xs[5]),
            7 => std::mem::transmute::<
                usize,
                extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> f64,
            >(ptr)(xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6]),
            8 => std::mem::transmute::<
                usize,
                extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i64) -> f64,
            >(ptr)(xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7]),
            n => {
                return Err(Error::Eval(format!(
                    "native fn `{name}`: arity {n} not supported for (i64-ABI...)->f64"
                )));
            }
        }
    };
    Ok(Value::Float(r))
}

/// f64 args returning i64.
fn invoke_fargs_iret(ptr: usize, name: &str, args: &[Value]) -> Result<Value> {
    let mut xs = Vec::with_capacity(args.len());
    for (i, a) in args.iter().enumerate() {
        xs.push(extract_f64(name, i, a)?);
    }
    let r: i64 = unsafe {
        match xs.len() {
            1 => std::mem::transmute::<usize, extern "C" fn(f64) -> i64>(ptr)(xs[0]),
            2 => std::mem::transmute::<usize, extern "C" fn(f64, f64) -> i64>(ptr)(xs[0], xs[1]),
            3 => std::mem::transmute::<usize, extern "C" fn(f64, f64, f64) -> i64>(ptr)(
                xs[0], xs[1], xs[2],
            ),
            4 => std::mem::transmute::<usize, extern "C" fn(f64, f64, f64, f64) -> i64>(ptr)(
                xs[0], xs[1], xs[2], xs[3],
            ),
            n => {
                return Err(Error::Eval(format!(
                    "native fn `{name}`: arity {n} not supported for (f64...)->i64"
                )));
            }
        }
    };
    Ok(Value::Int(r))
}

fn invoke_f64(ptr: usize, name: &str, args: &[Value]) -> Result<Value> {
    let mut xs = Vec::with_capacity(args.len());
    for (i, a) in args.iter().enumerate() {
        xs.push(extract_f64(name, i, a)?);
    }
    // SAFETY: same contract as invoke_i64; f64 matches `double` at the C ABI.
    let r: f64 = unsafe {
        match xs.len() {
            0 => std::mem::transmute::<usize, extern "C" fn() -> f64>(ptr)(),
            1 => std::mem::transmute::<usize, extern "C" fn(f64) -> f64>(ptr)(xs[0]),
            2 => std::mem::transmute::<usize, extern "C" fn(f64, f64) -> f64>(ptr)(xs[0], xs[1]),
            3 => std::mem::transmute::<usize, extern "C" fn(f64, f64, f64) -> f64>(ptr)(
                xs[0], xs[1], xs[2],
            ),
            4 => std::mem::transmute::<usize, extern "C" fn(f64, f64, f64, f64) -> f64>(ptr)(
                xs[0], xs[1], xs[2], xs[3],
            ),
            5 => std::mem::transmute::<usize, extern "C" fn(f64, f64, f64, f64, f64) -> f64>(
                ptr,
            )(xs[0], xs[1], xs[2], xs[3], xs[4]),
            6 => std::mem::transmute::<usize, extern "C" fn(f64, f64, f64, f64, f64, f64) -> f64>(
                ptr,
            )(xs[0], xs[1], xs[2], xs[3], xs[4], xs[5]),
            7 => std::mem::transmute::<
                usize,
                extern "C" fn(f64, f64, f64, f64, f64, f64, f64) -> f64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6]
            ),
            8 => std::mem::transmute::<
                usize,
                extern "C" fn(f64, f64, f64, f64, f64, f64, f64, f64) -> f64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7]
            ),
            9 => std::mem::transmute::<
                usize,
                extern "C" fn(f64, f64, f64, f64, f64, f64, f64, f64, f64) -> f64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8]
            ),
            10 => std::mem::transmute::<
                usize,
                extern "C" fn(f64, f64, f64, f64, f64, f64, f64, f64, f64, f64) -> f64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9]
            ),
            11 => std::mem::transmute::<
                usize,
                extern "C" fn(
                    f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64,
                ) -> f64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9],
                xs[10],
            ),
            12 => std::mem::transmute::<
                usize,
                extern "C" fn(
                    f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64,
                ) -> f64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9],
                xs[10], xs[11],
            ),
            13 => std::mem::transmute::<
                usize,
                extern "C" fn(
                    f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64,
                ) -> f64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9],
                xs[10], xs[11], xs[12],
            ),
            14 => std::mem::transmute::<
                usize,
                extern "C" fn(
                    f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64,
                    f64,
                ) -> f64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9],
                xs[10], xs[11], xs[12], xs[13],
            ),
            15 => std::mem::transmute::<
                usize,
                extern "C" fn(
                    f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64,
                    f64, f64,
                ) -> f64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9],
                xs[10], xs[11], xs[12], xs[13], xs[14],
            ),
            16 => std::mem::transmute::<
                usize,
                extern "C" fn(
                    f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64,
                    f64, f64, f64,
                ) -> f64,
            >(ptr)(
                xs[0], xs[1], xs[2], xs[3], xs[4], xs[5], xs[6], xs[7], xs[8], xs[9],
                xs[10], xs[11], xs[12], xs[13], xs[14], xs[15],
            ),
            n => {
                return Err(Error::Eval(format!(
                    "native fn `{name}`: arity {n} > 16 not supported"
                )));
            }
        }
    };
    Ok(Value::Float(r))
}
