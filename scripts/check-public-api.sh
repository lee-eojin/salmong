#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

export RUSTUP_AUTO_INSTALL=0

mode="${1:-check}"
crates=(salmong-core salmong-pack salmong-oracle salmong-game)

read -r -d '' walk <<'JQ' || true
.index as $ix
| def name_of($t): ($t.resolved_path.path // $t.primitive // ($t | tostring));
  def fn_sig($f): "(" + ([$f.sig.inputs[] | .[0]] | join(", ")) + ")";
  def impl_lines($impl_id; $path):
    ($ix[$impl_id | tostring].inner.impl) as $im
    | if $im.is_synthetic or $im.blanket_impl != null then empty
      elif $im.trait == null then
        ($im.items[] | $ix[tostring] | select(.visibility == "public")
          | if .inner.function then "method " + $path + "::" + .name + fn_sig(.inner.function)
            else "assoc " + $path + "::" + .name end)
      else ("impl " + name_of({resolved_path: $im.trait}) + " for " + $path) end;
  def members($id; $path):
    ($ix[$id | tostring]) as $it
    | if $it == null then empty
      elif $it.inner.struct then
        ("struct " + $path),
        ($it.inner.struct.kind | if type == "object" and .plain then .plain.fields[] elif type == "object" and .tuple then .tuple[] | select(. != null) else empty end
          | $ix[tostring] | select(.visibility == "public") | "field " + $path + "::" + .name),
        ($it.inner.struct.impls[] | impl_lines(.; $path))
      elif $it.inner.enum then
        ("enum " + $path),
        ($it.inner.enum.variants[] | $ix[tostring] | "variant " + $path + "::" + .name),
        ($it.inner.enum.impls[] | impl_lines(.; $path))
      elif $it.inner.union then ("union " + $path)
      elif $it.inner.trait then
        ("trait " + $path),
        ($it.inner.trait.items[] | $ix[tostring] | "trait_item " + $path + "::" + .name)
      elif $it.inner.function then ("fn " + $path + fn_sig($it.inner.function))
      elif $it.inner.constant then ("const " + $path)
      elif $it.inner.static then ("static " + $path)
      elif $it.inner.type_alias then ("type " + $path)
      elif $it.inner.macro then ("macro " + $path)
      elif $it.inner.module then
        ("mod " + $path),
        ($it.inner.module.items[] as $c | $ix[$c | tostring] as $ci
          | if $ci.inner.use then
              ($ci.inner.use) as $u
              | if $u.is_glob then ("use " + $path + "::" + $u.source + "::*")
                elif $u.id != null and $ix[$u.id | tostring] != null then members($u.id; $path + "::" + $u.name)
                else ("use " + $path + "::" + $u.name + " = " + $u.source) end
            elif $ci.visibility == "public" then members($c; $path + "::" + $ci.name)
            else empty end)
      else ("item " + $path) end;
  members(.root; $ix[.root | tostring].name)
JQ

status=0
for crate in "${crates[@]}"; do
    lib="${crate//-/_}"
    pinned="crates/$crate/public-api.txt"
    RUSTC_BOOTSTRAP=1 cargo rustdoc --locked -q -p "$crate" --lib -- -Z unstable-options --output-format json
    actual="$(jq -r "$walk" "target/doc/$lib.json" | LC_ALL=C sort -u)"
    if [[ "$mode" == "--bless" ]]; then
        printf '%s\n' "$actual" > "$pinned"
        echo "blessed: $pinned"
    elif ! diff -u "$pinned" <(printf '%s\n' "$actual"); then
        echo "error: public API of $crate (built without features) differs from $pinned" >&2
        echo "error: if intended, run scripts/check-public-api.sh --bless and commit $pinned in this PR" >&2
        status=1
    fi
done
[[ $status -eq 0 ]] && echo "ok: public API matches pinned files"
exit $status
