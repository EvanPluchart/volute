#!/usr/bin/env sh
# The tray icons as .ico, 16 to 48 px (100 % to 300 % scaling), from node src/tray.mjs.
set -eu
cd "$(dirname "$0")/.."
PY="${PY:-python3}"
RENDER="${RENDER:-$HOME/.claude/skills/logo-design/scripts/render_png.py}"
tmp="$(mktemp -d)"
for svg in icons/tray/*.svg; do
  name="$(basename "$svg" .svg)"
  for s in 16 20 24 32 40 48; do "$PY" "$RENDER" "$svg" -o "$tmp/$name-$s.png" --size "$s" >/dev/null; done
  node src/ico.mjs "icons/tray/$name.ico" $(for s in 16 20 24 32 40 48; do echo "$tmp/$name-$s.png"; done)
done
rm -rf "$tmp"
echo "tray: done"
