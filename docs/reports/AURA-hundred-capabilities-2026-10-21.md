# AURA: 100 additional capabilities and photo qualification

Later correctness fixes and fresh execution of the rebuilt desktop are recorded in
the [current revalidation report](AURA-revalidation-2026-10-21.md). The evidence below
is preserved as the preceding qualification run; its results are not relabelled as
tests of the corrected executable.

Date: 2026-10-21, Asia/Katmandu. This is a separate set of 100 additions beyond the two preceding 50-item passes. It contains 75 image/selection methods and 25 collection workflows; panel names, sliders, and parameter presets are not counted as algorithms. All implementations are original AURA code.

## Delivered behavior

| Area | Additions | Interface and native implementation |
| --- | ---: | --- |
| Selected colour and tone | 25 | Colour lab; `color_lab_v1`, `save_color_lab` |
| Spatial repair | 25 | Spatial repair; `spatial_lab_v1`, `save_spatial_lab` |
| Selection laboratory | 25 | Masks panel; `mask_workflow` |
| Cohort checkpoints and review | 25 | Review & checkpoints, four tabs; `workflow_lab` |

Old recipes remain neutral without these extensions. Image methods require an explicit authored mask and Apply. Writes check collection membership and saved revisions, validate domains, and use native history. Render integration includes cache invalidation, checkpoint exclusion and streamed-render fallback. Review mutations use catalogue revision checks; proposed batch edits show changes/conflicts before application and retain per-photo Undo.

Implementation details: [colour methods and Evoto observations](AURA-Evoto-colour-runtime-2026-10-21.md), [spatial methods](../spatial-repair-lab.md), [selection laboratory](../mask-selection-laboratory.md), [review workflows](AURA-workflow-lab-2026-10-20.md), [ADR-0134](../adr/ADR-0134-hundred-additions-and-real-photo-qualification.md).

Desktop captures: [spatial panel](assets/aura100-spatial-2026-10-21.png), [colour panel](assets/aura100-colour-2026-10-21.png), [selection laboratory](assets/aura100-selection-2026-10-21.png), [review workspace](assets/aura100-workflow-2026-10-21.png). Captures use an isolated synthetic chart, independently of the supplied portrait qualification.

## Qualification results

| Check | Recorded outcome |
| --- | --- |
| Frontend suite | 907 tests in 111 files passed; `.work-checks/aura100/ui-tests.log` |
| Frontend production build | Passed TypeScript and Vite; existing large-chunk advisory retained |
| Native workspace run, unscaled host | 4416 passed, 1 failed, 53 ignored, 291 result groups; timing configuration discussed below |
| Performance rerun, configured host | 116 passed, 0 failed, 0 ignored with repository-required `AURA_PERF_HOST_SCALE=6`; see `performance-host-final.log` |
| Strict workspace Clippy | Passed; all targets, warnings denied |
| Frozen contract check | Passed; no contract lock changes |
| IPC surface | 347 native commands = 347 registrations = 347 typed client calls; duplicate-guard Python tests 3/3 passed |
| Windows desktop rebuild | Passed; custom-protocol production UI |
| Review application launch | Saved 27-photo catalogue opened: True; `.work-checks/aura100/review-launch.json` |
| Rebuilt desktop execution | 112 checks recorded; completed: True; page errors: 0 |
| Supplied RAW previews | 27/27 edited with saved recipes |
| Full-resolution JPEG export | 27/27 verified; completed: True |
| Source integrity | All original SHA-256 values unchanged: True |

The first complete native run exposed neutral optional settings missing from serialized checkpoint comparisons, an integration-test expectation exceeding the existing 100-entry history cap, a source guard flagging a read-only finishing-key exclusion, and an unconfigured timing check. The neutral-settings bug was fixed; history assertions now honor the real cap, and the source guard permits only that exact read-only key exclusion. Reference budget definitions were unchanged. The five workflow regression tests passed after the fix. Earlier failure logs are retained.

The final unscaled native run again measured the 25-stage planner at **8451 ms against the 5000 ms reference-host limit**. `CLAUDE.md` explicitly requires host scale 6 on this machine. The separate configured performance run uses that existing timing calibration; it does not change size, count, cost or product-policy limits. The **exact originally failing executable** was also rerun with this setting: all three tests passed (`autopilot-same-binary-host6.log`). The primary run's timing failure remains visible in the table and logs rather than being relabelled as an uninterrupted green run.

The native tests exercise every colour/spatial method, every selection action, selected-pixel boundaries, missing/disabled masks, finite/HDR inputs, deterministic/zero-strength behavior, stale-save refusal, exact Undo/Redo, and source preservation. Workflow tests exercise plans/conflicts, checklist drift, issue dependencies, portable metadata trust boundaries, membership and reopening. Desktop checks invoke the registered native routes and exercise the new panels, draft retention and persisted review queue.

Ignored existing tests are not counted as passes. They include opt-in real-photo diagnostics, external catalogue/reference inputs, and hardware/model-dependent scenarios. The supplied-folder desktop audit separately exercises real RAW editing and delivery; it does not establish that every optional workflow in the existing application was exercised. Initial desktop-harness failures used non-UUID job IDs and omitted selecting/opening the mask controls. The script was corrected, the real mask selection was opened, and the finished scenario saved through the native route; these were test-driver corrections rather than product fixes.

On the desktop fixture, 2 operation(s) produced unchanged export pixels: chroma_winsorize, mosquito_suppress. A valid operation can be neutral when the fixture lacks the defect or selection population it addresses; a successful invocation is not a photographic-quality score. Numerical kernel behavior is covered separately by renderer/selection tests.

## The requested photos

All 27 CR3s in `H:/AURA EDIT` were imported by reference into an isolated catalogue, enhanced through AURA's actual native automatic edit command and rendered before/after. The original RAWs were never used as output paths. Each saved recipe and input hash is retained.

Open the [before/after gallery](../../output/aura100-final/index.html). Full JPEGs are under `output/aura100-final/full-resolution/<original-name>/edited/`. [Baseline manifest](../../output/aura100-photo-baseline/results.json) and [final export manifest](../../output/aura100-final/results.json) bind filenames, recipe hashes, dimensions, timing, source hashes and the executable used. The desktop feature audit is `.work-checks/aura100/desktop/results.json`.

The automatic edits use the existing adaptive portrait/tone pipeline. The 75 creative methods are explicitly chosen tools, so contradictory blur, denoise, texture and distribution effects are not all stacked onto the delivery photos. These checks establish execution, reproducibility and unchanged originals. They do not establish that every edit is aesthetically ideal; final skin detail and composition still require visual review at 100%.

## Evoto: actual evidence and remaining gap

The requested REA skill/MCP workflow was used with installed-artifact evidence. Focused static inspection confirmed nine decoded references within a response-validation wrapper, including diagnostic strings about weights and biases. Those strings do not recover weights, a network architecture, training data or an image-processing implementation. No proprietary model, trained skin asset or shader was copied into AURA.

Evoto was also used through its normal interface on the exact supplied `989A2698.CR3`. Automatic tone changed exposure from 0.00 to -0.60; facial Textured Smoothing 35 and Even (Dodge & Burn) 25 were committed and produced a localized preview change. Initial handle-only changes were excluded from evidence. Clicking Export opened a trial-enrollment/pricing prompt; it was dismissed without enrolling or purchasing. No native Evoto export was obtained.

The retained Evoto preview is watermarked, resampled and subject to an unknown display transform. AURA's baseline looked cooler/darker than that observed Evoto preview. A separately labelled screenshot-guided colour experiment is recorded in the final photo manifest; it approximates this display reference and cannot establish native output parity. Source pixels and watermarks are not pasted into AURA exports.

The experiment completed: True; baseline restored: True. The native matcher reported descriptor distance 9.699684 to 8.959021, with full-resolution checking: True. This is a descriptor measurement, not an Evoto fidelity percentage. [Separate screenshot-guided preview](../../output/aura100-final/989A2698-screenshot-guided-approximation.jpg). Its full-resolution JPEG and recipe are retained beside the qualification outputs.

**Exact Evoto output, all Evoto features, proprietary algorithms and trained-model parity remain unverified and incomplete.** This delivery adds 100 functioning independent capabilities; it is not an exact reconstruction of Evoto from A to Z. See the [runtime evidence report](AURA-Evoto-colour-runtime-2026-10-21.md) for hashes, screenshots, controls and provider limits.

## Practical limits

Spatial methods operate on a bounded 256-pixel correction grid and interpolate corrections into the original; fine sensor defects can require existing precision brushes. Iterative reconstruction uses a bounded iteration count, not an asserted converged solution. Selection image analysis uses a bounded 512-pixel grid. Colour statistics sample at most 65536 selected pixels. Selected pipelines can require whole-frame buffers at export resolution; the delivered RAW export run does not benchmark every creative stack at native resolution. Review cohorts are limited to 32 photos, eight checkpoints, and bounded catalogue metadata. Planned edits apply per photo and report partial failures; they are not a cross-photo transaction.

## Explicit 100-item ledger

Runtime “pass” means the retained desktop scenario completed for that operation/workflow; it does not mean exact Evoto equivalence or universal image-quality improvement. The [machine-readable ledger](AURA-hundred-capabilities-2026-10-21.json) includes per-fixture pixel-change observations.

| # | Area | Added capability | ID | Runtime |
| ---: | --- | --- | --- | --- |
| 1 | colour | Two-neutral spatial bridge | `spatial_neutral_bridge` | pass |
| 2 | colour | Two-neutral tonal bridge | `tonal_neutral_bridge` | pass |
| 3 | colour | Sampled colour-vector rejection | `chroma_reject` | pass |
| 4 | colour | Measured palette-axis projection | `palette_axis` | pass |
| 5 | colour | Covariance decorrelation stretch | `decorrelation_stretch` | pass |
| 6 | colour | Dominant chroma-axis suppression | `principal_chroma_suppress` | pass |
| 7 | colour | Soft gamut-ray compression | `soft_gamut` | pass |
| 8 | colour | Brightness-weighted chroma compression | `luma_chroma_compress` | pass |
| 9 | colour | Radial chroma cusp compression | `chroma_cusp` | pass |
| 10 | colour | Near-neutral chroma exclusion | `neutral_exclusion` | pass |
| 11 | colour | Selected luminance equalization | `histogram_equalize` | pass |
| 12 | colour | Clipped-histogram equalization | `clipped_equalize` | pass |
| 13 | colour | Log-luminance equalization | `log_equalize` | pass |
| 14 | colour | Rank logistic normalization | `rank_gaussian` | pass |
| 15 | colour | Bimodal tone separation | `bimodal_separate` | pass |
| 16 | colour | Percentile landmark remapping | `percentile_remap` | pass |
| 17 | colour | Log-average photographic mapping | `photographic_map` | pass |
| 18 | colour | Asinh dynamic-range mapping | `asinh_map` | pass |
| 19 | colour | Logarithmic toe and shoulder | `log_toe_shoulder` | pass |
| 20 | colour | Local log-contrast equalization | `local_log_contrast` | pass |
| 21 | colour | Selected Retinex ratio | `retinex_ratio` | pass |
| 22 | colour | Edge-protected colour enrichment | `chroma_edge_protect` | pass |
| 23 | colour | Chroma outlier Winsorization | `chroma_winsorize` | pass |
| 24 | colour | Local opponent-colour contrast | `opponent_contrast` | pass |
| 25 | colour | Sample-bound chroma replacement | `sampled_replace` | pass |
| 26 | spatial | Edge-stopped diffusion | `anisotropic_diffusion` | pass |
| 27 | spatial | Total variation repair | `total_variation` | pass |
| 28 | spatial | Patch similarity denoising | `non_local_means` | pass |
| 29 | spatial | Vector medoid cleanup | `vector_medoid` | pass |
| 30 | spatial | Trimmed neighborhood repair | `trimmed_mean` | pass |
| 31 | spatial | Adaptive Wiener restoration | `adaptive_wiener` | pass |
| 32 | spatial | Haar residual thresholding | `haar_denoise` | pass |
| 33 | spatial | Haar detail gain | `haar_detail` | pass |
| 34 | spatial | Laplacian band contrast | `laplacian_detail` | pass |
| 35 | spatial | Variance-region smoothing | `kuwahara` | pass |
| 36 | spatial | Structure-guided strand smoothing | `tensor_flow` | pass |
| 37 | spatial | Block seam repair | `block_deblock` | pass |
| 38 | spatial | Disc aperture bokeh | `aperture_bokeh` | pass |
| 39 | spatial | Diffraction star filter | `starburst` | pass |
| 40 | spatial | Rotational motion integration | `rotation_blur` | pass |
| 41 | spatial | Drawn-path motion integration | `trajectory_blur` | pass |
| 42 | spatial | Harmonic selection fill | `harmonic_fill` | pass |
| 43 | spatial | Gradient-domain donor repair | `poisson_donor` | pass |
| 44 | spatial | Local channel registration | `channel_register` | pass |
| 45 | spatial | Bright/dark structure cleanup | `morphology_clean` | pass |
| 46 | spatial | Local rank contrast | `local_rank_contrast` | pass |
| 47 | spatial | Edge-adjacent noise repair | `mosquito_suppress` | pass |
| 48 | spatial | Directional edge interpolation | `edge_directed_smooth` | pass |
| 49 | spatial | Donor-guided low-frequency repair | `donor_bilateral` | pass |
| 50 | spatial | Measured texture patch quilting | `texture_quilt` | pass |
| 51 | selection | Polygon marquee | `polygon_marquee` | pass |
| 52 | selection | Rounded rectangle marquee | `rounded_marquee` | pass |
| 53 | selection | Angular sector marquee | `sector_marquee` | pass |
| 54 | selection | Sampled global colour | `sample_colour` | pass |
| 55 | selection | Contiguous colour wand | `colour_wand` | pass |
| 56 | selection | Sampled luminance band | `sample_luminance` | pass |
| 57 | selection | Saturation range | `saturation_range` | pass |
| 58 | selection | Texture range | `texture_range` | pass |
| 59 | selection | Image edge selection | `image_edges` | pass |
| 60 | selection | Image-guided mask smoothing | `guided_smooth` | pass |
| 61 | selection | Edge-constrained growth | `edge_grow` | pass |
| 62 | selection | Interior distance falloff | `interior_falloff` | pass |
| 63 | selection | Enclosed negative-space selection | `negative_space` | pass |
| 64 | selection | Selection centreline | `skeleton` | pass |
| 65 | selection | Split disconnected regions into masks | `split_regions` | pass |
| 66 | selection | Append whole component stack | `append_stack` | pass |
| 67 | selection | Replace destination selection | `replace_selection` | pass |
| 68 | selection | Synchronize local adjustments | `sync_adjustments` | pass |
| 69 | selection | Align selection centroids | `align_centroid` | pass |
| 70 | selection | Fit selection to bounds | `fit_bounds` | pass |
| 71 | selection | Four-corner perspective placement | `perspective_selection` | pass |
| 72 | selection | Automatic coverage threshold | `auto_threshold` | pass |
| 73 | selection | Sampled chroma-key exclusion | `chroma_key` | pass |
| 74 | selection | Seed partition of selection | `voronoi_partition` | pass |
| 75 | selection | Filter regions by shape and area | `filter_regions` | pass |
| 76 | workflow | Multi-photo recipe checkpoint | `review-1` | pass |
| 77 | workflow | Checkpoint drift comparison | `review-2` | pass |
| 78 | workflow | Section-selective checkpoint restoration | `review-3` | pass |
| 79 | workflow | Conflict-aware rollback preview | `review-4` | pass |
| 80 | workflow | Relative edit transfer | `review-5` | pass |
| 81 | workflow | Batch change manifest | `review-6` | pass |
| 82 | workflow | Baseline-aware three-way transfer | `review-7` | pass |
| 83 | workflow | Batch saved-revision verification | `review-8` | pass |
| 84 | workflow | Edit fingerprint cohorts | `review-9` | pass |
| 85 | workflow | Cross-photo settings-section matrix | `review-10` | pass |
| 86 | workflow | Durable ordered review queue | `review-11` | pass |
| 87 | workflow | Revision-bound custom checklist signoff | `review-12` | pass |
| 88 | workflow | Approval drift tracking | `review-13` | pass |
| 89 | workflow | Photo-coordinate correction issues | `review-14` | pass |
| 90 | workflow | Correction prerequisite gating | `review-15` | pass |
| 91 | workflow | Batch correction propagation | `review-16` | pass |
| 92 | workflow | Local reviewer workload assignment | `review-17` | pass |
| 93 | workflow | Due-date review scheduling | `review-18` | pass |
| 94 | workflow | Saved-revision pair preference | `review-19` | pass |
| 95 | workflow | Unresolved comparison review | `review-20` | pass |
| 96 | workflow | Saved compound review filters | `review-21` | pass |
| 97 | workflow | Checklist-gated handoff manifest | `review-22` | pass |
| 98 | workflow | Portable review metadata export | `review-23` | pass |
| 99 | workflow | Validated review metadata import | `review-24` | pass |
| 100 | workflow | Durable workflow activity history | `review-25` | pass |

## Reproduce

Build the production UI with `npm.cmd run build` in `ui`, then build the custom-protocol desktop using `scripts/start-aura.ps1 -Rebuild -NoLaunch`. Run `python scripts/audit-aura100-desktop.py` for isolated native/UI qualification. `scripts/audit-aura100-photos.py` starts a new supplied-folder qualification; `scripts/finalize-aura100-photos.py` resumes saved qualification recipes and verified full-resolution exports. These scripts launch their own hidden test process and terminate only that process. `python scripts/report-aura100.py` regenerates this report from retained evidence.

After qualification, `scripts/open-aura-qualified-photos.ps1` opens AURA with the saved 27-photo catalogue for visual review. It does not replace the normal application catalogue.
