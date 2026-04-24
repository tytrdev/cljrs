# Mojo benchmark harness for the four cljrs-mojo kernels.
#
# The transpiler's tier=Max output uses SIMD load/store + vectorize[]
# — the stable API when cljrs-mojo was written. Mojo 0.26.3 nightly
# (April 2026) has renamed both: `SIMD.load` moved to pointer methods,
# `vectorize` takes a closure arg, and `UnsafePointer` now needs an
# `origin`. Rather than chase a moving target this adapter uses scalar
# loops over List[Float64] and relies on Mojo's LLVM backend for
# auto-vectorization. Same kernels, same shapes, different surface
# syntax — see bench/kernels_mojo/src.mojo.max for the transpiler's
# output that this file is a manual adaptation of.
#
# Usage: `mojo build -O3 run.mojo -o run-mojo && ./run-mojo`

from std.time import perf_counter_ns


def vector_add(mut a: List[Float64], mut b: List[Float64],
               mut dst: List[Float64], n: Int):
    for i in range(n):
        dst[i] = a[i] + b[i]


def saxpy(alpha: Float64,
          mut x: List[Float64], mut y: List[Float64],
          mut dst: List[Float64], n: Int):
    for i in range(n):
        dst[i] = alpha * x[i] + y[i]


def dot(mut x: List[Float64], mut y: List[Float64], n: Int) -> Float64:
    var acc: Float64 = 0.0
    for i in range(n):
        acc += x[i] * y[i]
    return acc


def sum_sq(mut x: List[Float64], n: Int) -> Float64:
    var acc: Float64 = 0.0
    for i in range(n):
        acc += x[i] * x[i]
    return acc


def main():
    alias N = 1_000_000
    alias iters = 20

    var a = List[Float64](length=N, fill=0.0)
    var b = List[Float64](length=N, fill=0.0)
    var c = List[Float64](length=N, fill=0.0)
    var dst = List[Float64](length=N, fill=0.0)

    for i in range(N):
        a[i] = Float64(i) * 1.0e-3
        b[i] = 1.0 + Float64(i) * 2.0e-3
        c[i] = 0.5 - Float64(i) * 0.5e-3

    var alpha: Float64 = 3.141592653589793

    # warmups
    for _ in range(2):
        vector_add(a, b, dst, N)
        saxpy(alpha, a, b, dst, N)
        _ = dot(a, b, N)
        _ = sum_sq(c, N)

    var dot_result: Float64 = 0.0
    var ss_result: Float64 = 0.0
    var best_vadd: Float64 = 1.0e20
    var best_saxpy: Float64 = 1.0e20
    var best_dot: Float64 = 1.0e20
    var best_sumsq: Float64 = 1.0e20

    for _ in range(iters):
        var t0 = perf_counter_ns()
        vector_add(a, b, dst, N)
        var dt = Float64(perf_counter_ns() - t0)
        if dt < best_vadd:
            best_vadd = dt

        t0 = perf_counter_ns()
        saxpy(alpha, a, b, dst, N)
        dt = Float64(perf_counter_ns() - t0)
        if dt < best_saxpy:
            best_saxpy = dt

        t0 = perf_counter_ns()
        dot_result = dot(a, b, N)
        dt = Float64(perf_counter_ns() - t0)
        if dt < best_dot:
            best_dot = dt

        t0 = perf_counter_ns()
        ss_result = sum_sq(c, N)
        dt = Float64(perf_counter_ns() - t0)
        if dt < best_sumsq:
            best_sumsq = dt

    print("{")
    print('  "n":', N, ',')
    print('  "iters":', iters, ',')
    print('  "mojo_version": "0.26.3-nightly",')
    print('  "stat": "min",')
    print('  "strategy": "scalar_loop_auto_vectorize",')
    print('  "kernels": [')
    print('    { "name": "vector_add", "mojo_ns":', Int(best_vadd), '},')
    print('    { "name": "saxpy",      "mojo_ns":', Int(best_saxpy), '},')
    print('    { "name": "dot",        "mojo_ns":', Int(best_dot), ', "result":', dot_result, '},')
    print('    { "name": "sum_sq",     "mojo_ns":', Int(best_sumsq), ', "result":', ss_result, '}')
    print('  ]')
    print("}")
