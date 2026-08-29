#!/usr/bin/env bash
# Step 0 of the phase ritual: cut the phase branch off an up-to-date main and
# push it to origin before a single line of the phase is written.
#
#   scripts/phase-branch.sh 25 gallery-consistency
#   scripts/phase-branch.sh 25 gallery-consistency --from origin/main
#   scripts/phase-branch.sh 25 hotfix-crop-safety --kind fix
#
# Why the push happens first rather than last. Until phase 24 the rule was "push
# at the end of the phase", and the whole of a phase therefore existed on one
# disk for as long as the phase took. Pushing an empty branch costs nothing, and
# from that moment a phase has a name everybody can see, a place for a pull
# request to hang off, and a point to bisect back to. It also makes the phase
# visible the moment it starts rather than the moment it finishes, which is the
# difference between a branch list that describes the work and one that describes
# the archive.
#
# The script is idempotent. Run it again on a branch that already exists and it
# checks it out, reconciles the upstream and prints where it stands.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  printf 'error: not inside a git repository\n' >&2
  exit 1
}
# shellcheck source=scripts/gh-lib.sh
. "$REPO_ROOT/scripts/gh-lib.sh"

usage() {
  cat <<'USAGE'
usage: scripts/phase-branch.sh <NN> <slug> [options]

  <NN>            two-digit phase number, e.g. 25
  <slug>          kebab-case name, e.g. gallery-consistency

options:
  --kind KIND     branch prefix: feat (default), fix, chore, perf, docs
  --from REF      base to cut from (default: origin/main)
  --allow-dirty   do not refuse to run with uncommitted changes
  --no-fetch      skip 'git fetch origin'
  -h, --help      this message
USAGE
}

kind="feat"
base="origin/main"
allow_dirty=0
do_fetch=1
nn=""
slug=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --kind)        kind="${2:?--kind needs a value}"; shift 2 ;;
    --from)        base="${2:?--from needs a value}"; shift 2 ;;
    --allow-dirty) allow_dirty=1; shift ;;
    --no-fetch)    do_fetch=0; shift ;;
    -h|--help)     usage; exit 0 ;;
    -*)            die "unknown option: $1" ;;
    *)
      if [ -z "$nn" ]; then nn="$1"
      elif [ -z "$slug" ]; then slug="$1"
      else die "unexpected argument: $1"
      fi
      shift ;;
  esac
done

[ -n "$nn" ] && [ -n "$slug" ] || { usage; exit 2; }

case "$kind" in
  feat|fix|chore|perf|docs|refactor|test) ;;
  *) die "unknown --kind '$kind'" ;;
esac

# A one-digit phase number would sort wrong beside the twenty-four branches that
# already exist, and a slug with a capital or a space is a branch nobody can type.
printf '%s' "$nn" | grep -Eq '^[0-9]{2}$' || die "phase number must be two digits, got '$nn'"
printf '%s' "$slug" | grep -Eq '^[a-z0-9]+(-[a-z0-9]+)*$' \
  || die "slug must be lower-case kebab-case, got '$slug'"

branch="$kind/phase-$nn-$slug"

cd "$REPO_ROOT"

if [ "$allow_dirty" -eq 0 ] && [ -n "$(git status --porcelain)" ]; then
  die "working tree is dirty. Commit, stash, or pass --allow-dirty.
     Cutting a phase branch over somebody else's half-finished edit is how two
     phases end up in one commit."
fi

if [ "$do_fetch" -eq 1 ]; then
  step "fetching origin"
  git fetch origin --prune --quiet
fi

git rev-parse --verify --quiet "$base" >/dev/null || die "base ref '$base' does not exist"

remote_has=0
git ls-remote --exit-code --heads origin "$branch" >/dev/null 2>&1 && remote_has=1
local_has=0
git rev-parse --verify --quiet "refs/heads/$branch" >/dev/null && local_has=1

step "branch $branch"

if [ "$local_has" -eq 1 ]; then
  say "already exists locally - checking it out"
  git checkout --quiet "$branch"
elif [ "$remote_has" -eq 1 ]; then
  say "already exists on origin - checking out a tracking branch"
  git checkout --quiet -b "$branch" "origin/$branch"
else
  say "cutting from $base"
  git checkout --quiet -b "$branch" "$base"
fi

step "publishing to origin"
if [ "$remote_has" -eq 1 ]; then
  # Never force. If the remote has moved ahead, that is somebody else's work and
  # this script is not the place to decide what happens to it.
  git branch --set-upstream-to "origin/$branch" "$branch" >/dev/null 2>&1 || true
  if git push origin "$branch" 2>&1 | tail -2; then
    say "upstream is origin/$branch"
  else
    warn "push refused - origin/$branch has commits this branch does not. Pull first."
  fi
else
  git push --set-upstream origin "$branch" 2>&1 | tail -2
fi

behind="$(git rev-list --count "HEAD..$base" 2>/dev/null || echo 0)"
ahead="$(git rev-list --count "$base..HEAD" 2>/dev/null || echo 0)"

step "state"
say "branch:   $branch"
say "base:     $base"
say "ahead:    $ahead commit(s)"
say "behind:   $behind commit(s)"
say ""
say "Phase $nn is now visible to everybody. Build it, then land it with:"
say "  scripts/phase-land.sh --message 'feat(<lane>): <what changed>'"
