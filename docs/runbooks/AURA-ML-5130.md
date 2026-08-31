# AURA-ML-5130 - One project's camera matching pass could not run

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The cameras at this wedding were not matched to each other. Every photograph still has the edit it
was given on its own, and nothing has been lost.

## What actually happened

`MatchingPass::run` returned before it wrote anything. Four ways to reach it:

1. **No photograph in the project names a camera.** The pass groups frames by body and flash state,
   and an empty grouping is a project whose `photo` rows never joined to a `camera` row. Note that a
   body whose *serial* could not be read is not this case: it becomes `CameraId::UNKNOWN`, which is
   a real value that fingerprints and matches like any other.
2. **No camera shot a measurable photograph**, so no reference could be chosen. A measurable frame
   is one that carries phase 15's temperature, phase 15's illuminant chromaticity and phase 05's
   descriptors.
3. **The run was cancelled** during fingerprinting or matching.
4. **A solved transform came out outside its own bounds**, which is a solver defect rather than a
   data problem and is refused before it is written. Migration 26's CHECK constraints are the second
   layer under the same rule; this is the first.

## Why it retries rather than falling back

There is no fallback. Without this pass every body keeps exactly the colour science it came with,
which is the state the product was in before phase 26 existed - a gallery that looks like two
weddings, and not a broken one. A caller that rendered that state as "matched" would be making a
claim nobody measured, which is why the recovery is `retry` and not `fallback`.

## Fixing it

The detail line says which of the four it was.

For (1) and (2), run the phase 05 embedding pass and the phase 15 tone pass over the project first -
this pass reads what they stored and opens no pixels of its own. `camera_status` reports `photos`
against `matched`, and a coverage near zero is this.

For (4), the detail names the camera. It should be unreachable; if it is not, the policy table and
the frozen contract disagree about a ceiling, which `AURA-ML-5133` normally catches at load.
