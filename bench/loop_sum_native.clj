;; Native port of loop_sum via tail recursion. Phase-2 MLIR emitter
;; doesn't do loop/recur yet (scf.while); we rely on LLVM -O3 to TCO the
;; self-tail-call. If it doesn't, 10000 frames of native stack fits but
;; isn't ideal. Same final value as the portable loop_sum.clj.
(defn-native sum-to ^i64 [^i64 n ^i64 i ^i64 acc]
  (if (> i n)
    acc
    (sum-to n (+ i 1) (+ acc i))))

(sum-to 10000 0 0)
