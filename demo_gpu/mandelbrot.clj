;; Auto-zooming Mandelbrot into a deep Misiurewicz point.
;;
;; Stabilized via 8× rotated-grid spatial AA (no temporal blur) +
;; smooth escape-time coloring. Iteration cap scales with log2(zoom)
;; so detail keeps emerging as we descend.
;;
;; Target: (-0.7436438870371587, 0.1318259042053119)  — Misiurewicz M23
;;
;; Sliders:
;;   s0  zoom rate            0..1000 → 0.05..0.35 per second
;;   s1  x nudge (×1e-3)      0..1000 → ±1
;;   s2  y nudge (×1e-3)      0..1000 → ±1
;;   s3  palette hue          0..1000 → 0..TAU

;; Factor out the per-sample escape computation so we can crank the
;; sample count without drowning the file in copy-pasted loops. This
;; macro expands into a fresh let+loop each time it's invoked, so the
;; GPU emitter sees N independent (let ...) blocks.
(defmacro sample-escape [dx-mul dy-mul]
  `(let [c-re# (+ base-re (* px-size (* ~dx-mul aspect)))
         c-im# (+ base-im (* py-size ~dy-mul))]
     (loop [z-re c-re#
            z-im c-im#
            it   (i32 0)]
       (if (>= it max-iter)
         max-iter-f
         (let [r2 (+ (* z-re z-re) (* z-im z-im))]
           (if (>= r2 4.0)
             (+ (f32 it) (- 1.0 (/ (log (log r2)) 0.6931)))
             (recur (+ (- (* z-re z-re) (* z-im z-im)) c-re#)
                    (+ (* 2.0 (* z-re z-im)) c-im#)
                    (+ it (i32 1)))))))))

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
        iter-float  (+ 100.0 (* 40.0 (/ (log zoom) 0.6931)))
        max-iter    (min (i32 2000) (max (i32 64) (i32 iter-float)))
        max-iter-f  (f32 max-iter)
        px-size     (/ 2.0 (* fw zoom))
        py-size     (/ 2.0 (* fh zoom))
        u0          (/ (- (* 2.0 (/ (f32 x) fw)) 1.0) zoom)
        v0          (/ (- (* 2.0 (/ (f32 y) fh)) 1.0) zoom)
        base-re     (+ cx (* u0 aspect))
        base-im     (+ cy v0)
        ;; 8× rotated-grid AA sample offsets. Chosen to evenly cover the
        ;; pixel area and not align with any natural axis — gives better
        ;; filament edge coverage than a 2×2 or 4-rooks pattern.
        s1v  (sample-escape -0.4375 -0.0625)
        s2v  (sample-escape -0.1875 -0.3125)
        s3v  (sample-escape  0.0625 -0.4375)
        s4v  (sample-escape  0.3125 -0.1875)
        s5v  (sample-escape  0.4375  0.0625)
        s6v  (sample-escape  0.1875  0.3125)
        s7v  (sample-escape -0.0625  0.4375)
        s8v  (sample-escape -0.3125  0.1875)
        avg  (* 0.125 (+ (+ (+ s1v s2v) (+ s3v s4v))
                         (+ (+ s5v s6v) (+ s7v s8v))))
        frac-in (* 0.125
                   (+ (+ (+ (if (>= s1v max-iter-f) 1.0 0.0)
                            (if (>= s2v max-iter-f) 1.0 0.0))
                         (+ (if (>= s3v max-iter-f) 1.0 0.0)
                            (if (>= s4v max-iter-f) 1.0 0.0)))
                      (+ (+ (if (>= s5v max-iter-f) 1.0 0.0)
                            (if (>= s6v max-iter-f) 1.0 0.0))
                         (+ (if (>= s7v max-iter-f) 1.0 0.0)
                            (if (>= s8v max-iter-f) 1.0 0.0)))))
        cval   (sqrt (/ avg max-iter-f))
        keep   (- 1.0 frac-in)
        r      (* keep (* 0.5 (+ 1.0 (sin (+ (* cval 12.0) shift)))))
        g      (* keep (* 0.5 (+ 1.0 (sin (+ (* cval 12.0) (+ shift 2.1))))))
        b      (* keep (* 0.5 (+ 1.0 (sin (+ (* cval 12.0) (+ shift 4.2))))))
        ri     (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 r)))))
        gi     (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 g)))))
        bi     (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 b)))))]
    (bit-or (bit-or (bit-shift-left ri (u32 16)) (bit-shift-left gi (u32 8))) bi)))
