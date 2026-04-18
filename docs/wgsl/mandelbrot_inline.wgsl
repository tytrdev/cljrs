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
    let v_rate_0: f32 = (f32(0.05) + (f32(0.3) * (f32(params.s0) / f32(1000.0))));
    let v_t_sec_1: f32 = (f32(0.001) * f32(params.t_ms));
    let v_period_2: f32 = f32(30.0);
    let v_t_cycle_3: f32 = (v_t_sec_1 - (v_period_2 * floor((v_t_sec_1 / v_period_2))));
    let v_zoom_4: f32 = exp((v_rate_0 * v_t_cycle_3));
    let v_tx_5: f32 = f32(-0.7436438870371587);
    let v_ty_6: f32 = f32(0.1318259042053119);
    let v_dx_7: f32 = (f32(0.001) * ((f32(2.0) * (f32(params.s1) / f32(1000.0))) - f32(1.0)));
    let v_dy_8: f32 = (f32(0.001) * ((f32(2.0) * (f32(params.s2) / f32(1000.0))) - f32(1.0)));
    let v_cx_9: f32 = (v_tx_5 + v_dx_7);
    let v_cy_10: f32 = (v_ty_6 + v_dy_8);
    let v_shift_11: f32 = (f32(6.2831853) * (f32(params.s3) / f32(1000.0)));
    let v_aspect_12: f32 = (f32(i32(params.width)) / f32(i32(params.height)));
    let v_fw_13: f32 = f32(i32(params.width));
    let v_fh_14: f32 = f32(i32(params.height));
    let v_iter_float_15: f32 = (f32(100.0) + (f32(40.0) * (log(v_zoom_4) / f32(0.6931))));
    let v_max_iter_16: i32 = min(i32((2000i)), max(i32((64i)), i32(v_iter_float_15)));
    let v_max_iter_f_17: f32 = f32(v_max_iter_16);
    let v_px_size_18: f32 = (f32(2.0) / (v_fw_13 * v_zoom_4));
    let v_py_size_19: f32 = (f32(2.0) / (v_fh_14 * v_zoom_4));
    let v_u0_20: f32 = (((f32(2.0) * (f32(k_x) / v_fw_13)) - f32(1.0)) / v_zoom_4);
    let v_v0_21: f32 = (((f32(2.0) * (f32(k_y) / v_fh_14)) - f32(1.0)) / v_zoom_4);
    let v_base_re_22: f32 = (v_cx_9 + (v_u0_20 * v_aspect_12));
    let v_base_im_23: f32 = (v_cy_10 + v_v0_21);
    let v_c_re__24: f32 = (v_base_re_22 + (v_px_size_18 * (f32(-0.4375) * v_aspect_12)));
    let v_c_im__25: f32 = (v_base_im_23 + (v_py_size_19 * f32(-0.0625)));
    var _lv26_z_re: f32 = v_c_re__24;
    var _lv27_z_im: f32 = v_c_im__25;
    var _lv28_it: i32 = i32((0i));
    var _lr29: f32 = 0.0;
    loop {
    if ((_lv28_it >= v_max_iter_16)) {
    _lr29 = v_max_iter_f_17;
    break;
    } else {
    if ((((_lv26_z_re * _lv26_z_re) + (_lv27_z_im * _lv27_z_im)) >= f32(4.0))) {
    _lr29 = (f32(_lv28_it) + (f32(1.0) - (log(log(((_lv26_z_re * _lv26_z_re) + (_lv27_z_im * _lv27_z_im)))) / f32(0.6931))));
    break;
    } else {
    let _rt30: f32 = (((_lv26_z_re * _lv26_z_re) - (_lv27_z_im * _lv27_z_im)) + v_c_re__24);
    let _rt31: f32 = ((f32(2.0) * (_lv26_z_re * _lv27_z_im)) + v_c_im__25);
    let _rt32: i32 = (_lv28_it + i32((1i)));
    _lv26_z_re = _rt30;
    _lv27_z_im = _rt31;
    _lv28_it = _rt32;
    continue;
    }
    }
    }
    let v_s1v_33: f32 = _lr29;
    let v_c_re__34: f32 = (v_base_re_22 + (v_px_size_18 * (f32(-0.1875) * v_aspect_12)));
    let v_c_im__35: f32 = (v_base_im_23 + (v_py_size_19 * f32(-0.3125)));
    var _lv36_z_re: f32 = v_c_re__34;
    var _lv37_z_im: f32 = v_c_im__35;
    var _lv38_it: i32 = i32((0i));
    var _lr39: f32 = 0.0;
    loop {
    if ((_lv38_it >= v_max_iter_16)) {
    _lr39 = v_max_iter_f_17;
    break;
    } else {
    if ((((_lv36_z_re * _lv36_z_re) + (_lv37_z_im * _lv37_z_im)) >= f32(4.0))) {
    _lr39 = (f32(_lv38_it) + (f32(1.0) - (log(log(((_lv36_z_re * _lv36_z_re) + (_lv37_z_im * _lv37_z_im)))) / f32(0.6931))));
    break;
    } else {
    let _rt40: f32 = (((_lv36_z_re * _lv36_z_re) - (_lv37_z_im * _lv37_z_im)) + v_c_re__34);
    let _rt41: f32 = ((f32(2.0) * (_lv36_z_re * _lv37_z_im)) + v_c_im__35);
    let _rt42: i32 = (_lv38_it + i32((1i)));
    _lv36_z_re = _rt40;
    _lv37_z_im = _rt41;
    _lv38_it = _rt42;
    continue;
    }
    }
    }
    let v_s2v_43: f32 = _lr39;
    let v_c_re__44: f32 = (v_base_re_22 + (v_px_size_18 * (f32(0.0625) * v_aspect_12)));
    let v_c_im__45: f32 = (v_base_im_23 + (v_py_size_19 * f32(-0.4375)));
    var _lv46_z_re: f32 = v_c_re__44;
    var _lv47_z_im: f32 = v_c_im__45;
    var _lv48_it: i32 = i32((0i));
    var _lr49: f32 = 0.0;
    loop {
    if ((_lv48_it >= v_max_iter_16)) {
    _lr49 = v_max_iter_f_17;
    break;
    } else {
    if ((((_lv46_z_re * _lv46_z_re) + (_lv47_z_im * _lv47_z_im)) >= f32(4.0))) {
    _lr49 = (f32(_lv48_it) + (f32(1.0) - (log(log(((_lv46_z_re * _lv46_z_re) + (_lv47_z_im * _lv47_z_im)))) / f32(0.6931))));
    break;
    } else {
    let _rt50: f32 = (((_lv46_z_re * _lv46_z_re) - (_lv47_z_im * _lv47_z_im)) + v_c_re__44);
    let _rt51: f32 = ((f32(2.0) * (_lv46_z_re * _lv47_z_im)) + v_c_im__45);
    let _rt52: i32 = (_lv48_it + i32((1i)));
    _lv46_z_re = _rt50;
    _lv47_z_im = _rt51;
    _lv48_it = _rt52;
    continue;
    }
    }
    }
    let v_s3v_53: f32 = _lr49;
    let v_c_re__54: f32 = (v_base_re_22 + (v_px_size_18 * (f32(0.3125) * v_aspect_12)));
    let v_c_im__55: f32 = (v_base_im_23 + (v_py_size_19 * f32(-0.1875)));
    var _lv56_z_re: f32 = v_c_re__54;
    var _lv57_z_im: f32 = v_c_im__55;
    var _lv58_it: i32 = i32((0i));
    var _lr59: f32 = 0.0;
    loop {
    if ((_lv58_it >= v_max_iter_16)) {
    _lr59 = v_max_iter_f_17;
    break;
    } else {
    if ((((_lv56_z_re * _lv56_z_re) + (_lv57_z_im * _lv57_z_im)) >= f32(4.0))) {
    _lr59 = (f32(_lv58_it) + (f32(1.0) - (log(log(((_lv56_z_re * _lv56_z_re) + (_lv57_z_im * _lv57_z_im)))) / f32(0.6931))));
    break;
    } else {
    let _rt60: f32 = (((_lv56_z_re * _lv56_z_re) - (_lv57_z_im * _lv57_z_im)) + v_c_re__54);
    let _rt61: f32 = ((f32(2.0) * (_lv56_z_re * _lv57_z_im)) + v_c_im__55);
    let _rt62: i32 = (_lv58_it + i32((1i)));
    _lv56_z_re = _rt60;
    _lv57_z_im = _rt61;
    _lv58_it = _rt62;
    continue;
    }
    }
    }
    let v_s4v_63: f32 = _lr59;
    let v_c_re__64: f32 = (v_base_re_22 + (v_px_size_18 * (f32(0.4375) * v_aspect_12)));
    let v_c_im__65: f32 = (v_base_im_23 + (v_py_size_19 * f32(0.0625)));
    var _lv66_z_re: f32 = v_c_re__64;
    var _lv67_z_im: f32 = v_c_im__65;
    var _lv68_it: i32 = i32((0i));
    var _lr69: f32 = 0.0;
    loop {
    if ((_lv68_it >= v_max_iter_16)) {
    _lr69 = v_max_iter_f_17;
    break;
    } else {
    if ((((_lv66_z_re * _lv66_z_re) + (_lv67_z_im * _lv67_z_im)) >= f32(4.0))) {
    _lr69 = (f32(_lv68_it) + (f32(1.0) - (log(log(((_lv66_z_re * _lv66_z_re) + (_lv67_z_im * _lv67_z_im)))) / f32(0.6931))));
    break;
    } else {
    let _rt70: f32 = (((_lv66_z_re * _lv66_z_re) - (_lv67_z_im * _lv67_z_im)) + v_c_re__64);
    let _rt71: f32 = ((f32(2.0) * (_lv66_z_re * _lv67_z_im)) + v_c_im__65);
    let _rt72: i32 = (_lv68_it + i32((1i)));
    _lv66_z_re = _rt70;
    _lv67_z_im = _rt71;
    _lv68_it = _rt72;
    continue;
    }
    }
    }
    let v_s5v_73: f32 = _lr69;
    let v_c_re__74: f32 = (v_base_re_22 + (v_px_size_18 * (f32(0.1875) * v_aspect_12)));
    let v_c_im__75: f32 = (v_base_im_23 + (v_py_size_19 * f32(0.3125)));
    var _lv76_z_re: f32 = v_c_re__74;
    var _lv77_z_im: f32 = v_c_im__75;
    var _lv78_it: i32 = i32((0i));
    var _lr79: f32 = 0.0;
    loop {
    if ((_lv78_it >= v_max_iter_16)) {
    _lr79 = v_max_iter_f_17;
    break;
    } else {
    if ((((_lv76_z_re * _lv76_z_re) + (_lv77_z_im * _lv77_z_im)) >= f32(4.0))) {
    _lr79 = (f32(_lv78_it) + (f32(1.0) - (log(log(((_lv76_z_re * _lv76_z_re) + (_lv77_z_im * _lv77_z_im)))) / f32(0.6931))));
    break;
    } else {
    let _rt80: f32 = (((_lv76_z_re * _lv76_z_re) - (_lv77_z_im * _lv77_z_im)) + v_c_re__74);
    let _rt81: f32 = ((f32(2.0) * (_lv76_z_re * _lv77_z_im)) + v_c_im__75);
    let _rt82: i32 = (_lv78_it + i32((1i)));
    _lv76_z_re = _rt80;
    _lv77_z_im = _rt81;
    _lv78_it = _rt82;
    continue;
    }
    }
    }
    let v_s6v_83: f32 = _lr79;
    let v_c_re__84: f32 = (v_base_re_22 + (v_px_size_18 * (f32(-0.0625) * v_aspect_12)));
    let v_c_im__85: f32 = (v_base_im_23 + (v_py_size_19 * f32(0.4375)));
    var _lv86_z_re: f32 = v_c_re__84;
    var _lv87_z_im: f32 = v_c_im__85;
    var _lv88_it: i32 = i32((0i));
    var _lr89: f32 = 0.0;
    loop {
    if ((_lv88_it >= v_max_iter_16)) {
    _lr89 = v_max_iter_f_17;
    break;
    } else {
    if ((((_lv86_z_re * _lv86_z_re) + (_lv87_z_im * _lv87_z_im)) >= f32(4.0))) {
    _lr89 = (f32(_lv88_it) + (f32(1.0) - (log(log(((_lv86_z_re * _lv86_z_re) + (_lv87_z_im * _lv87_z_im)))) / f32(0.6931))));
    break;
    } else {
    let _rt90: f32 = (((_lv86_z_re * _lv86_z_re) - (_lv87_z_im * _lv87_z_im)) + v_c_re__84);
    let _rt91: f32 = ((f32(2.0) * (_lv86_z_re * _lv87_z_im)) + v_c_im__85);
    let _rt92: i32 = (_lv88_it + i32((1i)));
    _lv86_z_re = _rt90;
    _lv87_z_im = _rt91;
    _lv88_it = _rt92;
    continue;
    }
    }
    }
    let v_s7v_93: f32 = _lr89;
    let v_c_re__94: f32 = (v_base_re_22 + (v_px_size_18 * (f32(-0.3125) * v_aspect_12)));
    let v_c_im__95: f32 = (v_base_im_23 + (v_py_size_19 * f32(0.1875)));
    var _lv96_z_re: f32 = v_c_re__94;
    var _lv97_z_im: f32 = v_c_im__95;
    var _lv98_it: i32 = i32((0i));
    var _lr99: f32 = 0.0;
    loop {
    if ((_lv98_it >= v_max_iter_16)) {
    _lr99 = v_max_iter_f_17;
    break;
    } else {
    if ((((_lv96_z_re * _lv96_z_re) + (_lv97_z_im * _lv97_z_im)) >= f32(4.0))) {
    _lr99 = (f32(_lv98_it) + (f32(1.0) - (log(log(((_lv96_z_re * _lv96_z_re) + (_lv97_z_im * _lv97_z_im)))) / f32(0.6931))));
    break;
    } else {
    let _rt100: f32 = (((_lv96_z_re * _lv96_z_re) - (_lv97_z_im * _lv97_z_im)) + v_c_re__94);
    let _rt101: f32 = ((f32(2.0) * (_lv96_z_re * _lv97_z_im)) + v_c_im__95);
    let _rt102: i32 = (_lv98_it + i32((1i)));
    _lv96_z_re = _rt100;
    _lv97_z_im = _rt101;
    _lv98_it = _rt102;
    continue;
    }
    }
    }
    let v_s8v_103: f32 = _lr99;
    let v_avg_104: f32 = (f32(0.125) * (((v_s1v_33 + v_s2v_43) + (v_s3v_53 + v_s4v_63)) + ((v_s5v_73 + v_s6v_83) + (v_s7v_93 + v_s8v_103))));
    let v_frac_in_105: f32 = (f32(0.125) * (((select(f32(0.0), f32(1.0), (v_s1v_33 >= v_max_iter_f_17)) + select(f32(0.0), f32(1.0), (v_s2v_43 >= v_max_iter_f_17))) + (select(f32(0.0), f32(1.0), (v_s3v_53 >= v_max_iter_f_17)) + select(f32(0.0), f32(1.0), (v_s4v_63 >= v_max_iter_f_17)))) + ((select(f32(0.0), f32(1.0), (v_s5v_73 >= v_max_iter_f_17)) + select(f32(0.0), f32(1.0), (v_s6v_83 >= v_max_iter_f_17))) + (select(f32(0.0), f32(1.0), (v_s7v_93 >= v_max_iter_f_17)) + select(f32(0.0), f32(1.0), (v_s8v_103 >= v_max_iter_f_17))))));
    let v_cval_106: f32 = sqrt((v_avg_104 / v_max_iter_f_17));
    let v_keep_107: f32 = (f32(1.0) - v_frac_in_105);
    let v_r_108: f32 = (v_keep_107 * (f32(0.5) * (f32(1.0) + sin(((v_cval_106 * f32(12.0)) + v_shift_11)))));
    let v_g_109: f32 = (v_keep_107 * (f32(0.5) * (f32(1.0) + sin(((v_cval_106 * f32(12.0)) + (v_shift_11 + f32(2.1)))))));
    let v_b_110: f32 = (v_keep_107 * (f32(0.5) * (f32(1.0) + sin(((v_cval_106 * f32(12.0)) + (v_shift_11 + f32(4.2)))))));
    let v_ri_111: u32 = u32(min(i32((255i)), max(i32((0i)), i32((f32(255.0) * v_r_108)))));
    let v_gi_112: u32 = u32(min(i32((255i)), max(i32((0i)), i32((f32(255.0) * v_g_109)))));
    let v_bi_113: u32 = u32(min(i32((255i)), max(i32((0i)), i32((f32(255.0) * v_b_110)))));
    dst[gid.y * params.width + gid.x] = (((v_ri_111 << u32((16i))) | (v_gi_112 << u32((8i)))) | v_bi_113);
}
