# cljrs editor support

Syntax highlighting and filetype detection for the cljrs Clojure
dialect. Three pieces:

| Directory | Purpose |
|---|---|
| [`tree-sitter-cljrs/`](./tree-sitter-cljrs) | Tree-sitter grammar + highlight queries. The canonical parse model — used by neovim and (eventually) any editor with tree-sitter integration. |
| [`nvim-cljrs/`](./nvim-cljrs) | Neovim plugin. Registers the `cljrs` filetype and wires the tree-sitter grammar into nvim-treesitter. |
| [`vscode-cljrs/`](./vscode-cljrs) | VS Code extension. Uses a TextMate grammar (regex-based) for v1 — simpler than embedding tree-sitter in VS Code. |

All three highlight the same set of forms, including the cljrs-specific
ones: `defn-native`, `defn-gpu`, `defn-gpu-pixel`, `defmulti`,
`defmethod`, `defprotocol`, `defrecord`, `deftest`, plus primitive type
hints `^i64` / `^f64` / `^bool` etc. (the visual signature of native
fns).

## Quick install

### Neovim (lazy.nvim)

```lua
{
  dir = "~/pl/cljrs/editor/nvim-cljrs",
  dependencies = { "nvim-treesitter/nvim-treesitter" },
  ft = { "cljrs" },
  config = function()
    require("nvim-cljrs").setup({
      grammar_path = "~/pl/cljrs/editor/tree-sitter-cljrs",
    })
    vim.cmd("TSInstall cljrs")
  end,
}
```

You'll need `tree-sitter-cli` installed (`npm i -g tree-sitter-cli`)
the first time so nvim-treesitter can build the parser.

### VS Code (dev install)

```sh
ln -s "$(pwd)/editor/vscode-cljrs" ~/.vscode/extensions/cljrs-0.1.0
# restart VS Code
```

Or package + install:

```sh
npm install -g @vscode/vsce
cd editor/vscode-cljrs && vsce package
code --install-extension cljrs-0.1.0.vsix
```

## Repo policy

- No binary artifacts committed: no `parser.so`, no `*.vsix`, no
  generated `parser.c`. Build locally.
- The grammar in `tree-sitter-cljrs/` is authoritative. The TextMate
  grammar in `vscode-cljrs/` mirrors it by hand; when you add new
  forms, update both.
- Source of truth for the token grammar is `src/reader.rs` in the
  parent repo. Don't drift.

## Status

| Feature                             | tree-sitter | TextMate (vscode) |
|-------------------------------------|:-----------:|:-----------------:|
| Lists / vectors / maps / sets       | yes         | yes (no nesting AST) |
| Strings, numbers, keywords          | yes         | yes               |
| Line comments                       | yes         | yes               |
| Regex `#"..."`                      | yes         | yes               |
| Reader macros `' \` ~ ~@ @`         | yes         | yes               |
| Metadata `^Tag`                     | yes         | yes               |
| Primitive type hints `^i64` etc.    | yes         | yes               |
| `defn-native` / `defn-gpu*` themed  | yes         | yes               |
| `(defn foo …)` -> `foo` as fn def   | yes         | no                |
| Anonymous fn `#()`                  | yes         | parses, not themed |
| `#_` discard, `#?` cond, tagged lit | no          | no                |
| Indent / fold queries               | no          | n/a               |

## Roadmap

- Add `#_` and `#?` to the grammar.
- Ship corpus tests under `tree-sitter-cljrs/test/`.
- Consider replacing the vscode TextMate grammar with a tree-sitter +
  language-server setup once the grammar stabilizes.
