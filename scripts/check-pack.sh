#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export BEND_NO_TELEMETRY=1

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ $# -eq 0 ]] || fail "usage: scripts/check-pack.sh"
scripts/check-bend.sh
cargo build -q --locked --release -p salmong-oracle
oracle=$PWD/target/release/salmong-oracle
work=target/pack-check
rm -rf "$work"
mkdir -p "$work"
pin=$(<assets/scenarios/pack.v1.sha256)
seeds=(1 2 3)
counts=(1 16 64)

pinned() {
  [[ $(sha256sum <"$1") == "$pin  -" ]]
}

for seed in "${seeds[@]}"; do
  for count in "${counts[@]}"; do
    cell=$work/bend.$seed.$count
    bend bend/scenarios.bend "$seed" "$count" >"$cell" || fail "bend $seed $count exited non-zero"
    "$oracle" pack "$seed" "$count" >"$work/rust.$seed.$count" || fail "pack $seed $count exited non-zero"
    cmp -s "$cell" "$work/rust.$seed.$count" || fail "seed $seed count $count differs from Rust"
    [[ $("$oracle" validate "$cell") == "ok: $count scenarios" ]] || fail "seed $seed count $count does not validate"
    echo "ok: seed $seed count $count cmp-equal and valid"
  done
done
pinned "$work/bend.1.16" || fail "bend 1 16 does not match pack.v1.sha256"
pinned "$work/rust.1.16" || fail "rust 1 16 does not match pack.v1.sha256"
echo "ok: seed 1 count 16 matches pack.v1.sha256"

good=$(<"$work/bend.1.16")
tamper() {
  local name=$1 from=$2 to=$3 word=$4
  printf '%s\n' "${good/"$from"/"$to"}" >"$work/$name.json"
  ! cmp -s "$work/$name.json" "$work/bend.1.16" || fail "$name did not apply"
  rc=0
  "$oracle" validate "$work/$name.json" >"$work/out" 2>"$work/err" || rc=$?
  [[ $rc -eq 1 ]] || fail "$name exited $rc"
  [[ ! -s $work/out ]] || fail "$name printed to stdout"
  grep -qF "$word" "$work/err" || fail "$name stderr: $(cat "$work/err")"
  echo "ok: $name refused with $word"
}
tamper T1 "[[35,32,0,0,4]" "[[70,32,0,0,4]" OutOfBounds
tamper T2 "[9000,89]" "[9000,39]" RampFalls
tamper T3 "[10,3,2,600,3]" "[10,3,3,600,3]" EarlyStar
tamper T4 '"pack_seed":1,' '"pack_seed": 1,' NonCanonical

rust=$work/rust
mutated=$rust/target/debug/salmong-oracle
mkdir -p "$rust"
cp -R Cargo.toml Cargo.lock rust-toolchain.toml crates "$rust/"

build() {
  (cd "$rust" && cargo build -q --locked -p salmong-oracle) || fail "$1 does not build"
}

compare() {
  local dir=$1 seed count cell
  differs=0
  refused=0
  mkdir -p "$dir"
  : >"$dir/err"
  for seed in "${seeds[@]}"; do
    for count in "${counts[@]}"; do
      cell=$dir/rust.$seed.$count
      "$mutated" pack "$seed" "$count" >"$cell" || fail "$dir: pack $seed $count exited non-zero"
      cmp -s "$cell" "$work/bend.$seed.$count" || differs=$((differs + 1))
      "$oracle" validate "$cell" >/dev/null 2>>"$dir/err" || refused=$((refused + 1))
    done
  done
}

build control
compare "$work/control"
[[ "$differs $refused" == "0 0" ]] || fail "control gave $differs $refused"
pinned "$work/control/rust.1.16" || fail "control does not match pack.v1.sha256"
echo "ok: unmutated scratch build equals Bend on all 9 cells"

caught=0
rust_mutant() {
  local name=$1 file=$2 from=$3 to=$4 want=$5 word=${6:-} src
  [[ $(grep -cF -- "$from" "$file") == 1 ]] || fail "$name: from must match exactly one line"
  src=$(<"$file")
  printf '%s\n' "${src/"$from"/"$to"}" >"$rust/$file"
  ! cmp -s "$rust/$file" "$file" || fail "$name did not apply"
  build "$name"
  cp "$file" "$rust/$file"
  compare "$work/$name"
  [[ "$differs $refused" == "$want" ]] || fail "$name gave $differs $refused, want $want"
  ! pinned "$work/$name/rust.1.16" || fail "$name still matches pack.v1.sha256"
  [[ $(grep -cF "$word" "$work/$name/err") == "$refused" ]] || fail "$name stderr: $(cat "$work/$name/err")"
  caught=$((caught + 1))
  echo "ok: $name $from -> $to: $differs differ, $refused refused"
}
G=crates/salmong-oracle/src/generate.rs
H=crates/salmong-oracle/src/h32.rs
rust_mutant C1 $G "u32::from(MAP_W) - 3" "u32::from(MAP_W) - 4" "9 0"
rust_mutant C2 $G "u32::from(MAP_H) - 3" "u32::from(MAP_H) - 4" "9 0"
rust_mutant C3 $G "pick(2, u32::from(MAP_H)" "pick(3, u32::from(MAP_H)" "9 0"
rust_mutant C4 $G "u32::from(MAP_W) - 3" "u32::from(MAP_W) + 3" "9 9" OutOfBounds
rust_mutant H1 $G "h32(pack_seed, id)" "h32(pack_seed, id + 1)" "9 0"
rust_mutant H2 $G "r(seed, 1, k, a)" "r(seed, 1, k, a + 1)" "9 0"
rust_mutant H3 $G "r(seed, 2, k, a)" "r(seed, 1, k, a)" "9 0"
rust_mutant H4 $G "pick(0, 2, r(seed, 3, k, 0))" "pick(0, 2, r(seed, 3, k, 1))" "9 0"
rust_mutant H5 $G "pick(0, 3, r(seed, 3, k, 0))" "pick(0, 3, r(seed, 3, k, 1))" "9 0"
rust_mutant H6 $G "r(seed, 4, k, 0)" "r(seed, 4, k, 1)" "9 0"
rust_mutant H7 $G "r(seed, 5, 0, 0)" "r(seed, 5, 0, 1)" "9 0"
rust_mutant H8 $H "p << 24" "p << 23" "9 0"
rust_mutant H9 $H "k << 8" "k << 7" "9 0"
rust_mutant R1 $G "rate: 40" "rate: 41" "9 0"
rust_mutant R2 $G "tick: 9000" "tick: 9001" "9 0"
rust_mutant R3 $G "pick(80, 100," "pick(81, 100," "9 0"
rust_mutant R4 $G "pick(80, 100," "pick(80, 101," "9 0"
rust_mutant R5 $G "rate: 40" "rate: 120" "9 9" RampFalls
rust_mutant V1 $G ">= 16" ">= 9" "9 9" TooClose
rust_mutant V2 $G "at_tick < 8400" "at_tick < 7800" "9 6" EarlyStar
rust_mutant V3 $G "shape_index = if k < 3" "shape_index = if k < 2" "7 7" StartShapes
rust_mutant S1 $G "(k - 2) * 600" "(k - 2) * 601" "9 0"
rust_mutant S2 $G "pick(3, 5," "pick(2, 5," "9 0"
[[ $caught -eq 23 ]] || fail "ran $caught mutants"
echo "ok: $caught Rust mutants fail as designed"
