;; Per-element body of a dot product — the Rust driver accumulates
;; the sum over the array. Separating the body from the reduction
;; lets us measure the per-call overhead of MLIR's JIT boundary.
(defn-native dot-elem ^f64 [^f64 x ^f64 y]
  (* x y))
