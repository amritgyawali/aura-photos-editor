"""Train the interaction head with person-box priors.

PHASE-10 sections 6.2 and 8 step 4, as a plan a reader can argue with and a script that
will run the day a labelled corpus exists.

    python ml/models/emotion/train_interaction.py --plan
    python ml/models/emotion/train_interaction.py --data frames.npz --out interaction_head.pt

## Why there is a `--plan` mode

The same reason `train_expression.py` and `train_focus.py` have one: there is no labelled
wedding data in this repository (condition **C1**), so what this script can do is state the
design precisely enough to be disagreed with before the labels are collected.

## The design, in short

**The prior plane is rendered from the same boxes at training time and at inference time.**
This is the one mistake that would be most expensive to make and hardest to notice: a prior
rendered from ground-truth person boxes during training and from a detector's face boxes
during inference is a train/serve skew, and the head would learn to trust a signal the
product cannot give it. So the miner renders the prior with
`aura_brain_wedding::emotion::interaction::frame_tensor`'s own geometry - a face box grown
`TORSO_HEIGHTS` down and `SHOULDER_WIDTHS` either side - from *detected* boxes, including
the frames where the detector was wrong.

**Frame-level multi-hot labels, not boxes.** The head is asked what is happening, not
where. A localisation target would be a more expensive annotation task for an output the
product has no field for, and `ImageEmotion::interactions` is a list of (kind, strength)
pairs.

**Per-class operating points, not one floor.** `DETECTION_FLOOR` is a single 0.35 today
because there is nothing to tune it against. The nine classes are wildly unbalanced -
`hand_hold` is in a tenth of a wedding's frames and `ring_exchange` is in twenty - and the
first thing a trained version should change about `interaction.rs` is that constant
becoming an array.
"""

from __future__ import annotations

import argparse
import sys

# The nine classes, in `Interaction::ALL` order. Reordering this relabels a trained head.
CLASSES = (
    "hug",
    "kiss",
    "hand_hold",
    "dance_hold",
    "ring_exchange",
    "blessing",
    "toast",
    "tears_wiped",
    "group_cheer",
)

# The four milestones a client buys a print of. `Interaction::is_milestone`.
MILESTONES = ("kiss", "ring_exchange", "blessing", "tears_wiped")

# The tensor the head reads, matching `aura_infer::onnx::fixtures`.
SIDE = 160
CHANNELS = 4

PLAN = f"""
Interaction head - training design
==================================

INPUT
  {SIDE}x{SIDE}x{CHANNELS}: three colour planes plus one person-prior plane.

  The colour planes are area-averaged down from the 2048 px proxy, not bilinear-sampled.
  The direction of the resample is the reason: a 13x downscale sampled bilinearly reads
  one pixel in 169 and aliases the rest. `interaction::frame_tensor` does it the right
  way and the miner must call the same function.

  The prior plane is painted from DETECTED face boxes, grown into torsos by the same
  constants the product uses. Rendering it from ground-truth boxes would be a train/serve
  skew - see this file's header.

OUTPUT
  9 independent sigmoids, in this order:
    {', '.join(CLASSES)}

  Multi-label. A frame can be a hug AND tears being wiped, and it very often is: somebody
  wiping somebody else's tears is holding them.

LOSS
  Per-class binary cross-entropy, class-balanced by inverse frequency, with the four
  milestones ({', '.join(MILESTONES)}) weighted x1.3 on positives.

  The milestone weighting is a product decision rather than a statistical one: a missed
  ring exchange costs a frame that the wedding has exactly twenty of, and a missed hug
  costs one of four hundred.

SAMPLING
  Stratified by class with a floor: no batch may be more than 40 % negatives, and every
  class must appear in every epoch. `ring_exchange` and `tears_wiped` are the two that
  will otherwise vanish.

  Stratified by tradition as well, and `blessing` is why. A tilak, feet being touched, a
  hand on a head and a priest's blessing are ONE class in the vocabulary and four quite
  different images. A training set drawn from one tradition would learn one of them and
  the head would silently stop firing on the others.

SPLITS
  By WEDDING, 70/15/15, as `train_expression.py` specifies and for the same reason.

LABELS
  Frame-level multi-hot from an annotator who has seen the scene label. The scene is given
  rather than hidden: an annotator deciding whether two people at an altar are holding
  hands or exchanging rings is doing the job better with the chapter in front of them, and
  the head gets the frame alone at inference time regardless.

AUGMENTATION
  Horizontal flip, with the prior plane flipped with it. Nothing else.
  NOT crop or scale jitter: the prior plane's geometry is the signal, and jittering the
  frame without re-rendering the prior teaches the head that the two disagree.

EVALUATION
  `eval_emotion.py --per-class` reports precision and recall for each of the nine
  separately. A macro-F1 that looks respectable because `hug` is excellent and
  `ring_exchange` is at chance is a head that has not been trained for this product.

  The operating point per class is chosen on the validation set at the precision the
  product needs, and the result replaces `interaction::DETECTION_FLOOR` with an array.

EXPORT
  `ml/export_onnx` at opset 13, fp32, fp16 and int8. int8 IS permitted here and forbidden
  on `expression_head`: nine coarse pose questions over a {SIDE} px frame are decided far
  from any threshold that matters, nothing here can convict a photograph, and this head
  runs once on every frame in the wedding.
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true", help="print the training design")
    parser.add_argument("--data", help="an .npz of four-plane frames and multi-hot targets")
    parser.add_argument("--out", help="where to write the trained checkpoint")
    parser.add_argument("--epochs", type=int, default=40)
    args = parser.parse_args()

    if args.plan or not args.data:
        print(PLAN.strip())
        if not args.plan:
            print()
            print("no --data given, so nothing was trained. See condition C1.")
        return 0

    print("interaction head: training requires torch, a GPU backend and a labelled corpus.")
    print("None of the three is present. See docs/progress/PHASE-10-EXIT.md condition C1.")
    print()
    print(PLAN.strip())
    return 1


if __name__ == "__main__":
    sys.exit(main())
