#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

fresh=$(mktemp)
trap 'rm -f "$fresh"' EXIT

scripts/make-notices.sh >"$fresh"

if ! cmp "$fresh" THIRD-PARTY-NOTICES.txt; then
  echo "FAIL: THIRD-PARTY-NOTICES.txt differs from scripts/make-notices.sh; run: scripts/make-notices.sh > THIRD-PARTY-NOTICES.txt" >&2
  exit 1
fi
echo "ok: THIRD-PARTY-NOTICES.txt matches scripts/make-notices.sh, $(grep -c '^Licence: ' THIRD-PARTY-NOTICES.txt) crates"
