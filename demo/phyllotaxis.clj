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

;; Walk seeds 0..count-1, track (best-d, best-i).
(defn-native best-idx ^i64
  [^f64 px ^f64 py ^f64 cx ^f64 cy ^f64 scale ^i64 count]
  (loop [i 1
         best-d (seed-dist-sq 0 px py cx cy scale)
         best-i 0]
    (if (>= i count)
      best-i
      (let [d (seed-dist-sq i px py cx cy scale)]
        (if (< d best-d)
          (recur (+ i 1) d i)
          (recur (+ i 1) best-d best-i))))))

(defn-native best-dist ^f64
  [^f64 px ^f64 py ^f64 cx ^f64 cy ^f64 scale ^i64 count]
  (loop [i 1 best-d (seed-dist-sq 0 px py cx cy scale)]
    (if (>= i count)
      best-d
      (let [d (seed-dist-sq i px py cx cy scale)]
        (if (< d best-d)
          (recur (+ i 1) d)
          (recur (+ i 1) best-d))))))

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
        d   (sqrt (best-dist fx fy cx cy scale count))
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
