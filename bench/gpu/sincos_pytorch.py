#!/usr/bin/env python3
"""PyTorch elementwise sin(x) + cos(x*2) bench.

Tests CPU, MPS (Apple Metal), and CUDA (if available). Matches the
cljrs gpu_bench input + iteration counts so the numbers line up.

Includes explicit synchronization before measuring, otherwise PyTorch
records launch time instead of completion time on GPU paths.
"""

import torch
import time

sizes = [100_000, 1_000_000, 10_000_000, 100_000_000]
iters = 10

def bench(name, device):
    print(f"\n--- {name} ---")
    print(f"{'N':>10}  {'first (ms)':>12}  {'steady (ms)':>14}")
    for n in sizes:
        x = torch.arange(n, dtype=torch.float32, device=device) * 1e-5

        # Warm up the pipeline.
        out = torch.sin(x) + torch.cos(x * 2)
        if device == "mps": torch.mps.synchronize()
        elif device == "cuda": torch.cuda.synchronize()
        del out

        # First call (may include lazy device init).
        t0 = time.perf_counter()
        out = torch.sin(x) + torch.cos(x * 2)
        if device == "mps": torch.mps.synchronize()
        elif device == "cuda": torch.cuda.synchronize()
        first_ms = (time.perf_counter() - t0) * 1000
        del out

        samples = []
        for _ in range(iters):
            t0 = time.perf_counter()
            out = torch.sin(x) + torch.cos(x * 2)
            if device == "mps": torch.mps.synchronize()
            elif device == "cuda": torch.cuda.synchronize()
            dt = time.perf_counter() - t0
            samples.append(dt * 1000)
            del out
        samples.sort()
        steady_ms = samples[len(samples) // 2]
        print(f"{n:>10}  {first_ms:>12.2f}  {steady_ms:>14.2f}")

print(f"pytorch {torch.__version__}")
bench("cpu", "cpu")
if torch.backends.mps.is_available():
    bench("mps (Apple Metal)", "mps")
if torch.cuda.is_available():
    bench("cuda", "cuda")
