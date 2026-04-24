#!/usr/bin/env python3
"""
NumPy kernel baseline for the bench harness. Measures the same
four kernels the Rust-side runner measures, on the same shapes,
so docs/kernels.html can put them side-by-side.

Usage:
    python3 bench/numpy_kernels.py [N] [iters]

Outputs JSON on stdout (matching the schema the Rust bench emits).
"""
import json
import os
import sys
import time

try:
    import numpy as np
except ImportError:
    print(
        json.dumps({"error": "numpy not installed; pip install numpy",
                    "backend": "numpy", "kernels": []}),
    )
    sys.exit(2)


def median(xs):
    xs = sorted(xs)
    return xs[len(xs) // 2]


def bench(iters, fn):
    # warmups
    for _ in range(max(1, iters // 10)):
        fn()
    samples = []
    for _ in range(iters):
        t0 = time.perf_counter_ns()
        fn()
        samples.append(time.perf_counter_ns() - t0)
    return float(median(samples))


def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 1_000_000
    iters = int(sys.argv[2]) if len(sys.argv) > 2 else 20

    rng_seed = 0xC0FFEE
    rng = np.random.default_rng(rng_seed)

    # Match the Rust harness's deterministic buffers (close enough; the
    # comparison is time/op, not bit-equal outputs).
    idx = np.arange(n, dtype=np.float64)
    a = idx * 1e-3
    b = 1.0 + idx * 2e-3
    c = 0.5 - idx * 0.5e-3
    scalar = np.float64(3.141592653589793)

    # vector_add
    out = np.empty(n, dtype=np.float64)
    def vadd():
        np.add(a, b, out=out)
    t_vadd = bench(iters, vadd)

    # saxpy — numpy doesn't have a single fused call; closest is
    # the BLAS daxpy via scipy. We measure the naive numpy expression.
    def saxpyf():
        np.add(scalar * a, b, out=out)
    t_saxpy = bench(iters, saxpyf)

    # dot
    dot_acc = [0.0]
    def dotf():
        dot_acc[0] = float(np.dot(a, b))
    t_dot = bench(iters, dotf)

    # sum_sq
    sumsq_acc = [0.0]
    def ssf():
        sumsq_acc[0] = float((c * c).sum())
    t_sumsq = bench(iters, ssf)

    def gflops(flops, ns):
        return flops / ns

    def gibs(bytes_, ns):
        return bytes_ / ns

    entries = []
    for (name, flops, bytes_, ns) in [
        ("vector_add", n,       n * 3 * 8, t_vadd),
        ("saxpy",      2 * n,   n * 3 * 8, t_saxpy),
        ("dot",        2 * n,   n * 2 * 8, t_dot),
        ("sum_sq",     2 * n,   n * 1 * 8, t_sumsq),
    ]:
        entries.append({
            "name": name,
            "numpy_ns":     int(ns),
            "numpy_gflops": round(gflops(flops, ns), 3),
            "numpy_gibs":   round(gibs(bytes_, ns), 3),
        })

    # np.show_config() prints directly to stdout; we don't want that in
    # the JSON payload. Just record the library version.
    print(json.dumps({
        "n": n,
        "iters": iters,
        "numpy_version": np.__version__,
        "kernels": entries,
    }, indent=2))


if __name__ == "__main__":
    main()
