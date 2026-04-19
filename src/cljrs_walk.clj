;; clojure.walk — generic tree walker for Clojure data structures.
;;
;; Loaded by `install_prelude` (see src/builtins.rs). Lives in the
;; `clojure.walk` namespace so callers reach functions as
;; `clojure.walk/postwalk`, `clojure.walk/keywordize-keys`, etc.

(ns clojure.walk)

;; ---------------------------------------------------------------
;; walk — single layer. Apply `inner` to each immediate child of
;; `form`, rebuild a same-shape collection, then apply `outer`.
;; ---------------------------------------------------------------

(defn walk [inner outer form]
  (cond
    (list? form)
    (outer (apply list (mapv inner form)))

    (vector? form)
    (outer (mapv inner form))

    (map? form)
    (outer (reduce-kv (fn [acc k v]
                        (let [pair (inner [k v])]
                          (assoc acc (nth pair 0) (nth pair 1))))
                      {}
                      form))

    (set? form)
    (outer (reduce (fn [acc x] (conj acc (inner x))) #{} (vec form)))

    :else
    (outer form)))

(defn postwalk [f form]
  (clojure.walk/walk (fn [x] (clojure.walk/postwalk f x)) f form))

(defn prewalk [f form]
  (clojure.walk/walk (fn [x] (clojure.walk/prewalk f x)) identity (f form)))

(defn postwalk-replace [smap form]
  (clojure.walk/postwalk
    (fn [x] (if (contains? smap x) (get smap x) x))
    form))

(defn prewalk-replace [smap form]
  (clojure.walk/prewalk
    (fn [x] (if (contains? smap x) (get smap x) x))
    form))

(defn postwalk-demo [form]
  (clojure.walk/postwalk
    (fn [x] (println "Walked:" x) x)
    form)
  nil)

(defn prewalk-demo [form]
  (clojure.walk/prewalk
    (fn [x] (println "Walked:" x) x)
    form)
  nil)

;; ---------------------------------------------------------------
;; keywordize-keys / stringify-keys — only touch map keys, leave
;; everything else alone.
;; ---------------------------------------------------------------

(defn keywordize-keys [form]
  (clojure.walk/postwalk
    (fn [x]
      (if (map? x)
        (reduce-kv (fn [acc k v]
                     (assoc acc
                            (if (string? k) (keyword k) k)
                            v))
                   {}
                   x)
        x))
    form))

(defn stringify-keys [form]
  (clojure.walk/postwalk
    (fn [x]
      (if (map? x)
        (reduce-kv (fn [acc k v]
                     (assoc acc
                            (if (keyword? k) (name k) k)
                            v))
                   {}
                   x)
        x))
    form))

;; ---------------------------------------------------------------
;; macroexpand-all — recursively macroexpand every subform. cljrs's
;; `macroexpand` special form expands the top of a list to fixpoint;
;; we postwalk so inner forms are reached, expanding lists at each
;; layer. Quoted forms are left alone (clojure.walk is data-driven so
;; we can't see the literal `quote` distinction here, but macroexpand
;; itself preserves quote bodies — see eval.rs::macroexpand_all).
;; ---------------------------------------------------------------

(defn macroexpand-all [form]
  (clojure.walk/postwalk
    (fn [x] (if (list? x) (macroexpand x) x))
    form))
