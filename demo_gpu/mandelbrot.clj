;; Auto-zooming Mandelbrot into a deep Misiurewicz point.
;; Stabilized vs naive per-pixel by:
;;   1. 2×2 supersampling (4 samples per pixel, averaged)
;;   2. Smooth escape-time coloring (continuous, no integer banding)
;;   3. Iteration cap grows with log2(zoom) so detail keeps emerging
;;
;; Target: (-0.7436438870371587, 0.1318259042053119)  — Misiurewicz M23
;;
;; Sliders:
;;   s0  zoom rate            0..1000 → 0.05..0.35 per second
;;   s1  x nudge (×1e-3)      0..1000 → ±1
;;   s2  y nudge (×1e-3)      0..1000 → ±1
;;   s3  palette hue          0..1000 → 0..TAU

(defn-gpu-pixel render
  [x y w h t-ms s0 s1 s2 s3]
  (let [rate        (+ 0.05 (* 0.3 (/ (f32 s0) 1000.0)))
        t-sec       (* 0.001 (f32 t-ms))
        period      30.0
        t-cycle     (- t-sec (* period (floor (/ t-sec period))))
        zoom        (exp (* rate t-cycle))
        tx          -0.7436438870371587
        ty          0.1318259042053119
        dx          (* 1.0e-3 (- (* 2.0 (/ (f32 s1) 1000.0)) 1.0))
        dy          (* 1.0e-3 (- (* 2.0 (/ (f32 s2) 1000.0)) 1.0))
        cx          (+ tx dx)
        cy          (+ ty dy)
        shift       (* 6.2831853 (/ (f32 s3) 1000.0))
        aspect      (/ (f32 w) (f32 h))
        fw          (f32 w)
        fh          (f32 h)
        ;; Iteration cap — floor(100 + 40*log2(zoom)), capped at 1200.
        iter-float  (+ 100.0 (* 40.0 (/ (log zoom) 0.6931)))
        max-iter    (min (i32 2000) (max (i32 64) (i32 iter-float)))
        max-iter-f  (f32 max-iter)
        ;; Four sub-pixel offsets (2×2 grid, in 0..1 pixel space).
        ;; Step size in complex-plane coordinates per fractional pixel:
        ;;   d = 2 / (fw * zoom)  (one full pixel width)
        px-size     (/ 2.0 (* fw zoom))
        ;; Same for Y (but we handled aspect via x). Use fh for vertical.
        py-size     (/ 2.0 (* fh zoom))
        ;; Center of pixel in complex plane.
        u0          (/ (- (* 2.0 (/ (f32 x) fw)) 1.0) zoom)
        v0          (/ (- (* 2.0 (/ (f32 y) fh)) 1.0) zoom)
        base-re     (+ cx (* u0 aspect))
        base-im     (+ cy v0)
        ;; Four offsets: (-0.25, -0.25), (+0.25, -0.25), (-0.25, +0.25), (+0.25, +0.25)
        q           (* 0.25 aspect)
        ;; Per-sample smooth-escape value. Stores it + 1 - log2(log2(|z|²)).
        ;; For `in-set` pixels we use max-iter-f (black via palette).
        s1v (let [c-re (+ base-re (* px-size (- q)))
                  c-im (+ base-im (* py-size -0.25))]
              (loop [z-re c-re z-im c-im it (i32 0)]
                (if (>= it max-iter) max-iter-f
                  (let [r2 (+ (* z-re z-re) (* z-im z-im))]
                    (if (>= r2 4.0)
                      ;; Smooth escape count: it + 1 - log2(0.5 * log2(r2))
                      ;; Approximation avoiding log2() repeat: it + 1 - log(r2)/(2 log 2)
                      (+ (f32 it) (- 1.0 (/ (log (log r2)) 0.6931)))
                      (recur (+ (- (* z-re z-re) (* z-im z-im)) c-re)
                             (+ (* 2.0 (* z-re z-im)) c-im)
                             (+ it (i32 1))))))))
        s2v (let [c-re (+ base-re (* px-size q))
                  c-im (+ base-im (* py-size -0.25))]
              (loop [z-re c-re z-im c-im it (i32 0)]
                (if (>= it max-iter) max-iter-f
                  (let [r2 (+ (* z-re z-re) (* z-im z-im))]
                    (if (>= r2 4.0)
                      (+ (f32 it) (- 1.0 (/ (log (log r2)) 0.6931)))
                      (recur (+ (- (* z-re z-re) (* z-im z-im)) c-re)
                             (+ (* 2.0 (* z-re z-im)) c-im)
                             (+ it (i32 1))))))))
        s3v (let [c-re (+ base-re (* px-size (- q)))
                  c-im (+ base-im (* py-size 0.25))]
              (loop [z-re c-re z-im c-im it (i32 0)]
                (if (>= it max-iter) max-iter-f
                  (let [r2 (+ (* z-re z-re) (* z-im z-im))]
                    (if (>= r2 4.0)
                      (+ (f32 it) (- 1.0 (/ (log (log r2)) 0.6931)))
                      (recur (+ (- (* z-re z-re) (* z-im z-im)) c-re)
                             (+ (* 2.0 (* z-re z-im)) c-im)
                             (+ it (i32 1))))))))
        s4v (let [c-re (+ base-re (* px-size q))
                  c-im (+ base-im (* py-size 0.25))]
              (loop [z-re c-re z-im c-im it (i32 0)]
                (if (>= it max-iter) max-iter-f
                  (let [r2 (+ (* z-re z-re) (* z-im z-im))]
                    (if (>= r2 4.0)
                      (+ (f32 it) (- 1.0 (/ (log (log r2)) 0.6931)))
                      (recur (+ (- (* z-re z-re) (* z-im z-im)) c-re)
                             (+ (* 2.0 (* z-re z-im)) c-im)
                             (+ it (i32 1))))))))
        ;; Average the four samples.
        avg         (* 0.25 (+ (+ s1v s2v) (+ s3v s4v)))
        ;; in-set if ALL four samples hit the cap. Using fraction for softness.
        frac-in     (+ (+ (if (>= s1v max-iter-f) 0.25 0.0)
                          (if (>= s2v max-iter-f) 0.25 0.0))
                       (+ (if (>= s3v max-iter-f) 0.25 0.0)
                          (if (>= s4v max-iter-f) 0.25 0.0)))
        ;; Smooth color from average escape value.
        cval        (sqrt (/ avg max-iter-f))
        ;; Dampen hue phase as we zoom — keeps bands stable over time.
        r-base      (* 0.5 (+ 1.0 (sin (+ (* cval 12.0) shift))))
        g-base      (* 0.5 (+ 1.0 (sin (+ (* cval 12.0) (+ shift 2.1)))))
        b-base      (* 0.5 (+ 1.0 (sin (+ (* cval 12.0) (+ shift 4.2)))))
        ;; Fade toward black as more of the pixel is in the set.
        keep        (- 1.0 frac-in)
        r           (* keep r-base)
        g           (* keep g-base)
        b           (* keep b-base)
        ri          (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 r)))))
        gi          (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 g)))))
        bi          (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 b)))))]
    (bit-or (bit-or (bit-shift-left ri (u32 16)) (bit-shift-left gi (u32 8))) bi)))
