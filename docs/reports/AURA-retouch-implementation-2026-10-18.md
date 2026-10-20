# AURA portrait implementation following the Evoto reports

18 October 2026. This implementation follows the independent requirements in `AURA-Evoto-audit-2026-10-18.md` and `AURA-Evoto-deep-analysis-2026-10-18.md`. It does not copy recovered pseudocode or private model resources. Evoto-equivalent output has not been established.

## Changes in this implementation

### Protected cells survive enlargement

The old contour matte path bounded refinement by an already bilinearly enlarged matte. That bound was insufficient: bilinear interpolation itself gave a zero-valued protected cell partial coverage. At even enlargement ratios, no output pixel necessarily coincides with the source cell centre, so the entire hole could have nonzero coverage.

For mattes with `refine_contour`, the renderer now preserves the footprint of every authored zero cell, using the same pixel-centre coordinate convention as resampling. Colour refinement remains bounded by that support. This applies to the existing stored protection masks for features and contours; it does not supply new semantic labels. Unprotected/refine-edges-only mattes retain their previous resampling behaviour.

This is intentionally conservative. A coarse incorrect zero cell can still exclude real skin. The fix prevents growing corrections into a protected cell; it cannot recover missing skin or make the original segmentation anatomically perfect.

### Shared inward correction envelope

Skin smoothing, skin colour uniformity, portrait dodge/burn and under-eye correction now share an inward-only distance envelope in `aura-render/src/retouch_boundary.rs`. It handles both exterior boundaries and enclosed protection holes. It uses two eight-neighbour chamfer passes, with a half-pixel boundary offset and a smoothstep ramp. Cost is linear in the bounded selection area, independent of feather width. Width follows the operation radius and is bounded to 1–16 rendered pixels.

The envelope is the minimum of the authored weight and the distance ramp, rather than their product. Existing softer feathers therefore remain soft without unnecessary repeated erosion. Zero coverage remains zero; narrow skin strips retain reduced coverage. Image-frame edges alone do not create artificial exclusions. Invalid support becomes zero.

Both selection overlays and rendering call the same boundary method. This does not claim that an overlay predicts the exact final pixel delta: skin-colour eligibility, low-band measurements and tool-specific correction decisions still apply later. It establishes one shared support boundary.

The selection memoization key now includes tool and radius, preventing a previous operation from supplying the wrong boundary width to a later one. Persistent render identities include the new boundary version and the updated matte version, invalidating cached output from the old renderer.

### Under-eye behaviour

The existing bounded exposure/partial-chroma method is retained. It preserves socket gradients and limits lift relative to the photograph's own cheek. The new inward envelope also applies to this method, avoiding an abrupt correction at a hard selection or protected hole. The change does not add a trained dark-circle model, eye-bag reconstruction, or recovered Evoto lower-lid logic.

### Batch preflight and explicit withholding

The one-click workflow now checks successful portrait edits before including them in automatic delivery. `portrait_preflight.rs` renders a before/after pair with identical global colour and manual work. Only enabled automatic portrait operations are disabled for the comparison baseline. Automatic scene corrections remain present. Both renders omit geometry/effects/sharpening using the existing analysis-render helper so that source-coordinate face regions align.

The report records the exact recipe hash, renderer identity, photo ID, catalog source hash at import, actual analysis dimensions, per-face contour readings, clipping readings, status and limitations. The import hash is catalog provenance, not a new read-back hash of the current original. Status is `measured`, `review`, `incomplete`, or `not_applicable`; there is no universal photographic-quality score.

These are **screen-resolution face/adjacent-neck seam and clipping checks**. The existing contour heuristic is a pilot warning mechanism. It does not certify full-resolution skin semantics, texture, body boundaries, clinical defect categories or aesthetic quality. Missing face geometry or insufficient face resolution remains incomplete. Unavailable/failed checks and review warnings withhold the photo from this automatic delivery path. Explicit manual export retains its existing behaviour.

`photo-edits.json` includes available preflight reports, including reports for withheld results. `photo-edit-failures.json` records failures and their `withheld_from_delivery` disposition. A failed edit can no longer silently export its older saved recipe. If every selected edit fails or is withheld, export does not begin. Successful photos can still be delivered with the existing `completed_with_issues` status for a mixed run.

The automatic exporter receives the accepted recipe hashes. It compares each hash with the actual owned recipe loaded for rendering, immediately before constructing the render request. A new edit or missing approval entry fails that render instead of inheriting the previous check. This closes the recipe-change race without overwriting the user's later edit. It does not freeze external source-file mutations or certify a different renderer build.

### Existing colour behaviour

The actual native RAW colour transforms, as-shot handling and measured white-balance consensus remain in use. The existing denied bounds-indexing lint in the spatial WB sample collection was replaced with checked access without changing the estimator. No untrained WB head was enabled or described as Evoto-equivalent. Mixed-light calibration and memory-bounded full-size paired-reference matching remain separate work.

## Validation

Synthetic checks cover protected-cell resampling at even, odd and fractional ratios, narrow strips, frame boundaries, invalid inputs, transposed masks, actual skin/eye rendering, cache reuse, missing measurements, baseline isolation, changed-recipe rejection and failure withholding. Native tests use isolated catalogs and preserve the original RAW files and previous deliveries.

| Check | Result |
|---|---|
| Application library tests | 250 passed, 0 failed, 2 intentionally ignored. |
| Renderer library tests | 222 passed, 0 failed, 2 intentionally ignored. |
| Sampled-skin integration tests | 11 passed, including final-render protection and selection-cache isolation. |
| One-click end-to-end test | Passed: successful delivery, original/manual-choice preservation, cancellation, and forced failed-look withholding despite an existing valid recipe. |
| Combined test command | Exit 0; 484 passed in total. `cargo test --locked -p aura-app -p aura-render --lib --test one_click --test sampled_skin -j 2`. |
| Full dependency Clippy | Blocked by 7 denied indexing lints in pre-existing modified RAW code: `codecs/crx.rs:1782` and `meta.rs:953,959,964,969,970,975`. This is not a successful lint run. |
| Python native-crop review helper | Syntax check passed. |

Logs are `.work-checks/evoto-implementation-tests.log` and `.work-checks/evoto-implementation-clippy.log`. Native-render and desktop results are recorded below.

The focused check, `cargo clippy --locked --no-deps -p aura-app -p aura-render --lib -j 2`, completed successfully after replacing new `json!` failure records with typed serialization. Existing warnings remain for the under-eye function length, jaw-guard clamp expression and bundled profile panic documentation. This focused success does not turn the failing dependency-wide check into a pass.

### Native RAW results

The ignored native integration test was explicitly run on all four flagged originals. It passed in 222.53 seconds in this run, while other build work also ran; this is not an isolated performance benchmark. Each image rendered at **4480 × 6720**. Source hashes, saved global colour and crop were preserved. Test work used isolated catalogs and review-only PPM output, without replacing any delivery.

| Photo | Detected faces | Saved operations | Pixels changed by at least 2 byte levels |
|---|---:|---:|---:|
| 989A2698 | 1 | 214 | 1,398,111 |
| 989A2699 | 1 | 168 | 1,902,748 |
| 989A2812 | 1 | 46 | 319,506 |
| 989A2843 | 1 | 121 | 944,398 |

The new `scripts/review-portrait-boundaries.py` generated 20 native-scale comparison crops covering both eyes, nose, chin/neck and forehead. Each includes a matching global-colour baseline, the previous v36 AURA render, and this renderer's result. The previous panel is **not an Evoto reference**. `boundary-crops.json` records each crop's coordinates and actual pixel changes. These are change measurements, not quality scores.

Visual inspection covered the 2698 face/forehead/chin, both 2699 eyes, the 2812 nose/chin, and the 2843 face/nose/chin. The inspected boundaries retain the photographed nose and chin shadows, and the 2699 socket retains its tonal gradient. The tika and brows remain visible. No obvious new skin rim or detached under-eye colour patch was seen in those displayed crops. Skin texture and shine remain uneven in some areas, and some smoothing remains strong; this is not approval of every pore, every image in the wedding, or equality with Evoto. The new output differs only slightly from the already corrected v36 render in many protected regions.

Review files: `.work-checks/evoto-implementation-native/Review.html` (whole portraits) and `BoundaryReview.html` (native crops). Original-preservation results are in `results.json`; full-resolution before/after pixels and saved recipes are alongside them.

The separate native contour review test also passed. Its pilot readings are:

| Photo | Sampled pairs | New seam pairs | Strong source-edge pairs | Weakened edge pairs | Review warning |
|---|---:|---:|---:|---:|---|
| 989A2698 | 240,900 | 5 | 31,661 | 3,440 | No |
| 989A2699 | 224,952 | 13 | 11,633 | 330 | No |
| 989A2812 | 286,296 | 22 | 36,941 | 570 | No |
| 989A2843 | 229,080 | 17 | 25,284 | 1,500 | No |

These are measurements of this renderer's output, not an Evoto comparison or proof that every feature is correctly selected. Some counts improve and others worsen relative to v36; the results do not establish universal superiority. Raw readings are in `native-contours.json`.

### Final application checks

After the typed failure-ledger correction, the full one-click end-to-end test was rerun and passed. The full-resolution contour integration test passed on the final native review artifacts. The desktop compiled successfully, including the latest failure-ledger code; a subsequent build confirmed it was current. The existing MinGW resource warning about multiple non-default manifests remains.

The idle older AURA process was replaced with `target/desktop-launch/debug/aura-desktop.exe`. The new process launched successfully. A read-only smoke check confirmed the AURA title, native IPC bridge and empty activity list, with zero image workers busy. Evidence is `desktop-smoke.json` in the native review directory, plus `.work-checks/evoto-implementation-desktop-build.log`, `evoto-implementation-desktop-current.log`, `evoto-implementation-final-batch-test.log` and `evoto-implementation-contour-test.log`. No production catalog edit or delivery command was invoked by the smoke check.

`git diff --check` passed. The working tree already contained extensive RAW/UI/planner changes before this implementation; they were preserved. The tested additions described here are the boundary/protected-cell changes, shared overlay/cache integration, automatic preflight, failed-edit withholding, export revision check and associated regression/QA tools. No new trained model or semantic colour-matching engine was added.

## Remaining requirements

The two Evoto reports still identify substantial unimplemented requirements: rights-verified dense facial parsing, calibrated per-person feature uncertainty, trained temporary-defect/permanent-mark classification, semantic makeup protection, separately validated pigment/lid/bag handling, representative mixed-light colour calibration, unified legacy/automatic analysis, full-resolution all-path appearance verification and a controlled Evoto reference benchmark. No exact-output claim is justified until those requirements and direct output comparisons have passed. This change is a concrete renderer/workflow improvement, not an exact Evoto replica.
