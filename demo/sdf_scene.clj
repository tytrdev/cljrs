;; Signed-distance-field scene with smooth union, box + sphere + torus +
;; ground, distance-based fog, and soft ambient occlusion approximated
;; from the march step count.
;;
;; Perf note: the previous version called `scene-sdf` ~206× per pixel
;; (200 march steps + 6 central-difference normal samples), and every
;; call recomputed all three object positions via sin/cos(t). Now the
;; primitive positions are computed ONCE at the top of render-pixel and
;; threaded through march-t / march-steps / scene-sdf as args. Trig
;; drops from ~1600 calls per pixel to 8.

(def slider-0-label "blend radius")
(def slider-1-label "rotation speed")
(def slider-2-label "fog density")
(def slider-3-label "AO strength")

(defn-native smin ^f64 [^f64 a ^f64 b ^f64 k]
  (let [h (max 0.0 (- k (abs (- a b))))
        ratio (/ h k)]
    (- (min a b) (* 0.25 (* k (* ratio ratio))))))

(defn-native sd-sphere ^f64
  [^f64 px ^f64 py ^f64 pz ^f64 cx ^f64 cy ^f64 cz ^f64 r]
  (let [dx (- px cx) dy (- py cy) dz (- pz cz)]
    (- (sqrt (+ (+ (* dx dx) (* dy dy)) (* dz dz))) r)))

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

(defn-native sd-torus ^f64
  [^f64 px ^f64 py ^f64 pz ^f64 cx ^f64 cy ^f64 cz ^f64 big-r ^f64 little-r]
  (let [dx (- px cx) dy (- py cy) dz (- pz cz)
        q (- (sqrt (+ (* dx dx) (* dz dz))) big-r)]
    (- (sqrt (+ (* q q) (* dy dy))) little-r)))

;; Scene: smooth union of sphere + box + torus + ground plane at y=0.
;; Object positions are taken as args so they can be precomputed once
;; per pixel instead of once per march step.
(defn-native scene-sdf ^f64
  [^f64 x ^f64 y ^f64 z
   ^f64 sx ^f64 sy ^f64 sz
   ^f64 bx ^f64 bz
   ^f64 tx ^f64 ty ^f64 tz
   ^f64 blend]
  (let [ds (sd-sphere x y z sx sy sz 0.8)
        db (sd-box x y z bx 0.9 bz 0.55)
        dt (sd-torus x y z tx ty tz 0.9 0.25)
        d1 (smin ds db blend)
        d2 (smin d1 dt blend)]
    (min d2 y)))

(defn-native march-t ^f64
  [^f64 ox ^f64 oy ^f64 oz ^f64 dx ^f64 dy ^f64 dz
   ^f64 sx ^f64 sy ^f64 sz
   ^f64 bx ^f64 bz
   ^f64 tx ^f64 ty ^f64 tz
   ^f64 blend]
  (loop [step 0 t 0.0]
    (if (>= step 128)
      100.0
      (let [d (scene-sdf (+ ox (* t dx)) (+ oy (* t dy)) (+ oz (* t dz))
                          sx sy sz bx bz tx ty tz blend)]
        (if (< d 0.002)
          t
          (if (> t 50.0)
            100.0
            (recur (+ step 1) (+ t d))))))))

;; NOTE: an earlier version of this demo also had a `march-steps` that
;; repeated the entire march a second time per pixel just to read the
;; step count for an ambient-occlusion fudge. That doubled the per-frame
;; work. The new version derives a cheap AO proxy from the hit distance
;; `t` instead — closer surfaces are darker, since short marches imply
;; deep cavities in screen space. Same rough feel at half the cost.

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
        ;; Precompute all time-dependent primitive positions ONCE per pixel.
        sx (* 1.2 (sin time))
        sy (+ 1.1 (* 0.3 (abs (sin (* time 2.0)))))
        sz (* 0.8 (cos time))
        bx (* -1.2 (sin (+ time 1.0)))
        bz (* 0.5  (cos (+ time 1.0)))
        tx (* 0.7 (cos (* time 0.7)))
        ty (+ 1.8 (* 0.15 (sin (* time 1.5))))
        tz (* 0.7 (sin (* time 0.7)))
        fx (float px)
        fy (float py)
        aspect (/ width height)
        nx (* (- (/ (* 2.0 fx) width)  1.0) aspect)
        ny (- 1.0 (/ (* 2.0 fy) height))
        ;; Fixed camera looking at roughly (0, 1, 0) from behind and above.
        ;; Previous orbit transform was rotating the ray the wrong way and
        ;; sending ~every pixel ray off into empty sky. Scene objects live
        ;; in a box roughly [-1.2, 1.2] x [0, 2] x [-1.2, 1.2].
        ox 0.0
        oy 2.0
        oz -5.0
        pitch 0.3
        rx nx
        ry (- ny pitch)
        rz 1.0
        rlen (sqrt (+ (+ (* rx rx) (* ry ry)) (* rz rz)))
        cam-dx (/ rx rlen)
        cam-dy (/ ry rlen)
        cam-dz (/ rz rlen)
        t     (march-t ox oy oz cam-dx cam-dy cam-dz sx sy sz bx bz tx ty tz blend)]
    (if (>= t 100.0)
      (let [sky-t (* 0.5 (+ 1.0 cam-dy))
            r (int (* 255.0 (+ 0.3 (* 0.3 sky-t))))
            g (int (* 255.0 (+ 0.4 (* 0.3 sky-t))))
            b (int (* 255.0 (+ 0.7 (* 0.2 sky-t))))]
        (+ (* 65536 r) (* 256 g) b))
      (let [eps 0.001
            hx (+ ox (* t cam-dx))
            hy (+ oy (* t cam-dy))
            hz (+ oz (* t cam-dz))
            nxp (scene-sdf (+ hx eps) hy hz sx sy sz bx bz tx ty tz blend)
            nxm (scene-sdf (- hx eps) hy hz sx sy sz bx bz tx ty tz blend)
            nyp (scene-sdf hx (+ hy eps) hz sx sy sz bx bz tx ty tz blend)
            nym (scene-sdf hx (- hy eps) hz sx sy sz bx bz tx ty tz blend)
            nzp (scene-sdf hx hy (+ hz eps) sx sy sz bx bz tx ty tz blend)
            nzm (scene-sdf hx hy (- hz eps) sx sy sz bx bz tx ty tz blend)
            gx (- nxp nxm) gy (- nyp nym) gz (- nzp nzm)
            glen (sqrt (+ (+ (* gx gx) (* gy gy)) (* gz gz)))
            n0x (/ gx glen) n0y (/ gy glen) n0z (/ gz glen)
            llen (sqrt (+ (+ 1.0 1.0) 0.25))
            lx (/ 1.0 llen) ly (/ 1.0 llen) lz (/ 0.5 llen)
            lambert (max 0.0 (+ (+ (* n0x lx) (* n0y ly)) (* n0z lz)))
            ;; Cheap AO proxy: close-in hits (short t) are darker, distant
            ;; hits brighter. Not physically motivated but cheap.
            ao (- 1.0 (* ao-k (max 0.0 (min 1.0 (/ 5.0 (+ t 1.0))))))
            fog (exp (- (* fog-d t)))
            is-ground (if (< hy 0.02) 1 0)
            base-r (if (= is-ground 1) 90 230)
            base-g (if (= is-ground 1) 110 180)
            base-b (if (= is-ground 1) 140 110)
            shade (* ao (+ 0.2 (* 0.8 lambert)))
            lit-r (* (float base-r) shade)
            lit-g (* (float base-g) shade)
            lit-b (* (float base-b) shade)
            fog-r 75.0 fog-g 100.0 fog-b 175.0
            r (int (+ (* fog lit-r) (* (- 1.0 fog) fog-r)))
            g (int (+ (* fog lit-g) (* (- 1.0 fog) fog-g)))
            b (int (+ (* fog lit-b) (* (- 1.0 fog) fog-b)))
            rc (max 0 (min 255 r))
            gc (max 0 (min 255 g))
            bc (max 0 (min 255 b))]
        (+ (* 65536 rc) (* 256 gc) bc)))))
