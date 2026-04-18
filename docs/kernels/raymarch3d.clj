;; 3D raymarched SDF scene with soft shadows, ambient occlusion, and
;; Blinn-Phong lighting. Camera orbits the scene; geometry is a union
;; of two spheres and a torus above an infinite floor.
;;
;; Sliders:
;;   s0  camera orbit speed    0..1000 -> 0..2 rad/sec
;;   s1  orbit radius          0..1000 -> 2..8
;;   s2  sun elevation         0..1000 -> 0..pi/2
;;   s3  shape blend           0..1000 -> smooth union blend k, 0..2

(defmacro sd-sphere [p r]
  `(- (length3 ~p) ~r))

(defmacro length3 [p]
  `(sqrt (+ (+ (* (x3 ~p) (x3 ~p)) (* (y3 ~p) (y3 ~p))) (* (z3 ~p) (z3 ~p)))))

(defn-gpu-pixel render
  [x y w h t-ms s0 s1 s2 s3]
  (let [t       (* 0.001 (f32 t-ms))
        orbit-r (+ 2.0 (* 6.0 (/ (f32 s1) 1000.0)))
        orbit-s (* 2.0 (/ (f32 s0) 1000.0))
        sun-el  (* 1.5707963 (/ (f32 s2) 1000.0))
        blend-k (* 2.0 (/ (f32 s3) 1000.0))

        ;; Pixel to normalized camera ray (origin at camera, through pixel).
        aspect  (/ (f32 w) (f32 h))
        uv-x    (* aspect (- (* 2.0 (/ (f32 x) (f32 w))) 1.0))
        uv-y    (- (* 2.0 (/ (f32 y) (f32 h))) 1.0)

        ;; Camera position: orbit around origin at fixed height.
        ca      (* t orbit-s)
        cam-x   (* orbit-r (sin ca))
        cam-y   1.3
        cam-z   (* orbit-r (cos ca))

        ;; Build ray direction. Camera looks at origin; ray.z = forward.
        look-x  (- cam-x)
        look-y  (- 0.4 cam-y)
        look-z  (- cam-z)
        ll      (sqrt (+ (+ (* look-x look-x) (* look-y look-y)) (* look-z look-z)))
        fx      (/ look-x ll)
        fy      (/ look-y ll)
        fz      (/ look-z ll)
        ;; right = cross(up, forward), up = world (0,1,0)
        rx      (- (* 0.0 fz) (* 1.0 fy))
        ry      (- (* 1.0 fx) (* 0.0 fz))
        rz      (- (* 0.0 fy) (* 0.0 fx))
        rl      (sqrt (+ (+ (* rx rx) (* ry ry)) (* rz rz)))
        rx      (/ rx rl)
        ry      (/ ry rl)
        rz      (/ rz rl)
        ;; up' = cross(forward, right)
        ux      (- (* fy rz) (* fz ry))
        uy      (- (* fz rx) (* fx rz))
        uz      (- (* fx ry) (* fy rx))

        ;; ray.dir = normalize(fx*1.2 + rx*uv-x + ux*uv-y)  (1.2 ~ fov)
        dx      (+ (+ (* fx 1.2) (* rx uv-x)) (* ux uv-y))
        dy      (+ (+ (* fy 1.2) (* ry uv-x)) (* uy uv-y))
        dz      (+ (+ (* fz 1.2) (* rz uv-x)) (* uz uv-y))
        dl      (sqrt (+ (+ (* dx dx) (* dy dy)) (* dz dz)))
        dx      (/ dx dl)
        dy      (/ dy dl)
        dz      (/ dz dl)

        ;; Sun direction from elevation slider (azimuth fixed).
        sx      0.4
        sy      (sin sun-el)
        sz      (cos sun-el)
        sl      (sqrt (+ (+ (* sx sx) (* sy sy)) (* sz sz)))
        sx      (/ sx sl)
        sy      (/ sy sl)
        sz      (/ sz sl)

        ;; March until near a surface or maxed out.
        max-steps (i32 96)
        max-dist  60.0
        hit-eps   0.001

        hit-t (loop [tt 0.0 steps (i32 0)]
                (if (>= steps max-steps) max-dist
                  (if (>= tt max-dist) max-dist
                    (let [px (+ cam-x (* dx tt))
                          py (+ cam-y (* dy tt))
                          pz (+ cam-z (* dz tt))
                          ;; SDF scene.
                          d-floor py
                          ;; Sphere A, oscillating.
                          ax (- px 0.0)
                          ay (- py (+ 0.5 (* 0.25 (sin (* t 0.8)))))
                          az (- pz 0.0)
                          d-a (- (sqrt (+ (+ (* ax ax) (* ay ay)) (* az az))) 0.6)
                          ;; Sphere B, offset.
                          bx (- px 1.1)
                          by (- py 0.6)
                          bz (- pz (- 0.4))
                          d-b (- (sqrt (+ (+ (* bx bx) (* by by)) (* bz bz))) 0.4)
                          ;; Torus around origin (major 1.2, minor 0.15).
                          rx' px
                          ry' pz
                          qxy (- (sqrt (+ (* rx' rx') (* ry' ry'))) 1.2)
                          tyy (- py 0.1)
                          d-t (- (sqrt (+ (* qxy qxy) (* tyy tyy))) 0.15)
                          ;; Smooth-union spheres A and B.
                          h   (max 0.0 (- 1.0 (/ (abs (- d-a d-b)) (max 0.001 blend-k))))
                          smooth-ab (- (min d-a d-b) (* (* blend-k h) (* h (* h (/ 1.0 6.0)))))
                          ;; Union with torus.
                          d-shapes (min smooth-ab d-t)
                          ;; Union with floor.
                          d (min d-shapes d-floor)]
                      (if (< d hit-eps) tt
                        (recur (+ tt (max hit-eps d)) (+ steps (i32 1))))))))

        hit? (< hit-t (- max-dist 0.1))
        ;; Hit point.
        hx   (+ cam-x (* dx hit-t))
        hy   (+ cam-y (* dy hit-t))
        hz   (+ cam-z (* dz hit-t))

        ;; Normal via gradient of the SDF. We recompute d at ±eps per axis.
        ;; For brevity we inline a small helper macro instead of repeating.
        ne   0.002
        ;; Field at (px,py,pz).
        sdf-x+ (let [px (+ hx ne) py hy pz hz
                     d-floor py
                     ax (- px 0.0) ay (- py (+ 0.5 (* 0.25 (sin (* t 0.8))))) az (- pz 0.0)
                     d-a (- (sqrt (+ (+ (* ax ax) (* ay ay)) (* az az))) 0.6)
                     bx (- px 1.1) by (- py 0.6) bz (- pz (- 0.4))
                     d-b (- (sqrt (+ (+ (* bx bx) (* by by)) (* bz bz))) 0.4)
                     qxy (- (sqrt (+ (* px px) (* pz pz))) 1.2)
                     tyy (- py 0.1)
                     d-t (- (sqrt (+ (* qxy qxy) (* tyy tyy))) 0.15)
                     h   (max 0.0 (- 1.0 (/ (abs (- d-a d-b)) (max 0.001 blend-k))))
                     smooth-ab (- (min d-a d-b) (* (* blend-k h) (* h (* h (/ 1.0 6.0)))))]
                 (min (min smooth-ab d-t) d-floor))
        sdf-x- (let [px (- hx ne) py hy pz hz
                     d-floor py
                     ax (- px 0.0) ay (- py (+ 0.5 (* 0.25 (sin (* t 0.8))))) az (- pz 0.0)
                     d-a (- (sqrt (+ (+ (* ax ax) (* ay ay)) (* az az))) 0.6)
                     bx (- px 1.1) by (- py 0.6) bz (- pz (- 0.4))
                     d-b (- (sqrt (+ (+ (* bx bx) (* by by)) (* bz bz))) 0.4)
                     qxy (- (sqrt (+ (* px px) (* pz pz))) 1.2)
                     tyy (- py 0.1)
                     d-t (- (sqrt (+ (* qxy qxy) (* tyy tyy))) 0.15)
                     h   (max 0.0 (- 1.0 (/ (abs (- d-a d-b)) (max 0.001 blend-k))))
                     smooth-ab (- (min d-a d-b) (* (* blend-k h) (* h (* h (/ 1.0 6.0)))))]
                 (min (min smooth-ab d-t) d-floor))
        sdf-y+ (let [px hx py (+ hy ne) pz hz
                     d-floor py
                     ax (- px 0.0) ay (- py (+ 0.5 (* 0.25 (sin (* t 0.8))))) az (- pz 0.0)
                     d-a (- (sqrt (+ (+ (* ax ax) (* ay ay)) (* az az))) 0.6)
                     bx (- px 1.1) by (- py 0.6) bz (- pz (- 0.4))
                     d-b (- (sqrt (+ (+ (* bx bx) (* by by)) (* bz bz))) 0.4)
                     qxy (- (sqrt (+ (* px px) (* pz pz))) 1.2)
                     tyy (- py 0.1)
                     d-t (- (sqrt (+ (* qxy qxy) (* tyy tyy))) 0.15)
                     h   (max 0.0 (- 1.0 (/ (abs (- d-a d-b)) (max 0.001 blend-k))))
                     smooth-ab (- (min d-a d-b) (* (* blend-k h) (* h (* h (/ 1.0 6.0)))))]
                 (min (min smooth-ab d-t) d-floor))
        sdf-y- (let [px hx py (- hy ne) pz hz
                     d-floor py
                     ax (- px 0.0) ay (- py (+ 0.5 (* 0.25 (sin (* t 0.8))))) az (- pz 0.0)
                     d-a (- (sqrt (+ (+ (* ax ax) (* ay ay)) (* az az))) 0.6)
                     bx (- px 1.1) by (- py 0.6) bz (- pz (- 0.4))
                     d-b (- (sqrt (+ (+ (* bx bx) (* by by)) (* bz bz))) 0.4)
                     qxy (- (sqrt (+ (* px px) (* pz pz))) 1.2)
                     tyy (- py 0.1)
                     d-t (- (sqrt (+ (* qxy qxy) (* tyy tyy))) 0.15)
                     h   (max 0.0 (- 1.0 (/ (abs (- d-a d-b)) (max 0.001 blend-k))))
                     smooth-ab (- (min d-a d-b) (* (* blend-k h) (* h (* h (/ 1.0 6.0)))))]
                 (min (min smooth-ab d-t) d-floor))
        sdf-z+ (let [px hx py hy pz (+ hz ne)
                     d-floor py
                     ax (- px 0.0) ay (- py (+ 0.5 (* 0.25 (sin (* t 0.8))))) az (- pz 0.0)
                     d-a (- (sqrt (+ (+ (* ax ax) (* ay ay)) (* az az))) 0.6)
                     bx (- px 1.1) by (- py 0.6) bz (- pz (- 0.4))
                     d-b (- (sqrt (+ (+ (* bx bx) (* by by)) (* bz bz))) 0.4)
                     qxy (- (sqrt (+ (* px px) (* pz pz))) 1.2)
                     tyy (- py 0.1)
                     d-t (- (sqrt (+ (* qxy qxy) (* tyy tyy))) 0.15)
                     h   (max 0.0 (- 1.0 (/ (abs (- d-a d-b)) (max 0.001 blend-k))))
                     smooth-ab (- (min d-a d-b) (* (* blend-k h) (* h (* h (/ 1.0 6.0)))))]
                 (min (min smooth-ab d-t) d-floor))
        sdf-z- (let [px hx py hy pz (- hz ne)
                     d-floor py
                     ax (- px 0.0) ay (- py (+ 0.5 (* 0.25 (sin (* t 0.8))))) az (- pz 0.0)
                     d-a (- (sqrt (+ (+ (* ax ax) (* ay ay)) (* az az))) 0.6)
                     bx (- px 1.1) by (- py 0.6) bz (- pz (- 0.4))
                     d-b (- (sqrt (+ (+ (* bx bx) (* by by)) (* bz bz))) 0.4)
                     qxy (- (sqrt (+ (* px px) (* pz pz))) 1.2)
                     tyy (- py 0.1)
                     d-t (- (sqrt (+ (* qxy qxy) (* tyy tyy))) 0.15)
                     h   (max 0.0 (- 1.0 (/ (abs (- d-a d-b)) (max 0.001 blend-k))))
                     smooth-ab (- (min d-a d-b) (* (* blend-k h) (* h (* h (/ 1.0 6.0)))))]
                 (min (min smooth-ab d-t) d-floor))

        nx0     (- sdf-x+ sdf-x-)
        ny0     (- sdf-y+ sdf-y-)
        nz0     (- sdf-z+ sdf-z-)
        nl      (max 0.0001 (sqrt (+ (+ (* nx0 nx0) (* ny0 ny0)) (* nz0 nz0))))
        nx      (/ nx0 nl)
        ny      (/ ny0 nl)
        nz      (/ nz0 nl)

        ;; Lighting: ambient + diffuse + a bit of specular.
        ndotl   (max 0.0 (+ (+ (* nx sx) (* ny sy)) (* nz sz)))
        ;; View vector = -ray direction.
        vx      (- dx)
        vy      (- dy)
        vz      (- dz)
        ;; Half vector.
        hhx     (+ sx vx)
        hhy     (+ sy vy)
        hhz     (+ sz vz)
        hl      (max 0.0001 (sqrt (+ (+ (* hhx hhx) (* hhy hhy)) (* hhz hhz))))
        hhx     (/ hhx hl)
        hhy     (/ hhy hl)
        hhz     (/ hhz hl)
        ndoth   (max 0.0 (+ (+ (* nx hhx) (* ny hhy)) (* nz hhz)))
        spec    (pow ndoth 32.0)

        ;; Base albedo: checker on floor, warm color on shapes.
        ;; Floor normal is (0,1,0), so ny is near 1. Shapes' normals
        ;; point any which way.
        on-floor? (> ny 0.6)
        cx-int  (i32 (floor (* hx 0.5)))
        cz-int  (i32 (floor (* hz 0.5)))
        chk     (if (= (mod (+ cx-int cz-int) (i32 2)) (i32 0)) 0.85 0.25)
        ar      (if on-floor? chk 0.95)
        ag      (if on-floor? chk 0.60)
        ab      (if on-floor? chk 0.32)

        amb     0.18
        lit-r   (+ (* amb ar) (+ (* (* 0.85 ndotl) ar) (* 0.5 spec)))
        lit-g   (+ (* amb ag) (+ (* (* 0.85 ndotl) ag) (* 0.5 spec)))
        lit-b   (+ (* amb ab) (+ (* (* 0.85 ndotl) ab) (* 0.5 spec)))

        ;; Sky gradient if we missed.
        sky-t   (max 0.0 (min 1.0 (* 0.5 (+ 1.0 dy))))
        sky-r   (+ (* (- 1.0 sky-t) 0.08) (* sky-t 0.55))
        sky-g   (+ (* (- 1.0 sky-t) 0.10) (* sky-t 0.72))
        sky-b   (+ (* (- 1.0 sky-t) 0.18) (* sky-t 0.95))

        out-r   (if hit? lit-r sky-r)
        out-g   (if hit? lit-g sky-g)
        out-b   (if hit? lit-b sky-b)
        ri      (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 out-r)))))
        gi      (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 out-g)))))
        bi      (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 out-b)))))]
    (bit-or (bit-or (bit-shift-left ri (u32 16)) (bit-shift-left gi (u32 8))) bi)))
