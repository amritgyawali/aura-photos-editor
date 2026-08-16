#!/usr/bin/env python3
"""Evaluate Phase 11 geometry, crop, background, ranking and calibration gates.

This is the Python side of ``tests/eval/composition_eval.rs``. It deliberately uses
only the standard library so a dataset curator can run it before installing a training
stack. Real evaluation input is one JSON object with ``tilts``, ``crops``,
``head_merges``, ``pairs`` and optional ``calibration`` arrays; run ``--schema`` for the
exact shape.

``--self-test`` proves every metric rejects a degenerate implementation. It is not a
quality claim about the placeholder weights shipped by this repository.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from collections.abc import Iterable, Sequence
from pathlib import Path
from typing import Any

HORIZON_ERROR_GATE_DEG = 0.40
CROP_F1_GATE = 0.90
HEAD_MERGE_RECALL_GATE = 0.85
HEAD_MERGE_FALSE_POSITIVE_GATE = 0.10
AESTHETIC_AGREEMENT_GATE = 0.78
CALIBRATION_ECE_GATE = 0.05

SCHEMA = {
    "tilts": [{"wedding_id": "w1", "predicted": 1.2, "actual": 1.0}],
    "crops": [{"wedding_id": "w1", "predicted": True, "actual": True}],
    "head_merges": [{"wedding_id": "w1", "predicted": False, "actual": False}],
    "pairs": [{"wedding_id": "w1", "winner": 0.8, "loser": 0.4}],
    "calibration": [{"score": 0.8, "kept": True}],
}


def confusion(predicted: Iterable[bool], actual: Iterable[bool]) -> tuple[int, int, int, int]:
    """Return true-positive, false-positive, false-negative and true-negative counts."""
    tp = fp = fn = tn = 0
    for guess, truth in zip(predicted, actual):
        if guess and truth:
            tp += 1
        elif guess:
            fp += 1
        elif truth:
            fn += 1
        else:
            tn += 1
    return tp, fp, fn, tn


def precision_recall_f1(tp: int, fp: int, fn: int) -> tuple[float, float, float]:
    precision = tp / (tp + fp) if tp + fp else 1.0
    recall = tp / (tp + fn) if tp + fn else 1.0
    f1 = 2.0 * precision * recall / (precision + recall) if precision + recall else 0.0
    return precision, recall, f1


def horizon_errors(predicted: Sequence[float], actual: Sequence[float]) -> tuple[float, float]:
    errors = [abs(guess - truth) for guess, truth in zip(predicted, actual)]
    if not errors:
        return math.inf, math.inf
    return sum(errors) / len(errors), max(errors)


def pairwise_agreement(pairs: Iterable[tuple[float, float]]) -> float:
    """Fraction of photographer winners scored strictly above their paired loser."""
    rows = list(pairs)
    if not rows:
        return 0.0
    return sum(1 for winner, loser in rows if winner > loser) / len(rows)


def expected_calibration_error(points: Sequence[tuple[float, bool]], bins: int = 10) -> float:
    """Equal-width ECE over keeper labels; empty bins contribute nothing."""
    if not points:
        return math.inf
    total = len(points)
    error = 0.0
    for index in range(bins):
        low = index / bins
        high = (index + 1) / bins
        bucket = [
            (score, kept)
            for score, kept in points
            if low <= score < high or (index == bins - 1 and score == 1.0)
        ]
        if not bucket:
            continue
        confidence = sum(score for score, _ in bucket) / len(bucket)
        accuracy = sum(1.0 for _, kept in bucket if kept) / len(bucket)
        error += len(bucket) / total * abs(confidence - accuracy)
    return error


def fit_isotonic(points: Sequence[tuple[float, bool]]) -> list[tuple[float, float]]:
    """Pool-adjacent-violators calibration, returned as sorted score/probability knots."""
    ordered = sorted((float(score), 1.0 if kept else 0.0) for score, kept in points)
    blocks: list[list[float]] = []
    for score, label in ordered:
        blocks.append([score, score, label, 1.0])
        while len(blocks) >= 2 and blocks[-2][2] > blocks[-1][2]:
            right = blocks.pop()
            left = blocks.pop()
            count = left[3] + right[3]
            mean = (left[2] * left[3] + right[2] * right[3]) / count
            blocks.append([left[0], right[1], mean, count])
    knots: list[tuple[float, float]] = []
    for low, high, mean, _ in blocks:
        knots.append((low, mean))
        if high != low:
            knots.append((high, mean))
    return knots


def split_leaks(rows: Iterable[dict[str, Any]]) -> set[str]:
    """Wedding ids appearing in more than one declared split."""
    seen: dict[str, set[str]] = {}
    for row in rows:
        wedding = str(row.get("wedding_id", ""))
        split = str(row.get("split", ""))
        if wedding and split:
            seen.setdefault(wedding, set()).add(split)
    return {wedding for wedding, splits in seen.items() if len(splits) > 1}


def evaluate(payload: dict[str, Any]) -> tuple[int, dict[str, float | None]]:
    tilts = payload.get("tilts", [])
    crop_rows = payload.get("crops", [])
    merge_rows = payload.get("head_merges", [])
    pair_rows = payload.get("pairs", [])
    calibration_rows = payload.get("calibration", [])

    mean_angle, worst_angle = horizon_errors(
        [float(row["predicted"]) for row in tilts],
        [float(row["actual"]) for row in tilts],
    )
    crop_counts = confusion(
        [bool(row["predicted"]) for row in crop_rows],
        [bool(row["actual"]) for row in crop_rows],
    )
    crop_precision, crop_recall, crop_f1 = precision_recall_f1(*crop_counts[:3])
    merge_counts = confusion(
        [bool(row["predicted"]) for row in merge_rows],
        [bool(row["actual"]) for row in merge_rows],
    )
    _, merge_recall, _ = precision_recall_f1(*merge_counts[:3])
    merge_fp_rate = merge_counts[1] / (merge_counts[1] + merge_counts[3]) if merge_counts[1] + merge_counts[3] else 0.0
    agreement = pairwise_agreement(
        (float(row["winner"]), float(row["loser"])) for row in pair_rows
    )
    ece: float | None = expected_calibration_error(
        [(float(row["score"]), bool(row["kept"])) for row in calibration_rows]
    ) if calibration_rows else None

    metrics = {
        "horizon_mean_error_deg": mean_angle,
        "horizon_worst_error_deg": worst_angle,
        "crop_precision": crop_precision,
        "crop_recall": crop_recall,
        "crop_f1": crop_f1,
        "head_merge_recall": merge_recall,
        "head_merge_false_positive_rate": merge_fp_rate,
        "aesthetic_pairwise_agreement": agreement,
        "calibration_ece": ece,
    }
    failures = int(worst_angle > HORIZON_ERROR_GATE_DEG)
    failures += int(crop_f1 < CROP_F1_GATE)
    failures += int(merge_recall < HEAD_MERGE_RECALL_GATE)
    failures += int(merge_fp_rate >= HEAD_MERGE_FALSE_POSITIVE_GATE)
    failures += int(agreement < AESTHETIC_AGREEMENT_GATE)
    failures += int(ece is not None and ece > CALIBRATION_ECE_GATE)
    return failures, metrics


def self_test() -> int:
    good = {
        "tilts": [
            {"predicted": value + 0.1, "actual": value} for value in (-8.0, -1.0, 0.0, 3.0)
        ],
        "crops": [
            {"predicted": True, "actual": True},
            {"predicted": False, "actual": False},
        ],
        "head_merges": [
            {"predicted": True, "actual": True},
            {"predicted": False, "actual": False},
        ],
        "pairs": [{"winner": 0.8, "loser": 0.2} for _ in range(8)],
    }
    failures, metrics = evaluate(good)
    print(json.dumps(metrics, indent=2, sort_keys=True))
    if failures:
        print("FAIL: known-good readings did not pass")
        return 1

    degenerate = {
        "tilts": [{"predicted": 0.0, "actual": value} for value in (-8.0, 8.0)],
        "crops": [
            {"predicted": True, "actual": False},
            {"predicted": True, "actual": True},
        ],
        "head_merges": [
            {"predicted": True, "actual": False},
            {"predicted": True, "actual": True},
        ],
        "pairs": [{"winner": 0.5, "loser": 0.5} for _ in range(8)],
    }
    bad_failures, _ = evaluate(degenerate)
    # This single degenerate reader should trip horizon, crop F1, head-merge false
    # positives and pairwise agreement. Its flag-everything merge output intentionally
    # has perfect recall, so recall is checked separately below.
    if bad_failures < 4:
        print(f"FAIL: degenerate analyser tripped only {bad_failures} gates")
        return 1
    missed_merges = confusion([False, False], [True, False])
    _, missed_recall, _ = precision_recall_f1(*missed_merges[:3])
    if missed_recall >= HEAD_MERGE_RECALL_GATE:
        print("FAIL: a miss-everything merge detector passed the recall gate")
        return 1
    print("self-test: metrics pass known-good data and reject a degenerate analyser")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--schema", action="store_true")
    parser.add_argument("--evaluate", metavar="FILE")
    parser.add_argument("--fit-calibration", metavar="FILE")
    args = parser.parse_args()

    if args.schema:
        print(json.dumps(SCHEMA, indent=2))
        return 0
    if args.self_test:
        return self_test()
    if args.evaluate:
        payload = json.loads(Path(args.evaluate).read_text(encoding="utf-8"))
        failures, metrics = evaluate(payload)
        print(json.dumps(metrics, indent=2, sort_keys=True))
        return 1 if failures else 0
    if args.fit_calibration:
        rows = json.loads(Path(args.fit_calibration).read_text(encoding="utf-8"))
        knots = fit_isotonic([(float(row["score"]), bool(row["kept"])) for row in rows])
        print(json.dumps({"knots": knots}, indent=2))
        return 0
    parser.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
