//! Tests for the expanded feature surface: raises/try, argument
//! conventions, parametric fns, aliases, @parameter if, list/optional,
//! traits, struct methods, assertions, and string helpers.

use cljrs_mojo::{emit, Tier};

fn has(s: &str, needle: &str) -> bool {
    s.contains(needle)
}

fn assert_has(out: &str, needles: &[&str]) {
    for n in needles {
        assert!(has(out, n), "missing {n:?} in:\n{out}");
    }
}

// ---------------- Feature 1: raises / try / raise ----------------

#[test]
fn raises_fn_emits_raises_keyword() {
    let src = r#"(raises-fn-mojo boom ^i32 [^i32 x] (raise (ValueError "nope")))"#;
    let out = emit(src, Tier::Readable).unwrap();
    assert_has(&out, &["fn boom(", "x: Int32", ") raises -> Int32",
                      "raise ValueError(\"nope\")"]);
}

#[test]
fn try_single_catch() {
    let src = r#"(raises-fn-mojo go ^i32 [^i32 x]
                    (try (do-stuff x)
                         (catch ValueError as e (handle e)))
                    0)"#;
    let out = emit(src, Tier::Readable).unwrap();
    assert_has(&out, &["try:", "do-stuff(x)",
                      "except ValueError as e:", "handle(e)"]);
}

#[test]
fn try_multiple_catches() {
    let src = r#"(raises-fn-mojo go ^i32 []
                    (try (work)
                         (catch ValueError v (a v))
                         (catch TypeError t (b t)))
                    0)"#;
    let out = emit(src, Tier::Readable).unwrap();
    assert_has(&out, &["except ValueError as v:", "except TypeError as t:"]);
}

#[test]
fn bare_raise_rereaises() {
    let src = r#"(raises-fn-mojo r ^i32 []
                    (try (work)
                         (catch Error e (raise)))
                    0)"#;
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("raise\n"), "re-raise should emit bare raise:\n{out}");
}

// ---------------- Feature 2: argument conventions ----------------

#[test]
fn owned_convention() {
    let src = "(defn-mojo take ^i32 [^owned ^i32 x] x)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("owned x: Int32"), "got:\n{out}");
}

#[test]
fn borrowed_convention() {
    let src = "(defn-mojo peek ^i32 [^borrowed ^i32 x] x)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("borrowed x: Int32"), "got:\n{out}");
}

#[test]
fn inout_convention() {
    let src = "(defn-mojo bump ^i32 [^inout ^i32 x] (+ x 1))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("inout x: Int32"), "got:\n{out}");
}

#[test]
fn ref_convention() {
    let src = "(defn-mojo look ^i32 [^ref ^i32 x] x)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("ref x: Int32"), "got:\n{out}");
}

// ---------------- Feature 3: parametric fns ----------------

#[test]
fn parametric_fn_emits_bracket_params() {
    let src = "(parametric-fn-mojo sum_simd [n Int] ^f32
                 [^SIMDf32x4 v] (reduce v))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("fn sum_simd[n: Int]"), "got:\n{out}");
    assert!(out.contains("-> Float32"), "got:\n{out}");
}

#[test]
fn parametric_fn_multiple_cparams() {
    let src = "(parametric-fn-mojo foo [n Int T AnyType] ^i32 [^T x] 0)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("fn foo[n: Int, T: AnyType]"), "got:\n{out}");
}

// ---------------- Feature 4: alias ----------------

#[test]
fn alias_without_type() {
    let src = "(alias-mojo NLANES 4)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("alias NLANES = 4"), "got:\n{out}");
}

#[test]
fn alias_with_type() {
    let src = "(alias-mojo ^i32 WIDTH 8)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("alias WIDTH: Int32 = 8"), "got:\n{out}");
}

// ---------------- Feature 5: @parameter if ----------------

#[test]
fn parameter_if_inside_parametric_fn() {
    let src = "(parametric-fn-mojo pick [n Int] ^i32 []
                 (parameter-if (= n 1) 42 99))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("@parameter"), "got:\n{out}");
    assert!(out.contains("if (n == 1):"), "got:\n{out}");
}

#[test]
fn parameter_if_errors_outside_parametric() {
    let src = "(defn-mojo bad ^i32 [] (parameter-if true 1 2))";
    let r = emit(src, Tier::Readable);
    assert!(r.is_err(), "should error: {:?}", r);
}

// ---------------- Feature 6: list/tuple/dict ----------------

#[test]
fn list_literal_constructor() {
    let src = "(defn-mojo mk ^i32 [] (let [xs (list 1 2 3)] 0))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("List[Int](1, 2, 3)"), "got:\n{out}");
}

#[test]
fn list_nth_indexing() {
    let src = "(defn-mojo at ^i32 [^i32 i] (nth xs i))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("xs[i]"), "got:\n{out}");
}

#[test]
fn len_call() {
    let src = "(defn-mojo n ^i32 [] (len xs))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("len(xs)"), "got:\n{out}");
}

#[test]
fn list_type_hint() {
    let src = "(defn-mojo go ^i32 [^List-f32 xs] 0)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("xs: List[Float32]"), "got:\n{out}");
}

// ---------------- Feature 7: Optional ----------------

#[test]
fn optional_type_hint() {
    let src = "(defn-mojo maybe ^Opt-f32 [^f32 x] (some x))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("-> Optional[Float32]"), "got:\n{out}");
    assert!(out.contains("Optional(x)"), "got:\n{out}");
}

#[test]
fn none_literal() {
    let src = "(defn-mojo nope ^Opt-f32 [] (none))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("return None"), "got:\n{out}");
}

#[test]
fn unwrap_method() {
    let src = "(defn-mojo use ^f32 [^Opt-f32 o] (unwrap o))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("o.value()"), "got:\n{out}");
}

// ---------------- Feature 9: traits ----------------

#[test]
fn trait_declaration() {
    let src = "(deftrait-mojo Shape (area ^f32 []) (perim ^f32 []))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("trait Shape:"), "got:\n{out}");
    assert!(out.contains("fn area(self) -> Float32: ..."), "got:\n{out}");
    assert!(out.contains("fn perim(self) -> Float32: ..."), "got:\n{out}");
}

#[test]
fn struct_impl_trait() {
    let src = "(defstruct-mojo Square :Shape [^f32 side])";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("struct Square(Shape):"), "got:\n{out}");
}

// ---------------- Feature 10: struct methods ----------------

#[test]
fn struct_method_attached() {
    let src = r#"
(defstruct-mojo Vec3 [^f32 x ^f32 y ^f32 z])
(defn-method-mojo Vec3 length ^f32 []
  (sqrt (+ (* (. self x) (. self x))
           (+ (* (. self y) (. self y))
              (* (. self z) (. self z))))))
"#;
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("struct Vec3:"), "got:\n{out}");
    assert!(out.contains("fn length(self)"), "got:\n{out}");
    assert!(out.contains("self.x"), "got:\n{out}");
    // Method should be indented inside the struct (8 spaces of body, 4 for method).
    assert!(out.contains("    fn length"), "got:\n{out}");
}

// ---------------- Feature 12: assertions ----------------

#[test]
fn mojo_assert_one_arg() {
    let src = "(defn-mojo check ^i32 [^i32 x] (mojo-assert (> x 0)) x)";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("debug_assert((x > 0), \"assertion failed\")"), "got:\n{out}");
}

#[test]
fn mojo_assert_with_message() {
    let src = r#"(defn-mojo check ^i32 [^i32 x] (mojo-assert (> x 0) "x must be positive") x)"#;
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains(r#"debug_assert((x > 0), "x must be positive")"#), "got:\n{out}");
}

// ---------------- Feature 13: string helpers ----------------

#[test]
fn str_len_to_len() {
    let src = r#"(defn-mojo n ^i32 [^str s] (str-len s))"#;
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("len(s)"), "got:\n{out}");
}

#[test]
fn str_slice_to_brackets() {
    let src = "(defn-mojo sub ^str [^str s ^i32 a ^i32 b] (str-slice s a b))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("s[a:b]"), "got:\n{out}");
}

#[test]
fn str_split_to_method() {
    let src = r#"(defn-mojo parts ^i32 [^str s] (str-split s ",") 0)"#;
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains(r#"s.split(",")"#), "got:\n{out}");
}

// ---------------- Feature 14: isinstance ----------------

#[test]
fn isinstance_mojo_lowers_to_builtin() {
    let src = "(defn-mojo chk ^bool [^Shape x] (isinstance-mojo x Square))";
    let out = emit(src, Tier::Readable).unwrap();
    assert!(out.contains("isinstance(x, Square)"), "got:\n{out}");
}
