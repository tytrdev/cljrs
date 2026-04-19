//! Build-time prerender for cljrs UI pages.
//!
//! Usage: cargo run --bin prerender -- <path-to-cljrs-file>
//!
//! Loads the standard prelude (core + test + music + ui), evals every
//! form in the file, then evals `(prerender)` and prints the result
//! string to stdout. Used by `docs/build-ui.sh` to inject SEO-visible
//! HTML into a page's mount point before any JS runs.
//!
//! Wasm-only builtins (mount!/hydrate!) are absent in the native env;
//! a page's source must guard those behind a check or simply not call
//! them at the top level. `(prerender)` is expected to be pure: it
//! returns a hiccup vector or pre-rendered string.

use std::env;
use std::fs;
use std::process;

use cljrs::{
    builtins,
    env::Env,
    eval, reader,
    value::Value,
};

fn die(msg: impl AsRef<str>) -> ! {
    eprintln!("prerender: {}", msg.as_ref());
    process::exit(1);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: prerender <file.cljrs>");
        process::exit(2);
    }
    let path = &args[1];
    let src = fs::read_to_string(path)
        .unwrap_or_else(|e| die(format!("read {path}: {e}")));

    let env = Env::new();
    builtins::install(&env);

    // Eval every form in the page's source.
    let forms = match reader::read_all(&src) {
        Ok(f) => f,
        Err(e) => die(format!("parse {path}: {e}")),
    };
    for f in forms {
        if let Err(e) = eval::eval(&f, &env) {
            die(format!("eval {path}: {e}"));
        }
    }

    // Look up `prerender` (current ns first, falling back to core).
    let f = match env.lookup("prerender") {
        Ok(v) => v,
        Err(_) => die("page does not define a 0-arg `prerender` fn"),
    };
    let out = match eval::apply(&f, &[]) {
        Ok(v) => v,
        Err(e) => die(format!("(prerender) raised: {e}")),
    };
    match out {
        Value::Str(s) => print!("{s}"),
        // Be tolerant: if the page returned a hiccup vector, render it
        // here via cljrs.ui/render-html so the page can stay declarative.
        other => {
            let render_fn = env
                .lookup("cljrs.ui/render-html")
                .unwrap_or_else(|_| die("cljrs.ui/render-html missing"));
            match eval::apply(&render_fn, &[other]) {
                Ok(Value::Str(s)) => print!("{s}"),
                Ok(v) => die(format!(
                    "(prerender) returned {}, not a string",
                    v.type_name()
                )),
                Err(e) => die(format!("render-html failed: {e}")),
            }
        }
    }
}
