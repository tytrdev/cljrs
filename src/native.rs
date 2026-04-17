//! Backend-agnostic representation of a JIT-compiled cljrs function.
//!
//! [`NativeFn`] holds a raw code pointer plus its type signature. Any
//! codegen backend (currently just MLIR; GPU / other backends later) can
//! produce one by providing an opaque holder that keeps the backing
//! resources alive while the pointer is in use.
//!
//! Phase 2 invocation is restricted to i64-only signatures of arity 0–4.
//! Phase 3 extends to f64 / bool and higher arities.

use std::any::Any;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::types::PrimType;
use crate::value::Value;

pub struct NativeFn {
    pub name: Arc<str>,
    pub arg_types: Vec<PrimType>,
    pub ret_type: PrimType,
    /// Function pointer as `usize` — kept Send+Sync without unsafe impls
    /// on this struct. Transmuted to the right `extern "C" fn(...)` at
    /// call time based on `arg_types`.
    pub ptr: usize,
    /// Opaque keep-alive for backing resources (e.g., MLIR ExecutionEngine
    /// and Context). Must outlive any invocation. `Any + Send + Sync` lets
    /// any backend slot in its own holder.
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
    ///
    /// Phase-2 rules: every arg must be `Value::Int`, every arg_type must be
    /// `PrimType::I64`, return type must be `PrimType::I64`. Arity ≤ 4.
    /// These are the same restrictions the emitter enforces, so anything
    /// that compiled can be called here.
    pub fn invoke(&self, args: &[Value]) -> Result<Value> {
        if args.len() != self.arg_types.len() {
            return Err(Error::Arity {
                expected: format!("{}", self.arg_types.len()),
                got: args.len(),
            });
        }
        if !matches!(self.ret_type, PrimType::I64) {
            return Err(Error::Eval(format!(
                "native fn: ret type {} not yet supported at call site",
                self.ret_type.as_str()
            )));
        }

        let mut ints: Vec<i64> = Vec::with_capacity(args.len());
        for (i, a) in args.iter().enumerate() {
            if !matches!(self.arg_types[i], PrimType::I64) {
                return Err(Error::Eval(format!(
                    "native fn: arg {i} type {} not yet supported",
                    self.arg_types[i].as_str()
                )));
            }
            match a {
                Value::Int(n) => ints.push(*n),
                other => {
                    return Err(Error::Type(format!(
                        "native fn `{}`: arg {i} expected int, got {}",
                        self.name,
                        other.type_name()
                    )));
                }
            }
        }

        // SAFETY: caller of compile_native_fn guarantees the pointer is a
        // valid JIT-compiled function matching the signature we built, and
        // the holder keeps it alive. i64 maps to C's `int64_t` with the
        // standard SysV/AAPCS calling convention on both macOS architectures.
        let result: i64 = unsafe {
            match ints.len() {
                0 => {
                    let f: extern "C" fn() -> i64 = std::mem::transmute(self.ptr);
                    f()
                }
                1 => {
                    let f: extern "C" fn(i64) -> i64 = std::mem::transmute(self.ptr);
                    f(ints[0])
                }
                2 => {
                    let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(self.ptr);
                    f(ints[0], ints[1])
                }
                3 => {
                    let f: extern "C" fn(i64, i64, i64) -> i64 =
                        std::mem::transmute(self.ptr);
                    f(ints[0], ints[1], ints[2])
                }
                4 => {
                    let f: extern "C" fn(i64, i64, i64, i64) -> i64 =
                        std::mem::transmute(self.ptr);
                    f(ints[0], ints[1], ints[2], ints[3])
                }
                n => {
                    return Err(Error::Eval(format!(
                        "native fn: arity {n} not yet supported (max 4 in phase 2)"
                    )));
                }
            }
        };
        Ok(Value::Int(result))
    }
}
