;; Burning Ship fractal — Mandelbrot variant where z is replaced by
;; |Re(z)| + |Im(z)|i before squaring. Named for the ship-like silhouette
;; at a specific zoom level (slide s0 toward the right to see it).

(def slider-0-label "pan x (-2..2)")
(def slider-1-label "pan y (-2..2)")
(def slider-2-label "zoom (0.5..8)")
(def slider-3-label "max iterations (32..256)")

(defn-native ship-iter ^i64 [^f64 cr ^f64 ci ^i64 max-iter]
  (loop [i 0 zr 0.0 zi 0.0]
    (if (>= i max-iter)
      max-iter
      (let [azr (abs zr)
            azi (abs zi)
            zr2 (* azr azr)
            zi2 (* azi azi)]
        (if (> (+ zr2 zi2) 4.0)
          i
          (recur (+ i 1)
                 (+ (- zr2 zi2) cr)
                 (+ (* 2.0 (* azr azi)) ci)))))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width  960.0
        height 540.0
        pan-x  (- (* 4.0 (/ (float s0) 1000.0)) 2.0)   ;; -2..2
        pan-y  (- (* 4.0 (/ (float s1) 1000.0)) 2.0)   ;; -2..2
        zoom   (+ 0.5 (* 7.5 (/ (float s2) 1000.0)))    ;; 0.5..8
        max-iter (+ 32 (int (* 224.0 (/ (float s3) 1000.0))))
        fx (float px)
        fy (float py)
        ;; Classic view: center around (-1.76, -0.04), negate y so ship is upright.
        cr (+ pan-x (/ (- (* 3.5 (/ fx width))  1.75)                zoom))
        ci (- (+ pan-y (/ (- (* 2.0 (/ fy height)) 1.0) zoom)))
        iters (ship-iter cr ci max-iter)]
    (if (= iters max-iter)
      0
      ;; Fiery palette: orange/red at low iters, white-hot at high iters.
      (let [u  (/ (float iters) (float max-iter))
            r  (int (* 255.0 (min 1.0 (+ 0.3 (* 1.2 u)))))
            g  (int (* 255.0 (min 1.0 (* 1.2 (* u u)))))
            b  (int (* 255.0 (min 1.0 (* 1.5 (* u u u u)))))]
        (+ (* 65536 r) (* 256 g) b)))))
