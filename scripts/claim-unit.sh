#!/usr/bin/env bash
set -euo pipefail

remote=${CLAIM_REMOTE:-origin}

fail() {
  echo "claim-unit: $*" >&2
  exit 1
}

[[ $# -eq 1 ]] || fail "usage: scripts/claim-unit.sh <unit>"
unit=$1
[[ $unit =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]] || fail "unit '$unit' must be lowercase words joined by '-', like m2-6"
branch=unit/$unit

git fetch -q "$remote" main
if git ls-remote --exit-code --heads "$remote" "$branch" >/dev/null; then
  fail "$branch already exists on $remote, so another worker holds $unit"
fi

open=$(gh pr list --state open --head "$branch" --json number --jq '.[].number')
[[ -z $open ]] || fail "open PR #$open already uses $branch, so another worker holds $unit"

base=$(git rev-parse "$remote/main")
claim=$(git commit-tree "$base^{tree}" -p "$base" -m "chore: claim $branch" -m "by $(git config user.name || whoami) on $(hostname) at $(date -u +%FT%TZ), nonce $$-$RANDOM$RANDOM")

if ! git push -q "$remote" "$claim:refs/heads/$branch" 2>/dev/null; then
  fail "$remote rejected $branch, so another worker claimed $unit first"
fi

echo "claimed $branch at $claim"
echo "next: git worktree add -b $branch <path> $remote/$branch"
