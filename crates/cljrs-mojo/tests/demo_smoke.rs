//! Smoke test for the tour example embedded in docs/clojo.html. If this
//! snippet stops emitting valid-looking Mojo, the deployed playground
//! will break — so we keep it pinned here.

use cljrs_mojo::{emit, Tier};

const TOUR: &str = r#"
(defn-mojo smoothstep ^f32
  [^f32 edge0 ^f32 edge1 ^f32 x]
  (let [t (max 0.0 (min 1.0 (/ (- x edge0) (- edge1 edge0))))]
    (* t (* t (- 3.0 (* 2.0 t))))))

(defn-mojo mix-shape ^bf16 [^bf16 x ^u32 n]
  (+ x (tanh (atan2 x (log2 (+ 1.0 x))))))

(defstruct-mojo Vec3 [^f32 x ^f32 y ^f32 z])

(defn-mojo length ^f32 [^Vec3 v]
  (sqrt (+ (* (. v x) (. v x))
        (+ (* (. v y) (. v y))
           (* (. v z) (. v z))))))

(defn-mojo first-hit ^i32 [^i32 n]
  (for-mojo [i 0 n]
    (if (hit? i) (break)))
  0)

(defn-mojo classify ^i32 [^f32 x]
  (cond (< x 0.0) -1
        (= x 0.0)  0
        (< x 1.0)  1
        :else      2))

(defn-mojo greet ^str
  ([^str name] (format "hi {}" name))
  ([^str name ^i32 age] (format "hi {} (age {})" name age)))

(always-inline-fn-mojo sq ^f32 [^f32 x] (* x x))
(parameter-fn-mojo specialized ^i32 [^i32 n] n)

(alias-mojo ^i32 NLANES 4)

(parametric-fn-mojo pick [n Int] ^i32 []
  (parameter-if (= n 1) 42 99))

(raises-fn-mojo safe-div ^f32 [^f32 a ^f32 b]
  (try (if (= b 0.0) (raise (ValueError "div by zero")) (/ a b))
       (catch ValueError e (raise))))

(defn-method-mojo Vec3 length-method ^f32 []
  (sqrt (+ (* (. self x) (. self x))
        (+ (* (. self y) (. self y))
           (* (. self z) (. self z))))))

(deftrait-mojo Shape (area ^f32 []))
(defstruct-mojo Square :Shape [^f32 side])

(defn-mojo bump ^i32 [^inout ^i32 x] (+ x 1))

(defn-mojo triple ^i32 [] (let [xs (list 10 20 30)] (nth xs 2)))

(defn-mojo maybe-pos ^Opt-f32 [^f32 x]
  (if (> x 0.0) (some x) (none)))
"#;

#[test]
fn tour_readable() {
    let out = emit(TOUR, Tier::Readable).expect("readable emit");
    for needle in [
        "fn smoothstep(",
        "BFloat16",
        "@value",
        "struct Vec3:",
        "v.x",
        "for i in range(0, n):",
        "break",
        "elif ",
        "fn greet(",
        "fn greet_2(",
        "@always_inline",
        "@parameter",
        "alias NLANES",
        "fn pick[n: Int]",
        ") raises -> Float32",
        "raise ValueError",
        "try:",
        "except ValueError as e",
        "trait Shape:",
        "struct Square(Shape)",
        "inout x: Int32",
        "List[Int](10, 20, 30)",
        "Optional(x)",
        "fn length_method(self)",
    ] {
        assert!(out.contains(needle), "missing {needle} in:\n{out}");
    }
}

#[test]
fn tour_optimized() {
    let out = emit(TOUR, Tier::Optimized).expect("optimized emit");
    assert!(out.contains("fn smoothstep("));
}

#[test]
fn tour_max() {
    let out = emit(TOUR, Tier::Max).expect("max emit");
    assert!(!out.contains("# cljrs:"), "max strips comments");
    assert!(out.contains("@always_inline"));
}
