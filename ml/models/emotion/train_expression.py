"""Train the expression head on labelled wedding faces.

PHASE-10 sections 6.1 and 8 steps 2-3, as a plan a reader can argue with and a script
that will run the day a labelled corpus exists.

    python ml/models/emotion/train_expression.py --plan
    python ml/models/emotion/train_expression.py --data faces.npz --out expression_head.pt

## Why there is a `--plan` mode

There is no labelled wedding data in this repository - condition **C1** of
`docs/progress/PHASE-10-EXIT.md` - so this script cannot be run for real here. What it can
do is state the training design precisely enough that the ML lead and the photographer
consultant can disagree with it *before* the labels section 9 gives `DATA` are collected
against the wrong problem.

`--plan` prints that design, and it is the same shape `train_focus.py --plan` and
`train_multihead.py --plan` take for the same reason.

## The design, in short

**Eight independent sigmoids, never a softmax.** Section 2.1: the outputs are "all
continuous, not one-hot". A face at a wedding can be laughing and crying at once, and a
loss that forced the eight to sum to one would teach the head to pick. That is the
modelling mistake which produces the emotionally-flat gallery section 1 names, and it is
prevented in the graph rather than warned about in a comment.

**The sampler is stratified by tradition and by scene, not by expression.** A uniform
sample of any wedding corpus this product is likely to be handed would be seventy percent
smiles, and a head trained on it learns that `composed` is what a face does when nothing is
happening. Section 12's first failure mode is cultural bias towards Western
expressiveness, and it enters through the sampler long before it enters through the loss.

**`tears` is trained with a precision-weighted loss.** Section 12's fourth failure mode:
"false tear/crying detection embarrasses the product". The operating point the product
needs is 0.90 precision at whatever recall that costs, so the positive class is
down-weighted rather than up-weighted - which is the opposite of what class balancing
usually does and is the right thing here.

**Wedding-level splits, never frame-level.** Two frames from the same burst three hundred
milliseconds apart are near-duplicates. A frame-level split puts one in train and one in
test and reports an accuracy that measures leakage. This matters more in phase 10 than it
did in phase 09, because a *moment* has more frames than a blur ladder has rungs.
"""

from __future__ import annotations

import argparse
import sys

# The eight channels, in `FaceExpression::channels` order. Reordering this relabels a
# trained head; `aura_core::contract::emotion` says so and this repeats it.
CHANNELS = (
    "smile",
    "genuineness",
    "laughter",
    "tears",
    "surprise",
    "tenderness",
    "composed",
    "discomfort",
)

# The crop the head reads, matching `aura_infer::onnx::fixtures::EXPRESSION_CROP_SIDE`.
CROP_SIDE = 112

# The traditions the sampler must balance across. `crates/aura-brain-wedding/config/rituals/`
# defines five; the ritual head knows eight. A corpus that covers three of them is a
# corpus that will produce a head with a cultural failure nobody measured.
TRADITIONS = ("hindu", "nepali", "muslim", "christian", "civil")

PLAN = f"""
Expression head - training design
=================================

INPUT
  {CROP_SIDE}x{CROP_SIDE} aligned RGB crop, `0..1`, produced by
  `aura_vision::face::align::warp_crop_from_eyes` - the SAME warp the product runs, from
  the SAME two eye landmarks. A training pipeline that used a five-point alignment and an
  inference path that used two points is a train/serve skew that looks like a modelling
  failure and is not one.

OUTPUT
  8 independent sigmoids, in this order:
    {', '.join(CHANNELS)}

LOSS
  Per-channel binary cross-entropy against continuous targets in 0..1, summed.
  Two per-channel weights, and both are product decisions rather than tuning:

    tears        x 0.6 on positives   - a precision-weighted loss. The operating point is
                                        0.90 precision; recall is what is spent to get it.
    composed     x 1.4                - the channel the cultural correction depends on,
                                        and the one a Western-sampled corpus under-teaches.

  No focal loss. The classes are not rare enough to need it, and a focal loss on `tears`
  would push exactly the wrong way against the precision weight above.

SAMPLING
  Stratified by (tradition, scene), NOT by expression. Target per-batch composition:
    - equal mass across {len(TRADITIONS)} traditions: {', '.join(TRADITIONS)}
    - within a tradition, equal mass across ceremony / portrait / reception
  Rationale: section 12's first failure mode enters through the sampler.

SPLITS
  By WEDDING. Never by frame, never by moment. 70/15/15, and the test set holds at least
  three weddings per tradition or the per-tradition metric below is noise.

LABELS
  Three sources, collected in three passes and never merged into one annotation task:
    1. per-channel continuous ratings from an expression annotator (this head's targets);
    2. the photographer's own keep/reject (a weak whole-face signal, used for calibration);
    3. pairwise "which of these two would you deliver" (the ranker's targets - see
       `train_ranker.py`).
  Section 9 gives DATA 10k of (3) across traditions. (1) and (2) are separate deliverables
  and conflating them produces a head that predicts delivery rather than expression.

AUGMENTATION
  Horizontal flip only, and the eight targets are unchanged by it.
  NOT colour jitter: `tears` is a specular wet cue and jitter destroys exactly the signal
  that separates it from cheekbone makeup.
  NOT rotation: the crop is already roll-normalised by the two-point warp, so rotating it
  trains the head on an input the product will never produce.

EVALUATION
  `eval_emotion.py --per-tradition` reports the eight channels' agreement separately for
  each tradition. A head at 0.90 on Christian weddings and 0.60 on Hindu ones has not been
  trained - it has been trained on the wrong thing, and shipping it would be section 12's
  first failure mode with a number attached.

EXPORT
  `ml/export_onnx` at opset 13, fp32 and fp16. **int8 is forbidden** and the manifest says
  so: eight sigmoids quantised per tensor lose their resolution near 0 and 1, and one of
  the eight is read against a 0.85 threshold that decides whether the product says the
  word "crying" to a photographer.

WHAT THIS DOES NOT TRAIN
  Gaze. It is geometry - phase 06's two eye landmarks against the face box - and it is
  measured in `aura_brain_wedding::emotion::gaze`. A gaze slot on this head would be a
  model guessing with authority about something that can be computed.
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true", help="print the training design")
    parser.add_argument("--data", help="an .npz of crops and eight-channel targets")
    parser.add_argument("--out", help="where to write the trained checkpoint")
    parser.add_argument("--epochs", type=int, default=30)
    args = parser.parse_args()

    if args.plan or not args.data:
        print(PLAN.strip())
        if not args.plan:
            print()
            print("no --data given, so nothing was trained. See condition C1.")
        return 0

    # The training loop needs torch, a GPU and a corpus, and this repository has none of
    # the three. Failing loudly with the reason is more useful than a stub that appears to
    # work: condition C1 is a Sev 2 trigger and a script that silently produced a
    # checkpoint from nothing would be how it stops being visible.
    print("expression head: training requires torch, a GPU backend and a labelled corpus.")
    print("None of the three is present. See docs/progress/PHASE-10-EXIT.md condition C1.")
    print()
    print(PLAN.strip())
    return 1


if __name__ == "__main__":
    sys.exit(main())
