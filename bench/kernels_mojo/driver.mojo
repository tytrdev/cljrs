# Benchmark driver for the transpiled kernels. This file is
# concatenated with `src.mojo.max` (the cljrs-mojo tier=Max output)
# at build time, so the kernel definitions are already in scope
# when main() runs them. See bench/kernels.sh.
#
# We deliberately do NOT hand-write kernels here — the whole point of
# the bench is to measure the transpiler's output.

from std.memory.unsafe_pointer import alloc
from std.time import perf_counter_ns


def main():
    alias N = 1_000_000
    alias iters = 20

    var a = alloc[Float64](N)
    var b = alloc[Float64](N)
    var c = alloc[Float64](N)
    var dst = alloc[Float64](N)

    for i in range(N):
        a[i] = Float64(i) * 1.0e-3
        b[i] = 1.0 + Float64(i) * 2.0e-3
        c[i] = 0.5 - Float64(i) * 0.5e-3

    var alpha: Float64 = 3.141592653589793

    # Saxpy wants (a, x, y, out, n) but the transpiled signature takes
    # `a` as a pointer (one-per-element input). To broadcast the scalar
    # alpha, we fill a pointer with the scalar value. That matches what
    # the harness does for numpy and Rust.
    var alpha_buf = alloc[Float64](N)
    for i in range(N):
        alpha_buf[i] = alpha

    # Warmups (one of each).
    vector_add(a, b, dst, N)
    saxpy(alpha_buf, a, b, dst, N)
    _ = dot(a, b, N)
    _ = sum_sq(c, N)

    var best_vadd: Float64 = 1.0e20
    var best_saxpy: Float64 = 1.0e20
    var best_dot: Float64 = 1.0e20
    var best_sumsq: Float64 = 1.0e20
    var dot_result: Float64 = 0.0
    var ss_result: Float64 = 0.0

    for _ in range(iters):
        var t0 = perf_counter_ns()
        vector_add(a, b, dst, N)
        var dt = Float64(perf_counter_ns() - t0)
        if dt < best_vadd:
            best_vadd = dt

        t0 = perf_counter_ns()
        saxpy(alpha_buf, a, b, dst, N)
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

    # Consume dst so Mojo's optimizer can't DCE the write paths. Without
    # this, vector_add and saxpy get eliminated and their timings read
    # as 0 ns.
    var checksum: Float64 = dst[0] + dst[N // 2] + dst[N - 1]

    print("{")
    print('  "n":', N, ',')
    print('  "iters":', iters, ',')
    print('  "mojo_version": "0.26.3-nightly",')
    print('  "stat": "min",')
    print('  "strategy": "cljrs_mojo_tier_max_simd",')
    print('  "checksum":', checksum, ',')
    print('  "kernels": [')
    print('    { "name": "vector_add", "mojo_ns":', Int(best_vadd), '},')
    print('    { "name": "saxpy",      "mojo_ns":', Int(best_saxpy), '},')
    print('    { "name": "dot",        "mojo_ns":', Int(best_dot), ', "result":', dot_result, '},')
    print('    { "name": "sum_sq",     "mojo_ns":', Int(best_sumsq), ', "result":', ss_result, '}')
    print('  ]')
    print("}")

    a.free()
    b.free()
    c.free()
    dst.free()
    alpha_buf.free()
