;; cljrs.string — pure-cljrs implementations of the clojure.string vars
;; that aren't installed natively in src/builtins.rs.
;;
;; To wire this file into the prelude, add:
;;     const STRING_NS: &str = include_str!("cljrs_string.clj");
;;     ... ("cljrs_string.clj", STRING_NS) ...
;; alongside the other namespaces in install_prelude (src/builtins.rs).
;; This file is namespaced `clojure.string`, so every defn here is
;; reachable as `clojure.string/foo` from any caller — matching the
;; canonical Clojure name. Existing native fns (str/trim etc.) are
;; already dual-bound under `clojure.string/`, so we don't redefine
;; them here.
;;
;; Implementation note: like cljrs.music, we fully qualify
;; intra-namespace references because cljrs resolves unqualified
;; symbols against the *caller's* namespace, not ours.

(ns clojure.string)

;; ---------------------------------------------------------------
;; Internal helpers
;; ---------------------------------------------------------------

(defn __char-at [s i] (subs s i (+ i 1)))

(defn __char-vec [s]
  ;; Return a vector of 1-char strings. cljrs has no Char value, so a
  ;; "character" is uniformly represented as a single-char string.
  (let [n (count s)]
    (mapv (fn [i] (clojure.string/__char-at s i)) (range n))))

;; ---------------------------------------------------------------
;; Trim variants
;; ---------------------------------------------------------------
;; clojure.string/trim trims any Java Character.isWhitespace char; we
;; approximate with the common ASCII whitespace set plus a few
;; well-known unicode separators. Good enough for typical text.

;; cljrs's string reader supports \n \t \r \\ \" only — no \f or
;; unicode escapes yet. TODO: once the reader grows those escapes,
;; add "\f" and "\u000B" to match Clojure's whitespace set exactly.
(def __ws-set
  #{" " "\t" "\n" "\r"})

(defn __ws? [c] (contains? clojure.string/__ws-set c))

(defn triml [s]
  ;; Drop leading whitespace.
  (let [n (count s)]
    (loop [i 0]
      (cond
        (>= i n) ""
        (clojure.string/__ws? (clojure.string/__char-at s i)) (recur (+ i 1))
        :else (subs s i n)))))

(defn trimr [s]
  ;; Drop trailing whitespace.
  (let [n (count s)]
    (loop [i n]
      (cond
        (<= i 0) ""
        (clojure.string/__ws? (clojure.string/__char-at s (- i 1))) (recur (- i 1))
        :else (subs s 0 i)))))

(defn trim-newline [s]
  ;; Drop trailing \n or \r\n (a single trailing CR alone is also
  ;; stripped — matches Clojure, which strips trailing \r and \n).
  (let [n (count s)]
    (loop [i n]
      (cond
        (<= i 0) ""
        (let [c (clojure.string/__char-at s (- i 1))]
          (or (= c "\n") (= c "\r")))
        (recur (- i 1))
        :else (subs s 0 i)))))

;; ---------------------------------------------------------------
;; capitalize — first char upper, rest lower
;; ---------------------------------------------------------------

(defn capitalize [s]
  (let [n (count s)]
    (cond
      (= n 0) ""
      (= n 1) (str/upper-case s)
      :else  (str (str/upper-case (subs s 0 1))
                  (str/lower-case (subs s 1 n))))))

;; ---------------------------------------------------------------
;; reverse — codepoint-correct (not grapheme-cluster correct)
;; ---------------------------------------------------------------
;; cljrs strings are Rust String / utf-8; subs / count operate on
;; chars (code points), so reversing the char-vector is codepoint
;; correct. Surrogate pairs aren't a concern under Rust char.

(defn reverse [s]
  (let [chars (clojure.string/__char-vec s)
        n     (count chars)]
    (apply str (mapv (fn [i] (nth chars (- (- n 1) i))) (range n)))))

;; ---------------------------------------------------------------
;; split-lines — split on \n or \r\n. Trailing empty line is dropped
;; (matches clojure.string/split-lines behaviour).
;; ---------------------------------------------------------------

(defn split-lines [s]
  ;; Normalize \r\n to \n, then split on \n. Drop one trailing empty
  ;; entry so "a\nb\n" yields ["a" "b"], matching Clojure.
  (let [norm (str/replace s "\r\n" "\n")
        parts (str/split norm "\n")
        n     (count parts)]
    (if (and (> n 0) (= (nth parts (- n 1)) ""))
      (subvec parts 0 (- n 1))
      parts)))

;; ---------------------------------------------------------------
;; escape — char-by-char remap via cmap
;; ---------------------------------------------------------------
;; cmap is a map whose keys are "characters" (in cljrs: 1-char
;; strings) and whose values are strings (or 1-char strings) to splice
;; in. Unmapped chars pass through unchanged.

(defn escape [s cmap]
  (let [chars (clojure.string/__char-vec s)]
    (apply str
           (mapv (fn [c]
                   (let [r (get cmap c)]
                     (if (nil? r) c (str r))))
                 chars))))

;; ---------------------------------------------------------------
;; re-quote-replacement — escape `$` and `\` in a replacement string
;; ---------------------------------------------------------------
;; In Clojure / Java, `$1`, `$2`, … in the replacement string refer
;; to capture groups, and `\` escapes. To use a literal replacement
;; with regex-aware replace, you have to double those characters.

(defn re-quote-replacement [s]
  ;; Order matters: replace backslashes first, then dollars.
  (-> s
      (str/replace "\\" "\\\\")
      (str/replace "$" "\\$")))

;; ---------------------------------------------------------------
;; replace-first — like replace but only the first match
;; ---------------------------------------------------------------
;; Matches Clojure: match arg is one of string / char / regex, and
;; replacement is correspondingly a string (or, with regex, a string
;; that may reference $1 capture groups — we treat replacement as a
;; literal string here, which is the common case; full regex
;; replacement-template support is a TODO).
;;
;; TODO: regex replacement should honor $1/$2 backreferences. For now
;; we substitute the replacement string verbatim, which matches
;; (clojure.string/replace-first s pat (re-quote-replacement repl)).

(defn __replace-first-string [s match replacement]
  (let [idx (str/index-of s match)]
    (if (nil? idx)
      s
      (str (subs s 0 idx)
           replacement
           (subs s (+ idx (count match)))))))

(defn replace-first [s match replacement]
  (cond
    ;; string match (also covers cljrs "char" — a 1-char string)
    (string? match)
    (clojure.string/__replace-first-string s match replacement)

    ;; otherwise treat as regex. cljrs has no `regex?` predicate, so
    ;; anything non-string falls through to re-find; if it isn't a
    ;; regex value, re-find raises a Type error which is reasonable.
    :else
    (let [hit (re-find match s)
          ;; re-find returns the whole-match string, or a vector
          ;; [whole g1 g2 ...] if the pattern has capture groups.
          whole (cond
                  (string? hit) hit
                  (vector? hit) (nth hit 0)
                  :else nil)]
      (if (nil? whole)
        s
        (clojure.string/__replace-first-string s whole replacement)))))
