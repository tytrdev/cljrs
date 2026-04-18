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
    let rate: f32 = 0.05 + (0.3 * (f32(params.s0) / 1000.0));
    let t_sec: f32 = 0.001 * f32(params.t_ms);
    let period: f32 = 30.0;
    let t_cycle: f32 = t_sec - (period * floor((t_sec / period)));
    let zoom: f32 = exp((rate * t_cycle));
    let tx: f32 = -0.7436438870371587;
    let ty: f32 = 0.1318259042053119;
    let dx: f32 = 0.001 * ((2.0 * (f32(params.s1) / 1000.0)) - 1.0);
    let dy: f32 = 0.001 * ((2.0 * (f32(params.s2) / 1000.0)) - 1.0);
    let cx: f32 = tx + dx;
    let cy: f32 = ty + dy;
    let shift: f32 = 6.2831853 * (f32(params.s3) / 1000.0);
    let aspect: f32 = f32(i32(params.width)) / f32(i32(params.height));
    let fw: f32 = f32(i32(params.width));
    let fh: f32 = f32(i32(params.height));
    let iter_float: f32 = 100.0 + (40.0 * (log(zoom) / 0.6931));
    let max_iter: i32 = min(2000i, max(64i, i32(iter_float)));
    let max_iter_f: f32 = f32(max_iter);
    let px_size: f32 = 2.0 / (fw * zoom);
    let py_size: f32 = 2.0 / (fh * zoom);
    let u0: f32 = ((2.0 * (f32(k_x) / fw)) - 1.0) / zoom;
    let v0: f32 = ((2.0 * (f32(k_y) / fh)) - 1.0) / zoom;
    let base_re: f32 = cx + (u0 * aspect);
    let base_im: f32 = cy + v0;
    let c_re_: f32 = base_re + (px_size * ((-0.4375) * aspect));
    let c_im_: f32 = base_im + (py_size * (-0.0625));
    var _lv0_z_re: f32 = c_re_;
    var _lv1_z_im: f32 = c_im_;
    var _lv2_it: i32 = 0i;
    var _lr3: f32 = 0.0;
    loop {
    if ((_lv2_it >= max_iter)) {
    _lr3 = max_iter_f;
    break;
    } else {
    if ((((_lv0_z_re * _lv0_z_re) + (_lv1_z_im * _lv1_z_im)) >= 4.0)) {
    _lr3 = (f32(_lv2_it) + (1.0 - (log(log(((_lv0_z_re * _lv0_z_re) + (_lv1_z_im * _lv1_z_im)))) / 0.6931)));
    break;
    } else {
    let _rt4: f32 = (((_lv0_z_re * _lv0_z_re) - (_lv1_z_im * _lv1_z_im)) + c_re_);
    let _rt5: f32 = ((2.0 * (_lv0_z_re * _lv1_z_im)) + c_im_);
    let _rt6: i32 = (_lv2_it + 1i);
    _lv0_z_re = _rt4;
    _lv1_z_im = _rt5;
    _lv2_it = _rt6;
    continue;
    }
    }
    }
    let s1v: f32 = _lr3;
    let c_re__2: f32 = base_re + (px_size * ((-0.1875) * aspect));
    let c_im__2: f32 = base_im + (py_size * (-0.3125));
    var _lv7_z_re: f32 = c_re__2;
    var _lv8_z_im: f32 = c_im__2;
    var _lv9_it: i32 = 0i;
    var _lr10: f32 = 0.0;
    loop {
    if ((_lv9_it >= max_iter)) {
    _lr10 = max_iter_f;
    break;
    } else {
    if ((((_lv7_z_re * _lv7_z_re) + (_lv8_z_im * _lv8_z_im)) >= 4.0)) {
    _lr10 = (f32(_lv9_it) + (1.0 - (log(log(((_lv7_z_re * _lv7_z_re) + (_lv8_z_im * _lv8_z_im)))) / 0.6931)));
    break;
    } else {
    let _rt11: f32 = (((_lv7_z_re * _lv7_z_re) - (_lv8_z_im * _lv8_z_im)) + c_re__2);
    let _rt12: f32 = ((2.0 * (_lv7_z_re * _lv8_z_im)) + c_im__2);
    let _rt13: i32 = (_lv9_it + 1i);
    _lv7_z_re = _rt11;
    _lv8_z_im = _rt12;
    _lv9_it = _rt13;
    continue;
    }
    }
    }
    let s2v: f32 = _lr10;
    let c_re__3: f32 = base_re + (px_size * (0.0625 * aspect));
    let c_im__3: f32 = base_im + (py_size * (-0.4375));
    var _lv14_z_re: f32 = c_re__3;
    var _lv15_z_im: f32 = c_im__3;
    var _lv16_it: i32 = 0i;
    var _lr17: f32 = 0.0;
    loop {
    if ((_lv16_it >= max_iter)) {
    _lr17 = max_iter_f;
    break;
    } else {
    if ((((_lv14_z_re * _lv14_z_re) + (_lv15_z_im * _lv15_z_im)) >= 4.0)) {
    _lr17 = (f32(_lv16_it) + (1.0 - (log(log(((_lv14_z_re * _lv14_z_re) + (_lv15_z_im * _lv15_z_im)))) / 0.6931)));
    break;
    } else {
    let _rt18: f32 = (((_lv14_z_re * _lv14_z_re) - (_lv15_z_im * _lv15_z_im)) + c_re__3);
    let _rt19: f32 = ((2.0 * (_lv14_z_re * _lv15_z_im)) + c_im__3);
    let _rt20: i32 = (_lv16_it + 1i);
    _lv14_z_re = _rt18;
    _lv15_z_im = _rt19;
    _lv16_it = _rt20;
    continue;
    }
    }
    }
    let s3v: f32 = _lr17;
    let c_re__4: f32 = base_re + (px_size * (0.3125 * aspect));
    let c_im__4: f32 = base_im + (py_size * (-0.1875));
    var _lv21_z_re: f32 = c_re__4;
    var _lv22_z_im: f32 = c_im__4;
    var _lv23_it: i32 = 0i;
    var _lr24: f32 = 0.0;
    loop {
    if ((_lv23_it >= max_iter)) {
    _lr24 = max_iter_f;
    break;
    } else {
    if ((((_lv21_z_re * _lv21_z_re) + (_lv22_z_im * _lv22_z_im)) >= 4.0)) {
    _lr24 = (f32(_lv23_it) + (1.0 - (log(log(((_lv21_z_re * _lv21_z_re) + (_lv22_z_im * _lv22_z_im)))) / 0.6931)));
    break;
    } else {
    let _rt25: f32 = (((_lv21_z_re * _lv21_z_re) - (_lv22_z_im * _lv22_z_im)) + c_re__4);
    let _rt26: f32 = ((2.0 * (_lv21_z_re * _lv22_z_im)) + c_im__4);
    let _rt27: i32 = (_lv23_it + 1i);
    _lv21_z_re = _rt25;
    _lv22_z_im = _rt26;
    _lv23_it = _rt27;
    continue;
    }
    }
    }
    let s4v: f32 = _lr24;
    let c_re__5: f32 = base_re + (px_size * (0.4375 * aspect));
    let c_im__5: f32 = base_im + (py_size * 0.0625);
    var _lv28_z_re: f32 = c_re__5;
    var _lv29_z_im: f32 = c_im__5;
    var _lv30_it: i32 = 0i;
    var _lr31: f32 = 0.0;
    loop {
    if ((_lv30_it >= max_iter)) {
    _lr31 = max_iter_f;
    break;
    } else {
    if ((((_lv28_z_re * _lv28_z_re) + (_lv29_z_im * _lv29_z_im)) >= 4.0)) {
    _lr31 = (f32(_lv30_it) + (1.0 - (log(log(((_lv28_z_re * _lv28_z_re) + (_lv29_z_im * _lv29_z_im)))) / 0.6931)));
    break;
    } else {
    let _rt32: f32 = (((_lv28_z_re * _lv28_z_re) - (_lv29_z_im * _lv29_z_im)) + c_re__5);
    let _rt33: f32 = ((2.0 * (_lv28_z_re * _lv29_z_im)) + c_im__5);
    let _rt34: i32 = (_lv30_it + 1i);
    _lv28_z_re = _rt32;
    _lv29_z_im = _rt33;
    _lv30_it = _rt34;
    continue;
    }
    }
    }
    let s5v: f32 = _lr31;
    let c_re__6: f32 = base_re + (px_size * (0.1875 * aspect));
    let c_im__6: f32 = base_im + (py_size * 0.3125);
    var _lv35_z_re: f32 = c_re__6;
    var _lv36_z_im: f32 = c_im__6;
    var _lv37_it: i32 = 0i;
    var _lr38: f32 = 0.0;
    loop {
    if ((_lv37_it >= max_iter)) {
    _lr38 = max_iter_f;
    break;
    } else {
    if ((((_lv35_z_re * _lv35_z_re) + (_lv36_z_im * _lv36_z_im)) >= 4.0)) {
    _lr38 = (f32(_lv37_it) + (1.0 - (log(log(((_lv35_z_re * _lv35_z_re) + (_lv36_z_im * _lv36_z_im)))) / 0.6931)));
    break;
    } else {
    let _rt39: f32 = (((_lv35_z_re * _lv35_z_re) - (_lv36_z_im * _lv36_z_im)) + c_re__6);
    let _rt40: f32 = ((2.0 * (_lv35_z_re * _lv36_z_im)) + c_im__6);
    let _rt41: i32 = (_lv37_it + 1i);
    _lv35_z_re = _rt39;
    _lv36_z_im = _rt40;
    _lv37_it = _rt41;
    continue;
    }
    }
    }
    let s6v: f32 = _lr38;
    let c_re__7: f32 = base_re + (px_size * ((-0.0625) * aspect));
    let c_im__7: f32 = base_im + (py_size * 0.4375);
    var _lv42_z_re: f32 = c_re__7;
    var _lv43_z_im: f32 = c_im__7;
    var _lv44_it: i32 = 0i;
    var _lr45: f32 = 0.0;
    loop {
    if ((_lv44_it >= max_iter)) {
    _lr45 = max_iter_f;
    break;
    } else {
    if ((((_lv42_z_re * _lv42_z_re) + (_lv43_z_im * _lv43_z_im)) >= 4.0)) {
    _lr45 = (f32(_lv44_it) + (1.0 - (log(log(((_lv42_z_re * _lv42_z_re) + (_lv43_z_im * _lv43_z_im)))) / 0.6931)));
    break;
    } else {
    let _rt46: f32 = (((_lv42_z_re * _lv42_z_re) - (_lv43_z_im * _lv43_z_im)) + c_re__7);
    let _rt47: f32 = ((2.0 * (_lv42_z_re * _lv43_z_im)) + c_im__7);
    let _rt48: i32 = (_lv44_it + 1i);
    _lv42_z_re = _rt46;
    _lv43_z_im = _rt47;
    _lv44_it = _rt48;
    continue;
    }
    }
    }
    let s7v: f32 = _lr45;
    let c_re__8: f32 = base_re + (px_size * ((-0.3125) * aspect));
    let c_im__8: f32 = base_im + (py_size * 0.1875);
    var _lv49_z_re: f32 = c_re__8;
    var _lv50_z_im: f32 = c_im__8;
    var _lv51_it: i32 = 0i;
    var _lr52: f32 = 0.0;
    loop {
    if ((_lv51_it >= max_iter)) {
    _lr52 = max_iter_f;
    break;
    } else {
    if ((((_lv49_z_re * _lv49_z_re) + (_lv50_z_im * _lv50_z_im)) >= 4.0)) {
    _lr52 = (f32(_lv51_it) + (1.0 - (log(log(((_lv49_z_re * _lv49_z_re) + (_lv50_z_im * _lv50_z_im)))) / 0.6931)));
    break;
    } else {
    let _rt53: f32 = (((_lv49_z_re * _lv49_z_re) - (_lv50_z_im * _lv50_z_im)) + c_re__8);
    let _rt54: f32 = ((2.0 * (_lv49_z_re * _lv50_z_im)) + c_im__8);
    let _rt55: i32 = (_lv51_it + 1i);
    _lv49_z_re = _rt53;
    _lv50_z_im = _rt54;
    _lv51_it = _rt55;
    continue;
    }
    }
    }
    let s8v: f32 = _lr52;
    let avg: f32 = 0.125 * (((s1v + s2v) + (s3v + s4v)) + ((s5v + s6v) + (s7v + s8v)));
    let frac_in: f32 = 0.125 * (((select(0.0, 1.0, (s1v >= max_iter_f)) + select(0.0, 1.0, (s2v >= max_iter_f))) + (select(0.0, 1.0, (s3v >= max_iter_f)) + select(0.0, 1.0, (s4v >= max_iter_f)))) + ((select(0.0, 1.0, (s5v >= max_iter_f)) + select(0.0, 1.0, (s6v >= max_iter_f))) + (select(0.0, 1.0, (s7v >= max_iter_f)) + select(0.0, 1.0, (s8v >= max_iter_f)))));
    let cval: f32 = sqrt((avg / max_iter_f));
    let keep: f32 = 1.0 - frac_in;
    let r: f32 = keep * (0.5 * (1.0 + sin(((cval * 12.0) + shift))));
    let g: f32 = keep * (0.5 * (1.0 + sin(((cval * 12.0) + (shift + 2.1)))));
    let b: f32 = keep * (0.5 * (1.0 + sin(((cval * 12.0) + (shift + 4.2)))));
    let ri: u32 = u32(min(255i, max(0i, i32((255.0 * r)))));
    let gi: u32 = u32(min(255i, max(0i, i32((255.0 * g)))));
    let bi: u32 = u32(min(255i, max(0i, i32((255.0 * b)))));
    dst[gid.y * params.width + gid.x] = (((ri << 16u) | (gi << 8u)) | bi);
}
