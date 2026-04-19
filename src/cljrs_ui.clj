;; cljrs.ui — tiny hiccup-style UI library for cljrs.
;;
;; Loaded after cljrs_music.clj by `install_prelude`. Two halves:
;;
;;   1. A pure `render-html` that converts hiccup vectors to an HTML
;;      string. Used at build-time by the `prerender` binary so docs
;;      pages ship SEO-visible HTML before any JS runs.
;;
;;   2. `mount!` / `hydrate!` / `reactive!` — wasm-only helpers that
;;      dispatch to host builtins (`__ui-mount`, `__ui-hydrate`)
;;      installed by `crates/cljrs-wasm/src/ui_bridge.rs`. On native
;;      (e.g. the prerender bin) those builtins are absent; calling
;;      `mount!` natively raises an unbound-symbol error.
;;
;; Hiccup shape:
;;   [:div {:class "x"} [:button {:on-click f} "go"] [:span "hi"]]
;; First element is a keyword tag, optional second is a props map,
;; the rest are children. Strings/numbers are stringified+escaped;
;; nil children are skipped; nested vectors recurse.
;;
;; cljrs resolves unqualified symbols against the *caller's* ns, so
;; every intra-library reference below is fully qualified as
;; cljrs.ui/foo. Same convention as cljrs.music.

(ns cljrs.ui)

;; Convenience hiccup constructor.
(defn h [tag props & children]
  [tag (or props {}) (vec children)])

(def __void-tags
  #{:area :base :br :col :embed :hr :img :input
    :link :meta :param :source :track :wbr})

(defn __escape-html [s]
  (let [s (str s)
        s (str/replace s "&" "&amp;")
        s (str/replace s "<" "&lt;")
        s (str/replace s ">" "&gt;")
        s (str/replace s "\"" "&quot;")
        s (str/replace s "'" "&#39;")]
    s))

(defn __attr-name [k]
  (if (keyword? k) (name k) (str k)))

(defn __event-attr? [k]
  (let [n (cljrs.ui/__attr-name k)]
    (and (>= (count n) 3)
         (= (subs n 0 3) "on-"))))

(defn __style-pair [pair]
  (str (cljrs.ui/__attr-name (first pair))
       ":"
       (str (first (rest pair)))))

(defn __style-str [v]
  (if (map? v)
    (str/join ";" (map cljrs.ui/__style-pair v))
    (str v)))

(defn __render-attr [pair]
  (let [k (first pair)
        v (first (rest pair))]
    (cond
      (nil? v) ""
      (= v false) ""
      (cljrs.ui/__event-attr? k) ""
      (= k :style)
      (str " style=\""
           (cljrs.ui/__escape-html (cljrs.ui/__style-str v))
           "\"")
      (= v true) (str " " (cljrs.ui/__attr-name k))
      :else (str " "
                 (cljrs.ui/__attr-name k)
                 "=\""
                 (cljrs.ui/__escape-html v)
                 "\""))))

(defn __render-attrs [props]
  (if (or (nil? props) (not (map? props)))
    ""
    (str/join "" (map cljrs.ui/__render-attr props))))

(defn __hiccup? [x]
  (and (vector? x)
       (pos? (count x))
       (keyword? (first x))))

;; Convert a hiccup value to an HTML string. Pure, build-time-safe.
(defn render-html [hv]
  (cond
    (nil? hv) ""
    (string? hv) (cljrs.ui/__escape-html hv)
    (number? hv) (str hv)
    (cljrs.ui/__hiccup? hv)
    (let [tag (first hv)
          rest-vs (rest hv)
          first-rest (first rest-vs)
          has-props (and (not (nil? first-rest)) (map? first-rest))
          props (if has-props first-rest {})
          kids-raw (if has-props (rest rest-vs) rest-vs)
          first-kid (first kids-raw)
          single-non-hic (and (= 1 (count kids-raw))
                              (vector? first-kid)
                              (not (cljrs.ui/__hiccup? first-kid)))
          kids (if single-non-hic first-kid kids-raw)
          tname (cljrs.ui/__attr-name tag)
          attr-html (cljrs.ui/__render-attrs props)]
      (if (contains? cljrs.ui/__void-tags tag)
        (str "<" tname attr-html "/>")
        (str "<" tname attr-html ">"
             (str/join "" (map cljrs.ui/render-html kids))
             "</" tname ">")))
    (vector? hv) (str/join "" (map cljrs.ui/render-html hv))
    (list? hv) (str/join "" (map cljrs.ui/render-html hv))
    (seq? hv) (str/join "" (map cljrs.ui/render-html hv))
    :else (cljrs.ui/__escape-html (str hv))))

;; --- wasm-only host dispatch ---

(defn mount! [root-id hiccup] (cljrs.ui/__ui-mount root-id hiccup))
(defn hydrate! [root-id hiccup] (cljrs.ui/__ui-hydrate root-id hiccup))

;; Initial paint + return a 0-arg re-render thunk the caller invokes
;; after every state mutation. cljrs has no `add-watch`; the thunk
;; pattern keeps this layer leaf-shaped.
(defn reactive! [root-id state-atom view-fn]
  (cljrs.ui/mount! root-id (view-fn @state-atom))
  (fn [] (cljrs.ui/mount! root-id (view-fn @state-atom))))
