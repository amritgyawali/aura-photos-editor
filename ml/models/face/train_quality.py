"""Train the face quality head, and specify how its labels are produced.

Section 6.1 of PHASE-06 asks for a "quality head [that] predicts usability (pose, blur,
occlusion, pixel height)". The training loop is the easy half. The hard half - and the
part this file exists to pin down before anybody writes a loop - is **where the labels
come from**, because getting that wrong produces a head that predicts what a human calls
a nice photograph rather than what the recogniser can actually use.

    python ml/models/face/train_quality.py --plan            # print the specification
    python ml/models/face/train_quality.py --dataset faces.npz --out face_quality.onnx

**No head is trained in this repository.** There is no consented, labelled face data here
and no GPU backend, so what ships is a placeholder with the right architecture and no
training, and `aura_vision::face::quality::QUALITY_MODEL_WEIGHT` is 0.0 - the gate is four
measured factors until a real head exists. That is condition C2 in
docs/progress/PHASE-06-EXIT.md, and this script refuses to pretend otherwise: without a
dataset it prints the specification and exits.

## The label is a regression against the recogniser, not a human judgement

For each face with a known identity, the target usability is **how well its template
matches that identity's centroid computed from its best frames**. A face that verifies is
usable; one that does not is not. That makes the head learn the recogniser's actual
weakness rather than an annotator's aesthetic, and it is why the dataset needs identity
labels rather than quality labels.

The three auxiliary targets - blur, occlusion, pose - are the *measured* factors from
`aura_vision::face::quality::measure` and `align::pose_from_landmarks`. The head learns to
predict them from a crop rather than to replace them, which is what makes it useful on the
cases the measurements are wrong about: a face behind glass measures sharp and verifies
badly, and only a learned head can tell.

## Why the two are blended rather than one replacing the other

`fuse` combines the head's usability with the four measured factors at
`QUALITY_MODEL_WEIGHT`. Neither alone is right: the measurements are robust and blind to
context, the head is context-aware and can be confidently wrong on a distribution it has
not seen. A weight, recorded in the model card, is how much this build trusts it.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# Mirrors `aura_vision::face::quality::FUSION_WEIGHTS`, in the order sharpness, occlusion,
# pose, exposure. Kept here so the training targets are weighted the way the gate weights
# them, rather than the head learning a different importance from the thing that uses it.
FUSION_WEIGHTS = (0.35, 0.25, 0.25, 0.15)

# Mirrors `MIN_PX_HEIGHT` and `MIN_USABLE`.
MIN_PX_HEIGHT = 48
MIN_USABLE = 0.40

# The subsets the evaluation must be reported over separately. Section 12's second failure
# mode is that accuracy varies across skin tones, and a quality head is a *gatekeeper* - a
# group whose faces score systematically lower is a group that gets under-clustered, and
# the failure presents as "the product did not group my cousins" rather than as a bias
# metric. Nobody would look for it there, so it is reported here by construction.
REQUIRED_SUBSETS = (
    "skin_tone_group",
    "dark_scene",
    "small_face",
    "occluded",
    "profile",
)

SPECIFICATION = {
    "architecture": {
        "input": "pixels [N,3,112,112], NCHW, 0..1, sRGB, ArcFace-aligned",
        "output": "quality [N,4] - usability, blur, occlusion, pose - sigmoid in the graph",
        "trunk": "Conv 3->16 s2, MaxPool, Conv 16->32 s2, GlobalAveragePool, Gemm 32->32, Gemm 32->4",
        "parameters": 6276,
        "note": "deliberately tiny: it runs once per face on a wedding's worth of faces "
        "and consumes a crop the recogniser has already produced",
    },
    "labels": {
        "usability": "cosine similarity between this face's template and its identity's "
        "centroid, where the centroid is computed from that identity's highest-quality "
        "frames only - so the target is 'does this face verify', not 'is this a nice "
        "photograph'",
        "blur": "1 - sharpness from aura_vision::face::quality::measure",
        "occlusion": "occlusion from the same function",
        "pose": "Pose::frontality_penalty from the five landmarks",
    },
    "splits": "wedding-level, and within a wedding identity-level: the same person must "
    "never appear in train and test",
    "occluders_required": [
        "veils",
        "microphones",
        "glasses with flash reflection",
        "hands",
        "hair across a face",
        "champagne flutes",
    ],
    "augmentation": "photographic only - exposure, white balance, mild noise, JPEG "
    "artefacts. Never horizontal flips of ritual scenes: handedness carries meaning in "
    "the rituals this product is built for.",
    "subsets_reported_separately": list(REQUIRED_SUBSETS),
    "acceptance": {
        "monotonicity": "a blurrier, more occluded or more turned face must never score "
        "higher; asserted in crates/aura-vision/tests/face_quality.rs",
        "vote_rate_parity": "the fraction of detected faces that pass the gate, reported "
        "per skin-tone group, with an agreed maximum disparity - not accuracy, which "
        "hides this failure",
        "gate_effect": "identity clustering F1 must not fall when the head's weight is "
        "raised from 0.0; if it does, the head is worse than the measurements",
    },
}


def print_specification() -> None:
    print(json.dumps(SPECIFICATION, indent=2))
    print()
    print("No head is trained here. See the module docstring and condition C2 in")
    print("docs/progress/PHASE-06-EXIT.md.")


def fused(sharpness: float, occlusion: float, pose_penalty: float, exposure: float) -> float:
    """The weighted geometric mean `aura_vision::face::quality::fuse` computes.

    Reimplemented here so a training run can report the fused score its targets imply and
    compare it with what the product would compute. An arithmetic mean would let three
    good factors outvote one catastrophic one; see that function for why that matters.
    """
    import math

    floor = 0.001
    factors = (
        max(sharpness, floor),
        max(1.0 - occlusion, floor),
        max(1.0 - pose_penalty, floor),
        max(exposure, floor),
    )
    weighted = sum(w * math.log(f) for w, f in zip(FUSION_WEIGHTS, factors))
    return min(1.0, max(0.0, math.exp(weighted / sum(FUSION_WEIGHTS))))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true", help="print the specification and exit")
    parser.add_argument("--dataset", type=Path, help="an .npz export of labelled faces")
    parser.add_argument("--out", type=Path, help="where to write the exported ONNX head")
    parser.add_argument("--epochs", type=int, default=30)
    args = parser.parse_args()

    if args.plan or args.dataset is None:
        print_specification()
        # Not an error: printing the plan is the useful thing this script can do today, and
        # exiting non-zero would make CI treat a correct outcome as a failure.
        return 0

    if not args.dataset.exists():
        print(f"{args.dataset} does not exist", file=sys.stderr)
        return 2

    print(
        "A dataset was supplied, but this build ships no training loop: there is no GPU\n"
        "backend in this repository and no consented face data to train on. Implementing\n"
        "the loop is the first task of the phase that closes condition C2, and the\n"
        "specification it must satisfy is printed by --plan.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
