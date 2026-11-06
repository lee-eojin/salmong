#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { argv, exit, version, versions } from "node:process";
import { brotliCompressSync, constants } from "node:zlib";

const BROTLI_QUALITY = 11;
const BROTLI_LGWIN = 24;
const BUILD_LOCK = ".trunk-lock";
const TRUNK_HASH = /-[0-9a-f]{16}(?=(_bg)?\.(js|wasm)$)/;
const URLS = /["'](\/[^"']*|[a-z]+:\/\/[^"']*)["']/g;
const COLUMNS = ["raw", "gzip", "brotli"];

function fail(message) {
  console.error(`FAIL: ${message}`);
  exit(1);
}

function run(command, ...args) {
  return execFileSync(command, args, { encoding: "utf8" });
}

function firstLine(command, ...args) {
  return run(command, ...args).split("\n")[0].trim();
}

function listFiles(dist) {
  const paths = [];
  for (const entry of readdirSync(dist, { recursive: true, withFileTypes: true })) {
    if (entry.isDirectory()) continue;
    const path = relative(dist, join(entry.parentPath, entry.name));
    if (path === BUILD_LOCK) continue;
    if (path.startsWith(".")) fail(`${path} is hidden, so dist is from a failed build`);
    if (!entry.isFile()) fail(`${path} is not a regular file`);
    paths.push(path);
  }
  return paths;
}

function checkReferences(dist, paths) {
  if (!paths.includes("index.html")) fail(`${dist} has no index.html`);
  const html = readFileSync(join(dist, "index.html"), "utf8");
  const urls = [...html.matchAll(URLS)].map((match) => match[1]);
  for (const url of urls) {
    if (!url.startsWith("/") || url.startsWith("//")) fail(`index.html loads ${url} from outside dist`);
    if (!paths.includes(url.slice(1))) fail(`index.html loads ${url}, which is not in dist`);
  }
  if (!urls.some((url) => url.endsWith(".wasm"))) fail("index.html loads no wasm");
}

function gzipSize(bytes) {
  return execFileSync("gzip", ["-9", "-n", "-c"], { input: bytes, maxBuffer: Infinity }).length;
}

function brotliSize(bytes) {
  return brotliCompressSync(bytes, {
    params: {
      [constants.BROTLI_PARAM_QUALITY]: BROTLI_QUALITY,
      [constants.BROTLI_PARAM_LGWIN]: BROTLI_LGWIN,
    },
  }).length;
}

function stableName(path) {
  return path.replace(TRUNK_HASH, "");
}

function measure(dist, path) {
  const bytes = readFileSync(join(dist, path));
  return {
    name: stableName(path),
    sha256: createHash("sha256").update(bytes).digest("hex"),
    raw: bytes.length,
    gzip: gzipSize(bytes),
    brotli: brotliSize(bytes),
  };
}

function byName(a, b) {
  return a.name < b.name ? -1 : 1;
}

function table(files, total) {
  const rows = [{ name: "file", raw: "raw", gzip: "gzip", brotli: "brotli" }, ...files, { name: "total", ...total }];
  const width = Math.max(...rows.map((row) => row.name.length));
  return rows
    .map((row) => [row.name.padEnd(width), ...COLUMNS.map((column) => String(row[column]).padStart(10))].join("  "))
    .join("\n");
}

if (argv.length !== 3) {
  console.error("usage: measure-size.mjs <dist>");
  exit(2);
}
const dist = argv[2];
if (!existsSync(dist)) fail(`${dist} does not exist`);

const paths = listFiles(dist);
checkReferences(dist, paths);
const names = paths.map(stableName);
const repeated = names.find((name, index) => names.indexOf(name) !== index);
if (repeated) fail(`dist has more than one ${repeated}`);
const files = paths.map((path) => measure(dist, path)).sort(byName);

const total = Object.fromEntries(
  COLUMNS.map((column) => [column, files.reduce((sum, file) => sum + file[column], 0)]),
);
const record = {
  schema: 1,
  commit: firstLine("git", "rev-parse", "HEAD"),
  dirty: run("git", "status", "--porcelain").length > 0,
  wasm_opt: readFileSync("web/index.html", "utf8").match(/data-wasm-opt="([^"]*)"/)?.[1] ?? "default",
  tools: {
    rustc: firstLine("rustc", "-V"),
    trunk: firstLine("trunk", "--version"),
    wasm_bindgen: firstLine("wasm-bindgen", "--version"),
    gzip: firstLine("gzip", "--version"),
    node: version,
    brotli: versions.brotli,
  },
  method: {
    gzip: "gzip -9 -n",
    brotli: { quality: BROTLI_QUALITY, lgwin: BROTLI_LGWIN },
  },
  files,
  total,
};

console.log(table(files, total));
console.log(JSON.stringify(record));
