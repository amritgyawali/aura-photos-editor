# Twenty-five additional selected colour methods and Evoto runtime evidence

Date: 2026-10-21, Asia/Katmandu. This extends the preceding fifty-capability passes; panel names, existing sliders and presets are not counted as additional algorithms.

## Delivered methods

The `color_lab_v1` extension stores an ordered stack of original colour/tone operations. Each operation requires an existing authored mask; disabled masks, zero mask opacity and zero operation strength remain neutral. Source points are explicit full-frame coordinates sampled before that operation. Target colour is linear Rec.2020, and the UI states that domain.

The extension is absent by default in old recipes. It validates finite values, reference domains, operation count and identities before persistence and rendering. Saves use project membership and a current recipe hash, with atomic history. Kernels and measured statistics use selected pixels; explicitly chosen source references may deliberately lie outside the destination mask.

| # | Additional method | Distinct native mechanism |
| ---: | --- | --- |
| 1 | Two-neutral spatial bridge | Interpolates two measured neutral chromatic adaptations along their spatial axis, retaining luminance |
| 2 | Two-neutral tonal bridge | Interpolates the two neutral adaptations by photographed luminance instead of position |
| 3 | Sampled colour-vector rejection | Orthogonally removes a measured chroma vector in normalized opponent coordinates |
| 4 | Measured palette-axis projection | Projects onto the line through two measured palette colours rather than shifting every colour equally |
| 5 | Covariance decorrelation stretch | Measures chroma covariance, resolves principal axes and applies bounded axis whitening toward a chosen spread |
| 6 | Dominant chroma-axis suppression | Suppresses the measured principal variation while retaining the mean and orthogonal variation |
| 7 | Soft gamut-ray compression | Applies a smooth tanh shoulder to colour excursions from the neutral axis near nominal white |
| 8 | Brightness-weighted chroma compression | Compresses colour as a function of both measured brightness and chroma radius |
| 9 | Radial chroma cusp compression | Shortens colours beyond a chosen radius according to their RGB ray geometry |
| 10 | Near-neutral chroma exclusion | Suppresses weak near-neutral colour with a radial gate while retaining stronger colour |
| 11 | Selected luminance equalization | Maps the selected weighted cumulative luminance distribution to its measured output range |
| 12 | Clipped-histogram equalization | Caps strong histogram populations and redistributes excess before deriving the cumulative map |
| 13 | Log-luminance equalization | Equalizes in stops and returns through the inverse mapping to linear light |
| 14 | Rank logistic normalization | Uses a symmetric logistic quantile to normalize measured ranks; the legacy enum spelling `rank_gaussian` does not imply a Gaussian algorithm |
| 15 | Bimodal tone separation | Finds an Otsu split and maps the two cumulative populations separately with a gap between their target populations |
| 16 | Percentile landmark remapping | Remaps measured 10th/50th/90th luminance landmarks through continuous piecewise linear intervals |
| 17 | Log-average photographic mapping | Normalizes by selected geometric-mean luminance, then applies a rational shoulder |
| 18 | Asinh dynamic-range mapping | Applies an inverse-hyperbolic-sine light map normalized at unit white |
| 19 | Logarithmic toe and shoulder | Combines a normalized log shoulder and a continuous rational toe |
| 20 | Local log-contrast equalization | Changes bounded local stop residuals around selected-only geometric illumination |
| 21 | Selected Retinex ratio | Divides luminance by local geometric illumination with a stabilizing anchor |
| 22 | Edge-protected colour enrichment | Suppresses local chroma enrichment where local luminance departures indicate stronger structure |
| 23 | Chroma outlier Winsorization | Clips local chroma outliers against measured neighborhood variance bounds |
| 24 | Local opponent-colour contrast | Increases chroma deviations from the selected-only local chroma mean |
| 25 | Sample-bound chroma replacement | Softly shifts colours near an explicit sampled source toward a chosen working-space target while retaining luminance |

These methods overlap existing broad colour/tone categories while adding distinct mathematical transforms. Existing Levels, selective CMYK, local curves/HSL/grading, conventional exposure/WB, reference fitting, material repairs and their controls remain separate. Several selected histogram maps are intentionally alternative distribution models, not promises of uniformly better photographic results.

Recipe/render/app files are `crates/aura-recipe/src/color_lab.rs`, `crates/aura-render/src/color_lab.rs` and `crates/aura-app/src/color_lab_commands.rs`. The interface is `ui/src/components/develop/ColorLabPanel.tsx` with `ui/src/ipc/colorLab.ts`. Shared registrations, render placement, cache invalidation, checkpoint exclusion and streamed-render fallback are integrated by the parent task.

## Evoto evidence: declared categories and original extensions

None of these twenty-five mathematical methods is claimed to be Evoto's recovered algorithm. Evoto colour matching, neutral handling and complexion categories are documented by previous installed-app work; an algorithm with a similar output purpose is not evidence of the same implementation.

The preceding REA MCP worker and GUI PE-resource records are reused only because the current worker scan matches the same installed worker SHA-256:

- Worker: `add7588f94aed190b97076f949709ba9622aa14f046b661783135d838d3d3bb2`, 230489488 bytes. REA evidence `ev_c06192a95f52887d5f461d2a0abdfb88dbb565de353627d7d87800856a1f6c30`.
- GUI: `7f020192f3b659ca4a1e4f8631b21879982bb21d97d26196a604ad70e2f3b2fa`, 237784976 bytes. REA evidence `ev_0aaab3b16b3d6b277303b9302dc151f4166d4f85cc5b158dbb0da34aa346c25b`.

A fresh, focused colour question examined whether the shipped matching path exposes output-validity requirements beyond the prior broad feature inventory. The bounded scanner retained 421 colour/complexion-related ASCII strings, 306 with candidate references, and 226 candidate unwind ranges. A single 3341-byte range at `0x141026430` was decoded with the existing GNU objdump. All nine selected LEA associations were confirmed against decoded absolute targets, including diagnostics for missing response weights, weight bounds, biases and bias bounds.

This supports a response-parsing wrapper with explicit data-validity checks. It does not establish a neural architecture, executable model weights, training method, service protocol, numerical transform, or runtime execution of that wrapper. The whole-program graph remains incomplete. No arrays, proprietary shaders, networks or trained assets were copied into AURA.

Full static evidence is retained locally under `.work-checks/evoto-next/color-100/`. REA's native Windows provider remained unavailable for deep analysis; its `observe_native_ui` tool explicitly binds native observation to an active Mach-O target. Windows runtime evidence below therefore uses separate native UI Automation and guarded physical interaction, and is not mislabeled as a REA native runtime capture.

## Actual Evoto editing of the requested CR3

The user explicitly authorized using installed Evoto to edit and test the real photos. Through Evoto's normal Create New / Import Image workflow, `H:/AURA EDIT/989A2698.CR3` was imported into a **new project**. An older same-name project pointed to a different folder and was not treated as proof that the requested source was loaded.

The selected source is 35170943 bytes, SHA-256 `562839abbb3ba6ef518cf49ea6c08c9bb7cc848d00d6eb74f630c9f4583e532f`. Its after-observation hash matches the parent's pre-run input manifest in `output/aura100-photo-baseline/results.json`. The original bytes are unchanged.

Observed editing results:

1. The newly imported file displayed as-shot WB **5929** and tint **12**. The seven visible Tone controls initially showed zero.
2. Clicking the normal Tone automatic wand changed **Exposure from 0.00 to -0.60**. Contrast, Brightness, Highlights, Shadows, White and Black remained visibly zero. The histogram and photograph visibly changed.
3. Facial **Textured Smoothing 35** and **Even (Dodge & Burn) 25** were committed through numeric inputs and Enter, then blurred/allowed to settle. The committed fields and a localized face-region preview change were observed.
4. Initial accessible RangeValue mutations only moved handles while numeric labels remained zero. Those attempts are excluded from successful processing evidence. The later numeric input and settled preview are the successful observation.

The preview reports **3000px Preview — Watermark displayed in preview only**. Captures preserve the actual application window. The same 520×779 displayed photograph rectangle changed across the automatic tone edit; after the two skin edits, the changed rectangle was localized to photo-relative bounds (205,193)–(403,374), around the face. The tiny average delta over the entire displayed portrait is evidence of actual changed preview pixels, not a skin-quality or identity score.

[Before automatic tone](../../.work-checks/evoto-next/runtime-color/basic-before.png), [automatic exposure -0.60](../../.work-checks/evoto-next/runtime-color/auto-tone-settled.png), and [settled facial edits](../../.work-checks/evoto-next/runtime-color/portrait-settled-final.png) are retained locally. The complete action, screenshot digest and viewport-delta ledger is `.work-checks/evoto-next/runtime-color/evidence-ledger.json`.

### Export blocker and comparison limit

Clicking Export opened **Try it free for 7 days**, offering 50 credits and requiring **Start 7-Day Free Trial** or **Continue to pricing**. The modal was dismissed. No trial was activated, no purchase or credit-spending action was taken, and account settings were not changed. The [actual export blocker](../../.work-checks/evoto-next/runtime-color/export-options-settled.png) is retained. **There is no Evoto exported output from this run.**

The parent's [AURA baseline result](../../output/aura100-photo-baseline/989A2698.CR3-after.jpg) was visually inspected beside the actual Evoto preview. AURA's baseline appears darker/cooler; Evoto's observed result has warmer/brighter skin and foliage. This is a qualitative observation across unequal output scaling, watermarking and rendering pipelines. It establishes a visible gap and does not establish pixel equivalence, an exact parameter translation, or a numeric fidelity score.

For a legitimate subsequent approximate matching attempt, a precisely bounded **520×779 screenshot-derived reference** was retained as `.work-checks/evoto-next/runtime-color/Evoto-989A2698-watermarked-preview.png`. Its rectangle is (921,84)–(1441,863) within the settled full-window capture. Every displayed watermark is preserved; no proprietary cache or hidden export payload was read. `.work-checks/evoto-next/runtime-color/bounded-reference.json` binds source/output hashes, controls, dimensions and the unknown display colour-transform limitation. The parent can apply AURA's existing reference-colour workflow to this observational reference and deliver a clearly labelled screenshot-guided variant separately from the baseline. Such a variant approximates the observed display, not an unavailable native Evoto export.

## Verification and remaining acceptance

Completed lane checks:

- Recipe: two tests passed for neutral old recipes, exact round-trip, invalid numeric/reference domains, duplicate identities and invalid-write atomicity.
- Renderer: six focused tests passed, covering nonzero effect from every one of the twenty-five methods; deterministic repeats and exact zero; selected-pixel/statistical isolation; source-reference sensitivity; monotone tone maps; colour-method luminance preservation; fractional masks; dark/HDR/single-pixel behavior; and missing/disabled/zero masks.
- Frontend: three tests passed for all twenty-five authored methods, explicit references/target persistence, stale-save errors, same-revision draft retention, mask requirements and navigation protection.
- The renderer subsequently received a sparse-selection sampling improvement, neutral black-chroma handling, and an Otsu helper extraction. The parent final verification must rerun the focused/native checks on that final source.
- A new native integration test `crates/aura-app/tests/color_lab.rs` covers saved native pixel changes, revision refusal, undo/redo, missing-mask refusal, original-byte preservation, clearing and reopening. It is part of the parent's final native test run.
- TypeScript checking reached an unrelated pre-existing-in-this-task `WorkflowLabPanel.test.tsx:45` optional-value error; no colour-lab TypeScript errors were reported in that pass. Parent verification owns the complete final check.

Statistical sampling is deterministic and capped at 65536 selected samples. Local methods still allocate full-frame working planes and fall back from streamed rendering to whole-frame rendering, like the existing selected material pipeline. Large native-resolution memory/cancellation and real-photograph qualification remain necessary. Distribution remapping is a creative operation and can emphasize noise or flatten desired lighting; explicit selection, conservative strength and human review are part of the workflow.

The actual Evoto preview observation strengthens evidence about this one real file and these three edits. It does not recover the proprietary trained logic, certify the twenty-five independent extensions as photographic improvements, or satisfy exact Evoto output equivalence.

