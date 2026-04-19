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

;; ---------- transducers --------------------------------------------------
;; Shadow the built-in sequence fns with multi-arity variants:
;;   1 arg  -> a transducer (rf -> rf')
;;   2 args -> delegate to the original eager builtin (preserved under a
;;             __*-coll alias)
;;
;; A transducer rf has three arities: (rf) init, (rf result) complete,
;; (rf result input) step. Composition via comp flows left to right.

(defn map
  ([f]
   (fn [rf]
     (fn
       ([] (rf))
       ([result] (rf result))
       ([result input] (rf result (f input))))))
  ([f coll] (__map-coll f coll))
  ([f c1 c2]
   ;; (rest x) returns (), which is truthy — only seq normalises to nil
   ;; on empty. Without re-seq'ing each step the loop never exits.
   (loop [a (seq c1) b (seq c2) acc []]
     (if (and a b)
       (recur (seq (rest a)) (seq (rest b))
              (conj acc (f (first a) (first b))))
       acc)))
  ([f c1 c2 c3]
   (loop [a (seq c1) b (seq c2) c (seq c3) acc []]
     (if (and a b c)
       (recur (seq (rest a)) (seq (rest b)) (seq (rest c))
              (conj acc (f (first a) (first b) (first c))))
       acc))))

(defn filter
  ([pred]
   (fn [rf]
     (fn
       ([] (rf))
       ([result] (rf result))
       ([result input]
        (if (pred input) (rf result input) result)))))
  ([pred coll] (__filter-coll pred coll)))

(defn remove
  ([pred] (filter (complement pred)))
  ([pred coll] (filter (complement pred) coll)))

(defn take
  ([n]
   (fn [rf]
     (let [counter (atom n)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (let [remaining @counter]
            (if (<= remaining 0)
              (reduced result)
              (do
                (swap! counter dec)
                (if (<= remaining 1)
                  (reduced (rf result input))
                  (rf result input))))))))))
  ([n coll] (__take-coll n coll)))

(defn drop
  ([n]
   (fn [rf]
     (let [counter (atom n)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (if (pos? @counter)
            (do (swap! counter dec) result)
            (rf result input)))))))
  ([n coll] (__drop-coll n coll)))

(defn take-while
  ([pred]
   (fn [rf]
     (fn
       ([] (rf))
       ([result] (rf result))
       ([result input]
        (if (pred input) (rf result input) (reduced result))))))
  ([pred coll] (__take-while-coll pred coll)))

(defn drop-while
  ([pred]
   (fn [rf]
     (let [dropping (atom true)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (if (and @dropping (pred input))
            result
            (do (reset! dropping false)
                (rf result input))))))))
  ([pred coll] (__drop-while-coll pred coll)))

(defn mapcat
  ([f] (comp (map f) (fn [rf]
                       (fn
                         ([] (rf))
                         ([result] (rf result))
                         ([result input] (reduce rf result input))))))
  ([f coll] (__mapcat-coll f coll)))

;; (into to xform from): eager into with an xform. 2-arg fallback to
;; the original into (from builtins).
(def __into-no-xf into)
(defn into
  ([] [])
  ([to] to)
  ([to from] (__into-no-xf to from))
  ([to xform from] (transduce xform conj to from)))

(defn sequence
  ([] [])
  ([coll] (vec (seq coll)))
  ([xform coll] (transduce xform conj [] coll)))

;; Additional transducer-aware fns. Each follows the same shape: 1-arg
;; returns a transducer, 2-arg delegates to a __*-coll builtin.

(defn partition-all
  ([n]
   (fn [rf]
     (let [chunk (atom [])]
       (fn
         ([] (rf))
         ([result]
          (let [pending @chunk]
            (if (empty? pending)
              (rf result)
              (rf (rf result pending)))))
         ([result input]
          (swap! chunk conj input)
          (if (= (count @chunk) n)
            (let [c @chunk]
              (reset! chunk [])
              (rf result c))
            result))))))
  ([n coll] (__partition-all-coll n coll)))

(defn dedupe
  ([]
   (fn [rf]
     (let [last (atom ::none)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (let [prev @last]
            (reset! last input)
            (if (= prev input) result (rf result input))))))))
  ([coll] (__dedupe-coll coll)))

(defn take-nth
  ([n]
   (fn [rf]
     (let [counter (atom 0)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (let [i @counter]
            (swap! counter inc)
            (if (zero? (mod i n)) (rf result input) result)))))))
  ([n coll] (__take-nth-coll n coll)))

(defn keep
  ([f]
   (fn [rf]
     (fn
       ([] (rf))
       ([result] (rf result))
       ([result input]
        (let [v (f input)]
          (if (nil? v) result (rf result v)))))))
  ([f coll] (__keep-coll f coll)))

(defn keep-indexed
  ([f]
   (fn [rf]
     (let [idx (atom 0)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (let [i @idx]
            (swap! idx inc)
            (let [v (f i input)]
              (if (nil? v) result (rf result v)))))))))
  ([f coll] (__keep-indexed-coll f coll)))

(defn map-indexed
  ([f]
   (fn [rf]
     (let [idx (atom 0)]
       (fn
         ([] (rf))
         ([result] (rf result))
         ([result input]
          (let [i @idx]
            (swap! idx inc)
            (rf result (f i input))))))))
  ([f coll] (__map-indexed-coll f coll)))

;; cat: a transducer that splices each step's input collection into
;; the result. (transduce (comp (map identity) cat) conj [] [[1 2] [3 4]])
;; -> [1 2 3 4]
(def cat
  (fn [rf]
    (fn
      ([] (rf))
      ([result] (rf result))
      ([result input] (reduce rf result input)))))

;; ---------- threading: as-> ---------------------------------------------
;; Bind `name` to expr, then thread it through subsequent forms with
;; explicit positional control.

(defmacro as->
  [expr name & forms]
  (if (empty? forms)
    expr
    `(let [~name ~expr]
       (as-> ~(first forms) ~name ~@(rest forms)))))

;; ---------- defn- (private) --------------------------------------------
;; Same as defn for now — namespace privacy is informational only since
;; we don't enforce visibility, but the form is accepted for compat.

(defmacro defn-
  [name & body]
  `(defn ~name ~@body))

;; ---------- lazy-cat ---------------------------------------------------
;; Lazily concat an arbitrary number of collections. Each one is wrapped
;; in lazy-seq so the whole chain is forced incrementally.

(defmacro lazy-cat
  [& colls]
  (if (empty? colls)
    `(lazy-seq nil)
    `(lazy-cat-helper (lazy-seq ~(first colls))
                      (fn [] (lazy-cat ~@(rest colls))))))

(defn lazy-cat-helper [head rest-fn]
  (lazy-seq
    (if (empty? head)
      (rest-fn)
      (cons (first head) (lazy-cat-helper (rest head) rest-fn)))))

;; ---------- small utility fns --------------------------------------------

(defn inc-all [xs] (mapv inc xs))
(defn dec-all [xs] (mapv dec xs))

;; ---------- additional conditional macros --------------------------------

(defmacro if-not
  ([test then] `(if ~test nil ~then))
  ([test then else] `(if ~test ~else ~then)))

(defmacro if-some
  ([binding then] `(if-some ~binding ~then nil))
  ([binding then else]
   (let [name (first binding)
         expr (first (rest binding))]
     `(let [v# ~expr]
        (if (nil? v#)
          ~else
          (let [~name v#] ~then))))))

(defmacro when-some
  [binding & body]
  (let [name (first binding)
        expr (first (rest binding))]
    `(let [v# ~expr]
       (if (nil? v#)
         nil
         (let [~name v#] ~@body)))))

;; condp: (condp pred expr clause...) where each clause is either
;;   test-expr result-expr
;; or test-expr :>> result-fn  (calls result-fn with the pred result).
;; A trailing single form is treated as the default.
(defmacro condp
  [pred expr & clauses]
  (let [pv (gensym "pred_")
        ev (gensym "expr_")]
    (letfn-build pv ev pred expr (vec clauses))))

(defn letfn-build [pv ev pred expr clauses]
  ;; helper that emits the cascading if for condp.
  `(let [~pv ~pred ~ev ~expr]
     ~(condp-emit pv ev (vec clauses))))

(defn condp-emit [pv ev clauses]
  (cond
    (empty? clauses)
    `(throw (ex-info "No matching clause" {}))

    (empty? (rest clauses))
    ;; lone trailing clause = default value
    (first clauses)

    (= (first (rest clauses)) :>>)
    ;; (test :>> result-fn) — call result-fn on (pred test expr)
    (let [test (first clauses)
          result-fn (first (rest (rest clauses)))
          more (rest (rest (rest clauses)))
          rv (gensym "rv_")]
      `(let [~rv (~pv ~test ~ev)]
         (if ~rv
           (~result-fn ~rv)
           ~(condp-emit pv ev (vec more)))))

    :else
    (let [test (first clauses)
          result (first (rest clauses))
          more (rest (rest clauses))]
      `(if (~pv ~test ~ev)
         ~result
         ~(condp-emit pv ev (vec more))))))

;; ---------- assertions ---------------------------------------------------

(defmacro assert
  ([expr]
   `(when-not ~expr
      (throw (ex-info (str "Assert failed: " (quote ~expr)) {}))))
  ([expr message]
   `(when-not ~expr
      (throw (ex-info (str "Assert failed: " ~message "\n" (quote ~expr)) {})))))

;; ---------- comment ------------------------------------------------------
;; Drops its body entirely; always returns nil. Useful for inline notes
;; or temporarily disabling a top-level form.

(defmacro comment [& _body] nil)

;; ---------- letfn --------------------------------------------------------
;; Mutually-recursive local fns. Each binding is (fn-name [params] body...).
;; We use atoms as forward references: each name is initially bound to a
;; trampoline that dereferences the atom; then we install the real fns.

(defmacro letfn
  [fnspecs & body]
  (let [n (count fnspecs)
        ;; Pre-compute parallel vectors of names + matching atom syms.
        names    (mapv first fnspecs)
        atoms    (mapv (fn [_] (gensym "lf_")) names)
        ;; Build the let-binding pairs as one flat vector.
        atom-bs  (reduce (fn [acc a] (conj acc a `(atom nil))) [] atoms)
        tramp-bs (loop [i 0 acc []]
                   (if (>= i n)
                     acc
                     (recur (+ i 1)
                            (conj acc (nth names i)
                                  `(fn [& args#]
                                     (apply (deref ~(nth atoms i)) args#))))))
        install-bs (loop [i 0 acc []]
                     (if (>= i n)
                       acc
                       (let [spec   (nth fnspecs i)
                             params (first (rest spec))
                             fbody  (rest (rest spec))]
                         (recur (+ i 1)
                                (conj acc (gensym "_")
                                      `(reset! ~(nth atoms i)
                                               (fn ~params ~@fbody)))))))]
    `(let ~(vec (concat atom-bs tramp-bs install-bs))
       ~@body)))

;; ---------- delay / force ------------------------------------------------
;; A delay is a tagged map {:__delay true :state (atom [:pending thunk])}.
;; The first force evaluates the thunk and caches the result.
;;
;; NOTE: realized? is a builtin that checks for LazySeq specifically; on
;; a delay it will wrongly return true. Use delay-realized? for delays.

(defn delay? [d]
  (and (map? d) (= (get d :__delay) true)))

(defn force [d]
  (if (delay? d)
    (let [st (get d :state)
          cur (deref st)]
      (if (= (first cur) :pending)
        (let [v ((first (rest cur)))]
          (reset! st [:done v])
          v)
        (first (rest cur))))
    d))

(defn delay-realized? [d]
  (and (delay? d) (= (first (deref (get d :state))) :done)))

(defmacro delay
  [& body]
  `(hash-map :__delay true :state (atom [:pending (fn [] ~@body)])))

;; Override the builtin `realized?` so it consults a delay's state when
;; given a delay map. For LazySeqs and other inputs we delegate to the
;; original built-in semantics (only LazySeq is "unrealized" there).
(let [builtin-realized? realized?]
  (defn realized? [x]
    (cond
      (delay? x) (delay-realized? x)
      :else (builtin-realized? x))))

;; ---------- sequence accessors ------------------------------------------

;; seq-coerce each level so nil and atoms pun through without crashing
;; ((ffirst nil) → nil rather than "first on non-sequence: nil").
(defn ffirst [coll] (first (seq (first (seq coll)))))
(defn fnext  [coll] (first (seq (rest  (seq coll)))))
(defn nfirst [coll] (rest  (seq (first (seq coll)))))
(defn nnext  [coll] (rest  (seq (rest  (seq coll)))))

;; ---------- not-any? / not-every? ---------------------------------------

(defn not-any? [pred coll] (not (some pred coll)))
(defn not-every? [pred coll] (not (every? pred coll)))

;; ---------- replace (sequence form) -------------------------------------
;; Replace each element of coll by looking it up in the smap (a map). If
;; not found, the element is kept as-is. 1-arg returns a transducer.

(defn replace
  ([smap]
   (map (fn [x] (if (contains? smap x) (get smap x) x))))
  ([smap coll]
   (mapv (fn [x] (if (contains? smap x) (get smap x) x)) coll)))

;; ---------- every-pred / some-fn / fnil ---------------------------------

(defn every-pred
  ([p]
   (fn [& args] (every? p args)))
  ([p1 p2]
   (fn [& args]
     (and (every? p1 args) (every? p2 args))))
  ([p1 p2 & ps]
   (let [all (cons p1 (cons p2 ps))]
     (fn [& args]
       (every? (fn [p] (every? p args)) all)))))

(defn some-fn
  ([p]
   (fn [& args] (some p args)))
  ([p1 p2]
   (fn [& args]
     (or (some p1 args) (some p2 args))))
  ([p1 p2 & ps]
   (let [all (cons p1 (cons p2 ps))]
     (fn [& args]
       (some (fn [p] (some p args)) all)))))

(defn fnil
  ([f a]
   (fn
     ([x] (f (if (nil? x) a x)))
     ([x y] (f (if (nil? x) a x) y))
     ([x y z] (f (if (nil? x) a x) y z))
     ([x y z & more] (apply f (if (nil? x) a x) y z more))))
  ([f a b]
   (fn
     ([x y] (f (if (nil? x) a x) (if (nil? y) b y)))
     ([x y z] (f (if (nil? x) a x) (if (nil? y) b y) z))
     ([x y z & more] (apply f (if (nil? x) a x) (if (nil? y) b y) z more))))
  ([f a b c]
   (fn
     ([x y z] (f (if (nil? x) a x) (if (nil? y) b y) (if (nil? z) c z)))
     ([x y z & more]
      (apply f (if (nil? x) a x) (if (nil? y) b y) (if (nil? z) c z) more)))))

;; ---------- numeric == ---------------------------------------------------
;; Variadic numeric equality. Coerces ints/floats to a common comparison
;; via subtraction-equals-zero. cljrs's = already compares ints and floats
;; structurally, so == here is essentially a numeric-typed alias.

(defn ==
  ([_x] true)
  ([x y] (and (number? x) (number? y) (zero? (- x y))))
  ([x y & more]
   (if (== x y)
     (loop [a y bs more]
       (if (empty? bs)
         true
         (if (== a (first bs))
           (recur (first bs) (rest bs))
           false)))
     false)))

;; ---------- printing -----------------------------------------------------
;; pr / prn print the readable representation (via pr-str). println /
;; print already exist as builtins and use the display representation.

(defn pr [& args]
  (print (apply pr-str args)))

(defn prn [& args]
  (println (apply pr-str args)))

(defn prn-str [& args]
  (str (apply pr-str args) "\n"))

;; ---------- control flow: case / when-first / while / time / halt-when ---

;; case — single-dispatch on compile-time literal keys. Unlike Clojure's
;; constant-time hash-table form, we expand to a chain of `=` tests; for
;; the small clause counts in real code this is plenty fast and keeps the
;; macro implementable without compile-time interning. A trailing odd
;; argument is the default; if no clauses match and there's no default,
;; an IllegalArgumentException-style ex-info is thrown — matching JVM
;; Clojure's "No matching clause" semantics.
;;
;; Key forms:
;;   (case x  k    result ...)
;;   (case x (k1 k2) result ...)   ;; list-of-keys = match any
;; Keys are NEVER evaluated — they're matched as data via (= x 'k).
(defn case-emit [g clauses]
  (cond
    (empty? clauses)
    ;; NB: syntax-quote here intentionally avoids a `{:value ~g}` map
    ;; literal — the reader doesn't recurse unquotes through map keys.
    `(throw (ex-info (str "No matching clause: " ~g) (hash-map :value ~g)))

    (empty? (rest clauses))
    ;; lone trailing clause = default value
    (first clauses)

    :else
    (let [k    (first clauses)
          v    (first (rest clauses))
          more (rest (rest clauses))
          test (if (list? k)
                 ;; (k1 k2 k3) — match any
                 (cons 'or (map (fn [ki] `(= ~g (quote ~ki))) k))
                 `(= ~g (quote ~k)))]
      `(if ~test ~v ~(case-emit g (vec more))))))

(defmacro case
  [e & clauses]
  (let [g (gensym "case_")]
    `(let [~g ~e]
       ~(case-emit g (vec clauses)))))

;; when-first — evaluate body with x bound to (first coll) when the
;; coll is non-empty. Uses `seq` (not `empty?`) so it works with lazy
;; seqs and treats `nil` as empty.
(defmacro when-first
  [binding & body]
  (let [name (first binding)
        coll (first (rest binding))]
    `(let [s# (seq ~coll)]
       (when s#
         (let [~name (first s#)]
           ~@body)))))

;; while — loop body while test is truthy. Returns nil.
(defmacro while
  [test & body]
  `(loop []
     (when ~test
       ~@body
       (recur))))

;; time — eval expr, print "Elapsed time: N msecs" and return its value.
;; Uses (time-ms) host builtin; matches Clojure's stderr-ish format
;; closely enough for benchmarking.
(defmacro time
  [expr]
  `(let [start# (time-ms)
         ret#   ~expr
         end#   (time-ms)]
     (println (str "\"Elapsed time: " (- end# start#) " msecs\""))
     ret#))

;; halt-when — transducer that halts reduction the first time pred is
;; true. Default behaviour: result becomes the offending input. With a
;; ret-fn, result becomes (ret-fn current-result input).
;;
;; To match Clojure's semantics, the halt value is wrapped in a special
;; sentinel map {::halt v} when handed back through (reduced ...). The
;; completion arity unwraps that sentinel and returns the bare value
;; (without re-running rf's completion), so downstream rf state never
;; sees the halt value as a legitimate result.
(def __halt-key ::halt-when-sentinel)

(defn halt-when
  ([pred] (halt-when pred nil))
  ([pred ret-fn]
   (fn [rf]
     (fn
       ([] (rf))
       ([result]
        (if (and (map? result) (contains? result __halt-key))
          (get result __halt-key)
          (rf result)))
       ([result input]
        (if (pred input)
          (reduced (hash-map __halt-key
                             (if (nil? ret-fn) input (ret-fn result input))))
          (rf result input)))))))

;; ---------- atoms: swap-vals! / reset-vals! ------------------------------
;; Both return [old-value new-value]. Non-atomic w.r.t. concurrent
;; writers — matches the existing swap!/reset! semantics in cljrs.

(defn swap-vals!
  ([a f]
   (let [old @a
         new (f old)]
     (reset! a new)
     [old new]))
  ([a f x]
   (let [old @a
         new (f old x)]
     (reset! a new)
     [old new]))
  ([a f x y]
   (let [old @a
         new (f old x y)]
     (reset! a new)
     [old new]))
  ([a f x y & more]
   (let [old @a
         new (apply f old x y more)]
     (reset! a new)
     [old new])))

(defn reset-vals! [a newval]
  (let [old @a]
    (reset! a newval)
    [old newval]))

;; ---------- hierarchies --------------------------------------------------
;; A hierarchy is a map with three keys, each itself a map from tag to
;; the set of related tags:
;;   :parents      tag -> direct parents
;;   :ancestors    tag -> transitive ancestors (parents + their ancestors)
;;   :descendants  tag -> transitive descendants
;;
;; Tags are typically keywords or symbols, but anything `=`-comparable
;; works. Numeric/Class isa? (e.g. (isa? 42 Number)) is JVM-specific and
;; intentionally NOT supported here.
;;
;; The 1-arity forms read/write a global hierarchy stored in an atom so
;; (derive ::child ::parent) mutates shared state, mirroring Clojure.

(defn make-hierarchy []
  {:parents {} :ancestors {} :descendants {}})

(def __global-hierarchy (atom (make-hierarchy)))

(defn __h-get-set [h k tag]
  (or (get (get h k) tag) #{}))

(defn __add-rel [m k v]
  ;; m is tag->set; ensure (m k) contains v (and exists).
  (let [cur (or (get m k) #{})]
    (assoc m k (conj cur v))))

(defn __union-into [m k vs]
  ;; m is tag->set; union vs into (m k).
  (let [cur (or (get m k) #{})]
    (assoc m k (reduce conj cur vs))))

(defn __remove-rel [m k v]
  (let [cur (or (get m k) #{})
        nxt (disj cur v)]
    (if (empty? nxt)
      (dissoc m k)
      (assoc m k nxt))))

;; derive — 2-arity mutates the global hierarchy; 3-arity returns an
;; updated hierarchy. Self-derivation throws; cyclic derivations throw.
(defn derive
  ([tag parent]
   (swap! __global-hierarchy (fn [h] (derive h tag parent)))
   nil)
  ([h tag parent]
   (cond
     (= tag parent)
     (throw (ex-info "tag must not be its own parent" {:tag tag}))

     (contains? (__h-get-set h :ancestors tag) parent)
     h  ;; already derived

     (or (= tag parent)
         (contains? (__h-get-set h :ancestors parent) tag))
     (throw (ex-info "Cyclic derivation" {:tag tag :parent parent}))

     :else
     (let [parents     (:parents h)
           ancestors   (:ancestors h)
           descendants (:descendants h)
           ;; All ancestors of parent (incl parent itself) become
           ;; ancestors of tag and of every descendant of tag.
           parent-and-ancs (conj (__h-get-set h :ancestors parent) parent)
           tag-and-descs   (conj (__h-get-set h :descendants tag) tag)
           new-parents     (__add-rel parents tag parent)
           new-ancestors   (reduce (fn [m d] (__union-into m d parent-and-ancs))
                                   ancestors
                                   tag-and-descs)
           new-descendants (reduce (fn [m a] (__union-into m a tag-and-descs))
                                   descendants
                                   parent-and-ancs)]
       {:parents new-parents
        :ancestors new-ancestors
        :descendants new-descendants}))))

(defn underive
  ([tag parent]
   (swap! __global-hierarchy (fn [h] (underive h tag parent)))
   nil)
  ([h tag parent]
   (if-not (contains? (__h-get-set h :parents tag) parent)
     h
     ;; Rebuild the hierarchy from scratch over the surviving direct
     ;; parent edges. This is O(edges^2) but trivially correct — and
     ;; the alternative (subtracting the right ancestor sets in place)
     ;; is subtle when multiple paths exist.
     (let [old-parents (:parents h)
           pruned      (__remove-rel old-parents tag parent)
           edges       (reduce (fn [acc [child ps]]
                                 (reduce (fn [a p] (conj a [child p])) acc ps))
                               []
                               pruned)]
       (reduce (fn [hh [c p]] (derive hh c p))
               (make-hierarchy)
               edges)))))

(defn parents
  ([tag] (parents @__global-hierarchy tag))
  ([h tag]
   (let [s (get (:parents h) tag)]
     (when (and s (not (empty? s))) s))))

(defn ancestors
  ([tag] (ancestors @__global-hierarchy tag))
  ([h tag]
   (let [s (get (:ancestors h) tag)]
     (when (and s (not (empty? s))) s))))

(defn descendants
  ([tag] (descendants @__global-hierarchy tag))
  ([h tag]
   (let [s (get (:descendants h) tag)]
     (when (and s (not (empty? s))) s))))

(defn isa?
  ([child parent] (isa? @__global-hierarchy child parent))
  ([h child parent]
   (cond
     (= child parent) true

     (and (vector? child) (vector? parent))
     (and (= (count child) (count parent))
          (loop [i 0]
            (cond
              (= i (count child)) true
              (isa? h (get child i) (get parent i)) (recur (+ i 1))
              :else false)))

     :else
     (contains? (__h-get-set h :ancestors child) parent))))
