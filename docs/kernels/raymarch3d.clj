;; 3D raymarched SDF scene: smooth-unioned spheres above a checker floor,
;; orbiting camera with a fixed world-up basis (so only the camera moves,
;; not the ground). Blinn-Phong lighting with a soft shadow.
;;
;; Sliders:
;;   s0  orbit speed      0..1000 -> 0..1.5 rad/sec
;;   s1  orbit radius     0..1000 -> 2.5..7
;;   s2  sun azimuth      0..1000 -> 0..2pi
;;   s3  shape blend      0..1000 -> 0.05..1.2

;; Scene SDF: two bouncing spheres smoothly unioned, plus an infinite
;; Y=0 floor. `t` is time in seconds, `k` is the smooth-union radius.
(defmacro scene-sdf [px py pz tt kk]
  `(let [ax (- ~px 0.0)
         ay (- ~py (+ 0.55 (* 0.25 (sin (* ~tt 1.1)))))
         az (- ~pz 0.0)
         da (- (sqrt (+ (+ (* ax ax) (* ay ay)) (* az az))) 0.55)
         bx (- ~px (* 0.9 (cos (* ~tt 0.7))))
         by (- ~py (+ 0.55 (* 0.15 (cos (* ~tt 1.3)))))
         bz (- ~pz (* 0.9 (sin (* ~tt 0.7))))
         db (- (sqrt (+ (+ (* bx bx) (* by by)) (* bz bz))) 0.45)
         ;; Polynomial smooth min for a soft union.
         hh (max 0.0 (- 1.0 (/ (abs (- da db)) (max 0.0001 ~kk))))
         smn (- (min da db)
                (* (* ~kk hh) (* hh (* hh (/ 1.0 6.0)))))
         ;; Floor at y=0.
         df ~py]
     (min smn df)))

(defn-gpu-pixel render
  [x y w h t-ms s0 s1 s2 s3]
  (let [t      (* 0.001 (f32 t-ms))
        orbit-s (* 1.5 (/ (f32 s0) 1000.0))
        orbit-r (+ 2.5 (* 4.5 (/ (f32 s1) 1000.0)))
        sun-az (* 6.2831853 (/ (f32 s2) 1000.0))
        blend-k (+ 0.05 (* 1.15 (/ (f32 s3) 1000.0)))

        aspect (/ (f32 w) (f32 h))
        uv-x   (* aspect (- (* 2.0 (/ (f32 x) (f32 w))) 1.0))
        uv-y   (- 1.0 (* 2.0 (/ (f32 y) (f32 h))))   ;; flip so +y is up on screen

        ;; Camera orbits around (0, 0.7, 0) at height 1.2.
        ca     (* t orbit-s)
        cam-x  (* orbit-r (sin ca))
        cam-y  1.2
        cam-z  (* orbit-r (cos ca))
        tgt-x  0.0
        tgt-y  0.6
        tgt-z  0.0

        ;; Forward = normalize(target - cam). This is the ONLY vector
        ;; that moves with the camera — right and up are derived from it
        ;; with world up as the anchor.
        lx     (- tgt-x cam-x)
        ly     (- tgt-y cam-y)
        lz     (- tgt-z cam-z)
        ll     (sqrt (+ (+ (* lx lx) (* ly ly)) (* lz lz)))
        fx     (/ lx ll)
        fy     (/ ly ll)
        fz     (/ lz ll)

        ;; right = normalize(world_up × forward). With world_up=(0,1,0),
        ;; cross gives (fz, 0, -fx). Always lies in the horizontal plane,
        ;; so the ground never tilts.
        rx0    fz
        ry0    0.0
        rz0    (- fx)
        rl     (max 0.0001 (sqrt (+ (+ (* rx0 rx0) (* ry0 ry0)) (* rz0 rz0))))
        rx     (/ rx0 rl)
        ry     (/ ry0 rl)
        rz     (/ rz0 rl)

        ;; up' = cross(forward, right) for a right-handed y-up frame.
        ux     (- (* fy rz) (* fz ry))
        uy     (- (* fz rx) (* fx rz))
        uz     (- (* fx ry) (* fy rx))

        ;; Ray direction through this pixel, FOV ~ 1/1.4.
        fov    1.4
        dx0    (+ (+ (* fx fov) (* rx uv-x)) (* ux uv-y))
        dy0    (+ (+ (* fy fov) (* ry uv-x)) (* uy uv-y))
        dz0    (+ (+ (* fz fov) (* rz uv-x)) (* uz uv-y))
        dl     (sqrt (+ (+ (* dx0 dx0) (* dy0 dy0)) (* dz0 dz0)))
        dx     (/ dx0 dl)
        dy     (/ dy0 dl)
        dz     (/ dz0 dl)

        ;; Sun direction from azimuth; fixed elevation.
        sel    0.9
        sx0    (* (cos sel) (cos sun-az))
        sz0    (* (cos sel) (sin sun-az))
        sy0    (sin sel)
        ;; Already normalized (unit vectors from trig).
        sx     sx0
        sy     sy0
        sz     sz0

        max-steps (i32 96)
        max-dist  40.0
        hit-eps   0.001

        hit-t (loop [tt 0.0 steps (i32 0)]
                (if (>= steps max-steps) max-dist
                  (if (>= tt max-dist) max-dist
                    (let [px (+ cam-x (* dx tt))
                          py (+ cam-y (* dy tt))
                          pz (+ cam-z (* dz tt))
                          d  (scene-sdf px py pz t blend-k)]
                      (if (< d hit-eps) tt
                        (recur (+ tt (max hit-eps d))
                               (+ steps (i32 1))))))))

        hit? (< hit-t (- max-dist 0.1))
        ;; Hit point.
        hx   (+ cam-x (* dx hit-t))
        hy   (+ cam-y (* dy hit-t))
        hz   (+ cam-z (* dz hit-t))

        ;; Normal by central differences through scene-sdf.
        ne   0.0015
        nx0  (- (scene-sdf (+ hx ne) hy hz t blend-k)
                (scene-sdf (- hx ne) hy hz t blend-k))
        ny0  (- (scene-sdf hx (+ hy ne) hz t blend-k)
                (scene-sdf hx (- hy ne) hz t blend-k))
        nz0  (- (scene-sdf hx hy (+ hz ne) t blend-k)
                (scene-sdf hx hy (- hz ne) t blend-k))
        nl   (max 0.0001 (sqrt (+ (+ (* nx0 nx0) (* ny0 ny0)) (* nz0 nz0))))
        nx   (/ nx0 nl)
        ny   (/ ny0 nl)
        nz   (/ nz0 nl)

        ;; Soft shadow: march from hit toward sun, track smallest ratio
        ;; of distance-to-scene over distance-along-ray.
        eps  0.02
        sox  (+ hx (* nx eps))
        soy  (+ hy (* ny eps))
        soz  (+ hz (* nz eps))
        soft (loop [tt 0.05 mn 1.0 steps (i32 0)]
               (if (>= steps (i32 32)) mn
                 (if (>= tt 8.0) mn
                   (let [px (+ sox (* sx tt))
                         py (+ soy (* sy tt))
                         pz (+ soz (* sz tt))
                         d  (scene-sdf px py pz t blend-k)]
                     (if (< d 0.001) 0.0
                       (recur (+ tt (max 0.005 d))
                              (min mn (/ (* 12.0 d) tt))
                              (+ steps (i32 1))))))))
        shadow (max 0.0 (min 1.0 soft))

        ndotl (max 0.0 (+ (+ (* nx sx) (* ny sy)) (* nz sz)))
        ;; Blinn-Phong specular.
        vx    (- dx)
        vy    (- dy)
        vz    (- dz)
        hhx   (+ sx vx)
        hhy   (+ sy vy)
        hhz   (+ sz vz)
        hl    (max 0.0001 (sqrt (+ (+ (* hhx hhx) (* hhy hhy)) (* hhz hhz))))
        hhx   (/ hhx hl)
        hhy   (/ hhy hl)
        hhz   (/ hhz hl)
        ndoth (max 0.0 (+ (+ (* nx hhx) (* ny hhy)) (* nz hhz)))
        spec  (pow ndoth 48.0)

        ;; Base albedo: checker on ground (detected by hit point at y~0),
        ;; warm for the shapes. Using the hit y coordinate rather than
        ;; the normal direction avoids painting sphere tops as floor.
        on-floor? (< (abs hy) 0.02)
        cxi   (i32 (floor (* hx 0.5)))
        czi   (i32 (floor (* hz 0.5)))
        chk   (if (= (mod (+ cxi czi) (i32 2)) (i32 0)) 0.82 0.22)
        ar    (if on-floor? chk 0.95)
        ag    (if on-floor? chk 0.55)
        ab    (if on-floor? chk 0.32)

        ;; Light = ambient + diffuse*shadow + specular*shadow.
        amb   0.18
        lit   (* shadow ndotl)
        rr    (+ (* amb ar) (+ (* (* 0.9 lit) ar) (* (* 0.55 shadow) spec)))
        gg    (+ (* amb ag) (+ (* (* 0.9 lit) ag) (* (* 0.55 shadow) spec)))
        bb    (+ (* amb ab) (+ (* (* 0.9 lit) ab) (* (* 0.55 shadow) spec)))

        ;; Sky gradient when we missed.
        skyt  (max 0.0 (min 1.0 (* 0.5 (+ 1.0 dy))))
        skr   (+ (* (- 1.0 skyt) 0.55) (* skyt 0.18))
        skg   (+ (* (- 1.0 skyt) 0.72) (* skyt 0.35))
        skb   (+ (* (- 1.0 skyt) 0.95) (* skyt 0.65))

        out-r (if hit? rr skr)
        out-g (if hit? gg skg)
        out-b (if hit? bb skb)
        ri    (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 out-r)))))
        gi    (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 out-g)))))
        bi    (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 out-b)))))]
    (bit-or (bit-or (bit-shift-left ri (u32 16)) (bit-shift-left gi (u32 8))) bi)))
