;; Wave interference pattern — four point sources emit circular waves;
;; the interference produces a swirling pattern of peaks and troughs.
;; Move the sources with the sliders.

(def slider-0-label "wave frequency")
(def slider-1-label "wave speed")
(def slider-2-label "source spread")
(def slider-3-label "sharpness (contrast)")

(defn-native wave ^f64 [^f64 dx ^f64 dy ^f64 t ^f64 k]
  (let [r (sqrt (+ (* dx dx) (* dy dy)))]
    (sin (- (* k r) t))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width   960.0
        height  540.0
        k       (+ 0.01 (* 0.09 (/ (float s0) 1000.0)))   ;; wave number
        speed   (+ 0.001 (* 0.009 (/ (float s1) 1000.0))) ;; ω
        spread  (+ 50.0 (* 350.0 (/ (float s2) 1000.0))) ;; source displacement
        sharp   (+ 0.5 (* 2.0 (/ (float s3) 1000.0)))    ;; contrast exponent
        t       (* speed (float t-ms))
        fx      (float px)
        fy      (float py)
        cx      480.0
        cy      270.0
        ;; Four sources arranged in a slowly rotating square.
        theta   (* 0.001 (float t-ms))
        c1 (cos theta) s1s (sin theta)
        s1x (+ cx (* spread c1))     s1y (+ cy (* spread s1s))
        s2x (- cx (* spread c1))     s2y (- cy (* spread s1s))
        s3x (+ cx (* spread s1s))    s3y (- cy (* spread c1))
        s4x (- cx (* spread s1s))    s4y (+ cy (* spread c1))
        w1 (wave (- fx s1x) (- fy s1y) t k)
        w2 (wave (- fx s2x) (- fy s2y) t k)
        w3 (wave (- fx s3x) (- fy s3y) t k)
        w4 (wave (- fx s4x) (- fy s4y) t k)
        sum (+ (+ w1 w2) (+ w3 w4))
        ;; Normalize roughly to 0..1 then apply contrast.
        u   (+ 0.5 (* 0.125 sum))
        v   (pow u sharp)
        ;; Teal-to-pink gradient based on v.
        r   (int (* 255.0 (min 1.0 (+ 0.1 v))))
        g   (int (* 255.0 (min 1.0 (* 0.4 v))))
        b   (int (* 255.0 (min 1.0 (+ 0.4 (* 0.6 (- 1.0 v))))))]
    (+ (* 65536 r) (* 256 g) b)))
