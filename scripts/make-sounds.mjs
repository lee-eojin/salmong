#!/usr/bin/env node
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { argv, exit } from "node:process";

const RATE = 22050n;
const PEAK = 16384n;
const ATTACK = 44n;
const HEADER = 44;

const SOUNDS = [
  { name: "board", samples: 1102, wave: "triangle", notes: [440n] },
  { name: "deliver", samples: 2646, wave: "triangle", notes: [660n, 880n] },
  { name: "station", samples: 6615, wave: "triangle", notes: [523n, 659n, 784n] },
  { name: "overflow", samples: 6615, wave: "square", notes: [196n, 196n] },
  { name: "reject", samples: 3307, wave: "square", notes: [147n] },
  { name: "over", samples: 19845, wave: "triangle", notes: [392n, 330n, 262n, 196n] },
];

function triangle(phase) {
  const q = 4n * phase;
  if (q < RATE) return q;
  if (q < 3n * RATE) return 2n * RATE - q;
  return q - 4n * RATE;
}

function square(phase) {
  return 2n * phase < RATE ? RATE : -RATE;
}

function envelope(at, length) {
  if (at < ATTACK) return [at ** 4n, ATTACK ** 4n];
  return [length - 1n - at, length - 1n - ATTACK];
}

function note(wave, hz, length) {
  const shape = wave === "square" ? square : triangle;
  const out = [];
  let phase = 0n;
  for (let at = 0n; at < length; at += 1n) {
    const [gain, full] = envelope(at, length);
    out.push(Number((PEAK * shape(phase) * gain) / (RATE * full)));
    phase = (phase + hz) % RATE;
  }
  return out;
}

function render({ samples, wave, notes }) {
  const total = BigInt(samples);
  const count = BigInt(notes.length);
  const out = [];
  notes.forEach((hz, k) => {
    const from = (BigInt(k) * total) / count;
    const to = (BigInt(k + 1) * total) / count;
    out.push(...note(wave, hz, to - from));
  });
  return out;
}

function encode(samples) {
  const data = samples.length * 2;
  const file = Buffer.alloc(HEADER + data);
  file.write("RIFF", 0, "ascii");
  file.writeUInt32LE(HEADER - 8 + data, 4);
  file.write("WAVE", 8, "ascii");
  file.write("fmt ", 12, "ascii");
  file.writeUInt32LE(16, 16);
  file.writeUInt16LE(1, 20);
  file.writeUInt16LE(1, 22);
  file.writeUInt32LE(Number(RATE), 24);
  file.writeUInt32LE(Number(RATE) * 2, 28);
  file.writeUInt16LE(2, 32);
  file.writeUInt16LE(16, 34);
  file.write("data", 36, "ascii");
  file.writeUInt32LE(data, 40);
  samples.forEach((value, k) => file.writeInt16LE(value, HEADER + 2 * k));
  return file;
}

if (argv.length !== 3) {
  console.error("usage: scripts/make-sounds.mjs <dir>");
  exit(2);
}

const dir = argv[2];
mkdirSync(dir, { recursive: true });
for (const sound of SOUNDS) {
  const samples = render(sound);
  if (samples.length !== sound.samples) {
    console.error(`FAIL: ${sound.name} has ${samples.length} samples, expected ${sound.samples}`);
    exit(1);
  }
  writeFileSync(join(dir, `${sound.name}.wav`), encode(samples));
}
