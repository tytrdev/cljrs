;; Lorenz-system chaos visualization — not the usual trajectory plot (that
;; requires persistent state we don't model per-pixel). Instead: treat
;; each pixel's (x, y) as an initial condition (x0, y0, z0=1.0) and
;; simulate N Euler steps of the Lorenz system. Color each pixel by its
;; final state, which encodes how the chaotic dynamics fan out across
;; the initial-condition plane.
;;
;; As `time` advances (via the frame counter), the simulation step count
;; grows, so you watch the structure of the attractor basin emerge.

(def slider-0-label "simulation steps (20..400)")
(def slider-1-label "time-step size (×1e-3)")
(def slider-2-label "initial z")
(def slider-3-label "coord scale (×)")

;; One Euler step: returns new x after applying dx = sigma*(y - x) * dt.
;; cljrs native can only return one value, so three separate steppers.
(defn-native next-x ^f64 [^f64 x ^f64 y ^f64 dt]
  (+ x (* dt (* 10.0 (- y x)))))

(defn-native next-y ^f64 [^f64 x ^f64 y ^f64 z ^f64 dt]
  (+ y (* dt (- (* x (- 28.0 z)) y))))

(defn-native next-z ^f64 [^f64 x ^f64 y ^f64 z ^f64 dt]
  (+ z (* dt (- (* x y) (* 2.6666667 z)))))

(defn-native simulate ^f64
  [^f64 x0 ^f64 y0 ^f64 z0 ^i64 steps ^f64 dt]
  (loop [i 0 x x0 y y0 z z0]
    (if (>= i steps)
      z
      (recur (+ i 1)
             (next-x x y dt)
             (next-y x y z dt)
             (next-z x y z dt)))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width  960.0
        height 540.0
        steps  (+ 20 (int (* 380.0 (/ (float s0) 1000.0))))
        dt     (* 0.001 (+ 1.0 (* 20.0 (/ (float s1) 1000.0))))
        z0     (+ -20.0 (* 40.0 (/ (float s2) 1000.0)))
        scale  (+ 0.5 (* 3.5 (/ (float s3) 1000.0)))
        fx (float px)
        fy (float py)
        ;; Map pixel to initial condition (x0, y0) centered on 0.
        x0 (* scale (- (/ fx width)  0.5))
        y0 (* scale (- (/ fy height) 0.5))
        ;; Slowly vary with time so the image animates.
        t  (* 0.01 (float frame))
        final-z (simulate (+ x0 (* 0.02 (sin t))) y0 z0 steps dt)
        ;; Map final z to color. Lorenz z typically wanders 0..50.
        u (+ 0.5 (* 0.02 final-z))
        ;; Clamp and palette-map.
        uc (max 0.0 (min 1.0 u))
        r (int (* 255.0 (+ 0.5 (* 0.5 (sin (* 6.2831853 uc))))))
        g (int (* 255.0 (+ 0.5 (* 0.5 (sin (+ (* 6.2831853 uc) 2.094))))))
        b (int (* 255.0 (+ 0.5 (* 0.5 (sin (+ (* 6.2831853 uc) 4.188))))))
        rc (min 255 (max 0 r))
        gc (min 255 (max 0 g))
        bc (min 255 (max 0 b))]
    (+ (* 65536 rc) (* 256 gc) bc)))
