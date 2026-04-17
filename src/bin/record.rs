//! Headless demo recorder. Renders a cljrs kernel frame-by-frame and
//! pipes raw RGBA into `ffmpeg` on stdin to produce an mp4 (or whatever
//! container ffmpeg infers from the output path).
//!
//! Designed for building the docs-site demo gallery from the command line:
//!   cargo run --release --features demo --bin record -- \
//!       --kernel demo/fractal.clj --seconds 6 --fps 60 --size 960x540 \
//!       --out out/fractal.mp4 --sliders 500,500,500,500
//!
//! Keeps the kernel ABI identical to the live demo so any kernel that
//! runs there records correctly.

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio, exit};

use cljrs::{builtins, env::Env, eval, reader, value::Value};
use rayon::prelude::*;

struct Args {
    kernel: PathBuf,
    seconds: f64,
    fps: u32,
    width: usize,
    height: usize,
    out: PathBuf,
    sliders: [i64; 4],
    crf: u32,
}

fn parse_args() -> Result<Args, String> {
    let mut kernel: Option<PathBuf> = None;
    let mut seconds = 6.0_f64;
    let mut fps = 60u32;
    let mut width = 960usize;
    let mut height = 540usize;
    let mut out: Option<PathBuf> = None;
    let mut sliders = [500i64; 4];
    let mut crf = 18u32;

    let argv: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--kernel" => {
                kernel = Some(PathBuf::from(argv.get(i + 1).ok_or("--kernel needs a value")?));
                i += 2;
            }
            "--seconds" => {
                seconds = argv
                    .get(i + 1)
                    .ok_or("--seconds needs a value")?
                    .parse()
                    .map_err(|e| format!("bad --seconds: {e}"))?;
                i += 2;
            }
            "--fps" => {
                fps = argv
                    .get(i + 1)
                    .ok_or("--fps needs a value")?
                    .parse()
                    .map_err(|e| format!("bad --fps: {e}"))?;
                i += 2;
            }
            "--size" => {
                let v = argv.get(i + 1).ok_or("--size needs WxH")?;
                let (w, h) = v.split_once('x').ok_or("--size must be WxH")?;
                width = w.parse().map_err(|e| format!("bad width: {e}"))?;
                height = h.parse().map_err(|e| format!("bad height: {e}"))?;
                i += 2;
            }
            "--out" => {
                out = Some(PathBuf::from(argv.get(i + 1).ok_or("--out needs a path")?));
                i += 2;
            }
            "--sliders" => {
                let v = argv.get(i + 1).ok_or("--sliders needs a,b,c,d")?;
                let parts: Vec<&str> = v.split(',').collect();
                if parts.len() != 4 {
                    return Err("--sliders needs exactly 4 comma-separated ints".into());
                }
                for (j, p) in parts.iter().enumerate() {
                    sliders[j] = p.parse().map_err(|e| format!("slider {j}: {e}"))?;
                }
                i += 2;
            }
            "--crf" => {
                crf = argv
                    .get(i + 1)
                    .ok_or("--crf needs a value")?
                    .parse()
                    .map_err(|e| format!("bad --crf: {e}"))?;
                i += 2;
            }
            other => return Err(format!("unknown arg: {other}")),
        }
    }

    Ok(Args {
        kernel: kernel.ok_or("--kernel is required")?,
        seconds,
        fps,
        width,
        height,
        out: out.ok_or("--out is required")?,
        sliders,
        crf,
    })
}

fn main() {
    let a = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("record: {e}");
            eprintln!(
                "usage: record --kernel <path> --out <file.mp4> \
                 [--seconds 6] [--fps 60] [--size 960x540] \
                 [--sliders 500,500,500,500] [--crf 18]"
            );
            exit(1);
        }
    };

    if let Some(parent) = a.out.parent()
        && !parent.as_os_str().is_empty()
    {
        let _ = fs::create_dir_all(parent);
    }

    let env = Env::new();
    builtins::install(&env);
    let src = match fs::read_to_string(&a.kernel) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("record: failed to read kernel {}: {e}", a.kernel.display());
            exit(1);
        }
    };
    if let Err(e) = eval_source(&env, &src) {
        eprintln!("record: kernel eval failed: {e}");
        exit(1);
    }
    let render_fn = match env.lookup("render-pixel") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("record: render-pixel missing: {e}");
            exit(1);
        }
    };

    let total_frames = (a.seconds * a.fps as f64).round() as u64;
    eprintln!(
        "record: {} frames @ {}x{} {}fps → {}",
        total_frames,
        a.width,
        a.height,
        a.fps,
        a.out.display()
    );

    let size = format!("{}x{}", a.width, a.height);
    let fps_s = a.fps.to_string();
    let crf_s = a.crf.to_string();
    let out_s = a.out.to_string_lossy().to_string();
    let mut ffmpeg = match Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel", "warning",
            "-f", "rawvideo",
            "-pix_fmt", "rgba",
            "-s", &size,
            "-r", &fps_s,
            "-i", "-",
            "-c:v", "libx264",
            "-pix_fmt", "yuv420p",
            "-crf", &crf_s,
            "-movflags", "+faststart",
            &out_s,
        ])
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("record: failed to spawn ffmpeg ({e}). Install ffmpeg and try again.");
            exit(1);
        }
    };
    let mut stdin = ffmpeg.stdin.take().expect("ffmpeg stdin");

    let mut buffer = vec![0u8; a.width * a.height * 4];
    let frame_dur_ms = 1000.0 / a.fps as f64;
    let start = std::time::Instant::now();

    for frame in 0..total_frames {
        let frame_i = frame as i64;
        let t_ms = (frame as f64 * frame_dur_ms).round() as i64;
        let sliders = a.sliders;
        let width = a.width;
        let render = &render_fn;

        buffer.par_chunks_mut(width * 4).enumerate().for_each(|(y, row)| {
            let mut args: [Value; 8] = [
                Value::Int(0),
                Value::Int(y as i64),
                Value::Int(frame_i),
                Value::Int(t_ms),
                Value::Int(sliders[0]),
                Value::Int(sliders[1]),
                Value::Int(sliders[2]),
                Value::Int(sliders[3]),
            ];
            for x in 0..width {
                args[0] = Value::Int(x as i64);
                match eval::apply(render, &args) {
                    Ok(Value::Int(c)) => {
                        let c = c as u32;
                        row[x * 4] = ((c >> 16) & 0xff) as u8;
                        row[x * 4 + 1] = ((c >> 8) & 0xff) as u8;
                        row[x * 4 + 2] = (c & 0xff) as u8;
                        row[x * 4 + 3] = 0xff;
                    }
                    _ => {
                        row[x * 4] = 0;
                        row[x * 4 + 1] = 0;
                        row[x * 4 + 2] = 0;
                        row[x * 4 + 3] = 0xff;
                    }
                }
            }
        });

        if let Err(e) = stdin.write_all(&buffer) {
            eprintln!("record: ffmpeg pipe closed at frame {frame}: {e}");
            break;
        }

        if frame % a.fps as u64 == 0 {
            let pct = 100.0 * frame as f64 / total_frames.max(1) as f64;
            eprintln!("  frame {frame}/{total_frames} ({pct:.0}%)");
        }
    }

    drop(stdin);
    let status = ffmpeg.wait().expect("ffmpeg wait");
    let elapsed = start.elapsed().as_secs_f64();
    if !status.success() {
        eprintln!("record: ffmpeg exited {status}");
        exit(1);
    }
    eprintln!(
        "record: done in {elapsed:.1}s ({:.1} fps render)",
        total_frames as f64 / elapsed
    );
}

fn eval_source(env: &Env, src: &str) -> Result<(), String> {
    let forms = reader::read_all(src).map_err(|e| format!("parse: {e}"))?;
    for f in forms {
        eval::eval(&f, env).map_err(|e| format!("eval: {e}"))?;
    }
    Ok(())
}
