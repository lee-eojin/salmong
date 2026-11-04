#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTUP_AUTO_INSTALL=0

cargo test --locked -p salmong-core --test no_comments
echo "ok: no comments in tracked source"
