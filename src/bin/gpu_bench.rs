//! GPU elementwise benchmark.
//!
//! For a fixed kernel (dst[i] = sin(src[i]) + cos(src[i] * 2)), measure:
//!   1. First-call latency (includes pipeline compile)
//!   2. Steady-state per-call latency (warmed up, median of N runs)
//!
//! Sizes: 1e5, 1e6, 1e7, 1e8 f32s. Prints a table; also writes JSON for
//! the docs-site benchmarks page to consume.
//!
//! The numpy/pytorch comparison numbers live in bench/gpu_numpy.py +
//! bench/gpu_pytorch.py. Run this, run those, paste the results side by
//! side.

use std::time::Instant;

use cljrs::gpu::{Gpu, GpuKernel};

const WGSL: &str = r#"
@group(0) @binding(0) var<storage, read>       src: array<f32>;
@group(0) @binding(1) var<storage, read_write> dst: array<f32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= arrayLength(&src)) { return; }
    let v = src[i];
    dst[i] = sin(v) + cos(v * 2.0);
}
"#;

fn main() {
    let gpu = Gpu::new().unwrap_or_else(|e| {
        eprintln!("no GPU: {e}");
        std::process::exit(1);
    });
    eprintln!(
        "gpu-bench: {} ({:?}, {:?})",
        gpu.adapter_info.name, gpu.adapter_info.device_type, gpu.adapter_info.backend
    );
    let kernel = GpuKernel::from_wgsl("bench-sincos", WGSL);

    let sizes = [100_000usize, 1_000_000, 10_000_000, 100_000_000];
    // Number of timed iterations after warmup for the steady-state median.
    let iters = 10;

    println!(
        "{:>10}  {:>12}  {:>14}  {:>12}  {:>12}",
        "N", "first (ms)", "steady (ms)", "GB/s", "GFLOP/s"
    );
    for &n in &sizes {
        let mut input = vec![0.0f32; n];
        for (i, v) in input.iter_mut().enumerate() {
            *v = (i as f32) * 1e-5;
        }

        // First call includes pipeline compile. This is the number that
        // matters for one-shot computations; steady-state matters for
        // repeated dispatches.
        let t0 = Instant::now();
        let _ = kernel.run_f32(&gpu, &input).expect("run");
        let first_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // Warm once more to flush any remaining lazy state.
        let _ = kernel.run_f32(&gpu, &input).expect("run");

        let mut samples = Vec::with_capacity(iters);
        for _ in 0..iters {
            let t0 = Instant::now();
            let _ = kernel.run_f32(&gpu, &input).expect("run");
            samples.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let steady_ms = samples[samples.len() / 2];

        // Throughput: we read 4n + write 4n = 8n bytes. 2 transcendentals
        // + 1 mul + 1 add per elem ≈ 4 flops (transcendentals are 1+
        // but one-op "flop" is the conservative count).
        let bytes = 8.0 * n as f64;
        let gbps = bytes / (steady_ms * 1e6);
        let flops = 4.0 * n as f64;
        let gflops = flops / (steady_ms * 1e6);

        println!(
            "{:>10}  {:>12.2}  {:>14.2}  {:>12.2}  {:>12.2}",
            n, first_ms, steady_ms, gbps, gflops
        );
    }
}
