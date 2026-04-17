;; Julia set — same escape-time formula as Mandelbrot but with the
;; constant `c` baked in and the initial z being the pixel coordinate.
;; Different values of c produce wildly different shapes; sliders let you
;; dial in a live family of them.

(def slider-0-label "c real (-1..1)")
(def slider-1-label "c imag (-1..1)")
(def slider-2-label "zoom (0.5..3)")
(def slider-3-label "color cycle")

(defn-native julia-iter ^i64 [^f64 zr ^f64 zi ^f64 cr ^f64 ci ^i64 max-iter]
  (loop [i 0 r zr im zi]
    (if (>= i max-iter)
      max-iter
      (let [r2 (* r r)
            i2 (* im im)]
        (if (> (+ r2 i2) 4.0)
          i
          (recur (+ i 1)
                 (+ (- r2 i2) cr)
                 (+ (* 2.0 (* r im)) ci)))))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width  960.0
        height 540.0
        ;; Sliders.
        cr   (- (* 2.0 (/ (float s0) 1000.0)) 1.0)   ;; -1..1
        ci   (- (* 2.0 (/ (float s1) 1000.0)) 1.0)   ;; -1..1
        zoom (+ 0.5 (* 2.5 (/ (float s2) 1000.0)))   ;; 0.5..3
        ccyc (* 0.2 (/ (float s3) 1000.0))           ;; 0..0.2
        fx (float px)
        fy (float py)
        zr (/ (- (* 3.5 (/ fx width))  1.75) zoom)
        zi (/ (- (* 2.0 (/ fy height)) 1.0)  zoom)
        max-iter 180
        iters (julia-iter zr zi cr ci max-iter)]
    (if (= iters max-iter)
      0
      (let [t (+ (float iters) (* ccyc (float frame)))
            r (int (* 127.0 (+ 1.0 (sin (* t 0.05)))))
            g (int (* 127.0 (+ 1.0 (sin (+ (* t 0.05) 2.094)))))
            b (int (* 127.0 (+ 1.0 (sin (+ (* t 0.05) 4.188)))))
            rc (min 255 (max 0 r))
            gc (min 255 (max 0 g))
            bc (min 255 (max 0 b))]
        (+ (* 65536 rc) (* 256 gc) bc)))))
