//! cljrs AST → MLIR textual source.
//!
//! Phase-2 subset (i64 only):
//!   - Int literals → `arith.constant N : i64`
//!   - Symbol ref → param SSA / let-bound SSA
//!   - `(+ a b …)` `(- a b …)` `(* a b …)` → `arith.addi/subi/muli` (left fold)
//!   - `(< a b)` `(> a b)` `(<= a b)` `(>= a b)` `(= a b)` → `arith.cmpi` (i1)
//!   - `(if cond then else)` → `scf.if … -> i64`
//!   - `(let [n v …] body…)` → sequential SSA bindings
//!   - `(do a b c)` → yields last
//!   - `(self-fn x y …)` → `func.call @self(...) : (i64…) -> i64`
//!
//! Everything else — f64, bool, other fn calls, closures, collections — is
//! rejected with a clear phase-2 error. Phase 3 widens the language.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::types::PrimType;
use crate::value::Value;

/// Materialized SSA value with tracked MLIR type — comparisons produce i1,
/// arithmetic/loads/calls produce i64. Tracking the type separately lets
/// `if` validate that its condition is an i1.
#[derive(Clone)]
struct EmVal {
    ssa: String,
    ty: MlirTy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MlirTy {
    I64,
    I1,
}

impl MlirTy {
    fn as_str(self) -> &'static str {
        match self {
            MlirTy::I64 => "i64",
            MlirTy::I1 => "i1",
        }
    }
}

fn prim_to_mlir(p: PrimType) -> Result<MlirTy> {
    match p {
        PrimType::I64 => Ok(MlirTy::I64),
        PrimType::F64 | PrimType::Bool => Err(Error::Eval(format!(
            "native codegen phase 2: only ^i64 is supported; got ^{}",
            p.as_str()
        ))),
    }
}

type Scope = HashMap<String, EmVal>;

struct Emitter<'a> {
    out: String,
    ssa_counter: usize,
    /// cljrs-level name as written in source; used to recognize self-calls
    /// in the AST (matches on Value::Symbol(name)).
    fn_name: &'a str,
    /// MLIR-safe version of fn_name; used in `func.func @NAME` and every
    /// `func.call @NAME` so the emitted module's symbol references resolve.
    mlir_name: &'a str,
    arg_types: Vec<PrimType>,
    ret_type: PrimType,
}

impl<'a> Emitter<'a> {
    fn fresh(&mut self) -> String {
        let n = self.ssa_counter;
        self.ssa_counter += 1;
        format!("%v{n}")
    }

    fn line(&mut self, s: impl AsRef<str>) {
        self.out.push_str("    ");
        self.out.push_str(s.as_ref());
        self.out.push('\n');
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
            Value::Symbol(s) => scope
                .get(s.as_ref())
                .cloned()
                .ok_or_else(|| Error::Unbound(s.to_string())),
            Value::List(xs) => {
                if xs.is_empty() {
                    return Err(Error::Eval("empty list in native fn body".into()));
                }
                let Value::Symbol(head) = &xs[0] else {
                    return Err(Error::Eval(
                        "native call: head must be a symbol".into(),
                    ));
                };
                match head.as_ref() {
                    "+" => self.emit_binop(xs, scope, "arith.addi"),
                    "-" => self.emit_binop(xs, scope, "arith.subi"),
                    "*" => self.emit_binop(xs, scope, "arith.muli"),
                    "<" => self.emit_cmp(xs, scope, "slt"),
                    ">" => self.emit_cmp(xs, scope, "sgt"),
                    "<=" => self.emit_cmp(xs, scope, "sle"),
                    ">=" => self.emit_cmp(xs, scope, "sge"),
                    "=" => self.emit_cmp(xs, scope, "eq"),
                    "if" => self.emit_if(xs, scope),
                    "let" => self.emit_let(xs, scope),
                    "do" => self.emit_do(xs, scope),
                    other if other == self.fn_name => self.emit_self_call(xs, scope),
                    other => Err(Error::Eval(format!(
                        "native fn body (phase 2): `{other}` not supported — \
                         only self-recursion, int arithmetic, comparisons, if, let, do"
                    ))),
                }
            }
            _ => Err(Error::Eval(format!(
                "native fn body (phase 2): can't codegen {}",
                form.type_name()
            ))),
        }
    }

    fn emit_binop(&mut self, xs: &[Value], scope: &Scope, op: &str) -> Result<EmVal> {
        if xs.len() < 3 {
            return Err(Error::Arity {
                expected: ">= 2".into(),
                got: xs.len() - 1,
            });
        }
        let mut acc = self.emit(&xs[1], scope)?;
        require_i64(&acc, op)?;
        for a in &xs[2..] {
            let rhs = self.emit(a, scope)?;
            require_i64(&rhs, op)?;
            let v = self.fresh();
            self.line(format!("{v} = {op} {}, {} : i64", acc.ssa, rhs.ssa));
            acc = EmVal {
                ssa: v,
                ty: MlirTy::I64,
            };
        }
        Ok(acc)
    }

    fn emit_cmp(&mut self, xs: &[Value], scope: &Scope, pred: &str) -> Result<EmVal> {
        if xs.len() != 3 {
            return Err(Error::Arity {
                expected: "2".into(),
                got: xs.len() - 1,
            });
        }
        let a = self.emit(&xs[1], scope)?;
        let b = self.emit(&xs[2], scope)?;
        require_i64(&a, pred)?;
        require_i64(&b, pred)?;
        let v = self.fresh();
        self.line(format!("{v} = arith.cmpi {pred}, {}, {} : i64", a.ssa, b.ssa));
        Ok(EmVal {
            ssa: v,
            ty: MlirTy::I1,
        })
    }

    fn emit_if(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        if xs.len() != 4 {
            return Err(Error::Eval(
                "native if requires exactly (if cond then else)".into(),
            ));
        }
        let cond = self.emit(&xs[1], scope)?;
        if cond.ty != MlirTy::I1 {
            return Err(Error::Type(
                "native if: condition must be a comparison (i1)".into(),
            ));
        }
        let ret_ty = prim_to_mlir(self.ret_type)?;
        let result = self.fresh();
        self.line(format!(
            "{result} = scf.if {} -> ({}) {{",
            cond.ssa,
            ret_ty.as_str()
        ));
        let then_val = self.emit(&xs[2], scope)?;
        self.require_matches_ret("if then-branch", &then_val)?;
        self.line(format!(
            "  scf.yield {} : {}",
            then_val.ssa,
            ret_ty.as_str()
        ));
        self.line("} else {");
        let else_val = self.emit(&xs[3], scope)?;
        self.require_matches_ret("if else-branch", &else_val)?;
        self.line(format!(
            "  scf.yield {} : {}",
            else_val.ssa,
            ret_ty.as_str()
        ));
        self.line("}");
        Ok(EmVal {
            ssa: result,
            ty: ret_ty,
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

    fn emit_self_call(&mut self, xs: &[Value], scope: &Scope) -> Result<EmVal> {
        let want = self.arg_types.len();
        let got = xs.len() - 1;
        if got != want {
            return Err(Error::Arity {
                expected: format!("{want}"),
                got,
            });
        }
        let mut arg_ssas = Vec::with_capacity(want);
        for (i, arg) in xs[1..].iter().enumerate() {
            let val = self.emit(arg, scope)?;
            let expected = prim_to_mlir(self.arg_types[i])?;
            if val.ty != expected {
                return Err(Error::Type(format!(
                    "native self-call arg {i}: expected {}, got {}",
                    expected.as_str(),
                    val.ty.as_str()
                )));
            }
            arg_ssas.push(val.ssa);
        }
        let ret = prim_to_mlir(self.ret_type)?;
        let arg_tys: Vec<&str> = self.arg_types.iter().map(|_| "i64").collect();
        let sig = format!("({}) -> {}", arg_tys.join(", "), ret.as_str());
        let v = self.fresh();
        self.line(format!(
            "{v} = func.call @{}({}) : {sig}",
            self.mlir_name,
            arg_ssas.join(", ")
        ));
        Ok(EmVal { ssa: v, ty: ret })
    }

    fn require_matches_ret(&self, label: &str, val: &EmVal) -> Result<()> {
        let expected = prim_to_mlir(self.ret_type)?;
        if val.ty != expected {
            return Err(Error::Type(format!(
                "{label}: expected {}, got {}",
                expected.as_str(),
                val.ty.as_str()
            )));
        }
        Ok(())
    }
}

fn require_i64(v: &EmVal, op: &str) -> Result<()> {
    if v.ty != MlirTy::I64 {
        return Err(Error::Type(format!(
            "{op}: operand must be i64 (got {})",
            v.ty.as_str()
        )));
    }
    Ok(())
}

/// MLIR identifiers can't include `-`, `?`, `!`, `/`, `.` etc. that are
/// perfectly fine in Clojure symbols. Map them all to `_` for the native
/// symbol name; the cljrs-level binding keeps the original name.
/// Collisions across fns would matter in a multi-fn module — today each
/// fn has its own module, so any collision is inside the one we own.
pub fn sanitize_mlir_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else if i == 0 {
            // MLIR identifiers can't start with a digit either — prefix.
            out.push('_');
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

/// Emit a full MLIR module containing one compiled function.
///
/// The `llvm.emit_c_interface` attribute lets us call via `invoke_packed`,
/// while direct `lookup` still returns a pointer to the native body for
/// the zero-overhead fast path used by eval_list dispatch.
pub fn emit_module(
    fn_name: &str,
    params: &[(Arc<str>, PrimType)],
    ret_type: PrimType,
    body: &Value,
) -> Result<String> {
    // Phase-2 sanity: i64 only.
    for (_, t) in params {
        prim_to_mlir(*t)?;
    }
    prim_to_mlir(ret_type)?;

    // MLIR name is sanitized (no hyphens, etc.); the emitter uses this for
    // both the function definition and any self-call references, so the
    // two always agree.
    let mlir_name = sanitize_mlir_name(fn_name);

    let mut e = Emitter {
        out: String::new(),
        ssa_counter: 0,
        fn_name,
        mlir_name: &mlir_name,
        arg_types: params.iter().map(|(_, t)| *t).collect(),
        ret_type,
    };

    e.out.push_str("module {\n");
    e.out.push_str(&format!("  func.func @{mlir_name}("));
    for (i, _) in params.iter().enumerate() {
        if i > 0 {
            e.out.push_str(", ");
        }
        e.out.push_str(&format!("%arg{i}: i64"));
    }
    e.out
        .push_str(") -> i64 attributes { llvm.emit_c_interface } {\n");

    let mut scope: Scope = HashMap::new();
    for (i, (name, _)) in params.iter().enumerate() {
        scope.insert(
            name.to_string(),
            EmVal {
                ssa: format!("%arg{i}"),
                ty: MlirTy::I64,
            },
        );
    }

    let body_val = e.emit(body, &scope)?;
    e.require_matches_ret("fn body", &body_val)?;
    e.line(format!("return {} : i64", body_val.ssa));
    e.out.push_str("  }\n");
    e.out.push_str("}\n");
    Ok(e.out)
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

    #[test]
    fn emits_fib_without_error() {
        let body = parse_body("(if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))");
        let params: &[(Arc<str>, PrimType)] = &[(Arc::from("n"), PrimType::I64)];
        let src = emit_module("fib", params, PrimType::I64, &body).expect("emit");
        // Quick structural checks — content correctness is verified via JIT
        // execution in compile.rs's test.
        assert!(src.contains("func.func @fib("));
        assert!(src.contains("arith.cmpi slt"));
        assert!(src.contains("scf.if"));
        assert!(src.contains("func.call @fib"));
        assert!(src.contains("llvm.emit_c_interface"));
    }

    #[test]
    fn rejects_float_params() {
        let body = parse_body("n");
        let params: &[(Arc<str>, PrimType)] = &[(Arc::from("n"), PrimType::F64)];
        let err = emit_module("f", params, PrimType::I64, &body).unwrap_err();
        assert!(
            err.to_string().contains("only ^i64 is supported"),
            "got: {err}"
        );
    }

    #[test]
    fn rejects_unsupported_body_form() {
        let body = parse_body("(println n)");
        let params: &[(Arc<str>, PrimType)] = &[(Arc::from("n"), PrimType::I64)];
        let err = emit_module("f", params, PrimType::I64, &body).unwrap_err();
        assert!(err.to_string().contains("not supported"), "got: {err}");
    }
}
