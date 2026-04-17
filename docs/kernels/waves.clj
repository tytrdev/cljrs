;; Interference wave field — sum of circular wavefronts from N sources
;; moving in time. Simple but pretty, and a great benchmark because
;; every pixel does identical work.
;;
;; Sliders:
;;   s0  wave count        0..1000 → 1..8 sources
;;   s1  frequency         0..1000 → 0.5..20
;;   s2  speed             0..1000 → 0..4
;;   s3  color palette     0..1000 → 0..TAU

(defn-gpu-pixel render
  [x y w h t-ms s0 s1 s2 s3]
  (let [freq   (+ 0.5 (* 19.5 (/ (f32 s1) 1000.0)))
        speed  (* 4.0 (/ (f32 s2) 1000.0))
        hue    (* 6.2831853 (/ (f32 s3) 1000.0))
        t      (* 0.001 (f32 t-ms))
        ;; pixel to [-1,1], preserve aspect
        aspect (/ (f32 w) (f32 h))
        u      (* aspect (- (* 2.0 (/ (f32 x) (f32 w))) 1.0))
        v      (- (* 2.0 (/ (f32 y) (f32 h))) 1.0)
        ;; 4 moving sources in a rough square.
        a1     0.0  b1  0.0
        a2     0.7  b2  0.5
        a3     (- 0.6)  b3  (- 0.7)
        a4     0.3  b4  (- 0.3)
        ;; wavefront phase for each: sin(freq * dist - speed * t)
        d1     (sqrt (+ (* (- u a1) (- u a1)) (* (- v b1) (- v b1))))
        d2     (sqrt (+ (* (- u a2) (- u a2)) (* (- v b2) (- v b2))))
        d3     (sqrt (+ (* (- u a3) (- u a3)) (* (- v b3) (- v b3))))
        d4     (sqrt (+ (* (- u a4) (- u a4)) (* (- v b4) (- v b4))))
        p1     (sin (- (* freq d1) (* speed t)))
        p2     (sin (- (* freq d2) (* speed t)))
        p3     (sin (- (* freq d3) (* speed t)))
        p4     (sin (- (* freq d4) (* speed t)))
        ;; Sum & renormalize to [0,1].
        sum    (+ (+ p1 p2) (+ p3 p4))
        n      (* 0.5 (+ 1.0 (* 0.25 sum)))
        ;; Color.
        r      (* 0.5 (+ 1.0 (sin (+ hue (* 6.28 n)))))
        g      (* 0.5 (+ 1.0 (sin (+ (+ hue 2.0) (* 6.28 n)))))
        b      (* 0.5 (+ 1.0 (sin (+ (+ hue 4.0) (* 6.28 n)))))
        ri     (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 r)))))
        gi     (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 g)))))
        bi     (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 b)))))]
    (bit-or (bit-or (bit-shift-left ri (u32 16)) (bit-shift-left gi (u32 8))) bi)))
