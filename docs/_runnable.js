// Tiny in-page runnable-example helper for the library docs pages.
//
// Usage in a page:
//   <div class="run-example" data-stateful="1">
//     <textarea>(+ 1 2)</textarea>
//     <div class="repl-toolbar"><button>Run</button></div>
//     <div class="repl-out"></div>
//   </div>
//
//   <script type="module">
//     import { mountChrome } from "./_layout.js";
//     import { wireAll } from "./_runnable.js";
//     mountChrome();
//     wireAll();
//   </script>
//
// Each .run-example becomes a button-driven Repl that lazy-loads the
// wasm bundle on first click. Stateful Repls share state across runs
// within their own block. Multiple blocks on a page do not share state
// unless you pass `shared: true` (then they all use one Repl).

import { wireRepl } from "./_layout.js";

export async function wireAll(opts = {}) {
  const nodes = document.querySelectorAll(".run-example");
  for (const el of nodes) {
    const stateful = el.dataset.stateful === "1";
    // wireRepl is async; firing them sequentially keeps init ordering
    // predictable (matters for `shared` mode if any caller adds it).
    await wireRepl(el, { stateful });
  }
}
