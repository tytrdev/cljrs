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
        editor_open: true,
        paused: false,
        scrub_t_ms: 0,
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
    /// Whether the left editor panel is visible. Toggle with Cmd/Ctrl-E
    /// or the "< hide editor" / "show editor >" button.
    editor_open: bool,
    /// Bret-Victor-style time scrubbing. When paused, the render clock
    /// doesn't advance from `Instant::now()`; it reads from `scrub_t_ms`
    /// so the user can rewind or hold on a specific frame.
    paused: bool,
    /// The clock value shown to the kernel. When unpaused this is
    /// `started.elapsed().as_millis()`; when paused it's whatever the
    /// scrub slider is set to.
    scrub_t_ms: i64,
}

impl DemoApp {
    /// Flip the pause state. On pause, freeze the clock at its current
    /// reading; on resume, adjust `started` so `elapsed()` picks up
    /// exactly where we paused (no jump forward).
    fn toggle_paused(&mut self) {
        if self.paused {
            // Resuming — shift `started` back so elapsed() yields scrub_t_ms right now.
            if let Some(new_started) = Instant::now()
                .checked_sub(Duration::from_millis(self.scrub_t_ms.max(0) as u64))
            {
                self.started = new_started;
            }
            self.paused = false;
        } else {
            // Pausing — freeze at current clock.
            self.scrub_t_ms = self.started.elapsed().as_millis() as i64;
            self.paused = true;
        }
    }
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

        // Cmd/Ctrl-E toggles the editor panel visibility.
        let toggle_editor = ctx.input(|i| {
            i.key_pressed(egui::Key::E)
                && (i.modifiers.command || i.modifiers.ctrl)
        });
        if toggle_editor {
            self.editor_open = !self.editor_open;
        }

        // Space toggles pause/play.
        let toggle_pause = ctx.input(|i| {
            i.key_pressed(egui::Key::Space) && !i.modifiers.any()
        });
        if toggle_pause {
            self.toggle_paused();
        }

        // Read slider-name hints from env globals if the kernel exported them.
        for i in 0..4 {
            let key = format!("slider-{i}-label");
            if let Ok(Value::Str(s)) = self.env.lookup(&key) {
                self.slider_names[i] = s.to_string();
            }
        }

        // Left-side panel: the code editor. Collapsible.
        if self.editor_open {
            egui::SidePanel::left("editor")
                .default_width(460.0)
                .resizable(true)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("◀ hide").clicked() {
                            self.editor_open = false;
                        }
                        ui.heading("editor");
                        ui.separator();
                        if ui.button("save").clicked() {
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
                        "edits re-eval 300ms after last keystroke. \
                         Cmd/Ctrl-S saves. Cmd/Ctrl-E toggles this panel.",
                    ).small().weak());
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // Custom layouter: produce a highlighted
                            // LayoutJob from the current buffer each
                            // frame. Small enough buffers (~100 lines)
                            // that re-scanning per frame is cheap.
                            let mut layouter =
                                |ui: &egui::Ui, src: &str, _wrap_width: f32| {
                                    let mut job = clojure_highlight(src);
                                    job.wrap.max_width = f32::INFINITY;
                                    ui.fonts(|f| f.layout_job(job))
                                };
                            let response = ui.add_sized(
                                [ui.available_width(), ui.available_height()],
                                egui::TextEdit::multiline(&mut self.source)
                                    .font(egui::TextStyle::Monospace)
                                    .code_editor()
                                    .desired_rows(40)
                                    .desired_width(f32::MAX)
                                    .layouter(&mut layouter),
                            );
                            if response.changed() {
                                self.last_edit_at = Instant::now();
                            }
                        });
                });
        } else {
            // A slim collapsed strip with a "show editor" button so the
            // UI is obviously recoverable.
            egui::SidePanel::left("editor-toggle")
                .exact_width(42.0)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        if ui
                            .button("▶")
                            .on_hover_text("show editor (Cmd/Ctrl-E)")
                            .clicked()
                        {
                            self.editor_open = true;
                        }
                    });
                });
        }

        // Right-side panel: sliders + perf HUD + time controls.
        egui::SidePanel::right("params")
            .min_width(280.0)
            .show(ctx, |ui| {
                ui.heading("kernel params");
                ui.separator();
                for i in 0..4 {
                    ui.label(&self.slider_names[i]);
                    let mut v = self.sliders[i] as i32;
                    ui.add(egui::Slider::new(&mut v, 0..=1000).show_value(true));
                    self.sliders[i] = v as i64;
                }

                ui.separator();
                ui.heading("time");

                // Current clock reading — either live or paused.
                let live_t = self.started.elapsed().as_millis() as i64;
                let effective_t = if self.paused { self.scrub_t_ms } else { live_t };

                ui.horizontal(|ui| {
                    let btn_label = if self.paused { "▶ play (space)" } else { "⏸ pause (space)" };
                    if ui.button(btn_label).clicked() {
                        self.toggle_paused();
                    }
                    if ui.button("⟲ reset").clicked() {
                        self.started = Instant::now();
                        self.scrub_t_ms = 0;
                    }
                });

                // Scrub slider — only meaningful while paused, but show
                // current t even while running.
                let mut scrub = effective_t as i32;
                // Upper bound grows with observed elapsed so the user
                // can scrub all the way back to 0 but also forward a bit.
                let max_t = (live_t.max(self.scrub_t_ms).max(10_000)) as i32;
                let resp = ui.add_enabled(
                    self.paused,
                    egui::Slider::new(&mut scrub, 0..=max_t)
                        .text("t (ms)")
                        .clamping(egui::SliderClamping::Always),
                );
                if resp.changed() {
                    self.scrub_t_ms = scrub as i64;
                }

                ui.separator();
                ui.label(format!(
                    "last frame: {:.2} ms ({:.0} fps)",
                    self.last_frame_ms,
                    1000.0 / self.last_frame_ms.max(0.001),
                ));
                ui.label(format!("frame #{}", self.frame));
                ui.label(format!("t = {} ms", effective_t));

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
                // If the user has scrubbed/paused, the kernel sees
                // scrub_t_ms instead of the wall clock.
                let t_ms = if self.paused {
                    self.scrub_t_ms
                } else {
                    self.started.elapsed().as_millis() as i64
                };
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

/// Lightweight Clojure syntax highlighter. Walks the buffer once, produces
/// an egui::text::LayoutJob with color-coded runs. Not a real parser —
/// just enough lexer state to tell comments, strings, keywords, numbers,
/// and special forms from everything else.
fn clojure_highlight(src: &str) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    use egui::{Color32, FontId};

    let mut job = LayoutJob::default();
    let font = FontId::monospace(13.0);

    // Palette — tuned for dark UI; lightish accents on a dark background.
    const DEFAULT_FG: Color32 = Color32::from_rgb(220, 220, 220);
    const COMMENT: Color32 = Color32::from_rgb(110, 110, 110);
    const STRING: Color32 = Color32::from_rgb(220, 180, 120);
    const NUMBER: Color32 = Color32::from_rgb(140, 200, 255);
    const KEYWORD: Color32 = Color32::from_rgb(150, 220, 150);
    const SPECIAL: Color32 = Color32::from_rgb(230, 140, 230);
    const FN_NAME: Color32 = Color32::from_rgb(240, 200, 100);
    const PAREN: Color32 = Color32::from_rgb(140, 140, 140);

    let specials: &[&str] = &[
        "defn", "defn-native", "defmacro", "def", "fn", "let", "loop",
        "recur", "if", "do", "quote", "ns", "require", "load-file",
        "in-ns", "macroexpand", "macroexpand-1",
    ];

    let fmt = |color: Color32| TextFormat {
        font_id: font.clone(),
        color,
        ..Default::default()
    };

    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // Comments: ; to end of line.
        if b == b';' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            job.append(&src[start..i], 0.0, fmt(COMMENT));
            continue;
        }
        // Strings: "..."
        if b == b'"' {
            let start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            job.append(&src[start..i], 0.0, fmt(STRING));
            continue;
        }
        // Keywords: :foo
        if b == b':' {
            let start = i;
            i += 1;
            while i < bytes.len() && is_sym_byte(bytes[i]) {
                i += 1;
            }
            job.append(&src[start..i], 0.0, fmt(KEYWORD));
            continue;
        }
        // Numbers: optional sign + digits (+ optional dot/digits/e).
        if b.is_ascii_digit()
            || ((b == b'-' || b == b'+')
                && i + 1 < bytes.len()
                && bytes[i + 1].is_ascii_digit())
        {
            let start = i;
            if b == b'-' || b == b'+' {
                i += 1;
            }
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            // Exponent like 1e-3
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                i += 1;
                if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                    i += 1;
                }
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
            }
            job.append(&src[start..i], 0.0, fmt(NUMBER));
            continue;
        }
        // Parens.
        if matches!(b, b'(' | b')' | b'[' | b']' | b'{' | b'}') {
            job.append(&src[i..i + 1], 0.0, fmt(PAREN));
            i += 1;
            continue;
        }
        // Symbol / identifier.
        if is_sym_byte(b) {
            let start = i;
            while i < bytes.len() && is_sym_byte(bytes[i]) {
                i += 1;
            }
            let word = &src[start..i];
            let color = if specials.contains(&word) {
                SPECIAL
            } else if is_probable_fn_name(src, start) {
                FN_NAME
            } else {
                DEFAULT_FG
            };
            job.append(word, 0.0, fmt(color));
            continue;
        }
        // Whitespace / everything else passes through.
        let mut j = i;
        while j < bytes.len() && !is_highlight_start(bytes[j]) {
            j += 1;
        }
        job.append(&src[i..j], 0.0, fmt(DEFAULT_FG));
        i = j;
    }

    job
}

fn is_sym_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'_' | b'-' | b'+' | b'*' | b'/' | b'<' | b'>' | b'=' | b'!' | b'?' | b'.' | b'&' | b'\''
        )
}

fn is_highlight_start(b: u8) -> bool {
    b == b';'
        || b == b'"'
        || b == b':'
        || b.is_ascii_digit()
        || matches!(b, b'(' | b')' | b'[' | b']' | b'{' | b'}')
        || is_sym_byte(b)
}

/// Heuristic: a symbol that follows an open paren and whitespace
/// (possibly after `defn` etc.) looks like a fn being called. Light
/// coloring just for the word right after `(`. Good enough for syntax
/// highlighting without a real parser.
fn is_probable_fn_name(src: &str, start: usize) -> bool {
    let bytes = src.as_bytes();
    let mut j = start;
    while j > 0 && matches!(bytes[j - 1], b' ' | b'\t' | b'\n') {
        j -= 1;
    }
    j > 0 && bytes[j - 1] == b'('
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
