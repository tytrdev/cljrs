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
    let v_zoom_0: f32 = (f32(0.5) + (f32(3.5) * (f32(params.s0) / f32(1000.0))));
    let v_warp_1: f32 = (f32(2.0) * (f32(params.s1) / f32(1000.0)));
    let v_hue_rot_2: f32 = (f32(6.2831853) * (f32(params.s2) / f32(1000.0)));
    let v_brightness_3: f32 = (f32(0.2) + (f32(1.3) * (f32(params.s3) / f32(1000.0))));
    let v_u_4: f32 = (v_zoom_0 * ((f32(2.0) * (f32(k_x) / f32(i32(params.width)))) - f32(1.0)));
    let v_v_5: f32 = (v_zoom_0 * ((f32(2.0) * (f32(k_y) / f32(i32(params.height)))) - f32(1.0)));
    let v_t_6: f32 = (f32(0.001) * f32(params.t_ms));
    let v_wu_7: f32 = (v_u_4 + (v_warp_1 * sin(((v_v_5 * f32(3.0)) + v_t_6))));
    let v_wv_8: f32 = (v_v_5 + (v_warp_1 * cos(((v_u_4 * f32(2.5)) + (v_t_6 * f32(0.7))))));
    let v_a_9: f32 = sin(((v_wu_7 * f32(4.0)) + (v_t_6 * f32(1.2))));
    let v_b_10: f32 = sin(((v_wv_8 * f32(4.0)) + (v_t_6 * f32(1.7))));
    let v_c_11: f32 = sin((sqrt(((v_wu_7 * v_wu_7) + (v_wv_8 * v_wv_8))) + (v_t_6 * f32(2.3))));
    let v_v0_12: f32 = ((v_a_9 + v_b_10) + v_c_11);
    let v_s_13: f32 = (f32(0.5) * (f32(1.0) + sin((v_v0_12 + v_hue_rot_2))));
    let v_r_14: f32 = (v_brightness_3 * (f32(0.5) * (f32(1.0) + sin((f32(6.2831853) * v_s_13)))));
    let v_g_15: f32 = (v_brightness_3 * (f32(0.5) * (f32(1.0) + sin((f32(6.2831853) * (v_s_13 + f32(0.333)))))));
    let v_bl_16: f32 = (v_brightness_3 * (f32(0.5) * (f32(1.0) + sin((f32(6.2831853) * (v_s_13 + f32(0.666)))))));
    let v_ri_17: u32 = u32(min(i32((255i)), max(i32((0i)), i32((f32(255.0) * v_r_14)))));
    let v_gi_18: u32 = u32(min(i32((255i)), max(i32((0i)), i32((f32(255.0) * v_g_15)))));
    let v_bi_19: u32 = u32(min(i32((255i)), max(i32((0i)), i32((f32(255.0) * v_bl_16)))));
    dst[gid.y * params.width + gid.x] = (((v_ri_17 << u32((16i))) | (v_gi_18 << u32((8i)))) | v_bi_19);
}
