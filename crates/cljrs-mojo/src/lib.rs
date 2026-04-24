//! cljrs-mojo: transpile cljrs source to Mojo source.
//!
//! Entry point: [`emit`]. Parses a cljrs source string with
//! `cljrs::reader::read_all`, lowers to a small Mojo-ish AST, runs
//! tier-specific passes, and pretty-prints Mojo source.
//!
//! ## Coverage (running tally)
//!
//! - typed `def` / `defn` / `defn-mojo` with `^Tag` metadata
//! - numeric primitives: `^i8 ^i16 ^i32 ^i64 ^u8 ^u16 ^u32 ^u64 ^f32 ^f64
//!   ^bf16 ^bool`, plus `^str` → `String`
//! - control flow: `if`, `cond` (flat `if/elif/else`), `do`, `let`,
//!   `loop`/`recur`. Single-counter
//!   loops auto-emit `for i in range(lo, hi):` instead of a while/break
//!   trampoline. Sugar `(for-mojo [i lo hi] body...)` skips the loop/recur
//!   ceremony entirely. `(break)` and `(continue)` lower verbatim inside
//!   loop bodies.
//! - arithmetic, comparisons, booleans, math fns (`sin cos tan sqrt exp
//!   log floor ceil pow`, `abs min max`)
//! - structs: `(defstruct-mojo Vec3 [^f32 x ^f32 y ^f32 z])` emits an
//!   `@value` Mojo struct with explicit `__init__`. Field access via
//!   `(. obj field)`. User-defined capitalized type names pass through
//!   as named types in fn signatures.
//! - SIMD types: `^SIMDf32x4` / `^SIMDi64x8` / `^SIMDbf16x16` →
//!   `SIMD[DType.float32, 4]` etc.
//! - string literals (`"hi"`) and `^str` → `String`
//! - `(print x)` / `(println x)` → `print(x)`; `(format "n={}" n)` →
//!   `"n=" + String(n)` left-folded concat
//! - tier 2 const-fold + CSE + 1-stmt-fn inlining; tier 3 `@always_inline`
//!   on small all-primitive fns
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
        MItem::Struct { name, fields, comment } => {
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
            if fields.is_empty() {
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
    if let Some(c) = &f.comment {
        if matches!(tier, Tier::Readable | Tier::Optimized) {
            out.push_str("# cljrs: ");
            out.push_str(c);
            out.push('\n');
        }
    }
    for d in &f.decorators {
        out.push_str(d);
        out.push('\n');
    }
    out.push_str("fn ");
    out.push_str(&f.name);
    out.push('(');
    for (i, (n, t)) in f.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(n);
        if !matches!(t, MType::Infer) {
            out.push_str(": ");
            out.push_str(&t.as_str());
        }
    }
    out.push(')');
    if !matches!(f.ret, MType::Infer) {
        out.push_str(" -> ");
        out.push_str(&f.ret.as_str());
    }
    out.push_str(":\n");
    if f.body.is_empty() {
        out.push_str("    pass\n");
    } else {
        for s in &f.body {
            print_stmt(out, s, 1);
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
