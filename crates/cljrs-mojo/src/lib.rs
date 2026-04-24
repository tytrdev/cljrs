//! cljrs-mojo: transpile cljrs source to Mojo source.
//!
//! Entry point: [`emit`]. Parses a cljrs source string with
//! `cljrs::reader::read_all`, lowers to a small Mojo-ish AST, runs
//! tier-specific passes, and pretty-prints Mojo source.
//!
//! ## Coverage
//!
//! ### Supported
//!
//! - **Definitions**: `def`, `defn`, `defn-mojo`, `parameter-fn-mojo`,
//!   `always-inline-fn-mojo`, `raises-fn-mojo`, `parametric-fn-mojo`,
//!   `defstruct-mojo`, `deftrait-mojo`, `defn-method-mojo`, `alias-mojo`.
//! - **Primitive types**: `^i8 ^i16 ^i32 ^i64 ^u8 ^u16 ^u32 ^u64 ^f32
//!   ^f64 ^bf16 ^bool ^str`, plus user-defined named types that start
//!   with a capital letter.
//! - **Composite types**: `^SIMDf32x4` → `SIMD[DType.float32, 4]`,
//!   `^List-f32` → `List[Float32]`, `^Opt-f32` → `Optional[Float32]`,
//!   `^Tuple-i32-f32` → `Tuple[Int32, Float32]`.
//! - **Argument conventions**: `^owned`, `^borrowed`, `^inout`, `^ref`
//!   stack with a type tag: `[^inout ^i32 x]` → `inout x: Int32`.
//! - **Control flow**: `if`, `cond` (flat `if/elif/else`), `do`, `let`,
//!   `loop`/`recur` (with for-range fast path), `(for-mojo [i lo hi])`
//!   sugar, `(break)`, `(continue)`.
//! - **Exceptions**: `(raise (Error "msg"))`, bare `(raise)` re-raises,
//!   `(try BODY (catch T as n HANDLER)...)`, `raises-fn-mojo` for
//!   signatures that propagate.
//! - **Compile-time**: `alias-mojo`, `parametric-fn-mojo`
//!   (emits `fn foo[n: Int, T: AnyType]`), and `(parameter-if ...)`
//!   inside parametric bodies.
//! - **Collections**: `(list e1 e2 ...)` → `List[T](e1, ...)`,
//!   `(nth xs i)` → `xs[i]`, `(len xs)` → `len(xs)`.
//! - **Optional**: `(some x)` → `Optional(x)`, `(none)` → `None`,
//!   `(unwrap o)` → `o.value()`.
//! - **Traits & methods**: `(deftrait-mojo Shape (area ^f32 []))`,
//!   `(defstruct-mojo Square :Shape [^f32 side])` → `struct Square(Shape):`,
//!   `(defn-method-mojo Vec3 length ^f32 [] ...)` appends indented
//!   methods inside the matching struct.
//! - **Assertions**: `(mojo-assert cond)` / `(mojo-assert cond msg)` →
//!   `debug_assert(...)`.
//! - **String helpers**: `(str-len s)`, `(str-slice s a b)`,
//!   `(str-split s sep)`.
//! - **Introspection**: `(isinstance-mojo v T)` → `isinstance(v, T)`.
//! - **I/O**: `(print x)`, `(println x)` → `print(x)`;
//!   `(format "n={}" n)` → `"n=" + String(n)` left-folded.
//! - **Math**: trig (`sin cos tan asin acos atan atan2 sinh cosh tanh`),
//!   exponentials (`exp expm1 log log1p log2 log10`), roots & rounding
//!   (`sqrt cbrt floor ceil round trunc`), plus `pow`, `hypot`,
//!   `copysign`, `abs`, `min`, `max`.
//! - **Tiers**: Readable (keeps `# cljrs:` comments), Optimized
//!   (const-fold + CSE + inline 1-stmt return fns), Max (adds
//!   `@always_inline` to pure, non-recursive, ≤10-stmt fns with
//!   control depth ≤ 2; strips comments).
//!
//! ### Not supported (errors on sight)
//!
//! - Collection literals `[1 2 3]`, `{:a 1}`, `#{:a}` in expr position —
//!   use `(list ...)` / `(dict ...)` / `(set ...)` instead.
//! - Variadic params (`& rest`).
//! - Higher-order fn refs as arguments — fn symbols must be called directly.
//! - `loop`, `let`, `cond`, `recur`, `for-mojo`, `try`, `raise`,
//!   `parameter-if`, `mojo-assert` in non-tail / non-stmt positions.
//!
//! Forms outside this set produce errors that quote the offending form.

pub mod ast;
pub mod runtime;
pub mod tier1;
pub mod tier2;
pub mod tier3;

use crate::ast::{MExpr, MFn, MItem, MModule, MStmt, MType};

/// Tier selector. `Readable` preserves cljrs source as comments; `Optimized`
/// runs const-fold / CSE / small-fn inlining; `Max` adds `@always_inline`
/// and parameter-specialization hooks and strips all comments.
#[derive(Debug, Clone, Copy)]
pub enum Tier {
    Readable,
    Optimized,
    Max,
}

/// Transpile a cljrs source string to Mojo source at the requested tier.
pub fn emit(src: &str, tier: Tier) -> Result<String, String> {
    let forms = cljrs::reader::read_all(src).map_err(|e| format!("read error: {e}"))?;
    let mut module = tier1::lower_module(&forms)?;
    match tier {
        Tier::Readable => {}
        Tier::Optimized => tier2::optimize(&mut module),
        Tier::Max => tier3::specialize(&mut module),
    }
    Ok(print_module(&module, tier))
}

// ---------------- printer ----------------

fn print_module(m: &MModule, tier: Tier) -> String {
    let mut out = String::new();
    for imp in &m.imports {
        out.push_str(imp);
        out.push('\n');
    }
    if !m.imports.is_empty() {
        out.push('\n');
    }
    for (i, item) in m.items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        print_item(&mut out, item, tier);
    }
    out
}

fn print_item(out: &mut String, item: &MItem, tier: Tier) {
    match item {
        MItem::Fn(f) => print_fn(out, f, tier),
        MItem::Struct { name, fields, methods, trait_impl, comment } => {
            if let Some(c) = comment {
                if matches!(tier, Tier::Readable) {
                    out.push_str("# cljrs: ");
                    out.push_str(c);
                    out.push('\n');
                }
            }
            out.push_str("@value\n");
            out.push_str("struct ");
            out.push_str(name);
            if let Some(t) = trait_impl {
                out.push('(');
                out.push_str(t);
                out.push(')');
            }
            out.push_str(":\n");
            for (fname, fty) in fields {
                out.push_str("    var ");
                out.push_str(fname);
                if !matches!(fty, MType::Infer) {
                    out.push_str(": ");
                    out.push_str(&fty.as_str());
                }
                out.push('\n');
            }
            // Explicit __init__ for clarity.
            out.push_str("    fn __init__(out self");
            for (fname, fty) in fields {
                out.push_str(", ");
                out.push_str(fname);
                if !matches!(fty, MType::Infer) {
                    out.push_str(": ");
                    out.push_str(&fty.as_str());
                }
            }
            out.push_str("):\n");
            if fields.is_empty() && methods.is_empty() {
                out.push_str("        pass\n");
            } else if fields.is_empty() {
                out.push_str("        pass\n");
            } else {
                for (fname, _) in fields {
                    out.push_str("        self.");
                    out.push_str(fname);
                    out.push_str(" = ");
                    out.push_str(fname);
                    out.push('\n');
                }
            }
            // Methods: each emitted as `fn name(self, ...):` indented.
            for m in methods {
                out.push('\n');
                print_fn_indented(out, m, tier, 1);
            }
        }
        MItem::Alias { name, ty, value, comment } => {
            if let Some(c) = comment {
                if matches!(tier, Tier::Readable) {
                    out.push_str("# cljrs: ");
                    out.push_str(c);
                    out.push('\n');
                }
            }
            out.push_str("alias ");
            out.push_str(name);
            if !matches!(ty, MType::Infer) {
                out.push_str(": ");
                out.push_str(&ty.as_str());
            }
            out.push_str(" = ");
            print_expr(out, value);
            out.push('\n');
        }
        MItem::Trait { name, methods, comment } => {
            if let Some(c) = comment {
                if matches!(tier, Tier::Readable) {
                    out.push_str("# cljrs: ");
                    out.push_str(c);
                    out.push('\n');
                }
            }
            out.push_str("trait ");
            out.push_str(name);
            out.push_str(":\n");
            if methods.is_empty() {
                out.push_str("    pass\n");
            } else {
                for m in methods {
                    out.push_str("    fn ");
                    out.push_str(&m.name);
                    out.push_str("(self");
                    for (n, t, c) in &m.params {
                        out.push_str(", ");
                        out.push_str(c.as_prefix());
                        out.push_str(n);
                        if !matches!(t, MType::Infer) {
                            out.push_str(": ");
                            out.push_str(&t.as_str());
                        }
                    }
                    out.push(')');
                    if !matches!(m.ret, MType::Infer) {
                        out.push_str(" -> ");
                        out.push_str(&m.ret.as_str());
                    }
                    out.push_str(": ...\n");
                }
            }
        }
        MItem::Var { name, ty, value, comment } => {
            if let Some(c) = comment {
                if matches!(tier, Tier::Readable) {
                    out.push_str("# cljrs: ");
                    out.push_str(c);
                    out.push('\n');
                }
            }
            out.push_str("var ");
            out.push_str(name);
            if !matches!(ty, MType::Infer) {
                out.push_str(": ");
                out.push_str(&ty.as_str());
            }
            out.push_str(" = ");
            print_expr(out, value);
            out.push('\n');
        }
    }
}

fn print_fn(out: &mut String, f: &MFn, tier: Tier) {
    print_fn_indented(out, f, tier, 0);
}

fn print_fn_indented(out: &mut String, f: &MFn, tier: Tier, base: usize) {
    if let Some(c) = &f.comment {
        if matches!(tier, Tier::Readable | Tier::Optimized) {
            indent(out, base);
            out.push_str("# cljrs: ");
            out.push_str(c);
            out.push('\n');
        }
    }
    for d in &f.decorators {
        indent(out, base);
        out.push_str(d);
        out.push('\n');
    }
    indent(out, base);
    out.push_str("fn ");
    out.push_str(&f.name);
    if !f.cparams.is_empty() {
        out.push('[');
        for (i, (n, t)) in f.cparams.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(n);
            out.push_str(": ");
            out.push_str(t);
        }
        out.push(']');
    }
    out.push('(');
    let mut idx = 0;
    if f.is_method {
        out.push_str("self");
        idx = 1;
    }
    for (n, t, c) in f.params.iter() {
        if idx > 0 {
            out.push_str(", ");
        }
        idx += 1;
        out.push_str(c.as_prefix());
        out.push_str(n);
        if !matches!(t, MType::Infer) {
            out.push_str(": ");
            out.push_str(&t.as_str());
        }
    }
    out.push(')');
    if f.raises {
        out.push_str(" raises");
    }
    if !matches!(f.ret, MType::Infer) {
        out.push_str(" -> ");
        out.push_str(&f.ret.as_str());
    }
    out.push_str(":\n");
    if f.body.is_empty() {
        indent(out, base + 1);
        out.push_str("pass\n");
    } else {
        for s in &f.body {
            print_stmt(out, s, base + 1);
        }
    }
}

fn indent(out: &mut String, lvl: usize) {
    for _ in 0..lvl {
        out.push_str("    ");
    }
}

fn print_stmt(out: &mut String, s: &MStmt, lvl: usize) {
    match s {
        MStmt::Let { name, ty, value } => {
            indent(out, lvl);
            out.push_str("var ");
            out.push_str(name);
            if !matches!(ty, MType::Infer) {
                out.push_str(": ");
                out.push_str(&ty.as_str());
            }
            out.push_str(" = ");
            print_expr(out, value);
            out.push('\n');
        }
        MStmt::Assign { name, value } => {
            indent(out, lvl);
            out.push_str(name);
            out.push_str(" = ");
            print_expr(out, value);
            out.push('\n');
        }
        MStmt::Return(e) => {
            indent(out, lvl);
            out.push_str("return ");
            print_expr(out, e);
            out.push('\n');
        }
        MStmt::Expr(e) => {
            indent(out, lvl);
            print_expr(out, e);
            out.push('\n');
        }
        MStmt::If { cond, then, els } => {
            indent(out, lvl);
            out.push_str("if ");
            print_expr(out, cond);
            out.push_str(":\n");
            if then.is_empty() {
                indent(out, lvl + 1);
                out.push_str("pass\n");
            } else {
                for s in then {
                    print_stmt(out, s, lvl + 1);
                }
            }
            // Flatten cond chains: `else: if X: ... else: ...` → `elif X: ... else: ...`
            let mut tail = els;
            loop {
                if tail.is_empty() {
                    break;
                }
                if tail.len() == 1 {
                    if let MStmt::If { cond: ec, then: et, els: ee } = &tail[0] {
                        indent(out, lvl);
                        out.push_str("elif ");
                        print_expr(out, ec);
                        out.push_str(":\n");
                        if et.is_empty() {
                            indent(out, lvl + 1);
                            out.push_str("pass\n");
                        } else {
                            for s in et {
                                print_stmt(out, s, lvl + 1);
                            }
                        }
                        tail = ee;
                        continue;
                    }
                }
                indent(out, lvl);
                out.push_str("else:\n");
                for s in tail {
                    print_stmt(out, s, lvl + 1);
                }
                break;
            }
        }
        MStmt::While { cond, body } => {
            indent(out, lvl);
            out.push_str("while ");
            print_expr(out, cond);
            out.push_str(":\n");
            if body.is_empty() {
                indent(out, lvl + 1);
                out.push_str("pass\n");
            } else {
                for s in body {
                    print_stmt(out, s, lvl + 1);
                }
            }
        }
        MStmt::Break => {
            indent(out, lvl);
            out.push_str("break\n");
        }
        MStmt::Continue => {
            indent(out, lvl);
            out.push_str("continue\n");
        }
        MStmt::Raise(e) => {
            indent(out, lvl);
            out.push_str("raise ");
            print_expr(out, e);
            out.push('\n');
        }
        MStmt::ReRaise => {
            indent(out, lvl);
            out.push_str("raise\n");
        }
        MStmt::Try { body, catches } => {
            indent(out, lvl);
            out.push_str("try:\n");
            if body.is_empty() {
                indent(out, lvl + 1);
                out.push_str("pass\n");
            } else {
                for s in body {
                    print_stmt(out, s, lvl + 1);
                }
            }
            for c in catches {
                indent(out, lvl);
                out.push_str("except");
                if !c.ty.is_empty() {
                    out.push(' ');
                    out.push_str(&c.ty);
                }
                if let Some(n) = &c.name {
                    out.push_str(" as ");
                    out.push_str(n);
                }
                out.push_str(":\n");
                if c.body.is_empty() {
                    indent(out, lvl + 1);
                    out.push_str("pass\n");
                } else {
                    for s in &c.body {
                        print_stmt(out, s, lvl + 1);
                    }
                }
            }
        }
        MStmt::ParameterIf { cond, then, els } => {
            indent(out, lvl);
            out.push_str("@parameter\n");
            indent(out, lvl);
            out.push_str("if ");
            print_expr(out, cond);
            out.push_str(":\n");
            if then.is_empty() {
                indent(out, lvl + 1);
                out.push_str("pass\n");
            } else {
                for s in then {
                    print_stmt(out, s, lvl + 1);
                }
            }
            if !els.is_empty() {
                indent(out, lvl);
                out.push_str("else:\n");
                for s in els {
                    print_stmt(out, s, lvl + 1);
                }
            }
        }
        MStmt::Raw(s) => {
            indent(out, lvl);
            out.push_str(s);
            out.push('\n');
        }
        MStmt::ForRange { name, ty: _, lo, hi, body } => {
            indent(out, lvl);
            out.push_str("for ");
            out.push_str(name);
            out.push_str(" in range(");
            print_expr(out, lo);
            out.push_str(", ");
            print_expr(out, hi);
            out.push_str("):\n");
            if body.is_empty() {
                indent(out, lvl + 1);
                out.push_str("pass\n");
            } else {
                for s in body {
                    print_stmt(out, s, lvl + 1);
                }
            }
        }
    }
}

fn print_expr(out: &mut String, e: &MExpr) {
    match e {
        MExpr::IntLit(i) => {
            out.push_str(&i.to_string());
        }
        MExpr::FloatLit(f) => {
            let s = format_float(*f);
            out.push_str(&s);
        }
        MExpr::BoolLit(b) => out.push_str(if *b { "True" } else { "False" }),
        MExpr::Var(n) => out.push_str(n),
        MExpr::BinOp { op, lhs, rhs } => {
            out.push('(');
            print_expr(out, lhs);
            out.push(' ');
            out.push_str(op);
            out.push(' ');
            print_expr(out, rhs);
            out.push(')');
        }
        MExpr::UnOp { op, rhs } => {
            out.push('(');
            out.push_str(op);
            if op.chars().next().map_or(false, |c| c.is_alphabetic()) {
                out.push(' ');
            }
            print_expr(out, rhs);
            out.push(')');
        }
        MExpr::Call { callee, args } => {
            // Special virtual callees for indexing, slicing, and method calls.
            if callee == "__index__" && args.len() == 2 {
                print_expr(out, &args[0]);
                out.push('[');
                print_expr(out, &args[1]);
                out.push(']');
                return;
            }
            if callee == "__slice__" && args.len() == 3 {
                print_expr(out, &args[0]);
                out.push('[');
                print_expr(out, &args[1]);
                out.push(':');
                print_expr(out, &args[2]);
                out.push(']');
                return;
            }
            if let Some(method) = callee.strip_prefix("__method__") {
                // First arg is receiver, rest are method args.
                if !args.is_empty() {
                    print_expr(out, &args[0]);
                    out.push('.');
                    out.push_str(method);
                    out.push('(');
                    for (i, a) in args[1..].iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        print_expr(out, a);
                    }
                    out.push(')');
                    return;
                }
            }
            out.push_str(callee);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                print_expr(out, a);
            }
            out.push(')');
        }
        MExpr::IfExpr { cond, then, els } => {
            out.push('(');
            print_expr(out, then);
            out.push_str(" if ");
            print_expr(out, cond);
            out.push_str(" else ");
            print_expr(out, els);
            out.push(')');
        }
        MExpr::Field { obj, field } => {
            print_expr(out, obj);
            out.push('.');
            out.push_str(field);
        }
        MExpr::StrLit(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    '\r' => out.push_str("\\r"),
                    _ => out.push(c),
                }
            }
            out.push('"');
        }
    }
}

fn format_float(f: f64) -> String {
    if f.is_nan() {
        return "Float64.nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Float64.inf".into() } else { "-Float64.inf".into() };
    }
    if f == f.trunc() && f.abs() < 1e16 {
        format!("{:.1}", f)
    } else {
        // Use Rust's default, which is shortest round-trippable.
        let s = format!("{f}");
        if s.contains('.') || s.contains('e') || s.contains('E') {
            s
        } else {
            format!("{s}.0")
        }
    }
}
