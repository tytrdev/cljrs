//! Pins the expected `line N col M:` prefix format for transpiler
//! errors. These tests are `#[ignore]` until the patch in
//! `tools/mojo-check/error_locations.patch` lands. Once it does,
//! remove the `#[ignore]` attributes and the tests become a
//! regression fence.
//!
//! Run with:
//!
//! ```sh
//! cargo test -p cljrs-mojo --test error_locations -- --ignored
//! ```

use cljrs_mojo::{emit, Tier};

#[test]
#[ignore = "drives follow-up: apply tools/mojo-check/error_locations.patch"]
fn error_on_bad_defn_carries_line_col() {
    // Leading blank + comment so the error form is definitively on line 3.
    let src = "\n;; intentionally bad\n(defn-mojo)";
    let r = emit(src, Tier::Readable);
    let err = r.expect_err("should fail");
    assert!(
        err.starts_with("line 3 col 1:"),
        "expected 'line 3 col 1:' prefix, got: {err:?}"
    );
}

#[test]
#[ignore = "drives follow-up: apply tools/mojo-check/error_locations.patch"]
fn error_on_second_form_has_correct_line() {
    let src = "\
(defn-mojo ok ^i32 [^i32 x] x)

(defn-mojo broken)
";
    let r = emit(src, Tier::Readable);
    let err = r.expect_err("should fail");
    assert!(
        err.contains("line 3"),
        "expected 'line 3' in error (second form), got: {err:?}"
    );
}

#[test]
#[ignore = "drives follow-up: apply tools/mojo-check/error_locations.patch"]
fn unknown_top_level_form_carries_location() {
    let src = "\n\n(not-a-real-mojo-form foo)";
    let r = emit(src, Tier::Readable);
    let err = r.expect_err("should fail");
    assert!(
        err.starts_with("line 3 col 1:"),
        "expected 'line 3 col 1:' prefix, got: {err:?}"
    );
}

#[test]
fn read_error_does_not_gain_location_prefix() {
    // Unbalanced paren — read error, which already has its own message
    // format and must not be re-prefixed with a synthetic line/col.
    let src = "(defn-mojo foo";
    let r = emit(src, Tier::Readable);
    let err = r.expect_err("should fail");
    assert!(
        !err.starts_with("line "),
        "read error should not be annotated with line/col: {err:?}"
    );
}
