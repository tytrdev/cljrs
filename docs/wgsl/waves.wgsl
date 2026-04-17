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
    let _s0: f32 = (f32(params.s1) / f32(1000.0));
    let _s1: f32 = (f32(19.5) * _s0);
    let _s2: f32 = (f32(0.5) + _s1);
    let _s3: f32 = _s2;
    let _s4: f32 = (f32(params.s2) / f32(1000.0));
    let _s5: f32 = (f32(4.0) * _s4);
    let _s6: f32 = _s5;
    let _s7: f32 = (f32(params.s3) / f32(1000.0));
    let _s8: f32 = (f32(6.2831853) * _s7);
    let _s9: f32 = _s8;
    let _s10: f32 = (f32(0.001) * f32(params.t_ms));
    let _s11: f32 = _s10;
    let _s12: f32 = (f32(i32(params.width)) / f32(i32(params.height)));
    let _s13: f32 = _s12;
    let _s14: f32 = (f32(k_x) / f32(i32(params.width)));
    let _s15: f32 = (f32(2.0) * _s14);
    let _s16: f32 = (_s15 - f32(1.0));
    let _s17: f32 = (_s13 * _s16);
    let _s18: f32 = _s17;
    let _s19: f32 = (f32(k_y) / f32(i32(params.height)));
    let _s20: f32 = (f32(2.0) * _s19);
    let _s21: f32 = (_s20 - f32(1.0));
    let _s22: f32 = _s21;
    let _s23: f32 = f32(0.0);
    let _s24: f32 = f32(0.0);
    let _s25: f32 = f32(0.7);
    let _s26: f32 = f32(0.5);
    let _s27: f32 = (-f32(0.6));
    let _s28: f32 = (-f32(0.7));
    let _s29: f32 = f32(0.3);
    let _s30: f32 = (-f32(0.3));
    let _s31: f32 = (_s18 - _s23);
    let _s32: f32 = (_s18 - _s23);
    let _s33: f32 = (_s31 * _s32);
    let _s34: f32 = (_s22 - _s24);
    let _s35: f32 = (_s22 - _s24);
    let _s36: f32 = (_s34 * _s35);
    let _s37: f32 = (_s33 + _s36);
    let _s38: f32 = sqrt(_s37);
    let _s39: f32 = (_s18 - _s25);
    let _s40: f32 = (_s18 - _s25);
    let _s41: f32 = (_s39 * _s40);
    let _s42: f32 = (_s22 - _s26);
    let _s43: f32 = (_s22 - _s26);
    let _s44: f32 = (_s42 * _s43);
    let _s45: f32 = (_s41 + _s44);
    let _s46: f32 = sqrt(_s45);
    let _s47: f32 = (_s18 - _s27);
    let _s48: f32 = (_s18 - _s27);
    let _s49: f32 = (_s47 * _s48);
    let _s50: f32 = (_s22 - _s28);
    let _s51: f32 = (_s22 - _s28);
    let _s52: f32 = (_s50 * _s51);
    let _s53: f32 = (_s49 + _s52);
    let _s54: f32 = sqrt(_s53);
    let _s55: f32 = (_s18 - _s29);
    let _s56: f32 = (_s18 - _s29);
    let _s57: f32 = (_s55 * _s56);
    let _s58: f32 = (_s22 - _s30);
    let _s59: f32 = (_s22 - _s30);
    let _s60: f32 = (_s58 * _s59);
    let _s61: f32 = (_s57 + _s60);
    let _s62: f32 = sqrt(_s61);
    let _s63: f32 = (_s3 * _s38);
    let _s64: f32 = (_s6 * _s11);
    let _s65: f32 = (_s63 - _s64);
    let _s66: f32 = sin(_s65);
    let _s67: f32 = (_s3 * _s46);
    let _s68: f32 = (_s6 * _s11);
    let _s69: f32 = (_s67 - _s68);
    let _s70: f32 = sin(_s69);
    let _s71: f32 = (_s3 * _s54);
    let _s72: f32 = (_s6 * _s11);
    let _s73: f32 = (_s71 - _s72);
    let _s74: f32 = sin(_s73);
    let _s75: f32 = (_s3 * _s62);
    let _s76: f32 = (_s6 * _s11);
    let _s77: f32 = (_s75 - _s76);
    let _s78: f32 = sin(_s77);
    let _s79: f32 = (_s66 + _s70);
    let _s80: f32 = (_s74 + _s78);
    let _s81: f32 = (_s79 + _s80);
    let _s82: f32 = _s81;
    let _s83: f32 = (f32(0.25) * _s82);
    let _s84: f32 = (f32(1.0) + _s83);
    let _s85: f32 = (f32(0.5) * _s84);
    let _s86: f32 = _s85;
    let _s87: f32 = (f32(6.28) * _s86);
    let _s88: f32 = (_s9 + _s87);
    let _s89: f32 = (f32(1.0) + sin(_s88));
    let _s90: f32 = (f32(0.5) * _s89);
    let _s91: f32 = _s90;
    let _s92: f32 = (_s9 + f32(2.0));
    let _s93: f32 = (f32(6.28) * _s86);
    let _s94: f32 = (_s92 + _s93);
    let _s95: f32 = (f32(1.0) + sin(_s94));
    let _s96: f32 = (f32(0.5) * _s95);
    let _s97: f32 = _s96;
    let _s98: f32 = (_s9 + f32(4.0));
    let _s99: f32 = (f32(6.28) * _s86);
    let _s100: f32 = (_s98 + _s99);
    let _s101: f32 = (f32(1.0) + sin(_s100));
    let _s102: f32 = (f32(0.5) * _s101);
    let _s103: f32 = _s102;
    let _s104: f32 = (f32(255.0) * _s91);
    let _s105: u32 = u32(min(i32((255i)), max(i32((0i)), i32(_s104))));
    let _s106: f32 = (f32(255.0) * _s97);
    let _s107: u32 = u32(min(i32((255i)), max(i32((0i)), i32(_s106))));
    let _s108: f32 = (f32(255.0) * _s103);
    let _s109: u32 = u32(min(i32((255i)), max(i32((0i)), i32(_s108))));
    dst[gid.y * params.width + gid.x] = (((_s105 << u32((16i))) | (_s107 << u32((8i)))) | _s109);
}
