#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

base=${1:-origin/main}
out=target/mutants-check
mkdir -p "$out"
diff=$out/pr.diff

scope=(--in-diff "$diff")
if [[ $base == --all ]]; then
  scope=(--workspace)
else
  git diff "$(git merge-base "$base" HEAD)" -- '*.rs' > "$diff"
  if [[ ! -s $diff ]]; then
    echo "skip: no .rs changes against $base"
    exit 0
  fi
fi

status=0
cargo mutants "${scope[@]}" --no-shuffle -o "$out" -j "${MUTANTS_JOBS:-2}" || status=$?
case $status in
  0 | 2 | 3) ;;
  *) echo "check-mutants: cargo mutants exited $status" >&2; exit "$status" ;;
esac

python3 - "$out/mutants.out" mutants-allow.toml <<'PY'
import pathlib, re, sys, tomllib
out, allow_path = map(pathlib.Path, sys.argv[1:])
entries = tomllib.loads(allow_path.read_text()).get("allow", [])
for entry in entries:
    if not entry.get("reason", "").strip():
        sys.exit(f"check-mutants: allowlist entry {entry.get('mutant')!r} has no reason")
allowed = {entry["mutant"] for entry in entries}
def key(m):
    return re.sub(r":\d+:\d+:", ":", m, count=1)
def read(name):
    path = out / name
    return [line for line in path.read_text().splitlines() if line] if path.exists() else []
for m in read("timeout.txt"):
    print(f"TIMEOUT counted as caught: {m}")
bad = [m for m in read("missed.txt") if key(m) not in allowed]
for m in bad:
    print(f"MISSED: {m}")
if bad:
    sys.exit(f"check-mutants: {len(bad)} missed mutant(s) not in mutants-allow.toml")
print(f"ok: {len(read('caught.txt'))} caught, {len(read('timeout.txt'))} timeout, {len(read('unviable.txt'))} unviable, {len(read('missed.txt'))} missed and allowlisted")
PY
