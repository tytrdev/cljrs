;; Mandelbrot set — escape-time iteration, compiled to a real WGSL loop.
;; Per-pixel complexity depends on zoom + iter cap. At 960×540 with
;; max-iter=256 this still hits hundreds of fps on an integrated GPU.
;;
;; Sliders:
;;   s0  zoom level     0..1000 → 10^(-0.5)..10^4  (log scale)
;;   s1  center-x       0..1000 → -2.0..1.0
;;   s2  center-y       0..1000 → -1.5..1.5
;;   s3  palette shift  0..1000 → 0..TAU

(defn-gpu-pixel render
  [x y w h t-ms s0 s1 s2 s3]
  (let [;; Slider → float.
        zoom-log  (- (* 4.5 (/ (f32 s0) 1000.0)) 0.5)
        zoom      (pow 10.0 zoom-log)
        cx        (- (* 3.0 (/ (f32 s1) 1000.0)) 2.0)
        cy        (- (* 3.0 (/ (f32 s2) 1000.0)) 1.5)
        shift     (* 6.2831853 (/ (f32 s3) 1000.0))
        ;; Pixel → complex plane.
        aspect    (/ (f32 w) (f32 h))
        u         (/ (- (* 2.0 (/ (f32 x) (f32 w))) 1.0) zoom)
        v         (/ (- (* 2.0 (/ (f32 y) (f32 h))) 1.0) zoom)
        c-re      (+ cx (* u aspect))
        c-im      (+ cy v)
        max-iter  (i32 256)
        ;; Escape-time loop. `(loop ...)` emits a real WGSL loop with
        ;; `continue` / `break`, so iteration count is runtime-variable.
        escape    (loop [z-re c-re
                         z-im c-im
                         it   (i32 0)]
                    (if (>= it max-iter)
                      it
                      (if (>= (+ (* z-re z-re) (* z-im z-im)) 4.0)
                        it
                        (recur (+ (- (* z-re z-re) (* z-im z-im)) c-re)
                               (+ (* 2.0 (* z-re z-im)) c-im)
                               (+ it (i32 1))))))
        ;; Smooth color from iteration count. Points in the set
        ;; (escape == max-iter) render black.
        in-set    (>= escape max-iter)
        t         (/ (f32 escape) (f32 max-iter))
        r         (if in-set 0.0 (* 0.5 (+ 1.0 (sin (+ (* t 9.0) shift)))))
        g         (if in-set 0.0 (* 0.5 (+ 1.0 (sin (+ (* t 9.0) (+ shift 2.0))))))
        b         (if in-set 0.0 (* 0.5 (+ 1.0 (sin (+ (* t 9.0) (+ shift 4.0))))))
        ri        (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 r)))))
        gi        (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 g)))))
        bi        (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 b)))))]
    (bit-or (bit-or (bit-shift-left ri (u32 16)) (bit-shift-left gi (u32 8))) bi)))
