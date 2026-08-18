#!/usr/bin/env python3
"""Evaluate Phase 15's exposure, white-balance, mood and skin-fairness gates.

This is the Python side of ``tests/eval/tone_eval.rs``. It deliberately uses only the
standard library so a dataset curator can run it before installing a training stack. Real
evaluation input is one JSON object with ``exposures``, ``white_balances``, ``skin``,
``moods`` and optional ``references`` arrays; run ``--schema`` for the exact shape.

``--self-test`` proves every metric rejects a degenerate implementation. It is not a
quality claim about the placeholder weights shipped by this repository.

The fairness metric is the one to read carefully. It reports **two** numbers, because a
solver can be uniformly mediocre or selectively good and only the second is a fairness
failure:

* ``mean`` - dE00 averaged over every sample, gated at 3.0;
* ``spread`` - the worst bucket's mean minus the best bucket's mean, gated at 1.0.

The buckets are Monk-scale groups and they exist **only here**. Nothing in the product
stores a skin-tone bucket, because measuring a disparity needs the grouping and shipping
the grouping into a catalog is how a measurement becomes a demographic record.
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
# `aura_core::contract::tone`. They are restated rather than imported because this script
# has to run with nothing installed; `tests/eval/tone_eval.rs` asserts the pair agree.
EXPOSURE_TOLERANCE_EV = 0.15
EXPOSURE_AGREEMENT_GATE = 0.85
WB_TOLERANCE_K = 200.0
WB_TOLERANCE_TINT = 4.0
WB_AGREEMENT_GATE = 0.85
SKIN_DE00_CEILING = 3.0
SKIN_DE00_SPREAD = 1.0
MIN_REFERENCE_FRAMES = 3

# Monk-scale groups, coarsened to five buckets. Coarser than the ten-point scale because a
# per-bucket mean needs samples in it, and finer than "light and dark" because that
# division is the one that hides the failure.
BUCKETS = ("monk_1_2", "monk_3_4", "monk_5_6", "monk_7_8", "monk_9_10")

SCHEMA = {
    "exposures": [
        {"wedding_id": "w1", "scene": "ceremony", "predicted_ev": 0.42, "expert_ev": 0.40,
         "face_anchored": True, "clipping_added": 0.001, "clip_tolerance": 0.02}
    ],
    "white_balances": [
        {"wedding_id": "w1", "lighting_class": "tungsten", "predicted_k": 3200.0,
         "expert_k": 3250.0, "predicted_tint": 6.0, "expert_tint": 4.0}
    ],
    "skin": [
        {"wedding_id": "w1", "bucket": "monk_7_8", "de00": 1.8}
    ],
    "moods": [
        {"wedding_id": "w1", "scene": "dance_floor", "illuminant_chroma": 0.06,
         "residual_chroma": 0.05, "preserved": True}
    ],
    "references": [
        {"wedding_id": "w1", "segment_id": "seg_1", "frames": 4}
    ],
}


def fraction_within(errors: Iterable[float], tolerance: float) -> float:
    values = list(errors)
    if not values:
        return 0.0
    return sum(1 for error in values if error <= tolerance) / len(values)


def exposure_report(rows: Sequence[dict[str, Any]]) -> dict[str, Any]:
    errors = [abs(float(row["predicted_ev"]) - float(row["expert_ev"])) for row in rows]
    agreement = fraction_within(errors, EXPOSURE_TOLERANCE_EV)

    # Section 10.1's second exposure clause, and it is a hard one rather than a fraction:
    # "never increases clipping beyond scene tolerance". One frame over its own scene's
    # tolerance fails the gate.
    over_tolerance = [
        row
        for row in rows
        if float(row.get("clipping_added", 0.0)) > float(row.get("clip_tolerance", 0.0))
    ]
    faceless = [row for row in rows if not row.get("face_anchored", False)]
    faceless_errors = [
        abs(float(row["predicted_ev"]) - float(row["expert_ev"])) for row in faceless
    ]
    return {
        "frames": len(rows),
        "agreement": agreement,
        "gate": EXPOSURE_AGREEMENT_GATE,
        "pass": agreement >= EXPOSURE_AGREEMENT_GATE and not over_tolerance,
        "clipping_violations": len(over_tolerance),
        "faceless_frames": len(faceless),
        "faceless_agreement": fraction_within(faceless_errors, EXPOSURE_TOLERANCE_EV),
    }


def white_balance_report(rows: Sequence[dict[str, Any]]) -> dict[str, Any]:
    # Both halves must hold on the same frame. A solver that is within 200 K on every frame
    # and 12 tint units off on half of them has a green wedding, and two independent
    # fractions would each report 100 percent.
    def within(row: dict[str, Any]) -> bool:
        return (
            abs(float(row["predicted_k"]) - float(row["expert_k"])) <= WB_TOLERANCE_K
            and abs(float(row["predicted_tint"]) - float(row["expert_tint"])) <= WB_TOLERANCE_TINT
        )

    agreement = (sum(1 for row in rows if within(row)) / len(rows)) if rows else 0.0
    by_class: dict[str, float] = {}
    for name in sorted({str(row.get("lighting_class", "unknown")) for row in rows}):
        subset = [row for row in rows if str(row.get("lighting_class", "unknown")) == name]
        by_class[name] = (sum(1 for row in subset if within(row)) / len(subset)) if subset else 0.0
    return {
        "frames": len(rows),
        "agreement": agreement,
        "gate": WB_AGREEMENT_GATE,
        "pass": agreement >= WB_AGREEMENT_GATE,
        "by_lighting_class": by_class,
        "worst_class": min(by_class.items(), key=lambda pair: pair[1]) if by_class else None,
    }


def skin_report(rows: Sequence[dict[str, Any]]) -> dict[str, Any]:
    """Mean dE00 and the spread across Monk-scale buckets. Section 6.3 and 10.1."""
    if not rows:
        return {"samples": 0, "mean": math.inf, "spread": math.inf, "pass": False, "by_bucket": {}}

    by_bucket: dict[str, float] = {}
    for name in BUCKETS:
        subset = [float(row["de00"]) for row in rows if row.get("bucket") == name]
        if subset:
            by_bucket[name] = sum(subset) / len(subset)

    mean = sum(float(row["de00"]) for row in rows) / len(rows)
    # A spread needs at least two populated buckets to mean anything. With one bucket the
    # honest answer is that the fairness gate was not measured, not that it passed.
    if len(by_bucket) < 2:
        return {
            "samples": len(rows),
            "mean": mean,
            "spread": math.inf,
            "pass": False,
            "by_bucket": by_bucket,
            "note": "fewer than two populated buckets; the fairness gate was not measured",
        }

    spread = max(by_bucket.values()) - min(by_bucket.values())
    return {
        "samples": len(rows),
        "mean": mean,
        "mean_gate": SKIN_DE00_CEILING,
        "spread": spread,
        "spread_gate": SKIN_DE00_SPREAD,
        "pass": mean <= SKIN_DE00_CEILING and spread <= SKIN_DE00_SPREAD,
        "by_bucket": by_bucket,
        "worst_bucket": max(by_bucket.items(), key=lambda pair: pair[1]),
        "best_bucket": min(by_bucket.items(), key=lambda pair: pair[1]),
    }


def mood_report(rows: Sequence[dict[str, Any]]) -> dict[str, Any]:
    """Coloured lighting is preserved, not neutralised. Section 2.1 and 6.2.

    The metric is the fraction of the illuminant's own chroma that survived the correction.
    A solver that neutralised a purple dance floor leaves a residual near zero; one that
    preserved it leaves most of it. The gate is deliberately not "unchanged": section 6.2
    says "correct only enough to keep skin within its plausible locus", so some movement is
    correct and total movement is the failure.
    """
    if not rows:
        return {"frames": 0, "preserved": 0.0, "pass": False}
    kept = []
    for row in rows:
        original = float(row["illuminant_chroma"])
        residual = float(row["residual_chroma"])
        kept.append(residual / original if original > 0.0 else 1.0)
    mean_kept = sum(kept) / len(kept)
    flagged = sum(1 for row in rows if row.get("preserved", False)) / len(rows)
    return {
        "frames": len(rows),
        "chroma_kept": mean_kept,
        "flagged_preserved": flagged,
        # Half the cast surviving is the line between "kept the mood" and "corrected it
        # away and left a trace". The fixture set is what this was chosen against.
        "pass": mean_kept >= 0.50 and flagged >= 0.90,
    }


def reference_report(rows: Sequence[dict[str, Any]]) -> dict[str, Any]:
    if not rows:
        return {"segments": 0, "anchored": 0, "pass": False}
    anchored = sum(1 for row in rows if int(row.get("frames", 0)) >= MIN_REFERENCE_FRAMES)
    return {
        "segments": len(rows),
        "anchored": anchored,
        "gate": MIN_REFERENCE_FRAMES,
        "pass": anchored == len(rows),
    }


def evaluate(document: dict[str, Any]) -> dict[str, Any]:
    report = {
        "exposure": exposure_report(document.get("exposures", [])),
        "white_balance": white_balance_report(document.get("white_balances", [])),
        "skin": skin_report(document.get("skin", [])),
        "mood": mood_report(document.get("moods", [])),
        "references": reference_report(document.get("references", [])),
    }
    report["pass"] = all(section.get("pass", False) for section in report.values())
    return report


def self_test() -> int:
    """Every metric must reject a degenerate implementation."""
    problems: list[str] = []

    # An exposure that is always zero fails, even when most experts also chose nearly zero.
    lazy = [
        {"predicted_ev": 0.0, "expert_ev": 0.9, "clipping_added": 0.0, "clip_tolerance": 0.02}
        for _ in range(10)
    ]
    if exposure_report(lazy)["pass"]:
        problems.append("exposure gate accepted a constant predictor")

    # One frame over its own scene's clipping tolerance fails, however good the agreement.
    clipping = [
        {"predicted_ev": 0.40, "expert_ev": 0.40, "clipping_added": 0.0, "clip_tolerance": 0.02}
        for _ in range(19)
    ] + [{"predicted_ev": 0.40, "expert_ev": 0.40, "clipping_added": 0.09, "clip_tolerance": 0.02}]
    if exposure_report(clipping)["pass"]:
        problems.append("exposure gate accepted a frame over its clipping tolerance")

    # A white balance that is right in kelvin and wrong in tint fails.
    green = [
        {"predicted_k": 5500.0, "expert_k": 5500.0, "predicted_tint": 20.0, "expert_tint": 0.0,
         "lighting_class": "outdoor_daylight"}
        for _ in range(10)
    ]
    if white_balance_report(green)["pass"]:
        problems.append("white-balance gate accepted a uniformly green solution")

    # A solver that is good on one bucket and bad on another fails the spread even when the
    # mean is comfortable.
    biased = [{"bucket": "monk_1_2", "de00": 0.5} for _ in range(10)]
    biased += [{"bucket": "monk_9_10", "de00": 2.4} for _ in range(10)]
    result = skin_report(biased)
    if result["pass"]:
        problems.append("fairness gate accepted a 1.9 dE00 spread across buckets")
    if result["mean"] > SKIN_DE00_CEILING:
        problems.append("the biased fixture should still have passed the mean gate")

    # One bucket is not a fairness measurement.
    single = [{"bucket": "monk_3_4", "de00": 0.4} for _ in range(20)]
    if skin_report(single)["pass"]:
        problems.append("fairness gate reported a pass from one bucket")

    # A solver that neutralised the dance floor fails the mood gate.
    neutralised = [
        {"illuminant_chroma": 0.08, "residual_chroma": 0.002, "preserved": True} for _ in range(10)
    ]
    if mood_report(neutralised)["pass"]:
        problems.append("mood gate accepted a neutralised coloured light")

    # A segment with two anchors is not anchored.
    if reference_report([{"segment_id": "s", "frames": 2}])["pass"]:
        problems.append("reference gate accepted a segment with two anchors")

    for problem in problems:
        print(f"[FAIL] {problem}", file=sys.stderr)
    if problems:
        return 1
    print("[PASS] every Phase 15 metric rejects its degenerate case")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--schema", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--evaluate", metavar="JSON")
    args = parser.parse_args()

    if args.schema:
        print(json.dumps(SCHEMA, indent=2))
        return 0
    if args.self_test:
        return self_test()
    if not args.evaluate:
        parser.print_help()
        print(
            "\nno --evaluate supplied. There are no expert edits in this repository "
            "(Phase 15 condition C1); run --self-test or --schema."
        )
        return 0

    document = json.loads(Path(args.evaluate).read_text(encoding="utf-8"))
    report = evaluate(document)
    print(json.dumps(report, indent=2))
    return 0 if report["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
