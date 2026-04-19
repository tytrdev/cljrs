#!/usr/bin/env bash
# Serve the docs site locally with the same wasm CI would deploy.
# Always rebuilds the wasm bundle first so local matches production.
# Pass --no-build to skip the rebuild (useful when only tweaking HTML/CSS).
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [[ "${1:-}" != "--no-build" ]]; then
  (cd "$ROOT/crates/cljrs-wasm" &&
    wasm-pack build --release --target web --out-dir "$ROOT/docs/wasm")
fi

PORT="${PORT:-8080}"
echo "serving http://localhost:$PORT/"
cd "$ROOT/docs" && exec python3 -m http.server "$PORT"
