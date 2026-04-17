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

/// WGSL identifiers can't use `-` (or other Clojure-legal symbol chars).
/// Loop vars get prefixed with `_lv<counter>_` anyway, so we just need
/// to strip the unfriendly chars to keep WGSL parsers happy while
/// leaving the name readable in generated code.
fn sanitize_ident(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
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

/// Emit a 2D pixel-shading kernel. The body is given the pixel
/// coordinates + uniforms (width, height, t-ms, 4 sliders), and must
/// return a u32 packed as 0x00RRGGBB. The host calls it as a 2D
/// workgroup grid and reads back a width*height u32 buffer.
///
/// `params`: the cljrs symbols the body uses, in order:
///   [x y width height t-ms s0 s1 s2 s3]
/// The host binds these to their corresponding slots.
pub fn emit_pixel(params: &[&str; 9], body: &Value) -> Result<String> {
    let mut ctx = Ctx::new();
    let mut scope = Scope::new();
    scope.insert(params[0].into(), Val { expr: "k_x".into(), ty: Ty::I32 });
    scope.insert(params[1].into(), Val { expr: "k_y".into(), ty: Ty::I32 });
    scope.insert(params[2].into(), Val { expr: "i32(params.width)".into(), ty: Ty::I32 });
    scope.insert(params[3].into(), Val { expr: "i32(params.height)".into(), ty: Ty::I32 });
    scope.insert(params[4].into(), Val { expr: "params.t_ms".into(), ty: Ty::I32 });
    scope.insert(params[5].into(), Val { expr: "params.s0".into(), ty: Ty::I32 });
    scope.insert(params[6].into(), Val { expr: "params.s1".into(), ty: Ty::I32 });
    scope.insert(params[7].into(), Val { expr: "params.s2".into(), ty: Ty::I32 });
    scope.insert(params[8].into(), Val { expr: "params.s3".into(), ty: Ty::I32 });

    let result = ctx.emit(body, &scope)?;
    // Body must return i32 or u32 — it's the packed pixel color.
    let final_expr = match result.ty {
        Ty::U32 => result.expr,
        Ty::I32 => format!("u32({})", result.expr),
        _ => {
            return Err(Error::Type(format!(
                "gpu pixel body must return i32 or u32 (packed 0xRRGGBB), got {}",
                result.ty.as_str()
            )));
        }
    };

    let mut out = String::from(
        r#"struct Params {
  width: u32,
  height: u32,
  t_ms: i32,
  s0: i32,
  s1: i32,
  s2: i32,
  s3: i32,
  _pad: i32,
};

@group(0) @binding(0) var<uniform>           params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) { return; }
    let k_x: i32 = i32(gid.x);
    let k_y: i32 = i32(gid.y);
"#,
    );
    out.push_str(&ctx.body);
    out.push_str(&format!(
        "    dst[gid.y * params.width + gid.x] = {final_expr};\n"
    ));
    out.push_str("}\n");
    Ok(out)
}

/// State threaded through tail-position emission inside a `loop`. We
/// need to know where to write the result and how to translate `recur`.
struct LoopTail {
    /// (wgsl_var_name, type) for each loop variable, in declaration
    /// order. recur writes these via temporaries to preserve ordering.
    vars: Vec<(String, Ty)>,
    /// WGSL name of the result variable that holds the loop's eventual
    /// value. Each non-recur tail form assigns here + `break`s.
    result_var: String,
    /// Discovered result type. We do a probe pass through the body to
    /// discover this before declaring the result var (since WGSL needs
    /// a type on declaration). Probe pass writes to a throwaway buffer.
    result_ty_known: Option<Ty>,
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
            "mod" | "rem" => self.emit_infix_binary(xs, scope, "%"),
            "bit-and" => self.emit_bitwise(xs, scope, "&"),
            "bit-or" => self.emit_bitwise(xs, scope, "|"),
            "bit-xor" => self.emit_bitwise(xs, scope, "^"),
            "bit-shift-left" => self.emit_bitwise(xs, scope, "<<"),
            "bit-shift-right" => self.emit_bitwise(xs, scope, ">>"),
            "u32" => self.emit_cast(xs, scope, Ty::U32),
            "i32" => self.emit_cast(xs, scope, Ty::I32),
            "f32" => self.emit_cast(xs, scope, Ty::F32),
            "loop" => self.emit_loop(xs, scope),
            "recur" => Err(Error::Eval(
                "gpu: `recur` only valid in tail position of a `loop`".into(),
            )),
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

    fn emit_infix_binary(&mut self, xs: &[Value], scope: &Scope, op: &str) -> Result<Val> {
        if xs.len() != 3 {
            return Err(Error::Arity {
                expected: "2".into(),
                got: xs.len() - 1,
            });
        }
        let a = self.emit(&xs[1], scope)?;
        let b = self.emit(&xs[2], scope)?;
        if a.ty != b.ty {
            return Err(Error::Type(format!("gpu {op}: mixed types")));
        }
        Ok(Val {
            expr: format!("({} {op} {})", a.expr, b.expr),
            ty: a.ty,
        })
    }

    fn emit_bitwise(&mut self, xs: &[Value], scope: &Scope, op: &str) -> Result<Val> {
        if xs.len() != 3 {
            return Err(Error::Arity {
                expected: "2".into(),
                got: xs.len() - 1,
            });
        }
        let a = self.emit(&xs[1], scope)?;
        let b = self.emit(&xs[2], scope)?;
        if !a.ty.is_int() || !b.ty.is_int() {
            return Err(Error::Type(format!("gpu {op}: operands must be integers")));
        }
        if a.ty != b.ty {
            return Err(Error::Type(format!("gpu {op}: mixed int types")));
        }
        Ok(Val {
            expr: format!("({} {op} {})", a.expr, b.expr),
            ty: a.ty,
        })
    }

    /// `(loop [v1 init1 v2 init2 ...] body...)` — compiled as a WGSL
    /// `loop { ... }` with mutable `var` locals for each loop variable.
    /// The body must end in either `(recur ...)` (which rewrites the
    /// vars + continues) or a plain value expression (which breaks out
    /// with that value as the loop's result). `if` / `let` / `do`
    /// propagate tail-position to their sub-expressions.
    ///
    /// Loop bodies MUST reach a non-recur expression on every path —
    /// WGSL requires `loop` to terminate, and we don't insert any
    /// implicit iteration cap. Kernels that naturally iterate a fixed
    /// N times should just `(recur ...)` until a counter-based `if`.
    fn emit_loop(&mut self, xs: &[Value], scope: &Scope) -> Result<Val> {
        if xs.len() < 3 {
            return Err(Error::Eval(
                "gpu loop: (loop [v init ...] body) required".into(),
            ));
        }
        let bindings = match &xs[1] {
            Value::Vector(v) => v,
            _ => return Err(Error::Eval("gpu loop: bindings must be a vector".into())),
        };
        if bindings.len() % 2 != 0 {
            return Err(Error::Eval("gpu loop: bindings must be pairs".into()));
        }
        // Eval each init in the outer scope first (so later inits can
        // reference earlier ones). Bind to fresh `var`s. Later we
        // reassign to them on `recur`.
        let mut cur = scope.clone();
        let mut loop_vars: Vec<(String, String, Ty)> = Vec::new(); // (user_name, wgsl_var, ty)
        let mut i = 0;
        while i < bindings.len() {
            let name = match &bindings[i] {
                Value::Symbol(s) => s.to_string(),
                _ => return Err(Error::Eval("gpu loop: binding name must be symbol".into())),
            };
            let init = self.emit(&bindings[i + 1], &cur)?;
            let var_name = format!("_lv{}_{}", self.counter, sanitize_ident(&name));
            self.counter += 1;
            self.line(&format!(
                "var {var_name}: {} = {};",
                init.ty.as_str(),
                init.expr
            ));
            cur.insert(
                name.clone(),
                Val { expr: var_name.clone(), ty: init.ty },
            );
            loop_vars.push((name, var_name, init.ty));
            i += 2;
        }

        // Type-check body's result by compiling once in a "probe" mode:
        // we need the result type before we can declare the result var.
        // Simple approach: require the first plain-value form we reach
        // to reveal its type, and declare the result var lazily. We use
        // an indirection: emit into a temp ctx first, throw it away,
        // just to discover the type. That's expensive but robust.
        let probe_ty = {
            let saved_body = std::mem::take(&mut self.body);
            let saved_counter = self.counter;
            let mut probe_tail = LoopTail {
                vars: loop_vars.iter().map(|(_, v, t)| (v.clone(), *t)).collect(),
                result_var: "__probe".into(),
                result_ty_known: None,
            };
            let _ = self.emit_tail(&xs[2..], &cur, &mut probe_tail);
            self.body = saved_body;
            self.counter = saved_counter;
            probe_tail
                .result_ty_known
                .ok_or_else(|| Error::Eval(
                    "gpu loop: body must reach a non-recur value on some path".into(),
                ))?
        };

        let result_var = format!("_lr{}", self.counter);
        self.counter += 1;
        // Declare result var (zero-init, reassigned on loop exit).
        let zero = match probe_ty {
            Ty::I32 => "0i",
            Ty::U32 => "0u",
            Ty::F32 => "0.0",
            Ty::Bool => "false",
        };
        self.line(&format!(
            "var {result_var}: {} = {};",
            probe_ty.as_str(),
            zero
        ));
        self.line("loop {");
        let mut tail = LoopTail {
            vars: loop_vars.iter().map(|(_, v, t)| (v.clone(), *t)).collect(),
            result_var: result_var.clone(),
            result_ty_known: Some(probe_ty),
        };
        self.emit_tail(&xs[2..], &cur, &mut tail)?;
        self.line("}");
        Ok(Val {
            expr: result_var,
            ty: probe_ty,
        })
    }

    /// Emit a sequence of forms in tail-position of the enclosing loop.
    /// The last form is the tail; earlier forms are ordinary expressions
    /// evaluated for side effect (though our DSL is pure, they're still
    /// bound to let-exprs).
    fn emit_tail(&mut self, forms: &[Value], scope: &Scope, tail: &mut LoopTail) -> Result<()> {
        if forms.is_empty() {
            return Err(Error::Eval("gpu loop: empty body".into()));
        }
        // Evaluate all but the last as normal expressions.
        for f in &forms[..forms.len() - 1] {
            self.emit(f, scope)?;
        }
        self.emit_tail_form(&forms[forms.len() - 1], scope, tail)
    }

    /// Emit a single form at tail position.
    fn emit_tail_form(&mut self, form: &Value, scope: &Scope, tail: &mut LoopTail) -> Result<()> {
        match form {
            Value::List(xs) if !xs.is_empty() => {
                if let Value::Symbol(head) = &xs[0] {
                    match head.as_ref() {
                        "recur" => return self.emit_tail_recur(xs, scope, tail),
                        "if" => return self.emit_tail_if(xs, scope, tail),
                        "let" => return self.emit_tail_let(xs, scope, tail),
                        "do" => {
                            if xs.len() < 2 {
                                return Err(Error::Eval("gpu do: empty body".into()));
                            }
                            return self.emit_tail(&xs[1..], scope, tail);
                        }
                        _ => {}
                    }
                }
                // Ordinary call in tail position → compute value, break.
                let v = self.emit(form, scope)?;
                self.emit_break_with(v, tail)
            }
            _ => {
                let v = self.emit(form, scope)?;
                self.emit_break_with(v, tail)
            }
        }
    }

    fn emit_break_with(&mut self, v: Val, tail: &mut LoopTail) -> Result<()> {
        if let Some(t) = tail.result_ty_known
            && t != v.ty
        {
            return Err(Error::Type(format!(
                "gpu loop: mixed result types ({} and {})",
                t.as_str(),
                v.ty.as_str()
            )));
        }
        tail.result_ty_known = Some(v.ty);
        self.line(&format!("{} = {};", tail.result_var, v.expr));
        self.line("break;");
        Ok(())
    }

    fn emit_tail_recur(
        &mut self,
        xs: &[Value],
        scope: &Scope,
        tail: &mut LoopTail,
    ) -> Result<()> {
        let got = xs.len() - 1;
        if got != tail.vars.len() {
            return Err(Error::Arity {
                expected: format!("{}", tail.vars.len()),
                got,
            });
        }
        // Evaluate all new values FIRST into temporaries, so recur
        // expressions that reference the loop vars see the old values
        // (matches Clojure's loop semantics). Then assign.
        let mut temps: Vec<(String, Val)> = Vec::with_capacity(tail.vars.len());
        for (i, form) in xs[1..].iter().enumerate() {
            let v = self.emit(form, scope)?;
            let (_, ty) = tail.vars[i];
            if v.ty != ty {
                return Err(Error::Type(format!(
                    "gpu recur arg {i}: expected {}, got {}",
                    ty.as_str(),
                    v.ty.as_str()
                )));
            }
            let tmp = format!("_rt{}", self.counter);
            self.counter += 1;
            self.line(&format!("let {tmp}: {} = {};", ty.as_str(), v.expr));
            temps.push((tmp, v));
        }
        for (i, (tmp, _)) in temps.iter().enumerate() {
            let (var, _) = &tail.vars[i];
            self.line(&format!("{var} = {tmp};"));
        }
        self.line("continue;");
        Ok(())
    }

    fn emit_tail_if(
        &mut self,
        xs: &[Value],
        scope: &Scope,
        tail: &mut LoopTail,
    ) -> Result<()> {
        if xs.len() != 4 {
            return Err(Error::Eval("gpu if: (if cond then else)".into()));
        }
        let cond = self.emit(&xs[1], scope)?;
        if !cond.ty.is_bool() {
            return Err(Error::Type("gpu if: condition must be bool".into()));
        }
        self.line(&format!("if ({}) {{", cond.expr));
        self.emit_tail_form(&xs[2], scope, tail)?;
        self.line("} else {");
        self.emit_tail_form(&xs[3], scope, tail)?;
        self.line("}");
        Ok(())
    }

    fn emit_tail_let(
        &mut self,
        xs: &[Value],
        scope: &Scope,
        tail: &mut LoopTail,
    ) -> Result<()> {
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
        self.emit_tail(&xs[2..], &cur, tail)
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
    fn emits_loop_recur_shape() {
        // Simple summation loop: (loop [i 0 acc 0.0] (if (>= i 10) acc (recur (+ i 1) (+ acc 1.0))))
        let body = parse(
            "(loop [i 0 acc 0.0] (if (>= i 10) acc (recur (+ i 1) (+ acc 1.0))))",
        );
        let wgsl = emit_elementwise("i", "v", &body).expect("emit");
        assert!(wgsl.contains("var _lv"), "expected var declarations in:\n{wgsl}");
        assert!(wgsl.contains("loop {"), "expected loop block in:\n{wgsl}");
        assert!(wgsl.contains("continue;"), "expected continue in:\n{wgsl}");
        assert!(wgsl.contains("break;"), "expected break in:\n{wgsl}");
    }

    #[test]
    fn rejects_non_f32_return() {
        let body = parse("(int v)");
        let err = emit_elementwise("i", "v", &body).unwrap_err();
        assert!(err.to_string().contains("must return f32"), "got: {err}");
    }
}
