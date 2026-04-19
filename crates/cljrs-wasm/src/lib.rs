//! wasm-bindgen wrapper around the cljrs tree-walker.
//!
//! Two entry points for the docs-site live REPL:
//!   - `eval_source(src)` — fresh env per call. Side-effect-free snippets.
//!   - `Repl` — persistent env across calls, for a real REPL session.
//!
//! The `mlir` feature is not compiled in this crate (melior links native
//! LLVM, which cannot target wasm32). defn-native falls back to the
//! tree-walker — slower but correct.

use cljrs::{builtins, env::Env, eval, reader};
use wasm_bindgen::prelude::*;

mod js_bridge;
mod ui_bridge;

#[wasm_bindgen(start)]
pub fn _start() {
    console_error_panic_hook::set_once();
}

/// Evaluate a full source string in a fresh env. Returns the pr-str of
/// the last form's value, or an error message.
#[wasm_bindgen]
pub fn eval_source(src: &str) -> String {
    let env = Env::new();
    builtins::install(&env);
    cljrs_physics::install(&env);
    cljrs_ml::install(&env);
    js_bridge::install(&env);
    ui_bridge::install(&env);
    eval_in(&env, src)
}

/// Stateful REPL: persistent env across calls. Lets the docs-site keep
/// `def`s alive across cells.
#[wasm_bindgen]
pub struct Repl {
    env: Env,
}

#[wasm_bindgen]
impl Repl {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Repl {
        let env = Env::new();
        builtins::install(&env);
        cljrs_physics::install(&env);
        cljrs_ml::install(&env);
        js_bridge::install(&env);
        ui_bridge::install(&env);
        Repl { env }
    }

    /// Evaluate a source string in this REPL's env. Returns pr-str of the
    /// last value or an error.
    pub fn eval(&self, src: &str) -> String {
        eval_in(&self.env, src)
    }

    /// Evaluate source expecting a flat vector of numbers; return them as
    /// a `Float32Array` for the JS side (fast per-frame state readout in
    /// demos). Non-numeric elements or a non-vector result returns an
    /// empty array. Nested numeric vectors are flattened one level so
    /// `[[x y r] ...]` round-trips.
    pub fn eval_floats(&self, src: &str) -> Vec<f32> {
        use cljrs::value::Value;
        let forms = match reader::read_all(src) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        let mut last = Value::Nil;
        for f in forms {
            match eval::eval(&f, &self.env) {
                Ok(v) => last = v,
                Err(_) => return Vec::new(),
            }
        }
        fn push_num(out: &mut Vec<f32>, v: &Value) -> bool {
            match v {
                Value::Float(f) => {
                    out.push(*f as f32);
                    true
                }
                Value::Int(i) => {
                    out.push(*i as f32);
                    true
                }
                _ => false,
            }
        }
        let mut out: Vec<f32> = Vec::new();
        if let Value::Vector(xs) = &last {
            for x in xs.iter() {
                if push_num(&mut out, x) {
                    continue;
                }
                if let Value::Vector(inner) = x {
                    for y in inner.iter() {
                        push_num(&mut out, y);
                    }
                }
            }
        }
        out
    }

    /// Fast-path for live demos: look up a Clojure fn by name and call
    /// it with pre-built i64 args, returning the result as a flat
    /// Vec<f32> (same shape as `eval_floats`). Skips the reader parse
    /// that dominates per-frame cost in an rAF loop.
    pub fn call_ints(&self, name: &str, args: Vec<i32>) -> Vec<f32> {
        use cljrs::value::Value;
        let f = match self.env.lookup(name) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let arg_vals: Vec<Value> = args.into_iter().map(|i| Value::Int(i as i64)).collect();
        let result = match eval::apply(&f, &arg_vals) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        fn push_num(out: &mut Vec<f32>, v: &Value) -> bool {
            match v {
                Value::Float(f) => { out.push(*f as f32); true }
                Value::Int(i) => { out.push(*i as f32); true }
                _ => false,
            }
        }
        let mut out: Vec<f32> = Vec::new();
        if let Value::Vector(xs) = &result {
            for x in xs.iter() {
                if push_num(&mut out, x) { continue; }
                if let Value::Vector(inner) = x {
                    for y in inner.iter() {
                        push_num(&mut out, y);
                    }
                }
            }
        }
        out
    }

    /// Fast-path for live audio/DSP demos: look up a Clojure fn by name
    /// and call it with pre-built f32 args, returning a flat `Vec<f32>`.
    /// Same flattening contract as `call_ints` / `eval_floats`, but the
    /// inputs are floats — needed by the synth page where buffer-index
    /// and sample-rate are naturally floats and we cannot afford to
    /// reparse the source per audio callback.
    pub fn call_floats(&self, name: &str, args: Vec<f32>) -> Vec<f32> {
        use cljrs::value::Value;
        let f = match self.env.lookup(name) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let arg_vals: Vec<Value> = args.into_iter().map(|x| Value::Float(x as f64)).collect();
        let result = match eval::apply(&f, &arg_vals) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        fn push_num(out: &mut Vec<f32>, v: &Value) -> bool {
            match v {
                Value::Float(f) => { out.push(*f as f32); true }
                Value::Int(i) => { out.push(*i as f32); true }
                _ => false,
            }
        }
        let mut out: Vec<f32> = Vec::new();
        if let Value::Vector(xs) = &result {
            for x in xs.iter() {
                if push_num(&mut out, x) { continue; }
                if let Value::Vector(inner) = x {
                    for y in inner.iter() {
                        push_num(&mut out, y);
                    }
                }
            }
        }
        out
    }

    /// Reset to a fresh prelude-initialized env.
    pub fn reset(&mut self) {
        self.env = Env::new();
        builtins::install(&self.env);
        cljrs_physics::install(&self.env);
        cljrs_ml::install(&self.env);
        js_bridge::install(&self.env);
        ui_bridge::install(&self.env);
    }
}

fn eval_in(env: &Env, src: &str) -> String {
    let forms = match reader::read_all(src) {
        Ok(f) => f,
        Err(e) => return format!("read error: {e}"),
    };
    let mut last = cljrs::value::Value::Nil;
    for f in forms {
        match eval::eval(&f, env) {
            Ok(v) => last = v,
            Err(e) => return format!("eval error: {e}"),
        }
    }
    last.to_pr_string()
}
