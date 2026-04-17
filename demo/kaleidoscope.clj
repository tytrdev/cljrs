;; Kaleidoscope — fold pixel coordinates into a symmetric wedge, then
;; sample a procedural color field at the folded coord. Result: an
;; N-fold symmetric pattern that flows as time advances.

(def slider-0-label "symmetry order")
(def slider-1-label "pattern scale")
(def slider-2-label "swirl")
(def slider-3-label "rotation speed")

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width  960.0
        height 540.0
        order  (+ 3.0 (* 12.0 (/ (float s0) 1000.0)))
        scale  (+ 0.01 (* 0.1 (/ (float s1) 1000.0)))
        swirl  (* 4.0 (/ (float s2) 1000.0))
        rot-sp (* 0.002 (/ (float s3) 1000.0))
        t      (* rot-sp (float t-ms))
        cx     (/ width 2.0)
        cy     (/ height 2.0)
        fx     (- (float px) cx)
        fy     (- (float py) cy)
        ;; To polar.
        r      (sqrt (+ (* fx fx) (* fy fy)))
        ;; atan2 not native — synthesize angle via small branch.
        ;; Use `ratio = fx / (|fx|+|fy|)` which ∈ [-1,1] and behaves
        ;; monotonically within each quadrant; good enough for visual fold.
        ax (abs fx) ay (abs fy)
        theta (/ fx (+ 1.0 (+ ax ay)))
        ;; Fold angle into [0, 1/order] via abs + rem.
        wedge (/ 6.2831853 order)
        a-raw (+ theta (* t 0.5))
        a-mod (- a-raw (* wedge (float (int (/ a-raw wedge)))))
        a-fold (abs (- a-mod (* 0.5 wedge)))
        ;; Swirl the radius by the folded angle.
        ur (* scale (+ r (* swirl (* r a-fold))))
        ;; Sample a layered sin field at (ur, a-fold).
        v1 (sin (+ (* ur 1.0) (* t 1.0)))
        v2 (sin (+ (* ur 2.3) (* a-fold 4.0) (* t 0.7)))
        v3 (sin (+ (* ur 0.8) (* a-fold 8.0) (* t 1.3)))
        mix (* 0.333 (+ v1 (+ v2 v3)))
        ph  (* 6.2831853 (+ 0.5 (* 0.5 mix)))
        R (int (* 255.0 (+ 0.5 (* 0.5 (sin ph)))))
        G (int (* 255.0 (+ 0.5 (* 0.5 (sin (+ ph 2.094))))))
        B (int (* 255.0 (+ 0.5 (* 0.5 (sin (+ ph 4.188))))))
        rc (max 0 (min 255 R))
        gc (max 0 (min 255 G))
        bc (max 0 (min 255 B))]
    (+ (* 65536 rc) (* 256 gc) bc)))
