;; cljrs core prelude — loaded automatically by builtins::install.
;;
;; Authored in cljrs itself, so it doubles as a smoke test of the
;; reader + evaluator + macro system on every startup.

;; ---------- threading macros ---------------------------------------------

(defmacro ->
  [x & forms]
  (if (empty? forms)
    x
    (let [form (first forms)
          more (rest forms)]
      (if (list? form)
        `(-> (~(first form) ~x ~@(rest form)) ~@more)
        `(-> (~form ~x) ~@more)))))

(defmacro ->>
  [x & forms]
  (if (empty? forms)
    x
    (let [form (first forms)
          more (rest forms)]
      (if (list? form)
        `(->> (~(first form) ~@(rest form) ~x) ~@more)
        `(->> (~form ~x) ~@more)))))

;; ---------- conditional macros -------------------------------------------
;; We don't have auto-gensym (`x#`) yet, so the hygienic names below use
;; `__name` prefixes. Good enough — these prelude macros never collide with
;; user code because `__` is reserved. When we land real gensyms, rewrite.

(defmacro when
  [test & body]
  `(if ~test (do ~@body) nil))

(defmacro when-not
  [test & body]
  `(if ~test nil (do ~@body)))

(defmacro cond
  [& clauses]
  (if (empty? clauses)
    nil
    (let [test (first clauses)
          then (first (rest clauses))
          more (rest (rest clauses))]
      (if (= test :else)
        then
        `(if ~test ~then (cond ~@more))))))

(defmacro if-let
  [binding then else]
  (let [name (first binding)
        expr (first (rest binding))]
    `(let [__iflet ~expr]
       (if __iflet
         (let [~name __iflet] ~then)
         ~else))))

(defmacro when-let
  [binding & body]
  (let [name (first binding)
        expr (first (rest binding))]
    `(let [__whenlet ~expr]
       (if __whenlet
         (let [~name __whenlet] ~@body)
         nil))))

;; ---------- boolean short-circuit ----------------------------------------

(defmacro and
  [& forms]
  (cond
    (empty? forms) true
    (empty? (rest forms)) (first forms)
    :else `(let [__and ~(first forms)]
             (if __and (and ~@(rest forms)) __and))))

(defmacro or
  [& forms]
  (cond
    (empty? forms) nil
    (empty? (rest forms)) (first forms)
    :else `(let [__or ~(first forms)]
             (if __or __or (or ~@(rest forms))))))

;; ---------- iteration macros ---------------------------------------------

(defmacro dotimes
  [binding & body]
  (let [n (first binding)
        cnt (first (rest binding))]
    `(loop [~n 0]
       (if (>= ~n ~cnt)
         nil
         (do ~@body (recur (+ ~n 1)))))))

(defmacro doseq
  [binding & body]
  (let [name (first binding)
        coll (first (rest binding))]
    `(loop [__seq ~coll]
       (if (empty? __seq)
         nil
         (let [~name (first __seq)]
           (do ~@body (recur (rest __seq))))))))

;; ---------- small utility fns --------------------------------------------

(defn inc-all [xs] (mapv inc xs))
(defn dec-all [xs] (mapv dec xs))
