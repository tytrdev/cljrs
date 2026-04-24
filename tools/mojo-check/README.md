# mojo-check — verify transpiler output actually parses

The cljrs → Mojo transpiler emits Mojo source. Until we run real
`mojo check` on the output, "it compiled to a Rust `String`" is
everything we can say about correctness. This directory holds the
tooling that closes that loop.

## CI

`.github/workflows/mojo-check.yml` runs `mojo check` over every
`.mojo.<tier>` file in `crates/cljrs-mojo/tests/goldens/` on each
push to main (and on PRs that touch the transpiler).

Install strategy: [pixi](https://pixi.sh/) with Modular's public
conda channel. Pixi doesn't require a Modular account token the
way the old `modular install` CLI did.

The job is marked `continue-on-error: true` during bring-up so a
flaky install doesn't gate the whole repo. Remove that flag once
the install path has been stable in main for ~2 weeks.

## Local run

If the CI install path breaks, you can always run the check locally:

```sh
# One-time: install pixi and Mojo
curl -fsSL https://pixi.sh/install.sh | bash
mkdir -p ~/mojo-env && cd ~/mojo-env
pixi init . -c https://conda.modular.com/max-nightly -c conda-forge
pixi add modular

# Run the check
cd /path/to/cljrs
bash tools/mojo-check/run-local.sh
```

See `run-local.sh` for the script body.

## Blockers

If `pixi add modular` / `pixi add mojo` ever starts requiring
credentials, document the new install path in `BLOCKED.md` and
update both the workflow and `run-local.sh` to match.

## Related work

- `tools/mojo-check/error_locations.md` — design note + patch for
  surfacing `line N col M:` prefixes on transpiler errors.
- `crates/cljrs-mojo/tests/goldens.rs` — the Rust-side golden
  walker that compares `emit(src, tier)` against the expected
  Mojo text before we hand it to `mojo check`.
- `crates/cljrs-mojo/tests/no_double_parens.rs` — regression fence
  for the `((expr))` printer bug.
