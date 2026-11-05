#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ $# -eq 0 ]] || fail "usage: scripts/check-trace.sh"
if [[ -n ${TLA2TOOLS:-} ]]; then
  command -v java >/dev/null || fail "java is required with TLA2TOOLS"
  runner=(java -cp "$TLA2TOOLS" tlc2.TLC)
elif command -v tlc >/dev/null; then
  runner=(tlc)
else
  fail "set TLA2TOOLS or provide tlc on PATH"
fi
seeds=(${TRACE_SEEDS:-1 2 3 4})
cargo build -q --locked --release -p salmong-oracle
oracle=$PWD/target/release/salmong-oracle
work=target/trace
rm -rf "$work"
mkdir -p "$work"

run() {
  local name=$1 seed=$2 kind=${3:-}
  local dir=$work/$name
  rc=0
  mkdir -p "$dir"
  cp spec/Boarding.tla spec/BoardingTrace.tla spec/BoardingTrace.cfg "$dir/"
  "$oracle" trace "$seed" $kind >"$dir/BoardingTraceData.tla"
  steps=$(grep -c '^  \[tick |-> ' "$dir/BoardingTraceData.tla" || true)
  (cd "$dir" && "${runner[@]}" -workers 1 -metadir states -config BoardingTrace.cfg BoardingTrace.tla >tlc.log 2>&1) || rc=$?
  [[ $(head -n 1 "$dir/tlc.log") == 'TLC2 Version 2.19 of 08 August 2024 (rev: 5a47802)' ]] || {
    tail -n 40 "$dir/tlc.log" >&2
    fail "$name: expected TLC 2.19 build"
  }
}

for seed in "${seeds[@]}"; do
  run "seed$seed" "$seed"
  log=$work/seed$seed/tlc.log
  want="$((steps + 1)) distinct states found, 0 states left on queue."
  if [[ $rc != 0 ]] || ! grep -Fxq 'Model checking completed. No error has been found.' "$log" ||
     ! grep -Eq "^[0-9]+ states generated, $want\$" "$log"; then
    grep -v '^[0-9]*\. Line' "$log" | tail -n 60 >&2
    fail "seed $seed: expected rc 0 and $want, actual rc $rc"
  fi
  edits=$(grep -c 'act |-> "EditNetwork"' "$work/seed$seed/BoardingTraceData.tla" || true)
  echo "ok: seed $seed accepted, $steps steps, $edits EditNetwork"
  for kind in drop swap alter; do
    run "seed$seed-$kind" "$seed" "$kind"
    log=$work/seed$seed-$kind/tlc.log
    if [[ $rc != 11 ]] || ! grep -Fxq 'Error: Deadlock reached.' "$log"; then
      grep -v '^[0-9]*\. Line' "$log" | tail -n 60 >&2
      fail "seed $seed $kind: expected rc 11 and Error: Deadlock reached., actual rc $rc"
    fi
    depth=$(grep -Eo '^[0-9]+ states generated, [0-9]+ distinct' "$log" | awk '{print $4}')
    echo "ok: seed $seed $kind rejected after $depth of $((steps + 1)) states"
  done
done
