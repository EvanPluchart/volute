// Exact geometry for Volute's logo: monoline paths of straight segments and circular arcs,
// turned into filled outlines (offset on both sides, round caps). Offsets of an arc are arcs
// about the same centre, so the outlines are exact. Used by bold.mjs and kit.mjs.

const f = (n) => String(Number(n.toFixed(2)));
const add = (a, b) => [a[0] + b[0], a[1] + b[1]];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1]];
const mul = (a, k) => [a[0] * k, a[1] * k];
const len = (a) => Math.hypot(a[0], a[1]);
const norm = (a) => mul(a, 1 / len(a));
const rad = (deg) => (deg * Math.PI) / 180;
// Screen coordinates, y down: a growing angle turns clockwise on screen.
const u = (t) => [Math.cos(t), Math.sin(t)];
const P = (p) => `${f(p[0])} ${f(p[1])}`;

const line = (a, b) => ({ kind: "line", a, b });
// An arc about c of radius r from angle t0 to t1 (cw when t1 > t0).
const arc = (c, r, t0, t1) => ({ kind: "arc", c, r, t0, t1 });

const startOf = (s) => (s.kind === "line" ? s.a : add(s.c, mul(u(s.t0), s.r)));
const endOf = (s) => (s.kind === "line" ? s.b : add(s.c, mul(u(s.t1), s.r)));
const dirAt = (s, end) => {
  if (s.kind === "line") return norm(sub(s.b, s.a));
  const t = end ? s.t1 : s.t0;
  const cw = s.t1 > s.t0;
  return cw ? u(t + rad(90)) : u(t - rad(90));
};
// Left of travel on screen: travelling right, left is up.
const leftOf = (d) => [d[1], -d[0]];

/** Filled outline of a tangent-continuous monoline path of width w with round caps. */
function stroke(segs, w) {
  const h = w / 2;
  const side = (s, k, end) => {
    if (s.kind === "line") return add(end ? s.b : s.a, mul(leftOf(dirAt(s, end)), k * h));
    const cw = s.t1 > s.t0;
    const r = s.r + (cw ? k * h : -k * h);
    return add(s.c, mul(u(end ? s.t1 : s.t0), r));
  };
  const arcTo = (s, k, end, reverse) => {
    const cw = s.t1 > s.t0;
    const r = s.r + (cw ? k * h : -k * h);
    if (r <= 0.5) throw new Error(`offset radius ${r} too small`);
    const sweep = cw !== reverse ? 1 : 0;
    const large = Math.abs(s.t1 - s.t0) > Math.PI ? 1 : 0;
    return ` A ${f(r)} ${f(r)} 0 ${large} ${sweep} ${P(side(s, k, end))}`;
  };
  let d = `M ${P(side(segs[0], 1, false))}`;
  for (const s of segs) d += s.kind === "line" ? ` L ${P(side(s, 1, true))}` : arcTo(s, 1, true, false);
  const last = segs[segs.length - 1];
  d += ` A ${f(h)} ${f(h)} 0 0 1 ${P(side(last, -1, true))}`;
  for (const s of [...segs].reverse()) d += s.kind === "line" ? ` L ${P(side(s, -1, false))}` : arcTo(s, -1, false, true);
  d += ` A ${f(h)} ${f(h)} 0 0 1 ${P(side(segs[0], 1, false))} Z`;
  return d;
}

/**
 * Quarter arcs whose radius changes by dr each quarter: an evenly spaced spiral. Starts on the
 * circle (c, r) at angle t0. Continuity: c_next = c + (r - r_next)·u(t_boundary).
 */
function spiral(c, r, t0, cw, dr, quarters) {
  const s = cw ? 1 : -1;
  const segs = [];
  let t = t0, centre = c, radius = r, left = quarters;
  while (left > 1e-6) {
    const step = Math.min(1, left);
    const t1 = t + s * rad(90) * step;
    segs.push(arc(centre, radius, t, t1));
    const next = radius + dr;
    centre = add(centre, mul(u(t1), radius - next));
    radius = next;
    t = t1;
    left -= step;
  }
  return segs;
}

const reverseSegs = (segs) =>
  [...segs].reverse().map((s) => (s.kind === "line" ? line(s.b, s.a) : arc(s.c, s.r, s.t1, s.t0)));

/** Bounding box of segments grown by half a stroke (sampled arcs). */
function bbox(segs, w) {
  const pts = [];
  for (const s of segs) {
    if (s.kind === "line") pts.push(s.a, s.b);
    else for (let i = 0; i <= 24; i++) pts.push(add(s.c, mul(u(s.t0 + ((s.t1 - s.t0) * i) / 24), s.r)));
  }
  const xs = pts.map((p) => p[0]), ys = pts.map((p) => p[1]), h = w / 2;
  return { x0: Math.min(...xs) - h, y0: Math.min(...ys) - h, x1: Math.max(...xs) + h, y1: Math.max(...ys) + h };
}

const shift = (segs, v) =>
  segs.map((s) => (s.kind === "line" ? line(add(s.a, v), add(s.b, v)) : arc(add(s.c, v), s.r, s.t0, s.t1)));

/**
 * The V of Volute: a straight left arm, and a right arm that rolls outward into a violin scroll.
 * Returns the two monoline paths (left arm; right arm + scroll), in mark units.
 */
function vee({ w, theta, leftLen, rightLen, R, gap, quarters }) {
  const vertex = [0, 0];
  const up = (a) => [Math.sin(a), -Math.cos(a)];
  const leftTop = add(vertex, mul([-Math.sin(theta), -Math.cos(theta)], leftLen));
  const dir = up(theta);
  const top = add(vertex, mul(dir, rightLen));
  // Clockwise roll: tangent at `top` is the arm's direction, so t0 = angle(dir) - 90°.
  const t0 = Math.atan2(dir[1], dir[0]) - rad(90);
  const c = add(top, mul(u(t0), -R));
  const scroll = spiral(c, R, t0, true, -(w + gap) / 4, quarters);
  return [[line(leftTop, vertex)], [line(vertex, top), ...scroll]];
}

function place(paths, w, size, lift) {
  const all = paths.flat();
  const b = bbox(all, w);
  const v = [size / 2 - (b.x0 + b.x1) / 2, size / 2 - lift - (b.y0 + b.y1) / 2];
  return { paths: paths.map((p) => shift(p, v)), box: { x0: b.x0 + v[0], y0: b.y0 + v[1], x1: b.x1 + v[0], y1: b.y1 + v[1] } };
}

function svg(title, body, w = 256, h = 256) {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${f(w)} ${f(h)}">\n  <title>${title}</title>\n${body}\n</svg>\n`;
}
const pathEl = (d, fill = "#000") => `  <path fill="${fill}" d="${d}"/>`;
const write = (name, text) => writeFileSync(join(OUT, name), text);


export { f, add, sub, mul, len, norm, rad, u, P, line, arc, stroke, spiral, bbox, shift, vee };
