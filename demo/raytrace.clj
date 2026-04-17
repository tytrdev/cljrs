;; Classical ray tracer — sphere + ground plane, Lambertian + specular,
;; hard shadow ray toward a moving point light. Analytical sphere
;; intersection via the quadratic formula — one sqrt per hit test,
;; no march loop.

(def slider-0-label "sphere size")
(def slider-1-label "light height")
(def slider-2-label "specular sharpness")
(def slider-3-label "ambient")

;; Ray-sphere intersection. Returns t (hit distance) or -1 on miss.
(defn-native ray-sphere ^f64
  [^f64 ox ^f64 oy ^f64 oz ^f64 dx ^f64 dy ^f64 dz
   ^f64 cx ^f64 cy ^f64 cz ^f64 r]
  (let [ocx (- ox cx) ocy (- oy cy) ocz (- oz cz)
        b   (+ (+ (* dx ocx) (* dy ocy)) (* dz ocz))
        c   (- (+ (+ (* ocx ocx) (* ocy ocy)) (* ocz ocz)) (* r r))
        disc (- (* b b) c)]
    (if (< disc 0.0)
      -1.0
      (let [sd (sqrt disc)
            t1 (- (- b) sd)
            t2 (+ (- b) sd)]
        (if (> t1 0.001) t1 (if (> t2 0.001) t2 -1.0))))))

;; Ray-plane (y = 0) intersection. Returns t or -1.
(defn-native ray-plane ^f64 [^f64 oy ^f64 dy]
  (let [absdy (abs dy)]
    (if (> absdy 0.0001)
      (let [t (/ (- oy) dy)]
        (if (> t 0.001) t -1.0))
      -1.0)))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width   960.0
        height  540.0
        radius  (+ 0.4 (* 1.1 (/ (float s0) 1000.0)))
        ly      (+ 1.0 (* 5.0 (/ (float s1) 1000.0)))
        spec-k  (+ 2.0 (* 60.0 (/ (float s2) 1000.0)))
        ambient (+ 0.05 (* 0.3 (/ (float s3) 1000.0)))
        time    (/ (float t-ms) 1000.0)
        fx (float px)
        fy (float py)
        aspect (/ width height)
        nx (* (- (/ (* 2.0 fx) width)  1.0) aspect)
        ny (- 1.0 (/ (* 2.0 fy) height))
        rx nx ry ny rz 2.0
        rlen (sqrt (+ (+ (* rx rx) (* ry ry)) (* rz rz)))
        dx (/ rx rlen) dy (/ ry rlen) dz (/ rz rlen)
        ox 0.0 oy 1.5 oz -4.0
        cx (* 1.5 (sin time))
        cy (+ 1.3 (* 0.3 (abs (sin (* time 2.0)))))
        cz (* 0.5 (cos time))
        lx (* 3.0 (cos (* 0.4 time)))
        lz (* 3.0 (sin (* 0.4 time)))
        ts (ray-sphere ox oy oz dx dy dz cx cy cz radius)
        tp (ray-plane oy dy)
        ;; Pick whichever positive hit is closer. Use sentinel 1e9 for "miss".
        t-big 1000000000.0
        ts-pos (if (> ts 0.0) ts t-big)
        tp-pos (if (> tp 0.0) tp t-big)
        hit-type (if (< ts-pos tp-pos)
                   (if (< ts-pos t-big) 1 0)
                   (if (< tp-pos t-big) 2 0))
        t (if (= hit-type 1) ts (if (= hit-type 2) tp 0.0))]
    (if (= hit-type 0)
      ;; Sky gradient.
      (let [sky-t (* 0.5 (+ 1.0 dy))
            r (int (* 255.0 (+ 0.3 (* 0.4 sky-t))))
            g (int (* 255.0 (+ 0.4 (* 0.4 sky-t))))
            b (int (* 255.0 (+ 0.6 (* 0.4 sky-t))))]
        (+ (* 65536 r) (* 256 g) b))
      (let [hx (+ ox (* t dx))
            hy (+ oy (* t dy))
            hz (+ oz (* t dz))
            ;; Normal depends on which object we hit.
            nx2 (if (= hit-type 1) (/ (- hx cx) radius) 0.0)
            ny2 (if (= hit-type 1) (/ (- hy cy) radius) 1.0)
            nz2 (if (= hit-type 1) (/ (- hz cz) radius) 0.0)
            ldx (- lx hx) ldy (- ly hy) ldz (- lz hz)
            lmag (sqrt (+ (+ (* ldx ldx) (* ldy ldy)) (* ldz ldz)))
            nldx (/ ldx lmag) nldy (/ ldy lmag) nldz (/ ldz lmag)
            ;; Shadow ray — offset origin slightly along normal to avoid self-hit.
            sox (+ hx (* 0.01 nx2))
            soy (+ hy (* 0.01 ny2))
            soz (+ hz (* 0.01 nz2))
            shadow-t (ray-sphere sox soy soz nldx nldy nldz
                                  cx cy cz radius)
            ;; "In shadow" if the shadow ray hits the sphere before the light.
            in-shadow (if (> shadow-t 0.0)
                        (if (< shadow-t lmag) 1 0)
                        0)
            lambert (max 0.0 (+ (+ (* nx2 nldx) (* ny2 nldy)) (* nz2 nldz)))
            ;; Reflect light direction around normal for specular.
            rdot (* 2.0 lambert)
            refx (- (* rdot nx2) nldx)
            refy (- (* rdot ny2) nldy)
            refz (- (* rdot nz2) nldz)
            vdx (- dx) vdy (- dy) vdz (- dz)
            spec-dot (max 0.0 (+ (+ (* refx vdx) (* refy vdy)) (* refz vdz)))
            specular (pow spec-dot spec-k)
            light-factor (if (= in-shadow 1)
                           ambient
                           (+ ambient (* 0.9 lambert)))
            ;; Ground checker or sphere base.
            check-x (mod (int (+ hx 100.0)) 2)
            check-z (mod (int (+ hz 100.0)) 2)
            check (mod (+ check-x check-z) 2)
            base-r (if (= hit-type 1)
                     255
                     (if (= check 0) 220 80))
            base-g (if (= hit-type 1)
                     120
                     (if (= check 0) 200 60))
            base-b (if (= hit-type 1)
                     80
                     (if (= check 0) 180 40))
            spec-contrib (if (= in-shadow 1) 0.0 (* 255.0 specular))
            r (int (min 255.0 (+ (* (float base-r) light-factor) spec-contrib)))
            g (int (min 255.0 (+ (* (float base-g) light-factor) spec-contrib)))
            b (int (min 255.0 (+ (* (float base-b) light-factor) spec-contrib)))
            rc (max 0 r)
            gc (max 0 g)
            bc (max 0 b)]
        (+ (* 65536 rc) (* 256 gc) bc)))))
