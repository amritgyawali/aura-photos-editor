#!/usr/bin/env python3
"""Section 10.1's emotion metrics, and the proof that they reject a useless reader.

The Python half of the phase 10 gates. `tests/eval/emotion_eval.rs` computes the same
arithmetic in Rust against the same synthetic fixtures; this file exists so that the day
a real dataset lands, the number a training run reports and the number the product's
gate reports can be *compared* rather than argued about.

Four metrics, in the order section 10.1 lists them:

* **pairwise agreement** - the fraction of photographer comparisons the ranker
  reproduces. Bradley-Terry's own likelihood, thresholded, which is the honest reading of
  "which of these two would you deliver";
* **peak recall** - the fraction of moments whose chosen frame is in the human top two;
* **per-channel F1** - tears and laughter, with tears also reported at its precision
  because a false tear is worse than a missed one;
* **reaction recall and spuriousness** - how many human-identified pairs are found, and
  how many links are not human-identified.

Three modes:

    python eval_emotion.py --self-test          prove the metrics reject a useless reader
    python eval_emotion.py --fit-ranker FILE    fit the Bradley-Terry coefficients
    python eval_emotion.py --fit-calibration F  fit the per-scene isotonic maps (PAVA)

`--fit-calibration` is what would close **condition C4**: the calibration layer ships as
the identity because there is no labelled keeper/reject data here to fit it on, and the
machinery to fit it is real and tested.

No third-party imports. Everything here is arithmetic over lists, for the reason
`eval_integrity.py` gives: a metric script that needs a scientific stack installed is a
metric script somebody runs once.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from typing import Iterable, Sequence

# The eight expression channels, in `FaceExpression::channels` order. Renaming or
# reordering this relabels a trained head; the Rust contract says so and this repeats it.
CHANNELS = (
    "smile",
    "genuineness",
    "laughter",
    "tears",
    "surprise",
    "tenderness",
    "composed",
    "discomfort",
)

# The nine ranker features, in `Ranker::FEATURE_NAMES` order.
FEATURES = (
    "subject_expression",
    "genuineness",
    "interaction",
    "peak",
    "reaction",
    "mutual_gaze",
    "composure",
    "tears",
    "group",
)

# Section 10.1's thresholds, in one place.
AGREEMENT_GATE = 0.80
PEAK_RECALL_GATE = 0.90
EXPRESSION_F1_GATE = 0.85
TEARS_PRECISION_GATE = 0.90
REACTION_RECALL_GATE = 0.80
REACTION_SPURIOUS_GATE = 0.10

# Section 12's fourth failure mode, and the same constant three places in the product
# read: `aura_core::contract::emotion::TEARS_CERTAIN`.
TEARS_CERTAIN = 0.85


# ---------------------------------------------------------------------------
# Metrics
# ---------------------------------------------------------------------------


def sigmoid(x: float) -> float:
    """The Bradley-Terry link."""
    if x >= 0:
        return 1.0 / (1.0 + math.exp(-x))
    # The numerically stable branch. `exp` of a large positive number overflows and a
    # metric script that returns `nan` on a confident model is worse than no script.
    z = math.exp(x)
    return z / (1.0 + z)


def utility(features: Sequence[float], weights: Sequence[float], bias: float) -> float:
    return bias + sum(w * f for w, f in zip(weights, features))


def pairwise_agreement(
    comparisons: Iterable[tuple[Sequence[float], Sequence[float]]],
    weights: Sequence[float],
    bias: float,
) -> float:
    """The fraction of comparisons whose winner the ranker scores higher.

    Each comparison is `(winner_features, loser_features)`. Ties count as disagreements,
    deliberately: a ranker that cannot separate two frames has not reproduced the
    photographer's preference, and counting a tie as half would make a constant ranker
    score 0.5 rather than 0.
    """
    total = 0
    agreed = 0
    for winner, loser in comparisons:
        total += 1
        if utility(winner, weights, bias) > utility(loser, weights, bias):
            agreed += 1
    return agreed / total if total else 0.0


def peak_recall(chosen: Sequence[int], human_top_two: Sequence[Sequence[int]]) -> float:
    """Section 10.1: the chosen frame is within the human-chosen top two."""
    total = min(len(chosen), len(human_top_two))
    if total == 0:
        return 0.0
    hits = sum(1 for i in range(total) if chosen[i] in human_top_two[i])
    return hits / total


def confusion(predicted: Sequence[bool], actual: Sequence[bool]) -> tuple[int, int, int]:
    """`(true positives, false positives, false negatives)`."""
    tp = fp = fn = 0
    for p, a in zip(predicted, actual):
        if p and a:
            tp += 1
        elif p and not a:
            fp += 1
        elif a and not p:
            fn += 1
    return tp, fp, fn


def precision_recall_f1(tp: int, fp: int, fn: int) -> tuple[float, float, float]:
    precision = tp / (tp + fp) if (tp + fp) else 1.0
    recall = tp / (tp + fn) if (tp + fn) else 1.0
    f1 = 2 * precision * recall / (precision + recall) if (precision + recall) else 0.0
    return precision, recall, f1


def reaction_scores(
    found: Iterable[tuple[str, str]], expected: Iterable[tuple[str, str]]
) -> tuple[float, float]:
    """`(recall, spurious rate)` over `(action, reaction)` pairs."""
    found_set = set(found)
    expected_set = set(expected)
    if not expected_set:
        return 0.0, 0.0
    recall = len(found_set & expected_set) / len(expected_set)
    spurious = (
        len(found_set - expected_set) / len(found_set) if found_set else 0.0
    )
    return recall, spurious


# ---------------------------------------------------------------------------
# Fitting
# ---------------------------------------------------------------------------


def fit_ranker(
    comparisons: Sequence[tuple[Sequence[float], Sequence[float]]],
    epochs: int = 400,
    learning_rate: float = 0.25,
    l2: float = 1e-3,
) -> tuple[list[float], float]:
    """Fit a Bradley-Terry utility by gradient ascent on the log likelihood.

    `P(winner beats loser) = sigmoid(u(winner) - u(loser))`, so the gradient of the log
    likelihood with respect to `w` is `(1 - p) * (f_winner - f_loser)`. Plain full-batch
    gradient ascent, deterministic, with a small ridge - a fitting routine somebody has to
    be able to read is worth more here than one that converges two epochs sooner.

    The bias is fitted too, and it matters: section 6.4's calibration is what makes the
    score comparable with phase 09's `technical_score`, and a bias fitted to the data is
    what stops the median frame of a wedding scoring half a mark.
    """
    n = len(FEATURES)
    weights = [0.0] * n
    bias = 0.0
    if not comparisons:
        return weights, bias

    for _ in range(epochs):
        gradient = [0.0] * n
        bias_gradient = 0.0
        for winner, loser in comparisons:
            margin = utility(winner, weights, bias) - utility(loser, weights, bias)
            residual = 1.0 - sigmoid(margin)
            for i in range(n):
                gradient[i] += residual * (winner[i] - loser[i])
            # The bias cancels in the difference, which is exactly right for the
            # comparisons and leaves it unidentified. It is set afterwards, below.
        scale = learning_rate / len(comparisons)
        for i in range(n):
            weights[i] += scale * gradient[i] - l2 * weights[i]
        bias += scale * bias_gradient

    # The bias is not identifiable from pairwise comparisons - it cancels in every
    # difference - so it is set from a separate requirement: the *median* frame in the
    # comparison set should score near 0.12 rather than near 0.5. A ranking in which
    # everything is a half carries no information and looks like a working one.
    everything = [utility(f, weights, 0.0) for pair in comparisons for f in pair]
    everything.sort()
    median = everything[len(everything) // 2] if everything else 0.0
    target = math.log(0.12 / (1.0 - 0.12))
    bias = target - median
    return weights, bias


def fit_isotonic(points: Sequence[tuple[float, float]], knots: int = 11) -> tuple[list[float], list[float]]:
    """Pool-adjacent-violators, then resample onto `knots` evenly spaced inputs.

    `points` is `(raw score, label)` where the label is 1 for a keeper and 0 for a
    reject. The output is the `x`/`y` arrays `emotion_weights.toml`'s `[calibration.*]`
    tables take.

    The resampling is what makes the result a *config file* rather than a model: eleven
    knots is a table a person can read and argue with, and the map between them is
    linear, which is what `Calibration::apply` implements.
    """
    if not points:
        return [], []
    ordered = sorted(points, key=lambda p: p[0])
    values = [float(label) for _, label in ordered]
    counts = [1] * len(values)

    # PAVA: walk left to right, pooling any block that violates monotonicity.
    i = 0
    while i < len(values) - 1:
        if values[i] <= values[i + 1]:
            i += 1
            continue
        total = values[i] * counts[i] + values[i + 1] * counts[i + 1]
        weight = counts[i] + counts[i + 1]
        values[i] = total / weight
        counts[i] = weight
        del values[i + 1]
        del counts[i + 1]
        if i > 0:
            i -= 1

    # Expand the pooled blocks back to per-point fitted values.
    fitted: list[float] = []
    for value, count in zip(values, counts):
        fitted.extend([value] * count)

    xs = [p[0] for p in ordered]
    out_x: list[float] = []
    out_y: list[float] = []
    for k in range(knots):
        target = k / (knots - 1)
        # The fitted value at the largest input at or below the knot.
        best = 0.0
        for x, y in zip(xs, fitted):
            if x <= target:
                best = y
            else:
                break
        out_x.append(round(target, 4))
        out_y.append(round(min(1.0, max(0.0, best)), 4))

    # Monotonicity is a property of PAVA's output, but the resample can flatten it into a
    # non-decreasing sequence with a rounding artefact. The loader refuses a non-monotone
    # map with `AURA-ML-5039`, so it is enforced here rather than discovered there.
    for i in range(1, len(out_y)):
        out_y[i] = max(out_y[i], out_y[i - 1])
    return out_x, out_y


# ---------------------------------------------------------------------------
# The self-test
# ---------------------------------------------------------------------------


def self_test() -> int:
    """Prove the metrics reject a reader that learned nothing.

    The guard phases 06 to 09 each wrote, for the fifth time. A metric that cannot fail is
    not a metric, and an agreement score is specifically vulnerable to a ranker that
    returns a constant - it scores 0.0 rather than 0.5 only because ties are counted as
    disagreements, which is the decision `pairwise_agreement` documents.
    """
    failures = 0

    # A separable comparison set: the winner is stronger on feature 0.
    good = [([0.9] + [0.1] * 8, [0.1] * 9) for _ in range(20)]
    weights, bias = fit_ranker(good)
    agreement = pairwise_agreement(good, weights, bias)
    print(f"fitted ranker agreement: {agreement:.3f}")
    if agreement < AGREEMENT_GATE:
        print(f"  FAIL: a separable set should be reproducible above {AGREEMENT_GATE}")
        failures += 1

    constant = [0.0] * len(FEATURES)
    flat = pairwise_agreement(good, constant, 0.0)
    print(f"constant ranker agreement: {flat:.3f}")
    if flat >= AGREEMENT_GATE:
        print("  FAIL: a constant ranker passed the agreement gate")
        failures += 1

    inverted = [-w for w in weights]
    upside_down = pairwise_agreement(good, inverted, -bias)
    print(f"inverted ranker agreement: {upside_down:.3f}")
    if upside_down >= 0.5:
        print("  FAIL: an inverted ranker did not score below a coin toss")
        failures += 1

    # A flag-everything tear detector must fail on precision, not merely on F1.
    predicted = [True] * 10
    actual = [True] + [False] * 9
    tp, fp, fn = confusion(predicted, actual)
    precision, _, f1 = precision_recall_f1(tp, fp, fn)
    print(f"flag-everything tears: precision {precision:.3f}, F1 {f1:.3f}")
    if precision >= TEARS_PRECISION_GATE:
        print("  FAIL: a flag-everything detector passed the tears precision gate")
        failures += 1

    # Peak recall must fall when the chosen frame drifts out of the human top two.
    good_peaks = peak_recall([3, 4, 2], [[3, 4], [4, 5], [1, 2]])
    bad_peaks = peak_recall([0, 0, 0], [[3, 4], [4, 5], [1, 2]])
    print(f"peak recall: good {good_peaks:.3f}, drifted {bad_peaks:.3f}")
    if good_peaks < PEAK_RECALL_GATE or bad_peaks >= PEAK_RECALL_GATE:
        print("  FAIL: peak recall does not separate a good chooser from a bad one")
        failures += 1

    # Reaction linking: a linker that links everything must fail on spuriousness.
    everything = [("a", str(i)) for i in range(20)]
    expected = [("a", "1"), ("a", "2")]
    recall, spurious = reaction_scores(everything, expected)
    print(f"link-everything: recall {recall:.3f}, spurious {spurious:.3f}")
    if spurious < REACTION_SPURIOUS_GATE:
        print("  FAIL: a link-everything linker passed the spuriousness gate")
        failures += 1

    # The isotonic fit refuses to reorder frames.
    xs, ys = fit_isotonic([(0.1, 0.0), (0.3, 0.0), (0.5, 1.0), (0.9, 1.0)])
    monotone = all(ys[i] <= ys[i + 1] for i in range(len(ys) - 1))
    print(f"isotonic knots: x={xs} y={ys} monotone={monotone}")
    if not monotone:
        print("  FAIL: the fitted calibration reorders frames")
        failures += 1

    print()
    if failures:
        print(f"self-test: {failures} check(s) failed")
        return 1
    print("self-test: the metrics reject a reader that learned nothing")
    return 0


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="prove the metrics reject a reader that learned nothing",
    )
    parser.add_argument(
        "--fit-ranker",
        metavar="FILE",
        help="fit Bradley-Terry coefficients from a JSON list of "
        '{"winner": [9 features], "loser": [9 features]}',
    )
    parser.add_argument(
        "--fit-calibration",
        metavar="FILE",
        help="fit per-scene isotonic maps from a JSON object of "
        '{"scene": [[score, keeper], ...]}; closes condition C4',
    )
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    if args.fit_ranker:
        with open(args.fit_ranker, encoding="utf-8") as handle:
            rows = json.load(handle)
        comparisons = [(row["winner"], row["loser"]) for row in rows]
        weights, bias = fit_ranker(comparisons)
        agreement = pairwise_agreement(comparisons, weights, bias)
        print("# paste into crates/aura-brain-wedding/config/emotion_weights.toml")
        print("[ranker]")
        print(f'rationale = "fitted on {len(comparisons)} comparisons; agreement {agreement:.3f}"')
        print(f"bias = {bias:.2f}")
        for name, value in zip(FEATURES, weights):
            print(f"{name} = {value:.2f}")
        print()
        print(f"# in-sample agreement: {agreement:.3f} (gate {AGREEMENT_GATE})")
        return 0 if agreement >= AGREEMENT_GATE else 1

    if args.fit_calibration:
        with open(args.fit_calibration, encoding="utf-8") as handle:
            by_scene = json.load(handle)
        print("# paste into crates/aura-brain-wedding/config/emotion_weights.toml")
        for scene, points in sorted(by_scene.items()):
            xs, ys = fit_isotonic([(p[0], p[1]) for p in points])
            print(f"[calibration.{scene}]")
            print(f'rationale = "fitted on {len(points)} labelled frames"')
            print(f"x = {xs}")
            print(f"y = {ys}")
            print()
        return 0

    parser.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
