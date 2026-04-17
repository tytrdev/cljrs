;; Moiré interference — two periodic line grids rotate against each other.
;; Where their peaks align you see bright bands; where they cancel you see
;; dark ones. Slight rotation of one relative to the other is what makes
;; moiré patterns hypnotic.

(def slider-0-label "grid spacing")
(def slider-1-label "relative rotation")
(def slider-2-label "contrast")
(def slider-3-label "color palette shift")

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [freq  (+ 0.05 (* 0.3 (/ (float s0) 1000.0)))
        rel   (- (* 0.3 (/ (float s1) 1000.0)) 0.15)       ;; -0.15..0.15 rad
        contrast (+ 0.5 (* 2.0 (/ (float s2) 1000.0)))
        hue   (/ (float s3) 1000.0)
        t     (* 0.0002 (float t-ms))
        cx 480.0 cy 270.0
        fx (- (float px) cx)
        fy (- (float py) cy)
        ;; Grid A: rotating slowly.
        a  t
        ca (cos a) sa (sin a)
        ua (+ (* fx ca) (* fy sa))
        va (- (* fy ca) (* fx sa))
        ;; Grid B: grid A plus a small relative rotation.
        b  (+ a rel)
        cb (cos b) sb (sin b)
        ub (+ (* fx cb) (* fy sb))
        ;; Two line grids (cosine stripes); sum + raise to power for contrast.
        sa1 (cos (* ua freq))
        sb1 (cos (* ub freq))
        sum (* 0.5 (+ sa1 sb1))
        v   (pow (max 0.0 sum) contrast)
        phase (+ (* 6.2831853 hue) (* 2.0 v))
        r (int (* 255.0 (* v (+ 0.5 (* 0.5 (sin phase))))))
        g (int (* 255.0 (* v (+ 0.5 (* 0.5 (sin (+ phase 2.094)))))))
        b-col (int (* 255.0 (* v (+ 0.5 (* 0.5 (sin (+ phase 4.188)))))))
        rc (max 0 (min 255 r))
        gc (max 0 (min 255 g))
        bc (max 0 (min 255 b-col))]
    (+ (* 65536 rc) (* 256 gc) bc)))
