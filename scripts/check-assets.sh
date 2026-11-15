#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f ASSETS.md ]] || fail "ASSETS.md is missing"

files=$(git ls-files --cached --others --exclude-standard -- assets ':(glob)**/*.wesl' | sort -u)

rows=$(awk -F '|' '
  /^\|/ {
    path = $2
    gsub(/^[ \t`]+|[ \t`]+$/, "", path)
    author = $4
    licence = $5
    gsub(/^[ \t]+|[ \t]+$/, "", author)
    gsub(/^[ \t]+|[ \t]+$/, "", licence)
    if (path == "Path" || path ~ /^-+$/) next
    printf "%s\037%s\037%s\n", path, author, licence
  }
' ASSETS.md)

[[ -n $rows ]] || fail "ASSETS.md has no rows"

while IFS=$'\037' read -r path author licence; do
  [[ -n $author ]] || fail "ASSETS.md row $path has no author"
  [[ -n $licence ]] || fail "ASSETS.md row $path has no licence"
  grep -Fxq -- "$path" <<<"$files" || fail "ASSETS.md row $path names a file that is not tracked under assets/ or a .wesl file"
done <<<"$rows"

count=0
while IFS= read -r file; do
  [[ -n $file ]] || continue
  n=$(cut -d $'\037' -f1 <<<"$rows" | grep -Fxc -- "$file" || true)
  [[ $n -eq 1 ]] || fail "$file has $n rows in ASSETS.md, expected 1"
  count=$((count + 1))
done <<<"$files"

echo "ok: $count asset files, each with one ASSETS.md row with an author and a licence"
