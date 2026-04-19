# tree-sitter-cljrs

Tree-sitter grammar for the cljrs Clojure dialect.

## What's covered

- Standard Clojure forms: lists, vectors, maps, sets, strings, numbers,
  characters, keywords, symbols, `nil`, booleans, line comments.
- Reader macros: `'`, `` ` ``, `~`, `~@`, `@`, `#{}`, `#()`, `#"regex"`,
  `^metadata`.
- cljrs-specific definers exposed as `defining_form` nodes so highlight
  queries can paint them distinctly: `defn-native`, `defn-gpu`,
  `defn-gpu-pixel`, plus `defmulti`, `defmethod`, `defprotocol`,
  `defrecord`, `defmacro`, `defonce`, `deftest`, the regular `def*`
  family.
- Primitive type hints (`^i64`, `^f64`, `^bool`, `^i32`, `^f32`, `^u8`,
  `^u32`, `^u64`) exposed as `type_hint > prim_type` for the
  `@type.builtin` capture — this is THE cljrs visual signature.
- Special forms (`fn`, `let`, `if`, `cond`, `case`, `loop`, `recur`,
  `try`, `ns`, `require`, `is`, `testing`, etc.).

## What's NOT covered (yet)

- `#_` form-discard reader macro.
- `#?(...)` reader conditionals.
- Tagged literals (`#inst`, `#uuid`, custom tags).
- Namespace-qualified keywords get parsed as a single `keyword` token
  but aren't split into `ns/name`.
- Indent / structural-edit queries (`folds.scm`, `indents.scm`).

These are easy to add in a follow-up.

## Build

Requires [`tree-sitter-cli`](https://tree-sitter.github.io/tree-sitter/creating-parsers#installation):

```sh
npm install -g tree-sitter-cli
cd editor/tree-sitter-cljrs
tree-sitter generate          # writes src/parser.c
tree-sitter test              # if you add tests/corpus/*.txt fixtures
```

`src/parser.c` and any compiled `parser.so` / `parser.dylib` are
intentionally not committed — generate them locally.

## Use from neovim

See `editor/nvim-cljrs/README.md`.
