//! Live-coded fractal demo.
//!
//! Opens a window, watches a `.clj` source file, re-evaluates it whenever
//! the file's mtime changes, and renders by calling a `defn-native`
//! `render-pixel` from cljrs once per pixel per frame.
//!
//! Edit the .clj in any editor; save; the window updates using the new
//! native code — usually in the same frame you saved.
//!
//! Usage:
//!   cargo run --release --features demo --bin demo -- demo/fractal.clj

use std::env;
use std::fs;
use std::path::Path;
use std::process;
use std::time::{Instant, SystemTime};

use cljrs::{builtins, env::Env, eval, reader, value::Value};
use minifb::{Key, Window, WindowOptions};

const WIDTH: usize = 640;
const HEIGHT: usize = 480;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: demo <fractal.clj> [--headless N]");
        process::exit(1);
    }
    let path = args[1].clone();
    let headless_frames: Option<u64> = args
        .iter()
        .position(|a| a == "--headless")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());

    let env = Env::new();
    builtins::install(&env);

    let mut last_mtime = load_file(&env, &path);

    if let Some(frames) = headless_frames {
        // Non-windowed sanity-check path: render N frames, measure, exit.
        // Useful for CI and for verifying render pipeline without a GUI.
        run_headless(&env, frames);
        return;
    }

    eprintln!("[demo] loaded {path}; save the file to hot-reload");

    let mut window = Window::new(
        "cljrs — live-coded fractal (edit the .clj file to hot-reload)",
        WIDTH,
        HEIGHT,
        WindowOptions {
            resize: false,
            ..WindowOptions::default()
        },
    )
    .expect("create window");
    window.set_target_fps(60);

    let mut buffer: Vec<u32> = vec![0; WIDTH * HEIGHT];
    let start = Instant::now();
    let mut frame: u64 = 0;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // Poll for file changes on each frame (cheap: just a stat).
        let new_mtime = mtime(&path);
        if new_mtime != last_mtime {
            eprintln!("[demo] file changed — reloading");
            last_mtime = load_file(&env, &path);
        }

        let render_fn = match env.lookup("render-pixel") {
            Ok(f) => f,
            Err(_) => {
                // File not loaded / has errors. Fill with a fault-pattern.
                for (i, px) in buffer.iter_mut().enumerate() {
                    *px = if (i / WIDTH + i) % 8 < 4 { 0xff0000 } else { 0x000000 };
                }
                window.update_with_buffer(&buffer, WIDTH, HEIGHT).ok();
                continue;
            }
        };

        let t_millis = start.elapsed().as_millis() as i64;
        render_frame(&render_fn, &mut buffer, frame, t_millis);

        window.update_with_buffer(&buffer, WIDTH, HEIGHT).ok();
        frame += 1;
    }
}

fn run_headless(env: &Env, frames: u64) {
    let render_fn = env
        .lookup("render-pixel")
        .expect("render-pixel not defined");
    let mut buffer: Vec<u32> = vec![0; WIDTH * HEIGHT];
    let start = Instant::now();
    for frame in 0..frames {
        render_frame(
            &render_fn,
            &mut buffer,
            frame,
            (frame as i64) * 16, // ~60fps worth of simulated ms
        );
    }
    let elapsed = start.elapsed();
    let per_frame_ms = elapsed.as_secs_f64() * 1000.0 / frames as f64;
    // Hash the buffer as a cheap correctness signal across runs.
    let mut hash: u64 = 0xcbf29ce484222325;
    for &px in buffer.iter() {
        hash ^= px as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    eprintln!(
        "[demo headless] {frames} frames, total={:.2}ms, per-frame={:.2}ms ({:.1} fps), buffer-hash={:#018x}",
        elapsed.as_secs_f64() * 1000.0,
        per_frame_ms,
        1000.0 / per_frame_ms,
        hash
    );
}

fn mtime(path: &str) -> Option<SystemTime> {
    fs::metadata(Path::new(path))
        .and_then(|m| m.modified())
        .ok()
}

fn load_file(env: &Env, path: &str) -> Option<SystemTime> {
    match fs::read_to_string(path) {
        Ok(src) => match reader::read_all(&src) {
            Ok(forms) => {
                for f in forms {
                    if let Err(e) = eval::eval(&f, env) {
                        eprintln!("[demo] eval error: {e}");
                    }
                }
            }
            Err(e) => eprintln!("[demo] parse error: {e}"),
        },
        Err(e) => eprintln!("[demo] read error: {e}"),
    }
    mtime(path)
}

fn render_frame(render_fn: &Value, buffer: &mut [u32], frame: u64, t_millis: i64) {
    // Native call per pixel. The JIT'd fn returns an i64 packed as 0xRRGGBB
    // that minifb interprets directly.
    // Pre-allocate args Vec to avoid re-allocation per call.
    let mut args: [Value; 4] = [
        Value::Int(0),
        Value::Int(0),
        Value::Int(frame as i64),
        Value::Int(t_millis),
    ];
    for y in 0..HEIGHT {
        args[1] = Value::Int(y as i64);
        for x in 0..WIDTH {
            args[0] = Value::Int(x as i64);
            match eval::apply(render_fn, &args) {
                Ok(Value::Int(c)) => buffer[y * WIDTH + x] = c as u32,
                Ok(other) => {
                    eprintln!("[demo] render-pixel returned non-int: {other}");
                    return;
                }
                Err(e) => {
                    eprintln!("[demo] render-pixel error: {e}");
                    return;
                }
            }
        }
    }
}
