;; Curl-noise flow field, visualized as streak-length on each pixel.
;; Each pixel walks backward along the curl of a 2D FBM scalar field
;; for a fixed number of steps, producing a streamline. Color encodes
;; the traversed arc so neighboring streamlines stay visually distinct.
;;
;; Sliders:
;;   s0  time warp       0..1000 -> 0..1 (freeze or flow)
;;   s1  flow scale      0..1000 -> 0.5..4 wavelength
;;   s2  trace length    0..1000 -> 8..48 steps
;;   s3  hue shift       0..1000 -> 0..TAU

;; hash2 / noise2 come from the shared stdlib.
(load-file "demo_gpu/stdlib.clj")

;; Potential field: 3-octave value noise with a time offset on x. Built
;; on stdlib `noise2` so the hash/interpolation stays identical to every
;; other noise-based kernel.
(defmacro pot [xf yf tt]
  `(let [n1 (noise2 (+ ~xf ~tt) ~yf)
         n2 (noise2 (* (+ ~xf ~tt) 2.13) (* ~yf 2.13))
         n3 (noise2 (* (+ ~xf ~tt) 4.47) (* ~yf 4.47))]
     (+ (* n1 0.5) (+ (* n2 0.3) (* n3 0.15)))))

(defn-gpu-pixel render
  [x y w h t-ms s0 s1 s2 s3]
  (let [flow-t (* 0.001 (f32 t-ms) (/ (f32 s0) 1000.0))
        scale  (+ 0.5 (* 3.5 (/ (f32 s1) 1000.0)))
        steps  (+ (i32 8) (i32 (* 40.0 (/ (f32 s2) 1000.0))))
        hue    (* 6.2831853 (/ (f32 s3) 1000.0))

        ;; Pixel to world coords (unit square with aspect).
        aspect (/ (f32 w) (f32 h))
        u0     (* scale (* aspect (- (/ (f32 x) (f32 w)) 0.5)))
        v0     (* scale (- (/ (f32 y) (f32 h)) 0.5))

        ;; Walk backward along velocity = (dP/dy, -dP/dx) for `steps` iters.
        ;; Curl of potential P is perpendicular gradient, divergence-free.
        eps    0.02
        ds     0.012

        ;; Walk the streamline. Return final position; we visualize how
        ;; far the point drifted (displacement magnitude) and its angle.
        ;; Loop returns a single f32; we encode by summing (dx + dy*K)
        ;; and unpacking outside. Cheaper: run twice, once per component.
        u-final (loop [uu u0  vv v0  i (i32 0)]
                  (if (>= i steps) uu
                    (let [px0 (pot (- uu eps) vv flow-t)
                          px1 (pot (+ uu eps) vv flow-t)
                          py0 (pot uu (- vv eps) flow-t)
                          py1 (pot uu (+ vv eps) flow-t)
                          dpdx (/ (- px1 px0) (* 2.0 eps))
                          dpdy (/ (- py1 py0) (* 2.0 eps))
                          vu   dpdy
                          vv2  (- dpdx)
                          vl   (max 0.001 (sqrt (+ (* vu vu) (* vv2 vv2))))]
                      (recur (- uu (/ (* vu ds) vl))
                             (- vv (/ (* vv2 ds) vl))
                             (+ i (i32 1))))))
        v-final (loop [uu u0  vv v0  i (i32 0)]
                  (if (>= i steps) vv
                    (let [px0 (pot (- uu eps) vv flow-t)
                          px1 (pot (+ uu eps) vv flow-t)
                          py0 (pot uu (- vv eps) flow-t)
                          py1 (pot uu (+ vv eps) flow-t)
                          dpdx (/ (- px1 px0) (* 2.0 eps))
                          dpdy (/ (- py1 py0) (* 2.0 eps))
                          vu   dpdy
                          vv2  (- dpdx)
                          vl   (max 0.001 (sqrt (+ (* vu vu) (* vv2 vv2))))]
                      (recur (- uu (/ (* vu ds) vl))
                             (- vv (/ (* vv2 ds) vl))
                             (+ i (i32 1))))))

        ;; Displacement magnitude (0 = didn't move, big = strong flow).
        dx    (- u-final u0)
        dy    (- v-final v0)
        disp  (sqrt (+ (* dx dx) (* dy dy)))
        ;; Angle of displacement in [0,1).
        ang   (+ 0.5 (/ (atan2 dy dx) 6.2831853))
        ;; Map angle to a smooth cyclic color (hue-ish) and modulate
        ;; brightness by displacement length so stagnant zones dim.
        phase (+ (* 6.2831853 ang) hue)
        amp   (max 0.2 (min 1.0 (* 3.5 disp)))
        r     (* amp (* 0.5 (+ 1.0 (sin phase))))
        g     (* amp (* 0.5 (+ 1.0 (sin (+ phase 2.094)))))
        b     (* amp (* 0.5 (+ 1.0 (sin (+ phase 4.188)))))]
    (pack-rgb r g b)))
