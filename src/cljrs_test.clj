;; cljrs.test — clojure.test-style assertions authored in cljrs.
;;
;; Loaded after core.clj by `install_prelude`. Lives in cljrs.core so
;; `deftest`, `is`, `testing`, and `run-tests` are callable from any
;; namespace without a require form.

;; Registry of {:name :fn :ns} maps. Re-running a deftest replaces by name.
(defonce __test-registry (atom []))

;; Per-run counters; reset by run-tests.
(defonce __test-counters (atom {:tests 0 :assertions 0 :failures 0 :errors 0}))

;; Stack of (testing "label") context strings, top of stack = innermost.
(defonce __test-context (atom []))

(defn __register-test! [nm f]
  (let [entry {:name nm :fn f :ns "cljrs.test"}
        existing @__test-registry
        without (filterv (fn [e] (not (= (:name e) nm))) existing)]
    (reset! __test-registry (conj without entry))
    nm))

;; clojure.string isn't implemented — inline a tiny join.
(defn __join-ctx [xs]
  (if (empty? xs)
    ""
    (let [sep " > "]
      (reduce (fn [acc s] (if (= acc "") s (str acc sep s))) "" xs))))

(defmacro is
  ([expr] `(is ~expr nil))
  ([expr msg]
   ;; Recognize (is (= a b)) and (is (thrown? ...)) specially.
   (cond
     (and (list? expr) (= (first expr) '=))
     (let [a (first (rest expr))
           b (first (rest (rest expr)))]
       `(let [av# ~a bv# ~b]
          (swap! __test-counters update :assertions inc)
          (if (= av# bv#)
            true
            (throw (ex-info "assertion failed"
                            (hash-map :kind :fail
                                      :form (quote ~expr)
                                      :expected (quote ~expr)
                                      :actual (list (quote =) av# bv#)
                                      :lhs av# :rhs bv#
                                      :msg ~msg
                                      :context (deref __test-context)))))))

     (and (list? expr) (= (first expr) 'thrown?))
     ;; (thrown? body) or (thrown? ExType body...) — treat first arg as
     ;; type only if there's more than one arg.
     (let [args (rest expr)
           body (if (> (count args) 1) (rest args) args)]
       `(do
          (swap! __test-counters update :assertions inc)
          (let [threw?# (try
                          (do ~@body)
                          false
                          (catch _ __e__ true))]
            (if threw?#
              true
              (throw (ex-info "assertion failed: expected throw"
                              (hash-map :kind :fail
                                        :form (quote ~expr)
                                        :expected (quote ~expr)
                                        :actual :no-throw
                                        :msg ~msg
                                        :context (deref __test-context))))))))

     :else
     `(let [v# ~expr]
        (swap! __test-counters update :assertions inc)
        (if v#
          true
          (throw (ex-info "assertion failed"
                          (hash-map :kind :fail
                                    :form (quote ~expr)
                                    :expected (quote ~expr)
                                    :actual v#
                                    :msg ~msg
                                    :context (deref __test-context)))))))))

(defmacro testing [label & body]
  `(do
     (swap! __test-context conj ~label)
     (try
       (do ~@body)
       (finally
         (swap! __test-context (fn [s#] (vec (take (dec (count s#)) s#))))))))

(defn __report-failure [nm data]
  (let [ctx (__join-ctx (or (:context data) []))
        form (:form data)
        msg (:msg data)]
    (println (str "FAIL in " nm
                  (if (= ctx "") "" (str " (" ctx ")"))
                  (if msg (str " — " msg) "")))
    (println (str "  form: " (pr-str form)))
    (when (contains? data :lhs)
      (println (str "  lhs:  " (pr-str (:lhs data))))
      (println (str "  rhs:  " (pr-str (:rhs data)))))
    (when (and (not (contains? data :lhs)) (contains? data :actual))
      (println (str "  actual: " (pr-str (:actual data)))))))

(defn __report-error [nm e]
  (println (str "ERROR in " nm " — " (pr-str e))))

(defmacro deftest [nm & body]
  `(do
     (__register-test!
       '~nm
       (fn []
         (try
           (do ~@body)
           (catch _ __e__
             (let [d# (ex-data __e__)]
               (if (and (map? d#) (= (:kind d#) :fail))
                 (do
                   (swap! __test-counters update :failures inc)
                   (__report-failure '~nm d#))
                 (do
                   (swap! __test-counters update :errors inc)
                   (__report-error '~nm __e__))))))))
     '~nm))

(defn run-tests []
  (reset! __test-counters {:tests 0 :assertions 0 :failures 0 :errors 0})
  (reset! __test-context [])
  (let [tests @__test-registry
        ns-name (if (empty? tests) "cljrs.test" (:ns (first tests)))]
    (doseq [t tests]
      (swap! __test-counters update :tests inc)
      ((:fn t)))
    (let [c @__test-counters]
      (println (str "\nRan " (:tests c) " tests in " ns-name ". "
                    (:assertions c) " assertions. "
                    (:failures c) " failures. "
                    (:errors c) " errors."))
      c)))
