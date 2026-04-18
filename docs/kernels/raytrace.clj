;; Whitted-style ray tracer. Analytic ray-sphere and ray-plane
;; intersections, shadow rays, iterative reflection up to 3 bounces.
;; Scene: 4 colored spheres above a checker floor. Runs hundreds of fps.
;;
;; Sliders:
;;   s0  camera orbit speed   0..1000 -> 0..1 rad/sec
;;   s1  orbit radius         0..1000 -> 3..9
;;   s2  sun azimuth          0..1000 -> 0..2pi
;;   s3  extra reflectivity   0..1000 -> 0..1 on every material

;; Closed-form ray-sphere hit distance. Returns a large sentinel on miss.
(defmacro sphere-hit [ox oy oz dx dy dz cx cy cz rad miss]
  `(let [oxc (- ~ox ~cx)
         oyc (- ~oy ~cy)
         ozc (- ~oz ~cz)
         bh  (+ (+ (* oxc ~dx) (* oyc ~dy)) (* ozc ~dz))
         ch  (- (+ (+ (* oxc oxc) (* oyc oyc)) (* ozc ozc))
                (* ~rad ~rad))
         disc (- (* bh bh) ch)]
     (if (< disc 0.0)
       ~miss
       (let [sq (sqrt disc)
             t1 (- (- bh) sq)
             t2 (+ (- bh) sq)]
         (if (> t1 0.001) t1
           (if (> t2 0.001) t2 ~miss))))))

(defmacro plane-hit [oy dy miss]
  `(if (> (abs ~dy) 0.0001)
     (let [tt (/ (- ~oy) ~dy)]
       (if (> tt 0.001) tt ~miss))
     ~miss))

;; Scene intersection: returns the nearest hit distance across 4 spheres
;; and a Y=0 floor. The trace macro inlines these and picks the min.
(defmacro scene-hit-nearest [ox oy oz dx dy dz miss]
  `(let [t-s1 (sphere-hit ~ox ~oy ~oz ~dx ~dy ~dz 0.0 0.75 0.0 0.75 ~miss)
         t-s2 (sphere-hit ~ox ~oy ~oz ~dx ~dy ~dz 1.7 0.50 1.0 0.50 ~miss)
         t-s3 (sphere-hit ~ox ~oy ~oz ~dx ~dy ~dz (- 1.7) 0.50 1.0 0.50 ~miss)
         t-s4 (sphere-hit ~ox ~oy ~oz ~dx ~dy ~dz 0.0 0.35 (- 1.8) 0.35 ~miss)
         t-fl (plane-hit ~oy ~dy ~miss)]
     (min (min (min t-s1 t-s2) (min t-s3 t-s4)) t-fl)))

;; One shading step. Given a ray origin/direction that hit something,
;; returns (shaded-scalar-channel, reflected-dir, reflection-strength).
;; We shade three scalar channels in sequence to avoid vec3 plumbing.

;; Trace one ray and produce a single color channel (R, G, or B).
;; `channel` is 0/1/2 selecting which of the three albedos to use.
;; This is run three times per pixel (once per channel). GPU work is
;; cheap; this keeps the DSL simple at the cost of ~3x the arithmetic.

(defmacro trace-channel [channel ox0 oy0 oz0 dx0 dy0 dz0 sx sy sz refl-k]
  `(let [miss  1.0e9
         ;; Bounce 0.
         ox1 ~ox0 oy1 ~oy0 oz1 ~oz0
         dx1 ~dx0 dy1 ~dy0 dz1 ~dz0
         t1  (scene-hit-nearest ox1 oy1 oz1 dx1 dy1 dz1 miss)
         hit1? (< t1 (* 0.5 miss))
         hx1 (+ ox1 (* dx1 t1))
         hy1 (+ oy1 (* dy1 t1))
         hz1 (+ oz1 (* dz1 t1))
         shade1 (if hit1?
                  (shade-hit ~channel hx1 hy1 hz1 dx1 dy1 dz1 ~sx ~sy ~sz ~refl-k)
                  (sky ~channel dy1))
         refl1  (if hit1? (material-refl hx1 hy1 hz1 ~refl-k) 0.0)
         ;; Reflected direction from the first hit.
         nx1 (hit-nx hx1 hy1 hz1)
         ny1 (hit-ny hx1 hy1 hz1)
         nz1 (hit-nz hx1 hy1 hz1)
         ddn1 (+ (+ (* dx1 nx1) (* dy1 ny1)) (* dz1 nz1))
         rx1 (- dx1 (* (* 2.0 ddn1) nx1))
         ry1 (- dy1 (* (* 2.0 ddn1) ny1))
         rz1 (- dz1 (* (* 2.0 ddn1) nz1))

         ;; Bounce 1 (only matters if refl1 > 0).
         ox2 (+ hx1 (* rx1 0.001))
         oy2 (+ hy1 (* ry1 0.001))
         oz2 (+ hz1 (* rz1 0.001))
         t2  (scene-hit-nearest ox2 oy2 oz2 rx1 ry1 rz1 miss)
         hit2? (< t2 (* 0.5 miss))
         hx2 (+ ox2 (* rx1 t2))
         hy2 (+ oy2 (* ry1 t2))
         hz2 (+ oz2 (* rz1 t2))
         shade2 (if hit2?
                  (shade-hit ~channel hx2 hy2 hz2 rx1 ry1 rz1 ~sx ~sy ~sz ~refl-k)
                  (sky ~channel ry1))
         refl2  (if hit2? (material-refl hx2 hy2 hz2 ~refl-k) 0.0)
         nx2 (hit-nx hx2 hy2 hz2)
         ny2 (hit-ny hx2 hy2 hz2)
         nz2 (hit-nz hx2 hy2 hz2)
         ddn2 (+ (+ (* rx1 nx2) (* ry1 ny2)) (* rz1 nz2))
         rx2 (- rx1 (* (* 2.0 ddn2) nx2))
         ry2 (- ry1 (* (* 2.0 ddn2) ny2))
         rz2 (- rz1 (* (* 2.0 ddn2) nz2))

         ;; Bounce 2.
         ox3 (+ hx2 (* rx2 0.001))
         oy3 (+ hy2 (* ry2 0.001))
         oz3 (+ hz2 (* rz2 0.001))
         t3  (scene-hit-nearest ox3 oy3 oz3 rx2 ry2 rz2 miss)
         hit3? (< t3 (* 0.5 miss))
         hx3 (+ ox3 (* rx2 t3))
         hy3 (+ oy3 (* ry2 t3))
         hz3 (+ oz3 (* rz2 t3))
         shade3 (if hit3?
                  (shade-hit ~channel hx3 hy3 hz3 rx2 ry2 rz2 ~sx ~sy ~sz ~refl-k)
                  (sky ~channel ry2))

         ;; Composite: local + refl*(local2 + refl2*local3).
         comp2 (+ (* (- 1.0 refl2) shade2) (* refl2 shade3))
         comp1 (+ (* (- 1.0 refl1) shade1) (* refl1 comp2))]
     comp1))

;; Sky color (single channel).
(defmacro sky [channel dy]
  `(let [t (max 0.0 (min 1.0 (* 0.5 (+ 1.0 ~dy))))]
     (if (= ~channel (i32 0))
       (+ (* (- 1.0 t) 0.55) (* t 0.12))
       (if (= ~channel (i32 1))
         (+ (* (- 1.0 t) 0.75) (* t 0.30))
         (+ (* (- 1.0 t) 0.95) (* t 0.55))))))

;; Given a hit point, pick which primitive was hit by testing distance
;; to each sphere center. Picks the one whose surface the point lies on.
(defmacro which-obj [hx hy hz]
  `(let [da (abs (- (sqrt (+ (+ (* ~hx ~hx) (* (- ~hy 0.75) (- ~hy 0.75))) (* ~hz ~hz))) 0.75))
         db (abs (- (sqrt (+ (+ (* (- ~hx 1.7) (- ~hx 1.7)) (* (- ~hy 0.5) (- ~hy 0.5))) (* (- ~hz 1.0) (- ~hz 1.0)))) 0.5))
         dc (abs (- (sqrt (+ (+ (* (+ ~hx 1.7) (+ ~hx 1.7)) (* (- ~hy 0.5) (- ~hy 0.5))) (* (- ~hz 1.0) (- ~hz 1.0)))) 0.5))
         dd (abs (- (sqrt (+ (+ (* ~hx ~hx) (* (- ~hy 0.35) (- ~hy 0.35))) (* (+ ~hz 1.8) (+ ~hz 1.8)))) 0.35))
         df (abs ~hy)]
     ;; 0=floor, 1=s1, 2=s2, 3=s3, 4=s4
     (if (< df (min (min da db) (min dc dd))) (i32 0)
       (if (< da (min (min db dc) dd)) (i32 1)
         (if (< db (min dc dd)) (i32 2)
           (if (< dc dd) (i32 3) (i32 4)))))))

(defmacro hit-nx [hx hy hz]
  `(let [o (which-obj ~hx ~hy ~hz)]
     (if (= o (i32 0)) 0.0
       (if (= o (i32 1)) (/ ~hx 0.75)
         (if (= o (i32 2)) (/ (- ~hx 1.7) 0.5)
           (if (= o (i32 3)) (/ (- ~hx (- 1.7)) 0.5)
             (/ ~hx 0.35)))))))

(defmacro hit-ny [hx hy hz]
  `(let [o (which-obj ~hx ~hy ~hz)]
     (if (= o (i32 0)) 1.0
       (if (= o (i32 1)) (/ (- ~hy 0.75) 0.75)
         (if (= o (i32 2)) (/ (- ~hy 0.5) 0.5)
           (if (= o (i32 3)) (/ (- ~hy 0.5) 0.5)
             (/ (- ~hy 0.35) 0.35)))))))

(defmacro hit-nz [hx hy hz]
  `(let [o (which-obj ~hx ~hy ~hz)]
     (if (= o (i32 0)) 0.0
       (if (= o (i32 1)) (/ ~hz 0.75)
         (if (= o (i32 2)) (/ (- ~hz 1.0) 0.5)
           (if (= o (i32 3)) (/ (- ~hz 1.0) 0.5)
             (/ (- ~hz (- 1.8)) 0.35)))))))

(defmacro material-refl [hx hy hz refl-k]
  `(let [o (which-obj ~hx ~hy ~hz)]
     (if (= o (i32 0)) (* ~refl-k 0.15)
       (if (= o (i32 1)) (+ 0.35 (* ~refl-k 0.3))
         (if (= o (i32 2)) (+ 0.15 (* ~refl-k 0.3))
           (if (= o (i32 3)) (+ 0.70 (* ~refl-k 0.3))
             (+ 0.20 (* ~refl-k 0.3))))))))

(defmacro albedo [channel hx hy hz]
  `(let [o (which-obj ~hx ~hy ~hz)]
     (if (= o (i32 0))
       (let [cxi (i32 (floor (* ~hx 0.6)))
             czi (i32 (floor (* ~hz 0.6)))]
         (if (= (mod (+ cxi czi) (i32 2)) (i32 0)) 0.85 0.20))
       (if (= ~channel (i32 0))
         (if (= o (i32 1)) 0.92 (if (= o (i32 2)) 0.30 (if (= o (i32 3)) 0.35 0.95)))
         (if (= ~channel (i32 1))
           (if (= o (i32 1)) 0.30 (if (= o (i32 2)) 0.85 (if (= o (i32 3)) 0.55 0.80)))
           (if (= o (i32 1)) 0.30 (if (= o (i32 2)) 0.40 (if (= o (i32 3)) 0.95 0.25))))))))

(defmacro shade-hit [channel hx hy hz dx dy dz sx sy sz refl-k]
  `(let [nx (hit-nx ~hx ~hy ~hz)
         ny (hit-ny ~hx ~hy ~hz)
         nz (hit-nz ~hx ~hy ~hz)
         ;; Shadow ray toward sun.
         sox (+ ~hx (* nx 0.01))
         soy (+ ~hy (* ny 0.01))
         soz (+ ~hz (* nz 0.01))
         sh  (scene-hit-nearest sox soy soz ~sx ~sy ~sz 1.0e9)
         shadow (if (< sh 1.0e8) 0.25 1.0)
         ndotl (max 0.0 (+ (+ (* nx ~sx) (* ny ~sy)) (* nz ~sz)))
         amb   0.22
         lit   (+ amb (* (* 0.85 shadow) ndotl))
         a     (albedo ~channel ~hx ~hy ~hz)]
     (* lit a)))

(defn-gpu-pixel render
  [x y w h t-ms s0 s1 s2 s3]
  (let [t       (* 0.001 (f32 t-ms))
        orbit-s (* 1.0 (/ (f32 s0) 1000.0))
        orbit-r (+ 3.0 (* 6.0 (/ (f32 s1) 1000.0)))
        sun-az  (* 6.2831853 (/ (f32 s2) 1000.0))
        refl-k  (/ (f32 s3) 1000.0)

        aspect (/ (f32 w) (f32 h))
        uv-x   (* aspect (- (* 2.0 (/ (f32 x) (f32 w))) 1.0))
        uv-y   (- 1.0 (* 2.0 (/ (f32 y) (f32 h))))

        ca     (* t orbit-s)
        cam-x  (* orbit-r (sin ca))
        cam-y  2.0
        cam-z  (* orbit-r (cos ca))
        tgt-x  0.0
        tgt-y  0.7
        tgt-z  0.0

        lx (- tgt-x cam-x) ly (- tgt-y cam-y) lz (- tgt-z cam-z)
        ll (sqrt (+ (+ (* lx lx) (* ly ly)) (* lz lz)))
        fx (/ lx ll) fy (/ ly ll) fz (/ lz ll)

        rx0 fz ry0 0.0 rz0 (- fx)
        rl  (max 0.0001 (sqrt (+ (+ (* rx0 rx0) (* ry0 ry0)) (* rz0 rz0))))
        rx  (/ rx0 rl) ry  (/ ry0 rl) rz  (/ rz0 rl)
        ux  (- (* fy rz) (* fz ry))
        uy  (- (* fz rx) (* fx rz))
        uz  (- (* fx ry) (* fy rx))

        fov 1.3
        dx0 (+ (+ (* fx fov) (* rx uv-x)) (* ux uv-y))
        dy0 (+ (+ (* fy fov) (* ry uv-x)) (* uy uv-y))
        dz0 (+ (+ (* fz fov) (* rz uv-x)) (* uz uv-y))
        dl  (sqrt (+ (+ (* dx0 dx0) (* dy0 dy0)) (* dz0 dz0)))
        dx  (/ dx0 dl) dy  (/ dy0 dl) dz  (/ dz0 dl)

        sel 0.85
        sx  (* (cos sel) (cos sun-az))
        sz  (* (cos sel) (sin sun-az))
        sy  (sin sel)

        r (trace-channel (i32 0) cam-x cam-y cam-z dx dy dz sx sy sz refl-k)
        g (trace-channel (i32 1) cam-x cam-y cam-z dx dy dz sx sy sz refl-k)
        b (trace-channel (i32 2) cam-x cam-y cam-z dx dy dz sx sy sz refl-k)

        ri  (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 r)))))
        gi  (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 g)))))
        bi  (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 b)))))]
    (bit-or (bit-or (bit-shift-left ri (u32 16)) (bit-shift-left gi (u32 8))) bi)))
