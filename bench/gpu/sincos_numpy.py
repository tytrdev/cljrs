#!/usr/bin/env python3
"""Numpy elementwise sin(x) + cos(x*2) bench.

Matches src/bin/gpu_bench.rs: same input (linear ramp, f32), same
kernel, same sizes. Ten warm iterations, median of ten timed. No
GPU — numpy with whatever BLAS the system ships (accelerate on mac,
MKL on linux).
"""

import numpy as np
import sys
import time

sizes = [100_000, 1_000_000, 10_000_000, 100_000_000]
iters = 10

print(f"numpy {np.__version__}")
print(f"{'N':>10}  {'first (ms)':>12}  {'steady (ms)':>14}")
for n in sizes:
    x = (np.arange(n, dtype=np.float32)) * 1e-5

    t0 = time.perf_counter()
    out = np.sin(x) + np.cos(x * 2)
    first_ms = (time.perf_counter() - t0) * 1000
    del out

    # warm
    out = np.sin(x) + np.cos(x * 2)
    del out

    samples = []
    for _ in range(iters):
        t0 = time.perf_counter()
        out = np.sin(x) + np.cos(x * 2)
        dt = time.perf_counter() - t0
        samples.append(dt * 1000)
        del out
    samples.sort()
    steady_ms = samples[len(samples) // 2]
    print(f"{n:>10}  {first_ms:>12.2f}  {steady_ms:>14.2f}")
