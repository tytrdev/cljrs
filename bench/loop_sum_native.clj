;; Idiomatic loop/recur compiles natively via a helper fn in the module.
;; Phase 1a turned this into a real defn-native without hand-porting.
(defn-native sum-to ^i64 [^i64 n]
  (loop [i 0 acc 0]
    (if (> i n) acc (recur (+ i 1) (+ acc i)))))

(sum-to 10000)
