#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTUP_AUTO_INSTALL=0
export LC_ALL=C

target=wasm32-unknown-unknown
apache_sha=a6cba85bc92e0cff7a450b1d873c0eaa2e9fc96bf472df0247a26bec77bf3ff9

fail() {
  echo "make-notices: $*" >&2
  exit 1
}

shipped=$(cargo tree --locked -p salmong-game --target "$target" -e normal,no-proc-macro --prefix none --format '{p}' \
  | grep -v ' (' | sed 's/ v/ /' | sort -u)

crates=$(cargo metadata --locked --format-version 1 --filter-platform "$target" | jq -r --arg shipped "$shipped" '
  ($shipped | split("\n")) as $set
  | [.packages[] | select(.source != null) | select((.name + " " + .version) as $k | $set | index($k))]
  | sort_by(.name, .version)[]
  | [.name, .version, (.license // ""), (.manifest_path | sub("/Cargo.toml$"; "")), (.authors | join(", "))]
  | @tsv')

want=$(grep -c . <<<"$shipped")
have=$(grep -c . <<<"$crates")
[[ $have -eq $want ]] || fail "cargo tree lists $want crates but cargo metadata matched $have"

licence_files() {
  find "$1" -maxdepth 1 -type f \( -name 'LICEN[CS]E*' -o -name 'COPYING*' -o -name 'NOTICE*' -o -name 'UNLICENSE*' \) -printf '%f\n' | sort
}

declare -A first
while IFS=$'\t' read -r name version licence dir authors; do
  while read -r file; do
    [[ -n $file ]] || continue
    sha=$(sha256sum "$dir/$file" | cut -d' ' -f1)
    [[ -v first[$sha] ]] || first[$sha]="$name $version $file"
  done < <(licence_files "$dir")
done <<<"$crates"

[[ -v first[$apache_sha] ]] || fail "no crate carries the Apache-2.0 text with sha256 $apache_sha"

mit_text() {
  cat <<MIT
MIT License

Copyright (c) $1

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
MIT
}

echo "salmong: third-party notices"
echo "salmong is licensed under MIT OR Apache-2.0. Source: https://github.com/lee-eojin/salmong"
echo "The salmong WebAssembly build contains the $want third-party crates below. Each entry gives the crate's"
echo "name, version and licence expression, then its licence and notice files. A text that appears more than"
echo "once is printed in full the first time and named by its sha256 after that."

declare -A printed
while IFS=$'\t' read -r name version licence dir authors; do
  echo
  echo "================================================================================"
  echo "$name $version"
  echo "Licence: $licence"
  files=$(licence_files "$dir")
  if [[ -z $files ]]; then
    if grep -qw 'Apache-2.0' <<<"$licence"; then
      echo "--- Apache-2.0: same text as ${first[$apache_sha]} (sha256 $apache_sha)"
    elif [[ $licence == MIT && -n $authors ]]; then
      echo "--- MIT, from the crate's authors"
      mit_text "$authors"
    else
      fail "$name $version has no licence file and licence '$licence'"
    fi
    continue
  fi
  while read -r file; do
    sha=$(sha256sum "$dir/$file" | cut -d' ' -f1)
    if [[ -v printed[$sha] ]]; then
      echo "--- $file: same text as ${first[$sha]} (sha256 $sha)"
    else
      printed[$sha]=1
      echo "--- $file (sha256 $sha)"
      cat "$dir/$file"
      [[ -z $(tail -c 1 "$dir/$file") ]] || echo
    fi
  done <<<"$files"
done <<<"$crates"
