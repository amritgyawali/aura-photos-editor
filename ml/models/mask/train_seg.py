#!/usr/bin/env python3
"""Phase 18's segmentation training loop, and the self-test that proves the loss can fail.

There is no wedding segmentation data in this repository. Section 9 of the phase document gives
DATA a twelve-day task - "segmentation labels on 12k wedding frames incl. veils, ethnic attire,
varied skin tones" - and it did not happen and cannot happen here. So what this file ships is
the *training procedure*, exercised end to end on synthetic tiles whose labels are known by
construction, plus the three things about the procedure that are decisions rather than defaults.

``--self-test`` runs without PyTorch. It fits the same objective by gradient descent on a small
linear model over synthetic tiles and asserts three properties:

1. the loss decreases and the fitted model beats a constant predictor;
2. the **class weighting** actually changes what is learned - a run with uniform weights and a
   run with inverse-frequency weights disagree on the rare classes, which is the whole reason
   the weighting exists;
3. the **per-subset report** can fail: a model deliberately biased against one subset is caught
   by the gate rather than passing on the mean.

Property 3 is the one that matters. Section 10.1 requires per-class mIoU "including a dark-skin
subset and an ethnic-attire subset", and a report that could only pass would be a report that
proves nothing.

THE THREE DECISIONS

**Inverse-frequency class weighting, capped.** Twelve of the twenty classes cover under one per
cent of a wedding frame each - sclera, iris, teeth, eyebrows. Unweighted cross-entropy on a
768 px frame learns `background` and `clothing` and calls it a day. The cap matters as much as
the weighting: an uncapped inverse frequency gives `sclera` a weight in the hundreds, and a
model that gets one eye white right and the subject wrong is worse than useless.

**The loss is per pixel, and the metric is per class.** They are not the same thing and the gap
between them is where a segmentation model quietly gets worse. Training minimises weighted
cross-entropy; the gate in ``eval_mask.py`` reports mIoU per class and per subset, and the
report is what ships in the model card.

**No augmentation that changes colour.** Flips and crops are on; hue, saturation and exposure
jitter are off. The skin class in this product is seeded from *this frame's own faces* and grown
by colour distance, so a model trained on colour-jittered frames would be learning to ignore
exactly the signal the deterministic path depends on - and the two have to agree, because the
head is a prior over that path rather than a replacement for it.
"""

from __future__ import annotations

import argparse
import math
import random
import sys
from typing import Any

# The twenty classes, in the frozen order of `aura_vision::contract::mask::ALL_KINDS`.
CLASSES = (
    "skin",
    "face",
    "eyes",
    "sclera",
    "iris",
    "teeth",
    "lips",
    "eyebrows",
    "hair",
    "facial_hair",
    "clothing",
    "dress",
    "background",
    "sky",
    "subject",
    "greenery",
    "water",
    "floor",
    "window",
    "skin_safe",
)

# The largest a class weight may become. See "the three decisions".
WEIGHT_CAP = 12.0

# How many synthetic tiles the self-test fits on.
SELF_TEST_TILES = 600

# The seed. Everything in this file is deterministic; invariant 4 applies to training too,
# because a model card that reports a number nobody can reproduce is a model card.
SEED = 18


def class_weights(counts: dict[str, int], cap: float = WEIGHT_CAP) -> dict[str, float]:
    """Inverse-frequency weights, normalised to mean one and capped.

    Normalised to mean one so that changing the weighting does not change the effective learning
    rate - which is how a "small" weighting change turns into a training run that diverges and
    gets blamed on the weights.
    """
    total = sum(counts.values())
    if total <= 0:
        return {name: 1.0 for name in counts}
    raw = {}
    for name, count in counts.items():
        share = count / total
        raw[name] = min(cap, 1.0 / share) if share > 0 else cap
    mean = sum(raw.values()) / max(len(raw), 1)
    if mean <= 0:
        return {name: 1.0 for name in counts}
    return {name: value / mean for name, value in raw.items()}


def synthetic_tiles(n: int, rng: random.Random) -> list[tuple[list[float], int]]:
    """Tiles whose class is a linear function of two measurements, plus noise.

    Two features and three classes, which is enough to exercise the loss, the weighting and the
    report without a tensor library. The class frequencies are deliberately lopsided - 70 / 25 /
    5 - because that is the shape of a real frame and the weighting only matters when it is.
    """
    out: list[tuple[list[float], int]] = []
    for _ in range(n):
        roll = rng.random()
        if roll < 0.70:
            label = 0
            centre = (0.2, 0.8)
        elif roll < 0.95:
            label = 1
            centre = (0.8, 0.2)
        else:
            label = 2
            centre = (0.85, 0.85)
        features = [
            centre[0] + rng.gauss(0.0, 0.08),
            centre[1] + rng.gauss(0.0, 0.08),
        ]
        out.append((features, label))
    return out


def softmax(values: list[float]) -> list[float]:
    top = max(values)
    exps = [math.exp(v - top) for v in values]
    total = sum(exps)
    return [e / total for e in exps]


def fit(
    tiles: list[tuple[list[float], int]],
    weights: list[float],
    epochs: int = 900,
    lr: float = 0.8,
) -> tuple[list[list[float]], list[float]]:
    """Weighted multinomial logistic regression by gradient descent.

    A stand-in for the convolutional trunk, and a faithful one for the property being tested:
    the weighting enters the loss in exactly the same place it would in the real loop.
    """
    classes = len(weights)
    dim = len(tiles[0][0]) if tiles else 0
    w = [[0.0] * dim for _ in range(classes)]
    b = [0.0] * classes
    losses: list[float] = []

    for _ in range(epochs):
        grad_w = [[0.0] * dim for _ in range(classes)]
        grad_b = [0.0] * classes
        epoch_loss = 0.0
        for features, label in tiles:
            logits = [
                sum(w[c][d] * features[d] for d in range(dim)) + b[c] for c in range(classes)
            ]
            probs = softmax(logits)
            weight = weights[label]
            epoch_loss += -weight * math.log(max(probs[label], 1e-9))
            for c in range(classes):
                error = probs[c] - (1.0 if c == label else 0.0)
                for d in range(dim):
                    grad_w[c][d] += weight * error * features[d]
                grad_b[c] += weight * error
        n = max(len(tiles), 1)
        for c in range(classes):
            for d in range(dim):
                w[c][d] -= lr * grad_w[c][d] / n
            b[c] -= lr * grad_b[c] / n
        losses.append(epoch_loss / n)

    return w, b, losses  # type: ignore[return-value]


def per_class_recall(
    tiles: list[tuple[list[float], int]],
    model: tuple[list[list[float]], list[float]],
    classes: int,
) -> list[float]:
    w, b = model
    hit = [0] * classes
    seen = [0] * classes
    dim = len(tiles[0][0]) if tiles else 0
    for features, label in tiles:
        logits = [
            sum(w[c][d] * features[d] for d in range(dim)) + b[c] for c in range(classes)
        ]
        best = max(range(classes), key=lambda c: logits[c])
        seen[label] += 1
        if best == label:
            hit[label] += 1
    return [hit[c] / seen[c] if seen[c] else 0.0 for c in range(classes)]


def self_test() -> int:
    rng = random.Random(SEED)
    tiles = synthetic_tiles(SELF_TEST_TILES, rng)
    counts = {"a": 0, "b": 0, "c": 0}
    names = ["a", "b", "c"]
    for _, label in tiles:
        counts[names[label]] += 1

    problems: list[str] = []

    # 1. The loss decreases and the fit beats a constant predictor.
    uniform = [1.0, 1.0, 1.0]
    w_u, b_u, losses = fit(tiles, uniform)
    if losses[-1] >= losses[0]:
        problems.append(f"the loss did not decrease: {losses[0]:.4f} -> {losses[-1]:.4f}")
    recall_u = per_class_recall(tiles, (w_u, b_u), 3)
    majority = max(counts.values()) / max(sum(counts.values()), 1)
    mean_recall_u = sum(recall_u) / 3
    if mean_recall_u <= 1.0 / 3.0:
        problems.append(f"mean recall {mean_recall_u:.3f} is no better than guessing")

    # 2. The weighting changes what is learned, on the rare class.
    weighted = class_weights(counts)
    w_w, b_w, _ = fit(tiles, [weighted[n] for n in names])
    recall_w = per_class_recall(tiles, (w_w, b_w), 3)
    if recall_w[2] <= recall_u[2]:
        problems.append(
            f"the class weighting did not help the rare class: "
            f"uniform {recall_u[2]:.3f}, weighted {recall_w[2]:.3f}"
        )
    if max(weighted.values()) > WEIGHT_CAP:
        problems.append("a class weight exceeded the cap")

    # 3. The per-subset gate can fail.
    #    A model that is deliberately blind to class `c` must be caught, and the mean must not
    #    rescue it. Section 10.1's subset rows exist for exactly this reason.
    blinded = ([row[:] for row in w_w], b_w[:])
    blinded[1][2] -= 25.0
    recall_blind = per_class_recall(tiles, blinded, 3)
    if recall_blind[2] >= 0.05:
        problems.append("the blinded model was not actually blinded; the test proves nothing")
    if gate(recall_blind):
        problems.append(
            "the per-class gate passed a model that never predicts one of the classes"
        )
    if not gate(recall_w):
        problems.append("the per-class gate failed the honestly fitted model")

    for problem in problems:
        print(f"train_seg: {problem}", file=sys.stderr)
    if problems:
        return 1

    print(f"train_seg: loss {losses[0]:.4f} -> {losses[-1]:.4f} over {len(tiles)} tiles")
    print(
        "train_seg: rare-class recall "
        f"{recall_u[2]:.3f} unweighted -> {recall_w[2]:.3f} weighted "
        f"(majority share {majority:.2f})"
    )
    print("train_seg: the per-class gate rejects a model blinded to one class")
    print(
        "train_seg: NO WEDDING DATA IN THIS REPOSITORY. This exercises the procedure on "
        "synthetic tiles and says nothing about a photograph."
    )
    return 0


def describe() -> dict[str, Any]:
    """The training recipe, as data."""
    return {
        "classes": list(CLASSES),
        "loss": "per-pixel cross entropy, inverse-frequency class weights capped at "
        f"{WEIGHT_CAP} and normalised to mean one",
        "metric": "per-class mIoU, plus a dark-skin subset and an ethnic-attire subset",
        "augmentation": {
            "geometric": ["horizontal flip", "random crop", "small rotation"],
            "photometric": [],
            "why_no_photometric": "the skin class is seeded from the frame's own faces and "
            "grown by colour distance; a model trained on colour-jittered frames learns to "
            "ignore the signal the deterministic path depends on",
        },
        "seed": SEED,
        "trained": False,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the procedure checks")
    parser.add_argument("--describe", action="store_true", help="print the training recipe")
    args = parser.parse_args(argv)

    if args.describe:
        import json

        print(json.dumps(describe(), indent=2))
        return 0
    if args.self_test:
        return self_test()

    print(
        "train_seg: no labelled wedding frames are available in this repository, so there is "
        "nothing to train. Run with --self-test to exercise the procedure.",
        file=sys.stderr,
    )
    return 1


def gate(recall: list[float], floor: float = 0.35) -> bool:
    """The per-class gate: **every** class must clear the floor, not the mean.

    A mean over twenty classes is dominated by `background`, which is most of every frame and is
    the one class that is never hard. Section 10.1 asks for per-class figures for that reason,
    and this is the shape of the check the real gate has.
    """
    return all(value >= floor for value in recall)


if __name__ == "__main__":
    raise SystemExit(main())
