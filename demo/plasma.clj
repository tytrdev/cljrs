;; Classic demoscene plasma effect.
;;
;; Sum of shifted sine waves in x, y, diagonal, and radial coordinates
;; produces a smooth swirling color field. Every term is phase-shifted
;; by time to make it flow. All the compute is in one tight defn-native;
;; rayon parallelizes the rows.
;;
;; Sliders control wave frequency, time scale, color palette offset,
;; and radial-wave weight — try moving them around.

(def slider-0-label "wave frequency")
(def slider-1-label "time scale")
(def slider-2-label "palette offset")
(def slider-3-label "radial strength")

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [;; Slider mappings — all i64 0..1000 rescaled to useful ranges.
        freq      (+ 0.005 (* 0.05 (/ (float s0) 1000.0)))
        time-rate (* 0.003 (/ (float s1) 1000.0))
        pal-off   (* 6.2831853 (/ (float s2) 1000.0))
        radial-w  (* 2.0 (/ (float s3) 1000.0))
        t         (* time-rate (float t-ms))
        fx        (float px)
        fy        (float py)
        cx        480.0
        cy        270.0
        dx        (- fx cx)
        dy        (- fy cy)
        radius    (sqrt (+ (* dx dx) (* dy dy)))
        ;; Four independent wavefronts summed.
        w1 (sin (+ (* fx freq) t))
        w2 (sin (+ (* fy freq) (* t 1.3)))
        w3 (sin (+ (* (+ fx fy) freq 0.7) (* t 0.8)))
        w4 (sin (+ (* radius freq 1.5) (* t 2.0)))
        v  (+ w1 w2 w3 (* radial-w w4))
        ;; Normalize to 0..1 (v roughly in −4..4 range).
        u  (+ 0.5 (* 0.125 v))
        phase (+ (* 6.2831853 u) pal-off)
        ;; HSV-style palette: three phased sines for R, G, B.
        r (int (* 127.0 (+ 1.0 (sin phase))))
        g (int (* 127.0 (+ 1.0 (sin (+ phase 2.094)))))
        b (int (* 127.0 (+ 1.0 (sin (+ phase 4.188)))))
        rc (max 0 (min 255 r))
        gc (max 0 (min 255 g))
        bc (max 0 (min 255 b))]
    (+ (* 65536 rc) (* 256 gc) bc)))
