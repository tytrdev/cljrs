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
;; Full `for`: multiple bindings, :let, :when, :while, :when-not.
;; Eager (returns a vector). Builds nested loops by walking the binding
;; vector pair-by-pair and emitting reduce calls.

(defn build-for [bindings body]
  ;; Walks the binding vector. Returns a form that builds a vector of
  ;; results. Modifier keywords (:let, :when, :while, :when-not) are
  ;; consumed in place between iteration bindings.
  (if (empty? bindings)
    `(conj __for_acc__ ~body)
    (let [head (first bindings)
          rest-bs (rest bindings)
          following (vec rest-bs)]
      (cond
        (= head :let)
        `(let ~(first rest-bs)
           ~(build-for (vec (rest rest-bs)) body))

        (= head :when)
        `(if ~(first rest-bs)
           ~(build-for (vec (rest rest-bs)) body)
           __for_acc__)

        (= head :when-not)
        `(if ~(first rest-bs)
           __for_acc__
           ~(build-for (vec (rest rest-bs)) body))

        (= head :while)
        `(if (not ~(first rest-bs))
           (reduced __for_acc__)
           ~(build-for (vec (rest rest-bs)) body))

        :else
        (let [sym head
              coll (first rest-bs)
              following (vec (rest rest-bs))]
          `(reduce (fn [__for_acc__ ~sym]
                     ~(build-for following body))
                   __for_acc__
                   ~coll))))))

;; Top-level wrapper: seed the accumulator and unwrap on completion.
(defmacro for
  [bindings body]
  `(let [__for_acc__ []]
     ~(build-for (vec bindings) body)))

;; build-for relies on `__for_acc__` already being in scope. The wrapper
;; above supplies it. :while uses (reduced ...) as a sentinel; we don't
;; honor it yet so :while just becomes "skip this iter and continue."
;; Acceptable for now; full reduced-support is a follow-up.

;; ---------- records ------------------------------------------------------
;; Records are tagged maps: a map with a hidden :__type key set to the
;; record name keyword. We auto-generate:
;;   ->Name   constructor: positional args -> record
;;   map->Name map constructor: map -> record
;;   Name?    predicate
;; Fields are accessed as plain keyword keys; protocols (below) dispatch
;; via the :__type key.

(defmacro defrecord
  [name fields & body]
  (let [kw        (keyword (str name))
        ctor-sym  (symbol (str "->" name))
        mctor-sym (symbol (str "map->" name))
        pred-sym  (symbol (str name "?"))
        field-kws (mapv (fn [f] (keyword (str f))) fields)
        base-pairs (interleave field-kws fields)]
    ;; Emit the core definitions. Protocol bodies in `body` are handled
    ;; by a separate pass: `(defrecord ... ProtoName (method [this ...] body))`
    ;; expands each method into a defmethod on the proto's multifn.
    `(do
       ;; Primary definitions:
       (defn ~ctor-sym ~fields
         (hash-map :__type ~kw ~@base-pairs))
       (defn ~mctor-sym [m#]
         (assoc m# :__type ~kw))
       (defn ~pred-sym [x#]
         (and (map? x#) (= (get x# :__type) ~kw)))
       ;; Then any protocol method implementations.
       ~@(expand-record-methods kw body)
       ~kw)))

(defn expand-record-methods [kw forms]
  ;; forms looks like:  ProtoA (m1 [this ...] body) (m2 [...] body)
  ;;                    ProtoB (m3 [this ...] body) ...
  ;; Walk it, ignoring the protocol name tokens (they're just marker
  ;; symbols — methods install themselves via defmethod on the fn name).
  (loop [xs forms acc []]
    (if (empty? xs)
      acc
      (let [head (first xs)]
        (if (list? head)
          ;; A method list: (name [params...] body...)
          (let [mname  (first head)
                params (first (rest head))
                mbody  (rest (rest head))]
            (recur (rest xs)
                   (conj acc
                         (list 'defmethod mname kw params
                               (cons 'do mbody)))))
          ;; A bare symbol = protocol name, skip it.
          (recur (rest xs) acc))))))

;; ---------- protocols ----------------------------------------------------
;; A protocol is a named set of function names. Each function becomes a
;; multimethod dispatched on the first arg's :__type (or, for plain
;; values, the type-name keyword).

(defn type-kw [v]
  (cond
    (and (map? v) (contains? v :__type)) (get v :__type)
    (nil? v)     :nil
    (integer? v) :int
    (float? v)   :float
    (string? v)  :string
    (keyword? v) :keyword
    (symbol? v)  :symbol
    (vector? v)  :vector
    (map? v)     :map
    (set? v)     :set
    (list? v)    :list
    (fn? v)      :fn
    :else        :any))

(defmacro defprotocol
  [name & specs]
  ;; specs are function signatures like (method-name [this ...]) or
  ;; (method-name [this ...] "docstring"). Each expands to a defmulti.
  `(do
     ~@(map (fn [spec]
              (let [mname (first spec)]
                (list 'defmulti mname '(fn [& args] (type-kw (first args))))))
            specs)
     (quote ~name)))

;; ---------- lazy sequences -----------------------------------------------
;; (lazy-seq body) defers `body` until first/rest touches it. Works with
;; take, first, rest, empty? and composes for infinite sequences.

(defmacro lazy-seq
  [& body]
  `(__lazy-seq (fn [] (do ~@body))))

;; Infinite seq: x, f(x), f(f(x)), ...
(defn iterate [f x]
  (lazy-seq (cons x (iterate f (f x)))))

;; Infinite seq of (f) calls. `f` must be side-effecting or constant.
(defn repeatedly [f]
  (lazy-seq (cons (f) (repeatedly f))))

;; Infinite cycle of a collection's elements. The recursive walker is
;; defined as a top-level helper since we don't have letfn yet.
(defn cycle-step [xs original]
  (lazy-seq
    (if (empty? xs)
      (cycle-step original original)
      (cons (first xs) (cycle-step (rest xs) original)))))

(defn cycle [coll]
  (cycle-step coll coll))

;; ---------- small utility fns --------------------------------------------

(defn inc-all [xs] (mapv inc xs))
(defn dec-all [xs] (mapv dec xs))
