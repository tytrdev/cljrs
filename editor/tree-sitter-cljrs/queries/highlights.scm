;; Highlight queries for tree-sitter-cljrs.
;; Capture names follow the nvim-treesitter / Helix conventions so the
;; same file works in both editors (and the WASM build for vscode if/when
;; we add it).

;; ---- atoms -----------------------------------------------------------------

(comment) @comment
(string) @string
(regex)  @string.regex
(number) @number
(character) @character
(nil)     @constant.builtin
(boolean) @constant.builtin
(keyword) @string.special.symbol

;; ---- definer & special-form highlighting ----------------------------------

;; The first-position symbol of `(defn ...)` / `(defn-native ...)` etc.
(defining_form (definer) @keyword.function)
(special_form (special) @keyword)

;; The name being defined: `(defn foo ...)` -> highlight `foo` as a function
;; definition. Pattern matches `(<definer> <symbol> ...)`.
(list
  .
  (defining_form)
  .
  (symbol) @function)

;; ---- type hints ------------------------------------------------------------

;; `^i64`, `^f64`, `^bool` etc. — THE cljrs visual signature.
(metadata (type_hint (prim_type) @type.builtin))

;; Generic metadata payload (non-primitive symbols, keywords, maps).
(metadata (symbol) @type)
(metadata (keyword) @attribute)

;; ---- reader macros ---------------------------------------------------------

(quote          "'"  @punctuation.special)
(syntax_quote   "`"  @punctuation.special)
(unquote        "~"  @punctuation.special)
(unquote_splice "~@" @punctuation.special)
(deref          "@"  @punctuation.special)

;; ---- delimiters ------------------------------------------------------------

["(" ")" "[" "]" "{" "}" "#{" "#("] @punctuation.bracket

;; ---- fallback symbols ------------------------------------------------------

(symbol) @variable
