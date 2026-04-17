//! MLIR codegen backend, feature-gated behind `mlir`.
//!
//! Layout:
//!   - `mod.rs`      — MLIR Context factory, pass pipeline helpers.
//!   - `emit.rs`     — cljrs AST → MLIR textual source.
//!   - `compile.rs`  — MLIR text → JIT'd native function pointer.
//!
//! Phase 2 target: a minimal `defn-native` path for typed `i64` fns (fib,
//! factorial, sum loops). Phase 3 extends to f64/bool and cross-fn calls.

pub mod compile;
pub mod emit;

use melior::Context;
use melior::dialect::DialectRegistry;
use melior::pass;
use melior::utility::{register_all_dialects, register_all_llvm_translations};

/// Build a fully-loaded MLIR Context ready for JIT compilation.
///
/// The three required steps for the ExecutionEngine path to work:
///   1. `register_all_dialects` — makes `arith.*` / `scf.*` / `func.*` parse.
///   2. `register_all_llvm_translations` — lets the pipeline emit LLVM IR.
///   3. Attach a diagnostic handler so verify / parse errors surface.
pub fn create_context() -> Context {
    let registry = DialectRegistry::new();
    register_all_dialects(&registry);
    let context = Context::new();
    context.append_dialect_registry(&registry);
    context.load_all_available_dialects();
    register_all_llvm_translations(&context);
    context.attach_diagnostic_handler(|diag| {
        eprintln!("mlir diagnostic: {diag}");
        true
    });
    context
}

/// Build a PassManager with our standard lowering pipeline:
///   scf → cf → arith/cf/func → llvm, then reconcile-unrealized-casts.
/// `create_to_llvm()` alone doesn't cover scf, so we sequence explicitly.
pub fn build_lowering_pipeline(context: &Context) -> pass::PassManager<'_> {
    let pm = pass::PassManager::new(context);
    pm.add_pass(pass::conversion::create_scf_to_control_flow());
    pm.add_pass(pass::conversion::create_math_to_llvm());
    pm.add_pass(pass::conversion::create_arith_to_llvm());
    pm.add_pass(pass::conversion::create_control_flow_to_llvm());
    pm.add_pass(pass::conversion::create_func_to_llvm());
    pm.add_pass(pass::conversion::create_reconcile_unrealized_casts());
    pm
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Toolchain smoke test (Phase 1 holdover). Green = melior links + dylib
    /// loads + rpath from build.rs resolves.
    #[test]
    fn context_creation_links_and_loads() {
        let _ctx = create_context();
    }

    /// Proof of concept for the upcoming buffer ABI. Pass a Rust Vec's
    /// pointer as an i64 arg; inside the JIT'd fn, llvm.inttoptr converts
    /// it back to a pointer and we load/GEP into the buffer. This is the
    /// primitive all of the phase-3 buffer work (N-body, BVH, GPU kernels)
    /// is built on top of.
    #[test]
    fn jit_buffer_read_via_pointer_arg() {
        use melior::ExecutionEngine;
        use melior::ir::Module;
        use melior::ir::operation::OperationLike;

        let context = create_context();
        // sum_buf iterates 0..len, reads buf[i] as f64, accumulates.
        // No MLIR loop — we recurse tail-wise into sum_loop and let
        // LLVM -O3 TCO it into a native loop.
        let source = r#"
            module {
              func.func private @sum_loop(
                %buf: i64, %i: i64, %len: i64, %acc: f64
              ) -> f64 {
                %cond = arith.cmpi sge, %i, %len : i64
                %r = scf.if %cond -> (f64) {
                  scf.yield %acc : f64
                } else {
                  %ptr = llvm.inttoptr %buf : i64 to !llvm.ptr
                  %gep = llvm.getelementptr %ptr[%i]
                    : (!llvm.ptr, i64) -> !llvm.ptr, f64
                  %v = llvm.load %gep : !llvm.ptr -> f64
                  %c1 = arith.constant 1 : i64
                  %ni = arith.addi %i, %c1 : i64
                  %na = arith.addf %acc, %v : f64
                  %rec = func.call @sum_loop(%buf, %ni, %len, %na)
                    : (i64, i64, i64, f64) -> f64
                  scf.yield %rec : f64
                }
                return %r : f64
              }

              func.func @sum_buf(%buf: i64, %len: i64) -> f64
                  attributes { llvm.emit_c_interface } {
                %z = arith.constant 0.0 : f64
                %c0 = arith.constant 0 : i64
                %r = func.call @sum_loop(%buf, %c0, %len, %z)
                  : (i64, i64, i64, f64) -> f64
                return %r : f64
              }
            }
        "#;

        let mut module = Module::parse(&context, source).expect("parse MLIR");
        assert!(module.as_operation().verify(), "module failed verify");
        build_lowering_pipeline(&context)
            .run(&mut module)
            .expect("lowering failed");

        let engine = ExecutionEngine::new(&module, 3, &[], false, false);
        let data: Vec<f64> = (0..1000).map(|i| i as f64).collect();
        let expected: f64 = data.iter().sum();

        // Pass Vec's pointer as a raw i64. extern "C" fn(i64, i64) -> f64.
        let ptr_as_i64 = data.as_ptr() as usize as i64;
        let f: extern "C" fn(i64, i64) -> f64 =
            unsafe { std::mem::transmute(engine.lookup("sum_buf")) };
        let result = f(ptr_as_i64, data.len() as i64);
        assert!(
            (result - expected).abs() < 1e-9,
            "sum mismatch: {result} vs {expected}"
        );
    }

    /// Hardcoded-MLIR fib JIT (Phase 2 baseline). Green = whole pipeline —
    /// parse, lower, JIT, invoke — actually works on this machine.
    #[test]
    fn jit_hardcoded_fib_returns_correct_result() {
        use melior::ExecutionEngine;
        use melior::ir::Module;
        use melior::ir::operation::OperationLike;

        let context = create_context();
        let source = r#"
            module {
              func.func @fib(%n: i64) -> i64 attributes { llvm.emit_c_interface } {
                %c2 = arith.constant 2 : i64
                %is_base = arith.cmpi slt, %n, %c2 : i64
                %result = scf.if %is_base -> (i64) {
                  scf.yield %n : i64
                } else {
                  %c1 = arith.constant 1 : i64
                  %n_m1 = arith.subi %n, %c1 : i64
                  %fib_m1 = func.call @fib(%n_m1) : (i64) -> i64
                  %n_m2 = arith.subi %n, %c2 : i64
                  %fib_m2 = func.call @fib(%n_m2) : (i64) -> i64
                  %sum = arith.addi %fib_m1, %fib_m2 : i64
                  scf.yield %sum : i64
                }
                return %result : i64
              }
            }
        "#;

        let mut module = Module::parse(&context, source).expect("parse MLIR");
        assert!(module.as_operation().verify(), "module failed verify");

        build_lowering_pipeline(&context)
            .run(&mut module)
            .expect("lowering failed");

        let engine = ExecutionEngine::new(&module, 3, &[], false, false);

        let mut arg: i64 = 10;
        let mut result: i64 = -1;
        unsafe {
            engine
                .invoke_packed(
                    "fib",
                    &mut [
                        &mut arg as *mut i64 as *mut (),
                        &mut result as *mut i64 as *mut (),
                    ],
                )
                .expect("invoke_packed failed");
        }
        assert_eq!(result, 55);
    }
}
