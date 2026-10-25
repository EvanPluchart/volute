// The tray icon (decision 0013): a speaker, as Windows draws its own, whose waves are amber and
// follow the master volume. White for a dark taskbar, ink for a light one; a red cross when
// muted. 32-unit grid; run from assets/brand: node src/tray.mjs, then sh src/tray.sh.
import fs from 'fs';

const AMBER = '#FFB020', DANGER = '#FF4D5E';
const INK = { white: '#FFFFFF', ink: '#1B1530' };

// Magnet and cone, filled; the same-colour outline rounds the corners.
const speaker = (c) =>
  `<path d="M2.5 12.2 H6.5 L12.5 7 V25 L6.5 19.8 H2.5 Z" fill="${c}" stroke="${c}" stroke-width="2.4" stroke-linejoin="round"/>`;
// Arcs around the cone's mouth, ±43°.
const arc = (r) => {
  const a = Math.PI / 4.2, cx = 12.5, cy = 16;
  const x = (cx + r * Math.cos(a)).toFixed(2), y1 = (cy - r * Math.sin(a)).toFixed(2), y2 = (cy + r * Math.sin(a)).toFixed(2);
  return `M${x} ${y1} A${r} ${r} 0 0 1 ${x} ${y2}`;
};
const waves = (n) => [5.5, 10.5, 15.5].slice(0, n)
  .map((r) => `<path d="${arc(r)}" fill="none" stroke="${AMBER}" stroke-width="2.3" stroke-linecap="round"/>`).join('');
const cross = `<path d="M18.5 12 L26 19.5 M26 12 L18.5 19.5" stroke="${DANGER}" stroke-width="2.8" stroke-linecap="round"/>`;

fs.mkdirSync('icons/tray', { recursive: true });
for (const [name, colour] of Object.entries(INK)) {
  for (const state of ['0', '1', '2', '3', 'muted']) {
    const body = speaker(colour) + (state === 'muted' ? cross : waves(Number(state)));
    fs.writeFileSync(`icons/tray/speaker-${name}-${state}.svg`,
      `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><title>Volute</title>${body}</svg>\n`);
  }
}
console.log('tray: 10 SVGs in icons/tray');
