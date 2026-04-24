;; Per-element body of sum-of-squares (L2 norm squared).
(defn-native sumsq-elem ^f64 [^f64 x]
  (* x x))
