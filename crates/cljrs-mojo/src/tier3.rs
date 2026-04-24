//! Tier 3: SIMD / parameter specialization + comment stripping.
//!
//! This pass is conservative: it only adds `@always_inline` to small fns
//! (≤3 stmts, all-float signature) and strips comments. The full
//! `@parameter fn[nelts]` + `SIMD[DType.float32, nelts]` rewrite is
//! deferred behind a detection hook — we keep the infrastructure so the
//! printer can emit decorators for the simple case.

use crate::ast::{MItem, MModule, MStmt, MType};

pub fn specialize(m: &mut MModule) {
    // Run tier2 opts first.
    crate::tier2::optimize(m);

    for item in &mut m.items {
        match item {
            MItem::Fn(f) => {
                // Strip source comments.
                f.comment = None;
                // `@always_inline` on small, all-primitive-typed fns.
                let small = count_leaf_stmts(&f.body) <= 3;
                let primitive_sig =
                    f.params.iter().all(|(_, t)| !matches!(t, MType::Infer))
                        && !matches!(f.ret, MType::Infer);
                if small && primitive_sig && !f.decorators.iter().any(|d| d.contains("always_inline")) {
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
            _ => n += 1,
        }
    }
    n
}

fn is_simd_candidate(f: &crate::ast::MFn) -> bool {
    // Float32 throughout + contains a While (loop/recur) over the same.
    let all_f32 = f.params.iter().all(|(_, t)| matches!(t, MType::Float32))
        && matches!(f.ret, MType::Float32);
    if !all_f32 {
        return false;
    }
    f.body.iter().any(|s| matches!(s, MStmt::While { .. } | MStmt::ForRange { .. }))
}
