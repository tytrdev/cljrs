;; Wireframe 3D rasterizer — rotating cube, rendered per-pixel by
;; computing the distance from each pixel to each projected edge of the
;; cube. Under threshold = draw the edge, shaded by proximity and a
;; gentle pulse.
;;
;; Perf note: the key trick is that the rotation angle is the SAME for
;; every pixel in a given frame, so sin/cos of (ay, ax) are computed
;; ONCE per pixel (not per vertex, not per edge). All 8 projected
;; vertices are precomputed into let-bound scalars at the top of
;; render-pixel; the 12 edge-distance checks then use them directly.
;; Trig budget: 4 calls per pixel instead of ~576.

(def slider-0-label "rotation speed")
(def slider-1-label "cube size")
(def slider-2-label "line thickness")
(def slider-3-label "camera distance")

;; 2D distance from point P to the line segment AB.
(defn-native dist-seg ^f64 [^f64 px ^f64 py ^f64 ax ^f64 ay ^f64 bx ^f64 by]
  (let [dx (- bx ax) dy (- by ay)
        lensq (+ (* dx dx) (* dy dy))
        tt (max 0.0 (min 1.0 (/ (+ (* (- px ax) dx) (* (- py ay) dy))
                                 (max lensq 0.00001))))
        cx (+ ax (* tt dx))
        cy (+ ay (* tt dy))
        ddx (- px cx) ddy (- py cy)]
    (sqrt (+ (* ddx ddx) (* ddy ddy)))))

;; Project one cube vertex (index 0..7) through Y-rot then X-rot then
;; perspective. Returns the screen-x of vertex i.
;; Takes precomputed trig values so no sin/cos happens inside.
(defn-native pvx ^f64
  [^i64 i ^f64 s ^f64 cay ^f64 say ^f64 cax ^f64 sax ^f64 cam-z]
  (let [sx (if (= (mod i 2) 0)           (- s) s)
        sy (if (= (mod (/ i 2) 2) 0)     (- s) s)
        sz (if (= (mod (/ i 4) 2) 0)     (- s) s)
        x1 (- (* sx cay) (* sz say))
        z1 (+ (* sx say) (* sz cay))
        z2 (+ (* sy sax) (* z1 cax))]
    (/ (* x1 400.0) (+ z2 cam-z))))

;; Same vertex, screen-y.
(defn-native pvy ^f64
  [^i64 i ^f64 s ^f64 cay ^f64 say ^f64 cax ^f64 sax ^f64 cam-z]
  (let [sx (if (= (mod i 2) 0)           (- s) s)
        sy (if (= (mod (/ i 2) 2) 0)     (- s) s)
        sz (if (= (mod (/ i 4) 2) 0)     (- s) s)
        z1 (+ (* sx say) (* sz cay))
        y2 (- (* sy cax) (* z1 sax))
        z2 (+ (* sy sax) (* z1 cax))]
    (/ (* y2 400.0) (+ z2 cam-z))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width   960.0
        height  540.0
        rot-sp  (+ 0.3 (* 3.0 (/ (float s0) 1000.0)))
        size    (+ 0.5 (* 1.5 (/ (float s1) 1000.0)))
        thick   (+ 1.0 (* 6.0 (/ (float s2) 1000.0)))
        cam-z   (+ 2.0 (* 6.0 (/ (float s3) 1000.0)))
        time    (* rot-sp (/ (float t-ms) 1000.0))
        fx (- (float px) (/ width 2.0))
        fy (- (float py) (/ height 2.0))
        ;; Angle trig computed once per pixel, reused for all 8 vertices.
        ay time
        ax (* 0.6 time)
        cay (cos ay) say (sin ay)
        cax (cos ax) sax (sin ax)
        ;; All 8 vertex projections, computed once.
        x0 (pvx 0 size cay say cax sax cam-z)  y0 (pvy 0 size cay say cax sax cam-z)
        x1 (pvx 1 size cay say cax sax cam-z)  y1 (pvy 1 size cay say cax sax cam-z)
        x2 (pvx 2 size cay say cax sax cam-z)  y2 (pvy 2 size cay say cax sax cam-z)
        x3 (pvx 3 size cay say cax sax cam-z)  y3 (pvy 3 size cay say cax sax cam-z)
        x4 (pvx 4 size cay say cax sax cam-z)  y4 (pvy 4 size cay say cax sax cam-z)
        x5 (pvx 5 size cay say cax sax cam-z)  y5 (pvy 5 size cay say cax sax cam-z)
        x6 (pvx 6 size cay say cax sax cam-z)  y6 (pvy 6 size cay say cax sax cam-z)
        x7 (pvx 7 size cay say cax sax cam-z)  y7 (pvy 7 size cay say cax sax cam-z)
        ;; 12 cube edges — indexed pairs as described in the comment.
        ;; Bottom face.
        d01 (dist-seg fx fy x0 y0 x1 y1)
        d13 (dist-seg fx fy x1 y1 x3 y3)
        d32 (dist-seg fx fy x3 y3 x2 y2)
        d20 (dist-seg fx fy x2 y2 x0 y0)
        ;; Top face.
        d45 (dist-seg fx fy x4 y4 x5 y5)
        d57 (dist-seg fx fy x5 y5 x7 y7)
        d76 (dist-seg fx fy x7 y7 x6 y6)
        d64 (dist-seg fx fy x6 y6 x4 y4)
        ;; Verticals.
        d04 (dist-seg fx fy x0 y0 x4 y4)
        d15 (dist-seg fx fy x1 y1 x5 y5)
        d26 (dist-seg fx fy x2 y2 x6 y6)
        d37 (dist-seg fx fy x3 y3 x7 y7)
        m1 (min d01 (min d13 (min d32 d20)))
        m2 (min d45 (min d57 (min d76 d64)))
        m3 (min d04 (min d15 (min d26 d37)))
        d (min m1 (min m2 m3))]
    (if (< d thick)
      (let [glow (- 1.0 (/ d thick))
            pulse (+ 0.85 (* 0.15 (sin (* 2.0 time))))
            r (int (* 255.0 (* glow pulse 0.5)))
            g (int (* 255.0 (* glow pulse 0.95)))
            b (int (* 255.0 (* glow pulse 1.0)))
            rc (max 0 (min 255 r))
            gc (max 0 (min 255 g))
            bc (max 0 (min 255 b))]
        (+ (* 65536 rc) (* 256 gc) bc))
      (let [by (/ (float py) height)
            dv (int (* 20.0 (- 1.0 by)))]
        (+ (* 65536 dv) (* 256 dv) (+ dv 8))))))
