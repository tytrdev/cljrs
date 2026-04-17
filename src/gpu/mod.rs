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
            // Pull in the adapter's actual limits (e.g. Apple M3 supports
            // buffers well above wgpu's conservative 256MB default). Keeps
            // every feature we compile for without demanding optional
            // GPU features.
            let limits = adapter.limits();
            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("cljrs-gpu"),
                        required_features: wgpu::Features::empty(),
                        required_limits: limits,
                        memory_hints: wgpu::MemoryHints::Performance,
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

/// Mirrors WGSL's `struct BlendParams { alpha: f32, _pad: vec3<f32> }`.
/// vec3 aligns to 16 bytes, so `alpha` (offset 0) gets 12 bytes of
/// padding before the vec3 (offset 16, size 12). Struct stride rounds
/// up to 32 bytes. Getting this wrong produces a wgpu validation error
/// ("buffer is bound with size 16 where shader expects 32").
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct BlendUniform {
    alpha: f32,
    _pad_pre: [f32; 3],
    _pad_vec: [f32; 3],
    _pad_post: f32,
}

/// Uniform block shared by every pixel kernel. Mirrored in WGSL by the
/// `Params` struct in `emit.rs`. Keep in sync — size and field order
/// matter for the binding.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct PixelParams {
    pub width: u32,
    pub height: u32,
    pub t_ms: i32,
    pub s0: i32,
    pub s1: i32,
    pub s2: i32,
    pub s3: i32,
    pub _pad: i32,
}

/// A compiled pixel-shader-style GPU kernel. Renders a W×H u32 buffer
/// where each element is 0x00RRGGBB. Caches its pipeline after first
/// dispatch (amortizes compile cost over frames). Optionally maintains
/// a history buffer for temporal anti-aliasing.
pub struct GpuPixelKernel {
    pub name: String,
    pub wgsl: String,
    pipeline: std::sync::Mutex<Option<PixelPipeline>>,
    taa: std::sync::Mutex<Option<TaaState>>,
}

/// Persistent state for temporal AA. History buffer + blend pipeline.
/// Lives on the kernel, rebuilt if resolution changes.
struct TaaState {
    width: u32,
    height: u32,
    history: wgpu::Buffer,
    blend_pipeline: wgpu::ComputePipeline,
    blend_bgl: wgpu::BindGroupLayout,
    /// t_ms from the previous frame. Used to detect discontinuities
    /// (e.g. auto-loop reset) — if t jumps backward, we clear history.
    last_t_ms: i32,
}

/// Built-in WGSL: EMA blend of `current` with `history`. Writes result
/// into both `history` (for next frame) and an output buffer. Split 8-bit
/// channels so we can lerp independently.
const BLEND_WGSL: &str = r#"
struct BlendParams { alpha: f32, _pad: vec3<f32> };

@group(0) @binding(0) var<storage, read>       current: array<u32>;
@group(0) @binding(1) var<storage, read_write> history: array<u32>;
@group(0) @binding(2) var<storage, read_write> out_buf: array<u32>;
@group(0) @binding(3) var<uniform>             blend_params: BlendParams;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= arrayLength(&current)) { return; }
    let c = current[i];
    let h = history[i];
    let cr = f32((c >> 16u) & 0xffu);
    let cg = f32((c >> 8u)  & 0xffu);
    let cb = f32(c & 0xffu);
    let hr = f32((h >> 16u) & 0xffu);
    let hg = f32((h >> 8u)  & 0xffu);
    let hb = f32(h & 0xffu);
    let a = blend_params.alpha;
    let nr = u32(clamp(a * cr + (1.0 - a) * hr, 0.0, 255.0));
    let ng = u32(clamp(a * cg + (1.0 - a) * hg, 0.0, 255.0));
    let nb = u32(clamp(a * cb + (1.0 - a) * hb, 0.0, 255.0));
    let packed = (nr << 16u) | (ng << 8u) | nb;
    history[i] = packed;
    out_buf[i] = packed;
}
"#;

struct PixelPipeline {
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
}

impl GpuPixelKernel {
    pub fn from_wgsl(name: impl Into<String>, wgsl: impl Into<String>) -> Self {
        GpuPixelKernel {
            name: name.into(),
            wgsl: wgsl.into(),
            pipeline: std::sync::Mutex::new(None),
            taa: std::sync::Mutex::new(None),
        }
    }

    fn ensure_taa(&self, gpu: &Gpu, width: u32, height: u32) {
        let mut guard = self.taa.lock().unwrap();
        let need_rebuild = match &*guard {
            Some(s) => s.width != width || s.height != height,
            None => true,
        };
        if !need_rebuild {
            return;
        }
        let bytes = (width as u64) * (height as u64) * 4;
        let history = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cljrs-gpu::taa-history"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // Zero the history so first blend doesn't pick up garbage.
        gpu.queue.write_buffer(&history, 0, &vec![0u8; bytes as usize]);
        let module = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cljrs-gpu::taa-blend"),
            source: wgpu::ShaderSource::Wgsl(BLEND_WGSL.into()),
        });
        let bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cljrs-gpu::taa-bgl"),
            entries: &[
                // current (read-only storage)
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
                // history (read-write storage)
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
                // out_buf (read-write storage)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // blend_params (uniform)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pl = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cljrs-gpu::taa-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("cljrs-gpu::taa-blend"),
            layout: Some(&pl),
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        *guard = Some(TaaState {
            width,
            height,
            history,
            blend_pipeline: pipeline,
            blend_bgl: bgl,
            last_t_ms: i32::MIN,
        });
    }

    /// Render a frame with temporal anti-aliasing.
    ///
    /// Approach: render the kernel as usual, then blend its output with
    /// a persistent "history" buffer using exponential moving average
    /// (new = alpha * current + (1 - alpha) * history). Over N frames
    /// this gives an effective sample count of ~1/alpha, at no extra
    /// per-frame kernel cost beyond one cheap blend pass.
    ///
    /// `alpha` in [0, 1]: 1.0 = no TAA (just current frame), 0.1 ≈ 10
    /// frames of averaging. For a smoothly zooming fractal 0.15..0.3 is
    /// a sweet spot — enough averaging to kill aliasing flicker without
    /// visible motion smear at typical zoom rates.
    ///
    /// If `params.t_ms` jumps backward (e.g. loop reset), history is
    /// automatically cleared so the new cycle starts clean.
    pub fn render_frame_taa(
        &self,
        gpu: &Gpu,
        params: PixelParams,
        alpha: f32,
    ) -> Result<Vec<u32>, String> {
        self.ensure_pipeline(gpu)?;
        self.ensure_taa(gpu, params.width, params.height);
        let pipe_guard = self.pipeline.lock().unwrap();
        let cached = pipe_guard.as_ref().expect("ensured");
        let mut taa_guard = self.taa.lock().unwrap();
        let taa = taa_guard.as_mut().expect("ensured");

        // Detect time discontinuity (loop reset): clear history.
        let reset = params.t_ms + 1000 < taa.last_t_ms;
        if reset {
            let bytes = (taa.width as u64) * (taa.height as u64) * 4;
            gpu.queue.write_buffer(&taa.history, 0, &vec![0u8; bytes as usize]);
        }
        taa.last_t_ms = params.t_ms;
        // On very first frame (still implies history=0), use alpha=1.0.
        let effective_alpha = if reset { 1.0_f32 } else { alpha };

        let n = (params.width as usize) * (params.height as usize);
        let bytes = (n * std::mem::size_of::<u32>()) as u64;

        // Kernel's own uniforms + current (output) buffer.
        let uniform = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cljrs-gpu::px-uniform"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let current = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cljrs-gpu::taa-current"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let output = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cljrs-gpu::taa-output"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cljrs-gpu::taa-readback"),
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Kernel pass: write into `current`.
        let kernel_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &cached.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: current.as_entire_binding(),
                },
            ],
        });

        // Blend pass uniforms.
        let blend_params = BlendUniform {
            alpha: effective_alpha,
            _pad_pre: [0.0; 3],
            _pad_vec: [0.0; 3],
            _pad_post: 0.0,
        };
        let blend_uniform = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cljrs-gpu::taa-blend-uniform"),
            contents: bytemuck::bytes_of(&blend_params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let blend_bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &taa.blend_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: current.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: taa.history.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: blend_uniform.as_entire_binding(),
                },
            ],
        });

        let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("taa::kernel"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&cached.pipeline);
            pass.set_bind_group(0, &kernel_bg, &[]);
            let gx = (params.width + 7) / 8;
            let gy = (params.height + 7) / 8;
            pass.dispatch_workgroups(gx, gy, 1);
        }
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("taa::blend"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&taa.blend_pipeline);
            pass.set_bind_group(0, &blend_bg, &[]);
            let groups = ((n as u32) + 63) / 64;
            pass.dispatch_workgroups(groups, 1, 1);
        }
        enc.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
        gpu.queue.submit(std::iter::once(enc.finish()));
        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| { tx.send(res).ok(); });
        gpu.device.poll(wgpu::Maintain::Wait);
        rx.recv().map_err(|e| format!("map recv: {e}"))?
            .map_err(|e| format!("map async: {e}"))?;
        let mapped = slice.get_mapped_range();
        let out: Vec<u32> = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        readback.unmap();
        Ok(out)
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
        let bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cljrs-gpu::px-bgl"),
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
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pl = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cljrs-gpu::px-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(&self.name),
            layout: Some(&pl),
            module: &module,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        *guard = Some(PixelPipeline { pipeline, bgl });
        Ok(())
    }

    /// Render one frame into a fresh Vec<u32>. Each element is the
    /// packed pixel color (0x00RRGGBB). Callers flatten to RGBA bytes
    /// for display. Rendering reuses GPU buffers for the readback path
    /// per call — this is fine up to ~4K. For real-time at 4K you'd
    /// want buffer pooling; out of scope for v0.
    pub fn render_frame(
        &self,
        gpu: &Gpu,
        params: PixelParams,
    ) -> Result<Vec<u32>, String> {
        self.ensure_pipeline(gpu)?;
        let guard = self.pipeline.lock().unwrap();
        let cached = guard.as_ref().expect("ensured");

        let n = (params.width as usize) * (params.height as usize);
        let bytes = (n * std::mem::size_of::<u32>()) as u64;
        let uniform = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("cljrs-gpu::px-uniform"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let dst = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cljrs-gpu::px-dst"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cljrs-gpu::px-readback"),
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &cached.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dst.as_entire_binding(),
                },
            ],
        });
        let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&cached.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            let gx = (params.width + 7) / 8;
            let gy = (params.height + 7) / 8;
            pass.dispatch_workgroups(gx, gy, 1);
        }
        enc.copy_buffer_to_buffer(&dst, 0, &readback, 0, bytes);
        gpu.queue.submit(std::iter::once(enc.finish()));
        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| { tx.send(res).ok(); });
        gpu.device.poll(wgpu::Maintain::Wait);
        rx.recv().map_err(|e| format!("map recv: {e}"))?
            .map_err(|e| format!("map async: {e}"))?;
        let mapped = slice.get_mapped_range();
        let out: Vec<u32> = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        readback.unmap();
        Ok(out)
    }
}

/// A compiled GPU kernel. Holds WGSL source, a lazily-built pipeline,
/// and a per-size buffer cache so repeated dispatches at the same
/// element count reuse allocation.
pub struct GpuKernel {
    pub name: String,
    pub wgsl: String,
    pipeline: std::sync::Mutex<Option<CachedPipeline>>,
    /// Input/output/readback buffer trio keyed by element count. Grown
    /// on demand. A benchmark or render loop avoids per-call allocation
    /// after the first warmup, which matters a lot at small N (latency
    /// bound) and a measurable amount at large N.
    f32_cache: std::sync::Mutex<Option<F32BufferCache>>,
}

struct CachedPipeline {
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
}

struct F32BufferCache {
    n: usize,
    src: wgpu::Buffer,
    dst: wgpu::Buffer,
    readback: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl GpuKernel {
    pub fn from_wgsl(name: impl Into<String>, wgsl: impl Into<String>) -> Self {
        GpuKernel {
            name: name.into(),
            wgsl: wgsl.into(),
            pipeline: std::sync::Mutex::new(None),
            f32_cache: std::sync::Mutex::new(None),
        }
    }

    /// Run the kernel on an f32 slice. Returns a fresh Vec<f32>.
    /// Pipeline is compiled on first call. Buffers of matching size are
    /// reused across calls.
    pub fn run_f32(&self, gpu: &Gpu, input: &[f32]) -> Result<Vec<f32>, String> {
        self.ensure_pipeline(gpu)?;
        let pipe_guard = self.pipeline.lock().unwrap();
        let cached = pipe_guard.as_ref().expect("ensured");
        self.ensure_f32_buffers(gpu, cached, input.len())?;
        let buf_guard = self.f32_cache.lock().unwrap();
        let buf = buf_guard.as_ref().expect("ensured");
        run_with_cache(gpu, cached, buf, Some(input))
    }

    /// Re-run the kernel using whatever input was last uploaded via
    /// `run_f32`. Skips the CPU to GPU upload, which dominates at
    /// repeated benchmarks where the input doesn't change. Panics if
    /// called before `run_f32` has established a buffer cache.
    pub fn run_f32_reuse_input(&self, gpu: &Gpu, n: usize) -> Result<Vec<f32>, String> {
        let pipe_guard = self.pipeline.lock().unwrap();
        let cached = pipe_guard
            .as_ref()
            .ok_or("run_f32 must be called at least once before run_f32_reuse_input")?;
        let buf_guard = self.f32_cache.lock().unwrap();
        let buf = buf_guard
            .as_ref()
            .ok_or("buffer cache not initialized; call run_f32 first")?;
        if buf.n != n {
            return Err(format!("cached buffer has size {}, requested {n}", buf.n));
        }
        run_with_cache(gpu, cached, buf, None)
    }

    fn ensure_f32_buffers(
        &self,
        gpu: &Gpu,
        cached: &CachedPipeline,
        n: usize,
    ) -> Result<(), String> {
        let mut guard = self.f32_cache.lock().unwrap();
        if let Some(cache) = guard.as_ref()
            && cache.n == n
        {
            return Ok(());
        }
        let bytes = (n.max(1) * std::mem::size_of::<f32>()) as u64;
        let src = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cljrs-gpu::src"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dst = gpu.device.create_buffer(&wgpu::BufferDescriptor {
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
                wgpu::BindGroupEntry { binding: 0, resource: src.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: dst.as_entire_binding() },
            ],
        });
        *guard = Some(F32BufferCache { n, src, dst, readback, bind_group });
        Ok(())
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

/// Dispatch using a cached buffer trio. Upload input, dispatch, copy
/// dst into the mapped-readback buffer, map, copy out.
fn run_with_cache(
    gpu: &Gpu,
    cached: &CachedPipeline,
    buf: &F32BufferCache,
    input: Option<&[f32]>,
) -> Result<Vec<f32>, String> {
    let n = input.map(|i| i.len()).unwrap_or(buf.n);
    if n == 0 {
        return Ok(Vec::new());
    }
    let bytes = (n * std::mem::size_of::<f32>()) as u64;
    if let Some(data) = input {
        gpu.queue.write_buffer(&buf.src, 0, bytemuck::cast_slice(data));
    }

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&cached.pipeline);
        pass.set_bind_group(0, &buf.bind_group, &[]);
        // WebGPU caps workgroup count at 65535/dim. Kernels handling
        // more elements than that at workgroup_size * 65535 must use a
        // grid-stride loop (cap enforced here).
        let needed = ((n as u32) + 63) / 64;
        let groups = needed.min(16384);
        pass.dispatch_workgroups(groups, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&buf.dst, 0, &buf.readback, 0, bytes);
    gpu.queue.submit(std::iter::once(encoder.finish()));

    let slice = buf.readback.slice(..bytes);
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
    buf.readback.unmap();
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
