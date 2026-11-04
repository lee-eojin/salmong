#!/usr/bin/env bash
# M0.1 check. rust-toolchain.toml says `stable`, which moves on `rustup update`,
# so this is what holds the compiler at the version docs/plan.md records.
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTUP_AUTO_INSTALL=0
expected=1.99.1

actual=$(rustc -V)
if [[ "$actual" != "rustc $expected "* ]]; then
  echo "FAIL: expected rustc $expected, got: $actual" >&2
  exit 1
fi
echo "ok: $actual"

cargo check --locked --target wasm32-unknown-unknown
echo "ok: cargo check for wasm32-unknown-unknown"
