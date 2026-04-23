(defn-mojo classify ^i32 [^i32 x]
  (cond
    (< x 0) -1
    (= x 0) 0
    :else   1))
