#!/usr/bin/env bash
# Rebuild every artifact the docs site serves: WASM bundle, compiled
# WGSL, and kernel source copies. CI only rebuilds the WASM — the WGSL
# files are committed, so the page always shows whichever kernel was
# last compiled on a dev machine.
#
# Requires:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-pack

set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# WASM bundle (tree-walker REPL).
(cd "$ROOT/crates/cljrs-wasm" &&
  wasm-pack build --release --target web --out-dir "$ROOT/docs/wasm")

# GPU kernels → WGSL + source snapshots for the GPU docs page.
cargo build --release --features gpu --bin gpu-compile --manifest-path "$ROOT/Cargo.toml"
mkdir -p "$ROOT/docs/wgsl" "$ROOT/docs/kernels"
for k in plasma waves mandelbrot raymarch3d raytrace clouds flowfield; do
  # gpu-compile resolves (load-file "demo_gpu/stdlib.clj") relative to cwd,
  # so we invoke it from the repo root.
  (cd "$ROOT" && "$ROOT/target/release/gpu-compile" "demo_gpu/$k.clj") > "$ROOT/docs/wgsl/$k.wgsl"
  (cd "$ROOT" && "$ROOT/target/release/gpu-compile" --inline "demo_gpu/$k.clj") > "$ROOT/docs/wgsl/${k}_inline.wgsl"
  cp "$ROOT/demo_gpu/$k.clj" "$ROOT/docs/kernels/$k.clj"
done
# Ship the stdlib alongside the kernels so the docs site can display it.
cp "$ROOT/demo_gpu/stdlib.clj" "$ROOT/docs/kernels/stdlib.clj"

echo "done."
echo "  wasm:    $ROOT/docs/wasm/"
echo "  wgsl:    $ROOT/docs/wgsl/"
echo "  kernels: $ROOT/docs/kernels/"
echo
echo "preview: cd $ROOT/docs && python3 -m http.server 8080"
