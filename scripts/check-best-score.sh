#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTUP_AUTO_INSTALL=0
export PATH="${HOME}/.cargo/bin:${PATH}"

probe() {
  XDG_DATA_HOME=$1 NO_COLOR=1 cargo run --locked -q -p salmong-game --example best_probe 2>&1 |
    sed -n "s/.*best-probe $2 //p"
}

data=$(mktemp -d)
trap 'rm -rf "$data"' EXIT
run=$(probe "$data" run)
[[ -n $run ]] || { echo "FAIL: native probe printed no run" >&2; exit 1; }
stored=$(<"$data/salmong/best")
[[ $stored == "$run" ]] || { echo "FAIL: native store holds '$stored', want '$run'" >&2; exit 1; }
loaded=$(probe "$data" loaded)
[[ $loaded == "$run" ]] || { echo "FAIL: native reload read '$loaded', want '$run'" >&2; exit 1; }
echo "ok: native file store: $run"

cargo build --locked --release --target wasm32-unknown-unknown -p salmong-game --example best_probe
out=target/best-probe
wasm-bindgen --target web --no-typescript --out-dir "$out" \
  target/wasm32-unknown-unknown/release/examples/best_probe.wasm
node scripts/check-best-score.mjs "$out" "$run"
