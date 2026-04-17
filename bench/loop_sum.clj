;; Tight iterative loop - stresses loop/recur and arithmetic, no allocation.
(defn sum-to [n]
  (loop [i 0 acc 0]
    (if (> i n)
      acc
      (recur (+ i 1) (+ acc i)))))

(sum-to 10000)
