# Model card - `<model_name>` `<version>`

> Copy this file to `docs/model-cards/<model_name>.md` and fill in every section.
> `cargo xtask models --check` fails if a card is missing or if any of the seven
> headings below is absent, and CI runs that check on every commit. A model with
> no card does not ship - Article VI rule M1 of the Engineering Constitution.
>
> Write it for the engineer who is debugging this model at midnight eighteen
> months from now, not for the person who trained it last week.

| Field | Value |
|---|---|
| Name | `<model_name>` |
| Version | `<major.minor.patch>` |
| Task | `<one line>` |
| Class | `embedding` / `segmentation` / `retouch` |
| Owner | `<agent role, e.g. MLL>` |
| Licence | `<SPDX identifier or "proprietary">` |
| Opset | `<n>` |
| Precision policy | `<any / no int8 / fp32 only>`, and why |

## Purpose

What decision this model informs, in the photographer's language. What it is
**not** for: the most useful sentence in a model card is usually the one that
stops somebody reusing it for a task it was never evaluated on.

## Architecture

Family, depth, parameter count, input shape, output shape and meaning. Name the
operators it uses that are near the edge of the runtime's supported subset - see
`docs/adr/ADR-0007-inference-runtime.md` - because those are what will break on
the next runtime change.

## Training data

Every source, with its licence, its consent scope and its permitted uses. How the
wedding-level split was enforced (Article VI rule M7: the same ceremony must
never appear in both training and evaluation). Row counts per slice: scene,
lighting, camera, and Monk-scale skin-tone bucket.

If the model is a placeholder with no training data, say so in one sentence and
say what will replace it.

## Latency

Measured, never estimated (Article XXIII rule AI7). One row per reference
machine, and the machine each number was actually measured on.

| Machine | Provider | Precision | Cold load | Per image | Batch throughput |
|---|---|---|---|---|---|
| RTX 4070 laptop (Win 11, 32 GB) | | | | | |
| M3 Pro MacBook (18 GB) | | | | | |
| Intel iGPU desktop (Win 11, 16 GB) | | | | | |

Unmeasured rows stay empty and are called out in the phase exit report. An
invented number here poisons every scheduling decision downstream.

## Quality gate

The metric, the threshold, and the date the threshold was agreed - **before**
training started (Article VI rule M2). Include the worst-decile figure, not only
the mean (rule M6), and the per-skin-tone-bucket spread for anything that touches
skin or colour (rule M4: within 1.0 dE00).

## Known failure modes

Where it is wrong, and how wrong. Confidence calibration: the expected
calibration error, and the confidence band below which the model abstains (rule
M9). Any input distribution it has not seen.

## Fallback

What happens when this model is unavailable, refused by the integrity chain, or
abstains. Every AI stage has a heuristic baseline; name it, and say what the
product loses when it is used instead.
