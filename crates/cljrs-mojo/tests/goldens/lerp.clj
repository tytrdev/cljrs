(defn-mojo lerp ^f32 [^f32 a ^f32 b ^f32 t] (+ a (* t (- b a))))
