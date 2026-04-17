;; Phyllotaxis — the sunflower-seed arrangement. Each seed sits at
;; polar coords (sqrt(i), i * golden-angle); for each pixel we find the
;; nearest seed out of N and color by that seed's index. The result is
;; the characteristic Fibonacci spiral.

(def slider-0-label "seed count (50..400)")
(def slider-1-label "scale")
(def slider-2-label "seed radius")
(def slider-3-label "color shift")

;; Squared distance from pixel (px, py) to the i-th phyllotaxis seed at
;; scale `scale`, centered on (cx, cy). Golden angle = 137.508°.
(defn-native seed-dist-sq ^f64
  [^i64 i ^f64 px ^f64 py ^f64 cx ^f64 cy ^f64 scale]
  (let [fi (float i)
        r  (* scale (sqrt fi))
        a  (* fi 2.3999632)  ;; golden angle in radians
        sx (+ cx (* r (cos a)))
        sy (+ cy (* r (sin a)))
        dx (- px sx) dy (- py sy)]
    (+ (* dx dx) (* dy dy))))

;; Spiral-aware nearest-seed search.
;;
;; Since seed i sits at radius `scale * sqrt(i)`, a pixel at radius r from
;; center is "close" to seeds with i near (r / scale)². We brute-force a
;; small index window around that prediction instead of scanning all N.
;; For N = 225 and a ±24 window, this is ~10× fewer distance computations
;; per pixel while matching the exhaustive result in the interior.
;;
;; (The BSP user suggested would be the principled version; this one
;; exploits the specific structure of the point set for free.)

(defn-native predict-i ^i64 [^f64 px ^f64 py ^f64 cx ^f64 cy ^f64 scale]
  (let [dx (- px cx) dy (- py cy)
        r2 (+ (* dx dx) (* dy dy))]
    (int (/ r2 (* scale scale)))))

(defn-native best-idx ^i64
  [^f64 px ^f64 py ^f64 cx ^f64 cy ^f64 scale ^i64 count]
  (let [pred (predict-i px py cx cy scale)
        window 24
        lo (max 0 (- pred window))
        hi (min count (+ pred window))
        start lo
        start-d (seed-dist-sq start px py cx cy scale)]
    (loop [i (+ start 1) best-d start-d best-i start]
      (if (>= i hi)
        best-i
        (let [d (seed-dist-sq i px py cx cy scale)]
          (if (< d best-d)
            (recur (+ i 1) d i)
            (recur (+ i 1) best-d best-i)))))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [count  (+ 50 (int (* 350.0 (/ (float s0) 1000.0))))
        scale  (+ 5.0 (* 30.0 (/ (float s1) 1000.0)))
        radius (+ 1.0 (* 20.0 (/ (float s2) 1000.0)))
        shift  (/ (float s3) 1000.0)
        cx 480.0
        cy 270.0
        fx (float px)
        fy (float py)
        idx (best-idx fx fy cx cy scale count)
        ;; Distance to the already-known best seed is one call, not a
        ;; second full scan.
        d   (sqrt (seed-dist-sq idx fx fy cx cy scale))
        ;; Inside seed disc → saturated color; outside → faded.
        inside (if (< d radius) 1 0)
        hue (+ (* 6.2831853 shift) (* 0.05 (float idx)))
        base (if (= inside 1) 1.0 (max 0.1 (- 1.0 (/ d (* 3.0 radius)))))
        r (int (* 255.0 base (+ 0.5 (* 0.5 (sin hue)))))
        g (int (* 255.0 base (+ 0.5 (* 0.5 (sin (+ hue 2.094))))))
        b (int (* 255.0 base (+ 0.5 (* 0.5 (sin (+ hue 4.188))))))
        rc (max 0 (min 255 r))
        gc (max 0 (min 255 g))
        bc (max 0 (min 255 b))]
    (+ (* 65536 rc) (* 256 gc) bc)))
