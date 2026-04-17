;; Mandelbrot set — auto-zooming into a deep Misiurewicz point at a
;; constant rate. Each cycle (30 s) zooms from 1× to ~e^9 ≈ 8000×,
;; then resets. Iteration cap scales with zoom so new detail keeps
;; emerging as we go deeper.
;;
;; Target point: (-0.743643887037158, 0.131825904205311)
;; One of the classic Mandelbrot "spiral center" Misiurewicz points
;; — infinitely self-similar approach, always reveals more detail.
;;
;; Sliders:
;;   s0  zoom rate           0..1000 → 0.05..0.4 per second  (auto-zoom)
;;   s1  x offset from target 0..1000 → ±1e-3  (nudge target)
;;   s2  y offset from target 0..1000 → ±1e-3  (nudge target)
;;   s3  palette hue          0..1000 → 0..TAU

(defn-gpu-pixel render
  [x y w h t-ms s0 s1 s2 s3]
  (let [;; Auto-zoom: loop every 30 seconds, zoom level = e^(rate * t)
        rate        (+ 0.05 (* 0.35 (/ (f32 s0) 1000.0)))
        t-sec       (* 0.001 (f32 t-ms))
        period      30.0
        t-cycle     (- t-sec (* period (floor (/ t-sec period))))
        zoom        (exp (* rate t-cycle))
        ;; Deep Misiurewicz point — reveals endless detail on zoom in.
        tx          -0.7436438870371587
        ty          0.1318259042053119
        ;; Slight user nudges from the sliders.
        dx          (* 1.0e-3 (- (* 2.0 (/ (f32 s1) 1000.0)) 1.0))
        dy          (* 1.0e-3 (- (* 2.0 (/ (f32 s2) 1000.0)) 1.0))
        cx          (+ tx dx)
        cy          (+ ty dy)
        shift       (* 6.2831853 (/ (f32 s3) 1000.0))
        ;; Pixel → complex plane, centered on (cx, cy), spanning ~1/zoom.
        aspect      (/ (f32 w) (f32 h))
        u           (/ (- (* 2.0 (/ (f32 x) (f32 w))) 1.0) zoom)
        v           (/ (- (* 2.0 (/ (f32 y) (f32 h))) 1.0) zoom)
        c-re        (+ cx (* u aspect))
        c-im        (+ cy v)
        ;; Iteration cap grows with zoom so deep details are rendered
        ;; faithfully. log2(zoom) = log(zoom) / 0.6931. 100 + 40*log2(z).
        iter-float  (+ 100.0 (* 40.0 (/ (log zoom) 0.6931)))
        max-iter    (min (i32 1200) (max (i32 64) (i32 iter-float)))
        ;; Escape-time loop — cardinality scales automatically.
        escape      (loop [z-re c-re
                           z-im c-im
                           it   (i32 0)]
                      (if (>= it max-iter)
                        it
                        (if (>= (+ (* z-re z-re) (* z-im z-im)) 4.0)
                          it
                          (recur (+ (- (* z-re z-re) (* z-im z-im)) c-re)
                                 (+ (* 2.0 (* z-re z-im)) c-im)
                                 (+ it (i32 1))))))
        ;; Smooth coloring: normalize by max-iter, interior = black.
        in-set      (>= escape max-iter)
        nt          (/ (f32 escape) (f32 max-iter))
        ;; Non-linear remap so bands don't stretch as iter count grows.
        cval        (sqrt nt)
        r           (if in-set 0.0 (* 0.5 (+ 1.0 (sin (+ (* cval 12.0) shift)))))
        g           (if in-set 0.0 (* 0.5 (+ 1.0 (sin (+ (* cval 12.0) (+ shift 2.1))))))
        b           (if in-set 0.0 (* 0.5 (+ 1.0 (sin (+ (* cval 12.0) (+ shift 4.2))))))
        ri          (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 r)))))
        gi          (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 g)))))
        bi          (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 b)))))]
    (bit-or (bit-or (bit-shift-left ri (u32 16)) (bit-shift-left gi (u32 8))) bi)))
