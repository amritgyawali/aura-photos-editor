# PHASE-14 progress

Non-Destructive Edit Recipe & GPU Develop Engine. Branch
`feat/phase-14-develop-engine-edit-recipe`.

| Task | Files touched | Tests added | Notes |
|---|---|---|---|
| `COL` colour architecture | `docs/adr/ADR-0029-render-pipeline.md` | - | The working space, precision, the absent backend, the four inputs of `render_hash`, the protection rule, the versioning rules, the interchange split, the error domain. |
| `TLC` recipe schema v1 | `crates/aura-recipe/src/contract/recipe.rs` | 3 | Frozen. Section 5's document field for field, with ranges in every doc comment. |
| `TLC` canonical form and hash | `crates/aura-recipe/src/hash.rs` | 6 | Sorted keys, no whitespace, six fixed decimals. A `NaN` is refused rather than written as `null`. |
| `TLC` validation and clamping | `crates/aura-recipe/src/schema.rs` | 5 | Two kinds of wrong: a value is clamped, a shape is refused. |
| `TLC` the merge | `crates/aura-recipe/src/schema.rs` | 6 | Section 6.4. A pass that tries to empty `user_edited_fields` is refused too. |
| `TLC` migration framework | `crates/aura-recipe/src/migrate.rs`, `tests/migration.rs` | 11 | Frozen v1 document, blessed by an ignored test rather than regenerated on every run. |
| `SRC` XMP interchange | `crates/aura-recipe/src/xmp.rs` | 7 | 24 paths, the crop-angle sign flip, and a test that no identity reaches the packet. |
| `SRC` sidecar and reconciliation | `crates/aura-recipe/src/sidecar.rs` | 9 | Atomic write, kept backup, the sidecar wins and Lightroom edits become protected. |
| `SRC` edit history | `crates/aura-recipe/src/history.rs` | 9 | Whole documents per step, bounded at 100. Reset to AI suggestion is the only way a path becomes unprotected. |
| `SRC` migration 14 and the store | `crates/aura-catalog/migrations/0014_develop.sql`, `crates/aura-recipe/src/store.rs` | 5 | Four tables, one view, no path column anywhere. |
| `COL` colour stages | `crates/aura-render/src/colour.rs` | 9 | Kang CCT, green-normalised multipliers, recovery toward the brightest surviving channel. |
| `COL` camera profiles | `crates/aura-render/src/profiles.rs`, `config/camera_profiles.toml` | 6 | Eight bench bodies, a reference fallback, `AURA-RENDER-8008` for everything real. |
| `SRC` tone and colour maths | `crates/aura-render/src/tonemap.rs` | 15 | Fritsch-Carlson curve that cannot overshoot; every tone stage moves luminance and holds hue. |
| `SRC` spatial stages | `crates/aura-render/src/spatial.rs` | 13 | Separable, deterministic, and with every frame-wide reduction lifted into `Stats`. |
| `COL` output transform | `crates/aura-render/src/output.rs` | 7 | The only module that bakes tone. Positional dither. |
| `SRG` render graph | `crates/aura-render/src/graph.rs` | 14 | 23 stages, a plan that is a filter of ORDER, downstream-only invalidation. |
| `SRC` CPU reference | `crates/aura-render/src/cpu.rs` | 9 | Fused point-wise passes, `f32` throughout, no filesystem in scope. |
| `SRG` tiling | `crates/aura-render/src/tiles.rs` | 6 | Halo is the *sum* of enabled reaches; position-dependent stages know where they are. |
| `SRG` the accelerator port | `crates/aura-render/src/gpu.rs`, `shaders/*.wgsl` | 10 | No `wgpu`. Four WGSL sources, gated against the reference. |
| `QAL` parity harness | `crates/aura-render/src/parity.rs`, `tests/parity.rs` | 14 | Tolerance 1/1024 in linear light; tested against a perturbation, not a device. |
| `QAL` golden suite | `crates/aura-render/src/golden.rs` | 6 | Digest plus dE2000. Stability, explicitly not accuracy. |
| `QAL` colour discipline gate | `crates/aura-render/tests/colour_discipline.rs` | 4 | A grep, as a test: one encoder, one working space, no filesystem. |
| `QAL` section 10.1 gates | `tests/eval/render_eval.rs` | 18 | Seven rows, six asserted directly and the seventh as budget arithmetic plus the tiling identity. |
| `PERF` budgets | `perf/budgets.toml`, `crates/aura-perf/tests/develop_budgets.rs` | 5 | Four GPU rows waived, the CPU fallback row asserted, two rows added. |
| `SFE` IPC surface | `crates/aura-app/src/contract/ipc.rs`, `develop_commands.rs`, `docs/adr/ADR-0030-develop-ipc-surface.md` | 7 | Nine commands, no destination, no way to overwrite a protected path. |
| `MFE` develop panel | `ui/src/components/develop/DevelopPanel.tsx` + test | 7 | Protected dot, caveats in plain words, and a test that no label reads as a delivery decision. |
| `QAL` the phase gate | `crates/aura-cli/src/phase14.rs` | - | Twelve sections, and it prints what it does not prove. |
| `DOC` documentation | `docs/recipe-schema-v1.md`, `docs/colour-management.md` | - | The published schema and the colour explainer. |

## Totals

- **229 tests** across `aura-recipe` (63), `aura-render` (148 including the eval gate) and
  `aura-app` (7 develop), plus **7 UI tests** and **5 budget tests**.
- **10 error codes**, `AURA-RENDER-8001` to `8010`, each with a runbook.
- **23 pipeline stages**, each with a WGSL entry point and a declared halo.
- `aura-cli verify --phase 14` exits 0.

## Defects found and fixed during the phase

**The crop was half a pixel off.** `crop_rotate` centred on `(left + right) / 2`, which is
the midpoint of the *edges* rather than of the pixel centres. Every crop resampled by half a
pixel, softening a frame that should have been copied exactly. Caught by
`the_full_crop_with_no_rotation_is_the_identity`.

**The halo was a maximum and needed to be a sum.** Stages compose: a pixel committed after
sharpening needs correct values twelve pixels away *after clarity*, which needs correct
values forty-eight pixels away in the input. The maximum is right for one spatial stage and
wrong for two, and the failure would have been a faint seam visible only on large exports.

**Two stages were position-dependent and did not know where they were.** Vignette correction
measures a radius from the frame's centre and a mask is drawn in frame coordinates; both were
using the tile's own size. That put a vignette in every tile. `spatial::Position` is the fix
and `tiled_render_equals_whole_render` is what caught it.

**Three stages took a frame-wide reduction inside themselves.** Sharpening's normaliser, noise
reduction's edge keeper and dehaze's floor were each measured over whatever buffer they were
handed - a different number in every tile. `spatial::Stats` lifts all three out, measured once
on a fixed 512 px reduction so the numbers are also independent of the render level.

**The plan was out of pipeline order.** For a camera-native frame the input half pushed the
camera matrix before white balance, because that was the convenient shape of the `match`.
`a_plan_never_reorders_the_stages_it_selects` caught it.

**The Kang coefficients were transposed.** Two terms of the 4000 K-and-above branch were
swapped, which put daylight at x = 0.596 instead of 0.332 and made the blue multiplier
negative. Every white-balance test failed at once, which is the good kind of failure.
