;; SAXPY — single-precision a·x+y, the canonical BLAS level-1 op.
;; We use f64 here for fair cross-compare with numpy/Rust f64 paths.
(defn-native saxpy-elem ^f64 [^f64 a ^f64 x ^f64 y]
  (+ (* a x) y))
