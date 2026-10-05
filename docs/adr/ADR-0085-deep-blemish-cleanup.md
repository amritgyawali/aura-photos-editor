# ADR-0085: Native deep blemish cleanup

## Problem

The default portrait feature planner searches four small skin patches, limits repairs
to 24 spots, and protects dark marks. It leaves much of a dense acne portrait untreated.
A separate Python demonstration improved that example, but was not available from the
desktop's automatic retouch button.

## Implementation

`portrait_features/deep_blemish.rs` is an opt-in native Rust planner selected by the
**Deep acne cleanup** preset in Retouch. It replaces the sparse blemish planner for
that pass. It does not replace the input photograph or synthesize another face.

The planner samples the detected person's segmented face skin, fills only small
enclosed gaps, and then removes landmark eye, brow, nose, mouth and crease regions.
Three face-relative spatial scales measure dark, red and compact bright deviations
against local skin.
Connected components at three contrast thresholds are filtered by size, elongation
and distance from excluded pixels. Higher thresholds separate touching marks.
Duplicate proposals across scales are suppressed. Candidate spots and high-contrast
bright or dark defects are removed from the donor pool, and nearby donors are ranked
by local brightness and chroma.
Both repair disks and donor patches need clearance from protected areas; a further
downward mouth exclusion protects lower lips. Missing skin
segmentation or a suitable donor skips the repair.

Each accepted proposal is an ordinary native `PatchHeal` operation with an explicit
source, stable automatic ID and feathered boundary. The original matte is not applied
again to these fully bounded disks: doing so could exclude the dark blemish itself.
Existing texture transfer and harmonic tone matching render the repair at output
resolution. Automatic reruns replace automatic steps, and manual edits retain their
existing protection. The total recipe operation limit still applies.

If no full-size donor fits, the search tries successively smaller clean footprints.
The optional `sourceScale` recipe field defaults to 1 for old operations; the native
patch renderer reflects the smaller source at its original texture scale and matches
the target boundary with harmonic tone correction. Small repeated sources can show
texture repetition and are exposed as Source patch size in
the repair controls. It never relaxes the requirement that the source remain in skin.
The rendering and planning crates are optimized in desktop development builds so
dense repair stacks remain practical to preview.

Deep mode also adds a continuous frequency-separation finish after the spot repairs.
Its stored skin matte closes small internal holes, excludes landmark features, and
feathers inward. `refine_edges=false` prevents dark blemishes from punching new holes
in that explicitly protected selection. Other mattes retain the existing refinement
by default, including old recipes. The finishing blur normalizes by selected skin
coverage, so excluded eyes, lips, and hair do not contribute dark colour at boundaries.
Strength follows Smoothing and retained fine detail follows Texture; the saved finish
can also be adjusted as an ordinary Frequency separation step. Deep cleanup sets
`preserveMicrotexture=true`: a third, finer frequency band retains original pore
detail separately from the medium-scale irregularities. Texture no longer reduces
both bands together. The default preset retains 85% of the fine-band signal inside
the effect before opacity blending; this is a filter gain, not an accuracy claim.
Old operations default to the prior two-band behaviour. The explicit control is
stored with the operation and reusable tool presets. No random texture is added.
Texture missing from individual healed spots comes from the selected clean donor,
with the existing harmonic matching to the target illumination.

## Controls and compatibility

- `deepBlemishCleanup`: false by default, preserving existing behavior.
- `removeDarkMarks`: false by default. Explicitly enabled by the deep-acne preset;
  the interface explains that it can also remove freckles and beauty marks.
- `maxSpots`: up to 220 in deep mode; the original 24 limit remains in normal mode.
- Existing sensitivity, texture, tone, dodge/burn and hair controls remain adjustable.

Old saved options deserialize with the new fields disabled. Dense-pattern protection
remains available through Keep freckles. Dark-mark removal is a photographic choice,
not a medical diagnosis or reliable permanent-mark classifier.

## Validation

Unit tests cover enclosed mask holes versus open boundaries, uniform skin, long lines,
compact dark spots at different skin brightnesses, feature exclusions, clean donor
selection, dark-mark opt-in, deterministic planning, and actual native pixel repair
without changes outside the operation's selection. UI tests verify preset options and
the expanded spot limit. A renderer-backed surface test checks preserved pore contrast
and unchanged eyes, lips, background, and every zero-coverage pixel.
An independent multiscale signal test checks pore retention, reduced blotch contrast,
unchanged broad lighting and colour ratios, and neutral/legacy compatibility.
The manual photograph regression runner supports `deep` and
records the recipe and report alongside before/after pixels for inspection.

This is not evidence of 100% blemish recall or perfect skin selection. Dense overlapping
spots, facial hair, severe lighting, small faces and inaccurate landmarks remain limits.
