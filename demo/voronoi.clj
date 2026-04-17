;; Voronoi diagram — for each pixel, distance to eight moving seed
;; points; color by which is closest, with darker cell borders.
;;
;; Perf note: the previous version called `seed-x` / `seed-y` three times
;; per pixel (once for nearest-idx, nearest-dist, second-dist), so each
;; seed's position was computed three times and 48 sin/cos calls happened
;; per pixel. Now all 8 seed positions are evaluated once at the top of
;; render-pixel, cutting trig down to 16 per pixel — and the nearest/
;; second-nearest/idx work is inline scalar arithmetic on the 8 distances.

(def slider-0-label "seed drift radius")
(def slider-1-label "cell color intensity")
(def slider-2-label "border thickness")
(def slider-3-label "drift speed")

(defn-native dist-sq ^f64 [^f64 ax ^f64 ay ^f64 bx ^f64 by]
  (let [dx (- ax bx) dy (- ay by)]
    (+ (* dx dx) (* dy dy))))

(defn-native seed-x ^f64 [^i64 i ^f64 t ^f64 drift]
  (let [phase (* (float i) 0.785)
        base  (+ 480.0 (* 300.0 (cos (* 1.7 phase))))]
    (+ base (* drift (cos (+ t phase))))))

(defn-native seed-y ^f64 [^i64 i ^f64 t ^f64 drift]
  (let [phase (* (float i) 0.785)
        base  (+ 270.0 (* 180.0 (sin (* 1.7 phase))))]
    (+ base (* drift (sin (+ t (* 1.3 phase)))))))

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
        ;; Evaluate all 8 seeds ONCE per pixel. The underlying trig calls
        ;; are pixel-invariant, but cljrs can't hoist across pixel calls,
        ;; so this is the cheapest structure inside one render-pixel call.
        x0 (seed-x 0 t drift) y0 (seed-y 0 t drift)
        x1 (seed-x 1 t drift) y1 (seed-y 1 t drift)
        x2 (seed-x 2 t drift) y2 (seed-y 2 t drift)
        x3 (seed-x 3 t drift) y3 (seed-y 3 t drift)
        x4 (seed-x 4 t drift) y4 (seed-y 4 t drift)
        x5 (seed-x 5 t drift) y5 (seed-y 5 t drift)
        x6 (seed-x 6 t drift) y6 (seed-y 6 t drift)
        x7 (seed-x 7 t drift) y7 (seed-y 7 t drift)
        d0 (dist-sq fx fy x0 y0)
        d1 (dist-sq fx fy x1 y1)
        d2 (dist-sq fx fy x2 y2)
        d3 (dist-sq fx fy x3 y3)
        d4 (dist-sq fx fy x4 y4)
        d5 (dist-sq fx fy x5 y5)
        d6 (dist-sq fx fy x6 y6)
        d7 (dist-sq fx fy x7 y7)
        ;; Nearest distance across all 8.
        m01 (min d0 d1) m23 (min d2 d3)
        m45 (min d4 d5) m67 (min d6 d7)
        m0123 (min m01 m23) m4567 (min m45 m67)
        best (min m0123 m4567)
        ;; Second-nearest via "min of everyone except the winner". We do
        ;; this the cheap way: set the winner's distance to +inf and take
        ;; the min again. `1e12` is well above any realistic distance.
        big 1000000000000.0
        d0b (if (= d0 best) big d0)
        d1b (if (= d1 best) big d1)
        d2b (if (= d2 best) big d2)
        d3b (if (= d3 best) big d3)
        d4b (if (= d4 best) big d4)
        d5b (if (= d5 best) big d5)
        d6b (if (= d6 best) big d6)
        d7b (if (= d7 best) big d7)
        m01b (min d0b d1b) m23b (min d2b d3b)
        m45b (min d4b d5b) m67b (min d6b d7b)
        m0123b (min m01b m23b) m4567b (min m45b m67b)
        second (min m0123b m4567b)
        ;; Nearest index — cascading comparisons against `best`.
        idx (if (= d0 best) 0
              (if (= d1 best) 1
                (if (= d2 best) 2
                  (if (= d3 best) 3
                    (if (= d4 best) 4
                      (if (= d5 best) 5
                        (if (= d6 best) 6 7)))))))
        diff   (- (sqrt second) (sqrt best))
        hue    (* 0.785 (float idx))
        r      (int (* 255.0 (min 1.0 (* intens (+ 0.5 (* 0.5 (sin hue)))))))
        g      (int (* 255.0 (min 1.0 (* intens (+ 0.5 (* 0.5 (sin (+ hue 2.094))))))))
        b      (int (* 255.0 (min 1.0 (* intens (+ 0.5 (* 0.5 (sin (+ hue 4.188))))))))
        dark   (if (< diff border) 1 0)
        rc     (if (= dark 1) (int (* (float r) 0.15)) r)
        gc     (if (= dark 1) (int (* (float g) 0.15)) g)
        bc     (if (= dark 1) (int (* (float b) 0.15)) b)]
    (+ (* 65536 rc) (* 256 gc) bc)))
