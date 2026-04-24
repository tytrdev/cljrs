//! Regression fence: emitted Mojo source should not contain redundant
//! `((` ... `))` wrapping — it makes the output harder to read and is
//! a cosmetic bug the tier-Max vectorize path has historically hit
//! (e.g. `((av + bv)).store(...)`).
//!
//! The exact pattern we guard against is `((EXPR))` where the inner
//! expression is a single atom (var, literal, or call) — the outer pair
//! wraps nothing. `((a - b) * c)` is not a bug: the inner `(a - b)`
//! carries precedence for the surrounding `*`, so the two opens are
//! structurally required.

use cljrs_mojo::{emit, Tier};

/// Scan a line for a redundant `(( ... ))` pair where the outer layer
/// adds no precedence or delimiting value.
///
/// Heuristic: for each `((` position whose prev char is not an identifier
/// char (so not a call or subscript), walk forward balancing parens. If
/// the matching `)` of the OUTER paren is immediately followed by the
/// matching `)` of the INNER paren (i.e. the outer closes right after
/// the inner, with nothing else inside it) → redundant.
fn has_redundant_double_parens(out: &str) -> Option<String> {
    for line in out.lines() {
        let bytes = line.as_bytes();
        for i in 0..bytes.len().saturating_sub(1) {
            if bytes[i] != b'(' || bytes[i + 1] != b'(' {
                continue;
            }
            let prev = if i == 0 { b' ' } else { bytes[i - 1] };
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                continue;
            }
            // Walk from `i+1` (the inner `(`) to find its matching close.
            let mut depth = 0i32;
            let mut inner_close = None;
            for (j, &b) in bytes.iter().enumerate().skip(i + 1) {
                match b {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            inner_close = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(inner_close) = inner_close else { continue };
            // The outer `(` must close at inner_close + 1 for the outer to
            // wrap nothing but the inner paren.
            if bytes.get(inner_close + 1) == Some(&b')') {
                return Some(line.to_string());
            }
        }
    }
    None
}

#[test]
fn tier_max_vector_add_no_double_parens() {
    let src = "(elementwise-mojo vector-add [^f32 a ^f32 b] ^f32 (+ a b))";
    let out = emit(src, Tier::Max).unwrap();
    if let Some(offender) = has_redundant_double_parens(&out) {
        panic!(
            "tier-Max output contains redundant `((`:\n  offending line: {offender}\n\nfull output:\n{out}"
        );
    }
}

#[test]
fn tier_max_reduce_no_double_parens() {
    let src = "(reduce-mojo sum-sq-diff [^f32 a ^f32 b] ^f32 (* (- a b) (- a b)) 0.0)";
    let out = emit(src, Tier::Max).unwrap();
    if let Some(offender) = has_redundant_double_parens(&out) {
        panic!(
            "tier-Max reduce output contains redundant `((`:\n  offending line: {offender}\n\nfull output:\n{out}"
        );
    }
}

#[test]
fn all_goldens_free_of_redundant_double_parens() {
    use std::path::Path;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens");
    let mut offenders: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("read goldens").flatten() {
        let p = entry.path();
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else { continue };
        if !(name.ends_with(".mojo.readable") || name.ends_with(".mojo.optimized") || name.ends_with(".mojo.max")) {
            continue;
        }
        let body = std::fs::read_to_string(&p).expect("read golden");
        if let Some(line) = has_redundant_double_parens(&body) {
            offenders.push(format!("{name}: {line}"));
        }
    }
    if !offenders.is_empty() {
        panic!(
            "{} golden file(s) contain redundant `((`:\n{}",
            offenders.len(),
            offenders.join("\n")
        );
    }
}
