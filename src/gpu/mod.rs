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
//! - `mod.rs`   — `Gpu` handle: adapter/device/queue, pipeline cache.
//! - `emit.rs`  — cljrs AST → WGSL text (the DSL lives here).
//! - `kernel.rs` — compile + dispatch a WGSL kernel, read back results.
//!
//! # Phase 0 (this file): smoke test
//! Hand-written WGSL kernel, dispatched through the full pipeline. If
//! this works on a machine, the kernel DSL will too — the DSL only
//! changes the *text* of the shader, not any of the setup around it.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

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

#[cfg(test)]
mod tests {
    use super::*;

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
