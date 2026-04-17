#!/usr/bin/env bash
# Run all bench/*.clj microbenchmarks across every Clojure-family impl
# available on $PATH, print per-iter times, and cross-check that every
# impl agrees on the bench's return value. Always builds cljrs with the
# MLIR feature on so defn-native benches hit the JIT path.
#
# Usage:
#   bench/run.sh [iters]
#
# iters defaults to 100.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

ITERS="${1:-100}"

PORTABLE_BENCHES=(bench/fib.clj bench/loop_sum.clj bench/cond_chain.clj)
NATIVE_BENCHES=(bench/fib_native.clj bench/loop_sum_native.clj bench/cond_chain_native.clj)

have() { command -v "$1" >/dev/null 2>&1; }

# On macOS brew's openjdk is keg-only; auto-prepend when present.
if [[ -x /opt/homebrew/opt/openjdk/bin/java ]]; then
  export PATH="/opt/homebrew/opt/openjdk/bin:$PATH"
fi
have_java() { java -version >/dev/null 2>&1; }

echo "building cljrs --release --features mlir..." >&2
cargo build --release --quiet --features mlir --bin bench

declare -a IMPLS
IMPLS+=("cljrs")
if have clojure && have_java; then
  IMPLS+=("clojure")
elif have clojure; then
  echo "skipping clojure: no JDK available (install openjdk)" >&2
fi
if have bb;   then IMPLS+=("bb");   fi
if have jank; then IMPLS+=("jank"); fi

echo "implementations present: ${IMPLS[*]}" >&2
echo "iters per bench: $ITERS" >&2
echo

# Run one impl on one bench, emit the uniform `impl path iters=X total=Xms per-iter=Yns result=V` line.
run_impl() {
  local impl="$1" bench="$2" iters="$3"
  case "$impl" in
    cljrs)
      ./target/release/bench "$bench" "$iters"
      ;;
    clojure)
      clojure -M bench/clj_bench.clj jvm "$bench" "$iters"
      ;;
    bb)
      bb bench/clj_bench.clj bb "$bench" "$iters"
      ;;
    jank)
      # Jank has no System/nanoTime and rejects post-positional CLI args
      # without `--`. Its `time` macro prints to stdout; we parse that.
      local out total_ms per_ns result
      out=$(jank run bench/jank_bench.clj -- "$bench" "$iters" 2>&1) || {
        echo "jank   $bench  (run failed)"
        return
      }
      total_ms=$(echo "$out" | sed -n 's/.*Elapsed time: \([0-9.]*\) ms.*/\1/p' | head -1)
      result=$(echo "$out" | sed -n 's/.*JANK_BENCH_RESULT=== //p' | head -1)
      if [[ -z "$total_ms" ]]; then
        echo "jank   $bench  (no timing parsed)"
        echo "$out" >&2
        return
      fi
      per_ns=$(awk -v t="$total_ms" -v i="$iters" 'BEGIN {printf "%.0f", t * 1e6 / i}')
      printf "%-6s %-40s  iters=%-8d  total=%10.2fms  per-iter=%14sns  result=%s\n" \
        "jank" "$bench" "$iters" "$total_ms" "$per_ns" "$result"
      ;;
  esac
}

# Extract the `result=VALUE` from a uniform bench line (everything after result=).
extract_result() {
  sed -n 's/.*result=\(.*\)$/\1/p'
}

# Run every impl on one bench, emit lines, then verify all impls return the
# same value. Any mismatch prints a loud warning — speed is irrelevant if
# the implementations don't agree on what the program computes.
run_bench() {
  local bench="$1" header="$2"
  local line baseline="" baseline_impl=""
  local mismatch=0
  shift 2
  local impls=("$@")

  echo "=== $bench${header:+  $header} ==="
  for impl in "${impls[@]}"; do
    line=$(run_impl "$impl" "$bench" "$ITERS") || continue
    echo "$line"
    local result
    result=$(echo "$line" | extract_result)
    if [[ -n "$result" ]]; then
      if [[ -z "$baseline" ]]; then
        baseline="$result"
        baseline_impl="$impl"
      elif [[ "$result" != "$baseline" ]]; then
        echo "  !! CORRECTNESS MISMATCH: $impl result=$result  vs  $baseline_impl result=$baseline"
        mismatch=1
      fi
    fi
  done
  if [[ $mismatch -eq 0 && -n "$baseline" ]]; then
    echo "  ✓ all impls agree: result=$baseline"
  fi
  echo
}

for bench in "${PORTABLE_BENCHES[@]}"; do
  run_bench "$bench" "" "${IMPLS[@]}"
done

for bench in "${NATIVE_BENCHES[@]}"; do
  run_bench "$bench" "(cljrs-only: uses defn-native)" "cljrs"
done
