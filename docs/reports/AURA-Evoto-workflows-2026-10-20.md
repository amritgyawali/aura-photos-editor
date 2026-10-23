# Evoto workflows: research, implementation and qualification

Date: 2026-10-20. Implementation branch:
`feat/evoto-reference-and-adjustable-finishing`. This extends commit
`59fbe75a4011a68a5479b873ab34e5ddb4d0b3a4` and the
[2026-10-18 installed-app audit](AURA-Evoto-8-features-and-colour-matching-2026-10-18.md).

## Result and evidence scope

AURA now adds absolute master/category controls after automatic retouch, finer
reference colour strengths, reusable reference metadata and edited-preview
reference creation. These are native renderer/catalogue workflows with saved
history, not interface-only placeholders. Usage is documented in the
[workflow guide](../adjustable-finishing-and-reference-colour.md).

**Complete Evoto parity and superiority are not established.** Many Evoto families
already have partial AURA counterparts; learned generation, dense anatomy,
capture hardware and several workflow details still need implementation or
qualification. A list of controls is not evidence of photographic equivalence.

The installed `F:\evoto\Evoto.exe` reports **8.0.0.0**. Its SHA-256 was verified
again as `7f020192f3b659ca4a1e4f8631b21879982bb21d97d26196a604ad70e2f3b2fa`.
After correcting Windows foreground activation, the app was opened normally and
**21 panel groups** were freshly inspected and captured: library/culling, technical
colour, reference matching/modes, portrait blemishes/skin/geometry/expression,
teeth/eyes/makeup/hair/hands/body/feet, background, clothing, pets, crop and creative
effects, plus the AI Lab catalogue. The ledger records each control tree and screenshot separately. Some
Qt controls outside the scroll viewport still appear in accessibility; they are
flagged separately from controls within the window bounds.

Earlier wrong-foreground/black captures and an ineffective accessible toggle are
excluded from successful observation. The fresh masking panel remained at
**Detecting...**. Detailed masks and manual heal/patch/clone modes retain their
explicitly dated 2026-10-18 evidence. The AI Lab catalogue was opened after
returning to Full Image, inspected and cancelled without running a task. No retouch
slider change, trial activation, paid export or remote generation was applied
during the current inspection. These are observed controls, not qualification of
every operation, slider endpoint or photographic output.

Local diagnostic artifacts are under
`.work-checks/evoto-research-2026-10-20/`; the machine-readable ledger is
[AURA-Evoto-workflows-2026-10-20.json](AURA-Evoto-workflows-2026-10-20.json).
The older 143 marketing routes overlap, include use cases and separate products,
and cannot be used as a denominator for an implementation percentage.

## Research conclusions

Evoto's reference workflow includes imported/current-edit references, matching
strengths, reusable groups and batch application. AURA now expands the comparable
library and fine-control workflow. Independent regional fits are now implemented
as described in the [native continuation](AURA-Evoto-native-research-2026-10-20.md).
Evoto's proprietary Quick/Control modes remain unqualified. The completed image can guide output
appearance; it does not uniquely determine original editing parameters.
[Reference matching](https://support.evoto.ai/feature-introduction-ai-color-match/)

Evoto's masks include semantic regions and manual combinations. AURA has local
masks, brush refinement and selection tools, but dense apparel/anatomical parsing
still require further work. Regional reference strengths now have an independent
AURA implementation; photographic equivalence remains unqualified.
[Masking](https://support.evoto.ai/feature-introduction-masking/)

Expression generation and articulated body reshaping are separate reconstruction
and geometry problems. Local warping and smoothing cannot establish equivalent
identity, pose or occlusion handling. AURA's existing manual controls remain
partial counterparts.
[Expression](https://support.evoto.ai/facial-expression/),
[body geometry](https://support.evoto.ai/full-body-reshape/)

Background replacement requires edge, retained-object and shadow handling.
Clothing cleanup needs texture/print protection. Pet eye/fur operations need
appropriate localization. AURA's native material, optical, colour-range,
catchlight and lighting operations provide useful components, but are not proof
of matching the competitor's automatic decisions.
[Background controls](https://support.evoto.ai/backdrop-changer/),
[clothing](https://support.evoto.ai/clothing-adjustment/),
[pet body](https://support.evoto.ai/pet-body/),
[pet eyes](https://support.evoto.ai/pet-eyes/)

Evoto's documented workflow also covers crop placement, watched-folder delivery,
culling, shared assets, workflow templates and unified navigation. These should
be tested as complete jobs, including cancellation and recovery, rather than
counted from menu names. Camera compatibility needs actual connected hardware.
[Crop](https://support.evoto.ai/crop/),
[automation](https://support.evoto.ai/auto-import-and-export/),
[culling](https://support.evoto.ai/smart-culling/),
[navigation](https://support.evoto.ai/evoto-ai-assistant-and-unified-search/),
[desktop documentation index](https://support.evoto.ai/home/evoto-desktop/)

## Implementation

| Workflow | What changed | Persistence and protection |
| --- | --- | --- |
| Automatic finish | Overall plus ten possible retouch categories, 0-200%; original-strength restoration | Absolute per-operation baselines, one history step, current-recipe check, immutable mask snapshots, manual/disabled steps retained |
| Rerunning retouch | Recognizes group-adjusted and subsequently individually edited operation identities | Prevents duplicate automatic operations overriding those adjustments |
| Fine colour match | Twelve independent matching strengths in addition to the existing four groups | Old sessions default to 100%; same baseline and native highlight/skin safeguards |
| Reference library | Thumbnails, search, groups, favorites, rename, remove, saved default strengths | Catalogue-backed metadata; removal preserves source files, saved recipes and the existing adjustable matching session |
| Create reference | Captures the full-quality current saved render, including crop/retouch | Local thumbnail/descriptor; original remains untouched |
| Multi-file upload | Imports valid references even if another file fails | Per-run outcomes and navigation/cancellation checks |

The design is [ADR-0130](../adr/ADR-0130-adjustable-automatic-finishing-and-reference-workflows.md).
No new model weights, database migration or frozen-contract changes are required.
These changes reuse the existing renderer; no proprietary Evoto code/assets or
weights were incorporated.

## Remaining acceptance work by family

| Family | Existing AURA basis / change here | Remaining work before equivalence can be claimed |
| --- | --- | --- |
| Library/culling | Projects, metadata, ratings, grouping, cull views | Labelled burst/closed-eye/blur cohorts and decision agreement; workflow differences |
| Technical colour | Native develop pipeline, curves, HSL, colour management | Compare ranges, transforms, profiles, noise/detail and preview/export on real RAW cohorts |
| Reference/personal looks | Different-photo fitting, collection matching, expanded library/fine strengths | Regional edge/occlusion qualification, per-person correspondence, mode behavior, reference reorder, trained personal-style adaptation |
| Masks | Local masks and retouch selection/refinement | Dense anatomy/apparel/pet parsing; overlapping people, veils, hair and edge benchmarks |
| Blemishes/skin | Measured spot removal, texture/tone operations, new strength controls | Defect classification and permanent-mark protection across skin tones and light |
| Face/expression/body | Reversible manual reshape primitives | Landmark/pose-driven geometry, expression generation, teeth synthesis, identity tests |
| Eyes/teeth/makeup | Native local enhancement, colour, catchlights | Fine anatomical/occlusion selection, dedicated alignment and generated makeup quality |
| Hair/hands/feet | Local material, colour and refinement tools | Hair matting/reconstruction, anatomy-specific localization and vein cleanup validation |
| Background/sky | Existing masks, cleanup, compositing/effects infrastructure | Complete shadow/reflection/retained-object workflow and scene-consistent output qualification |
| Clothing | Fabric, moire/stain operations and strength category where generated | Automatic fine/coarse separation, seam/print protection and edge reshaping |
| Pets | Local enhancement and manual selections | Species/eye/fur models and automatic leash reconstruction |
| Crop/optics | Crop, transform and native optical tools | Documented subject-location/expand modes; physical depth/bokeh and edge quality |
| Manual tools | Native heal/patch/clone and saved operation controls | Compare every mode/range, high-resolution donor behavior and mixed-stack ordering |
| AI Lab | Existing repair/enhancement components | Dedicated validated generative restoration, expansion, people/removal and scene fusion |
| Capture/automation | Existing ingest/jobs and delivery infrastructure | Wired/wireless vendor camera sessions, watched-folder recovery and complete workflow templates |
| Settings/assets/export | Existing settings, presets and export | Side-by-side formats, metadata/ICC, preset portability, asset management, platform-specific details |

The ledger preserves the 26 observed historical panel/control groups and their
labels through a link to the prior JSON. It records implementation evidence and
remaining acceptance work separately. It does not silently mark old gaps done.

## Verification

The following checks pass on the final source:

- Production frontend build; **851 tests across 100 files**.
- Native application library: **299 passed**, 2 ignored.
- Reference/finishing integration: **4 passed**, 1 optional real-RAW test ignored.
- Strict `cargo clippy --workspace --all-targets -j2 -- -D warnings`.
- Workspace and desktop-shell formatting; banned-pattern scan; `git diff --check`.
- Frozen contracts: **83 entries, all locked**.

Native integration verifies actual pixel changes at zero, pixel-exact original
finishing restoration, Undo/Redo, stale-write refusal, manual-operation preservation,
original-file bytes, library metadata after reopening, edited-preview thumbnails and
separate-image reference fitting. Unit tests cover absolute 200% strengths, mask
snapshots, individual overrides, new operations and anatomical/scene categorization.
The optional RAW test, Lightroom-catalogue learning case and real-photo spot diagnostic remain explicitly ignored;
this run does not count them as passed. Preliminary invalid-fixture/linker failures
were corrected and are retained in diagnostic logs. The rebuilt desktop smoke is recorded below.

Required photographic qualification remains broader than these regression tests:
daylight/tungsten/mixed lighting; varied skin tones and age; makeup, jewelry,
ceremonial/permanent marks; groups, motion, glasses, profile views; hair/veils;
patterned clothing; studio/complex backgrounds; pets; and different camera RAWs.
Use fixed reference/target sets and review before/after at native resolution.
Measure colour/tonal agreement alongside texture retention, identity, mask spill,
preview/export consistency and original-file hashes. A distance metric alone
cannot certify a pleasing or identical photograph.

## Rebuilt desktop verification

The Windows desktop build with bundled production assets passes. The GNU linker
emits a resource-manifest merge warning; the resulting executable launches and
passes **all seven** smoke scenarios. The frontend build also reports the existing
large JavaScript chunk diagnostic. Neither diagnostic is represented as a clean
warning-free desktop build.

The test launches a dedicated catalogue and WebView profile, imports a generated
chart and seeds a saved automatic-operation fixture through native IPC. It does
not pretend that the chart was detected as a human portrait. Real GUI actions
then verify zero, restored 100% and increased 150% smoothing against actual native
pixels; preserved manual work; edited-preview reference capture; thumbnails,
name/group/favorite persistence; and independent temperature/tint defaults.
Native IPC verifies a separate-image reference fit with full-resolution guards
and removal without changing the saved photo. The source SHA-256 stays
`a473fb13b663cb251c2ec437d397ed47aeb5657d66a88118cabdeda27714e4c2`.

The generated fixture's internal appearance distance changes from **22.514046 to
19.107729**. This is a functional fit measurement, not an Evoto comparison, human
quality score or evidence of general skin-retouch accuracy. The complete desktop
result and screenshots are under
`.work-checks/evoto-research-2026-10-20/desktop-171926/`. The replay script is
`.work-checks/rea-evoto-2026-10-20/verify_finishing_desktop.py`.

No Evoto export was produced for a paired output comparison. Exact rendering,
all documented options, camera hardware and the remaining model-dependent tasks
remain unqualified as detailed above.

## Regional and native continuation

The [native research report](AURA-Evoto-native-research-2026-10-20.md) adds hash-bound
REA evidence, 24 targeted function traces, packaged-resource metadata, actual
regional matching and six fresh desktop checks. The seven finishing/library cases above were also rerun successfully against
the final rebuilt desktop. Together the two runs cover 13 scenarios. Full parity remains unestablished.
