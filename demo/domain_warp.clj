;; Domain-warped procedural noise. Layer three sin-based "noise" fields
;; at different scales; use the first two to displace (warp) the sampling
;; coordinates before evaluating the third. Recursive warping gives the
;; swirling organic look of cloud or smoke fields.

(def slider-0-label "base scale")
(def slider-1-label "warp strength")
(def slider-2-label "time speed")
(def slider-3-label "palette shift")

;; Cheap smooth pseudo-noise at (x, y). Not real Perlin — just three sin
;; waves summed, phase-shifted so the result looks unbiased.
(defn-native noise ^f64 [^f64 x ^f64 y]
  (let [a (sin (+ (* x 1.7) (* y 2.3)))
        b (sin (+ (* x 3.1) (* y 1.1)))
        c (sin (+ (* x 0.9) (* y 5.3)))]
    (* 0.333 (+ a (+ b c)))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width  960.0
        height 540.0
        scale  (+ 0.003 (* 0.02 (/ (float s0) 1000.0)))
        warp-k (+ 0.5 (* 4.0 (/ (float s1) 1000.0)))
        t-sp   (* 0.0006 (/ (float s2) 1000.0))
        hue-sh (/ (float s3) 1000.0)
        t      (* t-sp (float t-ms))
        fx     (* scale (float px))
        fy     (* scale (float py))
        ;; First warp layer: sample noise at base coords, use result as
        ;; a displacement for the next sample.
        w1x (noise (+ fx t) fy)
        w1y (noise fx (+ fy t))
        ;; Second warp: displace with w1.
        w2x (noise (+ fx (* warp-k w1x)) (+ fy (* warp-k w1y)))
        w2y (noise (+ fx (* warp-k w1y) 5.2) (+ fy (* warp-k w1x) 1.3))
        ;; Final sample — fully warped.
        v (noise (+ fx (* warp-k w2x)) (+ fy (* warp-k w2y)))
        u (+ 0.5 (* 0.5 v))
        phase (+ (* 6.2831853 hue-sh) (* 4.0 u))
        r (int (* 255.0 (+ 0.5 (* 0.5 (sin phase)))))
        g (int (* 255.0 (+ 0.5 (* 0.5 (sin (+ phase 2.094))))))
        b (int (* 255.0 (+ 0.5 (* 0.5 (sin (+ phase 4.188))))))
        rc (max 0 (min 255 r))
        gc (max 0 (min 255 g))
        bc (max 0 (min 255 b))]
    (+ (* 65536 rc) (* 256 gc) bc)))
