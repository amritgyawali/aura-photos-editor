# ADR-0006 - Starting phase 03 with the three phase 02 exit conditions open

- **Status:** accepted
- **Date:** 2026-08-12
- **Deciders:** CTO, PM, QAL, COL, DEVOPS
- **Phase:** 03 (entry)

## Context

`docs/progress/PHASE-02-EXIT.md` section 8 says phase 03 may start once three
things have happened:

1. one real RAW per supported manufacturer has been decoded and added to the
   fixture corpus;
2. a photographed ColorChecker from at least one real body has been rendered and
   signed off by COL;
3. the CI matrix has run the phase lanes on Windows, macOS and Linux.

None of the three can be satisfied by work inside this repository. Conditions 1
and 2 need physical camera files and a photographed chart, which only the product
owner can supply. Condition 3 needs a push to the GitHub remote and a GitHub
Actions run; `.github/workflows/ci.yml` already defines the three-OS matrix, but
nothing has run it.

Every gate that *can* be run locally was re-run on 2026-08-12 before this ADR was
written, on Windows 11 with the `1.97.1-x86_64-pc-windows-gnu` host toolchain
(ADR-0002 section 7):

| Gate | Result |
|---|---|
| `cargo test --workspace --all-targets` | exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo fmt --all -- --check` | clean |
| `bash scripts/check-banned.sh` | `check-banned: clean` |
| `cargo run -p xtask -- contracts --check` | `contracts: 10 entries, all locked` |
| `verify --phase 02` | `all fixtures clean`, worst mean dE2000 0.158 |

## Decision

Phase 03 starts now, under a written waiver as required by Article XXIV of the
Engineering Constitution ("there is exactly one exception mechanism: a written
waiver, naming the rule, the reason, the risk, the expiry date and the approving
veto-holder").

| Field | Value |
|---|---|
| Rule waived | `PHASE-02-EXIT.md` section 8, conditions 1, 2 and 3 |
| Reason | All three need inputs that do not exist in the repository: camera files, a photographed chart, and a CI run on the remote. Blocking indefinitely on inputs nobody in the loop can produce stops the project rather than protecting it. |
| Risk accepted | The RAW decoder's manufacturer rows stay *reasoned* rather than *measured*, and the colour pipeline stays *self-consistent* rather than *verified*. If a real NEF or ARW disagrees with our reading of its format, the defect is found later than it should have been. |
| Why the risk is bounded here | Phase 03 builds `aura-infer` and `aura-models`. It reads no RAW file, renders no pixel, and does not link against `aura-raw`. A decode defect found in phase 04 or later invalidates nothing built in phase 03. |
| Expiry | The waiver expires the day the first real camera file lands in `tests/fixtures/`. At that point conditions 1 and 2 are re-run as written, and any resulting decode defects are Sev 2 work that preempts the then-current phase. |
| Not waived | Phase 02's *local* gates. Every one of them stays green for the whole of phase 03; a phase 03 commit that reddens a phase 02 gate is reverted, not waived. |
| Approving veto-holders | CTO (offline guarantee, layering), QAL (test completeness), COL (colour sign-off deferred, not cancelled), PERF (the ADR-0004 waiver stays live and unchanged) |

## Consequences

- `docs/progress/PHASE-02-EXIT.md` is not edited. Its section 8 keeps saying what
  it said; this ADR records that we proceeded anyway and on what terms. An ADR
  supersedes prose (`CLAUDE.md` section 1), and the reasoning at the time is the
  valuable part (Article XIV, N2).
- The three conditions are carried forward verbatim into
  `docs/progress/PHASE-03-EXIT.md` as inherited debt so they cannot quietly
  disappear between phases.
- `docs/camera-support.md` keeps its "reasoned, not measured" language. Nothing in
  phase 03 upgrades any claim about a camera.
- The first real camera file is a **Sev 2 trigger**: it reopens phase 02's
  acceptance criteria immediately, whatever phase is in flight.

## Options rejected

- **Stop until the owner supplies camera files.** This is the correct answer if
  phase 03 depended on decode. It does not, and an idle project is not a safer
  project.
- **Quietly start phase 03 and mention it in the exit report.** This is the
  failure mode Article XXIV exists to prevent: a "just this once" that was never
  written down becomes custom.
- **Delete the conditions from the phase 02 exit report.** Rewriting history to
  make a gate pass is the one thing the constitution never permits (N2).
- **Synthesise a "real" camera file.** We already have synthetic fixtures; calling
  one of them real would be fabricating a measurement, banned by AI7.
