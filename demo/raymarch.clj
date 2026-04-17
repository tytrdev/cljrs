;; 3D raymarcher — signed distance field scene with Lambertian shading.
;; Sphere bouncing above a checkered ground plane, procedural sky.
;;
;; Every pixel: cast a ray, march along it sampling the nearest surface,
;; when it hits compute the normal via SDF gradient, then do a simple dot
;; with a light vector for shading. All in native MLIR-compiled code.
;;
;; Live-edit any number and save to see the scene update in real time.

;; Signed distance to the scene at point (x, y, z).
;; Scene = sphere at (0, sy, 0) + ground plane at y=0.
(defn-native scene-sdf ^f64 [^f64 x ^f64 y ^f64 z ^f64 sy]
  (let [;; Sphere at (0, sy, 0), radius 1.
        dx     x
        dy     (- y sy)
        dz     z
        d-sphere (- (sqrt (+ (+ (* dx dx) (* dy dy)) (* dz dz))) 1.0)
        ;; Ground plane y=0.
        d-plane  y]
    (min d-sphere d-plane)))

;; Raymarch from origin `(ox oy oz)` in direction `(dx dy dz)`.
;; Returns: distance travelled when we hit, or 100.0 if we missed.
;; 256-step budget + a looser hit threshold (0.002 instead of 0.001) kills
;; the silhouette halo. Grazing rays take many tiny steps along the sphere
;; tangent; 96 was running out right before they hit, producing a ring of
;; "miss" pixels that showed as sky.
(defn-native march ^f64 [^f64 ox ^f64 oy ^f64 oz ^f64 dx ^f64 dy ^f64 dz ^f64 sy]
  (loop [step 0 t 0.0]
    (if (>= step 256)
      100.0
      (let [px (+ ox (* t dx))
            py (+ oy (* t dy))
            pz (+ oz (* t dz))
            d  (scene-sdf px py pz sy)]
        (if (< d 0.002)
          t
          (if (> t 50.0)
            100.0
            (recur (+ step 1) (+ t d))))))))

;; Slider labels shown in the demo's UI panel.
(def slider-0-label "sphere bob speed")
(def slider-1-label "sphere bob height")
(def slider-2-label "light angle")
(def slider-3-label "checker scale")

;; Render one pixel. (px, py) in screen space; (frame, t-ms) drive animation;
;; (s0..s3) are UI sliders, i64 values in 0..1000.
(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width   960.0
        height  540.0
        ;; Slider 0: bob speed (0..4 Hz)
        bob-speed (* 4.0 (/ (float s0) 1000.0))
        ;; Slider 1: bob height (0..1)
        bob-height (/ (float s1) 1000.0)
        ;; Slider 2: light angle (0..2π)
        light-angle (* 6.2831853 (/ (float s2) 1000.0))
        ;; Slider 3: checker scale (1..20)
        check-scale (+ 1.0 (* 19.0 (/ (float s3) 1000.0)))
        ;; Sphere bobs up and down, clipped above the ground.
        time    (/ (float t-ms) 1000.0)
        sy      (+ 2.0 (* bob-height (sin (* time bob-speed))))
        ;; Camera at (0, 1.5, -4), looking toward origin.
        ;; Ray direction from camera through pixel.
        aspect  (/ width height)
        nx      (* (- (/ (* 2.0 (float px)) width) 1.0) aspect)
        ny      (- 1.0 (/ (* 2.0 (float py)) height))
        ;; Normalize ray dir (dx, dy, 2.0) so z component weights correctly.
        rx      nx
        ry      ny
        rz      2.0
        rlen    (sqrt (+ (+ (* rx rx) (* ry ry)) (* rz rz)))
        dx      (/ rx rlen)
        dy      (/ ry rlen)
        dz      (/ rz rlen)
        ;; Camera origin.
        ox      0.0
        oy      1.5
        oz     -4.0
        t       (march ox oy oz dx dy dz sy)]
    (if (>= t 100.0)
      ;; Miss = sky. Cheap horizon gradient: darker up, lighter down.
      (let [sky (int (+ 60.0 (* 160.0 (- 1.0 dy))))
            b   (min sky 255)]
        (+ (* (- b 40) 65536) (* (- b 20) 256) b))
      ;; Hit. Compute normal via central-difference SDF gradient.
      (let [eps 0.001
            px3 (+ ox (* t dx))
            py3 (+ oy (* t dy))
            pz3 (+ oz (* t dz))
            nxp (scene-sdf (+ px3 eps) py3 pz3 sy)
            nxm (scene-sdf (- px3 eps) py3 pz3 sy)
            nyp (scene-sdf px3 (+ py3 eps) pz3 sy)
            nym (scene-sdf px3 (- py3 eps) pz3 sy)
            nzp (scene-sdf px3 py3 (+ pz3 eps) sy)
            nzm (scene-sdf px3 py3 (- pz3 eps) sy)
            gx  (- nxp nxm)
            gy  (- nyp nym)
            gz  (- nzp nzm)
            glen (sqrt (+ (+ (* gx gx) (* gy gy)) (* gz gz)))
            nx2 (/ gx glen)
            ny2 (/ gy glen)
            nz2 (/ gz glen)
            ;; Light direction driven by slider 2; rotates around the scene.
            lx  (cos light-angle)
            ly  0.6
            lz  (sin light-angle)
            lnorm (sqrt (+ (+ (* lx lx) (* ly ly)) (* lz lz)))
            nlx (/ lx lnorm)
            nly (/ ly lnorm)
            nlz (/ lz lnorm)
            lambert (max 0.0 (+ (+ (* nx2 nlx) (* ny2 nly)) (* nz2 nlz)))
            ;; Ground gets a checker pattern; sphere gets a base color.
            py-hit (+ oy (* t dy))
            is-ground (if (< py-hit 0.02) 1 0)
            check-x (int (* check-scale (+ px3 100.0)))
            check-z (int (* check-scale (+ pz3 100.0)))
            check (mod (+ check-x check-z) 2)
            base-r (if (= is-ground 1)
                     (if (= check 0) 200 80)
                     220)
            base-g (if (= is-ground 1)
                     (if (= check 0) 180 60)
                     120)
            base-b (if (= is-ground 1)
                     (if (= check 0) 140 40)
                     60)
            shade (+ 0.2 (* 0.8 lambert))
            r (int (* (float base-r) shade))
            g (int (* (float base-g) shade))
            b (int (* (float base-b) shade))]
        (+ (* r 65536) (* g 256) b)))))
