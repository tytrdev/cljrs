;; Electric field visualization. Four point charges are placed around
;; the scene; for every pixel we sum the 2D electric field E = Σ q_i *
;; (p - p_i) / |p - p_i|³. Color is log-compressed magnitude with a hue
;; chosen by the field's angle — you get the dipole/quadrupole pattern
;; you'd see in a textbook.

(def slider-0-label "charge magnitude")
(def slider-1-label "charge separation")
(def slider-2-label "rotation speed")
(def slider-3-label "log compression")

(defn-native field-contrib-x ^f64 [^f64 px ^f64 py ^f64 cx ^f64 cy ^f64 q]
  (let [dx (- px cx) dy (- py cy)
        r2 (+ (* dx dx) (* dy dy))
        r  (sqrt r2)
        ;; Avoid divide-by-zero singularity right at the charge.
        r3 (* r (max r2 1.0))]
    (/ (* q dx) r3)))

(defn-native field-contrib-y ^f64 [^f64 px ^f64 py ^f64 cx ^f64 cy ^f64 q]
  (let [dx (- px cx) dy (- py cy)
        r2 (+ (* dx dx) (* dy dy))
        r  (sqrt r2)
        r3 (* r (max r2 1.0))]
    (/ (* q dy) r3)))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width  960.0
        height 540.0
        q-mag  (* 500.0 (+ 0.2 (/ (float s0) 1000.0)))
        sep    (+ 40.0 (* 300.0 (/ (float s1) 1000.0)))
        rot-sp (* 0.002 (/ (float s2) 1000.0))
        log-k  (+ 0.5 (* 10.0 (/ (float s3) 1000.0)))
        time   (* rot-sp (float t-ms))
        cx     480.0
        cy     270.0
        fx     (float px)
        fy     (float py)
        ;; Four charges: two positive, two negative, arranged on a slowly
        ;; rotating square.
        a1 time
        a2 (+ time 1.5707963)
        a3 (+ time 3.1415926)
        a4 (+ time 4.7123889)
        q1x (+ cx (* sep (cos a1))) q1y (+ cy (* sep (sin a1)))
        q2x (+ cx (* sep (cos a2))) q2y (+ cy (* sep (sin a2)))
        q3x (+ cx (* sep (cos a3))) q3y (+ cy (* sep (sin a3)))
        q4x (+ cx (* sep (cos a4))) q4y (+ cy (* sep (sin a4)))
        ex (+ (+ (field-contrib-x fx fy q1x q1y q-mag)
                  (field-contrib-x fx fy q2x q2y (- q-mag)))
               (+ (field-contrib-x fx fy q3x q3y q-mag)
                  (field-contrib-x fx fy q4x q4y (- q-mag))))
        ey (+ (+ (field-contrib-y fx fy q1x q1y q-mag)
                  (field-contrib-y fx fy q2x q2y (- q-mag)))
               (+ (field-contrib-y fx fy q3x q3y q-mag)
                  (field-contrib-y fx fy q4x q4y (- q-mag))))
        mag (sqrt (+ (* ex ex) (* ey ey)))
        ;; Log compression so the singularities don't blow the dynamic range.
        lm (log (+ 1.0 (* log-k mag)))
        bright (max 0.0 (min 1.0 (* 0.15 lm)))
        ;; Hue from field angle.
        theta (+ 3.1415926 0.0) ;; placeholder — we don't have atan2
        ;; Approximate angle with ex/(ex+ey) ratio for palette variety.
        hue-t (+ 0.5 (* 0.5 (/ ex (+ 0.001 (+ (abs ex) (abs ey))))))
        r (int (* 255.0 bright (+ 0.5 (* 0.5 (sin (+ (* 6.2831853 hue-t) 0.0))))))
        g (int (* 255.0 bright (+ 0.5 (* 0.5 (sin (+ (* 6.2831853 hue-t) 2.094))))))
        b (int (* 255.0 bright (+ 0.5 (* 0.5 (sin (+ (* 6.2831853 hue-t) 4.188))))))
        rc (max 0 (min 255 r))
        gc (max 0 (min 255 g))
        bc (max 0 (min 255 b))]
    (+ (* 65536 rc) (* 256 gc) bc)))
