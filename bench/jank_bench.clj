;; Jank bench harness. Run as:
;;   jank run bench/jank_bench.clj -- <bench-file.clj> [iters]
;;
;; Jank (alpha) has no JVM interop - no System/nanoTime, no
;; with-out-str. It *does* have Clojure core's time macro, which
;; prints "Elapsed time: X ms" to stdout. We can't capture that
;; in-process, so this script emits marker lines and bench/run.sh
;; parses the elapsed time to produce the uniform result line.
;;
;; The jank CLI's own arg parser consumes positional args, so
;; callers must pass -- before script args (see run.sh).

(let [[path iters-str] *command-line-args*
      iters (parse-long (or iters-str "100"))
      src (slurp path)
      forms (read-string (str "[" src "]"))
      setup (butlast forms)
      bench-form (last forms)]
  (doseq [f setup] (eval f))
  (let [callable (eval (list 'fn [] bench-form))]
    (dotimes [_ 3] (callable))
    (println "===JANK_BENCH_START===" path iters)
    (time (dotimes [_ iters] (callable)))
    (println "===JANK_BENCH_RESULT===" (pr-str (callable)))
    (println "===JANK_BENCH_END===")))
