// Shared header/footer injection + REPL wiring. Loaded as a module from
// every docs page so content files stay small.

const NAV = [
  { href: "./index.html", label: "Overview" },
  { href: "./syntax.html", label: "Syntax" },
  { href: "./coverage.html", label: "Coverage" },
  { href: "./demos.html", label: "Demos" },
  { href: "./gpu-web.html", label: "GPU" },
  { href: "./physics.html", label: "Physics" },
  { href: "./platformer.html", label: "Platformer" },
  { href: "./synth.html", label: "Synth" },
  { href: "./sequencer.html", label: "Sequencer" },
  { href: "./ml.html", label: "ML" },
  { href: "./js-interop.html", label: "JS" },
  { href: "./ui-demo.html", label: "UI" },
  { href: "./benchmarks.html", label: "Benchmarks" },
  { href: "./repl.html", label: "REPL" },
  { href: "./roadmap.html", label: "Roadmap" },
];

function basename(href) {
  return href.split("/").pop();
}

export async function attachHighlightOverlay(wrapper, ta) {
  const { highlight } = await import("./_highlight.js");
  // Build the overlay structure: <div class="repl-hl-wrap"> containing
  // <pre class="repl-hl"> and the existing <textarea>. CSS stacks them.
  const hlPre = document.createElement("pre");
  hlPre.className = "repl-hl";
  hlPre.setAttribute("aria-hidden", "true");
  const wrap = document.createElement("div");
  wrap.className = "repl-hl-wrap";
  ta.parentNode.insertBefore(wrap, ta);
  wrap.appendChild(hlPre);
  wrap.appendChild(ta);

  const sync = () => {
    let src = ta.value;
    // Trailing newline: textareas need it to render the last line's
    // height; mirror it in the <pre> so heights line up.
    if (src.endsWith("\n")) src += " ";
    hlPre.innerHTML = highlight(src, "clj");
  };
  ta.addEventListener("input", sync);
  ta.addEventListener("scroll", () => {
    hlPre.scrollTop = ta.scrollTop;
    hlPre.scrollLeft = ta.scrollLeft;
  });
  // Initial paint.
  sync();
}

/// Auto-highlight all `<pre><code>` blocks on the page. Detects language
/// from `class="language-clj"` or `class="language-wgsl"`. Runs after
/// `mountChrome` so new content injected by callers also gets scanned
/// (callers can invoke `highlightAll()` again after they inject text).
export async function highlightAll(root = document) {
  const { highlight } = await import("./_highlight.js");
  const nodes = root.querySelectorAll("pre code");
  for (const el of nodes) {
    if (el.dataset.hl === "1") continue;
    const cls = el.className || "";
    let lang = null;
    const m = cls.match(/language-(\w+)/);
    if (m) lang = m[1];
    else {
      // default: Clojure, since most docs-site snippets are cljrs.
      lang = "clj";
    }
    el.innerHTML = highlight(el.textContent, lang);
    el.dataset.hl = "1";
  }
}

export function mountChrome(activePath) {
  const header = document.createElement("header");
  const active = basename(activePath || location.pathname);
  header.innerHTML = `
    <h1><a href="./index.html">cljrs</a></h1>
    <nav>
      ${NAV.map(
        (n) =>
          `<a href="${n.href}" class="${
            basename(n.href) === active ? "active" : ""
          }">${n.label}</a>`
      ).join("")}
    </nav>
  `;
  document.body.prepend(header);

  const footer = document.createElement("footer");
  footer.innerHTML = `cljrs — a from-scratch Clojure in Rust. <a href="https://github.com/">source</a> · built ${new Date().toISOString().slice(0, 10)}`;
  document.body.append(footer);

  // Auto-highlight any static <pre><code> blocks on the page. Fires on
  // DOMContentLoaded or immediately if already loaded. Pages that inject
  // code dynamically can `import { highlightAll } from "./_layout.js"`
  // and call it after their injection.
  highlightAll().catch(() => {});
}

// --- Monaco editor (lazy) ----------------------------------------------
// Vendored via jsdelivr's AMD loader. First call injects loader.js,
// subsequent calls hit the cached promise. The same loader is reused
// for monaco-vim.

const MONACO_VS = "https://cdn.jsdelivr.net/npm/monaco-editor@0.45.0/min/vs";
const MONACO_VIM = "https://unpkg.com/monaco-vim@0.4.1/dist/monaco-vim";

let monacoReady = null;
function injectScript(src) {
  return new Promise((resolve, reject) => {
    const s = document.createElement("script");
    s.src = src; s.onload = resolve; s.onerror = reject;
    document.head.appendChild(s);
  });
}
export function loadMonaco() {
  if (monacoReady) return monacoReady;
  monacoReady = (async () => {
    await injectScript(`${MONACO_VS}/loader.js`);
    window.require.config({
      paths: { vs: MONACO_VS, "monaco-vim": MONACO_VIM },
    });
    return await new Promise((resolve) =>
      window.require(["vs/editor/editor.main"], () => resolve(window.monaco))
    );
  })();
  return monacoReady;
}
let monacoVimReady = null;
export function loadMonacoVim() {
  if (monacoVimReady) return monacoVimReady;
  monacoVimReady = (async () => {
    await loadMonaco();
    return await new Promise((resolve) =>
      window.require(["monaco-vim"], (vim) => resolve(vim))
    );
  })();
  return monacoVimReady;
}

/// Create a Monaco editor mounted into `container` configured for cljrs.
/// Options:
///   value           — initial source string
///   onApply(src)    — called debounced on edit (default 300ms) and on
///                     Cmd/Ctrl+Enter; if absent, no auto-apply wiring
///   debounceMs      — auto-apply delay (default 300)
///   vimToggleEl     — checkbox input element; if present, wires vim mode
///   vimStatusEl     — element to hold vim status bar (mode/keys)
///   vimKey          — localStorage key suffix (`cljrs.<key>.vim`); default 'editor'
///   monacoOptions   — extra options merged into monaco.editor.create
export async function makeEditor(container, opts = {}) {
  const monaco = await loadMonaco();
  const editor = monaco.editor.create(container, {
    value: opts.value || "",
    language: "clojure",
    theme: "vs-dark",
    fontSize: opts.fontSize || 13,
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    automaticLayout: true,
    tabSize: 2,
    lineNumbers: opts.lineNumbers || "on",
    renderWhitespace: "selection",
    wordWrap: "off",
    ...(opts.monacoOptions || {}),
  });

  if (opts.onApply) {
    const debounceMs = opts.debounceMs ?? 300;
    let timer = null;
    editor.onDidChangeModelContent(() => {
      clearTimeout(timer);
      timer = setTimeout(() => opts.onApply(editor.getValue()), debounceMs);
    });
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, () => {
      clearTimeout(timer);
      opts.onApply(editor.getValue());
    });
  }

  if (opts.vimToggleEl) {
    const vimLib = await loadMonacoVim();
    const key = `cljrs.${opts.vimKey || "editor"}.vim`;
    let vimMode = null;
    const setVim = (on) => {
      if (on && !vimMode) {
        vimMode = vimLib.initVimMode(editor, opts.vimStatusEl);
      } else if (!on && vimMode) {
        vimMode.dispose();
        vimMode = null;
        if (opts.vimStatusEl) opts.vimStatusEl.textContent = "";
      }
      try { localStorage.setItem(key, on ? "1" : "0"); } catch {}
      opts.vimToggleEl.checked = on;
    };
    opts.vimToggleEl.addEventListener("change", () =>
      setVim(opts.vimToggleEl.checked)
    );
    if (localStorage.getItem(key) === "1") setVim(true);
  }

  return editor;
}

let wasmReady = null;

export async function loadWasm() {
  if (wasmReady) return wasmReady;
  wasmReady = (async () => {
    const mod = await import("./wasm/cljrs_wasm.js");
    await mod.default();
    return mod;
  })();
  return wasmReady;
}

/// Attach an evaluator to a .repl element. The element contains:
///   <textarea>          — source code
///   <div class="repl-toolbar"><button>Run</button></div>
///   <div class="repl-out"></div>
///
/// Also wires up a live-highlighted overlay: the textarea is layered on
/// top of a <pre> that shows the same text with syntax colors. The
/// textarea itself has transparent text + caret-only so the user sees
/// the colored version. Standard textbox-with-syntax trick; zero deps.
export async function wireRepl(el, opts = {}) {
  const ta = el.querySelector("textarea");
  const out = el.querySelector(".repl-out");
  const btn = el.querySelector("button");
  const initial = opts.initial || ta.value || "";
  ta.value = initial;

  // Attach syntax-highlighted overlay unless explicitly disabled.
  if (opts.highlight !== false) {
    await attachHighlightOverlay(el, ta);
  }

  const mod = await loadWasm();
  const repl = opts.stateful ? new mod.Repl() : null;

  const run = () => {
    const src = ta.value;
    let result;
    try {
      result = repl ? repl.eval(src) : mod.eval_source(src);
    } catch (e) {
      result = String(e);
    }
    out.textContent = result;
    out.classList.toggle("err", result.startsWith("read error") || result.startsWith("eval error"));
  };

  btn.addEventListener("click", run);
  ta.addEventListener("keydown", (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
      e.preventDefault();
      run();
    }
  });

  if (opts.autoRun) run();
  return { run, repl };
}
