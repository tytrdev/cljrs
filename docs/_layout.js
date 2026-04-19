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
  { href: "./library.html", label: "Library" },
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

// Scrubbable-token visual affordance: subtle outline on numbers and
// hex-color strings, with each color tinted in its own color. CSS is
// injected once into <head>; per-color rules are added on demand.
let scrubStylesInstalled = false;
const knownColorClasses = new Set();
function ensureScrubStyles() {
  if (scrubStylesInstalled) return;
  scrubStylesInstalled = true;
  const css = `
    .cljrs-tok-number-deco {
      background: rgba(255,204,96,0.10);
      outline: 1px solid rgba(255,204,96,0.30);
      border-radius: 2px;
    }
    body.cljrs-scrub-mode .cljrs-tok-number-deco {
      background: rgba(255,204,96,0.30);
      outline-color: rgba(255,204,96,0.80);
      cursor: ew-resize !important;
    }
    body.cljrs-scrub-mode .cljrs-tok-color-deco {
      cursor: pointer !important;
    }
  `;
  const style = document.createElement("style");
  style.id = "cljrs-scrub-styles";
  style.textContent = css;
  document.head.appendChild(style);
}
function ensureColorClass(hex) {
  // hex looks like "#rrggbb"
  const cls = "cljrs-color-" + hex.slice(1).toLowerCase();
  if (knownColorClasses.has(cls)) return cls;
  knownColorClasses.add(cls);
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  const css = `
    .${cls} {
      background: rgba(${r},${g},${b},0.18);
      outline: 1px solid rgba(${r},${g},${b},0.55);
      border-radius: 2px;
    }
    body.cljrs-scrub-mode .${cls} {
      background: rgba(${r},${g},${b},0.45);
      outline-color: rgba(${r},${g},${b},0.95);
    }
  `;
  const style = document.createElement("style");
  style.textContent = css;
  document.head.appendChild(style);
  return cls;
}

// Single global tracker for whether Alt/Ctrl is currently held — drives
// the `cljrs-scrub-mode` body class so all editors highlight in sync.
let scrubModListeners = false;
function ensureScrubModListeners() {
  if (scrubModListeners) return;
  scrubModListeners = true;
  const update = (e) => {
    document.body.classList.toggle("cljrs-scrub-mode", !!(e.altKey || e.ctrlKey));
  };
  for (const ev of ["keydown", "keyup", "mousemove", "mousedown", "mouseup"]) {
    window.addEventListener(ev, update, true);
  }
  window.addEventListener("blur", () => document.body.classList.remove("cljrs-scrub-mode"));
}

/// Wire Alt/Ctrl + mousedown on numbers (drag to scrub) and on
/// `"#rrggbb"` string literals (click to open a color picker) for a
/// Monaco editor. The editor's onApply hook handles re-evaluation.
///
/// Uses `editor.getTargetAtClientPoint` to find the source position
/// under the cursor, then scans the line text for a numeric literal
/// or quoted hex color spanning that column. Edits go through
/// `editor.executeEdits` so Monaco's undo/redo and change events stay
/// coherent.
export function attachAltDragScrub(editor, monaco) {
  const dom = editor.getDomNode();
  if (!dom) return;

  // Disable Monaco's Alt-as-multicursor and column-selection so our
  // Alt+drag wins on macOS (where Alt-click otherwise opens a column
  // selection).
  try {
    editor.updateOptions({
      multiCursorModifier: "ctrlCmd",
      columnSelection: false,
    });
  } catch {}

  ensureScrubStyles();
  ensureScrubModListeners();

  // ----- decorations: scan buffer + mark numbers / hex colors -----
  let decoIds = [];
  function refreshDecorations() {
    const model = editor.getModel();
    if (!model) return;
    const decos = [];
    const lineCount = model.getLineCount();
    for (let ln = 1; ln <= lineCount; ln++) {
      const line = model.getLineContent(ln);
      let m;
      const seen = []; // [start,end) of color spans, to skip number scan inside
      const hexRe = /"#[0-9a-fA-F]{6}"/g;
      while ((m = hexRe.exec(line)) != null) {
        const s = m.index + 1;
        const e = s + m[0].length;
        const cls = ensureColorClass(m[0].slice(1, 8)); // strip outer quotes
        decos.push({
          range: new monaco.Range(ln, s, ln, e),
          options: { inlineClassName: `cljrs-tok-color-deco ${cls}` },
        });
        seen.push([s, e]);
      }
      const numRe = /-?\d+(?:\.\d+)?/g;
      while ((m = numRe.exec(line)) != null) {
        const s = m.index + 1;
        const e = s + m[0].length;
        // Skip numbers inside a color literal (e.g. "#abc123").
        if (seen.some(([cs, ce]) => s >= cs && e <= ce)) continue;
        decos.push({
          range: new monaco.Range(ln, s, ln, e),
          options: { inlineClassName: "cljrs-tok-number-deco" },
        });
      }
    }
    decoIds = editor.deltaDecorations(decoIds, decos);
  }
  refreshDecorations();
  editor.onDidChangeModelContent(refreshDecorations);

  // ----- helpers -----
  const NUM_RE   = /-?\d+(?:\.\d+)?/g;
  const HEX_RE   = /"#[0-9a-fA-F]{6}"/g;
  function modOf(e) { return !!(e.altKey || e.ctrlKey); }

  // Find a token (number or quoted hex color) on `line` whose
  // [start..end) column range contains `col` (1-based, Monaco-style).
  function findTokenAt(line, col) {
    HEX_RE.lastIndex = 0;
    let m;
    while ((m = HEX_RE.exec(line)) != null) {
      const s = m.index + 1, e = s + m[0].length; // 1-based cols
      if (col >= s && col < e) {
        return { kind: "color", text: m[0], startCol: s, endCol: e };
      }
    }
    NUM_RE.lastIndex = 0;
    while ((m = NUM_RE.exec(line)) != null) {
      const s = m.index + 1, e = s + m[0].length;
      if (col >= s && col < e) {
        return { kind: "number", text: m[0], startCol: s, endCol: e };
      }
    }
    return null;
  }

  function parseNumberText(s) {
    const v = parseFloat(s);
    if (!Number.isFinite(v)) return null;
    const dot = s.indexOf(".");
    const decimals = dot >= 0 ? s.length - dot - 1 : 0;
    return { value: v, decimals };
  }
  function formatNumber(v, decimals) {
    if (decimals === 0) return String(Math.round(v));
    return v.toFixed(decimals);
  }

  // ----- color picker (lazy, shared) -----
  let colorInput = null;
  function ensureColorInput() {
    if (colorInput) return colorInput;
    colorInput = document.createElement("input");
    colorInput.type = "color";
    Object.assign(colorInput.style, {
      position: "fixed", width: "1px", height: "1px",
      opacity: "0", pointerEvents: "none",
    });
    document.body.appendChild(colorInput);
    return colorInput;
  }

  // ----- state -----
  let active = null; // { startValue, decimals, startX, line, startCol, endCol }
  let colorCtx = null; // { line, startCol, endCol }

  function replaceRange(line, startCol, endCol, text) {
    const range = new monaco.Range(line, startCol, line, endCol);
    editor.executeEdits("alt-drag-scrub", [
      { range, text, forceMoveMarkers: true },
    ]);
    return startCol + text.length; // new endCol
  }

  // Suppress mousedown so Monaco doesn't move the cursor / start a
  // drag-select / column-select while we're scrubbing. We have to
  // catch on the WINDOW in capture phase + stopImmediatePropagation,
  // because Monaco attaches its own capture listeners on inner
  // overlay nodes that would otherwise run first.
  window.addEventListener("mousedown", (e) => {
    if (!modOf(e)) return;
    if (!dom.contains(e.target)) return;
    const target = editor.getTargetAtClientPoint(e.clientX, e.clientY);
    if (!target || !target.position) return;
    const { lineNumber, column } = target.position;
    const lineText = editor.getModel().getLineContent(lineNumber);
    const tok = findTokenAt(lineText, column);
    if (!tok) return;

    e.preventDefault();
    e.stopPropagation();
    e.stopImmediatePropagation();

    if (tok.kind === "color") {
      const ci = ensureColorInput();
      ci.value = tok.text.slice(2, 8).toLowerCase();
      ci.style.left = (e.clientX - 6) + "px";
      ci.style.top  = (e.clientY - 6) + "px";
      ci.style.pointerEvents = "auto";
      colorCtx = { line: lineNumber, startCol: tok.startCol, endCol: tok.endCol };
      // single shared listeners (idempotent re-bind via marker)
      if (!ci._cljrsBound) {
        ci._cljrsBound = true;
        ci.addEventListener("input", () => {
          if (!colorCtx) return;
          const replacement = `"${ci.value}"`;
          const newEnd = replaceRange(
            colorCtx.line, colorCtx.startCol, colorCtx.endCol, replacement);
          colorCtx.endCol = newEnd;
        });
        ci.addEventListener("change", () => {
          colorCtx = null;
          ci.style.pointerEvents = "none";
        });
      }
      ci.click();
      return;
    }

    // number scrub
    const parsed = parseNumberText(tok.text);
    if (!parsed) return;
    active = {
      startValue: parsed.value,
      decimals: parsed.decimals,
      startX: e.clientX,
      line: lineNumber,
      startCol: tok.startCol,
      endCol: tok.endCol,
    };
    document.body.style.cursor = "ew-resize";
  }, true);

  // Belt-and-braces: also block click + contextmenu while a modifier is
  // held over the editor so Monaco can't claim them mid-scrub.
  for (const ev of ["click", "contextmenu"]) {
    window.addEventListener(ev, (e) => {
      if (!modOf(e)) return;
      if (!dom.contains(e.target)) return;
      const target = editor.getTargetAtClientPoint(e.clientX, e.clientY);
      if (!target || !target.position) return;
      const lineText = editor.getModel().getLineContent(target.position.lineNumber);
      if (!findTokenAt(lineText, target.position.column)) return;
      e.preventDefault();
      e.stopPropagation();
      e.stopImmediatePropagation();
    }, true);
  }

  window.addEventListener("mousemove", (e) => {
    if (!active) return;
    const dx = e.clientX - active.startX;
    const mag = Math.max(0.1, Math.abs(active.startValue));
    const coarse = active.decimals === 0 ? 1 : Math.max(0.001, mag * 0.02);
    const step = e.shiftKey ? coarse * 10 : coarse;
    let v = active.startValue + dx * step;
    if (active.decimals === 0) v = Math.round(v);
    const newText = formatNumber(v, active.decimals);
    const newEnd = replaceRange(active.line, active.startCol, active.endCol, newText);
    active.endCol = newEnd;
    e.preventDefault();
  }, true);

  window.addEventListener("mouseup", () => {
    if (!active) return;
    document.body.style.cursor = "";
    active = null;
  }, true);
  window.addEventListener("blur", () => {
    if (active) { document.body.style.cursor = ""; active = null; }
  });
}

// --- Library docs sidebar -------------------------------------------
// Each lib-*.html page can call mountLibrarySidebar() to inject a
// sticky left-rail nav listing every library and highlighting the
// current page.

const LIBRARY_PAGES = [
  { href: "./library.html",  label: "Overview" },
  { href: "./lib-core.html", label: "Core" },
  { href: "./lib-ui.html",   label: "UI" },
  { href: "./lib-shader.html", label: "Shader" },
  { href: "./lib-ml.html",   label: "ML" },
  { href: "./lib-music.html",label: "Music" },
  { href: "./lib-js.html",   label: "JS" },
  { href: "./lib-rust.html", label: "Rust interop" },
];

export function mountLibrarySidebar() {
  // Wrap <main> in a flex layout if not already wrapped.
  const main = document.querySelector("main");
  if (!main || main.parentElement?.classList?.contains("lib-layout")) return;
  const wrap = document.createElement("div");
  wrap.className = "lib-layout";
  main.parentNode.insertBefore(wrap, main);
  const aside = document.createElement("aside");
  aside.className = "lib-sidebar";
  const here = basename(location.pathname);
  aside.innerHTML = `
    <h4>Libraries</h4>
    <ul>
      ${LIBRARY_PAGES.map(p =>
        `<li><a href="${p.href}" class="${
          basename(p.href) === here ? "active" : ""
        }">${p.label}</a></li>`
      ).join("")}
    </ul>
  `;
  wrap.appendChild(aside);
  wrap.appendChild(main);
}

// --- Share links via URL hash ----------------------------------------
// Each editor page can call `attachShareLink(editor)` to:
//   1. on load, if the URL hash contains `#src=<base64url>`, swap the
//      editor's contents to the decoded source. Auto-applies via the
//      editor's onApply hook.
//   2. wire every `[data-share]` button on the page to copy a fresh
//      share URL (origin + pathname + #src=encoded) to the clipboard,
//      with a tiny visual confirm.
function _b64urlEncode(s) {
  return btoa(unescape(encodeURIComponent(s)))
    .replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}
function _b64urlDecode(s) {
  s = s.replace(/-/g, "+").replace(/_/g, "/");
  while (s.length % 4) s += "=";
  return decodeURIComponent(escape(atob(s)));
}
export function attachShareLink(editor) {
  // Hydrate from hash on load.
  try {
    const m = (window.location.hash || "").match(/[#&]src=([^&]+)/);
    if (m && editor) {
      const decoded = _b64urlDecode(m[1]);
      if (decoded) editor.setValue(decoded);
    }
  } catch (e) {
    console.warn("share-link: bad #src= payload —", e);
  }
  // Wire every share button on the page.
  for (const btn of document.querySelectorAll("[data-share]")) {
    btn.addEventListener("click", async () => {
      const src = editor.getValue();
      const url =
        `${location.origin}${location.pathname}#src=${_b64urlEncode(src)}`;
      try {
        await navigator.clipboard.writeText(url);
        const orig = btn.textContent;
        btn.textContent = "✓ link copied";
        setTimeout(() => (btn.textContent = orig), 1500);
      } catch {
        prompt("Copy share link:", url);
      }
    });
  }
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
