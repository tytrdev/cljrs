;; Wireframe 3D rasterizer — rotating cube, rendered per-pixel by
;; computing the distance from each pixel to each projected edge of the
;; cube. If the minimum distance is under a threshold, we draw the edge
;; (shaded by 1/z for depth cues).
;;
;; Eight vertices, twelve edges, all inlined — cljrs native subset
;; doesn't have arrays yet, so loops-over-edges happen via inlined
;; self-call iteration.

(def slider-0-label "rotation speed")
(def slider-1-label "cube size")
(def slider-2-label "line thickness")
(def slider-3-label "camera distance")

;; Project a 3D point (after rotation) to 2D screen coords.
;; Returns x-component; the y is done by a sibling fn (no tuple returns).
(defn-native proj-x ^f64 [^f64 x ^f64 y ^f64 z ^f64 cam-z]
  (let [zr (+ z cam-z)]
    (/ (* x 400.0) zr)))

(defn-native proj-y ^f64 [^f64 x ^f64 y ^f64 z ^f64 cam-z]
  (let [zr (+ z cam-z)]
    (/ (* y 400.0) zr)))

;; Rotated vertex coords. Two rotations: first around Y, then around X.
(defn-native rot-x ^f64 [^f64 x ^f64 y ^f64 z ^f64 ay ^f64 ax]
  (let [cay (cos ay) say (sin ay)
        x1 (- (* x cay) (* z say))
        z1 (+ (* x say) (* z cay))
        cax (cos ax) sax (sin ax)
        y2 (- (* y cax) (* z1 sax))]
    x1))

(defn-native rot-y ^f64 [^f64 x ^f64 y ^f64 z ^f64 ay ^f64 ax]
  (let [cay (cos ay) say (sin ay)
        z1 (+ (* x say) (* z cay))
        cax (cos ax) sax (sin ax)]
    (- (* y cax) (* z1 sax))))

(defn-native rot-z ^f64 [^f64 x ^f64 y ^f64 z ^f64 ay ^f64 ax]
  (let [cay (cos ay) say (sin ay)
        z1 (+ (* x say) (* z cay))
        cax (cos ax) sax (sin ax)]
    (+ (* y sax) (* z1 cax))))

;; 2D distance from point to line segment AB.
(defn-native dist-seg ^f64 [^f64 px ^f64 py ^f64 ax ^f64 ay ^f64 bx ^f64 by]
  (let [dx (- bx ax) dy (- by ay)
        lensq (+ (* dx dx) (* dy dy))
        tt (max 0.0 (min 1.0 (/ (+ (* (- px ax) dx) (* (- py ay) dy))
                                 (max lensq 0.00001))))
        cx (+ ax (* tt dx))
        cy (+ ay (* tt dy))
        ddx (- px cx) ddy (- py cy)]
    (sqrt (+ (* ddx ddx) (* ddy ddy)))))

;; Projected screen-x for vertex index `i` in [0..7] of the unit cube.
;; Cube corners: (±s, ±s, ±s). We index i as (ix<<0 | iy<<1 | iz<<2).
(defn-native vx-rx ^f64 [^i64 i ^f64 s ^f64 ay ^f64 ax ^f64 cam-z]
  (let [sx (if (= (mod i 2) 0) (- s) s)
        sy (if (= (mod (/ i 2) 2) 0) (- s) s)
        sz (if (= (mod (/ i 4) 2) 0) (- s) s)
        rx-v (rot-x sx sy sz ay ax)
        ry-v (rot-y sx sy sz ay ax)
        rz-v (rot-z sx sy sz ay ax)]
    (proj-x rx-v ry-v rz-v cam-z)))

(defn-native vx-ry ^f64 [^i64 i ^f64 s ^f64 ay ^f64 ax ^f64 cam-z]
  (let [sx (if (= (mod i 2) 0) (- s) s)
        sy (if (= (mod (/ i 2) 2) 0) (- s) s)
        sz (if (= (mod (/ i 4) 2) 0) (- s) s)
        rx-v (rot-x sx sy sz ay ax)
        ry-v (rot-y sx sy sz ay ax)
        rz-v (rot-z sx sy sz ay ax)]
    (proj-y rx-v ry-v rz-v cam-z)))

;; Check one cube edge (given its two vertex indices) — returns the
;; distance from pixel (fx, fy) to the projected segment.
(defn-native edge-dist ^f64
  [^f64 fx ^f64 fy ^i64 a ^i64 b ^f64 s ^f64 ay ^f64 ax ^f64 cam-z]
  (let [ax2 (vx-rx a s ay ax cam-z)
        ay2 (vx-ry a s ay ax cam-z)
        bx2 (vx-rx b s ay ax cam-z)
        by2 (vx-ry b s ay ax cam-z)]
    (dist-seg fx fy ax2 ay2 bx2 by2)))

;; Minimum distance to any of the cube's 12 edges. Edges hardcoded by
;; vertex-index pairs. Using self-recursion over the 12 edges since we
;; don't have arrays yet.
;; Edge list (a, b): bottom face (0-1, 1-3, 3-2, 2-0),
;;                    top face    (4-5, 5-7, 7-6, 6-4),
;;                    verticals   (0-4, 1-5, 2-6, 3-7)
(defn-native min-edge-dist ^f64
  [^f64 fx ^f64 fy ^f64 s ^f64 ay ^f64 ax ^f64 cam-z]
  (let [d01 (edge-dist fx fy 0 1 s ay ax cam-z)
        d13 (edge-dist fx fy 1 3 s ay ax cam-z)
        d32 (edge-dist fx fy 3 2 s ay ax cam-z)
        d20 (edge-dist fx fy 2 0 s ay ax cam-z)
        d45 (edge-dist fx fy 4 5 s ay ax cam-z)
        d57 (edge-dist fx fy 5 7 s ay ax cam-z)
        d76 (edge-dist fx fy 7 6 s ay ax cam-z)
        d64 (edge-dist fx fy 6 4 s ay ax cam-z)
        d04 (edge-dist fx fy 0 4 s ay ax cam-z)
        d15 (edge-dist fx fy 1 5 s ay ax cam-z)
        d26 (edge-dist fx fy 2 6 s ay ax cam-z)
        d37 (edge-dist fx fy 3 7 s ay ax cam-z)
        m1 (min d01 (min d13 (min d32 d20)))
        m2 (min d45 (min d57 (min d76 d64)))
        m3 (min d04 (min d15 (min d26 d37)))]
    (min m1 (min m2 m3))))

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
        ay time
        ax (* 0.6 time)
        d (min-edge-dist fx fy size ay ax cam-z)]
    (if (< d thick)
      ;; On an edge — cyan/white gradient with a subtle pulse.
      (let [glow (- 1.0 (/ d thick))
            pulse (+ 0.85 (* 0.15 (sin (* 2.0 time))))
            r (int (* 255.0 (* glow pulse 0.5)))
            g (int (* 255.0 (* glow pulse 0.95)))
            b (int (* 255.0 (* glow pulse 1.0)))
            rc (max 0 (min 255 r))
            gc (max 0 (min 255 g))
            bc (max 0 (min 255 b))]
        (+ (* 65536 rc) (* 256 gc) bc))
      ;; Background — dark gradient by pixel-y for a subtle vignette.
      (let [by (/ (float py) height)
            dv (int (* 20.0 (- 1.0 by)))]
        (+ (* 65536 dv) (* 256 dv) (+ dv 8))))))
