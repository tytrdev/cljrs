struct Params {
  width: u32,
  height: u32,
  t_ms: i32,
  s0: i32,
  s1: i32,
  s2: i32,
  s3: i32,
  _pad: i32,
};

@group(0) @binding(0) var<uniform>           params: Params;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) { return; }
    let k_x: i32 = i32(gid.x);
    let k_y: i32 = i32(gid.y);
    let zoom: f32 = 0.5 + (3.5 * (f32(params.s0) / 1000.0));
    let warp: f32 = 2.0 * (f32(params.s1) / 1000.0);
    let hue_rot: f32 = 6.2831853 * (f32(params.s2) / 1000.0);
    let brightness: f32 = 0.2 + (1.3 * (f32(params.s3) / 1000.0));
    let u: f32 = zoom * ((2.0 * (f32(k_x) / f32(i32(params.width)))) - 1.0);
    let v: f32 = zoom * ((2.0 * (f32(k_y) / f32(i32(params.height)))) - 1.0);
    let t: f32 = 0.001 * f32(params.t_ms);
    let wu: f32 = u + (warp * sin(((v * 3.0) + t)));
    let wv: f32 = v + (warp * cos(((u * 2.5) + (t * 0.7))));
    let a: f32 = sin(((wu * 4.0) + (t * 1.2)));
    let b: f32 = sin(((wv * 4.0) + (t * 1.7)));
    let c: f32 = sin((sqrt(((wu * wu) + (wv * wv))) + (t * 2.3)));
    let v0: f32 = (a + b) + c;
    let s: f32 = 0.5 * (1.0 + sin((v0 + hue_rot)));
    let r: f32 = brightness * (0.5 * (1.0 + sin((6.2831853 * s))));
    let g: f32 = brightness * (0.5 * (1.0 + sin((6.2831853 * (s + 0.333)))));
    let bl: f32 = brightness * (0.5 * (1.0 + sin((6.2831853 * (s + 0.666)))));
    let ri: u32 = u32(min(255i, max(0i, i32((255.0 * r)))));
    let gi: u32 = u32(min(255i, max(0i, i32((255.0 * g)))));
    let bi: u32 = u32(min(255i, max(0i, i32((255.0 * bl)))));
    dst[gid.y * params.width + gid.x] = (((ri << 16u) | (gi << 8u)) | bi);
}
