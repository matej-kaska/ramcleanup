// Build-time SVG rasterizer only. Every ICO frame is an uncompressed bitmap.
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const modulePath = process.env.RAMCLEANUP_ICON_MODULES || resolve(root, '.tools/icon-tools/package.json');
const { Resvg } = createRequire(modulePath)('@resvg/resvg-js');
const svg = readFileSync(resolve(root, 'assets/icon.svg'), 'utf8');
const sizes = [16, 20, 24, 32, 40, 48];
const frames = sizes.map(size => {
  const pixels = new Resvg(svg, { fitTo: { mode: 'width', value: size } }).render().pixels;
  const stride = Math.ceil(size / 32) * 4;
  const mask = 40 + size * size * 4;
  const data = Buffer.alloc(mask + stride * size);
  data.writeUInt32LE(40, 0);
  data.writeInt32LE(size, 4);
  data.writeInt32LE(size * 2, 8);
  data.writeUInt16LE(1, 12);
  data.writeUInt16LE(32, 14);
  data.writeUInt32LE(size * size * 4, 20);
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const src = (y * size + x) * 4;
    const dest = 40 + ((size - y - 1) * size + x) * 4;
    const alpha = pixels[src + 3];
    for (let channel = 0; channel < 3; channel++) {
      data[dest + channel] = alpha ? Math.min(255, Math.round(pixels[src + 2 - channel] * 255 / alpha)) : 0;
    }
    data[dest + 3] = alpha;
    if (!alpha) data[mask + (size - y - 1) * stride + (x >> 3)] |= 0x80 >> (x & 7);
  }
  return data;
});
const header = Buffer.alloc(6 + sizes.length * 16);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(sizes.length, 4);
let offset = header.length;
sizes.forEach((size, i) => {
  const p = 6 + i * 16;
  header[p] = header[p + 1] = size;
  header.writeUInt16LE(1, p + 4);
  header.writeUInt16LE(32, p + 6);
  header.writeUInt32LE(frames[i].length, p + 8);
  header.writeUInt32LE(offset, p + 12);
  offset += frames[i].length;
});
writeFileSync(resolve(root, 'assets/icon.ico'), Buffer.concat([header, ...frames]));
console.log(`Bitmap-only ICO: ${offset} bytes`);
if (process.argv.includes('--preview')) {
  writeFileSync(resolve(root, 'dist/icon-preview.png'),
    new Resvg(svg, { fitTo: { mode: 'width', value: 256 } }).render().asPng());
}
