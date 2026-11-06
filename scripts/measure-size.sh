#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTUP_AUTO_INSTALL=0

if [[ $# -ne 0 ]]; then
  echo "usage: scripts/measure-size.sh" >&2
  exit 2
fi

trunk build --release --offline >&2
node scripts/measure-size.mjs dist
