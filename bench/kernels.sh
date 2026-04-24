#!/usr/bin/env bash
# Run the kernel benchmarks across every backend we can reach locally
# and emit a merged JSON report consumed by docs/kernels.html.
#
# Usage:
#   bench/kernels.sh [N] [iters]
#
# N defaults to 1_000_000, iters to 20.
#
# Backends:
#   cljrs-native  — defn-native → MLIR JIT, per-element fn driven by a Rust loop
#   rust-inline   — hand-written Rust inline, same shape as the Rust driver
#   numpy         — numpy's ufuncs + BLAS for dot
#   mojo-readable / mojo-optimized / mojo-max — transpiled (not timed locally
#                    unless `mojo` is on PATH; source is always emitted)
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

N="${1:-1000000}"
ITERS="${2:-20}"

OUT_DIR="$REPO_ROOT/docs/bench-kernels"
mkdir -p "$OUT_DIR"

echo "bench/kernels.sh: building cljrs --release --features mlir --bin bench-kernels" >&2
cargo build --release --features mlir --bin bench-kernels >&2

echo "bench/kernels.sh: running cljrs-native + rust-inline..." >&2
CLJRS_JSON="$(./target/release/bench-kernels "$N" "$ITERS")"

echo "bench/kernels.sh: running numpy..." >&2
NUMPY_JSON=""
if command -v uv >/dev/null 2>&1; then
  NUMPY_JSON="$(uv run --with numpy bench/numpy_kernels.py "$N" "$ITERS")"
elif command -v python3 >/dev/null 2>&1; then
  if python3 -c "import numpy" 2>/dev/null; then
    NUMPY_JSON="$(python3 bench/numpy_kernels.py "$N" "$ITERS")"
  else
    echo "bench/kernels.sh: numpy not installed and uv missing; skipping numpy backend" >&2
  fi
fi

echo "bench/kernels.sh: emitting transpiled Mojo alongside .clj sources..." >&2
cargo run --release -p cljrs-mojo --bin emit_mojo -- bench/kernels_mojo/src.clj --all >&2 || true

# ---- merge JSON ----
# Produce a single object keyed by kernel name with per-backend fields.
python3 - "$CLJRS_JSON" "$NUMPY_JSON" > "$OUT_DIR/results.json" <<'PY'
import json, sys, pathlib
cljrs = json.loads(sys.argv[1]) if sys.argv[1] else {"kernels": []}
numpy = json.loads(sys.argv[2]) if sys.argv[2] else {"kernels": []}

by_name = {}
for k in cljrs.get("kernels", []):
    by_name[k["name"]] = dict(k)
for k in numpy.get("kernels", []):
    by_name.setdefault(k["name"], {"name": k["name"]}).update(k)

merged = {
    "n": cljrs.get("n") or numpy.get("n"),
    "iters": cljrs.get("iters") or numpy.get("iters"),
    "numpy_version": numpy.get("numpy_version"),
    "machine": {
        "os": None, "arch": None,
    },
    "kernels": [by_name[k] for k in ["vector_add", "saxpy", "dot", "sum_sq"]
                if k in by_name],
}
# Best-effort machine info
try:
    import platform
    merged["machine"] = {
        "os": platform.system(),
        "arch": platform.machine(),
        "python": platform.python_version(),
    }
except Exception:
    pass
print(json.dumps(merged, indent=2))
PY

echo "bench/kernels.sh: wrote $OUT_DIR/results.json" >&2

# Copy the Mojo sources into the docs folder so the page can display them.
cp -f bench/kernels_mojo/src.mojo.readable  "$OUT_DIR/mojo.readable.mojo"
cp -f bench/kernels_mojo/src.mojo.optimized "$OUT_DIR/mojo.optimized.mojo"
cp -f bench/kernels_mojo/src.mojo.max       "$OUT_DIR/mojo.max.mojo"
for k in vector_add saxpy dot sum_sq; do
  cp -f "bench/kernels/$k.clj" "$OUT_DIR/$k.clj"
done
cp -f bench/kernels_mojo/src.clj "$OUT_DIR/src_mojo.clj"

echo "bench/kernels.sh: done. Results in $OUT_DIR/" >&2
