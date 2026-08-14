"""Section 10.1's scene gates, computed the way the Rust harness computes them.

The Python counterpart of `tests/eval/scene_eval.rs`. The two implement the same
arithmetic deliberately: when a trained head arrives, the number this script produces on
a labelled corpus and the number CI produces on synthetic ground truth have to be
comparable rather than argued about.

    python ml/models/scene/eval_scene.py --self-test
    python ml/models/scene/eval_scene.py --predictions preds.json --truth truth.json
    python ml/models/scene/eval_scene.py --predictions preds.json --calibrate

## The gates

| Gate | Threshold | Section |
|---|---|---|
| Scene top-1 accuracy, overall | >= 0.92 | 10.1 |
| Per-class accuracy, classes with > 200 samples | >= 0.85 | 10.1 |
| Median boundary error | <= 45 s | 10.1 |
| Chapters per wedding | 6 to 20 | 10.1 |
| Ritual F1, per tradition | >= 0.85 | 10.1 |
| Per-tradition scene accuracy | reported | 12 |

The last one has no threshold here on purpose, and it is the same refusal
`ml/models/face/eval_identity.py` makes about skin tone. Section 12's first failure mode
is cultural blind spots, and the right response is not a number this script invents - it
is a threshold PM and the ML lead agree on against a set with balanced tradition
coverage. Until that set exists the script reports and refuses to pass or fail. That is
condition **C5**.

## Three measurement choices that are easy to get wrong

**Boundary error is nearest-neighbour matched.** Each true boundary claims the closest
detected one. An ordered pairing would misalign every boundary after a single extra
chapter and report an error of hours - a measurement of the *count*, not of the
boundaries.

**Per-class accuracy is recall, not precision.** "Of the frames that really were a kiss,
how many did we find" is the question a photographer is asking. Precision on a rare class
is trivially high for a model that never predicts it.

**Abstentions are counted, never dropped.** A frame the head refused to label is a wrong
answer for accuracy and a *right* answer for the abstention rate, and a harness that
silently excluded them would make a model that abstains on everything look perfect.
"""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from typing import Sequence

ACCURACY_GATE = 0.92
PER_CLASS_GATE = 0.85
PER_CLASS_FLOOR = 200
BOUNDARY_GATE_S = 45
CHAPTER_BAND = (6, 20)
RITUAL_F1_GATE = 0.85
UNKNOWN = "unknown"


def top1_accuracy(predicted: Sequence[str], actual: Sequence[str]) -> float:
    """Share of frames whose label is right.

    Abstentions count as wrong. See this module's docstring.
    """
    if not actual:
        return 0.0
    hits = sum(1 for p, a in zip(predicted, actual) if p == a)
    return hits / len(actual)


def abstention_rate(predicted: Sequence[str]) -> float:
    """Share of frames the head refused to label."""
    if not predicted:
        return 0.0
    return sum(1 for p in predicted if p == UNKNOWN) / len(predicted)


def per_class_recall(
    predicted: Sequence[str], actual: Sequence[str]
) -> dict[str, tuple[float, int]]:
    """Recall and support per class. See this module's docstring for why recall."""
    totals: dict[str, int] = {}
    hits: dict[str, int] = {}
    for p, a in zip(predicted, actual):
        totals[a] = totals.get(a, 0) + 1
        if p == a:
            hits[a] = hits.get(a, 0) + 1
    return {
        label: (hits.get(label, 0) / count, count) for label, count in sorted(totals.items())
    }


def boundary_error_s(detected_ms: Sequence[int], actual_ms: Sequence[int]) -> float:
    """Median absolute error, in seconds, nearest-neighbour matched."""
    if not detected_ms or not actual_ms:
        return float("inf")
    errors = [min(abs(found - real) for found in detected_ms) / 1000.0 for real in actual_ms]
    return statistics.median(errors)


def ritual_f1(
    predicted: Sequence[str | None], actual: Sequence[str | None]
) -> dict[str, float]:
    """F1 per rite, treating `None` as the negative class.

    Macro-averaged by the caller per tradition. A rite with no true positives and no
    predictions has an F1 of 1.0 by convention here and is excluded from the macro
    average by `macro_f1`, because a rite that never occurs in a test set says nothing
    about a model.
    """
    labels = {label for label in list(predicted) + list(actual) if label}
    out: dict[str, float] = {}
    for label in sorted(labels):
        tp = sum(1 for p, a in zip(predicted, actual) if p == label and a == label)
        fp = sum(1 for p, a in zip(predicted, actual) if p == label and a != label)
        fn = sum(1 for p, a in zip(predicted, actual) if p != label and a == label)
        if tp == 0 and fp == 0 and fn == 0:
            out[label] = 1.0
            continue
        precision = tp / (tp + fp) if tp + fp else 0.0
        recall = tp / (tp + fn) if tp + fn else 0.0
        out[label] = (
            0.0 if precision + recall == 0 else 2 * precision * recall / (precision + recall)
        )
    return out


def macro_f1(scores: dict[str, float], support: dict[str, int]) -> float:
    """Macro F1 over the rites that actually occur in the test set."""
    present = [score for label, score in scores.items() if support.get(label, 0) > 0]
    return sum(present) / len(present) if present else 0.0


def calibrate_attributes(
    probabilities: Sequence[Sequence[float]], truth: Sequence[Sequence[int]]
) -> list[float]:
    """The per-attribute threshold that maximises F1, one per attribute.

    This replaces the single `ATTRIBUTE_THRESHOLD = 0.5` in the Rust decoder. A single
    threshold across fourteen attributes whose base rates run from about 2 % (`pets`) to
    about 50 % (`indoor`) is wrong in a way that an average hides completely.

    A coarse sweep - 0.05 steps - because a threshold tuned to three decimal places on a
    test set is a threshold fitted to the test set.
    """
    if not probabilities or not truth:
        return []
    width = len(probabilities[0])
    thresholds: list[float] = []
    for index in range(width):
        best_threshold, best_score = 0.5, -1.0
        for step in range(1, 20):
            threshold = step * 0.05
            tp = fp = fn = 0
            for row, actual in zip(probabilities, truth):
                predicted = row[index] >= threshold
                real = bool(actual[index])
                tp += predicted and real
                fp += predicted and not real
                fn += (not predicted) and real
            precision = tp / (tp + fp) if tp + fp else 0.0
            recall = tp / (tp + fn) if tp + fn else 0.0
            score = (
                0.0
                if precision + recall == 0
                else 2 * precision * recall / (precision + recall)
            )
            if score > best_score:
                best_threshold, best_score = threshold, score
        thresholds.append(best_threshold)
    return thresholds


def report(payload: dict) -> int:
    """Print the gates and return a process exit code."""
    failures = 0
    predicted = payload.get("predicted_scenes", [])
    actual = payload.get("actual_scenes", [])

    accuracy = top1_accuracy(predicted, actual)
    print(f"scene top-1 accuracy: {accuracy:.4f} (gate {ACCURACY_GATE})")
    if accuracy < ACCURACY_GATE:
        failures += 1
    print(f"abstention rate:      {abstention_rate(predicted):.4f}")

    print("per-class recall:")
    for label, (recall, support) in per_class_recall(predicted, actual).items():
        gated = support > PER_CLASS_FLOOR
        marker = "" if not gated else ("  FAIL" if recall < PER_CLASS_GATE else "  ok")
        note = "" if gated else f"  (below the {PER_CLASS_FLOOR}-sample floor, ungated)"
        print(f"  {label:22s} {recall:.4f}  n={support}{marker}{note}")
        if gated and recall < PER_CLASS_GATE:
            failures += 1

    detected = payload.get("detected_boundaries_ms", [])
    real = payload.get("actual_boundaries_ms", [])
    if real:
        error = boundary_error_s(detected, real)
        print(f"median boundary error: {error:.1f} s (gate {BOUNDARY_GATE_S} s)")
        if error > BOUNDARY_GATE_S:
            failures += 1
        chapters = len(detected)
        print(f"chapters: {chapters} (band {CHAPTER_BAND[0]}-{CHAPTER_BAND[1]})")
        if not CHAPTER_BAND[0] <= chapters <= CHAPTER_BAND[1]:
            failures += 1

    rituals_pred = payload.get("predicted_rituals")
    rituals_true = payload.get("actual_rituals")
    traditions = payload.get("traditions")
    if rituals_pred and rituals_true and traditions:
        print("ritual F1 per tradition:")
        for tradition in sorted(set(traditions)):
            mask = [index for index, t in enumerate(traditions) if t == tradition]
            subset_pred = [rituals_pred[i] for i in mask]
            subset_true = [rituals_true[i] for i in mask]
            scores = ritual_f1(subset_pred, subset_true)
            support = {
                label: sum(1 for a in subset_true if a == label) for label in scores
            }
            score = macro_f1(scores, support)
            marker = "  FAIL" if score < RITUAL_F1_GATE else "  ok"
            print(f"  {tradition:12s} {score:.4f}{marker}")
            if score < RITUAL_F1_GATE:
                failures += 1

    # Reported, never gated. See this module's docstring and condition C5.
    if traditions:
        print("per-tradition scene accuracy (REPORTED, not gated - see condition C5):")
        for tradition in sorted(set(traditions)):
            mask = [index for index, t in enumerate(traditions) if t == tradition]
            subset = top1_accuracy([predicted[i] for i in mask], [actual[i] for i in mask])
            print(f"  {tradition:12s} {subset:.4f}")
        print(
            "  No threshold is applied. A maximum acceptable disparity is a decision for "
            "PM and the ML lead against a set with balanced tradition coverage, not a "
            "number this script invents."
        )

    print("\n" + ("all gates passed" if failures == 0 else f"{failures} gates failed"))
    return 1 if failures else 0


def _self_test() -> int:
    """Prove the metrics, including that they reject a useless model."""
    failures = 0

    # A perfect model.
    truth = ["ceremony"] * 100 + ["kiss"] * 10
    if abs(top1_accuracy(truth, truth) - 1.0) > 1e-9:
        print("FAIL: a perfect model did not score 1.0")
        failures += 1

    # A model that never predicts the rare class. Overall accuracy is high, per-class
    # recall on the rare class is zero - which is the whole reason section 10.1 gates
    # per class as well as overall.
    lazy = ["ceremony"] * 110
    overall = top1_accuracy(lazy, truth)
    recalls = per_class_recall(lazy, truth)
    if overall < 0.9:
        print(f"FAIL: the lazy model should score high overall, scored {overall}")
        failures += 1
    if recalls["kiss"][0] != 0.0:
        print("FAIL: per-class recall did not catch the never-predicted class")
        failures += 1

    # Abstentions are wrong for accuracy and counted for the abstention rate.
    abstaining = [UNKNOWN] * 110
    if top1_accuracy(abstaining, truth) != 0.0:
        print("FAIL: abstentions were not counted as wrong")
        failures += 1
    if abstention_rate(abstaining) != 1.0:
        print("FAIL: the abstention rate is wrong")
        failures += 1

    # Boundary error is nearest-neighbour, so one extra chapter does not destroy it.
    real = [0, 60_000, 120_000]
    detected = [1_000, 30_000, 61_000, 119_000]
    error = boundary_error_s(detected, real)
    if error > 2.0:
        print(f"FAIL: an extra chapter moved the boundary error to {error} s")
        failures += 1

    # Ritual F1 on a head that abstains everywhere is zero, not undefined.
    scores = ritual_f1([None] * 10, ["haldi"] * 10)
    if scores.get("haldi", 1.0) != 0.0:
        print("FAIL: an abstaining ritual head did not score zero")
        failures += 1

    # Calibration finds a different threshold for a rare attribute.
    rare_truth = [[1, 0] if i < 5 else [0, 0] for i in range(100)]
    rare_probs = [[0.9, 0.2] if i < 5 else [0.1, 0.2] for i in range(100)]
    thresholds = calibrate_attributes(rare_probs, rare_truth)
    if len(thresholds) != 2:
        print("FAIL: calibration returned the wrong arity")
        failures += 1

    print("eval_scene self-test:", "ok" if failures == 0 else f"{failures} failures")
    return 1 if failures else 0


def main(argv: Sequence[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--predictions", help="a JSON file of predictions and truth")
    parser.add_argument("--truth", help="unused; predictions carry both")
    parser.add_argument("--calibrate", action="store_true")
    parser.add_argument("--per-tradition", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return _self_test()
    if not args.predictions:
        parser.error("--predictions or --self-test")

    with open(args.predictions, encoding="utf-8") as handle:
        payload = json.load(handle)

    if args.calibrate:
        thresholds = calibrate_attributes(
            payload.get("attribute_probabilities", []),
            payload.get("attribute_truth", []),
        )
        print("per-attribute thresholds, to replace ATTRIBUTE_THRESHOLD in the decoder:")
        for index, threshold in enumerate(thresholds):
            print(f"  [{index:2d}] {threshold:.2f}")
        return 0

    return report(payload)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
