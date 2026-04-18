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

;; some-> : thread while each step is non-nil, else short-circuit to nil.
;; Auto-gensym (`v#`) gets a fresh name per syntax-quote, so macros that
;; need to SHARE a name across multiple quoted fragments use (gensym)
;; explicitly. Each recursive expansion still creates its own fresh g,
;; so nested expansions don't collide.
(defmacro some->
  [x & forms]
  (if (empty? forms)
    x
    (let [form (first forms)
          more (rest forms)
          g    (gensym "v_")
          step (if (list? form)
                 (concat (list (first form) g) (rest form))
                 (list form g))]
      `(let [~g ~x]
         (if (nil? ~g) nil (some-> ~step ~@more))))))

;; some->> : like some-> but threads into the last position.
(defmacro some->>
  [x & forms]
  (if (empty? forms)
    x
    (let [form (first forms)
          more (rest forms)
          g    (gensym "v_")
          step (if (list? form)
                 (concat (list (first form)) (rest form) (list g))
                 (list form g))]
      `(let [~g ~x]
         (if (nil? ~g) nil (some->> ~step ~@more))))))

;; cond-> : like -> but each step is [test form]; skipped when test is false.
(defmacro cond->
  [x & clauses]
  (if (empty? clauses)
    x
    (let [test (first clauses)
          form (first (rest clauses))
          more (rest (rest clauses))
          g    (gensym "v_")
          step (if (list? form)
                 (concat (list (first form) g) (rest form))
                 (list form g))]
      `(let [~g ~x]
         (cond-> (if ~test ~step ~g) ~@more)))))

;; cond->> : cond-> but threads into last position.
(defmacro cond->>
  [x & clauses]
  (if (empty? clauses)
    x
    (let [test (first clauses)
          form (first (rest clauses))
          more (rest (rest clauses))
          g    (gensym "v_")
          step (if (list? form)
                 (concat (list (first form)) (rest form) (list g))
                 (list form g))]
      `(let [~g ~x]
         (cond->> (if ~test ~step ~g) ~@more)))))

;; ---------- conditional macros -------------------------------------------
;; Hygienic locals via auto-gensym (`v#` expands to a fresh unique symbol).

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
    `(let [v# ~expr]
       (if v#
         (let [~name v#] ~then)
         ~else))))

(defmacro when-let
  [binding & body]
  (let [name (first binding)
        expr (first (rest binding))]
    `(let [v# ~expr]
       (if v#
         (let [~name v#] ~@body)
         nil))))

;; ---------- boolean short-circuit ----------------------------------------

(defmacro and
  [& forms]
  (cond
    (empty? forms) true
    (empty? (rest forms)) (first forms)
    :else `(let [v# ~(first forms)]
             (if v# (and ~@(rest forms)) v#))))

(defmacro or
  [& forms]
  (cond
    (empty? forms) nil
    (empty? (rest forms)) (first forms)
    :else `(let [v# ~(first forms)]
             (if v# v# (or ~@(rest forms))))))

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
    `(loop [xs# ~coll]
       (if (empty? xs#)
         nil
         (let [~name (first xs#)]
           (do ~@body (recur (rest xs#))))))))

;; ---------- list comprehension -------------------------------------------
;; Simplified `for`: (for [x xs :when pred] body). Single binding only;
;; :when optional. Builds an eager list. For the general form with
;; :let / :while / multiple bindings, defer to a future version.

(defmacro for
  [bindings body]
  (let [sym  (first bindings)
        coll (first (rest bindings))
        rest-pairs (rest (rest bindings))
        when-pred (if (and (not (empty? rest-pairs))
                           (= (first rest-pairs) :when))
                    (first (rest rest-pairs))
                    nil)]
    (if when-pred
      `(vec (for-filter (fn [~sym] ~when-pred)
                       (fn [~sym] ~body)
                       ~coll))
      `(mapv (fn [~sym] ~body) ~coll))))

(defn for-filter [pred f coll]
  (loop [xs coll acc []]
    (if (empty? xs)
      acc
      (let [h (first xs)]
        (recur (rest xs)
               (if (pred h) (conj acc (f h)) acc))))))

;; ---------- small utility fns --------------------------------------------

(defn inc-all [xs] (mapv inc xs))
(defn dec-all [xs] (mapv dec xs))
