# nvim-cljrs

Neovim support for the cljrs Clojure dialect. Provides filetype
detection and tree-sitter-based syntax highlighting.

## Requirements

- Neovim >= 0.9 (uses `vim.filetype.add`).
- [`nvim-treesitter`](https://github.com/nvim-treesitter/nvim-treesitter)
- `tree-sitter` CLI (`npm i -g tree-sitter-cli`) — needed once, to build
  the parser from the grammar source.

## Install

### lazy.nvim

```lua
{
  dir = "~/pl/cljrs/editor/nvim-cljrs",   -- or a github url once published
  dependencies = { "nvim-treesitter/nvim-treesitter" },
  ft = { "cljrs" },
  config = function()
    require("nvim-cljrs").setup({
      adopt_clj = false,                  -- set true to claim *.clj too
      grammar_path = "~/pl/cljrs/editor/tree-sitter-cljrs",
    })
    vim.cmd("TSInstall cljrs")            -- one-time parser build
  end,
}
```

### packer

```lua
use {
  "~/pl/cljrs/editor/nvim-cljrs",
  requires = { "nvim-treesitter/nvim-treesitter" },
  config = function()
    require("nvim-cljrs").setup({
      grammar_path = "~/pl/cljrs/editor/tree-sitter-cljrs",
    })
  end,
}
```

### vim-plug

```vim
Plug '~/pl/cljrs/editor/nvim-cljrs'
" then in your init.lua:
" require('nvim-cljrs').setup({ grammar_path = '~/pl/cljrs/editor/tree-sitter-cljrs' })
" :TSInstall cljrs
```

## After install

1. Open any `*.cljrs` file. The filetype should report `cljrs`
   (`:set ft?`).
2. Run `:TSInstall cljrs` once. nvim-treesitter will clone/build the
   parser from `grammar_path`.
3. Highlighting should engage automatically — `defn-native`,
   `defn-gpu`, `defn-gpu-pixel` show as definers; `^i64` / `^f64` /
   `^bool` get the `@type.builtin` color.

## Adopting *.clj as cljrs

If you want any `*.clj` file in the cljrs repo to use this highlighter
(because it contains `defn-native` or `defn-gpu`), pass
`adopt_clj = true` to `setup()`. The detector reads the first 200 lines
and only promotes files that actually use cljrs-specific forms.

## Troubleshooting

- `:checkhealth nvim-treesitter` should list `cljrs` as installed.
- If highlights look plain, check `:Inspect` on a token to confirm the
  capture name resolves in your colorscheme.
- Type hints are captured as `@type.builtin`; many themes don't theme
  it distinctly. Add a `vim.api.nvim_set_hl(0, "@type.builtin", { ... })`
  in your config if you want a custom color.
