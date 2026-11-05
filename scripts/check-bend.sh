#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

version=2.0.39
binary_sha=2ec160fa4486d33aff51e9eedcc294d6b531dc10c279c00b0c00685dae2045cd
base_sha=3cd0b0944b11e15c4da5c9af2560e3c04f27d3d37027f1617899aeec17b499ce

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

shim=$HOME/.local/bin/bend
[[ $(command -v bend || true) == "$shim" ]] || fail "run scripts/install-bend.sh"

actual=$(bend version)
[[ $actual == "bend $version" ]] || fail "expected bend $version, got: $actual"
echo "$binary_sha  $HOME/.bend/bin/bend" | sha256sum -c --status || fail "bin/bend does not match its pin; run scripts/install-bend.sh"
echo "$base_sha  $HOME/.bend/bend2/base.bend" | sha256sum -c --status || fail "bend2/base.bend does not match its pin; run scripts/install-bend.sh"
echo "ok: bend $version, binary and base.bend match their pins"

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
export BEND_ORIGIN=http://127.0.0.1:9 BEND_HUB=http://127.0.0.1:9

env -u BEND_NO_TELEMETRY HOME="$scratch/off" "$shim" bend/hello.bend 1 16 > "$scratch/out"
printf 'hello from bend: 1 16\n' | cmp - "$scratch/out" || fail "hello printed: $(cat "$scratch/out")"
[[ ! -e $scratch/off/.bend/check.json ]] || fail "shim wrote check.json"
echo "ok: hello printed its line with no network"

env -u BEND_NO_TELEMETRY HOME="$scratch/on" "$HOME/.bend/bin/bend" bend/hello.bend 1 16 > /dev/null || true
[[ -e $scratch/on/.bend/check.json ]] || fail "control run wrote no check.json, so the telemetry check proves nothing"
echo "ok: shim wrote no check.json, control wrote it"
