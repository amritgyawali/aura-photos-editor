# AURA 100-tool expansion: corrected implementation and fresh qualification

Date: 2026-10-21, Asia/Katmandu.

This pass preserves and repairs the existing uncommitted hundred-capability expansion,
which follows the earlier two fifty-item passes. It does **not** count those same
tools as another new hundred. There are 25 colour methods, 25 spatial methods,
25 selection workflows and 25 cohort review workflows. All are original AURA
implementations. Exact Evoto behavior, proprietary trained models and full
feature parity remain incomplete and unverified.

Fresh desktop captures from the synthetic fixture:
[colour panel](assets/aura-revalidated-colour-2026-10-21.png),
[spatial panel](assets/aura-revalidated-spatial-2026-10-21.png),
[selection panel](assets/aura-revalidated-selection-2026-10-21.png),
[review workspace](assets/aura-revalidated-workflow-2026-10-21.png).

## Current checks

| Check | Recorded result |
| --- | --- |
| Native workspace, `--locked --no-fail-fast -j 4`, documented `AURA_PERF_HOST_SCALE=6` | 4432 passed, 0 failed, 53 ignored, 291 result groups; exit 0 |
| Frontend | 912 tests in 111 files passed; tests exit 0; production build exit 0 |
| Strict workspace Clippy | exit 0 |
| Frozen contracts | exit 0 |
| Rust format | exit 0 |
| IPC surface | 347 definitions = 347 registrations = 347 typed calls; see current gate log |
| Python evidence/packaging/IPC tests | 15 passed |
| Exact-bit spatial regression rerun | 9 passed; exit 0 |
| Desktop production rebuild | exit 0 |
| Actual desktop audit | completed: True; 138 checks; exit 0 |
| Photo edits | 27/27; completed: True |
| Full-resolution JPEG export | 27 written; 27 verified; render failures: 0; exit 0 |
| Original source SHA-256 unchanged | True |
| Desktop and photo executable identity agrees | True |
| Final AURA review window | opened: True; 27 saved recipe hashes verified against exports |

Logs are under `.work-checks/aura100-revalidation/`. A reported exit other than 0
or an incomplete item is not a pass. Ignored tests are excluded from pass counts.
The native suite covers kernel mathematics, validation, history and application
behavior; its breadth does not certify every camera, optional model, external
service or physical device. UI tests are automated component tests. Desktop checks
invoke the actual registered native routes and selected visible controls.
The frontend retains Vite's large-bundle advisory. The Windows desktop linker
reported a nonfatal `.rsrc` merge warning about multiple non-default manifests;
its build exit and actual startup/runtime results are recorded separately.
The first strict lint run rejected an oversized planning function. Validation
and plan-entry construction were extracted without suppressing that lint; the
first log is retained as `clippy-first.log`, and the table records the final run.
A later lint pass required the new exact-pixel regression assertions to compare
float bit patterns explicitly. That test-only correction preserves the strict
identity requirement; `clippy-exact-pixel-tests.log` retains the original diagnostic.
The final native verification is retained in `workspace-tests-verified.log`;
earlier compilation/qualification logs are separate and excluded from its totals.
The [individual native test ledger](AURA-revalidation-2026-10-21-native-tests.csv)
contains 4485 parsed console records; its pass/fail/ignore counts
match completed summaries: True. Logs remain authoritative if a
console record was interleaved or incomplete. The
[command ledger](AURA-revalidation-2026-10-21-commands.csv) is also available as CSV.

## Bugs repaired

- Guided spatial sampling now renormalizes only eligible bilinear taps, including
  optical/channel sampling. It cannot leak excluded colour through interpolation.
- Rank filtering uses tie midpoints so flat patches remain unchanged. Incomplete
  Haar blocks are left unchanged instead of duplicating edges. Neutral-bridge and
  rotation geometry consistently use pixel-centre coordinates.
- Colour, spatial and material saves no longer duplicate changed history paths or
  append empty history when numeric round-trips have the same canonical recipe;
  explicit user ownership and protection against later automated overwrite remain.
- Three-way transfer compares each target against its own checkpoint; rollback
  binds to its originating checkpoint; relative tone transfer includes clarity,
  texture and dehaze. Legacy plan serialization remains compatible.
- Reopening a prerequisite reopens its dependent issues and invalidates affected
  checklist approvals. Metadata writes recheck collection membership.
- Unedited imported photos can receive their first batch edit atomically, with
  exact Undo/Redo. All recipe saves check membership inside their write transaction.
- Boundary skeletons retain edge-touching structure; fully matching chroma-key
  selections can clear; region splitting respects its maximum-area limit.
- Spatial stacks can reorder. Panels hide irrelevant controls, clear errors on
  discard, reconcile missing selections and discard stale comparison/document
  state when navigation changes their meaning.
- The renderer carries `labs.selected-v2` in cache identity. Evidence scripts
  reject stale catalogues, incorrect import identities, incomplete exports and
  failures instead of writing a successful completion flag.

See [ADR-0135](../adr/ADR-0135-selected-lab-correctness-and-first-edit-qualification.md).
Regression tests include first-save races, foreign project refusal, exact history,
selected/excluded samples, tied patches, odd blocks, reference endpoints, approval
dependencies, ancestor divergence, old serialization and UI navigation.

## Supplied photographs

Input: `H:/AURA EDIT`, 27 Canon CR3s. The fresh run imports by reference into a
separate catalogue, saves automatic edit recipes and renders before/after through
AURA's native renderer. Creative laboratory alternatives remain explicitly selected
tools; incompatible filters are not stacked onto every delivery photo.

Open the [fresh before/after gallery](../../output/aura100-revalidated/index.html).
Full JPEGs are under `output/aura100-revalidated/full-resolution/edited/` and the
[photo manifest](../../output/aura100-revalidated/results.json) records exact source
hashes, saved recipes, export dimensions and executable identity. The prior
[screenshot-guided experiment](../../output/aura100-final/989A2698-screenshot-guided-approximation.jpg)
is preserved separately and is not relabelled as a fresh native Evoto comparison.
Visual inspection of the retained first portrait shows the Evoto display preview
brighter/warmer and smoother than AURA's automatic output; a visible gap remains.
Photographic quality still requires human review at 100%.

## Evoto evidence and unresolved request

REA MCP was run on the actual installed GUI and worker with an explicit 256 MiB
read budget after the default 64 MiB budget refused the worker. Fresh PE evidence:

- Worker `Evoto-worker.exe`, SHA-256
  `add7588f94aed190b97076f949709ba9622aa14f046b661783135d838d3d3bb2`,
  evidence `ev_01eb87daf29b21e756e15269f5e07667d0dd6a7d9aaef6799fd945075d15ed63`.
- GUI `Evoto.exe`, SHA-256
  `7f020192f3b659ca4a1e4f8631b21879982bb21d97d26196a604ad70e2f3b2fa`,
  evidence `ev_feea8fbc779500b8d7a76d654b78642e3231781ecba36f541e57baa2853661cb`.

The artifacts match the previously analysed installation, so retained targeted
native/static and UI evidence is reused with its limitations. PE resource evidence
establishes artifact identity and metadata, not image-processing algorithms. The
current REA MCP session has no configured native decompiler. The matching 6.7.0
public CLI was subsequently verified against the already installed Ghidra/JDK
using process-local environment variables; no configuration or skill was replaced.
CLI readiness: True. A bounded function-dossier attempt
for `0x141026430` records: `{"completed": false, "package": "6.7.0", "seconds": 240.11, "selectedFunction": "0x141026430", "target": "F:\\evoto\\Evoto-worker.exe", "timeoutSeconds": 240}`. A timeout
or failed dossier is not a completed decompilation. No new full-program
decompilation was completed. The previous Windows runtime observation edited
`989A2698.CR3`, but Export displayed a trial-enrollment prompt. No enrollment,
purchase or bypass was performed and no native Evoto export exists in that evidence.

The official [desktop product page](https://www.evoto.ai/ai-photo-editor) and
[Evoto 8.0 announcement](https://blog.evoto.ai/evoto-product-launch-2026/) describe
portrait, colour and workflow categories. Public feature descriptions guide the
inventory; they do not disclose proprietary models or prove equivalent results.

| Requested outcome | Evidence-based status |
| --- | --- |
| REA inspection of installed Evoto | Performed; identity and bounded prior static/runtime evidence |
| 100 additional AURA capabilities | Present and requalified below; independent implementations |
| Additional panels and sections | Colour lab, Spatial repair, Selection laboratory, Review & checkpoints |
| Edit supplied photos and export | Current results above and source-preservation manifest |
| Every Evoto feature and identical output | Incomplete; see previous [feature coverage](AURA-feature-coverage-2026-10-19.json) |
| Evoto neural architecture, weights, trained skins, training data | Not recovered or copied |
| Every optional AURA device/service and every control combination | Not exhaustively exercised; command coverage is explicit below |

The previous [native research](AURA-Evoto-native-research-2026-10-20.md) and
[runtime evidence](AURA-Evoto-colour-runtime-2026-10-21.md) retain their actual
scope. Similar tool purposes are not proof of the same kernels, models or output.

## Individual 100-tool current execution ledger

A pass means its scoped desktop scenario completed. Workflow entries point to
their action evidence within a sequential cohort scenario, supplemented by native
regressions; they are not 25 isolated photographic algorithms. Conditional defect
tools can produce unchanged pixels on a fixture lacking the relevant defect.
Unchanged pixels in this fixture were recorded for: `chroma_winsorize`, `mosquito_suppress`.
Numerical behavior is checked independently by renderer tests. Spatial correction
still uses a bounded 256-pixel grid, selection analysis a 512-pixel grid, and
colour statistics at most 65536 selected samples. These remain practical limits.

| # | Area | Capability | ID | Current desktop result |
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

## Registered command coverage

These invocation counts describe only the current desktop/photo scenarios.
Commands without a current record are explicitly unqualified at this layer.
They may have native/component tests or historical audit evidence; those are
different evidence and are not silently converted into current runtime passes.

| Native command | Current invocations | Scope |
| --- | ---: | --- |
| `accept_colour` | 0 | no current desktop invocation recorded |
| `accept_geometry` | 0 | no current desktop invocation recorded |
| `accept_local` | 0 | no current desktop invocation recorded |
| `accept_micro` | 0 | no current desktop invocation recorded |
| `accept_restore` | 0 | no current desktop invocation recorded |
| `accept_retouch` | 0 | no current desktop invocation recorded |
| `accept_tone` | 0 | no current desktop invocation recorded |
| `activate_licence` | 0 | no current desktop invocation recorded |
| `activity_status` | 0 | no current desktop invocation recorded |
| `adjust_automatic_finish` | 0 | no current desktop invocation recorded |
| `adopt_profile` | 0 | no current desktop invocation recorded |
| `ai_setup_status` | 0 | no current desktop invocation recorded |
| `analyse_composition` | 0 | no current desktop invocation recorded |
| `analyse_integrity` | 0 | no current desktop invocation recorded |
| `analyse_portrait` | 0 | no current desktop invocation recorded |
| `analyse_reference_style` | 0 | no current desktop invocation recorded |
| `apply_color_match` | 0 | no current desktop invocation recorded |
| `apply_edit_profile` | 0 | no current desktop invocation recorded |
| `apply_reference_style` | 0 | no current desktop invocation recorded |
| `apply_regional_color_match` | 0 | no current desktop invocation recorded |
| `auto_advanced_retouch` | 0 | no current desktop invocation recorded |
| `auto_portrait_retouch` | 0 | no current desktop invocation recorded |
| `auto_retouch` | 0 | no current desktop invocation recorded |
| `automatic_finish_state` | 0 | no current desktop invocation recorded |
| `automatic_start` | 0 | no current desktop invocation recorded |
| `autopilot_cancel` | 0 | no current desktop invocation recorded |
| `autopilot_events` | 0 | no current desktop invocation recorded |
| `autopilot_preflight` | 0 | no current desktop invocation recorded |
| `autopilot_progress` | 0 | no current desktop invocation recorded |
| `autopilot_set_settings` | 0 | no current desktop invocation recorded |
| `autopilot_stages` | 0 | no current desktop invocation recorded |
| `autopilot_start` | 0 | no current desktop invocation recorded |
| `autopilot_status` | 0 | no current desktop invocation recorded |
| `autopilot_summary` | 0 | no current desktop invocation recorded |
| `build_index` | 0 | no current desktop invocation recorded |
| `camera_fingerprints` | 0 | no current desktop invocation recorded |
| `camera_pairs` | 0 | no current desktop invocation recorded |
| `camera_pass` | 0 | no current desktop invocation recorded |
| `camera_reason_codes` | 0 | no current desktop invocation recorded |
| `camera_reports` | 0 | no current desktop invocation recorded |
| `camera_shooter_bias` | 0 | no current desktop invocation recorded |
| `camera_status` | 0 | no current desktop invocation recorded |
| `camera_transforms` | 0 | no current desktop invocation recorded |
| `cancel_job` | 0 | no current desktop invocation recorded |
| `cancel_previews` | 0 | no current desktop invocation recorded |
| `capture_color_reference` | 0 | no current desktop invocation recorded |
| `check_ai_key` | 0 | no current desktop invocation recorded |
| `classify_scenes` | 0 | no current desktop invocation recorded |
| `cleanup_blocked` | 0 | no current desktop invocation recorded |
| `cleanup_disclosures` | 0 | no current desktop invocation recorded |
| `cleanup_pass` | 0 | no current desktop invocation recorded |
| `cleanup_reason_codes` | 0 | no current desktop invocation recorded |
| `cleanup_status` | 0 | no current desktop invocation recorded |
| `clear_ai_key` | 0 | no current desktop invocation recorded |
| `cloud_cache_stats` | 0 | no current desktop invocation recorded |
| `cloud_calls` | 0 | no current desktop invocation recorded |
| `cloud_spend` | 0 | no current desktop invocation recorded |
| `cloud_status` | 0 | no current desktop invocation recorded |
| `color_match_status` | 0 | no current desktop invocation recorded |
| `color_only_preview` | 0 | no current desktop invocation recorded |
| `color_reference_library_workflow` | 0 | no current desktop invocation recorded |
| `colour_review_queue` | 0 | no current desktop invocation recorded |
| `colour_status` | 0 | no current desktop invocation recorded |
| `compact_ledger` | 0 | no current desktop invocation recorded |
| `compare_profiles` | 0 | no current desktop invocation recorded |
| `composition_status` | 0 | no current desktop invocation recorded |
| `create_local_mask` | 0 | no current desktop invocation recorded |
| `create_project` | 2 | invoked in current desktop scenario |
| `cull_project` | 0 | no current desktop invocation recorded |
| `cull_status` | 0 | no current desktop invocation recorded |
| `curate_album` | 0 | no current desktop invocation recorded |
| `curate_bw` | 0 | no current desktop invocation recorded |
| `curate_decide` | 0 | no current desktop invocation recorded |
| `curate_export` | 0 | no current desktop invocation recorded |
| `curate_heroes` | 0 | no current desktop invocation recorded |
| `curate_project` | 0 | no current desktop invocation recorded |
| `curate_set_order` | 0 | no current desktop invocation recorded |
| `curate_social` | 0 | no current desktop invocation recorded |
| `curate_spread` | 0 | no current desktop invocation recorded |
| `curate_status` | 0 | no current desktop invocation recorded |
| `curate_teaser` | 0 | no current desktop invocation recorded |
| `deactivate_licence` | 0 | no current desktop invocation recorded |
| `decide_cleanup` | 0 | no current desktop invocation recorded |
| `decision_by_id` | 0 | no current desktop invocation recorded |
| `decision_history` | 0 | no current desktop invocation recorded |
| `delete_personal_style` | 0 | no current desktop invocation recorded |
| `delivery_backup` | 0 | no current desktop invocation recorded |
| `delivery_items` | 0 | no current desktop invocation recorded |
| `delivery_providers` | 0 | no current desktop invocation recorded |
| `delivery_status` | 0 | no current desktop invocation recorded |
| `delivery_upload` | 0 | no current desktop invocation recorded |
| `develop_status` | 0 | no current desktop invocation recorded |
| `diagnostics_report` | 0 | no current desktop invocation recorded |
| `disable_camera` | 0 | no current desktop invocation recorded |
| `disable_cleanup` | 0 | no current desktop invocation recorded |
| `disable_gallery` | 0 | no current desktop invocation recorded |
| `dismiss_composition_flag` | 0 | no current desktop invocation recorded |
| `dismiss_flag` | 0 | no current desktop invocation recorded |
| `edit_mask` | 0 | no current desktop invocation recorded |
| `embed_project` | 0 | no current desktop invocation recorded |
| `emotion_status` | 0 | no current desktop invocation recorded |
| `enhance_photo` | 27 | invoked in current desktop scenario |
| `enhance_portrait` | 0 | no current desktop invocation recorded |
| `ensure_masks` | 0 | no current desktop invocation recorded |
| `erase_biometrics` | 0 | no current desktop invocation recorded |
| `estimate_colour` | 0 | no current desktop invocation recorded |
| `estimate_tone` | 0 | no current desktop invocation recorded |
| `explain_image` | 0 | no current desktop invocation recorded |
| `export_files` | 0 | no current desktop invocation recorded |
| `export_manifest` | 0 | no current desktop invocation recorded |
| `export_presets` | 0 | no current desktop invocation recorded |
| `export_preview_names` | 0 | no current desktop invocation recorded |
| `export_profile` | 0 | no current desktop invocation recorded |
| `export_run` | 1 | invoked in current desktop scenario |
| `export_run_watermarked` | 0 | no current desktop invocation recorded |
| `export_status` | 0 | no current desktop invocation recorded |
| `export_support_bundle` | 0 | no current desktop invocation recorded |
| `find_similar` | 0 | no current desktop invocation recorded |
| `flagged_composition` | 0 | no current desktop invocation recorded |
| `flagged_images` | 0 | no current desktop invocation recorded |
| `forget_look` | 0 | no current desktop invocation recorded |
| `framing_context` | 0 | no current desktop invocation recorded |
| `gallery` | 0 | no current desktop invocation recorded |
| `gallery_node_strip` | 0 | no current desktop invocation recorded |
| `gallery_nodes` | 0 | no current desktop invocation recorded |
| `gallery_outliers` | 0 | no current desktop invocation recorded |
| `gallery_pass` | 0 | no current desktop invocation recorded |
| `gallery_reason_codes` | 0 | no current desktop invocation recorded |
| `gallery_status` | 0 | no current desktop invocation recorded |
| `geometry_review_queue` | 0 | no current desktop invocation recorded |
| `geometry_status` | 0 | no current desktop invocation recorded |
| `get_preview` | 0 | no current desktop invocation recorded |
| `group_moments` | 0 | no current desktop invocation recorded |
| `group_people` | 0 | no current desktop invocation recorded |
| `hardware_plan` | 0 | no current desktop invocation recorded |
| `history_step` | 225 | invoked in current desktop scenario |
| `identity_cover` | 0 | no current desktop invocation recorded |
| `identity_timelines` | 0 | no current desktop invocation recorded |
| `image_cleanup` | 0 | no current desktop invocation recorded |
| `image_colour` | 0 | no current desktop invocation recorded |
| `image_composition` | 0 | no current desktop invocation recorded |
| `image_decision` | 0 | no current desktop invocation recorded |
| `image_descriptors` | 0 | no current desktop invocation recorded |
| `image_emotion` | 0 | no current desktop invocation recorded |
| `image_gallery` | 0 | no current desktop invocation recorded |
| `image_geometry` | 0 | no current desktop invocation recorded |
| `image_history` | 100 | invoked in current desktop scenario |
| `image_integrity` | 0 | no current desktop invocation recorded |
| `image_local` | 0 | no current desktop invocation recorded |
| `image_masks` | 0 | no current desktop invocation recorded |
| `image_micro` | 0 | no current desktop invocation recorded |
| `image_recipe` | 272 | invoked in current desktop scenario |
| `image_restore` | 0 | no current desktop invocation recorded |
| `image_retouch` | 0 | no current desktop invocation recorded |
| `image_scene` | 0 | no current desktop invocation recorded |
| `image_subjects` | 0 | no current desktop invocation recorded |
| `image_tone` | 0 | no current desktop invocation recorded |
| `import_color_reference` | 0 | no current desktop invocation recorded |
| `import_profile` | 0 | no current desktop invocation recorded |
| `index_status` | 0 | no current desktop invocation recorded |
| `infer_stats` | 0 | no current desktop invocation recorded |
| `ingest_progress` | 0 | no current desktop invocation recorded |
| `inspect_photo_diagnostics` | 0 | no current desktop invocation recorded |
| `integrity_status` | 0 | no current desktop invocation recorded |
| `internal_ai_feedback` | 0 | no current desktop invocation recorded |
| `internal_ai_status` | 0 | no current desktop invocation recorded |
| `learn_adopt` | 0 | no current desktop invocation recorded |
| `learn_buckets` | 0 | no current desktop invocation recorded |
| `learn_compare` | 0 | no current desktop invocation recorded |
| `learn_consent` | 0 | no current desktop invocation recorded |
| `learn_lightroom_style` | 0 | no current desktop invocation recorded |
| `learn_roll_back` | 0 | no current desktop invocation recorded |
| `learn_set_consent` | 0 | no current desktop invocation recorded |
| `learn_status` | 0 | no current desktop invocation recorded |
| `ledger_status` | 0 | no current desktop invocation recorded |
| `licence_status` | 0 | no current desktop invocation recorded |
| `list_ai_providers` | 0 | no current desktop invocation recorded |
| `list_color_references` | 0 | no current desktop invocation recorded |
| `list_edit_profiles` | 0 | no current desktop invocation recorded |
| `list_identities` | 0 | no current desktop invocation recorded |
| `list_images` | 4 | invoked in current desktop scenario |
| `list_looks` | 0 | no current desktop invocation recorded |
| `list_models` | 0 | no current desktop invocation recorded |
| `list_moments` | 0 | no current desktop invocation recorded |
| `list_problems` | 0 | no current desktop invocation recorded |
| `list_profiles` | 0 | no current desktop invocation recorded |
| `list_projects` | 0 | no current desktop invocation recorded |
| `live_preview` | 0 | no current desktop invocation recorded |
| `local_mask_coverage` | 0 | no current desktop invocation recorded |
| `local_masks` | 0 | no current desktop invocation recorded |
| `local_review_queue` | 0 | no current desktop invocation recorded |
| `local_status` | 0 | no current desktop invocation recorded |
| `lock_moment` | 0 | no current desktop invocation recorded |
| `look_buckets` | 0 | no current desktop invocation recorded |
| `look_match_report` | 0 | no current desktop invocation recorded |
| `look_status` | 0 | no current desktop invocation recorded |
| `manage_color_reference` | 0 | no current desktop invocation recorded |
| `manual_remove` | 0 | no current desktop invocation recorded |
| `mask_allowance` | 0 | no current desktop invocation recorded |
| `mask_kinds` | 0 | no current desktop invocation recorded |
| `mask_overlay` | 0 | no current desktop invocation recorded |
| `mask_status` | 0 | no current desktop invocation recorded |
| `mask_workflow` | 25 | invoked in current desktop scenario |
| `match_paired_reference` | 0 | no current desktop invocation recorded |
| `measure_look` | 0 | no current desktop invocation recorded |
| `measure_studio_color` | 0 | no current desktop invocation recorded |
| `merge_chapters` | 0 | no current desktop invocation recorded |
| `merge_identities` | 0 | no current desktop invocation recorded |
| `merge_moments` | 0 | no current desktop invocation recorded |
| `micro_composites` | 0 | no current desktop invocation recorded |
| `micro_matrix` | 0 | no current desktop invocation recorded |
| `micro_pass` | 0 | no current desktop invocation recorded |
| `micro_reason_codes` | 0 | no current desktop invocation recorded |
| `micro_review_queue` | 0 | no current desktop invocation recorded |
| `micro_status` | 0 | no current desktop invocation recorded |
| `moment_duplicates` | 0 | no current desktop invocation recorded |
| `moment_of_image` | 0 | no current desktop invocation recorded |
| `moment_peak` | 0 | no current desktop invocation recorded |
| `moment_status` | 0 | no current desktop invocation recorded |
| `move_chapter_boundary` | 0 | no current desktop invocation recorded |
| `native_retouch_blemish_review` | 0 | no current desktop invocation recorded |
| `native_retouch_draft_preview` | 0 | no current desktop invocation recorded |
| `native_retouch_edit` | 0 | no current desktop invocation recorded |
| `native_retouch_preview` | 0 | no current desktop invocation recorded |
| `native_retouch_saved_selection` | 0 | no current desktop invocation recorded |
| `native_retouch_selection_preview` | 0 | no current desktop invocation recorded |
| `one_click_cancel` | 0 | no current desktop invocation recorded |
| `one_click_finish` | 0 | no current desktop invocation recorded |
| `one_click_status` | 0 | no current desktop invocation recorded |
| `open_ai_provider_page` | 0 | no current desktop invocation recorded |
| `open_provider_page` | 0 | no current desktop invocation recorded |
| `override_decision` | 0 | no current desktop invocation recorded |
| `parse_reference` | 0 | no current desktop invocation recorded |
| `people_status` | 0 | no current desktop invocation recorded |
| `photo_analysis` | 0 | no current desktop invocation recorded |
| `photo_auto_edit` | 0 | no current desktop invocation recorded |
| `photo_original` | 0 | no current desktop invocation recorded |
| `pick_white_balance` | 0 | no current desktop invocation recorded |
| `pin_gallery_anchor` | 0 | no current desktop invocation recorded |
| `plan_geometry` | 0 | no current desktop invocation recorded |
| `portrait_retouch` | 0 | no current desktop invocation recorded |
| `prefer_frame` | 0 | no current desktop invocation recorded |
| `prefetch_previews` | 0 | no current desktop invocation recorded |
| `prepare_color_match` | 0 | no current desktop invocation recorded |
| `prepare_regional_color_match` | 0 | no current desktop invocation recorded |
| `preview_edit_profile` | 0 | no current desktop invocation recorded |
| `preview_problems` | 0 | no current desktop invocation recorded |
| `preview_stats` | 0 | no current desktop invocation recorded |
| `profile_pairs` | 0 | no current desktop invocation recorded |
| `profile_report` | 0 | no current desktop invocation recorded |
| `protected_features` | 0 | no current desktop invocation recorded |
| `purge_cache` | 0 | no current desktop invocation recorded |
| `purge_cloud_cache` | 0 | no current desktop invocation recorded |
| `qc_decide` | 0 | no current desktop invocation recorded |
| `qc_decide_bulk` | 0 | no current desktop invocation recorded |
| `qc_queue` | 0 | no current desktop invocation recorded |
| `qc_queue_grouped` | 0 | no current desktop invocation recorded |
| `qc_report` | 0 | no current desktop invocation recorded |
| `qc_report_markdown` | 0 | no current desktop invocation recorded |
| `qc_rounds` | 0 | no current desktop invocation recorded |
| `qc_run` | 0 | no current desktop invocation recorded |
| `qc_status` | 0 | no current desktop invocation recorded |
| `qc_tickets` | 0 | no current desktop invocation recorded |
| `ranked_by_emotion` | 0 | no current desktop invocation recorded |
| `reactions_of` | 0 | no current desktop invocation recorded |
| `recheck_hardware` | 0 | no current desktop invocation recorded |
| `record_decisions` | 0 | no current desktop invocation recorded |
| `reference_frames` | 0 | no current desktop invocation recorded |
| `refresh_licence` | 0 | no current desktop invocation recorded |
| `regenerate_mask` | 0 | no current desktop invocation recorded |
| `regional_color_match_status` | 0 | no current desktop invocation recorded |
| `rename_identity` | 0 | no current desktop invocation recorded |
| `rename_look` | 0 | no current desktop invocation recorded |
| `render_caps` | 0 | no current desktop invocation recorded |
| `render_image` | 280 | invoked in current desktop scenario |
| `resize_gallery` | 0 | no current desktop invocation recorded |
| `restore_identity_refusals` | 0 | no current desktop invocation recorded |
| `restore_pass` | 0 | no current desktop invocation recorded |
| `restore_reason_codes` | 0 | no current desktop invocation recorded |
| `restore_review_queue` | 0 | no current desktop invocation recorded |
| `restore_status` | 0 | no current desktop invocation recorded |
| `retouch_pass` | 0 | no current desktop invocation recorded |
| `retouch_review_queue` | 0 | no current desktop invocation recorded |
| `retouch_status` | 0 | no current desktop invocation recorded |
| `review_queue` | 0 | no current desktop invocation recorded |
| `save_ai_setup` | 0 | no current desktop invocation recorded |
| `save_color_lab` | 75 | invoked in current desktop scenario |
| `save_local_masks` | 2 | invoked in current desktop scenario |
| `save_material_lab` | 0 | no current desktop invocation recorded |
| `save_spatial_lab` | 75 | invoked in current desktop scenario |
| `save_studio_color` | 0 | no current desktop invocation recorded |
| `save_studio_finish` | 0 | no current desktop invocation recorded |
| `scan_archive` | 0 | no current desktop invocation recorded |
| `scan_faces` | 0 | no current desktop invocation recorded |
| `scene_profiles` | 0 | no current desktop invocation recorded |
| `score_emotion` | 0 | no current desktop invocation recorded |
| `sculpt_local` | 0 | no current desktop invocation recorded |
| `segment_story` | 0 | no current desktop invocation recorded |
| `select_colour_variant` | 0 | no current desktop invocation recorded |
| `select_look` | 0 | no current desktop invocation recorded |
| `set_ai_key` | 0 | no current desktop invocation recorded |
| `set_cache_budget` | 0 | no current desktop invocation recorded |
| `set_camera_label` | 0 | no current desktop invocation recorded |
| `set_camera_override` | 0 | no current desktop invocation recorded |
| `set_camera_reference` | 0 | no current desktop invocation recorded |
| `set_chapter` | 0 | no current desktop invocation recorded |
| `set_cloud_budget` | 0 | no current desktop invocation recorded |
| `set_cloud_privacy` | 0 | no current desktop invocation recorded |
| `set_colour_override` | 0 | no current desktop invocation recorded |
| `set_cull_mode` | 0 | no current desktop invocation recorded |
| `set_execution_provider` | 0 | no current desktop invocation recorded |
| `set_framing` | 0 | no current desktop invocation recorded |
| `set_gallery_override` | 0 | no current desktop invocation recorded |
| `set_identity_importance` | 0 | no current desktop invocation recorded |
| `set_identity_role` | 0 | no current desktop invocation recorded |
| `set_keep_hint` | 0 | no current desktop invocation recorded |
| `set_local_strength` | 0 | no current desktop invocation recorded |
| `set_look_strength` | 0 | no current desktop invocation recorded |
| `set_micro_matrix` | 0 | no current desktop invocation recorded |
| `set_moment_peak` | 0 | no current desktop invocation recorded |
| `set_param` | 2 | invoked in current desktop scenario |
| `set_portrait_retouch` | 0 | no current desktop invocation recorded |
| `set_project_profile` | 0 | no current desktop invocation recorded |
| `set_protection` | 0 | no current desktop invocation recorded |
| `set_restore_override` | 0 | no current desktop invocation recorded |
| `set_retouch` | 0 | no current desktop invocation recorded |
| `set_tone_override` | 0 | no current desktop invocation recorded |
| `skip_ai_setup` | 0 | no current desktop invocation recorded |
| `snapshot` | 0 | no current desktop invocation recorded |
| `split_chapter` | 0 | no current desktop invocation recorded |
| `split_identity` | 0 | no current desktop invocation recorded |
| `split_moment` | 0 | no current desktop invocation recorded |
| `start_ingest` | 2 | invoked in current desktop scenario |
| `story_outline` | 0 | no current desktop invocation recorded |
| `story_status` | 0 | no current desktop invocation recorded |
| `studio_finish` | 0 | no current desktop invocation recorded |
| `style_status` | 0 | no current desktop invocation recorded |
| `suggest_portrait_light` | 0 | no current desktop invocation recorded |
| `sync_settings` | 0 | no current desktop invocation recorded |
| `tone_review_queue` | 0 | no current desktop invocation recorded |
| `tone_status` | 0 | no current desktop invocation recorded |
| `train_profile` | 0 | no current desktop invocation recorded |
| `ui_audit` | 0 | no current desktop invocation recorded |
| `undo_moment_edit` | 0 | no current desktop invocation recorded |
| `warmup_models` | 0 | no current desktop invocation recorded |
| `within_moment` | 0 | no current desktop invocation recorded |
| `workflow_lab` | 35 | invoked in current desktop scenario |
