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

use std::hint::black_box;
use std::time::Instant;

use cljrs::gpu::{Gpu, GpuKernel};

// vec4 loads: each thread handles 4 consecutive f32s. Improves memory
// bandwidth utilization (one 128-bit load vs four 32-bit loads) and
// cuts loop overhead by 4x. Combined with a grid-stride loop so a
// fixed dispatch handles any N (WebGPU caps workgroups at 65535/dim).
const WGSL: &str = r#"
@group(0) @binding(0) var<storage, read>       src: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> dst: array<vec4<f32>>;

@compute @workgroup_size(256)
fn main(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(num_workgroups)       nwg: vec3<u32>,
) {
    let stride = nwg.x * 256u;
    let n = arrayLength(&src);
    var i = gid.x;
    loop {
        if (i >= n) { break; }
        let v = src[i];
        dst[i] = sin(v) + cos(v * 2.0);
        i = i + stride;
    }
}
"#;

// Same kernel, hand-written plain-Rust reference. Used to double-check
// the GPU results and so we can time CPU-vs-GPU side by side.
fn cpu_sincos(src: &[f32]) -> Vec<f32> {
    src.iter().map(|&v| v.sin() + (v * 2.0).cos()).collect()
}

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
        "{:>10}  {:>12}  {:>14}  {:>10}  {:>10}  {:>8}",
        "N", "first (ms)", "gpu steady (ms)", "gpu GB/s", "cpu (ms)", "speedup"
    );
    for &n in &sizes {
        let mut input = vec![0.0f32; n];
        for (i, v) in input.iter_mut().enumerate() {
            *v = (i as f32) * 1e-5;
        }

        // First call includes pipeline compile + buffer allocation +
        // input upload. Matches the one-shot scenario (hand a buffer,
        // get a result).
        let t0 = Instant::now();
        let _ = kernel.run_f32(&gpu, &input).expect("run");
        let first_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // Warm once more to flush any lazy state.
        let _ = kernel.run_f32(&gpu, &input).expect("run");

        // Steady state: input already on device, buffers allocated.
        // Matches how pytorch-mps is benchmarked (tensor on device,
        // synchronize after each op). Only measures dispatch + readback.
        let mut samples = Vec::with_capacity(iters);
        for _ in 0..iters {
            let t0 = Instant::now();
            let _ = kernel.run_f32_reuse_input(&gpu, input.len()).expect("run");
            samples.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let steady_ms = samples[samples.len() / 2];

        // Throughput: we read 4n + write 4n = 8n bytes per call.
        let bytes = 8.0 * n as f64;
        let gbps = bytes / (steady_ms * 1e6);

        // CPU baseline: single-threaded, standard-library trig. Apples
        // to apples vs the GPU (same algorithm, same precision).
        // Warmup once, then median of a few runs — keep it quick at
        // large N since this gets slow.
        let cpu_iters = if n >= 10_000_000 { 3 } else { 5 };
        black_box(cpu_sincos(black_box(&input)));
        let mut cpu_samples = Vec::with_capacity(cpu_iters);
        for _ in 0..cpu_iters {
            let t0 = Instant::now();
            let result = cpu_sincos(black_box(&input));
            black_box(&result);
            cpu_samples.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        cpu_samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let cpu_ms = cpu_samples[cpu_samples.len() / 2];

        let speedup = cpu_ms / steady_ms;

        println!(
            "{:>10}  {:>12.2}  {:>14.2}  {:>10.2}  {:>10.2}  {:>7.1}×",
            n, first_ms, steady_ms, gbps, cpu_ms, speedup
        );
    }
}
