# Phase 27 progress - AI Quality Control Agent

One line per task, in the order section 8 asks for them.

| Task | Files touched | Tests added | Note |
|---|---|---|---|
| Step 0 - branch | - | - | `feat/phase-27-ai-qc-agent`, cut and pushed before any code |
| CTO - ADR | `docs/adr/ADR-0055-quality-control-tickets-and-the-re-edit-loop.md` | - | Twelve decisions; the diagnosis is rendered, improvement is measured from what the ticket opened with, the planner cannot execute |
| CTO - ADR | `docs/adr/ADR-0056-qc-ipc-surface.md` | - | Nine commands; the queue is ordered by severity as a ratio, and bulk actions record verdicts without authorising remedies |
| TLC - freeze | `crates/aura-core/src/contract/qc.rs`, `contract/ids.rs` | 65 contract tests | `QcCategory`, 43 `QcCode`s, `QcReason`, `Evidence`, `Remedy`, `TicketStatus`, `QcTicket`, `QcRound`, `Replacement`, `QcReport`, `QcOutline`, `QcOverride`, `QcService`; `TicketId` is the sixteenth typed id |
| TLC - amendment | `crates/aura-core/src/contract/qc.rs` | contract test | `TicketStatus::Dismissed` - the fifth frozen-contract amendment in the product's history. ADR-0055 section 9 |
| SRC - migration | `crates/aura-catalog/migrations/0027_qc.sql` | catalog suite | Four tables, two views, four triggers; the bounds as CHECK constraints and no `diagnosis` column |
| PM - thresholds | `crates/aura-qc/config/qc_thresholds.toml`, `src/policy.rs` | policy tests | 23 argued-over scene rows, each with a written reason; 19 ceilings the code owns and a file may only tighten |
| SRC - checks | `crates/aura-qc/src/checks/` - ten modules plus the port | unit, per module | Every one a pure `fn inspect(&Frame, &Thresholds) -> Outcome`; `Clean`, `Found` and `Skipped` are three values |
| SRC - tickets | `src/ticket.rs`, `src/triage.rs` | unit | A code, a number, a threshold and a reason on every one; root causes worked before their symptoms; the clock is passed in |
| SRC - remedies | `src/remedy.rs` | unit | One rule per category, and `validate` is the single choke point a remedy can be built through |
| SRC - loop | `src/reedit.rs` | unit | Two rounds, half the promised gain, and a collateral check that reverts |
| SRC - replacement | `src/replace.rs` | unit | Four gates in order; the coverage guarantee filters before anything is scored |
| MLL - planner | `src/planner.rs` | unit | `Tier::Reasoning`, $0.06, no pixels, no identity; `ProposedStep` is deliberately not a `Remedy` |
| SRC - report | `src/report.rs` | unit | Leads with what was checked; Markdown rendered in Rust so the archive and the panel agree |
| SRC - store | `src/store.rs`, `src/api.rs` | integration | One transaction per pass; `take_decisions` carries a photographer's verdict across a re-pass |
| QAL - fixtures | `src/fixtures.rs` | - | 21 single-defect frames across nine categories, three multi-symptom frames, a clean gallery and a broken coverage set |
| QAL - gates | `tests/eval/qc_eval.rs` | 12 | Every section 10.1 row, plus the thrash guard and the two refusals |
| QAL - grep test | `crates/aura-qc/tests/no_pixel_ops.rs` | 7 | The seventh grep-as-a-test: no pixels, no recipe writes, no provider, no threshold widening - with a control that proves the stripper strips |
| SFE - IPC | `crates/aura-app/src/qc_commands.rs`, `contract/ipc.rs` | - | Nine commands; `AppField` is where `aura-qc` meets the thirteen deciding crates |
| SFE - readings | `crates/aura-app/src/state.rs` | - | `qc_set_readings` and `qc_frame`; every absent service row becomes a skipped check |
| SFE - shell | `ui/src-tauri/src/main.rs`, `ui/src/ipc/{client,types}.ts` | - | 220 handlers, 220 registered, 220 client wrappers - asserted by the gate for the first time |
| SFE - panels | `ui/src/components/qc/` - five components | 12 vitest | Report, category chips, grouped queue, before-and-after; mounted in `App.tsx` |
| PERF - budgets | `crates/aura-perf/tests/qc_budgets.rs`, `perf/budgets.toml` | 6 | 10 ms over 200 frames against a 90 s budget; 421 B/image against 1,500 B |
| QAL - catalog side | `ml/eval/qc_agreement.py` | self-test | The false-ticket rate against findings a person reviewed, and four bound violations no in-build check could see |
| CTO - gate | `crates/aura-cli/src/phase27.rs`, `main.rs`, `justfile`, CI | - | Eleven checks plus the IPC parity count; exits 0 |
| DOC - docs | `docs/how-qc-works.md` | - | What is checked, what is fixed, what is never done, and what this build cannot tell you |
| EM - registry | `crates/aura-core/errors.toml`, `docs/runbooks/AURA-ML-513{6..9},514{0,1}.md` | registry test | Six codes, six runbooks |

## Benchmark deltas

| Metric | Budget | Measured |
|---|---|---|
| Full QC pass | 90 s / 1,000 images | 10 ms over 200 frames |
| One remediation round | 1.2 s | under 1 ms |
| Report assembly | 3 s | under 1 ms |
| Storage | 1,500 B/image | 421 B/image worst case, 1 B/image on a clean gallery |

The pass is four orders of magnitude inside its budget for phase 25's and phase 26's reason: **it
opens no pixels**. Every number it inspects was stored by phases 08 to 26. The budget keeps its full
section 11 value rather than being re-based, because the re-edit loop in this build applies remedies
through a test double: when phases 15, 16 and 19 are wired to re-solve for real, a round becomes a
render rather than an arithmetic update, and 1.2 s is the figure that matters then.

## What the storage measurement corrected

The 421 B is a **worst case over a gallery where every frame carries findings**, and a clean gallery
measures 1 B per image - one `qc_run` row spread over the project. That spread is the opposite shape
from every phase between 09 and 20, which store one fixed-width verdict per photograph, and the same
shape as phase 21, which stores a list whose length is the number of things that were wrong.

Phase 21's rule applies to the sentence as much as to the figure: this note was written after the
measurement rather than before it.

## Two things the gate caught that the unit tests could not

**Gate 6 caught an escalated ticket being remediated again.** `TicketStatus::is_open()` is true for
`Open`, `Escalated` and `Reverted`, and both `triage::order` and `reedit::may_retry` read it - so a
finding already handed to a person consumed its second round without anybody asking for it. Every
unit test passed, because each one exercised a single round. The predicate is
`status == TicketStatus::Open` in three places now.

**A contract test was line-ending-dependent and failed only on Windows.**
`index_contract.rs` sliced the `IndexEvent` declaration by searching for a blank line as `"\n\n"`.
The working tree is CRLF here, the search never matched, and the test read the rest of `types.ts`
as part of the declaration — failing on a `cameraId` phase 26 had added three hundred lines away.
It is fixed rather than skipped: the slice looks for `"\n\r\n"` first.

**The gate's own schema scan matched migration 27's prose.** `sqlite_master.sql` stores a migration
verbatim, comments included, and migration 27's header is four paragraphs about why there is no
`diagnosis` column - so the check that enforces the rule failed on the text that documents it. The
scan strips `--` comments now. This is the second time in this phase: a grep test asserting the skin
module holds no fixed target had already failed on its own test name.
