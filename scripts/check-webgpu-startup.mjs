#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";

const DIST = new URL("../dist/", import.meta.url);
const CHROME = process.env.CHROME ?? "google-chrome";
const TIMEOUT_MS = 30_000;
const MIME = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".wasm": "application/wasm",
};

const ADAPTER_LINE = "salmong: WebGPU adapter ";
const NO_ADAPTER_LINE = "salmong: no WebGPU adapter";
const PACK_LINE = "salmong: scenario pack loaded, 16 scenarios";
const SOFTWARE_ADAPTER = [
  "--enable-unsafe-webgpu",
  "--enable-unsafe-swiftshader",
  "--use-webgpu-adapter=swiftshader",
];

const CASES = [
  {
    name: "software adapter",
    flags: SOFTWARE_ADAPTER,
    done: (lines) =>
      lines.some((l) => l.startsWith(ADAPTER_LINE)) &&
      lines.some((l) => l.includes("backend: BrowserWebGpu")) &&
      lines.some((l) => l.includes(PACK_LINE)),
    errorScreenVisible: false,
    canvasFillsWindow: true,
  },
  {
    name: "WebGPU present, no adapter",
    flags: ["--disable-gpu"],
    done: (lines) => lines.includes(NO_ADAPTER_LINE),
    errorScreenVisible: true,
  },
  {
    name: "no WebGPU in the browser",
    host: "salmong.test",
    flags: ["--disable-gpu", "--host-resolver-rules=MAP salmong.test 127.0.0.1"],
    done: (lines) => lines.includes(NO_ADAPTER_LINE),
    errorScreenVisible: true,
  },
];

const PAGE_STATE = `(() => {
  const canvas = document.querySelector("canvas")?.getBoundingClientRect();
  return {
    errorScreenVisible: !document.getElementById("no-webgpu").hidden,
    canvasFillsWindow: canvas?.width === innerWidth && canvas?.height === innerHeight,
  };
})()`;

function serveDist() {
  const server = createServer((req, res) => {
    const { pathname } = new URL(req.url, "http://localhost");
    const file = pathname === "/" ? "index.html" : pathname.slice(1);
    try {
      const body = readFileSync(new URL(file, DIST));
      res.writeHead(200, { "content-type": MIME[extname(file)] });
      res.end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((resolve) =>
    server.listen(0, "127.0.0.1", () => resolve(server)),
  );
}

function launchChrome(flags) {
  const profile = mkdtempSync(join(tmpdir(), "salmong-chrome-"));
  const chrome = spawn(
    CHROME,
    [
      "--headless=new",
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--disable-extensions",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      ...flags,
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  const exited = new Promise((resolve) => chrome.once("close", resolve));
  const close = async () => {
    chrome.kill("SIGKILL");
    await exited;
    rmSync(profile, { recursive: true, force: true });
  };
  const debugPort = new Promise((resolve, reject) => {
    let log = "";
    chrome.once("error", reject);
    chrome.stderr.on("data", (chunk) => {
      log += chunk;
      const match = log.match(/DevTools listening on ws:\/\/[^:]+:(\d+)\//);
      if (match) resolve(match[1]);
    });
  });
  return { debugPort, close };
}

async function pageTarget(port) {
  const deadline = Date.now() + TIMEOUT_MS;
  while (Date.now() < deadline) {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
    const page = targets.find((target) => target.type === "page");
    if (page) return page;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error("Chrome opened no page target");
}

async function connect(port) {
  const page = await pageTarget(port);
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });

  let nextId = 0;
  const pending = new Map();
  const lines = [];
  socket.addEventListener("message", ({ data }) => {
    const message = JSON.parse(data);
    if (message.id !== undefined) {
      pending.get(message.id)(message.result);
      pending.delete(message.id);
    } else if (message.method === "Runtime.consoleAPICalled") {
      lines.push(message.params.args.map((arg) => arg.value).join(" "));
    }
  });
  const send = (method, params = {}) =>
    new Promise((resolve) => {
      pending.set(++nextId, resolve);
      socket.send(JSON.stringify({ id: nextId, method, params }));
    });
  return { send, lines, close: () => socket.close() };
}

async function waitFor(condition) {
  const deadline = Date.now() + TIMEOUT_MS;
  while (!condition()) {
    if (Date.now() > deadline) return false;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return true;
}

async function runCase(testCase, port) {
  const { name, host = "127.0.0.1", flags, done, errorScreenVisible, canvasFillsWindow } = testCase;
  const chrome = launchChrome(flags);
  try {
    const page = await connect(await chrome.debugPort);
    await page.send("Runtime.enable");
    await page.send("Page.navigate", { url: `http://${host}:${port}/` });

    const finished = await waitFor(() => done(page.lines));
    const { result } = await page.send("Runtime.evaluate", {
      expression: PAGE_STATE,
      returnByValue: true,
    });
    const state = result.value;
    page.close();

    const adapter = page.lines.find((l) => l.startsWith(ADAPTER_LINE));
    const failures = [];
    if (!finished) failures.push("expected console lines did not appear");
    if (state.errorScreenVisible !== errorScreenVisible) {
      failures.push(`error screen visible: ${state.errorScreenVisible}, want ${errorScreenVisible}`);
    }
    if (canvasFillsWindow && !state.canvasFillsWindow) {
      failures.push("canvas does not fill the window");
    }
    if (failures.length > 0) {
      console.error(`FAIL: ${name}: ${failures.join("; ")}`);
      for (const line of page.lines.slice(-5)) {
        console.error(`  console: ${line.slice(0, 160)}`);
      }
      return false;
    }
    console.log(`ok: ${name}: ${adapter ?? NO_ADAPTER_LINE}`);
    return true;
  } finally {
    await chrome.close();
  }
}

const build = spawnSync("trunk", ["build", "--release", "--offline"], {
  cwd: new URL("..", import.meta.url),
  env: { ...process.env, RUSTUP_AUTO_INSTALL: "0" },
  stdio: ["ignore", "ignore", "inherit"],
});
if (build.status !== 0) {
  console.error("FAIL: trunk build");
  process.exit(1);
}

const server = await serveDist();
const { port } = server.address();
let passed = true;
for (const testCase of CASES) {
  passed = (await runCase(testCase, port)) && passed;
}
server.close();
process.exit(passed ? 0 : 1);
