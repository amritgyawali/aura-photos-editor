# AURA-ML-5072 - One archive pair could not be matched, read or fitted

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The "Teach My AI" report says `1,842 of 2,000 pairs used, 158 left out`, with a per-bucket
breakdown of where the rejections were. The training run itself finishes normally.

## What actually happened

PHASE-17 section 6.1 is explicit that a pair the fitter cannot reproduce must be **rejected
rather than down-weighted**: a residual the develop pipeline cannot express is unmodelled
work - a local dodge, a composite, a heavy crop, a sky replacement - and fitting it anyway
puts that work into a global tone delta. This code fires once per rejected pair. There are
four causes and the reason on the pair says which:

1. `pair_unmatched` - no original was found for a final, by hash, stem, capture time or
   perceptual match. Usually a final exported from a wedding whose RAWs are on another disk.
2. `fit_residual_too_high` - an original and a final were matched, and no setting of the
   twelve fitted parameters reproduces the final within `aura_style::fit::REJECT_DE00`.
3. `unmodelled_work` - the fit's residual is concentrated in a region rather than spread over
   the frame, which is the signature of local retouching.
4. A read failure: a truncated JPEG, an unreadable sidecar, a RAW format this build does not
   decode (see `docs/camera-support.md`).

## Operator steps

1. Look at the rejection histogram in the profile report. A run at 90 % acceptance is healthy
   and needs nothing.
2. Below about 60 %, check cause 1 first - it is nearly always a folder pairing problem and
   nearly never a fitter problem. `aura-cli verify --phase 17` prints the same breakdown.
3. Rejections concentrated in one bucket are informative rather than alarming: a photographer
   who retouches every portrait and no detail shot will see exactly that, and the profile is
   correct to learn only the global look from those frames.
4. A rejection rate above 95 % with cause 2 dominating means the archive's finals were not
   produced from those originals - a different export, a different generation of the catalog -
   and the honest answer is to point the scan at a matched pair of folders.
