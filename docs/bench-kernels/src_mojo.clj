;; Source kernels in the cljrs-mojo DSL. Running this file through
;; `cargo run -p cljrs-mojo --bin emit-mojo` produces the Mojo
;; companion files in this directory at each tier.
;;
;; Note: cljrs-mojo compiles at the kernel level (takes pointers,
;; produces vectorized Mojo) while the defn-native path compiles
;; the per-element op. Both are legitimate strategies — the Mojo
;; version has strictly more information (it sees the whole loop)
;; so its Max-tier output uses `vectorize[]` with SIMD load/store.

(elementwise-mojo vector_add
  [^f64 a ^f64 b]
  ^f64
  (+ a b))

(elementwise-mojo saxpy
  [^f64 a ^f64 x ^f64 y]
  ^f64
  (+ (* a x) y))

(reduce-mojo dot
  [^f64 x ^f64 y]
  ^f64
  (* x y)
  0.0)

(reduce-mojo sum_sq
  [^f64 x]
  ^f64
  (* x x)
  0.0)
