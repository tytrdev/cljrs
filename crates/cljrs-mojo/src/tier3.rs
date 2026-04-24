//! Tier 3: SIMD / parameter specialization + comment stripping.
//!
//! `@always_inline` is applied when ALL of the following hold for a fn:
//!   - declared return and all param types are non-Infer (primitive, named,
//!     SIMD, or String all count)
//!   - body has ≤ 10 leaf statements
//!   - no self-recursion (no Call to its own name anywhere in the body)
//!   - nested control flow never exceeds depth 2 (if-inside-if-inside-if
//!     disqualifies)
//!   - no While loop (break-trampolined loops are too heavyweight to inline)
//! ForRange loops are allowed because Mojo unrolls them well at the inline
//! site when the bounds are compile-time.

use crate::ast::{MExpr, MItem, MModule, MStmt, MType};
#[allow(unused_imports)]
use crate::ast::MCatch;

pub fn specialize(m: &mut MModule) {
    // Run tier2 opts first.
    crate::tier2::optimize(m);

    for item in &mut m.items {
        match item {
            MItem::Fn(f) => {
                // Strip source comments.
                f.comment = None;
                let sized = count_leaf_stmts(&f.body) <= 10;
                let typed_sig =
                    f.params.iter().all(|(_, t, _)| !matches!(t, MType::Infer))
                        && !matches!(f.ret, MType::Infer);
                let depth_ok = max_control_depth(&f.body) <= 2;
                let non_recursive = !body_calls(&f.body, &f.name);
                let no_while = !body_has_while(&f.body);
                if sized && typed_sig && depth_ok && non_recursive && no_while
                    && !f.decorators.iter().any(|d| d.contains("always_inline"))
                {
                    f.decorators.insert(0, "@always_inline".to_string());
                }
                // If the fn is a tight loop over a Float32 range, tag it
                // with `@parameter` — placeholder hook; the printer will
                // pick this up but we don't rewrite the body yet.
                if is_simd_candidate(f) {
                    if !f.decorators.iter().any(|d| d.contains("parameter")) {
                        f.decorators.insert(0, "@parameter".to_string());
                    }
                }
            }
            MItem::Var { comment, .. } => {
                *comment = None;
            }
            MItem::Struct { comment, .. } => {
                *comment = None;
            }
            MItem::Alias { comment, .. } => {
                *comment = None;
            }
            MItem::Trait { comment, .. } => {
                *comment = None;
            }
            MItem::Elementwise { comment, .. } => {
                *comment = None;
            }
            MItem::Reduce { comment, .. } => {
                *comment = None;
            }
            MItem::GpuElementwise { comment, .. } => {
                *comment = None;
            }
        }
    }
}

fn count_leaf_stmts(body: &[MStmt]) -> usize {
    let mut n = 0;
    for s in body {
        match s {
            MStmt::If { then, els, .. } => {
                n += count_leaf_stmts(then) + count_leaf_stmts(els);
            }
            MStmt::While { body, .. } => {
                n += count_leaf_stmts(body);
            }
            MStmt::ForRange { body, .. } => {
                n += count_leaf_stmts(body);
            }
            MStmt::Try { body, catches } => {
                n += count_leaf_stmts(body);
                for c in catches {
                    n += count_leaf_stmts(&c.body);
                }
            }
            MStmt::ParameterIf { then, els, .. } => {
                n += count_leaf_stmts(then) + count_leaf_stmts(els);
            }
            _ => n += 1,
        }
    }
    n
}

fn max_control_depth(body: &[MStmt]) -> usize {
    let mut best = 0usize;
    for s in body {
        let d = match s {
            MStmt::If { then, els, .. } => 1 + max_control_depth(then).max(max_control_depth(els)),
            MStmt::While { body, .. } | MStmt::ForRange { body, .. } => 1 + max_control_depth(body),
            MStmt::Try { body, catches } => {
                let mut d = 1 + max_control_depth(body);
                for c in catches {
                    d = d.max(1 + max_control_depth(&c.body));
                }
                d
            }
            MStmt::ParameterIf { then, els, .. } => {
                1 + max_control_depth(then).max(max_control_depth(els))
            }
            _ => 0,
        };
        if d > best { best = d; }
    }
    best
}

fn body_has_while(body: &[MStmt]) -> bool {
    body.iter().any(|s| match s {
        MStmt::While { .. } => true,
        MStmt::If { then, els, .. } => body_has_while(then) || body_has_while(els),
        MStmt::ForRange { body, .. } => body_has_while(body),
        MStmt::Try { body, catches } => {
            body_has_while(body) || catches.iter().any(|c| body_has_while(&c.body))
        }
        MStmt::ParameterIf { then, els, .. } => body_has_while(then) || body_has_while(els),
        _ => false,
    })
}

fn body_calls(body: &[MStmt], name: &str) -> bool {
    body.iter().any(|s| stmt_calls(s, name))
}

fn stmt_calls(s: &MStmt, name: &str) -> bool {
    match s {
        MStmt::Let { value, .. }
        | MStmt::Assign { value, .. }
        | MStmt::Return(value)
        | MStmt::Expr(value) => expr_calls(value, name),
        MStmt::If { cond, then, els } => expr_calls(cond, name) || body_calls(then, name) || body_calls(els, name),
        MStmt::While { cond, body } => expr_calls(cond, name) || body_calls(body, name),
        MStmt::ForRange { lo, hi, body, .. } => expr_calls(lo, name) || expr_calls(hi, name) || body_calls(body, name),
        MStmt::Break | MStmt::Continue | MStmt::ReRaise | MStmt::Raw(_) => false,
        MStmt::Raise(e) => expr_calls(e, name),
        MStmt::Try { body, catches } => {
            body_calls(body, name) || catches.iter().any(|c| body_calls(&c.body, name))
        }
        MStmt::ParameterIf { cond, then, els } => {
            expr_calls(cond, name) || body_calls(then, name) || body_calls(els, name)
        }
    }
}

fn expr_calls(e: &MExpr, name: &str) -> bool {
    match e {
        MExpr::Call { callee, args } => callee == name || args.iter().any(|a| expr_calls(a, name)),
        MExpr::BinOp { lhs, rhs, .. } => expr_calls(lhs, name) || expr_calls(rhs, name),
        MExpr::UnOp { rhs, .. } => expr_calls(rhs, name),
        MExpr::IfExpr { cond, then, els } => expr_calls(cond, name) || expr_calls(then, name) || expr_calls(els, name),
        MExpr::Field { obj, .. } => expr_calls(obj, name),
        _ => false,
    }
}

fn is_simd_candidate(f: &crate::ast::MFn) -> bool {
    // Float32 throughout + contains a While (loop/recur) over the same.
    let all_f32 = f.params.iter().all(|(_, t, _)| matches!(t, MType::Float32))
        && matches!(f.ret, MType::Float32);
    if !all_f32 {
        return false;
    }
    f.body.iter().any(|s| matches!(s, MStmt::While { .. } | MStmt::ForRange { .. }))
}
