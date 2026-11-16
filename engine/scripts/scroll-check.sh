#!/usr/bin/env bash
# Regression check: every app page must scroll (viewport taller than the window) in a short window,
# and no page may claim absurd empty scroll. Headless (xvfb) and throwaway HOME: opens nothing on your screen.
#   engine/scripts/scroll-check.sh [path/to/voicio]
set -euo pipefail
BIN=${1:-$(dirname "$0")/../target/release/voicio}
HOME=$(mktemp -d); export HOME; trap 'rm -rf "$HOME"' EXIT
fail=0
for p in 0 1 2 3 4 5; do
  line=$(VOICIO_SNAP_SCROLL=1 VOICIO_SNAP_SIZE=1000x600 xvfb-run -a "$BIN" ui-snapshot $p "$HOME/s.ppm" 2>&1 | grep scroll-report || true)
  c=$(sed -n 's/.*content=\([0-9.]*\).*/\1/p' <<<"$line"); v=$(sed -n 's/.*view=\([0-9.]*\).*/\1/p' <<<"$line")
  echo "page $p: content=$c view=$v"
  awk -v c="${c:-0}" -v v="${v:-0}" 'BEGIN{exit !(c>v && c<4000)}' || { echo "FAIL page $p (not scrollable, or inflated)"; fail=1; }
done
exit $fail
