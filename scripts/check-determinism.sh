#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTUP_AUTO_INSTALL=0
export PATH="${HOME}/.cargo/bin:${PATH}"

cargo test --locked -p salmong-core --test replay empty3_hashes_match_hand_table -- --exact
cargo build --locked -p salmong-core --target wasm32-unknown-unknown --release
wasm=target/wasm32-unknown-unknown/release/salmong_core.wasm
test -f "$wasm"
node scripts/check-determinism.mjs "$wasm"
echo "ok: native hand table and wasm Node hashes match"
