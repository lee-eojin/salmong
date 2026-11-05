#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export BEND_NO_TELEMETRY=1

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ $# -eq 0 ]] || fail "usage: scripts/check-scenarios.sh"
scripts/check-bend.sh
cargo build -q --locked --release -p salmong-oracle
oracle=$PWD/target/release/salmong-oracle
work=target/scenarios
rm -rf "$work"
mkdir -p "$work"

for f in bend/scenarios.bend bend/vectors.bend; do
  bend "$f" --check-only >"$work/check.out" 2>&1 || fail "$f does not check: $(cat "$work/check.out")"
  grep -qx "ALL PROOFS CHECK" "$work/check.out" || fail "$f does not check"
done
echo "ok: scenarios.bend and vectors.bend check"

expected='ok: h32(0, 0)
ok: h32(1, 0)
ok: h32(1, 1)
ok: h32(3735928559, 0)
ok: h32(42, 7)
ok: scenario head
ok: ramp
ok: placement
ok: station count'
got=$(bend bend/vectors.bend) || fail "vectors exited non-zero"
[[ $got == "$expected" ]] || fail "vectors printed: $got"
echo "ok: M2.3 vectors pass on the JS lane"

for seed in 1 0 4294967295; do
  bend bend/scenarios.bend "$seed" 1 >"$work/bend.$seed"
  "$oracle" pack "$seed" 1 >"$work/rust.$seed"
  cmp -s "$work/bend.$seed" "$work/rust.$seed" || fail "seed $seed count 1 differs from Rust"
done
sum=$(sha256sum <"$work/bend.1")
[[ $sum == "c0bc37bcb1a10147bc3dcab5c4b7aeaf36de640138151e498108dca714383059  -" ]] || fail "1 1 sha256 is $sum"
echo "ok: bend 1 1, 0 1, 4294967295 1 equal Rust byte for byte"
bend bend/scenarios.bend 1 16 | cmp -s - assets/scenarios/pack.v1.json || fail "1 16 differs from pack.v1.json"
echo "ok: bend 1 16 equals assets/scenarios/pack.v1.json"

refuse() {
  local message=$1
  shift
  rc=0
  bend bend/scenarios.bend "$@" >"$work/out" 2>"$work/err" || rc=$?
  [[ $rc -eq 2 ]] || fail "args '$*' exited $rc"
  [[ ! -s $work/out ]] || fail "args '$*' printed to stdout"
  grep -qF "$message" "$work/err" || fail "args '$*' stderr: $(cat "$work/err")"
}
refuse usage
refuse usage 1
refuse usage 1 1 2
refuse "seed must be a u32" x 1
refuse "seed must be a u32" 4294967296 1
refuse "count must be a u32" 1 x
refuse "count must be at least 1" 1 0
refuse "count must be a power of two" 1 3
echo "ok: bad arguments exit 2 with empty stdout"

src=$(<bend/scenarios.bend)
mutant() {
  local name=$1 from=$2 to=$3 want_vec=$4 want_one=$5 want_many=$6
  local dir=$work/$name
  mkdir -p "$dir"
  printf '%s\n' "${src/"$from"/"$to"}" >"$dir/scenarios.bend"
  cp bend/vectors.bend "$dir/vectors.bend"
  ! cmp -s "$dir/scenarios.bend" bend/scenarios.bend || fail "$name did not apply"
  bend "$dir/scenarios.bend" --check-only 2>&1 | grep -qx "ALL PROOFS CHECK" || fail "$name does not check"
  local vec=pass one=same many=same
  bend "$dir/vectors.bend" >/dev/null 2>&1 || vec=FAIL
  rc=0
  bend "$dir/scenarios.bend" 1 1 >"$dir/one" 2>"$dir/one.err" || rc=$?
  if [[ $rc -ne 0 ]]; then one=rc$rc; elif ! cmp -s "$dir/one" "$work/rust.1"; then one=differs; fi
  rc=0
  bend "$dir/scenarios.bend" 1 16 >"$dir/many" 2>/dev/null || rc=$?
  if [[ $rc -ne 0 ]]; then many=rc$rc; elif ! cmp -s "$dir/many" assets/scenarios/pack.v1.json; then many=differs; fi
  [[ "$vec $one $many" == "$want_vec $want_one $want_many" ]] || fail "$name gave $vec $one $many, want $want_vec $want_one $want_many"
  echo "ok: $name $from -> $to: $vec $one $many"
}
mutant M1 "2654435761" "2654435763" FAIL differs differs
mutant M2 "a >> 16n" "a >> 15n" FAIL differs differs
mutant M3 "b >> 13n" "b >> 14n" FAIL differs differs
mutant M4 "pick(80, 100," "pick(81, 100," FAIL differs differs
mutant M5 "pick(2, 61," "pick(2, 60," FAIL differs differs
mutant M6 "pick(2, 37," "pick(2, 36," FAIL differs differs
mutant M7 "* 600" "* 601" FAIL differs differs
mutant M8 "pick(3, 5," "pick(3, 6," FAIL differs differs
mutant M9 "[9000," "[9001," FAIL differs differs
mutant M10 ", 16)" ", 17)" pass differs differs
mutant M11 "U32.is_lt(k, 3)" "U32.is_lt(k, 2)" pass same differs
mutant M12 "8400" "8401" pass same differs
mutant M13 '",\n"' '"\n"' pass same differs
mutant M14 "64n" "63n" FAIL differs differs
mutant M15 "256n" "1n" FAIL differs differs
mutant M16 "256n" "0n" FAIL rc1 rc1
grep -qF "scenario 0 placed only 0 stations" "$work/M16/one.err" || fail "M16 stderr: $(cat "$work/M16/one.err")"
echo "ok: 16 mutants caught as designed"
