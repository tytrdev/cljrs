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
(defn-native march ^f64 [^f64 ox ^f64 oy ^f64 oz ^f64 dx ^f64 dy ^f64 dz ^f64 sy]
  (loop [step 0 t 0.0]
    (if (>= step 96)
      100.0
      (let [px (+ ox (* t dx))
            py (+ oy (* t dy))
            pz (+ oz (* t dz))
            d  (scene-sdf px py pz sy)]
        (if (< d 0.001)
          t
          (if (> t 50.0)
            100.0
            (recur (+ step 1) (+ t d))))))))

;; Render one pixel. (px, py) in screen space; (frame, t-ms) drive animation.
(defn-native render-pixel ^i64 [^i64 px ^i64 py ^i64 frame ^i64 t-ms]
  (let [width   960.0
        height  540.0
        ;; Sphere bobs up and down.
        time    (/ (float t-ms) 1000.0)
        sy      (+ 1.3 (* 0.5 (sin (* time 2.0))))
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
            ;; Light direction: upper-right, normalized.
            lx  0.577
            ly  0.577
            lz -0.577
            lambert (max 0.0 (+ (+ (* nx2 lx) (* ny2 ly)) (* nz2 lz)))
            ;; Ground gets a checker pattern; sphere gets a base color.
            ;; Detect "ground" by y-coordinate of hit point.
            py-hit (+ oy (* t dy))
            is-ground (if (< py-hit 0.01) 1 0)
            check-x (int (+ px3 100.0))
            check-z (int (+ pz3 100.0))
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
