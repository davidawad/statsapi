#!/usr/bin/env bash
# Regenerates the README hero image. Offline: the example decodes the crate's synthetic test fixtures
# through its public API (no network, no keys). Needs cargo and rsvg-convert (brew install librsvg).
# Usage: docs/showcase/make-screenshots.sh
set -euo pipefail
cd "$(dirname "$0")/../.."
out=docs/showcase
cargo run --quiet -p statsapi-bls --example showcase > "$out/bls-showcase.txt"
{
  lines=$(wc -l < "$out/bls-showcase.txt")
  height=$(( lines * 26 + 76 ))
  printf '<svg xmlns="http://www.w3.org/2000/svg" width="760" height="%s" font-family="Menlo, Monaco, Courier New, monospace" font-size="18">\n' "$height"
  printf '<rect width="760" height="%s" rx="12" fill="#1e2127"/>\n' "$height"
  printf '<circle cx="28" cy="26" r="7" fill="#ff5f56"/><circle cx="52" cy="26" r="7" fill="#ffbd2e"/><circle cx="76" cy="26" r="7" fill="#27c93f"/>\n'
  printf '<text x="28" y="66" fill="#7f848e" xml:space="preserve">$ cargo run -p statsapi-bls --example showcase</text>\n'
  y=66
  while IFS= read -r line; do
    y=$(( y + 26 ))
    esc=${line//&/&amp;}; esc=${esc//</&lt;}; esc=${esc//>/&gt;}
    printf '<text x="28" y="%s" fill="#d7dae0" xml:space="preserve">%s</text>\n' "$y" "$esc"
  done < "$out/bls-showcase.txt"
  printf '</svg>\n'
} > "$out/bls-terminal.svg"
rsvg-convert -w 760 "$out/bls-terminal.svg" -o "$out/bls-terminal.png"
for f in "$out"/*.png; do size=$(wc -c < "$f"); [ "$size" -lt 300000 ] || { echo "$f is $size bytes (limit 300000)" >&2; exit 1; }; echo "$f $size bytes"; done
