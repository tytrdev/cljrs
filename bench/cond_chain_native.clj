;; Native port of cond_chain. Phase-2 emitter can't do cross-fn calls, so
;; `classify` is inlined as a nested if-chain inside the counter fn.
;; Semantics match the portable cond_chain.clj: count how many of 1..10
;; are unmatched (only 10 is, so result = 1).
(defn-native count-others ^i64 [^i64 i ^i64 acc]
  (if (> i 10)
    acc
    (let [matched (if (= i 1) 1
                    (if (= i 2) 1
                      (if (= i 3) 1
                        (if (= i 4) 1
                          (if (= i 5) 1
                            (if (= i 6) 1
                              (if (= i 7) 1
                                (if (= i 8) 1
                                  (if (= i 9) 1
                                    0)))))))))]
      (count-others (+ i 1) (if (= matched 0) (+ acc 1) acc)))))

(count-others 1 0)
