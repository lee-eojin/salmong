#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTUP_AUTO_INSTALL=0

read -r -d '' rules <<'JSON' || true
{
  "salmong-core": {"external": false, "workspace": []},
  "salmong-pack": {"external": true, "workspace": ["salmong-core"]},
  "salmong-oracle": {"external": true, "workspace": ["salmong-core", "salmong-pack"]},
  "salmong-game": {"external": true, "workspace": ["salmong-core", "salmong-pack"]}
}
JSON

violations="$(cargo metadata --locked --no-deps --format-version 1 | jq -r --argjson rules "$rules" '
  [.packages[].name] as $members
  | .packages[]
  | .name as $crate
  | $rules[$crate] as $rule
  | if $rule == null then "error: \($crate) has no rule in scripts/check-deps.sh; add one with its allowed direction"
    else
      .dependencies[]
      | select(.name != $crate)
      | (.kind // "normal") as $kind
      | .name as $dep
      | if ($members | index($dep)) != null then
          select(($rule.workspace | index($dep)) == null)
          | "error: \($crate) must not depend on \($dep) (\($kind)); allowed workspace deps: \($rule.workspace | join(", ") | if . == "" then "none" else . end)"
        elif $rule.external | not then
          "error: \($crate) must have zero dependencies, found \($dep) (\($kind))"
        else empty end
    end')"

if [[ -n "$violations" ]]; then
    printf '%s\n' "$violations" >&2
    exit 1
fi
echo "ok: crate dependency direction holds"
