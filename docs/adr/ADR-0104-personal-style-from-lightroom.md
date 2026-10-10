# ADR-0104: A personal style learned from the photographer's Lightroom catalogue

Status: accepted.
Date: 2026-10-08

## Problem

Imagen and similar products sell one thing above all: a "personal AI profile" learned from the
photographer's own past edits, which then edits new weddings the way that photographer would.
AURA had the parts without the product:

- Phase 17's `StyleService` learns from original/final pairs, but it is not connected to the
  Studio, and it needs both files of every pair.
- `edit_profiles` applies researched and FiveK-learned looks over AURA's measured correction,
  with scene guards and manual-edit protection, from the start screen, the Studio and the
  whole-folder run. A learned FiveK profile is one constant offset per slider.

Most photographers do not have tidy before/after pairs. They have a **Lightroom Classic
catalogue**: a `SQLite` file holding every photograph's develop settings. The originals may or
may not still be on the disk the catalogue points to. The reference catalogue on this machine
(566 photographs, 538 RAW) has its settings and its exported finals but not its originals.

Two questions decided the design.

1. **Is one fixed value per slider good enough?** Measured on MIT-Adobe FiveK, expert C, leave-one-
   out over 32 RAW pairs (27 clean enough to train on), mean ΔE00 to the retoucher's final:

   | Method | ΔE00 |
   |---|---|
   | AURA's automatic correction alone | 10.56 |
   | plus one constant offset per slider (median) | 10.20 |
   | plus 7 nearest neighbours in feature space | 8.48 |
   | plus ridge regression, λ = 0.4·n | **8.18** |
   | the per-photograph fit itself (unreachable ceiling) | 3.55 |

   A constant closes 5 % of the gap between the automatic correction and the ceiling. A ridge
   regression on nine numbers AURA already measures closes 34 %. Retouchers do not use one value
   per slider; they recover a bright sky harder than a dull one. `crates/aura-app/tests/style_adaptive.rs`
   reproduces the table.

2. **What in a catalogue is taste, and what is the photograph?** A slider value mixes the two. A
   median Exposure of +0.34 says as much about how dark the camera was set as about how bright
   the photographer likes a picture, and Highlights −90 includes the recovery that particular
   sky needed - which AURA's own correction measures and adds underneath any profile. Adding the
   raw median on top would do that part twice.

## Decision

### Reading the catalogue (`aura_app::lightroom`)

The `.lrcat` is copied to a temporary file and opened read-only, so a running Lightroom is never
blocked and the catalogue is never written. One query joins `Adobe_images`, the file, folder and
root-folder tables and `Adobe_imageDevelopSettings`. The settings are a Lua table; a small
recursive-descent parser reads it into JSON, and only its **top level** is used, because nested
tables hold the camera profile's own look, masks and spot removal - a number in there is not a
slider the photographer moved.

### Learning (`aura_app::personal_style`)

Only photographs edited in process 2012 or later count, and RAW edits are preferred when there
are at least ten. Then two halves, kept apart on purpose:

- **Taste, from the settings alone.** The median of contrast, highlights, shadows, whites,
  blacks, texture, clarity, dehaze, vibrance, saturation, the eight HSL bands, the point and
  parametric tone curves, the per-channel curves, colour grading, calibration, grain, sharpening,
  noise reduction, the post-crop vignette and a majority black-and-white mix. Each finding is
  written as a sentence with how consistently it is used.
- **The photograph, left to measurement.** White balance is never learned: it is the day's light.
  Exposure is kept at half its median, and only when at least 60 % of photographs agree.
  Contrast, highlights and shadows are kept at half their median **when the originals are not
  available**, because the other half is what AURA measures per photograph.

**When the originals are on disk**, up to 400 of them, spread evenly through the catalogue, are
opened read-only and measured exactly as a whole-folder edit measures them: the first-tier preview,
`AutoCorrection` and `SceneStats`. Then, for each of exposure, contrast, highlights, shadows, whites
and blacks:

- The photographer's own part is their slider **less what AURA's correction does on that
  photograph** - the exact split, replacing the half rule.
- A ridge regression (λ = 0.4·n, standardised features clamped to ±3) fits how that part follows
  nine measurements: three luminance percentiles, warmth, chroma, and the four parts of the
  automatic correction.
- **Five-fold held out**: the model is kept for a slider only when it comes at least 5 % closer to
  the photographer's own value than one fixed value does, on photographs the fit never saw. The
  gain is stored on the profile and shown as a finding.

The result is an ordinary `EditProfile` with `origin: "personal"` (which, like `researched`, must
name its sources) and an optional `adaptive` model. At least 30 measured originals are needed for
a model.

### Applying it

`edit_profiles::build` moves the six tonal sliders by the model's prediction - bounded to ±0.75
stops and ±35 points however unusual a photograph is - **before** every scene guard, so the guards
still soften a look on a frame that cannot take it, and manual-edit protection still refuses to
overwrite anything the photographer set. The adaptation is reported as a sentence beside the
guards' own. Only a personal profile may carry a model; validation refuses one anywhere else.

Personal profiles are stored as JSON in the catalogue's `setting` table under
`personal_profiles_v1`, at most 24, and are re-validated when read. `edit_profiles::resolve` finds
them by their `personal-` prefix, so the Studio presets, the start screen, reference-look matching
and the whole-folder run all apply them through the same path as every other profile.

### Surface

Two commands, `learn_lightroom_style` and `delete_personal_style`; `list_edit_profiles` now reads
the catalogue and lists personal profiles first. The start screen's profile gallery gains "Teach
AURA your style": a name, a file picker filtered to `.lrcat`, and the findings. A personal profile
is badged "Yours" and can be deleted.

## What this does not claim

- Lightroom's sliders and AURA's are close in intent, not identical in effect. Contrast +20 in
  Lightroom over Adobe Standard is not the same pixels as contrast +20 in AURA. The profile
  reproduces the photographer's habits, not their exact output.
- The FiveK table is 27 pairs from one retoucher. It says a per-photograph model is worth having;
  it is not a measurement of any particular photographer's profile.
- On the reference catalogue the originals are absent, so it learns the settings-only profile:
  learning 538 photographs takes under a second. Its exported finals (in Google Drive) cannot
  substitute for originals.
- Local adjustments - masks, brushes, spot removal - are not learned. Neither is cropping.
