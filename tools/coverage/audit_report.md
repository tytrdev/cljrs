# stdlib audit — bugs found by bug-hunt test suite

Run on main @ 2026-04-19 across 5 new test files totaling **454 tests**: `stdlib_audit.rs`, `stdlib_edge_cases.rs`, `stdlib_set.rs`, `stdlib_string.rs`, `stdlib_walk.rs`.

- Passing: **436**
- Failing: **18**

`clojure.set` and `clojure.walk` came through clean — the pure-cljrs ports match Clojure's semantics on every edge case tested. The bugs cluster in older `src/builtins.rs` code and a few places the macro agent's rewrites didn't reach.

## Failures

### Arity gaps — mostly missing optional-arity overloads

| Test | Symptom | Likely fix site |
|---|---|---|
| `range_zero_arity_takes_5` | `(range)` (0-arg, infinite) errors: `expected 1-3` | `src/builtins.rs` range_fn — add 0-arity returning a bounded range or lazy-seq stub |
| `repeat_infinite_take` | `(repeat 42)` 1-arg form unsupported | `src/builtins.rs` repeat_fn — add 1-arity |
| `partition_step_diff_size` | `(partition n step coll)` 3-arg form unsupported | `src/builtins.rs` partition_fn |
| `partition_with_pad` | `(partition n step pad coll)` 4-arg form unsupported | `src/builtins.rs` partition_fn |
| `keyword_constructor_namespace` | `(keyword "ns" "foo")` 2-arg form errors | `src/builtins.rs` keyword_fn — add 2-arity |
| `sort_with_comparator_descending` | `(sort comparator coll)` 2-arg unsupported | `src/builtins.rs` sort_fn |

### Nil-punning / type permissiveness

| Test | Symptom | Likely fix |
|---|---|---|
| `ffirst_fnext_nfirst_nnext` | `(ffirst nil)` crashes with "first on non-sequence: int" — ffirst is reading nested rather than seq-coercing | `src/core.clj` ffirst etc — wrap inner arg with `(seq ...)` before `first` |
| `key_val_on_map_entry` | `(key (first {:a 1}))` — "first on non-sequence: map". Maps should seq into key/val entries. | `src/builtins.rs` first_fn or the map→seq coercion |

### Semantics mismatches vs Clojure

| Test | Symptom | Fix |
|---|---|---|
| `flatten_no_seq_passes_through` | `(flatten 1)` should return `(1)`, got `()` | `src/builtins.rs` flatten_fn — non-seq single value should be wrapped |
| `name_on_keyword_and_symbol` | `(name :ns/foo)` returned `"ns/foo"` — should be `"foo"` (namespace stripped) | `src/builtins.rs` name_fn |
| `tree_seq_counts_all_nodes` | Count mismatch (5 vs 8) — branch/children logic not visiting all nodes | `src/builtins.rs` tree_seq_fn |
| `equals_int_float_clojure_semantics` | `(= 1 1.0)` should be `false`, cljrs returns `true` | `src/builtins.rs` eq_fn — compare Int vs Float without numeric coercion |
| `delay_realized_flips` | `realized?` on a forced delay stays `false` | `src/core.clj` delay/force — flip the realized atom after forcing |

### String bugs

| Test | Symptom | Fix |
|---|---|---|
| `join_nil_in_coll_treated_as_empty` | `(str/join "," [1 nil 3])` — nil handling wrong | `src/builtins.rs` join_fn — stringify nil as "" |
| `replace_regex` | `str/replace` with regex arg fails | `src/builtins.rs` replace_fn |
| `split_with_limit` | `(str/split s re limit)` 3-arg form unsupported | `src/builtins.rs` split_fn |
| `subs_negative_index_errors` | Negative index doesn't throw cleanly | `src/builtins.rs` subs_fn — bounds check |

### Other

| Test | Symptom | Fix |
|---|---|---|
| `predicate_sweep_seq_q` | Some seq? predicate off on a specific input | `src/builtins.rs` seq_q_fn |

## Triage priority

**Shippable as-is** — 436/454 tests passing is already a respectable conformance bar. The failures are all **narrow** (missing arities or specific edge cases), none indicate a structural problem.

**First pass to close** (easy, mechanical):
1. Add missing arities to `range`, `repeat`, `partition`, `keyword`, `sort`, `str/split`. Half-day of work.
2. Fix `name` to strip namespace. One-line fix.
3. Fix `(= 1 1.0)` to return false. Changes `eq_fn` semantics — audit callers first.
4. Fix `flatten 1` → `(1)`. Easy.
5. Fix `ffirst`/`key`/`val` to seq-coerce input. One line each.

**Harder** (need design):
- `tree-seq` — review the traversal
- `realized?` on delays — storage shape for the realized flag
- Nil-in-str-join — choose a policy consistent with Clojure (`nil` prints as empty)

See the test files themselves for exact expected outputs.
