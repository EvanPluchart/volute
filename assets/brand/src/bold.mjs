// Bold direction of concept B: a heavy V whose right arm ends in a round scroll head, like a
// violin's, in the accent colour, a volute cut into it. `node bold.mjs <outdir>` writes the
// colour explorations.
import { mkdirSync, writeFileSync } from "node:fs";
import { f, add, mul, rad, u, line, stroke, spiral } from "./geometry.mjs";

/**
 * The mark in its own units (V vertex at the origin): the two arms as monoline paths, and the
 * head: a disk with a spiral groove, the groove unwinding from the centre to near the rim.
 */
export function mark({
  w = 54, theta = rad(20), armLen = 188, headR = 66, headAt = 178,
  groove = 15, turns = 5, tailAngle = 315, ring = 9,
} = {}) {
  const up = (a) => [Math.sin(a), -Math.cos(a)];
  const vertex = [0, 0];
  const leftTop = add(vertex, mul([-Math.sin(theta), -Math.cos(theta)], armLen));
  const dir = up(theta);
  const head = add(vertex, mul(dir, headAt));
  // The right arm stops a ring short of the head, on a concave arc: the head sits on it like a knob.
  const rightArm = armToHead(vertex, dir, w, head, headR + ring);
  // The groove: an outward spiral of quarter arcs, centred on the head, whose tail leaves along
  // its end tangent and stops a groove's width short of the rim.
  const tEnd = rad(tailAngle);
  const t0 = tEnd - rad(90) * turns;
  const r0 = groove * 0.6;
  const dr = (headR - groove * 1.45 - r0) / turns;
  let segs = spiral([0, 0], r0, t0, true, dr, turns);
  // Centre the spiral's turns on the head.
  const pts = segs.flatMap((s) => [0, 0.5, 1].map((k) => add(s.c, mul(u(s.t0 + (s.t1 - s.t0) * k), s.r))));
  const cx = (Math.min(...pts.map((p) => p[0])) + Math.max(...pts.map((p) => p[0]))) / 2;
  const cy = (Math.min(...pts.map((p) => p[1])) + Math.max(...pts.map((p) => p[1]))) / 2;
  const move = (s) => ({ ...s, c: add(s.c, [head[0] - cx, head[1] - cy]) });
  segs = segs.map(move);
  const last = segs[segs.length - 1];
  const end = add(last.c, mul(u(last.t1), last.r));
  const tangent = u(last.t1 + rad(90));
  let t = 0;
  while (Math.hypot(...add(add(end, mul(tangent, t)), mul(head, -1))) < headR - groove * 1.15) t += 0.25;
  segs.push(line(end, add(end, mul(tangent, t))));
  return {
    w,
    arms: [[line(leftTop, vertex)]],
    rightArm,
    head: { c: head, r: headR },
    groove: { segs, w: groove },
  };
}

/**
 * Outline of an arm of width w from `from` along `dir`, round at `from`, ending on the circle
 * (c, R) with a concave edge that follows the circle.
 */
function armToHead(from, dir, w, c, R) {
  const h = w / 2;
  const n = [dir[1], -dir[0]];
  // Where a side line from + k·h·n + t·dir meets the circle (nearest intersection).
  const hit = (k) => {
    const o = add(from, mul(n, k * h));
    const oc = [o[0] - c[0], o[1] - c[1]];
    const b = oc[0] * dir[0] + oc[1] * dir[1];
    const cc = oc[0] * oc[0] + oc[1] * oc[1] - R * R;
    const t = -b - Math.sqrt(b * b - cc);
    return add(o, mul(dir, t));
  };
  const a = add(from, mul(n, h)), b = add(from, mul(n, -h));
  const A = hit(1), B = hit(-1);
  const p = (q) => `${f(q[0])} ${f(q[1])}`;
  return `M ${p(b)} A ${f(h)} ${f(h)} 0 0 1 ${p(a)} L ${p(A)} A ${f(R)} ${f(R)} 0 0 1 ${p(B)} Z`;
}

const circle = (c, r) => `M ${f(c[0] - r)} ${f(c[1])} A ${f(r)} ${f(r)} 0 1 0 ${f(c[0] + r)} ${f(c[1])} A ${f(r)} ${f(r)} 0 1 0 ${f(c[0] - r)} ${f(c[1])} Z`;

/** Bounds of the mark: the arms' outlines and the head. */
function bounds(m) {
  // The right arm mirrors the left one up to the head.
  const pts = m.arms.flat().flatMap((s) => [s.a, s.b, [-s.a[0], s.a[1]]]);
  const h = m.w / 2;
  let x0 = Math.min(...pts.map((p) => p[0])) - h, x1 = Math.max(...pts.map((p) => p[0])) + h;
  let y0 = Math.min(...pts.map((p) => p[1])) - h, y1 = Math.max(...pts.map((p) => p[1])) + h;
  const { c, r } = m.head;
  return { x0: Math.min(x0, c[0] - r), x1: Math.max(x1, c[0] + r), y0: Math.min(y0, c[1] - r), y1: Math.max(y1, c[1] + r) };
}

/** A transform fitting the mark into a square of `size` centred at (cx, cy), lifted optically. */
export function fit(m, cx, cy, size, lift = 0) {
  const b = bounds(m);
  const k = size / Math.max(b.x1 - b.x0, b.y1 - b.y0);
  return `translate(${f(cx - ((b.x0 + b.x1) / 2) * k)} ${f(cy - lift - ((b.y0 + b.y1) / 2) * k)}) scale(${f(k)})`;
}

/**
 * The mark's shapes: arms in `v`, the head in `head`. What is cut out of the head (a real hole):
 * "groove", the volute (48 px and up); "none", a plain disk (16 to 32 px); "slash", the mute bar.
 */
export function markBody(m, v, head, cut = "groove") {
  const { c, r } = m.head;
  const holes = {
    groove: stroke(m.groove.segs, m.groove.w),
    none: "",
    slash: stroke([line(add(c, mul([-1, -1], r * 0.42)), add(c, mul([1, 1], r * 0.42)))], m.groove.w * 1.6),
  }[cut];
  return [
    ...m.arms.map((p) => `<path fill="${v}" d="${stroke(p, m.w)}"/>`),
    `<path fill="${v}" d="${m.rightArm}"/>`,
    `<path fill="${head}" fill-rule="evenodd" d="${circle(c, r)} ${holes}"/>`,
  ].join("\n    ");
}

export { bounds };

export function tile(m, { bg, v, head }, size = 256, scale = 0.66, cut = "groove") {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}">
  <title>Volute</title>
  <rect width="${size}" height="${size}" rx="${f(size * 0.225)}" fill="${bg}"/>
  <g transform="${fit(m, size / 2, size / 2, size * scale, size * 0.01)}">
    ${markBody(m, v, head, cut)}
  </g>
</svg>
`;
}

export const PALETTES = {
  "ink-amber": { bg: "#1B1530", v: "#FFF1DE", head: "#FFB020" },
  "orange-cream": { bg: "#FF5A1F", v: "#FFF1DE", head: "#1B1530" },
  "amber-ink": { bg: "#FFB020", v: "#1B1530", head: "#FF3D6E" },
  "violet-amber": { bg: "#5B2BFF", v: "#FFF1DE", head: "#FFB020" },
};

if (process.argv[1] && import.meta.url.endsWith(process.argv[1].split(/[\\/]/).pop())) {
  const out = process.argv[2] ?? ".";
  mkdirSync(out, { recursive: true });
  const m = mark();
  for (const [name, p] of Object.entries(PALETTES)) writeFileSync(`${out}/tile-${name}.svg`, tile(m, p));
  console.log(`wrote ${Object.keys(PALETTES).length} tiles to ${out}`);
}
