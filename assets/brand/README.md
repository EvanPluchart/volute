# Volute — logo kit

The V of Volute carries a scroll head, as a violin does: the sound you shape by hand. The head is the one accent, and a volute is cut into it.

![Volute](png/volute-horizontal.png)

## Files

| Need | File |
|---|---|
| App icon (Windows executable, taskbar, Start) | `icons/volute.ico` (16 to 256 px), `logo/volute-app-icon.svg` |
| Tray icon | `icons/tray.ico`, muted: `icons/tray-muted.ico` |
| Tray, one colour (monochrome taskbar style) | `icons/tray-white*.svg`, `icons/tray-black*.svg` |
| Logo on light grounds | `logo/volute-horizontal.svg`, `logo/volute-symbol.svg` |
| Logo on dark or violet grounds | `logo/volute-horizontal-reversed.svg`, `logo/volute-symbol-reversed.svg` |
| One colour (print, engraving, stamps) | `logo/*-black.svg`, `logo/*-white.svg` |
| Wordmark alone | `logo/volute-wordmark.svg`, `logo/volute-wordmark-reversed.svg` |
| Small sizes (16 to 32 px) | `logo/volute-app-icon-small.svg`, `logo/volute-symbol-small.svg` |
| Website (M4) | `icons/favicon.svg`, `icons/favicon.ico`, `icons/apple-touch-icon.png` |
| PNGs | `png/` |
| Presentation | `board/board.html`, `board/slides/` |

Everything is generated. Edit the geometry, never the outputs:

```sh
node assets/brand/src/kit.mjs                     # SVG masters, from src/bold.mjs and src/geometry.mjs
PY=python sh assets/brand/src/export.sh           # PNGs and .ico (needs the logo-design skill's render_png.py)
```

The shapes are exact: straight segments and circular arcs, offset into filled outlines. There are no strokes, no fonts and no effects. The wordmark is drawn, not typed, so it needs no font licence.

## Colours

| Name | HEX | RGB | Use |
|---|---|---|---|
| Violet | `#5B2BFF` | 91 43 255 | The app tile; the V on light grounds |
| Cream | `#FFF1DE` | 255 241 222 | The V on violet or dark grounds |
| Amber | `#FFB020` | 255 176 32 | The scroll head, and only it: the one accent |
| Ink | `#1B1530` | 27 21 48 | The wordmark on light grounds |
| Danger | `#FF4D5E` | 255 77 94 | The muted head only |

Contrast: cream on violet is 6.3:1, ink on cream 15:1, and the wordmark is ink or cream, never amber.

## Rules

* **Clear space:** the head's diameter on every side of the symbol, and half of it around the lockup.
* **Minimum sizes:** the full symbol from 48 px; the small cut (plain head) from 16 to 32 px; the lockup from 96 px wide. Print: symbol 8 mm, lockup 24 mm wide.
* **Grounds:** the violet tile, white, cream, and dark grounds. On a photo, use the app icon (the tile carries its own ground).
* **The head is amber.** It turns red only for the muted state, where its volute becomes a bar.
* **Don't:** recolour the V amber, add an outline, a shadow or a gradient, stretch or rotate it, put the wordmark in another font, set the symbol inside a circle other than the tile, or use the full-detail symbol below 48 px.

## Tests run

* 16, 20, 24 and 32 px, pixel for pixel, on light (`#F3F3F3`) and dark (`#202020`) taskbars: the V and the head read, and muted is told apart by colour and bar.
* One colour (black, white) and reversed versions keep the volute as a real hole, not a white shape.
* The concept rounds are in `concepts/`.

## Open

* A trademark search (databases, reverse image search) before the first public release.
* A round head can read as a lollipop. Keep it in mind when the mark is seen alone and small.
