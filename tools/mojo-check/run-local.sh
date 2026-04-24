#!/usr/bin/env bash
# Run `mojo check` against every .mojo.<tier> golden. Mirrors the CI job
# in .github/workflows/mojo-check.yml so local failures match CI.
#
# Prerequisites:
#   - `pixi` on PATH
#   - a pixi env with `modular` (or `mojo`) installed; set MOJO_ENV to
#     point at it, or default to ~/mojo-env.

set -eu

MOJO_ENV="${MOJO_ENV:-$HOME/mojo-env}"
GOLDENS_DIR="$(cd "$(dirname "$0")/../.." && pwd)/crates/cljrs-mojo/tests/goldens"

if [ ! -d "$MOJO_ENV" ]; then
  echo "error: pixi env not found at $MOJO_ENV" >&2
  echo "       set MOJO_ENV, or see tools/mojo-check/README.md for setup." >&2
  exit 2
fi

pass=0
fail=0
failed_files=()

for f in "$GOLDENS_DIR"/*.mojo.readable "$GOLDENS_DIR"/*.mojo.optimized "$GOLDENS_DIR"/*.mojo.max; do
  [ -e "$f" ] || continue
  name=$(basename "$f")
  staged="$(mktemp -d)/${name}.mojo"
  cp "$f" "$staged"
  if (cd "$MOJO_ENV" && pixi run mojo check "$staged") > /tmp/mojo-check-out 2>&1; then
    pass=$((pass + 1))
    printf '  pass  %s\n' "$name"
  else
    fail=$((fail + 1))
    failed_files+=("$name")
    printf '  FAIL  %s\n' "$name"
    sed 's/^/        /' /tmp/mojo-check-out
  fi
done

printf '\n%d passed, %d failed\n' "$pass" "$fail"
if [ "$fail" -gt 0 ]; then
  printf '\nfailed:\n'
  for n in "${failed_files[@]}"; do
    printf '  - %s\n' "$n"
  done
  exit 1
fi
