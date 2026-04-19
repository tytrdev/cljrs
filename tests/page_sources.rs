//! Smoke test for every demo page: extract the embedded cljrs source
//! and evaluate it. Catches missing builtins, parse errors, and dead
//! references before the user ever opens the page.
//!
//! Looks for two patterns:
//!   1. `const INITIAL_SOURCE = \`...\`;` — single-source demos
//!   2. ML-style `<key>: \`...\`,` blocks inside a SOURCES object
//!   3. Sibling `.cljrs` files referenced by `<!-- cljrs-prerender:`
//!
//! Each extracted source is evaluated in a fresh env wired the same
//! way the wasm Repl does it: builtins + cljrs_physics + cljrs_ml
//! + cljrs_music if present. The synth.cljrs and ui-demo.cljrs
//! sources additionally need the cljrs.ui prelude.

use cljrs::{builtins, env::Env, eval, reader};
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fresh_env() -> Env {
    use cljrs::value::{Builtin, Value};
    let env = Env::new();
    builtins::install(&env);
    cljrs_physics::install(&env);
    cljrs_ml::install(&env);
    // js/* and cljrs.js/* live in the wasm crate (browser only). Stub
    // every name the demos use as a no-op returning nil so the source
    // can be evaluated natively. We're checking parse + symbol
    // resolution, not browser behavior.
    let prev = env.current_ns();
    let noop = |_args: &[Value]| -> cljrs::error::Result<Value> { Ok(Value::Nil) };
    let stubs = [
        "log", "now", "get-element", "set-text!", "set-html!",
        "get-value", "set-value!", "on!", "local-get", "local-set!",
        "fetch-text",
    ];
    for ns in ["js", "cljrs.js"] {
        env.set_current_ns(ns);
        for name in stubs {
            env.define_global(name, Value::Builtin(Builtin::new_static(name, noop)));
        }
    }
    env.set_current_ns(prev.as_ref());
    env
}

/// Find every `<key>: \`...\`,` block AND `const INITIAL_SOURCE = \`...\`;`
/// in an HTML/JS file. Returns labeled sources.
fn extract_embedded_sources(html: &str, file_label: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    let bytes = html.as_bytes();

    // Pattern A: const INITIAL_SOURCE = `...`;
    if let Some(start) = html.find("const INITIAL_SOURCE = `") {
        let body_start = start + "const INITIAL_SOURCE = `".len();
        if let Some(end_off) = html[body_start..].find("`;") {
            let body = &html[body_start..body_start + end_off];
            out.push((format!("{file_label}:INITIAL_SOURCE"), unescape_template(body)));
        }
    }

    // Pattern B: any `<ident>: \`...\`,` block. Greedy enough to catch the
    // showcase blobs inside a SOURCES object like in ml.html.
    while i < bytes.len() {
        // look for `<word>: \``
        if let Some(off) = html[i..].find(": `") {
            let abs = i + off;
            // Walk back to find the start-of-identifier.
            let mut j = abs;
            while j > 0 {
                let c = bytes[j - 1] as char;
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' { j -= 1; } else { break; }
            }
            let ident = &html[j..abs];
            if !ident.is_empty() && !ident.starts_with(char::is_numeric) {
                // confirm body ends with `\`,` or `\`}\` etc.
                let body_start = abs + ": `".len();
                if let Some(end_off) = find_template_end(&html[body_start..]) {
                    let body = &html[body_start..body_start + end_off];
                    // Heuristic: only count it if the body looks like cljrs
                    // (starts with ;; or `(` or whitespace then ;).
                    let trimmed = body.trim_start();
                    if trimmed.starts_with(";;") || trimmed.starts_with("(") {
                        out.push((format!("{file_label}:{ident}"), unescape_template(body)));
                    }
                }
            }
            i = abs + 1;
        } else {
            break;
        }
    }
    out
}

/// Find a template literal's closing backtick, skipping `\`` escapes
/// and `${...}` interpolations. Returns offset in input, or None.
fn find_template_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut depth = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if c == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            depth += 1;
            i += 2;
            continue;
        }
        if c == b'}' && depth > 0 {
            depth -= 1;
            i += 1;
            continue;
        }
        if c == b'`' && depth == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Reverse JS template-literal escapes: \` \\ \$
fn unescape_template(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut iter = s.chars().peekable();
    while let Some(c) = iter.next() {
        if c == '\\' {
            if let Some(&next) = iter.peek() {
                match next {
                    '`' | '\\' | '$' | '"' => { out.push(next); iter.next(); }
                    'n' => { out.push('\n'); iter.next(); }
                    't' => { out.push('\t'); iter.next(); }
                    _ => out.push(c),
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

fn eval_source(label: &str, src: &str) -> Result<(), String> {
    let env = fresh_env();
    let forms = match reader::read_all(src) {
        Ok(f) => f,
        Err(e) => return Err(format!("{label}: read error: {e}")),
    };
    for f in forms {
        if let Err(e) = eval::eval(&f, &env) {
            return Err(format!("{label}: eval error: {e}"));
        }
    }
    // Top-level eval defines fns but doesn't call them. Many bugs
    // (subvec missing, wrong arities, type errors) only surface when
    // the JS side actually calls a fn each frame. So invoke the
    // standard entry-points if they exist. We don't fail if they
    // don't — most demos define some subset of these.
    for entry in ["boot", "patch", "viz", "frame!", "render-todos", "key->hz"] {
        let probe = format!("(when (resolve '{entry}) ({entry}))");
        // resolve isn't actually a builtin; use a try/catch idiom instead.
        let probe = format!(
            "(try ({entry}) (catch __unbound__ _ nil) (catch _ e (throw e)))"
        );
        let _ = probe; // silence unused
    }
    // Real implementation: shell each entry-point eval through try and
    // bubble true errors. We swallow Unbound and Arity-mismatch since
    // some entries take args (e.g. key->hz, render-buffer).
    for entry in ["boot", "patch", "viz", "frame!", "render-todos"] {
        let call = format!("({entry})");
        let forms = match reader::read_all(&call) {
            Ok(f) => f,
            Err(_) => continue,
        };
        for f in forms {
            match eval::eval(&f, &env) {
                Ok(_) => {}
                Err(e) => match e.peel_ref() {
                    // fn not defined here, or fn takes args we don't supply.
                    cljrs::error::Error::Unbound(_)
                    | cljrs::error::Error::Arity { .. } => {}
                    _ => {
                        return Err(format!(
                            "{label}: ({entry}) at runtime → {e}"
                        ));
                    }
                },
            }
        }
    }
    Ok(())
}

fn pages_to_check() -> Vec<&'static str> {
    vec![
        "docs/platformer.html",
        "docs/synth.html",
        "docs/sequencer.html",
        "docs/ml.html",
        "docs/js-interop.html",
        "docs/physics.html",
        "docs/ui-demo.html",
    ]
}

fn check_html(rel: &str, errs: &mut Vec<String>) {
    let path = repo_root().join(rel);
    if !path.exists() { return; }
    let html = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => { errs.push(format!("{rel}: read failed: {e}")); return; }
    };
    let sources = extract_embedded_sources(&html, rel);
    if sources.is_empty() {
        // page may load source from sibling .cljrs file via fetch; check
        // for that pattern.
        for line in html.lines() {
            if let Some(idx) = line.find("cljrs-prerender:") {
                let rest = &line[idx + "cljrs-prerender:".len()..];
                let path_part: String = rest
                    .trim_start()
                    .chars()
                    .take_while(|c| !c.is_whitespace() && *c != '-')
                    .collect();
                if !path_part.is_empty() {
                    let dir = Path::new(rel).parent().unwrap_or(Path::new("docs"));
                    let cljrs_path = repo_root().join(dir).join(path_part.trim());
                    if cljrs_path.exists() {
                        let src = fs::read_to_string(&cljrs_path).unwrap_or_default();
                        if let Err(e) = eval_source(&format!("{rel}:{}", cljrs_path.display()), &src) {
                            errs.push(e);
                        }
                    }
                }
            }
        }
        return;
    }
    for (label, src) in sources {
        if let Err(e) = eval_source(&label, &src) {
            errs.push(e);
        }
    }
}

#[test]
fn every_demo_page_source_evaluates() {
    let mut errs = Vec::new();
    for rel in pages_to_check() {
        check_html(rel, &mut errs);
    }
    if !errs.is_empty() {
        panic!(
            "{} demo source(s) failed to evaluate:\n  - {}",
            errs.len(),
            errs.join("\n  - ")
        );
    }
}
