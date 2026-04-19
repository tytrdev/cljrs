# stdlib audit — bugs found by bug-hunt test suite

Run on main @ 2026-04-19 across 5 new test files totaling **454 tests**: `stdlib_audit.rs`, `stdlib_edge_cases.rs`, `stdlib_set.rs`, `stdlib_string.rs`, `stdlib_walk.rs`.

- Passing: **454** (was 436)
- Failing: **0** (was 18) — all bugs below FIXED in this pass.

`clojure.set` and `clojure.walk` came through clean — the pure-cljrs ports match Clojure's semantics on every edge case tested. The bugs cluster in older `src/builtins.rs` code and a few places the macro agent's rewrites didn't reach.

## Failures (all FIXED — see commit notes below)

### Arity gaps — mostly missing optional-arity overloads

| Test | Status | Fix |
|---|---|---|
| `range_zero_arity_takes_5` | FIXED | `range_fn` now accepts 0-arity, returns bounded 0..10_000 list (pair with `take`). |
| `repeat_infinite_take` | FIXED | `repeat_fn` 1-arity produces a 10_000-element bounded repeat. |
| `partition_step_diff_size` | FIXED | `partition_fn` 3-arity `(partition n step coll)` added. |
| `partition_with_pad` | FIXED | `partition_fn` 4-arity `(partition n step pad coll)` added; trailing chunk is padded. |
| `keyword_constructor_namespace` | FIXED | `keyword_fn` 2-arity builds `"ns/name"` keyword. |
| `sort_with_comparator_descending` | FIXED | `sort_fn` 2-arity supports predicate + Clojure-style integer comparators. |

### Nil-punning / type permissiveness

| Test | Status | Fix |
|---|---|---|
| `ffirst_fnext_nfirst_nnext` | FIXED | `src/core.clj` ffirst/fnext/nfirst/nnext now `(seq ...)` at each level. `first_fn` also nil-puns non-collection scalars. |
| `key_val_on_map_entry` | FIXED | `first_fn` now returns a `[k v]` vector for maps (MapEntry shape). |

### Semantics mismatches vs Clojure

| Test | Status | Fix |
|---|---|---|
| `flatten_no_seq_passes_through` | FIXED | `flatten_fn` returns `()` for non-sequential roots. |
| `name_on_keyword_and_symbol` | FIXED | `name_fn` strips any namespace prefix before `/`. |
| `tree_seq_counts_all_nodes` | FIXED | `tree_seq_fn` surfaces the node's head as a leaf when children-fn skipped it (e.g. `rest` on a vector). |
| `equals_int_float_clojure_semantics` | FIXED | New `strict_eq` in `eq`: Int/Float/Ratio are distinct under `=`. Kept `==` for numeric coercion (`core.clj` `==` switched to `zero?` and `number?` extended to include Ratio). |
| `delay_realized_flips` | FIXED | `core.clj` overrides the built-in `realized?` so delays consult their `:state` atom. |

### String bugs

| Test | Status | Fix |
|---|---|---|
| `join_nil_in_coll_treated_as_empty` | FIXED | `str_join_fn` stringifies `nil` as `""`. |
| `replace_regex` | FIXED | `str_replace_fn` accepts `Value::Regex` for the match arg. |
| `split_with_limit` | FIXED | `str_split_fn` 3-arity honors Java-style limit (pos=cap with tail, 0=drop-trailing-empties, neg=unbounded). |
| `subs_negative_index_errors` | FIXED | `subs_fn` rejects negative start/end with a clean `Error::Eval`. |

### Other

| Test | Status | Fix |
|---|---|---|
| `predicate_sweep_seq_q` | FIXED | `seq_q` now matches only `List`/`Cons`/`LazySeq` (Clojure ISeq semantics). |

## Result

All 18 bugs FIXED in this pass. Full `cargo test` = 829 passing / 0 failing. Coverage report regenerated — 100% on clojure.string / set / walk / edn; clojure.core 61.2% (unchanged, the fixes touched already-covered fns).

See the test files themselves for exact expected outputs.
