#!/usr/bin/env bash
# Regenerate docs/coverage.html from canonical clojure.* var lists +
# whatever cljrs currently installs.
#
# - Canonical lists live in tools/coverage/clojure_*.txt and are scraped
#   from the official Clojure 1.12 API index pages. Re-run with
#   `./build.sh refresh` to re-scrape.
# - The "installed" set is derived by parsing src/builtins.rs and
#   src/core.clj — see generate.py for the patterns.

set -eu
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
COV="$ROOT/tools/coverage"

if [ "${1:-}" = "refresh" ]; then
  echo "Refreshing canonical Clojure var lists from clojure.github.io..."
  for ns in core string set walk edn; do
    curl -s --max-time 30 "https://clojure.github.io/clojure/clojure.${ns}-api.html" | \
      grep -oE "<a href=\"#clojure\\.${ns}/[^\"]+\"" | \
      sed "s|<a href=\"#clojure\\.${ns}/||;s|\"$||" | \
      python3 -c "
import sys, html
seen=set(); out=[]
for line in sys.stdin:
    n=html.unescape(line.strip())
    if not n or n in seen: continue
    seen.add(n); out.append(n)
out.sort()
[print(n) for n in out]
" > "$COV/clojure_${ns}.txt"
    echo "  clojure.${ns}: $(wc -l < "$COV/clojure_${ns}.txt") names"
  done
fi

python3 "$COV/generate.py"
