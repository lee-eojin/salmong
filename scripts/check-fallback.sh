#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTUP_AUTO_INSTALL=0

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ $# -eq 0 ]] || fail "usage: scripts/check-fallback.sh"

was=$(command -v bend || true)
clean=
IFS=: read -ra dirs <<<"$PATH"
for dir in "${dirs[@]}"; do
  if [[ -n $dir && ! -e $dir/bend ]]; then
    clean+=${clean:+:}$dir
  fi
done
PATH=$clean
rc=0
bend version >/dev/null 2>&1 || rc=$?
[[ $rc -eq 127 ]] || fail "bend still runs, rc $rc"
echo "ok: bend absent from PATH, was ${was:-absent}"

work=$PWD/target/fallback-check
rm -rf "$work"
mkdir -p "$work/empty"
asset=assets/scenarios/pack.v1.json
pin=$(<assets/scenarios/pack.v1.sha256)
regen=$work/pack.v1.json

cargo build -q --locked --release --target-dir "$work/target" -p salmong-oracle || fail "salmong-oracle does not build with Bend absent"

oracle() {
  env -i -C "$work/empty" HOME="$work/empty" PATH=/nonexistent "$work/target/release/salmong-oracle" "$@"
}

same() {
  cmp -s "$1" "$asset"
}

pinned() {
  [[ $(sha256sum <"$1") == "$pin  -" ]]
}

oracle pack 1 16 >"$regen" || fail "salmong-oracle pack 1 16 exited non-zero"
same "$regen" || fail "regenerated pack differs from $asset"
pinned "$regen" || fail "regenerated pack does not match pack.v1.sha256"
[[ $(oracle validate "$regen") == "ok: 16 scenarios" ]] || fail "regenerated pack does not validate"
echo "ok: salmong-oracle pack 1 16 regenerated the committed bytes with Bend absent"

good=$(<"$regen")
tamper() {
  local name=$1 from=$2 to=$3 word=$4 file=$work/$1.json rc=0
  printf '%s\n' "${good/"$from"/"$to"}" >"$file"
  ! cmp -s "$file" "$regen" || fail "$name did not apply"
  oracle validate "$file" >"$work/out" 2>"$work/err" || rc=$?
  [[ $rc -eq 1 ]] || fail "$name exited $rc"
  [[ ! -s $work/out ]] || fail "$name printed to stdout"
  grep -qF "$word" "$work/err" || fail "$name stderr: $(cat "$work/err")"
  echo "ok: $name refused with $word"
}
tamper V1 "[[35,32,0,0,4]" "[[70,32,0,0,4]" OutOfBounds
tamper V2 '"pack_seed":1,' '"pack_seed": 1,' NonCanonical

other=$work/seed2.json
oracle pack 2 16 >"$other" || fail "salmong-oracle pack 2 16 exited non-zero"
[[ $(oracle validate "$other") == "ok: 16 scenarios" ]] || fail "seed 2 pack does not validate"
! same "$other" || fail "seed 2 pack equals $asset"
! pinned "$other" || fail "seed 2 pack matches pack.v1.sha256"
echo "ok: a valid seed 2 pack fails cmp and sha256"
