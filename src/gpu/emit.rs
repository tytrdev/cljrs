//! cljrs AST → WGSL source. The "Clojure kernel DSL" — a subset of
//! cljrs that compiles to a WGSL compute shader with the fixed ABI
//! used by `Gpu::run_elementwise_f32`.
//!
//! # DSL shape
//!
//! ```clojure
//! (defn-gpu double-it ^f32 [^i32 i ^f32 v]
//!   (* v 2.0))
//! ```
//!
//! The first param is the thread index (u32 in WGSL, exposed as i32 for
//! ergonomics). The second is the element loaded from `src`. The body
//! evaluates to an f32 and is stored into `dst[i]`.
//!
//! # Supported operations
//!
//! - Arithmetic: `+ - * /` (binary and n-ary reduced), unary `-`
//! - Comparisons: `< > <= >= =` (always yielding bool)
//! - Math: `sqrt sin cos tan exp log abs min max floor ceil pow`
//! - Control: `if`, `let`, `do`
//! - Literals: i32, f32, bool
//! - Type conversions: `float`, `int`
//!
//! Not yet: loops, buffer cross-access, atomics. Next pass.

use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::value::Value;

/// WGSL type for an emitted SSA value. Kernel bodies must return `F32`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ty {
    I32,
    U32,
    F32,
    Bool,
}

impl Ty {
    fn as_str(self) -> &'static str {
        match self {
            Ty::I32 => "i32",
            Ty::U32 => "u32",
            Ty::F32 => "f32",
            Ty::Bool => "bool",
        }
    }
    fn is_int(self) -> bool {
        matches!(self, Ty::I32 | Ty::U32)
    }
    fn is_float(self) -> bool {
        matches!(self, Ty::F32)
    }
    fn is_bool(self) -> bool {
        matches!(self, Ty::Bool)
    }
}

#[derive(Clone)]
struct Val {
    expr: String,
    ty: Ty,
}

type Scope = HashMap<String, Val>;

/// Parse a cljrs type tag symbol into our internal Ty.
pub fn parse_gpu_type(name: &str) -> Result<Ty> {
    match name {
        "i32" | "int" => Ok(Ty::I32),
        "u32" => Ok(Ty::U32),
        "f32" | "float" => Ok(Ty::F32),
        "bool" => Ok(Ty::Bool),
        _ => Err(Error::Eval(format!(
            "gpu: unknown type `{name}` (allowed: i32, u32, f32, bool)"
        ))),
    }
}

fn ty_for_symbol(s: &Value) -> Result<Ty> {
    match s {
        Value::Symbol(n) => parse_gpu_type(n.as_ref()),
        _ => Err(Error::Eval("gpu: type must be a symbol".into())),
    }
}

/// Top-level emitter: given (index-param, value-param, body), produce a
/// full WGSL compute shader that fits the elementwise-f32 ABI.
///
/// `index_name` and `value_name` are the user-chosen symbols for the
/// thread index and the loaded element respectively.
pub fn emit_elementwise(
    index_name: &str,
    value_name: &str,
    body: &Value,
) -> Result<String> {
    let mut ctx = Ctx::new();
    let mut scope = Scope::new();
    // The thread index is surfaced as i32 (friendlier for cljrs users),
    // even though WGSL uses u32 under the hood. We bitcast on emit.
    scope.insert(
        index_name.to_string(),
        Val {
            expr: format!("{}", "k_i"),
            ty: Ty::I32,
        },
    );
    scope.insert(
        value_name.to_string(),
        Val {
            expr: "k_v".into(),
            ty: Ty::F32,
        },
    );

    let result = ctx.emit(body, &scope)?;
    if result.ty != Ty::F32 {
        return Err(Error::Type(format!(
            "gpu body must return f32, got {}",
            result.ty.as_str()
        )));
    }

    let mut out = String::new();
    out.push_str(
        r#"@group(0) @binding(0) var<storage, read>       src: array<f32>;
@group(0) @binding(1) var<storage, read_write> dst: array<f32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let k_gid = gid.x;
    if (k_gid >= arrayLength(&src)) { return; }
    let k_i: i32 = i32(k_gid);
    let k_v: f32 = src[k_gid];
"#,
    );
    out.push_str(&ctx.body);
    out.push_str(&format!("    dst[k_gid] = {};\n", result.expr));
    out.push_str("}\n");
    Ok(out)
}

struct Ctx {
    body: String,
    counter: usize,
}

impl Ctx {
    fn new() -> Self {
        Ctx {
            body: String::new(),
            counter: 0,
        }
    }
    fn fresh(&mut self) -> String {
        let n = self.counter;
        self.counter += 1;
        format!("_s{n}")
    }
    fn line(&mut self, s: &str) {
        self.body.push_str("    ");
        self.body.push_str(s);
        self.body.push('\n');
    }

    fn bind(&mut self, val: Val) -> Val {
        let name = self.fresh();
        self.line(&format!("let {name}: {} = {};", val.ty.as_str(), val.expr));
        Val {
            expr: name,
            ty: val.ty,
        }
    }

    fn emit(&mut self, form: &Value, scope: &Scope) -> Result<Val> {
        match form {
            Value::Int(n) => Ok(Val {
                expr: format!("({}i)", n),
                ty: Ty::I32,
            }),
            Value::Float(f) => {
                // Always emit a decimal so WGSL parses as f32.
                let lit = if f.is_finite() && f.fract() == 0.0 {
                    format!("{f:.1}")
                } else {
                    format!("{f}")
                };
                Ok(Val {
                    expr: format!("f32({lit})"),
                    ty: Ty::F32,
                })
            }
            Value::Bool(b) => Ok(Val {
                expr: b.to_string(),
                ty: Ty::Bool,
            }),
            Value::Symbol(s) => scope
                .get(s.as_ref())
                .cloned()
                .ok_or_else(|| Error::Unbound(s.to_string())),
            Value::List(xs) => self.emit_call(xs, scope),
            _ => Err(Error::Eval(format!(
                "gpu: can't codegen {}",
                form.type_name()
            ))),
        }
    }

    fn emit_call(&mut self, xs: &[Value], scope: &Scope) -> Result<Val> {
        if xs.is_empty() {
            return Err(Error::Eval("gpu: empty call".into()));
        }
        let head = match &xs[0] {
            Value::Symbol(s) => s.as_ref(),
            _ => return Err(Error::Eval("gpu: head must be a symbol".into())),
        };
        match head {
            "+" => self.emit_nary(xs, scope, "+"),
            "-" => {
                if xs.len() == 2 {
                    self.emit_unary_neg(xs, scope)
                } else {
                    self.emit_nary(xs, scope, "-")
                }
            }
            "*" => self.emit_nary(xs, scope, "*"),
            "/" => self.emit_nary(xs, scope, "/"),
            "<" => self.emit_cmp(xs, scope, "<"),
            ">" => self.emit_cmp(xs, scope, ">"),
            "<=" => self.emit_cmp(xs, scope, "<="),
            ">=" => self.emit_cmp(xs, scope, ">="),
            "=" => self.emit_cmp(xs, scope, "=="),
            "if" => self.emit_if(xs, scope),
            "let" => self.emit_let(xs, scope),
            "do" => self.emit_do(xs, scope),
            "float" => self.emit_cast(xs, scope, Ty::F32),
            "int" => self.emit_cast(xs, scope, Ty::I32),
            "sqrt" | "sin" | "cos" | "tan" | "exp" | "log" | "floor" | "ceil" | "abs" => {
                self.emit_math_unary(xs, scope, head)
            }
            "min" => self.emit_math_binary(xs, scope, "min"),
            "max" => self.emit_math_binary(xs, scope, "max"),
            "pow" => self.emit_math_binary(xs, scope, "pow"),
            _ => Err(Error::Eval(format!(
                "gpu: `{head}` not supported — \
                 allowed: arith, cmp, if, let, do, math intrinsics"
            ))),
        }
    }

    fn emit_nary(&mut self, xs: &[Value], scope: &Scope, op: &str) -> Result<Val> {
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
                return Err(Error::Type(format!(
                    "gpu: mixed types in `{op}` ({} and {})",
                    acc.ty.as_str(),
                    rhs.ty.as_str()
                )));
            }
            let expr = format!("({} {op} {})", acc.expr, rhs.expr);
            acc = self.bind(Val { expr, ty: acc.ty });
        }
        Ok(acc)
    }

    fn emit_cmp(&mut self, xs: &[Value], scope: &Scope, op: &str) -> Result<Val> {
        if xs.len() != 3 {
            return Err(Error::Arity {
                expected: "2".into(),
                got: xs.len() - 1,
            });
        }
        let a = self.emit(&xs[1], scope)?;
        let b = self.emit(&xs[2], scope)?;
        if a.ty != b.ty {
            return Err(Error::Type("gpu: comparison mixed types".into()));
        }
        Ok(Val {
            expr: format!("({} {op} {})", a.expr, b.expr),
            ty: Ty::Bool,
        })
    }

    fn emit_unary_neg(&mut self, xs: &[Value], scope: &Scope) -> Result<Val> {
        let v = self.emit(&xs[1], scope)?;
        if !(v.ty.is_int() || v.ty.is_float()) {
            return Err(Error::Type("gpu: unary - on non-numeric".into()));
        }
        Ok(Val {
            expr: format!("(-{})", v.expr),
            ty: v.ty,
        })
    }

    fn emit_if(&mut self, xs: &[Value], scope: &Scope) -> Result<Val> {
        if xs.len() != 4 {
            return Err(Error::Eval("gpu if: (if cond then else)".into()));
        }
        let cond = self.emit(&xs[1], scope)?;
        if !cond.ty.is_bool() {
            return Err(Error::Type("gpu if: condition must be bool".into()));
        }
        // Emit both arms and pick via WGSL's select() for simple cases.
        // Our emitter is pure-expression, so we rely on select(a,b,cond).
        let then_v = self.emit(&xs[2], scope)?;
        let else_v = self.emit(&xs[3], scope)?;
        if then_v.ty != else_v.ty {
            return Err(Error::Type(format!(
                "gpu if: branch types differ ({} and {})",
                then_v.ty.as_str(),
                else_v.ty.as_str()
            )));
        }
        // WGSL's select takes (false_val, true_val, condition).
        let expr = format!("select({}, {}, {})", else_v.expr, then_v.expr, cond.expr);
        Ok(self.bind(Val { expr, ty: then_v.ty }))
    }

    fn emit_let(&mut self, xs: &[Value], scope: &Scope) -> Result<Val> {
        if xs.len() < 3 {
            return Err(Error::Eval("gpu let: (let [n v ...] body...)".into()));
        }
        let bindings = match &xs[1] {
            Value::Vector(v) => v,
            _ => return Err(Error::Eval("gpu let: bindings must be a vector".into())),
        };
        if bindings.len() % 2 != 0 {
            return Err(Error::Eval("gpu let: bindings must be pairs".into()));
        }
        let mut cur = scope.clone();
        let mut i = 0;
        while i < bindings.len() {
            let name = match &bindings[i] {
                Value::Symbol(s) => s.to_string(),
                _ => return Err(Error::Eval("gpu let: binding name must be symbol".into())),
            };
            let val = self.emit(&bindings[i + 1], &cur)?;
            let bound = self.bind(val);
            cur.insert(name, bound);
            i += 2;
        }
        let mut last: Option<Val> = None;
        for f in &xs[2..] {
            last = Some(self.emit(f, &cur)?);
        }
        last.ok_or_else(|| Error::Eval("gpu let: empty body".into()))
    }

    fn emit_do(&mut self, xs: &[Value], scope: &Scope) -> Result<Val> {
        if xs.len() < 2 {
            return Err(Error::Eval("gpu do: empty body".into()));
        }
        let mut last: Option<Val> = None;
        for f in &xs[1..] {
            last = Some(self.emit(f, scope)?);
        }
        Ok(last.unwrap())
    }

    fn emit_cast(&mut self, xs: &[Value], scope: &Scope, to: Ty) -> Result<Val> {
        if xs.len() != 2 {
            return Err(Error::Arity {
                expected: "1".into(),
                got: xs.len() - 1,
            });
        }
        let v = self.emit(&xs[1], scope)?;
        Ok(Val {
            expr: format!("{}({})", to.as_str(), v.expr),
            ty: to,
        })
    }

    fn emit_math_unary(&mut self, xs: &[Value], scope: &Scope, fn_name: &str) -> Result<Val> {
        if xs.len() != 2 {
            return Err(Error::Arity {
                expected: "1".into(),
                got: xs.len() - 1,
            });
        }
        let v = self.emit(&xs[1], scope)?;
        if !v.ty.is_float() {
            return Err(Error::Type(format!("gpu {fn_name}: expects f32")));
        }
        Ok(Val {
            expr: format!("{fn_name}({})", v.expr),
            ty: Ty::F32,
        })
    }

    fn emit_math_binary(&mut self, xs: &[Value], scope: &Scope, fn_name: &str) -> Result<Val> {
        if xs.len() != 3 {
            return Err(Error::Arity {
                expected: "2".into(),
                got: xs.len() - 1,
            });
        }
        let a = self.emit(&xs[1], scope)?;
        let b = self.emit(&xs[2], scope)?;
        if a.ty != b.ty {
            return Err(Error::Type(format!(
                "gpu {fn_name}: mixed types ({} and {})",
                a.ty.as_str(),
                b.ty.as_str()
            )));
        }
        Ok(Val {
            expr: format!("{fn_name}({}, {})", a.expr, b.expr),
            ty: a.ty,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader;

    fn parse(src: &str) -> Value {
        reader::read_all(src)
            .expect("read")
            .into_iter()
            .next()
            .unwrap()
    }

    #[test]
    fn emits_elementwise_double() {
        let body = parse("(* v 2.0)");
        let wgsl = emit_elementwise("i", "v", &body).expect("emit");
        assert!(wgsl.contains("@compute"));
        assert!(wgsl.contains("dst[k_gid] ="));
        assert!(wgsl.contains("(k_v * f32(2.0))"));
    }

    #[test]
    fn emits_if_as_select() {
        let body = parse("(if (< v 0.0) (- v) v)");
        let wgsl = emit_elementwise("i", "v", &body).expect("emit");
        assert!(wgsl.contains("select("), "expected WGSL select in:\n{wgsl}");
    }

    #[test]
    fn emits_let_and_math() {
        let body = parse("(let [x (sin v) y (cos v)] (+ (* x x) (* y y)))");
        let wgsl = emit_elementwise("i", "v", &body).expect("emit");
        assert!(wgsl.contains("sin(k_v)"));
        assert!(wgsl.contains("cos(k_v)"));
    }

    #[test]
    fn rejects_non_f32_return() {
        let body = parse("(int v)");
        let err = emit_elementwise("i", "v", &body).unwrap_err();
        assert!(err.to_string().contains("must return f32"), "got: {err}");
    }
}
