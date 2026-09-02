#!/usr/bin/env bash
# The release checklist, executed.
#
# Reads `ops/release/release.toml` and runs every blocking gate in order, stopping at the first
# failure. The sign-offs are printed rather than checked, because a person saying "I read the
# privacy page" is not a thing a script can verify - and a script that pretended to would be worse
# than one that asks.
#
#   bash ops/release/check.sh              # every blocking gate
#   bash ops/release/check.sh --list       # what it would run, and who owns each
#
# Exit 0 means every gate passed and the sign-offs are outstanding. It does **not** mean the
# release is ready; `ops/release/release.toml`'s `[[signoff]]` rows are the rest of it.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
manifest="$root/ops/release/release.toml"

if [ ! -f "$manifest" ]; then
  echo "no release manifest at $manifest" >&2
  exit 1
fi

# A tiny TOML reader for the two shapes this file uses. Not a parser: the manifest is ours, its
# shape is fixed, and a dependency on a TOML tool would make the release checklist need a release
# checklist.
gates() {
  awk '
    # A state machine keyed on the section header. The first version of this printed on `why =`
    # without tracking which section it was in, so every `[[signoff]]` row inherited the last
    # gate command and both lists came out thirteen long. A parser that does not know where it is
    # is a parser that reports whatever it last saw.
    /^\[\[gate\]\]/    { section="gate"; id=""; cmd=""; owner=""; blocking=""; next }
    /^\[\[signoff\]\]/ { section="signoff"; next }
    /^\[/               { section=""; next }
    section != "gate"    { next }
    /^id *=/             { line=$0; gsub(/^id *= *"|"$/, "", line); id=line; next }
    /^command *=/        { line=$0; sub(/^command *= *"/, "", line); sub(/"$/, "", line); cmd=line; next }
    /^owner *=/          { line=$0; gsub(/^owner *= *"|"$/, "", line); owner=line; next }
    /^blocking *=/       { line=$0; gsub(/^blocking *= *| /, "", line); blocking=line; next }
    /^why *=/            { if (id != "" && cmd != "") { print id "\t" owner "\t" blocking "\t" cmd; id=""; cmd="" } }
  ' "$manifest"
}

signoffs() {
  awk '
    /^\[\[signoff\]\]/ { section="signoff"; id=""; owner=""; next }
    /^\[/               { section=""; next }
    section != "signoff" { next }
    /^id *=/             { line=$0; gsub(/^id *= *"|"$/, "", line); id=line; next }
    /^owner *=/          { line=$0; gsub(/^owner *= *"|"$/, "", line); owner=line; if (id != "") { print id "\t" owner; id="" } }
  ' "$manifest"
}

if [ "${1:-}" = "--list" ]; then
  echo "Blocking gates:"
  gates | while IFS=$'\t' read -r id owner blocking cmd; do
    printf '  %-12s %-8s %s\n' "$id" "$owner" "$cmd"
  done
  echo
  echo "Sign-offs (a person, in writing):"
  signoffs | while IFS=$'\t' read -r id owner; do
    printf '  %-12s %s\n' "$id" "$owner"
  done
  exit 0
fi

cd "$root"
failed=0
while IFS=$'\t' read -r id owner blocking cmd; do
  [ "$blocking" = "true" ] || continue
  printf '\n=== %s (%s) ===\n' "$id" "$owner"
  if ! bash -c "$cmd"; then
    echo "RELEASE GATE FAILED: $id, owned by $owner"
    failed=1
    break
  fi
done < <(gates)

if [ "$failed" -ne 0 ]; then
  echo ""
  echo "The release is not ready. Fix the gate above and run this again."
  exit 1
fi

echo ""
echo "Every blocking gate passed. Still outstanding, and not checkable by a script:"
signoffs | while IFS=$'\t' read -r id owner; do
  printf '  [ ] %-12s %s\n' "$id" "$owner"
done
echo ""
echo "A release with green gates and no sign-offs is a release nobody has looked at."
