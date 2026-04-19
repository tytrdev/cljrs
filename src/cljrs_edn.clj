;; clojure.edn — minimum-viable EDN reader. cljrs's reader already
;; parses the EDN subset of Clojure source, so we delegate to the
;; built-in `read-string` (registered in src/builtins.rs).
;;
;; No stream / PushbackReader support — `read` and `read-string` both
;; take a single string and return one form.

(ns clojure.edn)

(defn read-string
  ([s] (cljrs.core/read-string s))
  ([opts s] (cljrs.core/read-string s)))

(defn read
  ([s] (cljrs.core/read-string s))
  ([opts s] (cljrs.core/read-string s)))
