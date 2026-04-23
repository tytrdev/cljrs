(defn-mojo sphere-sdf ^f32 [^f32 px ^f32 py ^f32 pz ^f32 r]
  (- (sqrt (+ (* px px) (+ (* py py) (* pz pz)))) r))
