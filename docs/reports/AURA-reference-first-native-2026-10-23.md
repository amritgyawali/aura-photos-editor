# Main Match a look native fitting — 2026-10-23

Status: implemented and verified for the cases below. Universal accurate colour
transfer from an edited reference alone is **not established**.

This report covers the main **Match a look** screen shown in the request:
`InstagramStyle.tsx` → `referenceStyle.analyse/apply` → `reference_style.rs`.
The screen now accepts a single finished JPEG, PNG or TIFF, alongside the
existing folder and Instagram-export workflows.

## Implementation

The previous three-strength initial-delta probe is replaced with bounded,
per-photo render fitting. It explores exposure, white balance, tone controls,
individual curve anchors, supported colour bands and three-way colour casts.
The fitting frame comes from the native source decode; native-resolution output
determines final acceptance. See [the workflow guide](../reference-first-matching.md)
and [ADR-0140](../adr/ADR-0140-reference-first-native-fitting.md).

Recipes are merged with manual-field protection and canonicalised before
candidate evaluation. Identical repeated application does not compound the look
or create history from unpersisted floating-point differences. Zero strength
restores the automatic or selected-profile baseline while preserving manual
fields and retained retouch. It does not return an untouched camera rendering.

Native verification rejects increases in the selected distance objective,
excess clipping and violations of sampled detected-person skin guards. Saving
checks the starting recipe hash transactionally. Batch Stop propagates to the
native search; cancellation before saving leaves the current photo unchanged.

## Controlled native tests

`reference_style_native.rs` builds a known reference with the actual renderer
from a 128 × 96 synthetic colour scene. The reference includes contrast, white
balance, blue shadows and warm highlights. Mean absolute RGB byte error against
that known output improves from **4.593 to 0.710** on a 0–255 channel scale.
This measures one known grade; it is not an accuracy percentage or proof of
generalisation to different scenes.

The same integration case verifies exact repeat pixels, no additional changed
fields on repetition, zero-strength baseline restoration, manual exposure
protection and unchanged original bytes. A monochrome reference generated from
the same baseline is reproduced exactly in this fixture. A separate test cancels
an active registered fit and verifies unchanged recipe and history.

The RAW regression uses identical synthetic sensor data with substantially
different embedded JPEGs. Both targets receive identical saved global reference
settings. This demonstrates independence from embedded previews in that fixture;
it does not qualify every camera or input profile.

## Existing local photographs

The optional native review test processes two existing local target preview PNGs
from `output/aura-internet-reference-2026-10-22/ui-qualified`, with the existing
`warm-cinematic.jpg` and `vibrant-teal-orange.jpg` references. It uses the main
analysis/apply APIs, 80% requested strength, native preview output and isolated
catalogues. Target and reference bytes remain unchanged.

| Case | Native appearance distance before | After | Changed recipe fields |
|---|---:|---:|---:|
| Outdoor portrait / warm reference | 25.556 | 21.514 | 1 |
| Indoor portrait / teal-orange reference | 25.304 | 22.212 | 1 |

View the [reference / baseline / result comparisons](../../output/aura-reference-first-native-2026-10-23/index.html)
and [machine-readable results](../../output/aura-reference-first-native-2026-10-23/results.json).
The outputs were visually inspected. They are conservative changes and do not
reproduce the strong reference effects. These references have different scenes,
so an independent intended target grade is unknown. Lower descriptor distance
must not be interpreted as successful recovery of those editing operations.

## Verification

| Check | Result |
|---|---|
| `cargo check -p aura-app` | Passed |
| `cargo clippy -p aura-app --lib --no-deps -- -D warnings` | Passed |
| `cargo test -p aura-app --test reference_style_native --test one_click -- --nocapture` | 3 passed; optional review ignored in the regular run |
| `cargo test -p aura-app --test photo_pixels -- --nocapture` | 3 passed, including RAW preview independence |
| Optional `review_existing_photos_through_the_main_reference_workflow` with local paths | 1 passed, 2 photograph/reference cases |
| Main reference-screen, collection-preparation and autopilot UI tests | 25 passed |
| UI `npm.cmd run build` | Passed; existing large-chunk warning remains |
| Rust formatting and `git diff --check` | Passed |

The desktop installer/executable was not rebuilt or relaunched in this change.
The full repository test suite was not rerun. The separate experimental
distribution-transfer matcher still has the three known accuracy failures
documented in the [earlier reliability report](AURA-reference-grade-reliability-2026-10-23.md).
Those failures were not resolved or reclassified by the main-screen change.

Mixed illumination, clipped data, different object colours, camera response and
unsupported local effects remain limitations. An edited photo alone cannot
uniquely identify its original lighting and editing recipe. Original/edited
reference pairs and held-out camera/scene evaluation are needed for stronger
calibration and fidelity claims.
