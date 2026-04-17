;; Native-compiled fib via defn-native + MLIR JIT.
;; Compare against bench/fib.clj (tree-walker fib) to see the MLIR delta.
;; Other Clojure impls (clojure-JVM, bb) don't recognize `defn-native` so
;; we feed them the same source and they'll warn-and-compile as a plain fn.
(defn-native fib ^i64 [^i64 n]
  (if (< n 2)
    n
    (+ (fib (- n 1)) (fib (- n 2)))))

(fib 25)
