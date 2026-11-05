#!/usr/bin/env bash
set -euo pipefail

if [[ $# -eq 1 ]]; then
  prs=$(cat "$1")
else
  prs=$(gh pr list --state open --limit 1000 --json number,headRefName)
fi

report=$(jq -r '
  (.[] | select(.headRefName | test("^unit/[a-z0-9]+(-[a-z0-9]+)*$") | not)
    | "PR #\(.number) head \(.headRefName) is not unit/<unit>; claim one with scripts/claim-unit.sh"),
  (group_by(.headRefName)[] | select(length > 1)
    | "unit \(.[0].headRefName) has \(length) open PRs: \(map("#\(.number)") | join(", "))")
' <<<"$prs")

if [[ -n $report ]]; then
  echo "$report" >&2
  exit 1
fi
echo "ok: $(jq length <<<"$prs") open PRs, each on its own unit/<unit> branch"
