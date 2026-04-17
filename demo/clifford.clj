;; Clifford attractor — iterated 2D map
;;
;;   x' = sin(a·y) + c·cos(a·x)
;;   y' = sin(b·x) + d·cos(b·y)
;;
;; For each pixel, interpret (px, py) as an initial condition, iterate N
;; steps, and color by the final position's polar angle. Nearby initial
;; conditions can end up in wildly different places — the image reveals
;; the attractor's basin structure.

(def slider-0-label "parameter a")
(def slider-1-label "parameter b")
(def slider-2-label "parameter c")
(def slider-3-label "iterations (10..120)")

(defn-native iter-x ^f64 [^f64 x ^f64 y ^f64 a ^f64 c]
  (+ (sin (* a y)) (* c (cos (* a x)))))

(defn-native iter-y ^f64 [^f64 x ^f64 y ^f64 b ^f64 d]
  (+ (sin (* b x)) (* d (cos (* b y)))))

(defn-native simulate-angle ^f64
  [^f64 x0 ^f64 y0 ^f64 a ^f64 b ^f64 c ^f64 d ^i64 steps]
  (loop [i 0 x x0 y y0]
    (if (>= i steps)
      ;; atan2 not native — synthesize angle via the x-to-|x|+|y| ratio.
      (/ x (+ 1.0 (+ (abs x) (abs y))))
      (recur (+ i 1) (iter-x x y a c) (iter-y x y b d)))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width  960.0
        height 540.0
        a      (+ 1.0 (* 2.5 (/ (float s0) 1000.0)))   ;; 1..3.5
        b      (+ 1.0 (* 2.5 (/ (float s1) 1000.0)))
        c-par  (+ -2.0 (* 4.0 (/ (float s2) 1000.0))) ;; -2..2
        steps  (+ 10 (int (* 110.0 (/ (float s3) 1000.0))))
        d-par  (* 1.2 (sin (* 0.0007 (float t-ms))))   ;; slight time drift
        fx (float px)
        fy (float py)
        x0 (* 3.0 (- (/ fx width)  0.5))
        y0 (* 3.0 (- (/ fy height) 0.5))
        final-ratio (simulate-angle x0 y0 a b c-par d-par steps)
        u (+ 0.5 (* 0.5 final-ratio))
        phase (* 6.2831853 u)
        r (int (* 255.0 (+ 0.5 (* 0.5 (sin phase)))))
        g (int (* 255.0 (+ 0.5 (* 0.5 (sin (+ phase 2.094))))))
        b-col (int (* 255.0 (+ 0.5 (* 0.5 (sin (+ phase 4.188))))))
        rc (max 0 (min 255 r))
        gc (max 0 (min 255 g))
        bc (max 0 (min 255 b-col))]
    (+ (* 65536 rc) (* 256 gc) bc)))
