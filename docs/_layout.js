// Shared header/footer injection + REPL wiring. Loaded as a module from
// every docs page so content files stay small.

const NAV = [
  { href: "./index.html", label: "Overview" },
  { href: "./syntax.html", label: "Syntax" },
  { href: "./coverage.html", label: "Coverage" },
  { href: "./demos.html", label: "Demos" },
  { href: "./gpu-web.html", label: "GPU" },
  { href: "./benchmarks.html", label: "Benchmarks" },
  { href: "./repl.html", label: "REPL" },
  { href: "./roadmap.html", label: "Roadmap" },
];

function basename(href) {
  return href.split("/").pop();
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
}

let wasmReady = null;

async function loadWasm() {
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
export async function wireRepl(el, opts = {}) {
  const ta = el.querySelector("textarea");
  const out = el.querySelector(".repl-out");
  const btn = el.querySelector("button");
  const initial = opts.initial || ta.value || "";
  ta.value = initial;

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
