# Phase 19 progress - Local Light Sculpting

One line per task group, in the order section 8 asks for them. Files touched, tests added,
benchmark delta.

## T1 - DATA/MLL: targets from expert difference maps (section 8 step 1)

**Not done, and it cannot be done here.** Section 9 budgets five days for a "difference-map
dataset from expert edits" and four for learning targets from it. There is no corpus of RAW
files paired with expert final edits in this repository. What shipped instead is
`ml/models/local/train_light_targets.py` - the extraction and the fit, written and self-tested
against a synthetic corpus whose answer is known by construction - and
`crates/aura-brain-photo/src/local/fixtures.rs`, seven frames whose faces, backgrounds and hot
spots were chosen first and then painted in. Condition C2.

## T2 - COL: the luminosity-masked face lift (section 8 step 2)

Files: `local/luminosity.rs`, `local/face_light.rs`, `local/measure.rs`. The split that stops
a lifted face glowing - shadows move, mid-tones move less, highlights barely move - plus the
dynamic noise cap from phase 09's measurement and phase 15's per-scene shadow scale. Tests: a
dark face lifts through its shadows, a face near the band lifts flat, a pull-down is all
exposure and never deepens, the highlight term can never brighten, the share has no corner at
the pivot, a small face gets a wider relative feather. 6 unit tests.

## T3 - COL/SRC: the paired subject and background move (section 8 step 3)

Files: `local/subject.rs`, `local/background.rs`. One decision in two halves, solved together
so neither can be applied without the other. Three measured triggers - a luminance ratio, a
chroma energy and phase 11's own bright-blob finding - and the pairing scales *both* halves
back until the frame's mean luminance is within three per cent of where it started. Tests: a
calm background triggers nothing, a bright window is brought down, the halves always carry the
same number, the mean stays within tolerance on a frame built to break it, a saturated but
correctly exposed background is desaturated rather than darkened, severity takes the worst
trigger rather than their sum. 11 unit tests.

## T4 - COL: frequency separation and the shaping model (section 8 step 4)

Files: `local/freqsep.rs`, `local/dodgeburn.rs`. Three bands rather than two, because a blotch
is neither form nor a pore; the finest band is **never produced**, which is what makes "shapes
form without touching texture" a property of the decomposition. Ten named zones placed from
the face box and the two eye landmarks, mirrored onto whichever side the light already left
darker. Tests: a gradient is all form, a blotch reaches the mid band, pore-scale detail
reaches neither, no zone exceeds a sixth of a stop, a dodge-only zone is never negative, the
burn follows the shadow side, a flatly lit face is shaped less, the grid is deterministic,
evening can never move the band past its tolerance. 15 unit tests.

## T5 - COL: shine detection and luminance-only reduction (section 8 step 5)

Files: `local/shine.rs`. Four conditions - high luma, low chroma, small area, near the face's
own bright end - and the second is the one that protects a well-lit dark forehead from being
read as sheen. Deterministic flood fill on a coarse grid. Tests: a clean face reports nothing,
a bright desaturated patch is found, a bright *saturated* patch is not, a large bright region
is the lighting, no skin mask means no work, the reduction is bounded and always negative. 8
unit tests.

## T6 - SRC: the strength governor and mask-quality scaling (section 8 step 6)

Files: `local/governor.rs`, `local/policy.rs`. One shared per-image allowance, allocated in
priority order; section 6.4's sentence read as ADR-0033 section 5 records. Mask confidence and
edge quality are two numbers rather than one because they fail differently. Tests: an empty
frame spends nothing, face lighting is paid first and shaping last, an operation that cannot
have it all is scaled rather than dropped, a tighter scene budget exhausts sooner, two
operations that cancel still both cost. 6 unit tests.

## T7 - SRC: group joint solving (section 8 step 7)

Files: `local/face_light.rs` (`solve`, `enforce_spread`). A common target agreed over three
weighted rounds, then a pass that may only ever move a face *down* toward the group and never
below where the photograph put it. **The guarantee was rewritten during this task** - see
ADR-0033 section 6. Tests: a group that can be evened ends inside the threshold, one that
cannot is still made more even, the rule never brightens a face it did not lift, an unmaskable
face does not decide the group. 4 unit tests.

## T8 - SRC/SFE: the recipe, the store, the IPC surface and the panel (section 8 step 8)

Files: `local/store.rs`, `local/api.rs`, `local/plan.rs`, `local/guard.rs`,
`crates/aura-catalog/migrations/0016_local_light.sql`,
`crates/aura-app/src/local_commands.rs`, `crates/aura-app/src/contract/ipc.rs`,
`ui/src/ipc/{types.ts,client.ts}`, `ui/src/components/develop/LocalPanel.tsx`,
`ui/src-tauri/src/main.rs`. Six commands, three tables, one view, five indexes. The panel makes
an invisible edit visible: a strength per operation, a gated operation shown as *unavailable*
rather than as off, and what stopped each lift beside how far it went. Tests: 7 store and
command tests, 8 panel tests.

## T9 - SRG: the shaders (section 9, SRG)

Files: `crates/aura-render/shaders/{luminosity_mask,freq_sep,local_apply}.wgsl`,
`crates/aura-render/src/local.rs`. The first shader *libraries* in the product - no entry
point, called by `stage_masks` - so `shader_parity.rs`'s frame-uniform assertion is narrowed to
files that are dispatched and a new test keeps a library from acquiring an entry point. Six
shared constants are held to the processor reference. Tests: 9 reference tests, 2 parity tests.

## T10 - QAL/QAIQ: the gates and the audits (section 8 step 9)

Files: `tests/eval/local_eval.rs`, `crates/aura-cli/src/phase19.rs`,
`crates/aura-perf/tests/local_budgets.rs`, `ml/models/local/eval_local.py`. 38 evaluation
gates, the mechanical gate, two performance budgets.

**The harness found a real halo on its first run**, and three of its own formulations were
wrong before that. ADR-0033 section 7 records all four. The defect: `apply_face_light`
evaluated its luminosity weights on the partially-edited pixel, so the highlight restraint grew
quadratically in the matte while the lift grew linearly, and a bright pixel received *more*
lift at the mask's edge than at its centre. Fixed on both paths.

The expert subtlety study of section 10.1 and the four-hundred-frame halo audit of section 9
do not exist and cannot. Condition C3.

## T11 - PERF: the budgets (section 11)

Files: `perf/budgets.toml`, `crates/aura-perf/tests/local_budgets.rs`.

Storage started at **2,236 B per image** and the profiler said why: the shaping was a child
table of one row per zone, and ten zones on each of four faces cost 1,286 B on its own. Every
zone is a pure function of the face region, the light direction and the strength, so the
catalog stores those four numbers and `zones_for` reproduces the list - **114 B**, and the
panel still shows every zone by name. Measured total is now **1,064 B**.

Section 11's 12 ms render row is waived twice over: no `wgpu` backend, and no phase 18 matte to
apply through.

## T12 - PM/DOC: the policy and the documentation (section 9, PM and DOC)

Files: `crates/aura-brain-photo/config/local_light.toml`, `docs/local-light.md`,
`docs/adr/ADR-0033-local-light-sculpting.md`, `docs/adr/ADR-0034-local-ipc-surface.md`, six
runbooks. 22 scene rows with a written reason each; the loader refuses a row with no reason and
a row that reverses the priority order. Two tests assert every reason code has a sentence in
the product document and that the withdrawal count matches what the document claims.
