;; Shared Clojure-family bench harness. Run as:
;;
;;   <impl> bench/clj_bench.clj <label> <bench-file.clj> [iters]
;;
;; e.g.
;;   clojure -M bench/clj_bench.clj jvm  bench/fib.clj 100
;;   bb       bench/clj_bench.clj bb   bench/fib.clj 100
;;
;; Mirrors src/bin/bench.rs: eval every form but the last as setup, wrap
;; the last form in (fn [] ...), warm up, then time N calls. Prints one
;; line in a uniform format, with `result=` at the end so bench/run.sh
;; can cross-check every impl agrees on the value.

(let [[label path iters-str] *command-line-args*
      iters (Long/parseLong (or iters-str "100"))
      src (slurp path)
      forms (read-string (str "[" src "]"))
      setup (butlast forms)
      bench-form (last forms)]
  (doseq [f setup] (eval f))
  (let [callable (eval (list 'fn [] bench-form))]
    (dotimes [_ 3] (callable))
    (let [start (System/nanoTime)
          _ (dotimes [_ iters] (callable))
          elapsed-ns (- (System/nanoTime) start)
          per-ns (/ (double elapsed-ns) iters)
          total-ms (/ elapsed-ns 1e6)
          final-result (callable)]
      (println (format "%-6s %-40s  iters=%-8d  total=%10.2fms  per-iter=%14.0fns  result=%s"
                       label path iters total-ms per-ns (pr-str final-result))))))
