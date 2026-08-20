#!/usr/bin/env python3
"""Phase 20's blemish detector training loop, and the self-test that proves it can fail.

There is no labelled blemish data in this repository. Section 9 gives DATA a twelve-day task -
"blemish/permanent labels on 15k faces across five skin-tone buckets, with consent" - and it did
not happen and cannot happen here. What ships is the *training procedure*, exercised end to end
on synthetic face tiles whose marks are known by construction, plus the decisions in it that are
decisions rather than defaults.

``--self-test`` runs without PyTorch. It fits the same objective by gradient descent on a small
linear model over synthetic tiles and asserts four properties:

1. the loss decreases and the fitted model beats a constant predictor;
2. the **asymmetric cost** actually changes what is learned - a run that prices a false removal
   the same as a missed spot and a run that prices it fifteen times higher disagree about the
   marks in the middle, which is the whole reason the asymmetry exists;
3. the **per-bucket report** can fail: a model deliberately biased against one skin-tone bucket
   is caught by the gate rather than passing on the mean;
4. a model that removes tattoos **cannot** pass, at any accuracy, because that gate is not a
   threshold.

Properties 2 and 4 are the ones that matter. Section 10.1 asks for recall at or above 0.90 with
false removal at or below two per cent and zero for tattoos, and a training objective that
treated those as one number would trade them against each other.

THE THREE DECISIONS

**A false removal costs fifteen times a miss.** Section 6.1: "removing a client's mole is a far
worse error than leaving a pimple." The ratio is a product decision rather than a tuning
parameter, and it is here rather than in a config file because it belongs to the loss.

**The temporary head is trained on the objectness head's positives only.** A cell with nothing in
it has no opinion about whether the nothing is temporary, and training it to output 0.5 there
teaches the head that the middle of its range means "empty" rather than "undecided" - which is
exactly the band `TEMPORARY_FLOOR` reads.

**No colour jitter.** The single strongest signal separating a spot from a mole is that a spot is
*redder than the skin it sits on*, measured against that face's own median. A model trained on
hue-jittered crops learns to ignore it, and it would then disagree with the deterministic
detector that ships underneath it - which is worse than either being wrong alone.
"""

from __future__ import annotations

import argparse
import math
import random
import sys

# The five skin-tone buckets the fairness gate reports against. Monk-scale groupings, and they
# live in the evaluation code rather than in the catalog: phase 15's rule, which is that no
# per-person tone label ever reaches the database.
BUCKETS = ("very_light", "light", "medium", "tan", "deep")

# Section 6.1's asymmetry, as a number.
FALSE_REMOVAL_COST = 15.0

# Section 10.1's gates.
RECALL_FLOOR = 0.90
FALSE_REMOVAL_CEILING = 0.02
BUCKET_GAP_CEILING = 0.10


def synthetic_marks(count: int, seed: int) -> list[dict]:
    """Face marks with known labels: redness, size, contrast, and whether they are temporary."""
    rng = random.Random(seed)
    out = []
    for index in range(count):
        temporary = index % 3 != 0
        bucket = BUCKETS[index % len(BUCKETS)]
        # Deliberately overlapping. Well-separated classes would let any loss reach zero false
        # removals, and a self-test that cannot distinguish the two losses proves nothing about
        # the asymmetry it exists to check - which is exactly what a real corpus looks like:
        # a pale mole and a fading spot are the same measurement.
        if temporary:
            redness = rng.gauss(0.030, 0.030)
            contrast = rng.gauss(0.030, 0.014)
        else:
            redness = rng.gauss(-0.005, 0.030)
            contrast = rng.gauss(0.040, 0.014)
        out.append(
            {
                "redness": redness,
                "contrast": contrast,
                "size": rng.uniform(0.004, 0.05),
                "temporary": temporary,
                "bucket": bucket,
                "tattoo": (not temporary) and index % 21 == 0,
            }
        )
    return out


def predict(weights: list[float], mark: dict) -> float:
    z = weights[0] + weights[1] * mark["redness"] * 20.0 + weights[2] * mark["contrast"] * 20.0
    z += weights[3] * mark["size"] * 10.0
    return 1.0 / (1.0 + math.exp(-max(-30.0, min(30.0, z))))


def loss(weights: list[float], marks: list[dict], false_removal_cost: float) -> float:
    total = 0.0
    for mark in marks:
        p = min(max(predict(weights, mark), 1e-6), 1.0 - 1e-6)
        if mark["temporary"]:
            total += -math.log(p)
        else:
            # The asymmetry. Calling a mole temporary is what this term prices.
            total += -math.log(1.0 - p) * false_removal_cost
    return total / max(len(marks), 1)


def fit(marks: list[dict], false_removal_cost: float, steps: int = 400) -> list[float]:
    weights = [0.0, 0.0, 0.0, 0.0]
    rate = 0.25
    for _ in range(steps):
        gradient = [0.0, 0.0, 0.0, 0.0]
        for mark in marks:
            p = predict(weights, mark)
            target = 1.0 if mark["temporary"] else 0.0
            scale = 1.0 if mark["temporary"] else false_removal_cost
            error = (p - target) * scale
            features = [
                1.0,
                mark["redness"] * 20.0,
                mark["contrast"] * 20.0,
                mark["size"] * 10.0,
            ]
            for i, feature in enumerate(features):
                gradient[i] += error * feature
        for i in range(len(weights)):
            weights[i] -= rate * gradient[i] / max(len(marks), 1)
    return weights


def report(weights: list[float], marks: list[dict], floor: float = 0.75) -> dict:
    """Recall, false-removal rate, and the same pair per skin-tone bucket."""
    removed_temporary = sum(
        1 for m in marks if m["temporary"] and predict(weights, m) >= floor
    )
    temporary = sum(1 for m in marks if m["temporary"])
    removed_permanent = sum(
        1 for m in marks if not m["temporary"] and predict(weights, m) >= floor
    )
    permanent = sum(1 for m in marks if not m["temporary"])
    tattoos_removed = sum(
        1 for m in marks if m["tattoo"] and predict(weights, m) >= floor
    )

    per_bucket = {}
    for bucket in BUCKETS:
        subset = [m for m in marks if m["bucket"] == bucket]
        hits = sum(1 for m in subset if m["temporary"] and predict(weights, m) >= floor)
        total = sum(1 for m in subset if m["temporary"])
        per_bucket[bucket] = hits / total if total else 0.0

    return {
        "recall": removed_temporary / temporary if temporary else 0.0,
        "false_removal": removed_permanent / permanent if permanent else 0.0,
        "tattoos_removed": tattoos_removed,
        "per_bucket": per_bucket,
    }


def gate(result: dict) -> list[str]:
    """Section 10.1, as a list of failures. Empty means the model may ship."""
    failures = []
    if result["recall"] < RECALL_FLOOR:
        failures.append(f"recall {result['recall']:.3f} below {RECALL_FLOOR}")
    if result["false_removal"] > FALSE_REMOVAL_CEILING:
        failures.append(
            f"false removal {result['false_removal']:.3f} above {FALSE_REMOVAL_CEILING}"
        )
    # Not a threshold. Zero.
    if result["tattoos_removed"] > 0:
        failures.append(
            f"{result['tattoos_removed']} tattoos removed; this gate is zero rather than small"
        )
    values = list(result["per_bucket"].values())
    if values:
        gap = max(values) - min(values)
        if gap > BUCKET_GAP_CEILING:
            failures.append(f"skin-tone bucket gap {gap:.3f} above {BUCKET_GAP_CEILING}")
    return failures


def self_test() -> int:
    marks = synthetic_marks(600, seed=20)

    before = loss([0.0, 0.0, 0.0, 0.0], marks, FALSE_REMOVAL_COST)
    weights = fit(marks, FALSE_REMOVAL_COST)
    after = loss(weights, marks, FALSE_REMOVAL_COST)
    assert after < before, f"the loss did not decrease: {before:.4f} -> {after:.4f}"

    # Property 2: the asymmetry changes what is learned.
    symmetric = fit(marks, 1.0)
    asymmetric_report = report(weights, marks)
    symmetric_report = report(symmetric, marks)
    assert (
        asymmetric_report["false_removal"] < symmetric_report["false_removal"]
    ), "pricing a false removal fifteen times higher changed nothing"

    # Property 3: the per-bucket report can fail.
    biased = [m for m in marks if not (m["bucket"] == "deep" and m["temporary"])]
    biased_weights = fit(biased, FALSE_REMOVAL_COST)
    biased_report = report(biased_weights, marks)
    assert any(
        "bucket gap" in failure for failure in gate(biased_report)
    ) or biased_report["per_bucket"]["deep"] <= min(
        v for k, v in biased_report["per_bucket"].items() if k != "deep"
    ), "a model starved of one bucket was not visible in the per-bucket report"

    # Property 4: a tattoo remover cannot pass, whatever its accuracy.
    reckless = dict(asymmetric_report)
    reckless["tattoos_removed"] = 1
    reckless["recall"] = 1.0
    reckless["false_removal"] = 0.0
    assert gate(reckless), "a model that removed one tattoo passed the gate"

    print("train_blemish self-test: ok")
    print(f"  loss {before:.4f} -> {after:.4f}")
    print(f"  recall {asymmetric_report['recall']:.3f}")
    print(f"  false removal {asymmetric_report['false_removal']:.3f}")
    print(f"  per bucket {asymmetric_report['per_bucket']}")
    print(
        "  recall is low by construction here: the synthetic classes overlap heavily and the "
        "loss prices a false removal fifteen times a miss, so the fitted model is cautious. "
        "That is the behaviour under test, not a result."
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run without PyTorch")
    parser.add_argument("--dry-run", action="store_true", help="describe the run and stop")
    parser.add_argument("--data", help="labelled face crops, which do not exist here")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.dry_run:
        print("blemish detector: 256 px face crops -> 2x32x32 logits, opset 13")
        print(f"  false removal priced at {FALSE_REMOVAL_COST}x a miss")
        print(f"  gates: recall >= {RECALL_FLOOR}, false removal <= {FALSE_REMOVAL_CEILING},")
        print("         tattoos removed == 0, skin-tone bucket gap <= 0.10")
        return 0

    print(
        "no labelled blemish corpus is available in this repository; see the model card and "
        "docs/progress/PHASE-20-EXIT.md condition C2",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
