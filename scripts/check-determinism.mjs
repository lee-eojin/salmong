#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { argv, exit } from "node:process";

const expected = [
  14587225508827618376n,
  9340962682666644249n,
  7185562617216877458n,
];

const path = argv[2];
if (!path) {
  console.error("usage: check-determinism.mjs <salmong_core.wasm>");
  exit(2);
}

const bytes = readFileSync(path);
const { instance } = await WebAssembly.instantiate(bytes, {});
const exports = instance.exports;
const count = Number(exports.salmong_empty3_hash_count());
if (count !== expected.length) {
  console.error(`hash count ${count} != ${expected.length}`);
  exit(1);
}
const got = [];
for (let i = 0; i < expected.length; i++) {
  got.push(BigInt.asUintN(64, BigInt(exports.salmong_empty3_hash_at(i))));
}
for (let i = 0; i < expected.length; i++) {
  if (got[i] !== expected[i]) {
    console.error(`hash[${i}] wasm=${got[i]} expected=${expected[i]}`);
    exit(1);
  }
}
console.log(`ok: wasm Node hashes == hand table (${got.join(",")})`);
