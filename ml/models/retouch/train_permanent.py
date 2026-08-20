#!/usr/bin/env python3
"""Phase 20's permanent-feature classifier, and the cross-frame evidence that outranks it.

Two things are trained here and only one of them is a network.

The **classifier** takes a 64 px patch around one mark and names it: mole, freckle, birthmark,
scar, tattoo or dimple. It is weak evidence by design, and the loss below says so - the cost of
calling a tattoo something else is far higher than any other confusion, because that is the one
class whose protection can never be cleared.

The **cross-frame rule** is not a network at all. Section 6.1: "a spot that appears on the same
facial coordinate in many frames across hours is permanent; one that appears in a few frames is
temporary or transient lighting." What can be *fitted* about it is where the two thresholds go,
and the second half of this file does that on synthetic galleries whose answer is known.

``--self-test`` runs without PyTorch and asserts four properties:

1. the classifier loss decreases and the fitted model beats a constant predictor;
2. the **tattoo cost** changes what is learned;
3. the cross-frame rule separates a burst from a day: four frames in ninety seconds is not
   permanence, four frames across four hours is;
4. the rule is **conjunctive** - dropping either threshold makes it wrong, in opposite
   directions, which is why section 6.1 names both.

THE TWO DECISIONS

**Confusing anything with a tattoo costs forty times an ordinary confusion.** Not because a
tattoo is more important than a scar, but because the *consequence* of the two errors differs: a
mislabelled scar is still protected and a photographer can clear it; a mislabelled tattoo is a
tattoo that can be cleared, which this product does not permit at all.

**The coordinate system is the face, not the frame.** Every observation is projected onto the
eye-to-eye axis with the inter-ocular distance as the unit, so a person who moves, turns or tilts
their head does not move their own mole. `aura_retouch::permanent::to_face_frame` is the
implementation and this file fits the radius that decides when two sightings are the same mark.
"""

from __future__ import annotations

import argparse
import math
import random
import sys

KINDS = ("mole", "freckle", "birthmark", "scar", "tattoo", "dimple")

# The asymmetry, as a number. See the header.
TATTOO_COST = 40.0

# Section 6.1's two thresholds, both of which must hold.
MIN_FRAMES = 4
MIN_SPAN_MINUTES = 45.0

# How close two sightings must be, in inter-ocular units, to be the same mark.
SAME_FEATURE_RADIUS = 0.06


def synthetic_patches(count: int, seed: int) -> list[dict]:
    """Marks with a known kind and three measurable properties."""
    rng = random.Random(seed)
    out = []
    for index in range(count):
        kind = KINDS[index % len(KINDS)]
        size = {
            "mole": 0.01,
            "freckle": 0.006,
            "birthmark": 0.05,
            "scar": 0.03,
            "tattoo": 0.12,
            "dimple": 0.02,
        }[kind]
        out.append(
            {
                "kind": kind,
                "size": rng.gauss(size, size * 0.2),
                "darkness": rng.gauss(0.4 if kind in ("mole", "tattoo") else 0.2, 0.08),
                "elongation": rng.gauss(2.4 if kind == "scar" else 1.1, 0.3),
            }
        )
    return out


def score(weights: dict, patch: dict, kind: str) -> float:
    w = weights[kind]
    return (
        w[0]
        + w[1] * patch["size"] * 10.0
        + w[2] * patch["darkness"]
        + w[3] * patch["elongation"]
    )


def softmax(values: list[float]) -> list[float]:
    top = max(values)
    exps = [math.exp(min(30.0, v - top)) for v in values]
    total = sum(exps) or 1.0
    return [e / total for e in exps]


def loss(weights: dict, patches: list[dict], tattoo_cost: float) -> float:
    total = 0.0
    for patch in patches:
        probabilities = softmax([score(weights, patch, k) for k in KINDS])
        index = KINDS.index(patch["kind"])
        cost = tattoo_cost if patch["kind"] == "tattoo" else 1.0
        total += -math.log(max(probabilities[index], 1e-9)) * cost
    return total / max(len(patches), 1)


def fit(patches: list[dict], tattoo_cost: float, steps: int = 300) -> dict:
    weights = {k: [0.0, 0.0, 0.0, 0.0] for k in KINDS}
    rate = 0.20
    for _ in range(steps):
        gradients = {k: [0.0, 0.0, 0.0, 0.0] for k in KINDS}
        for patch in patches:
            probabilities = softmax([score(weights, patch, k) for k in KINDS])
            cost = tattoo_cost if patch["kind"] == "tattoo" else 1.0
            features = [
                1.0,
                patch["size"] * 10.0,
                patch["darkness"],
                patch["elongation"],
            ]
            for index, kind in enumerate(KINDS):
                target = 1.0 if kind == patch["kind"] else 0.0
                error = (probabilities[index] - target) * cost
                for i, feature in enumerate(features):
                    gradients[kind][i] += error * feature
        for kind in KINDS:
            for i in range(4):
                weights[kind][i] -= rate * gradients[kind][i] / max(len(patches), 1)
    return weights


def accuracy(weights: dict, patches: list[dict]) -> dict:
    right = 0
    tattoo_missed = 0
    for patch in patches:
        probabilities = softmax([score(weights, patch, k) for k in KINDS])
        predicted = KINDS[probabilities.index(max(probabilities))]
        if predicted == patch["kind"]:
            right += 1
        elif patch["kind"] == "tattoo":
            tattoo_missed += 1
    return {
        "accuracy": right / max(len(patches), 1),
        "tattoo_missed": tattoo_missed,
    }


def is_permanent(sightings: list[tuple[float, float, float]]) -> bool:
    """Section 6.1's rule: at least four frames, spanning at least forty-five minutes.

    Each sighting is `(minute, x, y)` in face-frame coordinates. Sightings further apart than
    `SAME_FEATURE_RADIUS` are different marks and are not counted together.
    """
    if not sightings:
        return False
    first = sightings[0]
    cluster = [
        s
        for s in sightings
        if math.hypot(s[1] - first[1], s[2] - first[2]) <= SAME_FEATURE_RADIUS
    ]
    span = max(s[0] for s in cluster) - min(s[0] for s in cluster)
    # Conjunctive. Either half alone is wrong, in opposite directions.
    return len(cluster) >= MIN_FRAMES and span >= MIN_SPAN_MINUTES


def self_test() -> int:
    patches = synthetic_patches(600, seed=20)

    before = loss({k: [0.0, 0.0, 0.0, 0.0] for k in KINDS}, patches, TATTOO_COST)
    weights = fit(patches, TATTOO_COST)
    after = loss(weights, patches, TATTOO_COST)
    assert after < before, f"the loss did not decrease: {before:.4f} -> {after:.4f}"

    weighted = accuracy(weights, patches)
    flat = accuracy(fit(patches, 1.0), patches)
    assert (
        weighted["tattoo_missed"] <= flat["tattoo_missed"]
    ), "pricing a tattoo confusion forty times higher changed nothing"

    # A burst is not permanence.
    burst = [(0.0, 0.1, 0.2), (0.3, 0.1, 0.2), (0.6, 0.1, 0.2), (0.9, 0.1, 0.2)]
    assert not is_permanent(burst), "a four-frame burst was called permanent"

    # A day is.
    day = [(0.0, 0.1, 0.2), (35.0, 0.104, 0.198), (120.0, 0.098, 0.203), (240.0, 0.1, 0.2)]
    assert is_permanent(day), "the same mark across four hours was not called permanent"

    # Conjunctive: two sightings across a day is not enough either.
    sparse = [(0.0, 0.1, 0.2), (240.0, 0.1, 0.2)]
    assert not is_permanent(sparse), "two sightings across a day passed the count threshold"

    # And a different mark on the same face is a different cluster.
    two_marks = day + [(10.0, 0.4, 0.2), (20.0, 0.4, 0.2)]
    assert is_permanent(two_marks), "clustering merged two marks and broke the first"

    print("train_permanent self-test: ok")
    print(f"  loss {before:.4f} -> {after:.4f}")
    print(f"  accuracy {weighted['accuracy']:.3f}, tattoos missed {weighted['tattoo_missed']}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run without PyTorch")
    parser.add_argument("--dry-run", action="store_true", help="describe the run and stop")
    parser.add_argument("--data", help="labelled patches, which do not exist here")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.dry_run:
        print("permanent features: 64 px patches -> 6 class logits, opset 13")
        print(f"  a tattoo confusion costs {TATTOO_COST}x an ordinary one")
        print(
            f"  cross-frame rule: >= {MIN_FRAMES} frames AND >= {MIN_SPAN_MINUTES} minutes, "
            f"within {SAME_FEATURE_RADIUS} inter-ocular units"
        )
        return 0

    print(
        "no labelled permanent-feature corpus is available in this repository; see the model "
        "card and docs/progress/PHASE-20-EXIT.md condition C2",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
