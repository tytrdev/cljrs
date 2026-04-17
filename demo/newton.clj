;; Newton fractal for f(z) = z³ − 1. Iterate z ← z − f(z)/f'(z) until z
;; converges to one of the three cube roots of unity. Color each pixel
;; by which root it hit, shaded by how many iterations it took.
;;
;; Beautifully intricate basins of attraction fractal around the boundaries.

(def slider-0-label "zoom (0.3..5)")
(def slider-1-label "pan x (-1..1)")
(def slider-2-label "pan y (-1..1)")
(def slider-3-label "shade intensity")

;; Complex multiply (a+bi) * (c+di) — returns real part only.
;; Full complex arith spelled out inline inside the iter fn for clarity.

;; Returns packed: root-index * 1000 + iter-count (so both fit in one i64).
(defn-native newton-iter ^i64 [^f64 zr ^f64 zi ^i64 max-iter]
  (loop [i 0 r zr im zi]
    (if (>= i max-iter)
      ;; Didn't converge — encode as root "3".
      3000
      (let [;; Check convergence to each cube root of unity.
            d0r (- r 1.0)        d0i im
            d1r (+ r 0.5)        d1i (- im 0.8660254)
            d2r (+ r 0.5)        d2i (+ im 0.8660254)
            e0  (+ (* d0r d0r) (* d0i d0i))
            e1  (+ (* d1r d1r) (* d1i d1i))
            e2  (+ (* d2r d2r) (* d2i d2i))
            eps 0.0001]
        (if (< e0 eps)
          (+ (* 0 1000) i)
          (if (< e1 eps)
            (+ (* 1 1000) i)
            (if (< e2 eps)
              (+ (* 2 1000) i)
              ;; Compute z - (z^3 - 1) / (3 z^2)
              ;; z^2 = (r^2-im^2) + 2r*im i
              (let [zr2 (- (* r r) (* im im))
                    zi2 (* 2.0 (* r im))
                    ;; z^3 = z * z^2 = (r*zr2 - im*zi2) + (r*zi2 + im*zr2) i
                    z3r (- (* r zr2) (* im zi2))
                    z3i (+ (* r zi2) (* im zr2))
                    ;; f = z^3 - 1
                    fr  (- z3r 1.0)
                    fi  z3i
                    ;; f' = 3 z^2
                    dfr (* 3.0 zr2)
                    dfi (* 3.0 zi2)
                    ;; f/f' = (fr+fi i) / (dfr+dfi i) = ((fr*dfr+fi*dfi) + (fi*dfr-fr*dfi)i) / (dfr^2+dfi^2)
                    denom (+ (* dfr dfr) (* dfi dfi))
                    qr    (/ (+ (* fr dfr) (* fi dfi)) denom)
                    qi    (/ (- (* fi dfr) (* fr dfi)) denom)]
                (recur (+ i 1) (- r qr) (- im qi))))))))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width  960.0
        height 540.0
        zoom   (+ 0.3 (* 4.7 (/ (float s0) 1000.0)))
        pan-x  (- (* 2.0 (/ (float s1) 1000.0)) 1.0)
        pan-y  (- (* 2.0 (/ (float s2) 1000.0)) 1.0)
        shade  (+ 0.3 (* 1.2 (/ (float s3) 1000.0)))
        fx (float px)
        fy (float py)
        zr (+ pan-x (/ (- (* 3.5 (/ fx width))  1.75) zoom))
        zi (+ pan-y (/ (- (* 2.0 (/ fy height)) 1.0)  zoom))
        packed (newton-iter zr zi 60)
        root   (/ packed 1000)
        iters  (mod packed 1000)
        fade   (- 1.0 (min 0.9 (/ (float iters) 60.0)))
        v      (* shade fade)
        r0     (if (= root 0) 255 (if (= root 1) 60  (if (= root 2) 60  30)))
        g0     (if (= root 0) 60  (if (= root 1) 255 (if (= root 2) 60  30)))
        b0     (if (= root 0) 60  (if (= root 1) 60  (if (= root 2) 255 30)))
        r      (int (* (float r0) v))
        g      (int (* (float g0) v))
        b      (int (* (float b0) v))
        rc     (min 255 (max 0 r))
        gc     (min 255 (max 0 g))
        bc     (min 255 (max 0 b))]
    (+ (* 65536 rc) (* 256 gc) bc)))
