// Generate LalaLM app icon (PNG + macOS icns) without external deps.
// Usage: node scripts/gen-icon.mjs
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";
import { join } from "node:path";

const SIZE = 1024;
const SS = 4; // supersampling factor

function clamp01(v) { return Math.max(0, Math.min(1, v)); }
function lerp(a, b, t) { return a + (b - a) * t; }

function sdRoundRect(px, py, cx, cy, hw, hh, r) {
  const qx = Math.abs(px - cx) - (hw - r);
  const qy = Math.abs(py - cy) - (hh - r);
  const ox = Math.max(qx, 0), oy = Math.max(qy, 0);
  return Math.hypot(ox, oy) + Math.min(Math.max(qx, qy), 0) - r;
}

function pointInPoly(px, py, poly) {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i], [xj, yj] = poly[j];
    if ((yi > py) !== (yj > py) &&
        px < ((xj - xi) * (py - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

// Lightning bolt polygon (normalized 0..1 coords).
const BOLT = [
  [0.600, 0.100], [0.295, 0.545], [0.462, 0.545],
  [0.400, 0.900], [0.705, 0.430], [0.520, 0.430],
];

function pixelColor(nx, ny) {
  // Background gradient (indigo -> cyan, top-left -> bottom-right)
  const t = clamp01((nx + ny) / 2);
  const bg = {
    r: lerp(0x6b / 255, 0x06 / 255, t),
    g: lerp(0x74 / 255, 0xb6 / 255, t),
    b: lerp(0xf1 / 255, 0xd4 / 255, t),
  };
  // Rounded square mask
  const d = sdRoundRect(nx, ny, 0.5, 0.5, 0.46, 0.46, 0.215);
  if (d > 0.004) return [0, 0, 0, 0]; // transparent outside
  const alpha = clamp01(0.5 - d / 0.008);
  // White bolt
  const boltHit = pointInPoly(nx, ny, BOLT);
  if (boltHit) return [255, 255, 255, Math.round(alpha * 255)];
  // Soft highlight overlay on upper half
  const hl = clamp01(0.10 * (1 - ny));
  return [
    Math.round(lerp(bg.r * 255, 255, hl)),
    Math.round(lerp(bg.g * 255, 255, hl)),
    Math.round(lerp(bg.b * 255, 255, hl)),
    Math.round(alpha * 255),
  ];
}

function render(size) {
  const rows = [];
  const step = 1 / size;
  for (let y = 0; y < size; y++) {
    const row = Buffer.alloc(1 + size * 4);
    row[0] = 0; // no filter
    for (let x = 0; x < size; x++) {
      let r = 0, g = 0, b = 0, a = 0;
      for (let sy = 0; sy < SS; sy++) {
        for (let sx = 0; sx < SS; sx++) {
          const nx = (x + (sx + 0.5) / SS) * step;
          const ny = (y + (sy + 0.5) / SS) * step;
          const [pr, pg, pb, pa] = pixelColor(nx, ny);
          r += pr; g += pg; b += pb; a += pa;
        }
      }
      const n = SS * SS;
      const o = 1 + x * 4;
      row[o] = Math.round(r / n);
      row[o + 1] = Math.round(g / n);
      row[o + 2] = Math.round(b / n);
      row[o + 3] = Math.round(a / n);
    }
    rows.push(row);
  }
  return Buffer.concat(rows);
}

// ---- PNG encoding ----
const CRC_TABLE = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();
function crc32(buf) {
  let c = 0xffffffff;
  for (const byte of buf) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
}
function encodePng(raw, w, h) {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; ihdr[9] = 6; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

console.log(`rendering ${SIZE}x${SIZE} (supersample ${SS}x)...`);
const raw = render(SIZE);
mkdirSync("src-tauri/icons", { recursive: true });
writeFileSync("src-tauri/icons/icon.png", encodePng(raw, SIZE, SIZE));
console.log("wrote src-tauri/icons/icon.png");

// macOS icns via sips + iconutil
try {
  const setDir = "src-tauri/icons/icon.iconset";
  mkdirSync(setDir, { recursive: true });
  const sizes = [16, 32, 64, 128, 256, 512, 1024];
  for (const s of sizes) {
    execSync(
      `sips -z ${s} ${s} src-tauri/icons/icon.png --out ${setDir}/icon_${s}x${s}.png >/dev/null`
    );
    const half = s / 2;
    if ([16, 32, 128, 256, 512].includes(s)) {
      execSync(
        `sips -z ${s} ${s} src-tauri/icons/icon.png --out ${setDir}/icon_${half}x${half}@2x.png >/dev/null`
      );
    }
  }
  execSync(`iconutil -c icns ${setDir} -o src-tauri/icons/icon.icns`);
  console.log("wrote src-tauri/icons/icon.icns");
} catch (e) {
  console.warn("icns generation skipped:", e.message);
}

// Windows .ico (PNG-compressed entries — valid since Vista).
try {
  const sizes = [256, 48, 32, 16];
  const pngs = [];
  for (const s of sizes) {
    pngs.push({ size: s, data: encodePng(render(s), s, s) });
  }
  // ICONDIR (6 bytes) + ICONDIRENTRY (16 bytes each)
  let offset = 6 + 16 * pngs.length;
  const dir = Buffer.alloc(6);
  dir.writeUInt16LE(0, 0); // reserved
  dir.writeUInt16LE(1, 2); // type: icon
  dir.writeUInt16LE(pngs.length, 4);
  const entries = [];
  for (const { size, data } of pngs) {
    const e = Buffer.alloc(16);
    e.writeUInt8(size === 256 ? 0 : size, 0); // width
    e.writeUInt8(size === 256 ? 0 : size, 1); // height
    e.writeUInt8(0, 2); // palette
    e.writeUInt8(0, 3); // reserved
    e.writeUInt16LE(1, 4); // planes
    e.writeUInt16LE(32, 6); // bpp
    e.writeUInt32LE(data.length, 8);
    e.writeUInt32LE(offset, 12);
    offset += data.length;
    entries.push(e);
  }
  const ico = Buffer.concat([dir, ...entries, ...pngs.map((p) => p.data)]);
  writeFileSync("src-tauri/icons/icon.ico", ico);
  console.log("wrote src-tauri/icons/icon.ico");
} catch (e) {
  console.warn("ico generation skipped:", e.message);
}
