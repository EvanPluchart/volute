#!/usr/bin/env sh
# Rebuilds every raster of the kit from the SVG masters: node src/kit.mjs first, then this.
# Needs Python 3 with the logo-design skill's render_png.py (RENDER), which uses Chrome or cairosvg.
set -eu
cd "$(dirname "$0")/.."
PY="${PY:-python3}"
RENDER="${RENDER:-$HOME/.claude/skills/logo-design/scripts/render_png.py}"
mkdir -p png icons
render() { "$PY" "$RENDER" "$1" -o "$2" --size "$3" >/dev/null; }

# App icon: the small cut up to 32 px, full detail from 40 px.
for s in 16 20 24 32; do render logo/volute-app-icon-small.svg "png/volute-app-icon-$s.png" "$s"; done
for s in 40 48 64 96 128 256 512 1024; do render logo/volute-app-icon.svg "png/volute-app-icon-$s.png" "$s"; done
node src/ico.mjs icons/volute.ico $(for s in 16 20 24 32 40 48 64 256; do echo "png/volute-app-icon-$s.png"; done)
cp png/volute-app-icon-180.png icons/apple-touch-icon.png 2>/dev/null || render logo/volute-app-icon.svg icons/apple-touch-icon.png 180

# Tray: 16 to 48 px (100 % to 300 % scaling), normal and muted.
for name in tray tray-muted; do
  for s in 16 20 24 32 40 48; do render "icons/$name.svg" "png/$name-$s.png" "$s"; done
  node src/ico.mjs "icons/$name.ico" $(for s in 16 20 24 32 40 48; do echo "png/$name-$s.png"; done)
done

# Favicon for the future site (M4).
node src/ico.mjs icons/favicon.ico png/volute-app-icon-16.png png/volute-app-icon-32.png png/volute-app-icon-48.png
cp logo/volute-app-icon.svg icons/favicon.svg

# Logos as PNG, for documents and the README.
for v in volute-horizontal volute-horizontal-reversed volute-symbol volute-symbol-reversed; do
  "$PY" "$RENDER" "logo/$v.svg" -o "png/$v.png" --width 1200 --height 1200 >/dev/null
done
echo "export: done"
