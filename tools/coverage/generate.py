#!/usr/bin/env python3
"""
Programmatic coverage report for cljrs.

Reads:
  tools/coverage/clojure_core.txt       (canonical clojure.core public vars)
  tools/coverage/clojure_string.txt     (clojure.string)
  tools/coverage/clojure_set.txt        (clojure.set)
  tools/coverage/clojure_walk.txt       (clojure.walk)
  tools/coverage/clojure_edn.txt        (clojure.edn)
  tools/coverage/not_applicable.txt     (vars deliberately out-of-scope)

Scans:
  src/builtins.rs      (Rust builtins installed into cljrs.core)
  src/core.clj         (prelude defs/defmacros/defns in cljrs.core)

Emits:
  docs/coverage.html   (single page, one section per namespace)

The page reports three buckets per namespace:
  implemented = name appears in cljrs   (class "ok")
  missing     = in canonical list, not in cljrs, not flagged N/A   (class "missing")
  n/a         = in canonical list, listed in not_applicable.txt    (class "partial")
"""
from __future__ import annotations

import html
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
COV_DIR = ROOT / "tools" / "coverage"
BUILTINS_RS = ROOT / "src" / "builtins.rs"
CORE_CLJ = ROOT / "src" / "core.clj"
EVAL_RS = ROOT / "src" / "eval.rs"
OUT_HTML = ROOT / "docs" / "coverage.html"

NAMESPACES = [
    ("clojure.core", "clojure_core.txt"),
    ("clojure.string", "clojure_string.txt"),
    ("clojure.set", "clojure_set.txt"),
    ("clojure.walk", "clojure_walk.txt"),
    ("clojure.edn", "clojure_edn.txt"),
]

# ---------------------------------------------------------------------------
# Load canonical lists + N/A annotations
# ---------------------------------------------------------------------------

def load_namelist(path: Path) -> set[str]:
    names: set[str] = set()
    if not path.exists():
        return names
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        names.add(line)
    return names


def load_not_applicable() -> dict[str, str]:
    """Map fully-qualified name -> reason."""
    out: dict[str, str] = {}
    path = COV_DIR / "not_applicable.txt"
    if not path.exists():
        return out
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        if ":" not in line:
            continue
        qname, reason = line.split(":", 1)
        out[qname.strip()] = reason.strip()
    return out


# ---------------------------------------------------------------------------
# Scan installed names
# ---------------------------------------------------------------------------

# Pattern for a `("name", fn)` row inside core_fns(). The name may
# include slashes (e.g. "str/split") and quite a few special characters.
BUILTIN_TUPLE = re.compile(r'\(\s*"([^"\\]+)"\s*,\s*[A-Za-z_][A-Za-z0-9_]*\s*\)')

# Pattern for env.define_global("NAME", ...).
DEFINE_GLOBAL = re.compile(r'define_global\(\s*"([^"\\]+)"')

# Pattern for clojure.string/X aliases inside builtins.rs.
STRING_ALIAS = re.compile(r'"clojure\.string/([^"\\]+)"')

# Pattern for top-level defs in core.clj.
DEF_FORM = re.compile(r'^\(\s*(defn-?|def|defmacro|defrecord|defmulti|defmethod|defprotocol)\s+([^\s\)]+)')


def scan_builtins() -> tuple[set[str], set[str]]:
    """Return (installed_in_cljrs_core, installed_in_clojure_string)."""
    src = BUILTINS_RS.read_text()

    # Slice out the body of `fn core_fns()` so we don't grab unrelated
    # tuple literals elsewhere in the file.
    m = re.search(r'fn core_fns\(\)\s*->[^{]+\{(.*?)^\}', src, re.MULTILINE | re.DOTALL)
    body = m.group(1) if m else src

    core_names: set[str] = set()
    string_names: set[str] = set()
    for name in BUILTIN_TUPLE.findall(body):
        if name.startswith("str/"):
            short = name[len("str/"):]
            string_names.add(short)
            core_names.add(name)  # str/X is also defined in cljrs.core
        elif name.startswith("__"):
            continue  # transducer plumbing, internal
        elif "/" in name and not name == "/":
            # qualified install — skip; we count by namespace separately
            continue
        else:
            core_names.add(name)

    # Also pick up env.define_global("X", ...) lines anywhere in the file.
    for name in DEFINE_GLOBAL.findall(src):
        if name.startswith("__"):
            continue
        if name.startswith("clojure.string/"):
            string_names.add(name[len("clojure.string/"):])
        elif name.startswith("clojure.") and "/" in name:
            continue  # other namespace qualified — handled separately
        elif "/" in name and not name == "/":
            continue
        else:
            core_names.add(name)

    # Explicit aliases written as "clojure.string/foo". Skip format!()
    # placeholders like "{rest}".
    for name in STRING_ALIAS.findall(src):
        if name.startswith("{"):
            continue
        string_names.add(name)

    return core_names, string_names


SPECIAL_FORMS_RE = re.compile(r'const SPECIAL_FORMS:[^\[]*\[(.*?)\];', re.DOTALL)


def scan_special_forms() -> set[str]:
    """Read the SPECIAL_FORMS slice in eval.rs."""
    if not EVAL_RS.exists():
        return set()
    src = EVAL_RS.read_text()
    m = SPECIAL_FORMS_RE.search(src)
    if not m:
        return set()
    out: set[str] = set()
    for tok in re.findall(r'"([^"\\]+)"', m.group(1)):
        if tok.startswith("__"):
            continue
        out.add(tok)
    return out


def scan_core_clj() -> set[str] | None:
    """Top-level defns/defs/defmacros from core.clj. None if missing."""
    if not CORE_CLJ.exists():
        return None
    return _scan_clj_defs(CORE_CLJ)


# Generic top-level def scanner usable for any cljrs source file. Skips
# private (defn-) and `__internal` names. Caller is responsible for
# attributing the result to the right namespace.
def _scan_clj_defs(path: Path) -> set[str]:
    out: set[str] = set()
    for line in path.read_text().splitlines():
        m = DEF_FORM.match(line)
        if not m:
            continue
        kind, name = m.group(1), m.group(2)
        if kind == "defn-":
            continue
        if name.startswith("__"):
            continue
        out.add(name)
    return out


def scan_extra_ns(path: Path, expected_ns: str) -> set[str]:
    """Scan a cljrs file that opens with (ns NAMESPACE) and return its
    top-level defs IF the file's ns declaration matches expected_ns.
    Returns empty set if the file is missing or the ns doesn't match."""
    if not path.exists():
        return set()
    text = path.read_text()
    m = re.search(r'\(\s*ns\s+([^\s)]+)', text)
    if not m or m.group(1) != expected_ns:
        return set()
    return _scan_clj_defs(path)


# ---------------------------------------------------------------------------
# Render
# ---------------------------------------------------------------------------

PAGE_TEMPLATE = """<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Coverage</title>
  <link rel="stylesheet" href="./style.css">
  <style>
    .cov-summary {{ font-family: var(--mono); font-size: 0.92rem; margin: 0.4rem 0 0.8rem 0; color: var(--fg-dim); }}
    .cov-bar {{ display: inline-block; width: 220px; height: 8px; background: var(--bg-elev); border: 1px solid var(--border); border-radius: 4px; vertical-align: middle; margin: 0 0.6rem; overflow: hidden; }}
    .cov-bar-fill {{ height: 100%; background: #86efac; }}
    details.cov-list {{ margin: 0.3rem 0; }}
    details.cov-list > summary {{ cursor: pointer; font-family: var(--mono); font-size: 0.9rem; padding: 0.2rem 0; }}
    details.cov-list ul {{ columns: 3; column-gap: 1.2rem; font-family: var(--mono); font-size: 0.82rem; padding-left: 1.2rem; margin: 0.4rem 0; }}
    details.cov-list li {{ break-inside: avoid; padding: 0.05rem 0; }}
    details.cov-list li .reason {{ color: var(--fg-dim); font-size: 0.78rem; }}
    .cov-howto {{ font-size: 0.85rem; color: var(--fg-dim); margin-top: 3rem; padding-top: 1rem; border-top: 1px solid var(--border); }}
    .cov-howto code {{ font-size: 0.8rem; }}
  </style>
</head>
<body>
  <main class="prose">
    <h2>Coverage</h2>
    <p>
      Auto-generated diff between cljrs's installed names and the public
      vars of Clojure's standard library namespaces. Every var in the
      canonical list is accounted for: implemented, deliberately
      out-of-scope, or missing. Regenerate with
      <code>tools/coverage/build.sh</code>.
    </p>
{sections}
    <div class="cov-howto">
      <h3>How this is computed</h3>
      <p>
        Canonical lists (<code>tools/coverage/clojure_*.txt</code>) are
        scraped from the official Clojure 1.12 API index pages
        (<code>https://clojure.github.io/clojure/clojure.NS-api.html</code>).
        The "installed" set is read from <code>src/builtins.rs</code>
        (Rust builtins inside <code>fn core_fns()</code>) and
        <code>src/core.clj</code> (top-level <code>defn</code> /
        <code>defmacro</code> / <code>def</code> forms; private
        <code>defn-</code> ignored). Names listed in
        <code>tools/coverage/not_applicable.txt</code> are pulled out of
        the "missing" bucket and shown separately with a one-line reason.
      </p>
      <p>
        Names installed in <code>cljrs.core</code> are reachable from
        user code as <code>clojure.core/X</code> via the lookup chain in
        <code>src/env.rs</code> — qualified lookups hit globals
        directly. <code>str/X</code> builtins are dual-bound under the
        canonical <code>clojure.string/</code> prefix.
      </p>
    </div>
  </main>
  <script type="module">
    import {{ mountChrome }} from "./_layout.js";
    mountChrome();
  </script>
</body>
</html>
"""


def render_section(ns: str, canonical: set[str], installed: set[str], na: dict[str, str]) -> tuple[str, int, int, int]:
    impl = sorted(n for n in canonical if n in installed)
    na_here = {n: na[f"{ns}/{n}"] for n in canonical if f"{ns}/{n}" in na and n not in installed}
    missing = sorted(n for n in canonical if n not in installed and f"{ns}/{n}" not in na)
    total = len(canonical)
    n_impl = len(impl)
    n_na = len(na_here)
    n_missing = len(missing)
    pct = (100 * n_impl / total) if total else 0
    bar_pct = pct
    extras = sorted(n for n in installed if n not in canonical)

    def li_simple(name: str) -> str:
        return f"<li><code>{html.escape(name)}</code></li>"

    def li_reason(name: str, reason: str) -> str:
        return f"<li><code>{html.escape(name)}</code> <span class=\"reason\">— {html.escape(reason)}</span></li>"

    parts = [f'<h3 id="{html.escape(ns)}">{html.escape(ns)}</h3>']
    parts.append(
        f'<div class="cov-summary">'
        f'<span class="ok">{n_impl}</span>/{total} implemented '
        f'({pct:.0f}%)'
        f'<span class="cov-bar"><span class="cov-bar-fill" style="width:{bar_pct:.1f}%"></span></span>'
        f'<span class="missing">{n_missing} missing</span>, '
        f'<span class="partial">{n_na} not applicable</span>'
        f'</div>'
    )
    parts.append(
        f'<details class="cov-list" open><summary class="ok">'
        f'implemented ({n_impl})</summary>'
        f'<ul>{"".join(li_simple(n) for n in impl) or "<li><i>none</i></li>"}</ul>'
        f'</details>'
    )
    parts.append(
        f'<details class="cov-list"><summary class="missing">'
        f'missing ({n_missing})</summary>'
        f'<ul>{"".join(li_simple(n) for n in missing) or "<li><i>none</i></li>"}</ul>'
        f'</details>'
    )
    parts.append(
        f'<details class="cov-list"><summary class="partial">'
        f'not applicable ({n_na})</summary>'
        f'<ul>{"".join(li_reason(n, r) for n, r in sorted(na_here.items())) or "<li><i>none</i></li>"}</ul>'
        f'</details>'
    )
    if extras:
        parts.append(
            f'<details class="cov-list"><summary>'
            f'cljrs extensions not in canonical {ns} ({len(extras)})</summary>'
            f'<ul>{"".join(li_simple(n) for n in extras)}</ul>'
            f'</details>'
        )
    return "\n".join(parts), n_impl, n_missing, n_na


def main() -> None:
    na = load_not_applicable()
    builtin_core, builtin_string = scan_builtins()
    core_clj_defs = scan_core_clj() or set()
    special_forms = scan_special_forms()
    installed_core = builtin_core | core_clj_defs | special_forms

    # cljrs-authored ports of standard namespaces. Each file declares
    # (ns clojure.X) at the top so we can attribute its defs cleanly.
    extras = {
        "clojure.string": ROOT / "src" / "cljrs_string.clj",
        "clojure.set":    ROOT / "src" / "cljrs_set.clj",
        "clojure.walk":   ROOT / "src" / "cljrs_walk.clj",
        "clojure.edn":    ROOT / "src" / "cljrs_edn.clj",
    }
    per_ns = {
        "clojure.core":   installed_core,
        "clojure.string": builtin_string | scan_extra_ns(extras["clojure.string"], "clojure.string"),
        "clojure.set":    scan_extra_ns(extras["clojure.set"],    "clojure.set"),
        "clojure.walk":   scan_extra_ns(extras["clojure.walk"],   "clojure.walk"),
        "clojure.edn":    scan_extra_ns(extras["clojure.edn"],    "clojure.edn"),
    }

    sections_html: list[str] = []
    totals: list[tuple[str, int, int, int, int]] = []
    for ns, fname in NAMESPACES:
        canonical = load_namelist(COV_DIR / fname)
        section, impl, miss, n_na = render_section(ns, canonical, per_ns.get(ns, set()), na)
        sections_html.append(section)
        totals.append((ns, impl, len(canonical), miss, n_na))

    OUT_HTML.write_text(PAGE_TEMPLATE.format(sections="\n".join(sections_html)))

    # Print summary so build.sh and humans see numbers.
    print("Coverage report written to", OUT_HTML)
    for ns, impl, total, miss, n_na in totals:
        pct = (100 * impl / total) if total else 0
        print(f"  {ns:<16} {impl:>4}/{total:<4} ({pct:5.1f}%)  missing={miss:<4}  n/a={n_na}")


if __name__ == "__main__":
    main()
