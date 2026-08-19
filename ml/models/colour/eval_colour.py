#!/usr/bin/env python3
"""Evaluate Phase 16's tone, curve, HSL, clipping and skin-protection gates.

This is the Python side of ``tests/eval/colour_eval.rs``. It deliberately uses only the
standard library so a dataset curator can run it before installing a training stack. Real
evaluation input is one JSON object with ``tone``, ``curves``, ``skin``, ``clipping`` and
optional ``greenery`` arrays; run ``--schema`` for the exact shape.

``--self-test`` proves every metric rejects a degenerate implementation. It is not a quality
claim about the placeholder weights shipped by this repository.

Two of the metrics here are worth reading carefully.

**The skin metric reports three numbers**, because a grade can be uniformly heavy-handed or
selectively so and only the second is a fairness failure:

* ``worst_hue`` - the largest hue rotation any sample suffered, gated at 2 degrees;
* ``worst_chroma`` - the largest relative chroma change, gated at 6 %;
* ``spread`` - the worst bucket's mean hue shift minus the best bucket's, gated at half the
  hue ceiling.

The buckets are Monk-scale groups and they exist **only here**. Nothing in the product
stores a skin-tone bucket, because measuring a disparity needs the grouping and shipping the
grouping into a catalog is how a measurement becomes a demographic record.

**The clipping metric is about what a frame GAINS.** A photograph that arrived with a blown
sparkler is not the grade's fault, and a metric that measured the absolute figure would
report every exit photograph as a failure and push a solver toward darkening faces.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from collections.abc import Iterable, Sequence
from pathlib import Path
from typing import Any

# Section 10.1, and every one of these is also a constant in
# `aura_core::contract::colour` or in `tests/eval/colour_eval.rs`. They are restated rather
# than imported because this script has to run with nothing installed; the Rust harness
# asserts the pair agree.
SUBJECT_CONTRAST_TOLERANCE = 0.08
TONE_AGREEMENT_GATE = 0.85
SKIN_HUE_CEILING_DEG = 2.0
SKIN_CHROMA_CEILING = 0.06
CLIPPING_GATE = 0.995
SUBTLETY_CEILING = 0.45
CURVE_MIN_POINTS = 4
CURVE_MAX_POINTS = 8

# Monk-scale groups, coarsened to five buckets, exactly as `eval_tone.py` does. Coarser than
# the ten-point scale because a per-bucket mean needs samples in it, and finer than "light
# and dark" because that division is the one that hides the failure.
BUCKETS = ("monk_1_2", "monk_3_4", "monk_5_6", "monk_7_8", "monk_9_10")

SCHEMA = {
    "tone": [
        {
            "wedding_id": "w1",
            "scene": "couple_portrait",
            "achieved_subject_contrast": 0.31,
            "intent_subject_contrast": 0.33,
            "subtlety": 0.24,
            "subtlety_cap": 0.30,
        }
    ],
    "curves": [
        {
            "wedding_id": "w1",
            "points": [[0, 0], [64, 54], [192, 202], [255, 255]],
        }
    ],
    "skin": [
        {
            "wedding_id": "w1",
            "bucket": "monk_7_8",
            "hue_shift_deg": 0.4,
            "chroma_change": 0.01,
            "withdrew": False,
        }
    ],
    "clipping": [
        {
            "wedding_id": "w1",
            "scene": "golden_hour",
            "before": 0.021,
            "after": 0.024,
            "tolerance": 0.03,
        }
    ],
    "greenery": [
        {
            "wedding_id": "w1",
            "hue_before_deg": 95.0,
            "hue_after_deg": 104.0,
            "target_deg": 128.0,
            "saturation_before": 0.62,
            "saturation_after": 0.55,
        }
    ],
}


# ---------------------------------------------------------------------------
# Metrics
# ---------------------------------------------------------------------------


def tone_agreement(rows: Sequence[dict[str, Any]]) -> dict[str, Any]:
    """Fraction of frames whose graded subject contrast lands on the scene's intent.

    Section 10.1's first gate. The reference is the intent rather than an expert edit until
    there is a corpus of expert edits to measure against; the tolerance is the same number
    the Rust harness uses and a test there asserts it.
    """
    if not rows:
        return {"count": 0, "rate": 0.0, "passed": False}
    agreed = 0
    for row in rows:
        achieved = float(row.get("achieved_subject_contrast", 0.0))
        intent = float(row.get("intent_subject_contrast", 0.0))
        if abs(achieved - intent) <= SUBJECT_CONTRAST_TOLERANCE:
            agreed += 1
    rate = agreed / len(rows)
    return {
        "count": len(rows),
        "agreed": agreed,
        "rate": rate,
        "passed": rate >= TONE_AGREEMENT_GATE,
    }


def subtlety(rows: Sequence[dict[str, Any]]) -> dict[str, Any]:
    """Whether any frame was graded past its scene's cap.

    Section 12's first failure mode. A single frame over its cap fails: the metric is a
    maximum rather than a mean, because one over-graded photograph in a delivered gallery is
    what a photographer notices.
    """
    if not rows:
        return {"count": 0, "over": 0, "passed": True}
    over = [
        row
        for row in rows
        if float(row.get("subtlety", 0.0))
        > min(float(row.get("subtlety_cap", SUBTLETY_CEILING)), SUBTLETY_CEILING) + 1e-3
    ]
    return {
        "count": len(rows),
        "over": len(over),
        "worst": max((float(r.get("subtlety", 0.0)) for r in rows), default=0.0),
        "passed": not over,
    }


def curve_monotonicity(rows: Sequence[dict[str, Any]]) -> dict[str, Any]:
    """Whether every curve's control points are monotone and inside the point bounds.

    This is the whole of section 10.1's "every generated curve is monotonic and produces no
    posterisation". Monotonicity of the rendered curve IS monotonicity of the control points,
    because the renderer interpolates with Fritsch-Carlson - so checking the points is
    checking the pixels. ADR-0033 decision 2.
    """
    bad: list[dict[str, Any]] = []
    for row in rows:
        points = [tuple(point) for point in row.get("points", [])]
        if not points:
            bad.append({"wedding_id": row.get("wedding_id"), "why": "no points"})
            continue
        identity = points == [(0, 0), (255, 255)]
        if not identity and not (CURVE_MIN_POINTS <= len(points) <= CURVE_MAX_POINTS):
            bad.append({"wedding_id": row.get("wedding_id"), "why": "point count"})
            continue
        if points[0][0] != 0 or points[-1][0] != 255:
            bad.append({"wedding_id": row.get("wedding_id"), "why": "not anchored"})
            continue
        for first, second in zip(points, points[1:]):
            if second[0] <= first[0]:
                bad.append({"wedding_id": row.get("wedding_id"), "why": "x not increasing"})
                break
            if second[1] < first[1]:
                bad.append({"wedding_id": row.get("wedding_id"), "why": "y decreases"})
                break
    return {"count": len(rows), "bad": bad, "passed": not bad}


def skin_protection(rows: Sequence[dict[str, Any]]) -> dict[str, Any]:
    """The headline guarantee, and the fairness half of it.

    Three numbers, described in this module's docstring. The spread is the one that would
    catch a product whose guard works harder on one skin tone than on another - which is a
    fairness failure even when every individual frame passes.
    """
    if not rows:
        return {"count": 0, "passed": False, "why": "no samples"}
    worst_hue = max(float(row.get("hue_shift_deg", 0.0)) for row in rows)
    worst_chroma = max(float(row.get("chroma_change", 0.0)) for row in rows)

    by_bucket: dict[str, list[float]] = {bucket: [] for bucket in BUCKETS}
    for row in rows:
        bucket = str(row.get("bucket", ""))
        if bucket in by_bucket:
            by_bucket[bucket].append(float(row.get("hue_shift_deg", 0.0)))
    means = {
        bucket: sum(values) / len(values)
        for bucket, values in by_bucket.items()
        if values
    }
    spread = (max(means.values()) - min(means.values())) if len(means) > 1 else 0.0

    return {
        "count": len(rows),
        "buckets": len(means),
        "worst_hue": worst_hue,
        "worst_chroma": worst_chroma,
        "means": means,
        "spread": spread,
        "withdrew": sum(1 for row in rows if row.get("withdrew")),
        "passed": (
            worst_hue <= SKIN_HUE_CEILING_DEG
            and worst_chroma <= SKIN_CHROMA_CEILING
            and spread <= SKIN_HUE_CEILING_DEG / 2.0
            and len(means) >= 2
        ),
    }


def clipping(rows: Sequence[dict[str, Any]]) -> dict[str, Any]:
    """Fraction of frames that gained no clipping beyond their scene tolerance.

    ``after - before``, floored at zero. See the module docstring for why the absolute figure
    would be the wrong measurement.
    """
    if not rows:
        return {"count": 0, "rate": 0.0, "passed": False}
    inside = 0
    for row in rows:
        added = max(0.0, float(row.get("after", 0.0)) - float(row.get("before", 0.0)))
        if added <= float(row.get("tolerance", 0.0)) + 1e-4:
            inside += 1
    rate = inside / len(rows)
    return {
        "count": len(rows),
        "inside": inside,
        "rate": rate,
        "passed": rate >= CLIPPING_GATE,
    }


def greenery(rows: Sequence[dict[str, Any]]) -> dict[str, Any]:
    """Whether foliage moved toward the scene's target and was not saturated further.

    Section 10.1's sixth gate is an expert-scoring comparison, which needs experts. Until
    there are, this measures the two things the expert would be scoring: the direction of the
    hue move and the sign of the saturation move.
    """
    if not rows:
        return {"count": 0, "improved": 0, "passed": True, "why": "no greenery frames"}
    improved = 0
    for row in rows:
        before = float(row.get("hue_before_deg", 0.0))
        after = float(row.get("hue_after_deg", 0.0))
        target = float(row.get("target_deg", 0.0))
        closer = abs(_signed(after, target)) <= abs(_signed(before, target)) + 1e-6
        calmer = float(row.get("saturation_after", 0.0)) <= float(
            row.get("saturation_before", 0.0)
        ) + 1e-6
        if closer and calmer:
            improved += 1
    return {
        "count": len(rows),
        "improved": improved,
        "rate": improved / len(rows),
        "passed": improved == len(rows),
    }


def _signed(a: float, b: float) -> float:
    """The shortest signed rotation from ``a`` to ``b``, in degrees."""
    raw = (b - a) % 360.0
    return raw - 360.0 if raw > 180.0 else raw


# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------


def report(payload: dict[str, Any]) -> dict[str, Any]:
    """Every gate, and whether the set as a whole passes."""
    sections = {
        "tone": tone_agreement(payload.get("tone", [])),
        "subtlety": subtlety(payload.get("tone", [])),
        "curves": curve_monotonicity(payload.get("curves", [])),
        "skin": skin_protection(payload.get("skin", [])),
        "clipping": clipping(payload.get("clipping", [])),
        "greenery": greenery(payload.get("greenery", [])),
    }
    sections["passed"] = all(
        section.get("passed", False)
        for name, section in sections.items()
        if isinstance(section, dict)
    )
    return sections


def self_test() -> int:
    """Prove every metric would reject a degenerate implementation.

    Six assertions, one per gate, each of them a plausible bad answer rather than a
    nonsensical one - a solver that never moves, a curve that goes backwards, a grade that
    turns one bucket orange, a guard that clips, and a foliage adjustment that pushes the
    wrong way.
    """
    failures: list[str] = []

    # A solver that never moves the contrast fails the agreement gate.
    lazy = [
        {"achieved_subject_contrast": 0.10, "intent_subject_contrast": 0.33}
        for _ in range(20)
    ]
    if tone_agreement(lazy)["passed"]:
        failures.append("a do-nothing tone solver passed the agreement gate")

    # A curve that goes backwards fails.
    if curve_monotonicity([{"points": [[0, 0], [128, 200], [64, 20], [255, 255]]}])["passed"]:
        failures.append("a non-monotone curve passed")

    # A grade that turns one bucket orange fails, even when the mean is fine.
    biased = [
        {"bucket": "monk_1_2", "hue_shift_deg": 0.1, "chroma_change": 0.01},
        {"bucket": "monk_9_10", "hue_shift_deg": 1.9, "chroma_change": 0.01},
    ]
    if skin_protection(biased)["passed"]:
        failures.append("a grade that treated one skin tone differently passed")

    # A single frame over the hue ceiling fails.
    if skin_protection(
        [
            {"bucket": "monk_1_2", "hue_shift_deg": 0.1, "chroma_change": 0.01},
            {"bucket": "monk_5_6", "hue_shift_deg": 2.4, "chroma_change": 0.01},
        ]
    )["passed"]:
        failures.append("a frame past the hue ceiling passed")

    # A grade that adds clipping past the tolerance fails.
    if clipping(
        [{"before": 0.0, "after": 0.05, "tolerance": 0.01} for _ in range(10)]
    )["passed"]:
        failures.append("a clipping grade passed")

    # A frame that arrived blown and gained nothing passes, which is the other half.
    if not clipping(
        [{"before": 0.30, "after": 0.30, "tolerance": 0.01} for _ in range(10)]
    )["passed"]:
        failures.append("an already-blown frame was blamed for its own clipping")

    # Foliage pushed further from the target fails.
    if greenery(
        [
            {
                "hue_before_deg": 95.0,
                "hue_after_deg": 88.0,
                "target_deg": 128.0,
                "saturation_before": 0.6,
                "saturation_after": 0.7,
            }
        ]
    )["passed"]:
        failures.append("foliage pushed the wrong way passed")

    # A grade past every cap fails the subtlety gate.
    if subtlety([{"subtlety": 0.40, "subtlety_cap": 0.20}])["passed"]:
        failures.append("an over-graded frame passed the subtlety gate")

    for line in failures:
        print(f"FAIL: {line}", file=sys.stderr)
    if failures:
        return 1
    print("self-test: every gate rejects a degenerate implementation")
    return 0


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, help="evaluation payload, as JSON")
    parser.add_argument("--schema", action="store_true", help="print the input schema")
    parser.add_argument("--self-test", action="store_true", help="prove the metrics bite")
    args = parser.parse_args(argv)

    if args.schema:
        print(json.dumps(SCHEMA, indent=2))
        return 0
    if args.self_test:
        return self_test()
    if not args.input:
        parser.error("one of --input, --schema or --self-test is required")

    payload = json.loads(args.input.read_text(encoding="utf-8"))
    result = report(payload)
    print(json.dumps(result, indent=2))
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
