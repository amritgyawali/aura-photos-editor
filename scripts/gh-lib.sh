#!/usr/bin/env bash
# Shared GitHub plumbing for the phase tooling. Sourced by scripts/phase-branch.sh
# and scripts/phase-land.sh; it does nothing on its own.
#
# Everything here happens in the terminal. The order in which a token is looked
# for is deliberate: an explicitly exported token beats an installed `gh`, which
# beats whatever the OS credential manager already holds for github.com. The last
# of those is what makes the flow work on this machine, where `gh` is not
# installed but `git push` has been authenticating over HTTPS for twenty-four
# phases.
#
# The token is never printed, never passed as a command-line argument and never
# written to a file that outlives the call: curl reads the Authorization header
# from a config on stdin, so it does not appear in `ps` output either.

# ---------------------------------------------------------------------------
# Small helpers
# ---------------------------------------------------------------------------

say()  { printf '%s\n' "$*"; }
step() { printf '\n== %s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die()  { printf 'error: %s\n' "$*" >&2; exit 1; }

# An interpreter that actually runs. `command -v python3` is not enough on
# Windows, where it finds the Microsoft Store stub - a program whose whole
# behaviour is to print an advertisement to stdout and exit non-zero, which is
# indistinguishable from a working Python right up until it corrupts the answer.
py_bin() {
  local candidate
  for candidate in python3 python py; do
    if command -v "$candidate" >/dev/null 2>&1 \
       && "$candidate" -c 'import sys' >/dev/null 2>&1; then
      printf '%s' "$candidate"
      return 0
    fi
  done
  return 1
}

# The Python on the Windows development machine is a Windows build, so it cannot
# open an MSYS path such as /tmp/x.json. Everything therefore reaches it on stdin.
json_get() {
  local path="$1" py
  shift
  if py="$(py_bin)"; then
    "$py" "$REPO_ROOT/scripts/json-get.py" "$path" "$@"
  else
    # Last resort for a flat scalar key, enough for `number` and `html_url`,
    # which is all this tooling has to report back.
    sed -n "s/.*\"${path##*.}\"[[:space:]]*:[[:space:]]*\"\{0,1\}\([^\",}]*\)\"\{0,1\}.*/\1/p" | head -1
  fi
}

# Build a JSON object from alternating key and value arguments. Python does the
# escaping when it is there, which it is everywhere this runs; the shell fallback
# handles the four characters a commit log can realistically contain.
json_object() {
  local py
  if py="$(py_bin)"; then
    "$py" -c 'import json,sys
a=sys.argv[1:]
print(json.dumps(dict(zip(a[0::2], a[1::2]))))' "$@"
    return 0
  fi
  local out="{" first=1
  while [ "$#" -ge 2 ]; do
    [ "$first" -eq 1 ] || out="$out,"
    first=0
    out="$out\"$(json_escape "$1")\":\"$(json_escape "$2")\""
    shift 2
  done
  printf '%s}\n' "$out"
}

# JSON string escaping without a JSON library. Backslash first, then the quote,
# then the carriage return and the tab, and the newline last, because that join
# is what turns the stream into one line.
json_escape() {
  printf '%s' "$1" \
    | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e 's/\r//g' -e "s/$(printf '\t')/\\\\t/g" \
    | sed -e ':a' -e 'N' -e '$!ba' -e 's/\n/\\n/g'
}

# ---------------------------------------------------------------------------
# Repository identity
# ---------------------------------------------------------------------------

repo_root() { git rev-parse --show-toplevel 2>/dev/null || die "not inside a git repository"; }

# owner/name out of the origin URL, in either the HTTPS or the SSH spelling.
repo_slug() {
  local url
  url="$(git remote get-url origin 2>/dev/null)" || die "no 'origin' remote"
  url="${url%.git}"
  case "$url" in
    git@*:*)       printf '%s\n' "${url#*:}" ;;
    ssh://git@*/*) printf '%s\n' "${url#*github.com/}" ;;
    https://*)     printf '%s\n' "${url#*github.com/}" ;;
    *)             die "cannot read owner/repo out of the origin URL: $url" ;;
  esac
}

# ---------------------------------------------------------------------------
# Credentials
# ---------------------------------------------------------------------------

# Prints the token on stdout. Callers capture it into a local and never echo it.
gh_token() {
  if [ -n "${GH_TOKEN:-}" ]; then printf '%s' "$GH_TOKEN"; return 0; fi
  if [ -n "${GITHUB_TOKEN:-}" ]; then printf '%s' "$GITHUB_TOKEN"; return 0; fi
  if command -v gh >/dev/null 2>&1; then
    local t
    if t="$(gh auth token 2>/dev/null)" && [ -n "$t" ]; then printf '%s' "$t"; return 0; fi
  fi
  local pw
  pw="$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill 2>/dev/null \
        | sed -n 's/^password=//p')"
  if [ -n "$pw" ]; then printf '%s' "$pw"; return 0; fi
  return 1
}

gh_have_token() { local t; t="$(gh_token 2>/dev/null)"; [ -n "$t" ]; }

# ---------------------------------------------------------------------------
# The API call
# ---------------------------------------------------------------------------
#
#   body="$(gh_api METHOD /path [body-file])"
#   [ "$(gh_status)" = 200 ] || ...
#
# Writes the response body to stdout and the HTTP status to a file, which is what
# `gh_status` reads. The status goes through a file rather than a variable because
# every caller captures the body in a command substitution, and a variable set
# inside that subshell is gone by the time the caller could read it - a bug that
# reads as "every request returned an empty status".
#
# It returns non-zero on a transport failure only: a 4xx is a successful call
# whose status the caller has to interpret, because "a pull request already
# exists" and "the branch is not mergeable" are both ordinary outcomes here.

GH_STATUS_FILE="${TMPDIR:-/tmp}/aura-gh-status.$$"
gh_status() { cat "$GH_STATUS_FILE" 2>/dev/null || printf 'unknown'; }

gh_api() {
  local method="$1" path="$2" body_file="${3:-}"
  local token out status
  token="$(gh_token)" || die "no GitHub credential. Export GH_TOKEN, run 'gh auth login', or let git store one for github.com."

  out="$(mktemp)" || die "mktemp failed"

  local args=(-K - -s -o "$out" -w '%{http_code}' -X "$method")
  if [ -n "$body_file" ]; then
    args+=(-H 'Content-Type: application/json' --data-binary "@$body_file")
  fi

  if ! status="$(
    {
      printf 'header = "Authorization: Bearer %s"\n' "$token"
      printf 'header = "Accept: application/vnd.github+json"\n'
      printf 'header = "X-GitHub-Api-Version: 2022-11-28"\n'
      printf 'user-agent = "aura-phase-tooling"\n'
    } | curl "${args[@]}" "https://api.github.com${path}"
  )"; then
    rm -f "$out"
    printf 'unknown' > "$GH_STATUS_FILE"
    return 1
  fi

  printf '%s' "$status" > "$GH_STATUS_FILE"
  cat "$out"
  rm -f "$out"
  return 0
}

# A one-line reason out of an error body, for a message somebody can act on.
gh_error_message() {
  local body="$1" msg
  msg="$(printf '%s' "$body" | json_get message "" 2>/dev/null)"
  if [ -n "$msg" ]; then printf '%s\n' "$msg"; else printf '%s' "$body" | head -3; fi
}
