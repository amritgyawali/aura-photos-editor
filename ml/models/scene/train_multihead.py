"""Train the scene and attribute heads on the frozen phase 05 trunk.

PHASE-07 section 6.1, as a plan a reader can argue with and a script that will run the
day a labelled corpus exists.

    python ml/models/scene/train_multihead.py --plan
    python ml/models/scene/train_multihead.py --data scenes.npz --out scene_head.pt

## Why there is a `--plan` mode

There is no labelled wedding data in this repository - condition **C2** of
`docs/progress/PHASE-07-EXIT.md` - so this script cannot be run for real here. What it
can do is state the training design precisely enough that the ML lead can disagree with
it *before* eight days of labelling are spent against the wrong loss.

`--plan` prints that design. It is the deliverable phase 07 can actually produce, and it
is the same shape `ml/models/face/train_quality.py --plan` takes for the same reason.

## The design, in short

**The trunk is frozen.** `wedding_embedding` is not fine-tuned here. Fine-tuning it
would invalidate every stored vector in every catalog - a re-embed of every wedding a
photographer owns - to gain accuracy on a head that costs 0.002 ms to run. If the trunk
needs to change, that is a phase 05 decision with a `MODEL_VER` bump, not a side effect
of training a scene head.

**Two heads, one adapter.** A 256-wide ReLU adapter, a 22-way softmax and 14 independent
sigmoids. The heads share the adapter because scene and attributes are correlated -
`dance_floor` implies `night` far more often than chance - and separating them would
make the model learn that correlation twice.

**Focal loss on the scene head.** Section 6.1 asks for it and the class balance is why:
`candid` has two orders of magnitude more examples than `kiss`. Cross-entropy on that
distribution produces a model that is 40 % accurate and never predicts `kiss`, which is
the one class a client will notice the absence of.

**Plain BCE on the attributes.** They are independent and roughly balanced; focal loss
there would be a complication with no cause.

**Wedding-level splits.** Enforced by `dataset.split_by_wedding`, which is the only
splitter that exists. See that module for why.

## The three numbers a reviewer should push back on

1. **Adapter width 256.** Chosen because the trunk is 512 and a bottleneck of half is
   the smallest that has not underfit in the literature this design borrows from. It is
   a guess and it should be swept.
2. **Focal gamma 2.0.** The paper's default. It should be swept against 1.0 and 3.0 on
   the rare classes specifically, not on overall accuracy, which will not move.
3. **The attribute threshold, currently a single 0.5 in the Rust decoder.** After
   training this becomes a per-attribute vector from `eval_scene.py --calibrate`. A
   single threshold across fourteen attributes with very different base rates - `indoor`
   is near half, `pets` is near 2 % - is wrong in a way that is invisible in an average.
"""

from __future__ import annotations

import argparse
import sys
from typing import Sequence

sys.path.insert(0, __file__.rsplit("/", 1)[0] if "/" in __file__ else ".")

try:
    from dataset import ATTRIBUTES, CONTEXT_SLOTS, EMBED_DIM, SCENES, thin_classes
except ImportError:  # pragma: no cover - direct execution from another directory
    from ml.models.scene.dataset import (  # type: ignore[no-redef]
        ATTRIBUTES,
        CONTEXT_SLOTS,
        EMBED_DIM,
        SCENES,
        thin_classes,
    )

ADAPTER_DIM = 256
FOCAL_GAMMA = 2.0
LEARNING_RATE = 3e-4
WEIGHT_DECAY = 1e-4
EPOCHS = 40
BATCH = 256

PLAN = f"""
scene_classifier training plan - PHASE-07 section 6.1
=====================================================

INPUT
  features         [N, {EMBED_DIM + CONTEXT_SLOTS}]  frozen trunk ({EMBED_DIM}) + context ({CONTEXT_SLOTS})
  scene labels     [N]      one of {len(SCENES)} classes
  attribute labels [N, {len(ATTRIBUTES)}]  independent binary

ARCHITECTURE
  Gemm {ADAPTER_DIM}x{EMBED_DIM + CONTEXT_SLOTS} -> ReLU -> two heads
    scene:      Gemm {len(SCENES)}x{ADAPTER_DIM} -> Softmax   (in the exported graph)
    attributes: Gemm {len(ATTRIBUTES)}x{ADAPTER_DIM} -> Sigmoid  (in the exported graph)

  The trunk is FROZEN. Fine-tuning it invalidates every stored vector in every
  catalog a photographer owns; that is a phase 05 decision with a MODEL_VER bump,
  not a side effect of training a head.

LOSS
  scene:      focal, gamma = {FOCAL_GAMMA}, alpha = inverse class frequency
  attributes: BCE, unweighted
  total:      scene + 0.5 * attributes

  Focal on the scene head because `candid` has two orders of magnitude more
  examples than `kiss`, and cross-entropy on that distribution produces a model
  that never predicts the one class a client notices the absence of.

OPTIMISATION
  AdamW, lr = {LEARNING_RATE}, weight decay = {WEIGHT_DECAY}
  {EPOCHS} epochs, batch {BATCH}, cosine schedule, early stop on the held-out
  per-class mean rather than on overall accuracy

SPLIT
  Wedding-level, always. dataset.split_by_wedding is the only splitter and
  split_by_frame refuses. Two frames from one ceremony three seconds apart are
  near-duplicates and a frame-level split measures leakage.

SAMPLING
  Oversample any class below 200 examples to the 200 floor. Section 10.1 gates
  per-class accuracy only above that floor, so a class below it is reported as
  ungated rather than silently included in a clean-looking average.

ABSTENTION
  NOT trained. `SceneId::Unknown` is a decoder decision from the top-1 margin.
  A model cannot usefully be trained to say "I am not sure" through a slot that
  competes with the classes it is unsure between.

AFTER TRAINING
  1. eval_scene.py --calibrate     -> per-attribute thresholds, replacing the
                                      single 0.5 in the Rust decoder
  2. eval_scene.py --per-tradition -> the fairness table the model card needs
  3. export.py                     -> ONNX at opset 13, then
                                      `cargo xtask models --generate` re-locks
  4. Re-point tests/eval/scene_eval.rs from the synthetic fixtures at the real
     test set. Section 10.1's numbers become claims about photographs.

WHAT TO PUSH BACK ON
  * adapter width {ADAPTER_DIM} is a guess and should be swept
  * focal gamma {FOCAL_GAMMA} should be swept against the RARE classes, not
    against overall accuracy, which will not move
  * a single attribute threshold across 14 attributes with base rates from 2 %
    (`pets`) to 50 % (`indoor`) is wrong in a way an average hides
"""


def train(data_path: str, out_path: str) -> int:
    """Train, when torch and a corpus are both present."""
    try:
        import torch
    except ImportError:
        print(
            "torch is not installed. This script's deliverable today is `--plan`; see "
            "condition C2 in docs/progress/PHASE-07-EXIT.md.",
            file=sys.stderr,
        )
        return 2

    try:
        import numpy as np

        bundle = np.load(data_path, allow_pickle=True)
    except (ImportError, FileNotFoundError) as error:
        print(f"cannot read {data_path}: {error}", file=sys.stderr)
        return 2

    features = torch.from_numpy(bundle["features"]).float()
    if features.shape[1] != EMBED_DIM + CONTEXT_SLOTS:
        print(
            f"features are {features.shape[1]} wide, expected {EMBED_DIM + CONTEXT_SLOTS}",
            file=sys.stderr,
        )
        return 2

    print(
        f"{features.shape[0]} examples, {features.shape[1]} features. "
        f"Training would start here; see --plan for the design.\n"
        f"Writing nothing to {out_path}: there is no labelled corpus in this repository."
    )
    return 0


def main(argv: Sequence[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true", help="print the training design")
    parser.add_argument("--data", help="an .npz from dataset.py")
    parser.add_argument("--out", default="scene_head.pt")
    args = parser.parse_args(argv)

    if args.plan or not args.data:
        print(PLAN.strip())
        if not args.plan:
            print("\n(no --data given, so the plan is what you get)")
        return 0
    return train(args.data, args.out)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
