;; Signed-distance-field scene with smooth union, box + sphere + torus +
;; ground, distance-based fog, and soft ambient occlusion approximated
;; from the march step count.
;;
;; The secret is `smin` (polynomial smooth min) — it blends two SDFs
;; into a single smooth surface where they'd otherwise meet at a sharp
;; corner. Slide "blend radius" to see shapes melt together.

(def slider-0-label "blend radius")
(def slider-1-label "rotation speed")
(def slider-2-label "fog density")
(def slider-3-label "AO strength")

;; Polynomial smooth min — iq's formula.
(defn-native smin ^f64 [^f64 a ^f64 b ^f64 k]
  (let [h (max 0.0 (- k (abs (- a b))))
        ratio (/ h k)]
    (- (min a b) (* 0.25 (* k (* ratio ratio))))))

;; Distance to a sphere centered at (cx, cy, cz) with radius r.
(defn-native sd-sphere ^f64
  [^f64 px ^f64 py ^f64 pz ^f64 cx ^f64 cy ^f64 cz ^f64 r]
  (let [dx (- px cx) dy (- py cy) dz (- pz cz)]
    (- (sqrt (+ (+ (* dx dx) (* dy dy)) (* dz dz))) r)))

;; Distance to a cube of half-size `s` centered at (cx, cy, cz).
(defn-native sd-box ^f64
  [^f64 px ^f64 py ^f64 pz ^f64 cx ^f64 cy ^f64 cz ^f64 s]
  (let [qx (- (abs (- px cx)) s)
        qy (- (abs (- py cy)) s)
        qz (- (abs (- pz cz)) s)
        outside (sqrt (+ (+ (* (max qx 0.0) (max qx 0.0))
                            (* (max qy 0.0) (max qy 0.0)))
                         (* (max qz 0.0) (max qz 0.0))))
        inside (min 0.0 (max qx (max qy qz)))]
    (+ outside inside)))

;; Torus in XZ plane around (cx, cy, cz), major R, minor r.
(defn-native sd-torus ^f64
  [^f64 px ^f64 py ^f64 pz ^f64 cx ^f64 cy ^f64 cz ^f64 big-r ^f64 little-r]
  (let [dx (- px cx) dy (- py cy) dz (- pz cz)
        q (- (sqrt (+ (* dx dx) (* dz dz))) big-r)]
    (- (sqrt (+ (* q q) (* dy dy))) little-r)))

;; Scene: smooth union of a sphere, a box, and a torus, sitting on a
;; ground plane at y = 0.
(defn-native scene-sdf ^f64
  [^f64 x ^f64 y ^f64 z ^f64 t ^f64 blend]
  (let [;; Sphere bobs and pans side to side.
        sx (* 1.2 (sin t))
        sy (+ 1.1 (* 0.3 (abs (sin (* t 2.0)))))
        sz (* 0.8 (cos t))
        ds (sd-sphere x y z sx sy sz 0.8)
        ;; Box rotates around in the other direction.
        bx (* -1.2 (sin (+ t 1.0)))
        by 0.9
        bz (* 0.5 (cos (+ t 1.0)))
        db (sd-box x y z bx by bz 0.55)
        ;; Torus higher up, orbiting.
        tx (* 0.7 (cos (* t 0.7)))
        ty (+ 1.8 (* 0.15 (sin (* t 1.5))))
        tz (* 0.7 (sin (* t 0.7)))
        dt (sd-torus x y z tx ty tz 0.9 0.25)
        ;; Smooth union of all three.
        d1 (smin ds db blend)
        d2 (smin d1 dt blend)
        ;; Ground plane at y=0, union (hard).
        dground y]
    (min d2 dground)))

;; Raymarcher returns (t, last-d, steps) packed. Native only returns
;; one value, so pack as i64: steps + 10000*int(t*100). Keep it simple —
;; separate fns.
(defn-native march-t ^f64
  [^f64 ox ^f64 oy ^f64 oz ^f64 dx ^f64 dy ^f64 dz ^f64 time ^f64 blend]
  (loop [step 0 t 0.0]
    (if (>= step 200)
      100.0
      (let [d (scene-sdf (+ ox (* t dx)) (+ oy (* t dy)) (+ oz (* t dz)) time blend)]
        (if (< d 0.002)
          t
          (if (> t 50.0)
            100.0
            (recur (+ step 1) (+ t d))))))))

;; Step count reached — proxy for ambient occlusion, since grazing rays
;; burn more steps than direct hits.
(defn-native march-steps ^i64
  [^f64 ox ^f64 oy ^f64 oz ^f64 dx ^f64 dy ^f64 dz ^f64 time ^f64 blend]
  (loop [step 0 t 0.0]
    (if (>= step 200)
      200
      (let [d (scene-sdf (+ ox (* t dx)) (+ oy (* t dy)) (+ oz (* t dz)) time blend)]
        (if (< d 0.002)
          step
          (if (> t 50.0)
            200
            (recur (+ step 1) (+ t d))))))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width  960.0
        height 540.0
        blend  (+ 0.1 (* 0.9 (/ (float s0) 1000.0)))
        rot-sp (+ 0.3 (* 2.0 (/ (float s1) 1000.0)))
        fog-d  (+ 0.01 (* 0.1 (/ (float s2) 1000.0)))
        ao-k   (/ (float s3) 1000.0)
        time   (* rot-sp (/ (float t-ms) 1000.0))
        fx (float px)
        fy (float py)
        aspect (/ width height)
        nx (* (- (/ (* 2.0 fx) width)  1.0) aspect)
        ny (- 1.0 (/ (* 2.0 fy) height))
        rx nx ry ny rz 2.0
        rlen (sqrt (+ (+ (* rx rx) (* ry ry)) (* rz rz)))
        dx (/ rx rlen) dy (/ ry rlen) dz (/ rz rlen)
        ox (* 3.5 (cos (* time 0.3)))
        oy 2.5
        oz (* 3.5 (sin (* time 0.3)))
        ;; Point camera at origin (subtract the camera position from target).
        tgx (- ox) tgy (- 1.5 oy) tgz (- oz)
        tgm (sqrt (+ (+ (* tgx tgx) (* tgy tgy)) (* tgz tgz)))
        ctx (/ tgx tgm) cty (/ tgy tgm) ctz (/ tgz tgm)
        ;; Simple camera basis: use up=(0,1,0). right = up × forward.
        rgx (- (* 0.0 ctz) (* 1.0 0.0))   ;; right_x = up_y*fwd_z - up_z*fwd_y = ctz
        rgx-2 ctz
        rgz-2 (- ctx)
        ;; Compose ray in camera basis.
        cam-dx (+ (* dx rgx-2) (* dz rgz-2))
        cam-dy dy
        cam-dz (- (+ (* dx ctx) (* dz ctz)))
        cm (sqrt (+ (+ (* cam-dx cam-dx) (* cam-dy cam-dy)) (* cam-dz cam-dz)))
        fdx (/ cam-dx cm) fdy (/ cam-dy cm) fdz (/ cam-dz cm)
        t (march-t ox oy oz fdx fdy fdz time blend)
        steps (march-steps ox oy oz fdx fdy fdz time blend)]
    (if (>= t 100.0)
      ;; Sky.
      (let [sky-t (* 0.5 (+ 1.0 fdy))
            r (int (* 255.0 (+ 0.3 (* 0.3 sky-t))))
            g (int (* 255.0 (+ 0.4 (* 0.3 sky-t))))
            b (int (* 255.0 (+ 0.7 (* 0.2 sky-t))))]
        (+ (* 65536 r) (* 256 g) b))
      ;; Shade.
      (let [eps 0.001
            hx (+ ox (* t fdx))
            hy (+ oy (* t fdy))
            hz (+ oz (* t fdz))
            nxp (scene-sdf (+ hx eps) hy hz time blend)
            nxm (scene-sdf (- hx eps) hy hz time blend)
            nyp (scene-sdf hx (+ hy eps) hz time blend)
            nym (scene-sdf hx (- hy eps) hz time blend)
            nzp (scene-sdf hx hy (+ hz eps) time blend)
            nzm (scene-sdf hx hy (- hz eps) time blend)
            gx (- nxp nxm) gy (- nyp nym) gz (- nzp nzm)
            glen (sqrt (+ (+ (* gx gx) (* gy gy)) (* gz gz)))
            n0x (/ gx glen) n0y (/ gy glen) n0z (/ gz glen)
            ;; Fixed light direction.
            llen (sqrt (+ (+ 1.0 1.0) 0.25))
            lx (/ 1.0 llen) ly (/ 1.0 llen) lz (/ 0.5 llen)
            lambert (max 0.0 (+ (+ (* n0x lx) (* n0y ly)) (* n0z lz)))
            ;; AO from step count.
            ao (- 1.0 (* ao-k (/ (float steps) 200.0)))
            ;; Fog.
            fog (exp (- (* fog-d t)))
            ;; Base: dim blue below (ground), warm on other objects.
            is-ground (if (< hy 0.02) 1 0)
            base-r (if (= is-ground 1) 90 230)
            base-g (if (= is-ground 1) 110 180)
            base-b (if (= is-ground 1) 140 110)
            shade (* ao (+ 0.2 (* 0.8 lambert)))
            lit-r (* (float base-r) shade)
            lit-g (* (float base-g) shade)
            lit-b (* (float base-b) shade)
            ;; Blend toward sky color via fog.
            fog-r 75.0 fog-g 100.0 fog-b 175.0
            r (int (+ (* fog lit-r) (* (- 1.0 fog) fog-r)))
            g (int (+ (* fog lit-g) (* (- 1.0 fog) fog-g)))
            b (int (+ (* fog lit-b) (* (- 1.0 fog) fog-b)))
            rc (max 0 (min 255 r))
            gc (max 0 (min 255 g))
            bc (max 0 (min 255 b))]
        (+ (* 65536 rc) (* 256 gc) bc)))))
