# ADR-0029 - The develop engine: the edit recipe, the colour pipeline and the determinism contract

**Status:** accepted · **Date:** 2026-08-17 · **Phase:** 14 · **Supersedes:** nothing

Phase 14 section 4 asks for this document under the name `ADR-0014-render-pipeline.md`. The
ADR numbering in this repository is sequential across the whole project rather than aligned
to phase numbers - ADR-0014 was taken by the people IPC surface in phase 06 - so the file
keeps its subject and takes the next free number. `docs/plan/CLAUDE.md` section 1 says the
newer ADR wins; this is the render-pipeline ADR that phase 14 section 8 step 1 requires.

## 1. Context

Thirteen phases have decided *which* photographs are delivered. None of them has changed a
pixel. Phase 14 is the hinge: from here on the product edits, and every phase from 15 to 26
writes its decisions into one document and expects one renderer to execute them identically
on every machine, for years, including on a machine that has not been built yet.

That makes three things load-bearing at once.

**Colour.** Invariant 8 says colour maths happens in linear scene-referred light and
converts once at the boundaries. Phase 02 already built that space -
`aura_raw::colour::working_space`, linear Rec.2020 at D65 - and phase 14 must grade inside
it rather than beside it.

**Determinism.** Invariant 4 says identical inputs produce identical output. A gallery
delivered in March and re-exported in November must be the same file. That is not a
nice-to-have: it is what makes `render_hash` a support answer rather than a guess.

**Honesty about the recipe.** Invariant 1 says the RAW is never modified. Every edit is a
row plus a JSON document, and the photographer can change any of it. The recipe is also the
interchange currency - XMP out to Lightroom, style learning in phase 17, QC re-edits in
phase 27, the learning loop in phase 30 - so its shape has to survive being read by builds
that do not exist yet.

## 2. Decision: the working space, and where tone is baked

The pipeline is one linear scene-referred pass with exactly two boundaries.

```
sensor code values
  -> black/white level, linearise           (aura-raw, phase 02)
  -> demosaic                                (aura-raw, phase 02)
  -> highlight recovery                      (phase 14, before white balance)
  -> camera matrix, Bradford to D65          (aura-raw::colour::working_space)
  === WORKING SPACE: linear Rec.2020, D65 ===
  -> white balance (temperature / tint)
  -> exposure
  -> tone (highlights / shadows / whites / blacks)
  -> curve
  -> HSL
  -> clarity / texture / dehaze
  -> vibrance / saturation
  -> black and white mix
  -> local masks (phase 18 fills them; the slot exists here)
  -> retouch operators (phases 20-22 fill them; the slot exists here)
  -> geometry (phase 23 fills perspective; crop and rotate ship here)
  === OUTPUT TRANSFORM: the only place tone is baked ===
  -> sRGB | Adobe RGB | Display P3, 8 or 16 bit
```

Three consequences are enforced in code rather than remembered.

**Highlight recovery happens before white balance and before the camera matrix.** A clipped
green channel reconstructed *after* a 1.9x red multiplier is a magenta veil, which is the
single most visible failure a wedding renderer can have. `colour::highlight_recovery` runs
on camera-native RGB with the per-channel clip points it was given, and
`tests/eval/render_eval.rs::highlight_recovery_runs_before_white_balance` asserts the stage
order rather than the outcome, because the outcome is what the order produces.

**The output transform is the only tone curve.** No stage before it applies a display
transfer function; `output.rs` is the only module in `aura-render` that calls
`encode_srgb`, `encode_gamma_2_2` or writes 8-bit samples. A grep for those names finding a
second file is the regression this rule exists to catch, and
`only_output_encodes_a_transfer_function` is that grep as a test.

**Wide gamut, not sRGB, inside.** Rec.2020 primaries, because a stage light or a red sari
clipped into a hue shift before the grade starts is detail the sensor really recorded and
the product threw away. This is phase 02's decision inherited, not a new one.

## 3. Decision: precision, and why this build is f32 rather than f16

Section 6.1 of the phase document specifies 16-bit half floats on the GPU. This build
computes in **`f32` throughout the CPU reference** and documents f16 as a GPU storage
format rather than a compute format.

The reason is the parity gate. Section 6.2 requires GPU and CPU to agree within 1/1024 in
linear light per stage. An f16 has about 11 bits of mantissa, so a single f16 rounding is
already 1/2048 relative - two of them in sequence can exceed the tolerance the gate is
written against, and a gate that fails on its own storage format teaches people to widen
gates. So: **f16 for textures, f32 for arithmetic**, which is what every wgpu backend does
anyway (`f16` in WGSL is a storage type; the ALU is 32-bit unless `shader-f16` is enabled).
The tolerance in `parity::TOLERANCE` is stated in linear light and holds for both.

## 4. Decision: there is no GPU backend in this build, and the port is real anyway

`aura-render` ships the CPU reference pipeline, the render graph, the tiler, the parity
harness and the WGSL sources. It does **not** link `wgpu`.

This is the same decision, for the same reason, that ADR-0004 made about LibRaw and ADR-0007
made about ONNX Runtime: the toolchain on the build machine has no Windows SDK, `wgpu`
pulls a large native dependency tree that cannot be verified here, and a backend that cannot
be run cannot be tested - and an untested renderer is worse than an absent one, because it
looks like a shipped feature.

What ships instead is the shape that makes the backend a later addition rather than a later
rewrite:

- `GpuBackend` is a **port**, deliberately not frozen, exactly as `aura_infer::Backend` is.
  A wgpu implementation adds a file and touches no caller and no ADR.
- The WGSL sources in `crates/aura-render/shaders/` are the *specification* of each stage,
  not decoration. `shaders::SOURCES` is compiled into the binary and
  `tests/shader_parity.rs` asserts that every stage in `graph::Stage` has a shader and that
  each shader's declared uniform block matches the Rust `StageParams` field-by-field. A
  shader that drifts from the reference fails the build today, before a GPU exists to run it.
- `RenderCaps::backend` reports `Cpu` and `render()` raises `AURA-RENDER-8001` at
  `Severity::Degraded` on every request whose purpose is `Interactive`, once per session.
  Invariant 9: the absence is a typed error with a fallback and a telemetry event, not a
  silence.

**The consequence for the budgets is recorded rather than hidden.** Section 11's five rows
all name a GPU. Four of them are waived in `perf/budgets.toml` with this ADR named as the
waiver, and the fifth - the CPU fallback at <= 6 s for a 45 MP render - is asserted, because
it is the path this build actually runs. The waiver expires when a GPU backend lands, which
is the same expiry ADR-0007 carries.

## 5. Decision: `render_hash` covers four things, and canonical JSON is why it is stable

`render_hash` is BLAKE3 over, in this order:

1. the RAW's `content_hash` (phase 01),
2. the recipe's **canonical** JSON bytes,
3. the engine version string, `aura-render/1.0.0`,
4. the output spec's canonical form (colour space, bit depth, ICC name).

Canonical JSON is defined in `aura_recipe::hash` and has exactly three rules: keys sorted
byte-wise, no insignificant whitespace, and **every float formatted as `{:.6}`** with the
sign of negative zero removed. The third rule is the one that matters. `serde_json`'s float
formatter is shortest-round-trip, which is correct and platform-stable today; six fixed
decimals is stable against a future change to that algorithm, against a different
serialiser on the other side of an XMP round trip, and against the difference between a
value a human typed and the same value after one f32 round trip. The cost is that recipe
parameters are quantised to a millionth, which is four orders of magnitude finer than any
control in the product.

`render_hash` is stored with every export so a delivered file can always be re-created, and
`AURA-RENDER-8007` is what a mismatch raises.

## 6. Decision: `user_edited_fields` is enforced in the merge, and the merge is the only way in

Section 6.4 says AI passes must never overwrite a field a human touched. The rule is not a
convention here: `Recipe` has no public mutable field access outside its own crate, and the
one function that changes a recipe from an automated pass is
`aura_recipe::schema::merge(base, proposal, source)`.

When `source` is `EditSource::Ai` or `EditSource::Qc`, `merge` skips every path listed in
`base.provenance.user_edited_fields` and returns them in `MergeReport::refused`. When
`source` is `EditSource::User`, the touched paths are *added* to that list. There is no
argument that switches the protection off, and `AURA-RENDER-8006` is raised when an
automated pass proposes a change to a protected path so the refusal is visible rather than
silent.

`MergeReport` also carries `changed`, which is what phase 27 will need to write a diff into
the ledger, and what `history.rs` records as one undo step.

## 7. Decision: the recipe is versioned forward, and fields are deprecated but never removed

`schema: 1` today. `migrate.rs` holds one function per version step and
`migrate::to_current` applies them in order. Three rules:

- **A field is never removed.** A v2 that drops `clarity` must keep reading it and mapping
  it, because a v1 recipe in a photographer's catalog from 2026 has to render the same way
  in 2029. `migrate::DEPRECATED` is the list, and it is only ever appended to.
- **An unknown field is preserved, not dropped.** `Recipe::extra` is a `BTreeMap<String,
  Value>` that round-trips anything a newer build wrote. A v1 engine reading a v2 recipe
  raises `AURA-RENDER-8003`, renders what it understands, and writes the document back
  intact - so opening a project in an older build does not destroy work done in a newer one.
- **A migration is tested against a frozen document, not against itself.**
  `crates/aura-recipe/tests/fixtures/recipe_v1_golden.json` is a byte-frozen v1 recipe;
  `migrate_v1_renders_identically_under_a_simulated_v2` migrates it and asserts the render
  hash of the result equals the render hash of the original. That is section 10.1's last row.

## 8. Decision: XMP carries the subset Lightroom understands, and the sidecar carries the rest

Two files, and the split is not negotiable.

`image.xmp` holds the twenty-two parameters Adobe's `crs:` namespace defines with the
meaning we mean - exposure, contrast, the four tone sliders, temperature and tint, clarity,
texture, dehaze, vibrance, saturation, the eight-band HSL, the point curve, sharpening and
noise reduction, crop and angle. A photographer who hands the folder to a Lightroom user
gets an edit that opens.

`image.aura.json` holds the whole recipe, byte-for-byte canonical. Masks, retouch
operations, restoration, provenance, `user_edited_fields` and the decision id have no
Adobe equivalent that means the same thing, and writing an approximation into `crs:` would
be worse than writing nothing: a lossy round trip that *looks* lossless is how a
photographer loses a mask and never finds out.

Reading is the mirror. If both files exist, the sidecar wins and the XMP's Lightroom-subset
values are compared against it; a difference means the photographer edited in Lightroom, and
`xmp::reconcile` returns those fields as user edits so section 6.4's protection covers them.
`AURA-RENDER-8004` is raised when the XMP is unreadable and the sidecar is used alone.

## 9. Decision: the error domain

Phase 14 opens `AURA-RENDER-8001` to `AURA-RENDER-8010`. The 8000-8999 block was reserved in
`crates/aura-core/tests/error_registry.rs` under the name `EXPORT` and carried no codes; it
is renamed here rather than a new block being invented, because an export is a render
written to a file and phase 30 will want codes in the same domain for the same subject.

| Code | What it says |
|---|---|
| `AURA-RENDER-8001` | No GPU backend; the processor reference path rendered this. |
| `AURA-RENDER-8002` | The recipe is not valid and was refused. |
| `AURA-RENDER-8003` | The recipe is from a newer schema; the understood part rendered. |
| `AURA-RENDER-8004` | The XMP sidecar could not be read; the AURA sidecar was used. |
| `AURA-RENDER-8005` | The AURA sidecar could not be read and the edit was not recovered. |
| `AURA-RENDER-8006` | An automated pass tried to change a parameter a person set. |
| `AURA-RENDER-8007` | A re-render did not reproduce the hash the export recorded. |
| `AURA-RENDER-8008` | The camera profile is unknown; the reference profile was used. |
| `AURA-RENDER-8009` | The render would not fit in the memory budget; it was tiled. |
| `AURA-RENDER-8010` | The processor and accelerator paths disagreed beyond tolerance. |

## 10. Consequences

**Good.** One renderer, one document, one hash. Phases 15 to 26 write parameters into a
shape that already exists and is already tested. A delivered JPEG can be re-created from
four values. A photographer's slider survives every autonomous pass in the product, by
construction rather than by care.

**Bad.** No GPU backend means the interactive budget of 60 ms at 2048 px is not met by this
build; the measured CPU figure is in the exit report and the row is waived here. Golden
renders are authored synthetic mosaics rather than twelve real camera bodies, because
phase 02's condition - no real camera files in the repository - is still open, and the
goldens are therefore a determinism and regression gate rather than a claim about colour
accuracy. Both are named as conditions in `docs/progress/PHASE-14-EXIT.md`.

**Ugly.** `f32` on the CPU reference and `f16` storage on a future GPU means the parity gate
will be doing real work the day a backend lands, and some stage will fail it. That is the
gate working. The tolerance is not to be widened to make it pass; the stage is to be fixed,
or the stage is to be marked `Stage::PRECISION_SENSITIVE` and computed in f32 on both sides.
