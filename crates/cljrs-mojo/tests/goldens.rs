//! Walks `tests/goldens/` and asserts `emit(src, tier)` matches the
//! corresponding `.mojo.readable`, `.mojo.optimized`, and `.mojo.max`
//! files.
//!
//! Layout (flat):
//!
//! ```text
//! tests/goldens/
//!   NAME.clj
//!   NAME.mojo.readable
//!   NAME.mojo.optimized
//!   NAME.mojo.max
//! ```
//!
//! If the repo is migrated to a per-case subdirectory layout
//! (`goldens/NAME/NAME.clj` + `goldens/NAME/NAME.mojo.<tier>`) this
//! walker also handles that for backward compatibility.
//!
//! # Env vars
//!
//! - `UPDATE_GOLDENS=1` — rewrite the `.mojo.<tier>` files in place
//!   instead of asserting. Intended for intentional, reviewed updates.
//!
//! # Missing goldens
//!
//! A `NAME.clj` with no `.mojo.<tier>` siblings is allowed and emits a
//! warning on stderr (not a failure) so adding new cases is friction-
//! free. Once even one `.mojo.<tier>` exists for the case, all three
//! tiers are expected — a missing tier in that case is a failure.

use std::path::{Path, PathBuf};

use cljrs_mojo::{emit, Tier};

const TIERS: &[(&str, Tier)] = &[
    ("readable", Tier::Readable),
    ("optimized", Tier::Optimized),
    ("max", Tier::Max),
];

fn normalize(s: &str) -> String {
    // Strip trailing whitespace per line and any trailing newlines at EOF.
    let lines: Vec<&str> = s.lines().map(|l| l.trim_end()).collect();
    let mut out = lines.join("\n");
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Render two strings side-by-side with a unified-diff-ish marker so a
/// failing golden is easy to read in CI logs.
fn render_mismatch(name: &str, tier: &str, expected: &str, got: &str) -> String {
    let exp_n = normalize(expected);
    let got_n = normalize(got);
    let exp_lines: Vec<&str> = exp_n.lines().collect();
    let got_lines: Vec<&str> = got_n.lines().collect();
    let mut out = String::new();
    out.push_str(&format!("\n--- {name}/{tier} mismatch ---\n"));
    let n = exp_lines.len().max(got_lines.len());
    for i in 0..n {
        let e = exp_lines.get(i).copied().unwrap_or("<EOF>");
        let g = got_lines.get(i).copied().unwrap_or("<EOF>");
        if e == g {
            out.push_str(&format!("   {i:>3} | {e}\n"));
        } else {
            out.push_str(&format!(" - {i:>3} | {e}\n"));
            out.push_str(&format!(" + {i:>3} | {g}\n"));
        }
    }
    out
}

/// Collect (case_name, clj_path, expected_path_for_tier) tuples by
/// scanning the goldens dir. Supports both flat and subdir layouts.
fn collect_cases(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut cases = Vec::new();
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return cases,
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Subdir layout: goldens/NAME/NAME.clj
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let clj = path.join(format!("{name}.clj"));
            if clj.exists() {
                cases.push((name, clj));
            }
        } else if path.extension().and_then(|s| s.to_str()) == Some("clj") {
            // Flat layout: goldens/NAME.clj
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            cases.push((stem, path));
        }
    }
    cases.sort_by(|a, b| a.0.cmp(&b.0));
    cases
}

#[test]
fn walk_goldens() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens");
    if !dir.exists() {
        eprintln!("goldens dir absent — skipping");
        return;
    }
    let update = std::env::var("UPDATE_GOLDENS").ok().as_deref() == Some("1");
    let cases = collect_cases(&dir);
    if cases.is_empty() {
        eprintln!("no golden cases found — skipping");
        return;
    }

    let mut failures: Vec<String> = Vec::new();
    let mut case_count = 0usize;
    let mut tier_count = 0usize;
    let mut missing_warn = 0usize;

    for (name, clj_path) in &cases {
        let src = std::fs::read_to_string(clj_path).expect("read clj");
        let base_dir = clj_path.parent().unwrap();
        // An expected file may live as NAME.mojo.<tier> or NAME.<tier>.mojo.
        let expected_path = |tier_name: &str| -> Option<PathBuf> {
            let a = base_dir.join(format!("{name}.mojo.{tier_name}"));
            if a.exists() {
                return Some(a);
            }
            let b = base_dir.join(format!("{name}.{tier_name}.mojo"));
            if b.exists() {
                return Some(b);
            }
            None
        };

        let any_tier_present = TIERS
            .iter()
            .any(|(tn, _)| expected_path(tn).is_some());
        if !any_tier_present {
            eprintln!(
                "warning: golden case {:?} has no .mojo.<tier> siblings — skipping \
                 (add one to start asserting)",
                name
            );
            missing_warn += 1;
            continue;
        }
        case_count += 1;

        for (tier_name, tier) in TIERS {
            let got = match emit(&src, *tier) {
                Ok(s) => s,
                Err(e) => {
                    failures.push(format!(
                        "{name}/{tier_name}: emit() returned error: {e}"
                    ));
                    continue;
                }
            };
            let exp_path = match expected_path(tier_name) {
                Some(p) => p,
                None => {
                    if update {
                        // Create a new golden in the flat layout.
                        let p = base_dir.join(format!("{name}.mojo.{tier_name}"));
                        std::fs::write(&p, &got).expect("write golden");
                        eprintln!("UPDATE_GOLDENS: wrote {}", p.display());
                        continue;
                    } else {
                        failures.push(format!(
                            "{name}/{tier_name}: expected file missing \
                             (case has other tiers, but not this one; \
                             re-run with UPDATE_GOLDENS=1 to create)"
                        ));
                        continue;
                    }
                }
            };

            tier_count += 1;
            let expected = std::fs::read_to_string(&exp_path).expect("read expected");
            if normalize(&got) != normalize(&expected) {
                if update {
                    std::fs::write(&exp_path, &got).expect("rewrite golden");
                    eprintln!("UPDATE_GOLDENS: updated {}", exp_path.display());
                } else {
                    failures.push(render_mismatch(name, tier_name, &expected, &got));
                }
            }
        }
    }

    eprintln!(
        "golden walker: {} cases, {} tier comparisons, {} cases missing all tiers",
        case_count, tier_count, missing_warn
    );

    if update {
        return;
    }
    if !failures.is_empty() {
        panic!(
            "{} golden comparison(s) failed:{}",
            failures.len(),
            failures.join("")
        );
    }
}
