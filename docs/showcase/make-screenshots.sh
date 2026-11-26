#!/usr/bin/env bash
# Regenerates every showcase image. Offline: each crate's examples/showcase.rs decodes that crate's
# synthetic test fixtures through its public API (no network, no keys).
# Usage: docs/showcase/make-screenshots.sh [--text|--render]   (default: both)
#   --text    run every showcase example into docs/showcase/<crate>.txt   (needs cargo)
#   --render  turn each .txt into an SVG terminal frame and a PNG         (needs rsvg-convert: brew install librsvg)
set -euo pipefail
cd "$(dirname "$0")/../.."
out=docs/showcase
stage=${1:-all}
crates=$(ls crates/*/examples/showcase.rs | cut -d/ -f2 | sort)
if [ "$stage" != --render ]; then
  for c in $crates; do cargo run --quiet -p "$c" --example showcase > "$out/$c.txt"; done
fi
if [ "$stage" != --text ]; then
  for c in $crates; do
    txt="$out/$c.txt"; svg="$out/$c.svg"
    width=$(awk '{ if (length > m) m = length } END { print (m < 60 ? 60 : m) * 11 + 56 }' "$txt")
    lines=$(wc -l < "$txt"); height=$(( lines * 26 + 76 ))
    {
      printf '<svg xmlns="http://www.w3.org/2000/svg" width="%s" height="%s" font-family="Menlo, Monaco, Courier New, monospace" font-size="18">\n' "$width" "$height"
      printf '<rect width="%s" height="%s" rx="12" fill="#1e2127"/>\n' "$width" "$height"
      printf '<circle cx="28" cy="26" r="7" fill="#ff5f56"/><circle cx="52" cy="26" r="7" fill="#ffbd2e"/><circle cx="76" cy="26" r="7" fill="#27c93f"/>\n'
      printf '<text x="28" y="66" fill="#7f848e" xml:space="preserve">$ cargo run -p %s --example showcase</text>\n' "$c"
      y=66
      while IFS= read -r line; do
        y=$(( y + 26 ))
        esc=${line//&/&amp;}; esc=${esc//</&lt;}; esc=${esc//>/&gt;}
        printf '<text x="28" y="%s" fill="#d7dae0" xml:space="preserve">%s</text>\n' "$y" "$esc"
      done < "$txt"
      printf '</svg>\n'
    } > "$svg"
    rsvg-convert -w "$width" "$svg" -o "$out/$c.png"
  done
  for f in "$out"/*.png; do size=$(wc -c < "$f"); [ "$size" -lt 300000 ] || { echo "$f is $size bytes (limit 300000)" >&2; exit 1; }; done
  echo "rendered $(echo $crates | wc -w) showcase images"
fi
