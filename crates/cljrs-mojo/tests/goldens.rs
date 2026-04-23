//! Walks `tests/goldens/*/` (if present) and asserts `emit` matches the
//! `.mojo.readable`, `.mojo.optimized`, and `.mojo.max` outputs.
//!
//! If the goldens directory isn't populated yet (another agent owns it),
//! this test is a no-op — we skip rather than fail, because the internal
//! tests in `internal.rs` cover the same cases.

use std::path::Path;

use cljrs_mojo::{emit, Tier};

#[test]
fn walk_goldens() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens");
    if !dir.exists() {
        eprintln!("goldens dir absent — skipping");
        return;
    }
    let mut any_case = false;
    let mut failures: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("read goldens dir") {
        let entry = entry.expect("dir entry");
        let sub = entry.path();
        if !sub.is_dir() {
            continue;
        }
        let name = sub.file_name().unwrap().to_string_lossy().to_string();
        // Look for NAME.clj inside the directory. If the agent layout uses
        // a different convention, also try any .clj inside.
        let clj_path = sub.join(format!("{name}.clj"));
        let clj_path = if clj_path.exists() {
            clj_path
        } else if let Some(found) = std::fs::read_dir(&sub)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().and_then(|s| s.to_str()) == Some("clj"))
        {
            found
        } else {
            continue;
        };
        let src = std::fs::read_to_string(&clj_path).expect("read clj");
        any_case = true;
        for (tier_name, tier) in [
            ("readable", Tier::Readable),
            ("optimized", Tier::Optimized),
            ("max", Tier::Max),
        ] {
            let ext = format!("mojo.{tier_name}");
            // Expected file can be named NAME.mojo.<tier> or
            // NAME.<tier>.mojo — check both.
            let a = sub.join(format!("{name}.{ext}"));
            let b = sub.join(format!("{name}.{tier_name}.mojo"));
            let expected_path = if a.exists() { a } else if b.exists() { b } else { continue };
            let expected = std::fs::read_to_string(&expected_path).expect("read expected");
            let got = match emit(&src, tier) {
                Ok(s) => s,
                Err(e) => {
                    failures.push(format!("{name}/{tier_name}: emit error: {e}"));
                    continue;
                }
            };
            if got.trim() != expected.trim() {
                failures.push(format!(
                    "{name}/{tier_name} mismatch.\n--- expected ---\n{expected}\n--- got ---\n{got}"
                ));
            }
        }
    }
    if !any_case {
        eprintln!("no golden cases found — skipping");
        return;
    }
    if !failures.is_empty() {
        panic!("{} golden case(s) failed:\n{}", failures.len(), failures.join("\n\n"));
    }
}
