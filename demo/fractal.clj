;; Live-coded Mandelbrot fractal.
;;
;; Invoked by: cargo run --release --features demo --bin demo -- demo/fractal.clj
;;
;; The demo's Rust side calls:
;;   (render-pixel px py frame t-ms s0 s1 s2 s3)
;; once per pixel per frame; every call goes into MLIR-JIT'd native code
;; via Value::Native. Edit numbers below and save — the window picks up
;; the new version within the next frame.

;; Slider labels shown in the demo's UI panel.
(def slider-0-label "zoom speed")
(def slider-1-label "max iterations")
(def slider-2-label "color cycle speed")
(def slider-3-label "saturation")

;; How many iterations to decide if a complex c is in the Mandelbrot set.
(defn-native mandel-iter ^i64 [^f64 cr ^f64 ci ^i64 max-iter]
  (loop [i 0 zr 0.0 zi 0.0]
    (if (>= i max-iter)
      max-iter
      (let [zr2 (* zr zr)
            zi2 (* zi zi)]
        (if (> (+ zr2 zi2) 4.0)
          i
          (recur
            (+ i 1)
            (+ (- zr2 zi2) cr)
            (+ (* 2.0 (* zr zi)) ci)))))))

(defn-native render-pixel ^i64
  [^i64 px ^i64 py ^i64 frame ^i64 t-ms
   ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3]
  (let [width   960.0
        height  540.0
        ;; s0: zoom speed (0..0.001)
        zoom-rate (* 0.001 (/ (float s0) 1000.0))
        ;; s1: max iterations (32..256)
        max-iter  (+ 32 (int (* 224.0 (/ (float s1) 1000.0))))
        ;; s2: color-cycle speed (0..10)
        col-speed (* 10.0 (/ (float s2) 1000.0))
        ;; s3: color saturation amp (0.5..2.0)
        sat       (+ 0.5 (* 1.5 (/ (float s3) 1000.0)))
        zoom   (+ 1.0 (* zoom-rate (float t-ms)))
        cr-c   -0.743643887037151
        ci-c    0.131825904205330
        fx     (float px)
        fy     (float py)
        cr     (+ cr-c (/ (- (* 3.5 (/ fx width))  1.75) zoom))
        ci     (+ ci-c (/ (- (* 2.0 (/ fy height)) 1.0)  zoom))
        iters  (mandel-iter cr ci max-iter)]
    (if (= iters max-iter)
      0
      ;; Sinusoidal color palette, driven by iter + frame*col-speed.
      (let [t (+ (float iters) (* col-speed (float frame) 0.05))
            r (int (* sat (+ 128.0 (* 127.0 (sin (* t 0.05))))))
            g (int (* sat (+ 128.0 (* 127.0 (sin (+ (* t 0.05) 2.0))))))
            b (int (* sat (+ 128.0 (* 127.0 (sin (+ (* t 0.05) 4.0))))))
            rc (min 255 (max 0 r))
            gc (min 255 (max 0 g))
            bc (min 255 (max 0 b))]
        (+ (* 65536 rc) (* 256 gc) bc)))))
