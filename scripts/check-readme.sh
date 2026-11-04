#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTUP_AUTO_INSTALL=0
server='trunk serve --release'
url=http://localhost:8080/

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

anchors() {
  sed -nE 's/^#+ //p' "$1" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9 _-]//g; s/ /-/g'
}

grep -Fq '](docs/plan.md)' README.md || fail "README.md does not link docs/plan.md"

while read -r link; do
  if [[ $link == http* ]]; then
    curl -fsL --max-time 30 -o /dev/null "$link" || fail "dead link: $link"
    continue
  fi
  path=${link%%#*}
  path=${path:-README.md}
  [[ -e $path ]] || fail "missing file: $link"
  if [[ $link == *#* ]]; then
    anchors "$path" | grep -Fx "${link#*#}" >/dev/null || fail "missing anchor: $link"
  fi
done < <(grep -oE '\]\([^)]+\)' README.md | sed -E 's/^\]\(//; s/\)$//')
echo "ok: links resolve"

commands=()
while IFS= read -r line; do
  commands+=("$line")
done < <(awk '/^```sh$/ { on = 1; next } /^```$/ { on = 0 } on' README.md)
printf '%s\n' "${commands[@]}" | grep -Fx "$server" >/dev/null || fail "README.md does not list $server"

curl -s -o /dev/null "$url" && fail "port 8080 is already in use"

for command in "${commands[@]}"; do
  [[ $command == "$server" ]] && continue
  bash -c "$command" || fail "exit $?: $command"
  echo "ok: $command"
done

$server &
pid=$!
trap 'kill "$pid" 2>/dev/null || true' EXIT
for _ in {1..300}; do
  curl -fs -o /dev/null "$url" && break
  kill -0 "$pid" 2>/dev/null || fail "$server exited"
  sleep 1
done
wasm=$(curl -fs "$url" | grep -oE '[^"/]+_bg\.wasm' | head -1) || fail "$server did not serve the page"
curl -fs -o /dev/null "$url$wasm" || fail "$server did not serve $wasm"
echo "ok: $server serves the page and $wasm"
