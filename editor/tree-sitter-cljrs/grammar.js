/**
 * tree-sitter-cljrs
 *
 * Grammar for the cljrs Clojure dialect. Forked-in-spirit from
 * tree-sitter-clojure but kept self-contained and aware of cljrs-specific
 * forms: `defn-native`, `defn-gpu`, `defn-gpu-pixel`, and the primitive
 * type hints (`^i64`, `^f64`, `^bool`) that are the visual signature of
 * native fns.
 *
 * Design notes
 * ------------
 *  - The reader in `src/reader.rs` is the source of truth for what a token
 *    is. Symbol charset is permissive (`?!+ slash <>=.-` etc.); we mirror that.
 *  - We expose `defining_form` as a distinct node so highlight queries can
 *    paint `(defn foo ...)` with the definer in a different color than
 *    the symbol being defined. The first-position symbol of a list whose
 *    head matches a known definer becomes a `defining_form` node.
 *  - Type hints are exposed as their own `type_hint` node with a
 *    `prim_type` child for the primitive set we recognize. Non-primitive
 *    metadata still parses (as `metadata`), it just doesn't get the
 *    @type capture.
 */

const PREC = {
  // Nothing fancy yet; reserved for future ambiguity handling.
  metadata: 1,
};

// Charset for the body of a symbol/keyword. Matches the cljrs reader's
// "anything not whitespace, not a delimiter, not a string quote".
const SYM_CHAR = /[^\s()\[\]{}"@,;'`~^\\]/;
const SYM_HEAD = /[^\s()\[\]{}"@,;'`~^\\:#0-9][^\s()\[\]{}"@,;'`~^\\]*/;

// Primitive type hints we want to highlight specially. Anything else after
// `^` is still parsed as metadata, just without the @type capture.
const PRIM_TYPES = ["i64", "i32", "f64", "f32", "bool", "u8", "u32", "u64"];

// Definers — list-head symbols whose first child should be themed as a
// definer keyword. Mirrors `CLJ_SPECIAL` in docs/_highlight.js but split
// by role so query files can target each.
const DEFINERS = [
  "def", "defn", "defn-", "defmacro", "defonce",
  "defn-native", "defn-gpu", "defn-gpu-pixel",
  "defmulti", "defmethod", "defprotocol", "defrecord",
  "deftest",
];

const SPECIAL_FORMS = [
  "fn", "let", "loop", "recur", "if", "do", "when", "when-not",
  "if-let", "when-let", "cond", "case", "try", "catch", "finally",
  "throw", "ns", "require", "import", "quote",
  "and", "or", "->", "->>", "dotimes", "doseq",
  "is", "testing", "run-tests",
];

module.exports = grammar({
  name: "cljrs",

  extras: $ => [
    /\s/,
    $.comment,
  ],

  word: $ => $.symbol,

  rules: {
    source: $ => repeat($._form),

    _form: $ => choice(
      $.list,
      $.vector,
      $.map,
      $.set,
      $.anonymous_fn,
      $.regex,
      $.quote,
      $.syntax_quote,
      $.unquote_splice,
      $.unquote,
      $.deref,
      $.metadata,
      $.string,
      $.number,
      $.character,
      $.keyword,
      $.nil,
      $.boolean,
      $.defining_form,
      $.special_form,
      $.symbol,
    ),

    // ---- collections -----------------------------------------------------

    list: $ => seq("(", repeat($._form), ")"),
    vector: $ => seq("[", repeat($._form), "]"),
    map: $ => seq("{", repeat($._form), "}"),
    set: $ => seq("#{", repeat($._form), "}"),
    anonymous_fn: $ => seq("#(", repeat($._form), ")"),

    // ---- reader macros ---------------------------------------------------

    quote: $ => seq("'", $._form),
    syntax_quote: $ => seq("`", $._form),
    unquote_splice: $ => seq("~@", $._form),
    unquote: $ => seq("~", $._form),
    deref: $ => seq("@", $._form),

    metadata: $ => prec(PREC.metadata, seq(
      "^",
      choice(
        $.type_hint,
        $.keyword,
        $.map,
        $.string,
        $.symbol,
      ),
    )),

    // A type hint is just a symbol-shaped meta payload; we promote it to
    // its own node when the body matches a known primitive.
    type_hint: $ => choice(...PRIM_TYPES.map(t => alias(token(t), $.prim_type))),

    // ---- atoms -----------------------------------------------------------

    string: $ => token(seq(
      '"',
      repeat(choice(/[^"\\]/, /\\./)),
      '"',
    )),

    regex: $ => token(seq(
      '#"',
      repeat(choice(/[^"\\]/, /\\./)),
      '"',
    )),

    // Numbers: ints, floats, hex, scientific, ratios, optional sign.
    number: $ => token(choice(
      /[-+]?0[xX][0-9a-fA-F]+/,
      /[-+]?[0-9]+\/[0-9]+/,
      /[-+]?[0-9]+(\.[0-9]+)?([eE][-+]?[0-9]+)?M?/,
    )),

    character: $ => token(seq(
      "\\",
      choice(/./, "newline", "space", "tab", "return", "formfeed", "backspace"),
    )),

    keyword: $ => token(seq(":", optional(":"), /[^\s()\[\]{}"@,;'`~^\\][^\s()\[\]{}"@,;'`~^\\]*/)),

    nil: $ => token("nil"),
    boolean: $ => token(choice("true", "false")),

    // The catch-all symbol rule. `defining_form` and `special_form` below
    // shadow specific tokens via higher precedence.
    symbol: $ => token(SYM_HEAD),

    defining_form: $ => choice(...DEFINERS.map(d => prec(2, alias(token(prec(2, d)), $.definer)))),
    special_form: $ => choice(...SPECIAL_FORMS.map(d => prec(1, alias(token(prec(1, d)), $.special)))),

    // ---- comments --------------------------------------------------------

    comment: $ => token(seq(";", /.*/)),
  },
});
