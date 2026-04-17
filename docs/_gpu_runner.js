// Run a cljrs-compiled WGSL kernel in a <canvas> via WebGPU.
//
// Same ABI as the native gpu-demo binary:
//   uniform Params { width, height, t_ms, s0, s1, s2, s3, _pad }
//   storage dst: array<u32>  (each u32 = 0x00RRGGBB)
//
// Workflow each frame:
//   1) write uniforms
//   2) dispatch compute (WxH / 8x8 workgroups)
//   3) convert packed u32 → RGBA, draw to canvas via ImageData
//
// We use a second "blit" compute pass that unpacks the u32 into RGBA
// bytes directly on the GPU, then read back via a mapped buffer. This
// is simple and fast enough for the docs demo (no swap-chain dance).

const BLIT_WGSL = /* wgsl */ `
@group(0) @binding(0) var<storage, read>       src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;  // packed rgba per pixel

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= arrayLength(&src)) { return; }
  let c = src[i];
  let r = (c >> 16u) & 0xffu;
  let g = (c >> 8u)  & 0xffu;
  let b =  c         & 0xffu;
  dst[i] = (0xffu << 24u) | (b << 16u) | (g << 8u) | r;
}
`;

export class GpuRunner {
  constructor(canvas, wgsl, { width = 512, height = 288 } = {}) {
    this.canvas = canvas;
    this.wgsl = wgsl;
    this.width = width;
    this.height = height;
    this.canvas.width = width;
    this.canvas.height = height;
    this.ctx = canvas.getContext("2d");
    this.imageData = this.ctx.createImageData(width, height);
    this.initialized = false;
    this.rafHandle = null;
    this.startedAt = performance.now();
    this.sliders = [500, 500, 500, 500];
    this.fpsEma = 0;
    this.lastFrameAt = 0;
    this.onStats = null; // optional callback(fps)
  }

  async init() {
    if (!navigator.gpu) {
      throw new Error(
        "WebGPU not available in this browser. Try Chrome 113+, Edge, or a recent Safari Tech Preview."
      );
    }
    const adapter = await navigator.gpu.requestAdapter();
    if (!adapter) throw new Error("no compatible GPU adapter");
    this.device = await adapter.requestDevice();
    this.adapterInfo = adapter.info || {};

    const n = this.width * this.height;
    const bytes = n * 4;
    this.uniformBuf = this.device.createBuffer({
      size: 32, // matches Params struct (i32 × 8)
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    this.computeOut = this.device.createBuffer({
      size: bytes,
      usage: GPUBufferUsage.STORAGE,
    });
    this.rgbaBuf = this.device.createBuffer({
      size: bytes,
      usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
    });
    this.readbackBuf = this.device.createBuffer({
      size: bytes,
      usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
    });

    // Kernel pipeline: cljrs-compiled WGSL.
    const kernelModule = this.device.createShaderModule({
      label: "cljrs-kernel",
      code: this.wgsl,
    });
    this.kernelPipeline = await this.device.createComputePipelineAsync({
      label: "cljrs-kernel",
      layout: "auto",
      compute: { module: kernelModule, entryPoint: "main" },
    });
    this.kernelBG = this.device.createBindGroup({
      layout: this.kernelPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: this.uniformBuf } },
        { binding: 1, resource: { buffer: this.computeOut } },
      ],
    });

    // Blit pipeline: unpacks 0x00RRGGBB → canvas-native RGBA bytes.
    const blitModule = this.device.createShaderModule({
      label: "cljrs-blit",
      code: BLIT_WGSL,
    });
    this.blitPipeline = await this.device.createComputePipelineAsync({
      label: "cljrs-blit",
      layout: "auto",
      compute: { module: blitModule, entryPoint: "main" },
    });
    this.blitBG = this.device.createBindGroup({
      layout: this.blitPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: this.computeOut } },
        { binding: 1, resource: { buffer: this.rgbaBuf } },
      ],
    });

    this.initialized = true;
  }

  setSliders(arr) {
    this.sliders = arr.slice(0, 4);
  }

  async step() {
    if (!this.initialized) return;
    const t_ms = Math.floor(performance.now() - this.startedAt);
    const params = new ArrayBuffer(32);
    const u32 = new Uint32Array(params);
    const i32 = new Int32Array(params);
    u32[0] = this.width;
    u32[1] = this.height;
    i32[2] = t_ms;
    i32[3] = this.sliders[0];
    i32[4] = this.sliders[1];
    i32[5] = this.sliders[2];
    i32[6] = this.sliders[3];
    i32[7] = 0;
    this.device.queue.writeBuffer(this.uniformBuf, 0, params);

    const enc = this.device.createCommandEncoder();
    {
      const pass = enc.beginComputePass();
      pass.setPipeline(this.kernelPipeline);
      pass.setBindGroup(0, this.kernelBG);
      const gx = Math.ceil(this.width / 8);
      const gy = Math.ceil(this.height / 8);
      pass.dispatchWorkgroups(gx, gy, 1);
      pass.end();
    }
    {
      const pass = enc.beginComputePass();
      pass.setPipeline(this.blitPipeline);
      pass.setBindGroup(0, this.blitBG);
      pass.dispatchWorkgroups(Math.ceil((this.width * this.height) / 64), 1, 1);
      pass.end();
    }
    enc.copyBufferToBuffer(
      this.rgbaBuf, 0,
      this.readbackBuf, 0,
      this.width * this.height * 4
    );
    this.device.queue.submit([enc.finish()]);

    await this.readbackBuf.mapAsync(GPUMapMode.READ);
    const data = new Uint8ClampedArray(this.readbackBuf.getMappedRange().slice(0));
    this.readbackBuf.unmap();
    this.imageData.data.set(data);
    this.ctx.putImageData(this.imageData, 0, 0);

    // FPS EMA
    const now = performance.now();
    if (this.lastFrameAt) {
      const dt = now - this.lastFrameAt;
      const fps = 1000 / Math.max(dt, 0.001);
      this.fpsEma = this.fpsEma ? 0.9 * this.fpsEma + 0.1 * fps : fps;
      if (this.onStats) this.onStats(this.fpsEma);
    }
    this.lastFrameAt = now;
  }

  start() {
    const loop = async () => {
      await this.step();
      this.rafHandle = requestAnimationFrame(loop);
    };
    this.rafHandle = requestAnimationFrame(loop);
  }

  stop() {
    if (this.rafHandle) cancelAnimationFrame(this.rafHandle);
    this.rafHandle = null;
  }
}
