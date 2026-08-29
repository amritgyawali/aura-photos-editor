# Phase 21 progress - Micro-Retouch Suite: hair, teeth, eyes, clothing and glare

One line per task group, in the order section 8 asks for them. Files touched, tests added,
benchmark delta.

## T1 - PM/CTO: the ethics policy, published before implementation (section 8 step 1)

Files: `docs/retouch-ethics.md`, `docs/adr/ADR-0045-micro-retouch-and-cross-frame-borrowing.md`,
`crates/aura-retouch/config/micro_retouch.toml`. The policy is not a paragraph: it is a list of
operations this product will never build, a ceiling on every operation it does build that a
config file can lower and never raise, and one rule that decides what a borrow may replace - a
region that carries no information. Section 11 of `docs/plan/CLAUDE.md` - no reshaping, no
lightening, no swapping - is enforced by there being nowhere in the contract, the schema or the
wire to express any of them. Tests: `crates/aura-core/tests/micro_contract.rs` (23),
`micro_eval.rs` gate 9.

## T2 - DATA/COL: labels for flyaways, glare and lint; the teeth and sclera loci (section 8 step 2)

**Not done, and it cannot be done here.** Section 9 budgets seven days for "labels for flyaways,
glare, lint; hair-type diversity coverage" and four for "measure natural teeth/sclera loci". There
is no consented wedding photography in this repository. What shipped instead is
`crates/aura-retouch/src/micro/fixtures.rs` - synthetic frames whose strands, sheets, marks, teeth
and catchlights are painted in at known amplitudes - and a **relative** locus: ADR-0045 section 3
records why the teeth locus is centred on the frame's own neutral rather than on a measured
absolute, which is the difference between a colour target and a distance from one. Condition C3.

## T3 - SRML/MLL: the flyaway, glare and lint detectors (section 8 step 3)

Files: `ml/models/micro/{train_flyaway,train_glare,train_lint,export}.py`,
`crates/aura-infer/src/onnx/fixtures.rs`, `xtask/src/models.rs`,
`docs/model-cards/{flyaway_detector,glare_detector,lint_detector}.md`. Three heads registered,
signed and carded; all three untrained and **none consulted**. The three training procedures each
carry a property that is about safety rather than accuracy and each self-tests that it can fail:
a model firing inside the hair mass cannot pass at any accuracy, a catchlight is never a sheet,
and the lint head cannot name a strap or a crease because those two classes are not in it. Tests:
12 Python self-test properties plus `export.py --verify`, all in `just micro-eval`.

## T4 - SRC: hair reduction with area caps and background gating (section 8 step 4)

Files: `crates/aura-retouch/src/micro/hair.rs`. Thin high-contrast structures in the halo outside
the hair alpha, scored against the detail of the background immediately behind them, capped per
candidate and again per frame. Reduce rather than remove: `MAX_FLYAWAY_STRENGTH` is 0.60, so the
strongest permitted edit leaves two fifths of the strand's contrast. Tests: 5 unit tests,
`micro_eval.rs` gates 1 to 3.

## T5 - SRC/COL: teeth and eyes with loci and ceilings (section 8 step 5)

Files: `crates/aura-retouch/src/micro/{teeth,eyes}.rs`, `crates/aura-render/src/micro.rs`. The
teeth half evens the row toward its own upper quartile and removes a share of its own excess
chromaticity, clamped so teeth never outshine the skin around them. The eye half takes redness
out of the sclera as chroma only and raises iris local contrast, with specular pixels excluded
**by construction** rather than by a threshold applied afterwards. Tests: 13 unit tests,
`micro_eval.rs` gates 4 and 5.

## T6 - SRC: clothing cleanup, reusing phase 20's inpainting (section 8 step 6)

Files: `crates/aura-retouch/src/micro/clothing.rs`, `crates/aura-render/src/micro.rs`. Small
high-frequency anomalies inside the clothing region whose colour departs from the fabric around
them, classified by shape into lint, thread and stain, and vetoed entirely on patterned fabric.
The donor search and the patch synthesis are phase 20's, one region up. A strap and a crease are
opt-in per studio and off by default, in the contract, in the schema default and in a trigger.
Tests: 6 unit tests, `micro_eval.rs` gate 6.

## T7 - SRG/SRC: glare reduction and cross-frame borrowing (section 8 step 7)

Files: `crates/aura-retouch/src/micro/{glare,borrow}.rs`,
`crates/aura-render/shaders/micro_borrow.wgsl`, `crates/aura-catalog/migrations/0022_micro_retouch.sql`.
A specular sheet is a connected region of near-clipped, near-neutral pixels overlapping an iris.
Where the record is destroyed - `MIN_SPECULAR_FRACTION` of it at or above the clipped floor - a
sibling frame from the same moment may repair it, if the alignment search clears `MIN_ALIGNMENT`
and the region is below `MAX_BORROW_AREA`. Otherwise the highlight is reduced conservatively from
this frame. **Every borrow is disclosed in five places** and the database refuses an undisclosed
one. Tests: 11 unit tests, `micro_eval.rs` gates 7 and 8, and the phase gate end to end.

## T8 - SRC: the naturalness guard and the opt-in matrix (section 8 step 8)

Files: `crates/aura-retouch/src/micro/{guard,matrix}.rs`,
`crates/aura-retouch/config/micro_retouch.toml`. The guard applies the plan through the **real
renderer** and measures three things on the result - the peak iris luminance, the hair region's
edge energy, and how much further from its locus the teeth moved - then re-solves the offending
family at three quarters strength up to three times and withdraws that family if it still misses.
Per family rather than per plan, because the three measurements are over disjoint regions: a frame
whose teeth could not be evened safely still gets its lint removed. Tests: 15 unit tests across
the two modules, `micro_eval.rs` gate 9, phase gate sections 3 and 4.

## T9 - SFE: the micro-retouch panel (section 8 step 9)

Files: `ui/src/components/develop/MicroRetouchPanel.tsx`, `ui/src/ipc/types.ts`,
`crates/aura-app/src/{micro_commands.rs,contract/ipc.rs,state.rs}`,
`docs/adr/ADR-0046-micro-ipc-surface.md`. Nine commands, five operator switches, five clothing
switches and one for borrowing - and **no strength field anywhere on the wire**, which is what
keeps `docs/retouch-ethics.md` a promise about the product rather than a description of the
defaults. A borrowed region is drawn with a visible marker and the project header carries the
count. Tests: 12 vitest cases.

## T10 - QAL/QAIQ: the ceiling-refusal tests and the naturalness audit (section 8 step 10)

Files: `tests/eval/micro_eval.rs`, `crates/aura-cli/src/phase21.rs`,
`crates/aura-perf/tests/micro_budgets.rs`, `ml/models/micro/eval_micro.py`, `justfile`. Ten gates
under `cargo test`, a mechanical assembly gate under `just phase-21-verify`, and three budget
rows. **The naturalness audit did not happen**: it is four hundred frames judged by retouchers and
there are neither. What shipped is the arithmetic that would score one, self-tested against
synthetic judgements, including the per-bucket check that stops a mean hiding one group.
Condition C2.

## Corrections made while building

**The agreement statistic in `eval_micro.py` could not be satisfied by a correct panel.** It
required the judges to agree an absolute 0.10 above what chance predicts, and at a 97 % natural
rate chance agreement is already 0.92 - eight points of headroom for a ten-point margin, so a
perfect panel failed. It is now a share of the available headroom, which is Scott's pi, and both
properties survive: coin flips score zero and a real panel scores 0.21. Phase 19's halo test had
the same shape of defect and the same answer - a threshold a correct implementation cannot meet
is a bug in the threshold.

**The per-image storage figure was documented at 612 B and measures 1,633 B.** The constant was
written before it was measured over a thousand rows. This is the first phase since 09 whose figure
is above a kilobyte per image, and the reason is structural: every phase from 09 to 20 stores one
fixed-width verdict per photograph and this stores a *list* of operations, each with its own
rectangle and magnitudes. `BYTES_PER_IMAGE`, `perf/budgets.toml` and the store's own documentation
now carry the measurement and the argument for it.

**The micro modules had never been through `clippy -D warnings`.** Thirty-odd findings, and the
useful half were the same shape: pixel-neighbourhood arithmetic written by casting `usize` to
`i32`, testing for negatives, and casting back. Every one is now unsigned - the window is clamped
to the frame rather than allowed to go negative and then rejected - which is shorter, has no
platform-width caveat, and puts the frame edge in one place per loop. The contract also lost a
narrowing cast: `NATURALNESS_MAX_RESOLVES * OpFamily::COUNT as u8` is now a constant of its own
with an assertion that keeps it in step (ADR-0045 section 11.4).

**`ui/src-tauri/src/main.rs` had no `fn main`.** It was lost in the phase 19 to 20 merge and the
crate is outside the workspace, so nothing had compiled it since. Restored here rather than left
for phase 22.
