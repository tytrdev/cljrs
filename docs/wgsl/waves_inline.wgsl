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
    let v_freq_0: f32 = (f32(0.5) + (f32(19.5) * (f32(params.s1) / f32(1000.0))));
    let v_speed_1: f32 = (f32(4.0) * (f32(params.s2) / f32(1000.0)));
    let v_hue_2: f32 = (f32(6.2831853) * (f32(params.s3) / f32(1000.0)));
    let v_t_3: f32 = (f32(0.001) * f32(params.t_ms));
    let v_aspect_4: f32 = (f32(i32(params.width)) / f32(i32(params.height)));
    let v_u_5: f32 = (v_aspect_4 * ((f32(2.0) * (f32(k_x) / f32(i32(params.width)))) - f32(1.0)));
    let v_v_6: f32 = ((f32(2.0) * (f32(k_y) / f32(i32(params.height)))) - f32(1.0));
    let v_a1_7: f32 = f32(0.0);
    let v_b1_8: f32 = f32(0.0);
    let v_a2_9: f32 = f32(0.7);
    let v_b2_10: f32 = f32(0.5);
    let v_a3_11: f32 = (-f32(0.6));
    let v_b3_12: f32 = (-f32(0.7));
    let v_a4_13: f32 = f32(0.3);
    let v_b4_14: f32 = (-f32(0.3));
    let v_d1_15: f32 = sqrt((((v_u_5 - v_a1_7) * (v_u_5 - v_a1_7)) + ((v_v_6 - v_b1_8) * (v_v_6 - v_b1_8))));
    let v_d2_16: f32 = sqrt((((v_u_5 - v_a2_9) * (v_u_5 - v_a2_9)) + ((v_v_6 - v_b2_10) * (v_v_6 - v_b2_10))));
    let v_d3_17: f32 = sqrt((((v_u_5 - v_a3_11) * (v_u_5 - v_a3_11)) + ((v_v_6 - v_b3_12) * (v_v_6 - v_b3_12))));
    let v_d4_18: f32 = sqrt((((v_u_5 - v_a4_13) * (v_u_5 - v_a4_13)) + ((v_v_6 - v_b4_14) * (v_v_6 - v_b4_14))));
    let v_p1_19: f32 = sin(((v_freq_0 * v_d1_15) - (v_speed_1 * v_t_3)));
    let v_p2_20: f32 = sin(((v_freq_0 * v_d2_16) - (v_speed_1 * v_t_3)));
    let v_p3_21: f32 = sin(((v_freq_0 * v_d3_17) - (v_speed_1 * v_t_3)));
    let v_p4_22: f32 = sin(((v_freq_0 * v_d4_18) - (v_speed_1 * v_t_3)));
    let v_sum_23: f32 = ((v_p1_19 + v_p2_20) + (v_p3_21 + v_p4_22));
    let v_n_24: f32 = (f32(0.5) * (f32(1.0) + (f32(0.25) * v_sum_23)));
    let v_r_25: f32 = (f32(0.5) * (f32(1.0) + sin((v_hue_2 + (f32(6.28) * v_n_24)))));
    let v_g_26: f32 = (f32(0.5) * (f32(1.0) + sin(((v_hue_2 + f32(2.0)) + (f32(6.28) * v_n_24)))));
    let v_b_27: f32 = (f32(0.5) * (f32(1.0) + sin(((v_hue_2 + f32(4.0)) + (f32(6.28) * v_n_24)))));
    let v_ri_28: u32 = u32(min(i32((255i)), max(i32((0i)), i32((f32(255.0) * v_r_25)))));
    let v_gi_29: u32 = u32(min(i32((255i)), max(i32((0i)), i32((f32(255.0) * v_g_26)))));
    let v_bi_30: u32 = u32(min(i32((255i)), max(i32((0i)), i32((f32(255.0) * v_b_27)))));
    dst[gid.y * params.width + gid.x] = (((v_ri_28 << u32((16i))) | (v_gi_29 << u32((8i)))) | v_bi_30);
}
