# CLAUDE.md - operating manual for this repository

The full operating manual, the nine invariants and the phase ritual live in
[docs/plan/CLAUDE.md](docs/plan/CLAUDE.md). Read that first. This file records what
is specific to the checked-out repository.

## Reading order for an agent

1. `docs/plan/CLAUDE.md` - invariants, phase ritual, hard rules for code.
2. `docs/plan/12-ENGINEERING-CONSTITUTION.md` - the binding engineering rules.
3. `docs/adr/` - every recorded decision; the newest ADR wins over older prose.
4. The single phase file you are implementing, and nothing else.

Never load two phase files into one session.

Look things up when the task needs them:

- [docs/where-things-are.md](docs/where-things-are.md) - where every feature, config file,
  evaluation gate, harness and product doc lives (one row per concern).
- [docs/history/current-state.md](docs/history/current-state.md) - what each phase built,
  its open exit conditions, and the rules and lessons each phase added. Read the section for
  any phase whose code you are changing.

## Where things are (most-used)

| Concern | Location |
|---|---|
| Error registry | `crates/aura-core/errors.toml` (one runbook per code in `docs/runbooks/`) |
| Pinned model set | `models/models.lock` + `models/manifest.sig`, checked by `cargo xtask models` |
| Model cards | `docs/model-cards/` |
| Frozen contracts | `crates/*/src/contract/**`, `crates/aura-catalog/migrations/0001_init.sql`, `ui/src/ipc/types.ts` |
| Contract digests | `contracts.lock`, checked by `cargo xtask contracts --check` |
| Budgets | `perf/budgets.toml`, asserted by `cargo test -p aura-perf` |
| Phase progress | `docs/progress/PHASE-0N.md` and `PHASE-0N-EXIT.md` |
| Branching, landing and merging a phase | `scripts/phase-branch.sh`, `scripts/phase-land.sh`, `docs/runbooks/phase-landing.md` |
| Everything else | [docs/where-things-are.md](docs/where-things-are.md) |

## Non-negotiables enforced by the build

- `scripts/check-banned.sh` fails on `unwrap()`, `expect(`, `panic!`, `HashMap::new`,
  `SystemTime::now`, `Instant::now` and `any` in UI source, outside tests, benches,
  `xtask` and `main.rs`.
- Every crate root carries the lint block, including `#![forbid(unsafe_code)]`.
- `aura-core` depends on no other workspace crate; a test asserts it.
- Changing a frozen contract requires an ADR and a re-lock, in that order.
- **Every phase begins by cutting its branch and pushing it**, and **ends by merging its
  own pull request into `main`**, without being asked. Steps 0 and 9 of the ritual in
  `docs/plan/CLAUDE.md`, and two commands:

  ```bash
  scripts/phase-branch.sh 25 gallery-consistency        # step 0, before any code
  scripts/phase-land.sh --message "feat(gallery): ..."  # step 9, after the gate exits 0
  ```

  The first cuts `feat/phase-NN-<slug>` off an up-to-date `origin/main` and pushes it
  immediately, so a phase is visible from its first minute rather than its last. The
  second commits what is left, pushes, opens the pull request over the GitHub REST API,
  refuses to merge on a failed check, merges into `main` and leaves the checkout on an
  up-to-date `main`. `just phase-start` and `just phase-ship` are the same two commands.
  `gh` is used for the token when it is installed and is not required; the runbook is
  `docs/runbooks/phase-landing.md`.

## Building on this machine

The Windows SDK is absent, so the MSVC linker is not available. Use the GNU host
toolchain for everything:

```bash
RUSTUP_TOOLCHAIN=1.97.1-x86_64-pc-windows-gnu cargo test --workspace --all-targets
```

`cargo run --release --package xtask` is the one exception that does not link:
`windows-sys` needs `dlltool` for a release import library and MinGW is not
installed. Run xtask in debug (`cargo xtask ...`, which is what the alias does).

**This machine needs `AURA_PERF_HOST_SCALE=6`**, for `cargo test -p aura-perf` and
for `scripts/check-phase-gates.sh` alike. Every wall-clock budget in the product is
stated against a reference machine and multiplied by that variable; without it
phase 04's call-overhead budget and phase 14's proxy guardrail both fail on timing
alone, which reads as a regression and is a measurement of this laptop.

```bash
AURA_PERF_HOST_SCALE=6 scripts/check-phase-gates.sh
```

## State in brief

All thirty phases are implemented, plus post-plan work (bring-your-own AI providers over TLS,
portrait retouch, editing and finishing tools; see the history file). The product is
architecturally complete and evidentially empty: every model-capability flag is false, the
shipped heads are placeholders, nothing is calibrated, no real camera file has been decoded, no
lens or brand baseline is measured, there is no GPU backend or network transport, and nothing is
signed. Every quality number was measured against fixtures this repository authored. **The first
real camera file reopens phase 02's criteria whatever phase is in flight.**

Rules that hold across phases (details and the reasoning behind each are in the history file):

- **One service per concern.** Each phase's frozen service (`RenderService`, `MaskService`,
  `ToneService`, ...) is the only way to ask its question. Never keep a second implementation.
- **A person's decision is unbeatable.** Nothing automatic overwrites a user-edited value;
  `aura_recipe::schema::merge` is the only function that writes one recipe into another.
- **A guarantee is measured, not asserted** - on the rendered pixel, and stored on the row.
- **An absent input is ignorance, not permission.** Clean and skipped are different values;
  report coverage and say what the denominator is.
- **Bump the version column** (`PIPELINE_VER`, `MODEL_VER`, `ANALYSIS_VER`, ...) whenever the
  thing it keys changes; never compare across versions silently.
- **A component with passing tests is not a mounted component** (`ui/src/reachability.test.ts`).
- **A guard written in a doc comment is not a guard** - write the test that asserts it.
- **A check that only runs inside one phase's gate stops running.** Cross-product checks belong
  in `scripts/`, in CI and in `ops/release/release.toml` - all three.
