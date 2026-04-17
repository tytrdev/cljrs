;; Live-coded Mandelbrot fractal.
;;
;; Invoked by: cargo run --release --features demo --bin demo -- demo/fractal.clj
;;
;; The demo's Rust side calls (render-pixel x y frame t-ms) once per pixel
;; per frame; every call goes into MLIR-JIT'd native code via Value::Native.
;;
;; Change any number below, save the file — the window picks up the new
;; version within the next frame, via mtime-watching + hot recompilation.

;; How many iterations to decide if a complex c is in the Mandelbrot set.
;; More = prettier edges, slower to compute.
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

;; Map each pixel to a complex coordinate, count iterations, color from it.
;; Slow zoom driven by wall-clock time.
(defn-native render-pixel ^i64 [^i64 px ^i64 py ^i64 frame ^i64 t-ms]
  (let [width  960.0
        height 540.0
        zoom   (+ 5.0 (* 0.50515 (float t-ms)))
        cr-c   -0.743643887037151
        ci-c    0.131825904205330
        fx     (float px)
        fy     (float py)
        cr     (+ cr-c (/ (- (* 3.5 (/ fx width))  1.75) zoom))
        ci     (+ ci-c (/ (- (* 2.0 (/ fy height)) 1.0)  zoom))
        iters  (mandel-iter cr ci 96)]
    (if (= iters 96)
      0
      ;; Simple RGB gradient indexed by iter count; cycled by frame.
      (let [shift (int (float (* frame 1)))
            r     (* (+ iters shift) 7)
            g     (* (+ iters shift) 3)
            b     (* (+ iters shift) 13)]
        (+ (* 65536 r) (* 256 g) b)))))
