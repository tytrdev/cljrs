//! Tier 2: optimization passes on the Mojo AST.
//! - Constant folding on MExpr.
//! - Common-subexpression elimination within an MFn.
//! - Inlining of ≤3-stmt fns at call sites.
//! - Strip source-level `# cljrs:` comments down to just the head.
//!
//! We keep each pass small and idempotent; tier 3 runs tier 2 first.

use std::collections::HashMap;

use crate::ast::{MExpr, MFn, MItem, MModule, MStmt, MType};

pub fn optimize(m: &mut MModule) {
    // First, collect simple inlineable fn bodies — fn with a single
    // `return EXPR` stmt and no inner control flow.
    let inlineable = collect_inlineable(m);

    for item in &mut m.items {
        match item {
            MItem::Fn(f) => {
                apply_in_fn(f, &inlineable);
                // Shorten the source comment (tier 2: just the head).
                if let Some(c) = &f.comment {
                    f.comment = Some(short_comment(c));
                }
            }
            MItem::Var { value, comment, .. } => {
                *value = fold(value.clone());
                if let Some(c) = comment {
                    *comment = Some(short_comment(c));
                }
            }
            MItem::Struct { comment, .. }
            | MItem::Alias { comment, .. }
            | MItem::Trait { comment, .. } => {
                if let Some(c) = comment {
                    *comment = Some(short_comment(c));
                }
            }
            MItem::Elementwise { body, comment, .. } => {
                *body = fold(body.clone());
                if let Some(c) = comment {
                    *comment = Some(short_comment(c));
                }
            }
            MItem::Reduce { body, comment, .. } => {
                *body = fold(body.clone());
                if let Some(c) = comment {
                    *comment = Some(short_comment(c));
                }
            }
            MItem::GpuElementwise { body, comment, .. } => {
                *body = fold(body.clone());
                if let Some(c) = comment {
                    *comment = Some(short_comment(c));
                }
            }
        }
    }
}

fn apply_in_fn(f: &mut MFn, inlineable: &HashMap<String, InlineFn>) {
    // Multiple passes: inlining can expose new folding opportunities.
    for _ in 0..3 {
        let mut new_body = Vec::with_capacity(f.body.len());
        for s in f.body.drain(..) {
            new_body.push(opt_stmt(s, inlineable));
        }
        f.body = new_body;
        cse_fn(f);
    }
}

fn opt_stmt(s: MStmt, inlineable: &HashMap<String, InlineFn>) -> MStmt {
    match s {
        MStmt::Let { name, ty, value } => MStmt::Let {
            name,
            ty,
            value: fold(inline_expr(value, inlineable)),
        },
        MStmt::Assign { name, value } => MStmt::Assign {
            name,
            value: fold(inline_expr(value, inlineable)),
        },
        MStmt::Return(e) => MStmt::Return(fold(inline_expr(e, inlineable))),
        MStmt::Expr(e) => MStmt::Expr(fold(inline_expr(e, inlineable))),
        MStmt::If { cond, then, els } => {
            let cond = fold(inline_expr(cond, inlineable));
            // Known-true / known-false branch pruning: we preserve the
            // stmts but in practice the printer still works either way.
            MStmt::If {
                cond,
                then: then.into_iter().map(|s| opt_stmt(s, inlineable)).collect(),
                els: els.into_iter().map(|s| opt_stmt(s, inlineable)).collect(),
            }
        }
        MStmt::While { cond, body } => MStmt::While {
            cond: fold(inline_expr(cond, inlineable)),
            body: body.into_iter().map(|s| opt_stmt(s, inlineable)).collect(),
        },
        MStmt::Break => MStmt::Break,
        MStmt::Continue => MStmt::Continue,
        MStmt::ForRange { name, ty, lo, hi, body } => MStmt::ForRange {
            name,
            ty,
            lo: fold(inline_expr(lo, inlineable)),
            hi: fold(inline_expr(hi, inlineable)),
            body: body.into_iter().map(|s| opt_stmt(s, inlineable)).collect(),
        },
        MStmt::ForIn { name, ty, iter, body } => MStmt::ForIn {
            name,
            ty,
            iter: fold(inline_expr(iter, inlineable)),
            body: body.into_iter().map(|s| opt_stmt(s, inlineable)).collect(),
        },
        MStmt::Raise(e) => MStmt::Raise(fold(inline_expr(e, inlineable))),
        MStmt::ReRaise => MStmt::ReRaise,
        MStmt::Try { body, catches } => MStmt::Try {
            body: body.into_iter().map(|s| opt_stmt(s, inlineable)).collect(),
            catches: catches
                .into_iter()
                .map(|mut c| {
                    c.body = c.body.into_iter().map(|s| opt_stmt(s, inlineable)).collect();
                    c
                })
                .collect(),
        },
        MStmt::ParameterIf { cond, then, els } => MStmt::ParameterIf {
            cond: fold(inline_expr(cond, inlineable)),
            then: then.into_iter().map(|s| opt_stmt(s, inlineable)).collect(),
            els: els.into_iter().map(|s| opt_stmt(s, inlineable)).collect(),
        },
        MStmt::Raw(s) => MStmt::Raw(s),
    }
}

/// Constant-fold a Mojo expression. Numeric ops on two literals collapse;
/// identity ops (x+0, x*1) shrink; booleans simplify.
pub fn fold(e: MExpr) -> MExpr {
    match e {
        MExpr::BinOp { op, lhs, rhs } => {
            let l = fold(*lhs);
            let r = fold(*rhs);
            fold_binop(&op, l, r)
        }
        MExpr::UnOp { op, rhs } => {
            let r = fold(*rhs);
            match (&op[..], &r) {
                ("-", MExpr::IntLit(i)) => MExpr::IntLit(-*i),
                ("-", MExpr::FloatLit(f)) => MExpr::FloatLit(-*f),
                ("not", MExpr::BoolLit(b)) => MExpr::BoolLit(!*b),
                _ => MExpr::UnOp { op, rhs: Box::new(r) },
            }
        }
        MExpr::IfExpr { cond, then, els } => {
            let c = fold(*cond);
            let t = fold(*then);
            let e2 = fold(*els);
            match c {
                MExpr::BoolLit(true) => t,
                MExpr::BoolLit(false) => e2,
                _ => MExpr::IfExpr {
                    cond: Box::new(c),
                    then: Box::new(t),
                    els: Box::new(e2),
                },
            }
        }
        MExpr::Call { callee, args } => MExpr::Call {
            callee,
            args: args.into_iter().map(fold).collect(),
        },
        other => other,
    }
}

fn fold_binop(op: &str, l: MExpr, r: MExpr) -> MExpr {
    // both ints
    if let (MExpr::IntLit(a), MExpr::IntLit(b)) = (&l, &r) {
        let (a, b) = (*a, *b);
        if let Some(v) = match op {
            "+" => Some(a.wrapping_add(b)),
            "-" => Some(a.wrapping_sub(b)),
            "*" => Some(a.wrapping_mul(b)),
            "%" if b != 0 => Some(a.rem_euclid(b)),
            "//" if b != 0 => Some(a.div_euclid(b)),
            _ => None,
        } {
            return MExpr::IntLit(v);
        }
        if let Some(v) = match op {
            "<" => Some(a < b),
            ">" => Some(a > b),
            "<=" => Some(a <= b),
            ">=" => Some(a >= b),
            "==" => Some(a == b),
            "!=" => Some(a != b),
            _ => None,
        } {
            return MExpr::BoolLit(v);
        }
    }
    // both floats
    if let (MExpr::FloatLit(a), MExpr::FloatLit(b)) = (&l, &r) {
        let (a, b) = (*a, *b);
        if let Some(v) = match op {
            "+" => Some(a + b),
            "-" => Some(a - b),
            "*" => Some(a * b),
            "/" if b != 0.0 => Some(a / b),
            _ => None,
        } {
            return MExpr::FloatLit(v);
        }
    }
    // identities
    match (op, &l, &r) {
        ("+", MExpr::IntLit(0), _) => return r,
        ("+", _, MExpr::IntLit(0)) => return l,
        ("-", _, MExpr::IntLit(0)) => return l,
        ("*", MExpr::IntLit(1), _) => return r,
        ("*", _, MExpr::IntLit(1)) => return l,
        ("*", MExpr::IntLit(0), _) | ("*", _, MExpr::IntLit(0)) => return MExpr::IntLit(0),
        ("and", MExpr::BoolLit(true), _) => return r,
        ("and", _, MExpr::BoolLit(true)) => return l,
        ("or", MExpr::BoolLit(false), _) => return r,
        ("or", _, MExpr::BoolLit(false)) => return l,
        _ => {}
    }
    MExpr::BinOp { op: op.into(), lhs: Box::new(l), rhs: Box::new(r) }
}

/// Common-subexpression elimination within a fn body. Very small: for each
/// pair of `let`s bound to an identical RHS, redirect later uses to the
/// earlier name. Only touches top-level body stmts (not nested if/while).
fn cse_fn(f: &mut MFn) {
    let mut seen: Vec<(MExpr, String)> = Vec::new();
    let mut rename: HashMap<String, String> = HashMap::new();
    for s in &mut f.body {
        if let MStmt::Let { name, value, .. } = s {
            let v = rewrite_vars(value.clone(), &rename);
            *value = v.clone();
            if let Some((_, earlier)) = seen.iter().find(|(e, _)| exprs_equal(e, &v)) {
                rename.insert(name.clone(), earlier.clone());
            } else {
                seen.push((v, name.clone()));
            }
        } else {
            // In-place rewrite of vars
            rewrite_stmt_vars(s, &rename);
        }
    }
}

fn rewrite_stmt_vars(s: &mut MStmt, map: &HashMap<String, String>) {
    match s {
        MStmt::Let { value, .. } | MStmt::Assign { value, .. } => {
            *value = rewrite_vars(value.clone(), map);
        }
        MStmt::Return(e) | MStmt::Expr(e) => {
            *e = rewrite_vars(e.clone(), map);
        }
        MStmt::If { cond, then, els } => {
            *cond = rewrite_vars(cond.clone(), map);
            for s in then.iter_mut() {
                rewrite_stmt_vars(s, map);
            }
            for s in els.iter_mut() {
                rewrite_stmt_vars(s, map);
            }
        }
        MStmt::While { cond, body } => {
            *cond = rewrite_vars(cond.clone(), map);
            for s in body.iter_mut() {
                rewrite_stmt_vars(s, map);
            }
        }
        MStmt::Break | MStmt::Continue | MStmt::ReRaise | MStmt::Raw(_) => {}
        MStmt::ForRange { lo, hi, body, .. } => {
            *lo = rewrite_vars(lo.clone(), map);
            *hi = rewrite_vars(hi.clone(), map);
            for s in body.iter_mut() {
                rewrite_stmt_vars(s, map);
            }
        }
        MStmt::ForIn { iter, body, .. } => {
            *iter = rewrite_vars(iter.clone(), map);
            for s in body.iter_mut() {
                rewrite_stmt_vars(s, map);
            }
        }
        MStmt::Raise(e) => {
            *e = rewrite_vars(e.clone(), map);
        }
        MStmt::Try { body, catches } => {
            for s in body.iter_mut() {
                rewrite_stmt_vars(s, map);
            }
            for c in catches.iter_mut() {
                for s in c.body.iter_mut() {
                    rewrite_stmt_vars(s, map);
                }
            }
        }
        MStmt::ParameterIf { cond, then, els } => {
            *cond = rewrite_vars(cond.clone(), map);
            for s in then.iter_mut() {
                rewrite_stmt_vars(s, map);
            }
            for s in els.iter_mut() {
                rewrite_stmt_vars(s, map);
            }
        }
    }
}

fn rewrite_vars(e: MExpr, map: &HashMap<String, String>) -> MExpr {
    match e {
        MExpr::Var(n) => {
            if let Some(m) = map.get(&n) {
                MExpr::Var(m.clone())
            } else {
                MExpr::Var(n)
            }
        }
        MExpr::BinOp { op, lhs, rhs } => MExpr::BinOp {
            op,
            lhs: Box::new(rewrite_vars(*lhs, map)),
            rhs: Box::new(rewrite_vars(*rhs, map)),
        },
        MExpr::UnOp { op, rhs } => MExpr::UnOp {
            op,
            rhs: Box::new(rewrite_vars(*rhs, map)),
        },
        MExpr::Call { callee, args } => MExpr::Call {
            callee,
            args: args.into_iter().map(|a| rewrite_vars(a, map)).collect(),
        },
        MExpr::IfExpr { cond, then, els } => MExpr::IfExpr {
            cond: Box::new(rewrite_vars(*cond, map)),
            then: Box::new(rewrite_vars(*then, map)),
            els: Box::new(rewrite_vars(*els, map)),
        },
        other => other,
    }
}

fn exprs_equal(a: &MExpr, b: &MExpr) -> bool {
    // Sufficient structural equality for CSE.
    match (a, b) {
        (MExpr::IntLit(x), MExpr::IntLit(y)) => x == y,
        (MExpr::FloatLit(x), MExpr::FloatLit(y)) => x.to_bits() == y.to_bits(),
        (MExpr::BoolLit(x), MExpr::BoolLit(y)) => x == y,
        (MExpr::Var(x), MExpr::Var(y)) => x == y,
        (MExpr::BinOp { op: o1, lhs: l1, rhs: r1 }, MExpr::BinOp { op: o2, lhs: l2, rhs: r2 }) => {
            o1 == o2 && exprs_equal(l1, l2) && exprs_equal(r1, r2)
        }
        (MExpr::UnOp { op: o1, rhs: r1 }, MExpr::UnOp { op: o2, rhs: r2 }) => {
            o1 == o2 && exprs_equal(r1, r2)
        }
        (MExpr::Call { callee: c1, args: a1 }, MExpr::Call { callee: c2, args: a2 }) => {
            c1 == c2 && a1.len() == a2.len() && a1.iter().zip(a2.iter()).all(|(x, y)| exprs_equal(x, y))
        }
        _ => false,
    }
}

// ---------------- inlining ----------------

#[derive(Clone)]
struct InlineFn {
    params: Vec<String>,
    ret_expr: MExpr,
}

fn collect_inlineable(m: &MModule) -> HashMap<String, InlineFn> {
    let mut out = HashMap::new();
    for item in &m.items {
        if let MItem::Fn(f) = item {
            // Must be a single `return EXPR` (≤3 stmts allowed but only if
            // purely Lets feeding a Return — we conservatively only inline
            // the 1-stmt return case).
            if f.body.len() == 1 {
                if let MStmt::Return(e) = &f.body[0] {
                    if is_pure_expr(e) {
                        out.insert(
                            f.name.clone(),
                            InlineFn {
                                params: f.params.iter().map(|(n, _, _)| n.clone()).collect(),
                                ret_expr: e.clone(),
                            },
                        );
                    }
                }
            }
        }
    }
    out
}

fn is_pure_expr(e: &MExpr) -> bool {
    match e {
        MExpr::IntLit(_) | MExpr::FloatLit(_) | MExpr::BoolLit(_) | MExpr::Var(_) | MExpr::StrLit(_) => true,
        MExpr::Field { obj, .. } => is_pure_expr(obj),
        MExpr::BinOp { lhs, rhs, .. } => is_pure_expr(lhs) && is_pure_expr(rhs),
        MExpr::UnOp { rhs, .. } => is_pure_expr(rhs),
        MExpr::IfExpr { cond, then, els } => {
            is_pure_expr(cond) && is_pure_expr(then) && is_pure_expr(els)
        }
        // Math calls are side-effect free and safe to duplicate.
        MExpr::Call { .. } => true,
    }
}

fn inline_expr(e: MExpr, fns: &HashMap<String, InlineFn>) -> MExpr {
    match e {
        MExpr::Call { callee, args } => {
            let args: Vec<_> = args.into_iter().map(|a| inline_expr(a, fns)).collect();
            if let Some(ifn) = fns.get(&callee) {
                if ifn.params.len() == args.len() {
                    let mut subst = HashMap::new();
                    for (p, a) in ifn.params.iter().zip(args.iter()) {
                        subst.insert(p.clone(), a.clone());
                    }
                    return substitute(ifn.ret_expr.clone(), &subst);
                }
            }
            MExpr::Call { callee, args }
        }
        MExpr::BinOp { op, lhs, rhs } => MExpr::BinOp {
            op,
            lhs: Box::new(inline_expr(*lhs, fns)),
            rhs: Box::new(inline_expr(*rhs, fns)),
        },
        MExpr::UnOp { op, rhs } => MExpr::UnOp {
            op,
            rhs: Box::new(inline_expr(*rhs, fns)),
        },
        MExpr::IfExpr { cond, then, els } => MExpr::IfExpr {
            cond: Box::new(inline_expr(*cond, fns)),
            then: Box::new(inline_expr(*then, fns)),
            els: Box::new(inline_expr(*els, fns)),
        },
        other => other,
    }
}

fn substitute(e: MExpr, map: &HashMap<String, MExpr>) -> MExpr {
    match e {
        MExpr::Var(n) => map.get(&n).cloned().unwrap_or(MExpr::Var(n)),
        MExpr::BinOp { op, lhs, rhs } => MExpr::BinOp {
            op,
            lhs: Box::new(substitute(*lhs, map)),
            rhs: Box::new(substitute(*rhs, map)),
        },
        MExpr::UnOp { op, rhs } => MExpr::UnOp {
            op,
            rhs: Box::new(substitute(*rhs, map)),
        },
        MExpr::Call { callee, args } => MExpr::Call {
            callee,
            args: args.into_iter().map(|a| substitute(a, map)).collect(),
        },
        MExpr::IfExpr { cond, then, els } => MExpr::IfExpr {
            cond: Box::new(substitute(*cond, map)),
            then: Box::new(substitute(*then, map)),
            els: Box::new(substitute(*els, map)),
        },
        other => other,
    }
}

/// Propagate type hints from a fn's params into inferred `Let`s where the
/// RHS is a single-Var lookup of a typed param. Minimal pass.
#[allow(dead_code)]
fn propagate_types(f: &mut MFn) {
    let mut env: HashMap<String, MType> = HashMap::new();
    for (n, t, _) in &f.params {
        env.insert(n.clone(), t.clone());
    }
    for s in &mut f.body {
        if let MStmt::Let { name, ty, value } = s {
            if matches!(ty, MType::Infer) {
                if let MExpr::Var(n) = value {
                    if let Some(t) = env.get(n) {
                        *ty = t.clone();
                    }
                }
            }
            env.insert(name.clone(), ty.clone());
        }
    }
}

fn short_comment(c: &str) -> String {
    // Keep first line only; collapse whitespace. Used for tier 2.
    let first = c.lines().next().unwrap_or("").trim();
    // Trim to 60 chars-ish.
    if first.len() > 60 {
        format!("{}...", &first[..60])
    } else {
        first.to_string()
    }
}
