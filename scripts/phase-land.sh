#!/usr/bin/env bash
# The last step of the phase ritual, end to end, from the terminal: commit what
# is left, push the phase branch, open the pull request, and merge it into main.
#
#   scripts/phase-land.sh --message "feat(generative): cleanup safety foundation"
#   scripts/phase-land.sh --title "Phase 25 - gallery consistency" --wait-ci 1800
#   scripts/phase-land.sh --no-merge          # open the PR, stop before merging
#   scripts/phase-land.sh --local-merge       # no forge: merge --no-ff and push main
#
# The pull request is created and merged over the GitHub REST API. `gh` is used
# for the token when it is installed and is not required: the OS credential
# manager already holds a github.com credential on any machine that has pushed
# this repository, and scripts/gh-lib.sh reads it without printing it.
#
# What this script refuses to do:
#
#   * merge a pull request whose checks have failed, without --force-merge;
#   * force-push anything, ever;
#   * merge a branch with no commits on it.
#
# It deliberately does not run the phase gate. The gate is step 7 of the ritual
# and it exits 0 before landing is started; running the whole suite again here
# would double a phase's slowest hour and would hide which of the two runs was
# the one that counted.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || {
  printf 'error: not inside a git repository\n' >&2
  exit 1
}
# shellcheck source=scripts/gh-lib.sh
. "$REPO_ROOT/scripts/gh-lib.sh"

usage() {
  cat <<'USAGE'
usage: scripts/phase-land.sh [options]

  Run from the phase branch. Commits, pushes, opens the pull request, merges it.

options:
  --message MSG      commit message for whatever is still uncommitted
  --title TITLE      pull request title (default: derived from the branch)
  --body-file FILE   pull request body (default: the commit log against main)
  --base BRANCH      merge target (default: main)
  --merge-method M   merge | squash | rebase (default: merge)
  --merge-title T    title of the merge commit (default: 'merge: <branch>')
  --wait-ci SECONDS  wait up to SECONDS for checks to finish (default: 0)
  --ignore-check N   a check whose verdict does not gate this merge (repeatable)
  --force-merge      merge even when a check has failed
  --no-merge         create or update the pull request and stop
  --no-commit        do not commit; fail if the tree is dirty
  --no-verify        skip the pre-commit hook on the landing commit
  --delete-branch    delete the phase branch locally and on origin after merging
  --local-merge      no forge: merge --no-ff into base locally and push it
  -h, --help         this message
USAGE
}

message=""
title=""
body_file=""
base="main"
merge_method="merge"
merge_title=""
wait_ci=0
force_merge=0
do_merge=1
do_commit=1
no_verify=0
delete_branch=0
local_merge=0
ignored_checks=()

while [ "$#" -gt 0 ]; do
  case "$1" in
    --message)      message="${2:?--message needs a value}"; shift 2 ;;
    --title)        title="${2:?--title needs a value}"; shift 2 ;;
    --body-file)    body_file="${2:?--body-file needs a value}"; shift 2 ;;
    --base)         base="${2:?--base needs a value}"; shift 2 ;;
    --merge-method) merge_method="${2:?--merge-method needs a value}"; shift 2 ;;
    --merge-title)  merge_title="${2:?--merge-title needs a value}"; shift 2 ;;
    --wait-ci)      wait_ci="${2:?--wait-ci needs a value}"; shift 2 ;;
    --ignore-check) ignored_checks+=("${2:?--ignore-check needs a value}"); shift 2 ;;
    --force-merge)  force_merge=1; shift ;;
    --no-merge)     do_merge=0; shift ;;
    --no-commit)    do_commit=0; shift ;;
    --no-verify)    no_verify=1; shift ;;
    --delete-branch) delete_branch=1; shift ;;
    --local-merge)  local_merge=1; shift ;;
    -h|--help)      usage; exit 0 ;;
    *)              die "unknown option: $1" ;;
  esac
done

case "$merge_method" in
  merge|squash|rebase) ;;
  *) die "unknown --merge-method '$merge_method'" ;;
esac
printf '%s' "$wait_ci" | grep -Eq '^[0-9]+$' || die "--wait-ci wants a number of seconds"

cd "$REPO_ROOT"

branch="$(git rev-parse --abbrev-ref HEAD)"
[ "$branch" != "HEAD" ] || die "detached HEAD - check out the phase branch first"
[ "$branch" != "$base" ] || die "on '$base'. Landing runs from the phase branch, never from the target."

# Phase number and slug out of the branch name, when it has them. They are what
# the default title and merge message are built from; a branch that does not
# follow the convention still lands, it just needs a --title.
phase_nn=""
phase_slug=""
if printf '%s' "$branch" | grep -Eq '^[a-z]+/phase-[0-9]{2}-'; then
  phase_nn="$(printf '%s' "$branch" | sed -E 's|^[a-z]+/phase-([0-9]{2})-.*$|\1|')"
  phase_slug="$(printf '%s' "$branch" | sed -E 's|^[a-z]+/phase-[0-9]{2}-(.*)$|\1|' | tr '-' ' ')"
fi

# ---------------------------------------------------------------------------
# 1. Commit whatever is still in the tree
# ---------------------------------------------------------------------------

step "committing"
if [ -n "$(git status --porcelain)" ]; then
  [ "$do_commit" -eq 1 ] || die "the working tree is dirty and --no-commit was given"
  if [ -z "$message" ]; then
    if [ -n "$phase_nn" ]; then
      message="feat(phase-$phase_nn): $phase_slug"
    else
      message="chore($branch): land outstanding changes"
    fi
    warn "no --message given; using '$message'"
  fi
  git add -A
  if [ "$no_verify" -eq 1 ]; then
    git commit --no-verify -m "$message"
  else
    git commit -m "$message"
  fi
  say "committed: $(git log -1 --oneline)"
else
  say "working tree clean - nothing to commit"
fi

# ---------------------------------------------------------------------------
# 2. Push the branch
# ---------------------------------------------------------------------------

step "pushing $branch"
git fetch origin --prune --quiet

ahead="$(git rev-list --count "origin/$base..HEAD" 2>/dev/null || echo 0)"
[ "$ahead" -gt 0 ] || die "no commits on $branch that origin/$base does not already have. Nothing to land."

if git rev-parse --verify --quiet "refs/remotes/origin/$branch" >/dev/null; then
  git push origin "$branch" 2>&1 | tail -2
else
  git push --set-upstream origin "$branch" 2>&1 | tail -2
fi
head_sha="$(git rev-parse HEAD)"
say "$ahead commit(s) ahead of origin/$base at ${head_sha:0:9}"

# ---------------------------------------------------------------------------
# 3. The local escape hatch
# ---------------------------------------------------------------------------

if [ "$local_merge" -eq 1 ]; then
  step "local merge into $base (no pull request)"
  merge_msg="${merge_title:-merge: $branch}"
  git checkout --quiet "$base"
  git pull --ff-only --quiet origin "$base"
  git merge --no-ff -m "$merge_msg" "$branch"
  git push origin "$base" 2>&1 | tail -2
  say "merged $branch into $base locally and pushed it"
  exit 0
fi

# ---------------------------------------------------------------------------
# 4. Open (or find) the pull request
# ---------------------------------------------------------------------------

slug="$(repo_slug)"
owner="${slug%%/*}"

gh_have_token || die "no GitHub credential for the pull request.
     Export GH_TOKEN, run 'gh auth login', or re-run with --local-merge."

step "pull request on $slug"

existing="$(gh_api GET "/repos/$slug/pulls?state=open&head=$owner:$branch&base=$base")"
[ "$(gh_status)" = "200" ] || die "listing pull requests failed (HTTP $(gh_status)): $(gh_error_message "$existing")"

pr_number="$(printf '%s' "$existing" | json_get 0.number "" || true)"

if [ -z "$pr_number" ]; then
  if [ -z "$title" ]; then
    if [ -n "$phase_nn" ]; then
      title="Phase $phase_nn - $phase_slug"
    else
      title="$(git log -1 --pretty=%s)"
    fi
  fi

  body=""
  if [ -n "$body_file" ]; then
    [ -f "$body_file" ] || die "--body-file '$body_file' does not exist"
    body="$(cat "$body_file")"
  else
    body="$(printf 'Commits on this branch, oldest first:\n\n%s\n' \
      "$(git log --reverse --pretty='- %s' "origin/$base..HEAD")")"
    if [ -n "$phase_nn" ] && [ -f "docs/progress/PHASE-$phase_nn-EXIT.md" ]; then
      body="$body
Exit report: docs/progress/PHASE-$phase_nn-EXIT.md"
    fi
  fi

  req="$(mktemp)"
  json_object title "$title" head "$branch" base "$base" body "$body" > "$req"
  created="$(gh_api POST "/repos/$slug/pulls" "$req")"
  rm -f "$req"
  if [ "$(gh_status)" != "201" ]; then
    die "creating the pull request failed (HTTP $(gh_status)): $(gh_error_message "$created")"
  fi
  pr_number="$(printf '%s' "$created" | json_get number)"
  pr_url="$(printf '%s' "$created" | json_get html_url)"
  say "opened #$pr_number  $pr_url"
else
  pr_url="$(printf '%s' "$existing" | json_get 0.html_url "" || true)"
  say "already open: #$pr_number  $pr_url"
fi

if [ "$do_merge" -eq 0 ]; then
  step "stopping before the merge (--no-merge)"
  say "merge it later with: scripts/phase-land.sh --merge-method $merge_method"
  exit 0
fi

# ---------------------------------------------------------------------------
# 5. Checks
# ---------------------------------------------------------------------------
#
# A pending check is not a reason to stop - CI on this repository builds the whole
# workspace and a phase should not have to hold a terminal open for it - but a
# *failed* check is, and --force-merge is the only way past one.

reduce_state() {
  local py
  if py="$(py_bin)"; then
    "$py" "$REPO_ROOT/scripts/check-state.py" ${ignored_checks[@]+"${ignored_checks[@]}"}
  else
    # Without Python the safe answer is the one that does not wave a merge
    # through on a guess.
    cat >/dev/null; printf 'unknown\n'
  fi
}

check_state() {
  local runs state statuses
  runs="$(gh_api GET "/repos/$slug/commits/$head_sha/check-runs?per_page=100")"
  [ "$(gh_status)" = "200" ] || { printf 'unknown\n'; return 0; }
  state="$(printf '%s' "$runs" | reduce_state)"
  if [ "$state" = "none" ]; then
    # Nothing from the Actions API. A classic commit status may still be there,
    # which is what an external CI reports through.
    statuses="$(gh_api GET "/repos/$slug/commits/$head_sha/status")"
    [ "$(gh_status)" = "200" ] || { printf 'none\n'; return 0; }
    state="$(printf '%s' "$statuses" | reduce_state)"
  fi
  printf '%s\n' "$state"
}

step "checks on ${head_sha:0:9}"
state="$(check_state)"
waited=0
while [ "$state" = "pending" ] && [ "$waited" -lt "$wait_ci" ]; do
  say "checks pending, ${waited}s of ${wait_ci}s waited"
  sleep 30
  waited=$((waited + 30))
  state="$(check_state)"
done
say "checks: $state"

if [ "$state" = "failing" ] && [ "$force_merge" -eq 0 ]; then
  failed=""
  if py="$(py_bin)"; then
    failed="$(gh_api GET "/repos/$slug/commits/$head_sha/check-runs?per_page=100" \
      | "$py" "$REPO_ROOT/scripts/check-state.py" --list-failures \
        ${ignored_checks[@]+"${ignored_checks[@]}"} | paste -sd', ' -)"
  fi
  die "a check on this head has failed${failed:+: $failed}.
     Fix it, name it with --ignore-check, or re-run with --force-merge.
     Pull request: ${pr_url:-#$pr_number}"
fi

# ---------------------------------------------------------------------------
# 6. Merge
# ---------------------------------------------------------------------------

step "merging #$pr_number into $base"

if [ -z "$merge_title" ]; then
  if [ -n "$phase_nn" ]; then
    merge_title="merge: phase $phase_nn $phase_slug"
  else
    merge_title="merge: $branch"
  fi
fi
merge_msg="$merge_title"
req="$(mktemp)"
json_object commit_title "$merge_msg" commit_message "" merge_method "$merge_method" \
  sha "$head_sha" > "$req"
merged="$(gh_api PUT "/repos/$slug/pulls/$pr_number/merge" "$req")"
rm -f "$req"

if [ "$(gh_status)" != "200" ]; then
  die "the merge was refused (HTTP $(gh_status)): $(gh_error_message "$merged")
     Pull request: ${pr_url:-#$pr_number}"
fi

merge_sha="$(printf '%s' "$merged" | json_get sha "")"
say "merged as ${merge_sha:0:9}"

# ---------------------------------------------------------------------------
# 7. Leave the checkout on an up-to-date base
# ---------------------------------------------------------------------------

step "syncing $base"
git checkout --quiet "$base"
git pull --ff-only --quiet origin "$base"
say "$base is at $(git log -1 --oneline)"

if [ "$delete_branch" -eq 1 ]; then
  step "deleting $branch"
  git push origin --delete "$branch" 2>&1 | tail -1 || warn "could not delete origin/$branch"
  git branch -d "$branch" 2>&1 | tail -1 || warn "could not delete the local $branch"
fi

step "done"
say "pull request: ${pr_url:-#$pr_number}"
say "$base now carries $branch"
