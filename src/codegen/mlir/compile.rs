//! End-to-end JIT orchestration: cljrs body → MLIR text → lowered module →
//! ExecutionEngine → [`NativeFn`] ready for tree-walker dispatch.
//!
//! The returned `NativeFn` owns a boxed holder containing the Context and
//! ExecutionEngine. As long as the `NativeFn` (inside an `Arc` in
//! `Value::Native`) is alive, the JIT'd code stays mapped.

use std::sync::Arc;

use melior::ExecutionEngine;
use melior::ir::Module;
use melior::ir::operation::OperationLike;

use crate::codegen::mlir::emit::{emit_module, sanitize_mlir_name};
use crate::codegen::mlir::{build_lowering_pipeline, create_context};
use crate::error::{Error, Result};
use crate::native::{NativeFn, NativeRegistry};
use crate::types::PrimType;
use crate::value::Value;

/// Opaque keep-alive for the MLIR JIT. Fields drop top-to-bottom, so
/// `_engine` releases JIT memory before `_context` tears down MLIR state,
/// preventing use-after-free on the code pointer.
///
/// Context and ExecutionEngine hold raw MLIR C pointers and don't
/// auto-derive Send/Sync. cljrs is single-threaded today, and the holder
/// is only ever read (for keep-alive); it's never dereferenced to call
/// MLIR APIs after construction. Marking Send+Sync is sound under that
/// usage and lets `Value::Native` stay Send+Sync.
struct MlirJitHolder {
    _engine: ExecutionEngine,
    _context: melior::Context,
}
unsafe impl Send for MlirJitHolder {}
unsafe impl Sync for MlirJitHolder {}

/// Compile a typed cljrs fn body to a `NativeFn` via MLIR + LLVM.
///
/// Pipeline: emit text → `Module::parse` → verify → scf/arith/cf/func → llvm
/// → `reconcile-unrealized-casts` → ExecutionEngine (O3) → symbol lookup.
pub fn compile_native_fn(
    name: &str,
    params: &[(Arc<str>, PrimType)],
    ret_type: PrimType,
    body: &Value,
    registry: &NativeRegistry,
) -> Result<NativeFn> {
    let emitted = emit_module(name, params, ret_type, body, registry)?;

    let context = create_context();
    let mut module = Module::parse(&context, &emitted.source).ok_or_else(|| {
        Error::Eval(format!(
            "MLIR parse failed (emitted source below)\n---\n{}---",
            emitted.source
        ))
    })?;
    if !module.as_operation().verify() {
        return Err(Error::Eval(format!(
            "MLIR verify failed (emitted source below)\n---\n{}---",
            emitted.source
        )));
    }

    build_lowering_pipeline(&context)
        .run(&mut module)
        .map_err(|_| Error::Eval("MLIR lowering pipeline failed".into()))?;

    let engine = ExecutionEngine::new(&module, 3, &[], false, false);

    // Register every cross-fn external native's pointer so MLIR's symbol
    // resolver can link the forward-declared `@other_fn` to real code.
    for ext_mlir_name in &emitted.referenced_externals {
        let (orig_name, sig) = registry
            .by_name
            .iter()
            .find(|(k, _)| sanitize_mlir_name(k) == *ext_mlir_name)
            .expect("emit stage validated this exists");
        // SAFETY: sig.ptr points at a JIT'd function owned by another
        // NativeFn that lives in Env globals; its lifetime exceeds this
        // engine's because Env owns the Arc<NativeFn>. If the previous
        // native is ever un-bound while this one is still callable,
        // we have a use-after-free — phase-2 deliberately doesn't let
        // anything un-bind.
        unsafe {
            engine.register_symbol(ext_mlir_name, sig.ptr as *mut ());
        }
        let _ = orig_name; // available if debug logging wants it
    }

    let mlir_name = sanitize_mlir_name(name);
    let ptr = engine.lookup(&mlir_name);
    if ptr.is_null() {
        return Err(Error::Eval(format!(
            "JIT symbol lookup for `{mlir_name}` (original `{name}`) returned null"
        )));
    }

    let holder = MlirJitHolder {
        _engine: engine,
        _context: context,
    };

    Ok(NativeFn::new(
        Arc::from(name),
        params.iter().map(|(_, t)| *t).collect(),
        ret_type,
        ptr as usize,
        Box::new(holder),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader;

    fn parse_body(src: &str) -> Value {
        let forms = reader::read_all(src).expect("read");
        forms.into_iter().next().expect("non-empty")
    }

    /// Phase-2 validation: emit + JIT + invoke a cljrs-authored fib via
    /// `NativeFn::invoke` (the same entry point eval::apply uses).
    #[test]
    fn compile_and_invoke_fib_via_native_fn() {
        let body = parse_body("(if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))");
        let params: &[(Arc<str>, PrimType)] = &[(Arc::from("n"), PrimType::I64)];

        let native =
            compile_native_fn("fib", params, PrimType::I64, &body, &NativeRegistry::default())
            .expect("compile fib");

        // Call through the tree-walker's invocation path.
        let r = native.invoke(&[Value::Int(10)]).expect("invoke fib(10)");
        assert_eq!(r, Value::Int(55));
        let r = native.invoke(&[Value::Int(20)]).expect("invoke fib(20)");
        assert_eq!(r, Value::Int(6765));
    }

    /// Direct fn-pointer invocation (fast path — what eval_list uses once
    /// compiled, no NativeFn overhead per call). Mirrors the earlier test.
    #[test]
    fn direct_fn_pointer_matches() {
        let body = parse_body("(if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))");
        let params: &[(Arc<str>, PrimType)] = &[(Arc::from("n"), PrimType::I64)];
        let native = compile_native_fn(
            "fib",
            params,
            PrimType::I64,
            &body,
            &NativeRegistry::default(),
        )
        .expect("compile");
        let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(native.ptr) };
        assert_eq!(f(25), 75025);
    }
}
