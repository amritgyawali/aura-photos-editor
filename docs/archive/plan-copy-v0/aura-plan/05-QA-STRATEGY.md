# QA Strategy

An AI application cannot be tested by asserting exact pixel values, and it cannot be shipped on vibes.
We test three different things with three different instruments: **correctness**, **quality** and **behaviour under stress**.

## The pyramid

- **Unit** - Pure functions, thresholds, scoring maths, serialisation round-trips, error taxonomy.
- **Property/fuzz** - Corrupt RAWs, truncated previews, absurd EXIF, 0-face and 60-face frames, 1-image and 6,000-image projects.
- **Golden image** - Frozen fixture set rendered and compared pixel-wise; dE2000 mean <= 0.5, max <= 2.0 unless intentionally changed and re-blessed.
- **Perceptual (human)** - QAIQ blind A/B against the previous build and against the named competitor for this feature; >= 60 % preference required.
- **Performance** - Throughput, wall clock, peak RAM, peak VRAM on the three reference machines.
- **Resume/kill** - Kill the process at 10 %, 50 %, 90 %; restart must continue without recomputation or corruption.
- **Regression** - Full previous-phase suite must stay green; no acceptance criterion from an earlier phase may regress.

## Fixture weddings (the backbone)

Three complete, consented, permanently versioned weddings live in `tests/fixtures/weddings/`:

| Fixture | Character | Why it exists |
| --- | --- | --- |
| `hindu_night` | Mixed tungsten and LED, heavy rituals, 3,200 frames, two bodies | Hardest white balance and ritual understanding |
| `daylight_church` | Bright windows, dark interior, formal groups, 2,400 frames | Extreme dynamic range and group logic |
| `nepali_reception` | Flash plus ambient, dance floor, high ISO, 2,800 frames | Restoration, motion, emotional peaks |

Every phase adds its own labels to these fixtures. Every quality gate is measured on them.

## Quality gates (examples, all enforced in CI)

| Gate | Threshold |
| --- | --- |
| Duplicate detection | recall 0.98, precision 0.95 |
| Face recall / identity F1 | 0.97 / 0.93 |
| Scene top-1 / ritual F1 | 0.92 / 0.85 |
| Blink F1 (intentional-closed false positives) | 0.95 (2 % or fewer) |
| Keeper agreement / missed must-haves | 0.85 / zero |
| Confidence calibration (ECE) | 0.05 or better |
| Exposure within 0.15 EV / WB within 200 K | 85 % of frames |
| Skin dE00 after grading | 3.0 or better, spread 1.0 or less across skin-tone buckets |
| Texture retention after retouch | 0.90 band-energy ratio |
| Permanent-feature false removal / tattoos | 2 % / 0 % |
| Identity drift after face recovery | below threshold, else operation skipped |
| Gallery WB spread reduction | 60 % or better |
| Per-identity skin dE00 spread across gallery | 2.0 or better |
| QC defect detection / auto-fix success | 90 % / 85 % |
| Cleanup artefact-free rate | 98 % |
| Crash-free session rate | 99.5 % |

## Perceptual and human review

- **Golden images:** reference renders with a perceptual difference threshold, not exact-pixel equality.
  Intentional changes require an explicit golden update in the pull request with a visual diff.
- **Blind studies:** for style match, retouch naturalness, restoration preference, curation agreement and
  camera matching, the QAIQ role runs blind comparisons with real photographers. These are release gates.
- **Zoom audits:** masks, retouch and cleanup are reviewed at 100 % zoom. Halos, bald patches and warped
  lines are bugs even when metrics pass.

## Chaos and endurance

Kill the process at 20 random points during a full wedding run. Sleep the machine. Unplug the drive.
Fill the disk. Reset the GPU driver. Revoke the API key mid-run. Every case must leave a resumable,
uncorrupted state with an honest message. Nightly CI runs a full 3,000-image wedding on real GPU hardware.

## Release gate

A release ships only when: all CI gates are green, the nightly long-run passed on all three reference
machines, no open blocker bugs, the perceptual golden set is clean, model cards are current, the privacy
document matches the code, and the CTO role has signed the checklist.
