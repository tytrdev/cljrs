#!/usr/bin/env bash
# docs/build-ui.sh — build-time prerender pass for cljrs UI pages.
#
# For each HTML file under docs/ that contains a marker line of the
# form
#     <!-- cljrs-prerender: ./some-page.cljrs -->
# we run `cargo run --bin prerender -- docs/some-page.cljrs`, capture
# its stdout (the rendered HTML), and inject it into the page's
# <div id="cljrs-root"></div> mount point.
#
# The script is idempotent: re-running it strips any previously
# injected content first.

set -euo pipefail

cd "$(dirname "$0")/.."

# Build the prerender binary once up front so per-page invocations are
# fast and we surface compile errors early.
cargo build --bin prerender --quiet

inject_one() {
  local html="$1"
  local marker_line
  marker_line=$(grep -E '<!-- cljrs-prerender:' "$html" || true)
  if [ -z "$marker_line" ]; then
    return 0
  fi
  # Extract the cljrs path. Marker shape: <!-- cljrs-prerender: PATH -->
  local cljrs_path
  cljrs_path=$(echo "$marker_line" | sed -E 's/.*cljrs-prerender:[[:space:]]*([^[:space:]]+).*/\1/')
  # Resolve relative to the html's directory.
  local dir
  dir=$(dirname "$html")
  local target="$dir/$cljrs_path"
  if [ ! -f "$target" ]; then
    echo "build-ui: $html references missing $target" >&2
    exit 1
  fi
  echo "build-ui: prerender $target -> $html"

  local rendered
  rendered=$(./target/debug/prerender "$target")

  # Build the replacement <div>. Using python3 to do the substitution
  # so we don't get bitten by sed's quirks around newlines and HTML.
  python3 - "$html" "$rendered" <<'PY'
import re, sys, pathlib
path = pathlib.Path(sys.argv[1])
rendered = sys.argv[2]
src = path.read_text()
# Inject between explicit <!--ssr--> ... <!--/ssr--> markers inside the
# mount div. Idempotent: re-running cleanly replaces previous content
# without the nested-</div> ambiguity that broke regex matching.
new, n = re.subn(
    r'<!--ssr-->.*?<!--/ssr-->',
    f'<!--ssr-->{rendered}<!--/ssr-->',
    src,
    count=1,
    flags=re.DOTALL,
)
if n == 0:
    print(f"build-ui: {path} has no <!--ssr-->...<!--/ssr--> markers", file=sys.stderr)
    sys.exit(1)
path.write_text(new)
PY
}

for html in docs/*.html; do
  inject_one "$html"
done

echo "build-ui: done"
