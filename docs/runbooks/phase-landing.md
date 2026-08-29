# Runbook: branching, landing and merging a phase

Two commands bracket every phase. The first is the first thing that happens in a phase and
the second is the last, and neither of them waits to be asked.

```bash
scripts/phase-branch.sh 25 gallery-consistency          # step 0 of the ritual
# ... the whole phase ...
scripts/phase-land.sh --message "feat(gallery): ..."    # step 9 of the ritual
```

`just phase-start 25 gallery-consistency` and `just phase-ship "feat(gallery): ..."` are the
same two commands with less to type.

## Why the branch is pushed before the work rather than after it

Until phase 24 the rule was "commit and push at the end of the phase". The consequence was
that a phase - a month of decisions, a migration, a frozen contract, an exit report - lived
on exactly one disk for as long as it took to write. Pushing an empty branch costs one round
trip and buys three things immediately:

- a name everybody can see, so two people cannot start phase 25 twice under two spellings;
- a place for the pull request to hang off, which means the diff is reviewable from the
  first commit rather than the last;
- a commit to bisect back to, which is the thing you want on the morning a phase turns out
  to have broken something in phase 09.

The push is idempotent. Running `phase-branch.sh` again on a branch that already exists
checks it out, reconciles the upstream, prints how far ahead and behind it is, and changes
nothing else. It never force-pushes.

## What `phase-land.sh` does, in order

1. **Commits** whatever is still in the working tree, through the pre-commit hook, with the
   message you gave it. A clean tree is fine; it says so and moves on.
2. **Pushes** the branch, setting the upstream if it is missing. It refuses to run if the
   branch has no commits that `origin/main` does not already have.
3. **Opens the pull request** over the GitHub REST API - or finds the one that is already
   open for this branch, which is what happens the second time you run it.
4. **Reads the checks** on the head commit. A pending check does not stop it; a *failed*
   check does.
5. **Merges** into `main` with a merge commit titled `merge: phase NN <slug>`.
6. **Leaves the checkout on an up-to-date `main`**, so the next phase's `phase-branch.sh`
   cuts from the right place.

## Authentication

The scripts look for a token in this order, and print none of them:

1. `GH_TOKEN`
2. `GITHUB_TOKEN`
3. `gh auth token`, when `gh` is installed
4. the OS credential manager, through `git credential fill` for `github.com`

The fourth is what makes this work on the Windows development machine, where `gh` is not
installed but `git push` has been authenticating over HTTPS since phase 01. The token is
never passed as a command-line argument: `curl` reads the `Authorization` header from a
config on stdin, so it does not appear in `ps` output either.

## The failed-check rule

A merge is refused when a check on the head commit has failed. There are two ways past it,
and they are not equivalent:

```bash
scripts/phase-land.sh --ignore-check benchmarks   # excuse one named job
scripts/phase-land.sh --force-merge               # excuse every job at once
```

Reach for the first. `benchmarks` is red on `main` for a recorded reason - the interactive
render budget is waived while no `wgpu` backend is linked, phase 14 condition C1 - and
naming it excuses that job and nothing else. `--force-merge` also excuses the test job
somebody broke this morning.

## When there is no forge to reach

```bash
scripts/phase-land.sh --local-merge
```

commits, pushes the branch, then merges it into `main` locally with `--no-ff` and pushes
`main`. No pull request is created. Use it when the API is unreachable and the work has to
land anyway; the branch is still on `origin`, so the review can happen after the fact.

## Common outcomes

| What you see | What it means | What to do |
|---|---|---|
| `no commits on <branch> that origin/main does not already have` | The phase is already merged, or nothing was committed. | Check `git log origin/main..HEAD`. |
| `the merge was refused (HTTP 405)` | GitHub will not merge it - usually a conflict with `main`. | `git fetch origin && git merge origin/main`, fix, re-run. |
| `a check on this head has failed: benchmarks` | The known-red job. | Re-run with `--ignore-check benchmarks`. |
| `no GitHub credential` | Nothing in the four sources above had a token. | `gh auth login`, export `GH_TOKEN`, or `--local-merge`. |
| `working tree is dirty` from `phase-branch.sh` | You are cutting a phase branch over unfinished work. | Commit or stash it. `--allow-dirty` if you meant it. |

## Options worth knowing

```
phase-branch.sh <NN> <slug> [--kind feat|fix|chore|...] [--from REF] [--allow-dirty] [--no-fetch]

phase-land.sh [--message MSG] [--title TITLE] [--body-file FILE] [--base BRANCH]
              [--merge-method merge|squash|rebase] [--merge-title T]
              [--wait-ci SECONDS] [--ignore-check NAME] [--force-merge]
              [--no-merge] [--no-commit] [--no-verify] [--delete-branch] [--local-merge]
```

`--wait-ci 1800` polls every thirty seconds for up to half an hour before deciding, which is
what you want when you would rather the merge waited for CI than went in ahead of it.
`--no-merge` opens the pull request and stops, for a phase that wants a human review before
it lands.

## What the tooling will not do

- It never force-pushes, in any mode.
- It never merges a pull request whose checks have failed without being told which ones to
  excuse.
- It never runs the phase gate. That is step 7 of the ritual and it has exited 0 before
  landing starts; running the suite again here would double a phase's slowest hour and hide
  which of the two runs was the one that counted.
