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
    let t: f32 = 0.001 * f32(params.t_ms);
    let orbit_s: f32 = 1.5 * (f32(params.s0) / 1000.0);
    let orbit_r: f32 = 2.5 + (4.5 * (f32(params.s1) / 1000.0));
    let sun_az: f32 = 6.2831853 * (f32(params.s2) / 1000.0);
    let blend_k: f32 = 0.05 + (1.15 * (f32(params.s3) / 1000.0));
    let aspect: f32 = f32(i32(params.width)) / f32(i32(params.height));
    let uv_x: f32 = aspect * ((2.0 * (f32(k_x) / f32(i32(params.width)))) - 1.0);
    let uv_y: f32 = 1.0 - (2.0 * (f32(k_y) / f32(i32(params.height))));
    let ca: f32 = t * orbit_s;
    let cam_x: f32 = orbit_r * sin(ca);
    let cam_y: f32 = 1.2;
    let cam_z: f32 = orbit_r * cos(ca);
    let tgt_x: f32 = 0.0;
    let tgt_y: f32 = 0.6;
    let tgt_z: f32 = 0.0;
    let lx: f32 = tgt_x - cam_x;
    let ly: f32 = tgt_y - cam_y;
    let lz: f32 = tgt_z - cam_z;
    let ll: f32 = sqrt((((lx * lx) + (ly * ly)) + (lz * lz)));
    let fx: f32 = lx / ll;
    let fy: f32 = ly / ll;
    let fz: f32 = lz / ll;
    let rx0: f32 = fz;
    let ry0: f32 = 0.0;
    let rz0: f32 = -fx;
    let rl: f32 = max(0.0001, sqrt((((rx0 * rx0) + (ry0 * ry0)) + (rz0 * rz0))));
    let rx: f32 = rx0 / rl;
    let ry: f32 = ry0 / rl;
    let rz: f32 = rz0 / rl;
    let ux: f32 = (fy * rz) - (fz * ry);
    let uy: f32 = (fz * rx) - (fx * rz);
    let uz: f32 = (fx * ry) - (fy * rx);
    let fov: f32 = 1.4;
    let dx0: f32 = ((fx * fov) + (rx * uv_x)) + (ux * uv_y);
    let dy0: f32 = ((fy * fov) + (ry * uv_x)) + (uy * uv_y);
    let dz0: f32 = ((fz * fov) + (rz * uv_x)) + (uz * uv_y);
    let dl: f32 = sqrt((((dx0 * dx0) + (dy0 * dy0)) + (dz0 * dz0)));
    let dx: f32 = dx0 / dl;
    let dy: f32 = dy0 / dl;
    let dz: f32 = dz0 / dl;
    let sel: f32 = 0.9;
    let sx0: f32 = cos(sel) * cos(sun_az);
    let sz0: f32 = cos(sel) * sin(sun_az);
    let sy0: f32 = sin(sel);
    let sx: f32 = sx0;
    let sy: f32 = sy0;
    let sz: f32 = sz0;
    let max_steps: i32 = 96i;
    let max_dist: f32 = 40.0;
    let hit_eps: f32 = 0.001;
    var _lv0_tt: f32 = 0.0;
    var _lv1_steps: i32 = 0i;
    var _lr2: f32 = 0.0;
    loop {
    if ((_lv1_steps >= max_steps)) {
    _lr2 = max_dist;
    break;
    } else {
    if ((_lv0_tt >= max_dist)) {
    _lr2 = max_dist;
    break;
    } else {
    let ax_2: f32 = (cam_x + (dx * _lv0_tt)) - 0.0;
    let ay_2: f32 = (cam_y + (dy * _lv0_tt)) - (0.55 + (0.25 * sin((t * 1.1))));
    let az_2: f32 = (cam_z + (dz * _lv0_tt)) - 0.0;
    let da_2: f32 = sqrt((((ax_2 * ax_2) + (ay_2 * ay_2)) + (az_2 * az_2))) - 0.55;
    let bx_2: f32 = (cam_x + (dx * _lv0_tt)) - (0.9 * cos((t * 0.7)));
    let by_2: f32 = (cam_y + (dy * _lv0_tt)) - (0.55 + (0.15 * cos((t * 1.3))));
    let bz_2: f32 = (cam_z + (dz * _lv0_tt)) - (0.9 * sin((t * 0.7)));
    let db_2: f32 = sqrt((((bx_2 * bx_2) + (by_2 * by_2)) + (bz_2 * bz_2))) - 0.45;
    let hh_2: f32 = max(0.0, (1.0 - (abs((da_2 - db_2)) / max(0.0001, blend_k))));
    let smn_2: f32 = min(da_2, db_2) - ((blend_k * hh_2) * (hh_2 * (hh_2 * (1.0 / 6.0))));
    let df_2: f32 = cam_y + (dy * _lv0_tt);
    if ((min(smn_2, df_2) < hit_eps)) {
    _lr2 = _lv0_tt;
    break;
    } else {
    let _rt3: f32 = (_lv0_tt + max(hit_eps, min(smn_2, df_2)));
    let _rt4: i32 = (_lv1_steps + 1i);
    _lv0_tt = _rt3;
    _lv1_steps = _rt4;
    continue;
    }
    }
    }
    }
    let hit_t: f32 = _lr2;
    let hit_: bool = hit_t < (max_dist - 0.1);
    let hx: f32 = cam_x + (dx * hit_t);
    let hy: f32 = cam_y + (dy * hit_t);
    let hz: f32 = cam_z + (dz * hit_t);
    let ne: f32 = 0.0015;
    let ax_3: f32 = (hx + ne) - 0.0;
    let ay_3: f32 = hy - (0.55 + (0.25 * sin((t * 1.1))));
    let az_3: f32 = hz - 0.0;
    let da_3: f32 = sqrt((((ax_3 * ax_3) + (ay_3 * ay_3)) + (az_3 * az_3))) - 0.55;
    let bx_3: f32 = (hx + ne) - (0.9 * cos((t * 0.7)));
    let by_3: f32 = hy - (0.55 + (0.15 * cos((t * 1.3))));
    let bz_3: f32 = hz - (0.9 * sin((t * 0.7)));
    let db_3: f32 = sqrt((((bx_3 * bx_3) + (by_3 * by_3)) + (bz_3 * bz_3))) - 0.45;
    let hh_3: f32 = max(0.0, (1.0 - (abs((da_3 - db_3)) / max(0.0001, blend_k))));
    let smn_3: f32 = min(da_3, db_3) - ((blend_k * hh_3) * (hh_3 * (hh_3 * (1.0 / 6.0))));
    let df_3: f32 = hy;
    let ax_4: f32 = (hx - ne) - 0.0;
    let ay_4: f32 = hy - (0.55 + (0.25 * sin((t * 1.1))));
    let az_4: f32 = hz - 0.0;
    let da_4: f32 = sqrt((((ax_4 * ax_4) + (ay_4 * ay_4)) + (az_4 * az_4))) - 0.55;
    let bx_4: f32 = (hx - ne) - (0.9 * cos((t * 0.7)));
    let by_4: f32 = hy - (0.55 + (0.15 * cos((t * 1.3))));
    let bz_4: f32 = hz - (0.9 * sin((t * 0.7)));
    let db_4: f32 = sqrt((((bx_4 * bx_4) + (by_4 * by_4)) + (bz_4 * bz_4))) - 0.45;
    let hh_4: f32 = max(0.0, (1.0 - (abs((da_4 - db_4)) / max(0.0001, blend_k))));
    let smn_4: f32 = min(da_4, db_4) - ((blend_k * hh_4) * (hh_4 * (hh_4 * (1.0 / 6.0))));
    let df_4: f32 = hy;
    let nx0: f32 = min(smn_3, df_3) - min(smn_4, df_4);
    let ax_5: f32 = hx - 0.0;
    let ay_5: f32 = (hy + ne) - (0.55 + (0.25 * sin((t * 1.1))));
    let az_5: f32 = hz - 0.0;
    let da_5: f32 = sqrt((((ax_5 * ax_5) + (ay_5 * ay_5)) + (az_5 * az_5))) - 0.55;
    let bx_5: f32 = hx - (0.9 * cos((t * 0.7)));
    let by_5: f32 = (hy + ne) - (0.55 + (0.15 * cos((t * 1.3))));
    let bz_5: f32 = hz - (0.9 * sin((t * 0.7)));
    let db_5: f32 = sqrt((((bx_5 * bx_5) + (by_5 * by_5)) + (bz_5 * bz_5))) - 0.45;
    let hh_5: f32 = max(0.0, (1.0 - (abs((da_5 - db_5)) / max(0.0001, blend_k))));
    let smn_5: f32 = min(da_5, db_5) - ((blend_k * hh_5) * (hh_5 * (hh_5 * (1.0 / 6.0))));
    let df_5: f32 = hy + ne;
    let ax_6: f32 = hx - 0.0;
    let ay_6: f32 = (hy - ne) - (0.55 + (0.25 * sin((t * 1.1))));
    let az_6: f32 = hz - 0.0;
    let da_6: f32 = sqrt((((ax_6 * ax_6) + (ay_6 * ay_6)) + (az_6 * az_6))) - 0.55;
    let bx_6: f32 = hx - (0.9 * cos((t * 0.7)));
    let by_6: f32 = (hy - ne) - (0.55 + (0.15 * cos((t * 1.3))));
    let bz_6: f32 = hz - (0.9 * sin((t * 0.7)));
    let db_6: f32 = sqrt((((bx_6 * bx_6) + (by_6 * by_6)) + (bz_6 * bz_6))) - 0.45;
    let hh_6: f32 = max(0.0, (1.0 - (abs((da_6 - db_6)) / max(0.0001, blend_k))));
    let smn_6: f32 = min(da_6, db_6) - ((blend_k * hh_6) * (hh_6 * (hh_6 * (1.0 / 6.0))));
    let df_6: f32 = hy - ne;
    let ny0: f32 = min(smn_5, df_5) - min(smn_6, df_6);
    let ax_7: f32 = hx - 0.0;
    let ay_7: f32 = hy - (0.55 + (0.25 * sin((t * 1.1))));
    let az_7: f32 = (hz + ne) - 0.0;
    let da_7: f32 = sqrt((((ax_7 * ax_7) + (ay_7 * ay_7)) + (az_7 * az_7))) - 0.55;
    let bx_7: f32 = hx - (0.9 * cos((t * 0.7)));
    let by_7: f32 = hy - (0.55 + (0.15 * cos((t * 1.3))));
    let bz_7: f32 = (hz + ne) - (0.9 * sin((t * 0.7)));
    let db_7: f32 = sqrt((((bx_7 * bx_7) + (by_7 * by_7)) + (bz_7 * bz_7))) - 0.45;
    let hh_7: f32 = max(0.0, (1.0 - (abs((da_7 - db_7)) / max(0.0001, blend_k))));
    let smn_7: f32 = min(da_7, db_7) - ((blend_k * hh_7) * (hh_7 * (hh_7 * (1.0 / 6.0))));
    let df_7: f32 = hy;
    let ax_8: f32 = hx - 0.0;
    let ay_8: f32 = hy - (0.55 + (0.25 * sin((t * 1.1))));
    let az_8: f32 = (hz - ne) - 0.0;
    let da_8: f32 = sqrt((((ax_8 * ax_8) + (ay_8 * ay_8)) + (az_8 * az_8))) - 0.55;
    let bx_8: f32 = hx - (0.9 * cos((t * 0.7)));
    let by_8: f32 = hy - (0.55 + (0.15 * cos((t * 1.3))));
    let bz_8: f32 = (hz - ne) - (0.9 * sin((t * 0.7)));
    let db_8: f32 = sqrt((((bx_8 * bx_8) + (by_8 * by_8)) + (bz_8 * bz_8))) - 0.45;
    let hh_8: f32 = max(0.0, (1.0 - (abs((da_8 - db_8)) / max(0.0001, blend_k))));
    let smn_8: f32 = min(da_8, db_8) - ((blend_k * hh_8) * (hh_8 * (hh_8 * (1.0 / 6.0))));
    let df_8: f32 = hy;
    let nz0: f32 = min(smn_7, df_7) - min(smn_8, df_8);
    let nl: f32 = max(0.0001, sqrt((((nx0 * nx0) + (ny0 * ny0)) + (nz0 * nz0))));
    let nx: f32 = nx0 / nl;
    let ny: f32 = ny0 / nl;
    let nz: f32 = nz0 / nl;
    let eps: f32 = 0.02;
    let sox: f32 = hx + (nx * eps);
    let soy: f32 = hy + (ny * eps);
    let soz: f32 = hz + (nz * eps);
    var _lv5_tt: f32 = 0.05;
    var _lv6_mn: f32 = 1.0;
    var _lv7_steps: i32 = 0i;
    var _lr8: f32 = 0.0;
    loop {
    if ((_lv7_steps >= 32i)) {
    _lr8 = _lv6_mn;
    break;
    } else {
    if ((_lv5_tt >= 8.0)) {
    _lr8 = _lv6_mn;
    break;
    } else {
    let ax_10: f32 = (sox + (sx * _lv5_tt)) - 0.0;
    let ay_10: f32 = (soy + (sy * _lv5_tt)) - (0.55 + (0.25 * sin((t * 1.1))));
    let az_10: f32 = (soz + (sz * _lv5_tt)) - 0.0;
    let da_10: f32 = sqrt((((ax_10 * ax_10) + (ay_10 * ay_10)) + (az_10 * az_10))) - 0.55;
    let bx_10: f32 = (sox + (sx * _lv5_tt)) - (0.9 * cos((t * 0.7)));
    let by_10: f32 = (soy + (sy * _lv5_tt)) - (0.55 + (0.15 * cos((t * 1.3))));
    let bz_10: f32 = (soz + (sz * _lv5_tt)) - (0.9 * sin((t * 0.7)));
    let db_10: f32 = sqrt((((bx_10 * bx_10) + (by_10 * by_10)) + (bz_10 * bz_10))) - 0.45;
    let hh_10: f32 = max(0.0, (1.0 - (abs((da_10 - db_10)) / max(0.0001, blend_k))));
    let smn_10: f32 = min(da_10, db_10) - ((blend_k * hh_10) * (hh_10 * (hh_10 * (1.0 / 6.0))));
    let df_10: f32 = soy + (sy * _lv5_tt);
    if ((min(smn_10, df_10) < 0.001)) {
    _lr8 = 0.0;
    break;
    } else {
    let _rt9: f32 = (_lv5_tt + max(0.005, min(smn_10, df_10)));
    let _rt10: f32 = min(_lv6_mn, ((12.0 * min(smn_10, df_10)) / _lv5_tt));
    let _rt11: i32 = (_lv7_steps + 1i);
    _lv5_tt = _rt9;
    _lv6_mn = _rt10;
    _lv7_steps = _rt11;
    continue;
    }
    }
    }
    }
    let soft: f32 = _lr8;
    let shadow: f32 = max(0.0, min(1.0, soft));
    let ndotl: f32 = max(0.0, (((nx * sx) + (ny * sy)) + (nz * sz)));
    let vx: f32 = -dx;
    let vy: f32 = -dy;
    let vz: f32 = -dz;
    let hhx: f32 = sx + vx;
    let hhy: f32 = sy + vy;
    let hhz: f32 = sz + vz;
    let hl: f32 = max(0.0001, sqrt((((hhx * hhx) + (hhy * hhy)) + (hhz * hhz))));
    let hhx_2: f32 = hhx / hl;
    let hhy_2: f32 = hhy / hl;
    let hhz_2: f32 = hhz / hl;
    let ndoth: f32 = max(0.0, (((nx * hhx_2) + (ny * hhy_2)) + (nz * hhz_2)));
    let spec: f32 = pow(ndoth, 48.0);
    let on_floor_: bool = abs(hy) < 0.02;
    let cxi: i32 = i32(floor((hx * 0.5)));
    let czi: i32 = i32(floor((hz * 0.5)));
    let chk: f32 = select(0.22, 0.82, (((cxi + czi) % 2i) == 0i));
    let ar: f32 = select(0.95, chk, on_floor_);
    let ag: f32 = select(0.55, chk, on_floor_);
    let ab: f32 = select(0.32, chk, on_floor_);
    let amb: f32 = 0.18;
    let lit: f32 = shadow * ndotl;
    let rr: f32 = (amb * ar) + (((0.9 * lit) * ar) + ((0.55 * shadow) * spec));
    let gg: f32 = (amb * ag) + (((0.9 * lit) * ag) + ((0.55 * shadow) * spec));
    let bb: f32 = (amb * ab) + (((0.9 * lit) * ab) + ((0.55 * shadow) * spec));
    let skyt: f32 = max(0.0, min(1.0, (0.5 * (1.0 + dy))));
    let skr: f32 = ((1.0 - skyt) * 0.55) + (skyt * 0.18);
    let skg: f32 = ((1.0 - skyt) * 0.72) + (skyt * 0.35);
    let skb: f32 = ((1.0 - skyt) * 0.95) + (skyt * 0.65);
    let out_r: f32 = select(skr, rr, hit_);
    let out_g: f32 = select(skg, gg, hit_);
    let out_b: f32 = select(skb, bb, hit_);
    let ri: u32 = u32(min(255i, max(0i, i32((255.0 * out_r)))));
    let gi: u32 = u32(min(255i, max(0i, i32((255.0 * out_g)))));
    let bi: u32 = u32(min(255i, max(0i, i32((255.0 * out_b)))));
    dst[gid.y * params.width + gid.x] = (((ri << 16u) | (gi << 8u)) | bi);
}
