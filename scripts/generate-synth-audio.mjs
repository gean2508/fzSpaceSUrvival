#!/usr/bin/env node
/**
 * Generate Blood Dragon synthwave WAV assets for mobile + static hosting.
 * Run: node scripts/generate-synth-audio.mjs
 */
import { writeFileSync, mkdirSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const OUT_DIRS = [
  resolve(ROOT, "client/public/audio"),
  resolve(ROOT, "apps/mobile/assets/audio"),
];

const SAMPLE_RATE = 44100;
const BPM = 118;

function osc(t, freq, wave, gain = 0.3) {
  const p = (t * freq) % 1;
  if (wave === "sine") return gain * Math.sin(2 * Math.PI * p);
  if (wave === "saw") return gain * (2 * p - 1);
  return gain * (p < 0.5 ? 1 : -1);
}

function env(t, attack, release, len) {
  if (t < 0) return 0;
  if (t < attack) return t / attack;
  if (t > len - release) return Math.max(0, (len - t) / release);
  return 1;
}

function renderLoop(seconds = 8) {
  const len = Math.floor(SAMPLE_RATE * seconds);
  const out = new Float32Array(len);
  const beat = 60 / BPM;
  const progression = [
    [110, 130.81, 164.81],
    [87.31, 110, 130.81],
    [130.81, 164.81, 196],
    [98, 123.47, 146.83],
  ];

  for (let i = 0; i < len; i++) {
    const t = i / SAMPLE_RATE;
    const beatT = t % (beat * 16);
    const bar = Math.floor(t / (beat * 4)) % 4;
    const chord = progression[bar];
    const bass = chord[0];
    let s = 0;
    s += osc(t, bass * 0.5, "saw", 0.14);
    s += osc(t, bass, "square", 0.08);
    const step = Math.floor(beatT / (beat / 2)) % 8;
    const note = chord[step % 3] * (step > 5 ? 2 : 1);
    const arpLen = beat / 2;
    const arpT = beatT % arpLen;
    s += osc(t, note, "square", 0.06 * env(arpT, 0.008, 0.04, arpLen));
    const kickPhase = beatT % beat;
    if (kickPhase < 0.05) s += 0.35 * (1 - kickPhase / 0.05);
    s += chord.map((f) => osc(t, f, "sine", 0.018)).reduce((a, b) => a + b, 0);
    out[i] = Math.tanh(s * 1.4) * 0.55;
  }
  return out;
}

function renderClick() {
  const len = Math.floor(SAMPLE_RATE * 0.12);
  const out = new Float32Array(len);
  for (let i = 0; i < len; i++) {
    const t = i / SAMPLE_RATE;
    out[i] = osc(t, 880, "square", 0.25 * (1 - t / 0.12)) + osc(t, 1760, "sine", 0.1 * (1 - t / 0.12));
  }
  return out;
}

function floatToWav(samples) {
  const buf = Buffer.alloc(44 + samples.length * 2);
  buf.write("RIFF", 0);
  buf.writeUInt32LE(36 + samples.length * 2, 4);
  buf.write("WAVE", 8);
  buf.write("fmt ", 12);
  buf.writeUInt32LE(16, 16);
  buf.writeUInt16LE(1, 20);
  buf.writeUInt16LE(1, 22);
  buf.writeUInt32LE(SAMPLE_RATE, 24);
  buf.writeUInt32LE(SAMPLE_RATE * 2, 28);
  buf.writeUInt16LE(2, 32);
  buf.writeUInt16LE(16, 34);
  buf.write("data", 36);
  buf.writeUInt32LE(samples.length * 2, 40);
  for (let i = 0; i < samples.length; i++) {
    const v = Math.max(-1, Math.min(1, samples[i]));
    buf.writeInt16LE(Math.round(v * 32767), 44 + i * 2);
  }
  return buf;
}

const menuLoop = renderLoop(8);
const click = renderClick();

for (const dir of OUT_DIRS) {
  mkdirSync(dir, { recursive: true });
  writeFileSync(resolve(dir, "menu-loop.wav"), floatToWav(menuLoop));
  writeFileSync(resolve(dir, "click.wav"), floatToWav(click));
  console.log(`✓ ${dir}`);
}

console.log("✓ Synthwave WAV assets generated (menu-loop.wav, click.wav)");
