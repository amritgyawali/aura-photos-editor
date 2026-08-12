# ADR-0003 - The colour pipeline: working space, transfer function and profiles

- **Status:** accepted
- **Date:** 2026-08-12
- **Deciders:** COL (Colour Scientist), CTO, MLL (ML Lead - Vision)
- **Phase:** 02

> The phase 02 document calls this file `ADR-0002-colour-pipeline.md`. That number
> was already taken by the phase 01 toolchain decision, so the colour ADR is
> numbered 0003. Nothing else about it differs from what section 8 step 3 asks
> for: the colour path is decided, written down and signed before tier 2 exists.

## Context

Preview quality decides AI quality. Every model in phases 05 to 22 is trained on
the pixels tier 2 produces, so the rendering is not a preference that can be
tuned later - it is a contract, and changing it invalidates trained models.

Three things had to be decided before a line of tier 2 was written: what space
the numbers live in, what curve maps scene to display, and where the camera
matrix comes from.

## Decisions

### 1. The working space is linear Rec.2020, D65

Scene-referred linear light, Rec.2020 primaries, D65 white, no transfer
function. Implemented in `crates/aura-raw/src/colour/working_space.rs`, and named
`WORKING_SPACE` so that a grep finds every conversion.

sRGB primaries were rejected: a saturated stage light, a red sari or a deep blue
uplighter falls outside sRGB, and clipping those into gamut *before* the grade
starts destroys information the colourist cannot get back. ACES AP0 was rejected
as overkill for a 2048 px proxy - it would cost precision in the 16-bit buffer
for a gamut no wedding contains.

### 2. Tier 2 is written twice: 8-bit sRGB and 16-bit linear

Both buffers are produced together, cached together and invalidated together;
`ProxyPair` in the frozen contract makes it impossible to have one without the
other.

An 8-bit JPEG cannot be re-linearised without losing the highlight detail that
white-balance and exposure models need. A model that only ever sees the 8-bit
buffer would learn to compensate for a curve, and phases 15 and 16 would be
tuning against that compensation rather than against the scene.

The 16-bit buffer keeps four stops of headroom above diffuse white
(`LINEAR_HEADROOM = 4.0`), so a specular highlight on a ring or a dress remains
representable rather than clipping to the same value as diffuse white.

### 3. The preview curve is deliberately dull

`filmic_lite`: linear below a knee at 0.6, then a soft exponential shoulder that
approaches 1.0 asymptotically and never clips.

- **No toe.** Crushing shadows destroys the separation that face detection and
  frame-integrity models need in a dark ceremony.
- **Continuous slope at the knee**, so no gradient shows a seam.
- **No automatic brightness.** LibRaw-style auto-exposure is off. The mapping
  from scene to display is a pure function of the pixel and nothing else, which
  is what makes two runs, two machines and two camera brands agree.

The curve is invertible on `[0, 1)`, and a test proves it: phase 15 needs to know
how much headroom a preview still has.

### 4. Every camera-baked look is disabled

No picture style, no film simulation, no maker-specific tone curve. A Canon and a
Sony pointed at the same altar must produce the same numbers, or phase 26 cannot
match bodies and phase 17 learns the camera instead of the photographer.

This is why `PixelSource` travels with every buffer: when tier 2 has to fall back
to the camera's own JPEG (see ADR-0004), the pixels carry the camera's look, the
sidecar says so, and any score derived from them is marked.

### 5. Profiles have exactly three sources, in this order

1. **The file.** DNG `ColorMatrix1`/`ColorMatrix2`, measured by whoever wrote the
   file. Nothing bundled can beat that.
2. **The bundled table**, signed off by COL against a ColorChecker frame shot on
   that body.
3. **The generic matrix** - sRGB primaries under D50 - which raises
   `AURA-RAW-2006`, sets `profile=generic` in the sidecar and shows a badge in
   the UI.

**No invented matrices for real cameras.** An unsigned profile cannot ship, so
the bundled table currently contains only the eight synthetic bench bodies whose
matrices are exact by construction and which the golden suite renders. A real
camera therefore takes path 1 when it shoots DNG and path 3 otherwise, until a
ColorChecker frame arrives and COL signs a profile. That is a smaller claim than
"we support 200 cameras", and it is a true one.

The white balance convention is worth stating because getting it backwards is
plausible enough to survive review: DNG's `AsShotNeutral` holds the *camera's
reading of a neutral subject*, so the multipliers the pipeline applies are its
reciprocals.

### 6. The tolerance is dE2000, and it is measured

Mean dE2000 <= 2.0 for the 24 chart patches on the proxy path, asserted for all
eight bench bodies by `the_colour_chart_survives_the_proxy_pipeline_on_every_bench_body`
and again by `aura-cli verify --phase 02`. The implementation of CIEDE2000 is
checked against the worked examples published by Sharma, Wu and Dalal.

Measured at the time of writing: worst body 0.158 mean dE2000.

### 7. `pipeline_ver` is part of every key

`PIPELINE_VER` is in the cache key and in the dataset key. Bumping it invalidates
cached proxies cleanly and forces a model re-validation run, and doing so
requires MLL sign-off. This is the mechanism that stops a colour change from
silently altering what the models were trained on.

## Consequences

- Phases 15, 16 and 26 can rely on a documented, reproducible rendering.
- Adding a camera profile is a process (frame, measurement, sign-off, version
  bump), not a code change.
- The generic-matrix path is visible in telemetry (`colour.profile_missing`), so
  the profile backlog is driven by what photographers actually shoot.
