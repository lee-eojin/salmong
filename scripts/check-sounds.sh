#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

names=(board deliver station overflow reject over)
limit=100000

fresh=$(mktemp -d)
trap 'rm -rf "$fresh"' EXIT

node scripts/make-sounds.mjs "$fresh"

want=$(printf '%s.wav\n' "${names[@]}" | LC_ALL=C sort)
have=$(find assets/sounds -mindepth 1 -printf '%P\n' | LC_ALL=C sort)
if [[ $have != "$want" ]]; then
  echo "FAIL: assets/sounds must hold exactly: ${names[*]} (.wav), got:" >&2
  echo "$have" >&2
  exit 1
fi

total=0
for name in "${names[@]}"; do
  if ! cmp "$fresh/$name.wav" "assets/sounds/$name.wav"; then
    echo "FAIL: assets/sounds/$name.wav differs from scripts/make-sounds.mjs; run: node scripts/make-sounds.mjs assets/sounds" >&2
    exit 1
  fi
  total=$((total + $(wc -c <"assets/sounds/$name.wav")))
done

if ((total > limit)); then
  echo "FAIL: assets/sounds is $total bytes, over $limit" >&2
  exit 1
fi
echo "ok: six sounds match scripts/make-sounds.mjs, $total bytes"
