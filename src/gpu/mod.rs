//! GPU compute backend, feature-gated behind `gpu`.
//!
//! # Why wgpu (and not MLIR's gpu dialect)
//!
//! The north-star requirement is "same kernel runs native AND in the
//! browser" — that's what makes this project distinct from JAX/PyTorch.
//! wgpu satisfies that with a single code path: one WGSL source string
//! dispatches to Metal on macOS, Vulkan on Linux/Android, DX12 on
//! Windows, and WebGPU in supported browsers.
//!
//! MLIR's gpu dialect is more powerful in principle (nested launch
//! hierarchies, direct SPIR-V emission, tighter integration with the
//! arith/scf pipeline used by `defn-native`) but it would require a
//! separate web path. We can always add MLIR lowering later for the
//! native case; it would emit the same WGSL-equivalent we emit today.
//!
//! # Layout
//! - `mod.rs`  — `Gpu` handle: adapter/device/queue, pipeline cache.
//! - `emit.rs` — cljrs AST → WGSL text (the DSL lives here).
//!
//! # Phase 0 (this file): smoke test
//! Hand-written WGSL kernel, dispatched through the full pipeline. If
//! this works on a machine, the kernel DSL will too — the DSL only
//! changes the *text* of the shader, not any of the setup around it.

pub mod emit;

use std::sync::{Arc, OnceLock};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Process-global GPU handle. The first `defn-gpu` (or explicit
/// init_global_gpu() call) pays the adapter-acquisition cost; every
/// subsequent kernel reuses the same device. Matches how JAX/PyTorch
/// handle the "one device per process" default.
static GLOBAL_GPU: OnceLock<Arc<Gpu>> = OnceLock::new();

pub fn global_gpu() -> Result<Arc<Gpu>, String> {
    if let Some(g) = GLOBAL_GPU.get() {
        return Ok(Arc::clone(g));
    }
    let g = Arc::new(Gpu::new()?);
    // Race-tolerant: if another thread won, discard ours.
    let _ = GLOBAL_GPU.set(Arc::clone(&g));
    Ok(Arc::clone(GLOBAL_GPU.get().unwrap()))
}

/// Core GPU handle. One per process — construct once, reuse for every
/// kernel. Cheap to clone (internally Arc'd by wgpu).
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_info: wgpu::AdapterInfo,
}

impl Gpu {
    /// Acquire a GPU adapter. Blocks on adapter/device futures via
    /// pollster so callers don't need an async runtime. Panics if no
    /// compatible adapter is found — the caller should check the
    /// `gpu` feature before calling this.
    pub fn new() -> Result<Self, String> {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .ok_or_else(|| "no compatible GPU adapter found".to_string())?;
            let adapter_info = adapter.get_info();
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("cljrs-gpu"),
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::default(),
                        memory_hints: wgpu::MemoryHints::default(),
                    },
                    None,
                )
                .await
                .map_err(|e| format!("request_device failed: {e}"))?;
            Ok(Gpu {
                device,
                queue,
                adapter_info,
            })
        })
    }

    /// Run a single elementwise f32 kernel expressed as a WGSL source
    /// string. The kernel must follow the cljrs-gpu ABI:
    ///   @group(0) @binding(0) var<storage, read>       src: array<f32>;
    ///   @group(0) @binding(1) var<storage, read_write> dst: array<f32>;
    ///   @compute @workgroup_size(64) fn main(...) { dst[i] = f(src[i]) }
    ///
    /// Returns a fresh Vec<f32> with dst contents.
    pub fn run_elementwise_f32(&self, wgsl: &str, input: &[f32]) -> Result<Vec<f32>, String> {
        let n = input.len();
        if n == 0 {
            return Ok(Vec::new());
        }
        let bytes = (n * std::mem::size_of::<f32>()) as u64;

        let src_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("cljrs-gpu::src"),
                contents: bytemuck::cast_slice(input),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
        let dst_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cljrs-gpu::dst"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        // Readback staging buffer: MAP_READ requires a dedicated buffer
        // because GPU storage buffers can't be mapped directly.
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cljrs-gpu::readback"),
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cljrs-gpu::kernel"),
            source: wgpu::ShaderSource::Wgsl(wgsl.into()),
        });

        let bgl = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cljrs-gpu::bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cljrs-gpu::pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("cljrs-gpu::pipeline"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cljrs-gpu::bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: src_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dst_buf.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("cljrs-gpu::pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            // Workgroup size is 64 in the WGSL ABI; round up.
            let groups = ((n as u32) + 63) / 64;
            pass.dispatch_workgroups(groups, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&dst_buf, 0, &readback, 0, bytes);
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            tx.send(res).ok();
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|e| format!("map recv: {e}"))?
            .map_err(|e| format!("map async: {e}"))?;
        // Map succeeded — safe to read.
        let mapped = slice.get_mapped_range();
        let out: Vec<f32> = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        readback.unmap();
        Ok(out)
    }
}

// Keep the Pod/Zeroable imports live in case future APIs add typed
// uniform blocks. (silences unused-import warnings.)
#[allow(dead_code)]
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Params {
    n: u32,
    _pad: [u32; 3],
}

/// A compiled GPU kernel — holds its WGSL source and a lazily-built
/// pipeline. The pipeline is cached after first dispatch.
pub struct GpuKernel {
    pub name: String,
    pub wgsl: String,
    /// Lazily-initialized pipeline; cached per `Gpu` the kernel is
    /// first dispatched against. Kept as a Mutex so dispatch can be
    /// called through a shared reference.
    pipeline: std::sync::Mutex<Option<CachedPipeline>>,
}

struct CachedPipeline {
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
}

impl GpuKernel {
    pub fn from_wgsl(name: impl Into<String>, wgsl: impl Into<String>) -> Self {
        GpuKernel {
            name: name.into(),
            wgsl: wgsl.into(),
            pipeline: std::sync::Mutex::new(None),
        }
    }

    /// Run the kernel on an f32 slice; returns a fresh Vec<f32> of
    /// equal length. Pipeline built + cached on first call.
    pub fn run_f32(&self, gpu: &Gpu, input: &[f32]) -> Result<Vec<f32>, String> {
        // First call: compile + cache. Subsequent calls skip that cost,
        // which is what makes GPU benchmarking meaningful — we amortize
        // compile over many dispatches just like JAX/PyTorch do.
        self.ensure_pipeline(gpu)?;
        let guard = self.pipeline.lock().unwrap();
        let cached = guard.as_ref().expect("ensured");
        run_cached(gpu, cached, input)
    }

    fn ensure_pipeline(&self, gpu: &Gpu) -> Result<(), String> {
        let mut guard = self.pipeline.lock().unwrap();
        if guard.is_some() {
            return Ok(());
        }
        let module = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(&self.name),
                source: wgpu::ShaderSource::Wgsl(self.wgsl.as_str().into()),
            });
        let bgl = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cljrs-gpu::bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: false },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("cljrs-gpu::pl"),
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            });
        let pipeline = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(&self.name),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
        *guard = Some(CachedPipeline { pipeline, bgl });
        Ok(())
    }
}

fn run_cached(gpu: &Gpu, cached: &CachedPipeline, input: &[f32]) -> Result<Vec<f32>, String> {
    let n = input.len();
    if n == 0 {
        return Ok(Vec::new());
    }
    let bytes = (n * std::mem::size_of::<f32>()) as u64;
    let src_buf = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cljrs-gpu::src"),
            contents: bytemuck::cast_slice(input),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
    let dst_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cljrs-gpu::dst"),
        size: bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cljrs-gpu::readback"),
        size: bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cljrs-gpu::bg"),
        layout: &cached.bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: src_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: dst_buf.as_entire_binding(),
            },
        ],
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&cached.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        let groups = ((n as u32) + 63) / 64;
        pass.dispatch_workgroups(groups, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&dst_buf, 0, &readback, 0, bytes);
    gpu.queue.submit(std::iter::once(encoder.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |res| {
        tx.send(res).ok();
    });
    gpu.device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .map_err(|e| format!("map recv: {e}"))?
        .map_err(|e| format!("map async: {e}"))?;
    let mapped = slice.get_mapped_range();
    let out: Vec<f32> = bytemuck::cast_slice(&mapped).to_vec();
    drop(mapped);
    readback.unmap();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end: compile a cljrs body to WGSL via emit_elementwise,
    /// then dispatch and verify the math.
    #[test]
    fn dsl_elementwise_end_to_end() {
        let gpu = match Gpu::new() {
            Ok(g) => g,
            Err(e) => {
                eprintln!("skipping GPU DSL e2e: {e}");
                return;
            }
        };
        // Kernel: "saturate a waveform" — if |v| > 1, clamp via tanh-ish
        // approximation. We'll use the simpler:  out = v * 0.5 + sin(v)
        let body = crate::reader::read_all("(+ (* v 0.5) (sin v))")
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let wgsl = emit::emit_elementwise("i", "v", &body).expect("emit");
        let kernel = GpuKernel::from_wgsl("dsl_test", wgsl);
        let input: Vec<f32> = (0..2048).map(|i| (i as f32) * 0.01).collect();
        let out = kernel.run_f32(&gpu, &input).expect("run");
        for (idx, (&o, &v)) in out.iter().zip(input.iter()).enumerate() {
            let expected = v * 0.5 + v.sin();
            assert!(
                (o - expected).abs() < 1e-4,
                "idx {idx}: {o} vs {expected}"
            );
        }
    }

    /// Smoke test: dispatch a hand-written WGSL kernel that multiplies
    /// every element by 2. Green = wgpu works on this machine and the
    /// pipeline shape (bindings, dispatch, readback) is correct.
    #[test]
    fn elementwise_double_smoke() {
        let gpu = match Gpu::new() {
            Ok(g) => g,
            Err(e) => {
                // In CI with no GPU this may fail — skip rather than fail.
                eprintln!("skipping GPU smoke test: {e}");
                return;
            }
        };
        eprintln!(
            "gpu adapter: {} ({:?}, backend={:?})",
            gpu.adapter_info.name, gpu.adapter_info.device_type, gpu.adapter_info.backend
        );
        let wgsl = r#"
            @group(0) @binding(0) var<storage, read>       src: array<f32>;
            @group(0) @binding(1) var<storage, read_write> dst: array<f32>;
            @compute @workgroup_size(64)
            fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
                let i = gid.x;
                if (i >= arrayLength(&src)) { return; }
                dst[i] = src[i] * 2.0;
            }
        "#;
        let input: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        let output = gpu.run_elementwise_f32(wgsl, &input).expect("dispatch");
        assert_eq!(output.len(), input.len());
        for (i, (&o, &e)) in output.iter().zip(input.iter().map(|v| v * 2.0).collect::<Vec<_>>().iter()).enumerate() {
            assert!((o - e).abs() < 1e-6, "idx {i}: {o} vs {e}");
        }
    }
}
