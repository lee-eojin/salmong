#!/usr/bin/env node
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, join, resolve } from "node:path";

const [dir, RUN] = process.argv.slice(2);
const CHROME = process.env.CHROME ?? "google-chrome";
const KEY = "salmong.best";
const TIMEOUT_MS = 30_000;
const MIME = { ".js": "text/javascript", ".wasm": "application/wasm" };
const PAGE = `<!doctype html><script type="module">import init from "/best_probe.js"; await init();</script>`;
const HIGHER = "salmong.best.1 999999 1";
const LOWER = "salmong.best.1 0 1";
const DENY = `Object.defineProperty(window, "localStorage", { get() { throw new DOMException("denied", "SecurityError"); } });`;
const FULL = `Storage.prototype.setItem = () => { throw new DOMException("full", "QuotaExceededError"); };`;

function serve() {
  const server = createServer((req, res) => {
    const { pathname } = new URL(req.url, "http://localhost");
    if (pathname === "/") {
      res.writeHead(200, { "content-type": "text/html" }).end(PAGE);
      return;
    }
    try {
      const body = readFileSync(join(resolve(dir), pathname));
      res.writeHead(200, { "content-type": MIME[extname(pathname)] }).end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((done) => server.listen(0, "127.0.0.1", () => done(server)));
}

function launch(profile) {
  const chrome = spawn(
    CHROME,
    [
      "--headless=new",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--disable-extensions",
      "--disable-gpu",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  const exited = new Promise((done) => chrome.once("close", done));
  const port = new Promise((done, fail) => {
    let log = "";
    chrome.once("error", fail);
    chrome.stderr.on("data", (chunk) => {
      log += chunk;
      const match = log.match(/DevTools listening on ws:\/\/[^:]+:(\d+)\//);
      if (match) done(match[1]);
    });
  });
  return { port, exited, kill: () => chrome.kill("SIGKILL") };
}

async function socketTo(url) {
  const socket = new WebSocket(url);
  await new Promise((done, fail) => {
    socket.addEventListener("open", done, { once: true });
    socket.addEventListener("error", fail, { once: true });
  });
  let id = 0;
  const pending = new Map();
  const handlers = [];
  socket.addEventListener("message", ({ data }) => {
    const message = JSON.parse(data);
    if (message.id !== undefined) {
      pending.get(message.id)(message.result);
      pending.delete(message.id);
    } else {
      for (const handler of handlers) handler(message);
    }
  });
  const send = (method, params = {}) =>
    new Promise((done) => {
      pending.set(++id, done);
      socket.send(JSON.stringify({ id, method, params }));
    });
  return { send, on: (handler) => handlers.push(handler), close: () => socket.close() };
}

async function pageTarget(port) {
  const deadline = Date.now() + TIMEOUT_MS;
  while (Date.now() < deadline) {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
    const page = targets.find((target) => target.type === "page");
    if (page) return page;
    await new Promise((done) => setTimeout(done, 50));
  }
  throw new Error("chrome opened no page");
}

async function open(port) {
  const page = await socketTo((await pageTarget(port)).webSocketDebuggerUrl);
  const state = { lines: [], errors: [] };
  page.on(({ method, params }) => {
    if (method === "Runtime.consoleAPICalled") {
      state.lines.push(params.args.map((arg) => arg.value).join(" "));
    } else if (method === "Runtime.exceptionThrown") {
      state.errors.push(params.exceptionDetails.exception?.description ?? params.exceptionDetails.text);
    }
  });
  await page.send("Runtime.enable");
  await page.send("Page.enable");
  return { page, state };
}

const field = (lines, name) => {
  for (const line of lines) {
    const match = line.match(new RegExp(`best-probe ${name} (none|salmong\\.best\\.1 \\d+ \\d+)`));
    if (match) return match[1];
  }
  return undefined;
};

async function load({ page, state }, url) {
  state.lines.length = 0;
  state.errors.length = 0;
  await page.send("Page.navigate", { url });
  const deadline = Date.now() + TIMEOUT_MS;
  while (field(state.lines, "best") === undefined && Date.now() < deadline) {
    if (state.errors.length > 0) break;
    await new Promise((done) => setTimeout(done, 50));
  }
  return {
    loaded: field(state.lines, "loaded"),
    run: field(state.lines, "run"),
    best: field(state.lines, "best"),
    warned: state.lines.some((line) => line.includes("best score not saved")),
    errors: [...state.errors],
  };
}

const evaluate = async ({ page }, expression) =>
  (await page.send("Runtime.evaluate", { expression, returnByValue: true })).result.value;
const stored = (tab) => evaluate(tab, `localStorage.getItem(${JSON.stringify(KEY)})`);
const seed = (tab, text) =>
  evaluate(tab, `localStorage.setItem(${JSON.stringify(KEY)}, ${JSON.stringify(text)})`);

let passed = true;
function report(name, got, want) {
  const bad = Object.keys(want).filter((key) => got[key] !== want[key]);
  if (got.errors?.length) bad.push(`errors: ${got.errors[0]}`);
  if (bad.length > 0) {
    passed = false;
    console.error(`FAIL: ${name}: ${bad.join(", ")}: ${JSON.stringify(got)}`);
  } else {
    console.log(`ok: ${name}`);
  }
}

const server = await serve();
const url = `http://127.0.0.1:${server.address().port}/`;
const profile = mkdtempSync(join(tmpdir(), "salmong-best-"));
let chrome = launch(profile);
try {
  let tab = await open(await chrome.port);

  let got = await load(tab, url);
  report("clean profile writes the run", { ...got, stored: await stored(tab) }, {
    loaded: "none",
    run: RUN,
    best: RUN,
    stored: RUN,
    warned: false,
  });

  got = await load(tab, url);
  report("reload reads the record", { ...got, stored: await stored(tab) }, {
    loaded: RUN,
    best: RUN,
    stored: RUN,
  });

  const browser = await socketTo(
    (await (await fetch(`http://127.0.0.1:${await chrome.port}/json/version`)).json())
      .webSocketDebuggerUrl,
  );
  browser.send("Browser.close");
  await chrome.exited;
  chrome = launch(profile);
  tab = await open(await chrome.port);
  got = await load(tab, url);
  report("browser restart reads the record", { ...got, stored: await stored(tab) }, {
    loaded: RUN,
    best: RUN,
    stored: RUN,
  });

  await seed(tab, HIGHER);
  got = await load(tab, url);
  report("a higher record stays", { ...got, stored: await stored(tab) }, {
    loaded: HIGHER,
    run: RUN,
    best: HIGHER,
    stored: HIGHER,
  });

  await seed(tab, LOWER);
  got = await load(tab, url);
  report("a lower record is replaced", { ...got, stored: await stored(tab) }, {
    loaded: LOWER,
    best: RUN,
    stored: RUN,
  });

  await seed(tab, "garbage");
  got = await load(tab, url);
  report("a corrupt record is replaced", { ...got, stored: await stored(tab) }, {
    loaded: "none",
    best: RUN,
    stored: RUN,
  });

  await seed(tab, LOWER);
  const full = await tab.page.send("Page.addScriptToEvaluateOnNewDocument", { source: FULL });
  got = await load(tab, url);
  await tab.page.send("Page.removeScriptToEvaluateOnNewDocument", { identifier: full.identifier });
  report("a full store keeps the session best", { ...got, stored: await stored(tab) }, {
    loaded: LOWER,
    best: RUN,
    stored: LOWER,
    warned: true,
  });

  await tab.page.send("Page.addScriptToEvaluateOnNewDocument", { source: DENY });
  got = await load(tab, url);
  report("a denied store keeps the session best", got, {
    loaded: "none",
    best: RUN,
    warned: true,
  });
} finally {
  chrome.kill();
  await chrome.exited;
  server.close();
  rmSync(profile, { recursive: true, force: true });
}
process.exit(passed ? 0 : 1);
