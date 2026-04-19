;; nvim-treesitter looks up highlight queries under
;; `queries/<lang>/highlights.scm` on the runtimepath. We copy the
;; grammar's queries verbatim so users can install just this plugin
;; without symlinking into the grammar repo.
;;
;; Keep in sync with editor/tree-sitter-cljrs/queries/highlights.scm.

(comment) @comment
(string) @string
(regex)  @string.regex
(number) @number
(character) @character
(nil)     @constant.builtin
(boolean) @constant.builtin
(keyword) @string.special.symbol

(defining_form (definer) @keyword.function)
(special_form (special) @keyword)

(list
  .
  (defining_form)
  .
  (symbol) @function)

(metadata (type_hint (prim_type) @type.builtin))
(metadata (symbol) @type)
(metadata (keyword) @attribute)

(quote          "'"  @punctuation.special)
(syntax_quote   "`"  @punctuation.special)
(unquote        "~"  @punctuation.special)
(unquote_splice "~@" @punctuation.special)
(deref          "@"  @punctuation.special)

["(" ")" "[" "]" "{" "}" "#{" "#("] @punctuation.bracket

(symbol) @variable
