;; Per-element kernel: a single f64 addition.
;; The bench driver compiles this via MLIR (defn-native), extracts the
;; code pointer, and invokes it in a tight Rust loop over two N-element
;; Vec<f64>s. That measures how fast MLIR-JIT'd cljrs scalar code is
;; when driven by a native outer loop — comparable to numpy's inner
;; BLAS routines when the only "native" part is the per-element op.
(defn-native vadd-elem ^f64 [^f64 a ^f64 b]
  (+ a b))
