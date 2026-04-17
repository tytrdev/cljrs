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
        Repl { env }
    }

    /// Evaluate a source string in this REPL's env. Returns pr-str of the
    /// last value or an error.
    pub fn eval(&self, src: &str) -> String {
        eval_in(&self.env, src)
    }

    /// Reset to a fresh prelude-initialized env.
    pub fn reset(&mut self) {
        self.env = Env::new();
        builtins::install(&self.env);
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
