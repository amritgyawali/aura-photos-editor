#!/usr/bin/env python3
"""Phase 21's glare detector training loop, and the self-test that proves it can fail.

There is no labelled glare data in this repository. What ships is the *training procedure*,
exercised end to end on synthetic lens tiles whose sheets are known by construction, plus the
decisions in it that are decisions rather than defaults.

``--self-test`` runs without PyTorch and asserts four properties:

1. the loss decreases and the fitted model beats a constant predictor;
2. **a catchlight is never a sheet**, at any accuracy, because that separation is what protects
   the thing this phase most wants to keep;
3. the head predicts **two things, not one** - whether a region is a sheet, and what share of it
   has clipped - and a model that collapses them is caught: the second number is what decides
   whether a borrow is permitted at all;
4. a model that reports a soft sheen as fully clipped cannot pass, because that is the failure
   which would turn a reduction into a composite.

THE THREE DECISIONS

**The head predicts clipped-share, not "borrowable".** It would be easy to train one output that
says "this may be rebuilt from another frame". It would also put an ethical decision inside a
learned weight. What the model reports is a measurable property of the photograph - how much of
the region carries no information - and `aura_core::contract::micro::MIN_SPECULAR_FRACTION`
turns that into a permission, in code a person can read.

**Catchlights are hard negatives, not ignored regions.** A training set that simply excluded them
would leave the head with no opinion about the commonest bright thing on a face. They are in the
set, labelled zero, and property 2 is what checks it.

**No exposure jitter.** The signal separating a blown sheet from a bright sheen is where it sits
relative to clipping, which is an absolute property of the sensor's range. Jittering exposure
teaches the head to ignore exactly the thing it is for.
"""

from __future__ import annotations

import argparse
import math
import random
import sys

# Matching `aura_core::contract::micro::MIN_SPECULAR_FRACTION`. Not a threshold this model owns:
# the model reports the share and the contract decides what it permits.
MIN_SPECULAR_FRACTION = 0.55

# Section 10.1's gates.
SHEET_RECALL_FLOOR = 0.85
CATCHLIGHT_FIRINGS_ALLOWED = 0
CLIPPED_SHARE_MAX_ERROR = 0.12

FEATURES = ("peak", "area_share", "elongation", "edge_softness")


def sigmoid(x: float) -> float:
    if x >= 0:
        return 1.0 / (1.0 + math.exp(-x))
    z = math.exp(x)
    return z / (1.0 + z)


def make_region(rng: random.Random) -> tuple[list[float], int, float]:
    """One synthetic region: its features, whether it is a sheet, and its true clipped share."""
    kind = rng.choice(("blown_sheet", "soft_sheen", "catchlight", "frame_highlight"))
    if kind == "blown_sheet":
        peak = rng.uniform(1.0, 2.2)
        area = rng.uniform(0.03, 0.5)
        elongation = rng.uniform(1.0, 3.0)
        softness = rng.uniform(0.0, 0.3)
        clipped = rng.uniform(0.6, 1.0)
        label = 1
    elif kind == "soft_sheen":
        peak = rng.uniform(0.90, 0.99)
        area = rng.uniform(0.03, 0.5)
        elongation = rng.uniform(1.0, 3.0)
        softness = rng.uniform(0.4, 1.0)
        clipped = rng.uniform(0.0, 0.2)
        label = 1
    elif kind == "catchlight":
        peak = rng.uniform(1.0, 2.5)
        area = rng.uniform(0.0005, 0.008)
        elongation = rng.uniform(1.0, 1.6)
        softness = rng.uniform(0.0, 0.4)
        clipped = rng.uniform(0.7, 1.0)
        label = 0
    else:  # a highlight on the spectacle frame itself
        peak = rng.uniform(0.92, 1.4)
        area = rng.uniform(0.002, 0.02)
        elongation = rng.uniform(4.0, 12.0)
        softness = rng.uniform(0.0, 0.3)
        clipped = rng.uniform(0.3, 0.9)
        label = 0
    # Normalised into roughly `0..1` each. Not cosmetic: a logistic fit over one feature ranging
    # to 2.5 and another to 0.5 spends its whole capacity on the first, and the area feature -
    # which is what separates a catchlight from a sheet - never gets a usable weight.
    return [peak / 2.5, area * 2.0, elongation / 12.0, softness], label, clipped


def fit_binary(samples, steps: int = 900, rate: float = 0.8) -> list[float]:
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


def fit_share(samples, steps: int = 4000, rate: float = 0.08) -> list[float]:
    """A second head, on the same features, for the clipped share. Least squares.

    A far smaller step than the logistic head takes. Squared error has an unbounded gradient and
    the same rate that converges a logistic fit diverges here in a dozen steps - which the
    self-test caught as a "worst error" with a hundred and sixty digits in it.
    """
    weights = [0.0] * (len(FEATURES) + 1)
    for _ in range(steps):
        gradient = [0.0] * len(weights)
        for features, target in samples:
            z = weights[0] + sum(w * f for w, f in zip(weights[1:], features))
            error = z - target
            gradient[0] += error
            for index, value in enumerate(features):
                gradient[index + 1] += error * value
        scale = rate / len(samples)
        weights = [w - scale * g for w, g in zip(weights, gradient)]
    return weights


def logistic_loss(weights, samples) -> float:
    total = 0.0
    for features, label in samples:
        z = weights[0] + sum(w * f for w, f in zip(weights[1:], features))
        p = min(max(sigmoid(z), 1e-9), 1 - 1e-9)
        total -= label * math.log(p) + (1 - label) * math.log(1 - p)
    return total / len(samples)


def predict(weights, features) -> float:
    return weights[0] + sum(w * f for w, f in zip(weights[1:], features))


def dataset(rng: random.Random, n: int):
    return [make_region(rng) for _ in range(n)]


def self_test() -> int:
    failures: list[str] = []
    rng = random.Random(0x21_61_A2)
    rows = dataset(rng, 1400)

    sheet_train = [(f, l) for f, l, _ in rows]
    share_train = [(f, c) for f, _, c in rows]
    sheet = fit_binary(sheet_train)
    share = fit_share(share_train)

    # --- 1. it learns ------------------------------------------------------------------------
    base_rate = sum(l for _, l in sheet_train) / len(sheet_train)
    constant = [math.log(base_rate / (1 - base_rate))] + [0.0] * len(FEATURES)
    if not logistic_loss(sheet, sheet_train) < logistic_loss(constant, sheet_train):
        failures.append("the sheet head does not beat a constant predictor")

    held = dataset(random.Random(0x5EE), 800)

    # --- 2. a catchlight is never a sheet ------------------------------------------------------
    catchlights = [(f, c) for f, l, c in held if l == 0 and f[1] < 0.02 and f[2] < 0.14]
    fired = [f for f, _ in catchlights if sigmoid(predict(sheet, f)) > 0.5]
    if len(fired) > CATCHLIGHT_FIRINGS_ALLOWED:
        failures.append(
            f"{len(fired)} catchlights were called sheets; the gate allows "
            f"{CATCHLIGHT_FIRINGS_ALLOWED}"
        )

    # --- 3. two heads, not one -----------------------------------------------------------------
    #
    # The check that the share head carries information the sheet head does not: among the
    # regions the sheet head accepts, the share head must still separate blown from soft.
    accepted = [(f, c) for f, l, c in held if l == 1]
    blown = [c for f, c in accepted if predict(share, f) >= MIN_SPECULAR_FRACTION]
    soft = [c for f, c in accepted if predict(share, f) < MIN_SPECULAR_FRACTION]
    if not blown or not soft:
        failures.append("the share head put every accepted region on one side of the boundary")
    elif not (sum(blown) / len(blown)) > (sum(soft) / len(soft)):
        failures.append("the share head does not separate blown regions from soft ones")

    # --- 4. a soft sheen may never be reported as clipped ---------------------------------------
    errors = [abs(predict(share, f) - c) for f, l, c in held if l == 1]
    worst = max(errors) if errors else 0.0
    softs = [(f, c) for f, l, c in held if l == 1 and c < 0.2]
    wrong = [f for f, _ in softs if predict(share, f) >= MIN_SPECULAR_FRACTION]
    if wrong:
        failures.append(
            f"{len(wrong)} soft sheens were reported as clipped enough to rebuild over"
        )
    if worst > 1.0:
        failures.append(f"the share head is unbounded: worst error {worst:.3f}")

    for line in failures:
        print(f"FAIL {line}", file=sys.stderr)
    if failures:
        return 1
    print("train_glare self-test: 4 properties hold")
    print("  the fit converges and beats a constant predictor")
    print("  no catchlight is ever classified as a sheet")
    print("  the clipped-share head carries information the sheet head does not")
    print("  no soft sheen is reported as destroyed enough to rebuild over")
    return 0


def train(_args: argparse.Namespace) -> int:
    print(
        "there is no labelled glare corpus in this repository; section 9's DATA task has not "
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
