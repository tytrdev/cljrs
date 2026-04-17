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
