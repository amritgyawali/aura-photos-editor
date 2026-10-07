# Acne clear validation - 2026-10-07

What was run for [ADR-0092](adr/ADR-0092-acne-clear-and-the-blemish-brush.md), and what the
result looked like.

## The photograph

One real portrait: a stock photograph of an adult with dense acne on the forehead, nose,
between the brows and both cheeks, 2048 x 3072 (SHA-256 `4ac9e7fa...273f19`). A generated
portrait (`c1192553...4ddd9a`, the one ADR-0090 used) was checked in the lab as well.

## In the desktop application

A debug build of the native shell with the production UI bundled, an isolated catalog and
WebView profile. The scripts press the application's own controls and read back what it saved.

| Step | Script | Result |
|---|---|---|
| Reset photo, **Acne only · preserve detail**, Auto retouch: Face, export with verification | `verify-professional-retouch.py --resume --retouch-only --acne-only` | 1 operation (`acne_clear`), no spot repairs, 7.8 s; export verified, original unchanged |
| **Blemish brush**, three dabs on the nose and above a brow, Apply retouch | `verify-blemish-brush.py --at ... --radius 1.6` | 1 more operation (`acne_clear`, brush, no matte); brow kept |
| **Show retouched areas**, both views | same | teal selection and orange changed-pixel screenshots |
| Export with verification | Export panel | verified, 27.6 s |

Evidence is under `output/acne-clear/` (ignored by git): the exported JPEG, side-by-sides of
the face, the nose and the whole photograph, and the two retouched-areas screenshots.

## What it looks like

Looked at by the author of the change at 100 % against the original. One person's look at one
photograph; nobody else has judged it.

- The forehead, the space between the brows, both cheeks and the chin are clear of the red and
  brown marks at 100 %. The pores and the fine hairs are where they were.
- The nose: the marks on its bridge and lit side are gone; a darker smudge on the shadowed side
  of the bridge, beside the inner corner of the eye, is softened but still visible. It sits
  inside the eye guard, which keeps automatic steps away from the inner corner; the brush
  reaches it but this one is wide and in deep shadow.
- The shadowed temple and the jaw keep their shading; no pale patches.
- Brows, eyes, lips and nostrils are unchanged.

**Still visible:** that smudge; the original pitting of the cheeks (texture, not colour - acne
clear does not reshape relief); a little residual redness in a broad flush on the right cheek.

## Automated checks run locally

- `cargo test -p aura-render -p aura-recipe`: every test passed, including the ten painted-
  fixture tests in `acne_clear.rs` and the local-percentile unit test.
- `cargo test -p aura-app --lib`: passed, including the heal-selection and guard tests.
- `cargo clippy -p aura-recipe -p aura-render -p aura-app --all-targets -- -D warnings`: clean.
  `cargo fmt --all`: clean.
- UI: `tsc --noEmit` clean; the develop and IPC suites (211 tests) passed; production build
  succeeded.

## What this does not show

- Any other photograph of a real person, or any skin tone but these two, on real pixels.
- That a retoucher would agree with the result.
- Behaviour on a camera RAW file.
