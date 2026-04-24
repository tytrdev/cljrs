;; Logistic regression kernels in the cljrs-mojo DSL. These are the ONLY
;; Mojo kernels used by the training harness — everything else
;; (data loading, the epoch loop, metrics) is plain Mojo scaffolding
;; that just happens to call these.
;;
;; The workflow:
;;   forward:   z = X @ w + b     using `dot` over each row
;;              p = sigmoid(z)    using `sigmoid`
;;              err = p - y       using `vsub`
;;   backward:  g_j = (1/N) Σ err[i] * X[i,j]  using `dot` again
;;              g_b = (1/N) Σ err[i]            using `vsum`
;;   update:    w -= lr * g                    using `update_weights`
;;              b -= lr * g_b                  (scalar, done in harness)

(reduce-mojo dot [^f64 a ^f64 b] ^f64 (* a b) 0.0)

(elementwise-mojo sigmoid
  [^f64 z]
  ^f64
  (/ 1.0 (+ 1.0 (exp (- 0.0 z)))))

(elementwise-mojo vsub
  [^f64 a ^f64 b]
  ^f64
  (- a b))

(reduce-mojo vsum [^f64 x] ^f64 x 0.0)

(elementwise-mojo update_weights
  [^f64 w ^f64 g ^scalar ^f64 lr]
  ^f64
  (- w (* lr g)))
