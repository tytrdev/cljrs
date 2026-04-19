# vscode-cljrs

VS Code extension that adds syntax highlighting for the cljrs Clojure
dialect via a TextMate grammar.

## What's covered

- Line comments (`;`), strings, regex literals (`#"..."`).
- Numbers (int / float / hex / scientific / ratio / `M` decimal).
- Keywords (`:foo`, `::ns/foo`).
- `nil`, `true`, `false`.
- Reader macros: `'`, `` ` ``, `~`, `~@`, `@`.
- Metadata: `^Tag` highlighted as attribute; primitive type hints
  (`^i64`, `^f64`, `^bool`, `^i32`, `^f32`, `^u8`, `^u16`, `^u32`,
  `^u64`, `^isize`, `^usize`) get `support.type.primitive` so themes
  paint them as types — the cljrs visual signature.
- Definers as `storage.type.function`: `def`, `defn`, `defn-`,
  `defmacro`, `defonce`, `defn-native`, `defn-gpu`, `defn-gpu-pixel`,
  `defmulti`, `defmethod`, `defprotocol`, `defrecord`, `deftest`.
- Control / special forms as `keyword.control`: `fn`, `let`, `loop`,
  `recur`, `if`, `do`, `when`, `cond`, `case`, `try`, `catch`,
  `finally`, `throw`, `ns`, `require`, `import`, `quote`, `and`, `or`,
  `->`, `->>`, `dotimes`, `doseq`, `is`, `testing`, `run-tests`.
- Bracket matching, comment toggle (`;`).

## What's NOT covered (yet)

- `#_` form-discard, `#?(...)` reader conditionals, tagged literals.
- Anonymous fn `#()` body — parses fine, just not specially themed.
- Nested string escapes beyond `\\.`.
- Sexp navigation / paredit (this is a TextMate grammar, not a language
  server).

## Install

### Dev — symlink into VS Code's extensions dir

```sh
ln -s "$(pwd)/editor/vscode-cljrs" ~/.vscode/extensions/cljrs-0.1.0
# restart VS Code
```

### Package and install a `.vsix`

Requires [`vsce`](https://github.com/microsoft/vscode-vsce):

```sh
npm install -g @vscode/vsce
cd editor/vscode-cljrs
vsce package                          # produces cljrs-0.1.0.vsix
code --install-extension cljrs-0.1.0.vsix
```

The repo intentionally does not commit the `.vsix` artifact.

## Verify

Open a `.cljrs` file. Bottom-right of VS Code should show "cljrs". Run
`Developer: Inspect Editor Tokens and Scopes` over a `^i64` to confirm
it has the `support.type.primitive.cljrs` scope.
