# Phase 30 exit report - Delivery, Integrations, Learning Loop & Release Engineering

**Status:** implemented conditionally. Six conditions, four of them Sev 2.

This is the last phase of the plan. There is no phase 31 to carry the conditions forward into, so
they belong to the product now rather than to the next piece of work, and every one of them is
printed by `aura-cli verify --phase 30` on every run.

**Nothing in this build may be described as delivered to a client gallery, fitted from a
photographer's corrections, signed, notarised, or measured for crash-free rate.**

---

## 1. What shipped

| Deliverable | Where |
|---|---|
| Frozen contracts | `crates/aura-core/src/contract/delivery.rs`, `contract/learn.rs` |
| Decisions | `docs/adr/ADR-0061-delivery-learning-loop-and-release.md`, `ADR-0062-delivery-ipc-surface.md` |
| Export | `crates/aura-export/src/` — naming, resample, ICC, metadata, three writers, verification, manifest, presets, store |
| Delivery | `crates/aura-delivery/src/` — backup, providers, transport, per-set mapping, resumable upload, store |
| Learning | `crates/aura-learn/src/` — capture, attribution, aggregation, the update, review, rollback, store |
| Schema | `crates/aura-catalog/migrations/0030_delivery.sql` — 13 tables, 3 views, 5 triggers |
| Policy | `crates/aura-export/config/export_presets.toml` — six presets, each with a written reason |
| Plugins | `plugins/lightroom/aura.lrdevplugin/`, `plugins/photoshop/aura-uxp/` |
| Release machinery | `ops/release/`, `ops/sign/`, `ops/notarise/`, `ops/update/`, `ops/crash/`, `ops/flags/` |
| IPC | `crates/aura-app/src/delivery_commands.rs`, `learn_commands.rs` — seventeen commands |
| Panels | `ui/src/components/delivery/` — five pure views and a container, 17 tests, mounted in `App.tsx` |
| Gates | `tests/eval/delivery_eval.rs` (21), `crates/aura-learn/tests/no_guarantee_learning.rs` (5), 64 + 26 + 17 unit tests, 9 + 8 + 11 integration tests, 20 contract tests |
| Budgets | `crates/aura-perf/tests/delivery_budgets.rs` (5), `perf/budgets.toml` |
| Executable gate | `cargo run --release -p aura-cli -- verify --phase 30` — twelve checks |
| In the product's own voice | `docs/delivery.md`, `docs/learning-loop.md`, `docs/release-process.md`, `docs/privacy.md` |

**No model.** The tenth phase since 08 to ship none, and the reason is a fourth one, distinct from
"nothing to train" (17, 23, 25, 29), "no data" (24) and "a model would be worse than a measurement"
(27, 28). Here there is nothing a model would be *for*: an export is arithmetic and file I/O, a
digest is a digest, and the learning loop is a trimmed median. A phase that trained something to
decide how to write a JPEG would be a phase that had given the writer an opinion.

**One rule this phase is built on, and it is a negative.**
`crates/aura-learn/tests/no_guarantee_learning.rs` is the tenth grep-as-a-test in the repository. It
fails the build if `Learnable` grows a member naming a guarantee, if the crate reaches a renderer,
opens a file, opens a socket, or acquires a field that could carry a threshold. `Learnable` is
closed at fifteen members and **has no `Other`**, which is the whole enforcement: a preference the
vocabulary cannot name is a preference the loop cannot learn.

---

## 2. Acceptance criteria (section 13)

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | Export with quality, resize, sharpen, ICC, metadata, naming and per-set output | **Met** | 24 files across three formats and three templates in the gate; six presets; every file carries its profile |
| 2 | Every written file is verified | **Met** | `verify::write_and_verify` re-opens and re-reads; the gate corrupts a file and the pass detects it; the digest stored is the digest of the file on disk |
| 3 | A delivery bundle with checksums, and backup to a second destination | **Met** | The manifest is sealed once and cannot be updated; backup reports matched, missing and **diverged**, and diverged halts |
| 4 | Lightroom and Photoshop hand-off | **Partly met** | XMP sidecars and both plugins ship. Neither has been run against a real installation of either application. Condition C6 |
| 5 | Client gallery upload with resume and per-set mapping | **Partly met** | The provider trait, mapping, state machine and resume all ship and are exercised against a transport that drops on demand. **No socket ships.** Condition C3 |
| 6 | Corrections captured, attributed, aggregated and offered for review | **Met** | 15 learnables, two floors, a trimmed fit, a held-out measurement and an explicit offer; `learning_loop.rs` runs the whole path |
| 7 | Style match improves >= 15 % after three corrected weddings | **Not met** | Unmeasured. There are no consented archives. Condition C4 |
| 8 | Signed model packs, staged rollout, one-click rollback | **Partly met** | Rollback is built and tested — ten versions, exact restore, and an update that was rolled back is not offered again. Signing and rollout are specified and have not been executed. Condition C5 |
| 9 | Crash-free session rate >= 99.5 % | **Not met** | No closed beta. Condition C5 |

---

## 3. Section 10.1 gates

| Gate | Result |
|---|---|
| Every exported file's digest matches a re-read | **Met** — 24 files in the gate, 60 in the budget suite, every one |
| A corrupted write is detected and stops the job | **Met** — the gate corrupts a file between write and check and the pass refuses |
| 4,000 frames sharing 12 original stems produce 4,000 unique names | **Met** — every collision suffixed, deterministically, planned before a byte is written |
| A naming template cannot escape the destination | **Met** — `{date}/{seq}` refused; dot runs collapsed; a name that tidies to nothing gets its sequence number |
| Location is absent from a delivered file by default | **Met** — the block is **built**, so an unknown tag cannot survive by accident |
| An update needs 12 corrections from 2 projects | **Met** — both floors, per bucket |
| An update moves a value by at most half of what was asked | **Met** — and never past that value's own ceiling |
| An update improves the held-out corrections, or is not offered | **Met** — a quarter held out by the correction's own id, a 2 % floor below which nothing is offered |
| No guarantee is learnable | **Met** — structurally: the vocabulary has no member for one and no `Other` |
| A rollback restores the previous profile exactly | **Met** — and the rolled-back update is not offered again |
| Upload resumes from where it stopped | **Met against a scripted transport** — the offset comes from the far end, not from local state |
| Export 1,000 45 MP JPEGs in 12 minutes | **Waived** — no GPU backend. Condition C2 |
| Crash-free >= 99.5 % | **Not measured**. Condition C5 |

---

## 4. Conditions

**C1 — every pixel in every delivered file came from a placeholder. Sev 2.**
The writers are exact and the verification is real, and what they write is a render through camera
profiles nobody measured (phase 14's C2) from heads that are placeholders (phase 05's C10 and every
condition that closes with it). A delivery from this build is a byte-perfect, digest-verified,
correctly-profiled file containing pixels that are not yet a claim about a photograph. **The
guarantee this phase makes is about the bytes and not about the picture**, and `docs/delivery.md`
says so. Closes with phase 05's C10 and phase 14's C2 rather than separately.

**C2 — the headline export budget is waived. Sev 2.**
Section 11's first two rows — 1,000 45 MP JPEGs in twelve minutes, 1.4 images a second — are
dominated by the render graph rather than by the writer, and this build links no `wgpu` backend
(phase 14's C1). What is measured instead is the writer and the read-back: the verification overhead
is 2 % against an 8 % budget, which is the number that decides whether anybody switches the
guarantee off. **No claim about export throughput may be made from this build.** Closes with a GPU
backend.

**C3 — no network transport ships, so nothing has been uploaded. Sev 2.**
`scripts/check-banned.sh` forbids a socket outside `aura-cloud`, and a client-gallery provider is
not a model provider. Rather than widen the rule or route a delivery through a crate built for
prompts and cost governors, everything above the socket was built and the socket was not:
the `Transport` port, the provider registry, the per-set mapping, the chunked resumable state
machine, the digest comparison and the three-attempt bound are all real and all exercised against a
folder transport and a scripted one that drops on demand. `NETWORK_TRANSPORT_AVAILABLE` is false and
is on the wire, and the panel says so rather than failing at 60 %. **What is proved is the state
machine; what is unproved is that any real provider's API accepts what it would send.** ADR-0061
decision 4. Closes with a transport and one recorded session per provider.

**C4 — no profile has been fitted from a real photographer's corrections. Sev 2.**
Section 9's DATA row asks for consented correction histories from working photographers, and there
are none. Every archive in every gate is authored: the corrections were chosen, aggregated through
the real fitter, split by the real rule and measured on the real held-out set. That proves the
floors bind, the trim works, the step is bounded, the improvement is measured on data the fit never
saw, the offer is refused below 2 % and the rollback is exact. **It proves nothing about whether a
photographer would recognise the result.** `FITTED_ON_REAL_CORRECTIONS` is false and is on the wire,
so a panel cannot present an authored fit as a learned one, and the fifteen per cent style-match
improvement of section 13 is unmeasured. Compounds with phase 17's C1, which is the same absence one
layer down.

**C5 — the release machinery is specified and has not been executed.**
Nine executable gates, four sign-offs, a signing script, a notarisation script, a staged rollout
with a crash-free floor, feature flags with kill switches and an opt-in crash reporter. Nothing has
been signed, because there is no certificate here; nothing has been notarised, because there is no
Apple account; no rollout has run, because there is no install base; and the 99.5 % crash-free rate
is a floor with no measurement behind it, because there has been no closed beta.

`ops/release/check.sh` runs the nine gates today. Eight are green. The ninth — `budgets` — fails on
this container on **phase 14's** rows and not on this phase's: the processor-path proxy render takes
801 ms against a 450 ms budget, because that budget was measured on a machine roughly four times
faster and assumes a GPU backend this build does not link. `AURA_PERF_HOST_SCALE=4` is what a slower
host sets, and every budget in the workspace passes at it, this phase's two included. The budget was
not moved: lowering a guardrail to make a slow container green is how a guardrail stops being one.

**The procedure is evidence of intent and not of a release.**

**A gap this phase's own checklist had, found by running it.** The `ui` gate was `npm test`, which
runs vitest and nothing else — and vitest transpiles rather than type-checks, so a type error in a
panel test passed the release gate and would have failed CI. The gate runs `npm run lint` as well
now. It is the second time in this phase that a checker under-reported and looked like a clean
codebase; `scripts/check-ipc-surface.sh` was the first.

**C6 — neither plugin has met the application it is for.**
The Lightroom plugin and the Photoshop hand-off are written against the published shapes of both
APIs and neither has been loaded into either application, because neither is installed here. The
sidecar half is testable and tested — an XMP AURA writes is parsed back in `export_pass.rs` — and the
plugin half is not. A photographer's first attempt is where the first real error message will come
from.

---

## 5. What is deliberately absent

- **No `export_destination` that automation can set.** A destination is a decision a photographer
  makes about a client. The autopilot repeats an export a wedding was already given and skips with
  `SkipCause::NoInput` when there is none; there is no setting, no default folder, and no code path
  in which a run picks a place to write three thousand files.
- **No strength, threshold or weight on the learning surface.** An update is offered or refused. A
  photographer cannot dial the loop up, and neither can a studio's config file, because a loop with
  a strength knob is a loop that can be turned past its own bounds.
- **No `Other` on `Learnable`.** The vocabulary is closed at fifteen. This is the single most
  important line in the phase: an open vocabulary is one where the next feature adds "retouch
  texture floor" as a learnable and the guarantee erodes one correction at a time.
- **No `Approve` anywhere in the delivery path.** The one cloud call phase 24 gave this shape
  is inherited rather than re-argued.
- **No deletion of an original, anywhere.** Ninth phase running.
- **No file written outside the destination.** A naming template cannot contain a separator, and the
  refusal is in the parser rather than in a check somewhere downstream of it.

---

## 6. What phase 30 closes

**Phase 28's condition C7.** "This build writes no files" — half closed by phase 29 and closed here.
`AppRunner::availability` is empty for the first time since phase 28 wrote it: every stage in the
autopilot's DAG exists, and a completed run writes a delivery. `crates/aura-jobs/src/stages/deliver.rs`
predicted exactly this and its prediction held — phases 29 and 30 each changed one `availability`
answer in `aura-app` and changed nothing in `aura-jobs`.

What survives the closure is the machinery it was built for. Export is the first stage in the
product that declines on the *wedding* rather than on the release, and `SkipCause`,
`CompletedDegraded` and `degraded_stages` are exactly what it uses to say so.

---

## 7. What is still open across the product

This is the last phase, so the list is the product's rather than the next phase's.

| Condition | Phase | What closes it |
|---|---|---|
| Real camera files, a photographed ColorChecker, a three-OS CI run | 02 | Camera files. A Sev 2 trigger that reopens phase 02 whatever else is in flight |
| The embedding carries no wedding semantics | 05 (C10) | A trained backbone and consented weddings. Most later conditions close with it |
| Face detection and recognition are placeholders | 06 (C1) | Consented face data and a GPU backend |
| Scene classification is a placeholder | 07 (C1) | The same |
| Nothing is calibrated | 13 (C2) | A calibration set. Until then nothing acts unattended |
| No `wgpu` backend | 14 (C1) | A GPU backend. Four of five render budgets waived |
| No measured camera profile | 14 (C2) | One photographed ColorChecker |
| Every quality study | 15-29 | Photographers |
| No network transport | 30 (C3) | A transport and a recorded session per provider |
| Nothing signed, notarised or rolled out | 30 (C5) | A certificate, an Apple account and an install base |

The shape of the product is complete and the evidence underneath it is not. Every gate in this
repository measures an algorithm against a fixture whose answer this repository chose. That is worth
exactly what it is worth: the arithmetic is right, the refusals refuse, the bounds bind and the
guarantees are enforced where they are enforceable. **None of it is a claim about a wedding.**

---

## 8. What was got wrong first

**A ratio measured as a difference of two large numbers measured nothing.** The verification
overhead was first measured by running the same export twice, with and without the read-back, and
subtracting. On a 60-frame job the two whole-run timings came out within a third of a per cent of
each other — and the *verified* run was faster. That reads as an overhead of zero, passes an 8 %
budget, and would have shipped as evidence that the guarantee is free. It is measured directly now:
a re-open, a re-read and a hash of each written file, which is exactly the work verification adds.
Phase 19's halo test and phase 22's ringing measurement are the same defect in two other phases, and
this is the third.

**A budget asserted in a debug build asserts the wrong thing.** The same ratio, once measured
correctly, is *flattered* by a debug build: the JPEG encoder is several times slower than it ships,
which inflates the denominator. The assertion is release-only now, following the convention phase 04
established, and the debug run prints the number with a note rather than passing on it.

**A fixture that moved every bucket at once measured a shape the contract refuses.** The learning
budget's first fixture proposed 45 changes; `LearningUpdate::validate` refuses anything over
`MAX_DIFF_LINES`, which is 24, because 24 is about what a photographer reads before agreeing to it.
The fixture now folds all 45 buckets and moves 24, which is the largest fit the loop can ever
legitimately be asked for. **A performance fixture must be a shape the product would actually
produce**, or the number it reports is about nothing.

**Ten of fifteen learnables were unattributable, silently.** `DecisionKind` has six members and
phase 13's reason registry carried a vocabulary for one of them, so a correction to an `Edit`
decision could never be recorded — `AURA-ML-5054` refuses a decision citing a code that is not in the
shipped registry. Every unit test passed, because each exercised the kind that worked. The registry
now covers `ToneCode`, `ColourCode`, `CurateCode` and `QcCode` as well, and `docs/reason-codes.md` is
regenerated at 227 codes across eight sections. This is phase 27's lesson from the other side: a
predicate that answers one question was never asked the second one.

**MAD is zero on a bucket of identical corrections, and zero is not "agreement".** The trim divides
by the median absolute deviation, and a bucket of sixty identical corrections plus four extreme ones
has a MAD of exactly zero — so the guard that says "perfect agreement keeps everything" protected the
four outliers as well. `aggregate::scale` falls back on the mean absolute deviation, and only a
wholly identical bucket returns zero.

**A fixture put its own outliers in a different bucket.** `fixtures::outlier` carried an identity,
which `attribute` sorts into the `subject_close` bucket, so the trim never saw the thing the fixture
existed to exercise. Phase 29's lesson about fixtures, in the phase that needed it least and found
it anyway.

**Twice more, a store test was handed ids rather than making them.** `delivery_upload` and
`delivery_backup` reference `export_job`, and the first integration test seeded photographs and no
job. Phase 25's gate failed the same way on an identity that did not exist and phase 26's on a photo
that did not. Three phases, one shape: nothing below the gate exercises a referential constraint,
because every test above it is handed the keys.

**A shell script's quoting hid four bugs in the check that counts the IPC surface.** An apostrophe
inside a single-quoted awk program closed the string; `//` comments were read as handler names; the
sign-off and gate sections were confused by a shared field name; and a regex for `invoke('...')`
missed nested generics, so `invoke<Array<[string, string]>>` was reported as dead surface. It counts
259 = 259 = 259 now, and the fourth bug is the one worth remembering: **a checker that under-reports
looks exactly like a codebase that is clean.**
