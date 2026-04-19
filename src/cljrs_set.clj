;; clojure.set — set algebra and tiny relational helpers.
;;
;; Loaded by `install_prelude` (see src/builtins.rs). Lives in the
;; `clojure.set` namespace so callers reach functions as
;; `clojure.set/union`, `clojure.set/join`, etc.
;;
;; Like cljrs.music, every intra-namespace reference is fully qualified
;; so unqualified-symbol resolution (which uses the *caller's* ns) does
;; not depend on the caller having required us with :refer.

(ns clojure.set)

;; ---------------------------------------------------------------
;; Core set algebra
;; ---------------------------------------------------------------

(defn union
  ([] #{})
  ([s1] (if (nil? s1) #{} (set s1)))
  ([s1 s2]
   (let [a (if (nil? s1) #{} (set s1))
         b (if (nil? s2) [] (vec s2))]
     (reduce (fn [acc x] (conj acc x)) a b)))
  ([s1 s2 & more]
   (reduce clojure.set/union
           (clojure.set/union s1 s2)
           more)))

(defn intersection
  ([s1] (if (nil? s1) #{} (set s1)))
  ([s1 s2]
   (let [a (if (nil? s1) #{} (set s1))
         b (if (nil? s2) #{} (set s2))
         ;; Iterate over the smaller set for fewer membership checks.
         [small big] (if (<= (count a) (count b)) [a b] [b a])]
     (reduce (fn [acc x]
               (if (contains? big x) (conj acc x) acc))
             #{}
             (vec small))))
  ([s1 s2 & more]
   (reduce clojure.set/intersection
           (clojure.set/intersection s1 s2)
           more)))

(defn difference
  ([s1] (if (nil? s1) #{} (set s1)))
  ([s1 s2]
   (let [a (if (nil? s1) #{} (set s1))
         b (if (nil? s2) #{} (set s2))]
     (reduce (fn [acc x]
               (if (contains? b x) acc (conj acc x)))
             #{}
             (vec a))))
  ([s1 s2 & more]
   (let [removed (reduce clojure.set/union (set s2) more)]
     (clojure.set/difference s1 removed))))

;; ---------------------------------------------------------------
;; Predicates
;; ---------------------------------------------------------------

(defn subset? [s1 s2]
  (let [a (if (nil? s1) #{} (set s1))
        b (if (nil? s2) #{} (set s2))]
    (if (> (count a) (count b))
      false
      (every? (fn [x] (contains? b x)) (vec a)))))

(defn superset? [s1 s2]
  (clojure.set/subset? s2 s1))

;; ---------------------------------------------------------------
;; Filtering
;; ---------------------------------------------------------------

(defn select [pred xset]
  (reduce (fn [acc x] (if (pred x) (conj acc x) acc))
          #{}
          (vec (if (nil? xset) #{} xset))))

;; ---------------------------------------------------------------
;; Map utilities
;; ---------------------------------------------------------------

(defn map-invert [m]
  (if (nil? m)
    {}
    (reduce-kv (fn [acc k v] (assoc acc v k)) {} m)))

(defn rename-keys [m kmap]
  (if (nil? m)
    nil
    (reduce-kv (fn [acc old new]
                 (if (contains? m old)
                   (-> acc
                       (dissoc old)
                       (assoc new (get m old)))
                   acc))
               m
               (if (nil? kmap) {} kmap))))

;; ---------------------------------------------------------------
;; Relational helpers — xrel is a set of maps (records).
;; ---------------------------------------------------------------

(defn project [xrel ks]
  (reduce (fn [acc r] (conj acc (select-keys r ks)))
          #{}
          (vec (if (nil? xrel) #{} xrel))))

(defn rename [xrel kmap]
  (reduce (fn [acc r] (conj acc (clojure.set/rename-keys r kmap)))
          #{}
          (vec (if (nil? xrel) #{} xrel))))

(defn index [xrel ks]
  (reduce (fn [acc r]
            (let [k (select-keys r ks)
                  bucket (get acc k #{})]
              (assoc acc k (conj bucket r))))
          {}
          (vec (if (nil? xrel) #{} xrel))))

;; -- join --------------------------------------------------------
;;
;; (join r1 r2)        natural join on shared keys
;; (join r1 r2 kmap)   inner join, kmap maps r1 keys -> r2 keys

(defn __shared-keys [r1 r2]
  ;; Pick a sample record from each rel; intersect their key sets.
  ;; Returns a vector for stable iteration.
  (let [a (first (vec r1))
        b (first (vec r2))]
    (if (or (nil? a) (nil? b))
      []
      (let [ks-a (set (keys a))
            ks-b (set (keys b))]
        (vec (clojure.set/intersection ks-a ks-b))))))

(defn __record-merge [a b]
  (reduce-kv (fn [acc k v] (assoc acc k v)) a b))

(defn join
  ([r1 r2]
   (let [r1 (if (nil? r1) #{} r1)
         r2 (if (nil? r2) #{} r2)
         shared (clojure.set/__shared-keys r1 r2)]
     (if (empty? shared)
       ;; No shared keys → cartesian product (Clojure semantics).
       (reduce (fn [acc a]
                 (reduce (fn [acc2 b]
                           (conj acc2 (clojure.set/__record-merge a b)))
                         acc
                         (vec r2)))
               #{}
               (vec r1))
       ;; Index r2 by the shared keys, then probe with each record from r1.
       (let [idx (clojure.set/index r2 shared)]
         (reduce (fn [acc a]
                   (let [k (select-keys a shared)
                         matches (get idx k #{})]
                     (reduce (fn [acc2 b]
                               (conj acc2 (clojure.set/__record-merge a b)))
                             acc
                             (vec matches))))
                 #{}
                 (vec r1))))))
  ([r1 r2 kmap]
   ;; Explicit (join r1 r2 {r1-key r2-key ...}). Per Clojure semantics,
   ;; we invert kmap and rename r2 keys *into* r1's namespace, then
   ;; natural-join on those.
   (let [inverted (clojure.set/map-invert kmap)
         r2-renamed (clojure.set/rename r2 inverted)]
     (clojure.set/join r1 r2-renamed))))
