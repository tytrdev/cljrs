-- nvim-cljrs: editor support for the cljrs Clojure dialect.
--
-- Responsibilities:
--   1. Register the `cljrs` filetype for `*.cljrs` files.
--   2. Optionally promote `*.clj` files containing cljrs-specific forms
--      (`defn-native`, `defn-gpu`, `defn-gpu-pixel`) to `cljrs` so the
--      same highlighter applies. Opt in via `setup({ adopt_clj = true })`.
--   3. Register the cljrs tree-sitter parser with nvim-treesitter and
--      tell it where to find the grammar's `highlights.scm`.
--
-- The actual parser binary is NOT shipped — the user is expected to run
-- `:TSInstall cljrs` after the parser source is registered. See README.

local M = {}

local default_opts = {
  -- If true, *.clj files containing `defn-native` / `defn-gpu` get the
  -- `cljrs` filetype instead of `clojure`. Off by default to avoid
  -- stomping on a user's existing Clojure setup.
  adopt_clj = false,
  -- Path to the tree-sitter-cljrs grammar checkout. Defaults to a
  -- sibling directory in the cljrs repo.
  grammar_path = nil,
}

local function register_filetype(opts)
  vim.filetype.add({
    extension = {
      cljrs = "cljrs",
    },
  })

  if opts.adopt_clj then
    vim.filetype.add({
      extension = {
        clj = function(_, bufnr)
          local lines = vim.api.nvim_buf_get_lines(bufnr, 0, 200, false)
          for _, line in ipairs(lines) do
            if line:match("defn%-native") or line:match("defn%-gpu") then
              return "cljrs"
            end
          end
          return "clojure"
        end,
      },
    })
  end
end

local function register_parser(opts)
  local ok, parsers = pcall(require, "nvim-treesitter.parsers")
  if not ok then
    -- nvim-treesitter not present; user must wire it up themselves.
    return
  end

  local parser_config = parsers.get_parser_configs()
  local url = opts.grammar_path
    or (vim.fn.stdpath("config") .. "/../../pl/cljrs/editor/tree-sitter-cljrs")

  parser_config.cljrs = {
    install_info = {
      url = url,
      files = { "src/parser.c" },
      branch = "main",
      generate_requires_npm = true,
      requires_generate_from_grammar = true,
    },
    filetype = "cljrs",
  }
end

function M.setup(opts)
  opts = vim.tbl_deep_extend("force", default_opts, opts or {})
  register_filetype(opts)
  register_parser(opts)
end

return M
