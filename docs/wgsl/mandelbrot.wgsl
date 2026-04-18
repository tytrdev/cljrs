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
    let _s0: f32 = (f32(params.s0) / 1000.0);
    let _s1: f32 = (0.3 * _s0);
    let _s2: f32 = (0.05 + _s1);
    let _s3: f32 = _s2;
    let _s4: f32 = (0.001 * f32(params.t_ms));
    let _s5: f32 = _s4;
    let _s6: f32 = 30.0;
    let _s7: f32 = (_s5 / _s6);
    let _s8: f32 = (_s6 * floor(_s7));
    let _s9: f32 = (_s5 - _s8);
    let _s10: f32 = _s9;
    let _s11: f32 = (_s3 * _s10);
    let _s12: f32 = exp(_s11);
    let _s13: f32 = (-0.7436438870371587);
    let _s14: f32 = 0.1318259042053119;
    let _s15: f32 = (f32(params.s1) / 1000.0);
    let _s16: f32 = (2.0 * _s15);
    let _s17: f32 = (_s16 - 1.0);
    let _s18: f32 = (0.001 * _s17);
    let _s19: f32 = _s18;
    let _s20: f32 = (f32(params.s2) / 1000.0);
    let _s21: f32 = (2.0 * _s20);
    let _s22: f32 = (_s21 - 1.0);
    let _s23: f32 = (0.001 * _s22);
    let _s24: f32 = _s23;
    let _s25: f32 = (_s13 + _s19);
    let _s26: f32 = _s25;
    let _s27: f32 = (_s14 + _s24);
    let _s28: f32 = _s27;
    let _s29: f32 = (f32(params.s3) / 1000.0);
    let _s30: f32 = (6.2831853 * _s29);
    let _s31: f32 = _s30;
    let _s32: f32 = (f32(i32(params.width)) / f32(i32(params.height)));
    let _s33: f32 = _s32;
    let _s34: f32 = f32(i32(params.width));
    let _s35: f32 = f32(i32(params.height));
    let _s36: f32 = (log(_s12) / 0.6931);
    let _s37: f32 = (40.0 * _s36);
    let _s38: f32 = (100.0 + _s37);
    let _s39: f32 = _s38;
    let _s40: i32 = min(2000i, max(64i, i32(_s39)));
    let _s41: f32 = f32(_s40);
    let _s42: f32 = (_s34 * _s12);
    let _s43: f32 = (2.0 / _s42);
    let _s44: f32 = _s43;
    let _s45: f32 = (_s35 * _s12);
    let _s46: f32 = (2.0 / _s45);
    let _s47: f32 = _s46;
    let _s48: f32 = (f32(k_x) / _s34);
    let _s49: f32 = (2.0 * _s48);
    let _s50: f32 = (_s49 - 1.0);
    let _s51: f32 = (_s50 / _s12);
    let _s52: f32 = _s51;
    let _s53: f32 = (f32(k_y) / _s35);
    let _s54: f32 = (2.0 * _s53);
    let _s55: f32 = (_s54 - 1.0);
    let _s56: f32 = (_s55 / _s12);
    let _s57: f32 = _s56;
    let _s58: f32 = (_s52 * _s33);
    let _s59: f32 = (_s26 + _s58);
    let _s60: f32 = _s59;
    let _s61: f32 = (_s28 + _s57);
    let _s62: f32 = _s61;
    let _s63: f32 = ((-0.4375) * _s33);
    let _s64: f32 = (_s44 * _s63);
    let _s65: f32 = (_s60 + _s64);
    let _s66: f32 = _s65;
    let _s67: f32 = (_s47 * (-0.0625));
    let _s68: f32 = (_s62 + _s67);
    let _s69: f32 = _s68;
    var _lv70_z_re: f32 = _s66;
    var _lv71_z_im: f32 = _s69;
    var _lv72_it: i32 = 0i;
    var _lr73: f32 = 0.0;
    loop {
    if ((_lv72_it >= _s40)) {
    _lr73 = _s41;
    break;
    } else {
    let _s74: f32 = (_lv70_z_re * _lv70_z_re);
    let _s75: f32 = (_lv71_z_im * _lv71_z_im);
    let _s76: f32 = (_s74 + _s75);
    let _s77: f32 = _s76;
    if ((_s77 >= 4.0)) {
    let _s78: f32 = (log(log(_s77)) / 0.6931);
    let _s79: f32 = (1.0 - _s78);
    let _s80: f32 = (f32(_lv72_it) + _s79);
    _lr73 = _s80;
    break;
    } else {
    let _s81: f32 = (_lv70_z_re * _lv70_z_re);
    let _s82: f32 = (_lv71_z_im * _lv71_z_im);
    let _s83: f32 = (_s81 - _s82);
    let _s84: f32 = (_s83 + _s66);
    let _rt85: f32 = _s84;
    let _s86: f32 = (_lv70_z_re * _lv71_z_im);
    let _s87: f32 = (2.0 * _s86);
    let _s88: f32 = (_s87 + _s69);
    let _rt89: f32 = _s88;
    let _s90: i32 = (_lv72_it + 1i);
    let _rt91: i32 = _s90;
    _lv70_z_re = _rt85;
    _lv71_z_im = _rt89;
    _lv72_it = _rt91;
    continue;
    }
    }
    }
    let _s92: f32 = _lr73;
    let _s93: f32 = ((-0.1875) * _s33);
    let _s94: f32 = (_s44 * _s93);
    let _s95: f32 = (_s60 + _s94);
    let _s96: f32 = _s95;
    let _s97: f32 = (_s47 * (-0.3125));
    let _s98: f32 = (_s62 + _s97);
    let _s99: f32 = _s98;
    var _lv100_z_re: f32 = _s96;
    var _lv101_z_im: f32 = _s99;
    var _lv102_it: i32 = 0i;
    var _lr103: f32 = 0.0;
    loop {
    if ((_lv102_it >= _s40)) {
    _lr103 = _s41;
    break;
    } else {
    let _s104: f32 = (_lv100_z_re * _lv100_z_re);
    let _s105: f32 = (_lv101_z_im * _lv101_z_im);
    let _s106: f32 = (_s104 + _s105);
    let _s107: f32 = _s106;
    if ((_s107 >= 4.0)) {
    let _s108: f32 = (log(log(_s107)) / 0.6931);
    let _s109: f32 = (1.0 - _s108);
    let _s110: f32 = (f32(_lv102_it) + _s109);
    _lr103 = _s110;
    break;
    } else {
    let _s111: f32 = (_lv100_z_re * _lv100_z_re);
    let _s112: f32 = (_lv101_z_im * _lv101_z_im);
    let _s113: f32 = (_s111 - _s112);
    let _s114: f32 = (_s113 + _s96);
    let _rt115: f32 = _s114;
    let _s116: f32 = (_lv100_z_re * _lv101_z_im);
    let _s117: f32 = (2.0 * _s116);
    let _s118: f32 = (_s117 + _s99);
    let _rt119: f32 = _s118;
    let _s120: i32 = (_lv102_it + 1i);
    let _rt121: i32 = _s120;
    _lv100_z_re = _rt115;
    _lv101_z_im = _rt119;
    _lv102_it = _rt121;
    continue;
    }
    }
    }
    let _s122: f32 = _lr103;
    let _s123: f32 = (0.0625 * _s33);
    let _s124: f32 = (_s44 * _s123);
    let _s125: f32 = (_s60 + _s124);
    let _s126: f32 = _s125;
    let _s127: f32 = (_s47 * (-0.4375));
    let _s128: f32 = (_s62 + _s127);
    let _s129: f32 = _s128;
    var _lv130_z_re: f32 = _s126;
    var _lv131_z_im: f32 = _s129;
    var _lv132_it: i32 = 0i;
    var _lr133: f32 = 0.0;
    loop {
    if ((_lv132_it >= _s40)) {
    _lr133 = _s41;
    break;
    } else {
    let _s134: f32 = (_lv130_z_re * _lv130_z_re);
    let _s135: f32 = (_lv131_z_im * _lv131_z_im);
    let _s136: f32 = (_s134 + _s135);
    let _s137: f32 = _s136;
    if ((_s137 >= 4.0)) {
    let _s138: f32 = (log(log(_s137)) / 0.6931);
    let _s139: f32 = (1.0 - _s138);
    let _s140: f32 = (f32(_lv132_it) + _s139);
    _lr133 = _s140;
    break;
    } else {
    let _s141: f32 = (_lv130_z_re * _lv130_z_re);
    let _s142: f32 = (_lv131_z_im * _lv131_z_im);
    let _s143: f32 = (_s141 - _s142);
    let _s144: f32 = (_s143 + _s126);
    let _rt145: f32 = _s144;
    let _s146: f32 = (_lv130_z_re * _lv131_z_im);
    let _s147: f32 = (2.0 * _s146);
    let _s148: f32 = (_s147 + _s129);
    let _rt149: f32 = _s148;
    let _s150: i32 = (_lv132_it + 1i);
    let _rt151: i32 = _s150;
    _lv130_z_re = _rt145;
    _lv131_z_im = _rt149;
    _lv132_it = _rt151;
    continue;
    }
    }
    }
    let _s152: f32 = _lr133;
    let _s153: f32 = (0.3125 * _s33);
    let _s154: f32 = (_s44 * _s153);
    let _s155: f32 = (_s60 + _s154);
    let _s156: f32 = _s155;
    let _s157: f32 = (_s47 * (-0.1875));
    let _s158: f32 = (_s62 + _s157);
    let _s159: f32 = _s158;
    var _lv160_z_re: f32 = _s156;
    var _lv161_z_im: f32 = _s159;
    var _lv162_it: i32 = 0i;
    var _lr163: f32 = 0.0;
    loop {
    if ((_lv162_it >= _s40)) {
    _lr163 = _s41;
    break;
    } else {
    let _s164: f32 = (_lv160_z_re * _lv160_z_re);
    let _s165: f32 = (_lv161_z_im * _lv161_z_im);
    let _s166: f32 = (_s164 + _s165);
    let _s167: f32 = _s166;
    if ((_s167 >= 4.0)) {
    let _s168: f32 = (log(log(_s167)) / 0.6931);
    let _s169: f32 = (1.0 - _s168);
    let _s170: f32 = (f32(_lv162_it) + _s169);
    _lr163 = _s170;
    break;
    } else {
    let _s171: f32 = (_lv160_z_re * _lv160_z_re);
    let _s172: f32 = (_lv161_z_im * _lv161_z_im);
    let _s173: f32 = (_s171 - _s172);
    let _s174: f32 = (_s173 + _s156);
    let _rt175: f32 = _s174;
    let _s176: f32 = (_lv160_z_re * _lv161_z_im);
    let _s177: f32 = (2.0 * _s176);
    let _s178: f32 = (_s177 + _s159);
    let _rt179: f32 = _s178;
    let _s180: i32 = (_lv162_it + 1i);
    let _rt181: i32 = _s180;
    _lv160_z_re = _rt175;
    _lv161_z_im = _rt179;
    _lv162_it = _rt181;
    continue;
    }
    }
    }
    let _s182: f32 = _lr163;
    let _s183: f32 = (0.4375 * _s33);
    let _s184: f32 = (_s44 * _s183);
    let _s185: f32 = (_s60 + _s184);
    let _s186: f32 = _s185;
    let _s187: f32 = (_s47 * 0.0625);
    let _s188: f32 = (_s62 + _s187);
    let _s189: f32 = _s188;
    var _lv190_z_re: f32 = _s186;
    var _lv191_z_im: f32 = _s189;
    var _lv192_it: i32 = 0i;
    var _lr193: f32 = 0.0;
    loop {
    if ((_lv192_it >= _s40)) {
    _lr193 = _s41;
    break;
    } else {
    let _s194: f32 = (_lv190_z_re * _lv190_z_re);
    let _s195: f32 = (_lv191_z_im * _lv191_z_im);
    let _s196: f32 = (_s194 + _s195);
    let _s197: f32 = _s196;
    if ((_s197 >= 4.0)) {
    let _s198: f32 = (log(log(_s197)) / 0.6931);
    let _s199: f32 = (1.0 - _s198);
    let _s200: f32 = (f32(_lv192_it) + _s199);
    _lr193 = _s200;
    break;
    } else {
    let _s201: f32 = (_lv190_z_re * _lv190_z_re);
    let _s202: f32 = (_lv191_z_im * _lv191_z_im);
    let _s203: f32 = (_s201 - _s202);
    let _s204: f32 = (_s203 + _s186);
    let _rt205: f32 = _s204;
    let _s206: f32 = (_lv190_z_re * _lv191_z_im);
    let _s207: f32 = (2.0 * _s206);
    let _s208: f32 = (_s207 + _s189);
    let _rt209: f32 = _s208;
    let _s210: i32 = (_lv192_it + 1i);
    let _rt211: i32 = _s210;
    _lv190_z_re = _rt205;
    _lv191_z_im = _rt209;
    _lv192_it = _rt211;
    continue;
    }
    }
    }
    let _s212: f32 = _lr193;
    let _s213: f32 = (0.1875 * _s33);
    let _s214: f32 = (_s44 * _s213);
    let _s215: f32 = (_s60 + _s214);
    let _s216: f32 = _s215;
    let _s217: f32 = (_s47 * 0.3125);
    let _s218: f32 = (_s62 + _s217);
    let _s219: f32 = _s218;
    var _lv220_z_re: f32 = _s216;
    var _lv221_z_im: f32 = _s219;
    var _lv222_it: i32 = 0i;
    var _lr223: f32 = 0.0;
    loop {
    if ((_lv222_it >= _s40)) {
    _lr223 = _s41;
    break;
    } else {
    let _s224: f32 = (_lv220_z_re * _lv220_z_re);
    let _s225: f32 = (_lv221_z_im * _lv221_z_im);
    let _s226: f32 = (_s224 + _s225);
    let _s227: f32 = _s226;
    if ((_s227 >= 4.0)) {
    let _s228: f32 = (log(log(_s227)) / 0.6931);
    let _s229: f32 = (1.0 - _s228);
    let _s230: f32 = (f32(_lv222_it) + _s229);
    _lr223 = _s230;
    break;
    } else {
    let _s231: f32 = (_lv220_z_re * _lv220_z_re);
    let _s232: f32 = (_lv221_z_im * _lv221_z_im);
    let _s233: f32 = (_s231 - _s232);
    let _s234: f32 = (_s233 + _s216);
    let _rt235: f32 = _s234;
    let _s236: f32 = (_lv220_z_re * _lv221_z_im);
    let _s237: f32 = (2.0 * _s236);
    let _s238: f32 = (_s237 + _s219);
    let _rt239: f32 = _s238;
    let _s240: i32 = (_lv222_it + 1i);
    let _rt241: i32 = _s240;
    _lv220_z_re = _rt235;
    _lv221_z_im = _rt239;
    _lv222_it = _rt241;
    continue;
    }
    }
    }
    let _s242: f32 = _lr223;
    let _s243: f32 = ((-0.0625) * _s33);
    let _s244: f32 = (_s44 * _s243);
    let _s245: f32 = (_s60 + _s244);
    let _s246: f32 = _s245;
    let _s247: f32 = (_s47 * 0.4375);
    let _s248: f32 = (_s62 + _s247);
    let _s249: f32 = _s248;
    var _lv250_z_re: f32 = _s246;
    var _lv251_z_im: f32 = _s249;
    var _lv252_it: i32 = 0i;
    var _lr253: f32 = 0.0;
    loop {
    if ((_lv252_it >= _s40)) {
    _lr253 = _s41;
    break;
    } else {
    let _s254: f32 = (_lv250_z_re * _lv250_z_re);
    let _s255: f32 = (_lv251_z_im * _lv251_z_im);
    let _s256: f32 = (_s254 + _s255);
    let _s257: f32 = _s256;
    if ((_s257 >= 4.0)) {
    let _s258: f32 = (log(log(_s257)) / 0.6931);
    let _s259: f32 = (1.0 - _s258);
    let _s260: f32 = (f32(_lv252_it) + _s259);
    _lr253 = _s260;
    break;
    } else {
    let _s261: f32 = (_lv250_z_re * _lv250_z_re);
    let _s262: f32 = (_lv251_z_im * _lv251_z_im);
    let _s263: f32 = (_s261 - _s262);
    let _s264: f32 = (_s263 + _s246);
    let _rt265: f32 = _s264;
    let _s266: f32 = (_lv250_z_re * _lv251_z_im);
    let _s267: f32 = (2.0 * _s266);
    let _s268: f32 = (_s267 + _s249);
    let _rt269: f32 = _s268;
    let _s270: i32 = (_lv252_it + 1i);
    let _rt271: i32 = _s270;
    _lv250_z_re = _rt265;
    _lv251_z_im = _rt269;
    _lv252_it = _rt271;
    continue;
    }
    }
    }
    let _s272: f32 = _lr253;
    let _s273: f32 = ((-0.3125) * _s33);
    let _s274: f32 = (_s44 * _s273);
    let _s275: f32 = (_s60 + _s274);
    let _s276: f32 = _s275;
    let _s277: f32 = (_s47 * 0.1875);
    let _s278: f32 = (_s62 + _s277);
    let _s279: f32 = _s278;
    var _lv280_z_re: f32 = _s276;
    var _lv281_z_im: f32 = _s279;
    var _lv282_it: i32 = 0i;
    var _lr283: f32 = 0.0;
    loop {
    if ((_lv282_it >= _s40)) {
    _lr283 = _s41;
    break;
    } else {
    let _s284: f32 = (_lv280_z_re * _lv280_z_re);
    let _s285: f32 = (_lv281_z_im * _lv281_z_im);
    let _s286: f32 = (_s284 + _s285);
    let _s287: f32 = _s286;
    if ((_s287 >= 4.0)) {
    let _s288: f32 = (log(log(_s287)) / 0.6931);
    let _s289: f32 = (1.0 - _s288);
    let _s290: f32 = (f32(_lv282_it) + _s289);
    _lr283 = _s290;
    break;
    } else {
    let _s291: f32 = (_lv280_z_re * _lv280_z_re);
    let _s292: f32 = (_lv281_z_im * _lv281_z_im);
    let _s293: f32 = (_s291 - _s292);
    let _s294: f32 = (_s293 + _s276);
    let _rt295: f32 = _s294;
    let _s296: f32 = (_lv280_z_re * _lv281_z_im);
    let _s297: f32 = (2.0 * _s296);
    let _s298: f32 = (_s297 + _s279);
    let _rt299: f32 = _s298;
    let _s300: i32 = (_lv282_it + 1i);
    let _rt301: i32 = _s300;
    _lv280_z_re = _rt295;
    _lv281_z_im = _rt299;
    _lv282_it = _rt301;
    continue;
    }
    }
    }
    let _s302: f32 = _lr283;
    let _s303: f32 = (_s92 + _s122);
    let _s304: f32 = (_s152 + _s182);
    let _s305: f32 = (_s303 + _s304);
    let _s306: f32 = (_s212 + _s242);
    let _s307: f32 = (_s272 + _s302);
    let _s308: f32 = (_s306 + _s307);
    let _s309: f32 = (_s305 + _s308);
    let _s310: f32 = (0.125 * _s309);
    let _s311: f32 = _s310;
    let _s312: f32 = select(0.0, 1.0, (_s92 >= _s41));
    let _s313: f32 = select(0.0, 1.0, (_s122 >= _s41));
    let _s314: f32 = (_s312 + _s313);
    let _s315: f32 = select(0.0, 1.0, (_s152 >= _s41));
    let _s316: f32 = select(0.0, 1.0, (_s182 >= _s41));
    let _s317: f32 = (_s315 + _s316);
    let _s318: f32 = (_s314 + _s317);
    let _s319: f32 = select(0.0, 1.0, (_s212 >= _s41));
    let _s320: f32 = select(0.0, 1.0, (_s242 >= _s41));
    let _s321: f32 = (_s319 + _s320);
    let _s322: f32 = select(0.0, 1.0, (_s272 >= _s41));
    let _s323: f32 = select(0.0, 1.0, (_s302 >= _s41));
    let _s324: f32 = (_s322 + _s323);
    let _s325: f32 = (_s321 + _s324);
    let _s326: f32 = (_s318 + _s325);
    let _s327: f32 = (0.125 * _s326);
    let _s328: f32 = _s327;
    let _s329: f32 = (_s311 / _s41);
    let _s330: f32 = sqrt(_s329);
    let _s331: f32 = (1.0 - _s328);
    let _s332: f32 = _s331;
    let _s333: f32 = (_s330 * 12.0);
    let _s334: f32 = (_s333 + _s31);
    let _s335: f32 = (1.0 + sin(_s334));
    let _s336: f32 = (0.5 * _s335);
    let _s337: f32 = (_s332 * _s336);
    let _s338: f32 = _s337;
    let _s339: f32 = (_s330 * 12.0);
    let _s340: f32 = (_s31 + 2.1);
    let _s341: f32 = (_s339 + _s340);
    let _s342: f32 = (1.0 + sin(_s341));
    let _s343: f32 = (0.5 * _s342);
    let _s344: f32 = (_s332 * _s343);
    let _s345: f32 = _s344;
    let _s346: f32 = (_s330 * 12.0);
    let _s347: f32 = (_s31 + 4.2);
    let _s348: f32 = (_s346 + _s347);
    let _s349: f32 = (1.0 + sin(_s348));
    let _s350: f32 = (0.5 * _s349);
    let _s351: f32 = (_s332 * _s350);
    let _s352: f32 = _s351;
    let _s353: f32 = (255.0 * _s338);
    let _s354: u32 = u32(min(255i, max(0i, i32(_s353))));
    let _s355: f32 = (255.0 * _s345);
    let _s356: u32 = u32(min(255i, max(0i, i32(_s355))));
    let _s357: f32 = (255.0 * _s352);
    let _s358: u32 = u32(min(255i, max(0i, i32(_s357))));
    dst[gid.y * params.width + gid.x] = (((_s354 << 16u) | (_s356 << 8u)) | _s358);
}
