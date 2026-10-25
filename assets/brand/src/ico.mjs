// Packs PNG files into one .ico (PNG-compressed entries, Windows Vista and later):
// node ico.mjs out.ico a-16.png a-32.png ...
import { readFileSync, writeFileSync } from "node:fs";

const [out, ...inputs] = process.argv.slice(2);
const images = inputs.map((path) => {
  const data = readFileSync(path);
  // Width and height from the PNG header (IHDR).
  return { data, width: data.readUInt32BE(16), height: data.readUInt32BE(20) };
});
const header = Buffer.alloc(6 + 16 * images.length);
header.writeUInt16LE(0, 0);
header.writeUInt16LE(1, 2); // icon
header.writeUInt16LE(images.length, 4);
let offset = header.length;
images.forEach((img, i) => {
  const e = 6 + 16 * i;
  header.writeUInt8(img.width >= 256 ? 0 : img.width, e);
  header.writeUInt8(img.height >= 256 ? 0 : img.height, e + 1);
  header.writeUInt16LE(1, e + 4); // planes
  header.writeUInt16LE(32, e + 6); // bits per pixel
  header.writeUInt32LE(img.data.length, e + 8);
  header.writeUInt32LE(offset, e + 12);
  offset += img.data.length;
});
writeFileSync(out, Buffer.concat([header, ...images.map((i) => i.data)]));
console.log(`wrote ${out} (${images.map((i) => i.width).join(", ")})`);
