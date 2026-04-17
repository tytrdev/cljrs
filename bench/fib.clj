;; Tree-recursive Fibonacci. Stresses function call path + arithmetic.
(defn fib [n]
  (if (< n 2)
    n
    (+ (fib (- n 1)) (fib (- n 2)))))

(fib 25)
