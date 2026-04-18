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
    let freq: f32 = 0.5 + (19.5 * (f32(params.s1) / 1000.0));
    let speed: f32 = 4.0 * (f32(params.s2) / 1000.0);
    let hue: f32 = 6.2831853 * (f32(params.s3) / 1000.0);
    let t: f32 = 0.001 * f32(params.t_ms);
    let aspect: f32 = f32(i32(params.width)) / f32(i32(params.height));
    let u: f32 = aspect * ((2.0 * (f32(k_x) / f32(i32(params.width)))) - 1.0);
    let v: f32 = (2.0 * (f32(k_y) / f32(i32(params.height)))) - 1.0;
    let a1: f32 = 0.0;
    let b1: f32 = 0.0;
    let a2: f32 = 0.7;
    let b2: f32 = 0.5;
    let a3: f32 = -0.6;
    let b3: f32 = -0.7;
    let a4: f32 = 0.3;
    let b4: f32 = -0.3;
    let d1: f32 = sqrt((((u - a1) * (u - a1)) + ((v - b1) * (v - b1))));
    let d2: f32 = sqrt((((u - a2) * (u - a2)) + ((v - b2) * (v - b2))));
    let d3: f32 = sqrt((((u - a3) * (u - a3)) + ((v - b3) * (v - b3))));
    let d4: f32 = sqrt((((u - a4) * (u - a4)) + ((v - b4) * (v - b4))));
    let p1: f32 = sin(((freq * d1) - (speed * t)));
    let p2: f32 = sin(((freq * d2) - (speed * t)));
    let p3: f32 = sin(((freq * d3) - (speed * t)));
    let p4: f32 = sin(((freq * d4) - (speed * t)));
    let sum: f32 = (p1 + p2) + (p3 + p4);
    let n: f32 = 0.5 * (1.0 + (0.25 * sum));
    let r: f32 = 0.5 * (1.0 + sin((hue + (6.28 * n))));
    let g: f32 = 0.5 * (1.0 + sin(((hue + 2.0) + (6.28 * n))));
    let b: f32 = 0.5 * (1.0 + sin(((hue + 4.0) + (6.28 * n))));
    let ri: u32 = u32(min(255i, max(0i, i32((255.0 * r)))));
    let gi: u32 = u32(min(255i, max(0i, i32((255.0 * g)))));
    let bi: u32 = u32(min(255i, max(0i, i32((255.0 * b)))));
    dst[gid.y * params.width + gid.x] = (((ri << 16u) | (gi << 8u)) | bi);
}
