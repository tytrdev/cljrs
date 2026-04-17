;; Classic plasma effect — sum of sinusoids over space+time.
;;
;; Body is WGSL-emitted on the GPU; runs one invocation per pixel in
;; parallel. Sliders:
;;   s0  zoom           0..1000 → 0.5..4.0
;;   s1  warp amount    0..1000 → 0..2
;;   s2  hue rotation   0..1000 → 0..TAU
;;   s3  brightness     0..1000 → 0.2..1.5

(defn-gpu-pixel render
  [x y w h t-ms s0 s1 s2 s3]
  (let [;; Normalize slider → float param (all cheap, runs on GPU).
        zoom        (+ 0.5 (* 3.5 (/ (f32 s0) 1000.0)))
        warp        (* 2.0 (/ (f32 s1) 1000.0))
        hue-rot     (* 6.2831853 (/ (f32 s2) 1000.0))
        brightness  (+ 0.2 (* 1.3 (/ (f32 s3) 1000.0)))
        ;; Map pixel to [-1, 1].
        u  (* zoom (- (* 2.0 (/ (f32 x) (f32 w))) 1.0))
        v  (* zoom (- (* 2.0 (/ (f32 y) (f32 h))) 1.0))
        t  (* 0.001 (f32 t-ms))
        ;; Warp coordinates.
        wu (+ u (* warp (sin (+ (* v 3.0) t))))
        wv (+ v (* warp (cos (+ (* u 2.5) (* t 0.7)))))
        ;; Superposition of sine bands.
        a  (sin (+ (* wu 4.0) (* t 1.2)))
        b  (sin (+ (* wv 4.0) (* t 1.7)))
        c  (sin (+ (sqrt (+ (* wu wu) (* wv wv))) (* t 2.3)))
        v0 (+ (+ a b) c)
        ;; v0 is in ~[-3,3] — fold through sin again for a smooth color.
        s  (* 0.5 (+ 1.0 (sin (+ v0 hue-rot))))
        ;; Cheap palette: map s,s+0.33,s+0.66 to RGB.
        r  (* brightness (* 0.5 (+ 1.0 (sin (* 6.2831853 s)))))
        g  (* brightness (* 0.5 (+ 1.0 (sin (* 6.2831853 (+ s 0.333))))))
        bl (* brightness (* 0.5 (+ 1.0 (sin (* 6.2831853 (+ s 0.666))))))
        ;; Quantize to bytes and pack (u32 for WGSL bitwise).
        ri (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 r)))))
        gi (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 g)))))
        bi (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 bl)))))]
    (bit-or (bit-or (bit-shift-left ri (u32 16)) (bit-shift-left gi (u32 8))) bi)))
