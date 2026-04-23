(defn-mojo factorial ^i64 [^i64 n]
  (loop [i 1 acc 1]
    (if (> i n)
      acc
      (recur (+ i 1) (* acc i)))))
