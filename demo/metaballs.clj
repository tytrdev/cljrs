;; Metaballs — four moving "field sources" contribute 1/r² to a scalar
;; field. Threshold it around 1.0 to get the blob isosurface; blend
;; smoothly across the threshold for a gooey look. Classic demo effect.

(def slider-0-label "ball radius")
(def slider-1-label "motion speed")
(def slider-2-label "motion spread")
(def slider-3-label "palette hue")

(defn-native meta-sum ^f64
  [^f64 px ^f64 py
   ^f64 x0 ^f64 y0 ^f64 r0
   ^f64 x1 ^f64 y1 ^f64 r1
   ^f64 x2 ^f64 y2 ^f64 r2
   ^f64 x3 ^f64 y3 ^f64 r3]
  (let [d0 (+ 1.0 (+ (* (- px x0) (- px x0)) (* (- py y0) (- py y0))))
        d1 (+ 1.0 (+ (* (- px x1) (- px x1)) (* (- py y1) (- py y1))))
        d2 (+ 1.0 (+ (* (- px x2) (- px x2)) (* (- py y2) (- py y2))))
        d3 (+ 1.0 (+ (* (- px x3) (- px x3)) (* (- py y3) (- py y3))))]
    (+ (+ (/ (* r0 r0) d0) (/ (* r1 r1) d1))
       (+ (/ (* r2 r2) d2) (/ (* r3 r3) d3)))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [base-r (+ 60.0 (* 100.0 (/ (float s0) 1000.0)))
        speed  (* 0.002 (/ (float s1) 1000.0))
        base-spread (+ 50.0 (* 250.0 (/ (float s2) 1000.0)))
        hue    (/ (float s3) 1000.0)
        t      (* speed (float t-ms))
        ;; Ten-second breathing zoom: the field is sampled in scaled
        ;; pixel space so the whole composition zooms in and out.
        zt     (* 0.628 (* speed 1000.0))   ;; faster slider = faster zoom
        zoom   (+ 1.0 (* 0.9 (sin zt)))     ;; ranges 0.1..1.9
        spread (* base-spread zoom)
        radius (* base-r (/ 1.0 zoom))      ;; counter-scale to keep blobs same on screen
        cx 480.0
        cy 270.0
        x0 (+ cx (* spread (sin (* 1.0 t))))
        y0 (+ cy (* spread (cos (* 1.3 t))))
        x1 (+ cx (* spread (sin (+ (* 1.2 t) 1.5))))
        y1 (+ cy (* spread (cos (+ (* 0.8 t) 0.5))))
        x2 (+ cx (* spread (sin (+ (* 0.9 t) 3.0))))
        y2 (+ cy (* spread (cos (+ (* 1.5 t) 2.5))))
        x3 (+ cx (* spread (sin (+ (* 1.1 t) 4.5))))
        y3 (+ cy (* spread (cos (+ (* 0.7 t) 3.8))))
        ;; Sample-space zoom: pixel coords scaled toward center.
        fx (+ cx (/ (- (float px) cx) zoom))
        fy (+ cy (/ (- (float py) cy) zoom))
        v (meta-sum fx fy x0 y0 radius x1 y1 radius
                           x2 y2 radius x3 y3 radius)
        ;; Map field value to a two-tone palette with a soft edge around 1.0.
        sigma (/ 1.0 (+ 1.0 (exp (* -12.0 (- v 1.0)))))
        base  (+ 0.1 (* 0.9 sigma))
        ;; Hue rotation driven by slider.
        ph (* 6.2831853 hue)
        r (int (* 255.0 (* base (+ 0.5 (* 0.5 (sin ph))))))
        g (int (* 255.0 (* base (+ 0.5 (* 0.5 (sin (+ ph 2.094)))))))
        b (int (* 255.0 (* base (+ 0.5 (* 0.5 (sin (+ ph 4.188)))))))
        rc (max 0 (min 255 r))
        gc (max 0 (min 255 g))
        bc (max 0 (min 255 b))]
    (+ (* 65536 rc) (* 256 gc) bc)))
