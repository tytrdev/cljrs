#!/usr/bin/env bash
# Full cljrs → Mojo → trained model pipeline.
#
# 1. Transpile kernels.clj via cljrs-mojo (needs workspace build)
# 2. Prepare the breast cancer dataset via uv + scikit-learn
# 3. Concatenate transpiled kernels + train.mojo, mojo build -O3
# 4. Run training, capture JSON output
# 5. Merge with sklearn baseline from meta.json into docs/clojo-ml/results.json
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

OUT_DIR="$REPO_ROOT/docs/clojo-ml"
mkdir -p "$OUT_DIR"

# --- step 1: transpile ---
echo "[clojo_logreg] transpiling kernels.clj..." >&2
cargo run --release -p cljrs-mojo --bin emit_mojo -- \
  bench/clojo_logreg/kernels.clj --all >&2

# --- step 2: data ---
if [ ! -f "bench/clojo_logreg/data/meta.json" ]; then
  echo "[clojo_logreg] preparing dataset..." >&2
  uv run --with scikit-learn --with numpy \
    bench/clojo_logreg/prepare_data.py >&2
fi

# --- step 3: mojo build ---
MOJO_BIN=""
if command -v mojo >/dev/null 2>&1; then
  MOJO_BIN="mojo"
elif [ -x "/tmp/mojo-bench/.venv/bin/mojo" ]; then
  MOJO_BIN="/tmp/mojo-bench/.venv/bin/mojo"
else
  echo "[clojo_logreg] error: mojo not found. install via:" >&2
  echo "  uv init hello && cd hello && uv venv && source .venv/bin/activate" >&2
  echo "  uv pip install mojo --index https://whl.modular.com/nightly/simple/ --prerelease allow" >&2
  exit 1
fi

BUILD_DIR="$(mktemp -d)"
trap "rm -rf '$BUILD_DIR'" EXIT
echo "[clojo_logreg] concatenating transpiled kernels + training harness..." >&2
cat bench/clojo_logreg/kernels.mojo.max bench/clojo_logreg/train.mojo \
  > "$BUILD_DIR/train.mojo"
echo "[clojo_logreg] mojo build -O3 ..." >&2
"$MOJO_BIN" build -O3 "$BUILD_DIR/train.mojo" -o "$BUILD_DIR/train" 2>&1 >&2

# --- step 4: run training ---
echo "[clojo_logreg] training ..." >&2
TRAIN_JSON="$("$BUILD_DIR/train")"

# --- step 5: merge + publish ---
python3 - <<PY > "$OUT_DIR/results.json"
import json, os, platform
train = json.loads('''$TRAIN_JSON''')
with open("bench/clojo_logreg/data/meta.json") as f:
    meta = json.load(f)
out = {
    "dataset": "wisconsin breast cancer (sklearn)",
    "task": "binary classification, malignant vs benign",
    "machine": {"os": platform.system(), "arch": platform.machine()},
    "model": {
        "type": "logistic regression",
        "loss": "BCE",
        "optimizer": "gradient descent",
        "epochs": train["epochs"],
        "lr": train["lr"],
    },
    "n_train": train["n_train"],
    "n_test": train["n_test"],
    "n_features": train["n_features"],
    "train_time_ms": train["train_ns"] / 1e6,
    "cljrs_mojo": {
        "train_acc": train["train_acc"],
        "test_acc":  train["test_acc"],
        "train_loss": train["train_loss"],
        "test_loss":  train["test_loss"],
    },
    "sklearn_baseline": {
        "train_acc": meta["sklearn_train_acc"],
        "test_acc":  meta["sklearn_test_acc"],
        "C": meta["sklearn_C"],
        "iters": meta["sklearn_iters"],
    },
    "weights": train["weights"],
    "bias": train["bias"],
    "feature_names": meta["feature_names"],
}
print(json.dumps(out, indent=2))
PY

# Copy the cljrs source + transpiled Mojo for the page to render.
cp -f bench/clojo_logreg/kernels.clj           "$OUT_DIR/kernels.clj"
cp -f bench/clojo_logreg/kernels.mojo.readable "$OUT_DIR/kernels.mojo.readable"
cp -f bench/clojo_logreg/kernels.mojo.max      "$OUT_DIR/kernels.mojo.max"
cp -f bench/clojo_logreg/train.mojo            "$OUT_DIR/train.mojo"

echo "[clojo_logreg] wrote $OUT_DIR/results.json" >&2
