// Volute's logo kit: `node assets/brand/src/kit.mjs` rewrites every SVG master in assets/brand
// from the geometry in bold.mjs (the mark) and the wordmark below. PNGs and .ico files are made
// from these by assets/brand/src/export.sh.
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { f, add, rad, line, arc, stroke } from "./geometry.mjs";
import { mark, fit, markBody, bounds, tile } from "./bold.mjs";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const LOGO = join(ROOT, "logo");
const ICONS = join(ROOT, "icons");
mkdirSync(LOGO, { recursive: true });
mkdirSync(ICONS, { recursive: true });

export const COLOURS = {
  violet: "#5B2BFF", // the tile, the brand's ground
  cream: "#FFF1DE", // the V on violet or on dark
  amber: "#FFB020", // the scroll head: the one accent
  ink: "#1B1530", // text and the V on light grounds
  danger: "#FF4D5E", // the muted head only
};

const svg = (w, h, body, title = "Volute") =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${f(w)} ${f(h)}">\n  <title>${title}</title>\n${body}\n</svg>\n`;
const write = (dir, name, text) => writeFileSync(join(dir, name), text);

const m = mark();

// --- The symbol, on no ground: 256 square, the mark filling 220 of it.
function symbol(v, head, cut = "groove") {
  return svg(256, 256, `  <g transform="${fit(m, 128, 128, 220, 2)}">\n    ${markBody(m, v, head, cut)}\n  </g>`);
}
write(LOGO, "volute-symbol.svg", symbol(COLOURS.violet, COLOURS.amber));
write(LOGO, "volute-symbol-reversed.svg", symbol(COLOURS.cream, COLOURS.amber));
write(LOGO, "volute-symbol-black.svg", symbol("#000000", "#000000"));
write(LOGO, "volute-symbol-white.svg", symbol("#FFFFFF", "#FFFFFF"));
write(LOGO, "volute-symbol-small.svg", symbol(COLOURS.violet, COLOURS.amber, "none"));

// --- The app icon: the violet tile. Full detail from 48 px; the small cut (plain head, larger
// mark) below that, where the groove would only blur.
const palette = { bg: COLOURS.violet, v: COLOURS.cream, head: COLOURS.amber };
write(LOGO, "volute-app-icon.svg", tile(m, palette));
write(LOGO, "volute-app-icon-small.svg", tile(m, palette, 256, 0.74, "none"));
// The tray: the small tile reads on light and dark taskbars alike. Muted: the head turns red and
// its volute becomes the mute bar.
write(ICONS, "tray.svg", tile(m, palette, 256, 0.74, "none"));
write(ICONS, "tray-muted.svg", tile(m, { ...palette, head: COLOURS.danger }, 256, 0.74, "slash"));
// One-colour tray glyphs, for a monochrome taskbar style.
write(ICONS, "tray-white.svg", symbol("#FFFFFF", "#FFFFFF", "none"));
write(ICONS, "tray-black.svg", symbol("#000000", "#000000", "none"));
write(ICONS, "tray-white-muted.svg", symbol("#FFFFFF", "#FFFFFF", "slash"));
write(ICONS, "tray-black-muted.svg", symbol("#000000", "#000000", "slash"));

// --- The wordmark: lowercase "volute" in heavy monoline letters with round ends, the mark's
// rounded weight. Built from segments: no font, so no licence.
function wordmark() {
  const X = 120, A = 172, w = 30, sp = 16;
  const yx = A - X + w / 2, yb = A - w / 2, ya = w / 2;
  const ro = (X - w) / 2, cy = A - X / 2;
  const parts = [];
  let x = w / 2;
  // v
  {
    const vw = X * 0.9;
    const vtx = [x + vw / 2, yb];
    parts.push([line([x, yx], vtx)], [line(vtx, [x + vw, yx])]);
    x += vw + w + sp - 4;
  }
  // o
  {
    const c = [x + ro, cy];
    parts.push([arc(c, ro, rad(-90), rad(90)), arc(c, ro, rad(90), rad(270))]);
    x += 2 * ro + w + sp;
  }
  // l
  {
    parts.push([line([x, ya], [x, yb])]);
    x += w + sp;
  }
  // u: the left stem into the bowl; the right stem, from the x-height to the baseline, closes it.
  {
    const r = ro * 0.96, cb = [x + r, yb - r];
    parts.push([line([x, yx], [x, cb[1]]), arc(cb, r, rad(180), rad(0))]);
    parts.push([line([x + 2 * r, yx], [x + 2 * r, yb])]);
    x += 2 * r + w + sp;
  }
  // t: a stem from below the ascender, a short crossbar at the x-height.
  {
    const stem = x + 22;
    parts.push([line([stem, ya + 26], [stem, yb])]);
    parts.push([line([x - 6, yx], [stem + 30, yx])]);
    x = stem + 30 + w + sp - 8;
  }
  // e: a bar across the middle, then the bowl from its right end, over the top, round to the
  // lower right.
  {
    const c = [x + ro, cy];
    parts.push({ bar: [line([x + 4, cy], [x + 2 * ro, cy])] });
    parts.push([arc(c, ro, rad(0), rad(-180)), arc(c, ro, rad(-180), rad(-318))]);
    x += 2 * ro + w / 2;
  }
  return { parts, w, width: x, height: A };
}

const wm = wordmark();
// The e's bar is lighter than the strokes, so both of its counters stay open.
const wordmarkBody = (colour) =>
  wm.parts
    .map((p) => `  <path fill="${colour}" d="${p.bar ? stroke(p.bar, wm.w * 0.78) : stroke(p, wm.w)}"/>`)
    .join("\n");
write(LOGO, "volute-wordmark.svg", svg(wm.width, wm.height, wordmarkBody(COLOURS.ink)));
write(LOGO, "volute-wordmark-reversed.svg", svg(wm.width, wm.height, wordmarkBody(COLOURS.cream)));

// --- The horizontal lockup: the symbol a quarter taller than the wordmark's ascender, its base
// on the baseline, then a gap of half the symbol's height.
function lockup(v, head, text) {
  const b = bounds(m);
  const symH = wm.height * 1.25;
  const k = symH / (b.y1 - b.y0);
  const symW = (b.x1 - b.x0) * k;
  const gap = symH * 0.3;
  const top = wm.height - symH;
  const W = symW + gap + wm.width, H = symH;
  const body = [
    `  <g transform="translate(${f(-b.x0 * k)} ${f(-b.y0 * k)}) scale(${f(k)})">`,
    `    ${markBody(m, v, head)}`,
    `  </g>`,
    `  <g transform="translate(${f(symW + gap)} ${f(-top)})">`,
    wordmarkBody(text).replace(/^/gm, "  "),
    `  </g>`,
  ].join("\n");
  return svg(W, H, body);
}
write(LOGO, "volute-horizontal.svg", lockup(COLOURS.violet, COLOURS.amber, COLOURS.ink));
write(LOGO, "volute-horizontal-reversed.svg", lockup(COLOURS.cream, COLOURS.amber, COLOURS.cream));
write(LOGO, "volute-horizontal-black.svg", lockup("#000000", "#000000", "#000000"));
write(LOGO, "volute-horizontal-white.svg", lockup("#FFFFFF", "#FFFFFF", "#FFFFFF"));

console.log("wrote the kit's SVG masters");
void add;
