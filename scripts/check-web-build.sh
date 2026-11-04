#!/usr/bin/env bash
# M0.2 check. --offline makes a missing tool fail here instead of being
# downloaded. The webgl check guards plan Q2: Bevy's `2d` feature pulls in
# `webgl2`, which adds the GL backend even though `webgpu` wins at runtime.
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTUP_AUTO_INSTALL=0

trunk build --release --offline

wasm=(dist/*_bg.wasm)
if [[ ${#wasm[@]} -ne 1 || ! -f ${wasm[0]} ]]; then
  echo "FAIL: expected one wasm file in dist, got: ${wasm[*]}" >&2
  exit 1
fi
node -e 'new WebAssembly.Module(require("fs").readFileSync(process.argv[1]))' "${wasm[0]}"
echo "ok: wasm compiles, $(wc -c <"${wasm[0]}") bytes, $(gzip -9 -c "${wasm[0]}" | wc -c) gzipped"

tree=$(cargo tree --locked -p salmong-game -e features --target wasm32-unknown-unknown)
if ! grep -q 'wgpu feature "webgpu"' <<<"$tree"; then
  echo "FAIL: wgpu feature webgpu is not enabled" >&2
  exit 1
fi
if grep -E 'feature "webgl2?"' <<<"$tree"; then
  echo "FAIL: webgl is in the wasm feature graph" >&2
  exit 1
fi
echo "ok: webgpu enabled, webgl absent from the wasm feature graph"

CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
  cargo test --locked -p salmong-game --target wasm32-unknown-unknown --test web_backend
echo "ok: web build selects the WebGPU backend only"
