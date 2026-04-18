;; Userland shader "standard library" for cljrs `defn-gpu-pixel` kernels.
;; Zero compiler changes: this file is just a bundle of `defmacro`s.
;; Every helper expands inline via syntax-quote, so the GPU emitter sees
;; a single flat expression tree — exactly as if you had written it by hand.
;;
;; Usage: put this at the top of a kernel file, before `defn-gpu-pixel`:
;;
;;     (load-file "demo_gpu/stdlib.clj")
;;
;; The `gpu-compile` binary evaluates every non-kernel form into its env
;; before extracting the kernel body, so `load-file` registers these
;; macros in time for macroexpand-all. Paths resolve relative to the
;; process cwd; `docs/build.sh` runs from the repo root, so the relative
;; path above is what matters in practice.
;;
;; The DSL is scalar-only (no vec/mat types at the user level), so helpers
;; that would naturally return vec3 either (a) return a single channel
;; parameterized by a `channel` i32, or (b) expand to a let-block that
;; binds three names you destructure yourself.
;;
;; Contents:
;;   pack-rgb                 — clamp+pack three f32 channels to a u32 pixel
;;   hash2, noise2, fbm2      — value-noise building blocks
;;   sd-sphere / sd-box / sd-plane / sd-torus         — SDF primitives
;;   sd-union / sd-intersect / sd-subtract            — SDF combinators
;;   sd-smooth-union          — polynomial smooth-min soft union
;;   tone-reinhard            — x/(1+x) tonemap on a single channel

;; --- Color packing --------------------------------------------------------

;; Clamp r,g,b in [0,1] to 8-bit and pack into one 0x00RRGGBB u32. Every
;; kernel ends with this; factoring it out saves 4 lines at the bottom.
(defmacro pack-rgb [r g b]
  `(let [ri# (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 ~r)))))
         gi# (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 ~g)))))
         bi# (u32 (min (i32 255) (max (i32 0) (i32 (* 255.0 ~b)))))]
     (bit-or (bit-or (bit-shift-left ri# (u32 16))
                     (bit-shift-left gi# (u32 8)))
             bi#)))

;; --- Noise ---------------------------------------------------------------

;; Hash integer (ix,iy) coords to f32 in [0,1). xxHash-flavored; every
;; literal fits in i32 so the emitter's integer path is happy.
(defmacro hash2 [ix iy]
  `(let [n# (+ (* (u32 ~ix) (u32 73856093))
               (* (u32 ~iy) (u32 19349663)))
         x# (bit-xor n# (u32 61))
         x# (bit-xor x# (bit-shift-right x# (u32 16)))
         x# (* x# (u32 668265261))
         x# (bit-xor x# (bit-shift-right x# (u32 13)))
         x# (* x# (u32 374761393))
         x# (bit-xor x# (bit-shift-right x# (u32 16)))]
     (/ (f32 (bit-and x# (u32 16777215))) 16777215.0)))

;; 2D value noise with smoothstep weights. Args are f32 coords.
(defmacro noise2 [xf yf]
  `(let [fx# ~xf
         fy# ~yf
         ix# (i32 (floor fx#))
         iy# (i32 (floor fy#))
         tx# (- fx# (floor fx#))
         ty# (- fy# (floor fy#))
         wx# (* (* tx# tx#) (- 3.0 (* 2.0 tx#)))
         wy# (* (* ty# ty#) (- 3.0 (* 2.0 ty#)))
         h00# (hash2 ix# iy#)
         h10# (hash2 (+ ix# 1) iy#)
         h01# (hash2 ix# (+ iy# 1))
         h11# (hash2 (+ ix# 1) (+ iy# 1))
         a#   (+ h00# (* wx# (- h10# h00#)))
         b#   (+ h01# (* wx# (- h11# h01#)))]
     (+ a# (* wy# (- b# a#)))))

;; 5-octave FBM normalized to ~[0,1].
(defmacro fbm2 [xf yf]
  `(let [n1# (noise2 ~xf ~yf)
         n2# (noise2 (* ~xf 2.07) (* ~yf 2.07))
         n3# (noise2 (* ~xf 4.13) (* ~yf 4.13))
         n4# (noise2 (* ~xf 8.19) (* ~yf 8.19))
         n5# (noise2 (* ~xf 16.3) (* ~yf 16.3))
         s#  (+ (+ (* n1# 0.5) (* n2# 0.25))
                (+ (* n3# 0.125) (+ (* n4# 0.0625) (* n5# 0.03125))))]
     (/ s# 0.96875)))

;; --- SDF primitives ------------------------------------------------------

;; All primitives take point (px,py,pz) as first three args. Returning a
;; single f32 (signed distance) keeps them composable with the combinators
;; below — no vec plumbing needed.

;; Unit sphere of radius `r` centered at (cx,cy,cz).
(defmacro sd-sphere [px py pz cx cy cz r]
  `(let [ax# (- ~px ~cx)
         ay# (- ~py ~cy)
         az# (- ~pz ~cz)]
     (- (sqrt (+ (+ (* ax# ax#) (* ay# ay#)) (* az# az#))) ~r)))

;; Axis-aligned box of half-extents (hx,hy,hz) centered at (cx,cy,cz).
(defmacro sd-box [px py pz cx cy cz hx hy hz]
  `(let [qx# (- (abs (- ~px ~cx)) ~hx)
         qy# (- (abs (- ~py ~cy)) ~hy)
         qz# (- (abs (- ~pz ~cz)) ~hz)
         ox# (max 0.0 qx#)
         oy# (max 0.0 qy#)
         oz# (max 0.0 qz#)
         outside# (sqrt (+ (+ (* ox# ox#) (* oy# oy#)) (* oz# oz#)))
         inside#  (min 0.0 (max qx# (max qy# qz#)))]
     (+ outside# inside#)))

;; Infinite Y=`y0` plane. Negative below, positive above.
(defmacro sd-plane [px py pz y0]
  `(- ~py ~y0))

;; Torus of major radius `R` and minor radius `r` centered at (cx,cy,cz),
;; axis aligned with Y.
(defmacro sd-torus [px py pz cx cy cz R r]
  `(let [ax# (- ~px ~cx)
         ay# (- ~py ~cy)
         az# (- ~pz ~cz)
         qx# (- (sqrt (+ (* ax# ax#) (* az# az#))) ~R)]
     (- (sqrt (+ (* qx# qx#) (* ay# ay#))) ~r)))

;; --- SDF combinators -----------------------------------------------------

(defmacro sd-union [a b]
  `(min ~a ~b))

(defmacro sd-intersect [a b]
  `(max ~a ~b))

;; Subtract `b` from `a` (carve b out of a).
(defmacro sd-subtract [a b]
  `(max ~a (- ~b)))

;; Polynomial smooth-min union with blend radius `k` (in world units).
;; This is the standard IQ formulation; `k -> 0` recovers sd-union.
(defmacro sd-smooth-union [a b k]
  `(let [aa# ~a
         bb# ~b
         kk# ~k
         h#  (max 0.0 (- 1.0 (/ (abs (- aa# bb#)) (max 0.0001 kk#))))]
     (- (min aa# bb#)
        (* (* kk# h#) (* h# (* h# (/ 1.0 6.0)))))))

;; --- Tonemapping ---------------------------------------------------------

;; Reinhard v/(1+v) — single channel. Keeps highlights from clipping
;; when you sum several light contributions.
(defmacro tone-reinhard [v]
  `(let [vv# ~v]
     (/ vv# (+ 1.0 vv#))))
