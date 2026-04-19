//! Naive GPU mat-mul via wgpu compute shader.
//!
//! Scope: forward `C = A · B` over f32 row-major dense matrices.
//! Backward stays on CPU for now (the autograd graph flows through
//! the CPU path; only the *forward* matmul has a GPU option).
//!
//! Lifecycle:
//!   - Lazy: first call to `matmul_gpu` constructs a process-global
//!     `Gpu` (adapter + device + queue) and caches a single
//!     compute pipeline + bind-group layout.
//!   - Each invocation creates three buffers (A, B, C+staging),
//!     dispatches workgroups of 16x16 threads tiling the output, and
//!     reads C back synchronously via pollster.
//!
//! WGSL shader sketch:
//!
//! ```wgsl
//! struct Dims { m:u32, n:u32, k:u32, _pad:u32 };
//! @group(0) @binding(0) var<uniform>            dims : Dims;
//! @group(0) @binding(1) var<storage, read>      a    : array<f32>;
//! @group(0) @binding(2) var<storage, read>      b    : array<f32>;
//! @group(0) @binding(3) var<storage, read_write> c   : array<f32>;
//!
//! @compute @workgroup_size(16, 16, 1)
//! fn main(@builtin(global_invocation_id) gid : vec3<u32>) {
//!   let row = gid.y; let col = gid.x;
//!   if (row >= dims.m || col >= dims.n) { return; }
//!   var acc = 0.0;
//!   for (var i = 0u; i < dims.k; i = i + 1u) {
//!     acc = acc + a[row * dims.k + i] * b[i * dims.n + col];
//!   }
//!   c[row * dims.n + col] = acc;
//! }
//! ```
//!
//! Not warp-tiled, no shared-memory blocking. Good enough to demo
//! "matmul ran on the GPU and produced the same numbers". A v2 would
//! add tiled shared-mem and double-buffered loads.

#![cfg(not(target_arch = "wasm32"))]

use std::sync::{Arc, OnceLock};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Dims {
    m: u32,
    n: u32,
    k: u32,
    _pad: u32,
}

pub struct GpuMat {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

const SHADER: &str = r#"
struct Dims { m:u32, n:u32, k:u32, _pad:u32 };
@group(0) @binding(0) var<uniform>             dims : Dims;
@group(0) @binding(1) var<storage, read>       a    : array<f32>;
@group(0) @binding(2) var<storage, read>       b    : array<f32>;
@group(0) @binding(3) var<storage, read_write> c    : array<f32>;

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) gid : vec3<u32>) {
  let row : u32 = gid.y;
  let col : u32 = gid.x;
  if (row >= dims.m || col >= dims.n) { return; }
  var acc : f32 = 0.0;
  for (var i : u32 = 0u; i < dims.k; i = i + 1u) {
    acc = acc + a[row * dims.k + i] * b[i * dims.n + col];
  }
  c[row * dims.n + col] = acc;
}
"#;

impl GpuMat {
    fn new() -> Result<Self, String> {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .ok_or_else(|| "no compatible GPU adapter".to_string())?;
            let limits = adapter.limits();
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("cljrs-ml-gpu"),
                        required_features: wgpu::Features::empty(),
                        required_limits: limits,
                        memory_hints: wgpu::MemoryHints::Performance,
                    },
                    None,
                )
                .await
                .map_err(|e| format!("request_device failed: {e}"))?;
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("cljrs-ml-matmul"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("cljrs-ml-matmul-bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
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
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("cljrs-ml-matmul-pl"),
                bind_group_layouts: &[&layout],
                push_constant_ranges: &[],
            });
            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("cljrs-ml-matmul-pipeline"),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            });
            Ok(GpuMat {
                device,
                queue,
                pipeline,
                layout,
            })
        })
    }

    pub fn matmul(&self, a: &[f32], b: &[f32], m: usize, k: usize, n: usize) -> Vec<f32> {
        assert_eq!(a.len(), m * k);
        assert_eq!(b.len(), k * n);
        let dims = Dims {
            m: m as u32,
            n: n as u32,
            k: k as u32,
            _pad: 0,
        };
        let dims_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("dims"),
                contents: bytemuck::bytes_of(&dims),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let a_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("a"),
                contents: bytemuck::cast_slice(a),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let b_buf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("b"),
                contents: bytemuck::cast_slice(b),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let c_bytes = (m * n * std::mem::size_of::<f32>()) as u64;
        let c_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("c"),
            size: c_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("c-staging"),
            size: c_bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("matmul-bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: dims_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: a_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: b_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: c_buf.as_entire_binding(),
                },
            ],
        });
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("matmul-enc"),
            });
        {
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("matmul-pass"),
                timestamp_writes: None,
            });
            cp.set_pipeline(&self.pipeline);
            cp.set_bind_group(0, &bg, &[]);
            let gx = ((n as u32) + 15) / 16;
            let gy = ((m as u32) + 15) / 16;
            cp.dispatch_workgroups(gx.max(1), gy.max(1), 1);
        }
        enc.copy_buffer_to_buffer(&c_buf, 0, &staging, 0, c_bytes);
        self.queue.submit(Some(enc.finish()));

        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().expect("map failed");
        let data = slice.get_mapped_range();
        let out: Vec<f32> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        staging.unmap();
        out
    }
}

static GPU: OnceLock<Result<Arc<GpuMat>, String>> = OnceLock::new();

pub fn global() -> Result<Arc<GpuMat>, String> {
    let r = GPU.get_or_init(|| GpuMat::new().map(Arc::new));
    match r {
        Ok(g) => Ok(Arc::clone(g)),
        Err(e) => Err(e.clone()),
    }
}
