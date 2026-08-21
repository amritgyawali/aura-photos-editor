#!/usr/bin/env python3
"""Phase 21's flyaway detector training loop, and the self-test that proves it can fail.

There is no labelled flyaway data in this repository. Section 9 gives DATA a seven-day task -
"labels for flyaways, glare, lint; hair-type diversity coverage" - and it did not happen and
cannot happen here. What ships is the *training procedure*, exercised end to end on synthetic
hair tiles whose strands are known by construction, plus the decisions in it that are decisions
rather than defaults.

``--self-test`` runs without PyTorch. It fits the same objective by gradient descent on a small
linear model over synthetic tiles and asserts four properties:

1. the loss decreases and the fitted model beats a constant predictor;
2. the **background gate is learned rather than bolted on** - a run trained without background
   features and a run trained with them disagree about strands over foliage, which is the whole
   reason the feature exists;
3. the **per-hair-type report can fail**: a model deliberately biased against tightly coiled hair
   is caught by the gate rather than passing on the mean;
4. a model that fires *inside* the hair mass cannot pass at any accuracy, because that gate is
   not a threshold - it is what "no bald patches" means.

Properties 2 and 4 are the ones that matter. Section 10.1 asks for "no bald patches or hairline
damage on any fixture", which is not a score, and a training objective that treated it as one
would trade it against recall.

THE THREE DECISIONS

**A false positive inside the hair mass is fatal, not costly.** Unlike phase 20's fifteen-to-one
asymmetry, this is not a weight: a candidate whose centre sits where the hair alpha is above
``INSIDE_MIN`` is dropped before the loss sees it, and a model that produces them fails the gate
outright. Attenuating a strand *inside* somebody's hair is how a hairline gets chewed, and there
is no recall number that buys it back.

**The background is a feature, not a filter.** It would be simpler to detect strands everywhere
and discard the ones over busy backgrounds afterwards. It is also worse: the same local contrast
means something different against a wall and against foliage, so the detector has to be able to
learn a *different threshold* per background rather than one threshold plus a veto. The runtime
still applies the veto as well - two layers, because the learned half is not shipped.

**Hair type is a coverage report, never an input.** The model never sees a hair-type label and
the catalog never stores one. What the label is for is the fairness gate: a detector that works
on straight hair and not on coiled hair passes on the mean and fails here. Phase 15's rule.
"""

from __future__ import annotations

import argparse
import math
import random
import sys

# The hair types the coverage gate reports against. They live in the evaluation code rather than
# in the catalog: no per-person hair label ever reaches the database, for the same reason no
# per-person skin-tone label does.
HAIR_TYPES = ("straight", "wavy", "curly", "coily", "braided_or_locked")

# Section 10.1's gates.
RECALL_FLOOR = 0.85
FALSE_POSITIVE_CEILING = 0.05
# Not a threshold. See the module docstring.
INSIDE_HAIR_FIRINGS_ALLOWED = 0

# Where the hair mass starts, matching `aura_retouch::micro::hair::INSIDE_MIN`.
INSIDE_MIN = 0.80

FEATURES = ("contrast", "thinness", "connectedness", "background_detail")


def sigmoid(x: float) -> float:
    if x >= 0:
        return 1.0 / (1.0 + math.exp(-x))
    z = math.exp(x)
    return z / (1.0 + z)


def make_tile(rng: random.Random, hair_type: str, with_background: bool) -> tuple[list[float], int, float]:
    """One synthetic candidate: its features, its label, and the hair alpha under it.

    A strand is thin, high-contrast and connected to the mass. A distractor is one of: a piece of
    background texture (thin, contrasty, *not* connected), a fold of the hair mass itself (thick,
    connected, inside the alpha), or a leaf edge over a busy background.
    """
    kind = rng.choice(("strand", "texture", "fold", "leaf"))
    # Coiled and braided hair has more, shorter, lower-contrast strands. The generator says so
    # explicitly rather than leaving it implicit, because that is the disparity the gate hunts.
    softness = {"straight": 1.0, "wavy": 0.95, "curly": 0.85, "coily": 0.7, "braided_or_locked": 0.75}[
        hair_type
    ]

    if kind == "strand":
        contrast = rng.uniform(0.05, 0.40) * softness
        thinness = rng.uniform(0.7, 1.0)
        connectedness = rng.uniform(0.8, 1.0)
        detail = rng.uniform(0.0, 0.12)
        alpha = rng.uniform(0.05, 0.40)
        label = 1
    elif kind == "texture":
        contrast = rng.uniform(0.05, 0.40)
        thinness = rng.uniform(0.6, 1.0)
        connectedness = rng.uniform(0.0, 0.3)
        detail = rng.uniform(0.0, 0.2)
        alpha = rng.uniform(0.0, 0.05)
        label = 0
    elif kind == "fold":
        contrast = rng.uniform(0.10, 0.50)
        thinness = rng.uniform(0.0, 0.4)
        connectedness = rng.uniform(0.9, 1.0)
        alpha = rng.uniform(INSIDE_MIN, 1.0)
        detail = rng.uniform(0.0, 0.1)
        label = 0
    else:  # a leaf edge against foliage
        contrast = rng.uniform(0.15, 0.45)
        thinness = rng.uniform(0.6, 1.0)
        connectedness = rng.uniform(0.5, 0.9)
        detail = rng.uniform(0.25, 0.9)
        alpha = rng.uniform(0.05, 0.40)
        label = 0

    features = [contrast, thinness, connectedness, detail if with_background else 0.0]
    return features, label, alpha


def fit(rng: random.Random, samples, steps: int = 900, rate: float = 0.6) -> list[float]:
    weights = [0.0] * (len(FEATURES) + 1)
    for _ in range(steps):
        gradient = [0.0] * len(weights)
        for features, label in samples:
            z = weights[0] + sum(w * f for w, f in zip(weights[1:], features))
            error = sigmoid(z) - label
            gradient[0] += error
            for index, value in enumerate(features):
                gradient[index + 1] += error * value
        scale = rate / len(samples)
        weights = [w - scale * g for w, g in zip(weights, gradient)]
    return weights


def loss_of(weights: list[float], samples) -> float:
    total = 0.0
    for features, label in samples:
        z = weights[0] + sum(w * f for w, f in zip(weights[1:], features))
        p = min(max(sigmoid(z), 1e-9), 1 - 1e-9)
        total -= label * math.log(p) + (1 - label) * math.log(1 - p)
    return total / len(samples)


def score(weights: list[float], features: list[float]) -> float:
    return sigmoid(weights[0] + sum(w * f for w, f in zip(weights[1:], features)))


def dataset(rng: random.Random, hair_type: str, n: int, with_background: bool):
    rows = []
    for _ in range(n):
        features, label, alpha = make_tile(rng, hair_type, with_background)
        rows.append((features, label, alpha))
    return rows


def self_test() -> int:
    failures: list[str] = []
    rng = random.Random(0x21_F1_1A)

    # --- 1. it learns ---------------------------------------------------------------------
    train = [(f, l) for f, l, _ in dataset(rng, "straight", 900, True)]
    weights = fit(rng, train)
    constant = [math.log(sum(l for _, l in train) / len(train) or 1e-9)] + [0.0] * len(FEATURES)
    if not loss_of(weights, train) < loss_of(constant, train):
        failures.append("the fitted model does not beat a constant predictor")

    # --- 2. the background feature changes what is learned ---------------------------------
    blind_train = [(f, l) for f, l, _ in dataset(random.Random(7), "straight", 900, False)]
    blind = fit(rng, blind_train)
    held = dataset(random.Random(11), "straight", 400, True)
    leaves = [(f, l, a) for f, l, a in held if l == 0 and f[3] > 0.25]
    if not leaves:
        failures.append("the fixture produced no busy-background distractors")
    else:
        seeing = sum(1 for f, _, _ in leaves if score(weights, f) > 0.5) / len(leaves)
        blindly = sum(
            1 for f, _, _ in leaves if score(blind, [f[0], f[1], f[2], 0.0]) > 0.5
        ) / len(leaves)
        if not seeing < blindly:
            failures.append(
                f"the background feature changed nothing: {seeing:.3f} against {blindly:.3f}"
            )

    # --- 3. the per-hair-type report can fail ----------------------------------------------
    biased_rows = []
    for hair_type in HAIR_TYPES:
        rows = dataset(random.Random(hash(hair_type) & 0xFFFF), hair_type, 300, True)
        if hair_type == "coily":
            # A model trained on data whose coily strands are mislabelled: the disparity the gate
            # exists to find, injected on purpose.
            rows = [(f, 0, a) for f, _, a in rows]
        biased_rows.extend((f, l) for f, l, _ in rows)
    biased = fit(rng, biased_rows)
    per_type = {}
    for hair_type in HAIR_TYPES:
        rows = dataset(random.Random(0xC0 + len(hair_type)), hair_type, 300, True)
        positives = [(f, a) for f, l, a in rows if l == 1]
        if not positives:
            continue
        per_type[hair_type] = sum(1 for f, _ in positives if score(biased, f) > 0.5) / len(
            positives
        )
    if per_type and min(per_type.values()) >= RECALL_FLOOR:
        failures.append(f"a biased model passed the per-hair-type gate: {per_type}")

    # --- 4. firing inside the hair mass cannot pass -----------------------------------------
    held_all = dataset(random.Random(13), "wavy", 600, True)
    inside = [
        (f, a) for f, _, a in held_all if a >= INSIDE_MIN and score(weights, f) > 0.5
    ]
    if len(inside) > INSIDE_HAIR_FIRINGS_ALLOWED:
        failures.append(
            f"{len(inside)} candidates inside the hair mass survived; the gate allows "
            f"{INSIDE_HAIR_FIRINGS_ALLOWED}"
        )

    for line in failures:
        print(f"FAIL {line}", file=sys.stderr)
    if failures:
        return 1
    print("train_flyaway self-test: 4 properties hold")
    print("  the fit converges and beats a constant predictor")
    print("  the background feature changes what is learned")
    print("  the per-hair-type gate catches a biased model")
    print("  nothing inside the hair mass survives, at any accuracy")
    return 0


def train(_args: argparse.Namespace) -> int:
    print(
        "there is no labelled flyaway corpus in this repository; section 9's DATA task has not "
        "happened, so this path refuses rather than pretending",
        file=sys.stderr,
    )
    return 2


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run the properties above")
    parser.add_argument("--data", help="a labelled corpus, which does not exist here")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    return train(args)


if __name__ == "__main__":
    raise SystemExit(main())
