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
use std::time::{Duration, Instant};

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
    let initial_source = fs::read_to_string(&path).unwrap_or_default();
    // Seed the env from the initial source so the very first frame renders.
    if let Err(e) = eval_source(&env, &initial_source) {
        eprintln!("[demo] initial eval: {e}");
    }

    if let Some(frames) = headless_frames {
        run_headless(&env, frames);
        return;
    }

    let app = DemoApp {
        env,
        path,
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
        source: initial_source.clone(),
        last_evaled_source: initial_source,
        last_edit_at: Instant::now(),
        save_flash: None,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WIDTH as f32 + 720.0, HEIGHT as f32 + 60.0])
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
    frame: u64,
    started: Instant,
    sliders: [i64; 4],
    slider_names: [String; 4],
    buffer: Vec<u8>, // RGBA8 for egui ColorImage
    texture: Option<egui::TextureHandle>,
    last_frame_ms: f64,
    last_error: Option<String>,
    /// Current editor buffer. Mutated by the TextEdit.
    source: String,
    /// Last buffer content that was fed to the evaluator. We re-evaluate
    /// when `source != last_evaled_source` AND the user has paused typing.
    last_evaled_source: String,
    /// Most recent edit time — used to debounce re-evaluation so we don't
    /// try to compile after every keystroke.
    last_edit_at: Instant,
    /// Transient "saved!" toast shown next to the save button.
    save_flash: Option<Instant>,
}

impl eframe::App for DemoApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Debounced re-eval of the editor buffer. We rebuild the env from
        // scratch each time so stale defn-native JIT'd fns are released
        // and the new kernel takes over cleanly.
        let debounce_elapsed = self.last_edit_at.elapsed() > Duration::from_millis(300);
        if self.source != self.last_evaled_source && debounce_elapsed {
            let fresh = Env::new();
            builtins::install(&fresh);
            match eval_source(&fresh, &self.source) {
                Ok(()) => {
                    self.env = fresh;
                    self.last_error = None;
                    self.last_evaled_source = self.source.clone();
                }
                Err(msg) => {
                    // Keep the old env running so the render doesn't flash.
                    self.last_error = Some(msg);
                    self.last_evaled_source = self.source.clone();
                }
            }
        }

        // Ctrl+S / Cmd+S saves the editor buffer to the file on disk.
        let save_pressed = ctx.input(|i| {
            i.key_pressed(egui::Key::S)
                && (i.modifiers.command || i.modifiers.ctrl)
        });
        if save_pressed {
            if fs::write(&self.path, &self.source).is_ok() {
                self.save_flash = Some(Instant::now());
            }
        }

        // Read slider-name hints from env globals if the kernel exported them.
        for i in 0..4 {
            let key = format!("slider-{i}-label");
            if let Ok(Value::Str(s)) = self.env.lookup(&key) {
                self.slider_names[i] = s.to_string();
            }
        }

        // Left-side panel: the code editor.
        egui::SidePanel::left("editor")
            .default_width(460.0)
            .resizable(true)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("editor");
                    ui.separator();
                    if ui.button("save to disk").clicked() {
                        if fs::write(&self.path, &self.source).is_ok() {
                            self.save_flash = Some(Instant::now());
                        }
                    }
                    if let Some(when) = self.save_flash {
                        if when.elapsed() < Duration::from_millis(1500) {
                            ui.colored_label(egui::Color32::LIGHT_GREEN, "✓ saved");
                        } else {
                            self.save_flash = None;
                        }
                    }
                });
                ui.label(format!("file: {}", self.path.display()));
                ui.label(egui::RichText::new(
                    "edits re-eval 300ms after last keystroke. Cmd/Ctrl-S saves.",
                ).small().weak());
                ui.separator();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let response = ui.add_sized(
                            [ui.available_width(), ui.available_height()],
                            egui::TextEdit::multiline(&mut self.source)
                                .code_editor()
                                .desired_rows(40)
                                .desired_width(f32::MAX),
                        );
                        if response.changed() {
                            self.last_edit_at = Instant::now();
                        }
                    });
            });

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

/// Parse + evaluate a full source string into the given env. Returns
/// Ok(()) on full success, or Err(message) with the first parse/eval
/// error — the error is surfaced to the UI.
fn eval_source(env: &Env, src: &str) -> std::result::Result<(), String> {
    let forms = reader::read_all(src).map_err(|e| format!("parse: {e}"))?;
    for f in forms {
        eval::eval(&f, env).map_err(|e| format!("eval: {e}"))?;
    }
    Ok(())
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
