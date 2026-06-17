#!/usr/bin/env node
/**
 * Generate stepped grayscale gradient-map PNGs for MeshToonMaterial.
 *
 * These are optional, hand-tunable alternatives to the runtime generator in
 * client/src/game/art/toon.ts. Output: client/public/gradients/toonN.png
 *
 * Zero dependencies: writes a minimal valid PNG by hand (Nx1 grayscale).
 *
 * Usage: node .cursor/skills/ghibli-toon-graphics/scripts/gen-gradient-maps.mjs
 */
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const __dirname = dirname(fileURLToPath(import.meta.url));
const outDir = resolve(__dirname, "../../../../client/public/gradients");

function crc32(buf) {
  let c = ~0;
  for (let i = 0; i < buf.length; i++) {
    c ^= buf[i];
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return ~c >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const body = Buffer.concat([typeBuf, data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([len, body, crc]);
}

/** width x 1 grayscale (8-bit) PNG from an array of 0..255 luminance values. */
function grayPng(values) {
  const w = values.length;
  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(1, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 0; // color type: grayscale
  // 10..12 = compression/filter/interlace = 0
  // One scanline: filter byte (0) + w samples
  const raw = Buffer.concat([Buffer.from([0]), Buffer.from(values)]);
  const idat = deflateSync(raw);
  return Buffer.concat([
    sig,
    chunk("IHDR", ihdr),
    chunk("IDAT", idat),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function ramp(steps) {
  const out = [];
  for (let i = 0; i < steps; i++) {
    const t = i / (steps - 1);
    out.push(Math.round(Math.pow(t, 0.8) * 255));
  }
  return out;
}

mkdirSync(outDir, { recursive: true });
for (const steps of [3, 4, 5]) {
  const file = resolve(outDir, `toon${steps}.png`);
  writeFileSync(file, grayPng(ramp(steps)));
  console.log("wrote", file);
}
console.log("Done. Load with THREE.TextureLoader + NearestFilter.");
