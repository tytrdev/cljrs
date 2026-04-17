//! Live-coded GPU demo. Loads a cljrs file that defines a
//! `(defn-gpu-pixel render ...)` kernel, dispatches it via wgpu every
//! frame, and blits the result into an eframe window.
//!
//! Usage:
//!   cargo run --release --features gpu-demo --bin gpu-demo -- demo_gpu/plasma.clj
//!
//! File-watching (recompile on save) is kept simple for v0: the window
//! checks mtime each frame and re-evals if changed. That's enough for
//! the Bret-Victor loop.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::Arc;
use std::time::Instant;

use cljrs::{
    builtins,
    env::Env as CljEnv,
    eval, reader,
    gpu::{global_gpu, Gpu, GpuPixelKernel, PixelParams},
    value::Value,
};
use eframe::egui;

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: gpu-demo <kernel.clj>");
        process::exit(1);
    }
    let path = PathBuf::from(&args[1]);
    let env = CljEnv::new();
    builtins::install(&env);

    let src = fs::read_to_string(&path).unwrap_or_default();
    if let Err(e) = eval_source(&env, &src) {
        eprintln!("[gpu-demo] initial eval: {e}");
    }

    // Warm the GPU up-front so the first frame isn't a stall.
    let gpu: Arc<Gpu> = match global_gpu() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("[gpu-demo] no GPU: {e}");
            process::exit(1);
        }
    };
    eprintln!(
        "[gpu-demo] adapter: {} ({:?}, {:?})",
        gpu.adapter_info.name, gpu.adapter_info.device_type, gpu.adapter_info.backend
    );

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WIDTH as f32 + 240.0, HEIGHT as f32 + 40.0]),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "cljrs — GPU demo",
        opts,
        Box::new(move |_cc| {
            Ok(Box::new(GpuDemo {
                env,
                path,
                gpu,
                started: Instant::now(),
                frame: 0,
                sliders: [500, 500, 500, 500],
                rgba: vec![0u8; (WIDTH * HEIGHT * 4) as usize],
                texture: None,
                last_error: None,
                last_mtime: None,
                last_frame_ms: 0.0,
            }))
        }),
    );
}

struct GpuDemo {
    env: CljEnv,
    path: PathBuf,
    gpu: Arc<Gpu>,
    started: Instant,
    frame: u64,
    sliders: [i32; 4],
    rgba: Vec<u8>,
    texture: Option<egui::TextureHandle>,
    last_error: Option<String>,
    last_mtime: Option<std::time::SystemTime>,
    last_frame_ms: f64,
}

impl eframe::App for GpuDemo {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Hot-reload: re-eval the file if its mtime changed.
        if let Ok(meta) = fs::metadata(&self.path)
            && let Ok(mtime) = meta.modified()
            && Some(mtime) != self.last_mtime
        {
            self.last_mtime = Some(mtime);
            if let Ok(src) = fs::read_to_string(&self.path)
                && let Err(e) = eval_source(&self.env, &src)
            {
                self.last_error = Some(e);
            } else {
                self.last_error = None;
            }
        }

        // Right panel: sliders + stats.
        egui::SidePanel::right("ctrl").show(ctx, |ui| {
            ui.heading("GPU");
            ui.label(format!(
                "{} · {:?}",
                self.gpu.adapter_info.name, self.gpu.adapter_info.backend
            ));
            ui.separator();
            ui.heading("Sliders (0–1000)");
            for (i, v) in self.sliders.iter_mut().enumerate() {
                ui.add(egui::Slider::new(v, 0..=1000).text(format!("s{i}")));
            }
            ui.separator();
            ui.label(format!(
                "frame: {:.2} ms · {:.0} fps",
                self.last_frame_ms,
                1000.0 / self.last_frame_ms.max(0.001)
            ));
            ui.label(format!("frame #{}", self.frame));
            if let Some(err) = &self.last_error {
                ui.separator();
                ui.colored_label(egui::Color32::LIGHT_RED, "kernel error:");
                ui.label(err);
            }
        });

        // Render the frame.
        egui::CentralPanel::default().show(ctx, |ui| {
            let start = Instant::now();
            let kernel = match self.env.lookup("render") {
                Ok(Value::GpuPixelKernel(k)) => Some(k),
                Ok(other) => {
                    self.last_error = Some(format!(
                        "`render` is {}, expected gpu-pixel-kernel (use defn-gpu-pixel)",
                        other.type_name()
                    ));
                    None
                }
                Err(e) => {
                    self.last_error = Some(format!("no `render` defined: {e}"));
                    None
                }
            };
            if let Some(kernel) = kernel {
                let params = PixelParams {
                    width: WIDTH,
                    height: HEIGHT,
                    t_ms: self.started.elapsed().as_millis() as i32,
                    s0: self.sliders[0],
                    s1: self.sliders[1],
                    s2: self.sliders[2],
                    s3: self.sliders[3],
                    _pad: 0,
                };
                match kernel.render_frame(&self.gpu, params) {
                    Ok(buf) => {
                        unpack_rgba(&buf, &mut self.rgba);
                        self.last_error = None;
                    }
                    Err(e) => self.last_error = Some(e),
                }
            }
            let img = egui::ColorImage::from_rgba_unmultiplied(
                [WIDTH as usize, HEIGHT as usize],
                &self.rgba,
            );
            let tex = self.texture.get_or_insert_with(|| {
                ui.ctx()
                    .load_texture("gpu-frame", img.clone(), egui::TextureOptions::LINEAR)
            });
            tex.set(img, egui::TextureOptions::LINEAR);
            ui.image((tex.id(), egui::vec2(WIDTH as f32, HEIGHT as f32)));
            self.last_frame_ms = start.elapsed().as_secs_f64() * 1000.0;
            self.frame += 1;
        });

        ctx.request_repaint();
    }
}

/// Unpack u32[w*h] (0x00RRGGBB) into a u8[w*h*4] RGBA.
fn unpack_rgba(buf: &[u32], out: &mut [u8]) {
    for (i, &c) in buf.iter().enumerate() {
        let o = i * 4;
        out[o] = ((c >> 16) & 0xff) as u8;
        out[o + 1] = ((c >> 8) & 0xff) as u8;
        out[o + 2] = (c & 0xff) as u8;
        out[o + 3] = 0xff;
    }
}

fn eval_source(env: &CljEnv, src: &str) -> Result<(), String> {
    let forms = reader::read_all(src).map_err(|e| format!("parse: {e}"))?;
    for f in forms {
        eval::eval(&f, env).map_err(|e| format!("eval: {e}"))?;
    }
    Ok(())
}
