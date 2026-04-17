//! Live-coded CPU kernel demo with sliders.
//!
//! eframe/egui for the UI shell; rayon for per-frame pixel parallelism;
//! MLIR-JIT-compiled cljrs `defn-native` kernels for the pixel work.
//!
//! Four parameter sliders are passed to the kernel as i64 values in the
//! range 0..1000; the cljrs kernel converts to floats with whatever
//! semantic it wants (zoom, iter count, color shift, etc.).
//!
//! The kernel signature:
//!   (defn-native render-pixel ^i64
//!     [^i64 px ^i64 py ^i64 frame ^i64 t-ms
//!      ^i64 s0 ^i64 s1 ^i64 s2 ^i64 s3] ...)
//!
//! File-watcher still rebuilds the JIT whenever the .clj source changes.
//!
//! Usage:
//!   cargo run --release --features demo --bin demo -- demo/fractal.clj
//!   cargo run --release --features demo --bin demo -- demo/raymarch.clj --headless 60

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::time::{Instant, SystemTime};

use cljrs::{builtins, env::Env, eval, reader, value::Value};
use eframe::egui;
use rayon::prelude::*;

const WIDTH: usize = 960;
const HEIGHT: usize = 540;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: demo <kernel.clj> [--headless N]");
        process::exit(1);
    }
    let path = PathBuf::from(&args[1]);
    let headless_frames: Option<u64> = args
        .iter()
        .position(|a| a == "--headless")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok());

    let env = Env::new();
    builtins::install(&env);
    let last_mtime = load_file(&env, &path);

    if let Some(frames) = headless_frames {
        run_headless(&env, frames);
        return;
    }

    let app = DemoApp {
        env,
        path,
        last_mtime,
        frame: 0,
        started: Instant::now(),
        sliders: [500, 500, 500, 500],
        slider_names: [
            String::from("s0 (param 0)"),
            String::from("s1 (param 1)"),
            String::from("s2 (param 2)"),
            String::from("s3 (param 3)"),
        ],
        buffer: vec![0u8; WIDTH * HEIGHT * 4],
        texture: None,
        last_frame_ms: 0.0,
        last_error: None,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WIDTH as f32 + 280.0, HEIGHT as f32 + 60.0])
            .with_title("cljrs — live-coded CPU kernel"),
        ..Default::default()
    };

    eframe::run_native(
        "cljrs demo",
        options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .expect("eframe run_native failed");
}

struct DemoApp {
    env: Env,
    path: PathBuf,
    last_mtime: Option<SystemTime>,
    frame: u64,
    started: Instant,
    sliders: [i64; 4],
    slider_names: [String; 4],
    buffer: Vec<u8>, // RGBA8 for egui ColorImage
    texture: Option<egui::TextureHandle>,
    last_frame_ms: f64,
    last_error: Option<String>,
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Hot-reload on file mtime change.
        let current_mtime = mtime(&self.path);
        if current_mtime != self.last_mtime {
            self.last_mtime = load_file(&self.env, &self.path);
            self.last_error = None;
        }

        // Read slider-name hints from env globals if the kernel exported them.
        // Convention: if (def slider-N-label "...") exists, use that. Otherwise
        // fall back to "sN (param N)".
        for i in 0..4 {
            let key = format!("slider-{i}-label");
            if let Ok(Value::Str(s)) = self.env.lookup(&key) {
                self.slider_names[i] = s.to_string();
            }
        }

        // Right-side panel: sliders + perf HUD.
        egui::SidePanel::right("params")
            .min_width(260.0)
            .show(ctx, |ui| {
                ui.heading("cljrs kernel params");
                ui.separator();
                for i in 0..4 {
                    ui.label(&self.slider_names[i]);
                    let mut v = self.sliders[i] as i32;
                    ui.add(egui::Slider::new(&mut v, 0..=1000).show_value(true));
                    self.sliders[i] = v as i64;
                }
                ui.separator();
                ui.label(format!(
                    "last frame: {:.2} ms ({:.0} fps)",
                    self.last_frame_ms,
                    1000.0 / self.last_frame_ms.max(0.001),
                ));
                ui.label(format!("frame #{}", self.frame));
                ui.separator();
                ui.label("Edit the .clj file — the window");
                ui.label("picks up the save within a frame.");
                if let Some(err) = &self.last_error {
                    ui.separator();
                    ui.colored_label(egui::Color32::LIGHT_RED, "kernel error:");
                    ui.label(err);
                }
            });

        // Central panel: the rendered image.
        egui::CentralPanel::default().show(ctx, |ui| {
            let render_start = Instant::now();

            let render_fn = match self.env.lookup("render-pixel") {
                Ok(f) => Some(f),
                Err(e) => {
                    self.last_error = Some(format!("render-pixel missing: {e}"));
                    None
                }
            };

            if let Some(render_fn) = render_fn {
                let t_ms = self.started.elapsed().as_millis() as i64;
                let frame_i = self.frame as i64;
                let sliders = self.sliders;

                // Probe one pixel before spawning parallel work. Catches
                // signature mismatches, unbound symbols in the kernel,
                // non-int returns, etc., surfaces a readable error in the
                // UI, and avoids producing a silent black frame.
                let probe_args: [Value; 8] = [
                    Value::Int((WIDTH / 2) as i64),
                    Value::Int((HEIGHT / 2) as i64),
                    Value::Int(frame_i),
                    Value::Int(t_ms),
                    Value::Int(sliders[0]),
                    Value::Int(sliders[1]),
                    Value::Int(sliders[2]),
                    Value::Int(sliders[3]),
                ];
                let probe = eval::apply(&render_fn, &probe_args);
                let ok = match &probe {
                    Ok(Value::Int(_)) => {
                        self.last_error = None;
                        true
                    }
                    Ok(other) => {
                        self.last_error = Some(format!(
                            "render-pixel must return an i64 packed as 0xRRGGBB; got {} ({})",
                            other,
                            other.type_name()
                        ));
                        false
                    }
                    Err(e) => {
                        self.last_error = Some(e.to_string());
                        false
                    }
                };

                if ok {
                    self.buffer.par_chunks_mut(WIDTH * 4).enumerate().for_each(
                        |(y, row)| {
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
                            for x in 0..WIDTH {
                                args[0] = Value::Int(x as i64);
                                if let Ok(Value::Int(c)) = eval::apply(&render_fn, &args) {
                                    let c = c as u32;
                                    row[x * 4] = ((c >> 16) & 0xff) as u8;
                                    row[x * 4 + 1] = ((c >> 8) & 0xff) as u8;
                                    row[x * 4 + 2] = (c & 0xff) as u8;
                                    row[x * 4 + 3] = 0xff;
                                } else {
                                    // Shouldn't happen after probe, but blank
                                    // the pixel rather than leaving stale data.
                                    row[x * 4] = 0;
                                    row[x * 4 + 1] = 0;
                                    row[x * 4 + 2] = 0;
                                    row[x * 4 + 3] = 0xff;
                                }
                            }
                        },
                    );
                } else {
                    // Error frame — fill with a dark-red diagonal stripe so
                    // it's visually obvious the kernel isn't running.
                    for y in 0..HEIGHT {
                        for x in 0..WIDTH {
                            let on = ((x + y) / 8) % 2 == 0;
                            let i = (y * WIDTH + x) * 4;
                            self.buffer[i] = if on { 80 } else { 30 };
                            self.buffer[i + 1] = 0;
                            self.buffer[i + 2] = 0;
                            self.buffer[i + 3] = 0xff;
                        }
                    }
                }
            }

            self.last_frame_ms = render_start.elapsed().as_secs_f64() * 1000.0;

            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [WIDTH, HEIGHT],
                &self.buffer,
            );
            let texture = match &mut self.texture {
                Some(t) => {
                    t.set(color_image, egui::TextureOptions::NEAREST);
                    t.clone()
                }
                None => {
                    let t = ctx.load_texture(
                        "cljrs-frame",
                        color_image,
                        egui::TextureOptions::NEAREST,
                    );
                    self.texture = Some(t.clone());
                    t
                }
            };
            ui.image((texture.id(), egui::vec2(WIDTH as f32, HEIGHT as f32)));
        });

        self.frame += 1;
        ctx.request_repaint();
    }
}

fn mtime(path: &std::path::Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn load_file(env: &Env, path: &std::path::Path) -> Option<SystemTime> {
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

fn run_headless(env: &Env, frames: u64) {
    let render_fn = env
        .lookup("render-pixel")
        .expect("render-pixel not defined");
    let mut buffer = vec![0u8; WIDTH * HEIGHT * 4];
    let sliders: [i64; 4] = [500, 500, 500, 500];
    let start = Instant::now();
    for frame in 0..frames {
        let frame_i = frame as i64;
        let t_ms = (frame as i64) * 16;
        buffer.par_chunks_mut(WIDTH * 4).enumerate().for_each(
            |(y, row)| {
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
                for x in 0..WIDTH {
                    args[0] = Value::Int(x as i64);
                    match eval::apply(&render_fn, &args) {
                        Ok(Value::Int(c)) => {
                            let c = c as u32;
                            row[x * 4] = ((c >> 16) & 0xff) as u8;
                            row[x * 4 + 1] = ((c >> 8) & 0xff) as u8;
                            row[x * 4 + 2] = (c & 0xff) as u8;
                            row[x * 4 + 3] = 0xff;
                        }
                        _ => {
                            row[x * 4 + 3] = 0xff;
                        }
                    }
                }
            },
        );
    }
    let elapsed = start.elapsed();
    let per_ms = elapsed.as_secs_f64() * 1000.0 / frames as f64;
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in buffer.iter() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    eprintln!(
        "[demo headless] {frames} frames, total={:.2}ms, per-frame={:.2}ms ({:.1} fps), buffer-hash={:#018x}",
        elapsed.as_secs_f64() * 1000.0,
        per_ms,
        1000.0 / per_ms,
        hash
    );
}
