;; Voronoi diagram — for each pixel, compute distance to eight moving
;; seed points and color by which one is closest. Seeds drift in slow
;; circles so the cells flow around. Classic "stained-glass" demo.

(def slider-0-label "seed drift radius")
(def slider-1-label "cell color intensity")
(def slider-2-label "border thickness")
(def slider-3-label "drift speed")

(defn-native dist-sq ^f64 [^f64 ax ^f64 ay ^f64 bx ^f64 by]
  (let [dx (- ax bx) dy (- ay by)]
    (+ (* dx dx) (* dy dy))))

;; Hand-rolled 8-way min returning (index, squared distance to nearest).
;; Emitter doesn't have tuple returns yet; we compute min and emit an
;; integer pack of (nearest-idx << 32 | nearest-dist-int).
;; Simpler: two passes — one for idx, one for dist. Costly. Inline both.
(defn-native seed-x ^f64 [^i64 i ^f64 t ^f64 drift]
  (let [phase (* (float i) 0.785)  ;; π/4 offset per seed
        base-x (+ 480.0 (* 300.0 (cos (+ phase (* 0.7 phase)))))]
    (+ base-x (* drift (cos (+ t phase))))))

(defn-native seed-y ^f64 [^i64 i ^f64 t ^f64 drift]
  (let [phase (* (float i) 0.785)
        base-y (+ 270.0 (* 180.0 (sin (+ phase (* 0.7 phase)))))]
    (+ base-y (* drift (sin (+ t (* 1.3 phase)))))))

(defn-native nearest ^i64 [^f64 px ^f64 py ^f64 t ^f64 drift]
  (loop [i 1 best-idx 0 best-d (dist-sq px py (seed-x 0 t drift) (seed-y 0 t drift))]
    (if (>= i 8)
      best-idx
      (let [sx (seed-x i t drift)
            sy (seed-y i t drift)
            d  (dist-sq px py sx sy)]
        (if (< d best-d)
          (recur (+ i 1) i d)
          (recur (+ i 1) best-idx best-d))))))

(defn-native nearest-dist ^f64 [^f64 px ^f64 py ^f64 t ^f64 drift]
  (loop [i 1 best-d (dist-sq px py (seed-x 0 t drift) (seed-y 0 t drift))]
    (if (>= i 8)
      best-d
      (let [sx (seed-x i t drift)
            sy (seed-y i t drift)
            d  (dist-sq px py sx sy)]
        (if (< d best-d)
          (recur (+ i 1) d)
          (recur (+ i 1) best-d))))))

;; Second-nearest distance — used to draw borders between cells.
(defn-native second-dist ^f64 [^f64 px ^f64 py ^f64 t ^f64 drift]
  (loop [i 0 best 1000000.0 second 1000000.0]
    (if (>= i 8)
      second
      (let [d (dist-sq px py (seed-x i t drift) (seed-y i t drift))]
        (if (< d best)
          (recur (+ i 1) d best)
          (if (< d second)
            (recur (+ i 1) best d)
            (recur (+ i 1) best second)))))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [drift  (+ 20.0 (* 180.0 (/ (float s0) 1000.0)))
        intens (+ 0.3 (* 1.2  (/ (float s1) 1000.0)))
        border (+ 1.0 (* 14.0 (/ (float s2) 1000.0)))
        speed  (* 0.005 (/ (float s3) 1000.0))
        t      (* speed (float t-ms))
        fx     (float px)
        fy     (float py)
        idx    (nearest fx fy t drift)
        d1     (nearest-dist fx fy t drift)
        d2     (second-dist fx fy t drift)
        diff   (- (sqrt d2) (sqrt d1))
        ;; Color per cell index via HSV-ish rotation.
        hue    (* 0.785 (float idx))   ;; π/4 per cell
        r      (int (* 255.0 (min 1.0 (* intens (+ 0.5 (* 0.5 (sin hue)))))))
        g      (int (* 255.0 (min 1.0 (* intens (+ 0.5 (* 0.5 (sin (+ hue 2.094))))))))
        b      (int (* 255.0 (min 1.0 (* intens (+ 0.5 (* 0.5 (sin (+ hue 4.188))))))))
        ;; If the pixel is near a border (d2 - d1 < border), darken.
        dark   (if (< diff border) 1 0)
        rc     (if (= dark 1) (int (* (float r) 0.15)) r)
        gc     (if (= dark 1) (int (* (float g) 0.15)) g)
        bc     (if (= dark 1) (int (* (float b) 0.15)) b)]
    (+ (* 65536 rc) (* 256 gc) bc)))
