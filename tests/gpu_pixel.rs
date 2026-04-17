//! Pixel-kernel end-to-end: compile each demo_gpu/*.clj via defn-gpu-pixel,
//! render one frame at modest resolution, verify non-zero pixel output.
//! This is the "do the demos actually run on GPU" check.

#![cfg(feature = "gpu")]

use std::fs;

use cljrs::{
    builtins,
    env::Env,
    eval, reader,
    gpu::{global_gpu, PixelParams},
    value::Value,
};

fn skip_if_no_gpu() -> bool {
    match cljrs::gpu::Gpu::new() {
        Ok(_) => false,
        Err(e) => {
            eprintln!("skipping: {e}");
            true
        }
    }
}

fn compile_and_render(src: &str) -> Vec<u32> {
    let env = Env::new();
    builtins::install(&env);
    for f in reader::read_all(src).expect("read") {
        eval::eval(&f, &env).expect("eval");
    }
    let k = match env.lookup("render").expect("render") {
        Value::GpuPixelKernel(k) => k,
        _ => panic!("render is not a gpu pixel kernel"),
    };
    let gpu = global_gpu().expect("gpu");
    k.render_frame(
        &gpu,
        PixelParams {
            width: 128,
            height: 72,
            t_ms: 500,
            s0: 500,
            s1: 500,
            s2: 500,
            s3: 500,
            _pad: 0,
        },
    )
    .expect("render")
}

#[test]
fn plasma_kernel_renders() {
    if skip_if_no_gpu() { return; }
    let src = fs::read_to_string("demo_gpu/plasma.clj").expect("read plasma.clj");
    let buf = compile_and_render(&src);
    // Every pixel should have nonzero color (plasma is always vivid).
    let nonzero = buf.iter().filter(|&&c| (c & 0xffffff) != 0).count();
    assert!(
        nonzero > buf.len() / 2,
        "expected most pixels nonzero, got {nonzero}/{}",
        buf.len()
    );
}

#[test]
fn waves_kernel_renders() {
    if skip_if_no_gpu() { return; }
    let src = fs::read_to_string("demo_gpu/waves.clj").expect("read waves.clj");
    let buf = compile_and_render(&src);
    assert!(buf.iter().any(|&c| c != 0));
}

#[test]
fn mandelbrot_kernel_renders() {
    if skip_if_no_gpu() { return; }
    let src = fs::read_to_string("demo_gpu/mandelbrot.clj").expect("read mandelbrot.clj");
    let buf = compile_and_render(&src);
    assert!(buf.iter().any(|&c| c != 0));
}
