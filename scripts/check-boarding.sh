#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

mode=${1:-}
[[ $# -le 1 && ( -z $mode || $mode == --simulate ) ]] || fail "usage: scripts/check-boarding.sh [--simulate]"
if [[ -n ${TLA2TOOLS:-} ]]; then
  command -v java >/dev/null || fail "java is required with TLA2TOOLS"
  runner=(java -cp "$TLA2TOOLS" tlc2.TLC)
elif command -v tlc >/dev/null; then
  runner=(tlc)
else
  fail "set TLA2TOOLS or provide tlc on PATH"
fi
rows=(
  'BoardAtCapacity INVARIANT CapacitySafe'
  'BoardBeforeAlight PROPERTY AlightBeforeBoard'
  'SplitTransfer PROPERTY TransferSound'
  'SplitTransfer INVARIANT Conserved'
  'RestartEditedOnly PROPERTY AlightBeforeBoard'
  'DeliverAnywhere PROPERTY NoWrongDelivery'
)
rm -rf target/boarding
mkdir -p target/boarding
cp spec/Boarding.tla spec/Boarding.safety.cfg spec/Boarding.fault.cfg target/boarding/
cd target/boarding
model_faults=$(sed -nE 's/.*Fault = "([^"]+)".*/\1/p' Boarding.tla | sort -u)
table_faults=$(printf '%s\n' "${rows[@]}" | awk '{print $1}' | sort -u)
[[ $model_faults == "$table_faults" ]] || fail "model and table fault names differ"

check_version() {
  [[ $(head -n 1 "$1") == 'TLC2 Version 2.19 of 08 August 2024 (rev: 5a47802)' ]] || {
    tail -n 40 "$1" >&2
    fail "$2: expected TLC 2.19 build, expected rc $3, actual rc $4"
  }
}

flags=(-workers auto)
[[ $mode != --simulate ]] || flags=(-simulate num=50000 -depth 200 -seed 1)
log=Boarding.safety.log
rc=0
"${runner[@]}" "${flags[@]}" -metadir safety.states -config Boarding.safety.cfg Boarding.tla >"$log" 2>&1 || rc=$?
check_version "$log" safety 0 "$rc"
pass=false
if [[ $rc == 0 ]]; then
  if [[ $mode == --simulate ]]; then
    grep -Eq '^Progress: .* states checked\.$' "$log" && pass=true
  elif grep -Fxq 'Model checking completed. No error has been found.' "$log" &&
       grep -Eq '[0-9,]+ states generated, [0-9,]+ distinct states found, 0 states left on queue\.' "$log"; then
    pass=true
  fi
fi
if [[ $pass != true ]]; then
  tail -n 40 "$log" >&2
  fail "safety: expected rc 0 and completion evidence, actual rc $rc"
fi
if [[ $mode == --simulate ]]; then
  grep -E '^Progress: .* states checked\.$' "$log" | sed 's/^/ok: /'
else
  grep -E '[0-9,]+ states generated, [0-9,]+ distinct states found, 0 states left on queue\.' "$log" | sed 's/^/ok: /'
fi
grep '^Finished in' "$log" | sed 's/^/ok: /'

for row in "${rows[@]}"; do
  read -r fault keyword property <<<"$row"
  cfg=$fault.$property.cfg
  log=$fault.$property.log
  sed -e "s/^  Fault = .*/  Fault = \"$fault\"/" -e "s/^INVARIANT CapacitySafe$/$keyword $property/" Boarding.fault.cfg >"$cfg"
  expected=12
  error="Error: Invariant $property is violated."
  if [[ $keyword == PROPERTY ]]; then
    expected=13
    error="Error: Action property $property is violated."
  fi
  flags=(-workers auto)
  [[ $mode != --simulate ]] || flags=(-simulate num=1000000 -depth 24 -seed 1)
  rc=0
  "${runner[@]}" "${flags[@]}" -metadir "$fault.$property.states" -config "$cfg" Boarding.tla >"$log" 2>&1 || rc=$?
  check_version "$log" "$row" "$expected" "$rc"
  if [[ $rc != "$expected" ]] || ! grep -Fxq "$error" "$log"; then
    tail -n 40 "$log" >&2
    fail "$row: expected rc $expected, actual rc $rc; expected $error"
  fi
  echo "ok: $fault breaks $property"
done
[[ $mode != --simulate ]] || echo 'ok: simulated only, not exhaustive'
