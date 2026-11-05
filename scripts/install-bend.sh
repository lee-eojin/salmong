#!/usr/bin/env bash
set -euo pipefail

version=2.0.39
archive_sha=6cf462fd8f70b094a4950ebb709a9c4ffd8f1d1189d9cedf1fd63cd5d041c7ab
binary_sha=2ec160fa4486d33aff51e9eedcc294d6b531dc10c279c00b0c00685dae2045cd

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ $(uname -sm) == "Linux x86_64" ]] || fail "only Linux x86_64 is pinned, got $(uname -sm)"

home=$HOME/.bend
shim=$HOME/.local/bin/bend
name=bend-$version-linux-x64.tar.gz
url=https://github.com/bendlang/bend/releases/download/v$version/$name

have() {
  [[ -f $home/bin/bend ]] && sha256sum "$home/bin/bend" | cut -d' ' -f1 | grep -qx "$binary_sha"
}

if ! have; then
  mkdir -p "$home"
  tmp=$(mktemp -d "$home/tmp.XXXXXX")
  trap 'rm -rf "$tmp"' EXIT
  curl --proto '=https' --tlsv1.2 -fsSL -o "$tmp/$name" "$url"
  if ! echo "$archive_sha  $tmp/$name" | sha256sum -c --status; then
    fail "$name does not match its pinned sha256; not installed"
  fi
  tar --warning=no-unknown-keyword -xzf "$tmp/$name" -C "$tmp"
  if ! echo "$binary_sha  $tmp/bend/bin/bend" | sha256sum -c --status; then
    fail "bin/bend does not match its pinned sha256; not installed"
  fi
  mkdir -p "$home/bin"
  rm -rf "$home/bend2" "$home/guide"
  mv "$tmp/bend/bend2" "$home/bend2"
  mv "$tmp/bend/guide" "$home/guide"
  mv -f "$tmp/bend/bin/bend" "$home/bin/bend"
fi

mkdir -p "$(dirname "$shim")"
printf '%s\n' '#!/bin/sh' 'export BEND_NO_TELEMETRY=1' "exec \"$home/bin/bend\" \"\$@\"" > "$shim"
chmod 755 "$shim"
echo "ok: bend $version at $home, shim $shim"
