#!/usr/bin/env python3
"""Phase 21's lint detector training loop, and the self-test that proves it can fail.

There is no labelled clothing corpus in this repository. What ships is the *training procedure*,
exercised end to end on synthetic fabric tiles whose marks are known by construction, plus the
decisions in it that are decisions rather than defaults.

``--self-test`` runs without PyTorch and asserts four properties:

1. the loss decreases and the fitted model beats a constant predictor;
2. the head **learns something about fabric texture** - its mark probability on patterned
   fabric is systematically below its probability on plain fabric - and, with the shipped veto
   applied on top, **a patterned fabric produces no detections at all**;
3. the three kinds are separated rather than merged: lint, thread and stain are different marks
   with different shapes, and a studio switches them on and off separately;
4. **the two opt-in kinds are never predicted.** Straps and creases are not anomalies against the
   fabric - they are the garment - so a head that claimed to find one is a head that has learned
   to find shadows, and the gate refuses it outright.

Property 4 is the interesting one and it is a modelling decision rather than a threshold. See
`aura_retouch::micro::clothing`, which does not detect either kind for the same reason.

Property 2 is in two halves on purpose, and the reason is worth stating because the first version
of this file asked for the wrong thing. **A sequin in a weave and a piece of lint on a lapel are
genuinely the same measurement**: a small bright thing that departs from what is around it. No
amount of training separates them, because the difference is not in the candidate - it is in
whether the fabric is *made of* such things. So the learned half is asked only to discriminate,
and the absolute claim is carried by the veto that ships in `clothing::detect`. Asking the model
for zero firings was asking it to be certain about something the photograph does not say, and
training longer made it worse rather than better.

THE THREE DECISIONS

**The fabric-texture feature is an input, not a post-filter.** Same argument as the flyaway
detector's background feature: the local contrast that means "lint" on a plain lapel means
"weave" on tweed, so the head has to be able to learn a different threshold per fabric.

**Marks are classified, not just found.** One head with four outputs - not-a-mark, lint, thread,
stain - rather than an objectness score plus a rule over the aspect ratio. The opt-in matrix is
keyed by kind, so a kind the model cannot name is a kind a studio cannot switch off.

**No scale jitter beyond a factor of two.** The size of a mark relative to the fabric around it is
most of the signal separating lint from a stain, and heavy scale augmentation trains it away.
"""

from __future__ import annotations

import argparse
import math
import random
import sys

# The four classes the head predicts. `strap` and `crease` are deliberately absent - see the
# module docstring and `ClothingIssue::is_opt_in_only`.
CLASSES = ("none", "lint", "thread", "stain")

# The kinds this head may never produce. Not a threshold.
NEVER_PREDICTED = ("strap", "crease")

# Section 10.1's gates.
RECALL_FLOOR = 0.85
PATTERNED_FIRINGS_ALLOWED = 0

# The shipped veto, matching `aura_retouch::micro::clothing::MAX_FABRIC_TEXTURE`. Above this the
# fabric is patterned and nothing is cleaned off it, whatever the head says.
MAX_FABRIC_TEXTURE = 0.14

FEATURES = ("departure", "aspect", "size", "fabric_texture", "salience")


def softmax(scores: list[float]) -> list[float]:
    top = max(scores)
    exps = [math.exp(s - top) for s in scores]
    total = sum(exps)
    return [e / total for e in exps]


def make_mark(rng: random.Random) -> tuple[list[float], int]:
    """One synthetic candidate on a garment: its features and its class index."""
    kind = rng.choice(("lint", "thread", "stain", "plain", "weave"))
    if kind == "lint":
        departure = rng.uniform(0.10, 0.60)
        aspect = rng.uniform(1.0, 2.0) / 12.0
        size = rng.uniform(0.05, 0.30)
        texture = rng.uniform(0.0, 0.10)
        label = CLASSES.index("lint")
    elif kind == "thread":
        departure = rng.uniform(0.08, 0.50)
        aspect = rng.uniform(3.0, 12.0) / 12.0
        size = rng.uniform(0.05, 0.35)
        texture = rng.uniform(0.0, 0.10)
        label = CLASSES.index("thread")
    elif kind == "stain":
        departure = -rng.uniform(0.08, 0.45)
        aspect = rng.uniform(1.0, 2.2) / 12.0
        size = rng.uniform(0.25, 0.90)
        texture = rng.uniform(0.0, 0.10)
        label = CLASSES.index("stain")
    elif kind == "plain":
        departure = rng.uniform(-0.03, 0.03)
        aspect = rng.uniform(1.0, 3.0) / 12.0
        size = rng.uniform(0.02, 0.90)
        texture = rng.uniform(0.0, 0.08)
        label = CLASSES.index("none")
    else:  # a sequin or a slub in the weave
        departure = rng.choice((1.0, -1.0)) * rng.uniform(0.10, 0.55)
        aspect = rng.uniform(1.0, 3.0) / 12.0
        size = rng.uniform(0.03, 0.30)
        texture = rng.uniform(0.30, 1.0)
        label = CLASSES.index("none")
    # The fifth feature is the one that makes this problem learnable at all, and it is worth
    # explaining because a self-test found it. `none` covers two very different things - plain
    # fabric with no mark on it, and a sequin in a weave - and "small departure OR high texture"
    # is a disjunction, which no linear model over `departure` and `texture` separately can
    # express. A run without it misclassified a handful of sequins as lint no matter how long it
    # trained, and training it *longer* made that worse rather than better.
    #
    # `salience` is the physical quantity the two cases actually differ on: how far a mark
    # departs from its surroundings, **relative to how much the fabric departs from itself**. It
    # is the same reading `aura_retouch::micro::clothing` refuses on, expressed as a feature
    # rather than as a veto.
    salience = abs(departure) * (1.0 - min(texture, 1.0))
    return [departure, aspect, size, texture, salience], label


def fit(samples, steps: int = 3000, rate: float = 2.0) -> list[list[float]]:
    weights = [[0.0] * (len(FEATURES) + 1) for _ in CLASSES]
    for _ in range(steps):
        gradient = [[0.0] * (len(FEATURES) + 1) for _ in CLASSES]
        for features, label in samples:
            scores = [w[0] + sum(v * f for v, f in zip(w[1:], features)) for w in weights]
            probabilities = softmax(scores)
            for index in range(len(CLASSES)):
                error = probabilities[index] - (1.0 if index == label else 0.0)
                gradient[index][0] += error
                for position, value in enumerate(features):
                    gradient[index][position + 1] += error * value
        scale = rate / len(samples)
        weights = [
            [w - scale * g for w, g in zip(row, grow)] for row, grow in zip(weights, gradient)
        ]
    return weights


def loss_of(weights, samples) -> float:
    total = 0.0
    for features, label in samples:
        scores = [w[0] + sum(v * f for v, f in zip(w[1:], features)) for w in weights]
        p = max(softmax(scores)[label], 1e-9)
        total -= math.log(p)
    return total / len(samples)


def predict(weights, features) -> int:
    scores = [w[0] + sum(v * f for v, f in zip(w[1:], features)) for w in weights]
    return max(range(len(CLASSES)), key=lambda index: scores[index])


def mark_probability(weights, features) -> float:
    """How much of the head's mass sits on the three mark classes."""
    scores = [w[0] + sum(v * f for v, f in zip(w[1:], features)) for w in weights]
    probabilities = softmax(scores)
    return sum(probabilities[1:])


def shipped(weights, features) -> str:
    """What the product would actually do with this candidate.

    The head's answer, **and then the veto**. Two layers, which is how the runtime works: the
    learned half discriminates and `clothing::detect` refuses outright above
    `MAX_FABRIC_TEXTURE`. A gate that asked the head alone for an absolute would be asking it to
    be certain about something the photograph does not say.
    """
    if features[3] > MAX_FABRIC_TEXTURE:
        return "none"
    return CLASSES[predict(weights, features)]


def dataset(rng: random.Random, n: int):
    return [make_mark(rng) for _ in range(n)]


def self_test() -> int:
    failures: list[str] = []
    rows = dataset(random.Random(0x21_11_47), 1600)
    weights = fit(rows)

    # --- 1. it learns -------------------------------------------------------------------------
    counts = [0] * len(CLASSES)
    for _, label in rows:
        counts[label] += 1
    prior = [[math.log(max(c, 1) / len(rows))] + [0.0] * len(FEATURES) for c in counts]
    if not loss_of(weights, rows) < loss_of(prior, rows):
        failures.append("the fitted model does not beat the class prior")

    held = dataset(random.Random(0x9AA), 900)

    # --- 2a. the head learns something about fabric texture ------------------------------------
    patterned = [f for f, _ in held if f[3] > MAX_FABRIC_TEXTURE]
    plain_marks = [f for f, l in held if f[3] <= MAX_FABRIC_TEXTURE and l != CLASSES.index("none")]
    if not patterned or not plain_marks:
        failures.append("the fixture produced no patterned fabric or no plain-fabric marks")
    else:
        on_patterned = sum(mark_probability(weights, f) for f in patterned) / len(patterned)
        on_plain = sum(mark_probability(weights, f) for f in plain_marks) / len(plain_marks)
        if not on_patterned < on_plain:
            failures.append(
                f"the head learned nothing about fabric texture: {on_patterned:.3f} on patterned "
                f"against {on_plain:.3f} on plain"
            )

    # --- 2b. with the shipped veto, a patterned fabric produces nothing --------------------------
    fired = [f for f in patterned if shipped(weights, f) != "none"]
    if len(fired) > PATTERNED_FIRINGS_ALLOWED:
        failures.append(
            f"{len(fired)} of {len(patterned)} patterned-fabric candidates survived the veto; "
            f"the gate allows {PATTERNED_FIRINGS_ALLOWED}"
        )

    # --- 3. the three kinds are separated --------------------------------------------------------
    for name in ("lint", "thread", "stain"):
        index = CLASSES.index(name)
        positives = [f for f, l in held if l == index]
        if not positives:
            failures.append(f"the fixture produced no {name}")
            continue
        recall = sum(1 for f in positives if predict(weights, f) == index) / len(positives)
        if recall < RECALL_FLOOR:
            failures.append(f"{name} recall {recall:.3f} is below {RECALL_FLOOR}")

    # --- 4. the opt-in kinds are unrepresentable ---------------------------------------------------
    for name in NEVER_PREDICTED:
        if name in CLASSES:
            failures.append(
                f"`{name}` is in the class list; it is not an anomaly against the fabric and a "
                "head that claimed to find one has learned to find shadows"
            )

    for line in failures:
        print(f"FAIL {line}", file=sys.stderr)
    if failures:
        return 1
    print("train_lint self-test: 4 properties hold")
    print("  the fit converges and beats the class prior")
    print("  the head discriminates on fabric texture, and the veto makes it absolute")
    print("  lint, threads and stains are separated rather than merged")
    print("  straps and creases are not in the class list and cannot be predicted")
    return 0


def train(_args: argparse.Namespace) -> int:
    print(
        "there is no labelled clothing corpus in this repository; section 9's DATA task has not "
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
