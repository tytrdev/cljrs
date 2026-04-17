;; Truchet tiling — divide the screen into square tiles; each tile
;; deterministically picks one of two diagonal arcs; the result forms
;; meandering continuous curves across the plane. Animate by rotating
;; the tile orientation over time and letting the tile size pulse.

(def slider-0-label "tile size")
(def slider-1-label "line thickness")
(def slider-2-label "color shift")
(def slider-3-label "animation speed")

;; Poor-man's deterministic hash for tile (ix, iy): a small int in {0,1}.
(defn-native tile-bit ^i64 [^i64 ix ^i64 iy ^i64 seed]
  (mod (+ (+ (* ix 73) (* iy 19)) (+ (* ix iy) seed)) 2))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [tile   (+ 20.0 (* 100.0 (/ (float s0) 1000.0)))
        thick  (+ 0.02 (* 0.2 (/ (float s1) 1000.0)))
        hue    (/ (float s2) 1000.0)
        speed  (* 0.003 (/ (float s3) 1000.0))
        t      (* speed (float t-ms))
        fx (float px)
        fy (float py)
        ;; Tile coordinates and in-tile UV [0, 1].
        ix (int (/ fx tile))
        iy (int (/ fy tile))
        u  (- (/ fx tile) (float ix))
        v  (- (/ fy tile) (float iy))
        ;; Which arc: NW-SE or NE-SW? Time-shifted seed makes patterns shimmer.
        seed (int (* 4.0 t))
        bit  (tile-bit ix iy seed)
        ;; Compute distance to the chosen arc. Each tile has two quarter-circle
        ;; arcs (radius 0.5) from opposite corners; we return min distance.
        ;; Bit 0: arcs at (0,0) and (1,1). Bit 1: arcs at (1,0) and (0,1).
        du0 u   dv0 v
        du1 (- u 1.0) dv1 (- v 1.0)
        d00 (abs (- (sqrt (+ (* du0 du0) (* dv0 dv0))) 0.5))
        d11 (abs (- (sqrt (+ (* du1 du1) (* dv1 dv1))) 0.5))
        du2 (- u 1.0) dv2 v
        du3 u          dv3 (- v 1.0)
        d10 (abs (- (sqrt (+ (* du2 du2) (* dv2 dv2))) 0.5))
        d01 (abs (- (sqrt (+ (* du3 du3) (* dv3 dv3))) 0.5))
        d   (if (= bit 0) (min d00 d11) (min d10 d01))
        on  (if (< d thick) 1 0)
        ;; Hue rotates with position + time for a flowing look.
        phase (+ (* 6.2831853 hue) (* 0.01 (float ix)) (* 0.01 (float iy)))
        r0 (int (* 255.0 (+ 0.5 (* 0.5 (sin phase)))))
        g0 (int (* 255.0 (+ 0.5 (* 0.5 (sin (+ phase 2.094))))))
        b0 (int (* 255.0 (+ 0.5 (* 0.5 (sin (+ phase 4.188))))))
        bg 20
        r (if (= on 1) r0 bg)
        g (if (= on 1) g0 bg)
        b (if (= on 1) b0 (+ bg 10))]
    (+ (* 65536 r) (* 256 g) b)))
