//! Headless GPU recorder. Mirrors `bin/record` but dispatches a
//! defn-gpu-pixel kernel per frame and pipes the packed RGBA into ffmpeg.
//!
//! Usage:
//!   cargo run --release --features gpu-demo --bin gpu-record -- \
//!       --kernel demo_gpu/plasma.clj --seconds 6 --fps 60 \
//!       --size 960x540 --out out/plasma_gpu.mp4

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio, exit};
use std::time::Instant;

use cljrs::{
    builtins,
    env::Env,
    eval, reader,
    gpu::{global_gpu, PixelParams},
    value::Value,
};

struct Args {
    kernel: PathBuf,
    seconds: f64,
    fps: u32,
    width: u32,
    height: u32,
    out: PathBuf,
    sliders: [i32; 4],
    crf: u32,
}

fn parse_args() -> Result<Args, String> {
    let mut kernel = None::<PathBuf>;
    let mut seconds = 6.0;
    let mut fps = 60u32;
    let (mut w, mut h) = (960u32, 540u32);
    let mut out = None::<PathBuf>;
    let mut sliders = [500i32; 4];
    let mut crf = 20u32;

    let argv: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--kernel" => { kernel = Some(argv[i+1].clone().into()); i += 2; }
            "--seconds" => { seconds = argv[i+1].parse().map_err(|e| format!("{e}"))?; i += 2; }
            "--fps" => { fps = argv[i+1].parse().map_err(|e| format!("{e}"))?; i += 2; }
            "--size" => {
                let v = &argv[i+1];
                let (a, b) = v.split_once('x').ok_or("--size WxH")?;
                w = a.parse().map_err(|e| format!("{e}"))?;
                h = b.parse().map_err(|e| format!("{e}"))?;
                i += 2;
            }
            "--out" => { out = Some(argv[i+1].clone().into()); i += 2; }
            "--sliders" => {
                let parts: Vec<&str> = argv[i+1].split(',').collect();
                if parts.len() != 4 {
                    return Err("--sliders a,b,c,d".into());
                }
                for (j, p) in parts.iter().enumerate() {
                    sliders[j] = p.parse().map_err(|e| format!("{e}"))?;
                }
                i += 2;
            }
            "--crf" => { crf = argv[i+1].parse().map_err(|e| format!("{e}"))?; i += 2; }
            other => return Err(format!("unknown arg: {other}")),
        }
    }
    Ok(Args {
        kernel: kernel.ok_or("--kernel required")?,
        seconds, fps, width: w, height: h,
        out: out.ok_or("--out required")?,
        sliders, crf,
    })
}

fn main() {
    let a = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("gpu-record: {e}");
            eprintln!("usage: gpu-record --kernel <path> --out <file.mp4> [--seconds 6] [--fps 60] [--size 960x540] [--sliders 500,500,500,500]");
            exit(1);
        }
    };
    if let Some(p) = a.out.parent() && !p.as_os_str().is_empty() {
        let _ = fs::create_dir_all(p);
    }

    let env = Env::new();
    builtins::install(&env);
    let src = fs::read_to_string(&a.kernel).unwrap_or_else(|e| {
        eprintln!("read kernel: {e}"); exit(1);
    });
    for f in reader::read_all(&src).unwrap_or_else(|e| {
        eprintln!("parse: {e}"); exit(1);
    }) {
        if let Err(e) = eval::eval(&f, &env) {
            eprintln!("eval: {e}"); exit(1);
        }
    }
    let kernel = match env.lookup("render") {
        Ok(Value::GpuPixelKernel(k)) => k,
        Ok(other) => {
            eprintln!("`render` is {}, expected gpu-pixel-kernel", other.type_name());
            exit(1);
        }
        Err(e) => { eprintln!("no `render`: {e}"); exit(1); }
    };
    let gpu = global_gpu().unwrap_or_else(|e| {
        eprintln!("no GPU: {e}"); exit(1);
    });
    eprintln!(
        "gpu-record: {} ({:?}, {:?})",
        gpu.adapter_info.name, gpu.adapter_info.device_type, gpu.adapter_info.backend
    );

    let size = format!("{}x{}", a.width, a.height);
    let fps_s = a.fps.to_string();
    let crf_s = a.crf.to_string();
    let out_s = a.out.to_string_lossy().to_string();
    let mut ffmpeg = Command::new("ffmpeg")
        .args([
            "-y", "-loglevel", "warning",
            "-f", "rawvideo", "-pix_fmt", "rgba",
            "-s", &size, "-r", &fps_s, "-i", "-",
            "-c:v", "libx264", "-pix_fmt", "yuv420p",
            "-crf", &crf_s, "-movflags", "+faststart", &out_s,
        ])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| {
            eprintln!("spawn ffmpeg: {e}"); exit(1);
        });
    let mut stdin = ffmpeg.stdin.take().unwrap();

    let total = (a.seconds * a.fps as f64).round() as u64;
    let mut rgba = vec![0u8; (a.width * a.height * 4) as usize];
    let frame_dur_ms = 1000.0 / a.fps as f64;
    let start = Instant::now();
    eprintln!("gpu-record: {total} frames @ {} {}fps", size, a.fps);

    for frame in 0..total {
        let params = PixelParams {
            width: a.width,
            height: a.height,
            t_ms: (frame as f64 * frame_dur_ms).round() as i32,
            s0: a.sliders[0], s1: a.sliders[1],
            s2: a.sliders[2], s3: a.sliders[3],
            _pad: 0,
        };
        // Use TAA for smoother video. Matches the live demo's settings.
        match kernel.render_frame_taa(&gpu, params, 0.22) {
            Ok(buf) => {
                for (i, &c) in buf.iter().enumerate() {
                    let o = i * 4;
                    rgba[o] = ((c >> 16) & 0xff) as u8;
                    rgba[o+1] = ((c >> 8) & 0xff) as u8;
                    rgba[o+2] = (c & 0xff) as u8;
                    rgba[o+3] = 0xff;
                }
            }
            Err(e) => { eprintln!("render frame {frame}: {e}"); break; }
        }
        if stdin.write_all(&rgba).is_err() {
            eprintln!("ffmpeg pipe closed at frame {frame}");
            break;
        }
        if frame % a.fps as u64 == 0 {
            eprintln!("  {frame}/{total}");
        }
    }

    drop(stdin);
    let _ = ffmpeg.wait();
    let elapsed = start.elapsed().as_secs_f64();
    eprintln!("gpu-record: done in {elapsed:.2}s ({:.1} fps render)", total as f64 / elapsed);
}
