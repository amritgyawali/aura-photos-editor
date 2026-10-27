# AURA exported colour verification — 2026-10-22

**The exported new photos do not establish the same colour, tone or calibrated white balance as the references.** AURA preserves its saved matched treatment through the tested export paths, but the treatment remains an approximation. No export-induced colour fault was found in these sRGB cases. This distinction is based on decoded delivered files, not just a preview or a successful export status.

I adjusted the existing AURA matching strengths and exported four selected matches as JPEG and lossless PNG. All four selections have a lower native regional appearance distance than their default match. Tone distribution gaps also decreased; chroma gaps did not necessarily improve. This is not an exact-grade or universal quality claim.

## Reference versus delivered result

Two original-resolution [warm](https://www.pexels.com/photo/cinematic-portrait-of-woman-in-warm-lighting-36672685/) and [fashion](https://www.pexels.com/photo/dramatic-portrait-of-a-woman-in-vibrant-fashion-29653811/) references from the [previous audit](AURA-reference-colour-match-2026-10-22.md) were reused. The four target tests use the same 682 × 1024 supplied-photo proxies and initial corrections, restored exactly before each fit.

| Reference / target | Native regional distance: default → selected | Tone quantile gap: previous JPEG → selected JPEG | Chroma quantile gap: previous JPEG → selected JPEG |
|---|---:|---:|---:|
| warm-cinematic / target-0.jpg | 18.176 → 16.615 | 23.773 → 21.524 | 9.866 → 10.783 |
| warm-cinematic / target-1.jpg | 18.869 → 17.346 | 18.566 → 14.350 | 11.716 → 12.361 |
| vibrant-teal-orange / target-0.jpg | 20.837 → 20.088 | 15.489 → 12.814 | 17.058 → 16.809 |
| vibrant-teal-orange / target-1.jpg | 23.449 → 22.419 | 8.696 → 4.550 | 21.907 → 18.506 |

Tone gap is mean absolute difference of Lab L* quantiles at 5/25/50/75/95%; chroma gap is the same for sqrt(a*²+b*²). These compare whole-image distributions and depend on subject matter, clothing and background as well as the edit. They do not isolate grading error or establish paired-scene equivalence. The machine report also records normalized a*/b* palette Jensen–Shannon distance (0 identical histograms, larger means more different), without treating it as a quality percentage.

**White balance: not verified as equal.** These unrelated photos contain no corresponding calibrated neutral target or known original WB recipe. The warm reference contains no qualifying bright near-neutral samples under the diagnostic rule. Near-neutral a*/b* cast differences for the other reference are recorded as descriptive proxies only; they are not Kelvin/tint recovery or proof of white-balance equality. Matching slider percentages refer to a fitted contribution and cannot be read as the reference's original WB settings.

## Applied matching strengths

| Reference / target | Amount | Tone | Colour | Warmth |
|---|---:|---:|---:|---:|
| warm-cinematic / target-0.jpg | 100% | 200% | 150% | 150% |
| warm-cinematic / target-1.jpg | 200% | 200% | 200% | 200% |
| vibrant-teal-orange / target-0.jpg | 100% | 200% | 100% | 50% |
| vibrant-teal-orange / target-1.jpg | 200% | 200% | 200% | 200% |

Each pair used read-only analysis followed by coordinate trials of 50%, 150% and 200% for tone, colour, warmth and amount, retaining a candidate only when its full-resolution regional descriptor improved. Baseline and selected-final applications are included in **56 applications: 45 accepted, 11 explicit safety refusals**. Refusals kept the recipe and history unchanged. All candidates started from the same immutable analysis baseline; they did not compound. This is a bounded search of existing AURA controls, not proof of a global optimum or a change to the native fitting algorithm.

## Does export preserve the selected treatment?

Every new delivery used AURA's real `export_run`: sRGB, 8-bit, original dimensions, no output sharpening, JPEG quality 95 or PNG. Read-back verification was enabled. Images were independently decoded with Pillow, embedded ICC profiles converted to sRGB, and compared against the saved full export-purpose render of the **same target and recipe**. Delivered PNGs carry an sRGB chunk, while JPEGs carry ICC profiles; both were confirmed in a post-run metadata check. Only genuinely untagged inputs were assumed sRGB. EXIF orientation was applied before comparison. Source hashes and geometry/retouch/mask preservation were checked.

| Selected match | JPEG mean CIE76 vs render | JPEG p95 CIE76 vs render | PNG vs render |
|---|---:|---:|---|
| warm-cinematic / target-0.jpg | 0.9863 | 2.3749 | exact |
| warm-cinematic / target-1.jpg | 0.9232 | 1.8305 | exact |
| vibrant-teal-orange / target-0.jpg | 0.9235 | 2.0767 | exact |
| vibrant-teal-orange / target-1.jpg | 0.9657 | 1.9496 | exact |

All four PNG exports are pixel-identical after colour-profile interpretation. JPEG differences are nonzero and within the declared test limits (mean CIE76 <2 and p95 <5), with median L* shift zero and small signed a*/b* shifts recorded in JSON. Those limits are comparison thresholds for this test, not calibrated perceptual guarantees. Export did not change the saved recipe; all eight recipe sidecars equal the exported recipe. AURA's existing sidecars contain canonical recipe JSON with an `.xmp` suffix; Adobe/Evoto sidecar interoperability is not claimed.

The four previous gallery JPEGs were also decoded and compared to their saved renders: mean CIE76 ranged 1.0809–1.1519. The earlier reference/result differences were already present in the matched edit, rather than being introduced mainly by JPEG delivery.

## Original camera file export

`H:\AURA EDIT\989A2865.CR3` was matched to the same downloaded warm reference in an isolated copy of the saved 27-photo catalogue, preserving its existing geometry, retouch and masks. The full decode and both delivered files are **4480 × 6720**, rather than embedded-thumbnail dimensions. Accepted match amount: 100%. Native-resolution guards passed; internal regional descriptor 19.837 → 18.584.

The exported PNG equals the native export render at **every pixel**: True. JPEG quality 95 has mean CIE76 1.8394, p95 3.6656, measured on an explicitly sampled 4-pixel grid without resizing. Both delivery hashes and recipe sidecars were verified. Export did not change the recipe. RAW source SHA-256 remained `b0c804647a3b066772ad6fb8f056b0c73894d513a3b7265658ca33a66a28e5ae`.

The [full-resolution matched JPEG](../../output/aura-export-colour-2026-10-22/989A2865-reference-matched-full.jpg) is copied byte-for-byte into the review folder. Native PNG, sidecars and evidence are in `F:\AURA-verification\aura-export-colour-raw-2026-10-22` (F: was used for the larger artifacts). Gallery previews were resized only for display from those actual exported files. Calibrated white balance and exact reference-grade equivalence remain unverified for this RAW too.

## Artifacts and checks

- [View the actual exported JPEG comparisons](../../output/aura-export-colour-2026-10-22/index.html), including reference, before, previous export and selected export. The PNG and recipe links allow lossless inspection.
- [Machine report](AURA-exported-colour-verification-2026-10-22.json) and [four-case CSV](AURA-exported-colour-verification-2026-10-22-cases.csv).
- Desktop executable SHA-256: `d77e7351c4e736681ab3f1afb79a343588afd0a94a55f031f405b4b723b0f2c5`; no production source changes or new desktop build were needed for this verification. The previously tested native matcher and export pipeline performed all edits and deliveries.
- Primary command metadata: `.work-checks/aura-export-colour-2026-10-22/exported-colour.json`; additional RAW command metadata, when included, is alongside it.
- Independent metric tests exercise sRGB black/white/red Lab conversion, identical pixels, colour casts, dimension rejection, symmetric palette gaps, PNG sRGB tags and missing neutral evidence. All 22 script tests passed. The full frontend/native suites were not rerun for these Python-only verification additions.
- The metric helper gained an identical-pixel fast path for native RAW memory use after the preview run, then more precise PNG-tag reporting. Neither changes the measurement formula or image pixels. Recorded run hashes remain intact; the exact preview-run helper is retained in `.work-checks/aura-export-colour-2026-10-22/metrics-at-preview-run.py`.
- Exact reference-grade cloning, reference-scene lighting reconstruction, equal calibrated WB and Evoto equivalence remain unproved. A lower aggregate distance can coexist with a larger individual chroma gap; the report retains both.
