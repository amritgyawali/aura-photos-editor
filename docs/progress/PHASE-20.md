# Phase 20 progress - Portrait Retouch AI with Natural Texture Protection

One line per task group, in the order section 8 asks for them. Files touched, tests added,
benchmark delta.

## T1 - PM/CTO: the retouch ethics policy (section 8 step 1)

Files: `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md`, `docs/retouch.md`,
`crates/aura-retouch/config/retouch_presets.toml`. The policy is not a paragraph: it is a veto
that removes a candidate rather than scaling it, a kind of feature that no setting can
unprotect, a conservative default written as an asymmetric pair of floors, and a preset table
where every row carries a written reason. Section 11 of `docs/plan/CLAUDE.md` - no reshaping, no
lightening, no swapping - is enforced by there being nowhere to express any of them. Tests:
`crates/aura-retouch/tests/boundaries.rs` (3), `retouch_eval.rs` gate 12.

## T2 - DATA: blemish and permanent-feature labels across skin tones (section 8 step 2)

**Not done, and it cannot be done here.** Section 9 budgets twelve days for "blemish/permanent
labels on 15k faces across five skin-tone buckets, with consent". There is no consented face
data in this repository. What shipped instead is
`crates/aura-retouch/src/fixtures.rs` - synthetic faces whose spots, moles, freckles, tattoos,
dark circles and pore texture are painted in at known amplitudes - and the per-bucket reporting
machinery in `ml/models/retouch/train_blemish.py`, self-tested against a synthetic bias it can
catch. Condition C2.

## T3 - SRML/MLL: the detector and the temporary/permanent classifier (section 8 step 3)

Files: `ml/models/retouch/train_blemish.py`, `ml/models/retouch/train_permanent.py`,
`crates/aura-infer/src/onnx/fixtures.rs`, `xtask/src/models.rs`,
`docs/model-cards/{blemish_detector,permanent_features}.md`. Two heads registered, signed and
carded; both untrained and **neither consulted**. The loss that would train them prices a false
removal at fifteen times a miss and a tattoo confusion at forty times an ordinary one, and both
self-tests prove the asymmetry changes what is learned. Tests: 4 Python self-tests, all in CI.

## T4 - SRC: cross-frame permanence in face-normalised coordinates (section 8 step 4)

Files: `crates/aura-retouch/src/permanent.rs`. The projection onto the eye-to-eye axis, the
inverse that puts a protect row back on a frame, the single-frame classifier, and the
accumulation that needs **both** four frames and forty-five minutes. Tests: a mark survives a
tilted head, a face with no landmarks is refused rather than guessed, four frames across an hour
is permanent, a burst is not, two marks a centimetre apart stay two, a mark that looked temporary
never reaches the protect set. 6 unit tests.

## T5 - COL/SRG: patch synthesis with frequency-band blending (section 8 step 5)

Files: `crates/aura-render/src/bands.rs`, `crates/aura-render/src/retouch.rs`,
`crates/aura-render/shaders/{freq_bands,inpaint_patch,retouch_apply}.wgsl`,
`crates/aura-render/src/shaders.rs`, `crates/aura-render/tests/shader_parity.rs`,
`crates/aura-render/shaders/spatial.wgsl`. The three-band separation moved out of phase 19 and
into the renderer for its second consumer; the healing operator borrows a donor patch, matches
its tone to the ring around the mark and its texture energy to the same ring, and the phase 14
pass-through `stage_retouch` in `spatial.wgsl` retired. Tests: 6 band tests, 5 operator tests, 8
shader-parity tests. Benchmark: not previously measured; the whole decision now costs 57.6 ms
per frame in release, including at least one full render.

## T6 - COL: under-eye correction and mid-frequency evening (section 8 step 6)

Files: `crates/aura-retouch/src/undereye.rs`, `crates/aura-retouch/src/evening.rs`. Both measure
against the skin around them rather than against a target, both are capped by the contract, and
evening reconstructs from `low + mid * k + high` so it cannot reach a pore at any strength.
Tests: a dark circle lifts and is bounded, an even face needs nothing, a face with no landmarks
is skipped, a deep shadow reports that it was capped, strength scales without passing the cap;
a blotchy face is evened, an even one is not, a crop with no skin is not measured, evening is
deterministic. 9 unit tests.

## T7 - QAL/COL: the texture guard, with measurement and re-solve (section 8 step 7)

Files: `crates/aura-retouch/src/texture_guard.rs`. The guarantee as a post-condition: apply the
plan through the real renderer, divide the high-band skin energies, re-solve at three quarters
strength up to three times, and withdraw the whole plan rather than ship one that failed its
floor. Tests: an ordinary heal passes and says what it measured, an evening-only plan costs
nothing, an impossible floor withdraws everything, an empty plan reports an untouched ratio, a
re-solve scales both halves of an under-eye correction. 5 unit tests.

## T8 - SRC: per-identity strength and gallery consistency (section 8 step 8)

Files: `crates/aura-retouch/src/strength.rs`, `crates/aura-retouch/src/store.rs`,
`crates/aura-catalog/migrations/0021_retouch.sql`. One number per person per project, from four
gallery statistics, multiplied rather than averaged so no term can rescue another. Tests: the
bride in portraits beats a guest on the dance floor, `off` gives everybody zero, a face below the
floor is never retouched, a person who is always small keeps half their strength, no single term
rescues another, the spread of a stored strength is zero. 6 unit tests plus `retouch_eval.rs`
gate 3.

## T9 - SFE/MFE: the retouch panel (section 8 step 9)

Files: `ui/src/components/develop/RetouchPanel.tsx`, `ui/src/ipc/types.ts`,
`crates/aura-app/src/retouch_commands.rs`, `crates/aura-app/src/contract/ipc.rs`,
`crates/aura-app/src/state.rs`, `ui/src-tauri/src/main.rs`,
`docs/adr/ADR-0044-retouch-ipc-surface.md`. Eight commands, and a panel that shows what was left
alone as prominently as what was done. Tests: 10 panel tests, 3 command tests.

## T10 - QAIQ: the blind expert comparison (section 8 step 10)

**Not done.** Section 9 budgets five days for retouchers to judge AURA against Retouch4me, Evoto
and Aperty. There are no retouchers and no competitor outputs in this repository, and - more to
the point - the two heads are untrained and phase 06's detector finds no faces, so what would be
judged is not what the product will ship. Condition C4, and
`retouch_eval::the_gates_this_build_cannot_measure_are_named` keeps it visible beside the gates
that do run.

## T11 - QAL: the gates and the assembly proof

Files: `tests/eval/retouch_eval.rs`, `crates/aura-cli/src/phase20.rs`,
`crates/aura-perf/tests/retouch_budgets.rs`, `perf/budgets.toml`, `justfile`,
`.github/workflows/ci.yml`. 13 evaluation gates, the mechanical gate, and the two budget rows
that are not waived. Benchmark: `retouch_plan_frame` 57.6 ms per image,
`retouch_store_per_1000_images` 659 B per image against a 1,000 B budget.

## T12 - DOC/SEC: the documentation and the sign-off

Files: `docs/retouch.md`, `docs/runbooks/AURA-ML-509{0,1,2,3,4,5}.md`,
`crates/aura-core/errors.toml`, `CHANGELOG.md`, `CLAUDE.md`. Six error codes, six runbooks, and
the product's own page including every one of the twenty-six reason sentences - which two gates
assert. SEC's task is discharged by the dependency list: `aura-retouch` does not depend on
`aura-cloud`, and `boundaries.rs` fails the build if it ever does.
