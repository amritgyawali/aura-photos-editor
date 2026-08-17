# PHASE-14 exit report - Non-Destructive Edit Recipe & GPU Develop Engine

**Branch:** `feat/phase-14-develop-engine-edit-recipe` · **Gate:** `aura-cli verify --phase 14`
exits 0 · **Status:** implemented **conditionally**, on the six conditions in section 8.

## 1. What shipped

Two crates, one migration, one IPC surface, one panel, two ADRs and a gate.

`aura-recipe` owns the document. `contract/recipe.rs` freezes schema v1 - section 5's JSON,
field for field, with every range in a doc comment. `hash.rs` is the canonical form: sorted
keys, no whitespace between tokens, six fixed decimals, and a refusal rather than a `null`
for a non-finite number. `schema.rs` holds the two kinds of wrong (a value is clamped, a
shape is refused) and **the merge**, which is the one function in the workspace that writes
one recipe into another and the place section 6.4 stops being a convention. `migrate.rs` is
the version framework and its never-remove-a-field rule. `xmp.rs` carries the twenty-four
parameters Lightroom understands, with the crop-angle sign flipped in both directions.
`sidecar.rs` writes the lossless copy atomically, keeps a backup, and reconciles a Lightroom
edit into a protected one. `history.rs` is undo, redo, snapshots and the two resets.
`store.rs` owns migration 14's four tables.

`aura-render` owns the pixels. `colour.rs` recovers highlights before white balance and
turns a temperature into green-normalised multipliers. `profiles.rs` resolves a body to a
matrix and says so when it cannot. `tonemap.rs` is exposure, the four tone bands, contrast, a
Fritsch-Carlson curve that cannot overshoot, HSL, vibrance and the monochrome mix - every one
of them moving luminance and holding hue. `spatial.rs` is the neighbourhood stages plus the
frame-wide statistics they are forbidden to measure themselves. `output.rs` is the only module
that bakes tone. `graph.rs` is 23 stages in one array, a plan that is a filter of that array,
and downstream-only invalidation. `cpu.rs` is the reference pipeline. `tiles.rs` makes a
streamed render bit-identical to a whole one. `gpu.rs` is the port, `parity.rs` the harness,
`shaders/` the four WGSL sources, `golden.rs` the digest and the dE2000.

Migration 14 adds `edit_recipes`, `edit_history`, `edit_snapshots`, `export_renders` and
`v_develop_coverage`. **There is no path column and no `deleted` flag anywhere in it.**

The IPC surface is nine commands (ADR-0030) and the Develop panel renders the protected dot,
the caveats in plain words, and the engine that drew the frame.

## 2. Acceptance criteria (section 13)

| # | Criterion | Evidence |
|---|---|---|
| 1 | A recipe fully describes an edit, renders identically on any machine, and never modifies the RAW | `render_eval::a_recipe_round_trip_produces_identical_pixels`, `the_same_recipe_renders_byte_identically_twice`, `colour_discipline::nothing_in_this_crate_can_open_a_file`. **Partially met:** "any machine" is asserted as determinism within this build; there is no second platform in CI (condition C4). |
| 2 | Sliders respond within the interactive budget at 2048 px | **Not met.** 6.4 s in debug, ~210 ms in release, against a 60 ms RTX 4070 target. No GPU backend (condition C1). The processor guardrail of 450 ms is asserted in release by `develop_budgets`. |
| 3 | Exported JPEG/TIFF files are colour-managed and match the on-screen render | **Partially met.** The output transform, the three spaces and the ICC names ship and are tested (`render_eval::a_neutral_render_stays_neutral`, `output::*`). File *encoding* is phase 30; nothing here writes a file. |
| 4 | XMP export opens in Lightroom with the compatible parameters intact | **Partially met.** 24/24 subset paths survive a write-then-read (`render_eval::the_xmp_round_trip_preserves_the_lightroom_subset`) and nothing outside the subset reaches the packet. **Not opened in Lightroom** - condition C5. |
| 5 | Parameters a user has touched survive every subsequent AI pass | **Met.** `render_eval::a_photographers_parameter_survives_every_automated_pass` runs twenty passes across all three automated sources; `a_protected_parameter_changes_no_pixels_after_an_automated_pass` asserts it at the pixel level. |
| 6 | Golden and parity gates run on NVIDIA, Apple and DirectML CI runners | **Not met.** No GPU runners and no GPU backend (conditions C1 and C4). The gates run on the processor path and the parity harness is proven against a perturbation. |

## 3. What the section 10.1 gates measured

`tests/eval/render_eval.rs`, 18 tests, all green.

| Row | Result |
|---|---|
| Golden renders across bodies and scenes | 8 bench bodies x 5 scene fixtures, byte-identical run to run, 8 distinct digests |
| GPU/CPU parity within 1/1024 per stage | Harness catches a drift at 1.01x the tolerance; **no backend to compare** |
| Recipe round trip produces an identical hash | Identical hash and identical pixels |
| XMP round trip preserves the Lightroom subset | 24/24 |
| `user_edited_fields` never overwritten | 20 consecutive automated passes, 0 overwrites |
| Full-resolution export inside the memory budget | 100 MP exceeds the 1 GB ceiling and streams; streamed == whole |
| v1 renders under a simulated v2 | Identical pixels, `AURA-RENDER-8003` raised, unknown field preserved |

## 4. Benchmarks

Measured by `crates/aura-perf/tests/develop_budgets.rs` on the development machine
(Intel i5-10300H, 4 cores, no discrete GPU). **Every GPU row in section 11 is waived**
(ADR-0029 section 4), and nothing below is presented as a GPU number.

| Row | Section 11 | This build | Status |
|---|---|---|---|
| Proxy 2048 px (RTX 4070) | <= 60 ms | - | waived, no backend |
| Proxy 2048 px (M3 Pro) | <= 110 ms | - | waived, no backend |
| Full 45 MP + export (GPU) | <= 900 ms | - | waived, no backend |
| Full 45 MP (CPU fallback) | <= 6 s | asserted per megapixel at 133 ms | **met** |
| Batch export (RTX 4070) | >= 1.2 img/s | - | waived, no backend |
| Proxy 2048 px (processor guardrail) | - | 450 ms budget | added, met |
| Slider re-render (processor) | - | 225 ms budget | added, met |
| Canonical + hash, per recipe | - | 1 ms budget | added, met |
| Storage per 1,000 images | - | 12 KB/image | added, met |

The storage figure is the largest per-image number in the product and the budget file
carries its decomposition and the two reductions that were considered and rejected. The short
version: `edit_history.body` is a complete document per step because an undo that is nearly
right is worse than no undo, and what bounds the cost is `MAX_ENTRIES` plus a trim on write.

## 5. Telemetry (section 11)

`render.request` is `RenderDto`'s `{level, ms, backend, cacheHit, stagesRun}` on the IPC
surface. `render.parity_fail` is `AURA-RENDER-8010` with `stage` and `max_delta` in its
context. `export.batch` is **not emitted**: nothing in this phase exports, and an event for a
thing that does not happen would be a metric nobody can interpret. Phase 30 adds it.

## 6. Invariants

| Invariant | How it is kept |
|---|---|
| 1. Never mutate a RAW | No path type in `aura-render`, no path column in migration 14, no destination on the IPC surface. Three tests grep for it. |
| 2. Confidence and reasons | `Provenance::confidence` and `decision_id` are required fields; `RenderedImage::notes` says what was skipped and why. |
| 3. Three-tier compute | `RenderLevel` and `RenderPurpose::may_skip_heavy_stages`. |
| 4. Determinism | Same recipe, same pixels, same bytes - tiled or whole, run to run. No atomics, ordered reductions, positional dither, frame-wide statistics measured once. |
| 5. Resumability | The sidecar write is atomic; the store's save is one transaction. |
| 6. Local-first | No network anywhere in either crate. Section 7 asks for no cloud call and there is none. |
| 7. Scene-conditioned | Inherited rather than added: this phase executes parameters, and phases 15 to 17 decide them per scene. |
| 8. Colour discipline | One working space, entered once and left once; `colour_discipline.rs` is the grep that keeps it. |
| 9. No silent failure | Ten codes, ten runbooks, and `SkipReason` so an absent capability is a note rather than a silence. |

## 7. Rollback

Feature flag: the develop panel reads `render_caps` and renders nothing if the engine is
unavailable. Migration: the down migration is five drops, listed in `0014_develop.sql`, and it
**loses every edit** - the second non-recomputable migration in a row. The sidecars beside the
RAWs are the second copy that makes it survivable, which is why `sidecar_written` exists and
why the develop status reports how many are behind.

## 8. Conditions carried out of this phase

**C1 - There is no GPU backend, and four of section 11's five rows are waived.** `Sev 2.`
The processor path is the reference and is correct; it is roughly 3.5x slower than the
interactive budget in release and the panel says so once per session
(`AURA-RENDER-8001`). This closes when a `wgpu` backend lands and passes
`parity::compare` on every stage at 1/1024. Until then no phase may claim an interactive
figure that depends on hardware acceleration.

**C2 - No golden render in this repository is a claim about colour accuracy.** `Sev 2.`
Section 10.1 asks for twelve camera bodies across six scene types; what runs is eight
*synthetic* bench profiles across five authored frames, because there are no camera files and
no photographed ColorChecker here - phase 02's exit conditions, carried forward through
ADR-0006 and still open. The suite is a determinism and regression gate. **No later phase may
claim a colour result that depends on a camera profile being measured until this closes**, and
the gate prints the distinction on every run.

**C3 - `camera_profiles.toml` ships no real body.** Every real camera renders through the
neutral reference profile and raises `AURA-RENDER-8008`. Closing C2 closes this.

**C4 - The three-OS, three-GPU CI matrix does not exist.** Phase 02's condition, inherited
for the fourth time. Determinism is asserted within one build on one machine.

**C5 - The XMP has never been opened in Lightroom.** The round trip is asserted against our
own reader, which proves the format is self-consistent and not that Adobe agrees with it. A
single manual check with a real Lightroom installation closes it.

**C6 - Lens distortion and chromatic aberration are not corrected.** They need per-lens
profiles that arrive with phase 23. The stages exist in the graph, are refused with
`SkipReason::LensProfileAbsent`, and are visible in the panel. Vignette correction, which is
a number rather than a model, ships.

## 9. What was deliberately not built

**No `wgpu` dependency.** ADR-0029 section 4. The WGSL sources ship and are gated against the
Rust reference by `shader_parity.rs`, so the shaders cannot drift while the backend is absent
- which is the failure mode that would otherwise cost a month the day one lands.

**No mask generators.** Section 2.2 gives them to phase 18. Linear and radial masks are pure
geometry and run; the other six kinds are refused with a note naming the mask.

**No retouch operators, no restoration, no perspective.** Phases 20, 21, 22 and 23. Each has a
stage in the graph, a WGSL entry point that is currently the identity, and a `SkipReason`.

**No export.** Nothing in this phase writes an image file. Phase 30 owns delivery, and the
absence is structural: there is no path anywhere on the surface.

**No cloud call.** Section 7 asks for none and there is none. The phase works with the network
cable unplugged, which is invariant 6 at its easiest.

## 10. Five rules this phase adds and every later phase inherits

- **`RenderService` is the only way to turn a recipe into pixels.** Sixth service of its kind
  and the first that produces an output rather than a judgement. Two answers to "what does
  this photograph look like" is a gallery that does not match the album that does not match
  the proof.
- **A parameter a person set is never overwritten, and the merge is where that is true.**
  `aura_recipe::schema::merge` is the only function that writes one recipe into another. There
  is no argument that disables the protection, no IPC field that routes around it, and
  `AURA-RENDER-8006` makes every refusal visible.
- **The output transform is the only place tone is baked.** Everything before it is
  reversible and lives in linear Rec.2020. `colour_discipline.rs` is a grep as a test, so the
  second module to start encoding fails the build.
- **A render says what it skipped.** `SkipReason` is a closed set and each variant names the
  phase that fills the gap. An absent mask generator is a sentence in the panel, not a mask
  that silently did nothing.
- **A delivered file can be re-created from four values.** The RAW's content hash, the
  canonical recipe, the engine string and the output spec. All four are stored with every
  export, and `AURA-RENDER-8007` says which one moved.

## 11. Two decisions worth remembering because they will be re-argued

**The halo is a sum, not a maximum.** Stages compose: a pixel committed after sharpening needs
correct values twelve pixels away after clarity, which needs correct values forty-eight pixels
away in the input. A maximum is right for one spatial stage and wrong for two, and the failure
is a faint seam that only appears on large exports. The first person who finds the halo
"wasteful" should read this paragraph before changing it.

**Frame-wide statistics are measured once and passed in.** Sharpening's normaliser, noise
reduction's edge keeper and dehaze's floor are properties of the photograph, not of a tile.
Measured inside a stage they take a different value in every tile, and the tiled render stops
matching the whole one. `spatial::Stats` exists for that reason alone, and it is measured on a
fixed 512 px reduction so the numbers are independent of the render level too.
