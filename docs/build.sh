#!/usr/bin/env bash
# Rebuild the WASM bundle that powers the docs-site REPL.
# Requires: rustup target add wasm32-unknown-unknown && cargo install wasm-pack
set -eu
cd "$(dirname "$0")/.."
cd crates/cljrs-wasm
wasm-pack build --release --target web --out-dir ../../docs/wasm
echo "wasm bundle written to docs/wasm/"
echo "preview locally: cd docs && python3 -m http.server 8080"
