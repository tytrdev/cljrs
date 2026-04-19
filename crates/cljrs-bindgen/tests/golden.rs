//! Golden test: run the bindgen on the committed `examples/rand.toml`
//! manifest and assert that the codegen output matches the committed
//! `examples/expected/rand_install.rs` snapshot.
//!
//! Whitespace-only differences are ignored: a trailing newline mismatch
//! shouldn't blow up CI on every editor.

use std::fs;
use std::path::PathBuf;

#[test]
fn rand_manifest_matches_expected_snapshot() {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manifest_src = fs::read_to_string(here.join("examples/rand.toml"))
        .expect("read example manifest");
    let manifest = cljrs_bindgen::Manifest::from_str(&manifest_src)
        .expect("parse example manifest");
    let got = cljrs_bindgen::generate_install_rs(&manifest)
        .expect("codegen");

    let expected_path = here.join("examples/expected/rand_install.rs");
    let expected = fs::read_to_string(&expected_path)
        .expect("read expected snapshot");

    if normalize(&got) != normalize(&expected) {
        // Write the diverged output next to the expected snapshot so a
        // human can `diff` and either fix the codegen or refresh the
        // snapshot in one step.
        let dump = expected_path.with_extension("rs.actual");
        let _ = fs::write(&dump, &got);
        panic!(
            "generated output differs from snapshot.\n\
             expected: {}\nactual  : {}\n\
             (refresh by `cp {} {}` if the codegen change is intended)",
            expected_path.display(),
            dump.display(),
            dump.display(),
            expected_path.display()
        );
    }
}

fn normalize(s: &str) -> String {
    s.lines()
        .map(|l| l.trim_end())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
