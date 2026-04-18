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
    let v_t_0: f32 = (f32(0.001) * f32(params.t_ms));
    let v_orbit_s_1: f32 = (f32(1.5) * (f32(params.s0) / f32(1000.0)));
    let v_orbit_r_2: f32 = (f32(2.5) + (f32(4.5) * (f32(params.s1) / f32(1000.0))));
    let v_sun_az_3: f32 = (f32(6.2831853) * (f32(params.s2) / f32(1000.0)));
    let v_blend_k_4: f32 = (f32(0.05) + (f32(1.15) * (f32(params.s3) / f32(1000.0))));
    let v_aspect_5: f32 = (f32(i32(params.width)) / f32(i32(params.height)));
    let v_uv_x_6: f32 = (v_aspect_5 * ((f32(2.0) * (f32(k_x) / f32(i32(params.width)))) - f32(1.0)));
    let v_uv_y_7: f32 = (f32(1.0) - (f32(2.0) * (f32(k_y) / f32(i32(params.height)))));
    let v_ca_8: f32 = (v_t_0 * v_orbit_s_1);
    let v_cam_x_9: f32 = (v_orbit_r_2 * sin(v_ca_8));
    let v_cam_y_10: f32 = f32(1.2);
    let v_cam_z_11: f32 = (v_orbit_r_2 * cos(v_ca_8));
    let v_tgt_x_12: f32 = f32(0.0);
    let v_tgt_y_13: f32 = f32(0.6);
    let v_tgt_z_14: f32 = f32(0.0);
    let v_lx_15: f32 = (v_tgt_x_12 - v_cam_x_9);
    let v_ly_16: f32 = (v_tgt_y_13 - v_cam_y_10);
    let v_lz_17: f32 = (v_tgt_z_14 - v_cam_z_11);
    let v_ll_18: f32 = sqrt((((v_lx_15 * v_lx_15) + (v_ly_16 * v_ly_16)) + (v_lz_17 * v_lz_17)));
    let v_fx_19: f32 = (v_lx_15 / v_ll_18);
    let v_fy_20: f32 = (v_ly_16 / v_ll_18);
    let v_fz_21: f32 = (v_lz_17 / v_ll_18);
    let v_rx0_22: f32 = v_fz_21;
    let v_ry0_23: f32 = f32(0.0);
    let v_rz0_24: f32 = (-v_fx_19);
    let v_rl_25: f32 = max(f32(0.0001), sqrt((((v_rx0_22 * v_rx0_22) + (v_ry0_23 * v_ry0_23)) + (v_rz0_24 * v_rz0_24))));
    let v_rx_26: f32 = (v_rx0_22 / v_rl_25);
    let v_ry_27: f32 = (v_ry0_23 / v_rl_25);
    let v_rz_28: f32 = (v_rz0_24 / v_rl_25);
    let v_ux_29: f32 = ((v_fy_20 * v_rz_28) - (v_fz_21 * v_ry_27));
    let v_uy_30: f32 = ((v_fz_21 * v_rx_26) - (v_fx_19 * v_rz_28));
    let v_uz_31: f32 = ((v_fx_19 * v_ry_27) - (v_fy_20 * v_rx_26));
    let v_fov_32: f32 = f32(1.4);
    let v_dx0_33: f32 = (((v_fx_19 * v_fov_32) + (v_rx_26 * v_uv_x_6)) + (v_ux_29 * v_uv_y_7));
    let v_dy0_34: f32 = (((v_fy_20 * v_fov_32) + (v_ry_27 * v_uv_x_6)) + (v_uy_30 * v_uv_y_7));
    let v_dz0_35: f32 = (((v_fz_21 * v_fov_32) + (v_rz_28 * v_uv_x_6)) + (v_uz_31 * v_uv_y_7));
    let v_dl_36: f32 = sqrt((((v_dx0_33 * v_dx0_33) + (v_dy0_34 * v_dy0_34)) + (v_dz0_35 * v_dz0_35)));
    let v_dx_37: f32 = (v_dx0_33 / v_dl_36);
    let v_dy_38: f32 = (v_dy0_34 / v_dl_36);
    let v_dz_39: f32 = (v_dz0_35 / v_dl_36);
    let v_sel_40: f32 = f32(0.9);
    let v_sx0_41: f32 = (cos(v_sel_40) * cos(v_sun_az_3));
    let v_sz0_42: f32 = (cos(v_sel_40) * sin(v_sun_az_3));
    let v_sy0_43: f32 = sin(v_sel_40);
    let v_sx_44: f32 = v_sx0_41;
    let v_sy_45: f32 = v_sy0_43;
    let v_sz_46: f32 = v_sz0_42;
    let v_max_steps_47: i32 = i32((96i));
    let v_max_dist_48: f32 = f32(40.0);
    let v_hit_eps_49: f32 = f32(0.001);
    var _lv50_tt: f32 = f32(0.0);
    var _lv51_steps: i32 = i32((0i));
    var _lr52: f32 = 0.0;
    loop {
    if ((_lv51_steps >= v_max_steps_47)) {
    _lr52 = v_max_dist_48;
    break;
    } else {
    if ((_lv50_tt >= v_max_dist_48)) {
    _lr52 = v_max_dist_48;
    break;
    } else {
    let v_ax_53: f32 = ((v_cam_x_9 + (v_dx_37 * _lv50_tt)) - f32(0.0));
    let v_ay_54: f32 = ((v_cam_y_10 + (v_dy_38 * _lv50_tt)) - (f32(0.55) + (f32(0.25) * sin((v_t_0 * f32(1.1))))));
    let v_az_55: f32 = ((v_cam_z_11 + (v_dz_39 * _lv50_tt)) - f32(0.0));
    let v_da_56: f32 = (sqrt((((v_ax_53 * v_ax_53) + (v_ay_54 * v_ay_54)) + (v_az_55 * v_az_55))) - f32(0.55));
    let v_bx_57: f32 = ((v_cam_x_9 + (v_dx_37 * _lv50_tt)) - (f32(0.9) * cos((v_t_0 * f32(0.7)))));
    let v_by_58: f32 = ((v_cam_y_10 + (v_dy_38 * _lv50_tt)) - (f32(0.55) + (f32(0.15) * cos((v_t_0 * f32(1.3))))));
    let v_bz_59: f32 = ((v_cam_z_11 + (v_dz_39 * _lv50_tt)) - (f32(0.9) * sin((v_t_0 * f32(0.7)))));
    let v_db_60: f32 = (sqrt((((v_bx_57 * v_bx_57) + (v_by_58 * v_by_58)) + (v_bz_59 * v_bz_59))) - f32(0.45));
    let v_hh_61: f32 = max(f32(0.0), (f32(1.0) - (abs((v_da_56 - v_db_60)) / max(f32(0.0001), v_blend_k_4))));
    let v_smn_62: f32 = (min(v_da_56, v_db_60) - ((v_blend_k_4 * v_hh_61) * (v_hh_61 * (v_hh_61 * (f32(1.0) / f32(6.0))))));
    let v_df_63: f32 = (v_cam_y_10 + (v_dy_38 * _lv50_tt));
    if ((min(v_smn_62, v_df_63) < v_hit_eps_49)) {
    _lr52 = _lv50_tt;
    break;
    } else {
    let _rt64: f32 = (_lv50_tt + max(v_hit_eps_49, min(v_smn_62, v_df_63)));
    let _rt65: i32 = (_lv51_steps + i32((1i)));
    _lv50_tt = _rt64;
    _lv51_steps = _rt65;
    continue;
    }
    }
    }
    }
    let v_hit_t_66: f32 = _lr52;
    let v_hit__67: bool = (v_hit_t_66 < (v_max_dist_48 - f32(0.1)));
    let v_hx_68: f32 = (v_cam_x_9 + (v_dx_37 * v_hit_t_66));
    let v_hy_69: f32 = (v_cam_y_10 + (v_dy_38 * v_hit_t_66));
    let v_hz_70: f32 = (v_cam_z_11 + (v_dz_39 * v_hit_t_66));
    let v_ne_71: f32 = f32(0.0015);
    let v_ax_72: f32 = ((v_hx_68 + v_ne_71) - f32(0.0));
    let v_ay_73: f32 = (v_hy_69 - (f32(0.55) + (f32(0.25) * sin((v_t_0 * f32(1.1))))));
    let v_az_74: f32 = (v_hz_70 - f32(0.0));
    let v_da_75: f32 = (sqrt((((v_ax_72 * v_ax_72) + (v_ay_73 * v_ay_73)) + (v_az_74 * v_az_74))) - f32(0.55));
    let v_bx_76: f32 = ((v_hx_68 + v_ne_71) - (f32(0.9) * cos((v_t_0 * f32(0.7)))));
    let v_by_77: f32 = (v_hy_69 - (f32(0.55) + (f32(0.15) * cos((v_t_0 * f32(1.3))))));
    let v_bz_78: f32 = (v_hz_70 - (f32(0.9) * sin((v_t_0 * f32(0.7)))));
    let v_db_79: f32 = (sqrt((((v_bx_76 * v_bx_76) + (v_by_77 * v_by_77)) + (v_bz_78 * v_bz_78))) - f32(0.45));
    let v_hh_80: f32 = max(f32(0.0), (f32(1.0) - (abs((v_da_75 - v_db_79)) / max(f32(0.0001), v_blend_k_4))));
    let v_smn_81: f32 = (min(v_da_75, v_db_79) - ((v_blend_k_4 * v_hh_80) * (v_hh_80 * (v_hh_80 * (f32(1.0) / f32(6.0))))));
    let v_df_82: f32 = v_hy_69;
    let v_ax_83: f32 = ((v_hx_68 - v_ne_71) - f32(0.0));
    let v_ay_84: f32 = (v_hy_69 - (f32(0.55) + (f32(0.25) * sin((v_t_0 * f32(1.1))))));
    let v_az_85: f32 = (v_hz_70 - f32(0.0));
    let v_da_86: f32 = (sqrt((((v_ax_83 * v_ax_83) + (v_ay_84 * v_ay_84)) + (v_az_85 * v_az_85))) - f32(0.55));
    let v_bx_87: f32 = ((v_hx_68 - v_ne_71) - (f32(0.9) * cos((v_t_0 * f32(0.7)))));
    let v_by_88: f32 = (v_hy_69 - (f32(0.55) + (f32(0.15) * cos((v_t_0 * f32(1.3))))));
    let v_bz_89: f32 = (v_hz_70 - (f32(0.9) * sin((v_t_0 * f32(0.7)))));
    let v_db_90: f32 = (sqrt((((v_bx_87 * v_bx_87) + (v_by_88 * v_by_88)) + (v_bz_89 * v_bz_89))) - f32(0.45));
    let v_hh_91: f32 = max(f32(0.0), (f32(1.0) - (abs((v_da_86 - v_db_90)) / max(f32(0.0001), v_blend_k_4))));
    let v_smn_92: f32 = (min(v_da_86, v_db_90) - ((v_blend_k_4 * v_hh_91) * (v_hh_91 * (v_hh_91 * (f32(1.0) / f32(6.0))))));
    let v_df_93: f32 = v_hy_69;
    let v_nx0_94: f32 = (min(v_smn_81, v_df_82) - min(v_smn_92, v_df_93));
    let v_ax_95: f32 = (v_hx_68 - f32(0.0));
    let v_ay_96: f32 = ((v_hy_69 + v_ne_71) - (f32(0.55) + (f32(0.25) * sin((v_t_0 * f32(1.1))))));
    let v_az_97: f32 = (v_hz_70 - f32(0.0));
    let v_da_98: f32 = (sqrt((((v_ax_95 * v_ax_95) + (v_ay_96 * v_ay_96)) + (v_az_97 * v_az_97))) - f32(0.55));
    let v_bx_99: f32 = (v_hx_68 - (f32(0.9) * cos((v_t_0 * f32(0.7)))));
    let v_by_100: f32 = ((v_hy_69 + v_ne_71) - (f32(0.55) + (f32(0.15) * cos((v_t_0 * f32(1.3))))));
    let v_bz_101: f32 = (v_hz_70 - (f32(0.9) * sin((v_t_0 * f32(0.7)))));
    let v_db_102: f32 = (sqrt((((v_bx_99 * v_bx_99) + (v_by_100 * v_by_100)) + (v_bz_101 * v_bz_101))) - f32(0.45));
    let v_hh_103: f32 = max(f32(0.0), (f32(1.0) - (abs((v_da_98 - v_db_102)) / max(f32(0.0001), v_blend_k_4))));
    let v_smn_104: f32 = (min(v_da_98, v_db_102) - ((v_blend_k_4 * v_hh_103) * (v_hh_103 * (v_hh_103 * (f32(1.0) / f32(6.0))))));
    let v_df_105: f32 = (v_hy_69 + v_ne_71);
    let v_ax_106: f32 = (v_hx_68 - f32(0.0));
    let v_ay_107: f32 = ((v_hy_69 - v_ne_71) - (f32(0.55) + (f32(0.25) * sin((v_t_0 * f32(1.1))))));
    let v_az_108: f32 = (v_hz_70 - f32(0.0));
    let v_da_109: f32 = (sqrt((((v_ax_106 * v_ax_106) + (v_ay_107 * v_ay_107)) + (v_az_108 * v_az_108))) - f32(0.55));
    let v_bx_110: f32 = (v_hx_68 - (f32(0.9) * cos((v_t_0 * f32(0.7)))));
    let v_by_111: f32 = ((v_hy_69 - v_ne_71) - (f32(0.55) + (f32(0.15) * cos((v_t_0 * f32(1.3))))));
    let v_bz_112: f32 = (v_hz_70 - (f32(0.9) * sin((v_t_0 * f32(0.7)))));
    let v_db_113: f32 = (sqrt((((v_bx_110 * v_bx_110) + (v_by_111 * v_by_111)) + (v_bz_112 * v_bz_112))) - f32(0.45));
    let v_hh_114: f32 = max(f32(0.0), (f32(1.0) - (abs((v_da_109 - v_db_113)) / max(f32(0.0001), v_blend_k_4))));
    let v_smn_115: f32 = (min(v_da_109, v_db_113) - ((v_blend_k_4 * v_hh_114) * (v_hh_114 * (v_hh_114 * (f32(1.0) / f32(6.0))))));
    let v_df_116: f32 = (v_hy_69 - v_ne_71);
    let v_ny0_117: f32 = (min(v_smn_104, v_df_105) - min(v_smn_115, v_df_116));
    let v_ax_118: f32 = (v_hx_68 - f32(0.0));
    let v_ay_119: f32 = (v_hy_69 - (f32(0.55) + (f32(0.25) * sin((v_t_0 * f32(1.1))))));
    let v_az_120: f32 = ((v_hz_70 + v_ne_71) - f32(0.0));
    let v_da_121: f32 = (sqrt((((v_ax_118 * v_ax_118) + (v_ay_119 * v_ay_119)) + (v_az_120 * v_az_120))) - f32(0.55));
    let v_bx_122: f32 = (v_hx_68 - (f32(0.9) * cos((v_t_0 * f32(0.7)))));
    let v_by_123: f32 = (v_hy_69 - (f32(0.55) + (f32(0.15) * cos((v_t_0 * f32(1.3))))));
    let v_bz_124: f32 = ((v_hz_70 + v_ne_71) - (f32(0.9) * sin((v_t_0 * f32(0.7)))));
    let v_db_125: f32 = (sqrt((((v_bx_122 * v_bx_122) + (v_by_123 * v_by_123)) + (v_bz_124 * v_bz_124))) - f32(0.45));
    let v_hh_126: f32 = max(f32(0.0), (f32(1.0) - (abs((v_da_121 - v_db_125)) / max(f32(0.0001), v_blend_k_4))));
    let v_smn_127: f32 = (min(v_da_121, v_db_125) - ((v_blend_k_4 * v_hh_126) * (v_hh_126 * (v_hh_126 * (f32(1.0) / f32(6.0))))));
    let v_df_128: f32 = v_hy_69;
    let v_ax_129: f32 = (v_hx_68 - f32(0.0));
    let v_ay_130: f32 = (v_hy_69 - (f32(0.55) + (f32(0.25) * sin((v_t_0 * f32(1.1))))));
    let v_az_131: f32 = ((v_hz_70 - v_ne_71) - f32(0.0));
    let v_da_132: f32 = (sqrt((((v_ax_129 * v_ax_129) + (v_ay_130 * v_ay_130)) + (v_az_131 * v_az_131))) - f32(0.55));
    let v_bx_133: f32 = (v_hx_68 - (f32(0.9) * cos((v_t_0 * f32(0.7)))));
    let v_by_134: f32 = (v_hy_69 - (f32(0.55) + (f32(0.15) * cos((v_t_0 * f32(1.3))))));
    let v_bz_135: f32 = ((v_hz_70 - v_ne_71) - (f32(0.9) * sin((v_t_0 * f32(0.7)))));
    let v_db_136: f32 = (sqrt((((v_bx_133 * v_bx_133) + (v_by_134 * v_by_134)) + (v_bz_135 * v_bz_135))) - f32(0.45));
    let v_hh_137: f32 = max(f32(0.0), (f32(1.0) - (abs((v_da_132 - v_db_136)) / max(f32(0.0001), v_blend_k_4))));
    let v_smn_138: f32 = (min(v_da_132, v_db_136) - ((v_blend_k_4 * v_hh_137) * (v_hh_137 * (v_hh_137 * (f32(1.0) / f32(6.0))))));
    let v_df_139: f32 = v_hy_69;
    let v_nz0_140: f32 = (min(v_smn_127, v_df_128) - min(v_smn_138, v_df_139));
    let v_nl_141: f32 = max(f32(0.0001), sqrt((((v_nx0_94 * v_nx0_94) + (v_ny0_117 * v_ny0_117)) + (v_nz0_140 * v_nz0_140))));
    let v_nx_142: f32 = (v_nx0_94 / v_nl_141);
    let v_ny_143: f32 = (v_ny0_117 / v_nl_141);
    let v_nz_144: f32 = (v_nz0_140 / v_nl_141);
    let v_eps_145: f32 = f32(0.02);
    let v_sox_146: f32 = (v_hx_68 + (v_nx_142 * v_eps_145));
    let v_soy_147: f32 = (v_hy_69 + (v_ny_143 * v_eps_145));
    let v_soz_148: f32 = (v_hz_70 + (v_nz_144 * v_eps_145));
    var _lv149_tt: f32 = f32(0.05);
    var _lv150_mn: f32 = f32(1.0);
    var _lv151_steps: i32 = i32((0i));
    var _lr152: f32 = 0.0;
    loop {
    if ((_lv151_steps >= i32((32i)))) {
    _lr152 = _lv150_mn;
    break;
    } else {
    if ((_lv149_tt >= f32(8.0))) {
    _lr152 = _lv150_mn;
    break;
    } else {
    let v_ax_153: f32 = ((v_sox_146 + (v_sx_44 * _lv149_tt)) - f32(0.0));
    let v_ay_154: f32 = ((v_soy_147 + (v_sy_45 * _lv149_tt)) - (f32(0.55) + (f32(0.25) * sin((v_t_0 * f32(1.1))))));
    let v_az_155: f32 = ((v_soz_148 + (v_sz_46 * _lv149_tt)) - f32(0.0));
    let v_da_156: f32 = (sqrt((((v_ax_153 * v_ax_153) + (v_ay_154 * v_ay_154)) + (v_az_155 * v_az_155))) - f32(0.55));
    let v_bx_157: f32 = ((v_sox_146 + (v_sx_44 * _lv149_tt)) - (f32(0.9) * cos((v_t_0 * f32(0.7)))));
    let v_by_158: f32 = ((v_soy_147 + (v_sy_45 * _lv149_tt)) - (f32(0.55) + (f32(0.15) * cos((v_t_0 * f32(1.3))))));
    let v_bz_159: f32 = ((v_soz_148 + (v_sz_46 * _lv149_tt)) - (f32(0.9) * sin((v_t_0 * f32(0.7)))));
    let v_db_160: f32 = (sqrt((((v_bx_157 * v_bx_157) + (v_by_158 * v_by_158)) + (v_bz_159 * v_bz_159))) - f32(0.45));
    let v_hh_161: f32 = max(f32(0.0), (f32(1.0) - (abs((v_da_156 - v_db_160)) / max(f32(0.0001), v_blend_k_4))));
    let v_smn_162: f32 = (min(v_da_156, v_db_160) - ((v_blend_k_4 * v_hh_161) * (v_hh_161 * (v_hh_161 * (f32(1.0) / f32(6.0))))));
    let v_df_163: f32 = (v_soy_147 + (v_sy_45 * _lv149_tt));
    if ((min(v_smn_162, v_df_163) < f32(0.001))) {
    _lr152 = f32(0.0);
    break;
    } else {
    let _rt164: f32 = (_lv149_tt + max(f32(0.005), min(v_smn_162, v_df_163)));
    let _rt165: f32 = min(_lv150_mn, ((f32(12.0) * min(v_smn_162, v_df_163)) / _lv149_tt));
    let _rt166: i32 = (_lv151_steps + i32((1i)));
    _lv149_tt = _rt164;
    _lv150_mn = _rt165;
    _lv151_steps = _rt166;
    continue;
    }
    }
    }
    }
    let v_soft_167: f32 = _lr152;
    let v_shadow_168: f32 = max(f32(0.0), min(f32(1.0), v_soft_167));
    let v_ndotl_169: f32 = max(f32(0.0), (((v_nx_142 * v_sx_44) + (v_ny_143 * v_sy_45)) + (v_nz_144 * v_sz_46)));
    let v_vx_170: f32 = (-v_dx_37);
    let v_vy_171: f32 = (-v_dy_38);
    let v_vz_172: f32 = (-v_dz_39);
    let v_hhx_173: f32 = (v_sx_44 + v_vx_170);
    let v_hhy_174: f32 = (v_sy_45 + v_vy_171);
    let v_hhz_175: f32 = (v_sz_46 + v_vz_172);
    let v_hl_176: f32 = max(f32(0.0001), sqrt((((v_hhx_173 * v_hhx_173) + (v_hhy_174 * v_hhy_174)) + (v_hhz_175 * v_hhz_175))));
    let v_hhx_177: f32 = (v_hhx_173 / v_hl_176);
    let v_hhy_178: f32 = (v_hhy_174 / v_hl_176);
    let v_hhz_179: f32 = (v_hhz_175 / v_hl_176);
    let v_ndoth_180: f32 = max(f32(0.0), (((v_nx_142 * v_hhx_177) + (v_ny_143 * v_hhy_178)) + (v_nz_144 * v_hhz_179)));
    let v_spec_181: f32 = pow(v_ndoth_180, f32(48.0));
    let v_on_floor__182: bool = (abs(v_hy_69) < f32(0.02));
    let v_cxi_183: i32 = i32(floor((v_hx_68 * f32(0.5))));
    let v_czi_184: i32 = i32(floor((v_hz_70 * f32(0.5))));
    let v_chk_185: f32 = select(f32(0.22), f32(0.82), (((v_cxi_183 + v_czi_184) % i32((2i))) == i32((0i))));
    let v_ar_186: f32 = select(f32(0.95), v_chk_185, v_on_floor__182);
    let v_ag_187: f32 = select(f32(0.55), v_chk_185, v_on_floor__182);
    let v_ab_188: f32 = select(f32(0.32), v_chk_185, v_on_floor__182);
    let v_amb_189: f32 = f32(0.18);
    let v_lit_190: f32 = (v_shadow_168 * v_ndotl_169);
    let v_rr_191: f32 = ((v_amb_189 * v_ar_186) + (((f32(0.9) * v_lit_190) * v_ar_186) + ((f32(0.55) * v_shadow_168) * v_spec_181)));
    let v_gg_192: f32 = ((v_amb_189 * v_ag_187) + (((f32(0.9) * v_lit_190) * v_ag_187) + ((f32(0.55) * v_shadow_168) * v_spec_181)));
    let v_bb_193: f32 = ((v_amb_189 * v_ab_188) + (((f32(0.9) * v_lit_190) * v_ab_188) + ((f32(0.55) * v_shadow_168) * v_spec_181)));
    let v_skyt_194: f32 = max(f32(0.0), min(f32(1.0), (f32(0.5) * (f32(1.0) + v_dy_38))));
    let v_skr_195: f32 = (((f32(1.0) - v_skyt_194) * f32(0.55)) + (v_skyt_194 * f32(0.18)));
    let v_skg_196: f32 = (((f32(1.0) - v_skyt_194) * f32(0.72)) + (v_skyt_194 * f32(0.35)));
    let v_skb_197: f32 = (((f32(1.0) - v_skyt_194) * f32(0.95)) + (v_skyt_194 * f32(0.65)));
    let v_out_r_198: f32 = select(v_skr_195, v_rr_191, v_hit__67);
    let v_out_g_199: f32 = select(v_skg_196, v_gg_192, v_hit__67);
    let v_out_b_200: f32 = select(v_skb_197, v_bb_193, v_hit__67);
    let v_ri_201: u32 = u32(min(i32((255i)), max(i32((0i)), i32((f32(255.0) * v_out_r_198)))));
    let v_gi_202: u32 = u32(min(i32((255i)), max(i32((0i)), i32((f32(255.0) * v_out_g_199)))));
    let v_bi_203: u32 = u32(min(i32((255i)), max(i32((0i)), i32((f32(255.0) * v_out_b_200)))));
    dst[gid.y * params.width + gid.x] = (((v_ri_201 << u32((16i))) | (v_gi_202 << u32((8i)))) | v_bi_203);
}
