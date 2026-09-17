// Original code-generated application icon; no external artwork or runtime dependency.
import { deflateSync } from 'node:zlib';
import { mkdirSync, writeFileSync } from 'node:fs';
const size = 32;
const scan = Buffer.alloc(size * (size * 4 + 1));
for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
  const mark = x >= 8 && x <= 23 && y >= 8 && y <= 23 && (Math.abs(x-y) <= 2 || Math.abs(x+y-31) <= 2);
  const index = y * (size * 4 + 1) + 1 + x * 4;
  scan.set(mark ? [160,157,255,255] : [30,31,42,255], index);
}
function chunk(type, data) {
  const body = Buffer.concat([Buffer.from(type), data]); let crc = 0xffffffff;
  for (const byte of body) { crc ^= byte; for (let n = 0; n < 8; n++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0); }
  const out = Buffer.alloc(data.length + 12); out.writeUInt32BE(data.length); body.copy(out, 4); out.writeUInt32BE((crc ^ 0xffffffff) >>> 0, out.length - 4); return out;
}
const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(size); ihdr.writeUInt32BE(size, 4); ihdr[8] = 8; ihdr[9] = 6;
const png = Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk('IHDR',ihdr),chunk('IDAT',deflateSync(scan)),chunk('IEND',Buffer.alloc(0))]);
const ico = Buffer.alloc(22); ico.writeUInt16LE(1,2); ico.writeUInt16LE(1,4); ico[6] = size; ico[7] = size; ico.writeUInt16LE(1,10); ico.writeUInt16LE(32,12); ico.writeUInt32LE(png.length,14); ico.writeUInt32LE(22,18);
const directory = new URL('../apps/desktop/src-tauri/icons/', import.meta.url); mkdirSync(directory, {recursive:true});
writeFileSync(new URL('icon.png', directory), png); writeFileSync(new URL('icon.ico', directory), Buffer.concat([ico,png]));
