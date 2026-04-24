//! Regression fence: emitted Mojo source should not contain redundant
//! `((` ... `))` wrapping — it makes the output harder to read and is
//! a cosmetic bug the tier-Max vectorize path has historically hit
//! (e.g. `((av + bv)).store(...)`).
//!
//! These tests are marked `#[ignore]` so they drive a follow-up fix
//! without blocking CI. Run with:
//!
//! ```sh
//! cargo test -p cljrs-mojo --test no_double_parens -- --ignored
//! ```
//!
//! Once the printer stops emitting redundant wrappers, remove the
//! `#[ignore]` attributes so the fence stays up.

use cljrs_mojo::{emit, Tier};

/// Scan a line for any `((` that is NOT preceded by an identifier
/// character — that's the signature of a redundant expression
/// wrapper. `foo((x + 1))` is the bug; `foo(bar(x))` is fine.
fn has_redundant_double_parens(out: &str) -> Option<String> {
    for line in out.lines() {
        let bytes = line.as_bytes();
        for i in 0..bytes.len().saturating_sub(1) {
            if bytes[i] == b'(' && bytes[i + 1] == b'(' {
                let prev = if i == 0 { b' ' } else { bytes[i - 1] };
                let is_ident = prev.is_ascii_alphanumeric() || prev == b'_';
                if !is_ident {
                    return Some(line.to_string());
                }
            }
        }
    }
    None
}

#[test]
#[ignore = "drives follow-up fix in crates/cljrs-mojo/src/ printer"]
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
#[ignore = "drives follow-up fix in crates/cljrs-mojo/src/ printer"]
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
#[ignore = "drives follow-up fix in crates/cljrs-mojo/src/ printer"]
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
