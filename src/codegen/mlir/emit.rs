//! cljrs AST → MLIR textual source.
//!
//! Supports i64, f64, and bool (i1) on the native path. One cljrs
//! `defn-native` produces one MLIR module; within that module the main
//! function may spawn any number of helper functions for `loop`/`recur`.
//! Each helper fn takes (outer captures..., loop vars...) and every
//! `recur` inside a loop compiles to a self-`func.call` on the helper —
//! LLVM -O3's tail-call optimization collapses the chain into a native
//! machine loop.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::native::NativeRegistry;
use crate::types::PrimType;
use crate::value::Value;

/// Materialized SSA value with tracked MLIR type.
#[derive(Clone)]
struct EmVal {
    ssa: String,
    ty: MlirTy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Ptr reserved for when we bind pointer values in scope (buf-set, raw ptrs).
enum MlirTy {
    I64,
    F64,
    I1,
    /// Opaque pointer for buffer args. Handled by buf-get / buf-set
    /// intrinsics; not legal in arith ops directly.
    Ptr,
}

impl MlirTy {
    fn as_str(self) -> &'static str {
        match self {
            MlirTy::I64 => "i64",
            MlirTy::F64 => "f64",
            MlirTy::I1 => "i1",
            MlirTy::Ptr => "!llvm.ptr",
        }
    }

    fn is_int(self) -> bool {
        matches!(self, MlirTy::I64)
    }
    fn is_float(self) -> bool {
        matches!(self, MlirTy::F64)
    }
    fn is_bool(self) -> bool {
        matches!(self, MlirTy::I1)
    }
}

fn prim_to_mlir(p: PrimType) -> MlirTy {
    match p {
        PrimType::I64 => MlirTy::I64,
        PrimType::F64 => MlirTy::F64,
        PrimType::Bool => MlirTy::I1,
        // F64Buf crosses the FFI as i64 (pointer bits) and is
        // `inttoptr`-converted to !llvm.ptr at the top of the fn body.
        // This function returns the *FFI* type; the internal Ptr
        // binding is set up separately in emit_module.
        PrimType::F64Buf => MlirTy::I64,
    }
}

type Scope = HashMap<String, EmVal>;

/// Shared module-level state across the main fn and any loop helpers.
struct ModuleState<'r> {
    /// Fully-formed helper function bodies to append after the main fn.
    helpers: Vec<String>,
    helper_counter: usize,
    /// Previously-compiled natives available for cross-fn calls.
    registry: &'r NativeRegistry,
    /// Set of MLIR-sanitized names of externals actually referenced in
    /// this module. We emit a `func.func private @NAME` forward
    /// declaration for each, and compile.rs registers the pointer.
    referenced_externals: HashSet<String>,
}

impl<'r> ModuleState<'r> {
    fn fresh_helper_name(&mut self, base: &str) -> String {
        let n = self.helper_counter;
        self.helper_counter += 1;
        format!("{base}_loop_{n}")
    }
}

/// Per-function emission context. Each MLIR function gets its own counter,
/// its own body buffer, and its own self-call shape (main fn self-calls
/// by its user-given name; helpers self-call via `recur`).
struct FnEmitter<'m, 'r> {
    body: String,
    ssa_counter: usize,
    /// Sym in source that routes to self-call (main fn: its own name; helper: "recur").
    self_trigger: String,
    /// MLIR symbol name of this fn, used in the emitted `func.call @NAME`.
    self_mlir_name: String,
    /// Params of this fn, in order — both name and type.
    params: Vec<(Arc<str>, MlirTy)>,
    /// Return type.
    ret: MlirTy,
    /// When this fn is a loop helper, these SSA values (from the caller's
    /// scope) are threaded as the first N params — they're captures.
    /// For the main fn, empty.
    capture_names: Vec<Arc<str>>,
    module: &'m mut ModuleState<'r>,
}

impl<'m, 'r> FnEmitter<'m, 'r> {
    fn fresh(&mut self) -> String {
        let n = self.ssa_counter;
        self.ssa_counter += 1;
        format!("%v{n}")
    }

    fn line(&mut self, s: impl AsRef<str>) {
        self.body.push_str("    ");
        self.body.push_str(s.as_ref());
        self.body.push('\n');
    }

    fn init_scope(&self) -> Scope {
        let mut s: Scope = HashMap::new();
        for (i, (name, ty)) in self.params.iter().enumerate() {
            s.insert(
                name.to_string(),
                EmVal {
                    ssa: format!("%arg{i}"),
                    ty: *ty,
                },
            );
        }
        s
    }

    fn emit(&mut self, form: &Value, scope: &Scope) -> Result<EmVal> {
        match form {
            Value::Int(n) => {
                let v = self.fresh();
                self.line(format!("{v} = arith.constant {n} : i64"));
                Ok(EmVal {
                    ssa: v,
                    ty: MlirTy::I64,
                })
            }
            Value::Float(f) => {
                let v = self.fresh();
                // MLIR float literals accept exponent form; ensure we
                // always emit a decimal so parse doesn't confuse with int.
                let lit = if f.fract() == 0.0 && f.is_finite() {
                    format!("{f:.1}")
                } else {
                    format!("{f}")
                };
                self.line(format!("{v} = arith.constant {lit} : f64"));
                Ok(EmVal {
                    ssa: v,
                    ty: MlirTy::F64,
                })
            }
            Value::Bool(b) => {
                let v = self.fresh();
                let n = if *b { 1 } else { 0 };
                self.line(format!("{v} = arith.constant {n} : i1"));
                Ok(EmVal {
                    ssa: v,
                    ty: MlirTy::I1,
                })
            }
            Value::Symbol(s) => scope
                .get(s.as_ref())
                .cloned()
                .ok_or_else(|| Error::Unbound(s.to_string())),
            Value::List(xs) => self.emit_call(xs, scope),
            _ => Err(Error::Eval(format!(
                "native fn body: can't codegen {}",
                form.type_name()
            ))),
        }
    }

    fn emit_call(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        if xs.is_empty() {
            return Err(Error::Eval("empty list in native fn body".into()));
        }
        let Value::Symbol(head) = &xs[0] else {
            return Err(Error::Eval("native call: head must be a symbol".into()));
        };
        match head.as_ref() {
            "+" => self.emit_num_binop(xs, scope, "arith.addi", "arith.addf"),
            "-" => {
                if xs.len() == 2 {
                    self.emit_unary_neg(xs, scope)
                } else {
                    self.emit_num_binop(xs, scope, "arith.subi", "arith.subf")
                }
            }
            "*" => self.emit_num_binop(xs, scope, "arith.muli", "arith.mulf"),
            "/" => self.emit_num_binop(xs, scope, "arith.divsi", "arith.divf"),
            "<" => self.emit_cmp(xs, scope, "slt", "olt"),
            ">" => self.emit_cmp(xs, scope, "sgt", "ogt"),
            "<=" => self.emit_cmp(xs, scope, "sle", "ole"),
            ">=" => self.emit_cmp(xs, scope, "sge", "oge"),
            "=" => self.emit_cmp(xs, scope, "eq", "oeq"),
            "if" => self.emit_if(xs, scope),
            "let" => self.emit_let(xs, scope),
            "do" => self.emit_do(xs, scope),
            "loop" => self.emit_loop(xs, scope),
            "float" => self.emit_convert(xs, scope, MlirTy::F64, "arith.sitofp"),
            "int" => self.emit_convert(xs, scope, MlirTy::I64, "arith.fptosi"),
            "buf-get" => self.emit_buf_get(xs, scope),
            "sqrt" => self.emit_math_unary(xs, scope, "math.sqrt"),
            "sin" => self.emit_math_unary(xs, scope, "math.sin"),
            "cos" => self.emit_math_unary(xs, scope, "math.cos"),
            "tan" => self.emit_math_unary(xs, scope, "math.tan"),
            "exp" => self.emit_math_unary(xs, scope, "math.exp"),
            "log" => self.emit_math_unary(xs, scope, "math.log"),
            "abs" => self.emit_abs(xs, scope),
            "min" => self.emit_min_max(xs, scope, "arith.minsi", "arith.minimumf"),
            "max" => self.emit_min_max(xs, scope, "arith.maxsi", "arith.maximumf"),
            "mod" | "rem" => self.emit_mod(xs, scope),
            "pow" => self.emit_pow(xs, scope),
            h if h == self.self_trigger => self.emit_self_call(xs, scope),
            h if self.module.registry.get(h).is_some() => {
                self.emit_external_call(xs, scope, h)
            }
            _ => Err(Error::Eval(format!(
                "native fn body: `{}` not supported — \
                 allowed: arithmetic, comparisons, if, let, do, loop, recur, self-call, \
                 or call to another defn-native",
                head
            ))),
        }
    }

    fn emit_num_binop(
        &mut self,
        xs: &[Value],
        scope: &Scope,
        int_op: &str,
        float_op: &str,
    ) -> Result<EmVal> {
        if xs.len() < 3 {
            return Err(Error::Arity {
                expected: ">= 2".into(),
                got: xs.len() - 1,
            });
        }
        let mut acc = self.emit(&xs[1], scope)?;
        require_numeric(&acc, int_op)?;
        for a in &xs[2..] {
            let rhs = self.emit(a, scope)?;
            require_numeric(&rhs, int_op)?;
            if acc.ty != rhs.ty {
                return Err(Error::Type(format!(
                    "{int_op}/{float_op}: mixed int/float not supported (got {} and {})",
                    acc.ty.as_str(),
                    rhs.ty.as_str()
                )));
            }
            let v = self.fresh();
            let op = if acc.ty.is_int() { int_op } else { float_op };
            self.line(format!("{v} = {op} {}, {} : {}", acc.ssa, rhs.ssa, acc.ty.as_str()));
            acc = EmVal { ssa: v, ty: acc.ty };
        }
        Ok(acc)
    }

    fn emit_cmp(
        &mut self,
        xs: &[Value],
        scope: &Scope,
        int_pred: &str,
        float_pred: &str,
    ) -> Result<EmVal> {
        if xs.len() != 3 {
            return Err(Error::Arity {
                expected: "2".into(),
                got: xs.len() - 1,
            });
        }
        let a = self.emit(&xs[1], scope)?;
        let b = self.emit(&xs[2], scope)?;
        require_numeric(&a, int_pred)?;
        require_numeric(&b, int_pred)?;
        if a.ty != b.ty {
            return Err(Error::Type(format!(
                "comparison: mixed types not supported (got {} and {})",
                a.ty.as_str(),
                b.ty.as_str()
            )));
        }
        let v = self.fresh();
        let op = if a.ty.is_int() {
            format!("arith.cmpi {int_pred}")
        } else {
            format!("arith.cmpf {float_pred}")
        };
        self.line(format!("{v} = {op}, {}, {} : {}", a.ssa, b.ssa, a.ty.as_str()));
        Ok(EmVal {
            ssa: v,
            ty: MlirTy::I1,
        })
    }

    fn emit_if(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        if xs.len() != 4 {
            return Err(Error::Eval(
                "native if: (if cond then else) required".into(),
            ));
        }
        let cond = self.emit(&xs[1], scope)?;
        if !cond.ty.is_bool() {
            return Err(Error::Type(
                "native if: condition must be a comparison / bool (i1)".into(),
            ));
        }
        // Infer result type from the then branch; else must match.
        // We emit ops in a temp buffer so we can know the then type before
        // writing the scf.if header (which declares result types).
        // Simpler: emit both branches, require both match, use that type.
        // MLIR scf.if requires type declaration up-front, so we speculate
        // the ret type. For simplicity we infer from the first branch,
        // then verify the second matches.
        // We'll use a small trick: buffer then/else into temp strings.
        let saved = std::mem::take(&mut self.body);
        let saved_ssa = self.ssa_counter;

        // then
        self.body.clear();
        let then_val = self.emit(&xs[2], scope)?;
        let then_body = std::mem::take(&mut self.body);

        // else
        let else_val = self.emit(&xs[3], scope)?;
        let else_body = std::mem::take(&mut self.body);
        let _ = saved_ssa;

        // Restore main buffer
        self.body = saved;
        let _ = self.body.len(); // no-op

        if then_val.ty != else_val.ty {
            return Err(Error::Type(format!(
                "if branches: then={} else={}",
                then_val.ty.as_str(),
                else_val.ty.as_str()
            )));
        }
        let rty = then_val.ty;

        let result = self.fresh();
        self.line(format!(
            "{result} = scf.if {} -> ({}) {{",
            cond.ssa,
            rty.as_str()
        ));
        // splice then body
        self.body.push_str(&then_body);
        self.line(format!("  scf.yield {} : {}", then_val.ssa, rty.as_str()));
        self.line("} else {");
        self.body.push_str(&else_body);
        self.line(format!("  scf.yield {} : {}", else_val.ssa, rty.as_str()));
        self.line("}");
        Ok(EmVal {
            ssa: result,
            ty: rty,
        })
    }

    fn emit_let(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        if xs.len() < 3 {
            return Err(Error::Eval("native let: (let [n v ...] body)".into()));
        }
        let Value::Vector(bindings) = &xs[1] else {
            return Err(Error::Eval("native let: bindings must be a vector".into()));
        };
        if bindings.len() % 2 != 0 {
            return Err(Error::Eval("native let: bindings must be pairs".into()));
        }
        let mut scope = scope.clone();
        let mut i = 0;
        while i < bindings.len() {
            let Value::Symbol(name) = &bindings[i] else {
                return Err(Error::Eval(
                    "native let: binding name must be symbol".into(),
                ));
            };
            let val = self.emit(&bindings[i + 1], &scope)?;
            scope.insert(name.to_string(), val);
            i += 2;
        }
        let mut result: Option<EmVal> = None;
        for f in &xs[2..] {
            result = Some(self.emit(f, &scope)?);
        }
        Ok(result.expect("let body non-empty"))
    }

    fn emit_do(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        if xs.len() < 2 {
            return Err(Error::Eval("native do: at least one form required".into()));
        }
        let mut result: Option<EmVal> = None;
        for f in &xs[1..] {
            result = Some(self.emit(f, scope)?);
        }
        Ok(result.expect("do body non-empty"))
    }

    /// `(loop [v1 init1 v2 init2 ...] body...)` — spawn a helper fn in the
    /// module that takes (outer_captures..., v1, v2, ...) and runs body.
    /// Emit a single call to the helper in the current fn.
    fn emit_loop(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        if xs.len() < 3 {
            return Err(Error::Eval(
                "native loop: (loop [v init ...] body) required".into(),
            ));
        }
        let Value::Vector(bindings) = &xs[1] else {
            return Err(Error::Eval("native loop: bindings must be a vector".into()));
        };
        if bindings.len() % 2 != 0 {
            return Err(Error::Eval("native loop: bindings must be pairs".into()));
        }

        // Evaluate init exprs in outer scope; collect (name, initial_ssa, type).
        let mut loop_vars: Vec<(Arc<str>, EmVal)> = Vec::with_capacity(bindings.len() / 2);
        let mut i = 0;
        // For sequential binding visibility, extend scope as we go.
        let mut cur = scope.clone();
        while i < bindings.len() {
            let Value::Symbol(name) = &bindings[i] else {
                return Err(Error::Eval(
                    "native loop: binding name must be symbol".into(),
                ));
            };
            let v = self.emit(&bindings[i + 1], &cur)?;
            cur.insert(name.to_string(), v.clone());
            loop_vars.push((Arc::clone(name), v));
            i += 2;
        }

        // Captures: every name in the enclosing scope becomes a helper param.
        // The outer emitter passes them as the initial args; they're
        // threaded unchanged on every recur.
        let mut capture_entries: Vec<(Arc<str>, EmVal)> = scope
            .iter()
            .map(|(k, v)| (Arc::<str>::from(k.as_str()), v.clone()))
            .collect();
        // Deterministic order so the helper's arg positions are stable.
        capture_entries.sort_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()));

        let helper_name = self.module.fresh_helper_name(&self.self_mlir_name);

        // Build helper param list: captures first, then loop vars.
        let mut helper_params: Vec<(Arc<str>, MlirTy)> = Vec::new();
        for (n, v) in &capture_entries {
            helper_params.push((Arc::clone(n), v.ty));
        }
        for (n, v) in &loop_vars {
            helper_params.push((Arc::clone(n), v.ty));
        }

        // Emit the helper fn body. Its scope starts with its params; its
        // self-trigger is "recur", with captures threaded automatically.
        let body_forms = &xs[2..];
        let helper_ret = self.ret; // loop's value is the enclosing fn's return (same-fn loop)
        let capture_count = capture_entries.len();
        {
            let mut helper = FnEmitter {
                body: String::new(),
                ssa_counter: 0,
                self_trigger: "recur".to_string(),
                self_mlir_name: helper_name.clone(),
                params: helper_params.clone(),
                ret: helper_ret,
                capture_names: capture_entries.iter().map(|(n, _)| Arc::clone(n)).collect(),
                module: self.module,
            };
            let helper_scope = helper.init_scope();
            let mut last: Option<EmVal> = None;
            for f in body_forms {
                last = Some(helper.emit(f, &helper_scope)?);
            }
            let body_val = last.expect("loop body non-empty");
            if body_val.ty != helper_ret {
                return Err(Error::Type(format!(
                    "loop body: expected {}, got {}",
                    helper_ret.as_str(),
                    body_val.ty.as_str()
                )));
            }
            helper.line(format!(
                "return {} : {}",
                body_val.ssa,
                helper_ret.as_str()
            ));
            let fn_text = helper.finalize();
            self.module.helpers.push(fn_text);
        }

        // Emit the call in the current fn: captures (current ssa) then init ssas.
        let mut arg_ssas: Vec<String> = Vec::with_capacity(capture_count + loop_vars.len());
        let mut arg_types: Vec<&'static str> = Vec::with_capacity(capture_count + loop_vars.len());
        for (_, v) in &capture_entries {
            arg_ssas.push(v.ssa.clone());
            arg_types.push(v.ty.as_str());
        }
        for (_, v) in &loop_vars {
            arg_ssas.push(v.ssa.clone());
            arg_types.push(v.ty.as_str());
        }
        let result = self.fresh();
        self.line(format!(
            "{result} = func.call @{}({}) : ({}) -> {}",
            helper_name,
            arg_ssas.join(", "),
            arg_types.join(", "),
            helper_ret.as_str()
        ));
        Ok(EmVal {
            ssa: result,
            ty: helper_ret,
        })
    }

    /// `(- x)` unary negation: `arith.subi 0, x` for i64; `arith.negf x`
    /// for f64. Lets kernels write `(- foo)` the way Clojure does.
    fn emit_unary_neg(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        let v = self.emit(&xs[1], scope)?;
        let out = self.fresh();
        match v.ty {
            MlirTy::I64 => {
                let zero = self.fresh();
                self.line(format!("{zero} = arith.constant 0 : i64"));
                self.line(format!("{out} = arith.subi {zero}, {} : i64", v.ssa));
            }
            MlirTy::F64 => {
                self.line(format!("{out} = arith.negf {} : f64", v.ssa));
            }
            _ => return Err(Error::Type(format!("(- x): bad type {}", v.ty.as_str()))),
        }
        Ok(EmVal { ssa: out, ty: v.ty })
    }

    /// Unary f64 math intrinsic lowered via MLIR's `math` dialect:
    /// sqrt, sin, cos, tan, exp, log. Argument must be f64.
    fn emit_math_unary(&mut self, xs: &[Value], scope: &Scope, op: &str) -> Result<EmVal> {
        if xs.len() != 2 {
            return Err(Error::Arity {
                expected: "1".into(),
                got: xs.len() - 1,
            });
        }
        let v = self.emit(&xs[1], scope)?;
        if !v.ty.is_float() {
            return Err(Error::Type(format!("{op}: argument must be f64")));
        }
        let out = self.fresh();
        self.line(format!("{out} = {op} {} : f64", v.ssa));
        Ok(EmVal {
            ssa: out,
            ty: MlirTy::F64,
        })
    }

    /// `(abs x)` — f64 absolute value via `math.absf`, or i64 via a subf-and-select
    /// equivalent (no arith.absi in current MLIR; use `max(x, -x)`).
    fn emit_abs(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        if xs.len() != 2 {
            return Err(Error::Arity {
                expected: "1".into(),
                got: xs.len() - 1,
            });
        }
        let v = self.emit(&xs[1], scope)?;
        let out = self.fresh();
        match v.ty {
            MlirTy::F64 => self.line(format!("{out} = math.absf {} : f64", v.ssa)),
            MlirTy::I64 => {
                // i64 abs via arith.maxsi(x, -x) — MLIR has no direct absi.
                let neg = self.fresh();
                let zero = self.fresh();
                self.line(format!("{zero} = arith.constant 0 : i64"));
                self.line(format!("{neg} = arith.subi {zero}, {} : i64", v.ssa));
                self.line(format!("{out} = arith.maxsi {}, {neg} : i64", v.ssa));
            }
            _ => return Err(Error::Type(format!("abs: bad type {}", v.ty.as_str()))),
        }
        Ok(EmVal { ssa: out, ty: v.ty })
    }

    fn emit_min_max(
        &mut self,
        xs: &[Value],
        scope: &Scope,
        int_op: &str,
        float_op: &str,
    ) -> Result<EmVal> {
        if xs.len() < 3 {
            return Err(Error::Arity {
                expected: ">= 2".into(),
                got: xs.len() - 1,
            });
        }
        let mut acc = self.emit(&xs[1], scope)?;
        for a in &xs[2..] {
            let rhs = self.emit(a, scope)?;
            if acc.ty != rhs.ty {
                return Err(Error::Type("min/max: mixed types".into()));
            }
            let v = self.fresh();
            let op = if acc.ty.is_int() { int_op } else { float_op };
            self.line(format!("{v} = {op} {}, {} : {}", acc.ssa, rhs.ssa, acc.ty.as_str()));
            acc = EmVal { ssa: v, ty: acc.ty };
        }
        Ok(acc)
    }

    /// `(mod a b)` — remainder. `arith.remsi` for i64, `math.fpowi` path
    /// not used (we use `arith.remf` which MLIR provides for floats).
    fn emit_mod(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        if xs.len() != 3 {
            return Err(Error::Arity {
                expected: "2".into(),
                got: xs.len() - 1,
            });
        }
        let a = self.emit(&xs[1], scope)?;
        let b = self.emit(&xs[2], scope)?;
        if a.ty != b.ty {
            return Err(Error::Type("mod: mixed types".into()));
        }
        let out = self.fresh();
        let op = if a.ty.is_int() {
            "arith.remsi"
        } else {
            "arith.remf"
        };
        self.line(format!("{out} = {op} {}, {} : {}", a.ssa, b.ssa, a.ty.as_str()));
        Ok(EmVal { ssa: out, ty: a.ty })
    }

    /// `(pow base exp)` — f64 power via `math.powf`. Both args must be f64.
    fn emit_pow(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        if xs.len() != 3 {
            return Err(Error::Arity {
                expected: "2".into(),
                got: xs.len() - 1,
            });
        }
        let a = self.emit(&xs[1], scope)?;
        let b = self.emit(&xs[2], scope)?;
        if !a.ty.is_float() || !b.ty.is_float() {
            return Err(Error::Type("pow: both args must be f64".into()));
        }
        let out = self.fresh();
        self.line(format!("{out} = math.powf {}, {} : f64", a.ssa, b.ssa));
        Ok(EmVal {
            ssa: out,
            ty: MlirTy::F64,
        })
    }

    /// `(buf-get buf i)` — load an f64 from a buffer at index `i`.
    /// `buf` is a pointer-as-i64 (FFI convention: pointer bits in an i64
    /// register). We inttoptr it locally each time, which LLVM will CSE
    /// across repeated uses in the same fn.
    fn emit_buf_get(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        if xs.len() != 3 {
            return Err(Error::Arity {
                expected: "2".into(),
                got: xs.len() - 1,
            });
        }
        let buf = self.emit(&xs[1], scope)?;
        let idx = self.emit(&xs[2], scope)?;
        if !buf.ty.is_int() {
            return Err(Error::Type(
                "buf-get: first arg must be an f64-buf (i64 handle)".into(),
            ));
        }
        if !idx.ty.is_int() {
            return Err(Error::Type("buf-get: index must be i64".into()));
        }
        let ptr = self.fresh();
        self.line(format!(
            "{ptr} = llvm.inttoptr {} : i64 to !llvm.ptr",
            buf.ssa
        ));
        let gep = self.fresh();
        self.line(format!(
            "{gep} = llvm.getelementptr {ptr}[{}] : (!llvm.ptr, i64) -> !llvm.ptr, f64",
            idx.ssa
        ));
        let v = self.fresh();
        self.line(format!(
            "{v} = llvm.load {gep} : !llvm.ptr -> f64"
        ));
        Ok(EmVal {
            ssa: v,
            ty: MlirTy::F64,
        })
    }

    /// `(float x)` → `arith.sitofp %x : i64 to f64`, and
    /// `(int x)` → `arith.fptosi %x : f64 to i64`. Makes fractal-style
    /// kernels with mixed int/float intermediates ergonomic.
    fn emit_convert(
        &mut self,
        xs: &[Value],
        scope: &Scope,
        to: MlirTy,
        op: &str,
    ) -> Result<EmVal> {
        if xs.len() != 2 {
            return Err(Error::Arity {
                expected: "1".into(),
                got: xs.len() - 1,
            });
        }
        let val = self.emit(&xs[1], scope)?;
        let v = self.fresh();
        self.line(format!(
            "{v} = {op} {} : {} to {}",
            val.ssa,
            val.ty.as_str(),
            to.as_str()
        ));
        Ok(EmVal { ssa: v, ty: to })
    }

    /// `(other-native args...)` — resolves via the registry, emits a
    /// `func.call` to the sanitized name, records the reference so the
    /// module finalizer inserts a forward declaration.
    fn emit_external_call(
        &mut self,
        xs: &[Value],
        scope: &Scope,
        name: &str,
    ) -> Result<EmVal> {
        let sig = self
            .module
            .registry
            .get(name)
            .cloned()
            .expect("caller checked registry.get(name)");
        let want = sig.arg_types.len();
        let got = xs.len() - 1;
        if got != want {
            return Err(Error::Arity {
                expected: format!("{want} (for native `{name}`)"),
                got,
            });
        }

        let mut arg_ssas: Vec<String> = Vec::with_capacity(want);
        let mut arg_ty_strs: Vec<&'static str> = Vec::with_capacity(want);
        for (i, arg_form) in xs[1..].iter().enumerate() {
            let param_ty = prim_to_mlir(sig.arg_types[i]);
            let val = self.emit(arg_form, scope)?;
            if val.ty != param_ty {
                return Err(Error::Type(format!(
                    "call to `{name}` arg {i}: expected {}, got {}",
                    param_ty.as_str(),
                    val.ty.as_str()
                )));
            }
            arg_ssas.push(val.ssa);
            arg_ty_strs.push(param_ty.as_str());
        }

        let ret_ty = prim_to_mlir(sig.ret_type);
        let mlir_name = sanitize_mlir_name(name);
        let v = self.fresh();
        self.line(format!(
            "{v} = func.call @{mlir_name}({}) : ({}) -> {}",
            arg_ssas.join(", "),
            arg_ty_strs.join(", "),
            ret_ty.as_str()
        ));
        self.module.referenced_externals.insert(mlir_name);
        Ok(EmVal { ssa: v, ty: ret_ty })
    }

    /// Main fn: `(self-name args...)`.
    /// Helper fn: `(recur args...)` — prepend captures automatically so the
    /// user only writes the loop vars, not the captures.
    fn emit_self_call(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        let head_name = match &xs[0] {
            Value::Symbol(s) => s.as_ref().to_string(),
            _ => unreachable!(),
        };
        let is_recur = head_name == "recur";

        // Expected args: for recur, only the loop vars (captures auto-threaded).
        let capture_count = self.capture_names.len();
        let expected_user_args = if is_recur {
            self.params.len() - capture_count
        } else {
            self.params.len()
        };
        let got = xs.len() - 1;
        if got != expected_user_args {
            return Err(Error::Arity {
                expected: format!("{expected_user_args}"),
                got,
            });
        }

        // Collect arg SSAs. For recur, captures come from the helper's OWN
        // params (unchanged passthrough); user forms supply the loop-var args.
        let mut arg_ssas: Vec<String> = Vec::with_capacity(self.params.len());
        let mut arg_types: Vec<&'static str> = Vec::with_capacity(self.params.len());
        if is_recur {
            // Captures: our own param SSAs %arg0..%arg{capture_count-1}.
            for i in 0..capture_count {
                arg_ssas.push(format!("%arg{i}"));
                arg_types.push(self.params[i].1.as_str());
            }
        }
        let arg_offset = if is_recur { capture_count } else { 0 };
        for (i, arg_form) in xs[1..].iter().enumerate() {
            let param_ty = self.params[arg_offset + i].1;
            let val = self.emit(arg_form, scope)?;
            if val.ty != param_ty {
                return Err(Error::Type(format!(
                    "self-call arg {i}: expected {}, got {}",
                    param_ty.as_str(),
                    val.ty.as_str()
                )));
            }
            arg_ssas.push(val.ssa);
            arg_types.push(param_ty.as_str());
        }

        let v = self.fresh();
        self.line(format!(
            "{v} = func.call @{}({}) : ({}) -> {}",
            self.self_mlir_name,
            arg_ssas.join(", "),
            arg_types.join(", "),
            self.ret.as_str()
        ));
        Ok(EmVal {
            ssa: v,
            ty: self.ret,
        })
    }

    /// Wrap the accumulated body into a full `  func.func @name(...) -> ret { ... }`.
    fn finalize(self) -> String {
        let mut out = String::new();
        out.push_str("  func.func @");
        out.push_str(&self.self_mlir_name);
        out.push('(');
        for (i, (_, ty)) in self.params.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("%arg{i}: {}", ty.as_str()));
        }
        out.push_str(&format!(
            ") -> {} attributes {{ llvm.emit_c_interface }} {{\n",
            self.ret.as_str()
        ));
        out.push_str(&self.body);
        out.push_str("  }\n");
        out
    }
}

fn require_numeric(v: &EmVal, op: &str) -> Result<()> {
    if !(v.ty.is_int() || v.ty.is_float()) {
        return Err(Error::Type(format!(
            "{op}: operand must be numeric (got {})",
            v.ty.as_str()
        )));
    }
    Ok(())
}

/// MLIR identifiers can't include `-`, `?`, `!`, `/`, `.` etc. that are
/// perfectly fine in Clojure symbols. Map them all to `_` for the native
/// symbol name; the cljrs-level binding keeps the original name.
pub fn sanitize_mlir_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() || out.chars().next().unwrap().is_ascii_digit() {
        out.insert(0, '_');
    }
    out
}

/// Result of emitting a module — the MLIR source plus the set of
/// previously-compiled native fn names this module calls. `compile.rs`
/// registers those names with the new ExecutionEngine.
#[derive(Debug)]
pub struct EmittedModule {
    pub source: String,
    /// MLIR-sanitized names of external natives referenced by this module.
    pub referenced_externals: HashSet<String>,
}

/// Emit a full MLIR module containing the main function plus any loop
/// helpers it spawns during emission. Cross-fn calls to `registry`
/// natives produce `func.call @NAME` + a forward declaration for NAME.
pub fn emit_module(
    fn_name: &str,
    params: &[(Arc<str>, PrimType)],
    ret_type: PrimType,
    body: &Value,
    registry: &NativeRegistry,
) -> Result<EmittedModule> {
    // Phase-2 FFI boundary: reject ^bool at params/return. Internal i1 (from
    // comparisons / if conditions) works fine; only the fn signature is
    // constrained because LLVM's i1 ABI at function boundaries is
    // platform-inconsistent and we haven't wired a safe FFI for it yet.
    for (_, t) in params {
        if matches!(t, PrimType::Bool) {
            return Err(Error::Eval(
                "native codegen: ^bool not supported at fn boundary yet \
                 (use ^i64 0/1 for now)"
                    .into(),
            ));
        }
    }
    if matches!(ret_type, PrimType::Bool) {
        return Err(Error::Eval(
            "native codegen: ^bool return not supported at fn boundary yet".into(),
        ));
    }

    let mlir_name = sanitize_mlir_name(fn_name);

    let mut module = ModuleState {
        helpers: Vec::new(),
        helper_counter: 0,
        registry,
        referenced_externals: HashSet::new(),
    };

    let main_params: Vec<(Arc<str>, MlirTy)> = params
        .iter()
        .map(|(n, t)| (Arc::clone(n), prim_to_mlir(*t)))
        .collect();
    let main_ret = prim_to_mlir(ret_type);

    let main_body_text = {
        let mut em = FnEmitter {
            body: String::new(),
            ssa_counter: 0,
            self_trigger: fn_name.to_string(),
            self_mlir_name: mlir_name.clone(),
            params: main_params,
            ret: main_ret,
            capture_names: Vec::new(),
            module: &mut module,
        };
        let scope = em.init_scope();
        let body_val = em.emit(body, &scope)?;
        if body_val.ty != main_ret {
            return Err(Error::Type(format!(
                "fn body: expected {}, got {}",
                main_ret.as_str(),
                body_val.ty.as_str()
            )));
        }
        em.line(format!(
            "return {} : {}",
            body_val.ssa,
            main_ret.as_str()
        ));
        em.finalize()
    };

    let mut out = String::from("module {\n");
    // Forward declarations for every referenced external, so MLIR's
    // symbol resolver knows their signature. compile.rs registers the
    // actual pointers with the ExecutionEngine before JIT.
    for mlir_ext_name in &module.referenced_externals {
        // Look up the original name by sanitizing-match — registry is
        // keyed by original names.
        let sig = module
            .registry
            .by_name
            .iter()
            .find(|(k, _)| sanitize_mlir_name(k) == *mlir_ext_name)
            .map(|(_, v)| v.clone())
            .expect("referenced external must exist in registry");
        let arg_tys: Vec<&str> = sig
            .arg_types
            .iter()
            .map(|t| prim_to_mlir(*t).as_str())
            .collect();
        let ret_ty = prim_to_mlir(sig.ret_type).as_str();
        // Forward declaration only — no body, no llvm.emit_c_interface.
        // The actual fn pointer we register via ExecutionEngine::register_symbol
        // is the inner MLIR symbol, not the _mlir_ciface_ wrapper, so the
        // declared symbol here must match that name (no ciface wrapper).
        out.push_str(&format!(
            "  func.func private @{mlir_ext_name}({}) -> {ret_ty}\n",
            arg_tys.join(", ")
        ));
    }
    out.push_str(&main_body_text);
    for h in module.helpers {
        out.push_str(&h);
    }
    out.push_str("}\n");
    Ok(EmittedModule {
        source: out,
        referenced_externals: module.referenced_externals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader;

    fn parse_body(src: &str) -> Value {
        let forms = reader::read_all(src).expect("read");
        assert_eq!(forms.len(), 1, "expected exactly one form");
        forms.into_iter().next().unwrap()
    }

    fn empty_reg() -> NativeRegistry {
        NativeRegistry::default()
    }

    #[test]
    fn emits_fib_without_error() {
        let body = parse_body("(if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))");
        let params: &[(Arc<str>, PrimType)] = &[(Arc::from("n"), PrimType::I64)];
        let emitted =
            emit_module("fib", params, PrimType::I64, &body, &empty_reg()).expect("emit");
        let src = &emitted.source;
        assert!(src.contains("func.func @fib("));
        assert!(src.contains("arith.cmpi slt"));
        assert!(src.contains("scf.if"));
        assert!(src.contains("func.call @fib"));
        assert!(src.contains("llvm.emit_c_interface"));
    }

    #[test]
    fn emits_loop_as_helper() {
        let body = parse_body(
            "(loop [i 0 acc 0] (if (> i n) acc (recur (+ i 1) (+ acc i))))",
        );
        let params: &[(Arc<str>, PrimType)] = &[(Arc::from("n"), PrimType::I64)];
        let emitted =
            emit_module("sum-to", params, PrimType::I64, &body, &empty_reg()).expect("emit");
        let src = &emitted.source;
        assert!(src.contains("func.func @sum_to("));
        assert!(src.contains("func.func @sum_to_loop_0("));
        assert!(src.contains("func.call @sum_to_loop_0"));
    }

    #[test]
    fn emits_float_ops() {
        let body = parse_body("(+ a b)");
        let params: &[(Arc<str>, PrimType)] = &[
            (Arc::from("a"), PrimType::F64),
            (Arc::from("b"), PrimType::F64),
        ];
        let emitted =
            emit_module("fadd", params, PrimType::F64, &body, &empty_reg()).expect("emit");
        let src = &emitted.source;
        assert!(src.contains("arith.addf"));
        assert!(src.contains(": f64"));
    }

    #[test]
    fn rejects_mixed_int_float() {
        let body = parse_body("(+ a b)");
        let params: &[(Arc<str>, PrimType)] = &[
            (Arc::from("a"), PrimType::I64),
            (Arc::from("b"), PrimType::F64),
        ];
        let err = emit_module("f", params, PrimType::I64, &body, &empty_reg()).unwrap_err();
        assert!(
            err.to_string().contains("mixed int/float") || err.to_string().contains("mixed"),
            "got: {err}"
        );
    }

    #[test]
    fn rejects_unsupported_body_form() {
        let body = parse_body("(println n)");
        let params: &[(Arc<str>, PrimType)] = &[(Arc::from("n"), PrimType::I64)];
        let err = emit_module("f", params, PrimType::I64, &body, &empty_reg()).unwrap_err();
        assert!(err.to_string().contains("not supported"), "got: {err}");
    }
}
