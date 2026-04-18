;; 2D cloud layer raymarched in 3D. We treat clouds as a flat slab at
;; a fixed altitude, with 2D FBM noise driving density. The view ray
;; intersects the slab, then we step through it accumulating density
;; and a single sun-shadow probe per step. Cheaper than full 3D noise,
;; still reads as volumetric.
;;
;; Sliders:
;;   s0  cloud coverage   0..1000 -> -0.2..0.5 density threshold
;;   s1  wind speed       0..1000 -> 0..2 units/sec
;;   s2  sun azimuth      0..1000 -> 0..2pi
;;   s3  view tilt        0..1000 -> -0.1..0.4 radians

;; Hash 2D integer coords to [0,1). Uses 31-bit multipliers so every
;; literal fits in i32 (our emitter's integer literal path).
(defmacro hash2 [ix iy]
  `(let [n (+ (* (u32 ~ix) (u32 73856093))
              (* (u32 ~iy) (u32 19349663)))
         x (bit-xor n (u32 61))
         x (bit-xor x (bit-shift-right x (u32 16)))
         x (* x (u32 668265261))
         x (bit-xor x (bit-shift-right x (u32 13)))
         x (* x (u32 374761393))
         x (bit-xor x (bit-shift-right x (u32 16)))]
     (/ (f32 (bit-and x (u32 16777215))) 16777215.0)))

;; 2D value noise with smoothstep interpolation.
(defmacro noise2 [xf yf]
  `(let [fx ~xf
         fy ~yf
         ix (i32 (floor fx))
         iy (i32 (floor fy))
         tx (- fx (floor fx))
         ty (- fy (floor fy))
         wx (* (* tx tx) (- 3.0 (* 2.0 tx)))
         wy (* (* ty ty) (- 3.0 (* 2.0 ty)))
         h00 (hash2 ix iy)
         h10 (hash2 (+ ix 1) iy)
         h01 (hash2 ix (+ iy 1))
         h11 (hash2 (+ ix 1) (+ iy 1))
         a   (+ h00 (* wx (- h10 h00)))
         b   (+ h01 (* wx (- h11 h01)))]
     (+ a (* wy (- b a)))))

;; 5-octave 2D FBM, normalized to ~[0,1].
(defmacro fbm2 [xf yf]
  `(let [n1 (noise2 ~xf ~yf)
         n2 (noise2 (* ~xf 2.07) (* ~yf 2.07))
         n3 (noise2 (* ~xf 4.13) (* ~yf 4.13))
         n4 (noise2 (* ~xf 8.19) (* ~yf 8.19))
         n5 (noise2 (* ~xf 16.3) (* ~yf 16.3))
         s  (+ (+ (* n1 0.5) (* n2 0.25))
               (+ (* n3 0.125) (+ (* n4 0.0625) (* n5 0.03125))))]
     (/ s 0.96875)))

(defn-gpu-pixel render
  [x y w h t-ms s0 s1 s2 s3]
  (let [t       (* 0.001 (f32 t-ms))
        cov     (- (* 0.7 (/ (f32 s0) 1000.0)) 0.2)
        wind    (* 2.0 (/ (f32 s1) 1000.0))
        sun-az  (* 6.2831853 (/ (f32 s2) 1000.0))
        tilt    (- (* 0.5 (/ (f32 s3) 1000.0)) 0.1)

        aspect (/ (f32 w) (f32 h))
        uv-x   (* aspect (- (* 2.0 (/ (f32 x) (f32 w))) 1.0))
        uv-y   (- 1.0 (* 2.0 (/ (f32 y) (f32 h))))

        ;; Forward ray from origin. Slight upward tilt selectable via slider.
        dx     uv-x
        dy     (+ uv-y tilt)
        dz     1.4
        dl     (sqrt (+ (+ (* dx dx) (* dy dy)) (* dz dz)))
        dx     (/ dx dl)
        dy     (/ dy dl)
        dz     (/ dz dl)

        ;; Sun direction.
        sel    0.18
        sx     (* (cos sel) (cos sun-az))
        sz     (* (cos sel) (sin sun-az))
        sy     (sin sel)

        ;; Cloud slab between y=2 and y=3.5.
        slab-lo 2.0
        slab-hi 3.5

        ;; Find slab entry/exit along the ray. Skip if dy too small.
        valid?  (> dy 0.01)
        t-lo    (if valid? (/ (- slab-lo 0.0) dy) 0.0)
        t-hi    (if valid? (/ (- slab-hi 0.0) dy) 0.0)
        t-near  (max 0.05 t-lo)
        t-far   (min 60.0 t-hi)
        steps   (i32 32)
        wind-x  (* wind t)

        ;; Accumulate transmittance and emitted color along the ray.
        ;; We need three returns (trans, lit-color), so loop returns one
        ;; value packed: we use 1-trans as alpha and reconstruct color
        ;; outside via density-weighted average. Simpler: return trans
        ;; here, compute lit color in a second pass below.
        trans (if valid?
                (loop [tt    t-near
                       trans 1.0
                       step-i (i32 0)]
                  (if (>= step-i steps) trans
                    (if (>= tt t-far) trans
                      (if (< trans 0.02) trans
                        (let [px (+ (* dx tt) wind-x)
                              py (* dy tt)
                              pz (* dz tt)
                              v  (max 0.0 (- (fbm2 (* 0.4 px) (* 0.4 pz)) cov))
                              ;; Vertical density falloff at slab edges.
                              h-mid (* 0.5 (+ slab-lo slab-hi))
                              h-fade (max 0.0
                                       (- 1.0 (abs (* (- py h-mid) (/ 1.0 0.75)))))
                              dens (* v h-fade)
                              dt   (/ (- t-far t-near) (f32 steps))
                              a    (- 1.0 (exp (- (* dens dt 4.0))))]
                          (recur (+ tt dt)
                                 (* trans (- 1.0 a))
                                 (+ step-i (i32 1))))))))
                1.0)
        ;; Second pass: estimate light color contribution using shadow probe
        ;; at cloud center. Cheap, correlates with the trans drop.
        center-t (* 0.5 (+ t-near t-far))
        cpx (+ (* dx center-t) wind-x)
        cpz (* dz center-t)
        l-raw (fbm2 (+ (* 0.4 cpx) (* sx 1.5))
                    (+ (* 0.4 cpz) (* sz 1.5)))
        l-dens (max 0.0 (- l-raw cov))
        shadow (exp (- (* l-dens 3.5)))
        ;; Backscatter glow when looking near the sun.
        sun-dot (max 0.0 (+ (+ (* dx sx) (* dy sy)) (* dz sz)))
        glow    (pow sun-dot 16.0)
        cloud-r (+ (* 0.95 shadow) (* 0.4 glow))
        cloud-g (+ (* 0.85 shadow) (* 0.3 glow))
        cloud-b (+ (* 0.78 shadow) (* 0.2 glow))
        alpha   (- 1.0 trans)

        ;; Sky gradient.
        sky-t  (max 0.0 (min 1.0 (+ 0.5 (* 0.5 dy))))
        sky-r  (+ (* (- 1.0 sky-t) 0.95) (* sky-t 0.30))
        sky-g  (+ (* (- 1.0 sky-t) 0.72) (* sky-t 0.55))
        sky-b  (+ (* (- 1.0 sky-t) 0.55) (* sky-t 0.85))

        out-r (+ (* trans sky-r) (* alpha cloud-r))
        out-g (+ (* trans sky-g) (* alpha cloud-g))
        out-b (+ (* trans sky-b) (* alpha cloud-b))

        ri  (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 out-r)))))
        gi  (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 out-g)))))
        bi  (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 out-b)))))]
    (bit-or (bit-or (bit-shift-left ri (u32 16)) (bit-shift-left gi (u32 8))) bi)))
