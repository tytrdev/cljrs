;; Double-pendulum chaos diagram. Each pixel is an initial-condition
;; (θ1, θ2) for a double pendulum at rest. We integrate the Lagrangian
;; equations of motion for N steps and color the pixel by the final θ1.
;; The result shows the butterfly-effect boundary between stable and
;; chaotic starting angles — a famously intricate fractal pattern.

(def slider-0-label "simulation steps (50..600)")
(def slider-1-label "dt (×1e-3)")
(def slider-2-label "hue cycle")
(def slider-3-label "angle range (×)")

;; One Euler step of the double-pendulum equations. Returns new θ1.
;; Masses and lengths are 1.0 for simplicity; gravity = 9.81.
(defn-native dp-theta1 ^f64
  [^f64 t1 ^f64 t2 ^f64 w1 ^f64 w2 ^f64 dt]
  (+ t1 (* dt w1)))

(defn-native dp-theta2 ^f64
  [^f64 t1 ^f64 t2 ^f64 w1 ^f64 w2 ^f64 dt]
  (+ t2 (* dt w2)))

(defn-native dp-omega1 ^f64
  [^f64 t1 ^f64 t2 ^f64 w1 ^f64 w2 ^f64 dt]
  (let [g 9.81
        diff (- t1 t2)
        s12 (sin diff)
        c12 (cos diff)
        denom (- 2.0 (* c12 c12))
        num (- (- (- (* -3.0 (* g (sin t1))) (* g (sin (- t1 (* 2.0 t2)))))
                  (* 2.0 (* s12 (+ (* w2 w2) (* (* w1 w1) c12)))))
               0.0)
        a1 (/ num denom)]
    (+ w1 (* dt a1))))

(defn-native dp-omega2 ^f64
  [^f64 t1 ^f64 t2 ^f64 w1 ^f64 w2 ^f64 dt]
  (let [g 9.81
        diff (- t1 t2)
        s12 (sin diff)
        c12 (cos diff)
        denom (- 2.0 (* c12 c12))
        num (* 2.0 (* s12 (+ (* 2.0 (* w1 w1))
                             (+ (* 2.0 (* g (cos t1)))
                                (* (* w2 w2) c12)))))
        a2 (/ num denom)]
    (+ w2 (* dt a2))))

(defn-native simulate ^f64
  [^f64 t1-0 ^f64 t2-0 ^i64 steps ^f64 dt]
  (loop [i 0 t1 t1-0 t2 t2-0 w1 0.0 w2 0.0]
    (if (>= i steps)
      t1
      (let [nt1 (dp-theta1 t1 t2 w1 w2 dt)
            nt2 (dp-theta2 t1 t2 w1 w2 dt)
            nw1 (dp-omega1 t1 t2 w1 w2 dt)
            nw2 (dp-omega2 t1 t2 w1 w2 dt)]
        (recur (+ i 1) nt1 nt2 nw1 nw2)))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width  960.0
        height 540.0
        steps  (+ 50 (int (* 550.0 (/ (float s0) 1000.0))))
        dt     (* 0.001 (+ 1.0 (* 10.0 (/ (float s1) 1000.0))))
        hue-sp (/ (float s2) 1000.0)
        arng   (+ 1.0 (* 4.0 (/ (float s3) 1000.0)))
        fx (float px)
        fy (float py)
        ;; Map pixel to (θ1, θ2) with ±π * arng range, then sweep the
        ;; whole grid through a slow rotation + offset so the basin
        ;; structure animates visibly rather than just shimmering.
        t   (* 0.0008 (float t-ms))
        ux  (* arng (* 3.1415926 (- (/ (* 2.0 fx) width)  1.0)))
        vy  (* arng (* 3.1415926 (- (/ (* 2.0 fy) height) 1.0)))
        co  (cos (* 0.5 t))
        si  (sin (* 0.5 t))
        t1-0 (+ (- (* co ux) (* si vy)) (* 0.5 (sin (* 0.7 t))))
        t2-0 (+ (+ (* si ux) (* co vy)) (* 0.5 (cos (* 0.9 t))))
        final (simulate t1-0 t2-0 steps dt)
        ;; Wrap final angle to [0, 2π].
        wrapped (- final (* 6.2831853 (float (int (/ final 6.2831853)))))
        norm (/ wrapped 6.2831853)
        hue (+ (* 6.2831853 hue-sp) (* 6.2831853 norm))
        r (int (* 255.0 (+ 0.5 (* 0.5 (sin hue)))))
        g (int (* 255.0 (+ 0.5 (* 0.5 (sin (+ hue 2.094))))))
        b (int (* 255.0 (+ 0.5 (* 0.5 (sin (+ hue 4.188))))))
        rc (max 0 (min 255 r))
        gc (max 0 (min 255 g))
        bc (max 0 (min 255 b))]
    (+ (* 65536 rc) (* 256 gc) bc)))
