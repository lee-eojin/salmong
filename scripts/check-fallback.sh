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

cargo build -q --locked --release -p salmong-oracle
oracle=$PWD/target/release/salmong-oracle
work=target/fallback-check
rm -rf "$work"
mkdir -p "$work/home"
pin=$(<assets/scenarios/pack.v1.sha256)
regen=$work/pack.v1.json

env -i HOME="$PWD/$work/home" PATH=/nonexistent "$oracle" pack 1 16 >"$regen" || fail "salmong-oracle pack 1 16 exited non-zero"
cmp -s "$regen" assets/scenarios/pack.v1.json || fail "regenerated pack differs from assets/scenarios/pack.v1.json"
[[ $(sha256sum <"$regen") == "$pin  -" ]] || fail "regenerated pack does not match pack.v1.sha256"
[[ $("$oracle" validate "$regen") == "ok: 16 scenarios" ]] || fail "regenerated pack does not validate"
echo "ok: salmong-oracle pack 1 16 regenerated the committed bytes with Bend absent"

game=$work/game
asset=$game/assets/scenarios/pack.v1.json
mkdir -p "${asset%/*}"
cp -R Cargo.toml Cargo.lock rust-toolchain.toml crates "$game/"

load() {
  CARGO_PROFILE_DEV_DEBUG=0 cargo test -q --locked --manifest-path "$game/Cargo.toml" \
    --target-dir target/fallback-target -p salmong-game --test pack >"$work/out" 2>&1
}

cp "$regen" "$asset"
load || fail "salmong-game did not load the regenerated pack: $(tail -n 20 "$work/out")"
echo "ok: salmong-game loads the regenerated pack through its shipped plugins"

caught=0
refused() {
  local name=$1 word=$2 rc=0
  load || rc=$?
  [[ $rc -ne 0 ]] || fail "$name: salmong-game accepted it"
  grep -qF -- "$word" "$work/out" || fail "$name: $(tail -n 20 "$work/out")"
  caught=$((caught + 1))
  echo "ok: $name refused with $word"
}

good=$(<"$regen")
tamper() {
  local name=$1 from=$2 to=$3 word=$4
  printf '%s\n' "${good/"$from"/"$to"}" >"$asset"
  ! cmp -s "$asset" "$regen" || fail "$name did not apply"
  refused "$name" "$word"
}

rewire() {
  local name=$1 file=$2 from=$3 to=$4 word=$5 src
  [[ $(grep -cF -- "$from" "$file") == 1 ]] || fail "$name: from must match exactly one line"
  src=$(<"$file")
  printf '%s\n' "${src/"$from"/"$to"}" >"$game/$file"
  ! cmp -s "$game/$file" "$file" || fail "$name did not apply"
  refused "$name" "$word"
  cp "$file" "$game/$file"
}

rm "$asset"
refused G1 "couldn't read"
tamper G2 "[[35,32,0,0,4]" "[[70,32,0,0,4]" OutOfBounds
tamper G3 '"pack_seed":1,' '"pack_seed": 1,' NonCanonical
"$oracle" pack 2 16 >"$asset"
refused G4 "left: (1, 2, 16)"
cp "$regen" "$asset"
rewire W1 crates/salmong-game/src/lib.rs ".add(pack::PackPlugin)" "" "does not exist in the"
printf '%s\n' "${good/"[[35,32,0,0,4]"/"[[35,32,0,0,5]"}" >"$game/stale.json"
rewire W2 crates/salmong-game/src/pack.rs '"../../../assets/scenarios/pack.v1.json"' '"../../../stale.json"' "not the asset on disk"
[[ $caught -eq 6 ]] || fail "ran $caught refusals"
echo "ok: $caught bad packs and wirings refused as designed"
