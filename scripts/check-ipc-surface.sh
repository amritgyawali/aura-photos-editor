#!/usr/bin/env bash
# The three files that describe the IPC surface have to agree.
#
# PHASE-27 section 11 found this and left it inside its own gate; PHASE-30 lifts it out, because
# the release checklist needs to run it and a check that only runs inside one phase's gate is a
# check that stops running the day that phase is finished.
#
# What it compares:
#
#   1. every `#[tauri::command]` in `ui/src-tauri/src/main.rs`
#   2. every name inside its `generate_handler![...]` list
#   3. every command string the typed client in `ui/src/ipc/client.ts` invokes
#
# All three must be the same set. Phase 21's exit report found ninety client calls reaching a
# window that did not answer to them - `get_preview`, `render_image`, every mask command - because
# nothing anywhere compared these lists.
#
# It proves the names and the syntax and **not the types**: the shell's Rust does not compile on
# the reference machine (no `dlltool`), so this is a symbol cross-check rather than a build. That
# is weaker and the exit reports say so.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
shell="$root/ui/src-tauri/src/main.rs"
client="$root/ui/src/ipc/client.ts"

for f in "$shell" "$client"; do
  if [ ! -f "$f" ]; then
    echo "check-ipc-surface: $f is missing" >&2
    exit 1
  fi
done

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# 1. The definitions: the function name on the line after each `#[tauri::command]`.
grep -A 3 '^#\[tauri::command\]' "$shell" \
  | grep -oE '(async )?fn [a-z0-9_]+' \
  | sed -E 's/(async )?fn //' \
  | sort -u > "$tmp/defined"

# 2. The registrations, from inside `generate_handler![ ... ]`.
awk '
  /generate_handler!\[/ { inside=1; sub(/.*generate_handler!\[/, ""); }
  inside {
    line=$0
    # Comments inside the list are prose, not names. Reading them as names is the lesson phase 27
    # wrote down twice: a check that treats documentation as code fails hardest on the codebases
    # that document themselves best. (No apostrophes in here - the awk program is inside a shell
    # single-quoted string, and one would close it.)
    sub(/\/\/.*/, "", line)
    if (line ~ /\]/) { sub(/\].*/, "", line); inside=0 }
    n=split(line, parts, ",")
    for (i=1; i<=n; i++) {
      gsub(/[ \t\r\n]/, "", parts[i])
      if (parts[i] != "") print parts[i]
    }
  }
' "$shell" | sort -u > "$tmp/registered"

# 3. The client's own command strings.
#
# Two steps rather than one pattern. The obvious pattern - `invoke(<[^>]*>)?\('...'` - looks right
# and misses every call whose type argument nests, `invoke<Array<[string, string]>>(...)` among
# them, because `[^>]*` stops at the first `>`. It found 239 of 240 and reported the missing one as
# dead surface, which is a check that is wrong in the direction of raising a false alarm - the only
# direction that gets a check switched off.
grep "invoke" "$client" \
  | grep -oE "\('[a-z0-9_]+'" \
  | tr -d "('" \
  | sort -u > "$tmp/invoked"

defined=$(wc -l < "$tmp/defined")
registered=$(wc -l < "$tmp/registered")
invoked=$(wc -l < "$tmp/invoked")

fail=0
report() {
  local left="$1" right="$2" message="$3"
  local missing
  missing=$(comm -23 "$tmp/$left" "$tmp/$right")
  if [ -n "$missing" ]; then
    echo "$message"
    echo "$missing" | sed 's/^/  /'
    fail=1
  fi
}

report defined registered "commands defined but never registered - the window will not answer to them:"
report registered defined "commands registered but not defined - the shell will not compile:"
report invoked registered "commands the client calls that the window does not answer to:"
report registered invoked "commands registered that nothing calls - dead surface:"

if [ "$fail" -ne 0 ]; then
  echo ""
  echo "check-ipc-surface failed. $defined defined, $registered registered, $invoked invoked."
  exit 1
fi

echo "check-ipc-surface: $defined = $registered = $invoked"
