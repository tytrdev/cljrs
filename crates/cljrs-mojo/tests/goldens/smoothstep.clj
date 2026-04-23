(defn-mojo smoothstep ^f32 [^f32 edge0 ^f32 edge1 ^f32 x]
  (let [t (max 0.0 (min 1.0 (/ (- x edge0) (- edge1 edge0))))]
    (* t (* t (- 3.0 (* 2.0 t))))))
